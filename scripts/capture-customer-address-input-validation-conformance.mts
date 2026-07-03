/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type MutationRoot = {
  address?: { id?: unknown } | null;
  customer?: { id?: unknown } | null;
};
type MutationData = Record<string, MutationRoot | null | undefined>;
type GraphqlResult = ConformanceGraphqlResult<MutationData>;

function runGraphql(query: string, variables: Record<string, unknown> = {}): Promise<GraphqlResult> {
  return runGraphqlRequest<MutationData>(query, variables);
}

async function readGraphqlDocument(relativePath: string): Promise<string> {
  return readFile(relativePath, 'utf8');
}

const createCustomerMutation = await readGraphqlDocument(
  'config/parity-requests/customers/customerInputValidation-create.graphql',
);
const createAddressMutation = await readGraphqlDocument(
  'config/parity-requests/customers/customer-address-lifecycle-create-address.graphql',
);
const updateAddressMutation = await readGraphqlDocument(
  'config/parity-requests/customers/customer-address-lifecycle-update-address.graphql',
);
const phoneReadQuery = await readGraphqlDocument(
  'config/parity-requests/customers/customer-address-phone-normalization-read.graphql',
);
const customerSetMutation = await readGraphqlDocument('config/parity-requests/customers/customerSet-parity.graphql');

