//! Container ownership labels shared by creation and lifecycle operations.
//!
//! Podbot marks every container it creates with a managed label so listing and
//! stop operations never touch unrelated containers. Session, repository, and
//! branch labels carry the metadata operators need in `podbot ps` output.

use std::collections::HashMap;
use std::hash::BuildHasher;

/// Label key marking a container as podbot-managed.
pub const MANAGED_LABEL_KEY: &str = "io.podbot.managed";

/// Label value marking a container as podbot-managed.
pub const MANAGED_LABEL_VALUE: &str = "true";

/// Label key carrying the orchestration session identifier.
pub const SESSION_LABEL_KEY: &str = "io.podbot.session";

/// Label key carrying the `owner/name` repository identifier.
pub const REPOSITORY_LABEL_KEY: &str = "io.podbot.repository";

/// Label key carrying the checked-out branch.
pub const BRANCH_LABEL_KEY: &str = "io.podbot.branch";

/// Return the managed label as a key/value pair.
#[must_use]
pub fn managed_label() -> (String, String) {
    (
        String::from(MANAGED_LABEL_KEY),
        String::from(MANAGED_LABEL_VALUE),
    )
}

/// Return the engine label filter selecting managed containers.
#[must_use]
pub fn managed_label_filter() -> String {
    format!("{MANAGED_LABEL_KEY}={MANAGED_LABEL_VALUE}")
}

/// Build the ownership labels attached to a session container.
///
/// The order is stable so create-payload tests can assert the full label set.
#[must_use]
pub fn ownership_labels(session_id: &str, repository: &str, branch: &str) -> Vec<(String, String)> {
    vec![
        managed_label(),
        (String::from(SESSION_LABEL_KEY), String::from(session_id)),
        (String::from(REPOSITORY_LABEL_KEY), String::from(repository)),
        (String::from(BRANCH_LABEL_KEY), String::from(branch)),
    ]
}

/// Return whether a label map carries the managed marker.
#[must_use]
pub fn is_managed<S: BuildHasher>(labels: Option<&HashMap<String, String, S>>) -> bool {
    labels
        .and_then(|entries| entries.get(MANAGED_LABEL_KEY))
        .is_some_and(|value| value == MANAGED_LABEL_VALUE)
}

/// Read a label value from an optional label map.
#[must_use]
pub fn label_value<'a, S: BuildHasher>(
    labels: Option<&'a HashMap<String, String, S>>,
    key: &str,
) -> Option<&'a str> {
    labels
        .and_then(|entries| entries.get(key))
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    //! Unit tests for ownership-label helpers.

    use super::*;

    #[test]
    fn ownership_labels_start_with_managed_marker() {
        let labels = ownership_labels("session-1", "owner/name", "main");

        assert_eq!(labels.first(), Some(&managed_label()));
        assert_eq!(labels.len(), 4);
    }

    #[test]
    fn managed_filter_uses_key_value_form() {
        assert_eq!(managed_label_filter(), "io.podbot.managed=true");
    }

    #[test]
    fn is_managed_requires_matching_value() {
        let mut labels = HashMap::new();
        labels.insert(String::from(MANAGED_LABEL_KEY), String::from("false"));
        assert!(!is_managed(Some(&labels)));

        labels.insert(String::from(MANAGED_LABEL_KEY), String::from("true"));
        assert!(is_managed(Some(&labels)));

        // The generic has no inference input when the map is absent, so the
        // binding spells out the concrete hasher.
        let no_labels: Option<&HashMap<String, String>> = None;
        assert!(!is_managed(no_labels));
    }

    #[test]
    fn label_value_returns_requested_label() {
        let mut labels = HashMap::new();
        labels.insert(String::from(SESSION_LABEL_KEY), String::from("s1"));

        assert_eq!(label_value(Some(&labels), SESSION_LABEL_KEY), Some("s1"));
        assert_eq!(label_value(Some(&labels), BRANCH_LABEL_KEY), None);
    }
}
