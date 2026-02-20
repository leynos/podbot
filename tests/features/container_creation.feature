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

  Scenario: Create container uses image resolved from configuration
    Given resolved configuration provides sandbox image ghcr.io/example/podbot-sandbox:v2
    And sandbox security is minimal mode with /dev/fuse mounted
    When container creation is requested
    Then container creation succeeds
    And container image ghcr.io/example/podbot-sandbox:v2 is forwarded from resolved configuration

  Scenario: Create container fails when resolved image is missing
    Given resolved configuration has no sandbox image
    And sandbox security is minimal mode with /dev/fuse mounted
    When container creation is requested
    Then container creation fails with missing image error
    And container engine is not invoked

  Scenario: Create container fails when resolved image is whitespace only
    Given resolved configuration sandbox image is whitespace only
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

  Scenario: Privileged mode ignores /dev/fuse setting
    Given a configured sandbox image ghcr.io/example/podbot-sandbox:latest
    And sandbox security is privileged mode without /dev/fuse
    When container creation is requested
    Then container creation succeeds
    And privileged host configuration is used

  Scenario: Privileged mode ignores SELinux override
    Given a configured sandbox image ghcr.io/example/podbot-sandbox:latest
    And sandbox security is privileged mode with SELinux label disable
    When container creation is requested
    Then container creation succeeds
    And privileged host configuration is used

  Scenario: Minimal mode with SELinux kept at default
    Given a configured sandbox image ghcr.io/example/podbot-sandbox:latest
    And sandbox security is minimal mode with /dev/fuse and SELinux defaults
    When container creation is requested
    Then container creation succeeds
    And minimal host configuration with /dev/fuse but without SELinux disable is used

  Scenario: Minimal mode without /dev/fuse omits capabilities
    Given a configured sandbox image ghcr.io/example/podbot-sandbox:latest
    And sandbox security is minimal mode without /dev/fuse mounted
    When container creation is requested
    Then container creation succeeds
    And minimal host configuration without capabilities is used

  Scenario: Sandbox config SELinux label mode passes through to container
    Given a configured sandbox image ghcr.io/example/podbot-sandbox:latest
    And sandbox config has selinux_label_mode set to keep_default
    When container creation is requested
    Then container creation succeeds
    And minimal host configuration with /dev/fuse but without SELinux disable is used
