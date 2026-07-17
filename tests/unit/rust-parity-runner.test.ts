import { execFile } from 'node:child_process';
import { readdirSync } from 'node:fs';
import { promisify } from 'node:util';
import { describe, expect, it } from 'vitest';

import {
  defaultApiVersionForCapture,
  diffValues,
  parseJsonlRecordsForParity,
  selectPaths,
} from '../../scripts/parity-run.js';
import {
  formatRecordedCallMismatch,
  recordedCallMatchesBody,
  recordedCallMatchesRequest,
  validateRecordedUpstreamCalls,
} from '../../scripts/parity-cassette.js';
import { paritySpecSchema } from '../../scripts/support/json-schemas.js';

const repoRoot = new URL('../..', import.meta.url);
const paritySpecRoot = new URL('../../config/parity-specs/', import.meta.url);
const parityCliTimeoutMs = 30_000;
const execFileAsync = promisify(execFile);

describe('parity runner API version routing', () => {
  it('uses a supported captured version from metadata or the fixture path', () => {
    expect(defaultApiVersionForCapture('fixtures/example/2025-01/products/example.json', {})).toBe('2025-01');
    expect(
      defaultApiVersionForCapture('fixtures/example/unknown/products/example.json', { apiVersion: '2026-01' }),
    ).toBe('2026-01');
  });

  it('rejects unsupported capture versions instead of silently replaying another schema', () => {
    expect(() =>
      defaultApiVersionForCapture('fixtures/example/2026-10/customers/example.json', { apiVersion: '2026-10' }),
    ).toThrow(/2026-10.*executable schemas/u);
    expect(() => defaultApiVersionForCapture('fixtures/example/2026-10/customers/example.json', {})).toThrow(
      /2026-10.*executable schemas/u,
    );
  });

  it('uses the manifest default when capture metadata and path omit a version', () => {
    expect(defaultApiVersionForCapture('fixtures/example/unknown/customers/example.json', {})).toBe('2026-07');
  });
});

async function runPnpm(args: string[]): Promise<string> {
  const { stdout } = await execFileAsync('pnpm', args, {
    cwd: repoRoot,
    encoding: 'utf8',
    maxBuffer: 10 * 1024 * 1024,
  });
  return stdout.toString();
}

function countParitySpecs(directory: URL): number {
  return readdirSync(directory, { withFileTypes: true }).reduce((count, entry) => {
    if (entry.isDirectory()) return count + countParitySpecs(new URL(`${entry.name}/`, directory));
    return entry.isFile() && entry.name.endsWith('.json') ? count + 1 : count;
  }, 0);
}

describe('parity runner selected path projection', () => {
  it.each([
    {
      label: 'first selected path',
      proxy: { payload: { title: 'Proxy title', userErrors: [] } },
      expectedPath: '$.payload.title',
    },
    {
      label: 'last selected path',
      proxy: { payload: { title: 'Shopify title', userErrors: [{ message: 'Proxy error' }] } },
      expectedPath: '$.payload.userErrors[0]',
    },
  ])('keeps enough selected paths to catch a difference in the $label', ({ proxy, expectedPath }) => {
    const capture = { payload: { title: 'Shopify title', userErrors: [] } };
    const selectedPaths = ['$.payload.title', '$.payload.userErrors'];

    expect(diffValues(selectPaths(capture, selectedPaths), selectPaths(proxy, selectedPaths), [])).toEqual([
      expect.stringContaining(expectedPath),
    ]);
  });

  it('projects wildcard array selected paths without losing sibling selections', () => {
    const value = {
      userErrors: [
        { field: ['handle'], code: 'TAKEN', message: 'Handle has already been taken' },
        { field: ['type'], code: 'INVALID', message: 'Type is invalid' },
      ],
    };

    expect(selectPaths(value, ['$.userErrors[*].field', '$.userErrors[*].code'])).toEqual({
      userErrors: [
        { field: ['handle'], code: 'TAKEN' },
        { field: ['type'], code: 'INVALID' },
      ],
    });
  });
});

