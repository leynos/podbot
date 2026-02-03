Feature: Container engine connection

  The podbot CLI connects to Docker or Podman via socket endpoints resolved
  from configuration, environment variables, or platform defaults.

  Scenario: Socket resolved from DOCKER_HOST when config is not set
    Given no engine socket is configured
    And DOCKER_HOST is set to unix:///var/run/docker.sock
    When the socket is resolved
    Then the resolved socket is unix:///var/run/docker.sock

  Scenario: Config socket takes precedence over DOCKER_HOST
    Given engine socket is configured as unix:///config.sock
    And DOCKER_HOST is set to unix:///docker.sock
    When the socket is resolved
    Then the resolved socket is unix:///config.sock

  Scenario: Fallback to CONTAINER_HOST when DOCKER_HOST is not set
    Given no engine socket is configured
    And DOCKER_HOST is not set
    And CONTAINER_HOST is set to unix:///container.sock
    When the socket is resolved
    Then the resolved socket is unix:///container.sock

  Scenario: Fallback to PODMAN_HOST when higher-priority vars are not set
    Given no engine socket is configured
    And DOCKER_HOST is not set
    And CONTAINER_HOST is not set
    And PODMAN_HOST is set to unix:///podman.sock
    When the socket is resolved
    Then the resolved socket is unix:///podman.sock

  Scenario: Fallback to platform default when no sources are set
    Given no engine socket is configured
    And DOCKER_HOST is not set
    And CONTAINER_HOST is not set
    And PODMAN_HOST is not set
    When the socket is resolved
    Then the socket resolves to the platform default

  Scenario: Empty environment variable is skipped
    Given no engine socket is configured
    And DOCKER_HOST is empty
    And PODMAN_HOST is set to unix:///podman.sock
    When the socket is resolved
    Then the resolved socket is unix:///podman.sock

  Scenario: DOCKER_HOST takes priority over CONTAINER_HOST
    Given no engine socket is configured
    And DOCKER_HOST is set to unix:///docker.sock
    And CONTAINER_HOST is set to unix:///container.sock
    When the socket is resolved
    Then the resolved socket is unix:///docker.sock

  Scenario: CONTAINER_HOST takes priority over PODMAN_HOST
    Given no engine socket is configured
    And DOCKER_HOST is not set
    And CONTAINER_HOST is set to unix:///container.sock
    And PODMAN_HOST is set to unix:///podman.sock
    When the socket is resolved
    Then the resolved socket is unix:///container.sock

  # Health check scenarios
  # Note: These scenarios document the expected behaviour but require a running
  # container daemon for full integration testing. When no daemon is available,
  # these scenarios will be skipped.

  Scenario: Health check succeeds when engine is responsive
    Given a container engine is available
    When a health check is performed
    Then the health check succeeds

  Scenario: Health check fails when engine does not respond
    Given the container engine is not responding
    When a health check is attempted
    Then a health check failure error is returned

  Scenario: Health check times out on slow engine
    Given the container engine is slow to respond
    When a health check is attempted
    Then a health check timeout error is returned

  # Socket permission error scenarios

  Scenario: Permission denied error provides actionable guidance
    Given a socket path that requires elevated permissions
    When a connection is attempted
    Then a permission denied error is returned
    And the error message includes the socket path

  Scenario: Socket not found error provides actionable guidance
    Given a socket path that does not exist
    When a connection is attempted
    Then a socket not found error is returned
    And the error message includes the socket path
