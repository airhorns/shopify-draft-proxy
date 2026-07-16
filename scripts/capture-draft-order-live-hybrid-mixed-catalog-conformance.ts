/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';
import { captureDraftProxyShopPricingHydrate } from './support/shopify/runtime-hydration-capture.js';

type CaptureStep = {
  document: string;
  variables: JsonRecord;
  response: JsonRecord;
};

const scenarioId = 'draft-order-live-hybrid-mixed-catalog';
const expectedApiVersion = '2025-01';
const cap = await createConformanceCapture();

if (cap.apiVersion !== expectedApiVersion) {
  throw new Error(
    `${scenarioId} requires SHOPIFY_CONFORMANCE_API_VERSION=${expectedApiVersion}, got ${cap.apiVersion}`,
  );
}

const shopPricingHydrate = await captureDraftProxyShopPricingHydrate((query, variables) =>
  cap.runGraphqlRequest(query, variables),
);

const requestDir = path.join('config', 'parity-requests', 'orders');
const specPath = path.join('config', 'parity-specs', 'orders', `${scenarioId}.json`);
const createRequestPath = path.join(requestDir, `${scenarioId}-create.graphql`);
const updateRequestPath = path.join(requestDir, `${scenarioId}-update.graphql`);
const deleteRequestPath = path.join(requestDir, `${scenarioId}-delete.graphql`);
const readRequestPath = path.join(requestDir, `${scenarioId}-read.graphql`);
const fixturePath = cap.fixturePath('orders', `${scenarioId}.json`);
const draftOrderHydrateQuery = await cap.readRequestRaw('orders', 'draft-order-hydrate.graphql');
const stableScenarioTag = 'draft-order-live-hybrid-mixed-catalog';

