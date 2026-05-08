/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
  createAdminGraphqlClient,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedInteraction = {
  variables: Record<string, unknown>;
  status: number;
  response: ConformanceGraphqlPayload;
};

type CustomerCreateData = {
  customerCreate?: {
    customer?: {
      id?: unknown;
    } | null;
    userErrors?: unknown[];
  } | null;
};

type CustomerDeleteData = {
  customerDelete?: {
    deletedCustomerId?: unknown;
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const customerSlice = `
  id
  firstName
  lastName
  displayName
  email
  phone
  defaultEmailAddress {
    emailAddress
  }
  defaultPhoneNumber {
    phoneNumber
  }
  createdAt
  updatedAt
`;

const createMutation = `#graphql
  mutation CustomerCreateNameIdentity($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        ${customerSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const readQuery = `#graphql
  query CustomerCreateNameIdentityRead($id: ID!, $identifier: CustomerIdentifierInput!, $query: String!, $first: Int!) {
    customer(id: $id) {
      ${customerSlice}
    }
    customerByIdentifier(identifier: $identifier) {
      ${customerSlice}
    }
    customers(first: $first, query: $query, sortKey: UPDATED_AT, reverse: true) {
      nodes {
        ${customerSlice}
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation CustomerCreateNameIdentityDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function capture(variables: Record<string, unknown>, result: ConformanceGraphqlResult): CapturedInteraction {
  return {
    variables,
    status: result.status,
    response: result.payload,
  };
}

function userErrorsFor(result: ConformanceGraphqlResult<CustomerCreateData>): unknown[] {
  const errors = result.payload.data?.customerCreate?.userErrors;
  return Array.isArray(errors) ? errors : [];
}

function customerIdFrom(result: ConformanceGraphqlResult<CustomerCreateData>, context: string): string {
  const id = result.payload.data?.customerCreate?.customer?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${context} did not return a customer id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return id;
}

async function createCustomer(
  input: Record<string, unknown>,
  context: string,
): Promise<{ id: string; create: CapturedInteraction }> {
  const variables = { input };
  const result = await runGraphql<CustomerCreateData>(createMutation, variables);
  assertNoTopLevelErrors(result, context);
  const errors = userErrorsFor(result);
  if (errors.length !== 0) {
    throw new Error(`${context} unexpectedly returned userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return {
    id: customerIdFrom(result, context),
    create: capture(variables, result),
  };
}

async function createRejected(input: Record<string, unknown>, context: string): Promise<CapturedInteraction> {
  const variables = { input };
  const result = await runGraphql<CustomerCreateData>(createMutation, variables);
  assertNoTopLevelErrors(result, context);
  const errors = userErrorsFor(result);
  if (errors.length === 0) {
    throw new Error(`${context} did not return a userError: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return capture(variables, result);
}

async function readCustomer(id: string, query: string, context: string): Promise<CapturedInteraction> {
  const variables = {
    id,
    identifier: { id },
    query,
    first: 5,
  };
  const result = await runGraphql(readQuery, variables);
  assertNoTopLevelErrors(result, context);
  return capture(variables, result);
}

async function cleanupCustomers(ids: string[]): Promise<CapturedInteraction[]> {
  const cleanup: CapturedInteraction[] = [];
  for (const id of [...ids].reverse()) {
    const variables = { input: { id } };
    const result = await runGraphql<CustomerDeleteData>(deleteMutation, variables);
    cleanup.push(capture(variables, result));
  }
  return cleanup;
}

async function main(): Promise<void> {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const createdCustomerIds: string[] = [];

  try {
    const firstNameOnly = await createCustomer({ firstName: `HermesFirst${stamp}` }, 'firstName-only customerCreate');
    createdCustomerIds.push(firstNameOnly.id);
    const firstNameRead = await readCustomer(firstNameOnly.id, '', 'firstName-only downstream read');

    const lastNameOnly = await createCustomer({ lastName: `HermesLast${stamp}` }, 'lastName-only customerCreate');
    createdCustomerIds.push(lastNameOnly.id);
    const lastNameRead = await readCustomer(lastNameOnly.id, '', 'lastName-only downstream read');

    const blankInput = await createRejected({}, 'blank customerCreate');
    const cleanup = await cleanupCustomers(createdCustomerIds);

    const captureFile = path.join(outputDir, 'customer-create-name-identity.json');
    const captureBody = {
      storeDomain,
      apiVersion,
      metadata: {
        storeDomain,
        apiVersion,
        capturedAt: new Date().toISOString(),
        stamp,
      },
      scenarios: {
        firstNameOnly: {
          create: firstNameOnly.create,
          downstreamRead: firstNameRead,
        },
        lastNameOnly: {
          create: lastNameOnly.create,
          downstreamRead: lastNameRead,
        },
        blankInput: {
          create: blankInput,
        },
      },
      cleanup,
      upstreamCalls: [],
    };
    await writeFile(captureFile, `${JSON.stringify(captureBody, null, 2)}\n`, 'utf8');
    console.log(`Wrote ${captureFile}`);
  } catch (error) {
    await cleanupCustomers(createdCustomerIds);
    throw error;
  }
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});
