//! Session-scoped sidebar rows registered through `sidebar.provision`.
//!
//! Provisioned rows live only in `AppState` memory: they are never written to
//! the client config file and are dropped on restart. Rows reuse the
//! `[ui.sidebar.*]` token types so rendering, styling, and conditional rules
//! behave identically to configured rows.

use crate::api::schema::{SidebarPanel, SidebarProvisionInfo};
use crate::config::{AgentSidebarToken, SpaceSidebarToken};

use super::AppState;

/// Bound on concurrent provisioning sources so the merged row set stays small.
pub(crate) const MAX_SIDEBAR_PROVISION_SOURCES: usize = 16;
const MAX_SIDEBAR_PROVISION_SOURCE_CHARS: usize = 120;
const PLUGIN_SOURCE_PREFIX: &str = "plugin:";

/// Sidebar rows owned by one provision source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SidebarProvisionedRows {
    pub(crate) agents: Vec<Vec<AgentSidebarToken>>,
    pub(crate) spaces: Vec<Vec<SpaceSidebarToken>>,
}

impl SidebarProvisionedRows {
    pub(crate) fn is_empty(&self) -> bool {
        self.agents.is_empty() && self.spaces.is_empty()
    }
}

/// Provisioned rows from every source merged in deterministic source order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MergedSidebarProvisions {
    pub(crate) agents: Vec<Vec<AgentSidebarToken>>,
    pub(crate) spaces: Vec<Vec<SpaceSidebarToken>>,
}

/// Rows decoded and validated for one panel.
pub(crate) enum DecodedSidebarRows {
    Agents(Vec<Vec<AgentSidebarToken>>),
    Spaces(Vec<Vec<SpaceSidebarToken>>),
}

/// Decode provisioned rows through the same token grammar and row limits the
/// config file enforces, reporting the 1-based row index on failure.
pub(crate) fn decode_sidebar_provision_rows(
    panel: SidebarPanel,
    rows: Vec<Vec<serde_json::Value>>,
) -> Result<DecodedSidebarRows, String> {
    crate::config::validate_sidebar_rows(&rows)?;
    match panel {
        SidebarPanel::Agents => {
            decode_rows::<AgentSidebarToken>(rows).map(DecodedSidebarRows::Agents)
        }
        SidebarPanel::Spaces => {
            decode_rows::<SpaceSidebarToken>(rows).map(DecodedSidebarRows::Spaces)
        }
    }
}

fn decode_rows<T>(rows: Vec<Vec<serde_json::Value>>) -> Result<Vec<Vec<T>>, String>
where
    T: serde::de::DeserializeOwned,
{
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            if row.len() > crate::config::MAX_SIDEBAR_TOKENS_PER_ROW {
                return Err(format!(
                    "sidebar row {} exceeds the {} token limit",
                    index + 1,
                    crate::config::MAX_SIDEBAR_TOKENS_PER_ROW
                ));
            }
            serde_json::from_value(serde_json::Value::Array(row))
                .map_err(|error| format!("sidebar row {}: {error}", index + 1))
        })
        .collect()
}

/// Source identity shared by `sidebar.provision` and `sidebar.provision.clear`.
/// `plugin:` owners must exist and be enabled, mirroring agent view ownership.
pub(crate) fn normalize_sidebar_provision_source(source: &str) -> Result<String, String> {
    let source = source.trim();
    if source.is_empty()
        || source.chars().count() > MAX_SIDEBAR_PROVISION_SOURCE_CHARS
        || !source
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '.' | '_' | '-'))
    {
        return Err(format!(
            "sidebar provision source must be non-empty, at most {MAX_SIDEBAR_PROVISION_SOURCE_CHARS} characters, and contain only ASCII letters, digits, colon, dot, underscore, or hyphen"
        ));
    }
    Ok(source.to_string())
}

pub(crate) fn sidebar_provision_plugin_id(source: &str) -> Option<&str> {
    source.strip_prefix(PLUGIN_SOURCE_PREFIX)
}

fn encode_rows<T: serde::Serialize>(rows: &[Vec<T>]) -> Vec<Vec<serde_json::Value>> {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|token| serde_json::to_value(token).unwrap_or(serde_json::Value::Null))
                .collect()
        })
        .collect()
}

impl AppState {
    /// Rows appended after the configured agent rows, in source order.
    pub(crate) fn provisioned_agent_rows(&self) -> impl Iterator<Item = &Vec<AgentSidebarToken>> {
        self.sidebar_provisions
            .values()
            .flat_map(|rows| rows.agents.iter())
    }

    /// Rows appended after the configured space rows, in source order.
    pub(crate) fn provisioned_space_rows(&self) -> impl Iterator<Item = &Vec<SpaceSidebarToken>> {
        self.sidebar_provisions
            .values()
            .flat_map(|rows| rows.spaces.iter())
    }

    /// All provisioned rows merged for the endpoint projection, or `None` when
    /// no source has rows.
    pub(crate) fn merged_sidebar_provisions(&self) -> Option<MergedSidebarProvisions> {
        let agents: Vec<_> = self.provisioned_agent_rows().cloned().collect();
        let spaces: Vec<_> = self.provisioned_space_rows().cloned().collect();
        (!agents.is_empty() || !spaces.is_empty())
            .then_some(MergedSidebarProvisions { agents, spaces })
    }

    /// Wire representation of the rows still owned by one source.
    pub(crate) fn sidebar_provision_infos_for(&self, source: &str) -> Vec<SidebarProvisionInfo> {
        let Some(rows) = self.sidebar_provisions.get(source) else {
            return Vec::new();
        };
        let mut infos = Vec::new();
        if !rows.agents.is_empty() {
            infos.push(SidebarProvisionInfo {
                source: source.to_string(),
                panel: SidebarPanel::Agents,
                rows: encode_rows(&rows.agents),
            });
        }
        if !rows.spaces.is_empty() {
            infos.push(SidebarProvisionInfo {
                source: source.to_string(),
                panel: SidebarPanel::Spaces,
                rows: encode_rows(&rows.spaces),
            });
        }
        infos
    }

    /// Wire representation of every registered provision for `session.snapshot`.
    pub(crate) fn sidebar_provision_infos(&self) -> Vec<SidebarProvisionInfo> {
        self.sidebar_provisions
            .keys()
            .flat_map(|source| self.sidebar_provision_infos_for(source))
            .collect()
    }
}
