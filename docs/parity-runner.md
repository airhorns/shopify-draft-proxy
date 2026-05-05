# Parity runner

The parity runner drives every recorded scenario in `config/parity-specs/**`
through `draft_proxy.process_request` and compares the response (and, where
applicable, the proxy's emitted state and log) against the slice of the
captured Shopify response named by the spec. It is the canonical proof
that the Gleam port emulates Shopify with high fidelity.

This document describes the **cassette-playback** model. The previous
seed-based model (where the parity runner pre-wrote into `base_state` to
fake the proxy into knowing about resources it shouldn't yet have known
about) is unsupported.

## Model

A scenario consists of:

1. A **spec** in `config/parity-specs/<domain>/*.json` that names the
   GraphQL document, variables, and the targets (response paths /
   matchers / etc.) the runner compares.
2. A **capture** in `fixtures/conformance/**/*.json` that holds the real
   Shopify response the proxy is being graded against, plus an
   `upstreamCalls` cassette of upstream traffic the proxy made while
   serving the captured request.

The runner configures the proxy with `read_mode: LiveHybrid` and installs
the cassette transport via `draft_proxy.with_upstream_transport`.
Operation handlers may call `proxy/upstream_query.fetch_*` to ask upstream
for information they don't have locally; those calls are matched against
the cassette and served deterministically. **Mutations stage their effects
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
billing interval and `trialDays`. The executable local-runtime proof is
`config/parity-specs/apps/app-subscription-activation-readback.json`.

## Spec shape

```jsonc
{
  "scenarioId": "customer-detail-parity-plan",
  "liveCaptureFiles": ["fixtures/conformance/customer-detail.json"],
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
      "query": "sha:abc123…", // optional, debug-only
      "response": {
        "status": 200,
        "body": { "data": { "customer": { "id": "…", "email": "…" } } },
      },
    },
  ],
}
```

Match key: `(operationName, variables)` with **deep-equal**, **object-key-
order-insensitive** comparison on `variables`. Arrays are compared
element-wise (order matters). Numeric coercion: `1` and `1.0` are equal.

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
is just to forward), opt the operation into the dispatch-layer
passthrough list:

```gleam
// src/shopify_draft_proxy/proxy/draft_proxy.gleam
fn force_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: GraphQLOperationType,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case type_, primary_root_field {
    QueryOperation, "customersCount" -> True
    QueryOperation, "customerByIdentifier" -> True
    QueryOperation, "customer" ->
      !customers.local_has_customer_id(proxy, variables)
    QueryOperation, "customers" -> True
    // Add your operation here, gated on local state:
    QueryOperation, "discountNode" ->
      !discounts.local_has_discount_id(proxy, variables)
    QueryOperation, "discountNodes" ->
      !discounts.local_has_staged_discounts(proxy, variables)
    _, _ -> False
  }
}
```

When this returns `True`, `dispatch_graphql` forwards the entire
GraphQL document verbatim to the upstream transport (cassette in
tests, real HTTP in production). The proxy adds nothing to the
response; cassette replay is trivially correct because the cassette
captured the same response the spec is being graded against.

`Snapshot` mode is unaffected — `force_passthrough_in_live_hybrid`
only fires when `read_mode == LiveHybrid`. The local handler keeps
serving Snapshot reads.

#### **Almost always gate passthrough on local state**

Unconditional passthrough (`-> True`) regresses lifecycle scenarios
that stage or delete records, because passthrough forwards
proxy-synthetic gids upstream (where they 404) and bypasses the
empty/null answer the test expects after a delete. The discounts
domain demonstrates the pattern:

```gleam
// src/shopify_draft_proxy/proxy/discounts.gleam
pub fn local_has_discount_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id)
        || case store.get_effective_discount_by_id(proxy.store, id) {
          Some(_) -> True
          None -> False
        }
      _ -> False
    }
  })
}

pub fn local_has_staged_discounts(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  let has_synthetic =
    dict.values(variables)
    |> list.any(fn(value) {
      case value {
        root_field.StringVal(s) -> is_proxy_synthetic_gid(s)
        _ -> False
      }
    })
  has_synthetic || !list.is_empty(store.list_effective_discounts(proxy.store))
}
```

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
# Run every spec on both Gleam targets:
gleam test --target javascript && gleam test --target erlang

# Run the parity test module on one target:
gleam test --target javascript -- parity_test
```

The central gate test
(`test/parity_test.gleam:all_discovered_parity_specs_pass_test`)
discovers every spec and treats any runner error or comparison mismatch
as a hard test failure. A spec without a valid `upstreamCalls` cassette
does not run in a degraded mode; it fails until the capture is repaired.

## Debugging a single scenario

When a scenario fails and `first_line(message)` in the gate summary
isn't enough to figure out why, the runner has a debug mode that
streams every request, response, cassette match/miss, and per-target
assertion result to stderr.

Drop a tiny inspector test into `test/`:

```gleam
// test/inspect_spec_test.gleam
import gleam/io
import parity/runner

pub fn inspect_test() {
  let path = "config/parity-specs/customers/customerInputValidation-parity.json"
  case runner.run_debug(path) {
    Ok(report) ->
      case runner.into_assert(report) {
        Ok(Nil) -> io.println("PASS")
        Error(message) -> { io.println("FAIL:") io.println(message) }
      }
    Error(err) -> {
      io.println("RUNERR:")
      io.println(runner.render_error(err))
    }
  }
}
```

Run it with:

```sh
gleam test --target erlang -- inspect_spec_test
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

The recorder boots an in-memory `DraftProxy` (Gleam JS target) in
`LiveHybrid` mode against real Shopify, plays the spec's primary +
targets through it, intercepts every upstream call the operation
handlers issue, and writes the result into the capture file's
`upstreamCalls` field.

