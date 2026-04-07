Feature: GitHub App credential error classification

  When GitHub App credential validation fails, podbot classifies the
  failure mode and produces an actionable error message with
  remediation hints.

  Scenario: Credentials rejected by GitHub produce a clear hint
    Given a mock GitHub API that rejects credentials with HTTP 401
    And a valid RSA private key file exists at the configured path
    And the GitHub App ID is 12345
    When credentials are validated
    Then validation fails
    And the error mentions credentials rejected
    And the error includes a remediation hint

  Scenario: App not found produces a clear hint
    Given a mock GitHub API that returns HTTP 404
    And a valid RSA private key file exists at the configured path
    And the GitHub App ID is 99999
    When credentials are validated
    Then validation fails
    And the error mentions not found
    And the error includes an app ID verification hint

  Scenario: GitHub server error produces a retry hint
    Given a mock GitHub API that returns HTTP 503
    And a valid RSA private key file exists at the configured path
    And the GitHub App ID is 12345
    When credentials are validated
    Then validation fails
    And the error mentions unavailable
    And the error includes a status page hint

  Scenario: Permission error produces a permissions hint
    Given a mock GitHub API that returns HTTP 403
    And a valid RSA private key file exists at the configured path
    And the GitHub App ID is 12345
    When credentials are validated
    Then validation fails
    And the error mentions permissions
    And the error includes a settings hint
