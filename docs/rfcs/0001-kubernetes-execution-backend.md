# RFC 0001: Add a Kubernetes execution backend

## Preamble

- **RFC number:** 0001
- **Status:** Proposed
- **Created:** 2026-11-08
- **Scope:** Backend-neutral sandbox execution and Kubernetes-hosted agents.
- **Delivery plan:** [Development roadmap, phase 10](../roadmap.md#10-kubernetes-execution-backend).

## 1. Summary

Add Kubernetes as a first-class execution backend alongside the existing
Docker/Podman backend. Podbot remains both a command-line interface (CLI) and
an embeddable Rust library. Corbusier continues to own orchestration intent,
tenancy, policy, and durable application state; Podbot owns sandbox realization,
agent transport, workspace preparation, Model Context Protocol (MCP) wires,
and execution mechanics. Kubernetes schedules the resulting workloads.

The production target is a Corbusier application running in Kubernetes and
launching agent sandboxes as Kubernetes Pods through Podbot. Mounting a node's
container-engine socket into Corbusier does not satisfy this target.

Introduce a backend-neutral execution boundary before completing the pending
launch implementation. Preserve Docker/Podman behaviour through an adapter;
implement Kubernetes with `kube` and `k8s-openapi` behind an optional Cargo
feature. Start with one Pod per hosted session and an explicitly non-resumable
protocol transport. Graduate restart-safe hosting only after separate recovery
and conformance gates pass.

This RFC proposes work, not available functionality. Merging these documents
must not imply that either backend's complete hosted-agent workflow exists.

## 2. Problem and current state

Corbusier's [infrastructure design](https://github.com/leynos/corbusier/blob/main/docs/corbusier-design.md#832-cluster-architecture)
places Corbusier, agent backends, and tools on Kubernetes worker nodes. Its
[Podbot boundary](https://github.com/leynos/corbusier/blob/main/docs/adr-001-runtime-boundary-between-corbusier-and-podbot.md)
delegates runtime mechanics to Podbot. Podbot currently implements those
mechanics through Docker/Podman, not the Kubernetes application programming
interface (API).

The implementation review used Podbot commit
`84ebfb6c0b9c2cdd57bb40faf7b3ec9bc8ab2350`:

- `src/engine/connection/mod.rs` resolves engine sockets and returns Bollard
  clients. `ContainerCreator` in `create_container/mod.rs` exposes Bollard
  request, response, and error types; it is a testing seam, not a portable
  sandbox contract.
- `src/api/exec.rs` holds a Bollard client in `ExecContext`. Its existing
  public `ExecRequest` identifies a container with a string.
- `src/api/mod.rs` exposes implemented execution operations, but experimental
  `run_agent` currently validates inputs and GitHub credentials rather than
  completing the agent lifecycle. Listing, stopping, and token-daemon entry
  points still include compatibility stubs.
- `src/config/workspace.rs` defines clone-backed and host-mounted sources.
  `src/config/hosting.rs` assumes host-gateway or loopback MCP exposure.
- Protocol proxying, bounded buffers, and Agent Client Protocol (ACP)
  capability masking already have implementation and regression coverage.
  These behaviours should be reused, not rewritten for Kubernetes.

The existing design and ADRs supply the integration vocabulary, but their
status and implementation coverage must be checked individually. In particular,
[ADR 009](../adr-009-define-control-plane-observability-recovery-and-replay.md)
describes process-local recovery files. A file under a controller Pod's
`XDG_RUNTIME_DIR` cannot be the authoritative recovery store for replacement
controller Pods. This RFC proposes an extension, not an assertion that the
existing ADR already provides distributed recovery.

## 3. Goals, non-goals, and acceptance levels

### 3.1. Goals

Support Kubernetes from both delivery surfaces without exposing Kubernetes or
Bollard types in the new public contract. Preserve stream purity, explicit
capability enforcement, semantic errors, and secret isolation. Make launch,
stop, cleanup, and recovery outcomes observable and testable. Keep application
and agent workloads separately scheduled, with independent resource budgets.

### 3.2. Non-goals

The initial backend does not provision clusters, introduce custom resource
definitions (CRDs), require an operator or service mesh, implement a scheduler,
or promise multi-cluster scheduling. It does not provide Kubernetes parity for
arbitrary host mounts, privileged execution, nested Podman, or every hook-image
and helper-container topology. GPU scheduling, virtual-machine isolation,
transparent agent-process resurrection, and automatic replay of interrupted
agent actions remain outside this proposal's initial delivery.

### 3.3. Acceptance levels

Each level adds evidence; none may silently claim the next level's guarantees.

- **K0, execution proof:** A mock agent runs in a restricted Pod, exchanges
  exact protocol bytes, returns a classified outcome, and leaves no owned
  resources after explicit teardown. Connection loss interrupts the session.
- **K1, integrated hosted execution:** Clone-backed workspaces, expiring
  credentials, declared MCP helpers, hook control, and backend capability
  validation pass the shared hosted-session contract. Controller loss may
  still interrupt protocol continuity, which remains an advertised limitation.
- **K2, restart-safe hosting:** A replacement controller can reconcile the same
  live sandbox and safely re-establish supported protocol/control ownership.
  Unsupported recovery produces an explicit interrupted or abandoned outcome.
  No agent action or hook with an unknown outcome is blindly replayed.

K0 is the first implementation milestone. Corbusier must not advertise durable
Kubernetes-hosted sessions before K2 and the downstream integration gate pass.

## 4. Proposed architecture

### 4.1. Ownership and reuse boundary

Keep one launch model and one `HostedSession` contract, following
[ADR 001](../adr-001-define-the-stable-public-library-boundary.md),
[ADR 002](../adr-002-define-the-hosted-session-api-and-control-channel.md), and
[ADR 007](../adr-007-define-session-composition-and-artefact-materialization.md).
Introduce narrow internal ports for sandbox lifecycle, execution/transport,
workspace materialization, and observation. A small facade may compose them;
do not create a universal container-engine trait with unsupported methods.

Podbot-owned values describe requested images, commands, resource budgets,
workspace source, storage lifecycle, credential references, and required
capabilities. Provider adapters translate these values into engine requests or
Kubernetes resources. Existing Bollard-specific traits remain private adapter
implementation/test seams. Corbusier, CLI parsing, ACP policy, and launch
normalization must not depend on them or on Kubernetes API objects.

Backend adapters own provisioning and cleanup. The shared hosting layer owns
protocol policy, hook semantics, validation, and session events. A remote
session bridge, when introduced, implements this same contract rather than a
second orchestrator. Libraries use the caller's asynchronous runtime and return
stream handles; only the CLI connects them to process stdin/stdout/stderr.

The following proposed layout groups implementation by execution backend:

```plaintext
src/runtime/
  mod.rs                 # narrow internal contracts and composition
  model.rs               # Podbot-owned identities and capability values
  container/             # existing Bollard implementation behind an adapter
  kubernetes/            # client, resource planning, watches, and transport
```

_Figure 1: Proposed runtime module layout, not existing source files._

### 4.2. Stable identities and semantic outcomes

Separate logical `SessionId`, workspace identity, and execution generation from
physical runtime identity. A serializable, versioned runtime handle contains
backend kind, configured cluster identity, namespace, Pod name, Pod unique
identifier (UID),
container name, and execution generation where applicable. It contains no
credentials or serialized Kubernetes client configuration.

A Pod name alone is insufficient: Kubernetes gives replacement Pods different
UIDs, and does not move an existing Pod to another node.[^1] Use a fresh,
non-reused name for every execution generation. Reject discovery results that
mismatch the stored identity. Use UID preconditions for deletion. Attach/exec
has no equivalent atomic UID precondition; validate identity immediately before
connecting, never reuse names, and require a generation handshake for the
restart-safe bridge. Do not present a preflight UID check as an atomic
guarantee.

Distinguish scheduling, image-pull, volume-binding, admission, authentication,
authorization, API-unavailable, transport-lost, process-exited, and
outcome-unknown states. A transport error is not a successful process exit.
Declare which errors permit a retry, require reconciliation, or need an explicit
caller decision. Source errors remain available without exposing secrets.

### 4.3. Configuration and compatibility

Propose `runtime.backend = "container" | "kubernetes"`, with the existing
container backend as the compatibility default. Kubernetes configuration
selects an explicit namespace, trusted connection configuration, execution
profile, scheduling/resource defaults, and workspace storage policy. Names are
illustrative until the configuration task finalizes the schema.

Keep local kubeconfig authentication and in-cluster service-account
configuration distinct and deterministic. A Kubernetes selection must never
fall back to Docker/Podman on a permission or connection error. Kubeconfig and
its executable credential plugins are trusted operator input, not agent input.
Do not copy controller environment credentials through the agent allowlist.

Propose an optional `runtime-kubernetes` Cargo feature and an explicit
container-runtime feature preserving existing default behaviour. Keep `cli`
and `experimental` semantics intact. Feature availability and API maturity are
different concepts: enabling a provider must not imply that hosted APIs have
graduated to stable. Select compatible crate versions and Kubernetes API
features during implementation; test default, container-only,
Kubernetes-only library, and combined builds, including dependency-tree checks.

Preserve existing `ExecRequest` and engine-socket configuration for current
callers. Add runtime-aware request types or additive entry points under the
experimental boundary rather than reinterpreting old container identifiers.
Missing compiled features and unsupported capability combinations must fail
before resource mutation with field-specific diagnostics.

## 5. Kubernetes resource lifecycle

### 5.1. Session-scoped Pods

Use a namespaced Pod with `restartPolicy: Never` for each execution generation.
An initialization container prepares the repository; the agent and predeclared
helpers run in regular containers. Set explicit images, container identities,
security contexts, CPU/memory/ephemeral-storage requests and limits, startup
bounds, a termination grace period, and an optional session deadline.

Do not initially use a Deployment or a Job to recreate interactive agents.
Podbot must distinguish sandbox availability from permission to repeat an
agent's externally visible work. Controller reconciliation may rediscover an
existing Pod; it must not interpret a missing Pod as permission to launch a
replacement agent with the same execution identity.

Build typed resource plans, not arbitrary caller-supplied Pod YAML. Validate
required platform policy and capability compatibility before creation. Resource
creation is not a transaction: persist intent first, record each owned object,
and reconcile partial creation before retrying or compensating.

### 5.2. Idempotency, observation, and deletion

Derive resource names from opaque session and generation identities. Repeated
launch calls with the same idempotency key and plan digest resolve the same
intent; changed plans with the same key produce a conflict. After a lost API
response, inspect the named object and its ownership metadata before proceeding.
Labels support discovery, not authorization or proof of ownership.

Use namespace-scoped watches and bounded reconciliation work, rather than a
busy polling loop or one unbounded watcher per agent. Relist after watch gaps,
apply backoff with jitter, honour API throttling, and distinguish desired from
observed state. Bound event queues, logs, retained diagnostics, concurrent
launches, and retry budgets. Kubernetes events are diagnostics, not the durable
audit log.

Stop is idempotent, explicit, and asynchronous: request shutdown, wait within a
bound, delete with identity preconditions, and report remaining resources when
cleanup cannot finish. Dropping a Rust handle is not a reliable asynchronous
cleanup mechanism. Maintain run/session ownership metadata, a cleanup ledger,
and an expiry-based sweeper. Delete owned Secrets and ephemeral claims, but
never delete operator-supplied claims. Do not make session resources depend on
the lifetime of a replaceable Corbusier Pod through owner references.

## 6. Workspaces, credentials, and caches

### 6.1. Separate content source from storage

Keep repository source independent of storage allocation and retention. A
clone-backed workspace may use `emptyDir` for K0 or a PersistentVolumeClaim
(PVC) for retained working state. A later explicit existing-PVC option may
reuse a prepared workspace. Do not encode PVC selection as a fake Git source.

An `emptyDir` survives an individual container restart while the Pod remains,
but disappears with the Pod.[^2] A PVC preserves files subject to its storage
class and retention policy; it does not preserve the agent process or protocol
state. Document volume access modes, ownership, scheduling topology, size,
quotas, encryption expectations, and failure/debug retention. Initialize a
retained workspace idempotently and never overwrite an existing working tree
on a repeated initialization attempt.

Reject `host_mount` on the initial Kubernetes backend. A client-machine path is
not a worker-node path; silently turning it into `hostPath` would change the
trust boundary. Existing PVC references require explicit authorization and
same-namespace policy checks.

Keep compilation caches separate from mutable working trees. Offer only
explicit per-tenant/per-trust-domain cache reuse, with versioned cache keys,
capacity limits, and documented concurrency semantics. Never share an
unrestricted writable Cargo target tree across tenants or concurrent sessions.
An empty cache must affect performance, not correctness.

### 6.2. Credential delivery and rotation

The controller or an external trusted broker holds the GitHub App private key.
Agents receive only selected, scoped installation tokens and agent credentials.
Never place private keys, kubeconfig, controller service-account credentials,
or a node container socket in an agent Pod.

Start with session-scoped Secrets projected read-only into the containers that
need them. Require platform encryption at rest, restrictive Secret access,
redacted diagnostics, size limits, and deterministic cleanup. Secret references
are operator-authorized inputs; caller-supplied names are not sufficient
authorization to mount arbitrary namespace Secrets.

Share token acquisition and refresh scheduling with the container backend,
but abstract the delivery sink: local atomic file publication for Docker/Podman,
Secret updates for Kubernetes. Readers reopen the token file for each Git
operation. Refresh ahead of expiry and account for propagation delay; Secret
volume updates are eventually consistent and `subPath` mounts do not receive
updates.[^3] Environment variables do not provide a rotating-file contract.
Failure to refresh before expiry must produce a visible credential-expired
state, not repeated unauthenticated retries or a broader credential fallback.

Credential formats that agents mutate need a separate writable credential
scratch area with explicit expiry and cleanup. Do not claim that a read-only
projection alone supports refreshable OAuth caches. Prefer a narrow token
contract over copying an entire workstation home-directory credential family.

## 7. Protocol transport and hosted-session continuity

### 7.1. Initial non-resumable transport

Use the Kubernetes attach/exec streaming API through `kube`. Its attached
process exposes separate input, output, error, and status channels.[^4]
Keep terminal allocation disabled for ACP and app-server protocols. Extract
existing byte proxying, ACP masking, and blocked-method enforcement above the
Bollard-specific stream adaptation so both backends use the same rules.

For K0, select one documented launch pattern and test it end to end: a
restricted sandbox entry point waits while a single exec operation starts the
mock agent. The exec operation is a launch, not an attach-to-existing-process
primitive. It must never be retried after an uncertain response, because that
could start a second agent. Obtain the remote process status explicitly; a
completed WebSocket task alone is not a zero exit code.

Read stdout and stderr concurrently with bounded buffers and backpressure.
Preserve frame boundaries at the protocol layer without treating transport
chunks as frames. Cancellation and stdin closure have explicit semantics. A
transport disconnect reports interruption; it does not prove that the remote
process stopped. Terminate the sandbox explicitly when policy requires it,
and report an unknown outcome if termination cannot be verified.

K0 and K1 must advertise `reconnect = unsupported`. API-server attach/exec is
an initial transport, not the permanent durability architecture. Test stream
counts and API-server load before using it for a large agent fleet.

### 7.2. Restart-safe bridge and control channel

K2 introduces a small Pod-local supervisor/bridge that owns the agent process
and exposes authenticated, generation-bound session transport. It does not own
tenancy, policy decisions, or workflow scheduling. The host still receives the
same `HostedSession` protocol streams and typed control events.

Before implementing the bridge, ratify its transport, authentication,
controller-ownership fencing, bounded buffering, and disconnect policy.
Separate protocol data from lifecycle and hook control. Define replay cursors
for control events without inventing transparent replay of raw ACP requests.
A resumed TCP/WebSocket connection is not evidence that an interrupted agent
operation can safely execute again.

At most one controller may issue mutating session commands. The supervisor
must enforce a current ownership generation or equivalent fencing mechanism;
a renewable lease alone does not stop an old controller acting after a
partition. On ambiguity, suspend where the agent contract supports suspension,
or report interruption/abandonment. Never report exactly-once arbitrary tool
execution merely because hook acknowledgements are deduplicated.

The bridge and agent share a workload trust boundary. Do not store signing
keys or cluster authority in the bridge, or treat a sidecar as isolation from
other containers in the same Pod. Protocol continuity across a controller
restart is distinct from recovery after agent-process, Pod, or node loss.

## 8. MCP wires, hooks, and security policy

### 8.1. Kubernetes-native wire realization

Reuse the canonical MCP source model and hook contract rather than retaining a
Corbusier-side parallel tool runtime. Predeclare helper-container images and
volume access in the launch plan. Co-locate helpers that share the workspace
when that fits the declared access policy; the initial implementation does not
attach arbitrary new containers to an existing Pod after launch.

A source's `Stdio` command must specify its execution locus. On Kubernetes it
runs inside the sandbox/helper, not as an arbitrary subprocess in the
Corbusier controller. Direct Streamable HTTP sources remain remote endpoints.
Use Pod-local loopback for co-located bridges or an explicitly authorized
Service for separate workloads. Host-gateway defaults must not leak into the
Kubernetes realization.

Keep `RepoAccess::None` as the helper default; mount the workspace only when
explicitly requested, with read-only or read-write access per container.
Mount helper-specific secrets only into the relevant helper. Record helper
and wire lifecycle independently without treating mount permissions as a
complete boundary between mutually hostile processes sharing a Pod.

Support script hooks within the existing sandbox first. Hooks that require a
different container image need a declared execution topology and compatible
storage. Reject unsupported combinations before launch; do not silently run a
requested image hook in the agent's image. Dynamic helper replacement and
cross-Pod hook-image execution are later capabilities, not hidden fallbacks.

### 8.2. Namespace, authorization, and admission boundaries

Use platform-provisioned execution namespaces with separate trust domains or
tenants where isolation requires them. Keep Corbusier and signing credentials
outside agent namespaces. Agents use an unprivileged service account with
`automountServiceAccountToken: false`; the controller uses a distinct identity.

Provide a reviewed, namespaced Role containing only the operations used by the
selected profile. Expected resources include Pods and required stream/log
subresources, plus explicitly owned Secrets, claims, or Services. Watchers need
list/watch; transport authorization must match the selected client's actual
HTTP verbs. Bootstrap roles, namespaces, network policy, and quotas belong to
platform administration, not agent launch permissions.

Permission to create workloads can expose other mountable resources in the
namespace.[^5] Role-based access control (RBAC) alone therefore cannot
restrict arbitrary Pod specifications
to a safe service account or Secret. Use an allowlisted resource planner and
platform admission policy to constrain images, service accounts, secret/claim
references, security settings, scheduling fields, and namespace scope.

The initial Linux profile requires non-root execution, no privilege escalation,
dropped capabilities, a runtime-default seccomp profile, no host namespaces,
no host paths, and writable mounts only where required. Do not translate the
existing `/dev/fuse`, `SYS_ADMIN`, or SELinux-disable options into this profile.
Kubernetes Restricted Pod Security excludes these host-access/capability
patterns.[^6] Nested container execution requires a separate future capability
and platform approval; a node-pool label alone is not a security boundary.

### 8.3. Networking and resource admission

Require platform-installed ingress and egress policy before launching agents.
Allow DNS and only explicitly approved control, model, repository, cache, and
MCP paths. Standard NetworkPolicy operates on network identities and addresses,
not arbitrary domain allowlists, and requires an enforcing network plugin.[^7]
Use a reviewed egress proxy or supported network-plugin policy for domain-based
rules. Test denied cross-tenant and metadata/API access; do not infer
enforcement
from the mere existence of a NetworkPolicy object.

Namespace quotas and limit defaults complement per-session resource budgets.
Podbot bounds concurrent provisioning and reports quota/admission rejection
without spinning. Kubernetes owns placement; Podbot reports pending and
unschedulable states instead of creating duplicate workloads. Prefer separate
controller and agent resource profiles so compilation cannot consume the
control plane's entire budget.

## 9. Persistence, recovery, and observability

Expose a minimal runtime-state-store seam for intent, physical identities,
execution generations, ownership, pending hooks, event cursors, and cleanup.
An embedder may implement it using its database; Podbot must not acquire a
Corbusier-specific database dependency. A standalone file implementation may
serve single-host use, but Kubernetes multi-replica recovery requires a shared
durable store with compare-and-set ownership semantics.

The embedding host explicitly starts and supervises reconciliation tasks.
No hidden immortal library daemon is required. Reconciliation compares durable
intent with observed Kubernetes state, restores safe subscriptions, advances
cleanup, and returns typed recovery outcomes. Losing a watch does not mean
losing a Pod. Losing a Pod or process does not authorize repeating its work.

Revise ADR 009 during acceptance to distinguish local file recovery from shared
state, and interrupted execution from known failure. Persist hook intent before
execution and outcome before acknowledging completion. Deduplicate decisions
by invocation identity. An unknown external side effect requires reconciliation
or caller judgement, not automatic rerunning under an exactly-once label.

Emit structured tracing and low-cardinality metrics for launch latency,
pending reasons, API retries, active sessions, stream backpressure, credential
refresh, recovery, and cleanup. Put session/tenant identifiers in
access-controlled
traces, not metric labels. Bound and redact diagnostic captures, and avoid
storing secret bytes or complete prompts in Kubernetes metadata.

## 10. Delivery, compatibility, and validation

### 10.1. Dependency-aware migration

[Roadmap phase 10](../roadmap.md#10-kubernetes-execution-backend) owns the new
work. Its numbering preserves existing references; it is not deferred behind
phase 9. The runtime abstraction and compatibility work precede unfinished
agent launch tasks. Move launch-model definition ahead of concrete launch
implementation to avoid a dependency cycle and prevent a newly fixed
Docker-only API from becoming the integration contract.

Leave completed work and standalone container defaults intact. Ratification
must update the primary design and relevant ADRs before implementation proceeds.
New hosted/provider surfaces remain experimental until conformance and release
review justify graduation. Do not retrofit a previously completed task's
checkbox to claim that it covered Kubernetes.

### 10.2. Tests and release gates

Use shared contract tests for both providers and focused adapter tests. Cover
configuration precedence, unsupported capabilities, resource-plan snapshots,
identity serialization, error classification, and restricted Pod specifications.
Property tests should exercise launch idempotency, resource ownership, stale
identities, and state transitions; bounded model checking is appropriate for
ownership/recovery interleavings where it gives useful assurance.

Mock Kubernetes API tests must include create-response loss, already-existing
resources, authorization rejection, throttling, expired watches, and partial
cleanup. Mock transport tests cover framing, bounded memory, simultaneous
stderr/stdout, missing status, cancellation, and connection loss without retry.

Provide explicitly invoked k3d/kind end-to-end tests using a pinned test image
and deterministic mock agent/provider. The default unit-test suite must not
need a live cluster, a paid model endpoint, or private credentials. A separate
opt-in credential test may use a narrowly scoped disposable repository.
Cluster preflight verifies the supported API version, storage, admission,
network enforcement, permissions, and images before any scenario starts.

K2 fault injection must cover controller termination, competing controllers,
lease loss, stale fencing tokens, pending hook acknowledgements, Secret
rotation, eviction, node loss, UID mismatch, and cleanup after partial launch.
Verify that no uncertain mutation is replayed, no retained workspace is deleted,
and no stale controller can mutate a recovered session.

Corbusier adoption requires a separate linked change in its repository:
configure runtime selection, supply durable state and ownership, add deployment
RBAC/profile wiring, and exercise a real Podbot library launch from the running
application. This Podbot pull request does not modify Corbusier or declare
that integration
complete. Record pinned commits and end-to-end evidence at the adoption gate.

### 10.3. Rollback

Keep existing container-only builds and configuration usable throughout.
Disable new Kubernetes admission independently from reconciling or draining
existing sessions. Do not migrate a live session between backends. Rollback
must retain enough schema compatibility to inspect and clean up existing
resources; incompatible state requires an explicit migration or refusal.

## 11. Alternatives considered

A Docker socket mounted into the Corbusier Pod avoids a new adapter but leaves
agent workloads outside Kubernetes scheduling and violates the target boundary.
A separate Kubernetes launcher inside Corbusier duplicates Podbot's hosting and
policy-enforcement mechanics. Making Kubernetes implement Bollard-shaped traits
conceals incompatible storage, lifecycle, and security semantics.

A CRD/operator could later provide a useful durable desired-state surface, but
it is not required to establish the backend contract. A per-session Deployment
or retrying Job delegates process recreation before the product defines safe
re-execution. A bridge in the very first proof increases the amount of new
protocol code needed before validating basic execution. The proposed staged
backend avoids these costs while retaining an explicit K2 durability gate.

## 12. Decisions required before implementation

Ratification must settle the public versus internal runtime types, capability
matrix, existing-API compatibility strategy, and supported Kubernetes/crate
version matrix. Before K1, select the storage/retention profile, credential
broker contract, helper topology, and platform network/admission prerequisites.
Before K2, settle the bridge wire protocol, authentication, controller fencing,
state-store transactions, and recovery outcomes for each supported agent.

These are bounded design gates in the roadmap, not permission to silently
choose unsafe defaults. An unresolved capability remains unsupported.

## 13. Recommendation

Adopt a backend-neutral Podbot runtime boundary and deliver Kubernetes in
measurable stages. Preserve the existing Docker/Podman implementation, make
Kubernetes policy and lifecycle semantics explicit, and require independent
evidence for protocol continuity and Corbusier adoption. Kubernetes should own
workload execution without becoming a second owner of agent workflow policy.

[^1]: Kubernetes, [Pod lifecycle](https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/).
[^2]: Kubernetes, [Volumes](https://kubernetes.io/docs/concepts/storage/volumes/).
[^3]: Kubernetes, [Secrets](https://kubernetes.io/docs/concepts/configuration/secret/).
[^4]: kube-rs, [AttachedProcess](https://docs.rs/kube/latest/kube/api/struct.AttachedProcess.html).
[^5]: Kubernetes, [RBAC good practices](https://kubernetes.io/docs/concepts/security/rbac-good-practices/).
[^6]: Kubernetes, [Pod Security Standards](https://kubernetes.io/docs/concepts/security/pod-security-standards/).
[^7]: Kubernetes, [Network policies](https://kubernetes.io/docs/concepts/services-networking/network-policies/).
