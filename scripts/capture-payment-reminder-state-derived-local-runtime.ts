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
const scenarioId = 'payment-reminder-state-derived-local-staging';
const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'payments',
  `${scenarioId}.json`,
);
const specPath = path.join(repoRoot, 'config', 'parity-specs', 'payments', `${scenarioId}.json`);

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(repoRoot, 'config', 'parity-requests', 'payments', name), 'utf8');
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
    if (Array.isArray(current)) {
      const index = Number(segment);
      if (!Number.isInteger(index) || index < 0) return undefined;
      current = current[index];
      continue;
    }
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

const orderCreateQuery = await readRequest('payment-reminder-state-derived-order-create.graphql');
const paymentTermsCreateQuery = await readRequest('payment-reminder-state-derived-payment-terms-create.graphql');
const reminderQuery = await readRequest('payment-reminder-send.graphql');
const createVariables = {
  order: {
    currency: 'CAD',
    presentmentCurrency: 'CAD',
    email: 'payment-reminder-state-derived@example.test',
    financialStatus: 'PENDING',
    lineItems: [
      {
        title: 'State-derived payment reminder',
        quantity: 1,
        priceSet: {
          shopMoney: { amount: '10.00', currencyCode: 'CAD' },
          presentmentMoney: { amount: '10.00', currencyCode: 'CAD' },
        },
        taxable: false,
      },
    ],
  },
};
const createPaymentTermsAttrs = {
  paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
  paymentSchedules: [{ issuedAt: '2026-05-05T00:00:00Z' }],
};

const proxy = createDraftProxy({
  readMode: 'live-hybrid',
  unsupportedMutationMode: 'reject',
  port: 0,
  shopifyAdminOrigin: 'https://local-runtime.invalid',
});

try {
  const orderCreate = await runProxyRequest(proxy, orderCreateQuery, createVariables, 'orderCreate setup');
  const orderId = readRequiredString(orderCreate, ['data', 'orderCreate', 'order', 'id']);
  const termsCreateVariables = {
    referenceId: orderId,
    attrs: createPaymentTermsAttrs,
  };
  const paymentTermsCreate = await runProxyRequest(
    proxy,
    paymentTermsCreateQuery,
    termsCreateVariables,
    'paymentTermsCreate setup',
  );
  const scheduleId = readRequiredString(paymentTermsCreate, [
    'data',
    'paymentTermsCreate',
    'paymentTerms',
    'paymentSchedules',
    'nodes',
    '0',
    'id',
  ]);
  const reminderVariables = { paymentScheduleId: scheduleId };
  const firstReminder = await runProxyRequest(proxy, reminderQuery, reminderVariables, 'paymentReminderSend first');
  const secondReminder = await runProxyRequest(proxy, reminderQuery, reminderVariables, 'paymentReminderSend second');

  const fixture = {
    fixtureKind: 'local-runtime-payment-reminder-state-derived',
    scenarioId,
    storeDomain: 'local-runtime',
    apiVersion,
    capturedAt: '2026-07-03T00:00:00.000Z',
    summary:
      'Executable local-runtime parity for paymentReminderSend resolving a PaymentSchedule created by public orderCreate and paymentTermsCreate requests.',
    setup: {
      orderCreate: { variables: createVariables },
      paymentTermsCreate: { variables: termsCreateVariables },
      paymentReminderSend: { variables: reminderVariables },
    },
    expected: {
      orderCreate,
      paymentTermsCreate,
      firstReminder,
      secondReminder,
    },
    evidence: {
      source: 'Rust local-runtime recorder plus existing live Shopify paymentReminderSend eligibility fixture',
      liveCaptureFiles: [
        'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-reminder-send-eligibility.json',
      ],
      notes: [
        'The linked live fixture remains the Shopify evidence for eligible overdue order-owned PaymentSchedule reminder success and paid-schedule userErrors.',
        'This local-runtime fixture proves the proxy can resolve a synthetic PaymentSchedule from staged payment terms instead of using baked PaymentSchedule GID cases or runtime Shopify writes.',
      ],
    },
    upstreamCalls: [],
  };

  const spec = {
    scenarioId,
    operationNames: ['orderCreate', 'paymentTermsCreate', 'paymentReminderSend'],
    scenarioStatus: 'captured',
    assertionKinds: [
      'runtime-staging',
      'payload-shape',
      'user-errors-parity',
      'no-upstream-passthrough',
      'local-runtime-backed',
    ],
    liveCaptureFiles: [
      'fixtures/conformance/local-runtime/2026-04/payments/payment-reminder-state-derived-local-staging.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-reminder-send-eligibility.json',
    ],
    runtimeTestFiles: ['tests/graphql_routes/orders.rs'],
    proxyRequest: {
      documentPath: 'config/parity-requests/payments/payment-reminder-state-derived-order-create.graphql',
      variables: createVariables,
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'order-create-setup',
          capturePath: '$.expected.orderCreate',
          proxyPath: '$',
        },
        {
          name: 'payment-terms-create-setup',
          capturePath: '$.expected.paymentTermsCreate',
          proxyPath: '$',
          proxyRequest: {
            documentPath: 'config/parity-requests/payments/payment-reminder-state-derived-payment-terms-create.graphql',
            variables: {
              referenceId: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
              attrs: createPaymentTermsAttrs,
            },
            apiVersion,
          },
        },
        {
          name: 'first-reminder-uses-created-schedule',
          capturePath: '$.expected.firstReminder',
          proxyPath: '$',
          proxyRequest: {
            documentPath: 'config/parity-requests/payments/payment-reminder-send.graphql',
            variables: {
              paymentScheduleId: {
                fromProxyResponse: 'payment-terms-create-setup',
                path: '$.data.paymentTermsCreate.paymentTerms.paymentSchedules.nodes[0].id',
              },
            },
            apiVersion,
          },
        },
        {
          name: 'second-reminder-hits-staged-dedup-window',
          capturePath: '$.expected.secondReminder',
          proxyPath: '$',
          proxyRequest: {
            documentPath: 'config/parity-requests/payments/payment-reminder-send.graphql',
            variables: {
              paymentScheduleId: {
                fromProxyResponse: 'payment-terms-create-setup',
                path: '$.data.paymentTermsCreate.paymentTerms.paymentSchedules.nodes[0].id',
              },
            },
            apiVersion,
          },
        },
      ],
    },
    notes:
      'Executable local-runtime parity for the state-derived paymentReminderSend path requested during review. The scenario earns its PaymentSchedule through public orderCreate and paymentTermsCreate requests, then calls paymentReminderSend with that synthetic schedule ID. The old literal PaymentSchedule GID table would not locally handle this schedule.',
  };

  await mkdir(path.dirname(fixturePath), { recursive: true });
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  await mkdir(path.dirname(specPath), { recursive: true });
  await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');
  formatGeneratedFiles();
  console.log(`Wrote ${path.relative(repoRoot, fixturePath)}`);
  console.log(`Wrote ${path.relative(repoRoot, specPath)}`);
} finally {
  proxy.dispose();
}
