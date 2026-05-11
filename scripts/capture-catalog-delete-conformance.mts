/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CapturedCase<TData = unknown> = {
  name: string;
  query: string;
  request: { variables: JsonRecord };
  response: ConformanceGraphqlResult<TData>;
};

type UserError = {
  __typename?: string | null;
  field?: string[] | null;
  message?: string;
  code?: string | null;
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

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'catalog-delete-parity.json');
const missingCatalogId = 'gid://shopify/MarketCatalog/999999999999';
const unique = Date.now().toString(36);

const marketsReadDocument = `#graphql
query CatalogDeleteMarketsRead($first: Int!) {
  markets(first: $first) {
    nodes {
      id
      name
    }
  }
}
`;

const catalogCreateDocument = `#graphql
mutation CatalogDeleteCatalogCreate($input: CatalogCreateInput!) {
  catalogCreate(input: $input) {
    catalog {
      id
      title
      status
      ... on MarketCatalog {
        markets(first: 5) {
          nodes {
            id
            name
          }
        }
      }
    }
    userErrors {
      __typename
      field
      message
      code
    }
  }
}
`;

const catalogDeleteDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-delete-success.graphql'),
  'utf8',
);
const catalogDeleteUnknownDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-delete-unknown-id-validation.graphql'),
  'utf8',
);
const catalogSetupReadDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-delete-success-setup-read.graphql'),
  'utf8',
);

const companyCreateDocument = `#graphql
mutation CatalogDeleteCompanyCreate($input: CompanyCreateInput!) {
  companyCreate(input: $input) {
    company {
      id
      locations(first: 1) {
        nodes {
          id
        }
      }
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const companyDeleteDocument = `#graphql
mutation CatalogDeleteCompanyDelete($id: ID!) {
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

const companyLocationCatalogCreateDocument = `#graphql
mutation CatalogDeleteCompanyLocationCatalogCreate($input: CatalogCreateInput!) {
  catalogCreate(input: $input) {
    catalog {
      __typename
      id
      title
      status
    }
    userErrors {
      __typename
      field
      message
      code
    }
  }
}
`;

function operationName(query: string): string {
  const match = /\b(?:query|mutation)\s+([_A-Za-z][_0-9A-Za-z]*)/u.exec(query);
  if (!match) throw new Error(`Unable to read operation name from query: ${query}`);
  return match[1];
}

function rootPayload<TData>(result: ConformanceGraphqlResult<TData>, root: string): JsonRecord {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null || Array.isArray(data)) {
    throw new Error(`Missing response data for ${root}: ${JSON.stringify(result.payload)}`);
  }
  const payload = (data as JsonRecord)[root];
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) {
    throw new Error(`Missing ${root} payload: ${JSON.stringify(result.payload)}`);
  }
  return payload as JsonRecord;
}

function userErrors<TData>(result: ConformanceGraphqlResult<TData>, root: string): UserError[] {
  const errors = rootPayload(result, root)['userErrors'];
  return Array.isArray(errors) ? (errors as UserError[]) : [];
}

function assertGraphqlOk<TData>(label: string, result: ConformanceGraphqlResult<TData>): void {
  if (result.status !== 200 || result.payload.errors) {
    throw new Error(`${label} failed: status=${result.status} errors=${JSON.stringify(result.payload.errors ?? null)}`);
  }
}

