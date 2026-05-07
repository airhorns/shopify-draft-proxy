import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type CaptureEntry = {
  variables: JsonRecord;
  response: ConformanceGraphqlPayload<JsonRecord>;
};

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const outputPath = path.join(fixtureDir, 'draftOrder-tag-validation.json');

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

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

function readString(value: unknown, key: string): string | null {
  const fieldValue = asRecord(value)?.[key];
  return typeof fieldValue === 'string' && fieldValue.length > 0 ? fieldValue : null;
}

function draftOrderIdFromPayload(payload: ConformanceGraphqlPayload<JsonRecord>, rootName: string): string | null {
  const root = asRecord(asRecord(payload.data)?.[rootName]);
  return readString(asRecord(root?.['draftOrder']), 'id');
}

function numberedTags(count: number): string[] {
  return Array.from({ length: count }, (_, index) => `tag-${index + 1}`);
}

function inputWithTags(tags: string[], email: string): JsonRecord {
  return {
    input: {
      email,
      tags,
      lineItems: [
        {
          title: 'Tag validation item',
          quantity: 1,
          originalUnitPrice: '1.00',
        },
      ],
    },
  };
}

async function capture(document: string, variables: JsonRecord): Promise<CaptureEntry> {
  const result = await runGraphqlRequest<JsonRecord>(document, variables);
  return {
    variables,
    response: result.payload,
  };
}

const createDocument = await readText('config/parity-requests/orders/draftOrder-tag-validation-create.graphql');
const updateDocument = await readText('config/parity-requests/orders/draftOrder-tag-validation-update.graphql');
const calculateDocument = await readText('config/parity-requests/orders/draftOrder-tag-validation-calculate.graphql');
const readDocument = await readText('config/parity-requests/orders/draftOrder-tag-validation-read.graphql');
const deleteDocument = `#graphql
  mutation DraftOrderTagValidationDelete($input: DraftOrderDeleteInput!) {
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

const stamp = Date.now().toString();
const longTag = 'x'.repeat(41);
const normalizedOkTags = [...numberedTags(248), ' tag-248 ', 'TAG-249'];
const tooManyTags = [...numberedTags(250), 'TAG-251'];
const createdDraftOrderIds: string[] = [];

const setupCreateVariables = inputWithTags(['initial'], `draft-tags-setup-${stamp}@example.com`);
const createNormalizedOkVariables = inputWithTags(normalizedOkTags, `draft-tags-normalized-${stamp}@example.com`);
const createLongTagVariables = inputWithTags([longTag], `draft-tags-long-${stamp}@example.com`);
const createTooManyTagsVariables = inputWithTags(tooManyTags, `draft-tags-count-${stamp}@example.com`);
const calculateLongTagVariables = inputWithTags([longTag], `draft-tags-calc-long-${stamp}@example.com`);
const calculateTooManyTagsVariables = inputWithTags(tooManyTags, `draft-tags-calc-count-${stamp}@example.com`);

let setupCreate: CaptureEntry | null = null;
let createNormalizedOk: CaptureEntry | null = null;
let createLongTag: CaptureEntry | null = null;
let createTooManyTags: CaptureEntry | null = null;
let calculateLongTag: CaptureEntry | null = null;
let calculateTooManyTags: CaptureEntry | null = null;
let updateLongTag: CaptureEntry | null = null;
let updateTooManyTags: CaptureEntry | null = null;
let readAfterLongTag: CaptureEntry | null = null;
let readAfterTooManyTags: CaptureEntry | null = null;

try {
  setupCreate = await capture(createDocument, setupCreateVariables);
  const setupDraftOrderId = draftOrderIdFromPayload(setupCreate.response, 'draftOrderCreate');
  if (!setupDraftOrderId) {
    throw new Error(`Expected setup draftOrderCreate to return a draft order id: ${JSON.stringify(setupCreate)}`);
  }
  createdDraftOrderIds.push(setupDraftOrderId);

  createNormalizedOk = await capture(createDocument, createNormalizedOkVariables);
  const normalizedDraftOrderId = draftOrderIdFromPayload(createNormalizedOk.response, 'draftOrderCreate');
  if (normalizedDraftOrderId) {
    createdDraftOrderIds.push(normalizedDraftOrderId);
  }

  createLongTag = await capture(createDocument, createLongTagVariables);
  const unexpectedLongCreateId = draftOrderIdFromPayload(createLongTag.response, 'draftOrderCreate');
  if (unexpectedLongCreateId) {
    createdDraftOrderIds.push(unexpectedLongCreateId);
  }

  createTooManyTags = await capture(createDocument, createTooManyTagsVariables);
  const unexpectedTooManyCreateId = draftOrderIdFromPayload(createTooManyTags.response, 'draftOrderCreate');
  if (unexpectedTooManyCreateId) {
    createdDraftOrderIds.push(unexpectedTooManyCreateId);
  }

  calculateLongTag = await capture(calculateDocument, calculateLongTagVariables);
  calculateTooManyTags = await capture(calculateDocument, calculateTooManyTagsVariables);

  updateLongTag = await capture(updateDocument, {
    id: setupDraftOrderId,
    input: { tags: [longTag] },
  });
  readAfterLongTag = await capture(readDocument, { id: setupDraftOrderId });

  updateTooManyTags = await capture(updateDocument, {
    id: setupDraftOrderId,
    input: { tags: tooManyTags },
  });
  readAfterTooManyTags = await capture(readDocument, { id: setupDraftOrderId });

  await writeJson(outputPath, {
    metadata: {
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      description:
        'DraftOrderInput tag count and per-tag length validation for draftOrderCreate, draftOrderUpdate, and draftOrderCalculate.',
    },
    inputs: {
      longTag,
      normalizedOkTags,
      tooManyTags,
    },
    setupCreate,
    createNormalizedOk,
    createLongTag,
    createTooManyTags,
    calculateLongTag,
    calculateTooManyTags,
    updateLongTag,
    readAfterLongTag,
    updateTooManyTags,
    readAfterTooManyTags,
    upstreamCalls: [],
  });
} finally {
  const uniqueDraftOrderIds = [...new Set(createdDraftOrderIds)];
  await Promise.allSettled(
    uniqueDraftOrderIds.map((id) =>
      runGraphql(deleteDocument, {
        input: { id },
      }),
    ),
  );
}
