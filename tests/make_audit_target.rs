//! Integration tests for the Makefile `rust-audit` target.
//!
//! Validates that `make rust-audit`:
//! - invokes `cargo audit` exactly once at the workspace root derived from
//!   `cargo metadata`,
//! - propagates non-zero exit codes from both `cargo audit` and
//!   `cargo metadata` as a `make` failure,
//! - does not audit manifests found under `target/`, `node_modules/`, or
//!   `.venv/` unless they appear in workspace metadata.
//!
//! A fake `cargo` executable is generated at test time and records its
//! invocations to a log file so tests can assert on invocation count and
//! working directory without network access or a real RustSec database.
//!
//! See also: `Makefile` (`rust-audit` target), `docs/developers-guide.md`
//! (§ 2 Quality gates, § 2.1 Security audit ignores).

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use rstest::rstest;
use tempfile::TempDir;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn write_file(path: &Path, contents: &str) -> TestResult {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn write_fake_cargo(
    bin_dir: &Path,
    log_path: &Path,
    exit_status: i32,
    metadata_status: i32,
) -> TestResult<std::path::PathBuf> {
    let cargo_path = bin_dir.join("cargo");
    write_file(
        &cargo_path,
        &format!(
            "#!/usr/bin/env sh\nif [ \"$1\" = metadata ]; then\nprintf '%s\\n' \"$PODBOT_FAKE_CARGO_METADATA\"\nexit {metadata_status}\nfi\nprintf '%s|%s\\n' \"$PWD\" \"$*\" >> '{}'\nexit {exit_status}\n",
            log_path.display()
        ),
    )?;
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&cargo_path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&cargo_path, permissions)?;
    }
    Ok(cargo_path)
}

fn run_rust_audit(
    workspace: &Path,
    fake_cargo: &Path,
    metadata: &str,
) -> TestResult<std::process::Output> {
    Ok(Command::new("make")
        .arg("-f")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("Makefile"))
        .arg("rust-audit")
        .arg(format!("CARGO={}", fake_cargo.display()))
        .env("PODBOT_FAKE_CARGO_METADATA", metadata)
        .current_dir(workspace)
        .output()?)
}

fn create_manifest(path: &Path) -> TestResult {
    write_file(path, "[package]\nname = \"fixture\"\nversion = \"0.0.0\"\n")
}

fn cargo_metadata_for(workspace: &Path, manifests: &[&Path]) -> String {
    let packages = manifests
        .iter()
        .enumerate()
        .map(|(index, manifest)| {
            format!(
                r#"{{"id":"fixture {index}","manifest_path":"{}"}}"#,
                manifest.display()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let members = manifests
        .iter()
        .enumerate()
        .map(|(index, _manifest)| format!(r#""fixture {index}""#))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"workspace_root":"{}","packages":[{packages}],"workspace_members":[{members}]}}"#,
        workspace.display()
    )
}

#[test]
fn rust_audit_invokes_cargo_audit_once_at_workspace_root() {
    let temp = TempDir::new().expect("temporary workspace should be created");
    let workspace = temp.path();
    let root_manifest = workspace.join("Cargo.toml");
    let member_manifest = workspace.join("crates/agent/Cargo.toml");
    create_manifest(&root_manifest).expect("root manifest should be created");
    create_manifest(&member_manifest).expect("nested manifest should be created");
    create_manifest(&workspace.join("target/ignored/Cargo.toml"))
        .expect("target manifest fixture should be created");
    create_manifest(&workspace.join("node_modules/ignored/Cargo.toml"))
        .expect("node_modules manifest fixture should be created");
    create_manifest(&workspace.join(".venv/ignored/Cargo.toml"))
        .expect("virtualenv manifest fixture should be created");

    let log_path = workspace.join("cargo-audit.log");
    let fake_cargo = write_fake_cargo(&workspace.join("bin"), &log_path, 0, 0)
        .expect("fake cargo should be built");
    let metadata = cargo_metadata_for(workspace, &[&root_manifest, &member_manifest]);

    let output =
        run_rust_audit(workspace, &fake_cargo, &metadata).expect("make rust-audit should run");

    assert!(
        output.status.success(),
        "rust-audit should succeed with fake cargo: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fs::read_to_string(log_path).expect("fake cargo log should be readable");
    assert_eq!(
        log,
        format!("{}|audit\n", workspace.display()),
        "workspace root should be audited once"
    );
    assert!(
        !log.contains("target/ignored"),
        "target manifests should be ignored when absent from workspace metadata: {log}"
    );
    assert!(
        !log.contains("node_modules/ignored"),
        "node_modules manifests should be ignored when absent from workspace metadata: {log}"
    );
    assert!(
        !log.contains(".venv/ignored"),
        ".venv manifests should be ignored when absent from workspace metadata: {log}"
    );
}

#[rstest]
#[case(42, 0, "rust-audit should propagate cargo audit failure", true)]
#[case(0, 23, "rust-audit should propagate cargo metadata failure", false)]
fn rust_audit_propagates_failure(
    #[case] audit_exit_status: i32,
    #[case] metadata_exit_status: i32,
    #[case] failure_message: &str,
    #[case] should_audit_run: bool,
) {
    let temp = TempDir::new().expect("temporary workspace should be created");
    let workspace = temp.path();
    create_manifest(&workspace.join("Cargo.toml")).expect("root manifest should be created");

    let log_path = workspace.join("cargo-audit.log");
    let fake_cargo = write_fake_cargo(
        &workspace.join("bin"),
        &log_path,
        audit_exit_status,
        metadata_exit_status,
    )
    .expect("fake cargo should be built");
    let metadata = cargo_metadata_for(workspace, &[&workspace.join("Cargo.toml")]);

    let output =
        run_rust_audit(workspace, &fake_cargo, &metadata).expect("make rust-audit should run");

    assert!(
        !output.status.success(),
        "{failure_message}: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    if should_audit_run {
        let log = fs::read_to_string(&log_path).expect("fake cargo log should be readable");
        assert_eq!(
            log,
            format!("{}|audit\n", workspace.display()),
            "cargo audit should have been invoked once at the workspace root"
        );
    } else {
        assert!(
            !log_path.exists(),
            "cargo audit should not run after metadata failure"
        );
    }
}
