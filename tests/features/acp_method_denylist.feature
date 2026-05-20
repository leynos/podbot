Feature: ACP runtime method denylist

  Podbot must reject Agentic Control Protocol (ACP) capability methods that
  the hosted agent attempts after initialization, returning a synthesized
  JSON-RPC error response and recording each denial on stderr without
  forwarding the request to the host.

  Scenario: Blocked request returns a synthesized error and is not forwarded
    Given the ACP runtime adapter is configured with the default denylist
    When the agent emits a "terminal/create" request with id 7
    Then host stdout receives no bytes from the blocked request
    And container stdin receives a synthesized JSON-RPC error with id 7

  Scenario: Permitted method passes through unchanged byte-for-byte
    Given the ACP runtime adapter is configured with the default denylist
    When the agent emits a "session/new" request with id 1
    Then host stdout receives the permitted frame verbatim
    And container stdin receives no synthesized response

  Scenario: Blocked notification is dropped silently
    Given the ACP runtime adapter is configured with the default denylist
    When the agent emits an "fs/changed" notification
    Then host stdout receives no bytes from the blocked notification
    And container stdin receives no synthesized response

  Scenario: Frame split across two chunks reassembles before the policy applies
    Given the ACP runtime adapter is configured with the default denylist
    When the agent emits a blocked frame split across two output chunks
    Then host stdout receives no bytes from the blocked request
    And container stdin receives a synthesized JSON-RPC error with id 2

  Scenario: Permitted frame following a blocked frame still flushes correctly
    Given the ACP runtime adapter is configured with the default denylist
    When the agent emits a blocked request followed by a permitted request
    Then host stdout receives only the permitted frame verbatim
    And container stdin receives a synthesized JSON-RPC error for the blocked request
