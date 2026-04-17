//! Error classification for GitHub API responses.
//!
//! Maps HTTP status codes and error messages into actionable user-facing
//! messages with remediation hints.

use crate::error::GitHubError;

/// Classify a GitHub API error into an actionable authentication failure.
///
/// Extracts the HTTP status code from the Octocrab error (when available)
/// and produces a targeted error message with remediation hints. The raw
/// error is always preserved for debugging.
#[expect(
    clippy::needless_pass_by_value,
    reason = "required by map_err signature; error is consumed to extract status"
)]
pub(super) fn classify_github_api_error(error: octocrab::Error) -> GitHubError {
    match error {
        octocrab::Error::GitHub { ref source, .. } => {
            let code = source.status_code.as_u16();
            let raw = format!("{error}");
            let msg = classify_by_status(code, &raw);
            GitHubError::AuthenticationFailed { message: msg }
        }
        _ => GitHubError::AuthenticationFailed {
            message: format!(
                concat!(
                    "failed to validate GitHub App credentials: {error}. ",
                    "Hint: Check network connectivity and DNS resolution. ",
                    "The GitHub API endpoint may be unreachable.",
                ),
                error = error,
            ),
        },
    }
}

/// Format a classified message for a known HTTP status code.
///
/// `full_error` is the complete `Display` output from the Octocrab error,
/// preserving the GitHub API message body, documentation URL, error
/// details, and backtrace context for debugging.
#[must_use]
pub(crate) fn classify_by_status(code: u16, full_error: &str) -> String {
    match code {
        401 => format!(
            concat!(
                "credentials rejected (HTTP 401). ",
                "Hint: The private key may not match the App, or the App may have been ",
                "suspended. Verify the App ID and regenerate the private key from the ",
                "GitHub App settings page. If the system clock is significantly skewed, ",
                "JWT validation will also fail. Raw error: {raw}",
            ),
            raw = full_error,
        ),
        403 if is_rate_limited(full_error) => format!(
            concat!(
                "rate limit exceeded (HTTP 403). ",
                "Hint: The GitHub API rate limit has been exceeded. ",
                "Wait a few minutes and retry. Check https://www.githubstatus.com ",
                "if the problem persists. Raw error: {raw}",
            ),
            raw = full_error,
        ),
        403 => format!(
            concat!(
                "insufficient permissions (HTTP 403). ",
                "Hint: The App may lack the required permissions. Check the App's ",
                "permission settings in GitHub. Raw error: {raw}",
            ),
            raw = full_error,
        ),
        404 => format!(
            concat!(
                "App not found (HTTP 404). ",
                "Hint: Verify that github.app_id is correct. The App may have been ",
                "deleted. Raw error: {raw}",
            ),
            raw = full_error,
        ),
        500..=599 => format!(
            concat!(
                "GitHub API unavailable (HTTP {code}). ",
                "Hint: Check https://www.githubstatus.com for outage information. ",
                "Retry after the service recovers. Raw error: {raw}",
            ),
            code = code,
            raw = full_error,
        ),
        _ => format!(
            concat!(
                "unexpected response (HTTP {code}). ",
                "Hint: Check https://www.githubstatus.com for outage information. ",
                "Raw error: {raw}",
            ),
            code = code,
            raw = full_error,
        ),
    }
}

/// Check whether a GitHub API error message indicates a rate-limit
/// response rather than a genuine permissions failure.
///
/// GitHub returns HTTP 403 for both insufficient permissions *and*
/// rate-limit exhaustion. The message body distinguishes the two:
/// primary limits contain "API rate limit exceeded" and secondary
/// limits contain "secondary rate limit".
fn is_rate_limited(message: &str) -> bool {
    // Use ASCII case-insensitive search to avoid allocating a lowercased string.
    // GitHub's error messages are ASCII, so byte-wise comparison is safe.
    message
        .as_bytes()
        .windows("rate limit".len())
        .any(|window| window.eq_ignore_ascii_case(b"rate limit"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_rate_limited_detects_primary_rate_limit() {
        assert!(is_rate_limited(
            "API rate limit exceeded for installation ID 12345"
        ));
    }

    #[test]
    fn is_rate_limited_detects_secondary_rate_limit() {
        assert!(is_rate_limited("You have exceeded a secondary rate limit"));
    }

    #[test]
    fn is_rate_limited_is_case_insensitive() {
        assert!(is_rate_limited("API RATE LIMIT exceeded"));
        assert!(is_rate_limited("api Rate Limit exceeded"));
    }

    #[test]
    fn is_rate_limited_rejects_non_rate_limit_messages() {
        assert!(!is_rate_limited("Resource not accessible by integration"));
        assert!(!is_rate_limited("Bad credentials"));
    }
}
