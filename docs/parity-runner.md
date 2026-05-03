# Parity runner

The parity runner drives every recorded scenario in `config/parity-specs/**`
through `draft_proxy.process_request` and compares the response (and, where
applicable, the proxy's emitted state and log) against the slice of the
captured Shopify response named by the spec. It is the canonical proof
that the Gleam port emulates Shopify with high fidelity.

This document describes the **cassette-playback** model. The previous
seed-based model (where the parity runner pre-wrote into `base_state` to
fake the proxy into knowing about resources it shouldn't yet have known
about) is being retired; details on the migration are in
`/Users/harry.brundage/.claude/plans/create-a-plan-to-fluttering-pretzel.md`.

## Model

A scenario consists of:

1. A **spec** in `config/parity-specs/<domain>/*.json` that names the
   GraphQL document, variables, and the targets (response paths /
   matchers / etc.) the runner compares.
2. A **capture** in `fixtures/conformance/**/*.json` that holds the real
   Shopify response the proxy is being graded against, plus an
   `upstreamCalls` cassette of upstream traffic the proxy made while
   serving the captured request.

The runner runs the spec in one of two **modes**, declared by the spec's
optional top-level `mode` field:

- `live-hybrid` (the default) — the proxy is configured with
  `read_mode: LiveHybrid` and the cassette transport is installed via
  `draft_proxy.with_upstream_transport`. Operation handlers may call
  `proxy/upstream_query.fetch_*` to ask upstream for information they
  don't have locally; those calls are matched against the cassette and
  served deterministically. **Mutations stage their effects locally**;
  upstream is never written to from a parity test.
- `snapshot-empty` — the proxy starts in default `Snapshot` mode with
  no transport installed. Asserts the proxy's cold-state behavior
  (reads return null/empty, creates work end-to-end, validation errors
  surface correctly).

There is **no per-domain "hydrate from upstream" pass** and no uniform
"mirror upstream into local state." Each operation handler decides, on
its own, whether it needs an upstream read and what to do with the
result. See _Per-operation upstream access_ below.

## Spec shape

```jsonc
{
  "scenarioId": "customer-detail-parity-plan",
  "liveCaptureFiles": ["fixtures/conformance/customer-detail.json"],
  "mode": "live-hybrid", // optional, default "live-hybrid"
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

`mode` is the only new field this migration introduces; everything else
matches the pre-existing spec format.

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

Operation handlers in `gleam/src/shopify_draft_proxy/shopify/<domain>.gleam`
call upstream via the `proxy/upstream_query.fetch_sync` chokepoint.
There is **no** uniform `hydrate_*_from_upstream_response` pattern. Each
operation decides, per-operation:

- whether it needs anything from upstream at all (many won't);
- what minimal slice it needs (one ID-keyed read, a narrow filter, the
  prior record so a mutation can merge fields);
- whether to persist the result into `base_state` (so the same operation
  called again doesn't re-fetch and so the staged-state overlay can layer
  on top), or whether to use it transiently for this one reply;
- what `Snapshot` mode does — typically: serve from local state if
  present, otherwise return null/empty.

Reads can fetch upstream. **Mutations can fetch upstream too** — e.g., a
`customerUpdate` that needs the existing record to merge fields. Mutations
still stage their side effect locally; what they may also do is read from
upstream first.

The choice for each operation is a short inline comment next to the
handler explaining what it fetches, why, and what it does in `Snapshot`
mode.

## Running

```sh
# Run every spec on both Gleam targets:
cd gleam && gleam test --target javascript && gleam test --target erlang

# Run one spec:
pnpm parity:run customer-detail-parity-plan          # default mode
pnpm parity:run --mode snapshot-empty customer-detail-parity-plan-empty-snapshot
```

The central gate test
(`gleam/test/parity_test.gleam:all_discovered_parity_specs_follow_expected_failures_test`)
classifies each spec as one of:

- `Passed` — spec ran and matched.
- `Failed` — spec ran and mismatched (or surfaced an unexpected error).
- `Skipped` — runner returned `SpecNotMigrated` (the spec is in
  `live-hybrid` mode but its capture has no `upstreamCalls`); the gate
  counts these but does not fail on them. Skipped specs are awaiting a
  cassette re-record.

The gate fails the build only on unexpected `Failed` outcomes (i.e.,
specs that aren't in `config/gleam-port-ci-gates.json`'s
`expectedGleamParityFailures`). Skipped specs do not flag.

## Debugging a single scenario

When a scenario fails and `first_line(message)` in the gate summary
isn't enough to figure out why, the runner has a debug mode that
streams every request, response, cassette match/miss, and per-target
assertion result to stderr.

Drop a tiny inspector test into `gleam/test/`:

```gleam
// gleam/test/inspect_spec_test.gleam
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
cd gleam && gleam test --target erlang -- inspect_spec_test
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

The recorder boots an in-memory `DraftProxy` (Gleam JS target) in
`LiveHybrid` mode against real Shopify, plays the spec's primary +
targets through it, intercepts every upstream call the operation
handlers issue, and writes the result into the capture file's
`upstreamCalls` field.

Credentials come from the existing OAuth flow:
`pnpm conformance:auth-link`, `pnpm conformance:exchange-auth`,
`pnpm conformance:probe`. Stored in `~/.config/shopify-draft-proxy/`.

## Migration playbook (per-domain agent brief)

Each domain (customers, products, collections, …) is migrated
independently on the long-lived `parity-cassette-migration` branch,
opened as a sub-PR into the branch. Once the substrate (cassette
infrastructure + transport injection + `upstream_query` helper +
recording script) and the docs have landed, parallel agents can pick up
domains.

Per-scenario steps:

1. `pnpm parity:record <id>` — records the cassette into the capture file.
2. Update the spec: drop any seeding-related fields; add `"mode":
"live-hybrid"` if not default.
3. `pnpm parity:run <id>` on both targets. If you see:
   - `cassette miss: operation=<X>` — the operation needs an upstream
     call we didn't record. Re-record. (Or, the operation should be
     reading from local state — adjust the handler.)
   - parity diff — the operation isn't computing the right response.
     Either the handler needs to add a narrow `upstream_query.fetch`
     call, persist the result, or compute the response differently.
4. Document any new per-operation upstream-fetch choice as a short inline
   comment next to the handler.
5. If the scenario qualifies for an empty-snapshot variant (per the
   duplication heuristic in the migration plan), create
   `<id>-empty-snapshot.json` with `"mode": "snapshot-empty"`, no
   cassette, and assertions adjusted for cold-state expectations. Run
   that variant green too.
6. Once green, delete any seeders that scenario was the last consumer of.

## What `seedX` keys mean (legacy)

Legacy capture files carry `seedProducts`, `seedCustomers`, `seedDiscounts`,
… arrays. These were inputs to the seed-based runner. They are being
removed as part of the migration; **never add new ones**. The
cheating-lint test (post-cutover) will fail the build if any `seedX` key
appears under `fixtures/conformance/**`.

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
