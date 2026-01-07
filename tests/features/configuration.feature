Feature: Configuration loading

  The podbot configuration system supports layered configuration from files,
  environment variables, and command-line arguments.

  Scenario: Default configuration values
    Given no configuration is provided
    Then the sandbox is not privileged
    And dev/fuse mounting is enabled
    And the agent kind is Claude
    And the workspace base directory is /work

  Scenario: Configuration file overrides defaults
    Given a configuration file with privileged mode enabled
    Then the sandbox is privileged

  Scenario: Missing optional configuration is acceptable
    Given no GitHub configuration is provided
    Then the app ID is absent
    And the installation ID is absent
    And the private key path is absent
