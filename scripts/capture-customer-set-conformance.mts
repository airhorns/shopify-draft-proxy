// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
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

const customerSetIdNotAllowedMutation = await readFile(
  'config/parity-requests/customers/customer-set-id-not-allowed.graphql',
  'utf8',
);

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

async function runCustomerSetIdNotAllowed(variables) {
  return runGraphql(customerSetIdNotAllowedMutation, variables);
}

function assertIdNotAllowed(result, context) {
  assertNoTopLevelErrors(result, context);
  const userErrors = result.payload?.data?.customerSet?.userErrors;
  if (!Array.isArray(userErrors) || userErrors.length !== 1) {
    throw new Error(`${context} did not return exactly one userError: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const [error] = userErrors;
  if (
    JSON.stringify(error.field) !== JSON.stringify(['input']) ||
    error.message !== 'The id field is not allowed if identifier is provided.' ||
    error.code !== 'ID_NOT_ALLOWED'
  ) {
    throw new Error(`${context} returned unexpected userError: ${JSON.stringify(error, null, 2)}`);
  }
  if (result.payload?.data?.customerSet?.customer !== null) {
    throw new Error(`${context} unexpectedly returned a customer: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

async function main() {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const createdEmail = `hermes-customerset-create-${stamp}@example.com`;
  const upsertedEmail = `hermes-customerset-upsert-${stamp}@example.com`;
  const phoneUpsert = `+1${String(stamp + 2).slice(-10)}`;
  const createdPhone = `+1${String(stamp).slice(-10)}`;
  const updatedPhone = `+1${String(stamp + 1).slice(-10)}`;
  const cleanupIds = new Set();

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
  cleanupIds.add(createdCustomerId);

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
  cleanupIds.add(upsertedCustomerId);

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

  const phoneUpsertVariables = {
    identifier: { phone: phoneUpsert },
    input: {
      phone: phoneUpsert,
      firstName: 'Hermes',
      lastName: 'SetPhone',
      note: 'customerSet phone upsert parity probe',
      tags: ['set', 'phone'],
    },
  };
  const phoneUpsertResult = await runCustomerSet(phoneUpsertVariables);
  assertNoTopLevelErrors(phoneUpsertResult, 'customerSet upsert by phone');
  const phoneUpsertedCustomerId = phoneUpsertResult.payload?.data?.customerSet?.customer?.id;
  if (typeof phoneUpsertedCustomerId !== 'string' || !phoneUpsertedCustomerId) {
    throw new Error(
      `customerSet phone upsert did not return a customer id: ${JSON.stringify(phoneUpsertResult.payload, null, 2)}`,
    );
  }
  cleanupIds.add(phoneUpsertedCustomerId);

  const phoneUpdateVariables = {
    identifier: { phone: phoneUpsert },
    input: {
      phone: phoneUpsert,
      firstName: 'Hermes',
      lastName: 'SetPhoneUpdated',
      note: 'customerSet phone update parity probe',
      tags: ['set', 'phone-updated'],
    },
  };
  const phoneUpdateResult = await runCustomerSet(phoneUpdateVariables);
  assertNoTopLevelErrors(phoneUpdateResult, 'customerSet update by phone');

  const multiAddressVariables = {
    identifier: { id: createdCustomerId },
    input: {
      email: createdEmail,
      addresses: [
        { address1: '20 Set St', city: 'Ottawa', countryCode: 'CA', provinceCode: 'ON', zip: 'K1A 0B2' },
        { address1: '21 Set St', city: 'Toronto', countryCode: 'CA', provinceCode: 'ON', zip: 'M5H 2N3' },
      ],
    },
  };
  const multiAddressResult = await runCustomerSet(multiAddressVariables);
  assertNoTopLevelErrors(multiAddressResult, 'customerSet multi-address replacement');

  const duplicateEmailVariables = {
    input: {
      email: createdEmail,
      firstName: 'Hermes',
      lastName: 'DuplicateEmail',
    },
  };
  const duplicateEmailResult = await runCustomerSet(duplicateEmailVariables);
  assertNoTopLevelErrors(duplicateEmailResult, 'customerSet duplicate email create validation');
  const duplicateEmailCustomerId = duplicateEmailResult.payload?.data?.customerSet?.customer?.id;
  if (typeof duplicateEmailCustomerId === 'string' && duplicateEmailCustomerId) {
    cleanupIds.add(duplicateEmailCustomerId);
  }

  const duplicatePhoneVariables = {
    input: {
      phone: phoneUpsert,
      firstName: 'Hermes',
      lastName: 'DuplicatePhone',
    },
  };
  const duplicatePhoneResult = await runCustomerSet(duplicatePhoneVariables);
  assertNoTopLevelErrors(duplicatePhoneResult, 'customerSet duplicate phone create validation');
  const duplicatePhoneCustomerId = duplicatePhoneResult.payload?.data?.customerSet?.customer?.id;
  if (typeof duplicatePhoneCustomerId === 'string' && duplicatePhoneCustomerId) {
    cleanupIds.add(duplicatePhoneCustomerId);
  }

  const missingIdentifierEmailVariables = {
    identifier: { email: createdEmail },
    input: { firstName: 'Hermes', lastName: 'MissingIdentifierEmail' },
  };
  const missingIdentifierEmail = await runCustomerSet(missingIdentifierEmailVariables);
  assertNoTopLevelErrors(missingIdentifierEmail, 'customerSet missing identifier email validation');

  const mismatchedIdentifierEmailVariables = {
    identifier: { email: createdEmail },
    input: { email: upsertedEmail, firstName: 'Hermes', lastName: 'MismatchedIdentifierEmail' },
  };
  const mismatchedIdentifierEmail = await runCustomerSet(mismatchedIdentifierEmailVariables);
  assertNoTopLevelErrors(mismatchedIdentifierEmail, 'customerSet mismatched identifier email validation');

  const nullAddressListVariables = {
    identifier: { id: createdCustomerId },
    input: { email: createdEmail, addresses: null },
  };
  const nullAddressList = await runCustomerSet(nullAddressListVariables);
  assertNoTopLevelErrors(nullAddressList, 'customerSet null address list validation');

  const nullableUpdateVariables = {
    identifier: { id: upsertedCustomerId },
    input: {
      email: null,
      firstName: null,
      lastName: null,
      locale: null,
      note: null,
      phone: null,
      tags: null,
      taxExempt: null,
      taxExemptions: null,
    },
  };
  const nullableUpdate = await runCustomerSet(nullableUpdateVariables);
  assertNoTopLevelErrors(nullableUpdate, 'customerSet nullable update validation');

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

  const inputId = 'gid://shopify/Customer/999999999999998';
  const idNotAllowedByIdVariables = {
    identifier: { id: createdCustomerId },
    input: {
      id: inputId,
      email: `customer-set-id-not-allowed-id-${stamp}@example.com`,
      firstName: 'IdNotAllowed',
    },
  };
  const idNotAllowedById = await runCustomerSetIdNotAllowed(idNotAllowedByIdVariables);
  assertIdNotAllowed(idNotAllowedById, 'customerSet input.id with identifier.id validation');

  const idNotAllowedByEmailVariables = {
    identifier: { email: `customer-set-id-not-allowed-email-${stamp}@example.com` },
    input: {
      id: inputId,
      email: `customer-set-id-not-allowed-email-${stamp}@example.com`,
      firstName: 'IdNotAllowed',
    },
  };
  const idNotAllowedByEmail = await runCustomerSetIdNotAllowed(idNotAllowedByEmailVariables);
  assertIdNotAllowed(idNotAllowedByEmail, 'customerSet input.id with identifier.email validation');

  const idNotAllowedByPhoneVariables = {
    identifier: { phone: `+1${String(stamp + 3).slice(-10)}` },
    input: {
      id: inputId,
      phone: `+1${String(stamp + 3).slice(-10)}`,
      firstName: 'IdNotAllowed',
    },
  };
  const idNotAllowedByPhone = await runCustomerSetIdNotAllowed(idNotAllowedByPhoneVariables);
  assertIdNotAllowed(idNotAllowedByPhone, 'customerSet input.id with identifier.phone validation');

  const idNotAllowedByCustomIdVariables = {
    identifier: {
      customId: { namespace: 'custom', key: 'external_id', value: `customer-set-id-not-allowed-${stamp}` },
    },
    input: {
      id: inputId,
      firstName: 'IdNotAllowed',
    },
  };
  const idNotAllowedByCustomId = await runCustomerSetIdNotAllowed(idNotAllowedByCustomIdVariables);
  assertIdNotAllowed(idNotAllowedByCustomId, 'customerSet input.id with identifier.customId validation');

  const deletedIdentifierCleanup = await runGraphql(deleteMutation, { input: { id: phoneUpsertedCustomerId } });
  cleanupIds.delete(phoneUpsertedCustomerId);
  const deletedIdentifierVariables = {
    identifier: { id: phoneUpsertedCustomerId },
    input: { firstName: 'Hermes', lastName: 'DeletedIdentifier' },
  };
  const deletedIdentifier = await runCustomerSet(deletedIdentifierVariables);
  assertNoTopLevelErrors(deletedIdentifier, 'customerSet deleted id validation');

  const cleanup = [];
  for (const id of cleanupIds) {
    cleanup.push(await runGraphql(deleteMutation, { input: { id } }));
  }

  const capture = {
    expectedEmptyMutationLog: [],
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
    phoneUpsert: {
      variables: phoneUpsertVariables,
      response: phoneUpsertResult.payload,
    },
    phoneUpdate: {
      variables: phoneUpdateVariables,
      response: phoneUpdateResult.payload,
    },
    multiAddressReplacement: {
      variables: multiAddressVariables,
      response: multiAddressResult.payload,
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
      idNotAllowed: {
        byId: {
          variables: idNotAllowedByIdVariables,
          response: idNotAllowedById.payload,
        },
        byEmail: {
          variables: idNotAllowedByEmailVariables,
          response: idNotAllowedByEmail.payload,
        },
        byPhone: {
          variables: idNotAllowedByPhoneVariables,
          response: idNotAllowedByPhone.payload,
        },
        byCustomId: {
          variables: idNotAllowedByCustomIdVariables,
          response: idNotAllowedByCustomId.payload,
        },
      },
      duplicateEmail: {
        variables: duplicateEmailVariables,
        response: duplicateEmailResult.payload,
      },
      duplicatePhone: {
        variables: duplicatePhoneVariables,
        response: duplicatePhoneResult.payload,
      },
      missingIdentifierEmail: {
        variables: missingIdentifierEmailVariables,
        response: missingIdentifierEmail.payload,
      },
      mismatchedIdentifierEmail: {
        variables: mismatchedIdentifierEmailVariables,
        response: mismatchedIdentifierEmail.payload,
      },
      nullAddressList: {
        variables: nullAddressListVariables,
        response: nullAddressList.payload,
      },
      nullableUpdate: {
        variables: nullableUpdateVariables,
        response: nullableUpdate.payload,
      },
      deletedIdentifier: {
        cleanup: deletedIdentifierCleanup.payload,
        variables: deletedIdentifierVariables,
        response: deletedIdentifier.payload,
      },
    },
    cleanup: cleanup.map((result) => result.payload),
  };

  const outputPath = path.join(outputDir, 'customer-set-parity.json');
  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
}

await main();
