//! Unit tests for container exec lifecycle handling.

use super::terminal::TerminalSize;
use super::*;
use crate::error::ContainerError;
use rstest::rstest;
use serial_test::serial;
mod attached_tests;
mod detached_helpers;
mod detached_tests;
mod error_tests;
mod helpers;
mod lifecycle_helpers;
mod protocol_helpers;
mod protocol_proxy_bdd;
mod proxy_helpers;
mod start_options_tests;
mod validation_tests;
pub(super) use helpers::*;
