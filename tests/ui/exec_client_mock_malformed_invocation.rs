//! Compile-fail contract for malformed exec-client mock invocations.

#[path = "../test_utils/exec_client_mock.rs"]
mod exec_client_mock;

define_exec_client_mock! {
    mock_base = ExecClient,
}

fn main() {}
