import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

/* oxlint-disable no-console -- CLI capture scripts report output path and cleanup status. */

type JsonRecord = Record<string, unknown>;
type RecordedOperation = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: JsonRecord;
};

const scenarioId = 'b2b-location-input-normalization';
const timestamp = Date.now();
const companyName = `B2B location input ${timestamp}`;
const nestedPhoneLocal = '(415) 555-1234';
const createPhoneLocal = '415.555.5678';
const updatePhoneLocal = '(415) 555-9999';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const schemaProbeDocument = `#graphql
  query B2BLocationInputNormalizationSchema {
    locationType: __type(name: "CompanyLocation") { fields { name } }
    locationInputType: __type(name: "CompanyLocationInput") { inputFields { name } }
    locationUpdateInputType: __type(name: "CompanyLocationUpdateInput") { inputFields { name } }
    shop { billingAddress { countryCodeV2 } }
    shopLocales { locale primary published }
  }
`;

const companyCreateDocument = `#graphql
  mutation B2BLocationInputNormalizationCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        locations(first: 5) {
          nodes {
            id
            name
            phone
            locale
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

const locationCreateDocument = `#graphql
  mutation B2BLocationInputNormalizationLocationCreate($companyId: ID!, $input: CompanyLocationInput!) {
    companyLocationCreate(companyId: $companyId, input: $input) {
      companyLocation {
        id
        name
        phone
        locale
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const locationUpdateDocument = `#graphql
  mutation B2BLocationInputNormalizationLocationUpdate($companyLocationId: ID!, $input: CompanyLocationUpdateInput!) {
    companyLocationUpdate(companyLocationId: $companyLocationId, input: $input) {
      companyLocation {
        id
        name
        phone
        locale
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
  mutation B2BLocationInputNormalizationCompanyDelete($id: ID!) {
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

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    const record = readRecord(current);
    if (!record) return undefined;
    current = record[segment];
  }
  return current;
}

function readStringAtPath(value: unknown, pathSegments: string[], label: string): string {
  const pathValue = readPath(value, pathSegments);
  if (typeof pathValue !== 'string' || pathValue.length === 0) {
    throw new Error(`${label} did not return a string at ${pathSegments.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return pathValue;
}

function readUserErrors(payload: unknown, root: string): JsonRecord[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value.filter((item): item is JsonRecord => readRecord(item) !== null) : [];
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertSuccessful(result: ConformanceGraphqlResult, root: string, label: string): void {
  assertNoTopLevelErrors(result, label);
  const userErrors = readUserErrors(result.payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertUserErrorCodes(result: ConformanceGraphqlResult, root: string, codes: string[], label: string): void {
  assertNoTopLevelErrors(result, label);
  const actualCodes = readUserErrors(result.payload, root).map((error) => error['code']);
  for (const code of codes) {
    if (!actualCodes.includes(code)) {
      throw new Error(`${label} did not include ${code}: ${JSON.stringify(result.payload, null, 2)}`);
    }
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

async function runRequired(
  query: string,
  variables: JsonRecord,
  root: string,
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertSuccessful(result, root, label);
  return recordOperation(query, variables, result);
}

async function runValidation(
  query: string,
  variables: JsonRecord,
  root: string,
  expectedCodes: string[],
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertUserErrorCodes(result, root, expectedCodes, label);
  return recordOperation(query, variables, result);
}

async function runCleanup(companyId: string): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(companyDeleteDocument, { id: companyId });
  return recordOperation(companyDeleteDocument, { id: companyId }, result);
}

let companyId: string | null = null;
let companyDeleted = false;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const schemaProbeResult = await runGraphqlRequest(schemaProbeDocument, {});
  assertNoTopLevelErrors(schemaProbeResult, 'B2B location input schema probe');
  const schemaProbe = recordOperation(schemaProbeDocument, {}, schemaProbeResult);

  const companyCreate = await runRequired(
    companyCreateDocument,
    {
      input: {
        company: { name: `${companyName} nested success` },
        companyLocation: {
          name: `${companyName} HQ`,
          phone: nestedPhoneLocal,
        },
      },
    },
    'companyCreate',
    'nested companyLocation phone normalization/default locale',
  );
  companyId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'id'],
    'nested companyLocation phone normalization/default locale',
  );

  const invalidNestedPhone = await runValidation(
    companyCreateDocument,
    {
      input: {
        company: { name: `${companyName} nested invalid phone` },
        companyLocation: {
          name: `${companyName} invalid phone HQ`,
          phone: 'not-a-phone',
        },
      },
    },
    'companyCreate',
    ['INVALID'],
    'nested companyLocation invalid phone',
  );

  const nestedMalformedLocale = await runRequired(
    companyCreateDocument,
    {
      input: {
        company: { name: `${companyName} nested malformed locale` },
        companyLocation: {
          name: `${companyName} malformed locale HQ`,
          locale: 'not_a_locale',
        },
      },
    },
    'companyCreate',
    'nested companyLocation malformed locale passthrough',
  );
  const nestedMalformedLocaleCompanyId = readStringAtPath(
    nestedMalformedLocale.response,
    ['data', 'companyCreate', 'company', 'id'],
    'nested companyLocation malformed locale passthrough',
  );
  cleanup['nestedMalformedLocaleCompanyDelete'] = await runCleanup(nestedMalformedLocaleCompanyId);

  const locationCreate = await runRequired(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${companyName} branch`,
        phone: createPhoneLocal,
      },
    },
    'companyLocationCreate',
    'companyLocationCreate phone normalization/default locale',
  );

  const invalidLocationCreatePhone = await runValidation(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${companyName} invalid phone branch`,
        phone: 'not-a-phone',
      },
    },
    'companyLocationCreate',
    ['INVALID'],
    'companyLocationCreate invalid phone',
  );

  const locationCreateMalformedLocale = await runRequired(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${companyName} malformed locale branch`,
        locale: 'not_a_locale',
      },
    },
    'companyLocationCreate',
    'companyLocationCreate malformed locale passthrough',
  );

  const locationCreateForUpdate = await runRequired(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${companyName} update branch`,
        phone: '+14155550000',
        locale: 'fr-CA',
      },
    },
    'companyLocationCreate',
    'companyLocationCreate explicit locale setup for update',
  );
  const updateLocationId = readStringAtPath(
    locationCreateForUpdate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'companyLocationCreate explicit locale setup for update',
  );

  const locationUpdate = await runRequired(
    locationUpdateDocument,
    {
      companyLocationId: updateLocationId,
      input: {
        phone: updatePhoneLocal,
      },
    },
    'companyLocationUpdate',
    'companyLocationUpdate phone normalization/default locale',
  );

  const invalidLocationUpdatePhone = await runValidation(
    locationUpdateDocument,
    {
      companyLocationId: updateLocationId,
      input: {
        phone: 'not-a-phone',
      },
    },
    'companyLocationUpdate',
    ['INVALID'],
    'companyLocationUpdate invalid phone',
  );

  const locationUpdateMalformedLocale = await runRequired(
    locationUpdateDocument,
    {
      companyLocationId: updateLocationId,
      input: {
        locale: 'not_a_locale',
      },
    },
    'companyLocationUpdate',
    'companyLocationUpdate malformed locale passthrough',
  );

  cleanup['companyDelete'] = await runCleanup(companyId);
  companyDeleted = true;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      plan: 'Create disposable B2B company/location records and record companyLocation phone E.164 normalization, primary-locale defaulting, invalid phone validation, malformed locale passthrough, and cleanup.',
    },
    schemaProbe,
    companyCreate,
    invalidNestedPhone,
    nestedMalformedLocale,
    locationCreate,
    invalidLocationCreatePhone,
    locationCreateMalformedLocale,
    locationCreateForUpdate,
    locationUpdate,
    invalidLocationUpdatePhone,
    locationUpdateMalformedLocale,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (companyId && !companyDeleted) {
    cleanup['companyDelete'] = await runCleanup(companyId);
  }
}
