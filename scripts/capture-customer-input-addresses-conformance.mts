// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
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

const addressSlice = `
  id
  firstName
  lastName
  address1
  address2
  city
  company
  country
  countryCodeV2
  province
  provinceCode
  zip
  phone
  name
  formattedArea
`;

const customerSlice = `
  id
  email
  firstName
  lastName
  displayName
  defaultAddress {
    ${addressSlice}
  }
  addressesV2(first: 5) {
    nodes {
      ${addressSlice}
    }
    edges {
      cursor
      node {
        id
        city
      }
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
`;

const createMutation = `#graphql
  mutation CustomerInputAddressesCreate($input: CustomerInput!) {
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
  mutation CustomerInputAddressesUpdate($input: CustomerInput!) {
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

const downstreamReadQuery = `#graphql
  query CustomerInputAddressesDownstream($id: ID!, $identifier: CustomerIdentifierInput!) {
    customer(id: $id) {
      ${customerSlice}
    }
    customerByIdentifier(identifier: $identifier) {
      ${customerSlice}
    }
  }
`;

const deleteMutation = `#graphql
  mutation CustomerInputAddressesDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

async function main() {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const email = `hermes-input-addresses-${stamp}@example.com`;
  let createdCustomerId = null;

  try {
    const createVariables = {
      input: {
        email,
        firstName: 'Hermes',
        lastName: 'InputAddresses',
        addresses: [
          {
            firstName: 'Hermes',
            lastName: 'Primary',
            address1: '10 Input Create St',
            city: 'Ottawa',
            countryCode: 'CA',
            provinceCode: 'ON',
            zip: 'K1A 0B1',
            phone: '+14155550123',
          },
          {
            firstName: 'Hermes',
            lastName: 'Secondary',
            address1: '11 Input Create Ave',
            city: 'Toronto',
            countryCode: 'CA',
            provinceCode: 'ON',
            zip: 'M5H 2N2',
          },
        ],
      },
    };
    const create = await runGraphql(createMutation, createVariables);
    assertNoTopLevelErrors(create, 'customerCreate CustomerInput.addresses');
    createdCustomerId = create.payload?.data?.customerCreate?.customer?.id;
    if (typeof createdCustomerId !== 'string' || !createdCustomerId) {
      throw new Error(`customerCreate did not return id: ${JSON.stringify(create.payload, null, 2)}`);
    }

    const updateAddress = {
      firstName: 'Hermes',
      lastName: 'Replacement',
      address1: '20 Input Update Blvd',
      city: 'Vancouver',
      countryCode: 'CA',
      provinceCode: 'BC',
      zip: 'V6B 1A1',
      phone: '+14155550124',
    };
    const updateVariables = {
      input: {
        id: createdCustomerId,
        addresses: [updateAddress, updateAddress],
      },
    };
    const update = await runGraphql(updateMutation, updateVariables);
    assertNoTopLevelErrors(update, 'customerUpdate CustomerInput.addresses');

    const downstreamReadVariables = {
      id: createdCustomerId,
      identifier: { emailAddress: email },
    };
    const downstreamRead = await runGraphql(downstreamReadQuery, downstreamReadVariables);
    assertNoTopLevelErrors(downstreamRead, 'CustomerInput.addresses downstream read');

    const result = {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      create: {
        variables: createVariables,
        response: create.payload,
      },
      update: {
        variables: updateVariables,
        response: update.payload,
      },
      downstreamRead: {
        variables: downstreamReadVariables,
        response: downstreamRead.payload,
      },
    };

    const outputPath = path.join(outputDir, 'customer-input-addresses-parity.json');
    await writeFile(outputPath, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    console.log(`Wrote ${outputPath}`);
  } finally {
    if (createdCustomerId) {
      const cleanup = await runGraphql(deleteMutation, { input: { id: createdCustomerId } });
      if (cleanup.status < 200 || cleanup.status >= 300 || cleanup.payload?.errors) {
        console.error(`Customer cleanup failed for ${createdCustomerId}: ${JSON.stringify(cleanup, null, 2)}`);
      }
    }
  }
}

await main();
