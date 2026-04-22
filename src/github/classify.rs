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

/// Classify a GitHub API error raised while acquiring an installation token.
#[expect(
    clippy::needless_pass_by_value,
    reason = "required by map_err signature; error is consumed to extract status"
)]
pub(super) fn classify_installation_token_error(error: octocrab::Error) -> GitHubError {
    match error {
        octocrab::Error::GitHub { ref source, .. } => {
            let code = source.status_code.as_u16();
            let raw = format!("{error}");
            let message = classify_installation_token_by_status(code, &raw);
            GitHubError::TokenAcquisitionFailed { message }
        }
        _ => GitHubError::TokenAcquisitionFailed {
            message: format!(
                concat!(
                    "failed to acquire GitHub installation token: {error}. ",
                    "Hint: Check network connectivity, DNS resolution, and GitHub API reachability.",
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

/// Format installation-token acquisition failures into actionable guidance.
#[must_use]
pub(crate) fn classify_installation_token_by_status(code: u16, full_error: &str) -> String {
    match code {
        401 => format!(
            concat!(
                "GitHub rejected installation token acquisition (HTTP 401). ",
                "Hint: The App JWT may be invalid or expired. Verify the App ID, ",
                "the RSA private key, and the host clock. Raw error: {raw}",
            ),
            raw = full_error,
        ),
        403 if is_rate_limited(full_error) => format!(
            concat!(
                "GitHub rate-limited installation token acquisition (HTTP 403). ",
                "Hint: Wait and retry after the rate limit resets. ",
                "Check https://www.githubstatus.com if the problem persists. Raw error: {raw}",
            ),
            raw = full_error,
        ),
        403 => format!(
            concat!(
                "GitHub denied installation token acquisition (HTTP 403). ",
                "Hint: The installation may lack the required repository permissions ",
                "or the App may not be installed on the target repository. Raw error: {raw}",
            ),
            raw = full_error,
        ),
        404 => format!(
            concat!(
                "GitHub installation not found (HTTP 404). ",
                "Hint: Verify github.installation_id matches the configured App and ",
                "that the installation still exists. Raw error: {raw}",
            ),
            raw = full_error,
        ),
        500..=599 => format!(
            concat!(
                "GitHub API unavailable during installation token acquisition (HTTP {code}). ",
                "Hint: Retry after the service recovers and check https://www.githubstatus.com. ",
                "Raw error: {raw}",
            ),
            code = code,
            raw = full_error,
        ),
        _ => format!(
            concat!(
                "unexpected response while acquiring installation token (HTTP {code}). ",
                "Hint: Check GitHub service status and the raw error for details. Raw error: {raw}",
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
