//! BDD scenario wiring for ACP capability masking tests.

use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then, when};

use super::{
    initialize_without_blocked_capabilities, malformed_initialize_bytes,
    masked_initialize_with_follow_up, run_forwarding,
};

type StepResult<T> = Result<T, String>;

#[derive(Default, ScenarioState)]
struct AcpMaskingState {
    host_stdin: Slot<Vec<u8>>,
    expected_forwarded: Slot<Vec<u8>>,
    actual_forwarded: Slot<Vec<u8>>,
    succeeded: Slot<bool>,
}

#[rstest::fixture]
fn acp_masking_state() -> AcpMaskingState {
    AcpMaskingState::default()
}

#[given(
    "ACP stdin contains an initialize request with blocked capabilities and a follow-up request"
)]
fn acp_stdin_contains_blocked_initialize_and_follow_up(acp_masking_state: &AcpMaskingState) {
    let (host_stdin_bytes, expected) = masked_initialize_with_follow_up();
    acp_masking_state.host_stdin.set(host_stdin_bytes);
    acp_masking_state.expected_forwarded.set(expected);
}

#[given("ACP stdin contains malformed initialize bytes")]
fn acp_stdin_contains_malformed_initialize(acp_masking_state: &AcpMaskingState) {
    let malformed = malformed_initialize_bytes();
    acp_masking_state.host_stdin.set(malformed.clone());
    acp_masking_state.expected_forwarded.set(malformed);
}

#[given("ACP stdin contains initialize without blocked capabilities")]
fn acp_stdin_contains_safe_initialize(acp_masking_state: &AcpMaskingState) {
    let initialize = initialize_without_blocked_capabilities();
    acp_masking_state.host_stdin.set(initialize.clone());
    acp_masking_state.expected_forwarded.set(initialize);
}

#[when("ACP stdin forwarding runs")]
fn acp_stdin_forwarding_runs(acp_masking_state: &AcpMaskingState) -> StepResult<()> {
    let host_stdin_bytes = acp_masking_state
        .host_stdin
        .get()
        .ok_or_else(|| String::from("host stdin should be configured"))?;
    let (forwarded, _) = run_forwarding(&host_stdin_bytes);
    acp_masking_state.actual_forwarded.set(forwarded);
    acp_masking_state.succeeded.set(true);
    Ok(())
}

#[then("ACP stdin forwarding succeeds")]
fn acp_stdin_forwarding_succeeds(acp_masking_state: &AcpMaskingState) -> StepResult<()> {
    if acp_masking_state.succeeded.get() == Some(true) {
        Ok(())
    } else {
        Err(String::from("expected ACP stdin forwarding to succeed"))
    }
}

#[then("the forwarded ACP stdin matches the expected bytes")]
fn forwarded_acp_stdin_matches_expected(acp_masking_state: &AcpMaskingState) -> StepResult<()> {
    let actual = acp_masking_state
        .actual_forwarded
        .get()
        .ok_or_else(|| String::from("forwarded bytes should be recorded"))?;
    let expected = acp_masking_state
        .expected_forwarded
        .get()
        .ok_or_else(|| String::from("expected bytes should be recorded"))?;

    if actual == expected {
        Ok(())
    } else {
        Err(format!("expected {expected:?}, got {actual:?}"))
    }
}

#[scenario(
    path = "tests/features/acp_capability_masking.feature",
    name = "ACP initialize masks blocked capabilities before forwarding"
)]
fn acp_initialize_masks_blocked_capabilities(acp_masking_state: AcpMaskingState) {
    let _ = acp_masking_state;
}

#[scenario(
    path = "tests/features/acp_capability_masking.feature",
    name = "Malformed ACP initialize is forwarded unchanged"
)]
fn malformed_acp_initialize_is_forwarded_unchanged(acp_masking_state: AcpMaskingState) {
    let _ = acp_masking_state;
}

#[scenario(
    path = "tests/features/acp_capability_masking.feature",
    name = "ACP initialize without blocked capabilities stays unchanged"
)]
fn acp_initialize_without_blocked_capabilities_stays_unchanged(acp_masking_state: AcpMaskingState) {
    let _ = acp_masking_state;
}
