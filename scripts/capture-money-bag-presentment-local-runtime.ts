/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createDraftProxy, type DraftProxy, type DraftProxyHttpResponse } from '../js/src/index.js';

type JsonRecord = Record<string, unknown>;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const apiVersion = '2025-01';
const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  '2026-05',
  'orders',
  'money-bag-presentment-parity.json',
);

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(repoRoot, 'config', 'parity-requests', 'orders', name), 'utf8');
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
    if (!current || typeof current !== 'object' || Array.isArray(current)) {
      return undefined;
    }
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

function formatFixture(): void {
  const result = spawnSync('corepack', ['pnpm', 'exec', 'oxfmt', fixturePath], {
    cwd: repoRoot,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });

  if (result.status !== 0) {
    throw new Error(`Fixture formatting failed with status ${String(result.status)}`);
  }
}

const singleCurrencyVariables = {
  order: {
    currency: 'USD',
    lineItems: [
      {
        title: 'MoneyBag line',
        quantity: 1,
        priceSet: {
          shopMoney: {
            amount: '12.00',
            currencyCode: 'USD',
          },
        },
        taxLines: [
          {
            title: 'Line tax',
            rate: 0.125,
            priceSet: {
              shopMoney: {
                amount: '1.50',
                currencyCode: 'USD',
              },
            },
          },
        ],
      },
    ],
  },
};

const multiCurrencyVariables = {
  order: {
    currency: 'CAD',
    lineItems: [
      {
        title: 'FX line',
        quantity: 1,
        priceSet: {
          shopMoney: {
            amount: '10.00',
            currencyCode: 'CAD',
          },
          presentmentMoney: {
            amount: '7.00',
            currencyCode: 'USD',
          },
        },
      },
    ],
  },
};

const proxy = createDraftProxy({
  readMode: 'live-hybrid',
  unsupportedMutationMode: 'reject',
  port: 0,
  shopifyAdminOrigin: 'https://local-runtime.invalid',
});

try {
  const [
    singleCreateDocument,
    multiCreateDocument,
    markAsPaidDocument,
    refundDocument,
    editBeginDocument,
    editCommitDocument,
  ] = await Promise.all([
    readRequest('money-bag-presentment-single-create.graphql'),
    readRequest('money-bag-presentment-multi-create.graphql'),
    readRequest('money-bag-presentment-mark-as-paid.graphql'),
    readRequest('money-bag-presentment-refund.graphql'),
    readRequest('money-bag-presentment-order-edit-begin.graphql'),
    readRequest('money-bag-presentment-order-edit-commit.graphql'),
  ]);

  const singleCurrencyCreate = await runProxyRequest(
    proxy,
    singleCreateDocument,
    singleCurrencyVariables,
    'single-currency orderCreate',
  );
  const orderId = readRequiredString(singleCurrencyCreate, ['data', 'orderCreate', 'order', 'id']);
  const multiCurrencyCreate = await runProxyRequest(
    proxy,
    multiCreateDocument,
    multiCurrencyVariables,
    'multi-currency orderCreate',
  );
  const markAsPaid = await runProxyRequest(proxy, markAsPaidDocument, { input: { id: orderId } }, 'orderMarkAsPaid');
  const refund = await runProxyRequest(
    proxy,
    refundDocument,
    {
      input: {
        orderId,
        allowOverRefunding: true,
        transactions: [{ amount: '5.00', gateway: 'manual', kind: 'REFUND', orderId }],
      },
    },
    'refundCreate',
  );
  const orderEditBegin = await runProxyRequest(proxy, editBeginDocument, { id: orderId }, 'orderEditBegin');
  const calculatedOrderId = readRequiredString(orderEditBegin, ['data', 'orderEditBegin', 'calculatedOrder', 'id']);
  const orderEditCommit = await runProxyRequest(
    proxy,
    editCommitDocument,
    { id: calculatedOrderId },
    'orderEditCommit',
  );

  const fixture = {
    capturedAt: '2026-07-03T04:00:00.000Z',
    source: 'local-runtime-capture-script',
    apiVersion,
    liveGatewaySideEffects: false,
    notes:
      'Local-runtime strict parity fixture for MoneyBag presentment coverage. The empty upstream cassette proves supported order/refund/edit mutations remain locally staged for this scenario.',
    singleCurrencyCreate: {
      variables: singleCurrencyVariables,
      expected: singleCurrencyCreate,
    },
    multiCurrencyCreate: {
      variables: multiCurrencyVariables,
      expected: multiCurrencyCreate,
    },
    markAsPaid: {
      expected: markAsPaid,
    },
    refund: {
      expected: refund,
    },
    orderEditBegin: {
      expected: orderEditBegin,
    },
    orderEditCommit: {
      expected: orderEditCommit,
    },
    upstreamCalls: [],
  };

  await mkdir(path.dirname(fixturePath), { recursive: true });
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`);
  formatFixture();

  console.log(JSON.stringify({ ok: true, fixturePath: path.relative(repoRoot, fixturePath) }, null, 2));
} finally {
  proxy.dispose();
}
