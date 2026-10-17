//! Compile-pass contract for unit and fallible BDD step signatures.

use rstest::fixture;
use rstest_bdd_macros::{given, when};

type StepResult<T> = Result<T, &'static str>;

#[derive(Default)]
struct StepState {
    given_ran: bool,
    when_ran: bool,
}

#[fixture]
fn step_state() -> StepState {
    StepState::default()
}

#[given("a state for the signature contract")]
fn given_state(step_state: &mut StepState) {
    step_state.given_ran = true;
}

#[when("the aliased fixture state is updated")]
fn when_update_state(#[from(step_state)] renamed_state: &mut StepState) {
    renamed_state.when_ran = true;
}

#[when("the fallible step sees both updates")]
fn when_validate_state(step_state: &StepState) -> StepResult<()> {
    if step_state.given_ran && step_state.when_ran {
        Ok(())
    } else {
        Err("the Given and When steps must update the state")
    }
}

fn main() -> StepResult<()> {
    let mut state = step_state();
    given_state(&mut state);
    when_update_state(&mut state);
    when_validate_state(&state)
}
