import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('shopify admin worklist status sync', () => {
  it('does not leave media or customer families documented as blocked after live conformance closure', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const worklist = readFileSync(resolve(repoRoot, 'docs/shopify-admin-worklist.md'), 'utf8');
    const coverage = readFileSync(resolve(repoRoot, 'docs/generated/conformance-coverage.md'), 'utf8');
    const generatedWorklist = readFileSync(
      resolve(repoRoot, 'docs/generated/worklist-conformance-status.md'),
      'utf8',
    );

    expect(coverage).toContain('- Covered operations: 67');
    expect(coverage).toContain('- Declared gaps: 0');
    expect(coverage).not.toContain('## Declared gaps\n\n- `');
    expect(coverage).toContain('- `orderCreate` → `order-create-inline-missing-order-argument-error`');
    expect(coverage).toContain('`draftOrders`');
    expect(coverage).toContain('`draftOrdersCount`');
    expect(coverage).toContain('`orderEditBegin`');
    expect(coverage).toContain('`orderEditCommit`');
    expect(coverage).toContain('`fulfillmentTrackingInfoUpdate`');
    expect(coverage).toContain('`fulfillmentCancel`');

    expect(generatedWorklist).toContain('## Root operations with declared conformance gaps');
    expect(generatedWorklist).not.toContain('`orderCreate` —');
    expect(generatedWorklist).not.toContain('`fulfillmentTrackingInfoUpdate` —');
    expect(generatedWorklist).not.toContain('`fulfillmentCancel` —');
    expect(generatedWorklist).toContain('`draftOrders`');
    expect(generatedWorklist).toContain('`draftOrdersCount`');
    expect(generatedWorklist).toContain('`orderEditBegin`');
    expect(generatedWorklist).toContain('`orderEditCommit`');
    expect(generatedWorklist).toContain('`fulfillmentTrackingInfoUpdate`');
    expect(generatedWorklist).toContain('`fulfillmentCancel`');

    expect(worklist).toContain('## Media domain');
    expect(worklist).toContain('- [x] media-on-product read coverage');
    expect(worklist).toContain('- [x] media create family');
    expect(worklist).toContain('- [x] media update family');
    expect(worklist).toContain('- [x] media delete family');

    expect(worklist).not.toContain('real customer detail fixture capture is externally blocked');
    expect(worklist).not.toContain('real customer catalog fixture capture is externally blocked');
    expect(worklist).toContain('live customer detail capture now records a real Shopify fixture');
    expect(worklist).toContain('live customer catalog capture now records a real Shopify fixture');
    expect(worklist).toContain('variant inventory linkage mutations beyond the now-covered `inventoryItemUpdate` metadata root');
    expect(worklist).toContain('`inventoryActivate` now mirrors the current host-backed live slice');
    expect(worklist).toContain('`inventoryDeactivate` now mirrors both captured branches');
    expect(worklist).toContain('`inventoryBulkToggleActivation` now mirrors the broader live slice on this host');
    expect(worklist).not.toContain('broader multi-location activation/deactivation success parity remains externally blocked');
  });

  it('documents the current auth-regressed probe state while keeping draftOrderComplete as the remaining creation blocker after auth repair', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const worklist = readFileSync(resolve(repoRoot, 'docs/shopify-admin-worklist.md'), 'utf8');

    expect(worklist).toContain('`draftOrderCreate`');
    expect(worklist).toContain('draft-order-create-parity.json');
    expect(worklist).toContain('draft-order-detail.json');
    expect(worklist).toContain('draft-orders-catalog.json');
    expect(worklist).toContain('draft-orders-count.json');
    expect(worklist).toContain('the current orders-domain conformance probe on this host is auth-regressed');
    expect(worklist).toContain('`corepack pnpm conformance:probe` currently fails with `401` / `Invalid API key or access token` against the repo credential for `very-big-test-store.myshopify.com`');
    expect(worklist).toContain('the checked-in fixtures are the last verified live references and `corepack pnpm conformance:capture-orders` refreshes blocker notes without overwriting those safe fixtures with `401` payloads');
    expect(worklist).toContain('`pending/draft-order-read-conformance-scope-blocker.md` is recreated on the current auth-regressed run while the checked-in `draft-orders-catalog.json` / `draft-orders-count.json` fixtures remain the last verified live baseline');
    expect(worklist).toContain('the auth regression does not invalidate the checked-in `draftOrders` / `draftOrdersCount` baseline, and a failed repo-local refresh now has a concrete meaning on this host: `corepack pnpm conformance:refresh-auth` can return `invalid_request` / `This request requires an active refresh_token` once the saved grant is no longer refreshable');
    expect(worklist).toContain('the remaining creation blocker after auth repair is still `draftOrderComplete`');
    expect(worklist).toContain('snapshot mode and live-hybrid now both support a first narrow synthetic/local `draftOrderComplete` runtime slice');
    expect(worklist).toContain('`pending/order-editing-conformance-scope-blocker.md`');
    expect(worklist).toContain('`pending/fulfillment-lifecycle-conformance-scope-blocker.md`');
    expect(worklist).toContain('if `corepack pnpm conformance:refresh-auth` now fails with `invalid_request` / `This request requires an active refresh_token`, stop retrying the dead saved grant and generate a fresh manual store-auth link before rerunning `corepack pnpm conformance:probe` plus `corepack pnpm conformance:capture-orders`');
    expect(worklist).not.toContain('the current orders-domain conformance probe on this host is healthy again');
    expect(worklist).not.toContain('`corepack pnpm conformance:probe` now succeeds against the repo credential for `very-big-test-store.myshopify.com`');
    expect(worklist).not.toContain('healthy capture refreshed `draft-orders-catalog.json` and `draft-orders-count.json`, so `pending/draft-order-read-conformance-scope-blocker.md` is removed while the checked-in fixtures remain the current live baseline');
  });

});
