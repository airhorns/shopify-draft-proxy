/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type RecordedOperation = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: JsonRecord;
};

const scenarioId = 'b2b-cold-company-location-hydration';
const timestamp = Date.now();
const searchToken = `b2bcoldhydrate${timestamp}`;
const companyName = `B2B ${searchToken}`;
const originalLocationName = `${companyName} Warehouse`;
const updatedLocationName = `${companyName} Updated Warehouse`;
const taxRegistrationId = `B2B-COLD-${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const fixturePath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);

const companyDeleteDocument = `#graphql
  mutation B2BColdCompanyLocationHydrationCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors { field message code }
    }
  }
`;

const companyLocationHydrateDocument = `
query B2BCompanyLocationHydrate($id: ID!) {
  companyLocation(id: $id) {
    id
    name
    externalId
    note
    locale
    phone
    billingAddress { id address1 }
    shippingAddress { id address1 }
    taxSettings {
      taxRegistrationId
      taxExempt
      taxExemptions
    }
    buyerExperienceConfiguration {
      editableShippingAddress
      checkoutToDraft
      paymentTermsTemplate { id }
      deposit { __typename }
    }
    company {
      id
      name
      locations(first: 50) { nodes { id } }
    }
  }
}
`;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readPath(value: unknown, pathSegments: Array<string | number>): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current) && typeof segment === 'number') {
      current = current[segment];
      continue;
    }
    if (isRecord(current) && typeof segment === 'string') {
      current = current[segment];
      continue;
    }
    return undefined;
  }
  return current;
}

function readStringAtPath(value: unknown, pathSegments: Array<string | number>, label: string): string {
  const pathValue = readPath(value, pathSegments);
  if (typeof pathValue !== 'string' || pathValue.length === 0) {
    throw new Error(`${label} did not return a string at ${pathSegments.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return pathValue;
}

