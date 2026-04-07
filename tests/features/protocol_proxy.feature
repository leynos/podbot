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

  Scenario: Protocol proxy maintains stream purity through startup to shutdown
    Given container stdout emits startup-message
    And container stdout emits steady-state-data
    And the output stream ends
    When the protocol proxy runs
    Then host stdout concatenates startup-message and steady-state-data
    And host stdout contains no prefix or suffix bytes

  Scenario: Protocol proxy maintains purity when stream errors occur
    Given container stdout emits partial-output
    And the daemon stream fails with an error
    When the protocol proxy runs
    Then the protocol proxy fails with an exec error
    And host stdout contains only partial-output without error messages
