//! Integration tests for the Makefile Rust audit target.

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

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
) -> TestResult<std::path::PathBuf> {
    let cargo_path = bin_dir.join("cargo");
    write_file(
        &cargo_path,
        &format!(
            "#!/usr/bin/env sh\nif [ \"$1\" = metadata ]; then\nprintf '%s\\n' \"$PODBOT_FAKE_CARGO_METADATA\"\nexit 0\nfi\nprintf '%s|%s\\n' \"$PWD\" \"$*\" >> '{}'\nexit {exit_status}\n",
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

fn cargo_metadata_for(manifests: &[&Path]) -> String {
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
    format!(r#"{{"packages":[{packages}],"workspace_members":[{members}]}}"#)
}

#[test]
fn rust_audit_invokes_cargo_audit_for_each_workspace_manifest() {
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
    let fake_cargo =
        write_fake_cargo(&workspace.join("bin"), &log_path, 0).expect("fake cargo should be built");
    let metadata = cargo_metadata_for(&[&root_manifest, &member_manifest]);

    let output =
        run_rust_audit(workspace, &fake_cargo, &metadata).expect("make rust-audit should run");

    assert!(
        output.status.success(),
        "rust-audit should succeed with fake cargo: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fs::read_to_string(log_path).expect("fake cargo log should be readable");
    assert!(
        log.contains(&format!("{}|audit", workspace.display())),
        "root manifest should be audited: {log}"
    );
    assert!(
        log.contains(&format!(
            "{}|audit",
            workspace.join("crates/agent").display()
        )),
        "nested manifest should be audited: {log}"
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

#[test]
fn rust_audit_fails_when_cargo_audit_fails() {
    let temp = TempDir::new().expect("temporary workspace should be created");
    let workspace = temp.path();
    create_manifest(&workspace.join("Cargo.toml")).expect("root manifest should be created");

    let log_path = workspace.join("cargo-audit.log");
    let fake_cargo = write_fake_cargo(&workspace.join("bin"), &log_path, 42)
        .expect("failing fake cargo should be built");
    let metadata = cargo_metadata_for(&[&workspace.join("Cargo.toml")]);

    let output =
        run_rust_audit(workspace, &fake_cargo, &metadata).expect("make rust-audit should run");

    assert!(
        !output.status.success(),
        "rust-audit should propagate cargo audit failure"
    );
    let log = fs::read_to_string(log_path).expect("fake cargo log should be readable");
    assert_eq!(log, format!("{}|audit\n", workspace.display()));
}
