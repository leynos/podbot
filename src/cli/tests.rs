//! Unit tests for the CLI adapter types.

use camino::Utf8PathBuf;
use clap::Parser;
use rstest::rstest;

use super::{AgentKindArg, AgentModeArg, Cli, Commands, HostArgs};
use crate::config::{AgentKind, AgentMode, CommandIntent, ConfigOverrides};

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
            agent_kind: None,
            agent_mode: None,
        }
    );
    assert_eq!(options.command_intent, CommandIntent::Any);
}

#[rstest]
#[case(AgentKindArg::Claude, AgentKind::Claude)]
#[case(AgentKindArg::Codex, AgentKind::Codex)]
#[case(AgentKindArg::Custom, AgentKind::Custom)]
fn agent_kind_arg_converts_to_library_kind(#[case] arg: AgentKindArg, #[case] expected: AgentKind) {
    let converted: AgentKind = arg.into();
    assert_eq!(converted, expected);
}

#[rstest]
#[case(AgentModeArg::Podbot, AgentMode::Podbot)]
#[case(AgentModeArg::CodexAppServer, AgentMode::CodexAppServer)]
#[case(AgentModeArg::Acp, AgentMode::Acp)]
fn agent_mode_arg_converts_to_library_mode(#[case] arg: AgentModeArg, #[case] expected: AgentMode) {
    let converted: AgentMode = arg.into();
    assert_eq!(converted, expected);
}

#[rstest]
#[case(
    Commands::Host(HostArgs {
        agent: Some(AgentKindArg::Custom),
        mode: Some(AgentModeArg::Acp),
    }),
    CommandIntent::Host,
    AgentKind::Custom,
    AgentMode::Acp,
)]
#[case(
    Commands::Run(crate::cli::RunArgs {
        repo: String::from("owner/name"),
        branch: String::from("main"),
        agent: Some(AgentKindArg::Codex),
        mode: Some(AgentModeArg::Podbot),
    }),
    CommandIntent::Run,
    AgentKind::Codex,
    AgentMode::Podbot,
)]
fn config_load_options_sets_intent_and_agent_overrides(
    #[case] command: Commands,
    #[case] expected_intent: CommandIntent,
    #[case] expected_kind: AgentKind,
    #[case] expected_mode: AgentMode,
) {
    let cli = Cli {
        command,
        config: None,
        engine_socket: None,
        image: None,
    };

    let options = cli.config_load_options();

    assert_eq!(options.command_intent, expected_intent);
    assert_eq!(options.overrides.agent_kind, Some(expected_kind));
    assert_eq!(options.overrides.agent_mode, Some(expected_mode));
}

#[rstest]
#[case(
    Commands::Host(HostArgs {
        agent: None,
        mode: None,
    }),
    CommandIntent::Host,
)]
#[case(
    Commands::Run(crate::cli::RunArgs {
        repo: String::from("owner/name"),
        branch: String::from("main"),
        agent: None,
        mode: None,
    }),
    CommandIntent::Run,
)]
fn config_load_options_leaves_agent_overrides_unset_when_flags_are_omitted(
    #[case] command: Commands,
    #[case] expected_intent: CommandIntent,
) {
    let cli = Cli {
        command,
        config: None,
        engine_socket: None,
        image: None,
    };

    let options = cli.config_load_options();

    assert_eq!(options.command_intent, expected_intent);
    assert!(options.overrides.agent_kind.is_none());
    assert!(options.overrides.agent_mode.is_none());
}

#[rstest]
fn run_args_convert_to_library_run_request() {
    let args = super::RunArgs {
        repo: String::from("owner/name"),
        branch: String::from("main"),
        agent: None,
        mode: None,
    };

    let request = args
        .to_run_request()
        .expect("valid CLI run args should convert to request");

    assert_eq!(request.repository(), "owner/name");
    assert_eq!(request.branch(), "main");
}

#[rstest]
fn cli_parses_snake_case_hosted_agent_mode_values() {
    let cli = Cli::try_parse_from(["podbot", "host", "--agent-mode", "codex_app_server"])
        .expect("snake_case hosted mode should parse");

    let options = cli.config_load_options();

    assert_eq!(options.command_intent, CommandIntent::Host);
    assert_eq!(
        options.overrides.agent_mode,
        Some(AgentMode::CodexAppServer)
    );
}
