Feature: Credential injection

  Podbot uploads selected host credential directories into the sandbox
  container and reports the expected in-container paths.

  Scenario: Upload selected credentials when both toggles are enabled
    Given host has Claude credentials
    And host has Codex credentials
    And copy_claude toggle is enabled
    And copy_codex toggle is enabled
    When credential injection is requested
    Then credential injection succeeds
    And expected container credential paths are /root/.claude,/root/.codex
    And credential upload is attempted once

  Scenario: Upload only Claude credentials when Codex toggle is disabled
    Given host has Claude credentials
    And host has Codex credentials
    And copy_claude toggle is enabled
    And copy_codex toggle is disabled
    When credential injection is requested
    Then credential injection succeeds
    And expected container credential paths are /root/.claude
    And credential upload is attempted once

  Scenario: Upload only Codex credentials when Claude toggle is disabled
    Given host has Claude credentials
    And host has Codex credentials
    And copy_claude toggle is disabled
    And copy_codex toggle is enabled
    When credential injection is requested
    Then credential injection succeeds
    And expected container credential paths are /root/.codex
    And credential upload is attempted once

  Scenario: Missing source directory is skipped
    Given host has Codex credentials
    And copy_claude toggle is enabled
    And copy_codex toggle is enabled
    When credential injection is requested
    Then credential injection succeeds
    And expected container credential paths are /root/.codex
    And credential upload is attempted once

  Scenario: No upload occurs when both toggles are disabled
    Given host has Claude credentials
    And host has Codex credentials
    And copy_claude toggle is disabled
    And copy_codex toggle is disabled
    When credential injection is requested
    Then credential injection succeeds
    And expected container credential paths are empty
    And credential upload is not attempted

  Scenario: Upload failures map to UploadFailed
    Given host has Claude credentials
    And copy_claude toggle is enabled
    And copy_codex toggle is disabled
    And the credential upload operation fails
    When credential injection is requested
    Then credential injection fails with UploadFailed for container sandbox-credential-test
