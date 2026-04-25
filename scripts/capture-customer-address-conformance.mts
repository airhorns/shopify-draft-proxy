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

function assertHttpOk(result, context) {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

const customerSlice = `
  id
  email
  firstName
  lastName
  displayName
  defaultAddress {
    id
    address1
    city
    country
    countryCodeV2
    provinceCode
    zip
    formattedArea
  }
  addressesV2(first: 5) {
    nodes {
      id
      address1
      city
      country
      countryCodeV2
      provinceCode
      zip
      formattedArea
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

const createCustomerMutation = `#graphql
  mutation CustomerAddressLifecycleCreateCustomer($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        id
        email
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const createAddressMutation = `#graphql
  mutation CustomerAddressCreate($customerId: ID!, $address: MailingAddressInput!, $setAsDefault: Boolean) {
    customerAddressCreate(customerId: $customerId, address: $address, setAsDefault: $setAsDefault) {
      address {
        ${addressSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const updateAddressMutation = `#graphql
  mutation CustomerAddressUpdate(
    $customerId: ID!
    $addressId: ID!
    $address: MailingAddressInput!
    $setAsDefault: Boolean
  ) {
    customerAddressUpdate(
      customerId: $customerId
      addressId: $addressId
      address: $address
      setAsDefault: $setAsDefault
    ) {
      address {
        ${addressSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteAddressMutation = `#graphql
  mutation CustomerAddressDelete($customerId: ID!, $addressId: ID!) {
    customerAddressDelete(customerId: $customerId, addressId: $addressId) {
      deletedAddressId
      userErrors {
        field
        message
      }
    }
  }
`;

const defaultAddressMutation = `#graphql
  mutation CustomerUpdateDefaultAddress($customerId: ID!, $addressId: ID!) {
    customerUpdateDefaultAddress(customerId: $customerId, addressId: $addressId) {
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
  query CustomerAddressLifecycleDownstream($id: ID!, $identifier: CustomerIdentifierInput!, $query: String!) {
    customer(id: $id) {
      ${customerSlice}
    }
    customerByIdentifier(identifier: $identifier) {
      ${customerSlice}
    }
    customers(first: 5, query: $query, sortKey: UPDATED_AT, reverse: true) {
      nodes {
        ${customerSlice}
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const deleteCustomerMutation = `#graphql
  mutation CustomerAddressLifecycleDeleteCustomer($input: CustomerDeleteInput!) {
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
  const email = `hermes-address-${stamp}@example.com`;
  let createdCustomerId = null;

  try {
    const createCustomerVariables = {
      input: {
        email,
        firstName: 'Hermes',
        lastName: 'Address',
        tags: ['parity', `address-${stamp}`],
      },
    };
    const createCustomer = await runGraphql(createCustomerMutation, createCustomerVariables);
    assertNoTopLevelErrors(createCustomer, 'customerCreate for address lifecycle');
    createdCustomerId = createCustomer.payload?.data?.customerCreate?.customer?.id;
    if (typeof createdCustomerId !== 'string' || !createdCustomerId) {
      throw new Error(`customerCreate did not return id: ${JSON.stringify(createCustomer.payload, null, 2)}`);
    }

    const createFirstAddressVariables = {
      customerId: createdCustomerId,
      address: {
        firstName: 'Hermes',
        lastName: 'Default',
        address1: '1 Main St',
        city: 'Ottawa',
        countryCode: 'CA',
        provinceCode: 'ON',
        zip: 'K1A 0B1',
        phone: '+14155550123',
      },
      setAsDefault: true,
    };
    const createFirstAddress = await runGraphql(createAddressMutation, createFirstAddressVariables);
    assertNoTopLevelErrors(createFirstAddress, 'customerAddressCreate first');
    const firstAddressId = createFirstAddress.payload?.data?.customerAddressCreate?.address?.id;
    if (typeof firstAddressId !== 'string' || !firstAddressId) {
      throw new Error(
        `customerAddressCreate first did not return id: ${JSON.stringify(createFirstAddress.payload, null, 2)}`,
      );
    }

    const createSecondAddressVariables = {
      customerId: createdCustomerId,
      address: {
        address1: '2 Side St',
        city: 'Toronto',
        countryCode: 'CA',
        provinceCode: 'ON',
        zip: 'M5H 2N2',
      },
      setAsDefault: false,
    };
    const createSecondAddress = await runGraphql(createAddressMutation, createSecondAddressVariables);
    assertNoTopLevelErrors(createSecondAddress, 'customerAddressCreate second');
    const secondAddressId = createSecondAddress.payload?.data?.customerAddressCreate?.address?.id;
    if (typeof secondAddressId !== 'string' || !secondAddressId) {
      throw new Error(
        `customerAddressCreate second did not return id: ${JSON.stringify(createSecondAddress.payload, null, 2)}`,
      );
    }

    const updateSecondAddressVariables = {
      customerId: createdCustomerId,
      addressId: secondAddressId,
      address: {
        city: 'Montreal',
        provinceCode: 'QC',
        zip: 'H2Y 1C6',
      },
      setAsDefault: false,
    };
    const updateSecondAddress = await runGraphql(updateAddressMutation, updateSecondAddressVariables);
    assertNoTopLevelErrors(updateSecondAddress, 'customerAddressUpdate');

    const defaultAddressVariables = {
      customerId: createdCustomerId,
      addressId: secondAddressId,
    };
    const defaultAddress = await runGraphql(defaultAddressMutation, defaultAddressVariables);
    assertNoTopLevelErrors(defaultAddress, 'customerUpdateDefaultAddress');

    const deleteFirstAddressVariables = {
      customerId: createdCustomerId,
      addressId: firstAddressId,
    };
    const deleteFirstAddress = await runGraphql(deleteAddressMutation, deleteFirstAddressVariables);
    assertNoTopLevelErrors(deleteFirstAddress, 'customerAddressDelete');

    const downstreamReadVariables = {
      id: createdCustomerId,
      identifier: { emailAddress: email },
      query: `email:${email}`,
    };
    const downstreamRead = await runGraphql(downstreamReadQuery, downstreamReadVariables);
    assertNoTopLevelErrors(downstreamRead, 'customer address downstream read');

    const unknownCustomerVariables = {
      customerId: 'gid://shopify/Customer/999999999999999',
      address: createFirstAddressVariables.address,
      setAsDefault: true,
    };
    const unknownCustomerCreate = await runGraphql(createAddressMutation, unknownCustomerVariables);
    assertHttpOk(unknownCustomerCreate, 'customerAddressCreate unknown customer validation');

    const unknownAddressVariables = {
      customerId: createdCustomerId,
      addressId: 'gid://shopify/MailingAddress/999999999999999',
      address: { city: 'Ghost' },
      setAsDefault: false,
    };
    const unknownAddressUpdate = await runGraphql(updateAddressMutation, unknownAddressVariables);
    assertHttpOk(unknownAddressUpdate, 'customerAddressUpdate unknown address validation');

    const unknownDefaultAddress = await runGraphql(defaultAddressMutation, {
      customerId: createdCustomerId,
      addressId: 'gid://shopify/MailingAddress/999999999999999',
    });
    assertHttpOk(unknownDefaultAddress, 'customerUpdateDefaultAddress unknown address validation');

    const unknownAddressDelete = await runGraphql(deleteAddressMutation, {
      customerId: createdCustomerId,
      addressId: 'gid://shopify/MailingAddress/999999999999999',
    });
    assertHttpOk(unknownAddressDelete, 'customerAddressDelete unknown address validation');

    const result = {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      createCustomer: {
        variables: createCustomerVariables,
        response: createCustomer.payload,
      },
      createFirstAddress: {
        variables: createFirstAddressVariables,
        response: createFirstAddress.payload,
      },
      createSecondAddress: {
        variables: createSecondAddressVariables,
        response: createSecondAddress.payload,
      },
      updateSecondAddress: {
        variables: updateSecondAddressVariables,
        response: updateSecondAddress.payload,
      },
      defaultAddress: {
        variables: defaultAddressVariables,
        response: defaultAddress.payload,
      },
      deleteFirstAddress: {
        variables: deleteFirstAddressVariables,
        response: deleteFirstAddress.payload,
      },
      downstreamRead: {
        variables: downstreamReadVariables,
        response: downstreamRead.payload,
      },
      validations: {
        unknownCustomerCreate: {
          variables: unknownCustomerVariables,
          response: unknownCustomerCreate.payload,
        },
        unknownAddressUpdate: {
          variables: unknownAddressVariables,
          response: unknownAddressUpdate.payload,
        },
        unknownDefaultAddress: {
          variables: {
            customerId: createdCustomerId,
            addressId: 'gid://shopify/MailingAddress/999999999999999',
          },
          response: unknownDefaultAddress.payload,
        },
        unknownAddressDelete: {
          variables: {
            customerId: createdCustomerId,
            addressId: 'gid://shopify/MailingAddress/999999999999999',
          },
          response: unknownAddressDelete.payload,
        },
      },
    };

    const outputPath = path.join(outputDir, 'customer-address-lifecycle.json');
    await writeFile(outputPath, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    console.log(`Wrote ${outputPath}`);
  } finally {
    if (createdCustomerId) {
      const cleanup = await runGraphql(deleteCustomerMutation, { input: { id: createdCustomerId } });
      if (cleanup.status < 200 || cleanup.status >= 300 || cleanup.payload?.errors) {
        console.error(`Customer cleanup failed: ${JSON.stringify(cleanup, null, 2)}`);
      }
    }
  }
}

await main();