const draftOrderCreateDocument = `#graphql
  mutation DraftOrderLiveHybridMixedCatalogCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        email
        tags
        status
        updatedAt
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderUpdateDocument = `#graphql
  mutation DraftOrderLiveHybridMixedCatalogUpdate($id: ID!, $input: DraftOrderInput!) {
    draftOrderUpdate(id: $id, input: $input) {
      draftOrder {
        id
        email
        tags
        updatedAt
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderDeleteDocument = `#graphql
  mutation DraftOrderLiveHybridMixedCatalogDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const mixedCatalogReadDocument = `#graphql
  query DraftOrderLiveHybridMixedCatalogRead(
    $existingId: ID!
    $tagQuery: String!
    $existingTagQuery: String!
    $candidateTagQuery: String!
    $first: Int!
  ) {
    existing: draftOrder(id: $existingId) {
      id
      email
      tags
    }
    visible: draftOrders(first: $first, query: $tagQuery, sortKey: UPDATED_AT, reverse: true) {
      nodes {
        email
        tags
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    existingWindow: draftOrders(first: 1, query: $existingTagQuery) {
      nodes {
        email
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    candidateWindow: draftOrders(first: 1, query: $candidateTagQuery) {
      nodes {
        email
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    total: draftOrdersCount(query: $tagQuery) {
      count
      precision
    }
  }
`;

function trimGraphql(document: string): string {
  return document.replace(/^#graphql\n/u, '').trim();
}

async function writeText(filePath: string, payload: string): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${payload.trim()}\n`, 'utf8');
}

async function capture(document: string, variables: JsonRecord, label: string): Promise<CaptureStep> {
  const trimmed = trimGraphql(document);
  const response = await cap.run(trimmed, variables, label);
  return { document: trimmed, variables, response };
}

async function sleep(milliseconds: number): Promise<void> {
  await new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}

function dataRoot(step: CaptureStep, key: string): JsonRecord {
  const root = readRecord(readRecord(step.response['data'])?.[key]);
  if (!root) throw new Error(`Missing data.${key}: ${JSON.stringify(step.response, null, 2)}`);
  return root;
}

function draftOrderFromCreate(step: CaptureStep): JsonRecord {
  const payload = dataRoot(step, 'draftOrderCreate');
  const draftOrder = readRecord(payload['draftOrder']);
  const userErrors = readArray(payload['userErrors']);
  if (!draftOrder || userErrors.length > 0) {
    throw new Error(`draftOrderCreate did not create a draft: ${JSON.stringify(step.response, null, 2)}`);
  }
  return draftOrder;
}

function draftOrderIdFromCreate(step: CaptureStep): string {
  return requireString(draftOrderFromCreate(step)['id'], 'draftOrderCreate.draftOrder.id');
}

function assertDraftOrderDeleteSuccess(step: CaptureStep, id: string): void {
  const payload = dataRoot(step, 'draftOrderDelete');
  const userErrors = readArray(payload['userErrors']);
  if (payload['deletedId'] !== id || userErrors.length > 0) {
    throw new Error(`draftOrderDelete did not delete ${id}: ${JSON.stringify(step.response, null, 2)}`);
  }
}

function countAt(step: CaptureStep): unknown {
  return readRecord(readRecord(step.response['data'])?.['total'])?.['count'];
}

function windowLength(step: CaptureStep, key: string): number {
  return readArray(readRecord(readRecord(step.response['data'])?.[key])?.['nodes']).length;
}

function existingRootPresent(step: CaptureStep): boolean {
  return readRecord(readRecord(step.response['data'])?.['existing']) !== null;
}

async function captureReadWithRetry(
  variables: JsonRecord,
  expectedTotal: number,
  expectedExistingRoot: boolean,
  expectedExistingWindow: number,
  expectedCandidateWindow: number,
  label: string,
): Promise<CaptureStep> {
  let latest: CaptureStep | null = null;
  for (let attempt = 1; attempt <= 12; attempt += 1) {
    latest = await capture(mixedCatalogReadDocument, variables, `${label} attempt ${attempt}`);
    if (
      countAt(latest) === expectedTotal &&
      existingRootPresent(latest) === expectedExistingRoot &&
      windowLength(latest, 'existingWindow') === expectedExistingWindow &&
      windowLength(latest, 'candidateWindow') === expectedCandidateWindow
    ) {
      if (attempt > 1) console.log(`${label} indexed after ${attempt} attempts`);
      return latest;
    }
    if (attempt < 12) await sleep(2_000);
  }
  throw new Error(`${label} did not reach expected shape: ${JSON.stringify(latest?.response, null, 2)}`);
}

function draftOrderVariables(role: 'existing' | 'candidate', tag: string, roleTag: string, email: string): JsonRecord {
  return {
    input: {
      email,
      tags: [stableScenarioTag, tag, roleTag],
      lineItems: [
        {
          title: `Live hybrid ${role} mixed draft`,
          quantity: 1,
          originalUnitPrice: role === 'existing' ? '11.00' : '12.00',
        },
      ],
    },
  };
}

function readVariables(existingId: string, tag: string, existingTag: string, candidateTag: string): JsonRecord {
  return {
    existingId,
    tagQuery: `tag:${tag}`,
    existingTagQuery: `tag:${existingTag}`,
    candidateTagQuery: `tag:${candidateTag}`,
    first: 5,
  };
}

function draftOrderHydrateUpstreamCall(id: string, response: JsonRecord): JsonRecord {
  return {
    operationName: 'OrdersDraftOrderHydrate',
    variables: { id },
    query: draftOrderHydrateQuery,
    response: { status: 200, body: response },
  };
}

function specPayload(): JsonRecord {
  return {
    scenarioId,
    operationNames: [
      'draftOrderCreate',
      'draftOrderUpdate',
      'draftOrderDelete',
      'draftOrder',
      'draftOrders',
      'draftOrdersCount',
    ],
    scenarioStatus: 'captured',
    assertionKinds: [
      'downstream-read-parity',
      'search-filtering',
      'count-parity',
      'pagination-shape',
      'runtime-staging',
    ],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: createRequestPath,
      variablesCapturePath: '$.operations.candidateCreate.variables',
      apiVersion: cap.apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live 2025-01 Shopify capture for a mixed draft-order catalog: a real pre-existing tagged draft, a staged candidate draft, an update to the existing draft, and a delete tombstone. Proxy replay stages only the candidate draft locally and uses the recorded existing-draft hydrate cassette to observe the untouched catalog row, proving LiveHybrid draftOrders/draftOrdersCount/draftOrder merge upstream plus staged state without runtime Shopify writes.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'candidate-create-baseline',
          capturePath: '$.operations.candidateCreate.response.data.draftOrderCreate',
          proxyPath: '$.data.draftOrderCreate',
          selectedPaths: ['$.draftOrder.email', '$.draftOrder.tags', '$.userErrors'],
        },
        {
          name: 'mixed-read-after-staged-create',
          capturePath: '$.operations.mixedAfterCreateRead.response.data',
          proxyPath: '$.data',
          selectedPaths: [
            '$.existing.email',
            '$.existingWindow.nodes[0].email',
            '$.candidateWindow.nodes[0].email',
            '$.total.count',
            '$.total.precision',
          ],
          proxyRequest: {
            documentPath: readRequestPath,
            variablesCapturePath: '$.operations.mixedAfterCreateRead.variables',
            apiVersion: cap.apiVersion,
          },
        },
        {
          name: 'existing-draft-update-wins-by-id',
          capturePath: '$.operations.existingUpdate.response.data.draftOrderUpdate',
          proxyPath: '$.data.draftOrderUpdate',
          selectedPaths: ['$.draftOrder.email', '$.draftOrder.tags', '$.userErrors'],
          proxyRequest: {
            documentPath: updateRequestPath,
            variablesCapturePath: '$.operations.existingUpdate.variables',
            apiVersion: cap.apiVersion,
          },
        },
        {
          name: 'mixed-read-after-existing-update',
          capturePath: '$.operations.afterUpdateRead.response.data',
          proxyPath: '$.data',
          selectedPaths: [
            '$.existing.email',
            '$.existingWindow.nodes[0].email',
            '$.candidateWindow.nodes[0].email',
            '$.total.count',
            '$.total.precision',
          ],
          proxyRequest: {
            documentPath: readRequestPath,
            variablesCapturePath: '$.operations.afterUpdateRead.variables',
            apiVersion: cap.apiVersion,
          },
        },
        {
          name: 'existing-draft-delete-tombstone',
          capturePath: '$.operations.existingDelete.response.data.draftOrderDelete',
          proxyPath: '$.data.draftOrderDelete',
          proxyRequest: {
            documentPath: deleteRequestPath,
            variablesCapturePath: '$.operations.existingDelete.variables',
            apiVersion: cap.apiVersion,
          },
        },
        {
          name: 'mixed-read-after-existing-delete',
          capturePath: '$.operations.afterDeleteRead.response.data',
          proxyPath: '$.data',
          selectedPaths: [
            '$.existing',
            '$.existingWindow.nodes',
            '$.candidateWindow.nodes[0].email',
            '$.total.count',
            '$.total.precision',
          ],
          proxyRequest: {
            documentPath: readRequestPath,
            variablesCapturePath: '$.operations.afterDeleteRead.variables',
            apiVersion: cap.apiVersion,
          },
        },
      ],
    },
  };
}

await writeText(createRequestPath, trimGraphql(draftOrderCreateDocument));
await writeText(updateRequestPath, trimGraphql(draftOrderUpdateDocument));
await writeText(deleteRequestPath, trimGraphql(draftOrderDeleteDocument));
await writeText(readRequestPath, trimGraphql(mixedCatalogReadDocument));
await cap.writeJson(specPath, specPayload());

const stamp = cap.stamp;
const tag = `domix-${stamp}`;
const existingTag = `${tag}-ex`;
const candidateTag = `${tag}-ca`;
const existingEmail = `draft-order-existing-${stamp}@example.com`;
const editedExistingEmail = `draft-order-existing-edited-${stamp}@example.com`;
const candidateEmail = `draft-order-candidate-${stamp}@example.com`;
let existingDraftId: string | null = null;
let candidateDraftId: string | null = null;
let existingDeleted = false;
let candidateDeleted = false;
const cleanup: CaptureStep[] = [];

try {
  const existingCreate = await capture(
    draftOrderCreateDocument,
    draftOrderVariables('existing', tag, existingTag, existingEmail),
    'existing draftOrderCreate',
  );
  cap.mutationRoot(existingCreate.response, 'draftOrderCreate', 'existing draftOrderCreate');
  existingDraftId = draftOrderIdFromCreate(existingCreate);

  const baselineRead = await captureReadWithRetry(
    readVariables(existingDraftId, tag, existingTag, candidateTag),
    1,
    true,
    1,
    0,
    'baseline mixed draft-order read',
  );

  const candidateCreate = await capture(
    draftOrderCreateDocument,
    draftOrderVariables('candidate', tag, candidateTag, candidateEmail),
    'candidate draftOrderCreate',
  );
  cap.mutationRoot(candidateCreate.response, 'draftOrderCreate', 'candidate draftOrderCreate');
  candidateDraftId = draftOrderIdFromCreate(candidateCreate);

  const mixedAfterCreateRead = await captureReadWithRetry(
    readVariables(existingDraftId, tag, existingTag, candidateTag),
    2,
    true,
    1,
    1,
    'mixed draft-order read after candidate create',
  );
  const existingHydrate = await cap.run(
    draftOrderHydrateQuery,
    { id: existingDraftId },
    'existing draftOrder hydrate cassette',
  );

  const existingUpdate = await capture(
    draftOrderUpdateDocument,
    {
      id: existingDraftId,
      input: {
        email: editedExistingEmail,
        tags: [stableScenarioTag, tag, existingTag, 'edited'],
      },
    },
    'existing draftOrderUpdate',
  );
  cap.mutationRoot(existingUpdate.response, 'draftOrderUpdate', 'existing draftOrderUpdate');

  const afterUpdateRead = await captureReadWithRetry(
    readVariables(existingDraftId, tag, existingTag, candidateTag),
    2,
    true,
    1,
    1,
    'mixed draft-order read after existing update',
  );

  const existingDelete = await capture(
    draftOrderDeleteDocument,
    { input: { id: existingDraftId } },
    'existing draftOrderDelete',
  );
  assertDraftOrderDeleteSuccess(existingDelete, existingDraftId);
  existingDeleted = true;

  const afterDeleteRead = await captureReadWithRetry(
    readVariables(existingDraftId, tag, existingTag, candidateTag),
    1,
    false,
    0,
    1,
    'mixed draft-order read after existing delete',
  );

  const candidateCleanup = await capture(
    draftOrderDeleteDocument,
    { input: { id: candidateDraftId } },
    'candidate cleanup draftOrderDelete',
  );
  assertDraftOrderDeleteSuccess(candidateCleanup, candidateDraftId);
  candidateDeleted = true;
  cleanup.push(candidateCleanup);

  await cap.writeJson(fixturePath, {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain: cap.storeDomain,
    apiVersion: cap.apiVersion,
    source: 'live-shopify-admin-graphql',
    notes:
      'Captured from live Shopify Admin GraphQL. The live expected operations create/update/delete disposable draft orders and read draftOrders/draftOrdersCount before and after staged-equivalent local writes. The upstreamCalls cassette records the exact pre-local-write OrdersDraftOrderHydrate request that proxy replay emits to observe the existing draft while supported draft-order mutations remain staged locally.',
    operations: {
      existingCreate,
      baselineRead,
      candidateCreate,
      mixedAfterCreateRead,
      existingHydrate: { variables: { id: existingDraftId }, response: existingHydrate },
      existingUpdate,
      afterUpdateRead,
      existingDelete,
      afterDeleteRead,
    },
    upstreamCalls: [shopPricingHydrate, draftOrderHydrateUpstreamCall(existingDraftId, existingHydrate)],
    cleanup,
  });

  console.log(`Wrote ${fixturePath}`);
  console.log(`Wrote ${specPath}`);
  console.log(`Wrote ${createRequestPath}`);
  console.log(`Wrote ${updateRequestPath}`);
  console.log(`Wrote ${deleteRequestPath}`);
  console.log(`Wrote ${readRequestPath}`);
} catch (error) {
  for (const [draftId, alreadyDeleted, label] of [
    [existingDraftId, existingDeleted, 'existing'],
    [candidateDraftId, candidateDeleted, 'candidate'],
  ] as const) {
    if (!draftId || alreadyDeleted) continue;
    try {
      const cleanupDelete = await capture(draftOrderDeleteDocument, { input: { id: draftId } }, `${label} cleanup`);
      cleanup.push(cleanupDelete);
    } catch (cleanupError) {
      console.error(`Cleanup failed for ${label} ${draftId}:`, cleanupError);
    }
  }
  throw error;
}
