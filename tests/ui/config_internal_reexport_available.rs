//! Compile-pass proof that `merge_from_layers_for_tests` is available when the
//! crate is built with the `internal` feature.

use podbot::config::merge_from_layers_for_tests;

fn main() {
    let _:
        fn(Vec<ortho_config::MergeLayer<'static>>) -> ortho_config::OrthoResult<podbot::config::AppConfig> =
        merge_from_layers_for_tests::<Vec<ortho_config::MergeLayer<'static>>>;
}
