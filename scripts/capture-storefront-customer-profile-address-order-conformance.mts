/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { randomUUID } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, runStorefrontGraphqlRequest } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import {
  buildAdminAuthHeaders,
  buildStorefrontRequestHeaders,
  getStoredStorefrontAccessToken,
  getValidConformanceAccessToken,
} from './shopify-conformance-auth.mjs';

type GraphqlRecord = {
  method: 'POST';
  apiSurface: 'admin' | 'storefront';
  apiVersion: string;
  path: string;
  endpoint: string;
  authMode: string;
  headers?: Record<string, string>;
  operationName: string;
  query: string;
  variables: unknown;
  response: {
    status: number;
    body: unknown;
  };
};

const scenarioId = 'storefront-customer-profile-address-order-lifecycle';
const redacted = '<redacted:storefront-customer-profile-address-order>';
const documentPaths = {
  adminAddressCreate: 'config/parity-requests/storefront/storefront-customer-admin-address-create.graphql',
  adminDelete: 'config/parity-requests/storefront/storefront-customer-auth-admin-delete.graphql',
  adminOrderCreate: 'config/parity-requests/storefront/storefront-customer-admin-order-create.graphql',
  adminRead: 'config/parity-requests/storefront/storefront-customer-admin-read.graphql',
  adminUpdate: 'config/parity-requests/storefront/storefront-customer-auth-admin-update.graphql',
  addressCreate: 'config/parity-requests/storefront/storefront-customer-address-create.graphql',
  addressDelete: 'config/parity-requests/storefront/storefront-customer-address-delete.graphql',
  addressUpdate: 'config/parity-requests/storefront/storefront-customer-address-update.graphql',
  create: 'config/parity-requests/storefront/storefront-customer-auth-create.graphql',
  defaultAddressUpdate: 'config/parity-requests/storefront/storefront-customer-default-address-update.graphql',
  profileRead: 'config/parity-requests/storefront/storefront-customer-profile-address-order-read.graphql',
  profileUpdate: 'config/parity-requests/storefront/storefront-customer-profile-update.graphql',
  tokenCreate: 'config/parity-requests/storefront/storefront-customer-auth-token-create.graphql',
} as const;

const orderCancelDocument = `#graphql
  mutation StorefrontCustomerProfileAddressOrderCancel($orderId: ID!) {
    orderCancel(orderId: $orderId, reason: OTHER, notifyCustomer: false, restock: false) {
      orderCancelUserErrors {
        field
        message
        code
      }
    }
  }
`;

const orderDeleteDocument = `#graphql
  mutation StorefrontCustomerProfileAddressOrderDelete($orderId: ID!) {
    orderDelete(orderId: $orderId) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminClient = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const storedStorefrontAuth = await getStoredStorefrontAccessToken();
if (storedStorefrontAuth.shop && storedStorefrontAuth.shop !== storeDomain) {
  throw new Error(
    `Stored Storefront token is for ${storedStorefrontAuth.shop}, but SHOPIFY_CONFORMANCE_STORE_DOMAIN is ${storeDomain}. ` +
      'Run `corepack pnpm conformance:grant-storefront-token` for the target store.',
  );
}

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'storefront');
const outputPath = path.join(outputDir, `${scenarioId}.json`);
const storefrontEndpoint = `https://${storeDomain}/api/${apiVersion}/graphql.json`;
const storefrontPath = `/api/${apiVersion}/graphql.json`;
const adminEndpoint = `${adminOrigin}/admin/api/${apiVersion}/graphql.json`;
const adminPath = `/admin/api/${apiVersion}/graphql.json`;
const storefrontHeaders = Object.fromEntries(
  Object.keys(buildStorefrontRequestHeaders(storedStorefrontAuth.storefront_access_token)).map((name) => [
    name,
    '<redacted:storefront-access-token>',
  ]),
);
const suffix = `${Date.now().toString(36)}-${randomUUID().slice(0, 8)}`;
const email = `hermes-storefront-profile-${suffix}@example.com`;
const updatedEmail = `hermes-storefront-updated-${suffix}@example.com`;
const orderEmail = `hermes-storefront-order-${suffix}@example.com`;
const password = 'CodexPass123!';
const updatedPassword = 'NewCodexPass123!';
const phoneTail = suffix
  .replace(/[^0-9]/gu, '')
  .padEnd(4, '7')
  .slice(0, 4);
const updatedPhone = `+1613555${phoneTail}`;
const cleanupCustomerIds = new Set<string>();
const cleanupOrderIds = new Set<string>();

