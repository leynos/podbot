//! TOML serialization tests for `SandboxConfig`.
//!
//! These tests validate the TOML serialization and deserialization behaviour
//! of the `SandboxConfig` struct, ensuring round-trip consistency and correct
//! default value handling.

use podbot::config::{AppConfig, SandboxConfig};
use rstest::rstest;

#[rstest]
fn sandbox_config_serializes_to_toml() {
    let config = SandboxConfig::default();
    let toml_str = toml::to_string(&config).expect("serialization should succeed");
    let parsed: SandboxConfig = toml::from_str(&toml_str).expect("deserialization should succeed");
    assert!(!parsed.privileged, "default privileged should be false");
    assert!(
        parsed.mount_dev_fuse,
        "default mount_dev_fuse should be true"
    );
}

#[rstest]
fn sandbox_config_round_trips_through_toml() {
    let config = SandboxConfig {
        privileged: true,
        mount_dev_fuse: false,
        ..Default::default()
    };
    let toml_str = toml::to_string(&config).expect("serialization should succeed");
    let parsed: SandboxConfig = toml::from_str(&toml_str).expect("deserialization should succeed");
    assert_eq!(parsed.privileged, config.privileged);
    assert_eq!(parsed.mount_dev_fuse, config.mount_dev_fuse);
    assert_eq!(parsed.selinux_label_mode, config.selinux_label_mode);
}

#[rstest]
#[case(false, false, "minimal mode without fuse")]
#[case(false, true, "minimal mode with fuse (default)")]
#[case(true, false, "privileged mode without fuse")]
#[case(true, true, "privileged mode with fuse")]
fn sandbox_config_all_combinations(
    #[case] privileged: bool,
    #[case] mount_dev_fuse: bool,
    #[case] description: &str,
) {
    let config = SandboxConfig {
        privileged,
        mount_dev_fuse,
        ..Default::default()
    };
    let toml_str = toml::to_string(&config).expect("serialization should succeed");
    let parsed: SandboxConfig = toml::from_str(&toml_str).expect("deserialization should succeed");
    assert_eq!(
        parsed.privileged, privileged,
        "privileged mismatch for {description}"
    );
    assert_eq!(
        parsed.mount_dev_fuse, mount_dev_fuse,
        "mount_dev_fuse mismatch for {description}"
    );
}

#[rstest]
#[case(
    "both_fields_explicit",
    "[sandbox]\nprivileged = true\nmount_dev_fuse = false",
    true,
    false
)]
#[case(
    "section_omitted",
    r#"engine_socket = "unix:///tmp/test.sock""#,
    false,
    true
)]
#[case("missing_field_defaults", "[sandbox]\nprivileged = true", true, true)]
fn sandbox_config_parameterized(
    #[case] description: &str,
    #[case] toml_input: &str,
    #[case] expected_privileged: bool,
    #[case] expected_mount_dev_fuse: bool,
) {
    let config: AppConfig = toml::from_str(toml_input).expect("TOML parsing should succeed");
    assert_eq!(
        config.sandbox.privileged, expected_privileged,
        "{description}: privileged mismatch"
    );
    assert_eq!(
        config.sandbox.mount_dev_fuse, expected_mount_dev_fuse,
        "{description}: mount_dev_fuse mismatch"
    );
}
