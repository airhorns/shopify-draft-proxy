# Parity runner

The parity runner drives every recorded scenario in `config/parity-specs/**`
through `draft_proxy.process_request` and compares the response (and, where
applicable, the proxy's emitted state and log) against the slice of the
captured reference response named by the spec. For Shopify-fidelity claims, that
reference response must be a real Shopify Admin GraphQL capture from a registered
capture script. Proxy-generated, snapshot, runtime-test, or hand-authored output
is only a proxy regression guard; it is not Shopify parity evidence.

> **Anti-forgery rule:** do not add or rename non-Shopify artifacts so they look
> like captured parity/conformance evidence. A fixture, spec, cassette, or
> expected payload sourced from the proxy itself, a generator, a runtime test,
> a snapshot, an edited old response, or a guess is not Shopify evidence no
> matter what directory, status field, or assertion kind it uses. If live capture
> is blocked, document the blocker and add focused runtime tests instead of
> making the conformance corpus count local output as captured Shopify evidence.

This document describes the **cassette-playback** model. The previous
seed-based model (where the parity runner pre-wrote into `base_state`,
`staged_state`, or private setup hooks to fake the proxy into knowing about
resources it shouldn't yet have known about) is unsupported.

## Model

A scenario consists of:

1. A **spec** in `config/parity-specs/<domain>/*.json` that names the
   GraphQL document, variables, and the targets (response paths /
   matchers / etc.) the runner compares.
2. A **capture** in `fixtures/conformance/<store>/<api-version>/**/*.json`
   that holds the real Shopify response the proxy is being graded against,
   plus an `upstreamCalls` cassette of upstream traffic the proxy made while
   serving the captured request.

The runner configures the proxy in live-hybrid mode and installs a cassette
upstream transport through the TypeScript shim around the Rust runtime.
Operation handlers may use their injected upstream transport when they need
information they do not have locally; those calls are matched against the
cassette and served deterministically. **Mutations stage their effects
locally**; upstream is never written to from a parity test.

There is **no per-domain "hydrate from upstream" pass** and no uniform
"mirror upstream into local state." Each operation handler decides, on
its own, whether it needs an upstream read and what to do with the
result. See _Per-operation upstream access_ below.

### Apps billing test-charge activation

Real Shopify app billing charges remain pending until the merchant opens
the billing confirmation URL. For local parity and agent flows, the apps
handler treats `appSubscriptionCreate(test: true)` and
`appPurchaseOneTimeCreate(test: true)` as accepted test charges and stages
them with `status: "ACTIVE"` immediately. This does not write to upstream;
the synthetic confirmation URL is still returned for shape fidelity.

Activated test subscriptions are added to
`AppInstallation.activeSubscriptions` and receive a deterministic
`currentPeriodEnd` from the activation timestamp plus the line item's
billing interval and `trialDays`. This local billing activation behavior is
runtime-test-backed in `tests/graphql_routes/admin_app.rs`; it is not kept as
captured parity evidence until a billing-capable disposable app can record the
same lifecycle from Shopify.

## Spec shape

```jsonc
{
  "scenarioId": "customer-detail-parity-plan",
  "liveCaptureFiles": ["fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/customers/customer-detail.json"],
  "proxyRequest": {
    "documentPath": "config/parity-requests/customers/customer-detail.graphql",
    "apiVersion": "2025-01",
    "variables": { "id": "gid://shopify/Customer/123" },
  },
  "comparison": {
    "targets": [{ "name": "primary", "capturePath": "$.cases[0].response.payload", "proxyPath": "$" }],
  },
}
```

`comparison.mode` is the comparison contract for the target payloads. It
is distinct from proxy runtime read mode, which the runner owns.

Parity specs must not include `proxyRequest.localSetups` or any other
runner setup hook that pre-seeds proxy state before the request is
executed. If the proxy needs existing Shopify state to answer a request,
the operation handler must model that state from earlier scenario
requests or from cassette-backed upstream reads. If that is not
implemented yet, keep the gap out of the checked-in parity spec and
track the missing fidelity work outside the scenario corpus.

### Managing Setup State

Setup state for parity is earned through the same public request surface an
app or capture script would use:

- run setup GraphQL mutations/queries as explicit scenario requests when the
  setup is part of the behavior under test;
- let operation handlers issue the upstream reads they genuinely need, then
  record those reads under `upstreamCalls` with `pnpm parity:record`.

Do **not** hand-synthesize cassette entries from checked-in responses, local
proxy output, generated/snapshot data, or guessed payloads. If an older fixture
cannot be replayed against current live Shopify, either re-record it with
realistic setup/cleanup, retire it with replacement runtime tests, or leave the
blocker in the workpad.

Do **not** add or restore hidden setup-state files, `baseState` /
`stagedState` JSON, `proxyRequest.localSetups`, test-only state importers, or
`DraftProxy` store patching to make a scenario pass. Those shortcuts bypass the
runtime behavior being graded and turn parity into a fixture echo.

`POST /__meta/dump`, `POST /__meta/restore`, `dumpState`, and `restoreState`
remain legitimate meta API/runtime test surfaces for the features that expose
them. They are not a parity setup mechanism unless the scenario is specifically
testing those documented meta APIs. The parity runner may use dump/restore
internally to reset its own baseline between requests; specs and capture files
must not depend on those internals to smuggle resource data into the proxy.

## Cassette shape

The cassette lives inside the same capture file the spec already points
at, under a top-level `upstreamCalls` array:

```jsonc
{
  "cases": [
    /* … existing captured cases … */
  ],
  "upstreamCalls": [
    {
      "operationName": "CustomerById",
      "variables": { "id": "gid://shopify/Customer/123" },
      "query": "query CustomerById($id: ID!) { customer(id: $id) { id email } }",
      "response": {
        "status": 200,
        "body": { "data": { "customer": { "id": "…", "email": "…" } } },
      },
    },
  ],
}
```

Match key: exact recorded `query` text (after trailing-whitespace
normalization) plus exact variables using **object-key-order-insensitive**
comparison. `operationName` is diagnostic metadata only; it must never be used
as a fallback match key. A cassette `query` must be the exact GraphQL document
text sent upstream, not `sha:...`, `hand-synthesized ...`, `generated by ...`,
`recorded by ...`, or any other provenance descriptor.

A request that misses every cassette entry is a **hard failure** —
`commit.CommitTransportError("cassette miss: operation=<name> variables=
<json>")` so tests name exactly which call wasn't recorded. Re-record
the cassette to fix.

## Per-operation upstream access

There are **two patterns** for reaching upstream. Pick the simpler one
that fits the operation; do not over-engineer.

### Pattern 1 — Force passthrough in LiveHybrid (the simple default)

For read operations where the proxy has nothing local to add (the
captured response is exactly what upstream returns and the proxy's job
is just to forward), route the root through the Rust live-hybrid
passthrough path in `src/proxy.rs`.

When this returns `True`, `dispatch_graphql` forwards the entire
GraphQL document verbatim to the upstream transport (cassette in
tests, real HTTP in production). The proxy adds nothing to the
response; cassette replay is trivially correct because the cassette
captured the same response the spec is being graded against.

`Snapshot` mode is unaffected. Passthrough branches should only fire in
live-hybrid mode; the local handler keeps serving Snapshot reads.

#### **Almost always gate passthrough on local state**

Unconditional passthrough regresses lifecycle scenarios that stage or delete
records, because passthrough forwards proxy-synthetic gids upstream (where they 404) and bypasses the empty/null answer the test expects after a delete.
Existing domains demonstrate the pattern by checking staged state before
falling back to passthrough. Keep those local-state predicates close to the
domain logic so they are reviewed with the lifecycle behavior they protect.

Two important details:

1. **Scan every string variable, not just `$id`.** GraphQL operations
   frequently rebind the argument under a different variable name
   (e.g. `discountNode(id: $codeId)`). Keying off `dict.get(variables,
"id")` will silently fail on those operations. The existing
   `customers.local_has_customer_id` helper happens to work only
   because every customer query in the corpus uses `$id` literally;
   don't replicate the pattern verbatim.
2. **Two flavors of gate**: an "id-keyed" check (passthrough off when
   the requested id is staged or synthetic, used by `*Node` lookups)
   and a "domain-has-staged-records" check (passthrough off when _any_
   record is staged, used by connection / aggregate / by-code reads).
   Use the right one per operation; don't substitute one for the
   other.

This is the right pattern for: `*Node` lookups, list/connection reads
where the proxy has no local writes layered on, count aggregates, and
anything else where the proxy is a transparent forwarder.

### Pattern 2 — Per-handler `upstream_query.fetch_sync`

For operations that genuinely need to do something with the upstream
response — merge it with staged state, persist a slice into
`base_state`, fan out into multiple staged records, or compute a reply
that isn't just the upstream response verbatim — the handler issues
its own narrow upstream call via the `proxy/upstream_query.fetch_sync`
chokepoint.

The canonical case is a mutation that reads the prior record before
staging: `customerUpdate` fetches the existing customer (so the merged
result has fields the request didn't touch), then stages the update
locally. The fetch is captured in the cassette; the stage is not (it's
a local effect).

There is **no** uniform `hydrate_*_from_upstream_response` pattern.
Each operation decides, per-operation:

- what minimal slice it needs (one ID-keyed read, a narrow filter, the
  prior record so a mutation can merge fields);
- whether to persist the result into `base_state` (so the same operation
  called again doesn't re-fetch and so the staged-state overlay can layer
  on top), or whether to use it transiently for this one reply;
- what `Snapshot` mode does — typically: serve from local state if
  present, otherwise return null/empty.

Reads can fetch upstream. **Mutations can fetch upstream too.** Mutations
still stage their side effect locally; what they may also do is read
from upstream first.

The choice for each operation is a short inline comment next to the
handler explaining what it fetches, why, and what it does in `Snapshot`
mode.

### Picking between the two

| Situation                                                         | Pattern                       |
| ----------------------------------------------------------------- | ----------------------------- |
| Read returns upstream verbatim, proxy adds nothing                | 1                             |
| Read where staged local writes must overlay                       | 1, gated on local-state check |
| Mutation that needs the prior record to merge                     | 2                             |
| Operation that persists a slice for future reads                  | 2                             |
| Operation whose response can't be computed from one upstream call | 2                             |
| Aggregate / count operations                                      | 1                             |

Start with pattern 1. Reach for pattern 2 only when the response can't
be served by forwarding verbatim.

## Running

```sh
# Run every checked-in parity spec.
corepack pnpm parity:run

# Dry-run discovery without executing scenarios.
corepack pnpm parity:run -- --dry-run

# Run one scenario by ID.
corepack pnpm parity -- <scenario-id>

# Run one spec file.
corepack pnpm parity -- --spec config/parity-specs/products/product-empty-state-read.json
```

The parity CLI discovers every spec and treats any runner error or comparison
mismatch as a hard failure. A spec without a valid `upstreamCalls` cassette
does not run in a degraded mode; it fails until the capture is repaired.

## Debugging a single scenario

When a scenario fails and `first_line(message)` in the gate summary
isn't enough to figure out why, the runner has a debug mode that
streams every request, response, cassette match/miss, and per-target
assertion result to stderr.

Run the scenario with debug output:

```sh
corepack pnpm parity -- --debug --spec config/parity-specs/customers/customerInputValidation-parity.json
```

The output is line-prefixed:

- `[runner] mode=… cassette_entries=N` — proxy build for this scenario.
- `[runner] -> <ctx> query|mutation Name roots=[…] vars=…` — outgoing
  GraphQL request the runner sent to the proxy.
- `[runner] <- <ctx> status=… body=…` — proxy's response.
- `[cassette] HIT  op=… vars=… -> status=… body=…` — operation handler
  consulted upstream and the cassette had a match.
- `[cassette] MISS op=… vars=… -> <message>` — handler asked upstream
  but the cassette has no recording. Either re-record, or the handler
  needs `upstream_query.fetch` wiring.
- `[runner] target=… compared mismatches=N` — assertion result for
  each target. Each mismatch is followed by `at <path> / expected: …
/ actual: …` lines (truncated at 200 chars).

Long values are truncated with `…`. Delete the inspector when done —
it's not part of the suite.

`run_debug` is a thin convenience over
`runner.run_with_config(runner.with_debug(runner.default_config()),
spec_path)` if you want to compose other config knobs.

## Recording

Re-recording a cassette is a human/agent-driven action that hits real
Shopify. CI never re-records.

```sh
# Single scenario:
pnpm parity:record customer-detail-parity-plan

# Every scenario:
pnpm parity:record --all
```

> **Note on `corepack pnpm` vs `pnpm`.** AGENTS.md prefers `corepack
pnpm` for unattended/CI envs, but on local dev boxes `corepack pnpm`
> may error with "no longer supported in a global context". If you hit
> that, drop the `corepack` prefix — bare `pnpm` works wherever it's
> on `PATH`.

The recorder boots an in-memory `DraftProxy` in live-hybrid mode against real
Shopify, plays the spec's primary and targets through it, intercepts every
upstream call the operation handlers issue, and writes the result into the
capture file's `upstreamCalls` field.

Credentials come from the existing OAuth flow:
`pnpm conformance:auth-link`, `pnpm conformance:exchange-auth`,
`pnpm conformance:probe`. Stored in `~/.shopify-draft-proxy/`.

### Recorder hits the env-configured store, not the per-fixture store

The recorder reads `SHOPIFY_CONFORMANCE_ADMIN_ORIGIN` and runs the live query
there. Many fixtures in this repo were captured against older stores (e.g.
`very-big-test-store.myshopify.com`) but the OAuth token that's currently linked
points at a different store (e.g. `harry-test-heelo.myshopify.com`). When the
recorder targets a store that doesn't have the same data the original capture was
against — or worse, when the captured query references a Shopify schema field
that's since been removed (e.g. `DiscountAutomaticBasic.context`) — the live
recording can produce `{ errors: [{undefinedField}] }` or `wrote 0
upstreamCalls`.

That is a capture blocker, not permission to forge. Do **not** wrap the old
checked-in response as a new cassette entry, do **not** use a `sha:` placeholder,
and do **not** copy proxy-generated, snapshot, runtime-test, or guessed output
into conformance evidence paths. The right choices are:

1. make the capture script create realistic setup data on the current disposable
   store and clean it up;
2. re-record the scenario against a store/API version where the request is still
   valid;
3. retire the stale parity scenario and replace the implementation guard with
   focused Rust/unit/runtime tests; or
4. leave a clear blocker in the workpad/Linear issue.

Do not make the conformance corpus count local or hand-synthesized payloads as
captured Shopify parity evidence.

### Stale `expectedDifferences` rules become hard failures

Once a scenario is on cassette playback, **stale
`expectedDifferences` rules become hard failures, not silent
permissions.** The runner asserts each rule was satisfied — if a rule
declares "expect a difference at `$.events.pageInfo.endCursor`" but
the proxy now emits the upstream cursor verbatim (because pattern 1
forwards it), the test panics with `expectedDifference rule was not
satisfied`.

When migrating a read scenario, expect to delete cursor /
`pageInfo` / `cursor` rules that were originally needed because the
seed-based runner emitted synthetic cursors. AGENTS.md bans _adding_
new `expectedDifferences`; it does not ban removing stale ones.

### Evolving a hydration query's response shape — keep parsers additive

Cassette matching is exact-query plus exact-variables. If two scenarios use the
same handler-side hydrate query and you evolve that query's selection set, the
older scenario's cassette will miss until it is re-recorded with the same query
text. Do not “fix” that miss by falling back to `operationName` or a descriptor
query; that reintroduces false passes.

Prefer additive parsing and additive query evolution where possible: read new
keys first, fall back to old, or keep old aliases alongside new aliases so both
old and new response shapes can be parsed while you re-record the affected
scenario set. Concretely, if you rewrite `data.codeDiscountNode` to `data.codeNode`
/ `data.automaticNode` aliases, parse with
`option.or(non_null_node(json_get(data, "codeDiscountNode")))` so the legacy
cassette shape still resolves until it is refreshed.

When the operation is genuinely scenario-specific, give it a scenario-specific
name (`DiscountHydrateForBulkAppFlow`) for diagnostics, but remember the
operation name is not a cassette match key.

## Adding Or Repairing Coverage

Per-scenario steps:

1. `pnpm parity:record <id>` — records the cassette into the capture file.
   - **If the recorder reports `wrote 0 upstreamCalls`** AND the live
     recording succeeded (no errors visible): the operation handler
     isn't reaching upstream at all. Continue to step 2.
   - **If the live recording produced GraphQL errors** (e.g.
     `undefinedField`, schema drift, missing data): stop and fix the live
     capture/setup/API-version problem, or retire the stale scenario with
     replacement runtime tests. Do not synthesize a cassette from the old
     captured response.
   - **If it wrote N>0 upstreamCalls cleanly**, the cassette is
     populated; skip to step 3.
2. Decide the pattern (1 or 2 above) and wire the operation:
   - Pattern 1: add the root field to the live-hybrid passthrough branch in
     `src/proxy.rs`. **Gate it on local state** when staged or deleted rows can
     affect the same root.
   - Pattern 2: use the injected upstream transport inside the domain handler
     and document the choice inline.
   - Re-run `pnpm parity:record <id>` and confirm it writes the expected
     `upstreamCalls` entries. An empty array is valid for mutation-only
     scenarios that make no upstream reads.
3. `corepack pnpm parity -- --spec <path>` to run the repaired spec. If you see:
   - `cassette miss: operation=<X>` — the operation made an upstream
     call we didn't record. Re-record or extend the cassette.
   - `expectedDifference rule was not satisfied at <path>` — a stale
     `expectedDifferences` rule. Delete the rule from the spec.
   - parity diff — the operation isn't computing the right response.
     Adjust the handler.
4. **Verify with targeted Rust and parity tests.** The TypeScript shim should
   only adapt the public package surface around the Rust runtime.
5. **Watch for collateral regressions.** Adding an unconditional or
   incorrectly-gated passthrough branch can regress lifecycle
   scenarios in _other_ specs that touch the same root fields. After
   each handler change, run the full domain's tests (or the whole
   suite) — not just your scenario.

## Seed Keys Are Forbidden

Capture files must not carry top-level `seedProducts`, `seedCustomers`,
`seedDiscounts`, runtime-case imports, or similar artificial setup keys.
Parity specs must not carry `localSetups`, `baseState`, `stagedState`,
`setupState`, or equivalent private state payloads. Those keys were inputs to
the unsupported seed-based runner. Seed-style fixture and spec keys remain
banned by policy and should be removed rather than expanded. If a scenario
needs a precondition resource, create it through public GraphQL requests or
hydrate it from a recorded cassette-backed upstream call.

## Why we changed

The seed-based runner pre-wrote the captured response (or shapes
extracted from it) into `base_state` before running the captured request.
That made the parity test almost tautological: the proxy echoed what the
runner had just told it. Real upstream-needed behavior — fetching
records the proxy didn't know about, hydrating shape mismatches, the
overlay between local staged state and upstream state — was invisible.

Cassette playback flips the contract: the proxy starts cold, the
operation handlers do whatever they actually need to do (including
asking upstream), and the cassette plays the upstream side
deterministically. What the proxy emits is what it would emit in
production, modulo the cassette being a stand-in for a real network.
