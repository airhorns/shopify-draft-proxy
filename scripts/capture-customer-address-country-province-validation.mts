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

const addressSlice = `
  id
  address1
  city
  country
  countryCodeV2
  province
  provinceCode
  zip
  formattedArea
`;

const createCustomerMutation = `#graphql
  mutation CustomerAddressCountryProvinceCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        id
        email
        defaultAddress {
          ${addressSlice}
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const createAddressMutation = `#graphql
  mutation CustomerAddressCountryProvinceAddressCreate($customerId: ID!, $address: MailingAddressInput!, $setAsDefault: Boolean) {
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
  mutation CustomerAddressCountryProvinceAddressUpdate($customerId: ID!, $addressId: ID!, $address: MailingAddressInput!, $setAsDefault: Boolean) {
    customerAddressUpdate(customerId: $customerId, addressId: $addressId, address: $address, setAsDefault: $setAsDefault) {
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

const updateCustomerMutation = `#graphql
  mutation CustomerAddressCountryProvinceUpdate($input: CustomerInput!) {
    customerUpdate(input: $input) {
      customer {
        id
        email
        defaultAddress {
          ${addressSlice}
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const customerSetMutation = `#graphql
  mutation CustomerAddressCountryProvinceSet($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
    customerSet(identifier: $identifier, input: $input) {
      customer {
        id
        email
        defaultAddress {
          ${addressSlice}
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteCustomerMutation = `#graphql
  mutation CustomerAddressCountryProvinceDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

function assertHttpOk(result, context) {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function customerIdFrom(result, rootName) {
  const id = result.payload?.data?.[rootName]?.customer?.id;
  return typeof id === 'string' && id ? id : null;
}

async function deleteCustomer(id) {
  const result = await runGraphql(deleteCustomerMutation, { input: { id } });
  assertHttpOk(result, `delete ${id}`);
  return result;
}

const timestamp = Date.now();
const createdCustomerIds = new Set();

const validControlVariables = {
  input: {
    email: `har776-address-valid-${timestamp}@example.com`,
    addresses: [
      {
        address1: '1 Valid St',
        city: 'San Francisco',
        countryCode: 'US',
        country: 'Canada',
        provinceCode: 'CA',
        zip: '94105',
      },
    ],
  },
};

const customerCreateUnknownCountryVariables = {
  input: {
    email: `har776-address-unknown-country-${timestamp}@example.com`,
    addresses: [{ address1: '2 Unknown Country', city: 'Nowhere', country: 'Atlantis' }],
  },
};

const customerCreateDisplayConflictVariables = {
  input: {
    email: `har776-address-display-conflict-${timestamp}@example.com`,
    addresses: [{ address1: '3 Display Conflict', city: 'San Francisco', countryCode: 'US', country: 'Canada' }],
  },
};

const customerCreateNoZoneProvinceVariables = {
  input: {
    email: `har776-address-no-zone-${timestamp}@example.com`,
    addresses: [{ address1: '4 No Zone', city: 'Singapore', countryCode: 'SG', provinceCode: 'ON' }],
  },
};

let validControl;
let validAddressId;
let customerCreateUnknownCountry;
let customerCreateDisplayConflict;
let customerCreateNoZoneProvince;
let addressCreateUnknownCountry;
let addressCreateWrongProvinceCountry;
let addressUpdateWrongProvinceCountry;
let customerUpdateWrongProvinceCountry;
let customerSetWrongProvinceCountry;
let cleanupResults = [];

try {
  validControl = await runGraphql(createCustomerMutation, validControlVariables);
  assertHttpOk(validControl, 'valid control customerCreate');
  const primaryCustomerId = customerIdFrom(validControl, 'customerCreate');
  if (primaryCustomerId === null) {
    throw new Error(`valid control did not create a customer: ${JSON.stringify(validControl, null, 2)}`);
  }
  createdCustomerIds.add(primaryCustomerId);
  validAddressId = validControl.payload?.data?.customerCreate?.customer?.defaultAddress?.id;
  if (typeof validAddressId !== 'string' || !validAddressId) {
    throw new Error(`valid control did not create a default address: ${JSON.stringify(validControl, null, 2)}`);
  }

  addressCreateUnknownCountry = await runGraphql(createAddressMutation, {
    customerId: primaryCustomerId,
    address: { address1: '5 Invalid Country', city: 'Nowhere', countryCode: 'ZZ', provinceCode: 'ZZ' },
    setAsDefault: false,
  });
  assertHttpOk(addressCreateUnknownCountry, 'customerAddressCreate unknown country');

  addressCreateWrongProvinceCountry = await runGraphql(createAddressMutation, {
    customerId: primaryCustomerId,
    address: { address1: '6 Wrong Province', city: 'Chicago', countryCode: 'US', provinceCode: 'ON' },
    setAsDefault: false,
  });
  assertHttpOk(addressCreateWrongProvinceCountry, 'customerAddressCreate wrong province country');

  addressUpdateWrongProvinceCountry = await runGraphql(updateAddressMutation, {
    customerId: primaryCustomerId,
    addressId: validAddressId,
    address: { countryCode: 'US', provinceCode: 'ON' },
    setAsDefault: false,
  });
  assertHttpOk(addressUpdateWrongProvinceCountry, 'customerAddressUpdate wrong province country');

  customerUpdateWrongProvinceCountry = await runGraphql(updateCustomerMutation, {
    input: {
      id: primaryCustomerId,
      addresses: [{ address1: '7 Wrong Province', city: 'Chicago', countryCode: 'US', provinceCode: 'ON' }],
    },
  });
  assertHttpOk(customerUpdateWrongProvinceCountry, 'customerUpdate wrong province country');

  customerSetWrongProvinceCountry = await runGraphql(customerSetMutation, {
    identifier: { id: primaryCustomerId },
    input: {
      email: validControlVariables.input.email,
      addresses: [{ address1: '8 Wrong Province', city: 'Chicago', countryCode: 'US', provinceCode: 'ON' }],
    },
  });
  assertHttpOk(customerSetWrongProvinceCountry, 'customerSet wrong province country');

  customerCreateUnknownCountry = await runGraphql(createCustomerMutation, customerCreateUnknownCountryVariables);
  assertHttpOk(customerCreateUnknownCountry, 'customerCreate unknown country');

  customerCreateDisplayConflict = await runGraphql(createCustomerMutation, customerCreateDisplayConflictVariables);
  assertHttpOk(customerCreateDisplayConflict, 'customerCreate display conflict normalization');
  const conflictCustomerId = customerIdFrom(customerCreateDisplayConflict, 'customerCreate');
  if (conflictCustomerId !== null) {
    createdCustomerIds.add(conflictCustomerId);
  }

  customerCreateNoZoneProvince = await runGraphql(createCustomerMutation, customerCreateNoZoneProvinceVariables);
  assertHttpOk(customerCreateNoZoneProvince, 'customerCreate no-zone province normalization');
  const noZoneCustomerId = customerIdFrom(customerCreateNoZoneProvince, 'customerCreate');
  if (noZoneCustomerId !== null) {
    createdCustomerIds.add(noZoneCustomerId);
  }
} finally {
  for (const id of [...createdCustomerIds].reverse()) {
    cleanupResults = [...cleanupResults, { id, result: await deleteCustomer(id) }];
  }
}

await mkdir(outputDir, { recursive: true });
const outputPath = path.join(outputDir, 'customer-address-country-province-validation.json');
const capture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  validControl: { variables: validControlVariables, status: validControl.status, response: validControl.payload },
  addressCreateUnknownCountry: {
    variables: {
      customerId: '<valid-control-customer-id>',
      address: { address1: '5 Invalid Country', city: 'Nowhere', countryCode: 'ZZ', provinceCode: 'ZZ' },
      setAsDefault: false,
    },
    status: addressCreateUnknownCountry.status,
    response: addressCreateUnknownCountry.payload,
  },
  addressCreateWrongProvinceCountry: {
    variables: {
      customerId: '<valid-control-customer-id>',
      address: { address1: '6 Wrong Province', city: 'Chicago', countryCode: 'US', provinceCode: 'ON' },
      setAsDefault: false,
    },
    status: addressCreateWrongProvinceCountry.status,
    response: addressCreateWrongProvinceCountry.payload,
  },
  addressUpdateWrongProvinceCountry: {
    variables: {
      customerId: '<valid-control-customer-id>',
      addressId: '<valid-control-address-id>',
      address: { countryCode: 'US', provinceCode: 'ON' },
      setAsDefault: false,
    },
    status: addressUpdateWrongProvinceCountry.status,
    response: addressUpdateWrongProvinceCountry.payload,
  },
  customerUpdateWrongProvinceCountry: {
    variables: {
      input: {
        id: '<valid-control-customer-id>',
        addresses: [{ address1: '7 Wrong Province', city: 'Chicago', countryCode: 'US', provinceCode: 'ON' }],
      },
    },
    status: customerUpdateWrongProvinceCountry.status,
    response: customerUpdateWrongProvinceCountry.payload,
  },
  customerSetWrongProvinceCountry: {
    variables: {
      identifier: { id: '<valid-control-customer-id>' },
      input: {
        email: validControlVariables.input.email,
        addresses: [{ address1: '8 Wrong Province', city: 'Chicago', countryCode: 'US', provinceCode: 'ON' }],
      },
    },
    status: customerSetWrongProvinceCountry.status,
    response: customerSetWrongProvinceCountry.payload,
  },
  customerCreateUnknownCountry: {
    variables: customerCreateUnknownCountryVariables,
    status: customerCreateUnknownCountry.status,
    response: customerCreateUnknownCountry.payload,
  },
  customerCreateDisplayConflict: {
    variables: customerCreateDisplayConflictVariables,
    status: customerCreateDisplayConflict.status,
    response: customerCreateDisplayConflict.payload,
  },
  customerCreateNoZoneProvince: {
    variables: customerCreateNoZoneProvinceVariables,
    status: customerCreateNoZoneProvince.status,
    response: customerCreateNoZoneProvince.payload,
  },
  cleanup: cleanupResults,
  upstreamCalls: [],
  notes:
    'Captured Admin GraphQL evidence for HAR-776. Shopify normalizes countryCode over a conflicting country display name, and ignores province input for SG because the country has no zones; those captured branches intentionally override the original ticket assumptions.',
};

await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
