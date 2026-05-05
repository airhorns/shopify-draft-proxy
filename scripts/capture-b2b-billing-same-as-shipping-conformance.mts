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

const scenarioId = 'b2b-billing-same-as-shipping-validation';
const timestamp = Date.now();
const companyName = `HAR-612 B2B billing ${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation B2BBillingSameAsShippingCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        locations(first: 5) {
          nodes {
            id
            name
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
  mutation B2BBillingSameAsShippingLocationCreate($companyId: ID!, $input: CompanyLocationInput!) {
    companyLocationCreate(companyId: $companyId, input: $input) {
      companyLocation {
        id
        name
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
  mutation B2BBillingSameAsShippingLocationUpdate($companyLocationId: ID!, $input: CompanyLocationUpdateInput!) {
    companyLocationUpdate(companyLocationId: $companyLocationId, input: $input) {
      companyLocation {
        id
        name
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
  mutation B2BBillingSameAsShippingCompanyDelete($id: ID!) {
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

const createdCompanyIds = new Set<string>();
const cleanup: Record<string, RecordedOperation> = {};

function rememberCreatedCompany(operation: RecordedOperation): string | null {
  const companyId = readStringAtPath(operation.response, ['data', 'companyCreate', 'company', 'id']);
  if (companyId) createdCompanyIds.add(companyId);
  return companyId;
}

try {
  const companyCreateBillingSameAsShippingWithExplicitBilling = await runOperation(companyCreateDocument, {
    input: {
      company: { name: `${companyName} company create conflict` },
      companyLocation: {
        name: `${companyName} conflict HQ`,
        billingSameAsShipping: true,
        shippingAddress: { address1: 'Ship St' },
        billingAddress: { address1: 'Bill St' },
      },
    },
  });
  rememberCreatedCompany(companyCreateBillingSameAsShippingWithExplicitBilling);

  const companyCreateBillingSameAsShippingFalseNoBilling = await runOperation(companyCreateDocument, {
    input: {
      company: { name: `${companyName} company create missing billing` },
      companyLocation: {
        name: `${companyName} missing billing HQ`,
        billingSameAsShipping: false,
      },
    },
  });
  rememberCreatedCompany(companyCreateBillingSameAsShippingFalseNoBilling);

  const companyCreateTaxExemptNull = await runOperation(companyCreateDocument, {
    input: {
      company: { name: `${companyName} company create null tax` },
      companyLocation: {
        name: `${companyName} null tax HQ`,
        taxExempt: null,
      },
    },
  });
  rememberCreatedCompany(companyCreateTaxExemptNull);

  const setupCompany = await runOperation(companyCreateDocument, {
    input: {
      company: { name: `${companyName} setup` },
      companyLocation: {
        name: `${companyName} setup HQ`,
      },
    },
  });
  const setupCompanyId = rememberCreatedCompany(setupCompany);
  const setupLocationId = readStringAtPath(setupCompany.response, [
    'data',
    'companyCreate',
    'company',
    'locations',
    'nodes',
    '0',
    'id',
  ]);
  if (!setupCompanyId || !setupLocationId) {
    throw new Error(`Unable to create setup company/location: ${JSON.stringify(setupCompany.response, null, 2)}`);
  }

  const locationCreateBillingSameAsShippingWithExplicitBilling = await runOperation(locationCreateDocument, {
    companyId: setupCompanyId,
    input: {
      name: `${companyName} create conflict`,
      billingSameAsShipping: true,
      shippingAddress: { address1: 'Ship St' },
      billingAddress: { address1: 'Bill St' },
    },
  });

  const locationCreateBillingSameAsShippingFalseNoBilling = await runOperation(locationCreateDocument, {
    companyId: setupCompanyId,
    input: {
      name: `${companyName} create missing billing`,
      billingSameAsShipping: false,
    },
  });

  const locationCreateTaxExemptNull = await runOperation(locationCreateDocument, {
    companyId: setupCompanyId,
    input: {
      name: `${companyName} create null tax`,
      taxExempt: null,
    },
  });

  const locationUpdateBillingSameAsShippingWithExplicitBilling = await runOperation(locationUpdateDocument, {
    companyLocationId: setupLocationId,
    input: {
      name: `${companyName} update conflict`,
      billingSameAsShipping: true,
      billingAddress: { address1: 'Bill St' },
    },
  });

  const locationUpdateBillingSameAsShippingFalseNoBilling = await runOperation(locationUpdateDocument, {
    companyLocationId: setupLocationId,
    input: {
      name: `${companyName} update missing billing`,
      billingSameAsShipping: false,
    },
  });

  const locationUpdateTaxExemptNull = await runOperation(locationUpdateDocument, {
    companyLocationId: setupLocationId,
    input: {
      name: `${companyName} update null tax`,
      taxExempt: null,
    },
  });

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
      ticket: 'HAR-612',
      plan: 'Record B2B billingSameAsShipping/billingAddress mutual-exclusion validation and taxExempt null validation for companyCreate, companyLocationCreate, and companyLocationUpdate.',
    },
    companyCreateBillingSameAsShippingWithExplicitBilling,
    companyCreateBillingSameAsShippingFalseNoBilling,
    companyCreateTaxExemptNull,
    setupCompany,
    locationCreateBillingSameAsShippingWithExplicitBilling,
    locationCreateBillingSameAsShippingFalseNoBilling,
    locationCreateTaxExemptNull,
    locationUpdateBillingSameAsShippingWithExplicitBilling,
    locationUpdateBillingSameAsShippingFalseNoBilling,
    locationUpdateTaxExemptNull,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
} finally {
  for (const companyId of createdCompanyIds) {
    try {
      await runCleanup(companyId);
    } catch (error) {
      console.error(`Cleanup failed for ${companyId}:`, error);
    }
  }
}
