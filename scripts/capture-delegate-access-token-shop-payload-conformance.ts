/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const scenario = 'delegate-access-token-shop-payload';
const createPath = 'config/parity-requests/apps/delegateAccessTokenCreate-shop-payload.graphql';
const destroyPath = 'config/parity-requests/apps/delegateAccessTokenDestroy-shop-payload.graphql';
const unknownDestroyPath = 'config/parity-requests/apps/delegateAccessTokenDestroy-shop-payload-unknown.graphql';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'apps');

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readPath<T>(value: unknown, pathSegments: string[]): T | null {
  let current = value;
  for (const segment of pathSegments) {
    if (typeof current !== 'object' || current === null || !(segment in current)) {
      return null;
    }
    current = (current as Record<string, unknown>)[segment];
  }
  return current as T;
}

function assertShopPayload(payload: ConformanceGraphqlPayload, root: string, context: string): void {
  const mutationPayload = readPath<unknown>(payload, ['data', root]);
  const shop = readObject(readObject(mutationPayload)?.['shop']);
  if (typeof shop?.['id'] !== 'string' || !shop['id'].startsWith('gid://shopify/Shop/')) {
    throw new Error(`Expected ${context} ${root}.shop.id to be populated.`);
  }
}

function assertUserErrors(payload: ConformanceGraphqlPayload, root: string, context: string): void {
  const mutationPayload = readObject(readPath<unknown>(payload, ['data', root]));
  if (!Array.isArray(mutationPayload?.['userErrors'])) {
    throw new Error(`Expected ${context} ${root}.userErrors to be an array.`);
  }
}

function extractAccessToken(payload: ConformanceGraphqlPayload): string {
  const token = readPath<unknown>(payload, ['data', 'delegateAccessTokenCreate', 'delegateAccessToken', 'accessToken']);
  if (typeof token !== 'string' || token.trim() === '') {
    throw new Error('Expected delegateAccessTokenCreate to return a raw access token for destroy capture.');
  }
  return token;
}

function redactCreatePayload(payload: ConformanceGraphqlPayload): ConformanceGraphqlPayload {
  return JSON.parse(
    JSON.stringify(payload, (_key, value) =>
      typeof value === 'string' && (value.startsWith('shpat_') || value.startsWith('shpca_'))
        ? '[redacted-live-delegate-token]'
        : value,
    ),
  ) as ConformanceGraphqlPayload;
}

async function readRequest(filePath: string): Promise<string> {
  return await readFile(filePath, 'utf8');
}

async function cleanupToken(token: string | null, destroyDocument: string): Promise<void> {
  if (!token) return;

  try {
    await client.runGraphqlRequest(destroyDocument, { token });
  } catch (error) {
    console.error(`Failed to cleanup delegate token: ${error instanceof Error ? error.message : String(error)}`);
  }
}

const createDocument = await readRequest(createPath);
const destroyDocument = await readRequest(destroyPath);
const unknownDestroyDocument = await readRequest(unknownDestroyPath);

let rawToken: string | null = null;
let destroyCaptured = false;
try {
  const create = await client.runGraphqlRequest(createDocument);
  assertNoTopLevelErrors(create, 'delegateAccessTokenCreate shop payload capture');
  assertShopPayload(create.payload, 'delegateAccessTokenCreate', 'create');
  assertUserErrors(create.payload, 'delegateAccessTokenCreate', 'create');
  rawToken = extractAccessToken(create.payload);

  const destroy = await client.runGraphqlRequest(destroyDocument, { token: rawToken });
  assertNoTopLevelErrors(destroy, 'delegateAccessTokenDestroy shop payload capture');
  assertShopPayload(destroy.payload, 'delegateAccessTokenDestroy', 'destroy');
  assertUserErrors(destroy.payload, 'delegateAccessTokenDestroy', 'destroy');
  destroyCaptured = true;

  const unknownDestroy = await client.runGraphqlRequest(unknownDestroyDocument);
  assertNoTopLevelErrors(unknownDestroy, 'delegateAccessTokenDestroy unknown-token shop payload capture');
  assertShopPayload(unknownDestroy.payload, 'delegateAccessTokenDestroy', 'unknown destroy');
  assertUserErrors(unknownDestroy.payload, 'delegateAccessTokenDestroy', 'unknown destroy');

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenario,
    notes: [
      'HAR-753 live capture for DelegateAccessTokenCreatePayload.shop and DelegateAccessTokenDestroyPayload.shop.',
      'The create branch selected shop { id myshopifyDomain currencyCode } and destroyed the returned delegate token immediately.',
      'The destroy success and unknown-token userError branches both selected shop { id }; Shopify returned a non-null shop in both payloads.',
      'The raw live delegate token is redacted in the checked-in fixture and is not needed for proxy replay.',
    ],
    operationNames: ['delegateAccessTokenCreate', 'delegateAccessTokenDestroy'],
    upstreamCalls: [],
    evidence: {
      live: {
        cleanup: {
          destroyCaptured,
        },
      },
      parity: {
        expected: {
          delegateCreate: {
            documentPath: createPath,
            payload: redactCreatePayload(create.payload),
          },
          delegateDestroy: {
            documentPath: destroyPath,
            payload: destroy.payload,
          },
          delegateDestroyUnknown: {
            documentPath: unknownDestroyPath,
            payload: unknownDestroy.payload,
          },
        },
      },
    },
  };

  await mkdir(outputDir, { recursive: true });
  const fixturePath = path.join(outputDir, `${scenario}.json`);
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
} finally {
  if (!destroyCaptured) {
    await cleanupToken(rawToken, destroyDocument);
  }
}
