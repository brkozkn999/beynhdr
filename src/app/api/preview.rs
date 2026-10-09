//! `preview.open` / `preview.set` / `preview.close` — read-only display panes
//! with no PTY and no child process.
//!
//! A preview pane is a normal layout pane whose `TerminalState` carries a
//! [`crate::preview::PreviewState`] payload instead of a terminal runtime. It
//! participates in splits, tabs, and the plugin-style zoomed overlay
//! placement, and it shows up in `pane.list`/`pane.info` with a `preview`
//! marker. Runtime-dependent pane operations reject it with `preview_pane`.

use ratatui::layout::Direction;

use super::responses::{encode_error, encode_success};
use crate::api::schema::{
    EventData, EventEnvelope, EventKind, PaneTarget, PreviewCloseParams, PreviewOpenParams,
    PreviewPanePlacement, PreviewSetParams, ResponseResult, SplitDirection,
    PREVIEW_CONTENT_MAX_BYTES,
};
use crate::app::{App, Mode, OverlayPaneState};
use crate::preview::{validate_preview_content_size, PreviewState};

const CONTENT_TOO_LARGE: &str = "content_too_large";

impl App {
    pub(super) fn handle_preview_open(&mut self, id: String, params: PreviewOpenParams) -> String {
        let PreviewOpenParams {
            title,
            content_type,
            content,
            content_file,
            placement,
            workspace_id,
            target_pane_id,
            direction,
            focus,
        } = params;
        let Some(title) = normalize_preview_title(&title) else {
            return encode_error(id, "invalid_params", "preview title is empty");
        };
        let content = match resolve_content(&id, content, content_file) {
            Ok(Some(content)) => content,
            Ok(None) => {
                return encode_error(id, "invalid_params", "content or content_file is required")
            }
            Err(response) => return response,
        };
        let placement = placement.unwrap_or_default();
        if let Err(response) = validate_placement(
            &id,
            placement,
            workspace_id.is_some(),
            target_pane_id.is_some() || direction.is_some(),
        ) {
            return response;
        }
        let preview = PreviewState::new(content, content_type);

        match placement {
            PreviewPanePlacement::Overlay => self.open_preview_overlay(id, title, preview),
            PreviewPanePlacement::Split => {
                self.open_preview_split(id, title, preview, target_pane_id, direction, focus)
            }
            PreviewPanePlacement::Tab => {
                self.open_preview_tab(id, title, preview, workspace_id, focus)
            }
        }
    }

