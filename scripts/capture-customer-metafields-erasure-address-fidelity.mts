/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
  createAdminGraphqlClient,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type CapturedInteraction = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  status: number;
  response: ConformanceGraphqlPayload;
};

type CustomerCreateData = {
  customerCreate?: {
    customer?: {
      id?: unknown;
      tier?: unknown;
      birthday?: unknown;
      metafields?: { nodes?: unknown };
      defaultAddress?: { countryCodeV2?: unknown };
    } | null;
    userErrors?: unknown[];
  } | null;
};

type CustomerReadData = {
  customer?: {
    id?: unknown;
    tier?: unknown;
    birthday?: unknown;
    metafields?: { nodes?: unknown };
    defaultAddress?: { countryCodeV2?: unknown };
  } | null;
};

type DataErasureData = {
  customerRequestDataErasure?: {
    customerId?: unknown;
    userErrors?: unknown[];
  } | null;
};

type CustomerDeleteData = {
  customerDelete?: {
    deletedCustomerId?: unknown;
    userErrors?: unknown[];
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const outputPath = path.join(outputDir, 'customer-metafields-erasure-address-fidelity.json');

const createDocument = await readFile(
  'config/parity-requests/customers/customer-metafields-erasure-address-create.graphql',
  'utf8',
);
const readDocument = await readFile(
  'config/parity-requests/customers/customer-metafields-erasure-address-read.graphql',
  'utf8',
);
const erasureDocument = await readFile(
  'config/parity-requests/customers/customer-request-data-erasure.graphql',
  'utf8',
);
const hydrateDocument = await readFile('config/parity-requests/customers/customer-mutation-hydrate.graphql', 'utf8');

const cancelDataErasureDocument = `#graphql
  mutation CustomerMetafieldsErasureAddressCancel($customerId: ID!) {
    customerCancelDataErasure(customerId: $customerId) {
      customerId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteDocument = `#graphql
  mutation CustomerMetafieldsErasureAddressDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function record(query: string, variables: JsonRecord, result: ConformanceGraphqlResult): CapturedInteraction {
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function readUserErrors(payload: unknown, path: string[]): unknown[] {
  let cursor: unknown = payload;
  for (const part of path) {
    if (cursor === null || typeof cursor !== 'object' || Array.isArray(cursor)) return [];
    cursor = (cursor as JsonRecord)[part];
  }
  return Array.isArray(cursor) ? cursor : [];
}

function assertNoUserErrors(result: ConformanceGraphqlResult, path: string[], context: string): void {
  const errors = readUserErrors(result.payload, path);
  if (errors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function requireString(value: unknown, context: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${context} was not a non-empty string: ${JSON.stringify(value)}`);
  }
  return value;
}

function assertCapturedCustomerShape(
  customer: CustomerCreateData['customerCreate']['customer'],
  context: string,
): void {
  if (!customer) throw new Error(`${context} did not return a customer`);
  const metafieldNodes = customer.metafields?.nodes;
  if (!Array.isArray(metafieldNodes) || metafieldNodes.length < 2) {
    throw new Error(`${context} did not return multiple metafields: ${JSON.stringify(customer, null, 2)}`);
  }
  if (!customer.tier || !customer.birthday) {
    throw new Error(`${context} did not return both aliased metafields: ${JSON.stringify(customer, null, 2)}`);
  }
  if (customer.defaultAddress?.countryCodeV2 !== 'DK') {
    throw new Error(`${context} did not return a Denmark default address: ${JSON.stringify(customer, null, 2)}`);
  }
}

async function cleanupCustomer(customerId: string | null): Promise<{
  cancelDataErasure?: CapturedInteraction;
  delete?: CapturedInteraction;
}> {
  if (!customerId) return {};
  const cleanup: { cancelDataErasure?: CapturedInteraction; delete?: CapturedInteraction } = {};
  const cancelVariables = { customerId };
  const cancel = await runGraphql(cancelDataErasureDocument, cancelVariables);
  cleanup.cancelDataErasure = record(cancelDataErasureDocument, cancelVariables, cancel);

  const deleteVariables = { input: { id: customerId } };
  const deleted = await runGraphql<CustomerDeleteData>(deleteDocument, deleteVariables);
  cleanup.delete = record(deleteDocument, deleteVariables, deleted);
  return cleanup;
}

async function main(): Promise<void> {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const input = {
    email: `customer-fidelity-${stamp}@example.com`,
    firstName: 'Metafield',
    lastName: 'Fidelity',
    metafields: [
      {
        namespace: 'custom',
        key: 'tier',
        type: 'single_line_text_field',
        value: 'gold',
      },
      {
        namespace: 'profile',
        key: 'birthday',
        type: 'date',
        value: '1990-01-01',
      },
    ],
    addresses: [
      {
        address1: 'Radhuspladsen 1',
        city: 'Copenhagen',
        countryCode: 'DK',
        zip: '1550',
      },
    ],
  };

  let customerId: string | null = null;
  let cleanup: Awaited<ReturnType<typeof cleanupCustomer>> = {};
  try {
    const createVariables = { input };
    const create = await runGraphql<CustomerCreateData>(createDocument, createVariables);
    assertNoTopLevelErrors(create, 'customerCreate');
    assertNoUserErrors(create, ['data', 'customerCreate', 'userErrors'], 'customerCreate');
    const customer = create.payload.data?.customerCreate?.customer;
    assertCapturedCustomerShape(customer, 'customerCreate');
    customerId = requireString(customer?.id, 'customerCreate.customer.id');

    const readVariables = { id: customerId };
    const read = await runGraphql<CustomerReadData>(readDocument, readVariables);
    assertNoTopLevelErrors(read, 'customer read');
    assertCapturedCustomerShape(read.payload.data?.customer, 'customer read');

    const hydrateVariables = { id: customerId };
    const hydrate = await runGraphql(hydrateDocument, hydrateVariables);
    assertNoTopLevelErrors(hydrate, 'CustomerHydrate upstream cassette');

    const erasureVariables = { customerId };
    const erasure = await runGraphql<DataErasureData>(erasureDocument, erasureVariables);
    assertNoTopLevelErrors(erasure, 'customerRequestDataErasure');
    assertNoUserErrors(erasure, ['data', 'customerRequestDataErasure', 'userErrors'], 'customerRequestDataErasure');
    if (erasure.payload.data?.customerRequestDataErasure?.customerId !== customerId) {
      throw new Error(`customerRequestDataErasure did not echo customerId: ${JSON.stringify(erasure.payload)}`);
    }

    cleanup = await cleanupCustomer(customerId);

    const body = {
      storeDomain,
      apiVersion,
      recordedAt: new Date().toISOString(),
      create: record(createDocument, createVariables, create),
      downstreamRead: record(readDocument, readVariables, read),
      erasureHydrate: record(hydrateDocument, hydrateVariables, hydrate),
      erasureRequest: record(erasureDocument, erasureVariables, erasure),
      cleanup,
      upstreamCalls: [
        {
          operationName: 'CustomerHydrate',
          variables: hydrateVariables,
          query: hydrateDocument,
          response: {
            status: hydrate.status,
            body: hydrate.payload,
          },
        },
      ],
    };
    await writeFile(outputPath, `${JSON.stringify(body, null, 2)}\n`, 'utf8');
    console.log(`Wrote ${outputPath}`);
  } catch (error) {
    cleanup = await cleanupCustomer(customerId);
    console.error(`Cleanup after failure: ${JSON.stringify(cleanup, null, 2)}`);
    throw error;
  }
}

await main();
