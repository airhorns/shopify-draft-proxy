/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonObject = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const requestDir = path.join('config', 'parity-requests', 'customers');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join(requestDir, name), 'utf8');
}

function asRecord(value: unknown): JsonObject {
  if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
    return value as JsonObject;
  }
  return {};
}

function getPath(value: unknown, keys: string[]): unknown {
  let cursor: unknown = value;
  for (const key of keys) {
    cursor = asRecord(cursor)[key];
  }
  return cursor;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertHttpOk(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertMissingCustomerPayload(
  result: ConformanceGraphqlResult,
  root: string,
  nullableField: string,
  context: string,
): void {
  assertNoTopLevelErrors(result, context);
  const payload = getPath(result.payload, ['data', root]);
  const userErrors = getPath(payload, ['userErrors']);
  const nullableValue = getPath(payload, [nullableField]);
  const expectedUserErrors = [{ field: ['customerId'], message: 'Customer does not exist' }];
  if (JSON.stringify(userErrors) !== JSON.stringify(expectedUserErrors) || nullableValue !== null) {
    throw new Error(`${context} returned unexpected payload: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

const createCustomerMutation = await readRequest('customer-address-lifecycle-create-customer.graphql');
const createAddressMutation = await readRequest('customer-address-lifecycle-create-address.graphql');
const updateAddressMutation = await readRequest('customer-address-lifecycle-update-address.graphql');
const deleteAddressMutation = await readRequest('customer-address-lifecycle-delete-address.graphql');
const defaultAddressMutation = await readRequest('customer-address-lifecycle-default-address.graphql');

const deleteCustomerMutation = `#graphql
  mutation CustomerAddressMissingCustomerCleanup($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

await mkdir(outputDir, { recursive: true });

const stamp = Date.now();
const missingCustomerId = 'gid://shopify/Customer/999999999999999';
const unknownAddressId = 'gid://shopify/MailingAddress/999999999999999';
let foreignCustomerId: string | null = null;

try {
  const foreignCustomerVariables = {
    input: {
      email: `hermes-address-missing-customer-${stamp}@example.com`,
      firstName: 'Hermes',
      lastName: 'AddressMissingCustomer',
    },
  };
  const foreignCustomer = await runGraphqlRequest(createCustomerMutation, foreignCustomerVariables);
  assertNoTopLevelErrors(foreignCustomer, 'customerCreate for foreign address setup');
  const createdCustomerId = getPath(foreignCustomer.payload, ['data', 'customerCreate', 'customer', 'id']);
  if (typeof createdCustomerId !== 'string' || createdCustomerId.length === 0) {
    throw new Error(`customerCreate did not return id: ${JSON.stringify(foreignCustomer.payload, null, 2)}`);
  }
  foreignCustomerId = createdCustomerId;

  const foreignAddressVariables = {
    customerId: foreignCustomerId,
    address: {
      address1: '1 Missing Customer Foreign Address Rd',
      city: 'Ottawa',
      countryCode: 'CA',
      provinceCode: 'ON',
      zip: 'K1A 0B1',
    },
    setAsDefault: true,
  };
  const foreignAddress = await runGraphqlRequest(createAddressMutation, foreignAddressVariables);
  assertNoTopLevelErrors(foreignAddress, 'customerAddressCreate for foreign address setup');
  const foreignAddressId = getPath(foreignAddress.payload, ['data', 'customerAddressCreate', 'address', 'id']);
  if (typeof foreignAddressId !== 'string' || foreignAddressId.length === 0) {
    throw new Error(`customerAddressCreate did not return id: ${JSON.stringify(foreignAddress.payload, null, 2)}`);
  }

  const unknownAddressUpdateVariables = {
    customerId: missingCustomerId,
    addressId: unknownAddressId,
    address: { address1: 'Updated Missing Customer' },
    setAsDefault: false,
  };
  const unknownAddressUpdate = await runGraphqlRequest(updateAddressMutation, unknownAddressUpdateVariables);
  assertHttpOk(unknownAddressUpdate, 'customerAddressUpdate missing customer with unknown address');

  const unknownAddressDeleteVariables = {
    customerId: missingCustomerId,
    addressId: unknownAddressId,
  };
  const unknownAddressDelete = await runGraphqlRequest(deleteAddressMutation, unknownAddressDeleteVariables);
  assertHttpOk(unknownAddressDelete, 'customerAddressDelete missing customer with unknown address');

  const unknownAddressDefaultVariables = {
    customerId: missingCustomerId,
    addressId: unknownAddressId,
  };
  const unknownAddressDefault = await runGraphqlRequest(defaultAddressMutation, unknownAddressDefaultVariables);
  assertHttpOk(unknownAddressDefault, 'customerUpdateDefaultAddress missing customer with unknown address');

  const foreignAddressUpdateVariables = {
    customerId: missingCustomerId,
    addressId: foreignAddressId,
    address: { address1: 'Updated Missing Customer Foreign' },
    setAsDefault: false,
  };
  const foreignAddressUpdate = await runGraphqlRequest(updateAddressMutation, foreignAddressUpdateVariables);
  assertMissingCustomerPayload(
    foreignAddressUpdate,
    'customerAddressUpdate',
    'address',
    'customerAddressUpdate missing customer with foreign address',
  );

  const foreignAddressDeleteVariables = {
    customerId: missingCustomerId,
    addressId: foreignAddressId,
  };
  const foreignAddressDelete = await runGraphqlRequest(deleteAddressMutation, foreignAddressDeleteVariables);
  assertMissingCustomerPayload(
    foreignAddressDelete,
    'customerAddressDelete',
    'deletedAddressId',
    'customerAddressDelete missing customer with foreign address',
  );

  const foreignAddressDefaultVariables = {
    customerId: missingCustomerId,
    addressId: foreignAddressId,
  };
  const foreignAddressDefault = await runGraphqlRequest(defaultAddressMutation, foreignAddressDefaultVariables);
  assertMissingCustomerPayload(
    foreignAddressDefault,
    'customerUpdateDefaultAddress',
    'customer',
    'customerUpdateDefaultAddress missing customer with foreign address',
  );

  const output = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    setup: {
      foreignCustomer: {
        variables: foreignCustomerVariables,
        response: foreignCustomer.payload,
      },
      foreignAddress: {
        variables: foreignAddressVariables,
        response: foreignAddress.payload,
      },
    },
    missingCustomerUnknownAddress: {
      update: {
        variables: unknownAddressUpdateVariables,
        response: unknownAddressUpdate.payload,
      },
      delete: {
        variables: unknownAddressDeleteVariables,
        response: unknownAddressDelete.payload,
      },
      defaultAddress: {
        variables: unknownAddressDefaultVariables,
        response: unknownAddressDefault.payload,
      },
    },
    missingCustomerForeignAddress: {
      update: {
        variables: foreignAddressUpdateVariables,
        response: foreignAddressUpdate.payload,
      },
      delete: {
        variables: foreignAddressDeleteVariables,
        response: foreignAddressDelete.payload,
      },
      defaultAddress: {
        variables: foreignAddressDefaultVariables,
        response: foreignAddressDefault.payload,
      },
    },
    upstreamCalls: [],
  };

  const outputPath = path.join(outputDir, 'customer-address-missing-customer.json');
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (foreignCustomerId) {
    const cleanup = await runGraphqlRequest(deleteCustomerMutation, { input: { id: foreignCustomerId } });
    if (cleanup.status < 200 || cleanup.status >= 300 || cleanup.payload.errors) {
      console.error(`Customer cleanup failed for ${foreignCustomerId}: ${JSON.stringify(cleanup, null, 2)}`);
    }
  }
}
