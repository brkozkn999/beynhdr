//! `lane.*` API handlers: atomic lane create, list, and destroy.
//!
//! A lane is a first-class bundle — a Git worktree, its own workspace (and
//! root pane), and an optional agent — that plugin runners previously
//! assembled through separate `worktree`/`workspace`/`pane`/`agent` calls.
//! `lane.create` performs the whole chain as one operation. When any step
//! fails, every resource the lane created is rolled back in reverse order
//! and the error response reports exactly what cleanup ran and how it fared.
//!
//! Flow (all handlers run on the app loop; git work runs on worker threads):
//!   1. `start_api_lane_create` validates params, resolves the repo source,
//!      reserves the checkout key, and spawns a `git worktree add` worker.
//!   2. `LaneWorktreeAddFinished` → `handle_api_lane_worktree_add_finished`
//!      opens the lane workspace, stamps `core.lane` identity tokens, applies
//!      `run`, and starts the agent.
//!   3. Any post-add failure → `begin_lane_rollback`: with a lane workspace
//!      open, the managed `worktree.remove` flow tears it down (closing panes
//!      and emitting the usual events) through an intercepted response
//!      channel; without one, a worker removes the checkout directly. A
//!      lane-created branch is force-deleted once the worktree is gone. The
//!      worker reports `LaneCreateRollbackFinished` and
//!      `handle_api_lane_rollback_finished` answers the original request.
//!   4. `lane.destroy` reuses the same cleanup machinery; `lane.list` is a
//!      synchronous read of the lane registry.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use bytes::Bytes;

use crate::api::schema::{
    AgentStartParams, EventData, EventEnvelope, EventKind, LaneCreateParams, LaneDestroyParams,
    LaneInfo, LaneStatus, Method, Request, ResponseResult, WorktreeInfo, WorktreeRemoveParams,
};
use crate::app::App;
use crate::events::{AppEvent, LaneCleanupResult, LaneCreateRollbackResult};

use super::responses::{encode_error, encode_success};
use super::worktrees::{absolute_user_path, ApiFailure, WorktreeSource};

/// Metadata token source stamped by the lane lifecycle itself. Plugins report
/// their own sources (e.g. `beyn.lane`); `core.lane` marks identity tokens the
/// server writes and owns.
pub(crate) const LANE_METADATA_SOURCE: &str = "core.lane";
const LANE_TOKEN: &str = "lane";
const LANE_ID_TOKEN: &str = "lane_id";
const LANE_BRANCH_TOKEN: &str = "lane_branch";

/// Creation step names reported in rollback error messages.
const STEP_WORKTREE_ADD: &str = "worktree_add";
const STEP_SOURCE_WORKSPACE: &str = "source_workspace";
const STEP_WORKSPACE_CREATE: &str = "workspace_create";
const STEP_PANE_RUN: &str = "pane_run";
const STEP_AGENT_START: &str = "agent_start";

/// A lane that finished `lane.create` and has not been destroyed. The registry
/// persists inside the session snapshot; on restore a record whose workspace
/// did not come back simply lists as `orphaned` (see `lane_status_for_record`).
#[derive(Debug, Clone)]
pub(crate) struct LaneRecord {
    pub lane_id: String,
    pub name: String,
    pub branch: String,
    /// `lane.create` created this branch (it did not exist beforehand); only
    /// such branches are deleted by `lane.destroy`.
    pub branch_created: bool,
    pub checkout_path: PathBuf,
    pub repo_root: PathBuf,
    pub repo_name: String,
    /// Internal workspace id while the lane workspace is open.
    pub workspace_internal_id: Option<String>,
    pub trust_repository: bool,
    pub created_unix_ms: u64,
}

impl LaneRecord {
    pub(crate) fn to_snapshot(&self) -> crate::persist::LaneSnapshot {
        crate::persist::LaneSnapshot {
            lane_id: self.lane_id.clone(),
            name: self.name.clone(),
            branch: self.branch.clone(),
            branch_created: self.branch_created,
            checkout_path: self.checkout_path.clone(),
            repo_root: self.repo_root.clone(),
            repo_name: self.repo_name.clone(),
            workspace_internal_id: self.workspace_internal_id.clone(),
            trust_repository: self.trust_repository,
            created_unix_ms: self.created_unix_ms,
        }
    }

    pub(crate) fn from_snapshot(snapshot: &crate::persist::LaneSnapshot) -> Self {
        Self {
            lane_id: snapshot.lane_id.clone(),
            name: snapshot.name.clone(),
            branch: snapshot.branch.clone(),
            branch_created: snapshot.branch_created,
            checkout_path: snapshot.checkout_path.clone(),
            repo_root: snapshot.repo_root.clone(),
            repo_name: snapshot.repo_name.clone(),
            workspace_internal_id: snapshot.workspace_internal_id.clone(),
            trust_repository: snapshot.trust_repository,
            created_unix_ms: snapshot.created_unix_ms,
        }
    }

    /// Recompute the `lane-N` allocator after restoring persisted records so
    /// new lanes never collide with restored ids.
    pub(crate) fn next_id_after_restore(lanes: &HashMap<String, LaneRecord>) -> u64 {
        lanes
            .keys()
            .filter_map(|id| id.strip_prefix("lane-")?.parse::<u64>().ok())
            .max()
            .map_or(1, |max| max.saturating_add(1))
    }
}

/// In-flight `lane.create` state, keyed by worktree operation id.
#[derive(Debug)]
pub(crate) struct PendingLaneCreate {
    /// Reuses the worktree add request shape so source-workspace resolution
    /// and membership marking share the exact worktree code path.
    pub api: crate::events::ApiWorktreeAddRequest,
    pub lane_id: String,
    pub name: String,
    pub branch: String,
    pub checkout_path: PathBuf,
    pub run: Option<String>,
    pub agent: Option<PendingLaneAgent>,
    pub trust_repository: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingLaneAgent {
    /// Canonical agent kind label (`start_agent` revalidates it).
    pub kind: String,
    pub name: String,
    pub args: Vec<String>,
}

/// In-flight `lane.destroy` state, keyed by worktree operation id.
#[derive(Debug)]
pub(crate) struct PendingLaneDestroy {
    pub id: String,
    pub respond_to: std::sync::mpsc::Sender<String>,
    pub lane_id: String,
    /// Public workspace id while the lane workspace was open (for the
    /// `LaneDestroyed` response).
    pub workspace_id: Option<String>,
    pub path: PathBuf,
    pub branch: String,
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

/// Normalize a user-supplied lane name into something safe to store as a
/// metadata token value: trimmed, control characters removed, 80 chars max.
fn normalize_lane_name(name: &str) -> Option<String> {
    let normalized: String = name
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(80)
        .collect();
    let normalized = normalized.trim();
    (!normalized.is_empty()).then(|| normalized.to_string())
}

/// Derive a `valid_agent_name`-compatible agent name from a lane name.
fn default_lane_agent_name(name: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-') {
            slug.push(ch);
            last_dash = false;
        } else if ch.is_ascii_uppercase() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    let mut slug = if slug.is_empty() {
        "lane".to_string()
    } else {
        slug.to_string()
    };
    if !slug.starts_with(|ch: char| ch.is_ascii_lowercase()) {
        slug = format!("lane-{slug}");
    }
    slug.chars().take(32).collect()
}

fn lane_token_patch(lane_id: &str, name: &str, branch: &str) -> HashMap<String, Option<String>> {
    HashMap::from([
        (LANE_TOKEN.to_string(), Some(name.to_string())),
        (LANE_ID_TOKEN.to_string(), Some(lane_id.to_string())),
        (LANE_BRANCH_TOKEN.to_string(), Some(branch.to_string())),
    ])
}

/// Parse the JSON response line the managed `worktree.remove` flow sends to a
/// rollback/destroy intercept channel into a cleanup verdict.
fn managed_remove_outcome(line: Option<String>) -> Result<(), String> {
    match line {
        None => Err("worktree remove response channel closed".to_string()),
        Some(line) => match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(value) => match value.get("error") {
                Some(error) => Err(error
                    .get("message")
                    .and_then(|message| message.as_str())
                    .unwrap_or("worktree remove failed")
                    .to_string()),
                None => Ok(()),
            },
            Err(_) => Err("could not parse worktree remove response".to_string()),
        },
    }
}

