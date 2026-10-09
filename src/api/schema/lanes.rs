use serde::{Deserialize, Serialize};

use super::{AgentInfo, PaneInfo, WorkspaceInfo, WorktreeInfo};

/// Parameters for `lane.create`.
///
/// One atomic call that materializes a worktree, opens a dedicated workspace,
/// stamps `core.lane` identity tokens, and optionally submits a run command
/// and/or starts an agent on the lane's root pane. If any step fails, every
/// resource created earlier in the chain is rolled back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct LaneCreateParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_args: Vec<String>,
    #[serde(default)]
    pub focus: bool,
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub trust_repository: bool,
}

/// Parameters for `lane.destroy`: removes the lane's worktree (and workspace
/// when still open) and deletes the lane branch when `lane.create` created it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LaneDestroyParams {
    pub lane: String,
    #[serde(default)]
    pub force: bool,
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub trust_repository: bool,
}

/// Lifecycle state of a lane as reported by `lane.list` and `lane.create`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LaneStatus {
    /// Workspace open — fully usable lane.
    Live,
    /// Workspace closed (by the user or a failed restore); the worktree
    /// checkout and branch still exist on disk.
    Orphaned,
}

/// One lane entry as reported by `lane.create` and `lane.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LaneInfo {
    /// Stable lane identity, e.g. `"lane-1"`. Also stamped as the `lane_id`
    /// token (source `core.lane`) on the lane workspace and its root pane.
    pub lane_id: String,
    /// Human label (`name` param or derived from the repo/branch).
    pub name: String,
    pub status: LaneStatus,
    /// Branch checked out by the lane worktree.
    pub branch: String,
    /// True when `lane.create` created the branch (it did not exist locally
    /// beforehand). Only such branches are deleted by rollback/`lane.destroy`.
    pub branch_created: bool,
    /// Worktree checkout directory.
    pub path: String,
    /// Canonical path of the repository the worktree belongs to.
    pub repo_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceInfo>,
    /// Root pane of the lane workspace (where `run`/`agent` were applied).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_pane: Option<PaneInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<WorktreeInfo>,
    /// Agent started on the root pane, when requested and successful.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentInfo>,
    /// Unix milliseconds when the lane was created.
    pub created_unix_ms: u64,
}