function assertNoUserErrors<TData>(label: string, result: ConformanceGraphqlResult<TData>, root: string): void {
  const errors = userErrors(result, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertCatalogUserError<TData>(
  label: string,
  result: ConformanceGraphqlResult<TData>,
  root: string,
  expectedCode: string,
  expectedField: string[],
): void {
  const errors = userErrors(result, root);
  const found = errors.some(
    (error) =>
      error.__typename === 'CatalogUserError' &&
      error.code === expectedCode &&
      JSON.stringify(error.field ?? null) === JSON.stringify(expectedField),
  );
  if (!found) {
    throw new Error(`${label} missing expected CatalogUserError: ${JSON.stringify(errors)}`);
  }
}

function readNestedId<TData>(result: ConformanceGraphqlResult<TData>, root: string, field: string): string {
  const node = rootPayload(result, root)[field];
  const id = typeof node === 'object' && node !== null && !Array.isArray(node) ? (node as JsonRecord)['id'] : null;
  if (typeof id !== 'string') {
    throw new Error(`Missing ${root}.${field}.id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function firstMarketId(result: ConformanceGraphqlResult): string {
  const data = result.payload.data as { markets?: { nodes?: Array<{ id?: string }> } } | undefined;
  const id = data?.markets?.nodes?.[0]?.id;
  if (typeof id !== 'string') {
    throw new Error(`catalogDelete capture needs at least one existing market: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

async function captureCase<TData = unknown>(
  name: string,
  query: string,
  variables: JsonRecord,
): Promise<CapturedCase<TData>> {
  const response = await runGraphqlRequest<TData>(query, variables);
  assertGraphqlOk(name, response);
  return { name, query, request: { variables }, response };
}

function upstreamCall(entry: CapturedCase) {
  return {
    operationName: operationName(entry.query),
    variables: entry.request.variables,
    query: entry.query,
    response: {
      status: entry.response.status,
      body: entry.response.payload,
    },
  };
}

const cleanup: Array<{ type: 'catalog'; id: string; response: ConformanceGraphqlResult }> = [];
let createdCatalogId: string | null = null;
let successScenario: { setupRead: CapturedCase; delete: CapturedCase } | null = null;
let notFoundScenario: CapturedCase | null = null;
const driverRejectionAttempts: Array<{
  name: string;
  operations: CapturedCase[];
  cleanup: Array<{ type: 'company'; id: string; response: ConformanceGraphqlResult }>;
  outcome: 'no-rejection';
}> = [];

try {
  notFoundScenario = await captureCase('catalogDeleteUnknownId', catalogDeleteUnknownDocument, {
    id: missingCatalogId,
  });
  assertCatalogUserError('catalogDelete unknown id', notFoundScenario.response, 'catalogDelete', 'CATALOG_NOT_FOUND', [
    'id',
  ]);

  const marketsRead = await captureCase('catalogDeleteMarketsRead', marketsReadDocument, { first: 1 });
  const marketId = firstMarketId(marketsRead.response);
  const create = await captureCase('catalogDeleteCatalogCreate', catalogCreateDocument, {
    input: {
      title: `Catalog Delete ${unique}`,
      status: 'ACTIVE',
      context: {
        marketIds: [marketId],
      },
    },
  });
  assertNoUserErrors('catalogDelete catalogCreate setup', create.response, 'catalogCreate');
  createdCatalogId = readNestedId(create.response, 'catalogCreate', 'catalog');
  const setupRead = await captureCase('catalogDeleteSuccessSetupRead', catalogSetupReadDocument, {
    catalogId: createdCatalogId,
  });
  const deleteCase = await captureCase('catalogDeleteSuccess', catalogDeleteDocument, {
    id: createdCatalogId,
  });
  assertNoUserErrors('catalogDelete success', deleteCase.response, 'catalogDelete');
  const deletedId = rootPayload(deleteCase.response, 'catalogDelete')['deletedId'];
  if (deletedId !== createdCatalogId) {
    throw new Error(`catalogDelete returned unexpected deletedId ${JSON.stringify(deletedId)}`);
  }
  createdCatalogId = null;
  successScenario = { setupRead, delete: deleteCase };

  driverRejectionAttempts.push({
    name: 'marketCatalogCreatedWithExistingMarketDeletesSuccessfully',
    operations: [deleteCase],
    cleanup: [],
    outcome: 'no-rejection',
  });

  let companyId: string | null = null;
  let companyCatalogId: string | null = null;
  const companyCleanup: Array<{ type: 'company'; id: string; response: ConformanceGraphqlResult }> = [];
  const companyOperations: CapturedCase[] = [];
  try {
    const companyCreate = await captureCase('catalogDeleteCompanyCreate', companyCreateDocument, {
      input: {
        company: {
          name: `Catalog Delete ${unique}`,
          externalId: `catalog-delete-${unique}`,
        },
        companyLocation: {
          name: 'Catalog Delete HQ',
          billingAddress: {
            address1: '1 Catalog Delete Way',
            city: 'Ottawa',
            countryCode: 'CA',
          },
        },
      },
    });
    assertNoUserErrors('catalogDelete companyCreate setup', companyCreate.response, 'companyCreate');
    companyOperations.push(companyCreate);
    companyId = readNestedId(companyCreate.response, 'companyCreate', 'company');
    const company = rootPayload(companyCreate.response, 'companyCreate')['company'] as {
      locations?: { nodes?: Array<{ id?: string }> };
    };
    const companyLocationId = company.locations?.nodes?.[0]?.id;
    if (typeof companyLocationId !== 'string') {
      throw new Error(`companyCreate did not return a company location id: ${JSON.stringify(companyCreate.response)}`);
    }

    const companyCatalogCreate = await captureCase(
      'catalogDeleteCompanyLocationCatalogCreate',
      companyLocationCatalogCreateDocument,
      {
        input: {
          title: `Catalog Delete Company ${unique}`,
          status: 'ACTIVE',
          context: {
            companyLocationIds: [companyLocationId],
          },
        },
      },
    );
    assertNoUserErrors(
      'catalogDelete companyLocation catalogCreate setup',
      companyCatalogCreate.response,
      'catalogCreate',
    );
    companyOperations.push(companyCatalogCreate);
    companyCatalogId = readNestedId(companyCatalogCreate.response, 'catalogCreate', 'catalog');

    const companyCatalogDelete = await captureCase('catalogDeleteCompanyLocationCatalogDelete', catalogDeleteDocument, {
      id: companyCatalogId,
    });
    assertNoUserErrors('catalogDelete companyLocation catalog delete', companyCatalogDelete.response, 'catalogDelete');
    companyOperations.push(companyCatalogDelete);
    companyCatalogId = null;
  } finally {
    if (companyCatalogId) {
      const cleanupCatalog = await runGraphqlRequest(catalogDeleteDocument, { id: companyCatalogId });
      companyOperations.push({
        name: 'catalogDeleteCompanyLocationCatalogCleanup',
        query: catalogDeleteDocument,
        request: { variables: { id: companyCatalogId } },
        response: cleanupCatalog,
      });
    }
    if (companyId) {
      companyCleanup.push({
        type: 'company',
        id: companyId,
        response: await runGraphqlRequest(companyDeleteDocument, { id: companyId }),
      });
    }
  }
  driverRejectionAttempts.push({
    name: 'companyLocationCatalogDeletesSuccessfully',
    operations: companyOperations,
    cleanup: companyCleanup,
    outcome: 'no-rejection',
  });
} finally {
  if (createdCatalogId) {
    cleanup.push({
      type: 'catalog',
      id: createdCatalogId,
      response: await runGraphqlRequest(catalogDeleteDocument, { id: createdCatalogId }),
    });
  }
}

if (!notFoundScenario || !successScenario) {
  throw new Error('catalogDelete capture did not complete required scenarios.');
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      scope: 'catalogDelete typed userErrors and payload shape',
      notFound: notFoundScenario,
      success: successScenario,
      driverRejection: {
        status: 'captured-empty',
        todo: 'Capture a driver-owned catalogDelete rejection when the conformance store has a catalog context driver state that refuses deletion.',
        attempts: driverRejectionAttempts,
      },
      cleanup,
      upstreamCalls: [upstreamCall(successScenario.setupRead)],
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
      scenarios: ['notFound', 'success'],
      driverRejection: 'captured-empty',
      cleanup: cleanup.map((entry) => ({ type: entry.type, id: entry.id, status: entry.response.status })),
    },
    null,
    2,
  ),
);
