/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createDraftProxy, type DraftProxy, type DraftProxyHttpResponse } from '../js/src/index.js';

type JsonRecord = Record<string, unknown>;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const apiVersion = '2026-04';
const scenarioId = 'orderUpdate-snapshot-staging';
const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'orders',
  'orderUpdate-snapshot-staging.json',
);
const specPath = path.join(repoRoot, 'config', 'parity-specs', 'orders', 'orderUpdate-snapshot-staging.json');

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(repoRoot, 'config', 'parity-requests', 'orders', name), 'utf8');
}

async function readJsonObject(relativePath: string): Promise<JsonRecord> {
  const value = JSON.parse(await readFile(path.join(repoRoot, relativePath), 'utf8')) as unknown;
  return readObject(value, relativePath);
}

function readObject(value: unknown, context: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
}

function readPath(value: unknown, segments: string[]): unknown {
  let current = value;
  for (const segment of segments) {
    if (!current || typeof current !== 'object' || Array.isArray(current)) return undefined;
    current = (current as JsonRecord)[segment];
  }
  return current;
}

function readRequiredString(value: unknown, segments: string[]): string {
  const found = readPath(value, segments);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`Missing required string at ${segments.join('.')}: ${JSON.stringify(value)}`);
  }
  return found;
}

function assertResponseOk(response: DraftProxyHttpResponse, context: string): JsonRecord {
  if (response.status !== 200) {
    throw new Error(`${context} returned HTTP ${response.status}: ${JSON.stringify(response.body)}`);
  }
  const body = readObject(response.body, `${context} response body`);
  if ('errors' in body) {
    throw new Error(`${context} returned top-level GraphQL errors: ${JSON.stringify(body['errors'])}`);
  }
  return body;
}

async function runProxyRequest(
  proxy: DraftProxy,
  query: string,
  variables: JsonRecord,
  context: string,
): Promise<JsonRecord> {
  return assertResponseOk(await proxy.processGraphQLRequest({ query, variables }, { apiVersion }), context);
}

function orderUpdateVariables(orderId: string): JsonRecord {
  return {
    input: {
      id: orderId,
      email: 'order-update-snapshot-after@example.com',
      phone: '+16135551111',
      poNumber: 'PO-SNAPSHOT-123',
      note: 'order update snapshot after',
      tags: ['snapshot-after', 'vip'],
      customAttributes: [{ key: 'source', value: 'snapshot-staging' }],
      shippingAddress: {
        firstName: 'Ada',
        lastName: 'Lovelace',
        address1: '190 MacLaren',
        address2: 'Suite 200',
        company: 'Analytical Engines Ltd',
        city: 'Sudbury',
        province: 'Ontario',
        provinceCode: 'ON',
        country: 'Canada',
        countryCode: 'CA',
        zip: 'K2P0V6',
        phone: '+16135552222',
      },
      metafields: [
        {
          namespace: 'custom',
          key: 'gift',
          type: 'single_line_text_field',
          value: 'wrapped',
        },
      ],
    },
  };
}

function orderUpdateSpecVariables(): JsonRecord {
  return {
    input: {
      id: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
      email: 'order-update-snapshot-after@example.com',
      phone: '+16135551111',
      poNumber: 'PO-SNAPSHOT-123',
      note: 'order update snapshot after',
      tags: ['snapshot-after', 'vip'],
      customAttributes: [{ key: 'source', value: 'snapshot-staging' }],
      shippingAddress: {
        firstName: 'Ada',
        lastName: 'Lovelace',
        address1: '190 MacLaren',
        address2: 'Suite 200',
        company: 'Analytical Engines Ltd',
        city: 'Sudbury',
        province: 'Ontario',
        provinceCode: 'ON',
        country: 'Canada',
        countryCode: 'CA',
        zip: 'K2P0V6',
        phone: '+16135552222',
      },
      metafields: [
        {
          namespace: 'custom',
          key: 'gift',
          type: 'single_line_text_field',
          value: 'wrapped',
        },
      ],
    },
  };
}

function formatGeneratedFiles(): void {
  const result = spawnSync('corepack', ['pnpm', 'exec', 'oxfmt', fixturePath, specPath], {
    cwd: repoRoot,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });
  if (result.status !== 0) {
    throw new Error(`Generated JSON formatting failed with status ${String(result.status)}`);
  }
}

const createQuery = await readRequest('orderUpdate-snapshot-staging-create.graphql');
const updateQuery = await readRequest('orderUpdate-snapshot-staging.graphql');
const readQuery = await readRequest('orderUpdate-snapshot-staging-read.graphql');
const createVariables = await readJsonObject(
  'config/parity-requests/orders/orderUpdate-snapshot-staging-create.variables.json',
);

