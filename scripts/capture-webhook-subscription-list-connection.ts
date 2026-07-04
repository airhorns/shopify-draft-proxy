/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedRequest = {
  documentPath: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'webhooks');
const outputPath = path.join(outputDir, 'webhook-subscription-list-connection.json');

const requestDir = path.join('config', 'parity-requests', 'webhooks');
const createRequestPath = path.join(requestDir, 'webhookSubscriptionCreate-parity.graphql');
const deleteRequestPath = path.join(requestDir, 'webhookSubscriptionDelete-parity.graphql');
const listRequestPath = path.join(requestDir, 'webhook-subscription-list-connection-read.graphql');
const invalidSortKeyRequestPath = path.join(requestDir, 'webhook-subscription-invalid-sort-key.graphql');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readText(relativePath: string): Promise<string> {
  return readFile(path.join(process.cwd(), relativePath), 'utf8');
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function requireSuccessfulGraphql(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result.payload)}`);
  }
}

async function capture(documentPath: string, variables: Record<string, unknown>): Promise<CapturedRequest> {
  const document = await readText(documentPath);
  const response = await runGraphqlRequest(document, variables);
  requireSuccessfulGraphql(response, documentPath);

  return { documentPath, variables, response };
}

async function captureGraphqlValidation(documentPath: string): Promise<CapturedRequest> {
  const document = await readText(documentPath);
  const response = await runGraphqlRequest(document, {});
  if (response.status < 200 || response.status >= 300 || !response.payload.errors) {
    throw new Error(`${documentPath} did not capture GraphQL validation errors: ${JSON.stringify(response.payload)}`);
  }

  return { documentPath, variables: {}, response };
}

function createVariables(topic: string, uri: string): Record<string, unknown> {
  return {
    topic,
    webhookSubscription: {
      uri,
      format: 'JSON',
      includeFields: ['id'],
      metafieldNamespaces: [],
      filter: '',
    },
  };
}

function readCreatedWebhookId(createCapture: CapturedRequest): string {
  const data = createCapture.response.payload.data;
  if (!isObject(data)) throw new Error('webhookSubscriptionCreate did not return data.');

  const payload = data['webhookSubscriptionCreate'];
  if (!isObject(payload)) throw new Error('webhookSubscriptionCreate did not return a payload.');
  if (Array.isArray(payload['userErrors']) && payload['userErrors'].length > 0) {
    throw new Error(`webhookSubscriptionCreate returned userErrors: ${JSON.stringify(payload['userErrors'])}`);
  }

  const subscription = payload['webhookSubscription'];
  if (!isObject(subscription) || typeof subscription['id'] !== 'string') {
    throw new Error(`webhookSubscriptionCreate did not return a webhookSubscription.id: ${JSON.stringify(payload)}`);
  }

  return subscription['id'];
}

function readWebhookSubscriptionsConnection(captureResult: CapturedRequest, label: string): Record<string, unknown> {
  const data = captureResult.response.payload.data;
  if (!isObject(data)) {
    throw new Error(`${label} did not return data: ${JSON.stringify(captureResult.response.payload)}`);
  }

  const connection = data['webhookSubscriptions'];
  if (!isObject(connection)) {
    throw new Error(`${label} did not return webhookSubscriptions: ${JSON.stringify(captureResult.response.payload)}`);
  }

  return connection;
}

function readEndCursor(captureResult: CapturedRequest): string {
  const connection = readWebhookSubscriptionsConnection(captureResult, 'list read');
  const pageInfo = connection['pageInfo'];
  const cursor = isObject(pageInfo) ? pageInfo['endCursor'] : null;
  if (typeof cursor !== 'string') {
    throw new Error(
      `Expected list read to return pageInfo.endCursor: ${JSON.stringify(captureResult.response.payload)}`,
    );
  }
  return cursor;
}

function readEdgeCursor(captureResult: CapturedRequest, index: number): string {
  const connection = readWebhookSubscriptionsConnection(captureResult, 'list read');
  const edges = connection['edges'];
  const edge = Array.isArray(edges) ? edges[index] : null;
  const cursor = isObject(edge) ? edge['cursor'] : null;
  if (typeof cursor !== 'string') {
    throw new Error(
      `Expected list read to return edges[${index}].cursor: ${JSON.stringify(captureResult.response.payload)}`,
    );
  }
  return cursor;
}

function assertConnectionIds(captureResult: CapturedRequest, expectedIds: string[], label: string): void {
  const connection = readWebhookSubscriptionsConnection(captureResult, label);
  const nodes = connection['nodes'];
  if (!Array.isArray(nodes)) {
    throw new Error(`${label} did not return nodes: ${JSON.stringify(connection)}`);
  }
  const ids = nodes.map((node) => (isObject(node) ? node['id'] : null));
  if (JSON.stringify(ids) !== JSON.stringify(expectedIds)) {
    throw new Error(`${label} returned unexpected ids ${JSON.stringify(ids)} expected ${JSON.stringify(expectedIds)}`);
  }
}

function createdIdAt(index: number): string {
  const id = createdIds[index];
  if (typeof id !== 'string') {
    throw new Error(`Expected created webhook subscription id at index ${index}.`);
  }

  return id;
}

const suffix = `${Date.now()}`;
const uriPrefix = `https://example.com/hermes-webhook-list-${suffix}`;
const createdIds: string[] = [];
const cleanup: CapturedRequest[] = [];

