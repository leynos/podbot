//! Container command execution orchestration.
//!
//! This module provides the stable library-facing exec orchestration function
//! that accepts Podbot-owned request types and returns a typed command
//! outcome. Engine connection, runtime creation, and low-level exec plumbing
//! remain internal details behind this facade.

use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::engine::{ContainerExecClient, EngineConnector, SocketResolver};
use crate::error::{ContainerError, PodbotError, Result as PodbotResult};

use super::CommandOutcome;

/// Stable execution mode for container commands.
///
/// The attached and detached modes map directly to the operator-facing CLI
/// behaviour. Protocol mode keeps streams attached while disabling TTY so
/// byte-oriented hosted protocols remain unmodified.
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
///
/// `ExecRequest` intentionally contains only Podbot-owned data. Library
/// embedders do not need to import engine traits, runtime handles, or CLI
/// adapter types to invoke [`exec`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecRequest {
    /// Target container identifier or name.
    pub container: String,
    /// Command argv to execute.
    pub command: Vec<String>,
    /// Requested execution mode.
    pub mode: ExecMode,
    /// Whether to allocate a pseudo-terminal.
    ///
    /// This flag is honoured only in attached mode. Detached and protocol
    /// execution always disable TTY allocation.
    pub tty: bool,
}

impl ExecRequest {
    /// Build a new exec request with attached mode and no TTY by default.
    ///
    /// # Errors
    ///
    /// Returns `PodbotError::Config` when the container identifier is blank or
    /// the command is empty.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use podbot::api::ExecRequest;
    ///
    /// let request = ExecRequest::new("sandbox", vec![String::from("echo")])?;
    /// assert_eq!(request.container, "sandbox");
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

    /// Return a copy of the request with a different execution mode.
    #[must_use]
    pub const fn with_mode(mut self, mode: ExecMode) -> Self {
        self.mode = mode;
        self
    }

    /// Return a copy of the request with an updated TTY preference.
    #[must_use]
    pub const fn with_tty(mut self, tty: bool) -> Self {
        self.tty = tty;
        self
    }

    fn validate(&self) -> Result<(), PodbotError> {
        let container = self.container.trim();
        if container.is_empty() {
            return Err(PodbotError::from(
                crate::error::ConfigError::MissingRequired {
                    field: String::from("container"),
                },
            ));
        }
        if self.command.is_empty() {
            return Err(PodbotError::from(
                crate::error::ConfigError::MissingRequired {
                    field: String::from("command"),
                },
            ));
        }
        if self
            .command
            .first()
            .is_some_and(|executable| executable.trim().is_empty())
        {
            return Err(PodbotError::from(
                crate::error::ConfigError::MissingRequired {
                    field: String::from("command[0]"),
                },
            ));
        }
        Ok(())
    }
}

/// Hidden compatibility seam for tests and internal callers that already have
/// a connected engine client.
#[doc(hidden)]
pub struct ExecParams<'a, C: ContainerExecClient> {
    /// Pre-connected container engine client.
    pub connector: &'a C,
    /// Stable exec request.
    pub request: &'a ExecRequest,
    /// Tokio runtime handle for blocking execution.
    pub runtime_handle: &'a tokio::runtime::Handle,
}

/// Execute a command in a running container.
///
/// Builds an engine request from the stable [`ExecRequest`], resolves the
/// engine socket from `config`, and maps the exit code to a
/// [`CommandOutcome`].
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
    request.validate()?;
    let runtime = create_runtime()?;
    let env = mockable::DefaultEnv::new();
    let resolver = SocketResolver::new(&env);
    let docker =
        EngineConnector::connect_with_fallback(config.engine_socket.as_deref(), &resolver)?;
    let params = ExecParams {
        connector: &docker,
        request,
        runtime_handle: runtime.handle(),
    };
    exec_with_client(&params)
}

/// Execute a command using a pre-connected engine client.
#[doc(hidden)]
pub fn exec_with_client<C: ContainerExecClient>(
    params: &ExecParams<'_, C>,
) -> PodbotResult<CommandOutcome> {
    let ExecParams {
        connector,
        request,
        runtime_handle,
    } = *params;

    request.validate()?;
    let engine_request = crate::engine::ExecRequest::new(
        &request.container,
        request.command.clone(),
        request.mode.into(),
    )?
    .with_tty(request.tty);
    let exec_result = EngineConnector::exec(runtime_handle, connector, &engine_request)?;

    if exec_result.exit_code() == 0 {
        Ok(CommandOutcome::Success)
    } else {
        Ok(CommandOutcome::CommandExit {
            code: exec_result.exit_code(),
        })
    }
}

fn create_runtime() -> PodbotResult<tokio::runtime::Runtime> {
    tokio::runtime::Runtime::new().map_err(|error| {
        PodbotError::from(ContainerError::RuntimeCreationFailed {
            message: error.to_string(),
        })
    })
}
