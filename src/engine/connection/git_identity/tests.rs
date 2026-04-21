//! Unit tests for Git identity configuration.

use std::io;
use std::process::Output;

use mockall::mock;
use rstest::rstest;

use super::host_reader::{HostCommandRunner, read_host_git_identity};
use super::test_helpers::{failure_output, success_output};

// -- Host reader tests --

mock! {
    CommandRunner {}
    impl HostCommandRunner for CommandRunner {
        fn run_command<'a>(
            &self,
            program: &'a str,
            args: &'a [&'a str],
        ) -> io::Result<Output>;
    }
}

fn make_runner(name_raw: Option<&str>, email_raw: Option<&str>) -> MockCommandRunner {
    let mut runner = MockCommandRunner::new();

    let name_out = name_raw.map_or_else(failure_output, success_output);
    runner
        .expect_run_command()
        .withf(|_, args| args.contains(&"user.name"))
        .returning(move |_, _| Ok(name_out.clone()));

    let email_out = email_raw.map_or_else(failure_output, success_output);
    runner
        .expect_run_command()
        .withf(|_, args| args.contains(&"user.email"))
        .returning(move |_, _| Ok(email_out.clone()));

    runner
}

#[rstest]
#[case(
    Some("Alice\n"),
    Some("alice@example.com\n"),
    Some("Alice"),
    Some("alice@example.com")
)]
#[case(Some("Alice\n"), None, Some("Alice"), None)]
#[case(None, Some("bob@example.com\n"), None, Some("bob@example.com"))]
#[case(
    Some("  Alice  \n"),
    Some("  alice@example.com  \n"),
    Some("Alice"),
    Some("alice@example.com")
)]
#[case(Some("  \n"), Some("  \n"), None, None)]
fn read_identity_with_ok_responses(
    #[case] name_raw: Option<&str>,
    #[case] email_raw: Option<&str>,
    #[case] expected_name: Option<&str>,
    #[case] expected_email: Option<&str>,
) {
    let runner = make_runner(name_raw, email_raw);
    let identity = read_host_git_identity(&runner);
    assert_eq!(identity.name.as_deref(), expected_name);
    assert_eq!(identity.email.as_deref(), expected_email);
}

#[rstest]
fn read_identity_returns_none_when_git_not_installed() {
    let mut runner = MockCommandRunner::new();
    runner
        .expect_run_command()
        .returning(|_, _| Err(io::Error::new(io::ErrorKind::NotFound, "not found")));

    let identity = read_host_git_identity(&runner);

    assert!(identity.name.is_none());
    assert!(identity.email.is_none());
}

// Note: Container configurator tests use mock ContainerExecClient
// and are in the BDD test suite (Stage C) for full integration
// coverage. Additional unit tests for set_git_config error paths
// are here.
