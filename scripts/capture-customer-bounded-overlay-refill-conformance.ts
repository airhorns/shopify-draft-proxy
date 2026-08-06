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
  'customers',
  'customer-bounded-overlay-refill.json',
);
const createDocument = await readFile(
  path.join('config', 'parity-requests', 'customers', 'customer-live-hybrid-overlay-create.graphql'),
  'utf8',
);
const readDocument = await readFile(
  path.join('config', 'parity-requests', 'customers', 'customer-bounded-overlay-refill-read.graphql'),
  'utf8',
);
const duplicateHydrateDocument = await readFile(
  path.join('config', 'parity-requests', 'customers', 'customer-duplicate-hydrate.graphql'),
  'utf8',
);
const cleanupDocument = `#graphql
mutation CustomerBoundedOverlayRefillCleanup($input: CustomerDeleteInput!) {
  customerDelete(input: $input) {
    deletedCustomerId
    userErrors { field message }
  }
}
`;

function record(value: unknown): JsonObject | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? (value as JsonObject) : null;
}

function at(value: unknown, pathParts: Array<string | number>): unknown {
  let current = value;
  for (const part of pathParts) {
    if (typeof part === 'number') {
      current = Array.isArray(current) ? current[part] : undefined;
    } else {
      current = record(current)?.[part];
    }
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

function createdCustomerId(step: CapturedStep): string {
  const userErrors = at(step.response.body, ['data', 'customerCreate', 'userErrors']);
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${step.operationName} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
  const id = at(step.response.body, ['data', 'customerCreate', 'customer', 'id']);
  if (typeof id !== 'string') throw new Error(`${step.operationName} did not return a customer id`);
  return id;
}

function connectionEmails(step: CapturedStep, responseKey = 'customers'): string[] {
  const edges = at(step.response.body, ['data', responseKey, 'edges']);
  if (!Array.isArray(edges)) return [];
  return edges.flatMap((edge) => {
    const email = at(edge, ['node', 'email']);
    return typeof email === 'string' ? [email] : [];
  });
}

function pageInfoBool(step: CapturedStep, name: string, responseKey = 'customers'): boolean | null {
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

function customerRefillDocument(after: string): string {
  const required =
    'id firstName lastName displayName email phone locale note canDelete verifiedEmail dataSaleOptOut taxExempt taxExemptions state tags createdAt updatedAt defaultEmailAddress { emailAddress } defaultPhoneNumber { phoneNumber } defaultAddress { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea }';
  return `query DraftProxyConnectionOverlay($query: String!) { overlayWindow: customers(after: ${JSON.stringify(after)}, first: 2, query: $query, sortKey: NAME) { edges { cursor node { email displayName tags ${required} } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }`;
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

async function cleanupCustomer(id: string): Promise<CapturedStep> {
  return captureStep('CustomerBoundedOverlayRefillCleanup', cleanupDocument, { input: { id } });
}

const suffix = Date.now().toString(36);
const tag = `bounded-overlay-${suffix}`;
const labels = ['A', 'C', 'E', 'B'] as const;
type Label = (typeof labels)[number];
const customerVariables = (label: Label): JsonObject => ({
  input: {
    email: `bounded-overlay-${label.toLowerCase()}-${suffix}@example.com`,
    firstName: `Overlay${label}`,
    lastName: 'Bounded',
    tags: [tag],
  },
});
const variablesByLabel: Record<Label, JsonObject> = {
  A: customerVariables('A'),
  C: customerVariables('C'),
  E: customerVariables('E'),
  B: customerVariables('B'),
};
const ids: string[] = [];

try {
  const baseCreates: CapturedStep[] = [];
  for (const label of ['A', 'C', 'E'] as const) {
    const step = await captureStep(`CustomerBoundedOverlayBase${label}Create`, createDocument, variablesByLabel[label]);
    ids.push(createdCustomerId(step));
    baseCreates.push(step);
  }

  const readVariables = { query: `tag:${tag}` };
  const baseRead = await captureUntil(
    'base customer window',
    'CustomerBoundedOverlayRefillRead',
    readDocument,
    readVariables,
    (step) =>
      connectionEmails(step).join('|') ===
        `${at(variablesByLabel.A, ['input', 'email'])}|${at(variablesByLabel.C, ['input', 'email'])}` &&
      pageInfoBool(step, 'hasNextPage') === true,
  );
  const boundaryCursor = at(baseRead.response.body, ['data', 'customers', 'pageInfo', 'endCursor']);
  if (typeof boundaryCursor !== 'string') throw new Error('base customer window did not return an end cursor');

  const refill = await captureStep(
    'DraftProxyConnectionOverlay',
    customerRefillDocument(boundaryCursor),
    readVariables,
  );
  if (
    connectionEmails(refill, 'overlayWindow').join('|') !== String(at(variablesByLabel.E, ['input', 'email'])) ||
    pageInfoBool(refill, 'hasNextPage', 'overlayWindow') !== false
  ) {
    throw new Error(`customer refill did not return only the boundary row: ${JSON.stringify(refill, null, 2)}`);
  }

  const duplicateHydrate = await captureStep('CustomerDuplicateHydrate', duplicateHydrateDocument, {
    query: `email:${String(at(variablesByLabel.B, ['input', 'email']))}`,
  });
  const liveStagedCreate = await captureStep('CustomerBoundedOverlayStagedCreate', createDocument, variablesByLabel.B);
  ids.push(createdCustomerId(liveStagedCreate));

  const finalRead = await captureUntil(
    'final customer window',
    'CustomerBoundedOverlayRefillRead',
    readDocument,
    readVariables,
    (step) =>
      connectionEmails(step).join('|') ===
      `${at(variablesByLabel.A, ['input', 'email'])}|${at(variablesByLabel.B, ['input', 'email'])}`,
  );

  const cleanup: CapturedStep[] = [];
  for (const id of [...ids].reverse()) cleanup.push(await cleanupCustomer(id));

  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId: 'customer-bounded-overlay-refill',
        storeDomain,
        apiVersion,
        proxyVariables: { create: variablesByLabel.B, read: readVariables },
        setup: { baseCreates, baseRead, refill, duplicateHydrate },
        liveStagedCreate,
        finalRead,
        cleanup,
        upstreamCalls: [duplicateHydrate, baseRead, refill].map(upstreamCall),
      },
      null,
      2,
    )}\n`,
  );
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} catch (error) {
  const cleanup: unknown[] = [];
  for (const id of [...ids].reverse()) {
    try {
      cleanup.push(await cleanupCustomer(id));
    } catch (cleanupError) {
      cleanup.push(cleanupError instanceof Error ? cleanupError.message : String(cleanupError));
    }
  }
  console.error(JSON.stringify({ ok: false, cleanup }, null, 2));
  throw error;
}
