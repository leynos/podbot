//! Shared helpers for integration tests.
//!
//! Integration tests live in separate crates under `tests/`. This module exists
//! to share small helpers between those crates without requiring process-wide
//! environment mutation (which is forbidden by the project's testing guidance).

use std::collections::HashMap;

use mockable::MockEnv;

/// Helper: Creates a `MockEnv` that returns the provided values.
pub(crate) fn env_with(values: &[(&str, &str)]) -> MockEnv {
    let map: HashMap<String, String> = values
        .iter()
        .map(|(key, value)| (String::from(*key), String::from(*value)))
        .collect();

    let mut env = MockEnv::new();
    env.expect_string()
        .returning(move |key| map.get(key).cloned());
    env
}
