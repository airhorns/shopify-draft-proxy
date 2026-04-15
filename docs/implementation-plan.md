# Shopify Draft Proxy Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Build a high-fidelity Shopify Admin GraphQL draft proxy that stages supported mutations locally, preserves realistic read-after-write behavior, and provides durable documentation plus conformance infrastructure.

**Architecture:** A Koa reverse proxy classifies GraphQL operations into queries vs mutations, applies product-domain handlers against a normalized in-memory state model, overlays staged effects onto live or snapshot reads, and exposes a meta API for reset/commit/inspection.

**Tech Stack:** TypeScript, Koa, GraphQL AST parsing, pnpm, Vitest, Supertest.

---

## Phase 0: Documentation and guardrails

### Task 0.1: Preserve original intent in docs
**Objective:** Make the project vision durable for future agents.

**Files:**
- Create: `docs/original-intent.md`
- Create: `docs/architecture.md`
- Create: `docs/implementation-plan.md`
- Create: `docs/shopify-admin-worklist.md`
- Create: `AGENTS.md`

**Verification:**
- Docs explicitly state this is a digital twin / draft proxy, not a mock server.
- Docs specify product-first depth, meta API, in-memory state, and conformance-first development.

### Task 0.2: Bootstrap strict TypeScript repo
**Objective:** Create the baseline repo structure.

**Files:**
- Create: `package.json`
- Create: `tsconfig.json`
- Create: `.gitignore`
- Create: `README.md`
- Create: `src/server.ts`
- Create: `src/app.ts`

**Verification:**
- `pnpm install`
- `pnpm typecheck`
- `pnpm test`

---

## Phase 1: Request classification and proxy shell

### Task 1.1: Build configuration layer
**Objective:** Centralize runtime mode and upstream Shopify configuration.

**Files:**
- Create: `src/config.ts`
- Test: `tests/unit/config.test.ts`

### Task 1.2: Build GraphQL operation parser
**Objective:** Parse request body and classify operation type/name.

**Files:**
- Create: `src/graphql/parse-operation.ts`
- Test: `tests/unit/parse-operation.test.ts`

### Task 1.3: Build proxy route shell
**Objective:** Accept Admin GraphQL requests on versioned Shopify-like paths.

**Files:**
- Create: `src/proxy/routes.ts`
- Modify: `src/app.ts`
- Test: `tests/integration/proxy-route.test.ts`

### Task 1.4: Build upstream Shopify client
**Objective:** Proxy requests to Shopify while forwarding auth headers unchanged.

**Files:**
- Create: `src/shopify/upstream-client.ts`
- Test: `tests/unit/upstream-client.test.ts`

---

## Phase 2: In-memory state core

### Task 2.1: Define normalized domain types
**Objective:** Establish the runtime state model with product-first entities.

**Files:**
- Create: `src/state/types.ts`
- Test: `tests/unit/state-types.test.ts`

### Task 2.2: Build in-memory state store
**Objective:** Hold base state, staged state, and mutation log globally.

**Files:**
- Create: `src/state/store.ts`
- Test: `tests/unit/store.test.ts`

### Task 2.3: Build synthetic identity generator
**Objective:** Produce stable synthetic Shopify-like GIDs and timestamps.

**Files:**
- Create: `src/state/synthetic-identity.ts`
- Test: `tests/unit/synthetic-identity.test.ts`

### Task 2.4: Build mutation log model
**Objective:** Record original raw mutations plus interpreted metadata.

**Files:**
- Create: `src/state/mutation-log.ts`
- Test: `tests/unit/mutation-log.test.ts`

---

## Phase 3: Meta API

### Task 3.1: Add reset endpoint
**Objective:** Clear all staged data, caches, synthetic IDs, and logs.

**Files:**
- Create: `src/meta/routes.ts`
- Modify: `src/app.ts`
- Test: `tests/integration/meta-reset.test.ts`

### Task 3.2: Add log inspection endpoint
**Objective:** Return ordered staged mutation log for tests/debugging.

**Files:**
- Modify: `src/meta/routes.ts`
- Test: `tests/integration/meta-log.test.ts`

### Task 3.3: Add state inspection endpoint
**Objective:** Return current staged object graph summary.

**Files:**
- Modify: `src/meta/routes.ts`
- Test: `tests/integration/meta-state.test.ts`

### Task 3.4: Add commit endpoint shell
**Objective:** Replay raw staged mutations in original order and stop on first failure.

