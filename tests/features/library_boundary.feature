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

  Scenario: Stub orchestration functions return success
    When each stub orchestration function is called
    Then all outcomes are success
