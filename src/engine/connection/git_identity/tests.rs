//! Unit tests for Git identity configuration.

use std::io;
use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output};

use mockall::mock;
use rstest::rstest;

use super::host_reader::{HostCommandRunner, read_host_git_identity};

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

fn success_output(stdout: &str) -> Output {
    Output {
        status: ExitStatus::from_raw(0),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
    }
}

fn failure_output() -> Output {
    Output {
        status: ExitStatus::from_raw(256), // exit code 1
        stdout: Vec::new(),
        stderr: b"error".to_vec(),
    }
}

#[rstest]
fn read_identity_returns_both_when_configured() {
    let mut runner = MockCommandRunner::new();
    runner
        .expect_run_command()
        .withf(|prog, args| prog == "git" && args == ["config", "--get", "user.name"])
        .returning(|_, _| Ok(success_output("Alice\n")));
    runner
        .expect_run_command()
        .withf(|prog, args| prog == "git" && args == ["config", "--get", "user.email"])
        .returning(|_, _| Ok(success_output("alice@example.com\n")));

    let identity = read_host_git_identity(&runner);

    assert_eq!(identity.name.as_deref(), Some("Alice"));
    assert_eq!(identity.email.as_deref(), Some("alice@example.com"));
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

#[rstest]
fn read_identity_returns_none_for_unconfigured_fields() {
    let mut runner = MockCommandRunner::new();
    runner
        .expect_run_command()
        .withf(|_, args| args.contains(&"user.name"))
        .returning(|_, _| Ok(failure_output()));
    runner
        .expect_run_command()
        .withf(|_, args| args.contains(&"user.email"))
        .returning(|_, _| Ok(success_output("bob@example.com\n")));

    let identity = read_host_git_identity(&runner);

    assert!(identity.name.is_none());
    assert_eq!(identity.email.as_deref(), Some("bob@example.com"));
}

#[rstest]
fn read_identity_trims_whitespace() {
    let mut runner = MockCommandRunner::new();
    runner
        .expect_run_command()
        .withf(|_, args| args.contains(&"user.name"))
        .returning(|_, _| Ok(success_output("  Alice  \n")));
    runner
        .expect_run_command()
        .withf(|_, args| args.contains(&"user.email"))
        .returning(|_, _| Ok(success_output("  alice@example.com  \n")));

    let identity = read_host_git_identity(&runner);

    assert_eq!(identity.name.as_deref(), Some("Alice"));
    assert_eq!(identity.email.as_deref(), Some("alice@example.com"));
}

#[rstest]
fn read_identity_returns_none_for_empty_output() {
    let mut runner = MockCommandRunner::new();
    runner
        .expect_run_command()
        .returning(|_, _| Ok(success_output("  \n")));

    let identity = read_host_git_identity(&runner);

    assert!(identity.name.is_none());
    assert!(identity.email.is_none());
}

// Note: Container configurator tests use mock ContainerExecClient
// and are in the BDD test suite (Stage C) for full integration
// coverage. Additional unit tests for set_git_config error paths
// are here.
