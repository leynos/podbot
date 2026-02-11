//! Container creation with configurable sandbox security options.
//!
//! This module translates high-level security settings into `Bollard`
//! container-create payloads and provides async/sync helpers for creating
//! containers.

use std::future::Future;
use std::pin::Pin;

use bollard::Docker;
use bollard::models::{ContainerCreateBody, ContainerCreateResponse, DeviceMapping, HostConfig};
use bollard::query_parameters::{CreateContainerOptions, CreateContainerOptionsBuilder};

use super::EngineConnector;
use crate::config::SandboxConfig;
use crate::error::{ConfigError, ContainerError, PodbotError};

const DEV_FUSE_PATH: &str = "/dev/fuse";
const FUSE_DEVICE_PERMISSIONS: &str = "rwm";
const CAP_SYS_ADMIN: &str = "SYS_ADMIN";
const SELINUX_LABEL_DISABLE: &str = "label=disable";

/// Boxed future type returned by [`ContainerCreator`] implementors.
pub type CreateContainerFuture<'a> = Pin<
    Box<dyn Future<Output = Result<ContainerCreateResponse, bollard::errors::Error>> + Send + 'a>,
>;

/// Behaviour required to create a container via a backing engine client.
///
/// This abstraction exists to keep container-creation logic testable without a
/// running daemon.
pub trait ContainerCreator {
    /// Create a container from `Bollard` options and body payload.
    fn create_container(
        &self,
        options: Option<CreateContainerOptions>,
        config: ContainerCreateBody,
    ) -> CreateContainerFuture<'_>;
}

impl ContainerCreator for Docker {
    fn create_container(
        &self,
        options: Option<CreateContainerOptions>,
        config: ContainerCreateBody,
    ) -> CreateContainerFuture<'_> {
        Box::pin(async move { Self::create_container(self, options, config).await })
    }
}

/// How `SELinux` labels should be applied to the container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelinuxLabelMode {
    /// Keep engine defaults for `SELinux` labels.
    KeepDefault,

    /// Disable labels for the container process.
    #[default]
    DisableForContainer,
}

/// Container security options applied at create time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerSecurityOptions {
    /// Whether the container should run in privileged mode.
    pub privileged: bool,

    /// Whether `/dev/fuse` should be mounted into the container.
    pub mount_dev_fuse: bool,

    /// `SELinux` label handling mode.
    pub selinux_label_mode: SelinuxLabelMode,
}

impl ContainerSecurityOptions {
    /// Build security options from `[sandbox]` configuration.
    #[must_use]
    pub const fn from_sandbox_config(sandbox: &SandboxConfig) -> Self {
        let selinux_label_mode = if sandbox.privileged {
            SelinuxLabelMode::KeepDefault
        } else {
            SelinuxLabelMode::DisableForContainer
        };

        Self {
            privileged: sandbox.privileged,
            mount_dev_fuse: sandbox.mount_dev_fuse,
            selinux_label_mode,
        }
    }
}

impl Default for ContainerSecurityOptions {
    fn default() -> Self {
        Self {
            privileged: false,
            mount_dev_fuse: true,
            selinux_label_mode: SelinuxLabelMode::DisableForContainer,
        }
    }
}

/// Container-creation request parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateContainerRequest {
    /// The container image to create from.
    pub image: Option<String>,

    /// Optional container name.
    pub name: Option<String>,

    /// Optional command to run in the container.
    pub cmd: Option<Vec<String>>,

    /// Optional environment variables in `KEY=value` form.
    pub env: Option<Vec<String>>,

    /// Security profile to apply.
    pub security: ContainerSecurityOptions,
}

impl CreateContainerRequest {
    /// Create a request with image and security settings.
    #[must_use]
    pub const fn new(image: Option<String>, security: ContainerSecurityOptions) -> Self {
        Self {
            image,
            name: None,
            cmd: None,
            env: None,
            security,
        }
    }
}

impl EngineConnector {
    /// Create a container using a provided client abstraction (async version).
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingRequired` when `request.image` is missing
    /// or empty.
    ///
    /// Returns `ContainerError::CreateFailed` when the engine rejects the
    /// create request.
    pub async fn create_container_async<C: ContainerCreator>(
        creator: &C,
        request: &CreateContainerRequest,
    ) -> Result<String, PodbotError> {
        let image = validate_image(request.image.as_deref())?;
        let options = build_create_options(request.name.as_deref());
        let config = build_create_body(image, request);

        let response = creator
            .create_container(options, config)
            .await
            .map_err(|error| {
                PodbotError::from(ContainerError::CreateFailed {
                    message: error.to_string(),
                })
            })?;

        Ok(response.id)
    }

    /// Create a container using a provided client abstraction.
    ///
    /// This synchronous helper spins up a dedicated tokio runtime and blocks
    /// on [`Self::create_container_async`].
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::RuntimeCreationFailed` if a runtime cannot be
    /// created.
    ///
    /// Returns `ConfigError::MissingRequired` when `request.image` is missing
    /// or empty.
    ///
    /// Returns `ContainerError::CreateFailed` when the engine rejects the
    /// create request.
    pub fn create_container<C: ContainerCreator>(
        creator: &C,
        request: &CreateContainerRequest,
    ) -> Result<String, PodbotError> {
        let runtime = Self::create_runtime()?;
        runtime.block_on(Self::create_container_async(creator, request))
    }
}

