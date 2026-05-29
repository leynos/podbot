Feature: Repository cloning end-to-end

  Drive the public clone_repository_into_workspace API against a real
  container started by testcontainers, so the success path is exercised
  through the same Bollard exec seam used in production.

  Scenario: Repository clone succeeds against a real container
    Given a sandbox container running with git installed
    And a local bare repository leynos/podbot has branch main
    And the container rewrites GitHub URLs to the local repository server
    And the git askpass helper path is "/usr/local/bin/git-askpass"
    And the workspace base directory is "/work"
    When repository cloning is requested for repository leynos/podbot on branch main
    Then repository cloning succeeds
    And the workspace at "/work" contains a git repository
    And the checked out branch in the workspace is "main"

  Scenario: Clone exec failure is reported against a real container
    Given a sandbox container running with git installed
    And the git askpass helper path is "/usr/local/bin/git-askpass"
    And the workspace base directory is "/work"
    When repository cloning is requested for repository leynos/missing on branch main
    Then repository cloning fails with an exec error
    And the workspace at "/work" does not contain a git repository
