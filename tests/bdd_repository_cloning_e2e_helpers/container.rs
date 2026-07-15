//! Container-fixture wiring for end-to-end repository-cloning scenarios.
//!
//! Starts an alpine/git container via testcontainers, populates a local bare
//! git server, and exposes a Bollard `Docker` handle that points at the same
//! socket. The container is kept alive for the duration of the scenario by the
//! shared scenario state, and is torn down explicitly through the
//! [`SandboxBundle`] `Drop` implementation when the scenario completes.

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

const SETUP_SCRIPT: &str = concat!(
    "set -eu\n",
    "mkdir -p /root /srv/test-repos/leynos/podbot.git\n",
    "cat > /root/.gitconfig <<'GITCFG'\n",
    "[user]\n",
    "    name = Test\n",
    "    email = test@example.com\n",
    "[init]\n",
    "    defaultBranch = main\n",
    "[url \"file:///srv/test-repos/\"]\n",
    "    insteadOf = https://github.com/\n",
    "GITCFG\n",
    "git init --bare -b main /srv/test-repos/leynos/podbot.git >/dev/null 2>&1\n",
    "work=$(mktemp -d)\n",
    "git -C \"$work\" init -b main >/dev/null 2>&1\n",
    "echo hello > \"$work\"/README.md\n",
    "git -C \"$work\" add README.md >/dev/null 2>&1\n",
    "git -C \"$work\" commit -m init >/dev/null 2>&1\n",
    "git -C \"$work\" push /srv/test-repos/leynos/podbot.git main:main >/dev/null 2>&1\n",
    "cat > /usr/local/bin/git-askpass <<'ASKPASS'\n",
    "#!/bin/sh\n",
    "echo \"\"\n",
    "ASKPASS\n",
    "chmod +x /usr/local/bin/git-askpass\n",
    "echo ",
    "PODBOT_E2E_READY",
    "\n",
    "exec sleep infinity\n",
);

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

/// Ensure `DOCKER_HOST` reflects the resolved socket so `testcontainers` can
/// pick it up at client construction time.
///
/// `testcontainers` 0.27 has no programmatic API for injecting a Docker host;
/// it reads `DOCKER_HOST` (or `tc.host` from `~/.testcontainers.properties`)
/// during client init. When the surrounding test environment has not already
/// set `DOCKER_HOST` but a Podman or Docker socket is present, this helper
/// mirrors the resolved endpoint into the process environment exactly once,
/// gated through a dedicated `OnceLock<()>` so the one-shot unsafe write is
/// serialized across concurrent test threads.
fn ensure_docker_host() -> StepResult<DockerHostSocket> {
    static ENV_INIT: OnceLock<()> = OnceLock::new();
    let endpoint = resolve_docker_host_endpoint()?;
    ENV_INIT.get_or_init(|| {
        let already_set = std::env::var("DOCKER_HOST")
            .map(|existing| !existing.is_empty())
            .unwrap_or(false);
        if !already_set {
            // SAFETY: The surrounding `OnceLock::get_or_init` guarantees that
            // this branch executes exactly once per process, serialized
            // against concurrent test threads. The write is required because
            // `testcontainers` reads `DOCKER_HOST` during client construction
            // and exposes no in-process alternative.
            unsafe {
                std::env::set_var("DOCKER_HOST", &endpoint);
            }
        }
    });
    Ok(DockerHostSocket { endpoint })
}

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
    let socket = ensure_docker_host()?;
    let runtime =
        Arc::new(Runtime::new().map_err(|err| format!("failed to create tokio runtime: {err}"))?);
    let docker = Arc::new(connect_bollard(&socket)?);

    let container = runtime
        .block_on(async { start_sandbox_container().await })
        .map_err(|err| format!("failed to start sandbox container: {err}"))?;
    let container_id = container.id().to_owned();

    Ok(SandboxBundle {
        runtime,
        container: Some(container),
        docker,
        container_id,
    })
}

async fn start_sandbox_container() -> StepResult<ContainerAsync<GenericImage>> {
    GenericImage::new(SANDBOX_IMAGE, SANDBOX_TAG)
        .with_entrypoint("sh")
        .with_wait_for(WaitFor::message_on_stdout(READY_MARKER))
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
