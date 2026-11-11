//! Rendering for `podbot ps` container listings.
//!
//! The formatter keeps presentation out of the orchestration API. Callers
//! supply [`ContainerSummary`] records and receive a printable table, or a
//! friendly message when no managed containers are running.

use crate::api::ContainerSummary;

/// Column headings in display order.
const HEADINGS: [&str; 8] = [
    "CONTAINER ID",
    "NAME",
    "IMAGE",
    "STATE",
    "STATUS",
    "REPOSITORY",
    "BRANCH",
    "CREATED",
];

/// Render a container table, or an empty-state message.
///
/// # Examples
///
/// ```
/// use podbot::api::ContainerSummary;
/// use podbot::cli::render_container_table;
///
/// assert_eq!(
///     render_container_table(&[]),
///     "No podbot containers are running."
/// );
/// ```
#[must_use]
pub fn render_container_table(containers: &[ContainerSummary]) -> String {
    if containers.is_empty() {
        return String::from("No podbot containers are running.");
    }

    let rows: Vec<[String; 8]> = containers
        .iter()
        .map(|container| {
            [
                short_id(&container.id).to_owned(),
                container.name.clone(),
                container.image.clone(),
                container.state.clone(),
                container.status.clone(),
                or_dash(container.repository.as_deref()),
                or_dash(container.branch.as_deref()),
                render_created(container.created_unix_seconds),
            ]
        })
        .collect();

    let widths = column_widths(&rows);
    let mut lines = Vec::with_capacity(rows.len() + 1);
    lines.push(render_row(&HEADINGS.map(String::from), &widths));
    lines.extend(rows.iter().map(|row| render_row(row, &widths)));
    lines.join("\n")
}

/// Truncate engine identifiers to the familiar twelve-character prefix.
fn short_id(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}

/// Render an optional label value, using a dash when it is absent.
fn or_dash(value: Option<&str>) -> String {
    value.unwrap_or("-").to_owned()
}

/// Render a Unix timestamp as a UTC date and time, or a dash when unknown.
fn render_created(timestamp: Option<i64>) -> String {
    timestamp
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
        .map_or_else(
            || String::from("-"),
            |at| at.format("%Y-%m-%d %H:%M:%S").to_string(),
        )
}

/// Compute the width of every column from its heading and cell values.
fn column_widths(rows: &[[String; 8]]) -> [usize; 8] {
    let mut widths = HEADINGS.map(str::len);
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    widths
}

/// Render one padded row with two spaces between columns.
fn render_row(cells: &[String; 8], widths: &[usize; 8]) -> String {
    let padded: Vec<String> = cells
        .iter()
        .zip(widths)
        .map(|(cell, width)| format!("{cell:<width$}"))
        .collect();
    padded.join("  ").trim_end().to_owned()
}

#[cfg(test)]
mod tests;
