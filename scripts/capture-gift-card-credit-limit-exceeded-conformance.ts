/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedRequest = {
  label: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

type RecordedCall = {
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'gift-cards');
const outputPath = path.join(outputDir, 'gift-card-credit-limit-exceeded.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readPath(value: unknown, pathParts: string[]): unknown {
  return pathParts.reduce<unknown>((cursor, part) => {
    if (!isObject(cursor)) {
      return undefined;
    }
    return cursor[part];
  }, value);
}

function readStringPath(value: unknown, pathParts: string[]): string | null {
  const found = readPath(value, pathParts);
  return typeof found === 'string' ? found : null;
}

function readCreatedGiftCardId(capture: CapturedRequest): string | null {
  return readStringPath(capture.response.payload, ['data', 'giftCardCreate', 'giftCard', 'id']);
}

function decimalToCents(amount: string): bigint {
  const trimmed = amount.trim();
  const match = /^(\d+)(?:\.(\d{0,2}))?$/u.exec(trimmed);
  if (match === null) {
    throw new Error(`unsupported gift-card issue limit amount: ${amount}`);
  }
  const wholePart = match[1];
  if (wholePart === undefined) {
    throw new Error(`unsupported gift-card issue limit amount: ${amount}`);
  }
  const whole = BigInt(wholePart);
  const cents = BigInt((match[2] ?? '').padEnd(2, '0'));
  return whole * 100n + cents;
}

function centsToDecimal(cents: bigint): string {
  const whole = cents / 100n;
  const fractional = cents % 100n;
  if (fractional === 0n) {
    return `${whole.toString()}.0`;
  }
  return `${whole.toString()}.${fractional.toString().padStart(2, '0')}`;
}

async function capture(
  label: string,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<CapturedRequest> {
  const response = await runGraphqlRequest(query, variables);
  return { label, query, variables, response };
}

async function deactivateGiftCard(label: string, id: string): Promise<CapturedRequest> {
  return capture(
    label,
    `#graphql
      mutation GiftCardCreditLimitCleanup($id: ID!) {
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
    `,
    { id },
  );
}

async function hydrateGiftCard(id: string): Promise<RecordedCall> {
  const variables = { id };
  const query = `#graphql
    query GiftCardHydrate($id: ID!) {
      giftCard(id: $id) {
        id
        lastCharacters
        maskedCode
        enabled
        deactivatedAt
        expiresOn
        note
        templateSuffix
        createdAt
        updatedAt
        initialValue { amount currencyCode }
        balance { amount currencyCode }
        customer {
          id
          email
          defaultEmailAddress { emailAddress }
          defaultPhoneNumber { phoneNumber }
        }
        recipientAttributes {
          message
          preferredName
          sendNotificationAt
          recipient {
            id
            email
            defaultEmailAddress { emailAddress }
            defaultPhoneNumber { phoneNumber }
          }
        }
        transactions(first: 250) {
          nodes {
            __typename
            id
            note
            processedAt
            amount { amount currencyCode }
          }
        }
      }
      giftCardConfiguration {
        issueLimit { amount currencyCode }
        purchaseLimit { amount currencyCode }
      }
    }
  `;
  const response = await runGraphqlRequest(query, variables);
  return {
    operationName: 'GiftCardHydrate',
    variables,
    query,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

const configurationQuery = `#graphql
  query GiftCardCreditLimitConfiguration {
    giftCardConfiguration {
      issueLimit { amount currencyCode }
      purchaseLimit { amount currencyCode }
    }
  }
`;

const createMutation = `#graphql
  mutation GiftCardCreditLimitSetup($input: GiftCardCreateInput!) {
    giftCardCreate(input: $input) {
      giftCard {
        id
        initialValue { amount currencyCode }
        balance { amount currencyCode }
      }
      userErrors {
        field
        code
        message
      }
    }
  }
`;

const creditMutation = `#graphql
  mutation GiftCardCreditLimitExceeded($id: ID!, $input: GiftCardCreditInput!) {
    overLimitCredit: giftCardCredit(id: $id, creditInput: $input) {
      giftCardCreditTransaction {
        __typename
        amount { amount currencyCode }
      }
      userErrors {
        field
        code
        message
      }
    }
  }
`;

const debitMutation = `#graphql
  mutation GiftCardDebitAfterRejectedCredit($id: ID!, $input: GiftCardDebitInput!) {
    debitAfterRejectedCredit: giftCardDebit(id: $id, debitInput: $input) {
      giftCardDebitTransaction {
        __typename
        amount { amount currencyCode }
      }
      userErrors {
        field
        code
        message
      }
    }
  }
`;

const readAfterCreditQuery = `#graphql
  query GiftCardReadAfterRejectedCredit($id: ID!) {
    giftCard(id: $id) {
      id
      balance { amount currencyCode }
      transactions(first: 10) {
        nodes {
          __typename
          amount { amount currencyCode }
        }
      }
    }
  }
`;

const configurationRead = await capture('configurationRead', configurationQuery);
const issueLimitAmount = readStringPath(configurationRead.response.payload, [
  'data',
  'giftCardConfiguration',
  'issueLimit',
  'amount',
]);
const issueLimitCurrency = readStringPath(configurationRead.response.payload, [
  'data',
  'giftCardConfiguration',
  'issueLimit',
  'currencyCode',
]);
const purchaseLimitAmount = readStringPath(configurationRead.response.payload, [
  'data',
  'giftCardConfiguration',
  'purchaseLimit',
  'amount',
]);
const purchaseLimitCurrency = readStringPath(configurationRead.response.payload, [
  'data',
  'giftCardConfiguration',
  'purchaseLimit',
  'currencyCode',
]);

if (issueLimitAmount === null || issueLimitCurrency === null) {
  throw new Error('giftCardConfiguration.issueLimit was not readable from the conformance shop.');
}

const issueLimitCents = decimalToCents(issueLimitAmount);
if (issueLimitCents <= 0n) {
  throw new Error(
    `giftCardConfiguration.issueLimit must be non-zero for credit limit capture; got ${issueLimitAmount} ${issueLimitCurrency}.`,
  );
}

const overByCent = centsToDecimal(1n);
const boundaryValue = centsToDecimal(issueLimitCents);
const setupCreate = await capture('setupCreate', createMutation, {
  input: {
    initialValue: boundaryValue,
    code: `CREDLIM${Date.now().toString(36).toUpperCase()}`,
    note: 'Gift card credit limit conformance setup',
    expiresOn: '2099-01-01',
  },
});

const boundaryId = readCreatedGiftCardId(setupCreate);
if (boundaryId === null) {
  throw new Error('setup giftCardCreate did not return a giftCard.id; inspect setupCreate response.');
}

const upstreamCalls = [await hydrateGiftCard(boundaryId)];
const creditInput = {
  creditAmount: {
    amount: overByCent,
    currencyCode: issueLimitCurrency,
  },
};
const debitInput = {
  debitAmount: {
    amount: overByCent,
    currencyCode: issueLimitCurrency,
  },
};

const creditLimitExceeded = await capture('creditLimitExceeded', creditMutation, {
  id: boundaryId,
  input: creditInput,
});
const readAfterRejectedCredit = await capture('readAfterRejectedCredit', readAfterCreditQuery, {
  id: boundaryId,
});
const debitAfterRejectedCredit = await capture('debitAfterRejectedCredit', debitMutation, {
  id: boundaryId,
  input: debitInput,
});
const cleanup = [await deactivateGiftCard('cleanupDeactivate', boundaryId)];

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'Captures giftCardCredit validation when a card at the configured issue limit is credited by one cent.',
        `The captured shop issue limit is ${issueLimitAmount} ${issueLimitCurrency}; purchaseLimit is ${purchaseLimitAmount ?? 'unreadable'} ${purchaseLimitCurrency ?? ''}.`,
        'The setup gift card is created at the exact issue limit, over-limit credit is expected to reject without adding a transaction, and cleanup deactivates the setup card.',
        'A one-cent debit after the rejected credit is captured to confirm debit decreases the balance and does not surface GIFT_CARD_LIMIT_EXCEEDED in this public Admin path.',
      ],
      proxyVariables: {
        creditLimitExceeded: {
          boundaryId,
          creditInput,
          debitInput,
        },
      },
      operations: {
        configurationRead,
        setupCreate,
        creditLimitExceeded,
        readAfterRejectedCredit,
        debitAfterRejectedCredit,
      },
      cleanup,
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      outputPath,
      boundaryId,
      issueLimitAmount,
      issueLimitCurrency,
      purchaseLimitAmount,
      purchaseLimitCurrency,
      creditUserErrors: readPath(creditLimitExceeded.response.payload, ['data', 'overLimitCredit', 'userErrors']),
      debitUserErrors: readPath(debitAfterRejectedCredit.response.payload, [
        'data',
        'debitAfterRejectedCredit',
        'userErrors',
      ]),
    },
    null,
    2,
  ),
);
