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

const scenarioId = 'b2b-address-validation';
const timestamp = Date.now();
const runLabel = `b2b-address-validation-${timestamp}`;
const waveEmoji = String.fromCodePoint(0x1f44b);

const requestDir = path.join('config', 'parity-requests', 'b2b');

const companyCreateDocument = await readRequestDocument('b2b-address-validation-company-create.graphql');
const locationCreateDocument = await readRequestDocument('b2b-address-validation-location-create.graphql');
const assignAddressDocument = await readRequestDocument('b2b-address-validation-assign-address.graphql');

const companyDeleteDocument = `#graphql
  mutation B2BAddressValidationCompanyDelete($id: ID!) {
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readRequestDocument(name: string): Promise<string> {
  return await readFile(path.join(requestDir, name), 'utf8');
}

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

function sameStringArray(left: unknown, right: string[]): boolean {
  return (
    Array.isArray(left) &&
    left.length === right.length &&
    left.every((value, index) => typeof value === 'string' && value === right[index])
  );
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

function assertUserError(
  result: ConformanceGraphqlResult,
  root: string,
  field: string[],
  message: string,
  label: string,
): void {
  assertNoTopLevelErrors(result, label);
  const userErrors = readUserErrors(result.payload, root);
  const matched = userErrors.some((error) => {
    const record = readRecord(error);
    return record?.['code'] === 'INVALID' && record['message'] === message && sameStringArray(record['field'], field);
  });
  if (!matched) {
    throw new Error(`${label} did not return expected INVALID userError: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertNullPayloadObject(
  result: ConformanceGraphqlResult,
  root: string,
  payloadField: string,
  label: string,
): void {
  const value = readPath(result.payload, ['data', root, payloadField]);
  if (value !== null) {
    throw new Error(`${label} unexpectedly returned ${payloadField}: ${JSON.stringify(value, null, 2)}`);
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

async function runAddressValidation(
  query: string,
  variables: JsonRecord,
  root: string,
  payloadField: string,
  field: string[],
  message: string,
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertUserError(result, root, field, message, label);
  assertNullPayloadObject(result, root, payloadField, label);
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
  const setupCompany = await runRequired(
    companyCreateDocument,
    {
      input: {
        company: {
          name: `${runLabel} setup`,
        },
        companyLocation: {
          name: `${runLabel} HQ`,
          shippingAddress: {
            address1: '1 Pine St',
            countryCode: 'US',
            zoneCode: 'CA',
            zip: '94105',
          },
        },
      },
    },
    'companyCreate',
    'companyCreate setup',
  );
  companyId = readStringAtPath(setupCompany.response, ['data', 'companyCreate', 'company', 'id'], 'setup company');
  const locationId = readStringAtPath(
    setupCompany.response,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
    'setup location',
  );

  const locationCreateInvalidCountry = await runAddressValidation(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${runLabel} invalid country`,
        shippingAddress: {
          address1: '1 Pine St',
          countryCode: 'ZZ',
        },
      },
    },
    'companyLocationCreate',
    'companyLocation',
    ['input', 'shippingAddress', 'countryCode'],
    'Country code is invalid',
    'companyLocationCreate invalid country',
  );

  const locationCreateInvalidZone = await runAddressValidation(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${runLabel} invalid zone`,
        shippingAddress: {
          address1: '1 Pine St',
          countryCode: 'US',
          zoneCode: 'ZZ',
        },
      },
    },
    'companyLocationCreate',
    'companyLocation',
    ['input', 'shippingAddress', 'zoneCode'],
    'Zone code is invalid',
    'companyLocationCreate invalid zone',
  );

  const locationCreateInvalidZip = await runAddressValidation(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${runLabel} invalid zip`,
        shippingAddress: {
          address1: '1 Pine St',
          countryCode: 'US',
          zoneCode: 'CA',
          zip: 'abcde',
        },
      },
    },
    'companyLocationCreate',
    'companyLocation',
    ['input', 'shippingAddress', 'zip'],
    'Zip is invalid',
    'companyLocationCreate invalid zip',
  );

  const locationCreateHtmlAddress1 = await runAddressValidation(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${runLabel} html address1`,
        shippingAddress: {
          address1: '<b>1 Pine</b>',
          countryCode: 'US',
          zoneCode: 'CA',
          zip: '94105',
        },
      },
    },
    'companyLocationCreate',
    'companyLocation',
    ['input', 'shippingAddress', 'address1'],
    'Address1 is invalid',
    'companyLocationCreate HTML address1',
  );

  const locationCreateHtmlRecipient = await runAddressValidation(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${runLabel} html recipient`,
        billingAddress: {
          address1: '1 Pine St',
          recipient: '<b>Buyer</b>',
          countryCode: 'CA',
          zoneCode: 'ON',
          zip: 'K1A 0B1',
        },
      },
    },
    'companyLocationCreate',
    'companyLocation',
    ['input', 'billingAddress', 'recipient'],
    'Recipient is invalid',
    'companyLocationCreate HTML recipient',
  );

  const locationCreateHtmlAddress2 = await runAddressValidation(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${runLabel} html address2`,
        billingAddress: {
          address1: '1 Pine St',
          address2: '<i>Suite</i>',
          countryCode: 'CA',
          zoneCode: 'ON',
          zip: 'K1A 0B1',
        },
      },
    },
    'companyLocationCreate',
    'companyLocation',
    ['input', 'billingAddress', 'address2'],
    'Address2 is invalid',
    'companyLocationCreate HTML address2',
  );

  const locationCreateEmojiCity = await runAddressValidation(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${runLabel} emoji city`,
        billingAddress: {
          address1: '1 Pine St',
          city: `Ottawa ${waveEmoji}`,
          countryCode: 'CA',
          zoneCode: 'ON',
          zip: 'K1A 0B1',
        },
      },
    },
    'companyLocationCreate',
    'companyLocation',
    ['input', 'billingAddress', 'city'],
    'City is invalid',
    'companyLocationCreate emoji city',
  );

  const locationCreateEmojiFirstName = await runAddressValidation(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${runLabel} emoji first name`,
        shippingAddress: {
          address1: '1 Pine St',
          firstName: waveEmoji,
          countryCode: 'US',
          zoneCode: 'CA',
          zip: '94105',
        },
      },
    },
    'companyLocationCreate',
    'companyLocation',
    ['input', 'shippingAddress', 'firstName'],
    'First name is invalid',
    'companyLocationCreate emoji firstName',
  );

  const locationCreateUrlLastName = await runAddressValidation(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${runLabel} url last name`,
        shippingAddress: {
          address1: '1 Pine St',
          lastName: 'https://example.com',
          countryCode: 'US',
          zoneCode: 'CA',
          zip: '94105',
        },
      },
    },
    'companyLocationCreate',
    'companyLocation',
    ['input', 'shippingAddress', 'lastName'],
    'Last name is invalid',
    'companyLocationCreate URL lastName',
  );

  const assignAddressInvalidCountry = await runAddressValidation(
    assignAddressDocument,
    {
      locationId,
      address: {
        address1: '1 Pine St',
        countryCode: 'ZZ',
      },
      addressTypes: ['BILLING'],
    },
    'companyLocationAssignAddress',
    'addresses',
    ['address', 'countryCode'],
    'Country code is invalid',
    'companyLocationAssignAddress invalid country',
  );

  const companyCreateNestedInvalidCountry = await runAddressValidation(
    companyCreateDocument,
    {
      input: {
        company: {
          name: `${runLabel} nested invalid`,
        },
        companyLocation: {
          shippingAddress: {
            address1: '1 Pine St',
            countryCode: 'ZZ',
          },
        },
      },
    },
    'companyCreate',
    'company',
    ['input', 'companyLocation', 'shippingAddress', 'countryCode'],
    'Country code is invalid',
    'companyCreate nested invalid country',
  );

  const companyDelete = await runRequired(
    companyDeleteDocument,
    { id: companyId },
    'companyDelete',
    'companyDelete cleanup',
  );
  companyDeleted = true;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      plan: 'Record B2B CompanyAddressInput country, zone, zip, HTML, emoji, and URL validation branches for company location create, assign-address, and nested company create paths.',
    },
    setupCompany,
    locationCreateInvalidCountry,
    locationCreateInvalidZone,
    locationCreateInvalidZip,
    locationCreateHtmlAddress1,
    locationCreateHtmlRecipient,
    locationCreateHtmlAddress2,
    locationCreateEmojiCity,
    locationCreateEmojiFirstName,
    locationCreateUrlLastName,
    assignAddressInvalidCountry,
    companyCreateNestedInvalidCountry,
    companyDelete,
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
    cleanup['companyDelete'] = await runCleanup(companyId);
  }
}
