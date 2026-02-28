Feature: GitHub App client construction

  Podbot builds an authenticated Octocrab client from a GitHub App ID
  and RSA private key. The client is configured for JWT signing but does
  not make network calls during construction.

  Scenario: Valid credentials produce an App client
    Given a valid RSA private key file exists at the configured path
    And the GitHub App ID is 12345
    When the App client is built
    Then the App client is created successfully

  Scenario: Zero App ID is accepted by the builder
    Given a valid RSA private key file exists at the configured path
    And the GitHub App ID is 0
    When the App client is built
    Then the App client is created successfully