const deleteCustomerMutation = `#graphql
  mutation CustomerAddressInputValidationDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

function assertHttpOk(result: GraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function customerIdFrom(result: GraphqlResult, rootName: string): string | null {
  const id = result.payload?.data?.[rootName]?.customer?.id;
  return typeof id === 'string' && id ? id : null;
}

function addressIdFrom(result: GraphqlResult, rootName: string): string | null {
  const id = result.payload?.data?.[rootName]?.address?.id;
  return typeof id === 'string' && id ? id : null;
}

async function deleteCustomer(id: string): Promise<GraphqlResult> {
  const result = await runGraphql(deleteCustomerMutation, { input: { id } });
  assertHttpOk(result, `delete ${id}`);
  return result;
}

const timestamp = Date.now();
const createdCustomerIds = new Set<string>();
let cleanupResults: Array<{ id: string; result: GraphqlResult }> = [];

const setupCustomerVariables = {
  input: {
    email: `address-validation-${timestamp}@example.com`,
    firstName: 'Address',
    lastName: 'Validation',
  },
};

const tooLongAddress1 = 'x'.repeat(256);
const allBlankAddress = {
  address1: ' ',
  address2: ' ',
  city: ' ',
  company: ' ',
  zip: ' ',
  phone: ' ',
};

const createTrimmedAddress = {
  firstName: '  Trim  ',
  lastName: '  Create  ',
  address1: ' 100 Main ',
  address2: ' Suite 4 ',
  city: ' Ottawa ',
  company: ' Acme ',
  countryCode: 'CA',
  provinceCode: 'ON',
  zip: ' K1A 0B1 ',
  phone: ' +14155550123 ',
};

const updateTrimmedAddress = {
  address1: ' 200 Side ',
  city: ' Toronto ',
  countryCode: 'CA',
  provinceCode: 'ON',
  zip: ' M5H 2N2 ',
  phone: ' +14155550124 ',
};
const phoneSetupCustomerVariables = {
  input: {
    email: `address-phone-normalization-${timestamp}@example.com`,
    firstName: 'Address',
    lastName: 'Phone',
  },
};
const createFormattedPhoneAddress = {
  address1: '300 Phone Normalization',
  city: 'Ottawa',
  countryCode: 'CA',
  provinceCode: 'ON',
  phone: '+1 (613) 450-4538',
};
const updateFormattedPhoneAddress = {
  phone: '+1-613-450-4538',
};
const updateLocalPhoneAddress = {
  phone: '450-4538',
};
const updateRawFallbackPhoneAddress = {
  phone: 'not a phone',
};

let setupCustomer!: GraphqlResult;
let phoneSetupCustomer!: GraphqlResult;
let addressCreateTrimmedSuccess!: GraphqlResult;
let addressUpdateTrimmedSuccess!: GraphqlResult;
let addressCreateFormattedPhone!: GraphqlResult;
let addressCreateFormattedPhoneRead!: GraphqlResult;
let addressUpdateFormattedPhone!: GraphqlResult;
let addressUpdateFormattedPhoneRead!: GraphqlResult;
let addressUpdateLocalPhone!: GraphqlResult;
let addressUpdateLocalPhoneRead!: GraphqlResult;
let addressUpdateRawFallbackPhone!: GraphqlResult;
let addressUpdateRawFallbackPhoneRead!: GraphqlResult;
let addressCreateTooLong!: GraphqlResult;
let addressCreateCityHtml!: GraphqlResult;
let addressCreateCityUrl!: GraphqlResult;
let addressCreateZipUrl!: GraphqlResult;
let addressCreatePhoneHtml!: GraphqlResult;
let addressCreateAddressEmoji!: GraphqlResult;
let addressCreateBlankAccepted!: GraphqlResult;
let customerCreateNestedTooLong!: GraphqlResult;
let customerCreateNestedBlank!: GraphqlResult;
let customerSetNestedBlank!: GraphqlResult;

try {
  setupCustomer = await runGraphql(createCustomerMutation, setupCustomerVariables);
  assertHttpOk(setupCustomer, 'setup customerCreate');
  const primaryCustomerId = customerIdFrom(setupCustomer, 'customerCreate');
  if (primaryCustomerId === null) {
    throw new Error(`setup customerCreate did not create a customer: ${JSON.stringify(setupCustomer, null, 2)}`);
  }
  createdCustomerIds.add(primaryCustomerId);

  addressCreateTrimmedSuccess = await runGraphql(createAddressMutation, {
    customerId: primaryCustomerId,
    address: createTrimmedAddress,
    setAsDefault: true,
  });
  assertHttpOk(addressCreateTrimmedSuccess, 'customerAddressCreate trims address fields');
  const trimmedAddressId = addressIdFrom(addressCreateTrimmedSuccess, 'customerAddressCreate');
  if (trimmedAddressId === null) {
    throw new Error(
      `customerAddressCreate trimmed control did not create an address: ${JSON.stringify(addressCreateTrimmedSuccess, null, 2)}`,
    );
  }

  addressUpdateTrimmedSuccess = await runGraphql(updateAddressMutation, {
    customerId: primaryCustomerId,
    addressId: trimmedAddressId,
    address: updateTrimmedAddress,
    setAsDefault: true,
  });
  assertHttpOk(addressUpdateTrimmedSuccess, 'customerAddressUpdate trims address fields');

  phoneSetupCustomer = await runGraphql(createCustomerMutation, phoneSetupCustomerVariables);
  assertHttpOk(phoneSetupCustomer, 'phone setup customerCreate');
  const phoneCustomerId = customerIdFrom(phoneSetupCustomer, 'customerCreate');
  if (phoneCustomerId === null) {
    throw new Error(
      `phone setup customerCreate did not create a customer: ${JSON.stringify(phoneSetupCustomer, null, 2)}`,
    );
  }
  createdCustomerIds.add(phoneCustomerId);

  addressCreateFormattedPhone = await runGraphql(createAddressMutation, {
    customerId: phoneCustomerId,
    address: createFormattedPhoneAddress,
    setAsDefault: true,
  });
  assertHttpOk(addressCreateFormattedPhone, 'customerAddressCreate formatted phone normalization');
  const phoneAddressId = addressIdFrom(addressCreateFormattedPhone, 'customerAddressCreate');
  if (phoneAddressId === null) {
    throw new Error(
      `customerAddressCreate formatted phone did not create an address: ${JSON.stringify(addressCreateFormattedPhone, null, 2)}`,
    );
  }

  addressCreateFormattedPhoneRead = await runGraphql(phoneReadQuery, { id: phoneCustomerId });
  assertHttpOk(addressCreateFormattedPhoneRead, 'customerAddressCreate formatted phone readback');

  addressUpdateFormattedPhone = await runGraphql(updateAddressMutation, {
    customerId: phoneCustomerId,
    addressId: phoneAddressId,
    address: updateFormattedPhoneAddress,
    setAsDefault: true,
  });
  assertHttpOk(addressUpdateFormattedPhone, 'customerAddressUpdate formatted phone normalization');

  addressUpdateFormattedPhoneRead = await runGraphql(phoneReadQuery, { id: phoneCustomerId });
  assertHttpOk(addressUpdateFormattedPhoneRead, 'customerAddressUpdate formatted phone readback');

  addressUpdateLocalPhone = await runGraphql(updateAddressMutation, {
    customerId: phoneCustomerId,
    addressId: phoneAddressId,
    address: updateLocalPhoneAddress,
    setAsDefault: true,
  });
  assertHttpOk(addressUpdateLocalPhone, 'customerAddressUpdate local phone normalization');

  addressUpdateLocalPhoneRead = await runGraphql(phoneReadQuery, { id: phoneCustomerId });
  assertHttpOk(addressUpdateLocalPhoneRead, 'customerAddressUpdate local phone readback');

  addressUpdateRawFallbackPhone = await runGraphql(updateAddressMutation, {
    customerId: phoneCustomerId,
    addressId: phoneAddressId,
    address: updateRawFallbackPhoneAddress,
    setAsDefault: true,
  });
  assertHttpOk(addressUpdateRawFallbackPhone, 'customerAddressUpdate raw fallback phone');

  addressUpdateRawFallbackPhoneRead = await runGraphql(phoneReadQuery, { id: phoneCustomerId });
  assertHttpOk(addressUpdateRawFallbackPhoneRead, 'customerAddressUpdate raw fallback phone readback');

  addressCreateTooLong = await runGraphql(createAddressMutation, {
    customerId: primaryCustomerId,
    address: { address1: tooLongAddress1, countryCode: 'CA', provinceCode: 'ON' },
    setAsDefault: false,
  });
  assertHttpOk(addressCreateTooLong, 'customerAddressCreate address1 too long');

  addressCreateCityHtml = await runGraphql(createAddressMutation, {
    customerId: primaryCustomerId,
    address: { city: '<script>', countryCode: 'CA', provinceCode: 'ON' },
    setAsDefault: false,
  });
  assertHttpOk(addressCreateCityHtml, 'customerAddressCreate city HTML');

  addressCreateCityUrl = await runGraphql(createAddressMutation, {
    customerId: primaryCustomerId,
    address: { city: 'https://evil.example', countryCode: 'CA', provinceCode: 'ON' },
    setAsDefault: false,
  });
  assertHttpOk(addressCreateCityUrl, 'customerAddressCreate city URL');

  addressCreateZipUrl = await runGraphql(createAddressMutation, {
    customerId: primaryCustomerId,
    address: { zip: 'H0H 0H0 https://x', countryCode: 'CA', provinceCode: 'ON' },
    setAsDefault: false,
  });
  assertHttpOk(addressCreateZipUrl, 'customerAddressCreate zip URL');

  addressCreatePhoneHtml = await runGraphql(createAddressMutation, {
    customerId: primaryCustomerId,
    address: { phone: '<a>+1 613', countryCode: 'CA', provinceCode: 'ON' },
    setAsDefault: false,
  });
  assertHttpOk(addressCreatePhoneHtml, 'customerAddressCreate phone HTML');

  addressCreateAddressEmoji = await runGraphql(createAddressMutation, {
    customerId: primaryCustomerId,
    address: { address1: '100 Main 😀', countryCode: 'CA', provinceCode: 'ON' },
    setAsDefault: false,
  });
  assertHttpOk(addressCreateAddressEmoji, 'customerAddressCreate address1 emoji');

  addressCreateBlankAccepted = await runGraphql(createAddressMutation, {
    customerId: primaryCustomerId,
    address: allBlankAddress,
    setAsDefault: false,
  });
  assertHttpOk(addressCreateBlankAccepted, 'customerAddressCreate blank address');

  customerCreateNestedTooLong = await runGraphql(createCustomerMutation, {
    input: {
      email: `address-validation-too-long-${timestamp}@example.com`,
      addresses: [{ address1: tooLongAddress1, countryCode: 'CA', provinceCode: 'ON' }],
    },
  });
  assertHttpOk(customerCreateNestedTooLong, 'customerCreate nested address1 too long');

  customerCreateNestedBlank = await runGraphql(createCustomerMutation, {
    input: {
      email: `address-validation-blank-${timestamp}@example.com`,
      addresses: [allBlankAddress],
    },
  });
  assertHttpOk(customerCreateNestedBlank, 'customerCreate blank nested address');

  customerSetNestedBlank = await runGraphql(customerSetMutation, {
    identifier: { id: primaryCustomerId },
    input: {
      email: setupCustomerVariables.input.email,
      addresses: [allBlankAddress],
    },
  });
  assertHttpOk(customerSetNestedBlank, 'customerSet blank nested address');
} finally {
  for (const id of [...createdCustomerIds].reverse()) {
    cleanupResults = [...cleanupResults, { id, result: await deleteCustomer(id) }];
  }
}

await mkdir(outputDir, { recursive: true });
const outputPath = path.join(outputDir, 'customer-address-input-validation.json');
const capture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  setupCustomer: {
    variables: setupCustomerVariables,
    status: setupCustomer.status,
    response: setupCustomer.payload,
  },
  phoneSetupCustomer: {
    variables: phoneSetupCustomerVariables,
    status: phoneSetupCustomer.status,
    response: phoneSetupCustomer.payload,
  },
  addressCreateTrimmedSuccess: {
    variables: {
      customerId: '<setup-customer-id>',
      address: createTrimmedAddress,
      setAsDefault: true,
    },
    status: addressCreateTrimmedSuccess.status,
    response: addressCreateTrimmedSuccess.payload,
  },
  addressUpdateTrimmedSuccess: {
    variables: {
      customerId: '<setup-customer-id>',
      addressId: '<trimmed-address-id>',
      address: updateTrimmedAddress,
      setAsDefault: true,
    },
    status: addressUpdateTrimmedSuccess.status,
    response: addressUpdateTrimmedSuccess.payload,
  },
  addressCreateFormattedPhone: {
    variables: {
      customerId: '<phone-setup-customer-id>',
      address: createFormattedPhoneAddress,
      setAsDefault: true,
    },
    status: addressCreateFormattedPhone.status,
    response: addressCreateFormattedPhone.payload,
  },
  addressCreateFormattedPhoneRead: {
    variables: { id: '<phone-setup-customer-id>' },
    status: addressCreateFormattedPhoneRead.status,
    response: addressCreateFormattedPhoneRead.payload,
  },
  addressUpdateFormattedPhone: {
    variables: {
      customerId: '<phone-setup-customer-id>',
      addressId: '<phone-address-id>',
      address: updateFormattedPhoneAddress,
      setAsDefault: true,
    },
    status: addressUpdateFormattedPhone.status,
    response: addressUpdateFormattedPhone.payload,
  },
  addressUpdateFormattedPhoneRead: {
    variables: { id: '<phone-setup-customer-id>' },
    status: addressUpdateFormattedPhoneRead.status,
    response: addressUpdateFormattedPhoneRead.payload,
  },
  addressUpdateLocalPhone: {
    variables: {
      customerId: '<phone-setup-customer-id>',
      addressId: '<phone-address-id>',
      address: updateLocalPhoneAddress,
      setAsDefault: true,
    },
    status: addressUpdateLocalPhone.status,
    response: addressUpdateLocalPhone.payload,
  },
  addressUpdateLocalPhoneRead: {
    variables: { id: '<phone-setup-customer-id>' },
    status: addressUpdateLocalPhoneRead.status,
    response: addressUpdateLocalPhoneRead.payload,
  },
  addressUpdateRawFallbackPhone: {
    variables: {
      customerId: '<phone-setup-customer-id>',
      addressId: '<phone-address-id>',
      address: updateRawFallbackPhoneAddress,
      setAsDefault: true,
    },
    status: addressUpdateRawFallbackPhone.status,
    response: addressUpdateRawFallbackPhone.payload,
  },
  addressUpdateRawFallbackPhoneRead: {
    variables: { id: '<phone-setup-customer-id>' },
    status: addressUpdateRawFallbackPhoneRead.status,
    response: addressUpdateRawFallbackPhoneRead.payload,
  },
  addressCreateTooLong: {
    variables: {
      customerId: '<setup-customer-id>',
      address: { address1: tooLongAddress1, countryCode: 'CA', provinceCode: 'ON' },
      setAsDefault: false,
    },
    status: addressCreateTooLong.status,
    response: addressCreateTooLong.payload,
  },
  addressCreateCityHtml: {
    variables: {
      customerId: '<setup-customer-id>',
      address: { city: '<script>', countryCode: 'CA', provinceCode: 'ON' },
      setAsDefault: false,
    },
    status: addressCreateCityHtml.status,
    response: addressCreateCityHtml.payload,
  },
  addressCreateCityUrl: {
    variables: {
      customerId: '<setup-customer-id>',
      address: { city: 'https://evil.example', countryCode: 'CA', provinceCode: 'ON' },
      setAsDefault: false,
    },
    status: addressCreateCityUrl.status,
    response: addressCreateCityUrl.payload,
  },
  addressCreateZipUrl: {
    variables: {
      customerId: '<setup-customer-id>',
      address: { zip: 'H0H 0H0 https://x', countryCode: 'CA', provinceCode: 'ON' },
      setAsDefault: false,
    },
    status: addressCreateZipUrl.status,
    response: addressCreateZipUrl.payload,
  },
  addressCreatePhoneHtml: {
    variables: {
      customerId: '<setup-customer-id>',
      address: { phone: '<a>+1 613', countryCode: 'CA', provinceCode: 'ON' },
      setAsDefault: false,
    },
    status: addressCreatePhoneHtml.status,
    response: addressCreatePhoneHtml.payload,
  },
  addressCreateAddressEmoji: {
    variables: {
      customerId: '<setup-customer-id>',
      address: { address1: '100 Main 😀', countryCode: 'CA', provinceCode: 'ON' },
      setAsDefault: false,
    },
    status: addressCreateAddressEmoji.status,
    response: addressCreateAddressEmoji.payload,
  },
  addressCreateBlankAccepted: {
    variables: {
      customerId: '<setup-customer-id>',
      address: allBlankAddress,
      setAsDefault: false,
    },
    status: addressCreateBlankAccepted.status,
    response: addressCreateBlankAccepted.payload,
  },
  customerCreateNestedTooLong: {
    variables: {
      input: {
        email: `address-validation-too-long-${timestamp}@example.com`,
        addresses: [{ address1: tooLongAddress1, countryCode: 'CA', provinceCode: 'ON' }],
      },
    },
    status: customerCreateNestedTooLong.status,
    response: customerCreateNestedTooLong.payload,
  },
  customerCreateNestedBlank: {
    variables: {
      input: {
        email: `address-validation-blank-${timestamp}@example.com`,
        addresses: [allBlankAddress],
      },
    },
    status: customerCreateNestedBlank.status,
    response: customerCreateNestedBlank.payload,
  },
  customerSetNestedBlank: {
    variables: {
      identifier: { id: '<setup-customer-id>' },
      input: {
        email: setupCustomerVariables.input.email,
        addresses: [allBlankAddress],
      },
    },
    status: customerSetNestedBlank.status,
    response: customerSetNestedBlank.payload,
  },
  cleanup: cleanupResults,
  upstreamCalls: [],
  notes:
    'Captured Admin GraphQL evidence for customer address input length, HTML, URL, emoji, blank-address, whitespace normalization, and no-default-territory address phone normalization behavior.',
};

await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
