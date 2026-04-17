//! Shared helpers for exec request validation and Bollard option building.

use bollard::exec::{CreateExecOptions, StartExecOptions};

use super::{ExecRequest, exec_failed};
use crate::error::{ConfigError, PodbotError};

pub(super) fn build_create_exec_options(request: &ExecRequest) -> CreateExecOptions<String> {
    let attached = request.mode().is_attached();
    CreateExecOptions::<String> {
        attach_stdin: Some(attached),
        attach_stdout: Some(attached),
        attach_stderr: Some(attached),
        tty: Some(attached && request.tty()),
        env: request.env().map(<[String]>::to_vec),
        cmd: Some(request.command().to_vec()),
        ..CreateExecOptions::default()
    }
}

pub(super) const fn build_start_exec_options(request: &ExecRequest) -> StartExecOptions {
    let output_capacity = match request.mode() {
        super::ExecMode::Protocol => Some(super::PROTOCOL_OUTPUT_CAPACITY),
        super::ExecMode::Attached | super::ExecMode::Detached => None,
    };

    StartExecOptions {
        detach: !request.mode().is_attached(),
        tty: request.mode().is_attached() && request.tty(),
        output_capacity,
    }
}

pub(super) fn validate_command(command: Vec<String>) -> Result<Vec<String>, PodbotError> {
    if command.is_empty() {
        return Err(PodbotError::from(ConfigError::MissingRequired {
            field: String::from("command"),
        }));
    }

    let executable = command.first().map(String::as_str).unwrap_or_default();
    if executable.trim().is_empty() {
        return Err(PodbotError::from(ConfigError::InvalidValue {
            field: String::from("command"),
            reason: String::from("command executable must not be empty"),
        }));
    }

    Ok(command)
}

pub(super) fn validate_required_field<'a>(
    field: &str,
    value: &'a str,
) -> Result<&'a str, PodbotError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(PodbotError::from(ConfigError::MissingRequired {
            field: String::from(field),
        }));
    }

    Ok(trimmed)
}

pub(super) fn map_create_exec_error(
    container_id: &str,
    error: impl std::fmt::Display,
) -> PodbotError {
    exec_failed(container_id, format!("create exec failed: {error}"))
}

pub(super) fn map_start_exec_error(
    container_id: &str,
    error: impl std::fmt::Display,
) -> PodbotError {
    exec_failed(container_id, format!("start exec failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_command_rejects_empty_vec() {
        let err = validate_command(vec![]).expect_err("empty command should be rejected");
        assert!(
            matches!(
                err,
                crate::error::PodbotError::Config(
                    crate::error::ConfigError::MissingRequired { ref field }
                ) if field == "command"
            ),
            "unexpected error: {err:?}",
        );
    }

    #[test]
    fn validate_command_rejects_blank_executable() {
        let err = validate_command(vec![String::from("  ")])
            .expect_err("blank executable should be rejected");
        assert!(
            matches!(
                err,
                crate::error::PodbotError::Config(
                    crate::error::ConfigError::InvalidValue { ref field, .. }
                ) if field == "command"
            ),
            "unexpected error: {err:?}",
        );
    }

    #[test]
    fn validate_command_accepts_non_blank_executable() {
        let cmd = vec![String::from("echo"), String::from("hello")];
        let result = validate_command(cmd.clone()).expect("valid command should be accepted");
        assert_eq!(result, cmd);
    }

    #[test]
    fn validate_required_field_rejects_empty_string() {
        let err =
            validate_required_field("container", "").expect_err("empty field should be rejected");
        assert!(
            matches!(
                err,
                crate::error::PodbotError::Config(
                    crate::error::ConfigError::MissingRequired { ref field }
                ) if field == "container"
            ),
            "unexpected error: {err:?}",
        );
    }

    #[test]
    fn validate_required_field_rejects_whitespace_only() {
        let err = validate_required_field("container", "   ")
            .expect_err("whitespace-only field should be rejected");
        assert!(
            matches!(
                err,
                crate::error::PodbotError::Config(
                    crate::error::ConfigError::MissingRequired { ref field }
                ) if field == "container"
            ),
            "unexpected error: {err:?}",
        );
    }

    #[test]
    fn validate_required_field_accepts_non_blank_value() {
        let result = validate_required_field("container", "  sandbox  ")
            .expect("valid field should be accepted");
        assert_eq!(result, "sandbox");
    }
}
