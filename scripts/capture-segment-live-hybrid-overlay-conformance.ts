/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonObject = Record<string, unknown>;

type CapturedStep = {
  operationName: string;
  query: string;
  variables: JsonObject;
  response: {
    status: number;
    body: unknown;
  };
};

const segmentCreatePrerequisitesDocument =
  'query SegmentAuthoritativePrerequisites($name0: String!) {\n  count: segmentsCount(limit: 6000) { count precision }\n  name0: segments(first: 101, query: $name0) {\n    nodes { id name query creationDate lastEditDate }\n    pageInfo { hasNextPage }\n  }\n}';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({
  adminOrigin,
  apiVersion,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'segments');
const outputPath = path.join(outputDir, 'segment-live-hybrid-overlay.json');
const mutationFirstOutputPath = path.join(outputDir, 'segment-mutation-first-hydration.json');
const parityRequestDir = path.join('config', 'parity-requests', 'segments');
const createDocument = await readFile(
  path.join(parityRequestDir, 'segment-live-hybrid-overlay-create.graphql'),
  'utf8',
);
const readDocument = await readFile(path.join(parityRequestDir, 'segment-live-hybrid-overlay-read.graphql'), 'utf8');
const mutationTargetHydrateDocument = await readFile(
  path.join(parityRequestDir, 'segment-mutation-target-hydrate.graphql'),
  'utf8',
);
const mutationFirstUpdateDocument = await readFile(
  path.join(parityRequestDir, 'segment-mutation-first-update.graphql'),
  'utf8',
);
const mutationFirstUpdateReadDocument = await readFile(
  path.join(parityRequestDir, 'segment-mutation-first-update-read.graphql'),
  'utf8',
);
const mutationFirstDeleteDocument = await readFile(
  path.join(parityRequestDir, 'segment-mutation-first-delete.graphql'),
  'utf8',
);
const mutationFirstDeleteReadDocument = await readFile(
  path.join(parityRequestDir, 'segment-mutation-first-delete-read.graphql'),
  'utf8',
);

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const cleanupDocument = `#graphql
mutation SegmentLiveHybridOverlayCleanup($id: ID!) {
  segmentDelete(id: $id) {
    deletedSegmentId
    userErrors {
      field
      message
    }
  }
}
`;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function randomSuffix(): string {
  return `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
}

function assertGraphqlOk(label: string, result: ConformanceGraphqlResult, allowGraphqlErrors = false): void {
  if (result.status < 200 || result.status >= 300 || (!allowGraphqlErrors && result.payload.errors)) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

async function captureStep(
  operationName: string,
  query: string,
  variables: JsonObject,
  allowGraphqlErrors = false,
): Promise<CapturedStep> {
  const result = await runGraphqlRequest(query, variables);
  assertGraphqlOk(operationName, result, allowGraphqlErrors);
  return {
    operationName,
    query,
    variables,
    response: {
      status: result.status,
      body: result.payload,
    },
  };
}

function responseBody(step: CapturedStep): JsonObject {
  if (typeof step.response.body === 'object' && step.response.body !== null) {
    return step.response.body as JsonObject;
  }
  throw new Error(`${step.operationName} response body was not an object`);
}

function responseData(step: CapturedStep): JsonObject {
  const body = responseBody(step);
  if (typeof body['data'] === 'object' && body['data'] !== null) {
    return body['data'] as JsonObject;
  }
  throw new Error(`${step.operationName} response body did not contain data`);
}

function readSegmentId(step: CapturedStep, rootField = 'segmentCreate'): string {
  const root = responseData(step)[rootField];
  if (typeof root !== 'object' || root === null) {
    throw new Error(`${step.operationName} did not return ${rootField}`);
  }
  const segment = (root as JsonObject)['segment'];
  if (typeof segment !== 'object' || segment === null) {
    throw new Error(`${step.operationName} did not return a segment`);
  }
  const id = (segment as JsonObject)['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${step.operationName} did not return a segment id`);
  }
  return id;
}

