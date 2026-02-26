Feature: GitHub App private key loading

  Podbot loads a PEM-encoded RSA private key from a configured file
  path to establish GitHub App identity for JWT signing. Only RSA
  keys are accepted because GitHub requires RS256.

  Scenario: Valid RSA private key is loaded successfully
    Given a valid RSA private key file exists at the configured path
    When the private key is loaded
    Then the private key loads successfully

  Scenario: Missing key file produces a clear error
    Given no private key file exists at the configured path
    When the private key is loaded
    Then the private key load fails
    And the error mentions failed to read file

  Scenario: Empty key file produces a clear error
    Given an empty private key file exists at the configured path
    When the private key is loaded
    Then the private key load fails
    And the error mentions empty

  Scenario: Invalid PEM content produces a clear error
    Given a file with invalid PEM content exists at the configured path
    When the private key is loaded
    Then the private key load fails
    And the error mentions invalid RSA private key

  Scenario: ECDSA key is rejected with a clear error
    Given an ECDSA private key file exists at the configured path
    When the private key is loaded
    Then the private key load fails
    And the error mentions ECDSA

  Scenario: Ed25519 key is rejected with a clear error
    Given an Ed25519 private key file exists at the configured path
    When the private key is loaded
    Then the private key load fails
    And the error mentions invalid RSA private key

  Scenario: Public key is rejected with a clear error
    Given a public key file exists at the configured path
    When the private key is loaded
    Then the private key load fails
    And the error mentions public key

  Scenario: Certificate is rejected with a clear error
    Given a certificate file exists at the configured path
    When the private key is loaded
    Then the private key load fails
    And the error mentions certificate

  Scenario: OpenSSH key is rejected with a clear error
    Given an OpenSSH private key file exists at the configured path
    When the private key is loaded
    Then the private key load fails
    And the error mentions OpenSSH

  Scenario: Encrypted key is rejected with a clear error
    Given an encrypted private key file exists at the configured path
    When the private key is loaded
    Then the private key load fails
    And the error mentions encrypted
