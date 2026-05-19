# Rust Runtime Port Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task after this initial scaffold lands.

**Goal:** Replace the Gleam runtime with a Rust runtime while keeping Shopify Admin GraphQL draft-proxy behavior, checked-in parity specs, request/response fixtures, and public Node/HTTP embedding surfaces compatible. Final acceptance requires green CI, no checked-in `.gleam` runtime/test code, no parity spec or recorded fixture changes, and at least the current integration-test count.

**Architecture:** Build a Rust crate (`shopify_draft_proxy`) as the runtime authority. Keep the existing TypeScript tooling for conformance capture/reporting initially, but move all request processing, GraphQL parsing/dispatch, in-memory state, mutation logging, cassette parity replay, and HTTP route behavior into Rust. Expose the Rust runtime to Node through a thin adapter so existing consumers still import `js/dist/index.js`, while Rust unit/integration tests become the main runtime tests.

**Tech Stack:** Rust 2021/2024-compatible crate, `serde`/`serde_json`, `graphql-parser`, `http`, `tokio`, optional `axum` for standalone HTTP, `napi-rs` or a narrow native bridge for Node ESM interop, existing pnpm/TypeScript tooling for conformance metadata.

---

## Non-negotiable acceptance gates

1. CI is green on the Rust branch.
2. `config/parity-specs/**`, `config/parity-requests/**`, and `fixtures/conformance/**` are byte-for-byte unchanged from `origin/main` unless the user explicitly approves a re-recording. Add a CI/script gate for this.
3. Integration-test inventory is not reduced: current baseline is 3 TypeScript integration test files (`tests/integration/*.test.ts`). The final branch must have at least 3 integration tests that exercise the Rust runtime through public boundaries.
4. No `.gleam` files remain in the final tree, and Gleam setup/build/test CI steps are removed.
5. The public runtime value remains instance-owned: no global mutable proxy state. Each request mutates only that `DraftProxy` instance or returns the next state internally.
6. Supported mutations continue to stage locally; unsupported operations remain explicit passthrough/reject depending on config; `__meta/commit` replays original raw mutations in log order.
7. Parity runner continues to execute all existing parity specs with the existing recorded cassettes.

## Baseline recorded during branch creation

- Branch: `rust-port` from `origin/main` at `67264021d4dfa706b08bf4b2b98d8d994d8757fe`.
- Existing runtime source: 253 `.gleam` files under `src/**/*.gleam`.
- Existing Gleam test source: 67 `.gleam` files under `test/**/*.gleam`.
- Existing TypeScript test files: 23 `tests/**/*.test.ts`.
- Existing integration tests: 3 files under `tests/integration/`.
- Existing parity specs: 910 JSON files under `config/parity-specs/`.
- Existing conformance fixtures: 840 JSON files under `fixtures/conformance/`.
- Existing parity requests: 1949 GraphQL files under `config/parity-requests/`.

---

## Phase 1: Rust scaffold and invariant gates

### Task 1: Create Rust crate with route-surface smoke tests

**Objective:** Establish Rust as a buildable runtime target and lock in the meta route shapes before porting domains.

**Files:**

- Create: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/proxy.rs`
- Create: `tests/meta_routes.rs`
- Modify: `package.json` scripts to add Rust checks later.

**Steps:**

1. Write failing Rust integration tests for `GET /__meta/health`, `GET /__meta/config`, `GET /__meta/log`, `GET /__meta/state`, `POST /__meta/reset`, missing paths, and wrong methods.
2. Run `cargo test --test meta_routes` and confirm failure because the Rust API is missing.
3. Implement minimal `DraftProxy`, `Config`, `Request`, and `Response` types plus route dispatch.
4. Re-run `cargo test --test meta_routes` and confirm pass.
5. Commit as `feat: scaffold rust draft proxy runtime`.

### Task 2: Add fixture/spec immutability gate

**Objective:** Ensure porting does not silently edit Shopify parity evidence.

**Files:**

- Create: `scripts/check-rust-port-fixture-invariants.ts` or Rust equivalent.
- Modify: `package.json` scripts.
- Test: `tests/unit/no-parity-evidence-drift.test.ts` if implemented in TS.

**Steps:**

1. Write a failing check that compares the current branch against `origin/main` for `config/parity-specs`, `config/parity-requests`, and `fixtures/conformance`.
2. Make it pass on a clean branch.
3. Wire it into CI before parity tests.

### Task 3: Add no-Gleam final gate, initially non-blocking

**Objective:** Make final cutover measurable without blocking the early scaffold.

**Files:**

- Create: `scripts/check-no-gleam-runtime.ts`.
- Modify: `package.json` scripts.

**Steps:**

1. Add a check that reports all `.gleam` files and exits non-zero only when `RUST_PORT_FINAL=1` is set.
2. Wire normal CI to run it in report mode during incremental port.
3. Flip it to required failure in the final cutover commit.

---

## Phase 2: Public API and Node/HTTP bridge

### Task 4: Port the TypeScript-facing data model

**Objective:** Make Rust serialize the same config/log/state/response shapes consumed by `js/src/runtime.ts`.

**Files:**

- Modify: `src/proxy.rs`
- Create: `src/state.rs`
- Create: `src/types.rs`
- Tests: Rust tests plus existing `tests/integration/gleam-js-http-adapter-parity.test.ts` renamed to Rust.

**Steps:**

1. Add tests that assert JSON equality against existing TypeScript expectations.
2. Implement serde models with `#[serde(rename_all = ...)]` where needed.
3. Preserve state dump schema/version: `shopify-draft-proxy/state-dump`, version `1`.