    pub(super) fn handle_preview_set(&mut self, id: String, params: PreviewSetParams) -> String {
        let Some((ws_idx, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return preview_pane_not_found(&id, &params.pane_id);
        };
        let content = match resolve_content(&id, params.content, params.content_file) {
            Ok(content) => content,
            Err(response) => return response,
        };
        let title = match params.title.as_deref() {
            Some(title) => match normalize_preview_title(title) {
                Some(title) => Some(title),
                None => return encode_error(id, "invalid_params", "preview title is empty"),
            },
            None => None,
        };

        let Some(terminal_id) = self.state.terminal_id_for_pane(ws_idx, pane_id) else {
            return preview_pane_not_found(&id, &params.pane_id);
        };
        let Some(terminal) = self.state.terminals.get_mut(&terminal_id) else {
            return preview_pane_not_found(&id, &params.pane_id);
        };
        let Some(preview) = terminal.preview.as_mut() else {
            return encode_error(
                id,
                "not_preview_pane",
                format!("pane {} is not a preview pane", params.pane_id),
            );
        };
        match (content, params.content_type) {
            (Some(content), content_type) => {
                preview.set_content(content, content_type.unwrap_or(preview.content_type));
            }
            (None, Some(content_type)) => {
                // Re-interpret existing content under the new type.
                let existing = preview.content.clone();
                preview.set_content(existing, content_type);
            }
            (None, None) => {}
        }
        if let Some(title) = title {
            terminal.set_manual_label(title);
        }
        terminal.revision = terminal.revision.wrapping_add(1);
        self.state.mark_session_dirty();
        self.render_dirty.request_generic();
        self.render_notify.notify_one();
        self.emit_pane_updated(ws_idx, pane_id);

        let Some(pane) = self.pane_info(ws_idx, pane_id) else {
            return preview_pane_not_found(&id, &params.pane_id);
        };
        encode_success(id, ResponseResult::PreviewUpdated { pane })
    }

    pub(super) fn handle_preview_close(
        &mut self,
        id: String,
        params: PreviewCloseParams,
    ) -> String {
        let Some((ws_idx, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return preview_pane_not_found(&id, &params.pane_id);
        };
        if !self.is_preview_pane(ws_idx, pane_id) {
            return encode_error(
                id,
                "not_preview_pane",
                format!("pane {} is not a preview pane", params.pane_id),
            );
        }
        let Some(public_pane_id) = self.public_pane_id(ws_idx, pane_id) else {
            return preview_pane_not_found(&id, &params.pane_id);
        };

        // `close_pane` restores overlay focus/zoom for preview panes itself:
        // previews never emit `PaneDied`, which is what normally drives it.
        let target = PaneTarget {
            pane_id: params.pane_id,
        };
        if let Err(response) = self.close_pane(id.clone(), &target) {
            return response;
        }
        encode_success(
            id,
            ResponseResult::PreviewClosed {
                pane_id: public_pane_id,
            },
        )
    }

    // ------------------------------------------------------------------
    // Placement handlers
    // ------------------------------------------------------------------

    fn open_preview_overlay(&mut self, id: String, title: String, preview: PreviewState) -> String {
        let Some(ws_idx) = self.state.active else {
            return encode_error(id, "no_active_workspace", "no active workspace");
        };
        let previous_focus_target = self.state.current_pane_focus_target();
        let previous_focus = match self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.focused_pane_id())
        {
            Some(pane_id) => pane_id,
            None => return encode_error(id, "no_active_pane", "no active pane"),
        };
        let cwd = self.preview_cwd(ws_idx, previous_focus);

        let (tab_idx, surface, workspace_id) = {
            let Some(ws) = self.state.workspaces.get_mut(ws_idx) else {
                return encode_error(id, "workspace_not_found", "workspace not found");
            };
            let previous_zoomed = ws.active_tab().map(|tab| tab.zoomed).unwrap_or(false);
            let Some((tab_idx, surface)) =
                ws.split_surface_pane(previous_focus, Direction::Horizontal, Some(cwd), true)
            else {
                return encode_error(id, "preview_open_failed", "split target disappeared");
            };
            if let Some(tab) = ws.tabs.get_mut(tab_idx) {
                tab.zoomed = true;
            }
            self.overlay_panes.insert(
                surface.pane_id,
                OverlayPaneState {
                    ws_idx,
                    tab_idx,
                    previous_focus,
                    previous_zoomed,
                    temp_files: Vec::new(),
                },
            );
            (tab_idx, surface, ws.id.clone())
        };

        let new_focus_target = crate::app::state::PaneFocusTarget {
            workspace_id,
            pane_id: surface.pane_id,
        };
        if previous_focus_target.as_ref() != Some(&new_focus_target) {
            self.state.previous_pane_focus = previous_focus_target;
        }
        self.state.switch_workspace_tab(ws_idx, tab_idx);
        self.state.mode = Mode::Terminal;
        self.finish_preview_open(id, ws_idx, Some(tab_idx), None, surface, title, preview)
    }

    fn open_preview_split(
        &mut self,
        id: String,
        title: String,
        preview: PreviewState,
        target_pane_id: Option<String>,
        direction: Option<SplitDirection>,
        focus: bool,
    ) -> String {
        let target_pane_id = target_pane_id.or_else(|| {
            self.state.active.and_then(|ws_idx| {
                self.state
                    .workspaces
                    .get(ws_idx)
                    .and_then(|ws| ws.focused_pane_id())
                    .and_then(|pane_id| self.public_pane_id(ws_idx, pane_id))
            })
        });
        let Some(target_pane_id) = target_pane_id else {
            return encode_error(id, "no_active_pane", "no active pane");
        };
        let Some((ws_idx, target_pane)) = self.parse_pane_id(&target_pane_id) else {
            return preview_pane_not_found(&id, &target_pane_id);
        };
        let direction = match direction.unwrap_or(SplitDirection::Right) {
            SplitDirection::Right => Direction::Horizontal,
            SplitDirection::Down => Direction::Vertical,
        };
        let cwd = self.preview_cwd(ws_idx, target_pane);
        let previous_focus = self.state.current_pane_focus_target();
        let Some(ws) = self.state.workspaces.get_mut(ws_idx) else {
            return encode_error(id, "workspace_not_found", "workspace not found");
        };
        let Some((tab_idx, surface)) =
            ws.split_surface_pane(target_pane, direction, Some(cwd), focus)
        else {
            return preview_pane_not_found(&id, &target_pane_id);
        };
        if focus {
            self.state.switch_workspace_tab(ws_idx, tab_idx);
            self.state
                .record_pane_focus_change(previous_focus, ws_idx, surface.pane_id);
            self.state.mode = Mode::Terminal;
        }
        self.finish_preview_open(id, ws_idx, Some(tab_idx), None, surface, title, preview)
    }

    fn open_preview_tab(
        &mut self,
        id: String,
        title: String,
        preview: PreviewState,
        workspace_id: Option<String>,
        focus: bool,
    ) -> String {
        let ws_idx = match workspace_id.as_deref() {
            Some(workspace_id) => match self.parse_workspace_id(workspace_id) {
                Some(ws_idx) => ws_idx,
                None => return encode_error(id, "workspace_not_found", "workspace not found"),
            },
            None => match self.state.active {
                Some(ws_idx) => ws_idx,
                None => return encode_error(id, "no_active_workspace", "no active workspace"),
            },
        };
        let cwd = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.focused_pane_id())
            .and_then(|pane_id| self.preview_cwd_opt(ws_idx, pane_id))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| "/".into()));
        let Some(ws) = self.state.workspaces.get_mut(ws_idx) else {
            return encode_error(id, "workspace_not_found", "workspace not found");
        };
        let (tab_idx, surface_terminal) = ws.create_surface_tab(cwd);
        let pane_id = ws.tabs[tab_idx].root_pane;
        if focus {
            self.state.switch_workspace_tab(ws_idx, tab_idx);
            self.state.mode = Mode::Terminal;
        }
        let surface = crate::workspace::SurfacePane {
            pane_id,
            terminal: surface_terminal,
        };
        self.finish_preview_open(
            id,
            ws_idx,
            Some(tab_idx),
            Some(tab_idx),
            surface,
            title,
            preview,
        )
    }

    /// Shared tail for all placements: attach payload + label to the surface
    /// terminal, register state, emit events.
    fn finish_preview_open(
        &mut self,
        id: String,
        ws_idx: usize,
        layout_tab_idx: Option<usize>,
        created_tab_idx: Option<usize>,
        surface: crate::workspace::SurfacePane,
        title: String,
        preview: PreviewState,
    ) -> String {
        let mut terminal = surface.terminal;
        terminal.set_manual_label(title);
        terminal.set_preview(preview);
        let terminal_id = terminal.id.clone();
        self.state
            .remove_alias_shadowed_by_new_pane(surface.pane_id);
        self.state.terminals.insert(terminal_id, terminal);
        if let Some(tab_idx) = created_tab_idx {
            if let Some(tab) = self.tab_info(ws_idx, tab_idx) {
                self.emit_event(EventEnvelope {
                    event: EventKind::TabCreated,
                    data: EventData::TabCreated { tab },
                });
            }
        }
        self.schedule_session_save();
        let Some(pane) = self.pane_info(ws_idx, surface.pane_id) else {
            return encode_error(id, "preview_open_failed", "preview pane disappeared");
        };
        self.emit_event(EventEnvelope {
            event: EventKind::PaneCreated,
            data: EventData::PaneCreated { pane: pane.clone() },
        });
        if let Some(tab_idx) = layout_tab_idx {
            self.emit_layout_updated_event(ws_idx, tab_idx);
        }
        self.render_dirty.request_generic();
        self.render_notify.notify_one();
        encode_success(id, ResponseResult::PreviewOpened { pane })
    }

    /// Whether `pane_id` is a runtime-less preview surface.
    pub(super) fn is_preview_pane(&self, ws_idx: usize, pane_id: crate::layout::PaneId) -> bool {
        self.state
            .terminal_id_for_pane(ws_idx, pane_id)
            .and_then(|terminal_id| self.state.terminals.get(&terminal_id))
            .is_some_and(|terminal| terminal.is_preview())
    }

    /// `Some(encoded_error)` when `pane_id` is a preview surface — used by
    /// runtime-dependent pane operations so they fail with `preview_pane`
    /// instead of the misleading "no runtime ⇒ pane not found".
    pub(super) fn preview_pane_rejection(
        &self,
        id: &str,
        ws_idx: usize,
        pane_id: crate::layout::PaneId,
        public_pane_id: &str,
    ) -> Option<String> {
        let terminal_id = self.state.terminal_id_for_pane(ws_idx, pane_id)?;
        let terminal = self.state.terminals.get(&terminal_id)?;
        terminal.is_preview().then(|| {
            encode_error(
                id.to_string(),
                "preview_pane",
                format!("pane {public_pane_id} is a read-only preview pane"),
            )
        })
    }

    /// cwd for the new preview surface: inherit the target pane's terminal cwd
    /// when known (metadata only — previews never spawn in it).
    fn preview_cwd(&self, ws_idx: usize, pane_id: crate::layout::PaneId) -> std::path::PathBuf {
        self.preview_cwd_opt(ws_idx, pane_id)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| "/".into()))
    }

    fn preview_cwd_opt(
        &self,
        ws_idx: usize,
        pane_id: crate::layout::PaneId,
    ) -> Option<std::path::PathBuf> {
        let terminal_id = self.state.terminal_id_for_pane(ws_idx, pane_id)?;
        Some(self.state.terminals.get(&terminal_id)?.cwd.clone())
    }
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

