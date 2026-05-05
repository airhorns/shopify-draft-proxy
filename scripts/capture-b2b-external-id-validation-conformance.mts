import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
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

const scenarioId = 'b2b-external-id-validation';
const timestamp = Date.now();
const longExternalId = 'x'.repeat(65);
const invalidExternalId = 'bad id';
const primaryCompanyExternalId = `har-608-company-${timestamp}`;
const primaryLocationExternalId = `har-608-location-${timestamp}`;
const secondCompanyExternalId = `har-608-company-second-${timestamp}`;
const secondLocationExternalId = `har-608-location-second-${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation B2BExternalIdValidationCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        externalId
        locations(first: 5) {
          nodes {
            id
            name
            externalId
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

const companyUpdateDocument = `#graphql
  mutation B2BExternalIdValidationCompanyUpdate($companyId: ID!, $input: CompanyInput!) {
    companyUpdate(companyId: $companyId, input: $input) {
      company {
        id
        name
        externalId
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
  mutation B2BExternalIdValidationLocationCreate($companyId: ID!, $input: CompanyLocationInput!) {
    companyLocationCreate(companyId: $companyId, input: $input) {
      companyLocation {
        id
        name
        externalId
        company {
          id
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

const locationUpdateDocument = `#graphql
  mutation B2BExternalIdValidationLocationUpdate($companyLocationId: ID!, $input: CompanyLocationUpdateInput!) {
    companyLocationUpdate(companyLocationId: $companyLocationId, input: $input) {
      companyLocation {
        id
        name
        externalId
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
  mutation B2BExternalIdValidationCompanyDelete($id: ID!) {
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
    if (Array.isArray(current)) {
      const index = Number(segment);
      if (!Number.isInteger(index) || index < 0) {
        return undefined;
      }
      current = current[index];
      continue;
    }
    const record = readRecord(current);
    if (!record) {
      return undefined;
    }
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

function readUserErrors(payload: unknown, root: string): unknown[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value : [];
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

function assertUserErrorCode(result: ConformanceGraphqlResult, root: string, code: string, label: string): void {
  assertNoTopLevelErrors(result, label);
  const userErrors = readUserErrors(result.payload, root);
  const matched = userErrors.some((error) => readRecord(error)?.['code'] === code);
  if (!matched) {
    throw new Error(`${label} did not return ${code}: ${JSON.stringify(userErrors, null, 2)}`);
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
  code: string,
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertUserErrorCode(result, root, code, label);
  return recordOperation(query, variables, result);
}

async function runCleanup(companyId: string): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(companyDeleteDocument, { id: companyId });
  return recordOperation(companyDeleteDocument, { id: companyId }, result);
}

const createdCompanyIds = new Set<string>();
const cleanup: Record<string, RecordedOperation> = {};

function rememberCreatedCompany(operation: RecordedOperation, label: string): string {
  const companyId = readStringAtPath(operation.response, ['data', 'companyCreate', 'company', 'id'], label);
  createdCompanyIds.add(companyId);
  return companyId;
}

try {
  const setupCompany = await runRequired(
    companyCreateDocument,
    {
      input: {
        company: {
          name: `HAR-608 externalId primary ${timestamp}`,
          externalId: primaryCompanyExternalId,
        },
        companyLocation: {
          name: `HAR-608 externalId primary location ${timestamp}`,
          externalId: primaryLocationExternalId,
        },
      },
    },
    'companyCreate',
    'companyCreate primary setup',
  );
  const primaryCompanyId = rememberCreatedCompany(setupCompany, 'companyCreate primary setup');
  const primaryLocationId = readStringAtPath(
    setupCompany.response,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
    'companyCreate primary location setup',
  );

  const companyCreateTooLong = await runValidation(
    companyCreateDocument,
    {
      input: {
        company: {
          name: `HAR-608 company too long ${timestamp}`,
          externalId: longExternalId,
        },
      },
    },
    'companyCreate',
    'TOO_LONG',
    'companyCreate externalId too long',
  );

  const companyCreateInvalid = await runValidation(
    companyCreateDocument,
    {
      input: {
        company: {
          name: `HAR-608 company invalid ${timestamp}`,
          externalId: invalidExternalId,
        },
      },
    },
    'companyCreate',
    'INVALID',
    'companyCreate externalId invalid charset',
  );

  const companyCreateDuplicate = await runValidation(
    companyCreateDocument,
    {
      input: {
        company: {
          name: `HAR-608 company duplicate ${timestamp}`,
          externalId: primaryCompanyExternalId,
        },
      },
    },
    'companyCreate',
    'TAKEN',
    'companyCreate duplicate externalId',
  );

  const companyUpdateTooLong = await runValidation(
    companyUpdateDocument,
    {
      companyId: primaryCompanyId,
      input: {
        externalId: longExternalId,
      },
    },
    'companyUpdate',
    'TOO_LONG',
    'companyUpdate externalId too long',
  );

  const companyUpdateInvalid = await runValidation(
    companyUpdateDocument,
    {
      companyId: primaryCompanyId,
      input: {
        externalId: invalidExternalId,
      },
    },
    'companyUpdate',
    'INVALID',
    'companyUpdate externalId invalid charset',
  );

  const secondCompanySetup = await runRequired(
    companyCreateDocument,
    {
      input: {
        company: {
          name: `HAR-608 externalId second ${timestamp}`,
          externalId: secondCompanyExternalId,
        },
      },
    },
    'companyCreate',
    'companyCreate second setup',
  );
  const secondCompanyId = rememberCreatedCompany(secondCompanySetup, 'companyCreate second setup');

  const companyUpdateDuplicate = await runValidation(
    companyUpdateDocument,
    {
      companyId: secondCompanyId,
      input: {
        externalId: primaryCompanyExternalId,
      },
    },
    'companyUpdate',
    'TAKEN',
    'companyUpdate duplicate externalId',
  );

  const locationCreateTooLong = await runValidation(
    locationCreateDocument,
    {
      companyId: primaryCompanyId,
      input: {
        name: `HAR-608 location too long ${timestamp}`,
        externalId: longExternalId,
      },
    },
    'companyLocationCreate',
    'TOO_LONG',
    'companyLocationCreate externalId too long',
  );

  const locationCreateInvalid = await runValidation(
    locationCreateDocument,
    {
      companyId: primaryCompanyId,
      input: {
        name: `HAR-608 location invalid ${timestamp}`,
        externalId: invalidExternalId,
      },
    },
    'companyLocationCreate',
    'INVALID',
    'companyLocationCreate externalId invalid charset',
  );

  const locationCreateDuplicate = await runValidation(
    locationCreateDocument,
    {
      companyId: primaryCompanyId,
      input: {
        name: `HAR-608 location duplicate ${timestamp}`,
        externalId: primaryLocationExternalId,
      },
    },
    'companyLocationCreate',
    'TAKEN',
    'companyLocationCreate duplicate externalId',
  );

  const locationUpdateTooLong = await runValidation(
    locationUpdateDocument,
    {
      companyLocationId: primaryLocationId,
      input: {
        externalId: longExternalId,
      },
    },
    'companyLocationUpdate',
    'TOO_LONG',
    'companyLocationUpdate externalId too long',
  );

  const locationUpdateInvalid = await runValidation(
    locationUpdateDocument,
    {
      companyLocationId: primaryLocationId,
      input: {
        externalId: invalidExternalId,
      },
    },
    'companyLocationUpdate',
    'INVALID',
    'companyLocationUpdate externalId invalid charset',
  );

  const secondLocationSetup = await runRequired(
    locationCreateDocument,
    {
      companyId: primaryCompanyId,
      input: {
        name: `HAR-608 externalId second location ${timestamp}`,
        externalId: secondLocationExternalId,
      },
    },
    'companyLocationCreate',
    'companyLocationCreate second setup',
  );
  const secondLocationId = readStringAtPath(
    secondLocationSetup.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'companyLocationCreate second setup',
  );

  const locationUpdateDuplicate = await runValidation(
    locationUpdateDocument,
    {
      companyLocationId: secondLocationId,
      input: {
        externalId: primaryLocationExternalId,
      },
    },
    'companyLocationUpdate',
    'TAKEN',
    'companyLocationUpdate duplicate externalId',
  );

  for (const companyId of createdCompanyIds) {
    cleanup[`companyDelete:${companyId}`] = await runCleanup(companyId);
  }
  createdCompanyIds.clear();

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      ticket: 'HAR-608',
      plan: 'Record B2B company/company-location externalId charset, length, and uniqueness validation on create and update mutations.',
    },
    constants: {
      longExternalId,
      invalidExternalId,
      primaryCompanyExternalId,
      primaryLocationExternalId,
      secondCompanyExternalId,
      secondLocationExternalId,
    },
    setupCompany,
    companyCreateTooLong,
    companyCreateInvalid,
    companyCreateDuplicate,
    companyUpdateTooLong,
    companyUpdateInvalid,
    secondCompanySetup,
    companyUpdateDuplicate,
    locationCreateTooLong,
    locationCreateInvalid,
    locationCreateDuplicate,
    locationUpdateTooLong,
    locationUpdateInvalid,
    secondLocationSetup,
    locationUpdateDuplicate,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- capture scripts report their output path.
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  for (const companyId of createdCompanyIds) {
    cleanup[`companyDelete:${companyId}`] = await runCleanup(companyId);
  }
}
