Feature: GitHub App installation token acquisition

  Podbot acquires short-lived GitHub App installation tokens on the host so
  later Git operations can use scoped credentials without exposing the App
  private key to the sandbox.

  Scenario: Installation token acquisition succeeds
    Given a mock GitHub installation token API that returns a scoped token
    And the GitHub App installation ID is 4242
    And the token expiry buffer is 300 seconds
    When an installation token is acquired
    Then token acquisition succeeds
    And the token string is available for Git operations
    And expiry metadata includes the configured buffer
    And observable token metadata does not expose the token string

  Scenario: Installation token acquisition fails semantically
    Given a mock GitHub installation token API that rejects the installation
    And the GitHub App installation ID is 4242
    And the token expiry buffer is 300 seconds
    When an installation token is acquired
    Then token acquisition fails
    And the error mentions installation token acquisition
    And observable token metadata does not expose the token string
