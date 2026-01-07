Feature: Command-line interface

  The podbot CLI provides subcommands for running AI agents in sandboxed
  containers and managing those containers.

  Scenario: Display help information
    Given the CLI is invoked with --help
    Then the output contains Sandboxed execution environment
    And the output contains run
    And the output contains ps
    And the output contains stop

  Scenario: Display version information
    Given the CLI is invoked with --version
    Then the output contains podbot

  Scenario: Run command requires repository
    Given the CLI is invoked with run
    Then an error is returned
    And the error mentions --repo

  Scenario: Run command requires branch
    Given the CLI is invoked with run --repo owner/name
    Then an error is returned
    And the error mentions --branch