/// Delete a lane-created branch after the worktree went away, when requested.
fn delete_branch_if_created(
    branch_created: bool,
    remove_ok: bool,
    repo_root: &std::path::Path,
    branch: &str,
    trust_repository: bool,
) -> Option<Result<(), String>> {
    if !branch_created || !remove_ok {
        return None;
    }
    Some(crate::worktree::run_branch_delete_command(
        repo_root,
        branch,
        trust_repository,
    ))
}

/// Compose the `lane_create_failed` message: which step failed plus exactly
/// what rollback accomplished (or where rollback itself failed).
fn rollback_error_message(failed_step: &str, error: &str, cleanup: &LaneCleanupResult) -> String {
    let mut message = format!("lane.create failed at step \"{failed_step}\": {error}");
    let mut rolled_back: Vec<&'static str> = Vec::new();
    if cleanup.via_managed_remove {
        if cleanup.worktree_remove.is_ok() {
            rolled_back.push("workspace");
            rolled_back.push("worktree");
        }
    } else if cleanup.worktree_remove.is_ok() {
        rolled_back.push("worktree");
    }
    if matches!(cleanup.branch_delete, Some(Ok(()))) {
        rolled_back.push("branch");
    }
    if rolled_back.is_empty() {
        message.push_str("; rollback removed nothing");
    } else {
        message.push_str(&format!("; rolled back: {}", rolled_back.join(", ")));
    }
    if let Err(err) = &cleanup.worktree_remove {
        message.push_str(&format!("; worktree remove failed: {err}"));
    }
    if let Some(Err(err)) = &cleanup.branch_delete {
        message.push_str(&format!("; branch delete failed: {err}"));
    }
    message
}

impl App {
    pub(crate) fn handle_deferred_lane_api_request(
        &mut self,
        request: Request,
        respond_to: std::sync::mpsc::Sender<String>,
    ) -> bool {
        match request.method {
            Method::LaneCreate(params) => {
                self.start_api_lane_create(request.id, params, respond_to);
                true
            }
            Method::LaneDestroy(params) => {
                self.start_api_lane_destroy(request.id, params, respond_to);
                true
            }
            _ => false,
        }
    }

    fn start_api_lane_create(
        &mut self,
        id: String,
        params: LaneCreateParams,
        respond_to: std::sync::mpsc::Sender<String>,
    ) {
        macro_rules! fail {
            ($code:expr, $message:expr) => {{
                Self::send_api_response(respond_to, encode_error(id, $code, $message));
                return;
            }};
        }

        let name = params.name.as_deref().and_then(normalize_lane_name);
        let run = params
            .run
            .map(|run| run.trim().to_string())
            .filter(|run| !run.is_empty());
        let agent = match params.agent.as_deref() {
            Some(kind) => {
                let Some(agent) = crate::detect::parse_agent_label(kind) else {
                    fail!(
                        "unsupported_agent_kind",
                        format!("unsupported interactive agent kind {kind}")
                    );
                };
                let agent_name = match params.agent_name.as_deref() {
                    Some(name) if super::super::agents::valid_agent_name(name) => name.to_string(),
                    Some(_) => {
                        fail!(
                            "invalid_agent_name",
                            "agent name must start with a lowercase letter and contain only lowercase letters, digits, '-' or '_' (1-32 characters)"
                        );
                    }
                    None => {
                        let seed = name.as_deref().unwrap_or("lane");
                        default_lane_agent_name(seed)
                    }
                };
                Some(PendingLaneAgent {
                    kind: crate::detect::agent_label(agent).to_string(),
                    name: agent_name,
                    args: params.agent_args,
                })
            }
            None => {
                if params.agent_name.is_some() || !params.agent_args.is_empty() {
                    fail!(
                        "invalid_request",
                        "agent_name and agent_args require agent to be set"
                    );
                }
                None
            }
        };

        let source = match self.resolve_worktree_source(params.workspace_id, params.cwd) {
            Ok(source) => source,
            Err(err) => fail!(err.code, err.message),
        };
        let name = name.unwrap_or_else(|| match params.branch.as_deref() {
            Some(branch) => branch
                .rsplit(['/', '\\'])
                .next()
                .and_then(normalize_lane_name)
                .unwrap_or_else(|| source.repo_name.clone()),
            None => source.repo_name.clone(),
        });
        let branch = match params.branch {
            Some(branch) => {
                let branch = branch.trim().to_string();
                if branch.is_empty() {
                    fail!("invalid_request", "branch is required");
                }
                branch
            }
            None => format!("lane/{}", crate::worktree::branch_to_path_slug(&name)),
        };
        let base = params.base.unwrap_or_else(|| "HEAD".into());
        let checkout_path = match params.path {
            Some(path) => match absolute_user_path(&path) {
                Ok(path) => path,
                Err(err) => fail!(err.code, err.message),
            },
            None => crate::worktree::default_checkout_path(
                &self.state.worktree_directory,
                &source.repo_name,
                &branch,
            ),
        };
        if crate::worktree::canonical_or_original(&checkout_path)
            == crate::worktree::canonical_or_original(&source.source_checkout_path)
        {
            fail!(
                "invalid_request",
                "lane checkout path must differ from the source checkout"
            );
        }
        let checkout_key = crate::worktree::canonical_or_original(&checkout_path);
        if self
            .pending_api_worktree_creates
            .contains_key(&checkout_key)
            || self
                .pending_api_worktree_remove_paths
                .contains_key(&checkout_key)
        {
            fail!(
                "worktree_operation_in_progress",
                "worktree operation is already in progress for this checkout"
            );
        }

        let lane_id = format!("lane-{}", self.next_lane_id);
        self.next_lane_id = self.next_lane_id.saturating_add(1);
        let operation_id = self.next_api_worktree_operation_id();
        self.pending_api_worktree_creates
            .insert(checkout_key.clone(), operation_id);

        let parent_dir = checkout_path.parent().map(std::path::Path::to_path_buf);
        let source_workspace_id = source
            .workspace_idx
            .and_then(|idx| self.state.workspaces.get(idx).map(|ws| ws.id.clone()));
        let source_existing_membership = source_workspace_id.as_ref().and_then(|workspace_id| {
            self.state
                .workspaces
                .iter()
                .find(|ws| &ws.id == workspace_id)
                .and_then(|ws| ws.worktree_space().cloned())
        });
        let source_checkout_path = source.source_checkout_path.clone();
        let api_request = crate::events::ApiWorktreeAddRequest {
            id,
            operation_id,
            checkout_key,
            source_workspace_id,
            source_existing_membership,
            source_checkout_path: source.source_checkout_path,
            source_repo_root: source.source_repo_root,
            repo_key: source.repo_key,
            repo_name: source.repo_name,
            label: Some(name.clone()),
            focus: params.focus,
            respond_to,
        };
        self.pending_lane_creates.insert(
            operation_id,
            PendingLaneCreate {
                api: api_request,
                lane_id,
                name,
                branch: branch.clone(),
                checkout_path: checkout_path.clone(),
                run,
                agent,
                trust_repository: params.trust_repository,
            },
        );
        let trust_repository = params.trust_repository;
        let event_tx = self.event_tx.clone();
        let finished = crate::events::LaneAddResult {
            operation_id,
            path: checkout_path,
            branch_existed: true,
            result: Ok(()),
        };
        let spawned = crate::thread_spawn::spawn_named_with(
            "herdr-lane-add",
            finished,
            move |mut finished| {
                finished.result = if let Some(parent_dir) = parent_dir {
                    std::fs::create_dir_all(&parent_dir).map_err(|err| err.to_string())
                } else {
                    Ok(())
                }
                .and_then(|()| {
                    match crate::worktree::lane_worktree_add_command(
                        &source_checkout_path,
                        &finished.path,
                        &branch,
                        &base,
                        trust_repository,
                    ) {
                        Ok((command, branch_existed)) => {
                            finished.branch_existed = branch_existed;
                            crate::worktree::run_worktree_command(&command)
                        }
                        Err(err) => Err(err),
                    }
                });
                let _ =
                    event_tx.blocking_send(AppEvent::LaneWorktreeAddFinished(Box::new(finished)));
            },
        );
        if let Err((mut finished, err)) = spawned {
            finished.result = Err(format!("could not start lane worktree creation: {err}"));
            self.queue_worktree_spawn_failure(AppEvent::LaneWorktreeAddFinished(Box::new(
                finished,
            )));
        }
    }

