use serde::{Deserialize, Serialize};

/// Maximum payload accepted by `preview.open`/`preview.set` for `content` or
/// `content_file`, in bytes of UTF-8 text.
pub const PREVIEW_CONTENT_MAX_BYTES: usize = 256 * 1024;

/// Payload interpretation for a preview pane.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum PreviewContentType {
    #[default]
    Text,
    Markdown,
}

/// How a preview pane joins the layout. Preview panes are runtime-less
/// read-only surfaces, so the PTY-only `popup` and `zoomed` plugin placements
/// are intentionally absent.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum PreviewPanePlacement {
    /// Zoomed overlay split off the active pane; closing restores the previous
    /// focus and zoom state.
    #[default]
    Overlay,
    /// Split an existing pane (defaults to the focused pane, to the right).
    Split,
    /// New tab holding only the preview pane.
    Tab,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PreviewOpenParams {
    /// Border label for the pane. Must be non-empty after trimming.
    pub title: String,
    pub content_type: PreviewContentType,
    /// Inline UTF-8 content; exactly one of `content`/`content_file` is required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Server-side file path read once at open; same size limit as `content`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<PreviewPanePlacement>,
    /// Only valid with `placement: "tab"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    /// Only valid with `placement: "split"` (defaults to the focused pane).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_pane_id: Option<String>,
    /// Only valid with `placement: "split"` (defaults to `right`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<super::common::SplitDirection>,
    /// Focus the new pane. Always on for `overlay`; optional for `split`/`tab`.
    #[serde(default)]
    pub focus: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PreviewSetParams {
    pub pane_id: String,
    /// Replacement content; mutually exclusive with `content_file`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_file: Option<String>,
    /// Change the payload interpretation; defaults to the current type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<PreviewContentType>,
    /// Change the pane label; `null`/absent keeps the current label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PreviewCloseParams {
    pub pane_id: String,
}

/// Marks a `PaneInfo` as a read-only preview surface instead of a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PreviewPaneInfo {
    pub content_type: PreviewContentType,
    pub content_bytes: u64,
}
