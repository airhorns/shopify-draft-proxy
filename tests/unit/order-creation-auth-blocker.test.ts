import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('order creation auth blocker evidence', () => {
  it('keeps direct order creation narrowly covered for the captured validation slices while documenting the still-blocked happy path', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as Array<Record<string, unknown>>;
    const scenarios = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/conformance-scenarios.json'), 'utf8'),
    ) as Array<Record<string, unknown>>;
    const blockerNote = readFileSync(
      resolve(repoRoot, 'pending/order-creation-conformance-scope-blocker.md'),
      'utf8',
    );
    const worklist = readFileSync(resolve(repoRoot, 'docs/shopify-admin-worklist.md'), 'utf8');
    const weirdNotes = readFileSync(resolve(repoRoot, 'docs/hard-and-weird-notes.md'), 'utf8');

    const orderCreate = registry.find((entry) => entry['name'] === 'orderCreate');
    expect(orderCreate).toEqual(
      expect.objectContaining({
        name: 'orderCreate',
        implemented: true,
        runtimeTests: ['tests/integration/order-creation-flow.test.ts'],
        conformance: expect.objectContaining({
          status: 'covered',
          scenarioIds: [
            'order-create-inline-missing-order-argument-error',
            'order-create-inline-null-order-argument-error',
            'order-create-missing-order-invalid-variable',
            'order-create-live-parity',
          ],
        }),
      }),
    );

    const draftOrderCreate = registry.find((entry) => entry['name'] === 'draftOrderCreate');
    expect(draftOrderCreate).toEqual(
      expect.objectContaining({
        name: 'draftOrderCreate',
        implemented: true,
        runtimeTests: ['tests/integration/order-creation-flow.test.ts'],
        conformance: expect.objectContaining({
          status: 'covered',
          scenarioIds: expect.arrayContaining(['draft-order-create-live-parity']),
        }),
      }),
    );

    const draftOrderComplete = registry.find((entry) => entry['name'] === 'draftOrderComplete');
    expect(draftOrderComplete).toEqual(
      expect.objectContaining({
        name: 'draftOrderComplete',
        conformance: expect.objectContaining({
          status: 'covered',
          scenarioIds: [
            'draft-order-complete-inline-missing-id-argument-error',
            'draft-order-complete-inline-null-id-argument-error',
            'draft-order-complete-missing-id-invalid-variable',
          ],
        }),
      }),
    );

    const orderCreateScenario = scenarios.find((entry) => entry['id'] === 'order-create-live-parity');
    expect(orderCreateScenario).toEqual(
      expect.objectContaining({
        id: 'order-create-live-parity',
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-create-parity.json'],
      }),
    );

    const draftOrderCreateScenario = scenarios.find((entry) => entry['id'] === 'draft-order-create-live-parity');
    expect(draftOrderCreateScenario).toEqual(
      expect.objectContaining({
        id: 'draft-order-create-live-parity',
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-create-parity.json'],
      }),
    );

    const completionScenario = scenarios.find((entry) => entry['id'] === 'draft-order-complete-live-parity');
    expect(completionScenario).toEqual(
      expect.objectContaining({
        id: 'draft-order-complete-live-parity',
        status: 'planned',
      }),
    );

    expect(blockerNote).toContain('orderCreate');
    expect(blockerNote).toContain('order-create-parity.json');
    expect(blockerNote).toContain('last verified happy-path fixture');
    expect(blockerNote).toContain('draftOrderCreate');
    expect(blockerNote).toContain('draft-order-create-parity.json');
    expect(blockerNote).toContain('draft-order-detail.json');
    expect(blockerNote).toContain('draftOrderComplete');
    expect(blockerNote).toContain('last verified family-specific access-denied evidence');
    expect(blockerNote).toContain('mark-as-paid');
    expect(blockerNote).toContain('.manual-store-auth-token.json');
    expect(blockerNote).toContain('corepack pnpm conformance:capture-orders');
    expect(blockerNote).toContain('current run is auth-regressed before the family-specific creation roots can be reprobed');
    expect(blockerNote).toContain('remaining creation-family live blocker after auth is repaired is still `draftOrderComplete`');
    expect(blockerNote).not.toContain('missing requiredScopes blocker metadata');

    expect(worklist).toContain('order-create-parity.json');
    expect(worklist).toContain('local `orderCreate` now stages that same merchant-facing slice locally');
    expect(worklist).toContain('draftOrderCreate');
    expect(worklist).toContain('draft-order-create-parity.json');
    expect(worklist).toContain('the current orders-domain conformance probe on this host is auth-regressed');
    expect(worklist).toContain('`corepack pnpm conformance:probe` currently fails with `401` / `Invalid API key or access token` against the repo credential for `very-big-test-store.myshopify.com`');
    expect(worklist).toContain('the checked-in fixtures are the last verified live references and `corepack pnpm conformance:capture-orders` refreshes blocker notes without overwriting those safe fixtures with `401` payloads');
    expect(worklist).toContain('the remaining creation blocker after auth repair is still `draftOrderComplete`');
    expect(worklist).not.toContain('the current orders-domain conformance probe on this host is healthy again');

    expect(weirdNotes).toContain('order-create-parity.json');
    expect(weirdNotes).toContain('draftOrderCreate');
    expect(weirdNotes).toContain('draftOrderComplete');
  });
});
