Feature: Hosting configuration compatibility

  Hosted app-server configurations should load deterministically and fail with
  actionable semantic errors when the schema is used incorrectly.

  Scenario: Legacy interactive configuration remains valid
    Given the default application configuration
    When the configuration is normalized for run intent
    Then the normalized configuration uses github_clone workspace defaults
    And the normalized configuration uses podbot agent defaults

  Scenario: Host-mounted workspace gains a default container path
    Given a host-mounted custom agent configuration
    When the configuration is normalized for host intent
    Then the workspace container path defaults to /workspace

  Scenario: Run intent rejects hosted modes
    Given a hosted custom agent configuration
    When the configuration is normalized for run intent
    Then semantic validation fails for agent.mode mentioning podbot host

  Scenario: Host mount requires an explicit host path
    Given a host-mounted workspace without a host path
    When the configuration is normalized for host intent
    Then semantic validation fails for workspace.host_path mentioning host_mount

  Scenario: Custom agents require an explicit command
    Given a custom hosted agent without a command
    When the configuration is normalized for host intent
    Then semantic validation fails for agent.command mentioning requires a non-empty
