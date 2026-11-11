Feature: Library boundary stability

  Podbot can be embedded as a Rust library dependency with
  documented, semantic APIs and no CLI coupling requirements.

  Scenario: Library consumer loads configuration without CLI types
    Given a mock environment with engine socket configured
    And explicit load options without config file discovery
    When the library configuration loader is called
    Then a valid AppConfig is returned
    And the engine socket matches the override value

  Scenario: Library consumer executes a command via the API
    Given a mock container engine client
    And exec parameters for an attached echo command
    When the library exec function is called
    Then the outcome is success

  Scenario: Library consumer receives semantic error for exec failure
    Given a mock container engine client that fails on create exec
    And exec parameters for an attached echo command
    When the library exec function is called
    Then the error is a ContainerError variant

  Scenario: Orchestration functions reject incomplete configuration
    Given a run request for repository owner/name and branch main
    When run and token-daemon orchestration are called with incomplete configuration
    Then every outcome is an error
