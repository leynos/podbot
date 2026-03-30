//! Host-style integration tests for the documented stable library boundary.

use podbot::api::{ExecContext, ExecMode, ExecRequest};
use podbot::config::AppConfig;
use rstest::rstest;

#[rstest]
fn stable_embedder_path_uses_only_supported_modules() {
    let config = AppConfig::default();
    let request = ExecRequest::new("sandbox", vec![String::from("echo"), String::from("hello")])
        .expect("request should be valid")
        .with_mode(ExecMode::Protocol)
        .with_tty(true);

    let runtime = tokio::runtime::Runtime::new().expect("runtime should be created");
    let _context = ExecContext::connect(&config, runtime.handle());
    let _ = request;
}
