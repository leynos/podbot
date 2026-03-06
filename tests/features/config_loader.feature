Feature: Library configuration loader

  Podbot must be embeddable as a Rust library. Embedders should be able to load
  configuration without constructing CLI parse types.

  Scenario: Load defaults when no sources are provided
    Given no configuration sources are provided
    When the library configuration is loaded
    Then the loaded configuration uses defaults

  Scenario: Load configuration from an explicit path hint
    Given a configuration file sets image to file-image:v1
    When the library configuration is loaded
    Then the loaded configuration image is file-image:v1

  Scenario: Environment overrides configuration file
    Given a configuration file sets image to file-image:v1
    And the environment variable PODBOT_IMAGE is set to env-image:v2
    When the library configuration is loaded
    Then the loaded configuration image is env-image:v2

  Scenario: Host overrides take precedence over environment and file
    Given a configuration file sets image to file-image:v1
    And the environment variable PODBOT_IMAGE is set to env-image:v2
    And host overrides set image to overrides-image:v3
    When the library configuration is loaded
    Then the loaded configuration image is overrides-image:v3

  Scenario: Invalid typed environment values fail fast
    Given the environment variable PODBOT_SANDBOX_PRIVILEGED is set to maybe
    When the library configuration is loaded
    Then configuration loading fails mentioning PODBOT_SANDBOX_PRIVILEGED

