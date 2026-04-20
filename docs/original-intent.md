# Original Intent: Shopify Digital Twin / Draft Proxy

This document exists so future agents and developers can preserve the original goal of the project instead of accidentally collapsing it into a trivial mock server.

## What we are trying to build

We are building a **man-in-the-middle proxy** for the **Shopify Admin GraphQL API** that sits between Shopify apps and the real Shopify backend.

At runtime, the proxy should let the app behave as if it is talking to Shopify directly.

### Default behavior

- The app points at this proxy instead of Shopify.
- The proxy forwards the app's existing auth headers unchanged.
- Read requests proxy through to Shopify by default unless snapshot-only mode is enabled.
- If no local staged effects are relevant, the response should be the same as Shopify's response.

### Mutation behavior

When the app performs a mutation, the default long-term goal is **not** to send the mutation to Shopify immediately.

Instead, the proxy should:

1. parse the mutation and variables
2. decide whether the mutation is supported by the local emulation layer
3. if supported:
   - apply its effects to local staged state only
   - synthesize a Shopify-like mutation response
   - generate stable synthetic IDs, timestamps, and derived values as needed
   - return those values consistently for the remainder of the session
4. if unsupported:
   - proxy through to Shopify as an escape hatch

The system is intended to let tests act as if writes happened, while the real store remains unchanged until an explicit commit.

### Read-after-write behavior

After local mutations are staged, subsequent reads should behave **as if Shopify had accepted and materialized those writes**, without the writes having actually happened in the real store.

This means the project is not just a request recorder. It must maintain a **stateful local model** of Shopify objects and patch query results accordingly.

## Meta API requirements

The proxy must expose a meta/control API on the same Koa server.

Required operations:

- **reset** — discard all staged state, staged IDs, caches, and logs; return to default state
- **commit** — replay staged mutations to Shopify in original order, stop on first failure, return a complete report of attempted operations and results
- **log inspection** — expose ordered staged mutation log
- **state inspection** — expose staged object graph

For now the whole runtime is **global state** in memory.

## Runtime modes

The project should support multiple read modes.

### 1. Live / hybrid mode

- Reads go to Shopify.
- Local staged effects are overlaid onto Shopify responses.
- This allows incremental fidelity while implementation coverage grows.

### 2. Snapshot mode

- Server starts with a fixed snapshot dataset.
- Reads are answered from snapshot + staged state.
- Requests for data absent from the snapshot should behave like Shopify behaves when the backend contains no such data.
- The runtime should not invent arbitrary fake records merely to satisfy shape requirements.

Snapshot inputs should support:

- recorded GraphQL fixture bundles
- normalized internal state snapshots

Internally, runtime state should use a normalized object graph.

## Fidelity standard

This project is meant to be **high fidelity**, not a shallow mock.

That means:

- preserve Shopify GraphQL shapes, nullability, and common error semantics
- reproduce userErrors patterns for supported operations
- keep IDs/timestamps stable within a session
- reproduce derived read behavior after staged writes
- aim for domain-specific realism, not generic GraphQL stubbing

The system should start with **products** and go deep there first, but the architecture should anticipate broader Admin GraphQL coverage.

## Development strategy

Implementation tracking lives in Linear. Repository conformance coverage should be expressed by the operation registry, parity specs, runtime tests, and captured fixtures rather than a checked-in project-management worklist.

We are **not** trying to support every domain on day one. We are trying to:

1. build a durable architecture
2. create an exhaustive coverage map
3. implement products deeply first
4. use conformance testing against a real Shopify dev store to drive fidelity forward

## Conformance testing is a first-class goal

A major part of this project is a repeatable framework to compare proxy behavior against real Shopify.

Long-term conformance workflow:

1. run real GraphQL interactions against a dev store
2. record request/response fixtures and scenario traces
3. replay equivalent scenarios against the proxy
4. compare shape, nullability, errors, IDs, timestamps, and read-after-write behavior
5. track parity per operation and scenario over time

Conformance coverage should eventually exist for **basically every interaction** we care about.

## Non-goals / things to avoid

Future agents should avoid turning this into any of the following:

- a static mock server with hard-coded responses only
- a generic GraphQL passthrough plus ad hoc interceptors with no state model
- a broad but shallow surface area that claims support without real behavior fidelity
- a compatibility-shim pile that papers over mismatches instead of modeling the right domain logic
- a framework that mutates real Shopify data during normal test runtime

## Current intentional constraints

These are deliberate for v1:

- TypeScript project
- Koa server
- strict TypeScript configuration
- pnpm package manager
- in-memory state only
- no parallel test-session isolation yet
- Admin GraphQL only
- no webhooks yet
- original raw mutations should be retained for eventual commit

## Principle for future work

When in doubt, prefer:

- **faithful domain modeling** over generic hacks
- **conformance-tested behavior** over guessed behavior
- **stable internal abstractions** over quick one-off patches
- **clear explicit unsupported cases** over pretending a feature works
