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

6. **Docs must stay current**
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
- Repo scripts should be TypeScript files executed with `tsx` or similar, not
  `.mjs` files. Existing `.mjs` capture helpers are legacy and should not be
  used as the pattern for new scripts.

## GitHub repository

- The canonical GitHub repository is `airhorns/shopify-draft-proxy`.
- Open pull requests against `airhorns/shopify-draft-proxy`; do not target personal forks.
- If a workspace remote points at a personal fork, retarget it to `git@github.com:airhorns/shopify-draft-proxy.git` before pushing or creating a PR.

## Shopify conformance auth rule

- Do **not** read `SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN` from repo `.env` in scripts anymore.
- All live conformance scripts must get credentials through `scripts/shopify-conformance-auth.mjs`.
- The canonical credential file is `~/.shopify-draft-proxy/conformance-admin-auth.json`.
- `getValidConformanceAccessToken(...)` is the single entry point for token access. It probes the stored access token, refreshes it when possible, and throws a clear error when the stored credential is missing or unrecoverable.
- New auth grants should be generated with `corepack pnpm conformance:auth-link`, and callback exchange should go through `corepack pnpm conformance:exchange-auth -- '<full callback url>'`.

## Suggested workflow

1. Read `docs/original-intent.md`.
2. Read `docs/architecture.md`.
3. Read `docs/hard-and-weird-notes.md` before making fidelity assumptions.
4. Check Linear for the next operation to implement.
5. Add/adjust tests before implementation.
6. Update docs after shipping behavior.

## Repo status note

Early commits may only contain scaffolding. Do not mistake scaffolding for finished behavior.
