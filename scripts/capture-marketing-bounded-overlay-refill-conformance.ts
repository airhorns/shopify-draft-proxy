/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonObject = Record<string, unknown>;
type CapturedStep = {
  operationName: string;
  query: string;
  variables: JsonObject;
  response: { status: number; body: unknown };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const outputPath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'marketing',
  'marketing-bounded-overlay-refill.json',
);
const createDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-connection-window-create.graphql'),
  'utf8',
);
const readDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-bounded-overlay-refill-read.graphql'),
  'utf8',
);
const cleanupDocument = `#graphql
mutation MarketingBoundedOverlayRefillCleanup($remoteId: String) {
  marketingActivityDeleteExternal(remoteId: $remoteId) {
    deletedMarketingActivityId
    userErrors { field message code }
  }
}
`;

function record(value: unknown): JsonObject | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? (value as JsonObject) : null;
}

function at(value: unknown, parts: Array<string | number>): unknown {
  let current = value;
  for (const part of parts) {
    current = typeof part === 'number' ? (Array.isArray(current) ? current[part] : undefined) : record(current)?.[part];
  }
  return current;
}

function assertGraphqlOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || record(result.payload)?.['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

async function captureStep(operationName: string, query: string, variables: JsonObject): Promise<CapturedStep> {
  const result = await runGraphqlRequest(query, variables);
  assertGraphqlOk(operationName, result);
  return {
    operationName,
    query,
    variables,
    response: { status: result.status, body: result.payload },
  };
}

function assertCreated(step: CapturedStep): void {
  const userErrors = at(step.response.body, ['data', 'created', 'userErrors']);
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${step.operationName} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function connectionTitles(step: CapturedStep, responseKey = 'marketingActivities'): string[] {
  const edges = at(step.response.body, ['data', responseKey, 'edges']);
  if (!Array.isArray(edges)) return [];
  return edges.flatMap((edge) => {
    const title = at(edge, ['node', 'title']);
    return typeof title === 'string' ? [title] : [];
  });
}

function pageInfoBool(step: CapturedStep, name: string, responseKey = 'marketingActivities'): boolean | null {
  const value = at(step.response.body, ['data', responseKey, 'pageInfo', name]);
  return typeof value === 'boolean' ? value : null;
}

async function captureUntil(
  label: string,
  operationName: string,
  query: string,
  variables: JsonObject,
  predicate: (step: CapturedStep) => boolean,
): Promise<CapturedStep> {
  let last: CapturedStep | null = null;
  for (let attempt = 0; attempt < 20; attempt += 1) {
    last = await captureStep(operationName, query, variables);
    if (predicate(last)) return last;
    await sleep(1500);
  }
  throw new Error(`${label} did not reach the expected indexed state: ${JSON.stringify(last, null, 2)}`);
}

function marketingRefillDocument(after: string): string {
  const required =
    'id title createdAt updatedAt isExternal status tactic app { id title } marketingEvent { id remoteId startedAt scheduledToEndAt description channelHandle type }';
  return `query DraftProxyConnectionOverlay($query: String!) { overlayWindow: marketingActivities(after: ${JSON.stringify(after)}, first: 2, query: $query, sortKey: TITLE) { edges { cursor node { title status marketingEvent { remoteId } ${required} } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }`;
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

const suffix = `${Date.now()}${Math.random().toString(36).slice(2, 8)}`;
const labels = ['A', 'C', 'E', 'B'] as const;
type Label = (typeof labels)[number];
const common = `BoundedOverlay${suffix}`;
const remoteId = (label: Label): string => `bounded-overlay-${label.toLowerCase()}-${suffix}`;
const remoteIds: Record<Label, string> = {
  A: remoteId('A'),
  C: remoteId('C'),
  E: remoteId('E'),
  B: remoteId('B'),
};
const marketingVariables = (label: Label): JsonObject => ({
  input: {
    title: `${common}${label}`,
    remoteId: remoteIds[label],
    status: 'ACTIVE',
    remoteUrl: `https://example.com/${remoteIds[label]}`,
    tactic: 'NEWSLETTER',
    marketingChannelType: 'EMAIL',
    utm: { campaign: remoteIds[label], source: 'email', medium: 'newsletter' },
  },
});
const variablesByLabel: Record<Label, JsonObject> = {
  A: marketingVariables('A'),
  C: marketingVariables('C'),
  E: marketingVariables('E'),
  B: marketingVariables('B'),
};
const createdRemoteIds: string[] = [];

async function cleanup(remoteId: string): Promise<CapturedStep> {
  return captureStep('MarketingBoundedOverlayRefillCleanup', cleanupDocument, { remoteId });
}

try {
  const baseCreates: CapturedStep[] = [];
  for (const label of ['A', 'C', 'E'] as const) {
    const step = await captureStep(
      `MarketingBoundedOverlayBase${label}Create`,
      createDocument,
      variablesByLabel[label],
    );
    assertCreated(step);
    createdRemoteIds.push(remoteIds[label]);
    baseCreates.push(step);
  }

  const query = labels.map((label) => `title:${common}${label}`).join(' OR ');
  const readVariables = { query };
  const baseRead = await captureUntil(
    'base marketing window',
    'MarketingBoundedOverlayRefillRead',
    readDocument,
    readVariables,
    (step) =>
      connectionTitles(step).join('|') === `${common}A|${common}C` && pageInfoBool(step, 'hasNextPage') === true,
  );
  const boundaryCursor = at(baseRead.response.body, ['data', 'marketingActivities', 'pageInfo', 'endCursor']);
  if (typeof boundaryCursor !== 'string') throw new Error('base marketing window did not return an end cursor');
  const refill = await captureStep(
    'DraftProxyConnectionOverlay',
    marketingRefillDocument(boundaryCursor),
    readVariables,
  );
  if (
    connectionTitles(refill, 'overlayWindow').join('|') !== `${common}E` ||
    pageInfoBool(refill, 'hasNextPage', 'overlayWindow') !== false
  ) {
    throw new Error(`marketing refill did not return only the boundary row: ${JSON.stringify(refill, null, 2)}`);
  }

  const liveStagedCreate = await captureStep('MarketingBoundedOverlayStagedCreate', createDocument, variablesByLabel.B);
  assertCreated(liveStagedCreate);
  createdRemoteIds.push(remoteIds.B);
  const finalRead = await captureUntil(
    'final marketing window',
    'MarketingBoundedOverlayRefillRead',
    readDocument,
    readVariables,
    (step) => connectionTitles(step).join('|') === `${common}A|${common}B`,
  );

  const cleanupSteps: CapturedStep[] = [];
  for (const remoteId of [...createdRemoteIds].reverse()) cleanupSteps.push(await cleanup(remoteId));
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId: 'marketing-bounded-overlay-refill',
        storeDomain,
        apiVersion,
        proxyVariables: { create: variablesByLabel.B, read: readVariables },
        setup: { baseCreates, baseRead, refill },
        liveStagedCreate,
        finalRead,
        cleanup: cleanupSteps,
        upstreamCalls: [baseRead, refill].map(upstreamCall),
      },
      null,
      2,
    )}\n`,
  );
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} catch (error) {
  const cleanupSteps: unknown[] = [];
  for (const remoteId of [...createdRemoteIds].reverse()) {
    try {
      cleanupSteps.push(await cleanup(remoteId));
    } catch (cleanupError) {
      cleanupSteps.push(cleanupError instanceof Error ? cleanupError.message : String(cleanupError));
    }
  }
  console.error(JSON.stringify({ ok: false, cleanup: cleanupSteps }, null, 2));
  throw error;
}