fn preview_pane_not_found(id: &str, pane_id: &str) -> String {
    encode_error(
        id.to_string(),
        "pane_not_found",
        format!("pane {pane_id} not found"),
    )
}

fn normalize_preview_title(title: &str) -> Option<String> {
    let normalized: String = title
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(80)
        .collect();
    let normalized = normalized.trim().to_string();
    (!normalized.is_empty()).then_some(normalized)
}

/// `Ok(None)` means "caller did not supply content" — legal for `preview.set`
/// but rejected by `preview.open`.
fn resolve_content(
    id: &str,
    content: Option<String>,
    content_file: Option<String>,
) -> Result<Option<String>, String> {
    match (content, content_file) {
        (Some(_), Some(_)) => Err(encode_error(
            id.to_string(),
            "invalid_params",
            "provide either content or content_file, not both",
        )),
        (None, None) => Ok(None),
        (Some(content), None) => {
            if validate_preview_content_size(&content).is_err() {
                return Err(encode_error(
                    id.to_string(),
                    CONTENT_TOO_LARGE,
                    format!("preview content exceeds {PREVIEW_CONTENT_MAX_BYTES} bytes"),
                ));
            }
            Ok(Some(content))
        }
        (None, Some(path)) => {
            let bytes = std::fs::read(&path).map_err(|err| {
                encode_error(
                    id.to_string(),
                    "content_file_read_failed",
                    format!("could not read content_file {path}: {err}"),
                )
            })?;
            if bytes.len() > PREVIEW_CONTENT_MAX_BYTES {
                return Err(encode_error(
                    id.to_string(),
                    CONTENT_TOO_LARGE,
                    format!("preview content exceeds {PREVIEW_CONTENT_MAX_BYTES} bytes"),
                ));
            }
            String::from_utf8(bytes).map(Some).map_err(|_| {
                encode_error(
                    id.to_string(),
                    "invalid_params",
                    format!("content_file {path} is not valid UTF-8"),
                )
            })
        }
    }
}

