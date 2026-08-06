/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { readFile, writeFile } from 'node:fs/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const FIXTURE_PATHS = [
  'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/metafield-definition-catalog-connection.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/metafield-definition-non-product-metafields.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/metafield-definition-non-product-owner-types.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/metafield-definition-owner-scoped-duplicates.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/metafield-definition-update-name-description-length.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/metafield-definition-validation-option-names.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/metafield-definition-validations-input.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/metafields-set-validation-gaps.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/standard-metafield-definition-enable-validation.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/customers/customer-set-custom-id.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metafields/metafield-definition-access-validation.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metafields/metafield-definition-app-namespace-resolution.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metafields/metafield-definition-delete-type-guard-no-metafields.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/metaobject-display-name-conflict.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/metaobjectDefinitionUpdate-immutable.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/storefront/storefront-catalog-enrichment.json',
  'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/storefront/storefront-collections-read-after-admin-setup.json',
] as const;

const DOCUMENT_PATHS = {
  MetafieldDefinitionsHydratePinnedOwner:
    'src/runtime_graphql/metafields/metafield-definitions-hydrate-pinned-owner.graphql',
  MetafieldDefinitionsHydrateResourceScope:
    'src/runtime_graphql/metafields/metafield-definitions-hydrate-resource-scope.graphql',
} as const;

type BoundedOperationName = keyof typeof DOCUMENT_PATHS;
type JsonObject = Record<string, unknown>;
type RefreshContext = {
  apiVersion: string;
  cursors: Map<string, string | null>;
};

function readObject(value: unknown): JsonObject | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonObject) : null;
}

function fixtureApiVersion(fixturePath: string): string {
  const match = fixturePath.match(/\/(\d{4}-\d{2})\//u);
  if (!match?.[1]) throw new Error(`Cannot determine API version from ${fixturePath}`);
  return match[1];
}

function isBoundedOperationName(value: unknown): value is BoundedOperationName {
  return typeof value === 'string' && Object.hasOwn(DOCUMENT_PATHS, value);
}

function assertSuccessful(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: HTTP ${result.status} ${JSON.stringify(result.payload.errors)}`);
  }
}

const { storeDomain, adminOrigin, apiVersion: probeApiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
if (storeDomain !== 'harry-test-heelo.myshopify.com') {
  throw new Error(`This recorder refreshes harry-test-heelo.myshopify.com fixtures, not ${storeDomain}.`);
}

const accessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion: probeApiVersion });
const headers = buildAdminAuthHeaders(accessToken);
const documents = Object.fromEntries(
  await Promise.all(
    Object.entries(DOCUMENT_PATHS).map(async ([operationName, documentPath]) => [
      operationName,
      await readFile(documentPath, 'utf8'),
    ]),
  ),
) as Record<BoundedOperationName, string>;
const clients = new Map<string, ReturnType<typeof createAdminGraphqlClient>>();
const resultCache = new Map<string, ConformanceGraphqlResult>();
let liveRequestCount = 0;
let refreshedCallCount = 0;

function clientFor(apiVersion: string): ReturnType<typeof createAdminGraphqlClient> {
  const existing = clients.get(apiVersion);
  if (existing) return existing;
  const client = createAdminGraphqlClient({ adminOrigin, apiVersion, headers });
  clients.set(apiVersion, client);
  return client;
}

async function runBoundedCall(
  operationName: BoundedOperationName,
  query: string,
  variables: JsonObject,
  apiVersion: string,
): Promise<ConformanceGraphqlResult> {
  const cacheKey = JSON.stringify({ apiVersion, operationName, variables, query });
  const cached = resultCache.get(cacheKey);
  if (cached) return cached;

  const result = await clientFor(apiVersion).runGraphqlRaw(query, variables);
  assertSuccessful(result, `${operationName} ${apiVersion}`);
  resultCache.set(cacheKey, result);
  liveRequestCount += 1;
  return result;
}

async function refreshCall(call: JsonObject, operationName: BoundedOperationName, context: RefreshContext) {
  const originalVariables = readObject(call['variables']);
  if (!originalVariables) throw new Error(`${operationName} omitted variables`);
  const variables = { ...originalVariables };

  let scopeKey: string | null = null;
  if (operationName === 'MetafieldDefinitionsHydrateResourceScope') {
    scopeKey = JSON.stringify([variables['ownerType'], variables['query']]);
    if (variables['after'] === null || variables['after'] === undefined) {
      variables['after'] = null;
      context.cursors.set(scopeKey, null);
    } else {
      const currentCursor = context.cursors.get(scopeKey);
      if (typeof currentCursor !== 'string') {
        throw new Error(`${operationName} continuation has no live cursor for ${scopeKey}`);
      }
      variables['after'] = currentCursor;
    }
  }

  const query = documents[operationName];
  const result = await runBoundedCall(operationName, query, variables, context.apiVersion);
  if (scopeKey) {
    const data = readObject(result.payload.data);
    const connection = readObject(data?.['metafieldDefinitions']);
    const pageInfo = readObject(connection?.['pageInfo']);
    const endCursor = pageInfo?.['endCursor'];
    if (pageInfo?.['hasNextPage'] === true && typeof endCursor !== 'string') {
      throw new Error(`${operationName} live page omitted endCursor for ${scopeKey}`);
    }
    context.cursors.set(scopeKey, pageInfo?.['hasNextPage'] === true ? (endCursor as string) : null);
  }

  refreshedCallCount += 1;
  return {
    ...call,
    method: 'POST',
    apiSurface: 'admin',
    path: `/admin/api/${context.apiVersion}/graphql.json`,
    operationName,
    variables,
    query,
    response: { status: result.status, body: result.payload },
  };
}

async function refreshValue(value: unknown, context: RefreshContext): Promise<unknown> {
  if (Array.isArray(value)) {
    const refreshed: unknown[] = [];
    for (const item of value) refreshed.push(await refreshValue(item, context));
    return refreshed;
  }

  const object = readObject(value);
  if (!object) return value;
  const operationName = object['operationName'];
  if (isBoundedOperationName(operationName)) return refreshCall(object, operationName, context);

  const refreshed: JsonObject = {};
  for (const [key, child] of Object.entries(object)) refreshed[key] = await refreshValue(child, context);
  return refreshed;
}

for (const fixturePath of FIXTURE_PATHS) {
  const fixture = JSON.parse(await readFile(fixturePath, 'utf8')) as unknown;
  const refreshed = await refreshValue(fixture, {
    apiVersion: fixtureApiVersion(fixturePath),
    cursors: new Map(),
  });
  await writeFile(fixturePath, `${JSON.stringify(refreshed, null, 2)}\n`);
  console.log(`refreshed bounded prerequisite calls in ${fixturePath}`);
}

console.log(
  `refreshed ${refreshedCallCount} cassette calls across ${FIXTURE_PATHS.length} fixtures with ${liveRequestCount} deduplicated live requests`,
);