Credentials come from the existing OAuth flow:
`pnpm conformance:auth-link`, `pnpm conformance:exchange-auth`,
`pnpm conformance:probe`. Stored in `~/.shopify-draft-proxy/`.

### Recorder hits the env-configured store, not the per-fixture store

The recorder reads `SHOPIFY_CONFORMANCE_ADMIN_ORIGIN` and runs the
live query there. Many fixtures in this repo were captured against
older stores (e.g. `very-big-test-store.myshopify.com`) but the OAuth
token that's currently linked points at a different store (e.g.
`harry-test-heelo.myshopify.com`). When the recorder targets a store
that doesn't have the same data the original capture was against —
or worse, when the captured query references a Shopify schema field
that's since been removed (e.g. `DiscountAutomaticBasic.context`) —
the live recording will write a useless cassette: `{ errors:
[{undefinedField}] }` or simply `wrote 0 upstreamCalls`.

**Hand-synthesize the cassette from the captured response in this
case.** The captured response is already in the fixture file; the
cassette just wraps it as a single recorded call. Recipe:

```sh
# Capture lives at .response in most files; check .capturePath in the
# spec for the actual location (a few files use .response.response).
RESPONSE_BODY=$(jq '.response' fixtures/conformance/<store>/<api>/<domain>/<scenario>.json)

jq --argjson body "$RESPONSE_BODY" \
  --arg op "<OperationName>" \
  --argjson vars "$(jq '.proxyRequest.variables' config/parity-specs/<domain>/<scenario>.json)" \
  '.upstreamCalls = [{
     operationName: $op,
     variables: $vars,
     query: "<sha placeholder>",
     response: { status: 200, body: $body }
   }]' \
  fixtures/conformance/<store>/<api>/<domain>/<scenario>.json \
  > /tmp/cassette.json && mv /tmp/cassette.json fixtures/conformance/<store>/<api>/<domain>/<scenario>.json
```

Hand-synthesizing is the dominant case for older read fixtures; it's
faster and more reliable than re-pointing OAuth at the original
store. Reserve live re-recording for scenarios whose schema or
underlying data is current.

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

If two scenarios share the same Pattern 2 `operationName` (e.g. the
discounts domain reuses `DiscountHydrate` across the redeem-code-bulk
and app-bulk lifecycle scenarios), and you evolve the query's
selection set in one scenario's cassette, the **other scenario's
cassette will silently break** — the cassette HIT still matches on
`operationName`, but the parser sees the new keys as `null` because
the older cassette was recorded against the old selection set.

Prefer additive parsing: read new keys first, fall back to old.
Concretely, if you rewrite `data.codeDiscountNode` to
`data.codeNode` / `data.automaticNode` aliases, parse with
`option.or(non_null_node(json_get(data, "codeDiscountNode")))` so
the legacy cassette shape still resolves. Or: keep the old key in
the new query (`data { codeDiscountNode: ... codeNode: ... }`) so
both shapes coexist in the cassette.

When the operation is genuinely scenario-specific, give it a
scenario-specific name (`DiscountHydrateForBulkAppFlow`) instead of
overloading a shared one — the operation name is the cassette match
key.

## Adding Or Repairing Coverage

Per-scenario steps:

1. `pnpm parity:record <id>` — records the cassette into the capture file.
   - **If the recorder reports `wrote 0 upstreamCalls`** AND the live
     recording succeeded (no errors visible): the operation handler
     isn't reaching upstream at all. Continue to step 2.
   - **If the live recording produced GraphQL errors** (e.g.
     `undefinedField`, schema drift, missing data): hand-synthesize
     the cassette from the captured response (recipe above). Skip to
     step 3.
   - **If it wrote N>0 upstreamCalls cleanly**, the cassette is
     populated; skip to step 3.
2. Decide the pattern (1 or 2 above) and wire the operation:
   - Pattern 1: add the root field to `force_passthrough_in_live_hybrid`
     in `src/shopify_draft_proxy/proxy/draft_proxy.gleam`. **Gate
     it on local state** (use or add a `local_has_*_id` /
     `local_has_staged_*` helper in the domain module — see Pattern 1
     section above for the discounts example).
   - Pattern 2: add a `upstream_query.fetch_sync(...)` call inside the
     domain handler and document the choice inline.
   - Re-run `pnpm parity:record <id>` and confirm it writes the expected
     `upstreamCalls` entries. An empty array is valid for mutation-only
     scenarios that make no upstream reads.
3. `gleam test --target javascript -- parity_test` to run
   on JS, then `--target erlang` to run on Erlang. If you see:
   - `cassette miss: operation=<X>` — the operation made an upstream
     call we didn't record. Re-record or extend the cassette.
   - `expectedDifference rule was not satisfied at <path>` — a stale
     `expectedDifferences` rule. Delete the rule from the spec.
   - parity diff — the operation isn't computing the right response.
     Adjust the handler.
4. **Verify on both targets.** Drift between Erlang and JS is the
   most expensive bug class.
5. **Watch for collateral regressions.** Adding an unconditional or
   incorrectly-gated passthrough branch can regress lifecycle
   scenarios in _other_ specs that touch the same root fields. After
   each handler change, run the full domain's tests (or the whole
   suite) — not just your scenario.

## Seed Keys Are Forbidden

Capture files must not carry top-level `seedProducts`, `seedCustomers`,
`seedDiscounts`, or similar `seedX` keys. Those keys were inputs to the
unsupported seed-based runner. The cheating-lint test fails the build if
they reappear under `fixtures/conformance/**`.

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
