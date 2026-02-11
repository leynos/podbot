Feature: Container creation

  Podbot creates sandbox containers with security settings that balance
  compatibility and isolation.

  Scenario: Create container in privileged mode
    Given a configured sandbox image ghcr.io/example/podbot-sandbox:latest
    And sandbox security is privileged mode
    When container creation is requested
    Then container creation succeeds
    And privileged host configuration is used

  Scenario: Create container in minimal mode with /dev/fuse
    Given a configured sandbox image ghcr.io/example/podbot-sandbox:latest
    And sandbox security is minimal mode with /dev/fuse mounted
    When container creation is requested
    Then container creation succeeds
    And minimal host configuration with /dev/fuse is used

  Scenario: Create container in minimal mode without /dev/fuse
    Given a configured sandbox image ghcr.io/example/podbot-sandbox:latest
    And sandbox security is minimal mode without /dev/fuse mounted
    When container creation is requested
    Then container creation succeeds
    And minimal host configuration without /dev/fuse is used

  Scenario: Create container fails when image is missing
    Given no sandbox image is configured
    And sandbox security is minimal mode with /dev/fuse mounted
    When container creation is requested
    Then container creation fails with missing image error
    And container engine is not invoked

  Scenario: Create container fails when image is whitespace only
    Given sandbox image is configured as whitespace only
    And sandbox security is minimal mode with /dev/fuse mounted
    When container creation is requested
    Then container creation fails with missing image error
    And container engine is not invoked

  Scenario: Create container surfaces engine create failures
    Given a configured sandbox image ghcr.io/example/podbot-sandbox:latest
    And sandbox security is minimal mode with /dev/fuse mounted
    And the container engine create call fails
    When container creation is requested
    Then container creation fails with create failed error
