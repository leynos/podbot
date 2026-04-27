//! Compile-fail proof that stable embedders cannot import `podbot::engine`
//! unless the `internal` feature is enabled.

fn main() {
    let _ = podbot::engine::ExecMode::Attached;
}
