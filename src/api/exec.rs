//! Container command execution orchestration.
//!
//! This module provides the stable library-facing exec orchestration API.
//! `ExecRequest` is validated at construction time and then treated as a
//! trusted value object. For simple callers, [`exec`] resolves a connection on
//! demand. Embedders that need to reuse a runtime handle and engine
//! connection can create an [`ExecContext`] and call [`ExecContext::exec`].

use bollard::Docker;
use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::engine::{ContainerExecClient, EngineConnector, SocketResolver};
use crate::error::{ConfigError, PodbotError, Result as PodbotResult};

use super::CommandOutcome;

/// Stable execution mode for container commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ExecMode {
    /// Attach local stdin/stdout/stderr to the exec process.
    Attached,
    /// Start without stream attachment and wait for exit.
    Detached,
    /// Attach streams but permanently disable TTY allocation.
    Protocol,
}

impl From<ExecMode> for crate::engine::ExecMode {
    fn from(value: ExecMode) -> Self {
        match value {
            ExecMode::Attached => Self::Attached,
            ExecMode::Detached => Self::Detached,
            ExecMode::Protocol => Self::Protocol,
        }
    }
}

/// Stable request type for executing a command in a running container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ExecRequestDef")]
pub struct ExecRequest {
    container: String,
    command: Vec<String>,
    mode: ExecMode,
    tty: bool,
}

#[derive(Debug, Deserialize)]
struct ExecRequestDef {
    container: String,
    command: Vec<String>,
    #[serde(default = "default_exec_mode")]
    mode: ExecMode,
    #[serde(default)]
    tty: bool,
}

impl ExecRequest {
    /// Build a new exec request with attached mode and no TTY by default.
    ///
    /// # Errors
    ///
    /// Returns `PodbotError::Config` when `validate()` rejects the
    /// request because:
    /// - the container identifier is blank
    /// - the command vector is empty
    /// - `command[0]` is blank
    ///
    /// # Examples
    ///
    /// ```rust
    /// use podbot::api::ExecRequest;
    ///
    /// let request = ExecRequest::new("sandbox", vec![String::from("echo")])?;
    /// assert_eq!(request.container(), "sandbox");
    /// # Ok::<(), podbot::error::PodbotError>(())
    /// ```
    pub fn new(container: impl Into<String>, command: Vec<String>) -> Result<Self, PodbotError> {
        let request = Self {
            container: container.into(),
            command,
            mode: ExecMode::Attached,
            tty: false,
        };
        request.validate()?;
        Ok(request)
    }

    /// Return the target container identifier.
    #[must_use]
    pub fn container(&self) -> &str {
        &self.container
    }

    /// Return the command argv.
    #[must_use]
    pub fn command(&self) -> &[String] {
        &self.command
    }

    /// Return the requested execution mode.
    #[must_use]
    pub const fn mode(&self) -> ExecMode {
        self.mode
    }

    /// Return whether TTY allocation was requested.
    #[must_use]
    pub const fn tty(&self) -> bool {
        self.tty
    }

    /// Return a copy of the request with a different execution mode.
    #[must_use]
    pub const fn with_mode(mut self, mode: ExecMode) -> Self {
        self.mode = mode;
        self.tty = normalized_tty(mode, self.tty);
        self
    }

    /// Return a copy of the request with an updated TTY preference.
    #[must_use]
    pub const fn with_tty(mut self, tty: bool) -> Self {
        self.tty = normalized_tty(self.mode, tty);
        self
    }

    fn validate(&self) -> Result<(), PodbotError> {
        if self.container.trim().is_empty() {
            return Err(PodbotError::from(ConfigError::MissingRequired {
                field: String::from("container"),
            }));
        }
        if self.command.is_empty() {
            return Err(PodbotError::from(ConfigError::MissingRequired {
                field: String::from("command"),
            }));
        }
        if self
            .command
            .first()
            .is_some_and(|executable| executable.trim().is_empty())
        {
            return Err(PodbotError::from(ConfigError::MissingRequired {
                field: String::from("command[0]"),
            }));
        }
        Ok(())
    }
}

