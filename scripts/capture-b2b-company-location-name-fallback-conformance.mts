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

const scenarioId = 'company_location_name_fallback';
const timestamp = Date.now();
const nestedCompanyName = `B2B nested location fallback ${timestamp}`;
const standaloneCompanyName = `B2B standalone location fallback ${timestamp}`;
const nestedAddress1 = `Nested fallback street ${timestamp}`;
const standaloneAddress1 = `Standalone fallback street ${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

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

const companyReadDocument = `#graphql
  query B2BStringValidationCompanyRead($id: ID!) {
    company(id: $id) {
      id
      name
      locations(first: 10) {
        nodes {
          id
          name
        }
      }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BCompanyLocationNameFallbackCompanyDelete($id: ID!) {
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

function readUserErrors(payload: unknown, root: string): JsonRecord[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value.filter((item): item is JsonRecord => readRecord(item) !== null) : [];
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

async function runCleanup(companyId: string): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(companyDeleteDocument, { id: companyId });
  return recordOperation(companyDeleteDocument, { id: companyId }, result);
}

let nestedCompanyId: string | null = null;
let standaloneCompanyId: string | null = null;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const nestedCompanyCreate = await runRequired(
    companyCreateDocument,
    {
      input: {
        company: { name: nestedCompanyName },
        companyLocation: {
          shippingAddress: {
            address1: nestedAddress1,
            city: 'Boston',
            countryCode: 'US',
          },
        },
      },
    },
    'companyCreate',
    'nested companyCreate companyLocation name fallback',
  );
  nestedCompanyId = readStringAtPath(
    nestedCompanyCreate.response,
    ['data', 'companyCreate', 'company', 'id'],
    'nested companyCreate company id',
  );
  const nestedLocationName = readStringAtPath(
    nestedCompanyCreate.response,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'name'],
    'nested companyCreate location name',
  );
  if (nestedLocationName !== nestedCompanyName) {
    throw new Error(
      `nested companyCreate used ${JSON.stringify(nestedLocationName)} instead of company name ${JSON.stringify(
        nestedCompanyName,
      )}`,
    );
  }

  const nestedCompanyRead = await runRequired(
    companyReadDocument,
    { id: nestedCompanyId },
    'company',
    'nested companyCreate downstream company read',
  );
  const nestedReadLocationName = readStringAtPath(
    nestedCompanyRead.response,
    ['data', 'company', 'locations', 'nodes', '0', 'name'],
    'nested companyCreate downstream location name',
  );
  if (nestedReadLocationName !== nestedCompanyName) {
    throw new Error(
      `nested company read used ${JSON.stringify(nestedReadLocationName)} instead of company name ${JSON.stringify(
        nestedCompanyName,
      )}`,
    );
  }

  const standaloneSetupCompany = await runRequired(
    companyCreateDocument,
    {
      input: {
        company: { name: standaloneCompanyName },
      },
    },
    'companyCreate',
    'standalone companyLocationCreate setup company',
  );
  standaloneCompanyId = readStringAtPath(
    standaloneSetupCompany.response,
    ['data', 'companyCreate', 'company', 'id'],
    'standalone setup company id',
  );

  const standaloneLocationCreate = await runRequired(
    locationCreateDocument,
    {
      companyId: standaloneCompanyId,
      input: {
        phone: '+14155550179',
        shippingAddress: {
          address1: standaloneAddress1,
          city: 'Austin',
          countryCode: 'US',
        },
      },
    },
    'companyLocationCreate',
    'standalone companyLocationCreate shipping address fallback',
  );
  const standaloneLocationName = readStringAtPath(
    standaloneLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'name'],
    'standalone companyLocationCreate location name',
  );
  if (standaloneLocationName !== standaloneAddress1) {
    throw new Error(
      `standalone companyLocationCreate used ${JSON.stringify(
        standaloneLocationName,
      )} instead of shipping address ${JSON.stringify(standaloneAddress1)}`,
    );
  }

  if (nestedCompanyId) {
    cleanup['nestedCompanyDelete'] = await runCleanup(nestedCompanyId);
    nestedCompanyId = null;
  }
  if (standaloneCompanyId) {
    cleanup['standaloneCompanyDelete'] = await runCleanup(standaloneCompanyId);
    standaloneCompanyId = null;
  }

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      plan: 'Record divergent B2B location name fallbacks: nested companyCreate ignores shippingAddress.address1 and uses the company name, while standalone companyLocationCreate falls back to shippingAddress.address1.',
    },
    nestedCompanyCreate,
    nestedCompanyRead,
    standaloneSetupCompany,
    standaloneLocationCreate,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (nestedCompanyId) {
    cleanup['nestedCompanyDelete'] = await runCleanup(nestedCompanyId);
  }
  if (standaloneCompanyId) {
    cleanup['standaloneCompanyDelete'] = await runCleanup(standaloneCompanyId);
  }
}
