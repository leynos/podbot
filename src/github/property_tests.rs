//! Property-based tests for GitHub retry metric status classification.

use proptest::prelude::*;

use super::super::retry_metrics::github_status_class;

proptest! {
    #[test]
    fn github_status_class_range_invariant(raw_code in 100_u16..=999_u16) {
        let status = http::StatusCode::from_u16(raw_code)
            .expect("codes 100–999 are valid HTTP status codes");
        let class = github_status_class(status);
        let expected = match raw_code {
            100..=199 => "1xx",
            200..=299 => "2xx",
            300..=399 => "3xx",
            400..=499 => "4xx",
            500..=599 => "5xx",
            _ => "other",
        };
        prop_assert_eq!(
            class,
            expected,
            "github_status_class({}) should map to {}",
            raw_code,
            expected,
        );
    }
}
