# AGENTS.md

Guidance for future AI coding agents working in `shopify-draft-proxy`.

## Primary mission

Preserve the intent in `docs/original-intent.md`.

This project is a **Shopify Admin GraphQL digital twin / draft proxy**, not a generic mock server. The goal is to let tests stage realistic Shopify writes locally and observe downstream reads as if those writes happened, without mutating the real store during normal supported mutation handling.

## Non-negotiables

1. **Products first, but deep**
   - Start with products and their directly related sub-resources.
   - Do not chase broad coverage before product fidelity is credible.

2. **Domain fidelity over hacks**
   - Prefer modeling Shopify domain behavior over brittle response patching.
   - If uncertain, add a conformance test against a real dev store.

3. **Do not send supported mutations to Shopify at runtime**
   - Supported mutations must stage locally.
   - Unsupported mutations may proxy through, but must be visible in logs/observability.

4. **Keep original raw mutations for commit**
   - Commit should replay original mutations in original order.

5. **Match Shopify's empty/no-data behavior**
   - In snapshot mode and local reads, prefer the same null/empty structures Shopify returns when the backend lacks data.

6. **Docs and worklist must stay current**
   - Update `docs/shopify-admin-worklist.md` when adding or scoping support.
   - Update `docs/architecture.md` if runtime architecture changes.
   - Update `docs/original-intent.md` only if the product goal truly changes.

## Development rules

- Use strict TypeScript.
- Keep runtime state in memory unless the project scope explicitly expands.
- Preserve Shopify-like versioned routes.
- Forward auth headers unchanged to upstream Shopify.
- Expose and test the meta API.
- Add tests for every supported operation.
- Prefer conformance fixtures over hand-wavy comments about expected behavior.

## Suggested workflow

1. Read `docs/original-intent.md`.
2. Read `docs/architecture.md`.
3. Read `docs/hard-and-weird-notes.md` before making fidelity assumptions.
4. Check `docs/shopify-admin-worklist.md` for the next operation.
5. Add/adjust tests before implementation.
6. Update docs/worklist after shipping behavior.

## Repo status note

Early commits may only contain scaffolding. Do not mistake scaffolding for finished behavior.
