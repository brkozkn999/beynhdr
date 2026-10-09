use crate::api::schema::{
    ResponseResult, SidebarPanel, SidebarProvisionClearParams, SidebarProvisionParams,
};
use crate::app::sidebar_provision::{
    decode_sidebar_provision_rows, normalize_sidebar_provision_source, sidebar_provision_plugin_id,
    DecodedSidebarRows, SidebarProvisionedRows, MAX_SIDEBAR_PROVISION_SOURCES,
};
use crate::app::App;

use super::responses::{encode_error, encode_success};

impl App {
    pub(super) fn handle_sidebar_provision(
        &mut self,
        id: String,
        params: SidebarProvisionParams,
    ) -> String {
        let source = match normalize_sidebar_provision_source(&params.source) {
            Ok(source) => source,
            Err(message) => return encode_error(id, "invalid_sidebar_provision", message),
        };
        if let Some(plugin_id) = sidebar_provision_plugin_id(&source) {
            let Some(plugin_id) = super::plugins::normalize_plugin_id(plugin_id) else {
                return encode_error(
                    id,
                    "invalid_sidebar_provision",
                    "plugin-owned sidebar provision source has an invalid plugin id",
                );
            };
            let Some(plugin) = self.state.installed_plugins.get(&plugin_id) else {
                return encode_error(id, "plugin_not_found", "plugin not found");
            };
            if !plugin.enabled {
                return encode_error(id, "plugin_disabled", "plugin is disabled");
            }
        }
        let decoded = match decode_sidebar_provision_rows(params.panel, params.rows) {
            Ok(decoded) => decoded,
            Err(message) => return encode_error(id, "invalid_sidebar_provision", message),
        };
        let empty = match &decoded {
            DecodedSidebarRows::Agents(rows) => rows.is_empty(),
            DecodedSidebarRows::Spaces(rows) => rows.is_empty(),
        };
        if !empty
            && !self.state.sidebar_provisions.contains_key(&source)
            && self.state.sidebar_provisions.len() >= MAX_SIDEBAR_PROVISION_SOURCES
        {
            return encode_error(
                id,
                "sidebar_provision_source_limit",
                format!(
                    "at most {MAX_SIDEBAR_PROVISION_SOURCES} sidebar provision sources may be registered"
                ),
            );
        }
        match decoded {
            DecodedSidebarRows::Agents(rows) => {
                self.state
                    .sidebar_provisions
                    .entry(source.clone())
                    .or_default()
                    .agents = rows;
            }
            DecodedSidebarRows::Spaces(rows) => {
                self.state
                    .sidebar_provisions
                    .entry(source.clone())
                    .or_default()
                    .spaces = rows;
            }
        }
        // Never retain a fully cleared source entry.
        if self
            .state
            .sidebar_provisions
            .get(&source)
            .is_some_and(SidebarProvisionedRows::is_empty)
        {
            self.state.sidebar_provisions.remove(&source);
        }
        let provisions = self.state.sidebar_provision_infos_for(&source);
        encode_success(id, ResponseResult::SidebarProvisions { source, provisions })
    }

    pub(super) fn handle_sidebar_provision_clear(
        &mut self,
        id: String,
        params: SidebarProvisionClearParams,
    ) -> String {
        let source = match normalize_sidebar_provision_source(&params.source) {
            Ok(source) => source,
            Err(message) => return encode_error(id, "invalid_sidebar_provision", message),
        };
        if let Some(rows) = self.state.sidebar_provisions.get_mut(&source) {
            match params.panel {
                Some(SidebarPanel::Agents) => rows.agents.clear(),
                Some(SidebarPanel::Spaces) => rows.spaces.clear(),
                None => {
                    rows.agents.clear();
                    rows.spaces.clear();
                }
            }
            if rows.is_empty() {
                self.state.sidebar_provisions.remove(&source);
            }
        }
        let provisions = self.state.sidebar_provision_infos_for(&source);
        encode_success(id, ResponseResult::SidebarProvisions { source, provisions })
    }

