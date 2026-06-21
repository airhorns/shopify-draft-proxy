import { execFileSync } from 'node:child_process';
import { readdirSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import { recordedCallMatchesBody, validateRecordedUpstreamCalls } from '../../scripts/parity-cassette.js';

const repoRoot = new URL('../..', import.meta.url);
const paritySpecRoot = new URL('../../config/parity-specs/', import.meta.url);
const parityCliTimeoutMs = 30_000;

function countParitySpecs(directory: URL): number {
  return readdirSync(directory, { withFileTypes: true }).reduce((count, entry) => {
    if (entry.isDirectory()) return count + countParitySpecs(new URL(`${entry.name}/`, directory));
    return entry.isFile() && entry.name.endsWith('.json') ? count + 1 : count;
  }, 0);
}

describe('Rust parity runner cassette matching', () => {
  it('matches recorded upstream calls only by exact query text and exact variables', () => {
    const query = `
      query ProductsHydrateNodes($ids: [ID!]!) {
        nodes(ids: $ids) { id }
      }
    `;
    const requestBody = JSON.stringify({ query, variables: { ids: ['gid://shopify/Product/1'] } });

    expect(
      recordedCallMatchesBody(
        {
          operationName: 'CompletelyIgnoredForStrictMatching',
          variables: { ids: ['gid://shopify/Product/1'] },
          query,
        },
        requestBody,
      ),
    ).toBe(true);
  });

  it('does not match synthetic cassette descriptors even when operation name and variables match', () => {
    const requestBody = JSON.stringify({
      query: 'query ProductsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { id } }',
      variables: { ids: ['gid://shopify/Product/10170561036594'] },
    });

    expect(
      recordedCallMatchesBody(
        {
          operationName: 'ProductsHydrateNodes',
          variables: { ids: ['gid://shopify/Product/10170561036594'] },
          query: 'hand-synthesized from HAR-594 live seed product for mutation hydration',
        },
        requestBody,
      ),
    ).toBe(false);
  });

  it('does not let operation-name fallback hide real GraphQL document mismatches', () => {
    const requestBody = JSON.stringify({
      query: 'query ProductsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { id title } }',
      variables: { ids: ['gid://shopify/Product/1'] },
    });

    expect(
      recordedCallMatchesBody(
        {
          operationName: 'ProductsHydrateNodes',
          variables: { ids: ['gid://shopify/Product/1'] },
          query: 'query ProductsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { id } }',
        },
        requestBody,
      ),
    ).toBe(false);
  });

  it('does not match exact queries when variables differ', () => {
    const query = 'query ProductsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { id } }';
    const requestBody = JSON.stringify({ query, variables: { ids: ['gid://shopify/Product/1'] } });

    expect(
      recordedCallMatchesBody(
        {
          operationName: 'ProductsHydrateNodes',
          variables: { ids: ['gid://shopify/Product/2'] },
          query,
        },
        requestBody,
      ),
    ).toBe(false);
  });

  it('rejects non-GraphQL upstream call query descriptors during cassette validation', () => {
    expect(
      validateRecordedUpstreamCalls([
        {
          operationName: 'ProductsHydrateNodes',
          variables: { ids: ['gid://shopify/Product/1'] },
          query: 'sha:hand-synthesized-product-hydrate',
        },
        {
          operationName: 'CustomerHydrate',
          variables: { id: 'gid://shopify/Customer/1' },
        },
      ]),
    ).toEqual([
      'upstreamCalls[0].query is not a valid GraphQL document: "sha:hand-synthesized-product-hydrate"',
      'upstreamCalls[1].query is missing or is not a string',
    ]);
  });
});

describe('Rust parity runner CLI', () => {
  it(
    'discovers every checked-in parity spec before executing scenarios',
    () => {
      const output = execFileSync('corepack', ['pnpm', 'parity:run', '--', '--dry-run'], {
        cwd: repoRoot,
        encoding: 'utf8',
      });
      expect(output).toContain(`[parity] ${countParitySpecs(paritySpecRoot)} spec(s) selected`);
    },
    parityCliTimeoutMs,
  );

  it(
    'uses the captured target response as the passthrough cassette fallback for unsupported roots',
    () => {
      const output = execFileSync(
        'corepack',
        [
          'pnpm',
          'parity',
          '--',
          '--spec',
          'config/parity-specs/admin-platform/admin-platform-backup-region-update-access-blocker.json',
        ],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('admin-platform-backup-region-update-access-blocker.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'unwraps captured response.body payloads for passthrough cassette fallbacks',
    () => {
      const output = execFileSync(
        'corepack',
        ['pnpm', 'parity', '--', '--spec', 'config/parity-specs/admin-platform/by-id-not-found-read.json'],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('by-id-not-found-read.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'does not require local Rust handlers to consume every captured upstream call when output matches',
    () => {
      const output = execFileSync(
        'corepack',
        ['pnpm', 'parity', '--', '--spec', 'config/parity-specs/products/product-empty-state-read.json'],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('product-empty-state-read.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'uses each comparison target capture as fallback even when unrelated upstream recordings remain',
    () => {
      const output = execFileSync(
        'corepack',
        [
          'pnpm',
          'parity',
          '--',
          '--spec',
          'config/parity-specs/products/collectionCreate-and-add-products-parity.json',
        ],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('collectionCreate-and-add-products-parity.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'resolves capture-path variables before replaying recorded passthrough node reads',
    () => {
      const output = execFileSync(
        'corepack',
        [
          'pnpm',
          'parity',
          '--',
          '--spec',
          'config/parity-specs/admin-platform/admin-platform-delivery-profile-node-reads.json',
        ],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('admin-platform-delivery-profile-node-reads.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'executes proxyUpload targets as side-effect assertions for staged upload parity',
    () => {
      const output = execFileSync(
        'corepack',
        [
          'pnpm',
          'parity',
          '--',
          '--spec',
          'config/parity-specs/bulk-operations/bulk-operation-run-mutation-client-identifier-validation.json',
        ],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('bulk-operation-run-mutation-client-identifier-validation.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'uses the primary capture target, not the first target request, as primary passthrough fallback',
    () => {
      const output = execFileSync(
        'corepack',
        ['pnpm', 'parity', '--', '--spec', 'config/parity-specs/customers/customer-account-page-data-erasure.json'],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('customer-account-page-data-erasure.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'uses exact nested captured requests for primary passthrough fallback',
    () => {
      const output = execFileSync(
        'corepack',
        [
          'pnpm',
          'parity',
          '--',
          '--spec',
          'config/parity-specs/shipping-fulfillments/delivery-profile-update-validation.json',
        ],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('delivery-profile-update-validation.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'applies expected-difference rules to wildcard array paths',
    () => {
      const output = execFileSync(
        'corepack',
        [
          'pnpm',
          'parity',
          '--',
          '--spec',
          'config/parity-specs/products/inventory-transfer-ready-item-adjustments-local-staging.json',
        ],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('inventory-transfer-ready-item-adjustments-local-staging.json passed');
    },
    parityCliTimeoutMs,
  );
});