fn validate_image(image: Option<&str>) -> Result<&str, PodbotError> {
    image
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            PodbotError::from(ConfigError::MissingRequired {
                field: String::from("image"),
            })
        })
}

fn build_create_options(name: Option<&str>) -> Option<CreateContainerOptions> {
    name.filter(|value| !value.trim().is_empty())
        .map(|container_name| {
            CreateContainerOptionsBuilder::new()
                .name(container_name)
                .build()
        })
}

fn build_create_body(image: &str, request: &CreateContainerRequest) -> ContainerCreateBody {
    ContainerCreateBody {
        image: Some(String::from(image)),
        cmd: request.cmd.clone(),
        env: request.env.clone(),
        host_config: Some(build_host_config(&request.security)),
        ..ContainerCreateBody::default()
    }
}

fn build_host_config(security: &ContainerSecurityOptions) -> HostConfig {
    if security.privileged {
        return HostConfig {
            privileged: Some(true),
            ..HostConfig::default()
        };
    }

    HostConfig {
        privileged: Some(false),
        cap_add: security
            .mount_dev_fuse
            .then(|| vec![String::from(CAP_SYS_ADMIN)]),
        devices: security.mount_dev_fuse.then(|| vec![fuse_device_mapping()]),
        security_opt: security
            .selinux_label_mode
            .requires_label_disable()
            .then(|| vec![String::from(SELINUX_LABEL_DISABLE)]),
        ..HostConfig::default()
    }
}

fn fuse_device_mapping() -> DeviceMapping {
    DeviceMapping {
        path_on_host: Some(String::from(DEV_FUSE_PATH)),
        path_in_container: Some(String::from(DEV_FUSE_PATH)),
        cgroup_permissions: Some(String::from(FUSE_DEVICE_PERMISSIONS)),
    }
}

impl SelinuxLabelMode {
    const fn requires_label_disable(self) -> bool {
        matches!(self, Self::DisableForContainer)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use bollard::models::ContainerCreateResponse;
    use rstest::{fixture, rstest};

    use super::*;
    use crate::error::{ConfigError, ContainerError};

    #[derive(Debug)]
    struct MockCreatorState {
        result: Option<Result<ContainerCreateResponse, bollard::errors::Error>>,
        call_count: usize,
        options: Option<CreateContainerOptions>,
        body: Option<ContainerCreateBody>,
    }

    #[derive(Clone, Debug)]
    struct MockCreator {
        state: Arc<Mutex<MockCreatorState>>,
    }

    impl MockCreator {
        fn succeed_with(container_id: &str) -> Self {
            Self {
                state: Arc::new(Mutex::new(MockCreatorState {
                    result: Some(Ok(ContainerCreateResponse {
                        id: String::from(container_id),
                        warnings: vec![],
                    })),
                    call_count: 0,
                    options: None,
                    body: None,
                })),
            }
        }

        fn fail_with(error: bollard::errors::Error) -> Self {
            Self {
                state: Arc::new(Mutex::new(MockCreatorState {
                    result: Some(Err(error)),
                    call_count: 0,
                    options: None,
                    body: None,
                })),
            }
        }

        fn take_body(&self) -> Option<ContainerCreateBody> {
            self.state
                .lock()
                .expect("mock creator state lock should succeed")
                .body
                .clone()
        }

        fn take_options(&self) -> Option<CreateContainerOptions> {
            self.state
                .lock()
                .expect("mock creator state lock should succeed")
                .options
                .clone()
        }

        fn call_count(&self) -> usize {
            self.state
                .lock()
                .expect("mock creator state lock should succeed")
                .call_count
        }
    }

    impl ContainerCreator for MockCreator {
        fn create_container(
            &self,
            options: Option<CreateContainerOptions>,
            config: ContainerCreateBody,
        ) -> CreateContainerFuture<'_> {
            let state = Arc::clone(&self.state);

            Box::pin(async move {
                let mut locked = state
                    .lock()
                    .expect("mock creator state lock should succeed");
                locked.call_count += 1;
                locked.options = options;
                locked.body = Some(config);
                locked
                    .result
                    .take()
                    .expect("mock creator result should be configured")
            })
        }
    }

