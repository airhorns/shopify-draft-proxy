/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

type JsonRecord = Record<string, unknown>;
type ProxyResponse = {
  status: number;
  body: unknown;
};
type DraftProxyInstance = {
  processGraphQLRequest: (
    body: { query: string; variables?: JsonRecord },
    options?: { apiVersion?: string },
  ) => Promise<ProxyResponse>;
  getState: () => JsonRecord;
  getLog: () => JsonRecord;
};

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const apiVersion = '2026-04';
const scenarioId = 'draft-order-invoice-send-invoice-errors';
const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'orders',
  'draft-order-invoice-send-invoice-errors.json',
);

function ensureGleamJsBuild(): void {
  const result = spawnSync('corepack', ['pnpm', 'gleam:build:js'], {
    cwd: repoRoot,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });

  if (result.status !== 0) {
    throw new Error(`Gleam JS build failed with status ${String(result.status)}`);
  }
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

function assertResponseOk(response: ProxyResponse, context: string): JsonRecord {
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
  proxy: DraftProxyInstance,
  query: string,
  variables: JsonRecord,
  context: string,
): Promise<JsonRecord> {
  return assertResponseOk(await proxy.processGraphQLRequest({ query, variables }, { apiVersion }), context);
}

ensureGleamJsBuild();

const { createDraftProxy } = (await import('../js/src/index.js')) as {
  createDraftProxy: (options: { readMode: string; port: number; shopifyAdminOrigin: string }) => DraftProxyInstance;
};

const proxy = createDraftProxy({
  readMode: 'live-hybrid',
  port: 0,
  shopifyAdminOrigin: 'https://local-runtime.invalid',
});

const createQuery = await readRequest('draftOrderInvoiceSend-invoice-errors-create.graphql');
const sendQuery = await readRequest('draftOrderInvoiceSend-invoice-errors-send.graphql');

const createOpen = await runProxyRequest(proxy, createQuery, {}, 'create open draft');
const draftOrderId = readRequiredString(createOpen, ['data', 'draftOrderCreate', 'draftOrder', 'id']);

const noRecipient = await runProxyRequest(
  proxy,
  sendQuery,
  {
    id: draftOrderId,
    email: null,
    currency: null,
    template: null,
  },
  'send invoice with no recipient',
);

const validSend = await runProxyRequest(
  proxy,
  sendQuery,
  {
    id: draftOrderId,
    email: {
      to: 'buyer@example.com',
      subject: 'Draft invoice',
      customMessage: 'Thanks for the order',
      from: 'sales@example.com',
      bcc: ['ops@example.com', 'archive@example.com'],
    },
    currency: 'USD',
    template: 'DRAFT_ORDER_INVOICE',
  },
  'send invoice with recipient and metadata',
);

const fixture = {
  capturedAt: '2026-05-13T23:25:00.000Z',
  source: 'local-runtime-capture-script',
  storeDomain: 'local-runtime',
  apiVersion,
  scenarioId,
  publicSchemaProbe: {
    apiVersionsChecked: ['2025-01', '2026-04', 'unstable'],
    draftOrderInvoiceSendArgs: ['id', 'email'],
    draftOrderInvoiceSendPayloadFields: ['draftOrder', 'userErrors'],
    draftOrderInvoiceErrorType: null,
    draftOrderEmailTemplateType: null,
  },
  createOpen: {
    request: { documentPath: 'config/parity-requests/orders/draftOrderInvoiceSend-invoice-errors-create.graphql' },
    response: createOpen,
  },
  noRecipient: {
    request: {
      documentPath: 'config/parity-requests/orders/draftOrderInvoiceSend-invoice-errors-send.graphql',
      variables: {
        id: draftOrderId,
        email: null,
        currency: null,
        template: null,
      },
    },
    response: noRecipient,
  },
  validSend: {
    request: {
      documentPath: 'config/parity-requests/orders/draftOrderInvoiceSend-invoice-errors-send.graphql',
      variables: {
        id: draftOrderId,
        email: {
          to: 'buyer@example.com',
          subject: 'Draft invoice',
          customMessage: 'Thanks for the order',
          from: 'sales@example.com',
          bcc: ['ops@example.com', 'archive@example.com'],
        },
        currency: 'USD',
        template: 'DRAFT_ORDER_INVOICE',
      },
    },
    response: validSend,
    state: proxy.getState(),
    log: proxy.getLog(),
  },
  notes: [
    'Executable local-runtime fixture for private draftOrderInvoiceSend invoiceErrors and invoice metadata behavior.',
    'Valid live conformance credentials were available, but public Admin schemas through unstable did not expose invoiceErrors, DraftOrderInvoiceError, DraftOrderEmailTemplate, templateName, or presentmentCurrencyCode for this mutation.',
    'The scenario starts cold, creates the draft through the supported local draftOrderCreate mutation, and uses no upstream calls.',
  ],
  upstreamCalls: [],
};

await mkdir(path.dirname(fixturePath), { recursive: true });
await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`);
formatFixture();

console.log(JSON.stringify({ ok: true, fixturePath: path.relative(repoRoot, fixturePath), draftOrderId }, null, 2));
