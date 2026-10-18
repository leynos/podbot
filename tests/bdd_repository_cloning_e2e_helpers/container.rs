//! Container-fixture wiring for end-to-end repository-cloning scenarios.
//!
//! Starts an alpine/git container via testcontainers, populates a local bare
//! git server, and exposes a Bollard `Docker` handle that points at the same
//! socket. The container is kept alive for the duration of the scenario by the
//! shared scenario state. Normal teardown uses [`SandboxBundle::finish`] and
//! verifies same-ID absence; its `Drop` implementation provides fallback cleanup.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use bollard::API_DEFAULT_VERSION;
use bollard::Docker;
use testcontainers::core::WaitFor;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::runtime::Runtime;

use super::state::{SandboxBundle, StepResult};

const ASKPASS_PATH: &str = "/usr/local/bin/git-askpass";
const WORKSPACE_PATH: &str = "/work";

const SANDBOX_IMAGE: &str = "alpine/git";
const SANDBOX_TAG: &str = "v2.45.2";

const READY_MARKER: &str = "PODBOT_E2E_READY";

const CONNECTION_TIMEOUT_SECS: u64 = 30;

/// Shell script that provisions the in-container git server and the
/// `git-askpass` helper before signalling readiness. Kept in an external
/// fixture (rather than inline `concat!`) so it can be linted with
/// `shellcheck` and edited as ordinary shell.
const SETUP_SCRIPT: &str = include_str!("../fixtures/e2e_sandbox_setup.sh");

/// Selected container socket used by both testcontainers and the Bollard
/// client created for the scenario.
struct DockerHostSocket {
    /// Endpoint in `unix:///path` form (or other Docker host URL).
    endpoint: String,
}

/// Pick a usable Docker-compatible socket, preferring an existing `DOCKER_HOST`
/// or system socket and falling back to the rootless Podman socket under
/// `$XDG_RUNTIME_DIR`. The result is cached for the lifetime of the test
/// binary so concurrent scenarios share one detection pass.
fn resolve_docker_host_endpoint() -> StepResult<String> {
    static CACHED: OnceLock<Result<String, String>> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            if let Ok(existing) = std::env::var("DOCKER_HOST")
                && !existing.is_empty()
            {
                return Ok(existing);
            }
            detect_local_socket().ok_or_else(|| {
                String::from(
                    "no Docker- or Podman-compatible socket found; \
                 set DOCKER_HOST to use the e2e tests",
                )
            })
        })
        .clone()
}

/// Shared scoped environment boundary for synchronous test operations.
#[path = "../test_support/environment.rs"]
mod environment;

fn detect_local_socket() -> Option<String> {
    candidate_socket_paths()
        .into_iter()
        .find(|path| path.exists())
        .map(|path| format!("unix://{}", path.display()))
}

fn candidate_socket_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("/var/run/docker.sock")];
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR")
        && !dir.is_empty()
    {
        paths.push(PathBuf::from(dir).join("podman/podman.sock"));
    }
    paths
}

/// Build a Bollard client pointed at the same socket testcontainers uses.
fn connect_bollard(socket: &DockerHostSocket) -> StepResult<Docker> {
    if let Some(socket_path) = socket.endpoint.strip_prefix("unix://") {
        return Docker::connect_with_socket(
            socket_path,
            CONNECTION_TIMEOUT_SECS,
            API_DEFAULT_VERSION,
        )
        .map_err(|err| format!("connect_with_socket failed: {err}"));
    }
    Docker::connect_with_http(
        &socket.endpoint,
        CONNECTION_TIMEOUT_SECS,
        API_DEFAULT_VERSION,
    )
    .map_err(|err| format!("connect_with_http failed: {err}"))
}

/// Start the sandbox container, populate the in-container git server, and
/// return the lifecycle handles bundled together.
pub fn launch_sandbox_bundle() -> StepResult<SandboxBundle> {
    let socket = DockerHostSocket {
        endpoint: resolve_docker_host_endpoint()?,
    };
    let runtime =
        Arc::new(Runtime::new().map_err(|err| format!("failed to create tokio runtime: {err}"))?);
    let docker = Arc::new(connect_bollard(&socket)?);

    // testcontainers has no host-injection API. Scope its lazy client creation
    // and startup together, including all awaits, then restore the host value.
    // The returned container retains the client and its parsed configuration.
    let container = environment::with_test_environment("DOCKER_HOST", &socket.endpoint, || {
        runtime.block_on(start_sandbox_container())
    })
    .map_err(|err| format!("failed to start sandbox container: {err}"))?;
    let container_id = container.id().to_owned();

    let bundle = SandboxBundle {
        runtime,
        container: Some(container),
        docker,
        container_id,
    };
    bundle.record_created()?;
    Ok(bundle)
}

async fn start_sandbox_container() -> StepResult<ContainerAsync<GenericImage>> {
    GenericImage::new(SANDBOX_IMAGE, SANDBOX_TAG)
        .with_entrypoint("sh")
        .with_wait_for(WaitFor::message_on_stdout(READY_MARKER))
        .with_label("org.podbot.e2e.owner-pid", std::process::id().to_string())
        .with_cmd(["-c", SETUP_SCRIPT])
        .start()
        .await
        .map_err(|err| format!("testcontainers start: {err}"))
}

/// Path to the `GIT_ASKPASS` helper inside the sandbox container.
#[must_use]
pub const fn askpass_path() -> &'static str {
    ASKPASS_PATH
}

/// Workspace clone destination inside the sandbox container.
#[must_use]
pub const fn workspace_path() -> &'static str {
    WORKSPACE_PATH
}

#[cfg(test)]
mod tests {
    //! Live-engine proof that scoped startup retains the configured client.

    use super::launch_sandbox_bundle;

    #[test]
    fn container_client_remains_usable_after_host_environment_is_restored() {
        temp_env::with_vars::<&str, &str, _, _>([], || {
            let original_host = std::env::var_os("DOCKER_HOST");
            let bundle = launch_sandbox_bundle().expect("sandbox container should start");
            assert_eq!(
                std::env::var_os("DOCKER_HOST"),
                original_host,
                "startup must restore the original Docker host"
            );
            let container = bundle
                .container
                .as_ref()
                .expect("bundle should retain its container");
            let running = bundle
                .runtime
                .block_on(container.is_running())
                .expect("retained testcontainers client should inspect the container");
            assert!(
                running,
                "sandbox should remain running after environment restoration"
            );
            bundle
                .finish()
                .expect("owned sandbox removal and absence should be confirmed");
            assert_eq!(
                std::env::var_os("DOCKER_HOST"),
                original_host,
                "teardown must leave the original Docker host unchanged"
            );
        });
    }
}
