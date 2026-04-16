Feature: Git identity configuration

  Configure Git identity within the container by reading
  user.name and user.email from the host Git configuration.

  Scenario: Both name and email are configured
    Given host git user.name is Alice
    And host git user.email is alice@example.com
    And the container engine is available
    When git identity configuration is requested for container sandbox-1
    Then git identity result is configured
    And configured name is "Alice"
    And configured email is "alice@example.com"

  Scenario: Only name is configured on the host
    Given host git user.name is Bob
    And host git user.email is missing
    And the container engine is available
    When git identity configuration is requested for container sandbox-2
    Then git identity result is partial
    And configured name is "Bob"
    And email was not configured
    And warnings include "git user.email is not configured on the host"

  Scenario: Only email is configured on the host
    Given host git user.name is missing
    And host git user.email is carol@example.com
    And the container engine is available
    When git identity configuration is requested for container sandbox-3
    Then git identity result is partial
    And name was not configured
    And configured email is "carol@example.com"
    And warnings include "git user.name is not configured on the host"

  Scenario: Neither name nor email is configured
    Given host git user.name is missing
    And host git user.email is missing
    And the container engine is available
    When git identity configuration is requested for container sandbox-4
    Then git identity result is none configured
    And warnings include "git user.name is not configured on the host"
    And warnings include "git user.email is not configured on the host"

  Scenario: Multi-word name is configured
    Given host git user.name is Alice Smith
    And host git user.email is alice.smith@example.com
    And the container engine is available
    When git identity configuration is requested for container sandbox-5
    Then git identity result is configured
    And configured name is "Alice Smith"
    And configured email is "alice.smith@example.com"

  Scenario: Container exec failure propagates as error
    Given host git user.name is Alice
    And host git user.email is alice@example.com
    And the container engine exec will fail
    When git identity configuration is requested for container sandbox-6
    Then git identity configuration fails with an exec error
