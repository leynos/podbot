Feature: Repository cloning

  Clone a requested GitHub repository into the configured sandbox workspace
  using GIT_ASKPASS for authentication.

  Scenario: Repository clone succeeds
    Given repository input is "leynos/podbot"
    And branch input is "main"
    And workspace base directory is "/work"
    And git askpass helper path is "/usr/local/bin/git-askpass"
    And repository clone execs will succeed
    When repository cloning is requested for container sandbox-clone
    Then repository cloning succeeds
    And the workspace path is "/work"
    And the checked out branch is "main"
    And the clone command used GIT_ASKPASS

  Scenario: Malformed repository input fails before exec
    Given repository input is "leynos"
    And branch input is "main"
    And workspace base directory is "/work"
    And git askpass helper path is "/usr/local/bin/git-askpass"
    When repository cloning is requested for container sandbox-clone
    Then repository cloning fails with a configuration error
    And no repository clone exec was attempted

  Scenario: Clone exec failure is reported
    Given repository input is "leynos/podbot"
    And branch input is "main"
    And workspace base directory is "/work"
    And git askpass helper path is "/usr/local/bin/git-askpass"
    And repository clone exec will fail
    When repository cloning is requested for container sandbox-clone
    Then repository cloning fails with an exec error

  Scenario: Branch verification failure is reported
    Given repository input is "leynos/podbot"
    And branch input is "main"
    And workspace base directory is "/work"
    And git askpass helper path is "/usr/local/bin/git-askpass"
    And repository branch verification will fail
    When repository cloning is requested for container sandbox-clone
    Then repository cloning fails with an exec error
