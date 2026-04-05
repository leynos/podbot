//! Layer precedence tests for hosting-era config fields.

use ortho_config::serde_json::json;
use rstest::rstest;

use crate::config::tests::helpers::{create_composer_with_defaults, merge_config};
use crate::config::{AgentKind, AgentMode, AppConfig, WorkspaceSource};

#[rstest]
fn hosting_fields_merge_across_layers() {
    let mut composer = create_composer_with_defaults().expect("composer creation should succeed");

    composer.push_file(
        json!({
            "workspace": {
                "source": "host_mount",
                "host_path": "/from/file/project"
            },
            "agent": {
                "kind": "custom",
                "command": "opencode"
            }
        }),
        None,
    );
    composer.push_environment(json!({
        "workspace": {
            "container_path": "/from/env/workspace"
        },
        "agent": {
            "mode": "acp",
            "args": ["serve"]
        }
    }));

    let mut config: AppConfig = merge_config(composer).expect("merge should succeed");
    config
        .normalize_and_validate(crate::config::CommandIntent::Host)
        .expect("hosting config should validate");

    assert_eq!(config.workspace.source, WorkspaceSource::HostMount);
    assert_eq!(
        config.workspace.host_path,
        Some("/from/file/project".into())
    );
    assert_eq!(
        config.workspace.container_path,
        Some("/from/env/workspace".into())
    );
    assert_eq!(config.agent.kind, AgentKind::Custom);
    assert_eq!(config.agent.mode, AgentMode::Acp);
    assert_eq!(config.agent.command.as_deref(), Some("opencode"));
    assert_eq!(config.agent.args, vec![String::from("serve")]);
}
