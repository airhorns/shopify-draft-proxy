import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

/* oxlint-disable no-console -- CLI capture scripts report output path and best-effort cleanup failures. */

type JsonRecord = Record<string, unknown>;
type RecordedOperation = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: JsonRecord;
};

const scenarioId = 'b2b-no-input-validation';
const timestamp = Date.now();
const companyName = `HAR-759 B2B no input ${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const schemaProbeDocument = `#graphql
  query HAR759B2BNoInputSchema {
    companyInput: __type(name: "CompanyInput") { inputFields { name } }
    contactInput: __type(name: "CompanyContactInput") { inputFields { name } }
    locationUpdateInput: __type(name: "CompanyLocationUpdateInput") { inputFields { name } }
  }
`;

const companyCreateDocument = `#graphql
  mutation HAR759NoInputCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        note
        externalId
        contacts(first: 5) {
          nodes {
            id
            title
            locale
            isMainContact
          }
        }
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

const companyReadDocument = `#graphql
  query HAR759NoInputCompanyRead($companyId: ID!) {
    company(id: $companyId) {
      id
      name
      note
      externalId
      contacts(first: 5) {
        nodes {
          id
          title
          locale
          isMainContact
        }
      }
      locations(first: 5) {
        nodes {
          id
          name
          externalId
        }
      }
    }
  }
`;