    #[fixture]
    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Runtime::new().expect("tokio runtime creation should succeed")
    }

    #[rstest]
    fn from_sandbox_config_preserves_flags() {
        let sandbox = SandboxConfig {
            privileged: true,
            mount_dev_fuse: false,
        };

        let security = ContainerSecurityOptions::from_sandbox_config(&sandbox);

        assert!(security.privileged);
        assert!(!security.mount_dev_fuse);
        assert_eq!(security.selinux_label_mode, SelinuxLabelMode::KeepDefault);
    }

    #[rstest]
    fn create_container_privileged_mode_has_minimal_overrides(runtime: tokio::runtime::Runtime) {
        let creator = MockCreator::succeed_with("container-id");
        let request = CreateContainerRequest {
            image: Some(String::from("ghcr.io/example/sandbox:latest")),
            name: Some(String::from("podbot-test")),
            cmd: None,
            env: None,
            security: ContainerSecurityOptions {
                privileged: true,
                mount_dev_fuse: true,
                selinux_label_mode: SelinuxLabelMode::KeepDefault,
            },
        };

        let result = runtime.block_on(EngineConnector::create_container_async(&creator, &request));

        assert_eq!(
            result.expect("container creation should succeed"),
            "container-id"
        );
        assert_eq!(creator.call_count(), 1);

        let options = creator
            .take_options()
            .expect("create options should be captured");
        assert_eq!(options.name.as_deref(), Some("podbot-test"));

        let body = creator
            .take_body()
            .expect("container body should be captured");
        let host_config = body.host_config.expect("host config should be set");
        assert_eq!(host_config.privileged, Some(true));
        assert!(host_config.cap_add.is_none());
        assert!(host_config.devices.is_none());
        assert!(host_config.security_opt.is_none());
    }

    #[rstest]
    fn create_container_minimal_mode_mounts_fuse(runtime: tokio::runtime::Runtime) {
        let creator = MockCreator::succeed_with("container-id");
        let request = CreateContainerRequest::new(
            Some(String::from("ghcr.io/example/sandbox:latest")),
            ContainerSecurityOptions::default(),
        );

        let result = runtime.block_on(EngineConnector::create_container_async(&creator, &request));

        assert!(result.is_ok(), "container creation should succeed");
        let body = creator
            .take_body()
            .expect("container body should be captured");
        let host_config = body.host_config.expect("host config should be set");

        assert_eq!(host_config.privileged, Some(false));
        assert_eq!(host_config.cap_add, Some(vec![String::from("SYS_ADMIN")]));
        assert_eq!(
            host_config.security_opt,
            Some(vec![String::from("label=disable")])
        );

        let devices = host_config
            .devices
            .expect("/dev/fuse device should be mounted");
        assert_eq!(devices.len(), 1);
        let device = devices
            .first()
            .expect("`/dev/fuse` mapping should include one device");
        assert_eq!(device.path_on_host.as_deref(), Some("/dev/fuse"));
        assert_eq!(device.path_in_container.as_deref(), Some("/dev/fuse"));
        assert_eq!(device.cgroup_permissions.as_deref(), Some("rwm"));
    }

    #[rstest]
    fn create_container_minimal_without_fuse_avoids_mount(runtime: tokio::runtime::Runtime) {
        let creator = MockCreator::succeed_with("container-id");
        let request = CreateContainerRequest::new(
            Some(String::from("ghcr.io/example/sandbox:latest")),
            ContainerSecurityOptions {
                privileged: false,
                mount_dev_fuse: false,
                selinux_label_mode: SelinuxLabelMode::DisableForContainer,
            },
        );

        let result = runtime.block_on(EngineConnector::create_container_async(&creator, &request));

        assert!(result.is_ok(), "container creation should succeed");
        let body = creator
            .take_body()
            .expect("container body should be captured");
        let host_config = body.host_config.expect("host config should be set");

        assert_eq!(host_config.privileged, Some(false));
        assert!(host_config.cap_add.is_none());
        assert!(host_config.devices.is_none());
        assert_eq!(
            host_config.security_opt,
            Some(vec![String::from("label=disable")])
        );
    }

    #[rstest]
    fn create_container_requires_image(runtime: tokio::runtime::Runtime) {
        let creator = MockCreator::succeed_with("container-id");
        let request = CreateContainerRequest::new(None, ContainerSecurityOptions::default());

        let result = runtime.block_on(EngineConnector::create_container_async(&creator, &request));

        assert!(
            matches!(
                result,
                Err(PodbotError::Config(ConfigError::MissingRequired { ref field }))
                    if field == "image"
            ),
            "expected missing image validation error, got: {result:?}"
        );
        assert_eq!(creator.call_count(), 0);
    }

    #[rstest]
    fn create_container_maps_engine_error(runtime: tokio::runtime::Runtime) {
        let creator = MockCreator::fail_with(bollard::errors::Error::RequestTimeoutError);
        let request = CreateContainerRequest::new(
            Some(String::from("ghcr.io/example/sandbox:latest")),
            ContainerSecurityOptions::default(),
        );

        let result = runtime.block_on(EngineConnector::create_container_async(&creator, &request));

        assert!(
            matches!(
                result,
                Err(PodbotError::Container(ContainerError::CreateFailed { ref message }))
                    if message.contains("Timeout error")
            ),
            "expected create-failed mapping, got: {result:?}"
        );
    }
}
