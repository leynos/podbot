//! Contract tests for the `CodeScene` uploader inputs used by this
//! repository's workflows.
//!
//! At the approved pin the shared uploader treats its committed
//! `cli-manifest.json` as the trust anchor for the CLI archive, and it
//! *rejects* a non-empty `installer-checksum` with a hard failure rather than
//! ignoring it. A workflow that still passes the input therefore breaks the
//! upload step as soon as the pin moves, and the repository variable that fed
//! it could only ever repeat the manifest digest.
//!
//! Four separate concerns are asserted, each in its own test so that a failure
//! names the defect rather than a bundle:
//!
//! * no workflow passes the deprecated input;
//! * no workflow references the variable that fed it;
//! * every uploader reference is pinned to one approved full SHA;
//! * the dispatch workflow that refreshed the variable is absent.
//!
//! Every test that ranges over a collection asserts the collection has
//! content before it asserts compliance. A contract that ranges over an empty
//! collection is satisfied by deleting the thing it guards, so emptiness is a
//! failure in its own right.

use camino::{Utf8Path, Utf8PathBuf};
use cap_std::ambient_authority;
use cap_std::fs_utf8::Dir;
use rstest::rstest;

/// Full SHA of the approved `upload-codescene-coverage` pin.
const APPROVED_UPLOADER_PIN: &str = "a5765019912a8ab6882b12db049c7cde635f3a85";
/// Marker preceding the pin in a workflow's `uses:` expression.
const UPLOADER_REFERENCE: &str = "leynos/shared-actions/.github/actions/upload-codescene-coverage@";
/// Input the uploader rejects outright at the approved pin.
const DEPRECATED_INPUT: &str = "installer-checksum";
/// Repository variable whose only consumer was the deprecated input.
const DEPRECATED_VARIABLE: &str = "CODESCENE_CLI_SHA256";
/// Dispatch workflow that refreshed the now-unread repository variable.
const REFRESH_WORKFLOW: &str = "get-codescene-sha.yml";

/// Unwrap `result`, reporting `context` when it failed.
///
/// The gate denies `expect` and panicking `unwrap_or_else` closures, and it
/// denies assertions inside functions that return `Result`. A contract that
/// cannot read its own inputs has no answer to give, so the failure is raised
/// here alongside the context naming the filesystem operation.
fn or_panic<T, E: std::fmt::Display>(result: Result<T, E>, context: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{context}: {error}"),
    }
}

/// Directory holding this repository's own workflow definitions.
fn workflow_directory() -> Utf8PathBuf {
    Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows")
}

/// Open the workflow directory as a capability handle.
fn open_workflow_directory() -> Dir {
    let directory = workflow_directory();
    or_panic(
        Dir::open_ambient_dir(&directory, ambient_authority()),
        &format!("cannot open {directory}"),
    )
}

/// Whether `name` denotes a workflow document rather than any other file.
fn is_workflow_file(name: &str) -> bool {
    Utf8Path::new(name).extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("yml") || extension.eq_ignore_ascii_case("yaml")
    })
}

/// Every workflow file name paired with its text, sorted for stable failures.
fn workflows() -> Vec<(String, String)> {
    let directory = open_workflow_directory();
    let entries = or_panic(directory.entries(), "cannot list the workflow directory");
    let mut found: Vec<(String, String)> = entries
        .map(|entry| {
            let found = or_panic(entry, "cannot read a workflow directory entry");
            or_panic(found.file_name(), "cannot read a workflow file name")
        })
        .filter(|name| is_workflow_file(name))
        .map(|name| {
            let contents = or_panic(
                directory.read_to_string(&name),
                &format!("cannot read workflow {name}"),
            );
            (name, contents)
        })
        .collect();
    found.sort();
    found
}

/// Assert that no workflow mentions `needle`.
///
/// Both containment clauses have the same shape, so they share one assertion
/// rather than being copied: `reason` names why the mention is wrong, and the
/// caller stays a single test so a failure still names one defect.
fn assert_no_workflow_mentions(needle: &str, reason: &str) {
    let workflows = workflows();
    assert!(
        !workflows.is_empty(),
        "no workflow files were examined, so this contract would pass vacuously",
    );
    let offenders: Vec<&String> = workflows
        .iter()
        .filter(|(_, contents)| contents.contains(needle))
        .map(|(name, _)| name)
        .collect();
    assert!(
        offenders.is_empty(),
        "{needle} {reason}; remove it from {offenders:?}",
    );
}

/// Every uploader pin found in the workflows, paired with its workflow name.
///
/// A pin is the run of non-whitespace characters following the action
/// reference, so a tag, a branch name or an empty value is reported unchanged
/// and fails the allowlist below rather than being silently accepted.
fn uploader_pins() -> Vec<(String, String)> {
    workflows()
        .into_iter()
        .flat_map(|(name, contents)| {
            contents
                .split(UPLOADER_REFERENCE)
                .skip(1)
                .map(|tail| {
                    let pin = tail
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_owned();
                    (name.clone(), pin)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The uploader rejects a non-empty value, so no workflow may pass the input.
#[rstest]
fn no_workflow_passes_the_deprecated_installer_checksum() {
    assert_no_workflow_mentions(
        DEPRECATED_INPUT,
        &format!("is deprecated and rejected by the uploader at {APPROVED_UPLOADER_PIN}"),
    );
}

/// The variable existed only to feed the rejected input, so it must go too.
#[rstest]
fn no_workflow_references_the_deprecated_checksum_variable() {
    assert_no_workflow_mentions(
        DEPRECATED_VARIABLE,
        "fed the deprecated installer checksum and has no remaining consumer",
    );
}

/// One approved SHA, asserted as an allowlist rather than as a floor.
///
/// A floor would require ordering SHAs, which cannot be computed from a
/// checkout. Naming the approved pin keeps the contract hermetic and fails
/// closed on any other value, including a tag or a branch name.
#[rstest]
fn every_uploader_reference_is_pinned_to_the_approved_sha() {
    let pins = uploader_pins();
    assert!(
        !pins.is_empty(),
        "no upload-codescene-coverage reference was found, so this contract \
         would pass vacuously; this repository is expected to send coverage \
         to CodeScene"
    );
    let wrong: Vec<String> = pins
        .iter()
        .filter(|(_, pin)| pin != APPROVED_UPLOADER_PIN)
        .map(|(workflow, pin)| format!("{workflow}: {pin}"))
        .collect();
    assert!(
        wrong.is_empty(),
        "every upload-codescene-coverage reference must be pinned to \
         {APPROVED_UPLOADER_PIN}; found {}",
        wrong.join(", ")
    );
}

/// Nothing consumes the variable it wrote, so the workflow is dead code.
#[rstest]
fn the_checksum_refresh_workflow_is_absent() {
    let directory = open_workflow_directory();
    assert!(
        !directory.exists(REFRESH_WORKFLOW),
        "{REFRESH_WORKFLOW} refreshed {DEPRECATED_VARIABLE}, which no workflow \
         reads any more; delete it rather than leaving a dispatch that writes \
         an unused repository variable"
    );
}
