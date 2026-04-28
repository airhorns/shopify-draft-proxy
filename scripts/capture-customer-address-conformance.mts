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

const customerSetMutation = `#graphql
  mutation CustomerAddressCustomerSet($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
    customerSet(identifier: $identifier, input: $input) {
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

function summarizeAddressAttempt(result) {
  return {
    status: result.status,
    response: result.payload,
  };
}

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
  const cleanupCustomerIds = new Set();

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
    cleanupCustomerIds.add(createdCustomerId);

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

    const createThirdAddressVariables = {
      customerId: createdCustomerId,
      address: {
        address1: '3 Null Default St',
        city: 'Vancouver',
        countryCode: 'CA',
        provinceCode: 'BC',
        zip: 'V6B 1A1',
      },
    };
    const createThirdAddress = await runGraphql(createAddressMutation, createThirdAddressVariables);
    assertNoTopLevelErrors(createThirdAddress, 'customerAddressCreate setAsDefault omitted');
    const thirdAddressId = createThirdAddress.payload?.data?.customerAddressCreate?.address?.id;

    const createDuplicateAddressVariables = {
      customerId: createdCustomerId,
      address: createThirdAddressVariables.address,
      setAsDefault: false,
    };
    const createDuplicateAddress = await runGraphql(createAddressMutation, createDuplicateAddressVariables);
    assertNoTopLevelErrors(createDuplicateAddress, 'customerAddressCreate duplicate');

    const orderingRead = await runGraphql(downstreamReadQuery, downstreamReadVariables);
    assertNoTopLevelErrors(orderingRead, 'customer address ordering read');

    const deleteDefaultAddressVariables = {
      customerId: createdCustomerId,
      addressId: secondAddressId,
    };
    const deleteDefaultAddress = await runGraphql(deleteAddressMutation, deleteDefaultAddressVariables);
    assertNoTopLevelErrors(deleteDefaultAddress, 'customerAddressDelete default address');

    const deleteDefaultRead = await runGraphql(downstreamReadQuery, downstreamReadVariables);
    assertNoTopLevelErrors(deleteDefaultRead, 'customer address delete-default downstream read');

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

    const validationEmail = `hermes-address-validation-${stamp}@example.com`;
    const createValidationCustomerVariables = {
      input: {
        email: validationEmail,
        firstName: 'Hermes',
        lastName: 'AddressValidation',
      },
    };
    const createValidationCustomer = await runGraphql(createCustomerMutation, createValidationCustomerVariables);
    assertNoTopLevelErrors(createValidationCustomer, 'customerCreate for address validations');
    const validationCustomerId = createValidationCustomer.payload?.data?.customerCreate?.customer?.id;
    if (typeof validationCustomerId !== 'string' || !validationCustomerId) {
      throw new Error(
        `validation customerCreate did not return id: ${JSON.stringify(createValidationCustomer.payload, null, 2)}`,
      );
    }
    cleanupCustomerIds.add(validationCustomerId);

    const crossOwnerEmail = `hermes-address-cross-owner-${stamp}@example.com`;
    const createCrossOwnerCustomerVariables = {
      input: {
        email: crossOwnerEmail,
        firstName: 'Hermes',
        lastName: 'AddressCrossOwner',
      },
    };
    const createCrossOwnerCustomer = await runGraphql(createCustomerMutation, createCrossOwnerCustomerVariables);
    assertNoTopLevelErrors(createCrossOwnerCustomer, 'customerCreate for cross-owner address');
    const crossOwnerCustomerId = createCrossOwnerCustomer.payload?.data?.customerCreate?.customer?.id;
    if (typeof crossOwnerCustomerId !== 'string' || !crossOwnerCustomerId) {
      throw new Error(
        `cross-owner customerCreate did not return id: ${JSON.stringify(createCrossOwnerCustomer.payload, null, 2)}`,
      );
    }
    cleanupCustomerIds.add(crossOwnerCustomerId);

    const crossOwnerAddressVariables = {
      customerId: crossOwnerCustomerId,
      address: {
        address1: '4 Cross Owner St',
        city: 'Ottawa',
        countryCode: 'CA',
        provinceCode: 'ON',
        zip: 'K1A 0B1',
      },
      setAsDefault: true,
    };
    const crossOwnerAddress = await runGraphql(createAddressMutation, crossOwnerAddressVariables);
    assertNoTopLevelErrors(crossOwnerAddress, 'customerAddressCreate cross-owner source');
    const crossOwnerAddressId = crossOwnerAddress.payload?.data?.customerAddressCreate?.address?.id;
    if (typeof crossOwnerAddressId !== 'string' || !crossOwnerAddressId) {
      throw new Error(
        `cross-owner address create did not return id: ${JSON.stringify(crossOwnerAddress.payload, null, 2)}`,
      );
    }

    const blankAddressCreateVariables = {
      customerId: validationCustomerId,
      address: {},
      setAsDefault: true,
    };
    const blankAddressCreate = await runGraphql(createAddressMutation, blankAddressCreateVariables);

    const blankStringAddressCreateVariables = {
      customerId: validationCustomerId,
      address: {
        firstName: '',
        lastName: '',
        address1: '',
        city: '',
        countryCode: 'CA',
        provinceCode: '',
        zip: '',
      },
      setAsDefault: false,
    };
    const blankStringAddressCreate = await runGraphql(createAddressMutation, blankStringAddressCreateVariables);

    const invalidProvinceCreateVariables = {
      customerId: validationCustomerId,
      address: {
        address1: '5 Invalid Province St',
        city: 'Ottawa',
        countryCode: 'CA',
        provinceCode: 'ZZ',
        zip: 'K1A 0B1',
      },
      setAsDefault: false,
    };
    const invalidProvinceCreate = await runGraphql(createAddressMutation, invalidProvinceCreateVariables);

    const invalidCountryCreateVariables = {
      customerId: validationCustomerId,
      address: {
        address1: '6 Invalid Country St',
        city: 'Nowhere',
        countryCode: 'ZZ',
        provinceCode: 'ZZ',
        zip: '00000',
      },
      setAsDefault: false,
    };
    const invalidCountryCreate = await runGraphql(createAddressMutation, invalidCountryCreateVariables);

    const invalidPostalCreateVariables = {
      customerId: validationCustomerId,
      address: {
        address1: '7 Postal St',
        city: 'Ottawa',
        countryCode: 'CA',
        provinceCode: 'ON',
        zip: 'not-a-postal-code',
      },
      setAsDefault: false,
    };
    const invalidPostalCreate = await runGraphql(createAddressMutation, invalidPostalCreateVariables);

    const crossCustomerUpdate = await runGraphql(updateAddressMutation, {
      customerId: validationCustomerId,
      addressId: crossOwnerAddressId,
      address: { city: 'Cross Customer' },
      setAsDefault: false,
    });
    const crossCustomerDefault = await runGraphql(defaultAddressMutation, {
      customerId: validationCustomerId,
      addressId: crossOwnerAddressId,
    });
    const crossCustomerDelete = await runGraphql(deleteAddressMutation, {
      customerId: validationCustomerId,
      addressId: crossOwnerAddressId,
    });

    const customerSetValidAddressesVariables = {
      identifier: { id: validationCustomerId },
      input: {
        email: validationEmail,
        addresses: [
          {
            address1: '8 CustomerSet St',
            city: 'Toronto',
            countryCode: 'CA',
            provinceCode: 'ON',
            zip: 'M5H 2N2',
          },
          {
            address1: '8 CustomerSet St',
            city: 'Toronto',
            countryCode: 'CA',
            provinceCode: 'ON',
            zip: 'M5H 2N2',
          },
        ],
      },
    };
    const customerSetValidAddresses = await runGraphql(customerSetMutation, customerSetValidAddressesVariables);
    assertHttpOk(customerSetValidAddresses, 'customerSet duplicate address replacement');

    const customerSetBlankAddressesVariables = {
      identifier: { id: validationCustomerId },
      input: {
        email: validationEmail,
        addresses: [{}],
      },
    };
    const customerSetBlankAddresses = await runGraphql(customerSetMutation, customerSetBlankAddressesVariables);
    assertHttpOk(customerSetBlankAddresses, 'customerSet blank address replacement');

    const maximumCustomerEmail = `hermes-address-maximum-${stamp}@example.com`;
    const createMaximumCustomerVariables = {
      input: {
        email: maximumCustomerEmail,
        firstName: 'Hermes',
        lastName: 'AddressMaximum',
      },
    };
    const createMaximumCustomer = await runGraphql(createCustomerMutation, createMaximumCustomerVariables);
    assertNoTopLevelErrors(createMaximumCustomer, 'customerCreate for maximum address probe');
    const maximumCustomerId = createMaximumCustomer.payload?.data?.customerCreate?.customer?.id;
    if (typeof maximumCustomerId !== 'string' || !maximumCustomerId) {
      throw new Error(
        `maximum customerCreate did not return id: ${JSON.stringify(createMaximumCustomer.payload, null, 2)}`,
      );
    }
    cleanupCustomerIds.add(maximumCustomerId);

    const maximumAddressProbe = {
      attempted: 0,
      successCount: 0,
      firstFailure: null,
      lastSuccess: null,
    };
    for (let index = 0; index < 105; index += 1) {
      const maximumAddressVariables = {
        customerId: maximumCustomerId,
        address: {
          address1: `${index} Maximum Address St`,
          city: 'Ottawa',
          countryCode: 'CA',
          provinceCode: 'ON',
          zip: 'K1A 0B1',
        },
        setAsDefault: index === 0,
      };
      const maximumAddressCreate = await runGraphql(createAddressMutation, maximumAddressVariables);
      maximumAddressProbe.attempted += 1;
      const userErrors = maximumAddressCreate.payload?.data?.customerAddressCreate?.userErrors ?? [];
      if (
        maximumAddressCreate.status >= 200 &&
        maximumAddressCreate.status < 300 &&
        !maximumAddressCreate.payload?.errors &&
        Array.isArray(userErrors) &&
        userErrors.length === 0
      ) {
        maximumAddressProbe.successCount += 1;
        maximumAddressProbe.lastSuccess = {
          index,
          response: maximumAddressCreate.payload,
        };
        continue;
      }

      maximumAddressProbe.firstFailure = {
        index,
        variables: maximumAddressVariables,
        status: maximumAddressCreate.status,
        response: maximumAddressCreate.payload,
      };
      break;
    }

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
      createThirdAddressOmittedSetAsDefault: {
        variables: createThirdAddressVariables,
        response: createThirdAddress.payload,
        createdAddressId: thirdAddressId,
      },
      createDuplicateAddress: {
        variables: createDuplicateAddressVariables,
        response: createDuplicateAddress.payload,
      },
      orderingRead: {
        variables: downstreamReadVariables,
        response: orderingRead.payload,
      },
      deleteDefaultAddress: {
        variables: deleteDefaultAddressVariables,
        response: deleteDefaultAddress.payload,
      },
      deleteDefaultRead: {
        variables: downstreamReadVariables,
        response: deleteDefaultRead.payload,
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
        blankAddressCreate: {
          variables: blankAddressCreateVariables,
          ...summarizeAddressAttempt(blankAddressCreate),
        },
        blankStringAddressCreate: {
          variables: blankStringAddressCreateVariables,
          ...summarizeAddressAttempt(blankStringAddressCreate),
        },
        invalidProvinceCreate: {
          variables: invalidProvinceCreateVariables,
          ...summarizeAddressAttempt(invalidProvinceCreate),
        },
        invalidCountryCreate: {
          variables: invalidCountryCreateVariables,
          ...summarizeAddressAttempt(invalidCountryCreate),
        },
        invalidPostalCreate: {
          variables: invalidPostalCreateVariables,
          ...summarizeAddressAttempt(invalidPostalCreate),
        },
        crossCustomerUpdate: {
          variables: {
            customerId: validationCustomerId,
            addressId: crossOwnerAddressId,
            address: { city: 'Cross Customer' },
            setAsDefault: false,
          },
          ...summarizeAddressAttempt(crossCustomerUpdate),
        },
        crossCustomerDefault: {
          variables: {
            customerId: validationCustomerId,
            addressId: crossOwnerAddressId,
          },
          ...summarizeAddressAttempt(crossCustomerDefault),
        },
        crossCustomerDelete: {
          variables: {
            customerId: validationCustomerId,
            addressId: crossOwnerAddressId,
          },
          ...summarizeAddressAttempt(crossCustomerDelete),
        },
        customerSetValidAddresses: {
          variables: customerSetValidAddressesVariables,
          ...summarizeAddressAttempt(customerSetValidAddresses),
        },
        customerSetBlankAddresses: {
          variables: customerSetBlankAddressesVariables,
          ...summarizeAddressAttempt(customerSetBlankAddresses),
        },
        maximumAddressProbe,
      },
    };

    const outputPath = path.join(outputDir, 'customer-address-lifecycle.json');
    await writeFile(outputPath, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    console.log(`Wrote ${outputPath}`);
  } finally {
    if (createdCustomerId) {
      cleanupCustomerIds.add(createdCustomerId);
    }
    for (const customerId of cleanupCustomerIds) {
      const cleanup = await runGraphql(deleteCustomerMutation, { input: { id: customerId } });
      if (cleanup.status < 200 || cleanup.status >= 300 || cleanup.payload?.errors) {
        console.error(`Customer cleanup failed for ${customerId}: ${JSON.stringify(cleanup, null, 2)}`);
      }
    }
  }
}

await main();
