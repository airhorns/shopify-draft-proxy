/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CapturedRequest = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'admin-platform');
const outputPath = path.join(outputDir, 'admin-node-customer-balance-node-read.json');

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'admin-platform', name), 'utf8');
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let cursor = value;
  for (const part of pathParts) {
    if (!isRecord(cursor)) {
      return undefined;
    }
    cursor = cursor[part];
  }
  return cursor;
}

function requireString(value: unknown, context: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing required capture value for ${context}: ${JSON.stringify(value)}`);
  }
  return value;
}

function requireNoTopLevelErrors(capture: CapturedRequest, context: string): void {
  if (capture.response.status < 200 || capture.response.status >= 300 || capture.response.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(capture.response, null, 2)}`);
  }
}

function requireNoUserErrors(capture: CapturedRequest, rootName: string, context: string): void {
  requireNoTopLevelErrors(capture, context);
  const userErrors = readPath(capture.response.payload, ['data', rootName, 'userErrors']);
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

async function capture(query: string, variables: JsonRecord): Promise<CapturedRequest> {
  return {
    query,
    variables,
    response: await runGraphqlRequest<JsonRecord>(query, variables),
  };
}

async function cleanupCustomer(customerId: string): Promise<CapturedRequest> {
  const query = `#graphql
    mutation AdminNodeCustomerBalanceCleanupCustomer($input: CustomerDeleteInput!) {
      customerDelete(input: $input) {
        deletedCustomerId
        userErrors {
          field
          message
        }
      }
    }
  `;
  return await capture(query, { input: { id: customerId } });
}

async function cleanupGiftCard(giftCardId: string): Promise<CapturedRequest> {
  const query = `#graphql
    mutation AdminNodeCustomerBalanceCleanupGiftCard($id: ID!) {
      giftCardDeactivate(id: $id) {
        giftCard {
          id
          enabled
          deactivatedAt
        }
        userErrors {
          field
          code
          message
        }
      }
    }
  `;
  return await capture(query, { id: giftCardId });
}

const customerCreateQuery = await readRequest('admin-node-customer-balance-customer-create.graphql');
const storeCreditCreditQuery = await readRequest('admin-node-customer-balance-store-credit-credit.graphql');
const storeCreditDebitQuery = await readRequest('admin-node-customer-balance-store-credit-debit.graphql');
const giftCardCreateQuery = await readRequest('admin-node-customer-balance-gift-card-create.graphql');
const giftCardCreditQuery = await readRequest('admin-node-customer-balance-gift-card-credit.graphql');
const giftCardDebitQuery = await readRequest('admin-node-customer-balance-gift-card-debit.graphql');
const nodeReadQuery = await readRequest('admin-node-customer-balance-node-read.graphql');
const giftCardConfigurationQuery = `#graphql
  query GiftCardCreateConfiguration {
    giftCardConfiguration {
      issueLimit { amount currencyCode }
      purchaseLimit { amount currencyCode }
    }
  }
`;

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const cleanup: JsonRecord = {};
let customerId: string | null = null;
let giftCardId: string | null = null;

try {
  const customerCreateVariables = {
    input: {
      email: `admin-node-${stamp}@example.com`,
      firstName: 'Admin',
      lastName: `Node ${stamp}`,
      addresses: [
        {
          address1: '190 MacLaren Street',
          city: 'Ottawa',
          countryCode: 'CA',
          provinceCode: 'ON',
          zip: 'K2P 0L6',
        },
      ],
    },
  };
  const customerCreate = await capture(customerCreateQuery, customerCreateVariables);
  requireNoUserErrors(customerCreate, 'customerCreate', 'customerCreate setup');
  customerId = requireString(
    readPath(customerCreate.response.payload, ['data', 'customerCreate', 'customer', 'id']),
    'customer id',
  );
  const addressId = requireString(
    readPath(customerCreate.response.payload, ['data', 'customerCreate', 'customer', 'defaultAddress', 'id']),
    'customer default address id',
  );

  const storeCreditCreditVariables = {
    id: customerId,
    creditInput: {
      creditAmount: {
        amount: '18.00',
        currencyCode: 'CAD',
      },
    },
  };
  const storeCreditCredit = await capture(storeCreditCreditQuery, storeCreditCreditVariables);
  requireNoUserErrors(storeCreditCredit, 'storeCreditAccountCredit', 'storeCreditAccountCredit setup');
  const storeCreditAccountId = requireString(
    readPath(storeCreditCredit.response.payload, [
      'data',
      'storeCreditAccountCredit',
      'storeCreditAccountTransaction',
      'account',
      'id',
    ]),
    'store credit account id',
  );
  const storeCreditCreditTransactionId = requireString(
    readPath(storeCreditCredit.response.payload, [
      'data',
      'storeCreditAccountCredit',
      'storeCreditAccountTransaction',
      'id',
    ]),
    'store credit credit transaction id',
  );

  const storeCreditDebitVariables = {
    id: storeCreditAccountId,
    debitInput: {
      debitAmount: {
        amount: '5.00',
        currencyCode: 'CAD',
      },
    },
  };
  const storeCreditDebit = await capture(storeCreditDebitQuery, storeCreditDebitVariables);
  requireNoUserErrors(storeCreditDebit, 'storeCreditAccountDebit', 'storeCreditAccountDebit setup');
  const storeCreditDebitTransactionId = requireString(
    readPath(storeCreditDebit.response.payload, [
      'data',
      'storeCreditAccountDebit',
      'storeCreditAccountTransaction',
      'id',
    ]),
    'store credit debit transaction id',
  );

  const giftCardConfiguration = await capture(giftCardConfigurationQuery, {});
  requireNoTopLevelErrors(giftCardConfiguration, 'giftCardCreate configuration hydrate');

  const giftCardCreateVariables = {
    input: {
      initialValue: '20.00',
      code: `NODE${stamp}`,
      note: `Admin Node gift card ${stamp}`,
    },
  };
  const giftCardCreate = await capture(giftCardCreateQuery, giftCardCreateVariables);
  requireNoUserErrors(giftCardCreate, 'giftCardCreate', 'giftCardCreate setup');
  giftCardId = requireString(
    readPath(giftCardCreate.response.payload, ['data', 'giftCardCreate', 'giftCard', 'id']),
    'gift card id',
  );

  const giftCardCreditVariables = {
    id: giftCardId,
    creditInput: {
      creditAmount: {
        amount: '4.00',
        currencyCode: 'CAD',
      },
      note: `Admin Node gift card credit ${stamp}`,
    },
  };
  const giftCardCredit = await capture(giftCardCreditQuery, giftCardCreditVariables);
  requireNoUserErrors(giftCardCredit, 'giftCardCredit', 'giftCardCredit setup');
  const giftCardCreditTransactionId = requireString(
    readPath(giftCardCredit.response.payload, ['data', 'giftCardCredit', 'giftCardCreditTransaction', 'id']),
    'gift card credit transaction id',
  );

  const giftCardDebitVariables = {
    id: giftCardId,
    debitInput: {
      debitAmount: {
        amount: '3.00',
        currencyCode: 'CAD',
      },
      note: `Admin Node gift card debit ${stamp}`,
    },
  };
  const giftCardDebit = await capture(giftCardDebitQuery, giftCardDebitVariables);
  requireNoUserErrors(giftCardDebit, 'giftCardDebit', 'giftCardDebit setup');
  const giftCardDebitTransactionId = requireString(
    readPath(giftCardDebit.response.payload, ['data', 'giftCardDebit', 'giftCardDebitTransaction', 'id']),
    'gift card debit transaction id',
  );

  const nodeReadVariables = {
    ids: [
      customerId,
      addressId,
      storeCreditAccountId,
      storeCreditCreditTransactionId,
      storeCreditDebitTransactionId,
      giftCardId,
      giftCardCreditTransactionId,
      giftCardDebitTransactionId,
    ],
    missingIds: [
      'gid://shopify/Customer/999999999999999',
      'gid://shopify/MailingAddress/999999999999999',
      'gid://shopify/StoreCreditAccount/999999999999999',
      'gid://shopify/StoreCreditAccountCreditTransaction/999999999999999',
      'gid://shopify/StoreCreditAccountDebitTransaction/999999999999999',
      'gid://shopify/GiftCard/999999999999999',
      'gid://shopify/GiftCardCreditTransaction/999999999999999',
      'gid://shopify/GiftCardDebitTransaction/999999999999999',
    ],
  };
  const liveNodeRead = await capture(nodeReadQuery, nodeReadVariables);
  requireNoTopLevelErrors(liveNodeRead, 'generic Node read-after-write');

  const cleanupCreditBalanceVariables = {
    id: storeCreditAccountId,
    debitInput: {
      debitAmount: {
        amount: '13.00',
        currencyCode: 'CAD',
      },
    },
  };
  cleanup['storeCreditBalanceCleanupDebit'] = await capture(storeCreditDebitQuery, cleanupCreditBalanceVariables);
  if (customerId) {
    cleanup['customerDelete'] = await cleanupCustomer(customerId);
  }
  if (giftCardId) {
    cleanup['giftCardDeactivate'] = await cleanupGiftCard(giftCardId);
  }

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'admin-node-customer-balance-node-read',
    notes:
      'Live Shopify capture for generic Relay Node reads after public customer, store-credit, and gift-card writes. The script creates a disposable customer with an address, credits and debits the customer store-credit account, creates a disposable gift card, credits and debits it, reads all resulting IDs through nodes(ids:), captures never-created null entries, then performs best-effort cleanup.',
    setup: {
      customerCreate: {
        query: customerCreateQuery,
        variables: customerCreateVariables,
        response: customerCreate.response.payload,
      },
      storeCreditCredit: {
        query: storeCreditCreditQuery,
        variables: storeCreditCreditVariables,
        response: storeCreditCredit.response.payload,
      },
      storeCreditDebit: {
        query: storeCreditDebitQuery,
        variables: storeCreditDebitVariables,
        response: storeCreditDebit.response.payload,
      },
      giftCardConfiguration: {
        query: giftCardConfigurationQuery,
        variables: {},
        response: giftCardConfiguration.response.payload,
      },
      giftCardCreate: {
        query: giftCardCreateQuery,
        variables: giftCardCreateVariables,
        response: giftCardCreate.response.payload,
      },
      giftCardCredit: {
        query: giftCardCreditQuery,
        variables: giftCardCreditVariables,
        response: giftCardCredit.response.payload,
      },
      giftCardDebit: {
        query: giftCardDebitQuery,
        variables: giftCardDebitVariables,
        response: giftCardDebit.response.payload,
      },
    },
    nodeReads: {
      live: {
        query: nodeReadQuery,
        variables: nodeReadVariables,
        response: liveNodeRead.response.payload,
      },
    },
    cleanup,
    knownBlockedTargets: {
      customerPaymentMethod:
        'The active conformance token still lacks read_customer_payment_methods/write_customer_payment_methods, and disposable vaulted payment material is not available in unattended capture.',
      storeCreditAccountDebitRevertTransaction:
        'The checked-in public Admin mutation schema exposes StoreCreditAccountDebitRevertTransaction as a Node implementor but no disposable public mutation path that creates one.',
    },
    upstreamCalls: [
      {
        operationName: 'GiftCardCreateConfiguration',
        variables: {},
        query: giftCardConfigurationQuery,
        response: {
          status: giftCardConfiguration.response.status,
          body: giftCardConfiguration.response.payload,
        },
      },
    ],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  if (customerId) {
    cleanup['customerDeleteAfterError'] = await cleanupCustomer(customerId).catch((cleanupError: unknown) => ({
      error: String(cleanupError),
    }));
  }
  if (giftCardId) {
    cleanup['giftCardDeactivateAfterError'] = await cleanupGiftCard(giftCardId).catch((cleanupError: unknown) => ({
      error: String(cleanupError),
    }));
  }
  throw error;
}
