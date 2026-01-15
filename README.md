# podbot

> A sandboxed execution environment for AI coding agents

Podbot creates secure, isolated containers where AI coding assistants (like
Claude Code and Codex) can work on your repositories without direct access to
your host system. Think of it as a secure playpen for AI agents—they get the
tools they need, but can't accidentally (or intentionally) mess with your
actual machine.

> **⚠️ Work in Progress**
> Podbot is in early development. Core foundation is complete, but container
> orchestration and GitHub integration are still being built. See the
> [roadmap](docs/podbot-roadmap.md) for current status.

## Why podbot?

AI coding agents are powerful, but they need access to your code, containers,
and credentials. Podbot provides:

- **Security by design**: The host container socket stays with the trusted Rust
  CLI—agents never get direct access
- **Container-in-container support**: Agents can run nested Podman instances
  safely isolated from the host
- **Smart credential management**: GitHub tokens refresh automatically via a
  secure daemon; no static credentials in containers
- **Easy isolation**: Simple CLI to spin up and tear down agent sessions
  without complex setup

Podbot will ultimately serve as the agent encapsulation adapter for
[corbusier](https://github.com/leynos/corbusier).

## What's implemented?

### ✅ Foundation (Phase 1)

- Async runtime with Tokio
- Semantic error handling (Config, Container, GitHub, Filesystem errors)
- Layered configuration system (CLI → env vars → config file → defaults)
- CLI scaffolding with subcommands: `run`, `token-daemon`, `ps`, `stop`, `exec`

### 🚧 Coming soon

- Container orchestration with Bollard
- GitHub App authentication and token management
- Repository cloning and agent startup
- Container image with bundled agents

See [docs/podbot-roadmap.md](docs/podbot-roadmap.md) for the complete
implementation plan.

## Quick start

### Installation

```bash
git clone https://github.com/leynos/podbot
cd podbot
cargo install --path .
```

### Basic usage

Once the core features are complete, running an agent will look like this:

```bash
# Run Claude Code on a specific branch
podbot run --repo owner/name --branch feature-branch --agent claude
```

For detailed configuration options, see the [user's guide](docs/users-guide.md).

## How it works

1. **You run** `podbot run` with a repository and branch
2. **Podbot creates** a sandbox container with nested Podman support
3. **GitHub tokens** are managed by a background daemon (no static creds!)
4. **The agent starts** with access to the repo but isolated from your host
5. **You interact** with the agent normally—it just runs in a safe container
6. **Cleanup happens** automatically when you're done

The host Rust CLI maintains control of the actual Docker/Podman socket. The
agent gets a nested container environment where it can do its work without
elevated privileges.

## Documentation

- **[User's Guide](docs/users-guide.md)** - Installation, CLI reference,
  configuration, security model
- **[Roadmap](docs/podbot-roadmap.md)** - Detailed implementation status and
  upcoming features
- **[Design Document](docs/podbot-design.md)** - Architecture, security model,
  token management

## Technology stack

- **Rust 1.88+** with Edition 2024
- **Tokio** for async runtime
- **Bollard** for Docker/Podman API access
  - Primary target: Podman with nested container support
  - Docker support via compatible API (untested)
- **Octocrab** for GitHub App integration
- **OrthoConfig** for layered configuration
- **Cap-std** for capability-based filesystem access

## Project status

Podbot is actively developed and currently completing Phase 1 (Foundation). The
error handling, configuration system, and CLI structure are in place. Container
orchestration and GitHub integration are next up.

Watch this space, check the [roadmap](docs/podbot-roadmap.md), or star the repo
to follow along!

## Credits

Developed by **[df12 Productions](https://df12.studio)**

## Licence

ISC Licence - see [LICENSE](LICENSE) for details.