const contactCreateDocument = `#graphql
  mutation HAR759NoInputContactCreate($companyId: ID!, $input: CompanyContactInput!) {
    companyContactCreate(companyId: $companyId, input: $input) {
      companyContact {
        id
        title
        locale
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

const companyUpdateDocument = `#graphql
  mutation HAR759NoInputCompanyUpdate($companyId: ID!, $input: CompanyInput!) {
    companyUpdate(companyId: $companyId, input: $input) {
      company {
        id
        name
        note
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

const contactUpdateDocument = `#graphql
  mutation HAR759NoInputContactUpdate($companyContactId: ID!, $input: CompanyContactInput!) {
    companyContactUpdate(companyContactId: $companyContactId, input: $input) {
      companyContact {
        id
        title
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
  mutation HAR759NoInputLocationUpdate($companyLocationId: ID!, $input: CompanyLocationUpdateInput!) {
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
  mutation HAR759NoInputCompanyDelete($id: ID!) {
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
      if (!Number.isInteger(index)) return undefined;
      current = current[index];
      continue;
    }
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

function assertMutationReturnedUserErrors(result: ConformanceGraphqlResult, root: string, label: string): void {
  assertNoTopLevelErrors(result, label);
  const userErrors = readUserErrors(result.payload, root);
  if (userErrors.length < 1) {
    throw new Error(`${label} returned no userErrors: ${JSON.stringify(result.payload, null, 2)}`);
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

async function runRead(query: string, variables: JsonRecord, label: string): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertNoTopLevelErrors(result, label);
  return recordOperation(query, variables, result);
}

async function runValidationMutation(
  query: string,
  variables: JsonRecord,
  root: string,
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertMutationReturnedUserErrors(result, root, label);
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
  assertNoTopLevelErrors(schemaProbeResult, 'HAR-759 B2B schema probe');
  const schemaProbe = recordOperation(schemaProbeDocument, {}, schemaProbeResult);

  const setupCompany = await runRequired(
    companyCreateDocument,
    {
      input: {
        company: {
          name: companyName,
          note: 'unchanged note',
          externalId: `har-759-${timestamp}`,
        },
        companyContact: {
          firstName: 'Har',
          lastName: 'No Input',
          email: `har-759-no-input-${timestamp}@example.com`,
          title: 'Unchanged buyer',
        },
        companyLocation: {
          name: `${companyName} HQ`,
          externalId: `har-759-location-${timestamp}`,
          billingAddress: {
            address1: '759 B2B Way',
            city: 'Ottawa',
            countryCode: 'CA',
          },
        },
      },
    },
    'companyCreate',
    'HAR-759 companyCreate setup',
  );

  companyId = readStringAtPath(setupCompany.response, ['data', 'companyCreate', 'company', 'id'], 'setup company');
  const contactId = readStringAtPath(
    setupCompany.response,
    ['data', 'companyCreate', 'company', 'contacts', 'nodes', '0', 'id'],
    'setup contact',
  );
  const locationId = readStringAtPath(
    setupCompany.response,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
    'setup location',
  );

  const baselineRead = await runRead(companyReadDocument, { companyId }, 'HAR-759 baseline read');

  const contactCreateEmptyInput = await runValidationMutation(
    contactCreateDocument,
    { companyId, input: {} },
    'companyContactCreate',
    'HAR-759 contact create empty input',
  );
  const contactCreateNullOnlyInput = await runValidationMutation(
    contactCreateDocument,
    {
      companyId,
      input: {
        firstName: null,
        lastName: null,
        email: null,
        title: null,
        locale: null,
        phone: null,
      },
    },
    'companyContactCreate',
    'HAR-759 contact create null-only input',
  );
  const companyUpdateEmptyInput = await runValidationMutation(
    companyUpdateDocument,
    { companyId, input: {} },
    'companyUpdate',
    'HAR-759 company update empty input',
  );
  const companyUpdateNullOnlyInput = await runValidationMutation(
    companyUpdateDocument,
    {
      companyId,
      input: {
        name: null,
        note: null,
        externalId: null,
        customerSince: null,
      },
    },
    'companyUpdate',
    'HAR-759 company update null-only input',
  );
  const contactUpdateEmptyInput = await runValidationMutation(
    contactUpdateDocument,
    { companyContactId: contactId, input: {} },
    'companyContactUpdate',
    'HAR-759 contact update empty input',
  );
  const contactUpdateNullOnlyInput = await runValidationMutation(
    contactUpdateDocument,
    {
      companyContactId: contactId,
      input: {
        firstName: null,
        lastName: null,
        email: null,
        title: null,
        locale: null,
        phone: null,
      },
    },
    'companyContactUpdate',
    'HAR-759 contact update null-only input',
  );
  const locationUpdateEmptyInput = await runValidationMutation(
    locationUpdateDocument,
    { companyLocationId: locationId, input: {} },
    'companyLocationUpdate',
    'HAR-759 location update empty input',
  );
  const locationUpdateNullOnlyInput = await runValidationMutation(
    locationUpdateDocument,
    {
      companyLocationId: locationId,
      input: {
        name: null,
        phone: null,
        locale: null,
        externalId: null,
        note: null,
      },
    },
    'companyLocationUpdate',
    'HAR-759 location update null-only input',
  );

  const readAfterNoInput = await runRead(companyReadDocument, { companyId }, 'HAR-759 read after NO_INPUT probes');

  cleanup[`companyDelete:${companyId}`] = await runCleanup(companyId);
  companyDeleted = true;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      ticket: 'HAR-759',
      plan: 'Create a disposable B2B company with contact/location, record Shopify NO_INPUT validation for empty and null-only B2B update/contact-create inputs, verify readback remains unchanged, and clean up the company.',
    },
    schemaProbe,
    setupCompany,
    baselineRead,
    contactCreateEmptyInput,
    contactCreateNullOnlyInput,
    companyUpdateEmptyInput,
    companyUpdateNullOnlyInput,
    contactUpdateEmptyInput,
    contactUpdateNullOnlyInput,
    locationUpdateEmptyInput,
    locationUpdateNullOnlyInput,
    readAfterNoInput,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (companyId && !companyDeleted) {
    try {
      cleanup[`companyDelete:${companyId}`] = await runCleanup(companyId);
    } catch (error) {
      console.error(`Cleanup failed for ${companyId}:`, error);
    }
  }
}
