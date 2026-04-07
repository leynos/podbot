Feature: Git identity configuration

  Configure Git identity within the container using host settings.
  Missing identity fields produce warnings rather than failures.

  Scenario: Both name and email configured on host
    Given host Git user name is configured as Alice
    And host Git user email is configured as alice@example.com
    When Git identity is applied to the container
    Then Git identity configuration succeeds
    And user.name is applied to the container
    And user.email is applied to the container
    And no warnings are emitted

  Scenario: Only user name configured on host
    Given host Git user name is configured as Bob
    And host Git user email is absent
    When Git identity is applied to the container
    Then Git identity configuration succeeds
    And user.name is applied to the container
    And user.email is not applied to the container
    And a warning mentions user.email

  Scenario: Only user email configured on host
    Given host Git user name is absent
    And host Git user email is configured as carol@example.com
    When Git identity is applied to the container
    Then Git identity configuration succeeds
    And user.name is not applied to the container
    And user.email is applied to the container
    And a warning mentions user.name

  Scenario: No Git identity configured on host
    Given host Git user name is absent
    And host Git user email is absent
    When Git identity is applied to the container
    Then Git identity configuration succeeds
    And user.name is not applied to the container
    And user.email is not applied to the container
    And a warning mentions no Git identity configured

  Scenario: Container exec fails for one field
    Given host Git user name is configured as Dave
    And host Git user email is configured as dave@example.com
    And container exec will fail
    When Git identity is applied to the container
    Then Git identity configuration succeeds
    And a warning mentions failed
