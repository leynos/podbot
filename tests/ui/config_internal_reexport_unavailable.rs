//! Compile-fail proof that `merge_from_layers_for_tests` is not visible
//! without the `internal` feature.

fn main() {
    let _ = podbot::config::merge_from_layers_for_tests;
}
