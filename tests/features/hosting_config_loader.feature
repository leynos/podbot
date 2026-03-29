Feature: Library hosting configuration loader

  Embedders should be able to load hosted configurations through the library
  API without using Clap types.

  Scenario: Hosted configuration loads through the library API
    Given a hosting configuration file for a custom codex app server
    When the hosting library configuration is loaded
    Then the loaded hosting configuration uses a host-mounted workspace
    And the loaded hosting agent mode is codex_app_server
    And the loaded hosting workspace container path is /workspace

  Scenario: Hosting env vars override defaults
    Given hosting environment variables describe an ACP custom agent
    When the hosting library configuration is loaded
    Then the loaded hosting agent mode is acp
    And the loaded hosting workspace container path is /workspace

  Scenario: Run intent rejects hosted library configuration
    Given hosting environment variables describe an ACP custom agent
    And the hosting loader uses run intent
    When the hosting library configuration is loaded
    Then hosting configuration loading fails mentioning podbot host
