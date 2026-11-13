//! Container-creation request values.
//!
//! [`CreateContainerRequest`] collects the image, security profile, labels,
//! bind mounts, and working directory needed to create a sandbox container.
//! The type validates inputs at construction so the create path can treat a
//! request as a trusted value object.

use camino::{Utf8Path, Utf8PathBuf};

use crate::config::AppConfig;
use crate::error::{ConfigError, PodbotError};

use super::ContainerSecurityOptions;

/// A bind mount from a host path into a container.
///
/// Both paths must be absolute. Read-only mounts are used for secrets that the
/// container should observe but never modify.
///
/// # Examples
///
/// ```
/// use podbot::engine::BindMount;
///
/// let mount = BindMount::read_only("/run/user/1000/podbot/session", "/run/secrets/podbot")?;
/// assert!(mount.is_read_only());
/// # Ok::<(), podbot::error::PodbotError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindMount {
    source: Utf8PathBuf,
    target: Utf8PathBuf,
    read_only: bool,
}

impl BindMount {
    /// Create a bind mount from absolute host and container paths.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::InvalidValue` when either path is empty or
    /// relative.
    pub fn new(
        source: impl Into<Utf8PathBuf>,
        target: impl Into<Utf8PathBuf>,
        read_only: bool,
    ) -> Result<Self, PodbotError> {
        let validated_source = validate_absolute_path(&source.into(), "mount.source")?;
        let validated_target = validate_absolute_path(&target.into(), "mount.target")?;

        Ok(Self {
            source: validated_source,
            target: validated_target,
            read_only,
        })
    }

    /// Create a read-only bind mount.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::InvalidValue` when either path is empty or
    /// relative.
    pub fn read_only(
        source: impl Into<Utf8PathBuf>,
        target: impl Into<Utf8PathBuf>,
    ) -> Result<Self, PodbotError> {
        Self::new(source, target, true)
    }

    /// Create a read-write bind mount.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::InvalidValue` when either path is empty or
    /// relative.
    pub fn read_write(
        source: impl Into<Utf8PathBuf>,
        target: impl Into<Utf8PathBuf>,
    ) -> Result<Self, PodbotError> {
        Self::new(source, target, false)
    }

    /// Return the absolute host source path.
    #[must_use]
    pub const fn source(&self) -> &Utf8PathBuf {
        &self.source
    }

    /// Return the absolute container target path.
    #[must_use]
    pub const fn target(&self) -> &Utf8PathBuf {
        &self.target
    }

    /// Return whether the mount is read-only.
    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }
}

/// Container-creation request parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateContainerRequest {
    /// The container image to create from.
    image: String,

    /// Optional container name.
    name: Option<String>,

    /// Optional command to run in the container.
    cmd: Option<Vec<String>>,

    /// Optional environment variables in `KEY=value` form.
    env: Option<Vec<String>>,

    /// Ordered container labels.
    labels: Vec<(String, String)>,

    /// Bind mounts applied to the container.
    mounts: Vec<BindMount>,

    /// Optional working directory inside the container.
    working_dir: Option<String>,

    /// Security profile to apply.
    security: ContainerSecurityOptions,
}

impl CreateContainerRequest {
    /// Create a request with image and security settings.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingRequired` when `image` is empty or
    /// whitespace-only.
    pub fn new(
        image: impl Into<String>,
        security: ContainerSecurityOptions,
    ) -> Result<Self, PodbotError> {
        let image_value = image.into();
        let validated_image = String::from(validate_image(&image_value)?);

        Ok(Self {
            image: validated_image,
            name: None,
            cmd: None,
            env: None,
            labels: Vec::new(),
            mounts: Vec::new(),
            working_dir: None,
            security,
        })
    }

    /// Create a request from a resolved application configuration.
    ///
    /// This helper sources the image from `config.image` and security options
    /// from `config.sandbox`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use podbot::config::AppConfig;
    /// use podbot::engine::CreateContainerRequest;
    ///
    /// let mut config = AppConfig::default();
    /// config.image = Some(String::from("ghcr.io/example/sandbox:latest"));
    ///
    /// let request = CreateContainerRequest::from_app_config(&config);
    /// assert!(request.is_ok());
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingRequired` when `config.image` is missing,
    /// empty, or whitespace-only.
    pub fn from_app_config(config: &AppConfig) -> Result<Self, PodbotError> {
        let security = ContainerSecurityOptions::from_sandbox_config(&config.sandbox);
        Self::new(config.image.as_deref().unwrap_or_default(), security)
    }

    /// Attach an optional container name.
    #[must_use]
    pub fn with_name(mut self, name: Option<String>) -> Self {
        self.name = name.filter(|value| !value.trim().is_empty());
        self
    }

    /// Attach an optional command vector.
    #[must_use]
    pub fn with_cmd(mut self, cmd: Option<Vec<String>>) -> Self {
        self.cmd = cmd;
        self
    }

    /// Attach optional environment entries.
    #[must_use]
    pub fn with_env(mut self, env: Option<Vec<String>>) -> Self {
        self.env = env;
        self
    }

    /// Attach ordered container labels.
    #[must_use]
    pub fn with_labels(mut self, labels: Vec<(String, String)>) -> Self {
        self.labels = labels;
        self
    }

    /// Attach bind mounts.
    #[must_use]
    pub fn with_mounts(mut self, mounts: Vec<BindMount>) -> Self {
        self.mounts = mounts;
        self
    }

    /// Attach an absolute working directory.
    #[must_use]
    pub fn with_working_dir(mut self, working_dir: Option<String>) -> Self {
        self.working_dir = working_dir.filter(|value| !value.trim().is_empty());
        self
    }

    /// Return the configured image.
    #[must_use]
    pub fn image(&self) -> &str {
        &self.image
    }

    /// Return the optional configured name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Return the optional configured command.
    #[must_use]
    pub fn cmd(&self) -> Option<&[String]> {
        self.cmd.as_deref()
    }

    /// Return the optional configured environment list.
    #[must_use]
    pub fn env(&self) -> Option<&[String]> {
        self.env.as_deref()
    }

    /// Return the configured labels in insertion order.
    #[must_use]
    pub fn labels(&self) -> &[(String, String)] {
        &self.labels
    }

    /// Return the configured bind mounts.
    #[must_use]
    pub fn mounts(&self) -> &[BindMount] {
        &self.mounts
    }

    /// Return the optional working directory.
    #[must_use]
    pub fn working_dir(&self) -> Option<&str> {
        self.working_dir.as_deref()
    }

    /// Return the configured security options.
    #[must_use]
    pub const fn security(&self) -> &ContainerSecurityOptions {
        &self.security
    }
}

/// Reject a blank image; `CreateContainerRequest::new` returns `MissingRequired`.
pub(super) fn validate_image(image: &str) -> Result<&str, PodbotError> {
    let trimmed = image.trim();

    if trimmed.is_empty() {
        return Err(PodbotError::from(ConfigError::MissingRequired {
            field: String::from("image"),
        }));
    }

    Ok(trimmed)
}

/// Require bind-mount paths to be absolute.
fn validate_absolute_path(
    path: &Utf8Path,
    field: &'static str,
) -> Result<Utf8PathBuf, PodbotError> {
    let trimmed = path.as_str().trim();

    if trimmed.is_empty() || !trimmed.starts_with('/') {
        return Err(ConfigError::InvalidValue {
            field: String::from(field),
            reason: String::from("expected an absolute path"),
        }
        .into());
    }

    Ok(Utf8PathBuf::from(trimmed))
}
