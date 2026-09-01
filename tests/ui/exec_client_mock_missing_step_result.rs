//! Compile-fail contract for the exec-client mock caller scope.

use podbot::api::ExecMode;
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, InspectExecFuture, ResizeExecFuture, StartExecFuture,
};

#[path = "../test_utils/exec_client_mock.rs"]
mod exec_client_mock;

define_exec_client_mock! {
    mock_base = ExecClient,
    mock_type = MockExecClient,
    exec_mode = ExecMode,
    container_id = "compile-contract-sandbox",
    exec_id = "compile-contract-exec-id",
}

fn main() {}
