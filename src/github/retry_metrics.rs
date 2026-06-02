//! Octocrab retry policy metrics and warning emission.
//!
//! Bridges Octocrab's `RateLimitMetrics` hook into Podbot's `tracing`
//! warnings and `metrics` counters so retryable responses and rate-limit
//! waits are observable through the standard subscribers.

use octocrab::service::middleware::retry::RateLimitMetrics;

pub(super) struct PodbotOctocrabRetryMetrics;

struct RetryEventContext<'a> {
    req: &'a http::Request<octocrab::OctoBody>,
    status_code: http::StatusCode,
    retries_remaining: usize,
}

impl<'a> RetryEventContext<'a> {
    const fn new(
        req: &'a http::Request<octocrab::OctoBody>,
        status_code: http::StatusCode,
        retries_remaining: usize,
    ) -> Self {
        Self {
            req,
            status_code,
            retries_remaining,
        }
    }

    fn log_warn(&self, message: &'static str) {
        tracing::warn!(
            operation = "github_api",
            method = %self.req.method(),
            request_path = self.req.uri().path(),
            status_code = self.status_code.as_u16(),
            retries_remaining = self.retries_remaining,
            "{message}"
        );
    }

    fn log_warn_with_wait(&self, message: &'static str, waiting_seconds: u64) {
        tracing::warn!(
            operation = "github_api",
            method = %self.req.method(),
            request_path = self.req.uri().path(),
            status_code = self.status_code.as_u16(),
            retries_remaining = self.retries_remaining,
            waiting_seconds,
            "{message}"
        );
    }
}

impl RateLimitMetrics for PodbotOctocrabRetryMetrics {
    fn retry_after_error(
        &self,
        req: &http::Request<octocrab::OctoBody>,
        status_code: http::StatusCode,
        retries_remaining: usize,
    ) {
        RetryEventContext::new(req, status_code, retries_remaining)
            .log_warn("Octocrab retry policy observed a retryable GitHub API response");
        record_octocrab_retry_event("retryable_response", status_code);
    }

    fn rate_limited(
        &self,
        req: &http::Request<octocrab::OctoBody>,
        status_code: http::StatusCode,
        retries_remaining: usize,
        waiting_seconds: u64,
    ) {
        RetryEventContext::new(req, status_code, retries_remaining).log_warn_with_wait(
            "Octocrab retry policy is waiting before retrying a GitHub API request",
            waiting_seconds,
        );
        record_octocrab_retry_event("rate_limited", status_code);
    }
}

pub(super) fn record_octocrab_retry_event(event: &'static str, status_code: http::StatusCode) {
    metrics::counter!(
        "podbot.github.octocrab.retry.events.total",
        "operation" => "github_api",
        "event" => event,
        "status_class" => github_status_class(status_code),
    )
    .increment(1);
}

pub(super) const fn github_status_class(status_code: http::StatusCode) -> &'static str {
    match status_code.as_u16() {
        100..=199 => "1xx",
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    }
}

#[cfg(test)]
#[path = "retry_metrics_tests.rs"]
mod tests;
