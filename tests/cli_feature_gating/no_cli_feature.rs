//! This file must fail to compile when the `cli` feature is disabled.
//! trybuild will verify the compile error.
fn main() {
    // Referencing podbot::cli should be a compile error without the `cli` feature.
    let _ = podbot::cli::Cli::config_load_options;
}