describe('parity runner JSONL targets', () => {
  it('parses JSONL response bodies before selected-path comparison', () => {
    const capture = '{"title":"Product"}\n{"alt":"Front","__parentId":"gid://shopify/Product/1"}\n';
    const proxy = '{"title":"Product"}\n{"alt":"Front","__parentId":"gid://shopify/Product/2"}\n';
    const selectedPaths = ['$[*].title', '$[*].alt', '$[*].__parentId'];

    const diffs = diffValues(
      selectPaths(parseJsonlRecordsForParity(capture), selectedPaths),
      selectPaths(parseJsonlRecordsForParity(proxy), selectedPaths),
      [
        {
          path: '$[1].__parentId',
          matcher: 'shopify-gid:Product',
          reason: 'Shopify and the proxy allocate different product ids.',
        },
      ],
    );

    expect(diffs).toEqual([]);
  });
});

describe('Rust parity runner cassette matching', () => {
  it('accepts Storefront API parity requests as first-class captured scenario inputs', () => {
    expect(
      paritySpecSchema.parse({
        scenarioId: 'storefront-shop-name-proxy-parity',
        operationNames: ['shop'],
        scenarioStatus: 'captured',
        assertionKinds: ['storefront-api-proxy'],
        comparisonMode: 'captured-vs-proxy-request',
        liveCaptureFiles: [
          'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/online-store/storefront-shop-name-proxy-parity.json',
        ],
        proxyRequest: {
          apiSurface: 'storefront',
          apiVersion: '2025-01',
          documentPath: 'config/parity-requests/online-store/storefront-shop-name.graphql',
          headers: {
            'X-Shopify-Storefront-Access-Token': 'shpat_redacted',
          },
        },
        comparison: {
          mode: 'strict-json',
          expectedDifferences: [],
          targets: [
            {
              name: 'storefront-shop-name',
              capturePath: '$.primary.response.body',
              proxyPath: '$',
            },
          ],
        },
      }).proxyRequest?.apiSurface,
    ).toBe('storefront');
  });

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

  it('matches recorded upstream calls by method, API surface path, exact query text, and exact variables', () => {
    const query = 'query StorefrontShopName { shop { name } }';
    const body = JSON.stringify({ query, variables: {} });
    const call = {
      method: 'POST',
      apiSurface: 'storefront' as const,
      path: '/api/2026-04/graphql.json',
      operationName: 'StorefrontShopName',
      variables: {},
      query,
    };

    expect(
      recordedCallMatchesRequest(call, {
        method: 'POST',
        apiSurface: 'storefront',
        path: '/api/2026-04/graphql.json',
        body,
      }),
    ).toBe(true);
    expect(
      recordedCallMatchesRequest(call, {
        method: 'POST',
        apiSurface: 'admin',
        path: '/admin/api/2026-04/graphql.json',
        body,
      }),
    ).toBe(false);
    expect(
      recordedCallMatchesRequest(call, {
        method: 'POST',
        apiSurface: 'storefront',
        path: '/api/2025-01/graphql.json',
        body,
      }),
    ).toBe(false);
    expect(
      recordedCallMatchesRequest(call, {
        method: 'POST',
        apiSurface: 'storefront',
        path: '/api/2026-04/graphql.json',
        body: JSON.stringify({ query, variables: { country: 'CA' } }),
      }),
    ).toBe(false);
  });

  it('does not let legacy Admin cassettes without surface metadata satisfy Storefront requests', () => {
    const query = 'query SameBody { shop { name } }';
    const body = JSON.stringify({ query, variables: {} });

    expect(
      recordedCallMatchesRequest(
        {
          operationName: 'SameBody',
          variables: {},
          query,
        },
        {
          method: 'POST',
          apiSurface: 'storefront',
          path: '/api/2026-04/graphql.json',
          body,
        },
      ),
    ).toBe(false);
  });

  it('formats missing cassette diagnostics with method, surface, and path context', () => {
    const query = 'query StorefrontShopName { shop { name } }';
    const diagnostic = formatRecordedCallMismatch(
      {
        method: 'POST',
        apiSurface: 'storefront',
        path: '/api/2026-04/graphql.json',
        body: JSON.stringify({ query, variables: {} }),
      },
      [
        {
          method: 'POST',
          apiSurface: 'admin',
          path: '/admin/api/2026-04/graphql.json',
          operationName: 'StorefrontShopName',
          variables: {},
          query,
        },
      ],
      new Set(),
    );

    expect(diagnostic).toContain('Outgoing method: POST');
    expect(diagnostic).toContain('Outgoing apiSurface: storefront');
    expect(diagnostic).toContain('Outgoing path: /api/2026-04/graphql.json');
    expect(diagnostic).toContain('apiSurface: admin');
    expect(diagnostic).toContain('path: /admin/api/2026-04/graphql.json');
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

  it('requires Storefront upstream cassettes to carry non-secret method and path metadata', () => {
    const query = 'query StorefrontShopName { shop { name } }';

    expect(
      validateRecordedUpstreamCalls([
        {
          apiSurface: 'storefront',
          query,
          variables: {},
          headers: {
            'X-Shopify-Storefront-Access-Token': 'real-token-value',
          },
        },
      ]),
    ).toEqual([
      'upstreamCalls[0].method must be POST for Storefront GraphQL calls',
      'upstreamCalls[0].path is required for Storefront GraphQL calls',
      'upstreamCalls[0].headers.X-Shopify-Storefront-Access-Token must redact Storefront token values',
    ]);

    expect(
      validateRecordedUpstreamCalls([
        {
          method: 'POST',
          apiSurface: 'storefront',
          path: '/api/2026-04/graphql.json',
          query,
          variables: {},
          headers: {
            'X-Shopify-Storefront-Access-Token': '<redacted:storefront-access-token>',
          },
        },
      ]),
    ).toEqual([]);
  });
});

describe('Rust parity runner CLI', () => {
  it(
    'discovers every checked-in parity spec before executing scenarios',
    async () => {
      const output = await runPnpm(['parity:run', '--', '--dry-run']);
      expect(output).toContain(`[parity] ${countParitySpecs(paritySpecRoot)} spec(s) selected`);
    },
    parityCliTimeoutMs,
  );

  it(
    'uses the captured target response as the passthrough cassette fallback for unsupported roots',
    async () => {
      const output = await runPnpm([
        'parity',
        '--',
        '--spec',
        'config/parity-specs/admin-platform/admin-platform-backup-region-update-access-blocker.json',
      ]);
      expect(output).toContain('admin-platform-backup-region-update-access-blocker.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'unwraps captured response.body payloads for passthrough cassette fallbacks',
    async () => {
      const output = await runPnpm([
        'parity',
        '--',
        '--spec',
        'config/parity-specs/admin-platform/by-id-not-found-read.json',
      ]);
      expect(output).toContain('by-id-not-found-read.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'does not require local Rust handlers to consume every captured upstream call when output matches',
    async () => {
      const output = await runPnpm([
        'parity',
        '--',
        '--spec',
        'config/parity-specs/products/product-empty-state-read.json',
      ]);
      expect(output).toContain('product-empty-state-read.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'uses each comparison target capture as fallback even when unrelated upstream recordings remain',
    async () => {
      const output = await runPnpm([
        'parity',
        '--',
        '--spec',
        'config/parity-specs/products/collectionCreate-and-add-products-parity.json',
      ]);
      expect(output).toContain('collectionCreate-and-add-products-parity.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'resolves capture-path variables before replaying recorded passthrough node reads',
    async () => {
      const output = await runPnpm([
        'parity',
        '--',
        '--spec',
        'config/parity-specs/admin-platform/admin-platform-delivery-profile-node-reads.json',
      ]);
      expect(output).toContain('admin-platform-delivery-profile-node-reads.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'executes proxyUpload targets as side-effect assertions for staged upload parity',
    async () => {
      const output = await runPnpm([
        'parity',
        '--',
        '--spec',
        'config/parity-specs/bulk-operations/bulk-operation-run-mutation-client-identifier-validation.json',
      ]);
      expect(output).toContain('bulk-operation-run-mutation-client-identifier-validation.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'uses the primary capture target, not the first target request, as primary passthrough fallback',
    async () => {
      const output = await runPnpm([
        'parity',
        '--',
        '--spec',
        'config/parity-specs/customers/customer-account-page-data-erasure.json',
      ]);
      expect(output).toContain('customer-account-page-data-erasure.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'uses exact nested captured requests for primary passthrough fallback',
    async () => {
      const output = await runPnpm([
        'parity',
        '--',
        '--spec',
        'config/parity-specs/shipping-fulfillments/delivery-profile-update-validation.json',
      ]);
      expect(output).toContain('delivery-profile-update-validation.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'applies expected-difference rules to wildcard array paths',
    async () => {
      const output = await runPnpm([
        'parity',
        '--',
        '--spec',
        'config/parity-specs/shipping-fulfillments/fulfillment-order-split-multi.json',
      ]);
      expect(output).toContain('fulfillment-order-split-multi.json passed');
    },
    parityCliTimeoutMs,
  );
});
