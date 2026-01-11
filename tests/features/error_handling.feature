Feature: Root error handling

  Scenario: Successful operations return ok
    Given a successful operation
    When the result is inspected
    Then the outcome is ok

  Scenario: Missing configuration is reported clearly
    Given a missing configuration field github.app_id
    When the error is formatted
    Then the error message is missing required configuration: github.app_id

  Scenario: Container start failures include identifiers
    Given a container start failure for abc with message image missing
    When the error is formatted
    Then the error message is failed to start container 'abc': image missing
