Feature: ACP capability masking

  ACP hosting must remove host-delegated terminal and filesystem capabilities
  from the initialize request before the request reaches the sandboxed agent.

  Scenario: ACP initialize masks blocked capabilities before forwarding
    Given ACP stdin contains an initialize request with blocked capabilities and a follow-up request
    When ACP stdin forwarding runs
    Then ACP stdin forwarding succeeds
    And the forwarded ACP stdin matches the expected bytes

  Scenario: Malformed ACP initialize is forwarded unchanged
    Given ACP stdin contains malformed initialize bytes
    When ACP stdin forwarding runs
    Then ACP stdin forwarding succeeds
    And the forwarded ACP stdin matches the expected bytes

  Scenario: ACP initialize without blocked capabilities stays unchanged
    Given ACP stdin contains initialize without blocked capabilities
    When ACP stdin forwarding runs
    Then ACP stdin forwarding succeeds
    And the forwarded ACP stdin matches the expected bytes
