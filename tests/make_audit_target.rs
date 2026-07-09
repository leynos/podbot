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
//! working directory without network access or a real `RustSec` database.
//!
//! See also: `Makefile` (`rust-audit` target), `docs/developers-guide.md`
//! (§ 2 Quality gates, § 2.1 Security audit ignores).

use std::path::Path;
use std::process::Command;

use cap_std::fs::Dir;
use rstest::rstest;
use tempfile::TempDir;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Workspace-relative location of the generated fake `cargo` script.
const FAKE_CARGO_RELATIVE_PATH: &str = "bin/cargo";

/// Open a capability handle on the temporary workspace root.
fn open_workspace_dir(workspace: &Path) -> TestResult<Dir> {
    Ok(Dir::open_ambient_dir(
        workspace,
        cap_std::ambient_authority(),
    )?)
}

fn write_file(workspace_dir: &Dir, relative_path: &str, contents: &str) -> TestResult {
    if let Some(parent) = Path::new(relative_path).parent()
        && !parent.as_os_str().is_empty()
    {
        workspace_dir.create_dir_all(parent)?;
    }
    workspace_dir.write(relative_path, contents)?;
    Ok(())
}

/// Writes the fake `cargo` script at `bin/cargo` inside the workspace; the
/// caller derives its absolute path with `workspace.join(FAKE_CARGO_RELATIVE_PATH)`.
fn write_fake_cargo(
    workspace_dir: &Dir,
    log_path: &Path,
    exit_status: i32,
    metadata_status: i32,
) -> TestResult {
    write_file(
        workspace_dir,
        FAKE_CARGO_RELATIVE_PATH,
        &format!(
            "#!/usr/bin/env sh\nif [ \"$1\" = metadata ]; then\nprintf '%s\\n' \"$PODBOT_FAKE_CARGO_METADATA\"\nexit {metadata_status}\nfi\nprintf '%s|%s\\n' \"$PWD\" \"$*\" >> '{}'\nexit {exit_status}\n",
            log_path.display()
        ),
    )?;
    #[cfg(unix)]
    {
        use cap_std::fs::PermissionsExt;

        let permissions = cap_std::fs::Permissions::from_mode(0o755);
        workspace_dir.set_permissions(FAKE_CARGO_RELATIVE_PATH, permissions)?;
    }
    Ok(())
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

fn create_manifest(workspace_dir: &Dir, relative_path: &str) -> TestResult {
    write_file(
        workspace_dir,
        relative_path,
        "[package]\nname = \"fixture\"\nversion = \"0.0.0\"\n",
    )
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
    let workspace_dir = open_workspace_dir(workspace).expect("workspace dir should open");
    let root_manifest = workspace.join("Cargo.toml");
    let member_manifest = workspace.join("crates/agent/Cargo.toml");
    create_manifest(&workspace_dir, "Cargo.toml").expect("root manifest should be created");
    create_manifest(&workspace_dir, "crates/agent/Cargo.toml")
        .expect("nested manifest should be created");
    create_manifest(&workspace_dir, "target/ignored/Cargo.toml")
        .expect("target manifest fixture should be created");
    create_manifest(&workspace_dir, "node_modules/ignored/Cargo.toml")
        .expect("node_modules manifest fixture should be created");
    create_manifest(&workspace_dir, ".venv/ignored/Cargo.toml")
        .expect("virtualenv manifest fixture should be created");

    let log_path = workspace.join("cargo-audit.log");
    write_fake_cargo(&workspace_dir, &log_path, 0, 0).expect("fake cargo should be built");
    let fake_cargo = workspace.join(FAKE_CARGO_RELATIVE_PATH);
    let metadata = cargo_metadata_for(workspace, &[&root_manifest, &member_manifest]);

    let output =
        run_rust_audit(workspace, &fake_cargo, &metadata).expect("make rust-audit should run");

    assert!(
        output.status.success(),
        "rust-audit should succeed with fake cargo: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let log = workspace_dir
        .read_to_string("cargo-audit.log")
        .expect("fake cargo log should be readable");
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
    let workspace_dir = open_workspace_dir(workspace).expect("workspace dir should open");
    create_manifest(&workspace_dir, "Cargo.toml").expect("root manifest should be created");

    let log_path = workspace.join("cargo-audit.log");
    write_fake_cargo(
        &workspace_dir,
        &log_path,
        audit_exit_status,
        metadata_exit_status,
    )
    .expect("fake cargo should be built");
    let fake_cargo = workspace.join(FAKE_CARGO_RELATIVE_PATH);
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
        let log = workspace_dir
            .read_to_string("cargo-audit.log")
            .expect("fake cargo log should be readable");
        assert_eq!(
            log,
            format!("{}|audit\n", workspace.display()),
            "cargo audit should have been invoked once at the workspace root"
        );
    } else {
        assert!(
            !workspace_dir.exists("cargo-audit.log"),
            "cargo audit should not run after metadata failure"
        );
    }
}
