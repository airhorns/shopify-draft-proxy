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

const scenarioId = 'b2b-company-location-tax-settings-sequential';
const timestamp = Date.now();
const companyName = `B2B tax settings sequential ${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const fixturePath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);

const companyDeleteDocument = `#graphql
  mutation B2BTaxSettingsSequentialCompanyDelete($id: ID!) {
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
    if (Array.isArray(current)) {
      const index = Number(segment);
      if (!Number.isInteger(index) || index < 0) return undefined;
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

function readUserErrors(payload: unknown, root: string): unknown[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value : [];
}

function assertSuccessful(result: ConformanceGraphqlResult, root: string, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result, null, 2)}`);
  }
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

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = await readDocument(
  'config/parity-requests/b2b/b2b-contact-location-assignments-tax-create.graphql',
);
const taxUpdateDocument = await readDocument(
  'config/parity-requests/b2b/b2b-contact-location-assignments-tax-tax.graphql',
);
const taxReadDocument = await readDocument(
  'config/parity-requests/b2b/b2b-contact-location-assignments-tax-read.graphql',
);

let companyId: string | null = null;
let companyDeleted = false;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const companyCreateVariables = {
    input: {
      company: {
        name: companyName,
        note: 'B2B tax settings sequential parity',
        externalId: `b2b-tax-settings-sequential-${timestamp}`,
      },
      companyContact: {
        firstName: 'Tax',
        lastName: 'Settings',
        email: `b2b-tax-settings-sequential-${timestamp}@example.com`,
        title: 'Buyer',
      },
      companyLocation: {
        name: `${companyName} HQ`,
        phone: '+16135550121',
        billingAddress: {
          address1: '1 Sequential Tax Way',
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
    'companyCreate setup',
  );
  companyId = readStringAtPath(companyCreate.response, ['data', 'companyCreate', 'company', 'id'], 'companyCreate');
  const companyContactId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'mainContact', 'id'],
    'companyCreate main contact',
  );
  const companyLocationId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
    'companyCreate main location',
  );

  const readVariables = {
    companyContactId,
    companyLocationId,
    singleAssignmentLocationId: companyLocationId,
    contactBulkLocationId: companyLocationId,
    locationBulkLocationId: companyLocationId,
  };

  const registrationOnlyTaxUpdate = await runRequired(
    taxUpdateDocument,
    {
      companyLocationId,
      taxRegistrationId: 'B2B-TAX-REGISTRATION-ONLY',
    },
    'companyLocationTaxSettingsUpdate',
    'registration-only tax settings update',
  );

  const readAfterRegistrationOnly = await runRequired(
    taxReadDocument,
    readVariables,
    'companyLocation',
    'read after registration-only tax settings update',
  );

  const noKnobsTaxUpdate = await runRequired(
    taxUpdateDocument,
    {
      companyLocationId,
    },
    'companyLocationTaxSettingsUpdate',
    'no-knob tax settings update',
  );

  const readAfterNoKnobs = await runRequired(
    taxReadDocument,
    readVariables,
    'companyLocation',
    'read after no-knob tax settings update',
  );

  const initialTaxUpdate = await runRequired(
    taxUpdateDocument,
    {
      companyLocationId,
      taxRegistrationId: 'B2B-TAX-SEQUENTIAL',
      taxExempt: true,
      exemptionsToAssign: ['EU_REVERSE_CHARGE_EXEMPTION_RULE'],
      exemptionsToRemove: [],
    },
    'companyLocationTaxSettingsUpdate',
    'initial tax settings update',
  );

  const removeAbsentTaxUpdate = await runRequired(
    taxUpdateDocument,
    {
      companyLocationId,
      exemptionsToRemove: ['US_CA_RESELLER_EXEMPTION'],
    },
    'companyLocationTaxSettingsUpdate',
    'remove absent tax exemption while preserving staged tax settings',
  );

  const readAfterRemoveAbsent = await runRequired(
    taxReadDocument,
    readVariables,
    'companyLocation',
    'read after remove absent tax exemption',
  );

  const assignRemoveTaxUpdate = await runRequired(
    taxUpdateDocument,
    {
      companyLocationId,
      exemptionsToAssign: ['CA_BC_RESELLER_EXEMPTION'],
      exemptionsToRemove: ['US_CA_RESELLER_EXEMPTION'],
    },
    'companyLocationTaxSettingsUpdate',
    'assign and remove tax exemptions together',
  );

  const readAfterAssignRemove = await runRequired(
    taxReadDocument,
    readVariables,
    'companyLocation',
    'read after assign and remove tax exemptions together',
  );

  const companyDelete = await runRequired(companyDeleteDocument, { id: companyId }, 'companyDelete', 'company cleanup');
  companyDeleted = true;

  await mkdir(path.dirname(fixturePath), { recursive: true });
  await writeFile(
    fixturePath,
    `${JSON.stringify(
      {
        scenarioId,
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        intent: {
          plan: 'Create a disposable B2B company/location, update location tax settings through registration-only, no-knob, and assign/remove combinations, read back taxSettings after each relevant write, then delete the company.',
        },
        companyCreate,
        registrationOnlyTaxUpdate,
        readAfterRegistrationOnly,
        noKnobsTaxUpdate,
        readAfterNoKnobs,
        initialTaxUpdate,
        removeAbsentTaxUpdate,
        readAfterRemoveAbsent,
        assignRemoveTaxUpdate,
        readAfterAssignRemove,
        companyDelete,
        cleanup,
        upstreamCalls: [],
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
