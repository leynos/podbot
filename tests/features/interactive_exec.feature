Feature: Interactive execution

  Podbot executes commands in running containers with attached and detached
  modes and reports command exit status accurately.

  Scenario: Attached execution succeeds and returns zero exit code
    Given attached execution mode is selected
    And tty allocation is enabled
    And command is echo hello
    And command exit code is 0
    When execution is requested
    Then execution succeeds
    And reported exit code is 0

  Scenario: Detached execution returns non-zero exit code
    Given detached execution mode is selected
    And command is sh -c exit 7
    And command exit code is 7
    When execution is requested
    Then execution succeeds
    And reported exit code is 7

  Scenario: Execution fails when daemon create-exec call fails
    Given attached execution mode is selected
    And command is echo hello
    And daemon create-exec call fails
    When execution is requested
    Then execution fails with an exec error

  Scenario: Execution fails when daemon omits exit code
    Given detached execution mode is selected
    And command is sh -c true
    And daemon omits exit code from inspect response
    When execution is requested
    Then execution fails due to missing exit code

  Scenario: Attached execution with tty disabled still succeeds
    Given attached execution mode is selected
    And tty allocation is disabled
    And command is echo hello
    And command exit code is 0
    When execution is requested
    Then execution succeeds
    And reported exit code is 0