    pub(crate) fn handle_api_lane_worktree_add_finished(
        &mut self,
        result: crate::events::LaneAddResult,
    ) {
        let Some(pending) = self.pending_lane_creates.get(&result.operation_id) else {
            return;
        };
        let operation_matches = self
            .pending_api_worktree_creates
            .get(&pending.api.checkout_key)
            .is_some_and(|operation_id| *operation_id == result.operation_id);
        if !operation_matches {
            let pending = self
                .pending_lane_creates
                .remove(&result.operation_id)
                .expect("pending lane create checked above");
            Self::send_api_response(
                pending.api.respond_to,
                encode_error(
                    pending.api.id,
                    "stale_worktree_operation",
                    "lane create completed after the operation was superseded",
                ),
            );
            return;
        }
        self.pending_api_worktree_creates
            .remove(&pending.api.checkout_key);
        let pending = self
            .pending_lane_creates
            .remove(&result.operation_id)
            .expect("pending lane create checked above");
        let branch_created = !result.branch_existed;

        if let Err(err) = result.result {
            self.begin_lane_rollback(pending, STEP_WORKTREE_ADD, err, None, branch_created);
            return;
        }

        let source_workspace_idx = self.api_create_source_workspace_idx(&pending.api);
        let mut source = WorktreeSource {
            workspace_idx: source_workspace_idx,
            source_checkout_path: pending.api.source_checkout_path.clone(),
            source_repo_root: pending.api.source_repo_root.clone(),
            repo_key: pending.api.repo_key.clone(),
            repo_name: pending.api.repo_name.clone(),
        };
        if let Err(err) = self.ensure_source_parent_membership(&mut source, true) {
            self.begin_lane_rollback(
                pending,
                STEP_SOURCE_WORKSPACE,
                err.message,
                None,
                branch_created,
            );
            return;
        }

        let (ws_idx, created_workspace) =
            if let Some(ws_idx) = self.open_workspace_idx_for_checkout(&result.path) {
                if pending.api.focus {
                    self.state.switch_workspace(ws_idx);
                }
                (ws_idx, false)
            } else {
                match self.create_workspace_with_options(result.path.clone(), pending.api.focus) {
                    Ok(ws_idx) => (ws_idx, true),
                    Err(err) => {
                        self.begin_lane_rollback(
                            pending,
                            STEP_WORKSPACE_CREATE,
                            err.to_string(),
                            None,
                            branch_created,
                        );
                        return;
                    }
                }
            };

        self.mark_worktree_membership(
            &source,
            ws_idx,
            result.path.clone(),
            true,
            !created_workspace,
        );
        if let Some(ws) = self.state.workspaces.get_mut(ws_idx) {
            ws.set_custom_name(pending.name.clone());
        }
        let token_patch = lane_token_patch(&pending.lane_id, &pending.name, &pending.branch);
        self.apply_lane_tokens(ws_idx, &token_patch);
        self.state.mark_session_dirty();
        if created_workspace {
            self.emit_workspace_open_events(ws_idx);
        }
        if let Some(worktree) = self.worktree_info_for_workspace(ws_idx) {
            self.emit_worktree_created_event(ws_idx, worktree);
        }

        let tab_idx = self.state.workspaces[ws_idx].active_tab;
        let root_pane_id = self.state.workspaces[ws_idx].tabs[tab_idx].root_pane;

        if let Some(run) = pending.run.clone() {
            if let Err(err) = self.submit_lane_run(ws_idx, root_pane_id, &run) {
                self.begin_lane_rollback(pending, STEP_PANE_RUN, err, Some(ws_idx), branch_created);
                return;
            }
        }

        let mut agent_info = None;
        if let Some(agent) = pending.agent.clone() {
            let pane_id = self
                .public_pane_id(ws_idx, root_pane_id)
                .unwrap_or_default();
            match self.start_agent(AgentStartParams {
                name: agent.name,
                kind: agent.kind,
                pane_id,
                args: agent.args,
                timeout_ms: None,
            }) {
                Ok((info, _argv)) => {
                    self.emit_pane_updated(ws_idx, root_pane_id);
                    agent_info = Some(info);
                }
                Err(err) => {
                    let body = self.agent_start_error_body(err);
                    self.begin_lane_rollback(
                        pending,
                        STEP_AGENT_START,
                        body.message,
                        Some(ws_idx),
                        branch_created,
                    );
                    return;
                }
            }
        }

        let record = LaneRecord {
            lane_id: pending.lane_id.clone(),
            name: pending.name.clone(),
            branch: pending.branch.clone(),
            branch_created,
            checkout_path: result.path.clone(),
            repo_root: pending.api.source_repo_root.clone(),
            repo_name: pending.api.repo_name.clone(),
            workspace_internal_id: Some(self.state.workspaces[ws_idx].id.clone()),
            trust_repository: pending.trust_repository,
            created_unix_ms: unix_ms_now(),
        };
        let lane = self.lane_info(&record, Some(ws_idx), agent_info);
        self.lanes.insert(record.lane_id.clone(), record);
        self.state.mark_session_dirty();
        self.emit_event(EventEnvelope {
            event: EventKind::LaneCreated,
            data: EventData::LaneCreated {
                lane: Box::new(lane.clone()),
            },
        });
        let response = encode_success(
            pending.api.id,
            ResponseResult::LaneCreated {
                lane: Box::new(lane),
            },
        );
        Self::send_api_response(pending.api.respond_to, response);
    }

    /// Stamp `core.lane` identity tokens on the lane workspace and its root
    /// pane terminal. Tokens land before the created events are emitted, so
    /// `workspace.created`/`pane.created` snapshots already carry them. Token
    /// stores are source-less key/value maps, so `LANE_METADATA_SOURCE` is a
    /// logical ownership marker recorded in logs rather than per token.
    fn apply_lane_tokens(&mut self, ws_idx: usize, patch: &HashMap<String, Option<String>>) {
        if let Some(workspace) = self.state.workspaces.get_mut(ws_idx) {
            workspace
                .metadata_tokens
                .patch(patch.clone(), None, std::time::Instant::now());
            tracing::debug!(
                source = LANE_METADATA_SOURCE,
                workspace_id = %workspace.id,
                "stamped lane identity tokens"
            );
        }
        let Some(workspace) = self.state.workspaces.get(ws_idx) else {
            return;
        };
        let tab_idx = workspace.active_tab;
        let Some(root_pane) = workspace.tabs.get(tab_idx).map(|tab| tab.root_pane) else {
            return;
        };
        let Some(terminal_id) = workspace.terminal_id(root_pane).cloned() else {
            return;
        };
        if let Some(terminal) = self.state.terminals.get_mut(&terminal_id) {
            if terminal
                .metadata_tokens
                .patch(patch.clone(), None, std::time::Instant::now())
            {
                terminal.revision = terminal.revision.saturating_add(1);
            }
        }
    }

