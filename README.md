# shopify-draft-proxy

`shopify-draft-proxy` is a high-fidelity Shopify Admin GraphQL digital twin for test environments.

Point an app at this proxy instead of Shopify. Supported mutations are staged in local in-memory state, mutation payloads are synthesized with Shopify-like shapes, and later reads behave as if the writes happened. The real store remains unchanged during normal supported mutation handling until an explicit `/__meta/commit`.

This is not a generic mock server. The goal is to model Shopify domain behavior closely enough that app tests can exercise realistic write/read flows without normal test runs mutating a dev store.

## What it does

- Preserves Shopify-like versioned Admin GraphQL routes: `/admin/api/:version/graphql.json`.
- Forwards Shopify auth headers unchanged when an upstream Shopify call is required.
- Stages supported mutations locally and records the original raw GraphQL body for later commit replay.
- Overlays staged local effects onto downstream reads for supported domains.
- Proxies unsupported mutations to Shopify as an escape hatch by default and records that fact in logs/observability.
- Can reject unsupported mutations with `unsupportedMutationMode: 'reject'` when tests must fail closed.
- Exposes meta APIs for health, configuration, staged-state inspection, reset, log inspection, state dump/restore, and commit.
- Uses conformance captures and parity tests to keep behavior grounded in real Shopify responses.

The runtime is implemented in Rust. JavaScript and TypeScript consumers use the `shopify-draft-proxy` npm package, whose public API is a thin process bridge to the Rust HTTP runtime.

## Install from source

Release packaging is still private to this repository. For repository work, install the root toolchain and package dependencies:

```sh
corepack pnpm install
```

Prerequisites:

- Node 22 or newer
- Corepack
- A Rust toolchain compatible with the checked-in `Cargo.toml`
- A Shopify dev store plus an app Admin API access token for live-hybrid runtime or live conformance work

Useful root scripts:

```sh
corepack pnpm rust:fmt
corepack pnpm rust:clippy
corepack pnpm rust:test
corepack pnpm typecheck
corepack pnpm lint
corepack pnpm conformance:check
corepack pnpm parity:run
corepack pnpm test
```

The package name is:

- npm: `shopify-draft-proxy`

## Running the proxy

```sh
PORT=4000 corepack pnpm dev
```

The `dev` and `start` scripts launch the Rust binary:

```sh
cargo run --bin shopify-draft-proxy-server --quiet
```

Runtime configuration is read from environment variables by the Rust server, including `PORT`, `READ_MODE`, `UNSUPPORTED_MUTATION_MODE`, `SHOPIFY_ADMIN_ORIGIN`, and `SNAPSHOT_PATH`.

## Embedding from JavaScript

JavaScript callers use `createDraftProxy(config)` and HTTP-shaped request objects. The package keeps this surface stable while the implementation behind it is the Rust HTTP runtime.

```ts
import { createDraftProxy } from 'shopify-draft-proxy';

const proxy = createDraftProxy({
  readMode: 'snapshot',
  unsupportedMutationMode: 'passthrough',
  port: 4000,
  shopifyAdminOrigin: 'https://your-store.myshopify.com',
});

const response = await proxy.processRequest({
  method: 'POST',
  path: '/admin/api/2025-01/graphql.json',
  headers: {
    'x-shopify-access-token': 'shpat_test_token',
  },
  body: {
    query: '{ shop { name } }',
  },
});

console.log(response.status, response.body);
await proxy.dispose();
```

Each `DraftProxy` shim owns a Rust runtime process and therefore owns its in-memory store, mutation log, snapshot baseline, and synthetic identity allocation. Call `dispose()` when a test no longer needs a proxy instance.

## Runtime modes

`snapshot` answers supported reads from local snapshot and staged state. Absent data should match Shopify's null/empty behavior rather than inventing records.

`live-hybrid` forwards unknown or intentionally passthrough operations upstream and overlays staged local effects for supported domains. Upstream transport and commit replay preserve inbound auth headers.

`passthrough` is the live-only debugging posture exposed to JavaScript callers. It is not support for known mutation roots; supported mutations still stage locally, and unknown/unsupported passthrough remains visible in observability.

`unsupportedMutationMode` controls unsupported mutation roots in `live-hybrid`. It defaults to `passthrough`, preserving the escape hatch that forwards the request upstream and records a proxied log entry. Set it to `reject` to return a 400 GraphQL error envelope before any upstream call when the mutation root is not locally supported. Supported local mutations still stage locally in either mode.

`POST /__meta/commit` is the explicit exception to local-only supported mutation handling: it replays pending staged mutations upstream in original order.

## Supported routes

The package routes:

- `POST /admin/api/:version/graphql.json`
- `GET /__meta/health`
- `GET /__meta/config`
- `GET /__meta/log`
- `GET /__meta/state`
- `POST /__meta/reset`
- `POST /__meta/dump`
- `POST /__meta/restore`
- `POST /__meta/commit`
- `POST` / `PUT /staged-uploads/:target/:filename`
- `GET /__meta/bulk-operations/:encoded_id/result.jsonl`

The remaining intentionally unsupported HTTP boundaries are:

- `GET /__meta` operator UI
- staged-upload byte download/serving

Those routes are artifact-serving surfaces, not permission to weaken domain fidelity for GraphQL roots.

## Current domain coverage

Coverage is domain-specific. A root is not considered supported until the local lifecycle and downstream read-after-write behavior are modeled for that domain. Validation-only or branch-only handling is documented as a guardrail, not full support.

Current Rust runtime coverage includes product reads/mutations, saved-search roots, staged upload handling, meta route state/log/reset/dump/restore, commit replay, and live-hybrid passthrough/reject semantics. Endpoint-specific coverage notes live under `docs/endpoints/`.

## Conformance testing

We prove that the proxy correctly emulates Shopify by recording Shopify's real behavior and then making sure the proxy acts the same as Shopify, except for real-world side effects and nondeterministic values such as IDs, timestamps, cursors, and throttle metadata.

See the conformance specs at `config/parity-specs` and the capture fixtures at `fixtures/conformance`.
