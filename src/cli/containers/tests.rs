//! Unit tests for `podbot ps` table rendering.

use super::*;

fn summary(id: &str, name: &str) -> ContainerSummary {
    ContainerSummary {
        id: String::from(id),
        name: String::from(name),
        image: String::from("ghcr.io/example/podbot:latest"),
        state: String::from("running"),
        status: String::from("Up 2 minutes"),
        created_unix_seconds: Some(0),
        repository: Some(String::from("owner/name")),
        branch: Some(String::from("main")),
    }
}

#[test]
fn empty_listing_renders_friendly_message() {
    assert_eq!(
        render_container_table(&[]),
        "No podbot containers are running."
    );
}

#[test]
fn table_starts_with_headings() {
    let rendered = render_container_table(&[summary("0123456789abcdef", "podbot-one")]);

    let heading = rendered.lines().next().expect("table should have headings");
    assert!(heading.starts_with("CONTAINER ID"));
    assert!(heading.contains("REPOSITORY"));
    assert!(heading.ends_with("CREATED"));
}

#[test]
fn identifiers_are_truncated_to_twelve_characters() {
    let rendered = render_container_table(&[summary("0123456789abcdef", "podbot-one")]);

    let row = rendered.lines().nth(1).expect("table should have one row");
    assert!(row.starts_with("0123456789ab"));
    assert!(!row.contains("0123456789abcdef"));
}

#[test]
fn missing_labels_render_as_dashes() {
    let mut container = summary("0123456789abcdef", "podbot-one");
    container.repository = None;
    container.branch = None;

    let rendered = render_container_table(&[container]);

    let row = rendered.lines().nth(1).expect("table should have one row");
    assert!(row.contains(" - "), "row should contain a dash cell: {row}");
}

#[test]
fn created_timestamp_is_rendered_as_utc() {
    assert_eq!(render_created(Some(0)), "1970-01-01 00:00:00");
    assert_eq!(render_created(None), "-");
}
