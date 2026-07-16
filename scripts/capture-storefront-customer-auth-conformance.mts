/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { randomUUID } from 'node:crypto';

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

const scenarioId = 'storefront-customer-auth-lifecycle';
const redacted = '<redacted:storefront-customer-auth>';
const documentPaths = {
  adminActivationUrl: 'config/parity-requests/storefront/storefront-customer-auth-admin-activation-url.graphql',
  adminCreate: 'config/parity-requests/storefront/storefront-customer-auth-admin-create.graphql',
  adminDelete: 'config/parity-requests/storefront/storefront-customer-auth-admin-delete.graphql',
  adminUpdate: 'config/parity-requests/storefront/storefront-customer-auth-admin-update.graphql',
  activate: 'config/parity-requests/storefront/storefront-customer-auth-activate.graphql',
  activateByUrl: 'config/parity-requests/storefront/storefront-customer-auth-activate-by-url.graphql',
  create: 'config/parity-requests/storefront/storefront-customer-auth-create.graphql',
  multipass: 'config/parity-requests/storefront/storefront-customer-auth-multipass.graphql',
  read: 'config/parity-requests/storefront/storefront-customer-auth-read.graphql',
  recover: 'config/parity-requests/storefront/storefront-customer-auth-recover.graphql',
  reset: 'config/parity-requests/storefront/storefront-customer-auth-reset.graphql',
  resetByUrl: 'config/parity-requests/storefront/storefront-customer-auth-reset-by-url.graphql',
  tokenCreate: 'config/parity-requests/storefront/storefront-customer-auth-token-create.graphql',
  tokenDelete: 'config/parity-requests/storefront/storefront-customer-auth-token-delete.graphql',
  tokenRenew: 'config/parity-requests/storefront/storefront-customer-auth-token-renew.graphql',
} as const;

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
const storefrontCustomerEmail = `hermes-storefront-auth-${suffix}@example.com`;
const adminCustomerEmail = `hermes-storefront-admin-${suffix}@example.com`;
const missingCustomerEmail = `hermes-storefront-missing-${suffix}@example.com`;
const password = 'CodexPass123!';
const updatedPassword = 'NewCodexPass123!';
const cleanupCustomerIds = new Set<string>();