function readUserErrors(payload: unknown, root: string): unknown[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value : [];
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} returned GraphQL errors: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(result: ConformanceGraphqlResult, root: string, label: string): void {
  assertGraphqlOk(result, label);
  const userErrors = readUserErrors(result.payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function recordOperation(query: string, variables: JsonRecord, result: ConformanceGraphqlResult): RecordedOperation {
  return {
    request: { query, variables },
    response: {
      status: result.status,
      ...result.payload,
    },
  };
}

async function readDocument(documentPath: string): Promise<string> {
  return await readFile(documentPath, 'utf8');
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runRequired(
  query: string,
  variables: JsonRecord,
  root: string,
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertNoUserErrors(result, root, label);
  return recordOperation(query, variables, result);
}

async function runGraphql(query: string, variables: JsonRecord, label: string): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertGraphqlOk(result, label);
  return recordOperation(query, variables, result);
}

async function runCleanup(
  query: string,
  variables: JsonRecord,
  root: string,
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status >= 200 && result.status < 300 && !result.payload.errors) {
    return recordOperation(query, variables, result);
  }
  return recordOperation(query, variables, {
    status: result.status,
    payload: {
      ...result.payload,
      cleanupWarning: `${label} cleanup failed before userErrors could be inspected.`,
      cleanupRoot: root,
    },
  });
}

const companyCreateDocument = await readDocument(
  'config/parity-requests/b2b/b2b-contact-location-assignments-tax-create.graphql',
);
const coldCompaniesReadDocument = await readDocument('config/parity-requests/b2b/b2b-cold-companies-read.graphql');
const locationUpdateDocument = await readDocument(
  'config/parity-requests/b2b/b2b-cold-company-location-update.graphql',
);
const taxUpdateDocument = await readDocument(
  'config/parity-requests/b2b/b2b-contact-location-assignments-tax-tax.graphql',
);

let companyId: string | null = null;
let companyDeleted = false;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const companyCreateVariables = {
    input: {
      company: {
        name: companyName,
        note: 'B2B cold company-location hydration parity',
        externalId: `b2b-cold-hydration-${timestamp}`,
      },
      companyContact: {
        firstName: 'Cold',
        lastName: 'Hydration',
        email: `b2b-cold-hydration-${timestamp}@example.com`,
        title: 'Buyer',
      },
      companyLocation: {
        name: originalLocationName,
        phone: '+14165550124',
        billingAddress: {
          address1: '1 Cold Hydrate Way',
          city: 'Toronto',
          countryCode: 'CA',
        },
      },
    },
  };
  const companyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables,
    'companyCreate',
    'companyCreate setup',
  );
  companyId = readStringAtPath(companyCreate.response, ['data', 'companyCreate', 'company', 'id'], 'companyCreate');
  const companyLocationId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', 0, 'id'],
    'companyCreate main location',
  );

  const coldCompaniesVariables = { query: `name:${searchToken}` };
  let coldCompaniesRead: RecordedOperation | null = null;
  for (let attempt = 1; attempt <= 12; attempt += 1) {
    const candidate = await runGraphql(
      coldCompaniesReadDocument,
      coldCompaniesVariables,
      `cold companies read attempt ${attempt}`,
    );
    if (readPath(candidate.response, ['data', 'companies', 'nodes', 0, 'id']) === companyId) {
      coldCompaniesRead = candidate;
      break;
    }
    await sleep(5_000);
  }
  if (!coldCompaniesRead) {
    throw new Error('cold companies read did not expose the created company before timeout');
  }

  const hydrateBeforeLocationUpdate = await runGraphql(
    companyLocationHydrateDocument,
    { id: companyLocationId },
    'company location hydrate before update',
  );
  if (readPath(hydrateBeforeLocationUpdate.response, ['data', 'companyLocation', 'id']) !== companyLocationId) {
    throw new Error(
      `company location hydrate returned the wrong location: ${JSON.stringify(
        hydrateBeforeLocationUpdate.response,
        null,
        2,
      )}`,
    );
  }

  const locationUpdate = await runRequired(
    locationUpdateDocument,
    {
      companyLocationId,
      input: {
        name: updatedLocationName,
      },
    },
    'companyLocationUpdate',
    'companyLocationUpdate cold hydrate-backed mutation',
  );

  const taxUpdate = await runRequired(
    taxUpdateDocument,
    {
      companyLocationId,
      taxRegistrationId,
    },
    'companyLocationTaxSettingsUpdate',
    'companyLocationTaxSettingsUpdate cold hydrate-backed mutation',
  );

  const companyDelete = await runRequired(companyDeleteDocument, { id: companyId }, 'companyDelete', 'company cleanup');
  cleanup['companyDelete'] = companyDelete;
  companyDeleted = true;

  await mkdir(path.dirname(fixturePath), { recursive: true });
  await writeFile(
    fixturePath,
    `${JSON.stringify(
      {
        scenarioId,
        capturedAt: new Date().toISOString(),
        source: 'live-shopify-admin-graphql',
        storeDomain,
        apiVersion,
        intent: {
          plan: 'Create a disposable B2B company/location, capture a cold companies read, capture a pre-mutation companyLocation hydrate, run live companyLocationUpdate and companyLocationTaxSettingsUpdate against that real location id, then delete the company.',
        },
        companyCreate,
        coldCompaniesRead,
        hydrateBeforeLocationUpdate,
        locationUpdate,
        taxUpdate,
        cleanup,
        upstreamCalls: [
          {
            operationName: 'B2BColdCompaniesRead',
            variables: coldCompaniesRead.request.variables,
            query: coldCompaniesRead.request.query,
            response: {
              status: coldCompaniesRead.response.status,
              body: {
                data: coldCompaniesRead.response.data,
                extensions: coldCompaniesRead.response.extensions,
              },
            },
          },
          {
            operationName: 'B2BCompanyLocationHydrate',
            variables: hydrateBeforeLocationUpdate.request.variables,
            query: hydrateBeforeLocationUpdate.request.query,
            response: {
              status: hydrateBeforeLocationUpdate.response.status,
              body: {
                data: hydrateBeforeLocationUpdate.response.data,
                extensions: hydrateBeforeLocationUpdate.response.extensions,
              },
            },
          },
        ],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(JSON.stringify({ ok: true, scenarioId, fixturePath }, null, 2));
} finally {
  if (companyId && !companyDeleted) {
    cleanup['companyDelete'] = await runCleanup(
      companyDeleteDocument,
      { id: companyId },
      'companyDelete',
      'company cleanup',
    );
  }
}