const proxy = createDraftProxy({
  readMode: 'snapshot',
  unsupportedMutationMode: 'reject',
  port: 0,
  shopifyAdminOrigin: 'https://local-runtime.invalid',
});

try {
  const create = await runProxyRequest(proxy, createQuery, createVariables, 'orderCreate setup');
  const orderId = readRequiredString(create, ['data', 'orderCreate', 'order', 'id']);
  const updateVariables = orderUpdateVariables(orderId);
  const update = await runProxyRequest(proxy, updateQuery, updateVariables, 'orderUpdate snapshot staging');
  const downstreamRead = await runProxyRequest(proxy, readQuery, { id: orderId }, 'orderUpdate downstream read');
  const log = readObject(proxy.getLog(), 'proxy log');

  const fixture = {
    fixtureKind: 'local-runtime-order-update-snapshot-staging',
    apiVersion,
    capturedAt: '2026-06-15T00:00:00.000Z',
    storeDomain: 'local-runtime.myshopify.com',
    summary:
      'Executable local-runtime fixture for orderCreate -> orderUpdate -> downstream order reads using only public proxy GraphQL requests.',
    create: {
      variables: createVariables,
    },
    update: {
      variables: updateVariables,
    },
    downstreamRead: {
      variables: { id: orderId },
    },
    expected: {
      create,
      update,
      downstreamRead,
      logEntries: log['entries'],
    },
    evidence: {
      source: 'Rust local-runtime recorder plus existing live Shopify orderUpdate happy-path fixture',
      liveCaptureFiles: [
        'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-update-parity.json',
      ],
      notes: [
        'The local-runtime fixture proves snapshot staging and read-after-write mechanics without runtime Shopify writes.',
        'The linked live fixture remains the Shopify evidence for valid orderUpdate note/tags behavior; the expanded simple-field snapshot slice is additionally covered by Rust runtime tests.',
      ],
    },
    upstreamCalls: [],
  };

  const spec = {
    scenarioId,
    operationNames: ['orderCreate', 'orderUpdate', 'order', 'orders', 'ordersCount'],
    scenarioStatus: 'captured',
    assertionKinds: [
      'runtime-staging',
      'read-after-write',
      'downstream-read-parity',
      'mutation-log-raw-body',
      'no-upstream-passthrough',
      'local-runtime-backed',
    ],
    liveCaptureFiles: [
      'fixtures/conformance/local-runtime/2026-04/orders/orderUpdate-snapshot-staging.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-update-parity.json',
    ],
    runtimeTestFiles: ['tests/graphql_routes/orders.rs'],
    proxyRequest: {
      documentPath: 'config/parity-requests/orders/orderUpdate-snapshot-staging-create.graphql',
      variablesPath: 'config/parity-requests/orders/orderUpdate-snapshot-staging-create.variables.json',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'create-baseline-order',
          capturePath: '$.expected.create',
          proxyPath: '$',
        },
        {
          name: 'order-update-stages-simple-fields',
          capturePath: '$.expected.update',
          proxyPath: '$',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/orderUpdate-snapshot-staging.graphql',
            variables: orderUpdateSpecVariables(),
            apiVersion,
          },
        },
        {
          name: 'downstream-order-reads-reflect-update',
          capturePath: '$.expected.downstreamRead',
          proxyPath: '$',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/orderUpdate-snapshot-staging-read.graphql',
            variables: {
              id: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
            },
            apiVersion,
          },
        },
        {
          name: 'mutation-log-retains-raw-create-and-update',
          capturePath: '$.expected.logEntries',
          proxyLogPath: '$.entries',
        },
      ],
    },
    notes:
      'Executable local-runtime snapshot staging proof for orderUpdate over a public orderCreate setup. The scenario stages note, tags, customAttributes, email, phone, poNumber, shippingAddress, and an order-scoped metafield; then it verifies immediate order(id:), orders, and ordersCount read-after-write plus raw mutation-log retention. The existing live Shopify fixture remains linked as note/tags happy-path evidence, while this local-runtime fixture intentionally performs no Shopify writes.',
  };

  await mkdir(path.dirname(fixturePath), { recursive: true });
  await mkdir(path.dirname(specPath), { recursive: true });
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');
  formatGeneratedFiles();
  console.log(`Wrote ${path.relative(repoRoot, fixturePath)}`);
  console.log(`Wrote ${path.relative(repoRoot, specPath)}`);
} finally {
  proxy.dispose();
}
