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

const scenarioId = 'b2b-contact-input-normalization';
const timestamp = Date.now();
const companyName = `HAR-614 B2B input ${timestamp}`;
const baseEmail = `har-614-primary-${timestamp}@example.com`;
const secondEmail = `har-614-second-${timestamp}@example.com`;
const primaryPhoneLine = String(timestamp % 10_000_000).padStart(7, '0');
const secondPhoneLine = String((timestamp + 1212) % 10_000_000).padStart(7, '0');
const primaryPhoneLocal = `(415) ${primaryPhoneLine.slice(0, 3)}-${primaryPhoneLine.slice(3)}`;
const primaryPhoneE164 = `+1415${primaryPhoneLine}`;
const secondPhoneLocal = `(650) ${secondPhoneLine.slice(0, 3)}-${secondPhoneLine.slice(3)}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const schemaProbeDocument = `#graphql
  query HAR614B2BContactInputSchema {
    contactType: __type(name: "CompanyContact") { fields { name } }
    inputType: __type(name: "CompanyContactInput") { inputFields { name } }
    shop { billingAddress { countryCodeV2 } }
    shopLocales { locale primary published }
  }
`;

const companyCreateDocument = `#graphql
  mutation HAR614CompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        contacts(first: 5) {
          nodes {
            id
            locale
            title
            isMainContact
          }
        }
      }
      userErrors { field message code }
    }
  }
`;

const contactCreateDocument = `#graphql
  mutation HAR614ContactCreate($companyId: ID!, $input: CompanyContactInput!) {
    companyContactCreate(companyId: $companyId, input: $input) {
      companyContact {
        id
        locale
        title
        company { id }
      }
      userErrors { field message code }
    }
  }
`;

const contactUpdateDocument = `#graphql
  mutation HAR614ContactUpdate($companyContactId: ID!, $input: CompanyContactInput!) {
    companyContactUpdate(companyContactId: $companyContactId, input: $input) {
      companyContact {
        id
        locale
        title
      }
      userErrors { field message code }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation HAR614CompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors { field message code }
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
    request: {
      query,
      variables,
    },
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

let companyId: string | null = null;
let companyDeleted = false;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const schemaProbeResult = await runGraphqlRequest(schemaProbeDocument, {});
  assertNoTopLevelErrors(schemaProbeResult, 'HAR-614 B2B schema probe');
  const schemaProbe = recordOperation(schemaProbeDocument, {}, schemaProbeResult);

  const companyCreateVariables = {
    input: {
      company: {
        name: companyName,
      },
      companyContact: {
        firstName: 'Har',
        lastName: 'Primary',
        email: baseEmail,
        title: 'Primary buyer',
        phone: primaryPhoneLocal,
      },
      companyLocation: {
        name: `${companyName} HQ`,
        billingAddress: {
          address1: '614 B2B Way',
          city: 'Ottawa',
          countryCode: 'CA',
        },
      },
    },
  };
  const companyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables,
    'companyCreate',
    'HAR-614 companyCreate primary contact setup',
  );
  companyId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'id'],
    'HAR-614 companyCreate primary contact setup',
  );

  const duplicatePhone = await runValidation(
    contactCreateDocument,
    {
      companyId,
      input: {
        firstName: 'Har',
        lastName: 'Duplicate phone',
        email: `har-614-duplicate-phone-${timestamp}@example.com`,
        title: 'Duplicate phone',
        phone: primaryPhoneE164,
      },
    },
    'companyContactCreate',
    ['TAKEN'],
    'HAR-614 duplicate normalized phone create',
  );

  const duplicateEmail = await runValidation(
    contactCreateDocument,
    {
      companyId,
      input: {
        firstName: 'Har',
        lastName: 'Duplicate email',
        email: baseEmail.toUpperCase(),
        title: 'Duplicate email',
        phone: '+14155550000',
      },
    },
    'companyContactCreate',
    ['TAKEN'],
    'HAR-614 duplicate email create',
  );

  const invalidPhone = await runValidation(
    contactCreateDocument,
    {
      companyId,
      input: {
        firstName: 'Har',
        lastName: 'Invalid phone',
        email: `har-614-invalid-phone-${timestamp}@example.com`,
        title: 'Invalid phone',
        phone: 'not-a-phone',
      },
    },
    'companyContactCreate',
    ['INVALID'],
    'HAR-614 invalid phone create',
  );

  const invalidLocale = await runValidation(
    contactCreateDocument,
    {
      companyId,
      input: {
        firstName: 'Har',
        lastName: 'Invalid locale',
        email: `har-614-invalid-locale-${timestamp}@example.com`,
        title: 'Invalid locale',
        locale: 'not_a_locale',
      },
    },
    'companyContactCreate',
    ['INVALID'],
    'HAR-614 invalid locale create',
  );

  const secondContact = await runRequired(
    contactCreateDocument,
    {
      companyId,
      input: {
        firstName: 'Har',
        lastName: 'Second',
        email: secondEmail,
        title: 'Second buyer',
        phone: secondPhoneLocal,
      },
    },
    'companyContactCreate',
    'HAR-614 second contact create',
  );
  const secondContactId = readStringAtPath(
    secondContact.response,
    ['data', 'companyContactCreate', 'companyContact', 'id'],
    'HAR-614 second contact create',
  );

  const duplicateEmailUpdate = await runValidation(
    contactUpdateDocument,
    {
      companyContactId: secondContactId,
      input: {
        email: baseEmail,
      },
    },
    'companyContactUpdate',
    ['TAKEN'],
    'HAR-614 duplicate email update',
  );

  const duplicatePhoneUpdate = await runValidation(
    contactUpdateDocument,
    {
      companyContactId: secondContactId,
      input: {
        phone: primaryPhoneLocal,
      },
    },
    'companyContactUpdate',
    ['TAKEN'],
    'HAR-614 duplicate phone update',
  );

  cleanup['companyDelete'] = await runCleanup(
    companyDeleteDocument,
    { id: companyId },
    'companyDelete',
    'HAR-614 company cleanup',
  );
  companyDeleted = true;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      ticket: 'HAR-614',
      plan: 'Create a disposable B2B company and contacts; record phone normalization through duplicate-phone validation, locale defaulting, invalid phone/locale validation, duplicate email validation, duplicate update validation, and cleanup.',
    },
    schemaProbe,
    companyCreate,
    duplicatePhone,
    duplicateEmail,
    invalidPhone,
    invalidLocale,
    secondContact,
    duplicateEmailUpdate,
    duplicatePhoneUpdate,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- capture scripts report their output path.
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (companyId && !companyDeleted) {
    cleanup['companyDelete'] = await runCleanup(
      companyDeleteDocument,
      { id: companyId },
      'companyDelete',
      'HAR-614 company finally cleanup',
    );
  }
}
