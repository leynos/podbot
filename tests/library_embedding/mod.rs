//! Internal-feature integration suite for embedding and compatibility paths.
//!
//! These tests run only with `feature = "internal"` because they exercise
//! internal shims such as `podbot::engine`. The stable public embedding
//! boundary remains `podbot::api`, `podbot::config`, and `podbot::error`;
//! `podbot::engine` and `podbot::github` are internal compatibility modules,
//! while `podbot::cli` visibility is controlled by the `cli` feature.

#![cfg(feature = "internal")]

mod exec;

#[path = "../test_utils/mod.rs"]
mod test_utils;

use rstest::rstest;

use podbot::api::{CommandOutcome, RunRequest};
#[cfg(feature = "experimental")]
use podbot::api::{list_containers, run_agent, run_token_daemon, stop_container};
#[cfg(feature = "experimental")]
use podbot::config::AppConfig;
use podbot::config::{CommandIntent, ConfigLoadOptions, ConfigOverrides, load_config};
use podbot::error::{ConfigError, ContainerError, PodbotError};

// -------------------------------------------------------------------------
// Configuration loading from host-style call path
// -------------------------------------------------------------------------

#[rstest]
fn load_config_without_cli_types() {
    let options = ConfigLoadOptions {
        config_path_hint: None,
        discover_config: false,
        overrides: ConfigOverrides {
            engine_socket: Some(String::from("unix:///test/embed.sock")),
            image: Some(String::from("test-image:latest")),
            agent_kind: None,
            agent_mode: None,
        },
        command_intent: CommandIntent::Any,
    };

    let config = load_config(&options);
    assert!(config.is_ok(), "config loading should succeed");

    if let Ok(ref cfg) = config {
        assert_eq!(
            cfg.engine_socket.as_deref(),
            Some("unix:///test/embed.sock")
        );
        assert_eq!(cfg.image.as_deref(), Some("test-image:latest"));
    }
}

// -------------------------------------------------------------------------
// Error type contract
// -------------------------------------------------------------------------

#[rstest]
fn error_types_are_matchable() {
    let config_err: PodbotError = ConfigError::MissingRequired {
        field: String::from("image"),
    }
    .into();

    assert!(
        matches!(
            config_err,
            PodbotError::Config(ConfigError::MissingRequired { .. })
        ),
        "PodbotError::Config should be matchable"
    );

    let container_err: PodbotError = ContainerError::ConnectionFailed {
        message: String::from("refused"),
    }
    .into();

    assert!(
        matches!(
            container_err,
            PodbotError::Container(ContainerError::ConnectionFailed { .. })
        ),
        "PodbotError::Container should be matchable"
    );
}

// -------------------------------------------------------------------------
// Stub orchestration functions
// -------------------------------------------------------------------------

#[rstest]
fn run_request_can_be_constructed_without_cli_types() {
    let request =
        RunRequest::new("owner/name", "main").expect("library run request should be valid");

    assert_eq!(request.repository(), "owner/name");
    assert_eq!(request.branch(), "main");
}

#[rstest]
#[cfg(feature = "experimental")]
fn stub_orchestration_functions_return_success() {
    let config = AppConfig::default();
    let request = RunRequest::new("owner/name", "main").expect("run request should be valid");

    assert!(
        matches!(run_agent(&config, &request), Ok(CommandOutcome::Success)),
        "run_agent should return Success"
    );
    assert!(
        matches!(list_containers(), Ok(CommandOutcome::Success)),
        "list_containers should return Success"
    );
    assert!(
        matches!(stop_container("test-ctr"), Ok(CommandOutcome::Success)),
        "stop_container should return Success"
    );
    assert!(
        matches!(run_token_daemon("test-ctr"), Ok(CommandOutcome::Success)),
        "run_token_daemon should return Success"
    );
}
