//! Helpers for the end-to-end repository-cloning behavioural test.
//!
//! These scenarios drive `clone_repository_into_workspace` against a real
//! Bollard exec client connected to a container started by `testcontainers`.

pub mod assertions;
pub mod container;
pub mod state;
pub mod steps;

pub use state::{RepositoryCloningE2eState, repository_cloning_e2e_state};
