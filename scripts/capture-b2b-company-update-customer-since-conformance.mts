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

const scenarioId = 'b2b-company-update-customer-since';
const timestamp = Date.now();
const originalCustomerSince = '2024-01-01T00:00:00Z';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation B2BCustomerSinceCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        customerSince
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
  mutation B2BCustomerSinceCompanyUpdate($companyId: ID!, $input: CompanyInput!) {
    companyUpdate(companyId: $companyId, input: $input) {
      company {
        id
        name
        customerSince
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
  query B2BCustomerSinceCompanyRead($companyId: ID!) {
    company(id: $companyId) {
      name
      customerSince
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BCustomerSinceCompanyDelete($id: ID!) {
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
    if (!record) {
      return undefined;
    }
    current = record[segment];
  }
  return current;
}

function readStringAtPath(value: unknown, pathSegments: string[]): string | null {
  const pathValue = readPath(value, pathSegments);
  return typeof pathValue === 'string' && pathValue.length > 0 ? pathValue : null;
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

async function runOperation(query: string, variables: JsonRecord): Promise<RecordedOperation> {
  return recordOperation(query, variables, await runGraphqlRequest(query, variables));
}

async function runCleanup(companyId: string): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(companyDeleteDocument, { id: companyId });
  return recordOperation(companyDeleteDocument, { id: companyId }, result);
}

let createdCompanyId: string | null = null;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const setupCompany = await runOperation(companyCreateDocument, {
    input: {
      company: {
        name: `HAR-760 customerSince ${timestamp}`,
        customerSince: originalCustomerSince,
      },
    },
  });
  createdCompanyId = readStringAtPath(setupCompany.response, ['data', 'companyCreate', 'company', 'id']);
  if (!createdCompanyId) {
    throw new Error(`Unable to create setup company: ${JSON.stringify(setupCompany.response, null, 2)}`);
  }

  const updateCustomerSinceOnly = await runOperation(companyUpdateDocument, {
    companyId: createdCompanyId,
    input: {
      customerSince: '2020-01-01T00:00:00Z',
    },
  });
  const readAfterOnly = await runOperation(companyReadDocument, {
    companyId: createdCompanyId,
  });

  const updateCustomerSinceWithName = await runOperation(companyUpdateDocument, {
    companyId: createdCompanyId,
    input: {
      name: 'HAR-760 changed name',
      customerSince: '2020-02-01T00:00:00Z',
    },
  });
  const readAfterMixed = await runOperation(companyReadDocument, {
    companyId: createdCompanyId,
  });

  const updateCustomerSinceNull = await runOperation(companyUpdateDocument, {
    companyId: createdCompanyId,
    input: {
      customerSince: null,
    },
  });
  const readAfterNull = await runOperation(companyReadDocument, {
    companyId: createdCompanyId,
  });

  cleanup[`companyDelete:${createdCompanyId}`] = await runCleanup(createdCompanyId);
  createdCompanyId = null;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      ticket: 'HAR-760',
      plan: 'Record Shopify rejecting companyUpdate customerSince when present as a timestamp, alongside another update field, and as null; each rejected update is followed by a read proving the company record stayed unchanged.',
    },
    setupCompany,
    updateCustomerSinceOnly,
    readAfterOnly,
    updateCustomerSinceWithName,
    readAfterMixed,
    updateCustomerSinceNull,
    readAfterNull,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- capture scripts report their output path.
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (createdCompanyId) {
    cleanup[`companyDelete:${createdCompanyId}`] = await runCleanup(createdCompanyId);
  }
}
