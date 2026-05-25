//! Property tests for experimental repository name validation helpers.
//!
//! These tests exercise the private predicates used by `run_agent` before it
//! accepts a `RunRequest` for hosted orchestration. They live beside the API
//! tests because the helpers are private implementation details of
//! `crate::api`, not part of the stable public surface.

use proptest::prelude::*;

fn replace_whitespace_with_visible_text(input: &str) -> String {
    input
        .chars()
        .map(|character| {
            if character.is_whitespace() {
                'A'
            } else {
                character
            }
        })
        .collect()
}

proptest! {
    #[test]
    #[cfg(feature = "experimental")]
    fn repository_segment_validation_follows_non_empty_whitespace_free_rule(
        segment in "[\\sA-Za-z0-9._/-]{0,64}",
    ) {
        prop_assert_eq!(
            super::super::is_repository_segment(&segment),
            !segment.is_empty() && !segment.chars().any(char::is_whitespace)
        );
    }

    #[test]
    #[cfg(feature = "experimental")]
    fn owner_repository_validation_requires_two_valid_segments(
        owner in "[\\sA-Za-z0-9._-]{0,32}",
        name in "[\\sA-Za-z0-9._-]{0,32}",
        extra in proptest::option::of("[\\sA-Za-z0-9._-]{0,32}"),
    ) {
        let has_extra_segment = extra.is_some();
        let repository = extra.as_deref().map_or_else(
            || format!("{owner}/{name}"),
            |extra_segment| format!("{owner}/{name}/{extra_segment}"),
        );
        let expected = !has_extra_segment
            && super::super::is_repository_segment(&owner)
            && super::super::is_repository_segment(&name);

        prop_assert_eq!(super::super::is_owner_repository_name(&repository), expected);
    }

    #[test]
    #[cfg(feature = "experimental")]
    fn repository_segment_accepts_non_empty_non_whitespace(
        segment in proptest::string::string_regex(r"[^ \t\r\n\x0C]{1,64}")
            .expect("valid regex should compile")
            .prop_map(|segment| replace_whitespace_with_visible_text(&segment)),
    ) {
        prop_assert!(super::super::is_repository_segment(&segment));
    }

    #[test]
    #[cfg(feature = "experimental")]
    fn repository_segment_rejects_whitespace_bearing_strings(
        segment in proptest::string::string_regex(".{1,32}")
            .expect("valid regex should compile"),
    ) {
        prop_assume!(segment.chars().any(char::is_whitespace));
        prop_assert!(!super::super::is_repository_segment(&segment));
    }

    #[test]
    #[cfg(feature = "experimental")]
    fn owner_repository_name_accepts_valid_owner_slash_name(
        owner in proptest::string::string_regex(r"[^ \t\r\n\x0C/]{1,32}")
            .expect("valid regex should compile")
            .prop_map(|owner| replace_whitespace_with_visible_text(&owner)),
        name in proptest::string::string_regex(r"[^ \t\r\n\x0C/]{1,32}")
            .expect("valid regex should compile")
            .prop_map(|name| replace_whitespace_with_visible_text(&name)),
    ) {
        let repo = format!("{owner}/{name}");
        prop_assert!(super::super::is_owner_repository_name(&repo));
    }

    #[test]
    #[cfg(feature = "experimental")]
    fn owner_repository_name_rejects_no_slash(
        segment in proptest::string::string_regex(r"[^ \t\r\n\x0C/]{1,64}")
            .expect("valid regex should compile"),
    ) {
        prop_assert!(!super::super::is_owner_repository_name(&segment));
    }

    #[test]
    #[cfg(feature = "experimental")]
    fn owner_repository_name_rejects_two_or_more_slashes(
        owner in proptest::string::string_regex(r"[^ \t\r\n\x0C/]{1,16}")
            .expect("valid regex should compile"),
        middle in proptest::string::string_regex(r"[^ \t\r\n\x0C/]{1,16}")
            .expect("valid regex should compile"),
        name in proptest::string::string_regex(r"[^ \t\r\n\x0C/]{1,16}")
            .expect("valid regex should compile"),
    ) {
        let repo = format!("{owner}/{middle}/{name}");
        prop_assert!(!super::super::is_owner_repository_name(&repo));
    }
}

#[test]
#[cfg(feature = "experimental")]
fn repository_segment_rejects_empty() {
    assert!(!super::super::is_repository_segment(""));
}

#[test]
#[cfg(feature = "experimental")]
fn owner_repository_name_rejects_empty_segment() {
    assert!(!super::super::is_owner_repository_name("/name"));
    assert!(!super::super::is_owner_repository_name("owner/"));
    assert!(!super::super::is_owner_repository_name("/"));
}
