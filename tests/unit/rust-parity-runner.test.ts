import { execFile } from 'node:child_process';
import { readdirSync } from 'node:fs';
import { promisify } from 'node:util';
import { describe, expect, it } from 'vitest';

import {
  bindCorrespondingGidAliases,
  captureResponseForRequest,
  captureResponseForTarget,
  createParityGidAliasBindings,
  defaultApiVersionForCapture,
  diffValues,
  parseArgs as parseParityArgs,
  parseJsonlRecordsForParity,
  rewriteBoundGidAliases,
  scenarioClockFromCapture,
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

describe('captureResponseForTarget', () => {
  it('wraps legacy direct data targets as GraphQL passthrough responses', () => {
    expect(
      captureResponseForTarget(
        { downstreamRead: { data: { customersCount: { count: 42 } } } },
        { name: 'downstream', capturePath: '$.downstreamRead.data' },
      ),
    ).toEqual({
      status: 200,
      body: { data: { customersCount: { count: 42 } } },
    });
  });
});

describe('captureResponseForRequest', () => {
  it('finds the response beside a captured public request envelope', () => {
    const query = 'mutation ProductCreate($input: ProductInput!) { productCreate(product: $input) { product { id } } }';

    expect(
      captureResponseForRequest(
        {
          setup: {
            request: { query, variables: { input: { title: 'Captured product' } } },
            response: {
              status: 200,
              body: { data: { productCreate: { product: { id: 'gid://shopify/Product/100' } } } },
            },
          },
        },
        {
          query,
          variables: { input: { title: 'Captured product' } },
          headers: {},
          path: '/admin/api/2026-04/graphql.json',
          apiSurface: 'admin',
        },
      ),
    ).toEqual({
      status: 200,
      body: { data: { productCreate: { product: { id: 'gid://shopify/Product/100' } } } },
    });
  });
});

describe('scenarioClockFromCapture', () => {
  it('derives a lifecycle replay clock from one captured epoch run id', () => {
    expect(
      scenarioClockFromCapture({
        variables: { runId: '1783175824302' },
        request: { startsAt: '2026-07-18T14:37:04.302Z' },
      }),
    ).toBe('2026-07-04T14:37:04.302Z');
  });

  it('does not freeze a capture without a lifecycle timestamp', () => {
    expect(scenarioClockFromCapture({ variables: { runId: '1783175824302' } })).toBeUndefined();
  });

  it('does not freeze a capture with ambiguous epoch run ids', () => {
    expect(
      scenarioClockFromCapture({
        first: { runId: '1783175824302', startsAt: '2026-07-18T14:37:04.302Z' },
        second: { runId: '1783175824303' },
      }),
    ).toBeUndefined();
  });

  it('derives a lifecycle replay clock from a unique epoch embedded in captured values', () => {
    expect(
      scenarioClockFromCapture({
        variables: {
          email: 'draft-order-probe-1777076856718@example.com',
          reserveInventoryUntil: '2026-05-25T00:27:36Z',
        },
        response: { sku: 'custom-service-1777076856718' },
      }),
    ).toBe('2026-04-25T00:27:36.718Z');
  });

  it('does not freeze a lifecycle capture with ambiguous embedded epochs', () => {
    expect(
      scenarioClockFromCapture({
        variables: {
          email: 'draft-order-probe-1777076856718@example.com',
          reserveInventoryUntil: '2026-05-25T00:27:36Z',
        },
        response: { sku: 'custom-service-1777076856719' },
      }),
    ).toBeUndefined();
  });

  it('does not treat Shopify GID tails as embedded capture epochs', () => {
    expect(
      scenarioClockFromCapture({
        variables: {
          title: 'scheduled-discount-1783175824302',
          startsAt: '2026-07-18T14:37:04.302Z',
        },
        response: { id: 'gid://shopify/DiscountAutomaticNode/1658181583154' },
      }),
    ).toBe('2026-07-04T14:37:04.302Z');
  });
});

describe('parity runner CLI', () => {
  it('rejects attempts to suppress parity failures', () => {
    expect(() => parseParityArgs(['--allow-failures'])).toThrow('Unknown flag: --allow-failures');
  });
});

describe('parity comparison target schema', () => {
  it('allows a cold replay target to retain captured Shopify GIDs', () => {
    const parsed = paritySpecSchema.parse({
      scenarioId: 'cold-replay',
      operationNames: ['product'],
      scenarioStatus: 'captured',
      assertionKinds: ['payload-shape'],
      liveCaptureFiles: ['fixtures/conformance/example.myshopify.com/2026-04/example.json'],
      comparisonMode: 'captured-vs-proxy-request',
      comparison: {
        targets: [
          {
            name: 'cold-target',
            capturePath: '$.response.data',
            preserveProxyState: true,
            rewriteGidAliases: false,
          },
        ],
      },
    });

    expect(parsed.comparison?.targets?.[0]?.rewriteGidAliases).toBe(false);
  });
});

describe('parity runner exact Shopify GID aliases', () => {
  const addressRule = {
    path: '$',
    matcher: 'exact-string:gid://shopify/CompanyAddress/4?shopify-draft-proxy=synthetic',
    reason: 'The update must preserve the address allocated earlier in this scenario.',
  };

  it('binds an exact local alias to one actual GID across comparison targets', () => {
    const bindings = createParityGidAliasBindings();

    expect(
      diffValues(
        'gid://shopify/CompanyAddress/9352282418',
        'gid://shopify/CompanyAddress/5?shopify-draft-proxy=synthetic',
        [addressRule],
        '$',
        bindings,
      ),
    ).toEqual([]);
    expect(
      diffValues(
        'gid://shopify/CompanyAddress/9352282418',
        'gid://shopify/CompanyAddress/5?shopify-draft-proxy=synthetic',
        [addressRule],
        '$',
        bindings,
      ),
    ).toEqual([]);
    expect(
      diffValues(
        'gid://shopify/CompanyAddress/9352282418',
        'gid://shopify/CompanyAddress/6?shopify-draft-proxy=synthetic',
        [addressRule],
        '$',
        bindings,
      ),
    ).toEqual([expect.stringContaining('CompanyAddress/6?shopify-draft-proxy=synthetic')]);
  });

  it('keeps exact local aliases type-safe and one-to-one', () => {
    const bindings = createParityGidAliasBindings();
    const secondAddressRule = {
      ...addressRule,
      matcher: 'exact-string:gid://shopify/CompanyAddress/6?shopify-draft-proxy=synthetic',
    };

    expect(
      diffValues(
        'captured',
        'gid://shopify/CompanyAddress/5?shopify-draft-proxy=synthetic',
        [addressRule],
        '$',
        bindings,
      ),
    ).toEqual([]);
    expect(
      diffValues(
        'captured',
        'gid://shopify/CompanyAddress/5?shopify-draft-proxy=synthetic',
        [secondAddressRule],
        '$',
        bindings,
      ),
    ).not.toEqual([]);
    expect(
      diffValues(
        'captured',
        'gid://shopify/CompanyLocation/5?shopify-draft-proxy=synthetic',
        [addressRule],
        '$',
        bindings,
      ),
    ).not.toEqual([]);
  });
});

describe('parity runner lifecycle Shopify GID aliases', () => {
  it('validates captured public setup requests separately from the scenario request', () => {
    const parsed = paritySpecSchema.parse({
      scenarioId: 'captured-public-setup',
      operationNames: ['productCreate', 'sellingPlanGroupCreate'],
      scenarioStatus: 'captured',
      assertionKinds: ['downstream-read-parity'],
      comparisonMode: 'captured-vs-proxy-request',
      liveCaptureFiles: ['fixtures/conformance/example/2026-04/products/setup.json'],
      proxySetups: [
        {
          name: 'member-product',
          captureResponsePath: '$.captures[0].response',
          proxyRequest: {
            documentCapturePath: '$.captures[0].request.query',
            variablesCapturePath: '$.captures[0].request.variables',
          },
        },
      ],
      proxyRequest: { documentCapturePath: '$.captures[1].request.query' },
      comparison: {
        mode: 'strict-json',
        expectedDifferences: [],
        targets: [{ name: 'create-group', capturePath: '$.captures[1].response.data', proxyPath: '$.data' }],
      },
    });

    expect(parsed.proxySetups).toEqual([
      {
        name: 'member-product',
        captureResponsePath: '$.captures[0].response',
        proxyRequest: {
          documentCapturePath: '$.captures[0].request.query',
          variablesCapturePath: '$.captures[0].request.variables',
        },
      },
    ]);
  });

  it('rewrites later request variables from corresponding public setup responses', () => {
    const bindings = createParityGidAliasBindings();
    bindCorrespondingGidAliases(
      {
        data: {
          productCreate: {
            product: {
              id: 'gid://shopify/Product/100',
              variants: [{ id: 'gid://shopify/ProductVariant/200' }],
            },
          },
        },
      },
      {
        data: {
          productCreate: {
            product: {
              id: 'gid://shopify/Product/1?shopify-draft-proxy=synthetic',
              variants: [{ id: 'gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic' }],
            },
          },
        },
      },
      bindings,
    );

    expect(
      rewriteBoundGidAliases(
        {
          productId: 'gid://shopify/Product/100',
          componentIds: ['gid://shopify/ProductVariant/200', 'gid://shopify/ProductVariant/unrelated'],
        },
        bindings,
      ),
    ).toEqual({
      productId: 'gid://shopify/Product/1?shopify-draft-proxy=synthetic',
      componentIds: [
        'gid://shopify/ProductVariant/2?shopify-draft-proxy=synthetic',
        'gid://shopify/ProductVariant/unrelated',
      ],
    });
    expect(
      diffValues(
        'gid://shopify/Product/100',
        'gid://shopify/Product/1?shopify-draft-proxy=synthetic',
        [],
        '$',
        bindings,
      ),
    ).toEqual([]);
    expect(
      diffValues(
        'gid://shopify/Product/other',
        'gid://shopify/Product/3?shopify-draft-proxy=synthetic',
        [],
        '$',
        bindings,
      ),
    ).not.toEqual([]);

    expect(
      diffValues(
        'Invalid id: gid://shopify/Product/100',
        'Invalid id: gid://shopify/Product/1?shopify-draft-proxy=synthetic',
        [],
        '$',
        bindings,
      ),
    ).toEqual([]);
    expect(
      diffValues(
        'Invalid id: gid://shopify/Product/unrelated',
        'Invalid id: gid://shopify/Product/1?shopify-draft-proxy=synthetic',
        [],
        '$',
        bindings,
      ),
    ).not.toEqual([]);
  });

  it('does not bind mismatched resource types or one captured id to multiple proxy ids', () => {
    const bindings = createParityGidAliasBindings();
    bindCorrespondingGidAliases(
      { id: 'gid://shopify/Product/100' },
      { id: 'gid://shopify/Collection/1?shopify-draft-proxy=synthetic' },
      bindings,
    );
    bindCorrespondingGidAliases(
      { id: 'gid://shopify/Product/100' },
      { id: 'gid://shopify/Product/1?shopify-draft-proxy=synthetic' },
      bindings,
    );
    bindCorrespondingGidAliases(
      { id: 'gid://shopify/Product/100' },
      { id: 'gid://shopify/Product/2?shopify-draft-proxy=synthetic' },
      bindings,
    );

    expect(rewriteBoundGidAliases('gid://shopify/Product/100', bindings)).toBe(
      'gid://shopify/Product/1?shopify-draft-proxy=synthetic',
    );
  });

  it('does not rebind a Shopify id already observed unchanged through a public read', () => {
    const bindings = createParityGidAliasBindings();
    bindCorrespondingGidAliases(
      { article: { id: 'gid://shopify/Article/100' } },
      { article: { id: 'gid://shopify/Article/100' } },
      bindings,
    );
    bindCorrespondingGidAliases(
      { article: { id: 'gid://shopify/Article/100' } },
      { article: { id: 'gid://shopify/Article/1?shopify-draft-proxy=synthetic' } },
      bindings,
    );

    expect(rewriteBoundGidAliases('gid://shopify/Article/100', bindings)).toBe('gid://shopify/Article/100');
  });
});

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

describe('parity runner explicit null boundaries', () => {
  const nullDifference = {
    path: '$.product.onlineStorePreviewUrl',
    matcher: 'null',
    reason: 'A local-only product has no authoritative Shopify preview URL.',
  };

  it('accepts null while rejecting a plausible-looking fabricated URL', () => {
    const capture = { product: { onlineStorePreviewUrl: 'https://example.shopifypreview.com/products_preview' } };

    expect(diffValues(capture, { product: { onlineStorePreviewUrl: null } }, [nullDifference])).toEqual([]);
    expect(
      diffValues(capture, { product: { onlineStorePreviewUrl: 'https://shopify-draft-proxy.preview/products/1' } }, [
        nullDifference,
      ]),
    ).toEqual([expect.stringContaining('$.product.onlineStorePreviewUrl')]);
  });

  it('is a validated parity-spec matcher', () => {
    expect(
      paritySpecSchema.parse({
        scenarioId: 'preview-null-boundary',
        operationNames: ['productSet'],
        scenarioStatus: 'captured',
        assertionKinds: ['nullability-parity'],
        liveCaptureFiles: ['fixtures/conformance/example/2025-01/products/preview.json'],
        proxyRequest: { documentPath: 'config/parity-requests/products/preview.graphql' },
        comparisonMode: 'captured-vs-proxy-request',
        comparison: {
          mode: 'strict-json',
          expectedDifferences: [nullDifference],
          targets: [{ name: 'preview', capturePath: '$.response', proxyPath: '$' }],
        },
      }).comparison?.expectedDifferences,
    ).toEqual([nullDifference]);
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
