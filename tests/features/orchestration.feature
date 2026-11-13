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

  Scenario: Run orchestration rejects incomplete GitHub configuration
    When run orchestration is invoked
    Then the outcome is an error mentioning github.app_id

  Scenario: Stop orchestration stops a managed container
    Given a mock lifecycle client with a running managed container
    When stop orchestration is invoked with container test-ctr
    Then the outcome is success

  Scenario: Stop orchestration refuses an unmanaged container
    Given a mock lifecycle client with a running unmanaged container
    When stop orchestration is invoked with container test-ctr
    Then the outcome is an error mentioning not managed

  Scenario: List containers orchestration returns managed containers
    Given a mock lifecycle client listing 2 managed containers
    When list containers orchestration is invoked
    Then the listing contains 2 containers

  Scenario: Token daemon rejects a blank container identifier
    When token daemon orchestration is invoked with a blank container
    Then the outcome is an error mentioning container_id