**Files:**
- Create: `src/shopify/commit.ts`
- Modify: `src/meta/routes.ts`
- Test: `tests/integration/meta-commit.test.ts`

---

## Phase 4: Product-first query support

### Task 4.1: Define product query capability registry
**Objective:** Track supported product queries and fallback behavior.

**Files:**
- Create: `src/proxy/capabilities.ts`
- Test: `tests/unit/capabilities.test.ts`

### Task 4.2: Implement product base-state normalizer
**Objective:** Convert live/snapshot GraphQL payloads into normalized product entities.

**Files:**
- Create: `src/shopify/normalize-product.ts`
- Test: `tests/unit/normalize-product.test.ts`

### Task 4.3: Implement first read overlay path for `product`
**Objective:** Overlay local staged state onto a single-product query.

**Files:**
- Create: `src/proxy/overlay-product.ts`
- Test: `tests/integration/product-query-overlay.test.ts`

### Task 4.4: Implement first read overlay path for `products`
**Objective:** Overlay staged inserts/updates/deletes onto product list queries.

**Files:**
- Modify: `src/proxy/overlay-product.ts`
- Test: `tests/integration/products-query-overlay.test.ts`

### Task 4.5: Add snapshot mode query resolution
**Objective:** Resolve product queries from startup snapshot only.

**Files:**
- Create: `src/proxy/snapshot-resolver.ts`
- Test: `tests/integration/snapshot-products.test.ts`

---

## Phase 5: Product-first mutation support

### Task 5.1: Implement `productCreate`
**Objective:** Stage local product creation and synthesize Shopify-like response.

**Files:**
- Create: `src/proxy/mutations/product-create.ts`
- Test: `tests/integration/product-create.test.ts`

### Task 5.2: Implement `productUpdate`
**Objective:** Stage product updates with stable downstream reads.

**Files:**
- Create: `src/proxy/mutations/product-update.ts`
- Test: `tests/integration/product-update.test.ts`

### Task 5.3: Implement `productDelete`
**Objective:** Stage product deletion semantics.

**Files:**
- Create: `src/proxy/mutations/product-delete.ts`
- Test: `tests/integration/product-delete.test.ts`

### Task 5.4: Implement product variant mutation family
**Objective:** Support the first deep product sub-resource mutations.

**Files:**
- Create: `src/proxy/mutations/product-variant-*.ts`
- Test: `tests/integration/product-variant-*.test.ts`

### Task 5.5: Implement product option and metafield mutations
**Objective:** Support realistic downstream product query state.

**Files:**
- Create: `src/proxy/mutations/product-option-*.ts`
- Create: `src/proxy/mutations/metafield-*.ts`
- Test: matching integration tests

---

## Phase 6: Conformance framework

### Task 6.1: Define scenario fixture format
**Objective:** Record repeatable request/response traces against real Shopify.

**Files:**
- Create: `docs/conformance.md`
- Create: `fixtures/README.md`
- Create: `src/testing/scenario-types.ts`
- Test: `tests/unit/scenario-types.test.ts`

### Task 6.2: Build fixture recorder shell
**Objective:** Capture real Shopify interactions into scenario bundles.

**Files:**
- Create: `src/testing/record-scenario.ts`
- Test: `tests/unit/record-scenario.test.ts`

### Task 6.3: Build proxy parity runner
**Objective:** Replay recorded scenarios against the proxy and compare outputs.

**Files:**
- Create: `src/testing/run-parity.ts`
- Test: `tests/integration/parity-runner.test.ts`

### Task 6.4: Add coverage matrix reporting
**Objective:** Show support and parity status per operation.

**Files:**
- Create: `src/testing/coverage-matrix.ts`
- Modify: `docs/shopify-admin-worklist.md`
- Test: `tests/unit/coverage-matrix.test.ts`

---

## Phase 7: Expand breadth carefully

Work breadth should follow the worklist, but only after product-first depth is credible. Add each new operation only with:

1. worklist entry
2. domain model update if needed
3. direct integration test
4. conformance scenario or parity fixture if feasible

---

## Operating rules

- Do not mutate Shopify during normal supported mutation handling.
- Unsupported mutations may proxy through, but surface this clearly.
- Prefer null/empty behavior matching real Shopify over invented fake objects.
- Preserve raw original mutation documents for commit.
- Use conformance tests to settle uncertain behavior instead of guessing forever.
