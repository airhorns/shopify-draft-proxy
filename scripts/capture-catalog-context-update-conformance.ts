/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedCase = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

type CleanupEntry = {
  kind: 'catalog' | 'market';
  id: string;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-market-create.graphql'),
  'utf8',
);
const catalogCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-catalog-create.graphql'),
  'utf8',
);
const noArgsDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-no-args.graphql'),
  'utf8',
);
const removesOnlyDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-removes-only.graphql'),
  'utf8',
);
const duplicateAddDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-market-taken.graphql'),
  'utf8',
);
const catalogNotFoundDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-catalog-not-found.graphql'),
  'utf8',
);
const readDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-read.graphql'),
  'utf8',
);

const catalogDeleteDocument = `#graphql
mutation CatalogContextUpdateCatalogCleanup($id: ID!) {
  catalogDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const marketDeleteDocument = `#graphql
mutation CatalogContextUpdateMarketCleanup($id: ID!) {
  marketDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const cases: CapturedCase[] = [];
const cleanup: CleanupEntry[] = [];
const createdCatalogIds: string[] = [];
const createdMarketIds: string[] = [];
const suffix = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);

function assertGraphqlOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result)}`);
  }
}

function rootPayload(result: ConformanceGraphqlResult, root: string): Record<string, unknown> {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null || Array.isArray(data)) {
    throw new Error(`Missing data for ${root}: ${JSON.stringify(result.payload)}`);
  }
  const payload = (data as Record<string, unknown>)[root];
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) {
    throw new Error(`Missing root payload ${root}: ${JSON.stringify(result.payload)}`);
  }
  return payload as Record<string, unknown>;
}

function userErrors(result: ConformanceGraphqlResult, root: string): Array<Record<string, unknown>> {
  const errors = rootPayload(result, root)['userErrors'];
  if (!Array.isArray(errors)) return [];
  return errors.filter(
    (error): error is Record<string, unknown> =>
      typeof error === 'object' && error !== null && !Array.isArray(error),
  );
}

function assertNoUserErrors(label: string, result: ConformanceGraphqlResult, root: string): void {
  const errors = userErrors(result, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertUserError(
  label: string,
  result: ConformanceGraphqlResult,
  root: string,
  expectedCode: string,
  expectedField: string[],
): void {
  const errors = userErrors(result, root);
  const found = errors.some(
    (error) => error['code'] === expectedCode && JSON.stringify(error['field']) === JSON.stringify(expectedField),
  );
  if (!found) {
    throw new Error(
      `${label} missing expected ${expectedCode} at ${JSON.stringify(expectedField)}: ${JSON.stringify(errors)}`,
    );
  }
}

function nestedId(result: ConformanceGraphqlResult, root: string, field: string): string {
  const payload = rootPayload(result, root);
  const node = payload[field];
  if (typeof node !== 'object' || node === null || Array.isArray(node)) {
    throw new Error(`Missing ${root}.${field}: ${JSON.stringify(result.payload)}`);
  }
  const id = (node as Record<string, unknown>)['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`Missing ${root}.${field}.id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

