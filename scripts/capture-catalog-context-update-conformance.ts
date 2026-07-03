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
  kind: 'catalog' | 'market' | 'company';
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
const companyCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-company-create.graphql'),
  'utf8',
);
const companyLocationCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-company-location-create.graphql'),
  'utf8',
);
const companyCatalogCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-company-catalog-create.graphql'),
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
const companyLocationUpdateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-company-location-add-remove.graphql'),
  'utf8',
);
const companyLocationReadDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-company-read.graphql'),
  'utf8',
);
const driverMismatchDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-driver-mismatch.graphql'),
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

const companyDeleteDocument = `#graphql
mutation CatalogContextUpdateCompanyCleanup($id: ID!) {
  companyDelete(id: $id) {
    deletedCompanyId
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
const createdCompanyIds: string[] = [];
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
    (error): error is Record<string, unknown> => typeof error === 'object' && error !== null && !Array.isArray(error),
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

function stringAtPath(value: unknown, pathParts: Array<string | number>, label: string): string {
  let current = value;
  for (const pathPart of pathParts) {
    if (typeof pathPart === 'number') {
      if (!Array.isArray(current)) {
        throw new Error(`${label} expected array before ${pathPart}: ${JSON.stringify(value)}`);
      }
      current = current[pathPart];
    } else {
      if (typeof current !== 'object' || current === null || Array.isArray(current)) {
        throw new Error(`${label} expected object before ${pathPart}: ${JSON.stringify(value)}`);
      }
      current = (current as Record<string, unknown>)[pathPart];
    }
  }
  if (typeof current !== 'string' || current.length === 0) {
    throw new Error(`${label} missing string at ${pathParts.join('.')}: ${JSON.stringify(value)}`);
  }
  return current;
}

async function captureCase(name: string, query: string, variables: Record<string, unknown>): Promise<CapturedCase> {
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

async function createCompany(label: string): Promise<{ companyId: string; locationId: string }> {
  const capture = await captureCase(`${label}CompanyCreate`, companyCreateDocument, {
    input: {
      company: {
        name: `Catalog Context ${label} Company ${suffix}`,
        externalId: `catalog-context-${label.toLowerCase().replace(/\s+/gu, '-')}-${suffix}`,
      },
      companyLocation: {
        name: `Catalog Context ${label} HQ`,
        billingAddress: {
          address1: '1 Catalog Context Way',
          city: 'Ottawa',
          countryCode: 'CA',
        },
      },
    },
  });
  assertNoUserErrors(label, capture.response, 'companyCreate');
  const companyId = nestedId(capture.response, 'companyCreate', 'company');
  const locationId = stringAtPath(
    capture.response.payload,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', 0, 'id'],
    `${label} companyCreate location`,
  );
  createdCompanyIds.push(companyId);
  return { companyId, locationId };
}

async function createCompanyLocation(label: string, companyId: string): Promise<string> {
  const capture = await captureCase(`${label}CompanyLocationCreate`, companyLocationCreateDocument, {
    companyId,
    input: {
      name: `Catalog Context ${label} Branch`,
      phone: '+16135550111',
    },
  });
  assertNoUserErrors(label, capture.response, 'companyLocationCreate');
  return nestedId(capture.response, 'companyLocationCreate', 'companyLocation');
}

async function createCompanyCatalog(label: string, companyLocationIds: string[]): Promise<string> {
  const capture = await captureCase(`${label}CompanyCatalogCreate`, companyCatalogCreateDocument, {
    input: {
      title: `Catalog Context ${label} Company Catalog ${suffix}`,
      status: 'ACTIVE',
      context: {
        companyLocationIds,
      },
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

async function cleanupCompany(id: string): Promise<void> {
  const response = await runGraphqlRequest(companyDeleteDocument, { id });
  cleanup.push({ kind: 'company', id, response });
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

  const companyContext = await createCompany('Company Context');
  const secondCompanyLocationId = await createCompanyLocation('Company Context', companyContext.companyId);
  const companyCatalogId = await createCompanyCatalog('Company Context', [companyContext.locationId]);
  const companyLocationAddRemove = await captureCase(
    'catalogContextUpdateCompanyLocationAddRemove',
    companyLocationUpdateDocument,
    {
      catalogId: companyCatalogId,
      contextsToAdd: {
        companyLocationIds: [secondCompanyLocationId],
      },
      contextsToRemove: {
        companyLocationIds: [companyContext.locationId],
      },
    },
  );
  assertNoUserErrors(
    'catalogContextUpdate company location add/remove',
    companyLocationAddRemove.response,
    'catalogContextUpdate',
  );
  await captureCase('catalogReadAfterCompanyLocationUpdate', companyLocationReadDocument, { id: companyCatalogId });

  const companyLocationNotFound = await captureCase(
    'catalogContextUpdateCompanyLocationNotFound',
    companyLocationUpdateDocument,
    {
      catalogId: companyCatalogId,
      contextsToAdd: {
        companyLocationIds: ['gid://shopify/CompanyLocation/999999999999'],
      },
      contextsToRemove: {
        companyLocationIds: ['gid://shopify/CompanyLocation/999999999998'],
      },
    },
  );
  assertUserError(
    'catalogContextUpdate company location add not found',
    companyLocationNotFound.response,
    'catalogContextUpdate',
    'COMPANY_LOCATION_NOT_FOUND',
    ['contextsToAdd', 'companyLocationIds', '0'],
  );
  assertUserError(
    'catalogContextUpdate company location remove not found',
    companyLocationNotFound.response,
    'catalogContextUpdate',
    'COMPANY_LOCATION_NOT_FOUND',
    ['contextsToRemove', 'companyLocationIds', '0'],
  );

  const mismatchMarketId = await createMarket('Driver Mismatch');
  const mismatchCatalogId = await createCatalog('Driver Mismatch', [mismatchMarketId]);
  const driverMismatch = await captureCase('catalogContextUpdateDriverMismatch', driverMismatchDocument, {
    catalogId: mismatchCatalogId,
    contextsToAdd: {
      companyLocationIds: [companyContext.locationId],
    },
  });
  assertUserError(
    'catalogContextUpdate driver mismatch',
    driverMismatch.response,
    'catalogContextUpdate',
    'CONTEXT_DRIVER_MISMATCH',
    ['contextsToAdd', 'companyLocationIds'],
  );
} catch (error) {
  captureFailure = error;
} finally {
  for (const id of createdCatalogIds.slice().reverse()) {
    await cleanupCatalog(id);
  }
  for (const id of createdMarketIds.slice().reverse()) {
    await cleanupMarket(id);
  }
  for (const id of createdCompanyIds.slice().reverse()) {
    await cleanupCompany(id);
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
        'catalogContextUpdate required-context validation, remove-only update, duplicate market add behavior, catalog-not-found typing, company-location context updates, driver mismatch validation, and catalogsCount after catalog writes',
      setup: {
        suffix,
        createdMarketIds,
        createdCatalogIds,
        createdCompanyIds,
        cleanup:
          'The capture creates disposable markets, B2B companies, MarketCatalogs, and CompanyLocationCatalogs, records catalogContextUpdate branches, deletes catalogs in reverse creation order, deletes markets in reverse creation order, then deletes companies in reverse creation order.',
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
