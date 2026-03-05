Feature: GitHub App credential validation

  Podbot validates GitHub App credentials on startup by loading the
  private key, building an authenticated client, and calling the
  GitHub API to verify the credentials produce a valid JWT.

  Scenario: Valid credentials pass validation
    Given a mock GitHub API that accepts App credentials
    And a valid RSA private key file exists at the configured path
    And the GitHub App ID is 12345
    When credentials are validated
    Then validation succeeds

  Scenario: GitHub API rejects credentials
    Given a mock GitHub API that rejects invalid App credentials
    And a valid RSA private key file exists at the configured path
    And the GitHub App ID is 99999
    When credentials are validated
    Then validation fails
    And the error mentions invalid credentials

  Scenario: API failure is handled gracefully
    Given a mock GitHub API that returns a server error
    And a valid RSA private key file exists at the configured path
    And the GitHub App ID is 12345
    When credentials are validated
    Then validation fails
    And the error mentions failed to validate
