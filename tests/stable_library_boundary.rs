//! Host-style integration tests for the documented stable library boundary.

use podbot::api::{ExecMode, ExecRequest, exec};
use podbot::config::AppConfig;
use podbot::error::{ConfigError, PodbotError};
use rstest::rstest;

#[rstest]
fn stable_embedder_path_uses_only_supported_modules() {
    let config = AppConfig::default();
    let request = ExecRequest::new("sandbox", vec![String::from("echo"), String::from("hello")])
        .expect("request should be valid")
        .with_mode(ExecMode::Protocol)
        .with_tty(true);

    let _ = (&config, &request);
}

#[rstest]
fn exec_returns_podbot_error_for_invalid_request() {
    let config = AppConfig::default();
    let request = ExecRequest {
        container: String::from("sandbox"),
        command: Vec::new(),
        mode: ExecMode::Attached,
        tty: false,
    };

    let error = exec(&config, &request).expect_err("invalid request should not reach the engine");
    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::MissingRequired { field }) if field == "command"
    ));
}
