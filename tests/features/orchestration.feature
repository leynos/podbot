Feature: Command orchestration

  The podbot library provides orchestration functions that execute
  commands in containers and return typed outcomes without printing or
  exiting.

  Scenario: Exec orchestration returns success for zero exit code
    Given a mock container engine
    And exec mode is attached
    And tty is enabled
    And the command is echo hello
    And the daemon reports exit code 0
    When exec orchestration is invoked
    Then the outcome is success

  Scenario: Exec orchestration returns command exit for non-zero exit code
    Given a mock container engine
    And exec mode is detached
    And the command is sh -c exit 7
    And the daemon reports exit code 7
    When exec orchestration is invoked
    Then the outcome is command exit with code 7

  Scenario: Run stub returns success
    When run orchestration is invoked
    Then the outcome is success

  Scenario: Stop stub returns success
    When stop orchestration is invoked with container test-ctr
    Then the outcome is success

  Scenario: List containers stub returns success
    When list containers orchestration is invoked
    Then the outcome is success

  Scenario: Token daemon stub returns success
    When token daemon orchestration is invoked with container test-ctr
    Then the outcome is success
