/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import { spawnSync } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createDraftProxy, type DraftProxy, type DraftProxyHttpResponse } from '../js/src/index.js';

type JsonRecord = Record<string, unknown>;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const apiVersion = '2026-04';
const scenarioId = 'customer-payment-method-no-auto-seed';
const requestPath = path.join(repoRoot, 'config', 'parity-requests', 'customers', `${scenarioId}.graphql`);
const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'customers',
  `${scenarioId}.json`,
);
const specPath = path.join(repoRoot, 'config', 'parity-specs', 'customers', `${scenarioId}.json`);

const noAutoSeedRequest = `query CustomerPaymentMethodNoAutoSeed {
  baseCard: customerPaymentMethod(
    id: "gid://shopify/CustomerPaymentMethod/base-card"
    showRevoked: true
  ) {
    id
    instrument {
      __typename
    }
    customer {
      id
    }
  }
  customer(id: "gid://shopify/Customer/8801") {
    paymentMethods(first: 10, showRevoked: true) {
      nodes {
        id
      }
    }
  }
}
`;

function readObject(value: unknown, context: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
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

async function runProxyRequest(proxy: DraftProxy, query: string, context: string): Promise<JsonRecord> {
  return assertResponseOk(await proxy.processGraphQLRequest({ query, variables: {} }, { apiVersion }), context);
}

function formatGeneratedFiles(): void {
  const result = spawnSync('corepack', ['pnpm', 'exec', 'oxfmt', requestPath, fixturePath, specPath], {
    cwd: repoRoot,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });
  if (result.status !== 0) {
    throw new Error(`Generated file formatting failed with status ${String(result.status)}`);
  }
}

const proxy = createDraftProxy({
  readMode: 'snapshot',
  unsupportedMutationMode: 'reject',
  port: 0,
  shopifyAdminOrigin: 'https://local-runtime.invalid',
});

try {
  const response = await runProxyRequest(proxy, noAutoSeedRequest, 'customer payment method no-auto-seed read');

  const fixture = {
    fixtureKind: 'local-runtime-customer-payment-method-no-auto-seed',
    scenarioId,
    apiVersion,
    capturedAt: '2026-07-03T00:00:00.000Z',
    storeDomain: 'local-runtime.myshopify.com',
    summary:
      'Executable local-runtime fixture proving unhydrated customer payment-method reads do not auto-seed sentinel fixture records under real customer IDs.',
    request: {
      variables: {},
    },
    expected: {
      response,
    },
    evidence: {
      source: 'Rust local-runtime recorder using public Admin GraphQL requests',
      notes: [
        'Live customer payment-method success captures remain blocked by missing read_customer_payment_methods/write_customer_payment_methods scopes.',
        'This local-runtime scenario protects the anti-seeding contract: a blank snapshot proxy must return null/empty data rather than fabricating base-card records for Customer/8801.',
      ],
    },
    upstreamCalls: [],
  };

  const spec = {
    scenarioId,
    operationNames: ['customerPaymentMethod', 'customer'],
    scenarioStatus: 'captured',
    assertionKinds: ['snapshot-read', 'no-fixture-seeding', 'local-runtime-backed'],
    liveCaptureFiles: [`fixtures/conformance/local-runtime/${apiVersion}/customers/${scenarioId}.json`],
    runtimeTestFiles: ['tests/graphql_routes/orders.rs'],
    proxyConfig: {
      readMode: 'snapshot',
    },
    proxyRequest: {
      documentPath: `config/parity-requests/customers/${scenarioId}.graphql`,
      variables: {},
      apiVersion,
    },
    comparisonMode: 'captured-fixture',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'unhydrated-customer-payment-methods-stay-empty',
          capturePath: '$.expected.response',
          proxyPath: '$',
        },
      ],
    },
    notes:
      'Local-runtime parity for the customer payment-method anti-seeding regression. Before the de-seeding change, this request would have materialized the sentinel base-card method and Customer/8801 paymentMethods from fixture state; the protected contract is null/empty snapshot behavior until payment methods are hydrated or staged through public runtime paths.',
  };

  await mkdir(path.dirname(requestPath), { recursive: true });
  await mkdir(path.dirname(fixturePath), { recursive: true });
  await mkdir(path.dirname(specPath), { recursive: true });
  await writeFile(requestPath, noAutoSeedRequest);
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`);
  await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`);
  formatGeneratedFiles();
} finally {
  proxy.dispose();
}
