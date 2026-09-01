//! Compile-pass contract for the shared exec-client mock macro.

use podbot::api::ExecMode;
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, InspectExecFuture, ResizeExecFuture, StartExecFuture,
};

type StepResult<T> = Result<T, String>;

#[path = "../test_utils/exec_client_mock.rs"]
mod exec_client_mock;

define_exec_client_mock! {
    mock_base = ExecClient,
    mock_type = MockExecClient,
    exec_mode = ExecMode,
    container_id = "compile-contract-sandbox",
    exec_id = "compile-contract-exec-id",
}

fn main() {
    let mut client = MockExecClient::new();
    configure_create_exec(&mut client, false);
    configure_resize(&mut client, ExecMode::Attached)
        .expect("attached resize expectation should be configurable");
    configure_inspect(&mut client, Some(0));
    let _ = client.create_exec(
        "compile-contract-sandbox",
        bollard::exec::CreateExecOptions {
            cmd: Some(vec![String::from("true")]),
            ..bollard::exec::CreateExecOptions::default()
        },
    );
    let _ = client.inspect_exec("compile-contract-exec-id");
}
