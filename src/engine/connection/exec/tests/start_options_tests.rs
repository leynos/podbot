struct StartOptionsCase {
    mode: ExecMode,
    requested_tty: bool,
    expected_detach: bool,
    expected_tty: bool,
    expected_output_capacity: Option<usize>,
}

fn assert_start_options(options: &StartExecOptions, case: &StartOptionsCase) {
    assert_eq!(
        options.detach, case.expected_detach,
        "detach for {:?}",
        case.mode
    );
    assert_eq!(options.tty, case.expected_tty, "tty for {:?}", case.mode);
    assert_eq!(
        options.output_capacity, case.expected_output_capacity,
        "output_capacity for {:?}",
        case.mode
    );
}

#[rstest]
#[case(StartOptionsCase {
    mode: ExecMode::Protocol,
    requested_tty: false,
    expected_detach: false,
    expected_tty: false,
    expected_output_capacity: Some(PROTOCOL_OUTPUT_CAPACITY),
})]
#[case(StartOptionsCase {
    mode: ExecMode::Protocol,
    requested_tty: true,
    expected_detach: false,
    expected_tty: false,
    expected_output_capacity: Some(PROTOCOL_OUTPUT_CAPACITY),
})]
#[case(StartOptionsCase {
    mode: ExecMode::Attached,
    requested_tty: true,
    expected_detach: false,
    expected_tty: true,
    expected_output_capacity: None,
})]
#[case(StartOptionsCase {
    mode: ExecMode::Attached,
    requested_tty: false,
    expected_detach: false,
    expected_tty: false,
    expected_output_capacity: None,
})]
#[case(StartOptionsCase {
    mode: ExecMode::Detached,
    requested_tty: false,
    expected_detach: true,
    expected_tty: false,
    expected_output_capacity: None,
})]
#[case(StartOptionsCase {
    mode: ExecMode::Detached,
    requested_tty: true,
    expected_detach: true,
    expected_tty: false,
    expected_output_capacity: None,
})]
fn build_start_exec_options_per_mode(#[case] case: StartOptionsCase) -> TestResult {
    let request =
        ExecRequest::new("c", vec![String::from("cmd")], case.mode)?.with_tty(case.requested_tty);
    let options = build_start_exec_options(&request);
    assert_start_options(&options, &case);
    Ok(())
}