    /// Submit `run` to the lane's root pane, mirroring `pane.send_input` with a
    /// trailing Enter so the command executes in the pane's shell.
    fn submit_lane_run(
        &self,
        ws_idx: usize,
        pane_id: crate::layout::PaneId,
        command: &str,
    ) -> Result<(), String> {
        let Some(runtime) = self.lookup_runtime_sender(ws_idx, pane_id) else {
            return Err("lane root pane has no live terminal".to_string());
        };
        let bytes =
            crate::app::api_helpers::encode_api_input(runtime, command, &["Enter".to_string()])
                .map_err(|key| format!("unsupported key {key}"))?;
        runtime
            .try_send_bytes(Bytes::from(bytes))
            .map_err(|err| err.to_string())
    }

    /// Begin reverse-order rollback after a failed create step. Reinserts the
    /// pending entry so `handle_api_lane_rollback_finished` can answer the
    /// original request once cleanup lands.
    fn begin_lane_rollback(
        &mut self,
        pending: PendingLaneCreate,
        failed_step: &'static str,
        error: String,
        ws_idx: Option<usize>,
        branch_created: bool,
    ) {
        let operation_id = pending.api.operation_id;
        let repo_root = pending.api.source_repo_root.clone();
        let checkout_path = pending.checkout_path.clone();
        let branch = pending.branch.clone();
        let trust_repository = pending.trust_repository;
        self.pending_lane_creates.insert(operation_id, pending);

        if let Some(ws_idx) = ws_idx {
            // Managed path: run the real worktree.remove flow so runtimes are
            // shut down and workspace/worktree events still fire. Its response
            // goes to an intercept channel the rollback thread waits on.
            let workspace_id = self.public_workspace_id(ws_idx);
            let (rollback_tx, rollback_rx) = std::sync::mpsc::channel();
            self.start_api_worktree_remove(
                format!("lane-rollback:{operation_id}"),
                WorktreeRemoveParams {
                    workspace_id,
                    force: true,
                    trust_repository,
                },
                rollback_tx,
            );
            let event_tx = self.event_tx.clone();
            let spawn_fail_error = error.clone();
            let spawned = crate::thread_spawn::spawn_named("herdr-lane-rollback", move || {
                let worktree_remove = managed_remove_outcome(rollback_rx.recv().ok());
                let branch_delete = delete_branch_if_created(
                    branch_created,
                    worktree_remove.is_ok(),
                    &repo_root,
                    &branch,
                    trust_repository,
                );
                let cleanup = LaneCleanupResult {
                    operation_id,
                    via_managed_remove: true,
                    worktree_remove,
                    branch_delete,
                };
                let _ = event_tx.blocking_send(AppEvent::LaneCreateRollbackFinished(Box::new(
                    LaneCreateRollbackResult {
                        operation_id,
                        failed_step,
                        error,
                        cleanup,
                    },
                )));
            });
            if let Err(err) = spawned {
                self.queue_worktree_spawn_failure(AppEvent::LaneCreateRollbackFinished(Box::new(
                    LaneCreateRollbackResult {
                        operation_id,
                        failed_step,
                        error: spawn_fail_error,
                        cleanup: LaneCleanupResult {
                            operation_id,
                            via_managed_remove: true,
                            worktree_remove: Err(format!(
                                "could not start rollback cleanup: {err}"
                            )),
                            branch_delete: None,
                        },
                    },
                )));
            }
            return;
        }

        // Raw path: the lane workspace was never opened — remove the checkout
        // (and any half-created leftover) directly.
        let command = crate::worktree::build_worktree_remove_command(
            &repo_root,
            &checkout_path,
            true,
            trust_repository,
        );
        let event_tx = self.event_tx.clone();
        let spawn_fail_error = error.clone();
        let spawned = crate::thread_spawn::spawn_named("herdr-lane-rollback", move || {
            let worktree_remove = crate::worktree::run_worktree_remove_command_with_recovery(
                &command,
                &repo_root,
                &checkout_path,
                true,
                trust_repository,
            );
            let branch_delete = delete_branch_if_created(
                branch_created,
                worktree_remove.is_ok(),
                &repo_root,
                &branch,
                trust_repository,
            );
            let cleanup = LaneCleanupResult {
                operation_id,
                via_managed_remove: false,
                worktree_remove,
                branch_delete,
            };
            let _ = event_tx.blocking_send(AppEvent::LaneCreateRollbackFinished(Box::new(
                LaneCreateRollbackResult {
                    operation_id,
                    failed_step,
                    error,
                    cleanup,
                },
            )));
        });
        if let Err(err) = spawned {
            self.queue_worktree_spawn_failure(AppEvent::LaneCreateRollbackFinished(Box::new(
                LaneCreateRollbackResult {
                    operation_id,
                    failed_step,
                    error: spawn_fail_error,
                    cleanup: LaneCleanupResult {
                        operation_id,
                        via_managed_remove: false,
                        worktree_remove: Err(format!("could not start rollback cleanup: {err}")),
                        branch_delete: None,
                    },
                },
            )));
        }
    }

    pub(crate) fn handle_api_lane_rollback_finished(&mut self, result: LaneCreateRollbackResult) {
        let Some(pending) = self.pending_lane_creates.remove(&result.operation_id) else {
            return;
        };
        let message = rollback_error_message(result.failed_step, &result.error, &result.cleanup);
        Self::send_api_response(
            pending.api.respond_to,
            encode_error(pending.api.id, "lane_create_failed", message),
        );
    }

    pub(crate) fn handle_lane_list(&mut self, id: String) -> String {
        let mut lanes = Vec::new();
        let mut gone: Vec<String> = Vec::new();
        for record in self.lanes.values() {
            match self.lane_status_for_record(record) {
                LaneRecordStatus::Live(ws_idx) => {
                    lanes.push(self.lane_info(record, Some(ws_idx), None));
                }
                LaneRecordStatus::Orphaned => {
                    lanes.push(self.lane_info(record, None, None));
                }
                LaneRecordStatus::Gone => gone.push(record.lane_id.clone()),
            }
        }
        // A lane whose checkout disappeared entirely is dead; drop it so
        // repeated lists stay honest without a destroy round-trip.
        for lane_id in gone {
            self.lanes.remove(&lane_id);
        }
        lanes.sort_by(|a, b| a.lane_id.cmp(&b.lane_id));
        encode_success(id, ResponseResult::LaneList { lanes })
    }

    fn lane_status_for_record(&self, record: &LaneRecord) -> LaneRecordStatus {
        let ws_idx = record
            .workspace_internal_id
            .as_deref()
            .and_then(|id| self.state.workspaces.iter().position(|ws| ws.id == id));
        if let Some(ws_idx) = ws_idx {
            if self.state.workspaces[ws_idx]
                .worktree_space()
                .is_some_and(|space| space.is_linked_worktree)
            {
                return LaneRecordStatus::Live(ws_idx);
            }
        }
        if record.checkout_path.exists() {
            LaneRecordStatus::Orphaned
        } else {
            LaneRecordStatus::Gone
        }
    }

    fn lane_info(
        &self,
        record: &LaneRecord,
        live_ws_idx: Option<usize>,
        agent: Option<crate::api::schema::AgentInfo>,
    ) -> LaneInfo {
        let (workspace, root_pane, worktree, agent) = match live_ws_idx {
            Some(ws_idx) => {
                let tab_idx = self.state.workspaces[ws_idx].active_tab;
                let root_pane_id = self.state.workspaces[ws_idx].tabs[tab_idx].root_pane;
                (
                    Some(self.workspace_info(ws_idx)),
                    self.root_pane_info(ws_idx, tab_idx),
                    self.worktree_info_for_workspace(ws_idx),
                    agent.or_else(|| self.agent_info(ws_idx, root_pane_id)),
                )
            }
            None => (
                None,
                None,
                Some(WorktreeInfo {
                    path: record.checkout_path.display().to_string(),
                    branch: Some(record.branch.clone()),
                    is_bare: false,
                    is_detached: false,
                    is_prunable: false,
                    is_linked_worktree: true,
                    open_workspace_id: None,
                    label: record.repo_name.clone(),
                }),
                agent,
            ),
        };
        LaneInfo {
            lane_id: record.lane_id.clone(),
            name: record.name.clone(),
            status: if live_ws_idx.is_some() {
                LaneStatus::Live
            } else {
                LaneStatus::Orphaned
            },
            branch: record.branch.clone(),
            branch_created: record.branch_created,
            path: record.checkout_path.display().to_string(),
            repo_root: record.repo_root.display().to_string(),
            workspace,
            root_pane,
            worktree,
            agent,
            created_unix_ms: record.created_unix_ms,
        }
    }

