import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

/* oxlint-disable no-console -- CLI capture scripts report output path and cleanup failures. */

type JsonRecord = Record<string, unknown>;
type RecordedOperation = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: JsonRecord;
};

const scenarioId = 'b2b-buyer-experience-configuration';
const timestamp = Date.now();
const companyName = `B2B buyer experience ${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const buyerExperienceSelection = `#graphql
  buyerExperienceConfiguration {
    editableShippingAddress
    checkoutToDraft
    paymentTermsTemplate {
      id
    }
    deposit {
      __typename
    }
  }
`;

const schemaProbeDocument = `#graphql
  query B2BBuyerExperienceSchema {
    location: __type(name: "CompanyLocation") {
      fields { name }
    }
    buyerExperienceConfiguration: __type(name: "BuyerExperienceConfiguration") {
      fields { name }
    }
    buyerExperienceConfigurationInput: __type(name: "BuyerExperienceConfigurationInput") {
      inputFields { name }
    }
    depositInput: __type(name: "DepositInput") {
      inputFields { name }
    }
  }
`;

const companyCreateDocument = `#graphql
  mutation B2BBuyerExperienceCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        locations(first: 5) {
          nodes {
            id
            name
            ${buyerExperienceSelection}
          }
        }
      }
      userErrors { field message code }
    }
  }
`;

const locationCreateDocument = `#graphql
  mutation B2BBuyerExperienceLocationCreate($companyId: ID!, $input: CompanyLocationInput!) {
    companyLocationCreate(companyId: $companyId, input: $input) {
      companyLocation {
        id
        name
        ${buyerExperienceSelection}
      }
      userErrors { field message code }
    }
  }
`;

const locationUpdateDocument = `#graphql
  mutation B2BBuyerExperienceLocationUpdate($companyLocationId: ID!, $input: CompanyLocationUpdateInput!) {
    companyLocationUpdate(companyLocationId: $companyLocationId, input: $input) {
      companyLocation {
        id
        name
        ${buyerExperienceSelection}
      }
      userErrors { field message code }
    }
  }
`;

const companyReadDocument = `#graphql
  query B2BBuyerExperienceCompanyRead($companyId: ID!) {
    company(id: $companyId) {
      id
      name
      locations(first: 5) {
        nodes {
          id
          name
          ${buyerExperienceSelection}
        }
      }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BBuyerExperienceCompanyDelete($id: ID!) {
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

function assertValidationError(result: ConformanceGraphqlResult, root: string, label: string): void {
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

async function runSuccess(
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
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertValidationError(result, root, label);
  return recordOperation(query, variables, result);
}

async function runRead(query: string, variables: JsonRecord, label: string): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertNoTopLevelErrors(result, label);
  return recordOperation(query, variables, result);
}

let companyId: string | null = null;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const schemaProbeResult = await runGraphqlRequest(schemaProbeDocument, {});
  assertNoTopLevelErrors(schemaProbeResult, 'B2B buyer experience schema probe');
  const schemaProbe = recordOperation(schemaProbeDocument, {}, schemaProbeResult);

  const companyCreate = await runSuccess(
    companyCreateDocument,
    {
      input: {
        company: {
          name: companyName,
        },
        companyLocation: {
          name: `${companyName} HQ`,
          buyerExperienceConfiguration: {
            paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
            checkoutToDraft: true,
            editableShippingAddress: true,
          },
        },
      },
    },
    'companyCreate',
    'B2B buyer experience companyCreate',
  );

  companyId = readStringAtPath(companyCreate.response, ['data', 'companyCreate', 'company', 'id'], 'company');
  const primaryLocationId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
    'primary location',
  );

  const locationCreate = await runSuccess(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${companyName} Branch`,
        buyerExperienceConfiguration: {
          paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
          checkoutToDraft: false,
          editableShippingAddress: true,
        },
      },
    },
    'companyLocationCreate',
    'B2B buyer experience companyLocationCreate',
  );

  const validationLocationCreate = await runSuccess(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${companyName} Validation`,
      },
    },
    'companyLocationCreate',
    'B2B buyer experience validation location create',
  );

  const emptyBuyerExperienceUpdate = await runValidation(
    locationUpdateDocument,
    {
      companyLocationId: primaryLocationId,
      input: {
        buyerExperienceConfiguration: {},
      },
    },
    'companyLocationUpdate',
    'B2B buyer experience empty update validation',
  );

  const depositWithoutTermsUpdate = await runValidation(
    locationUpdateDocument,
    {
      companyLocationId: readStringAtPath(
        validationLocationCreate.response,
        ['data', 'companyLocationCreate', 'companyLocation', 'id'],
        'validation location',
      ),
      input: {
        buyerExperienceConfiguration: {
          deposit: { percentage: 50 },
        },
      },
    },
    'companyLocationUpdate',
    'B2B buyer experience deposit without terms validation',
  );

  const depositWithTermsUpdate = await runSuccess(
    locationUpdateDocument,
    {
      companyLocationId: primaryLocationId,
      input: {
        buyerExperienceConfiguration: {
          paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
          checkoutToDraft: true,
          editableShippingAddress: true,
          deposit: { percentage: 50 },
        },
      },
    },
    'companyLocationUpdate',
    'B2B buyer experience deposit with terms update',
  );

  const readAfterBuyerExperience = await runRead(
    companyReadDocument,
    { companyId },
    'B2B buyer experience read after writes',
  );

  const cleanupResult = await runGraphqlRequest(companyDeleteDocument, { id: companyId });
  cleanup.companyDelete = recordOperation(companyDeleteDocument, { id: companyId }, cleanupResult);
  companyId = null;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      plan: 'Create disposable B2B company locations with buyerExperienceConfiguration, record empty/deposit validation branches, update a location with deposit and payment terms, read back staged values, and clean up the company.',
      deposits:
        'The conformance shop accepted deposit with paymentTermsTemplateId; disabled-shop deposit validation remains covered by local runtime tests.',
    },
    schemaProbe,
    companyCreate,
    locationCreate,
    validationLocationCreate,
    emptyBuyerExperienceUpdate,
    depositWithoutTermsUpdate,
    depositWithTermsUpdate,
    readAfterBuyerExperience,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (companyId) {
    try {
      const cleanupResult = await runGraphqlRequest(companyDeleteDocument, { id: companyId });
      console.error(
        JSON.stringify({ cleanup: recordOperation(companyDeleteDocument, { id: companyId }, cleanupResult) }, null, 2),
      );
    } catch (error) {
      console.error(`Cleanup failed for ${companyId}: ${String(error)}`);
    }
  }
}
