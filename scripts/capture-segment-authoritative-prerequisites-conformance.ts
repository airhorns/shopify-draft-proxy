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
const segmentHydrateDocument =
  'query SegmentMutationTargetHydrate($id: ID!) {\n  segment(id: $id) {\n    id\n    name\n    query\n    creationDate\n    lastEditDate\n  }\n}';
const memberJobHydrateDocument =
  'query CustomerSegmentMembersQueryHydrate($ids: [ID!]!) {\n  nodes(ids: $ids) {\n    ... on CustomerSegmentMembersQuery {\n      id\n      currentCount\n      done\n    }\n  }\n}';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'segments');
const outputPath = path.join(outputDir, 'segment-authoritative-prerequisites.json');
const requestDir = path.join('config', 'parity-requests', 'segments');
const createDocument = await readFile(path.join(requestDir, 'segment-authoritative-name-create.graphql'), 'utf8');
const memberJobCreateDocument = await readFile(
  path.join(requestDir, 'customer-segment-members-query-authoritative-segment-id.graphql'),
  'utf8',
);
const memberJobPollDocument = await readFile(
  path.join(requestDir, 'customer-segment-members-query-authoritative-poll.graphql'),
  'utf8',
);

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const cleanupDocument = `#graphql
mutation SegmentAuthoritativePrerequisitesCleanup($id: ID!) {
  segmentDelete(id: $id) {
    deletedSegmentId
    userErrors {
      field
      message
    }
  }
}
`;

