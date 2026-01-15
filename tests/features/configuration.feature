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

  Scenario: Invalid agent kind is rejected
    Given a configuration file with an invalid agent kind
    Then the configuration load fails

  Scenario: GitHub configuration validates successfully when complete
    Given a complete GitHub configuration
    Then GitHub validation succeeds

  Scenario: GitHub configuration validation fails when app ID is missing
    Given a GitHub configuration missing the app ID
    Then GitHub validation fails
    And the validation error mentions "github.app_id"

  Scenario: GitHub configuration validation fails when all fields missing
    Given a GitHub configuration with no fields set
    Then GitHub validation fails
    And the validation error mentions all missing GitHub fields

  Scenario: GitHub configuration is not required for non-GitHub operations
    Given no GitHub configuration is provided
    Then the configuration loads successfully
    And GitHub is not configured
