//! Unit tests for `CreateContainerRequest` construction and validation.

use rstest::rstest;

use super::*;

#[rstest]
#[case::keep_default(SelinuxLabelMode::KeepDefault)]
#[case::disable_for_container(SelinuxLabelMode::DisableForContainer)]
fn from_sandbox_config_passes_through_all_fields(#[case] selinux: SelinuxLabelMode) {
    let sandbox = SandboxConfig {
        privileged: true,
        mount_dev_fuse: false,
        selinux_label_mode: selinux,
    };

    let security = ContainerSecurityOptions::from_sandbox_config(&sandbox);

    assert!(security.privileged);
    assert!(!security.mount_dev_fuse);
    assert_eq!(security.selinux_label_mode, selinux);
}

#[rstest]
fn create_container_request_from_app_config_uses_image_and_security() {
    let config = AppConfig {
        image: Some(String::from("ghcr.io/example/sandbox:latest")),
        sandbox: SandboxConfig {
            privileged: true,
            mount_dev_fuse: false,
            selinux_label_mode: SelinuxLabelMode::KeepDefault,
        },
        ..AppConfig::default()
    };
    let request = CreateContainerRequest::from_app_config(&config)
        .expect("request construction from config should succeed");
    assert_eq!(request.image(), "ghcr.io/example/sandbox:latest");
    assert!(request.security().privileged);
    assert!(!request.security().mount_dev_fuse);
    assert_eq!(
        request.security().selinux_label_mode,
        SelinuxLabelMode::KeepDefault
    );
}

#[rstest]
#[case::missing_image(None)]
#[case::whitespace_only_image(Some("   "))]
fn create_container_request_from_app_config_requires_image(#[case] image: Option<&str>) {
    let config = AppConfig {
        image: image.map(String::from),
        ..AppConfig::default()
    };
    let request = CreateContainerRequest::from_app_config(&config);
    assert!(
        matches!(
            request,
            Err(PodbotError::Config(ConfigError::MissingRequired { ref field }))
                if field == "image"
        ),
        "expected missing image validation error, got: {request:?}"
    );
}

#[rstest]
fn create_container_request_from_app_config_trims_surrounding_whitespace() {
    let config = AppConfig {
        image: Some(String::from("  ghcr.io/example/sandbox:latest  ")),
        ..AppConfig::default()
    };
    let request = CreateContainerRequest::from_app_config(&config)
        .expect("request construction from config should succeed");
    assert_eq!(request.image(), "ghcr.io/example/sandbox:latest");
}