function randomSuffix(): string {
  return `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
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

function responseData(step: CapturedStep): JsonObject {
  const body = step.response.body;
  if (typeof body !== 'object' || body === null) {
    throw new Error(`${step.operationName} response was not an object`);
  }
  const data = (body as JsonObject)['data'];
  if (typeof data !== 'object' || data === null) {
    throw new Error(`${step.operationName} response did not contain data`);
  }
  return data as JsonObject;
}

function rootObject(step: CapturedStep, root: string): JsonObject {
  const value = responseData(step)[root];
  if (typeof value !== 'object' || value === null) {
    throw new Error(`${step.operationName} did not return ${root}`);
  }
  return value as JsonObject;
}

function assertNoUserErrors(step: CapturedStep, root: string): void {
  const userErrors = rootObject(step, root)['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${step.operationName} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function readSegment(step: CapturedStep): JsonObject {
  const segment = rootObject(step, 'segmentCreate')['segment'];
  if (typeof segment !== 'object' || segment === null) {
    throw new Error(`${step.operationName} did not return a Segment`);
  }
  return segment as JsonObject;
}

function readRequiredString(value: JsonObject, key: string, label: string): string {
  const result = value[key];
  if (typeof result !== 'string' || result.length === 0) {
    throw new Error(`${label} did not contain ${key}`);
  }
  return result;
}

function readMemberJob(step: CapturedStep): JsonObject {
  const job = rootObject(step, 'customerSegmentMembersQueryCreate')['customerSegmentMembersQuery'];
  if (typeof job !== 'object' || job === null) {
    throw new Error(`${step.operationName} did not return a customer segment member query`);
  }
  return job as JsonObject;
}

function segmentNameProbeQuery(name: string): string {
  return `name:"${name.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`;
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

async function cleanupSegment(id: string | null): Promise<unknown> {
  if (!id) return null;
  try {
    const result = await runGraphqlRequest(cleanupDocument, { id });
    return {
      query: cleanupDocument,
      variables: { id },
      response: { status: result.status, body: result.payload },
    };
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
}

const marker = `segmentauthoritative${randomSuffix()}`;
const persistedName = `Authoritative Segment ${marker}`;
const persistedQuery = `customer_tags CONTAINS '${marker}'`;
const duplicateQuery = `customer_tags CONTAINS '${marker}-duplicate'`;
const unknownSegmentId = `gid://shopify/Segment/999999${Date.now()}`;

let persistedSegmentId: string | null = null;
let duplicateSegmentId: string | null = null;

try {
  const setupCreate = await captureStep('SegmentAuthoritativeSetupCreate', createDocument, {
    name: persistedName,
    query: persistedQuery,
  });
  assertNoUserErrors(setupCreate, 'segmentCreate');
  persistedSegmentId = readRequiredString(readSegment(setupCreate), 'id', 'setup Segment');

  let createPrerequisites: CapturedStep | null = null;
  let nameConnection: JsonObject | null = null;
  for (let attempt = 1; attempt <= 24; attempt += 1) {
    createPrerequisites = await captureStep('SegmentAuthoritativePrerequisites', segmentCreatePrerequisitesDocument, {
      name0: segmentNameProbeQuery(persistedName),
    });
    const candidate = responseData(createPrerequisites)['name0'];
    nameConnection = typeof candidate === 'object' && candidate !== null ? (candidate as JsonObject) : null;
    const nameNodes = nameConnection?.['nodes'];
    if (
      Array.isArray(nameNodes) &&
      nameNodes.some(
        (node) => typeof node === 'object' && node !== null && (node as JsonObject)['id'] === persistedSegmentId,
      )
    ) {
      break;
    }
    await sleep(1500);
  }
  if (createPrerequisites === null || nameConnection === null) {
    throw new Error('Segment authoritative name prerequisite did not return name0');
  }
  const nameNodes = nameConnection['nodes'];
  if (
    !Array.isArray(nameNodes) ||
    !nameNodes.some(
      (node) => typeof node === 'object' && node !== null && (node as JsonObject)['id'] === persistedSegmentId,
    )
  ) {
    throw new Error(
      `Segment authoritative name prerequisite missed ${persistedSegmentId}: ${JSON.stringify(nameConnection)}`,
    );
  }
  if ((nameConnection['pageInfo'] as JsonObject | undefined)?.['hasNextPage'] !== false) {
    throw new Error('Segment authoritative name prerequisite was not complete');
  }

  const liveNameCreate = await captureStep('SegmentAuthoritativeNameCreate', createDocument, {
    name: persistedName,
    query: duplicateQuery,
  });
  assertNoUserErrors(liveNameCreate, 'segmentCreate');
  const duplicateSegment = readSegment(liveNameCreate);
  duplicateSegmentId = readRequiredString(duplicateSegment, 'id', 'duplicate Segment');
  if (duplicateSegment['name'] !== `${persistedName} (2)`) {
    throw new Error(`Shopify did not suffix the persisted-name collision: ${JSON.stringify(duplicateSegment)}`);
  }

  const validSegmentHydrate = await captureStep('SegmentMutationTargetHydrate', segmentHydrateDocument, {
    id: persistedSegmentId,
  });
  if ((responseData(validSegmentHydrate)['segment'] as JsonObject | null)?.['id'] !== persistedSegmentId) {
    throw new Error('Valid Segment prerequisite did not return the persisted target');
  }
  const liveValidJobCreate = await captureStep(
    'CustomerSegmentMembersQueryAuthoritativeSegmentId',
    memberJobCreateDocument,
    { input: { segmentId: persistedSegmentId } },
  );
  assertNoUserErrors(liveValidJobCreate, 'customerSegmentMembersQueryCreate');
  const persistedJobId = readRequiredString(readMemberJob(liveValidJobCreate), 'id', 'member-query job');

  const invalidSegmentHydrate = await captureStep(
    'SegmentMutationTargetHydrate',
    segmentHydrateDocument,
    { id: unknownSegmentId },
    true,
  );
  if (responseData(invalidSegmentHydrate)['segment'] !== null) {
    throw new Error('Unknown Segment prerequisite unexpectedly returned a target');
  }
  const liveInvalidJobCreate = await captureStep(
    'CustomerSegmentMembersQueryAuthoritativeSegmentId',
    memberJobCreateDocument,
    { input: { segmentId: unknownSegmentId } },
  );
  const invalidUserErrors = rootObject(liveInvalidJobCreate, 'customerSegmentMembersQueryCreate')['userErrors'];
  if (!Array.isArray(invalidUserErrors) || invalidUserErrors.length === 0) {
    throw new Error('Unknown Segment member-query create did not return a userError');
  }

  const memberJobHydrate = await captureStep('CustomerSegmentMembersQueryHydrate', memberJobHydrateDocument, {
    ids: [persistedJobId],
  });
  const hydratedJobs = responseData(memberJobHydrate)['nodes'];
  if (!Array.isArray(hydratedJobs) || hydratedJobs.length !== 1 || hydratedJobs[0] === null) {
    throw new Error('Persisted member-query job prerequisite did not return one job');
  }
  const liveJobPoll = await captureStep('CustomerSegmentMembersQueryAuthoritativePoll', memberJobPollDocument, {
    id: persistedJobId,
  });
  const liveJob = responseData(liveJobPoll)['customerSegmentMembersQuery'];
  if (JSON.stringify(hydratedJobs[0]) !== JSON.stringify(liveJob)) {
    throw new Error(
      `Member-query job changed between prerequisite and public poll: ${JSON.stringify({ hydrated: hydratedJobs[0], polled: liveJob })}`,
    );
  }

  const cleanup = {
    duplicate: await cleanupSegment(duplicateSegmentId),
    persisted: await cleanupSegment(persistedSegmentId),
  };
  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        scenarioId: 'segment-authoritative-prerequisites',
        storeDomain,
        apiVersion,
        proxyVariables: {
          nameCreate: { name: persistedName, query: duplicateQuery },
          validJobCreate: { input: { segmentId: persistedSegmentId } },
          invalidJobCreate: { input: { segmentId: unknownSegmentId } },
          jobPoll: { id: persistedJobId },
        },
        setup: { setupCreate },
        createPrerequisites,
        liveNameCreate,
        validSegmentHydrate,
        liveValidJobCreate,
        invalidSegmentHydrate,
        liveInvalidJobCreate,
        memberJobHydrate,
        liveJobPoll,
        cleanup,
        upstreamCalls: [createPrerequisites, validSegmentHydrate, invalidSegmentHydrate, memberJobHydrate].map(
          upstreamCall,
        ),
        notes: [
          'Live Shopify evidence for a cold persisted-name segmentCreate collision, valid and invalid cold segmentId member-query jobs, and cold persisted member-job polling.',
          'Every upstreamCalls entry is the exact query-only GraphQL document and variables sent to Shopify by this registered capture script.',
        ],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} catch (error) {
  const cleanup = {
    duplicate: await cleanupSegment(duplicateSegmentId),
    persisted: await cleanupSegment(persistedSegmentId),
  };
  console.error(JSON.stringify({ ok: false, cleanup }, null, 2));
  throw error;
}