    /// Resolve `params.lane` (lane_id or name) to a registered lane id.
    fn resolve_lane_id(&self, target: &str) -> Result<String, ApiFailure> {
        if let Some(record) = self.lanes.get(target) {
            return Ok(record.lane_id.clone());
        }
        let matches: Vec<&LaneRecord> = self
            .lanes
            .values()
            .filter(|record| record.name == target)
            .collect();
        match matches.len() {
            0 => Err(ApiFailure::new(
                "lane_not_found",
                format!("lane {target} not found"),
            )),
            1 => Ok(matches[0].lane_id.clone()),
            _ => Err(ApiFailure::new(
                "ambiguous_lane",
                format!("multiple lanes are named {target}; use the lane_id"),
            )),
        }
    }

    fn start_api_lane_destroy(
        &mut self,
        id: String,
        params: LaneDestroyParams,
        respond_to: std::sync::mpsc::Sender<String>,
    ) {
        let lane_id = match self.resolve_lane_id(&params.lane) {
            Ok(lane_id) => lane_id,
            Err(err) => {
                Self::send_api_response(respond_to, encode_error(id, err.code, err.message));
                return;
            }
        };
        let Some(record) = self.lanes.get(&lane_id).cloned() else {
            Self::send_api_response(
                respond_to,
                encode_error(id, "lane_not_found", format!("lane {lane_id} not found")),
            );
            return;
        };

        let operation_id = self.next_api_worktree_operation_id();
        let ws_idx = record
            .workspace_internal_id
            .as_deref()
            .and_then(|id| self.state.workspaces.iter().position(|ws| ws.id == id));
        let live = ws_idx.is_some_and(|ws_idx| {
            self.state.workspaces[ws_idx]
                .worktree_space()
                .is_some_and(|space| space.is_linked_worktree)
        });
        let workspace_id = if live {
            ws_idx.map(|ws_idx| self.public_workspace_id(ws_idx))
        } else {
            None
        };
        self.pending_lane_destroys.insert(
            operation_id,
            PendingLaneDestroy {
                id,
                respond_to,
                lane_id,
                workspace_id: workspace_id.clone(),
                path: record.checkout_path.clone(),
                branch: record.branch.clone(),
            },
        );
        let event_tx = self.event_tx.clone();
        // The lane checkout already trusted this repository at create time, so
        // either flag enables trusted git operations for teardown.
        let trust_repository = params.trust_repository || record.trust_repository;

        if let Some(workspace_id) = workspace_id {
            // Live lane: reuse the managed worktree remove so panes shut down
            // and workspace.closed/worktree.removed events still fire.
            let (intercept_tx, intercept_rx) = std::sync::mpsc::channel();
            self.start_api_worktree_remove(
                format!("lane-destroy:{operation_id}"),
                WorktreeRemoveParams {
                    workspace_id,
                    force: params.force,
                    trust_repository,
                },
                intercept_tx,
            );
            let repo_root = record.repo_root.clone();
            let branch = record.branch.clone();
            let branch_created = record.branch_created;
            let spawned = crate::thread_spawn::spawn_named("herdr-lane-destroy", move || {
                let worktree_remove = managed_remove_outcome(intercept_rx.recv().ok());
                let branch_delete = delete_branch_if_created(
                    branch_created,
                    worktree_remove.is_ok(),
                    &repo_root,
                    &branch,
                    trust_repository,
                );
                let _ = event_tx.blocking_send(AppEvent::LaneDestroyFinished(Box::new(
                    LaneCleanupResult {
                        operation_id,
                        via_managed_remove: true,
                        worktree_remove,
                        branch_delete,
                    },
                )));
            });
            if let Err(err) = spawned {
                self.queue_worktree_spawn_failure(AppEvent::LaneDestroyFinished(Box::new(
                    LaneCleanupResult {
                        operation_id,
                        via_managed_remove: true,
                        worktree_remove: Err(format!("could not start lane destroy: {err}")),
                        branch_delete: None,
                    },
                )));
            }
            return;
        }

        // Orphaned lane: the workspace is already gone — remove the checkout
        // directly and delete a lane-created branch.
        let command = crate::worktree::build_worktree_remove_command(
            &record.repo_root,
            &record.checkout_path,
            params.force,
            trust_repository,
        );
        let repo_root = record.repo_root.clone();
        let checkout_path = record.checkout_path.clone();
        let branch = record.branch.clone();
        let branch_created = record.branch_created;
        let spawned = crate::thread_spawn::spawn_named("herdr-lane-destroy", move || {
            let worktree_remove = crate::worktree::run_worktree_remove_command_with_recovery(
                &command,
                &repo_root,
                &checkout_path,
                params.force,
                trust_repository,
            );
            let branch_delete = delete_branch_if_created(
                branch_created,
                worktree_remove.is_ok(),
                &repo_root,
                &branch,
                trust_repository,
            );
            let _ = event_tx.blocking_send(AppEvent::LaneDestroyFinished(Box::new(
                LaneCleanupResult {
                    operation_id,
                    via_managed_remove: false,
                    worktree_remove,
                    branch_delete,
                },
            )));
        });
        if let Err(err) = spawned {
            self.queue_worktree_spawn_failure(AppEvent::LaneDestroyFinished(Box::new(
                LaneCleanupResult {
                    operation_id,
                    via_managed_remove: false,
                    worktree_remove: Err(format!("could not start lane destroy: {err}")),
                    branch_delete: None,
                },
            )));
        }
    }

    pub(crate) fn handle_api_lane_destroy_finished(&mut self, result: LaneCleanupResult) {
        let Some(pending) = self.pending_lane_destroys.remove(&result.operation_id) else {
            return;
        };
        match &result.worktree_remove {
            Err(err) => {
                Self::send_api_response(
                    pending.respond_to,
                    encode_error(
                        pending.id,
                        "lane_destroy_failed",
                        format!("lane destroy failed: {err}"),
                    ),
                );
            }
            Ok(()) => {
                self.lanes.remove(&pending.lane_id);
                self.state.mark_session_dirty();
                let branch_deleted = matches!(result.branch_delete, Some(Ok(())));
                self.emit_event(EventEnvelope {
                    event: EventKind::LaneDestroyed,
                    data: EventData::LaneDestroyed {
                        lane_id: pending.lane_id.clone(),
                        workspace_id: pending.workspace_id.clone(),
                        path: pending.path.display().to_string(),
                        branch: pending.branch.clone(),
                        branch_deleted,
                    },
                });
                Self::send_api_response(
                    pending.respond_to,
                    encode_success(
                        pending.id,
                        ResponseResult::LaneDestroyed {
                            lane_id: pending.lane_id,
                            workspace_id: pending.workspace_id,
                            path: pending.path.display().to_string(),
                            branch: pending.branch,
                            branch_deleted,
                        },
                    ),
                );
            }
        }
    }
}

enum LaneRecordStatus {
    Live(usize),
    Orphaned,
    Gone,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use crate::api::schema::{
        ErrorResponse, LaneCreateParams, LaneDestroyParams, Request, SuccessResponse,
    };
    use crate::config::Config;
    use crate::workspace::Workspace;