function redactSensitive(value: unknown, key?: string): unknown {
  if (typeof value === 'string' && value.includes('customer_access_token=')) {
    return value.replace(/([?&]customer_access_token=)[^&#"]+/gu, `$1${redacted}`);
  }
  if (
    typeof value === 'string' &&
    key !== undefined &&
    [
      'accessToken',
      'activationToken',
      'activationUrl',
      'customerAccessToken',
      'deletedAccessToken',
      'password',
      'resetToken',
      'resetUrl',
      'token',
    ].includes(key)
  ) {
    return redacted;
  }
  if (Array.isArray(value)) return value.map((entry) => redactSensitive(entry));
  if (typeof value === 'object' && value !== null) {
    return Object.fromEntries(
      Object.entries(value).map(([childKey, child]) => [childKey, redactSensitive(child, childKey)]),
    );
  }
  return value;
}

function pathValue(root: unknown, keys: string[], label: string): unknown {
  let cursor = root;
  for (const key of keys) {
    if (typeof cursor !== 'object' || cursor === null || !(key in cursor)) {
      throw new Error(
        `Missing ${label} at ${keys.join('.')} in ${JSON.stringify(redactSensitive(root)).slice(0, 2000)}`,
      );
    }
    cursor = (cursor as Record<string, unknown>)[key];
  }
  return cursor;
}

function pathString(root: unknown, keys: string[], label: string): string {
  const value = pathValue(root, keys, label);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(
      `Expected ${label} to be a non-empty string in ${JSON.stringify(redactSensitive(root)).slice(0, 2000)}`,
    );
  }
  return value;
}

async function readDocument(documentPath: string): Promise<string> {
  return readFile(documentPath, 'utf8');
}

async function recordAdmin(operationName: string, documentPath: string, variables: Record<string, unknown>) {
  const query = await readDocument(documentPath);
  const response = await adminClient.runGraphqlRequest(query, variables);
  const record: GraphqlRecord = {
    method: 'POST',
    apiSurface: 'admin',
    apiVersion,
    path: adminPath,
    endpoint: adminEndpoint,
    authMode: 'admin-access-token',
    operationName,
    query,
    variables: redactSensitive(variables),
    response: {
      status: response.status,
      body: redactSensitive(response.payload),
    },
  };
  return { record, raw: response.payload };
}

async function recordStorefront(operationName: string, documentPath: string, variables: Record<string, unknown>) {
  const query = await readDocument(documentPath);
  const response = await runStorefrontGraphqlRequest(
    {
      storeOrigin: `https://${storeDomain}`,
      apiVersion,
      storefrontAccessToken: storedStorefrontAuth.storefront_access_token,
    },
    query,
    variables,
  );
  const record: GraphqlRecord = {
    method: 'POST',
    apiSurface: 'storefront',
    apiVersion,
    path: storefrontPath,
    endpoint: storefrontEndpoint,
    authMode: 'storefront-access-token',
    headers: storefrontHeaders,
    operationName,
    query,
    variables: redactSensitive(variables),
    response: {
      status: response.status,
      body: redactSensitive(response.payload),
    },
  };
  return { record, raw: response.payload };
}

async function cleanupOrder(orderId: string) {
  const cancel = await adminClient.runGraphqlRequest(orderCancelDocument, { orderId });
  const deleteOrder = await adminClient.runGraphqlRequest(orderDeleteDocument, { orderId });
  return {
    orderId,
    orderCancel: {
      status: cancel.status,
      body: redactSensitive(cancel.payload),
    },
    orderDelete: {
      status: deleteOrder.status,
      body: redactSensitive(deleteOrder.payload),
    },
  };
}

async function cleanupCustomer(customerId: string) {
  const query = await readDocument(documentPaths.adminDelete);
  const response = await adminClient.runGraphqlRequest(query, { input: { id: customerId } });
  return {
    customerId,
    response: {
      status: response.status,
      body: redactSensitive(response.payload),
    },
  };
}

try {
  const storefrontCreate = await recordStorefront('StorefrontCustomerAuthCreate', documentPaths.create, {
    input: {
      email,
      password,
      firstName: 'Storefront',
      lastName: 'Profile',
      acceptsMarketing: false,
    },
  });
  const customerId = pathString(storefrontCreate.raw, ['data', 'customerCreate', 'customer', 'id'], 'customer id');
  cleanupCustomerIds.add(customerId);

  const tokenCreate = await recordStorefront('StorefrontCustomerAuthTokenCreate', documentPaths.tokenCreate, {
    input: {
      email,
      password,
    },
  });
  const accessToken = pathString(
    tokenCreate.raw,
    ['data', 'customerAccessTokenCreate', 'customerAccessToken', 'accessToken'],
    'customer access token',
  );

  const profileEmailUpdateDenied = await recordStorefront(
    'StorefrontCustomerProfileUpdate',
    documentPaths.profileUpdate,
    {
      token: accessToken,
      customer: {
        email: updatedEmail,
        firstName: 'Denied',
        acceptsMarketing: true,
      },
    },
  );

  const profileUpdate = await recordStorefront('StorefrontCustomerProfileUpdate', documentPaths.profileUpdate, {
    token: accessToken,
    customer: {
      firstName: 'Updated',
      lastName: 'Profile',
      phone: updatedPhone,
      acceptsMarketing: true,
    },
  });

  const addressCreateOne = await recordStorefront('StorefrontCustomerAddressCreate', documentPaths.addressCreate, {
    token: accessToken,
    address: {
      address1: '1 Storefront Main St',
      city: 'Ottawa',
      province: 'Ontario',
      country: 'Canada',
      zip: 'K1A 0B1',
      phone: '+1 (613) 555-0199',
    },
  });
  const firstAddressId = pathString(
    addressCreateOne.raw,
    ['data', 'customerAddressCreate', 'customerAddress', 'id'],
    'first address id',
  );

  const addressCreateTwo = await recordStorefront('StorefrontCustomerAddressCreate', documentPaths.addressCreate, {
    token: accessToken,
    address: {
      firstName: 'Second',
      lastName: 'Address',
      address1: '2 Storefront Side St',
      city: 'Toronto',
      province: 'Ontario',
      country: 'Canada',
      zip: 'M5V 2T6',
    },
  });
  const secondAddressId = pathString(
    addressCreateTwo.raw,
    ['data', 'customerAddressCreate', 'customerAddress', 'id'],
    'second address id',
  );

  const defaultAddressUpdate = await recordStorefront(
    'StorefrontCustomerDefaultAddressUpdate',
    documentPaths.defaultAddressUpdate,
    {
      token: accessToken,
      addressId: secondAddressId,
    },
  );

  const addressUpdate = await recordStorefront('StorefrontCustomerAddressUpdate', documentPaths.addressUpdate, {
    token: accessToken,
    id: firstAddressId,
    address: {
      address1: '10 Storefront Main St',
      city: 'Gatineau',
      province: 'Quebec',
      country: 'Canada',
      zip: 'J8X 2X1',
    },
  });

  const addressDelete = await recordStorefront('StorefrontCustomerAddressDelete', documentPaths.addressDelete, {
    token: accessToken,
    id: secondAddressId,
  });

  const adminReadAfterStorefront = await recordAdmin('StorefrontCustomerAdminRead', documentPaths.adminRead, {
    id: customerId,
  });

  const adminUpdate = await recordAdmin('StorefrontCustomerAuthAdminUpdate', documentPaths.adminUpdate, {
    input: {
      id: customerId,
      firstName: 'Admin',
      lastName: 'Visible',
      email: updatedEmail,
    },
  });

  const adminAddressCreate = await recordAdmin(
    'StorefrontCustomerAdminAddressCreate',
    documentPaths.adminAddressCreate,
    {
      customerId,
      address: {
        firstName: 'Admin',
        lastName: 'Address',
        address1: '50 Admin Way',
        city: 'Montreal',
        province: 'Quebec',
        country: 'Canada',
        zip: 'H2Y 1C6',
      },
    },
  );

  const orderCreate = await recordAdmin('StorefrontCustomerAdminOrderCreate', documentPaths.adminOrderCreate, {
    order: {
      email: orderEmail,
      customerId,
      test: true,
      currency: 'CAD',
      lineItems: [
        {
          title: `Storefront customer visible order ${suffix}`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '19.90',
              currencyCode: 'CAD',
            },
          },
          requiresShipping: false,
          taxable: false,
          sku: `SFP-${suffix}`,
        },
      ],
      transactions: [
        {
          kind: 'AUTHORIZATION',
          status: 'SUCCESS',
          gateway: 'external',
          test: true,
          amountSet: {
            shopMoney: {
              amount: '19.90',
              currencyCode: 'CAD',
            },
          },
        },
      ],
    },
  });
  const orderId = pathString(orderCreate.raw, ['data', 'orderCreate', 'order', 'id'], 'order id');
  cleanupOrderIds.add(orderId);

  const storefrontReadAfterAdmin = await recordStorefront(
    'StorefrontCustomerProfileAddressOrderRead',
    documentPaths.profileRead,
    {
      token: accessToken,
    },
  );

  const passwordUpdate = await recordStorefront('StorefrontCustomerProfileUpdate', documentPaths.profileUpdate, {
    token: accessToken,
    customer: {
      password: updatedPassword,
    },
  });
  const rotatedAccessToken = pathString(
    passwordUpdate.raw,
    ['data', 'customerUpdate', 'customerAccessToken', 'accessToken'],
    'rotated customer access token',
  );

  const readWithOldTokenAfterPassword = await recordStorefront(
    'StorefrontCustomerProfileAddressOrderRead',
    documentPaths.profileRead,
    {
      token: accessToken,
    },
  );

  const readWithRotatedToken = await recordStorefront(
    'StorefrontCustomerProfileAddressOrderRead',
    documentPaths.profileRead,
    {
      token: rotatedAccessToken,
    },
  );

  const invalidTokenAddressCreate = await recordStorefront(
    'StorefrontCustomerAddressCreate',
    documentPaths.addressCreate,
    {
      token: 'not-a-valid-token',
      address: {
        address1: '3 Invalid Token St',
      },
    },
  );

  const cleanupOrders = [];
  for (const liveOrderId of cleanupOrderIds) {
    cleanupOrders.push(await cleanupOrder(liveOrderId));
  }
  cleanupOrderIds.clear();

  const cleanupCustomers = [];
  for (const liveCustomerId of cleanupCustomerIds) {
    cleanupCustomers.push(await cleanupCustomer(liveCustomerId));
  }
  cleanupCustomerIds.clear();

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId,
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        apiSurface: 'storefront',
        authMode: 'storefront-access-token-and-admin-access-token',
        storefrontToken: {
          id: storedStorefrontAuth.storefront_token_id || '<unknown>',
          title: storedStorefrontAuth.storefront_token_title || '<unknown>',
          accessScopes: storedStorefrontAuth.storefront_access_scopes,
          obtainedAt: storedStorefrontAuth.obtained_at || '<unknown>',
        },
        disposableInputs: {
          email,
          updatedEmail,
          orderEmail,
          updatedPhone,
        },
        captures: {
          storefrontCreate: storefrontCreate.record,
          tokenCreate: tokenCreate.record,
          profileEmailUpdateDenied: profileEmailUpdateDenied.record,
          profileUpdate: profileUpdate.record,
          addressCreateOne: addressCreateOne.record,
          addressCreateTwo: addressCreateTwo.record,
          defaultAddressUpdate: defaultAddressUpdate.record,
          addressUpdate: addressUpdate.record,
          addressDelete: addressDelete.record,
          adminReadAfterStorefront: adminReadAfterStorefront.record,
          adminUpdate: adminUpdate.record,
          adminAddressCreate: adminAddressCreate.record,
          orderCreate: orderCreate.record,
          storefrontReadAfterAdmin: storefrontReadAfterAdmin.record,
          passwordUpdate: passwordUpdate.record,
          readWithOldTokenAfterPassword: readWithOldTokenAfterPassword.record,
          readWithRotatedToken: readWithRotatedToken.record,
          invalidTokenAddressCreate: invalidTokenAddressCreate.record,
        },
        cleanup: {
          orderCleanup: cleanupOrders,
          customerDeletes: cleanupCustomers,
        },
        upstreamCalls: [],
        notes: [
          'Disposable Storefront customer, customer addresses, and Admin test order were created against live Shopify.',
          'The scenario proves Storefront profile/address mutations are visible through Admin reads, then Admin profile/address/order changes are visible through Storefront token-authenticated reads.',
          'Passwords and Storefront access tokens are redacted in this checked-in fixture; disposable example.com emails and Canadian safe addresses are not real customer data.',
          'Cleanup cancels/deletes the disposable test order before deleting the customer.',
        ],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(`Wrote ${outputPath}`);
  console.log(`Captured Storefront customer profile/address/order lifecycle for ${email}`);
} finally {
  for (const orderId of cleanupOrderIds) {
    try {
      await cleanupOrder(orderId);
    } catch (error) {
      console.warn(`Cleanup failed for order ${orderId}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  for (const customerId of cleanupCustomerIds) {
    try {
      await cleanupCustomer(customerId);
    } catch (error) {
      console.warn(
        `Cleanup failed for customer ${customerId}: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }
}