### Task 5: Replace the Gleam Node shim with a Rust bridge

**Objective:** Keep existing Node consumers working while the implementation moves to Rust.

**Files:**

- Modify: `js/src/runtime.ts`
- Modify/Create: native bridge config (`napi`, CLI protocol, or generated bindings)
- Modify: `js/src/app.ts` only if necessary.

**Steps:**

1. Write/rename integration tests so they call the Rust-backed `createApp` and `createDraftProxy`.
2. Verify the tests fail against the empty scaffold.
3. Implement the bridge with the narrowest stable surface: construct proxy, process request, process GraphQL request, get state/log/config, reset, dump/restore, commit, staged upload content, injected cassette transport.
4. Keep JS adapter thin: request/body parsing and Node HTTP server can remain TS if desired.

---

## Phase 3: GraphQL parsing, registry, and parity runner

### Task 6: Port GraphQL root-field parsing

**Objective:** Route by actual top-level GraphQL root fields, never by operation name alone.

**Files:**

- Create: `src/graphql/*`
- Tests: port `test/shopify_draft_proxy/graphql/*` into Rust tests.

**Steps:**

1. Port parser/root-field tests first and watch them fail.
2. Implement parser helpers using `graphql-parser` or a local parser only where Shopify-compatible behavior requires it.
3. Run parser tests and JS/Rust API smoke tests.

### Task 7: Port operation registry as data

**Objective:** Preserve the existing operation support map without duplicating incompatible generated sources.

**Files:**

- Create: `src/registry.rs`
- Modify: TypeScript conformance scripts to read Rust registry source or a generated JSON artifact.

**Steps:**

1. Add executable checks that current TypeScript tooling sees the same registry roots and notes.
2. Port the registry data.
3. Delete the Gleam registry only during final cutover.

### Task 8: Port parity runner to Rust

**Objective:** Run all existing parity specs and cassettes without changing them.

**Files:**

- Create: `src/parity/*` or `tests/parity_runner.rs`
- Modify: `package.json` `parity:run` to call Rust runner after enough domains are ported.

**Steps:**

1. Write tests that discover all 910 specs and validate schema/inventory without executing domain behavior.
2. Port cassette playback and JSONPath diff behavior.
3. Make the runner execute the same strict comparisons as the Gleam runner.

---

## Phase 4: Domain-by-domain runtime port

Port in dependency order, using strict TDD and parity fixtures as the oracle:

1. Shared JSON/state/identity helpers.
2. Common GraphQL response projection helpers.
3. Products and product variants.
4. Saved searches and search query parser.
5. Customers.
6. Orders/draft orders/order edits/fulfillment basics.
7. Shipping/fulfillments, inventory, locations.
8. Metafields/metaobjects/custom data.
9. Discounts, gift cards, payments.
10. Markets/B2B/localization.
11. Online store/events/webhooks/functions/admin platform/privacy/bulk operations.

For each domain:

- Port focused unit tests first.
- Run the new Rust unit tests and the existing parity spec subset for that domain.
- Do not edit recorded fixtures/specs to make Rust pass.
- Audit sibling serializers/validators when one family-level mismatch appears.

---

## Phase 5: Final cutover

### Task 9: Remove Gleam runtime and CI dependencies

**Objective:** Leave no Gleam code in the repo.

**Files:**

- Delete: `src/**/*.gleam`, `test/**/*.gleam`, `gleam.toml`, `manifest.toml`, Gleam docs/logs that only describe the old port.
- Modify: `.github/workflows/ci.yml`
- Modify: `package.json`
- Modify: `AGENTS.md`, `docs/architecture.md`, `README.md`.

**Steps:**

1. Flip `check-no-gleam-runtime` to required.
2. Replace Gleam CI steps with `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, Rust-backed parity run, existing TS conformance tooling, Node smoke tests, and fixture invariant checks.
3. Run full local CI equivalent.
4. Push branch and monitor CI to green.

### Task 10: Code quality review and maintainability pass

**Objective:** Be satisfied with the resulting Rust codebase.

**Checklist:**

- Modules are domain-oriented and not one giant translated file.
- Shared helpers are reused for connections/search/JSON projection.
- Public API has docs and stable serde shapes.
- Error handling uses typed errors, not stringly panics.
- No global mutable proxy state.
- Tests explain behavior and do not merely restate registry metadata.
