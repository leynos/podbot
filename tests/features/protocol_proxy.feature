Feature: Protocol byte proxying

  Protocol exec mode bridges raw stdio bytes between the host process and the
  container exec session without terminal-only behaviour.

  Scenario: Protocol proxy writes stdout bytes to host stdout
    Given container stdout emits reply
    When the protocol proxy runs
    Then host stdout receives reply
    And container stdin receives ""

  Scenario: Protocol proxy writes stderr bytes to host stderr
    Given container stderr emits warning
    When the protocol proxy runs
    Then host stderr receives warning
    And container stdin receives ""

  Scenario: Protocol proxy forwards host stdin to container stdin
    Given host stdin is request
    And container stdout emits reply
    When the protocol proxy runs
    Then container stdin receives request
    And host stdout receives reply

  Scenario: Protocol proxy fails when host stdout cannot be written
    Given container stdout emits reply
    And host stdout write fails
    When the protocol proxy runs
    Then the protocol proxy fails with an exec error