fn validate_placement(
    id: &str,
    placement: PreviewPanePlacement,
    has_workspace_id: bool,
    has_split_targeting: bool,
) -> Result<(), String> {
    if has_workspace_id && placement != PreviewPanePlacement::Tab {
        return Err(encode_error(
            id.to_string(),
            "invalid_params",
            "workspace_id is only valid with placement \"tab\"",
        ));
    }
    if has_split_targeting && placement != PreviewPanePlacement::Split {
        return Err(encode_error(
            id.to_string(),
            "invalid_params",
            "target_pane_id/direction are only valid with placement \"split\"",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::schema::{
        ErrorResponse, Method, PaneReadParams, PaneScrollParams, PaneSendKeysParams,
        PaneSendTextParams, PreviewContentType, ReadFormat, ReadSource, Request, SuccessResponse,
    };
    use crate::config::Config;
    use crate::workspace::Workspace;

    fn test_app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("preview")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.mode = Mode::Terminal;
        app
    }

    fn request(method: Method) -> Request {
        Request {
            id: "r".into(),
            method,
        }
    }

    fn open_params(content: &str) -> PreviewOpenParams {
        PreviewOpenParams {
            title: "Preview".into(),
            content_type: PreviewContentType::Text,
            content: Some(content.into()),
            content_file: None,
            placement: None,
            workspace_id: None,
            target_pane_id: None,
            direction: None,
            focus: false,
        }
    }

    fn error_code(response: &str) -> String {
        serde_json::from_str::<ErrorResponse>(response)
            .unwrap()
            .error
            .code
    }

    fn open_pane_id(app: &mut App, params: PreviewOpenParams) -> String {
        let response = app.handle_api_request(request(Method::PreviewOpen(params)));
        let response: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::PreviewOpened { pane } = response.result else {
            panic!("expected preview_opened");
        };
        pane.pane_id
    }

    fn root_pane_public_id(app: &App) -> String {
        let pane_id = app.state.workspaces[0].tabs[0].root_pane;
        app.public_pane_id(0, pane_id).unwrap()
    }

    fn preview_terminal_id(app: &App, public_pane_id: &str) -> crate::terminal::TerminalId {
        let (ws_idx, pane_id) = app.parse_pane_id(public_pane_id).unwrap();
        app.state.terminal_id_for_pane(ws_idx, pane_id).unwrap()
    }

    #[test]
    fn open_overlay_marks_preview_without_runtime() {
        let mut app = test_app();
        let pane_id = open_pane_id(&mut app, open_params("hello preview"));
        let (ws_idx, internal_id) = app.parse_pane_id(&pane_id).unwrap();

        assert!(app.is_preview_pane(ws_idx, internal_id));
        assert!(
            app.state
                .runtime_for_pane_in_workspace(&app.terminal_runtimes, ws_idx, internal_id)
                .is_none(),
            "preview panes must not own a terminal runtime"
        );
        assert_eq!(app.state.workspaces[0].tabs[0].layout.pane_count(), 2);
        assert!(app.state.workspaces[0].tabs[0].zoomed);
        assert!(app.overlay_panes.contains_key(&internal_id));
        assert_eq!(app.state.workspaces[0].focused_pane_id(), Some(internal_id));
    }

    #[test]
    fn open_split_places_preview_in_layout() {
        let mut app = test_app();
        let mut params = open_params("split body");
        params.placement = Some(PreviewPanePlacement::Split);
        params.direction = Some(SplitDirection::Down);
        params.focus = true;

        let pane_id = open_pane_id(&mut app, params);
        let (ws_idx, internal_id) = app.parse_pane_id(&pane_id).unwrap();

        assert_eq!(app.state.workspaces[0].tabs[0].layout.pane_count(), 2);
        assert!(!app.state.workspaces[0].tabs[0].zoomed);
        assert!(app.overlay_panes.is_empty());
        assert_eq!(app.state.workspaces[0].focused_pane_id(), Some(internal_id));
        assert!(app.is_preview_pane(ws_idx, internal_id));
    }

    #[test]
    fn open_tab_creates_tab_with_preview() {
        let mut app = test_app();
        let mut params = open_params("tab body");
        params.placement = Some(PreviewPanePlacement::Tab);

        let pane_id = open_pane_id(&mut app, params);
        assert_eq!(app.state.workspaces[0].tabs.len(), 2);
        let (ws_idx, internal_id) = app.parse_pane_id(&pane_id).unwrap();
        assert_eq!(ws_idx, 0);
        assert!(app.is_preview_pane(ws_idx, internal_id));
        assert_eq!(app.state.workspaces[0].tabs[1].root_pane, internal_id);
    }

    #[test]
    fn open_rejects_oversized_content() {
        let mut app = test_app();
        let mut params = open_params(&"x".repeat(PREVIEW_CONTENT_MAX_BYTES + 1));
        params.placement = Some(PreviewPanePlacement::Split);

        let response = app.handle_api_request(request(Method::PreviewOpen(params)));
        assert_eq!(error_code(&response), "content_too_large");
        assert_eq!(app.state.workspaces[0].tabs[0].layout.pane_count(), 1);
    }

    #[test]
    fn open_validates_title_and_content_sources() {
        let mut app = test_app();

        let mut params = open_params("body");
        params.title = "   ".into();
        let response = app.handle_api_request(request(Method::PreviewOpen(params)));
        assert_eq!(error_code(&response), "invalid_params");

        let mut params = open_params("body");
        params.content = None;
        params.content_file = None;
        let response = app.handle_api_request(request(Method::PreviewOpen(params)));
        assert_eq!(error_code(&response), "invalid_params");

        let mut params = open_params("body");
        params.content_file = Some("/tmp/also".into());
        let response = app.handle_api_request(request(Method::PreviewOpen(params)));
        assert_eq!(error_code(&response), "invalid_params");
    }

    #[test]
    fn open_rejects_placement_arg_mismatch() {
        let mut app = test_app();
        let mut params = open_params("body");
        params.placement = Some(PreviewPanePlacement::Split);
        params.workspace_id = Some("w_1".into());
        let response = app.handle_api_request(request(Method::PreviewOpen(params)));
        assert_eq!(error_code(&response), "invalid_params");

        let mut params = open_params("body");
        params.placement = Some(PreviewPanePlacement::Tab);
        params.target_pane_id = Some(root_pane_public_id(&app));
        let response = app.handle_api_request(request(Method::PreviewOpen(params)));
        assert_eq!(error_code(&response), "invalid_params");
    }

    #[test]
    fn open_reads_content_file() {
        let mut app = test_app();
        let path = std::env::temp_dir().join(format!(
            "herdr-preview-test-{}-content.txt",
            std::process::id()
        ));
        std::fs::write(&path, "from file").unwrap();
        let mut params = open_params("");
        params.content = None;
        params.content_file = Some(path.display().to_string());
        params.placement = Some(PreviewPanePlacement::Split);

        let pane_id = open_pane_id(&mut app, params);
        std::fs::remove_file(&path).ok();

        let terminal_id = preview_terminal_id(&app, &pane_id);
        let preview = app.state.terminals[&terminal_id].preview.as_ref().unwrap();
        assert_eq!(preview.content, "from file");
    }

    #[test]
    fn set_updates_content_title_and_revision() {
        let mut app = test_app();
        let pane_id = open_pane_id(&mut app, open_params("before"));
        let terminal_id = preview_terminal_id(&app, &pane_id);
        let revision_before = app.state.terminals[&terminal_id].revision;

        let response = app.handle_api_request(request(Method::PreviewSet(PreviewSetParams {
            pane_id: pane_id.clone(),
            content: Some("# after".into()),
            content_file: None,
            content_type: Some(PreviewContentType::Markdown),
            title: Some("Renamed".into()),
        })));
        let response: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::PreviewUpdated { pane } = response.result else {
            panic!("expected preview_updated");
        };
        let preview = pane.preview.expect("pane should stay a preview");
        assert_eq!(preview.content_type, PreviewContentType::Markdown);
        assert_eq!(preview.content_bytes, "# after".len() as u64);
        assert_eq!(pane.label.as_deref(), Some("Renamed"));

        let terminal = &app.state.terminals[&terminal_id];
        assert!(terminal.revision > revision_before);
        let preview = terminal.preview.as_ref().unwrap();
        assert_eq!(preview.content, "# after");
        assert_eq!(preview.lines[0].heading, Some(1));
    }

    #[test]
    fn set_rejects_non_preview_pane() {
        let mut app = test_app();
        let response = app.handle_api_request(request(Method::PreviewSet(PreviewSetParams {
            pane_id: root_pane_public_id(&app),
            content: Some("nope".into()),
            content_file: None,
            content_type: None,
            title: None,
        })));
        assert_eq!(error_code(&response), "not_preview_pane");
    }

    #[test]
    fn pane_read_returns_preview_content_with_tail_limit() {
        let mut app = test_app();
        let pane_id = open_pane_id(&mut app, open_params("one\ntwo\nthree"));

        let response = app.handle_api_request(request(Method::PaneRead(PaneReadParams {
            pane_id: pane_id.clone(),
            source: ReadSource::Recent,
            lines: Some(1),
            format: ReadFormat::Text,
            strip_ansi: true,
            intent: crate::api::schema::common::ReadIntent::Passive,
        })));
        let response: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::PaneRead { read } = response.result else {
            panic!("expected pane_read");
        };
        assert_eq!(read.text, "three");
        assert!(read.truncated);
    }

    #[test]
    fn pane_scroll_moves_preview_offset() {
        let mut app = test_app();
        let pane_id = open_pane_id(&mut app, open_params("a\nb\nc\nd\ne"));

        let response = app.handle_api_request(request(Method::PaneScroll(PaneScrollParams {
            pane_id: pane_id.clone(),
            offset_from_bottom: 2,
        })));
        let response: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::PaneInfo { pane } = response.result else {
            panic!("expected pane info");
        };
        assert!(pane.preview.is_some());
        let scroll = pane.scroll.expect("preview panes report scroll metrics");
        assert_eq!(scroll.offset_from_bottom, 2);
    }

    #[test]
    fn terminal_only_apis_reject_preview_pane() {
        let mut app = test_app();
        let pane_id = open_pane_id(&mut app, open_params("body"));

        for method in [
            Method::PaneSendText(PaneSendTextParams {
                pane_id: pane_id.clone(),
                text: "hi".into(),
            }),
            Method::PaneSendKeys(PaneSendKeysParams {
                pane_id: pane_id.clone(),
                keys: vec!["enter".into()],
            }),
            Method::PaneClear(PaneTarget {
                pane_id: pane_id.clone(),
            }),
            Method::PaneEditScrollback(PaneTarget {
                pane_id: pane_id.clone(),
            }),
            Method::PaneProcessInfo(crate::api::schema::PaneProcessInfoParams {
                pane_id: Some(pane_id.clone()),
            }),
        ] {
            let response = app.handle_api_request(request(method));
            assert_eq!(
                error_code(&response),
                "preview_pane",
                "terminal-only operation should reject preview panes"
            );
        }
    }

    #[test]
    fn close_overlay_restores_focus_and_zoom() {
        let mut app = test_app();
        let root_pane = app.state.workspaces[0].tabs[0].root_pane;
        let root_public = app.public_pane_id(0, root_pane).unwrap();
        let pane_id = open_pane_id(&mut app, open_params("overlay"));
        assert!(app.state.workspaces[0].tabs[0].zoomed);

        let response = app.handle_api_request(request(Method::PreviewClose(PreviewCloseParams {
            pane_id: pane_id.clone(),
        })));
        let response: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::PreviewClosed { pane_id: closed } = response.result else {
            panic!("expected preview_closed");
        };
        assert_eq!(closed, pane_id);

        assert_eq!(app.state.workspaces[0].tabs[0].layout.pane_count(), 1);
        assert!(!app.state.workspaces[0].tabs[0].zoomed);
        assert_eq!(
            app.state.workspaces[0].focused_pane_id(),
            Some(root_pane),
            "closing an overlay preview must restore the previous focus"
        );
        assert!(app.overlay_panes.is_empty());
        let terminal_id = preview_terminal_id(&app, &root_public);
        assert!(app.state.terminals.contains_key(&terminal_id));
    }

    #[test]
    fn pane_close_on_preview_also_restores_overlay() {
        let mut app = test_app();
        let root_pane = app.state.workspaces[0].tabs[0].root_pane;
        let pane_id = open_pane_id(&mut app, open_params("overlay"));

        let response = app.handle_api_request(request(Method::PaneClose(PaneTarget { pane_id })));
        let response: SuccessResponse = serde_json::from_str(&response).unwrap();
        assert!(matches!(response.result, ResponseResult::Ok {}));
        assert_eq!(app.state.workspaces[0].focused_pane_id(), Some(root_pane));
        assert!(app.overlay_panes.is_empty());
    }

    #[test]
    fn close_rejects_non_preview_pane() {
        let mut app = test_app();
        let response = app.handle_api_request(request(Method::PreviewClose(PreviewCloseParams {
            pane_id: root_pane_public_id(&app),
        })));
        assert_eq!(error_code(&response), "not_preview_pane");
    }

    #[test]
    fn split_preview_leaves_original_pane_focused_without_focus_flag() {
        let mut app = test_app();
        let root_pane = app.state.workspaces[0].tabs[0].root_pane;
        let mut params = open_params("unfocused");
        params.placement = Some(PreviewPanePlacement::Split);
        params.focus = false;

        open_pane_id(&mut app, params);
        assert_eq!(app.state.workspaces[0].focused_pane_id(), Some(root_pane));
    }
}