impl TryFrom<ExecRequestDef> for ExecRequest {
    type Error = PodbotError;

    fn try_from(value: ExecRequestDef) -> Result<Self, Self::Error> {
        Ok(Self::new(value.container, value.command)?
            .with_mode(value.mode)
            .with_tty(value.tty))
    }
}

const fn default_exec_mode() -> ExecMode {
    ExecMode::Attached
}

const fn normalized_tty(mode: ExecMode, tty: bool) -> bool {
    matches!(mode, ExecMode::Attached) && tty
}

/// Reusable exec context for embedders that want to cache engine state.
pub struct ExecContext {
    connector: Docker,
    runtime_handle: tokio::runtime::Handle,
}

impl ExecContext {
    /// Resolve and connect an engine client using the supplied runtime handle.
    ///
    /// # Errors
    ///
    /// Returns the same connection errors as [`exec`].
    pub fn connect(
        config: &AppConfig,
        runtime_handle: &tokio::runtime::Handle,
    ) -> PodbotResult<Self> {
        let env = mockable::DefaultEnv::new();
        let resolver = SocketResolver::new(&env);
        let connector =
            EngineConnector::connect_with_fallback(config.engine_socket.as_deref(), &resolver)?;

        Ok(Self {
            connector,
            runtime_handle: runtime_handle.clone(),
        })
    }

    /// Execute a validated request using the cached connector and runtime.
    ///
    /// # Errors
    ///
    /// Returns the same engine execution errors as [`exec`].
    pub fn exec(&self, request: &ExecRequest) -> PodbotResult<CommandOutcome> {
        exec_with_client(&self.connector, &self.runtime_handle, request)
    }
}

/// Execute a command in a running container.
///
/// This convenience API creates a runtime and engine connection per call.
/// Embedders that need to reuse those resources should prefer [`ExecContext`].
///
/// # Errors
///
/// Returns `PodbotError` variants:
/// - `ContainerError::ExecFailed` if command execution fails.
/// - `ConfigError::MissingRequired` if required fields are empty.
///
/// # Examples
///
/// ```rust,no_run
/// use podbot::api::{ExecRequest, exec};
/// use podbot::config::AppConfig;
///
/// let config = AppConfig::default();
/// let request = ExecRequest::new("sandbox", vec![String::from("echo"), String::from("hello")])?;
/// let result = exec(&config, &request);
/// let _ = result;
/// # Ok::<(), podbot::error::PodbotError>(())
/// ```
pub fn exec(config: &AppConfig, request: &ExecRequest) -> PodbotResult<CommandOutcome> {
    let runtime = super::create_runtime()?;
    let context = ExecContext::connect(config, runtime.handle())?;
    context.exec(request)
}

/// Execute a command using a pre-connected engine client and runtime handle.
///
/// This helper is intended for advanced embedders and test harnesses that
/// already own a connector implementation and Tokio runtime. Callers that want
/// the simpler stable surface should prefer [`exec`] or [`ExecContext`].
///
/// # Errors
///
/// Returns the same engine execution and request-conversion errors as [`exec`].
pub(crate) fn exec_with_client<C: ContainerExecClient + Sync>(
    connector: &C,
    runtime_handle: &tokio::runtime::Handle,
    request: &ExecRequest,
) -> PodbotResult<CommandOutcome> {
    let engine_request = crate::engine::ExecRequest::new(
        request.container(),
        request.command().to_vec(),
        request.mode().into(),
    )?
    .with_tty(request.tty());
    let exec_result = EngineConnector::exec(runtime_handle, connector, &engine_request)?;

    if exec_result.exit_code() == 0 {
        Ok(CommandOutcome::Success)
    } else {
        Ok(CommandOutcome::CommandExit {
            code: exec_result.exit_code(),
        })
    }
}
