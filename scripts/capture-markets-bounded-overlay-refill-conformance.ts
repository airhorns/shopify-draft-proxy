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
if (apiVersion !== '2026-04') throw new Error(`Expected SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${apiVersion}`);
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
  'markets',
  'markets-bounded-overlay-refill.json',
);
const createDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'markets-connection-arguments-create.graphql'),
  'utf8',
);
const readDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'markets-bounded-overlay-refill-read.graphql'),
  'utf8',
);
const cleanupDocument = `#graphql
mutation MarketsBoundedOverlayRefillCleanup($id: ID!) {
  marketDelete(id: $id) {
    deletedId
    userErrors { field message code }
  }
}
`;

function object(value: unknown): JsonObject | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? (value as JsonObject) : null;
}

function at(value: unknown, parts: Array<string | number>): unknown {
  let current = value;
  for (const part of parts) {
    current = typeof part === 'number' ? (Array.isArray(current) ? current[part] : undefined) : object(current)?.[part];
  }
  return current;
}

function assertOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || object(result.payload)?.['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

async function capture(operationName: string, query: string, variables: JsonObject): Promise<CapturedStep> {
  const result = await runGraphqlRequest(query, variables);
  assertOk(operationName, result);
  return { operationName, query, variables, response: { status: result.status, body: result.payload } };
}

function createdId(step: CapturedStep): string {
  const errors = at(step.response.body, ['data', 'marketCreate', 'userErrors']);
  if (!Array.isArray(errors) || errors.length > 0) {
    throw new Error(`${step.operationName} returned userErrors: ${JSON.stringify(errors)}`);
  }
  const id = at(step.response.body, ['data', 'marketCreate', 'market', 'id']);
  if (typeof id !== 'string') throw new Error(`${step.operationName} did not return a market id`);
  return id;
}

function connectionNames(step: CapturedStep, key = 'markets'): string[] {
  const edges = at(step.response.body, ['data', key, 'edges']);
  if (!Array.isArray(edges)) return [];
  return edges.flatMap((edge) => {
    const name = at(edge, ['node', 'name']);
    return typeof name === 'string' ? [name] : [];
  });
}

function pageFlag(step: CapturedStep, name: string, key = 'markets'): boolean | null {
  const value = at(step.response.body, ['data', key, 'pageInfo', name]);
  return typeof value === 'boolean' ? value : null;
}

async function captureUntil(
  label: string,
  operationName: string,
  query: string,
  variables: JsonObject,
  matches: (step: CapturedStep) => boolean,
): Promise<CapturedStep> {
  let last: CapturedStep | null = null;
  for (let attempt = 0; attempt < 20; attempt += 1) {
    last = await capture(operationName, query, variables);
    if (matches(last)) return last;
    await sleep(1500);
  }
  throw new Error(`${label} did not reach expected state: ${JSON.stringify(last, null, 2)}`);
}

function refillDocument(after: string): string {
  return `query DraftProxyConnectionOverlay($query: String!) { overlayWindow: markets(after: ${JSON.stringify(after)}, first: 2, query: $query, sortKey: NAME) { edges { cursor node { name handle status enabled type id name handle status enabled type } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }`;
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

const suffix = new Date().toISOString().replace(/\D/gu, '').slice(0, 17);
const prefix = `ZZZBoundedMarket${suffix}`;
const labels = ['A', 'C', 'E', 'B'] as const;
type Label = (typeof labels)[number];
const marketVariables = (label: Label): JsonObject => ({
  input: { name: `${prefix}${label}`, enabled: true },
});
const variables: Record<Label, JsonObject> = {
  A: marketVariables('A'),
  C: marketVariables('C'),
  E: marketVariables('E'),
  B: marketVariables('B'),
};
const ids: string[] = [];
const cleanup = (id: string) => capture('MarketsBoundedOverlayRefillCleanup', cleanupDocument, { id });

try {
  const baseCreates: CapturedStep[] = [];
  for (const label of ['A', 'C', 'E'] as const) {
    const step = await capture(`MarketsBoundedOverlayBase${label}Create`, createDocument, variables[label]);
    ids.push(createdId(step));
    baseCreates.push(step);
  }

  const readVariables = { query: `name:${prefix}*` };
  const baseRead = await captureUntil(
    'base markets window',
    'MarketsBoundedOverlayRefillRead',
    readDocument,
    readVariables,
    (step) => connectionNames(step).join('|') === `${prefix}A|${prefix}C` && pageFlag(step, 'hasNextPage') === true,
  );
  const cursor = at(baseRead.response.body, ['data', 'markets', 'pageInfo', 'endCursor']);
  if (typeof cursor !== 'string') throw new Error('base markets window did not return an end cursor');
  const refill = await capture('DraftProxyConnectionOverlay', refillDocument(cursor), readVariables);
  if (
    connectionNames(refill, 'overlayWindow').join('|') !== `${prefix}E` ||
    pageFlag(refill, 'hasNextPage', 'overlayWindow') !== false
  ) {
    throw new Error(`markets refill did not return only the boundary row: ${JSON.stringify(refill, null, 2)}`);
  }

  const liveStagedCreate = await capture('MarketsBoundedOverlayStagedCreate', createDocument, variables.B);
  ids.push(createdId(liveStagedCreate));
  const finalRead = await captureUntil(
    'final markets window',
    'MarketsBoundedOverlayRefillRead',
    readDocument,
    readVariables,
    (step) => connectionNames(step).join('|') === `${prefix}A|${prefix}B`,
  );
  const cleanupSteps: CapturedStep[] = [];
  for (const id of [...ids].reverse()) cleanupSteps.push(await cleanup(id));

  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId: 'markets-bounded-overlay-refill',
        storeDomain,
        apiVersion,
        proxyVariables: { create: variables.B, read: readVariables },
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
  const cleanupResults: unknown[] = [];
  for (const id of [...ids].reverse()) {
    try {
      cleanupResults.push(await cleanup(id));
    } catch (cleanupError) {
      cleanupResults.push(cleanupError instanceof Error ? cleanupError.message : String(cleanupError));
    }
  }
  console.error(JSON.stringify({ ok: false, cleanup: cleanupResults }, null, 2));
  throw error;
}
