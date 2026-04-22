import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';

describe('order creation auth blocker evidence', () => {
  it('keeps direct order creation narrowly covered for the captured validation slices while documenting the still-blocked happy path', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8')) as Array<
      Record<string, unknown>
    >;
    const scenarios = loadConformanceScenarios(repoRoot) as Array<Record<string, unknown>>;
    const blockerNote = readFileSync(resolve(repoRoot, 'pending/order-creation-conformance-scope-blocker.md'), 'utf8');
    const weirdNotes = readFileSync(resolve(repoRoot, 'docs/hard-and-weird-notes.md'), 'utf8');

    const orderCreate = registry.find((entry) => entry['name'] === 'orderCreate');
    expect(orderCreate).toEqual(
      expect.objectContaining({
        name: 'orderCreate',
        implemented: true,
        runtimeTests: ['tests/integration/order-creation-flow.test.ts'],
      }),
    );

    const draftOrderCreate = registry.find((entry) => entry['name'] === 'draftOrderCreate');
    expect(draftOrderCreate).toEqual(
      expect.objectContaining({
        name: 'draftOrderCreate',
        implemented: true,
        runtimeTests: ['tests/integration/order-creation-flow.test.ts'],
      }),
    );

    const draftOrderComplete = registry.find((entry) => entry['name'] === 'draftOrderComplete');
    expect(draftOrderComplete).toEqual(
      expect.objectContaining({
        name: 'draftOrderComplete',
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
    expect(blockerNote).toContain('~/.shopify-draft-proxy/conformance-admin-auth.json');
    expect(blockerNote).toContain('/tmp/shopify-conformance-app/hermes-conformance-products/.env');
    expect(blockerNote).toContain('corepack pnpm conformance:capture-orders');
    expect(blockerNote).toContain(
      'current run is auth-regressed before the family-specific creation roots can be reprobed',
    );
    expect(blockerNote).toContain(
      'remaining creation-family live blocker after auth is repaired is still `draftOrderComplete`',
    );
    expect(blockerNote).not.toContain('missing requiredScopes blocker metadata');

    expect(weirdNotes).toContain('order-create-parity.json');
    expect(weirdNotes).toContain('draftOrderCreate');
    expect(weirdNotes).toContain('draftOrderComplete');
  });
});
