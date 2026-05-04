Feature: GitHub installation token acquisition
  As a podbot operator
  I want installation tokens to be acquired with an expiry buffer
  So that repository operations do not start with nearly expired credentials

  Scenario: Valid credentials produce an installation token
    Given a valid RSA private key file exists at the configured path
    And the GitHub App ID is 12345
    And the GitHub installation ID is 67890
    And the expiry buffer is 300 seconds
    And GitHub returns a valid installation token
    When the installation token is requested
    Then installation token acquisition succeeds
    And the returned token is exposed for Git operations
    And the returned expiry metadata is preserved

  Scenario: Token expiry inside the buffer is rejected
    Given a valid RSA private key file exists at the configured path
    And the GitHub App ID is 12345
    And the GitHub installation ID is 67890
    And the expiry buffer is 300 seconds
    And GitHub returns an installation token that expires inside the buffer
    When the installation token is requested
    Then installation token acquisition fails with token expired

  Scenario: GitHub rejects installation token acquisition
    Given a valid RSA private key file exists at the configured path
    And the GitHub App ID is 12345
    And the GitHub installation ID is 67890
    And the expiry buffer is 300 seconds
    And GitHub rejects installation token acquisition
    When the installation token is requested
    Then the error mentions installation not found

  Scenario: Missing expiry metadata is rejected
    Given a valid RSA private key file exists at the configured path
    And the GitHub App ID is 12345
    And the GitHub installation ID is 67890
    And the expiry buffer is 300 seconds
    And GitHub omits the installation token expiry metadata
    When the installation token is requested
    Then the error mentions missing expires_at metadata
