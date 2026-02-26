//! Sandboxed execution environment for AI coding agents.
//!
//! `podbot` provides a secure container-based sandbox for running AI coding agents
//! such as Claude Code and Codex. The design prioritises security by treating the
//! host container engine as high-trust infrastructure, while the agent container
//! operates in a low-trust playpen with no access to the host socket.
//!
//! # Architecture
//!
//! The core principle is straightforward: the Rust CLI acts as the sole holder of
//! the host Podman or Docker socket. The agent container never receives access to
//! this socket. Instead, the agent runs an inner Podman service for any nested
//! container operations.
//!
//! # Modules
//!
//! - [`config`]: Configuration system with layered precedence (CLI > env > file > defaults)
//! - [`engine`]: Container engine connection and management
//! - [`error`]: Semantic error types for the application
//! - [`github`]: GitHub App authentication (internal, subject to change)

pub mod config;
pub mod engine;
pub mod error;
pub mod github;