function assertNoCreateUserErrors(step: CapturedStep): void {
  const segmentCreate = responseData(step)['segmentCreate'] as JsonObject | undefined;
  const userErrors = segmentCreate?.['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${step.operationName} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertNoUserErrors(step: CapturedStep, rootField: string): void {
  const root = responseData(step)[rootField];
  const userErrors = typeof root === 'object' && root !== null ? (root as JsonObject)['userErrors'] : null;
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${step.operationName} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function connectionNames(data: JsonObject, key: string): string[] {
  const connection = data[key];
  if (typeof connection !== 'object' || connection === null) return [];
  const edges = (connection as JsonObject)['edges'];
  if (!Array.isArray(edges)) return [];
  return edges
    .map((edge) => {
      if (typeof edge !== 'object' || edge === null) return null;
      const node = (edge as JsonObject)['node'];
      if (typeof node !== 'object' || node === null) return null;
      const name = (node as JsonObject)['name'];
      return typeof name === 'string' ? name : null;
    })
    .filter((name): name is string => name !== null);
}

function connectionNodeNames(data: JsonObject, key: string): string[] {
  const connection = data[key];
  if (typeof connection !== 'object' || connection === null) return [];
  const nodes = (connection as JsonObject)['nodes'];
  if (!Array.isArray(nodes)) return [];
  return nodes
    .map((node) => {
      if (typeof node !== 'object' || node === null) return null;
      const name = (node as JsonObject)['name'];
      return typeof name === 'string' ? name : null;
    })
    .filter((name): name is string => name !== null);
}

function readSegmentName(data: JsonObject, key: string): string | null {
  const segment = data[key];
  if (typeof segment !== 'object' || segment === null) return null;
  const name = (segment as JsonObject)['name'];
  return typeof name === 'string' ? name : null;
}

function readCount(data: JsonObject, key: string): number | null {
  const countObject = data[key];
  if (typeof countObject !== 'object' || countObject === null) return null;
  const count = (countObject as JsonObject)['count'];
  return typeof count === 'number' ? count : null;
}

function readPrecision(data: JsonObject, key: string): string | null {
  const countObject = data[key];
  if (typeof countObject !== 'object' || countObject === null) return null;
  const precision = (countObject as JsonObject)['precision'];
  return typeof precision === 'string' ? precision : null;
}

function readRealSegmentName(data: JsonObject): string | null {
  const realSegment = data['realSegment'];
  if (typeof realSegment !== 'object' || realSegment === null) return null;
  const name = (realSegment as JsonObject)['name'];
  return typeof name === 'string' ? name : null;
}

function readHasNextPage(data: JsonObject, key: string): boolean | null {
  const connection = data[key];
  if (typeof connection !== 'object' || connection === null) return null;
  const pageInfo = (connection as JsonObject)['pageInfo'];
  if (typeof pageInfo !== 'object' || pageInfo === null) return null;
  const hasNextPage = (pageInfo as JsonObject)['hasNextPage'];
  return typeof hasNextPage === 'boolean' ? hasNextPage : null;
}

function baseReadMatches(step: CapturedStep, baseName: string, stagedName: string): boolean {
  const data = responseData(step);
  return (
    readRealSegmentName(data) === baseName &&
    connectionNames(data, 'baseWindow').join('|') === baseName &&
    connectionNames(data, 'stagedWindow').length === 0 &&
    readCount(data, 'totalCount') !== null &&
    readPrecision(data, 'totalCount') === 'EXACT' &&
    readHasNextPage(data, 'combinedWindow') === false &&
    !connectionNames(data, 'combinedWindow').includes(stagedName)
  );
}

function finalReadMatches(step: CapturedStep, baseName: string, stagedName: string, baseTotalCount: number): boolean {
  const data = responseData(step);
  return (
    readRealSegmentName(data) === baseName &&
    connectionNames(data, 'baseWindow').join('|') === baseName &&
    connectionNames(data, 'stagedWindow').join('|') === stagedName &&
    readCount(data, 'totalCount') === baseTotalCount + 1 &&
    readPrecision(data, 'totalCount') === 'EXACT' &&
    readHasNextPage(data, 'combinedWindow') === true
  );
}

async function captureUntil(
  label: string,
  operationName: string,
  query: string,
  variables: JsonObject,
  matches: (step: CapturedStep) => boolean,
  allowGraphqlErrors = false,
): Promise<CapturedStep> {
  let lastStep: CapturedStep | null = null;
  for (let attempt = 1; attempt <= 24; attempt += 1) {
    lastStep = await captureStep(`${operationName}Attempt${attempt}`, query, variables, allowGraphqlErrors);
    lastStep.operationName = operationName;
    if (matches(lastStep)) return lastStep;
    await sleep(1500);
  }
  throw new Error(`${label} did not observe expected indexed state: ${JSON.stringify(lastStep, null, 2)}`);
}

async function cleanupSegment(id: string | null): Promise<unknown> {
  if (!id) return null;
  try {
    const result = await runGraphqlRequest(cleanupDocument, { id });
    return {
      query: cleanupDocument,
      variables: { id },
      response: {
        status: result.status,
        body: result.payload,
      },
    };
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
}

function upstreamCall(step: CapturedStep): JsonObject {
  return {
    method: 'POST',
    path: `/admin/api/${apiVersion}/graphql.json`,
    apiSurface: 'admin',
    apiVersion,
    operationName: step.operationName,
    variables: step.variables,
    query: step.query,
    response: step.response,
  };
}

const marker = `segmentlivehybridoverlay${randomSuffix()}`;
const baseName = `Live Hybrid Base ${marker}`;
const stagedName = `Live Hybrid Staged ${marker}`;
const baseCreateVariables = {
  name: baseName,
  query: `customer_tags CONTAINS '${marker}-base'`,
};
const stagedCreateVariables = {
  name: stagedName,
  query: `customer_tags CONTAINS '${marker}-staged'`,
};

let baseSegmentId: string | null = null;
let liveStagedSegmentId: string | null = null;

try {
  const baseCreate = await captureStep('SegmentLiveHybridOverlayBaseCreate', createDocument, baseCreateVariables);
  assertNoCreateUserErrors(baseCreate);
  baseSegmentId = readSegmentId(baseCreate);

  const readVariables = {
    realId: baseSegmentId,
    catalogQuery: marker,
    baseQuery: baseName,
    stagedQuery: stagedName,
    first: 5,
  };

  const baseRead = await captureUntil(
    'base segment overlay read',
    'SegmentLiveHybridOverlayRead',
    readDocument,
    readVariables,
    (step) => baseReadMatches(step, baseName, stagedName),
  );
  const baseTotalCount = readCount(responseData(baseRead), 'totalCount');
  if (baseTotalCount === null) {
    throw new Error(`base read did not return totalCount: ${JSON.stringify(baseRead, null, 2)}`);
  }

  const createPrerequisites = await captureStep(
    'SegmentAuthoritativePrerequisites',
    segmentCreatePrerequisitesDocument,
    { name0: `name:"${stagedName}"` },
  );
  const createPrerequisiteData = responseData(createPrerequisites);
  if (
    readCount(createPrerequisiteData, 'count') === null ||
    connectionNodeNames(createPrerequisiteData, 'name0').includes(stagedName)
  ) {
    throw new Error(
      `create prerequisites did not prove the expected count/name state: ${JSON.stringify(createPrerequisites, null, 2)}`,
    );
  }

  const liveStagedCreate = await captureStep('SegmentLiveHybridOverlayCreate', createDocument, stagedCreateVariables);
  assertNoCreateUserErrors(liveStagedCreate);
  liveStagedSegmentId = readSegmentId(liveStagedCreate);

  const finalRead = await captureUntil(
    'final segment overlay read',
    'SegmentLiveHybridOverlayRead',
    readDocument,
    readVariables,
    (step) => finalReadMatches(step, baseName, stagedName, baseTotalCount),
  );

  const cleanup = {
    staged: await cleanupSegment(liveStagedSegmentId),
    base: await cleanupSegment(baseSegmentId),
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        scenarioId: 'segment-live-hybrid-overlay',
        storeDomain,
        apiVersion,
        proxyVariables: {
          create: stagedCreateVariables,
          read: readVariables,
        },
        setup: {
          baseCreate,
          baseRead,
        },
        liveStagedCreate,
        finalRead,
        cleanup,
        upstreamCalls: [createPrerequisites, baseRead].map(upstreamCall),
        notes: [
          'Live Shopify evidence for LiveHybrid segment overlay after segmentCreate.',
          'Proxy replay stages only the second segment locally, while the upstreamCalls cassette records its authoritative count/name prerequisites and the base-only segment(id:)/segments/segmentsCount read captured before the second segment existed.',
        ],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        baseName,
        stagedName,
      },
      null,
      2,
    ),
  );
} catch (error) {
  const cleanup = {
    staged: await cleanupSegment(liveStagedSegmentId),
    base: await cleanupSegment(baseSegmentId),
  };
  console.error(JSON.stringify({ ok: false, cleanup }, null, 2));
  throw error;
}

const mutationFirstMarker = `segmentmutationfirst${randomSuffix()}`;
const updateOriginalName = `Mutation First Update Before ${mutationFirstMarker}`;
const updateFinalName = `Mutation First Update After ${mutationFirstMarker}`;
const deleteName = `Mutation First Delete ${mutationFirstMarker}`;
const updateCreateVariables = {
  name: updateOriginalName,
  query: `customer_tags CONTAINS '${mutationFirstMarker}-update-before'`,
};
const updateFinalQuery = `customer_tags CONTAINS '${mutationFirstMarker}-update-after'`;
const deleteCreateVariables = {
  name: deleteName,
  query: `customer_tags CONTAINS '${mutationFirstMarker}-delete'`,
};

let updateSegmentId: string | null = null;
let deleteSegmentId: string | null = null;

try {
  const updateCreate = await captureStep(
    'SegmentMutationFirstUpdateSetupCreate',
    createDocument,
    updateCreateVariables,
  );
  assertNoCreateUserErrors(updateCreate);
  updateSegmentId = readSegmentId(updateCreate);
  const updateVariables = {
    id: updateSegmentId,
    name: updateFinalName,
    query: updateFinalQuery,
  };
  const updateReadVariables = { id: updateSegmentId, name: updateFinalName };

  const updateHydrate = await captureUntil(
    'mutation-first update target hydration',
    'SegmentMutationTargetHydrate',
    mutationTargetHydrateDocument,
    { id: updateSegmentId },
    (step) => readSegmentName(responseData(step), 'segment') === updateOriginalName,
  );
  const updateReadBefore = await captureUntil(
    'mutation-first update baseline read',
    'SegmentMutationFirstUpdateRead',
    mutationFirstUpdateReadDocument,
    updateReadVariables,
    (step) => {
      const data = responseData(step);
      return (
        readSegmentName(data, 'detail') === updateOriginalName &&
        connectionNodeNames(data, 'list').length === 0 &&
        readCount(data, 'count') !== null &&
        readPrecision(data, 'count') === 'EXACT'
      );
    },
  );
  const updateCountBefore = readCount(responseData(updateReadBefore), 'count');
  if (updateCountBefore === null) {
    throw new Error(`update baseline did not return count: ${JSON.stringify(updateReadBefore, null, 2)}`);
  }

  const liveUpdate = await captureStep('SegmentMutationFirstUpdate', mutationFirstUpdateDocument, updateVariables);
  assertNoUserErrors(liveUpdate, 'segmentUpdate');
  const updateReadAfter = await captureUntil(
    'mutation-first update downstream read',
    'SegmentMutationFirstUpdateRead',
    mutationFirstUpdateReadDocument,
    updateReadVariables,
    (step) => {
      const data = responseData(step);
      return (
        readSegmentName(data, 'detail') === updateFinalName &&
        connectionNodeNames(data, 'list').join('|') === updateFinalName &&
        readCount(data, 'count') === updateCountBefore &&
        readPrecision(data, 'count') === 'EXACT'
      );
    },
  );

  const deleteCreate = await captureStep(
    'SegmentMutationFirstDeleteSetupCreate',
    createDocument,
    deleteCreateVariables,
  );
  assertNoCreateUserErrors(deleteCreate);
  deleteSegmentId = readSegmentId(deleteCreate);
  const deleteVariables = { id: deleteSegmentId };
  const deleteReadVariables = { id: deleteSegmentId, name: deleteName };

  const deleteHydrate = await captureUntil(
    'mutation-first delete target hydration',
    'SegmentMutationTargetHydrate',
    mutationTargetHydrateDocument,
    deleteVariables,
    (step) => readSegmentName(responseData(step), 'segment') === deleteName,
  );
  const deleteReadBefore = await captureUntil(
    'mutation-first delete baseline read',
    'SegmentMutationFirstDeleteRead',
    mutationFirstDeleteReadDocument,
    deleteReadVariables,
    (step) => {
      const data = responseData(step);
      return (
        readSegmentName(data, 'detail') === deleteName &&
        connectionNodeNames(data, 'list').join('|') === deleteName &&
        readCount(data, 'count') !== null &&
        readPrecision(data, 'count') === 'EXACT'
      );
    },
  );
  const deleteCountBefore = readCount(responseData(deleteReadBefore), 'count');
  if (deleteCountBefore === null) {
    throw new Error(`delete baseline did not return count: ${JSON.stringify(deleteReadBefore, null, 2)}`);
  }

  const liveDelete = await captureStep('SegmentMutationFirstDelete', mutationFirstDeleteDocument, deleteVariables);
  assertNoUserErrors(liveDelete, 'segmentDelete');
  const deleteReadAfter = await captureUntil(
    'mutation-first delete downstream read',
    'SegmentMutationFirstDeleteRead',
    mutationFirstDeleteReadDocument,
    deleteReadVariables,
    (step) => {
      const data = responseData(step);
      return (
        data['detail'] === null &&
        connectionNodeNames(data, 'list').length === 0 &&
        readCount(data, 'count') === deleteCountBefore - 1 &&
        readPrecision(data, 'count') === 'EXACT'
      );
    },
    true,
  );

  const cleanup = {
    updated: await cleanupSegment(updateSegmentId),
    deletedByScenario: liveDelete,
  };
  await mkdir(outputDir, { recursive: true });
  await writeFile(
    mutationFirstOutputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        scenarioId: 'segment-mutation-first-hydration',
        storeDomain,
        apiVersion,
        proxyVariables: {
          update: updateVariables,
          updateRead: updateReadVariables,
          delete: deleteVariables,
          deleteRead: deleteReadVariables,
        },
        setup: {
          updateCreate,
          updateHydrate,
          updateReadBefore,
          deleteCreate,
          deleteHydrate,
          deleteReadBefore,
        },
        liveUpdate,
        updateReadAfter,
        liveDelete,
        deleteReadAfter,
        cleanup,
        upstreamCalls: [updateHydrate, updateReadBefore, deleteHydrate, deleteReadBefore].map(upstreamCall),
        notes: [
          'Live Shopify evidence for segmentUpdate and segmentDelete against persisted targets before any proxy read.',
          'Proxy replay starts a fresh session for each mutation; each exact query-only segment(id:) hydrate and pre-mutation downstream read is recorded in upstreamCalls.',
          'The capture creates disposable update/delete targets, verifies detail/list/count materialization, deletes the delete target as part of the scenario, and cleans up the updated target.',
        ],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath: mutationFirstOutputPath,
        updateSegmentId,
        deleteSegmentId,
      },
      null,
      2,
    ),
  );
} catch (error) {
  const cleanup = {
    updated: await cleanupSegment(updateSegmentId),
    deleted: await cleanupSegment(deleteSegmentId),
  };
  console.error(JSON.stringify({ ok: false, cleanup }, null, 2));
  throw error;
}
