//! Unit tests for podbot configuration types.
//!
//! This module contains tests organised into:
//! - [`helpers`] - Shared fixtures and helper functions
//! - [`types_tests`] - Basic type and serialisation tests
//! - [`validation`] - `GitHubConfig` validation tests
//! - [`layer_precedence_tests`] - `MergeComposer` layer precedence tests

mod helpers;
mod layer_precedence_tests;
mod types_tests;
mod validation;