    /// Drop every provisioned row owned by `source` (plugin unlink/disable).
    pub(crate) fn clear_sidebar_provisions_for_source(&mut self, source: &str) -> bool {
        self.state.sidebar_provisions.remove(source).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::schema::{ErrorResponse, Method, SuccessResponse};

    fn test_app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            api_rx,
            crate::api::EventHub::default(),
        )
    }

    fn provision(
        source: &str,
        panel: SidebarPanel,
        rows: serde_json::Value,
    ) -> SidebarProvisionParams {
        SidebarProvisionParams {
            source: source.to_string(),
            panel,
            rows: serde_json::from_value(rows).unwrap(),
        }
    }

    fn success(response: String) -> SuccessResponse {
        serde_json::from_str(&response).unwrap()
    }

    fn error(response: String) -> ErrorResponse {
        serde_json::from_str(&response).unwrap()
    }

    #[test]
    fn provision_registers_and_replaces_rows_per_source() {
        let mut app = test_app();
        let response = success(app.handle_sidebar_provision(
            "a".into(),
            provision(
                "example.status",
                SidebarPanel::Agents,
                serde_json::json!([["state_icon", "agent"], [{ "token": "$load", "dim": true }]]),
            ),
        ));
        let ResponseResult::SidebarProvisions { source, provisions } = response.result else {
            panic!("expected sidebar provisions result");
        };
        assert_eq!(source, "example.status");
        assert_eq!(provisions.len(), 1);
        assert_eq!(provisions[0].rows.len(), 2);
        assert_eq!(
            app.state
                .sidebar_provisions
                .get("example.status")
                .map(|rows| rows.agents.len()),
            Some(2)
        );

        // A second call for the same panel replaces rows instead of appending.
        let response = success(app.handle_sidebar_provision(
            "b".into(),
            provision(
                "example.status",
                SidebarPanel::Agents,
                serde_json::json!([["state_text"]]),
            ),
        ));
        let ResponseResult::SidebarProvisions { provisions, .. } = response.result else {
            panic!("expected sidebar provisions result");
        };
        assert_eq!(provisions.len(), 1);
        assert_eq!(provisions[0].rows.len(), 1);
        assert_eq!(
            app.state
                .sidebar_provisions
                .get("example.status")
                .map(|rows| rows.agents.len()),
            Some(1)
        );
    }

    #[test]
    fn clear_scoped_by_source_and_panel() {
        let mut app = test_app();
        app.handle_sidebar_provision(
            "a".into(),
            provision("one", SidebarPanel::Agents, serde_json::json!([["agent"]])),
        );
        app.handle_sidebar_provision(
            "b".into(),
            provision(
                "one",
                SidebarPanel::Spaces,
                serde_json::json!([["workspace"]]),
            ),
        );
        app.handle_sidebar_provision(
            "c".into(),
            provision("two", SidebarPanel::Agents, serde_json::json!([["agent"]])),
        );

        app.handle_sidebar_provision_clear(
            "d".into(),
            SidebarProvisionClearParams {
                source: "one".into(),
                panel: Some(SidebarPanel::Agents),
            },
        );
        let rows = app.state.sidebar_provisions.get("one").unwrap();
        assert!(rows.agents.is_empty());
        assert_eq!(rows.spaces.len(), 1);
        assert!(app.state.sidebar_provisions.contains_key("two"));

        app.handle_sidebar_provision_clear(
            "e".into(),
            SidebarProvisionClearParams {
                source: "one".into(),
                panel: None,
            },
        );
        assert!(!app.state.sidebar_provisions.contains_key("one"));
    }

    #[test]
    fn empty_rows_clear_without_leaving_an_entry() {
        let mut app = test_app();
        app.handle_sidebar_provision(
            "a".into(),
            provision("one", SidebarPanel::Agents, serde_json::json!([["agent"]])),
        );
        let response = success(app.handle_sidebar_provision(
            "b".into(),
            provision("one", SidebarPanel::Agents, serde_json::json!([])),
        ));
        let ResponseResult::SidebarProvisions { provisions, .. } = response.result else {
            panic!("expected sidebar provisions result");
        };
        assert!(provisions.is_empty());
        assert!(!app.state.sidebar_provisions.contains_key("one"));
    }

    #[test]
    fn plugin_owned_sources_validate_installation_and_enabled_state() {
        let mut app = test_app();
        let missing = error(app.handle_sidebar_provision(
            "a".into(),
            provision(
                "plugin:example.missing",
                SidebarPanel::Agents,
                serde_json::json!([["agent"]]),
            ),
        ));
        assert_eq!(missing.error.code, "plugin_not_found");

        app.state.installed_plugins.insert(
            "example.missing".into(),
            crate::api::schema::InstalledPluginInfo {
                plugin_id: "example.missing".into(),
                name: "missing".into(),
                version: "0.1.0".into(),
                min_herdr_version: "0.7.0".into(),
                description: None,
                manifest_path: "/tmp/herdr-plugin.toml".into(),
                plugin_root: "/tmp".into(),
                enabled: false,
                platforms: None,
                build: Vec::new(),
                startup: Vec::new(),
                actions: Vec::new(),
                events: Vec::new(),
                panes: Vec::new(),
                link_handlers: Vec::new(),
                source: crate::api::schema::PluginSourceInfo::default(),
                warnings: Vec::new(),
            },
        );
        let disabled = error(app.handle_sidebar_provision(
            "b".into(),
            provision(
                "plugin:example.missing",
                SidebarPanel::Agents,
                serde_json::json!([["agent"]]),
            ),
        ));
        assert_eq!(disabled.error.code, "plugin_disabled");

        let invalid = error(app.handle_sidebar_provision(
            "c".into(),
            provision(
                "plugin:",
                SidebarPanel::Agents,
                serde_json::json!([["agent"]]),
            ),
        ));
        assert_eq!(invalid.error.code, "invalid_sidebar_provision");

        let bad_source = error(app.handle_sidebar_provision(
            "d".into(),
            provision("not a source", SidebarPanel::Agents, serde_json::json!([])),
        ));
        assert_eq!(bad_source.error.code, "invalid_sidebar_provision");
    }

    #[test]
    fn malformed_tokens_and_limits_are_rejected() {
        let mut app = test_app();
        for rows in [
            serde_json::json!([["bogus_token"]]),
            serde_json::json!([["$bad name"]]),
            serde_json::json!([[{ "token": "state_icon", "rules": [{ "equals": "x" }] }]]),
            serde_json::json!([[{ "token": "agent", "fg": "red" }]]),
            serde_json::json!([[42]]),
        ] {
            let response = error(app.handle_sidebar_provision(
                "x".into(),
                provision("one", SidebarPanel::Agents, rows),
            ));
            assert_eq!(response.error.code, "invalid_sidebar_provision");
        }
        // A token valid for agents but unknown to the space grammar fails on spaces.
        let response = error(app.handle_sidebar_provision(
            "x".into(),
            provision("one", SidebarPanel::Spaces, serde_json::json!([["agent"]])),
        ));
        assert_eq!(response.error.code, "invalid_sidebar_provision");

        let too_many_rows = serde_json::json!(vec![vec!["agent"; 1]; 17]);
        let response = error(app.handle_sidebar_provision(
            "x".into(),
            provision("one", SidebarPanel::Agents, too_many_rows),
        ));
        assert_eq!(response.error.code, "invalid_sidebar_provision");

        let too_many_tokens = serde_json::json!([vec!["agent"; 17]]);
        let response = error(app.handle_sidebar_provision(
            "x".into(),
            provision("one", SidebarPanel::Agents, too_many_tokens),
        ));
        assert_eq!(response.error.code, "invalid_sidebar_provision");

        assert!(app.state.sidebar_provisions.is_empty());
    }

    #[test]
    fn provisioned_rows_feed_the_session_snapshot_and_merge_helpers() {
        let mut app = test_app();
        app.handle_sidebar_provision(
            "a".into(),
            provision(
                "b.source",
                SidebarPanel::Agents,
                serde_json::json!([["state_text"]]),
            ),
        );
        app.handle_sidebar_provision(
            "b".into(),
            provision(
                "a.source",
                SidebarPanel::Spaces,
                serde_json::json!([["workspace"]]),
            ),
        );
        let merged = app.state.merged_sidebar_provisions().unwrap();
        assert_eq!(merged.agents.len(), 1);
        assert_eq!(merged.spaces.len(), 1);
        let infos = app.state.sidebar_provision_infos();
        assert_eq!(infos.len(), 2);
        assert_eq!(infos[0].source, "a.source");
        assert_eq!(infos[0].panel, SidebarPanel::Spaces);
        assert_eq!(infos[1].source, "b.source");

        let response = app.handle_api_request(crate::api::schema::Request {
            id: "snap".into(),
            method: Method::SessionSnapshot(crate::api::schema::EmptyParams::default()),
        });
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::SessionSnapshot { snapshot } = success.result else {
            panic!("expected session snapshot");
        };
        assert_eq!(snapshot.sidebar_provisions.len(), 2);
    }

    fn enabled_plugin(plugin_id: &str) -> crate::api::schema::InstalledPluginInfo {
        crate::api::schema::InstalledPluginInfo {
            plugin_id: plugin_id.into(),
            name: "plugin".into(),
            version: "0.1.0".into(),
            min_herdr_version: "0.7.0".into(),
            description: None,
            manifest_path: "/tmp/herdr-plugin.toml".into(),
            plugin_root: "/tmp".into(),
            enabled: true,
            platforms: None,
            build: Vec::new(),
            startup: Vec::new(),
            actions: Vec::new(),
            events: Vec::new(),
            panes: Vec::new(),
            link_handlers: Vec::new(),
            source: crate::api::schema::PluginSourceInfo::default(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn plugin_disable_and_unlink_clear_provisioned_rows() {
        let mut app = test_app();
        app.state
            .installed_plugins
            .insert("example.owner".into(), enabled_plugin("example.owner"));
        let params = || {
            provision(
                "plugin:example.owner",
                SidebarPanel::Agents,
                serde_json::json!([["agent"]]),
            )
        };

        app.handle_sidebar_provision("a".into(), params());
        assert!(app
            .state
            .sidebar_provisions
            .contains_key("plugin:example.owner"));

        app.handle_plugin_disable(
            "b".into(),
            crate::api::schema::PluginSetEnabledParams {
                plugin_id: "example.owner".into(),
            },
        );
        assert!(!app
            .state
            .sidebar_provisions
            .contains_key("plugin:example.owner"));

        app.handle_plugin_enable(
            "c".into(),
            crate::api::schema::PluginSetEnabledParams {
                plugin_id: "example.owner".into(),
            },
        );
        app.handle_sidebar_provision("d".into(), params());
        assert!(app
            .state
            .sidebar_provisions
            .contains_key("plugin:example.owner"));

        app.handle_plugin_unlink(
            "e".into(),
            crate::api::schema::PluginUnlinkParams {
                plugin_id: "example.owner".into(),
            },
        );
        assert!(!app
            .state
            .sidebar_provisions
            .contains_key("plugin:example.owner"));
    }
}