let createOne: CapturedRequest | null = null;
let createTwo: CapturedRequest | null = null;
let createThree: CapturedRequest | null = null;
let defaultSort: CapturedRequest | null = null;
let firstPage: CapturedRequest | null = null;
let secondPage: CapturedRequest | null = null;
let beforePage: CapturedRequest | null = null;
let reverseId: CapturedRequest | null = null;
let createdAtFilter: CapturedRequest | null = null;
let updatedAtFilter: CapturedRequest | null = null;
let invalidSortKey: CapturedRequest | null = null;

try {
  createOne = await capture(createRequestPath, createVariables('ORDERS_CREATE', `${uriPrefix}-1`));
  createdIds.push(readCreatedWebhookId(createOne));
  createTwo = await capture(createRequestPath, createVariables('ORDERS_PAID', `${uriPrefix}-2`));
  createdIds.push(readCreatedWebhookId(createTwo));
  createThree = await capture(createRequestPath, createVariables('PRODUCTS_CREATE', `${uriPrefix}-3`));
  createdIds.push(readCreatedWebhookId(createThree));

  defaultSort = await capture(listRequestPath, { first: 3 });
  assertConnectionIds(defaultSort, createdIds, 'default CREATED_AT sort read');

  firstPage = await capture(listRequestPath, { first: 2 });
  assertConnectionIds(firstPage, createdIds.slice(0, 2), 'first page read');

  secondPage = await capture(listRequestPath, { first: 2, after: readEndCursor(firstPage) });
  assertConnectionIds(secondPage, [createdIdAt(2)], 'second page read');

  beforePage = await capture(listRequestPath, { last: 1, before: readEdgeCursor(defaultSort, 2) });
  assertConnectionIds(beforePage, [createdIdAt(1)], 'last/before page read');

  reverseId = await capture(listRequestPath, { first: 3, sortKey: 'ID', reverse: true });
  assertConnectionIds(reverseId, [...createdIds].reverse(), 'reverse ID sort read');

  createdAtFilter = await capture(listRequestPath, { first: 3, query: 'created_at:>=2000-01-01' });
  assertConnectionIds(createdAtFilter, createdIds, 'created_at query read');

  updatedAtFilter = await capture(listRequestPath, { first: 3, query: 'updated_at:<=2100-01-01' });
  assertConnectionIds(updatedAtFilter, createdIds, 'updated_at query read');

  invalidSortKey = await captureGraphqlValidation(invalidSortKeyRequestPath);
} finally {
  for (const id of createdIds) {
    cleanup.push(await capture(deleteRequestPath, { id }));
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'Captures webhookSubscriptions cursor windows, pageInfo, CREATED_AT default sorting, ID reverse sorting, created_at/updated_at search filters, and invalid sortKey validation.',
        'The recorder creates three temporary API webhook subscriptions and deletes them during cleanup.',
      ],
      setup: {
        createOne,
        createTwo,
        createThree,
      },
      reads: {
        defaultSort,
        firstPage,
        secondPage,
        beforePage,
        reverseId,
        createdAtFilter,
        updatedAtFilter,
      },
      validation: {
        invalidSortKey,
      },
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote webhook subscription list connection fixture to ${outputPath}`);
