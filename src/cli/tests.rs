//! Unit tests for the CLI adapter types.

use camino::Utf8PathBuf;
use rstest::rstest;

use super::{AgentKindArg, AgentModeArg, Cli, Commands};
use crate::config::{AgentKind, AgentMode, ConfigOverrides};

#[rstest]
fn cli_config_load_options_preserves_global_flags() {
    let cli = Cli {
        command: Commands::Ps,
        config: Some(Utf8PathBuf::from("/tmp/podbot-config.toml")),
        engine_socket: Some(String::from("unix:///example.sock")),
        image: Some(String::from("example-image:latest")),
    };

    let options = cli.config_load_options();
    assert_eq!(
        options.config_path_hint,
        Some(Utf8PathBuf::from("/tmp/podbot-config.toml"))
    );
    assert!(options.discover_config);
    assert_eq!(
        options.overrides,
        ConfigOverrides {
            engine_socket: Some(String::from("unix:///example.sock")),
            image: Some(String::from("example-image:latest")),
        }
    );
}

#[rstest]
#[case(AgentKindArg::Claude, AgentKind::Claude)]
#[case(AgentKindArg::Codex, AgentKind::Codex)]
fn agent_kind_arg_converts_to_library_kind(#[case] arg: AgentKindArg, #[case] expected: AgentKind) {
    let converted: AgentKind = arg.into();
    assert_eq!(converted, expected);
}

#[rstest]
#[case(AgentModeArg::Podbot, AgentMode::Podbot)]
fn agent_mode_arg_converts_to_library_mode(#[case] arg: AgentModeArg, #[case] expected: AgentMode) {
    let converted: AgentMode = arg.into();
    assert_eq!(converted, expected);
}