function redactSensitive(value: unknown, key?: string): unknown {
  if (
    typeof value === 'string' &&
    key !== undefined &&
    [
      'accessToken',
      'accountActivationUrl',
      'activationToken',
      'activationUrl',
      'customerAccessToken',
      'deletedAccessToken',
      'multipassToken',
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

function activationTokenFromUrl(activationUrl: string): string {
  const token = activationUrl.split('/').pop();
  if (!token) throw new Error(`Could not parse activation token from ${activationUrl}`);
  return token;
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

async function cleanupCustomer(customerId: string) {
  const query = await readDocument(documentPaths.adminDelete);
  return adminClient.runGraphqlRequest(query, { input: { id: customerId } });
}

try {
  const storefrontCreate = await recordStorefront('StorefrontCustomerAuthCreate', documentPaths.create, {
    input: {
      email: storefrontCustomerEmail,
      password,
      firstName: 'Storefront',
      lastName: 'Auth',
      acceptsMarketing: true,
    },
  });
  const storefrontCustomerId = pathString(
    storefrontCreate.raw,
    ['data', 'customerCreate', 'customer', 'id'],
    'storefront customer id',
  );
  cleanupCustomerIds.add(storefrontCustomerId);

  const duplicateCreate = await recordStorefront('StorefrontCustomerAuthCreate', documentPaths.create, {
    input: {
      email: storefrontCustomerEmail,
      password,
    },
  });
  const invalidEmailCreate = await recordStorefront('StorefrontCustomerAuthCreate', documentPaths.create, {
    input: {
      email: 'not-an-email',
      password,
    },
  });
  const blankPasswordCreate = await recordStorefront('StorefrontCustomerAuthCreate', documentPaths.create, {
    input: {
      email: `hermes-storefront-blank-${suffix}@example.com`,
      password: '',
    },
  });
  const htmlNameCreate = await recordStorefront('StorefrontCustomerAuthCreate', documentPaths.create, {
    input: {
      email: `hermes-storefront-html-${suffix}@example.com`,
      password,
      firstName: '<b>Bad</b>',
    },
  });
  const tokenInvalid = await recordStorefront('StorefrontCustomerAuthTokenCreate', documentPaths.tokenCreate, {
    input: {
      email: storefrontCustomerEmail,
      password: 'wrong-password',
    },
  });
  const tokenValid = await recordStorefront('StorefrontCustomerAuthTokenCreate', documentPaths.tokenCreate, {
    input: {
      email: storefrontCustomerEmail,
      password,
    },
  });
  const storefrontAccessToken = pathString(
    tokenValid.raw,
    ['data', 'customerAccessTokenCreate', 'customerAccessToken', 'accessToken'],
    'storefront access token',
  );
  const customerRead = await recordStorefront('StorefrontCustomerAuthRead', documentPaths.read, {
    token: storefrontAccessToken,
  });
  const tokenRenew = await recordStorefront('StorefrontCustomerAuthTokenRenew', documentPaths.tokenRenew, {
    token: storefrontAccessToken,
  });
  const tokenDelete = await recordStorefront('StorefrontCustomerAuthTokenDelete', documentPaths.tokenDelete, {
    token: storefrontAccessToken,
  });
  const customerReadAfterTokenDelete = await recordStorefront('StorefrontCustomerAuthRead', documentPaths.read, {
    token: storefrontAccessToken,
  });
  const tokenDeleteAgain = await recordStorefront('StorefrontCustomerAuthTokenDelete', documentPaths.tokenDelete, {
    token: storefrontAccessToken,
  });
  const multipassInvalid = await recordStorefront('StorefrontCustomerAuthMultipass', documentPaths.multipass, {
    multipassToken: `invalid-multipass-${suffix}`,
  });

  const adminCreate = await recordAdmin('StorefrontCustomerAuthAdminCreate', documentPaths.adminCreate, {
    input: {
      email: adminCustomerEmail,
      firstName: 'Admin',
      lastName: 'Auth',
    },
  });
  const adminCustomerId = pathString(
    adminCreate.raw,
    ['data', 'customerCreate', 'customer', 'id'],
    'admin customer id',
  );
  cleanupCustomerIds.add(adminCustomerId);
  const disabledTokenCreate = await recordStorefront('StorefrontCustomerAuthTokenCreate', documentPaths.tokenCreate, {
    input: {
      email: adminCustomerEmail,
      password,
    },
  });
  const activationUrl = await recordAdmin(
    'StorefrontCustomerAuthAdminActivationUrl',
    documentPaths.adminActivationUrl,
    {
      customerId: adminCustomerId,
    },
  );
  const liveActivationUrl = pathString(
    activationUrl.raw,
    ['data', 'customerGenerateAccountActivationUrl', 'accountActivationUrl'],
    'activation URL',
  );
  const liveActivationToken = activationTokenFromUrl(liveActivationUrl);
  const activateInvalid = await recordStorefront('StorefrontCustomerAuthActivate', documentPaths.activate, {
    id: adminCustomerId,
    input: {
      activationToken: 'bad-token',
      password,
    },
  });
  const activateByUrl = await recordStorefront('StorefrontCustomerAuthActivateByUrl', documentPaths.activateByUrl, {
    activationUrl: liveActivationUrl,
    password,
  });
  const activatedAccessToken = pathString(
    activateByUrl.raw,
    ['data', 'customerActivateByUrl', 'customerAccessToken', 'accessToken'],
    'activated customer access token',
  );
  const activateAgain = await recordStorefront('StorefrontCustomerAuthActivate', documentPaths.activate, {
    id: adminCustomerId,
    input: {
      activationToken: liveActivationToken,
      password,
    },
  });
  const recoverExisting = await recordStorefront('StorefrontCustomerAuthRecover', documentPaths.recover, {
    email: adminCustomerEmail,
  });
  const resetInvalid = await recordStorefront('StorefrontCustomerAuthReset', documentPaths.reset, {
    id: adminCustomerId,
    input: {
      resetToken: 'bad-token',
      password: updatedPassword,
    },
  });
  const resetByUrlInvalid = await recordStorefront('StorefrontCustomerAuthResetByUrl', documentPaths.resetByUrl, {
    resetUrl: `https://${storeDomain}/account/reset/bad-token`,
    password: updatedPassword,
  });
  const adminUpdate = await recordAdmin('StorefrontCustomerAuthAdminUpdate', documentPaths.adminUpdate, {
    input: {
      id: adminCustomerId,
      firstName: 'Updated',
      lastName: 'Auth',
    },
  });
  const customerReadAfterAdminUpdate = await recordStorefront('StorefrontCustomerAuthRead', documentPaths.read, {
    token: activatedAccessToken,
  });
  const adminDelete = await recordAdmin('StorefrontCustomerAuthAdminDelete', documentPaths.adminDelete, {
    input: {
      id: adminCustomerId,
    },
  });
  cleanupCustomerIds.delete(adminCustomerId);
  const customerReadAfterAdminDelete = await recordStorefront('StorefrontCustomerAuthRead', documentPaths.read, {
    token: activatedAccessToken,
  });
  const recoverMissing = await recordStorefront('StorefrontCustomerAuthRecover', documentPaths.recover, {
    email: missingCustomerEmail,
  });

  const cleanupResponses = [];
  for (const customerId of cleanupCustomerIds) {
    const response = await cleanupCustomer(customerId);
    cleanupResponses.push({
      customerId,
      response: {
        status: response.status,
        body: redactSensitive(response.payload),
      },
    });
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
          storefrontCustomerEmail,
          adminCustomerEmail,
          missingCustomerEmail,
        },
        captures: {
          storefrontCreate: storefrontCreate.record,
          duplicateCreate: duplicateCreate.record,
          invalidEmailCreate: invalidEmailCreate.record,
          blankPasswordCreate: blankPasswordCreate.record,
          htmlNameCreate: htmlNameCreate.record,
          tokenInvalid: tokenInvalid.record,
          tokenValid: tokenValid.record,
          customerRead: customerRead.record,
          tokenRenew: tokenRenew.record,
          tokenDelete: tokenDelete.record,
          customerReadAfterTokenDelete: customerReadAfterTokenDelete.record,
          tokenDeleteAgain: tokenDeleteAgain.record,
          multipassInvalid: multipassInvalid.record,
          adminCreate: adminCreate.record,
          disabledTokenCreate: disabledTokenCreate.record,
          activationUrl: activationUrl.record,
          activateInvalid: activateInvalid.record,
          activateByUrl: activateByUrl.record,
          activateAgain: activateAgain.record,
          recoverExisting: recoverExisting.record,
          resetInvalid: resetInvalid.record,
          resetByUrlInvalid: resetByUrlInvalid.record,
          adminUpdate: adminUpdate.record,
          customerReadAfterAdminUpdate: customerReadAfterAdminUpdate.record,
          adminDelete: adminDelete.record,
          customerReadAfterAdminDelete: customerReadAfterAdminDelete.record,
          recoverMissing: recoverMissing.record,
        },
        cleanup: {
          customerDeletes: cleanupResponses,
        },
        upstreamCalls: [],
        notes: [
          'Disposable Storefront and Admin customers were created against live Shopify and deleted through Admin GraphQL cleanup.',
          'Passwords, Storefront access tokens, activation URLs, reset URLs, Multipass tokens, and deleted token values are redacted in this checked-in fixture.',
          'No email contents are captured. Recovery success records only Shopify mutation payloads; local reset success remains runtime-test-backed because live reset URLs require mailbox access.',
        ],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(`Wrote ${outputPath}`);
  console.log(`Captured Storefront customer auth lifecycle for ${storefrontCustomerEmail} and ${adminCustomerEmail}`);
} finally {
  for (const customerId of cleanupCustomerIds) {
    try {
      await cleanupCustomer(customerId);
    } catch (error) {
      console.warn(`Cleanup failed for ${customerId}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}
