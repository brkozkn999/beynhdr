use serde::{Deserialize, Serialize};

/// Sidebar panel that receives provisioned rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SidebarPanel {
    /// Rows appended to the agent panel after the configured agent rows.
    Agents,
    /// Rows appended to the workspace panel after the configured space rows.
    Spaces,
}

/// Schema shared by provisioned row sets. Rows mirror the `[ui.sidebar.*]`
/// token format without depending on the private config types.
fn sidebar_rows_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "array",
        "description": "Sidebar rows in the `[ui.sidebar.*]` config token format: each row is an array of tokens; a token is a builtin name (\"state_icon\", \"workspace\", ...), a custom \"$name\", or a styled object `{ token, fg, bold, dim, rules }`.",
        "maxItems": 16,
        "items": {
            "type": "array",
            "maxItems": 16,
            "items": true
        }
    })
}

/// Registers or replaces the sidebar rows owned by one source for one panel.
/// The rows live only in server memory and are cleared on restart, on
/// `sidebar.provision.clear`, and when an owning `plugin:` source is unlinked
/// or disabled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SidebarProvisionParams {
    /// Owner identity such as `plugin:example.status`. Sources prefixed with
    /// `plugin:` require the plugin to be installed and enabled.
    pub source: String,
    pub panel: SidebarPanel,
    /// Rows appended after the configured rows for the panel. An empty array
    /// clears this source's rows for the panel.
    #[schemars(schema_with = "sidebar_rows_schema")]
    pub rows: Vec<Vec<serde_json::Value>>,
}

/// Removes the sidebar rows owned by one source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SidebarProvisionClearParams {
    pub source: String,
    /// When omitted, clears the source's rows from both panels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<SidebarPanel>,
}

/// One source's registered row set for one panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SidebarProvisionInfo {
    pub source: String,
    pub panel: SidebarPanel,
    #[schemars(schema_with = "sidebar_rows_schema")]
    pub rows: Vec<Vec<serde_json::Value>>,
}