async function captureCase(
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedCase> {
  const response = await runGraphqlRequest(query, variables);
  assertGraphqlOk(name, response);
  const capture = { name, query, variables, response };
  cases.push(capture);
  return capture;
}

function marketInput(label: string): Record<string, unknown> {
  return {
    input: {
      name: `Catalog Context ${label} ${suffix}`,
    },
  };
}

function marketContext(marketIds: string[]): Record<string, unknown> {
  return {
    marketIds,
  };
}

async function createMarket(label: string): Promise<string> {
  const capture = await captureCase(`${label}MarketCreate`, marketCreateDocument, marketInput(label));
  assertNoUserErrors(label, capture.response, 'marketCreate');
  const id = nestedId(capture.response, 'marketCreate', 'market');
  createdMarketIds.push(id);
  return id;
}

async function createCatalog(label: string, marketIds: string[]): Promise<string> {
  const capture = await captureCase(`${label}CatalogCreate`, catalogCreateDocument, {
    input: {
      title: `Catalog Context ${label} Catalog ${suffix}`,
      status: 'ACTIVE',
      context: marketContext(marketIds),
    },
  });
  assertNoUserErrors(label, capture.response, 'catalogCreate');
  const id = nestedId(capture.response, 'catalogCreate', 'catalog');
  createdCatalogIds.push(id);
  return id;
}

async function cleanupCatalog(id: string): Promise<void> {
  const response = await runGraphqlRequest(catalogDeleteDocument, { id });
  cleanup.push({ kind: 'catalog', id, response });
}

async function cleanupMarket(id: string): Promise<void> {
  const response = await runGraphqlRequest(marketDeleteDocument, { id });
  cleanup.push({ kind: 'market', id, response });
}

let captureFailure: unknown = null;

try {
  const noArgsMarketId = await createMarket('No Args');
  const noArgsCatalogId = await createCatalog('No Args', [noArgsMarketId]);
  const noArgs = await captureCase('catalogContextUpdateNoArgs', noArgsDocument, {
    catalogId: noArgsCatalogId,
    contextsToAdd: null,
    contextsToRemove: null,
  });
  assertUserError(
    'catalogContextUpdate no args',
    noArgs.response,
    'catalogContextUpdate',
    'REQUIRES_CONTEXTS_TO_ADD_OR_REMOVE',
    ['contextsToAdd'],
  );

  const removeMarketId = await createMarket('Remove Target');
  const remainingMarketId = await createMarket('Remove Remaining');
  const removesCatalogId = await createCatalog('Removes', [removeMarketId, remainingMarketId]);
  const removesOnly = await captureCase('catalogContextUpdateRemovesOnly', removesOnlyDocument, {
    catalogId: removesCatalogId,
    contextsToRemove: {
      marketIds: [removeMarketId],
    },
  });
  assertNoUserErrors('catalogContextUpdate removes only', removesOnly.response, 'catalogContextUpdate');
  await captureCase('catalogReadAfterRemove', readDocument, { id: removesCatalogId });

  const takenMarketId = await createMarket('Taken Existing');
  const targetMarketId = await createMarket('Taken Target');
  await createCatalog('Taken Holder', [takenMarketId]);
  const takenTargetCatalogId = await createCatalog('Taken Target', [targetMarketId]);
  const duplicateAdd = await captureCase('catalogContextUpdateDuplicateAdd', duplicateAddDocument, {
    catalogId: takenTargetCatalogId,
    contextsToAdd: {
      marketIds: [takenMarketId],
    },
  });
  assertNoUserErrors('catalogContextUpdate duplicate add', duplicateAdd.response, 'catalogContextUpdate');
  await captureCase('catalogReadAfterDuplicateAdd', readDocument, { id: takenTargetCatalogId });

  const catalogNotFound = await captureCase('catalogContextUpdateCatalogNotFound', catalogNotFoundDocument, {
    catalogId: 'gid://shopify/MarketCatalog/999999999999',
    contextsToAdd: {
      marketIds: ['gid://shopify/Market/999999999999'],
    },
  });
  assertUserError(
    'catalogContextUpdate catalog not found',
    catalogNotFound.response,
    'catalogContextUpdate',
    'CATALOG_NOT_FOUND',
    ['catalogId'],
  );
} catch (error) {
  captureFailure = error;
} finally {
  for (const id of createdCatalogIds.toReversed()) {
    await cleanupCatalog(id);
  }
  for (const id of createdMarketIds.toReversed()) {
    await cleanupMarket(id);
  }
}

if (captureFailure) {
  throw captureFailure;
}

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'catalog-context-update-lifecycle.json');
await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      scope:
        'catalogContextUpdate required-context validation, remove-only update, duplicate market add behavior, and catalog-not-found typing',
      setup: {
        suffix,
        createdMarketIds,
        createdCatalogIds,
        cleanup:
          'The capture creates disposable markets and MarketCatalogs, records catalogContextUpdate branches, deletes catalogs in reverse creation order, then deletes markets in reverse creation order.',
      },
      cases,
      cleanup,
      upstreamCalls: [],
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
      storeDomain,
      apiVersion,
      cases: cases.map((entry) => entry.name),
      cleanup: cleanup.map((entry) => ({ kind: entry.kind, id: entry.id, status: entry.response.status })),
    },
    null,
    2,
  ),
);
