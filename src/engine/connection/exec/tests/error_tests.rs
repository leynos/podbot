struct ErrorScenario {
    name: &'static str,
    exec_id: &'static str,
    mode: ExecMode,
    command: Vec<String>,
    setup_failure: fn(&mut MockExecClient),
    expected_container_id: Option<&'static str>,
    expected_message_fragment: &'static str,
}

#[rstest]
#[case(ErrorScenario {
    name: "create_exec_failure",
    exec_id: "exec-create-failure",
    mode: ExecMode::Detached,
    command: vec![String::from("false")],
    setup_failure: setup_create_exec_failure_scenario,
    expected_container_id: Some("sandbox-123"),
    expected_message_fragment: "create exec failed",
})]
#[case(ErrorScenario {
    name: "missing_exit_code",
    exec_id: "exec-2",
    mode: ExecMode::Detached,
    command: vec![String::from("false")],
    setup_failure: setup_missing_exit_code_scenario,
    expected_container_id: None,
    expected_message_fragment: "without an exit code",
})]
#[case(ErrorScenario {
    name: "attached_rejects_detached_response",
    exec_id: "exec-3",
    mode: ExecMode::Attached,
    command: vec![String::from("echo"), String::from("hello")],
    setup_failure: setup_attached_detached_response_scenario,
    expected_container_id: None,
    expected_message_fragment: "detached start result",
})]
fn exec_async_error_scenarios(
    runtime: RuntimeFixture,
    #[case] scenario: ErrorScenario,
) -> TestResult {
    let runtime_handle = runtime?;
    let mut client = MockExecClient::new();
    (scenario.setup_failure)(&mut client);

    let request = ExecRequest::new("sandbox-123", scenario.command, scenario.mode)?;
    let result = runtime_handle.block_on(EngineConnector::exec_async(&client, &request));
    let assertion_context = format!(
        "{} ({}) expected error mapping",
        scenario.name, scenario.exec_id
    );

    if let Some(expected_container_id) = scenario.expected_container_id {
        assert_exec_failed_for_container_with_message(
            result,
            expected_container_id,
            scenario.expected_message_fragment,
            &assertion_context,
        );
    } else {
        assert_exec_failed_with_message(
            result,
            scenario.expected_message_fragment,
            &assertion_context,
        );
    }

    Ok(())
}
