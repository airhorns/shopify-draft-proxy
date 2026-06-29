import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type CaptureEntry = {
  document: string;
  variables: JsonRecord;
  response: ConformanceGraphqlPayload<JsonRecord>;
};

const scenarioId = 'draftOrderBulkTag-case-preservation';
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const fixturePath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'orders',
  'draft-order-bulk-tag-case-preservation.json',
);
const specPath = path.join('config', 'parity-specs', 'orders', `${scenarioId}.json`);

const createDocumentPath = 'config/parity-requests/orders/draftOrderBulkTag-validation-create.graphql';
const addDocumentPath = 'config/parity-requests/orders/draft-order-residual-helper-bulk-add-tags.graphql';
const readDocumentPath = 'config/parity-requests/orders/draftOrderBulkTag-validation-read.graphql';

const deleteDocument = `#graphql
  mutation DraftOrderBulkTagCasePreservationDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function absolutePath(relativePath: string): string {
  return path.join(repoRoot, relativePath);
}

async function readText(relativePath: string): Promise<string> {
  return readFile(absolutePath(relativePath), 'utf8');
}

async function writeJson(relativePath: string, value: unknown): Promise<void> {
  await mkdir(path.dirname(absolutePath(relativePath)), { recursive: true });
  await writeFile(absolutePath(relativePath), `${JSON.stringify(value, null, 2)}\n`);
}

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readRecord(value: unknown, key: string): JsonRecord | null {
  return asRecord(asRecord(value)?.[key]);
}

function readString(value: unknown, key: string): string | null {
  const fieldValue = asRecord(value)?.[key];
  return typeof fieldValue === 'string' && fieldValue.length > 0 ? fieldValue : null;
}

function draftOrderIdFromCreate(payload: ConformanceGraphqlPayload<JsonRecord>): string | null {
  return readString(readRecord(readRecord(payload.data, 'draftOrderCreate'), 'draftOrder'), 'id');
}

function draftOrderTagsFromRead(payload: ConformanceGraphqlPayload<JsonRecord>): string[] {
  const tags = readRecord(payload.data, 'draftOrder')?.['tags'];
  return Array.isArray(tags) ? tags.filter((tag): tag is string => typeof tag === 'string') : [];
}

function sleep(milliseconds: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}

async function captureReadUntilTags(
  document: string,
  draftOrderId: string,
  expectedTags: string[],
): Promise<{ entry: CaptureEntry; attempts: number }> {
  let latest: CaptureEntry | null = null;
  for (let attempt = 1; attempt <= 20; attempt += 1) {
    latest = await capture(document, { id: draftOrderId });
    const tags = draftOrderTagsFromRead(latest.response);
    if (expectedTags.every((tag) => tags.includes(tag))) {
      return { entry: latest, attempts: attempt };
    }
    await sleep(500);
  }

  throw new Error(
    `Expected draft order tags ${JSON.stringify(expectedTags)} after bulk add; latest read was ${JSON.stringify(
      latest,
      null,
      2,
    )}`,
  );
}

function paritySpec(): JsonRecord {
  return {
    scenarioId,
    operationNames: ['draftOrderCreate', 'draftOrderBulkAddTags', 'draftOrder'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'downstream-read-parity', 'runtime-staging'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes.rs'],
    proxyRequest: {
      documentPath: createDocumentPath,
      variablesCapturePath: '$.setup.draftOrderCreate.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'setup-create-user-errors',
          capturePath: '$.setup.draftOrderCreate.response.data.draftOrderCreate.userErrors',
          proxyPath: '$.data.draftOrderCreate.userErrors',
        },
        {
          name: 'bulk-add-tags-payload',
          capturePath: '$.draftOrderBulkAddTags.response.data.draftOrderBulkAddTags',
          proxyPath: '$.data.draftOrderBulkAddTags',
          proxyRequest: {
            documentPath: addDocumentPath,
            variables: {
              ids: [{ fromPrimaryProxyPath: '$.data.draftOrderCreate.draftOrder.id' }],
              tags: [' vip ', ' Wholesale ', 'wholesale'],
            },
            apiVersion,
          },
          expectedDifferences: [
            {
              path: '$.job.id',
              matcher: 'shopify-gid:Job',
              reason: 'The proxy creates a deterministic local Job id while Shopify returns an async live Job id.',
            },
          ],
        },
        {
          name: 'read-after-case-preserving-bulk-add',
          capturePath: '$.draftOrderBulkAddTags.downstreamRead.response.data.draftOrder',
          proxyPath: '$.data.draftOrder',
          proxyRequest: {
            documentPath: readDocumentPath,
            variables: {
              id: { fromPrimaryProxyPath: '$.data.draftOrderCreate.draftOrder.id' },
            },
            apiVersion,
          },
          expectedDifferences: [
            {
              path: '$.id',
              matcher: 'shopify-gid:DraftOrder',
              reason: 'The local setup draft gets a synthetic DraftOrder GID.',
            },
          ],
        },
      ],
    },
    notes:
      'Live Shopify bulk-add trims tags and deduplicates by case-insensitive identity while preserving the display case of the first stored occurrence. The scenario uses a disposable draft order and public GraphQL requests only.',
  };
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function capture(document: string, variables: JsonRecord): Promise<CaptureEntry> {
  const result = await runGraphqlRequest<JsonRecord>(document, variables);
  return {
    document,
    variables,
    response: result.payload,
  };
}

const createDocument = await readText(createDocumentPath);
const addDocument = await readText(addDocumentPath);
const readDocument = await readText(readDocumentPath);

const stamp = Date.now();
const createVariables = {
  input: {
    email: `draft-bulk-tag-case-${stamp}@example.com`,
    tags: ['VIP'],
    lineItems: [
      {
        title: 'Bulk tag case preservation item',
        quantity: 1,
        originalUnitPrice: '2.00',
      },
    ],
  },
};

const createdDraftOrderIds: string[] = [];

try {
  const draftOrderCreate = await capture(createDocument, createVariables);
  const draftOrderId = draftOrderIdFromCreate(draftOrderCreate.response);
  if (!draftOrderId) {
    throw new Error(`Expected draftOrderCreate.draftOrder.id: ${JSON.stringify(draftOrderCreate, null, 2)}`);
  }
  createdDraftOrderIds.push(draftOrderId);

  const addVariables = { ids: [draftOrderId], tags: [' vip ', ' Wholesale ', 'wholesale'] };
  const draftOrderBulkAddTags = await capture(addDocument, addVariables);
  const readAfterBulkAdd = await captureReadUntilTags(readDocument, draftOrderId, ['VIP', 'Wholesale']);

  await writeJson(fixturePath, {
    metadata: {
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      description: 'Draft-order bulk tag case preservation and case-insensitive dedupe read-after-write behavior.',
    },
    setup: {
      draftOrderCreate,
    },
    draftOrderBulkAddTags: {
      ...draftOrderBulkAddTags,
      downstreamRead: readAfterBulkAdd.entry,
      downstreamReadAttempts: readAfterBulkAdd.attempts,
    },
    upstreamCalls: [],
  });
  await writeJson(specPath, paritySpec());
} finally {
  const uniqueDraftOrderIds = [...new Set(createdDraftOrderIds)];
  await Promise.allSettled(uniqueDraftOrderIds.map((id) => runGraphql(deleteDocument, { input: { id } })));
}

// oxlint-disable-next-line no-console -- CLI scripts intentionally write status output to stdout.
console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixture: fixturePath, spec: specPath }, null, 2));