    fn unique_temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("herdr-{name}-{}-{nanos}", std::process::id()))
    }

    fn run_git(repo: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git command failed: git -C {} {}\n{}",
            repo.display(),
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn create_committed_repo(name: &str) -> PathBuf {
        let repo = unique_temp_path(name);
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init", "--quiet"]);
        run_git(&repo, &["config", "user.email", "herdr@example.invalid"]);
        run_git(&repo, &["config", "user.name", "Herdr Test"]);
        std::fs::write(repo.join("README.md"), "test\n").unwrap();
        run_git(&repo, &["add", "README.md"]);
        run_git(&repo, &["commit", "--quiet", "-m", "initial"]);
        repo
    }

    fn git_branch_list(repo: &Path, branch: &str) -> String {
        run_git(repo, &["branch", "--list", branch])
    }

    fn test_app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.default_shell = test_shell().into();
        app
    }

    #[cfg(windows)]
    fn test_shell() -> &'static str {
        "C:\\Windows\\System32\\whoami.exe"
    }

    #[cfg(not(windows))]
    fn test_shell() -> &'static str {
        "/usr/bin/true"
    }

    fn app_with_parent(repo: &Path) -> App {
        let mut app = test_app();
        let mut parent = Workspace::test_new("main");
        parent.identity_cwd = repo.to_path_buf();
        app.state.workspaces = vec![parent];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app
    }

    /// Drive a deferred `lane.create`/`lane.destroy` request to completion,
    /// pumping app events so worker-thread results (worktree add, managed
    /// worktree remove, rollback cleanup) are handled like the real app loop.
    fn drive_lane_request(app: &mut App, request: Request) -> String {
        let (respond_to, response_rx) = std::sync::mpsc::channel();
        assert!(app.handle_deferred_lane_api_request(request, respond_to));
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Ok(response) = response_rx.try_recv() {
                return response;
            }
            match app.event_rx.try_recv() {
                Ok(event) => app.handle_internal_event(event),
                Err(_) => {
                    assert!(
                        Instant::now() < deadline,
                        "timed out waiting for lane API response"
                    );
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    }

    fn lane_create_request(id: &str, params: LaneCreateParams) -> Request {
        Request {
            id: id.into(),
            method: Method::LaneCreate(params),
        }
    }

    fn lane_destroy_request(id: &str, lane: &str) -> Request {
        Request {
            id: id.into(),
            method: Method::LaneDestroy(LaneDestroyParams {
                lane: lane.into(),
                force: true,
                trust_repository: false,
            }),
        }
    }

    fn create_lane(app: &mut App, branch: &str) -> crate::api::schema::LaneInfo {
        let workspace_id = app.state.workspaces[0].id.clone();
        let response = drive_lane_request(
            app,
            lane_create_request(
                "create",
                LaneCreateParams {
                    workspace_id: Some(workspace_id),
                    branch: Some(branch.into()),
                    ..LaneCreateParams::default()
                },
            ),
        );
        let success: SuccessResponse = serde_json::from_str(&response)
            .unwrap_or_else(|err| panic!("lane.create did not succeed: {response} ({err})"));
        let ResponseResult::LaneCreated { lane } = success.result else {
            panic!("expected lane_created response: {response}");
        };
        *lane
    }

    fn lane_list(app: &mut App) -> Vec<crate::api::schema::LaneInfo> {
        let response = app.handle_lane_list("list".into());
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::LaneList { lanes } = success.result else {
            panic!("expected lane_list response: {response}");
        };
        lanes
    }

    fn lane_checkout(lane: &crate::api::schema::LaneInfo) -> PathBuf {
        PathBuf::from(&lane.path)
    }

    fn shutdown_runtimes(app: &mut App) {
        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
    }

    fn remove_checkout(repo: &Path, checkout: &Path) {
        if checkout.exists() {
            let remove =
                crate::worktree::build_worktree_remove_command(repo, checkout, true, false);
            let _ = crate::worktree::run_worktree_command(&remove);
            let _ = std::fs::remove_dir_all(checkout);
        }
    }

    #[tokio::test]
    async fn lane_create_opens_workspace_and_stamps_identity() {
        let repo = create_committed_repo("lane-create-repo");
        let worktree_root = unique_temp_path("lane-create-root");
        let mut app = app_with_parent(&repo);
        app.state.worktree_directory = worktree_root.clone();
        let workspace_id = app.state.workspaces[0].id.clone();

        let response = drive_lane_request(
            &mut app,
            lane_create_request(
                "req",
                LaneCreateParams {
                    workspace_id: Some(workspace_id),
                    branch: Some("lane/test-create".into()),
                    name: Some("Test Lane".into()),
                    ..LaneCreateParams::default()
                },
            ),
        );

        let success: SuccessResponse = serde_json::from_str(&response)
            .unwrap_or_else(|err| panic!("lane.create did not succeed: {response} ({err})"));
        let ResponseResult::LaneCreated { lane } = success.result else {
            panic!("expected lane_created response: {response}");
        };
        assert_eq!(lane.lane_id, "lane-1");
        assert_eq!(lane.name, "Test Lane");
        assert_eq!(lane.status, LaneStatus::Live);
        assert_eq!(lane.branch, "lane/test-create");
        assert!(lane.branch_created);
        assert!(lane.workspace.is_some());
        assert!(lane.root_pane.is_some());
        let worktree = lane.worktree.clone().expect("lane should carry worktree");
        assert!(worktree.is_linked_worktree);
        let checkout = lane_checkout(&lane);
        assert!(checkout.starts_with(&worktree_root));
        assert!(checkout.join("README.md").exists());
        assert_eq!(app.state.workspaces.len(), 2);
        assert!(app.lanes.contains_key("lane-1"));

        // `core.lane` identity tokens on the lane workspace...
        let tokens = app.state.workspaces[1].metadata_tokens.values();
        assert_eq!(
            tokens.get(LANE_TOKEN).map(String::as_str),
            Some("Test Lane")
        );
        assert_eq!(
            tokens.get(LANE_ID_TOKEN).map(String::as_str),
            Some("lane-1")
        );
        assert_eq!(
            tokens.get(LANE_BRANCH_TOKEN).map(String::as_str),
            Some("lane/test-create")
        );
        // ...and on the root pane terminal.
        let root_pane = app.state.workspaces[1].tabs[0].root_pane;
        let terminal_id = app.state.workspaces[1]
            .terminal_id(root_pane)
            .cloned()
            .expect("lane root pane should have a terminal");
        let pane_tokens = app.state.terminals[&terminal_id].metadata_tokens.values();
        assert_eq!(
            pane_tokens.get(LANE_ID_TOKEN).map(String::as_str),
            Some("lane-1")
        );
        // The lane workspace is a linked worktree of the source repo.
        assert!(app.state.workspaces[1]
            .worktree_space()
            .is_some_and(|space| space.is_linked_worktree));

        shutdown_runtimes(&mut app);
        remove_checkout(&repo, &checkout);
        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn lane_create_rolls_back_when_worktree_add_fails() {
        let repo = create_committed_repo("lane-add-fail-repo");
        let worktree_root = unique_temp_path("lane-add-fail-root");
        let mut app = app_with_parent(&repo);
        app.state.worktree_directory = worktree_root.clone();
        let workspace_id = app.state.workspaces[0].id.clone();

        let response = drive_lane_request(
            &mut app,
            lane_create_request(
                "req",
                LaneCreateParams {
                    workspace_id: Some(workspace_id),
                    branch: Some("lane/bad-base".into()),
                    base: Some("refs/does/not/exist".into()),
                    ..LaneCreateParams::default()
                },
            ),
        );

        let error: ErrorResponse = serde_json::from_str(&response)
            .unwrap_or_else(|err| panic!("expected error response: {response} ({err})"));
        assert_eq!(error.error.code, "lane_create_failed");
        assert!(
            error.error.message.contains(STEP_WORKTREE_ADD),
            "error should name the failed step: {}",
            error.error.message
        );
        assert_eq!(app.state.workspaces.len(), 1);
        assert!(app.lanes.is_empty());
        assert!(app.pending_lane_creates.is_empty());
        assert!(app.pending_api_worktree_creates.is_empty());
        assert!(git_branch_list(&repo, "lane/bad-base").trim().is_empty());

        shutdown_runtimes(&mut app);
        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn lane_create_agent_conflict_rolls_back_workspace_worktree_and_branch() {
        let repo = create_committed_repo("lane-agent-rb-repo");
        let worktree_root = unique_temp_path("lane-agent-rb-root");
        let mut app = app_with_parent(&repo);
        app.state.worktree_directory = worktree_root.clone();
        let workspace_id = app.state.workspaces[0].id.clone();
        // Pre-seed a managed agent named "dupe" on the parent root pane so the
        // lane agent start fails with a duplicate-name error.
        let root_pane = app.state.workspaces[0].tabs[0].root_pane;
        let terminal_id = app.state.workspaces[0]
            .terminal_id(root_pane)
            .cloned()
            .expect("parent root pane should have a terminal");
        app.state
            .terminals
            .get_mut(&terminal_id)
            .expect("parent terminal state")
            .begin_managed_agent(
                "dupe".into(),
                crate::detect::Agent::Claude,
                Instant::now(),
                Duration::ZERO,
                Duration::from_secs(30),
            );

        let response = drive_lane_request(
            &mut app,
            lane_create_request(
                "req",
                LaneCreateParams {
                    workspace_id: Some(workspace_id),
                    branch: Some("lane/agent-dupe".into()),
                    agent: Some("claude".into()),
                    agent_name: Some("dupe".into()),
                    ..LaneCreateParams::default()
                },
            ),
        );

        let error: ErrorResponse = serde_json::from_str(&response)
            .unwrap_or_else(|err| panic!("expected error response: {response} ({err})"));
        assert_eq!(error.error.code, "lane_create_failed");
        assert!(
            error.error.message.contains(STEP_AGENT_START),
            "error should name the failed step: {}",
            error.error.message
        );
        assert!(
            error.error.message.contains("rolled back"),
            "error should report cleanup: {}",
            error.error.message
        );
        // Workspace, checkout, and lane-created branch are all gone.
        assert_eq!(app.state.workspaces.len(), 1);
        assert!(app.lanes.is_empty());
        assert!(app.pending_lane_creates.is_empty());
        let checkout = crate::worktree::default_checkout_path(
            &worktree_root,
            &repo.file_name().unwrap().to_string_lossy(),
            "lane/agent-dupe",
        );
        assert!(
            !checkout.exists(),
            "lane checkout should be removed: {}",
            checkout.display()
        );
        assert!(git_branch_list(&repo, "lane/agent-dupe").trim().is_empty());

        shutdown_runtimes(&mut app);
        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn lane_list_and_destroy_roundtrip() {
        let repo = create_committed_repo("lane-destroy-repo");
        let worktree_root = unique_temp_path("lane-destroy-root");
        let mut app = app_with_parent(&repo);
        app.state.worktree_directory = worktree_root.clone();

        let lane = create_lane(&mut app, "lane/roundtrip");
        let checkout = lane_checkout(&lane);
        assert!(checkout.exists());

        let lanes = lane_list(&mut app);
        assert_eq!(lanes.len(), 1);
        assert_eq!(lanes[0].lane_id, "lane-1");
        assert_eq!(lanes[0].status, LaneStatus::Live);

        let response = drive_lane_request(&mut app, lane_destroy_request("destroy", "lane-1"));
        let success: SuccessResponse = serde_json::from_str(&response)
            .unwrap_or_else(|err| panic!("lane.destroy did not succeed: {response} ({err})"));
        let ResponseResult::LaneDestroyed {
            lane_id,
            branch_deleted,
            ..
        } = success.result
        else {
            panic!("expected lane_destroyed response: {response}");
        };
        assert_eq!(lane_id, "lane-1");
        assert!(branch_deleted, "lane-created branch should be deleted");
        assert!(!checkout.exists());
        assert!(git_branch_list(&repo, "lane/roundtrip").trim().is_empty());
        assert!(lane_list(&mut app).is_empty());
        assert_eq!(app.state.workspaces.len(), 1);

        // Destroying again reports lane_not_found.
        let response = drive_lane_request(&mut app, lane_destroy_request("destroy2", "lane-1"));
        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "lane_not_found");

        shutdown_runtimes(&mut app);
        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn lane_destroy_preserves_preexisting_branch() {
        let repo = create_committed_repo("lane-keep-branch-repo");
        let worktree_root = unique_temp_path("lane-keep-branch-root");
        let mut app = app_with_parent(&repo);
        app.state.worktree_directory = worktree_root.clone();
        run_git(&repo, &["branch", "keep-me"]);

        let lane = create_lane(&mut app, "keep-me");
        assert!(!lane.branch_created, "existing branch must not be owned");
        let checkout = lane_checkout(&lane);

        let response = drive_lane_request(&mut app, lane_destroy_request("destroy", "lane-1"));
        let success: SuccessResponse = serde_json::from_str(&response)
            .unwrap_or_else(|err| panic!("lane.destroy did not succeed: {response} ({err})"));
        let ResponseResult::LaneDestroyed { branch_deleted, .. } = success.result else {
            panic!("expected lane_destroyed response: {response}");
        };
        assert!(!branch_deleted);
        assert!(!checkout.exists());
        assert_eq!(git_branch_list(&repo, "keep-me").trim(), "keep-me");

        shutdown_runtimes(&mut app);
        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn lane_destroy_removes_orphaned_checkout() {
        let repo = create_committed_repo("lane-orphan-repo");
        let worktree_root = unique_temp_path("lane-orphan-root");
        let mut app = app_with_parent(&repo);
        app.state.worktree_directory = worktree_root.clone();

        let lane = create_lane(&mut app, "lane/orphan");
        let checkout = lane_checkout(&lane);
        // Simulate the lane workspace being closed outside the lane lifecycle:
        // shut down its runtime and drop it from app state.
        let lane_ws = app.state.workspaces.remove(1);
        for tab in &lane_ws.tabs {
            for pane in tab.panes.values() {
                if let Some(runtime) = app.terminal_runtimes.remove(&pane.attached_terminal_id) {
                    runtime.shutdown();
                }
                app.state.terminals.remove(&pane.attached_terminal_id);
            }
        }

        let lanes = lane_list(&mut app);
        assert_eq!(lanes.len(), 1);
        assert_eq!(lanes[0].status, LaneStatus::Orphaned);
        assert!(lanes[0].workspace.is_none());

        let response = drive_lane_request(&mut app, lane_destroy_request("destroy", "lane-1"));
        let success: SuccessResponse = serde_json::from_str(&response)
            .unwrap_or_else(|err| panic!("lane.destroy did not succeed: {response} ({err})"));
        let ResponseResult::LaneDestroyed {
            workspace_id,
            branch_deleted,
            ..
        } = success.result
        else {
            panic!("expected lane_destroyed response: {response}");
        };
        assert!(workspace_id.is_none());
        assert!(branch_deleted);
        assert!(!checkout.exists());
        assert!(git_branch_list(&repo, "lane/orphan").trim().is_empty());
        assert!(lane_list(&mut app).is_empty());

        shutdown_runtimes(&mut app);
        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn lane_create_rejects_agent_metadata_without_agent() {
        let mut app = test_app();
        let response = drive_lane_request(
            &mut app,
            lane_create_request(
                "req",
                LaneCreateParams {
                    agent_name: Some("solo".into()),
                    ..LaneCreateParams::default()
                },
            ),
        );
        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "invalid_request");
    }

    #[test]
    fn lane_create_rejects_invalid_agent_name() {
        let mut app = test_app();
        let response = drive_lane_request(
            &mut app,
            lane_create_request(
                "req",
                LaneCreateParams {
                    agent: Some("claude".into()),
                    agent_name: Some("Bad Name!".into()),
                    ..LaneCreateParams::default()
                },
            ),
        );
        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "invalid_agent_name");
    }

    #[test]
    fn lane_create_rejects_unsupported_agent_kind() {
        let mut app = test_app();
        let response = drive_lane_request(
            &mut app,
            lane_create_request(
                "req",
                LaneCreateParams {
                    agent: Some("definitely-not-an-agent".into()),
                    ..LaneCreateParams::default()
                },
            ),
        );
        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "unsupported_agent_kind");
    }

    #[test]
    fn lane_destroy_unknown_lane_reports_not_found() {
        let mut app = test_app();
        let response = drive_lane_request(&mut app, lane_destroy_request("req", "lane-99"));
        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "lane_not_found");
    }

    #[test]
    fn normalize_lane_name_trims_and_strips_controls() {
        assert_eq!(
            normalize_lane_name("  hello lane  "),
            Some("hello lane".into())
        );
        assert_eq!(normalize_lane_name("a\u{0}b\u{7f}"), Some("ab".into()));
        assert_eq!(normalize_lane_name("   \u{1b}   "), None);
        assert_eq!(normalize_lane_name(""), None);
    }

    #[test]
    fn default_lane_agent_name_produces_valid_names() {
        for seed in ["My Lane!", "lane", "123 start digit", "___", "UPPER case"] {
            let name = default_lane_agent_name(seed);
            assert!(
                crate::app::agents::valid_agent_name(&name),
                "derived agent name {name:?} from {seed:?} must be valid"
            );
        }
    }

    #[test]
    fn rollback_error_message_reports_step_and_cleanup() {
        let cleanup = LaneCleanupResult {
            operation_id: 7,
            via_managed_remove: true,
            worktree_remove: Ok(()),
            branch_delete: Some(Ok(())),
        };
        let message = rollback_error_message(STEP_AGENT_START, "duplicate name", &cleanup);
        assert!(message.contains("\"agent_start\""));
        assert!(message.contains("duplicate name"));
        assert!(message.contains("workspace"));
        assert!(message.contains("worktree"));
        assert!(message.contains("branch"));

        let cleanup = LaneCleanupResult {
            operation_id: 7,
            via_managed_remove: false,
            worktree_remove: Err("checkout still dirty".into()),
            branch_delete: None,
        };
        let message = rollback_error_message(STEP_WORKSPACE_CREATE, "pty failed", &cleanup);
        assert!(message.contains("\"workspace_create\""));
        assert!(message.contains("rollback removed nothing"));
        assert!(message.contains("worktree remove failed: checkout still dirty"));
    }

    #[test]
    fn managed_remove_outcome_parses_success_and_error() {
        assert!(
            managed_remove_outcome(Some(r#"{"id":"x","result":{"kind":"ok"}}"#.into())).is_ok()
        );
        assert_eq!(
            managed_remove_outcome(Some(
                r#"{"id":"x","error":{"code":"c","message":"boom"}}"#.into()
            )),
            Err("boom".to_string())
        );
        assert!(managed_remove_outcome(Some("not json".into())).is_err());
        assert!(managed_remove_outcome(None).is_err());
    }

    #[test]
    fn lane_record_snapshot_round_trip() {
        let record = LaneRecord {
            lane_id: "lane-3".into(),
            name: "feat".into(),
            branch: "lane/feat".into(),
            branch_created: true,
            checkout_path: PathBuf::from("checkout"),
            repo_root: PathBuf::from("repo"),
            repo_name: "repo".into(),
            workspace_internal_id: Some("w9".into()),
            trust_repository: true,
            created_unix_ms: 1_234,
        };
        let json = serde_json::to_string(&record.to_snapshot()).unwrap();
        let snapshot: crate::persist::LaneSnapshot = serde_json::from_str(&json).unwrap();
        let restored = LaneRecord::from_snapshot(&snapshot);
        assert_eq!(restored.lane_id, record.lane_id);
        assert_eq!(restored.name, record.name);
        assert_eq!(restored.branch, record.branch);
        assert_eq!(restored.branch_created, record.branch_created);
        assert_eq!(restored.checkout_path, record.checkout_path);
        assert_eq!(restored.repo_root, record.repo_root);
        assert_eq!(restored.repo_name, record.repo_name);
        assert_eq!(restored.workspace_internal_id, record.workspace_internal_id);
        assert_eq!(restored.trust_repository, record.trust_repository);
        assert_eq!(restored.created_unix_ms, record.created_unix_ms);
    }

    #[test]
    fn next_id_after_restore_skips_past_existing_ids() {
        let record = |lane_id: &str| LaneRecord {
            lane_id: lane_id.into(),
            name: "n".into(),
            branch: "b".into(),
            branch_created: false,
            checkout_path: PathBuf::new(),
            repo_root: PathBuf::new(),
            repo_name: "r".into(),
            workspace_internal_id: None,
            trust_repository: false,
            created_unix_ms: 0,
        };
        assert_eq!(LaneRecord::next_id_after_restore(&HashMap::new()), 1);
        let lanes: HashMap<String, LaneRecord> = ["lane-2", "lane-7", "other"]
            .into_iter()
            .map(|id| (id.to_string(), record(id)))
            .collect();
        assert_eq!(LaneRecord::next_id_after_restore(&lanes), 8);
    }

    #[tokio::test]
    async fn lane_create_emits_lane_created_event() {
        let repo = create_committed_repo("lane-evt-create-repo");
        let worktree_root = unique_temp_path("lane-evt-create-root");
        let mut app = app_with_parent(&repo);
        app.state.worktree_directory = worktree_root.clone();

        let baseline = app.event_hub.current_sequence();
        let lane = create_lane(&mut app, "lane/evt-create");
        let checkout = lane_checkout(&lane);

        let events = app.event_hub.events_after(baseline);
        let created: Vec<_> = events
            .iter()
            .filter(|(_, event)| event.event == EventKind::LaneCreated)
            .collect();
        assert_eq!(created.len(), 1);
        let EventData::LaneCreated { lane: emitted } = &created[0].1.data else {
            panic!("expected lane_created event data");
        };
        assert_eq!(emitted.lane_id, lane.lane_id);
        assert_eq!(emitted.name, lane.name);
        assert_eq!(emitted.branch, "lane/evt-create");
        assert_eq!(emitted.status, LaneStatus::Live);
        assert_eq!(emitted.path, lane.path);

        shutdown_runtimes(&mut app);
        remove_checkout(&repo, &checkout);
        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn lane_destroy_emits_lane_destroyed_event() {
        let repo = create_committed_repo("lane-evt-destroy-repo");
        let worktree_root = unique_temp_path("lane-evt-destroy-root");
        let mut app = app_with_parent(&repo);
        app.state.worktree_directory = worktree_root.clone();

        let lane = create_lane(&mut app, "lane/evt-destroy");
        let workspace_id = lane.workspace.as_ref().map(|w| w.workspace_id.clone());
        let baseline = app.event_hub.current_sequence();

        let response = drive_lane_request(&mut app, lane_destroy_request("destroy", "lane-1"));
        let success: SuccessResponse = serde_json::from_str(&response)
            .unwrap_or_else(|err| panic!("lane.destroy did not succeed: {response} ({err})"));
        let ResponseResult::LaneDestroyed { .. } = success.result else {
            panic!("expected lane_destroyed response: {response}");
        };

        let events = app.event_hub.events_after(baseline);
        let destroyed: Vec<_> = events
            .iter()
            .filter(|(_, event)| event.event == EventKind::LaneDestroyed)
            .collect();
        assert_eq!(destroyed.len(), 1);
        let EventData::LaneDestroyed {
            lane_id,
            workspace_id: event_workspace_id,
            path,
            branch,
            branch_deleted,
        } = &destroyed[0].1.data
        else {
            panic!("expected lane_destroyed event data");
        };
        assert_eq!(lane_id, "lane-1");
        assert_eq!(event_workspace_id, &workspace_id);
        assert_eq!(path, &lane.path);
        assert_eq!(branch, "lane/evt-destroy");
        assert!(branch_deleted);

        shutdown_runtimes(&mut app);
        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
    }
}
