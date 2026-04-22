//! Unit tests for container exec lifecycle handling.

use super::terminal::TerminalSize;
use super::*;
use crate::error::ContainerError;
use rstest::rstest;
use serial_test::serial;
mod detached_helpers;
mod helpers;
mod lifecycle_helpers;
mod protocol_helpers;
mod protocol_proxy_bdd;
mod proxy_helpers;
mod validation_tests;
pub(super) use helpers::*;

include!("tests/start_options_tests.rs");
include!("tests/detached_tests.rs");
include!("tests/error_tests.rs");
include!("tests/attached_tests.rs");
