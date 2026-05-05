/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlResult = {
  status: number;
  payload: {
    data?: Record<string, unknown> | null;
    errors?: unknown;
    extensions?: unknown;
  };
};

type CapturedInteraction = {
  variables: Record<string, unknown>;
  status: number;
  response: GraphqlResult['payload'];
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
  defaultEmailAddress {
    emailAddress
  }
  defaultPhoneNumber {
    phoneNumber
  }
`;

const createMutation = `#graphql
  mutation CustomerUpdateRequiresIdentityCreate($input: CustomerInput!) {
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

const updateMutation = `#graphql
  mutation CustomerUpdateRequiresIdentityUpdate($input: CustomerInput!) {
    customerUpdate(input: $input) {
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
  query CustomerUpdateRequiresIdentityRead($id: ID!) {
    customer(id: $id) {
      ${customerSlice}
    }
  }
`;

const deleteMutation = `#graphql
  mutation CustomerUpdateRequiresIdentityDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

function assertNoTopLevelErrors(result: GraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function capture(variables: Record<string, unknown>, result: GraphqlResult): CapturedInteraction {
  return {
    variables,
    status: result.status,
    response: result.payload,
  };
}

function customerIdFromCreate(result: GraphqlResult, context: string): string {
  const data = result.payload.data as { customerCreate?: { customer?: { id?: unknown } } } | undefined;
  const id = data?.customerCreate?.customer?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${context} did not return a customer id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return id;
}

function userErrorsFor(result: GraphqlResult, root: string): unknown[] {
  const data = result.payload.data as Record<string, { userErrors?: unknown[] } | undefined> | undefined;
  const errors = data?.[root]?.userErrors;
  return Array.isArray(errors) ? errors : [];
}

function assertRejected(result: GraphqlResult, context: string): void {
  assertNoTopLevelErrors(result, context);
  const errors = userErrorsFor(result, 'customerUpdate');
  if (errors.length === 0) {
    throw new Error(`${context} did not return a userError: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertAccepted(result: GraphqlResult, context: string): void {
  assertNoTopLevelErrors(result, context);
  const errors = userErrorsFor(result, 'customerUpdate');
  if (errors.length !== 0) {
    throw new Error(`${context} unexpectedly returned userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

async function runInteraction(document: string, variables: Record<string, unknown>, context: string) {
  const result = (await runGraphql(document, variables)) as GraphqlResult;
  assertNoTopLevelErrors(result, context);
  return capture(variables, result);
}

async function createCustomer(input: Record<string, unknown>, context: string) {
  const interaction = await runInteraction(createMutation, { input }, context);
  const id = customerIdFromCreate({ status: interaction.status, payload: interaction.response }, context);
  return { id, interaction };
}

async function updateCustomer(input: Record<string, unknown>, context: string, expect: 'accepted' | 'rejected') {
  const variables = { input };
  const result = (await runGraphql(updateMutation, variables)) as GraphqlResult;
  if (expect === 'accepted') {
    assertAccepted(result, context);
  } else {
    assertRejected(result, context);
  }
  return capture(variables, result);
}

async function readCustomer(id: string, context: string) {
  return runInteraction(readQuery, { id }, context);
}

async function cleanupCustomers(ids: string[]) {
  const cleanup: CapturedInteraction[] = [];
  for (const id of [...ids].reverse()) {
    const variables = { input: { id } };
    const result = (await runGraphql(deleteMutation, variables)) as GraphqlResult;
    cleanup.push(capture(variables, result));
  }
  return cleanup;
}

async function main(): Promise<void> {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const createdCustomerIds: string[] = [];

  try {
    const emailOnly = await createCustomer(
      { email: `hermes-update-identity-email-${stamp}@example.com` },
      'email-only precondition customer',
    );
    createdCustomerIds.push(emailOnly.id);
    const emailReject = await updateCustomer({ id: emailOnly.id, email: null }, 'clear last email', 'rejected');
    const emailRead = await readCustomer(emailOnly.id, 'clear last email downstream read');

    const phoneOnly = await createCustomer(
      { phone: `+1415${String(stamp).slice(-7).padStart(7, '0')}` },
      'phone-only precondition customer',
    );
    createdCustomerIds.push(phoneOnly.id);
    const phoneReject = await updateCustomer({ id: phoneOnly.id, phone: null }, 'clear last phone', 'rejected');
    const phoneRead = await readCustomer(phoneOnly.id, 'clear last phone downstream read');

    const namePair = await createCustomer(
      {
        email: `hermes-update-identity-name-${stamp}@example.com`,
        firstName: 'Hermes',
        lastName: 'Identity',
      },
      'name-pair precondition customer',
    );
    createdCustomerIds.push(namePair.id);
    const emailControl = await updateCustomer(
      { id: namePair.id, email: null },
      'clear email while name remains',
      'accepted',
    );
    const namePairReject = await updateCustomer(
      { id: namePair.id, firstName: null, lastName: null },
      'clear last name pair',
      'rejected',
    );
    const namePairRead = await readCustomer(namePair.id, 'clear last name pair downstream read');

    const cleanup = await cleanupCustomers(createdCustomerIds);
    const captureFile = path.join(outputDir, 'customer-update-requires-identity.json');
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
        clearLastEmail: {
          create: emailOnly.interaction,
          update: emailReject,
          downstreamRead: emailRead,
        },
        clearLastPhone: {
          create: phoneOnly.interaction,
          update: phoneReject,
          downstreamRead: phoneRead,
        },
        clearEmailWhileNameRemains: {
          create: namePair.interaction,
          update: emailControl,
        },
        clearLastNamePair: {
          update: namePairReject,
          downstreamRead: namePairRead,
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

await main();
