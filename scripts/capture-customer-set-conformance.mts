// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function assertNoTopLevelErrors(result, context) {
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

const customerSlice = `
  id
  firstName
  lastName
  displayName
  email
  locale
  note
  verifiedEmail
  taxExempt
  taxExemptions
  tags
  state
  canDelete
  defaultEmailAddress { emailAddress }
  defaultPhoneNumber { phoneNumber }
  defaultAddress { address1 city province country zip formattedArea }
  addressesV2(first: 5) {
    nodes { id address1 city province country zip formattedArea }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  createdAt
  updatedAt
`;

const customerSetMutation = `#graphql
  mutation CustomerSetParity($input: CustomerSetInput!, $identifier: CustomerSetIdentifiers) {
    customerSet(input: $input, identifier: $identifier) {
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

const downstreamReadQuery = `#graphql
  query CustomerSetDownstream($createdId: ID!, $upsertedId: ID!, $upsertedEmail: String!, $query: String!, $first: Int!) {
    created: customer(id: $createdId) {
      ${customerSlice}
    }
    upserted: customer(id: $upsertedId) {
      ${customerSlice}
    }
    byIdentifier: customerByIdentifier(identifier: { emailAddress: $upsertedEmail }) {
      id
      email
      displayName
    }
    customers(first: $first, query: $query, sortKey: UPDATED_AT, reverse: true) {
      nodes {
        id
        email
        displayName
        tags
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    customersCount {
      count
      precision
    }
  }
`;

const deleteMutation = `#graphql
  mutation CustomerSetCleanup($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

async function runCustomerSet(variables) {
  return runGraphql(customerSetMutation, variables);
}

async function main() {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const createdEmail = `hermes-customerset-create-${stamp}@example.com`;
  const upsertedEmail = `hermes-customerset-upsert-${stamp}@example.com`;
  const createdPhone = `+1${String(stamp).slice(-10)}`;
  const updatedPhone = `+1${String(stamp + 1).slice(-10)}`;

  const createVariables = {
    input: {
      email: createdEmail,
      firstName: 'Hermes',
      lastName: 'SetCreate',
      locale: 'en',
      note: 'customerSet create parity probe',
      phone: createdPhone,
      tags: ['set', 'create'],
      taxExempt: true,
      taxExemptions: ['CA_BC_RESELLER_EXEMPTION'],
    },
  };
  const createResult = await runCustomerSet(createVariables);
  assertNoTopLevelErrors(createResult, 'customerSet create');
  const createdCustomerId = createResult.payload?.data?.customerSet?.customer?.id;
  if (typeof createdCustomerId !== 'string' || !createdCustomerId) {
    throw new Error(
      `customerSet create did not return a customer id: ${JSON.stringify(createResult.payload, null, 2)}`,
    );
  }

  const updateVariables = {
    identifier: { id: createdCustomerId },
    input: {
      email: createdEmail,
      firstName: 'Hermes',
      lastName: 'SetUpdated',
      note: 'customerSet update parity probe',
      phone: updatedPhone,
      tags: ['set', 'updated'],
      taxExempt: false,
      taxExemptions: [],
      addresses: [{ address1: '10 Set St', city: 'Ottawa', countryCode: 'CA', provinceCode: 'ON', zip: 'K1A 0B1' }],
    },
  };
  const updateResult = await runCustomerSet(updateVariables);
  assertNoTopLevelErrors(updateResult, 'customerSet update by id');

  const upsertVariables = {
    identifier: { email: upsertedEmail },
    input: {
      email: upsertedEmail,
      firstName: 'Hermes',
      lastName: 'SetUpsert',
      note: 'customerSet upsert parity probe',
      tags: ['set', 'upsert'],
      taxExempt: false,
    },
  };
  const upsertResult = await runCustomerSet(upsertVariables);
  assertNoTopLevelErrors(upsertResult, 'customerSet upsert by email');
  const upsertedCustomerId = upsertResult.payload?.data?.customerSet?.customer?.id;
  if (typeof upsertedCustomerId !== 'string' || !upsertedCustomerId) {
    throw new Error(
      `customerSet upsert did not return a customer id: ${JSON.stringify(upsertResult.payload, null, 2)}`,
    );
  }

  const updateByEmailVariables = {
    identifier: { email: upsertedEmail },
    input: {
      email: upsertedEmail,
      firstName: 'Hermes',
      lastName: 'SetUpsertUpdated',
      note: 'customerSet upsert update parity probe',
      tags: ['set', 'upsert-updated'],
      addresses: [{ address1: '11 Upsert St', city: 'Toronto', countryCode: 'CA', provinceCode: 'ON', zip: 'M5H 2N2' }],
    },
  };
  const updateByEmailResult = await runCustomerSet(updateByEmailVariables);
  assertNoTopLevelErrors(updateByEmailResult, 'customerSet update by email');

  const clearAddressesVariables = {
    identifier: { id: upsertedCustomerId },
    input: {
      email: upsertedEmail,
      addresses: [],
    },
  };
  const clearAddressesResult = await runCustomerSet(clearAddressesVariables);
  assertNoTopLevelErrors(clearAddressesResult, 'customerSet empty address replacement');

  const downstreamVariables = {
    createdId: createdCustomerId,
    upsertedId: upsertedCustomerId,
    upsertedEmail,
    query: 'tag:set',
    first: 5,
  };
  const downstreamRead = await runGraphql(downstreamReadQuery, downstreamVariables);
  assertNoTopLevelErrors(downstreamRead, 'customerSet downstream read');

  const missingIdentityVariables = { input: { email: '' } };
  const missingIdentity = await runCustomerSet(missingIdentityVariables);
  assertNoTopLevelErrors(missingIdentity, 'customerSet missing identity validation');

  const unknownIdVariables = {
    identifier: { id: 'gid://shopify/Customer/999999999999999' },
    input: { firstName: 'Ghost' },
  };
  const unknownId = await runCustomerSet(unknownIdVariables);
  assertNoTopLevelErrors(unknownId, 'customerSet unknown id validation');

  const customIdVariables = {
    identifier: { customId: { namespace: 'custom', key: 'external_id', value: `customer-set-${stamp}` } },
    input: { firstName: 'Custom' },
  };
  const customId = await runCustomerSet(customIdVariables);
  if (!customId.payload?.errors) {
    throw new Error(`customerSet customId branch unexpectedly succeeded: ${JSON.stringify(customId.payload, null, 2)}`);
  }

  const cleanup = [];
  for (const id of [createdCustomerId, upsertedCustomerId]) {
    cleanup.push(await runGraphql(deleteMutation, { input: { id } }));
  }

  const capture = {
    mutation: {
      variables: createVariables,
      response: createResult.payload,
    },
    update: {
      variables: updateVariables,
      response: updateResult.payload,
    },
    upsert: {
      variables: upsertVariables,
      response: upsertResult.payload,
    },
    updateByEmail: {
      variables: updateByEmailVariables,
      response: updateByEmailResult.payload,
    },
    clearAddresses: {
      variables: clearAddressesVariables,
      response: clearAddressesResult.payload,
    },
    downstreamRead: {
      variables: downstreamVariables,
      response: downstreamRead.payload,
    },
    validation: {
      missingIdentity: {
        variables: missingIdentityVariables,
        response: missingIdentity.payload,
      },
      unknownId: {
        variables: unknownIdVariables,
        response: unknownId.payload,
      },
      customId: {
        variables: customIdVariables,
        response: customId.payload,
      },
    },
    cleanup: cleanup.map((result) => result.payload),
  };

  const outputPath = path.join(outputDir, 'customer-set-parity.json');
  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
}

await main();
