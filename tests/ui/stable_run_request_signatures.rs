//! Compile-pass signature lock for the stable `RunRequest` API.
//!
//! This fixture must remain compile-pass only and exists to catch accidental
//! signature drift for `RunRequest`.

use podbot::api::RunRequest;

fn main() {
    let _new: fn(String, String) -> podbot::error::Result<RunRequest> = RunRequest::new;
    let _repository: fn(&RunRequest) -> &str = RunRequest::repository;
    let _branch: fn(&RunRequest) -> &str = RunRequest::branch;
}
