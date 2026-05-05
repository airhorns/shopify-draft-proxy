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
const outputPath = path.join(outputDir, 'gift-card-transaction-validation.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readGiftCardId(capture: CapturedRequest): string | null {
  const data = capture.response.payload.data;
  if (!isObject(data)) return null;
  const payload = data['giftCardCreate'];
  if (!isObject(payload)) return null;
  const giftCard = payload['giftCard'];
  if (!isObject(giftCard)) return null;
  const id = giftCard['id'];
  return typeof id === 'string' ? id : null;
}

function readGiftCardCurrency(capture: CapturedRequest): string | null {
  const data = capture.response.payload.data;
  if (!isObject(data)) return null;
  const payload = data['giftCardCreate'];
  if (!isObject(payload)) return null;
  const giftCard = payload['giftCard'];
  if (!isObject(giftCard)) return null;
  const balance = giftCard['balance'];
  if (!isObject(balance)) return null;
  const currencyCode = balance['currencyCode'];
  return typeof currencyCode === 'string' ? currencyCode : null;
}

async function capture(
  label: string,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<CapturedRequest> {
  const response = await runGraphqlRequest(query, variables);
  return { label, query, variables, response };
}

const transactionSelection = `#graphql
  id
  __typename
  processedAt
  amount {
    amount
    currencyCode
  }
`;

async function createGiftCard(label: string, expiresOn: string): Promise<CapturedRequest> {
  const code = `H690${label[0]?.toUpperCase() ?? 'X'}${Date.now().toString(36)}`;
  return capture(
    label,
    `#graphql
      mutation GiftCardCreate($input: GiftCardCreateInput!) {
        giftCardCreate(input: $input) {
          giftCard {
            id
            enabled
            expiresOn
            balance {
              amount
              currencyCode
            }
          }
          userErrors {
            field
            code
            message
          }
        }
      }
    `,
    {
      input: {
        initialValue: '10.00',
        code,
        note: `HAR-690 ${label} validation gift card`,
        expiresOn,
      },
    },
  );
}

async function deactivateGiftCard(label: string, id: string): Promise<CapturedRequest> {
  return capture(
    label,
    `#graphql
      mutation GiftCardDeactivate($id: ID!) {
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

const setupActive = await createGiftCard('active', '2099-01-01');
const setupExpired = await createGiftCard('expired', '2020-01-01');
const setupDeactivated = await createGiftCard('deactivated', '2099-01-01');

const activeId = readGiftCardId(setupActive);
const expiredId = readGiftCardId(setupExpired);
const deactivatedId = readGiftCardId(setupDeactivated);
const cardCurrency = readGiftCardCurrency(setupActive) ?? 'CAD';
const mismatchCurrency = cardCurrency === 'EUR' ? 'USD' : 'EUR';
const setupDeactivate = deactivatedId === null ? null : await deactivateGiftCard('setupDeactivate', deactivatedId);
const upstreamCalls: RecordedCall[] = [];

if (activeId !== null && expiredId !== null && deactivatedId !== null) {
  upstreamCalls.push(await hydrateGiftCard(expiredId));
  upstreamCalls.push(await hydrateGiftCard(deactivatedId));
  upstreamCalls.push(await hydrateGiftCard(activeId));
}

const validCreditInput = {
  creditAmount: {
    amount: '5.00',
    currencyCode: cardCurrency,
  },
};
const mismatchCreditInput = {
  creditAmount: {
    amount: '5.00',
    currencyCode: mismatchCurrency,
  },
};
const futureCreditInput = {
  processedAt: '2099-01-01T00:00:00Z',
  creditAmount: {
    amount: '5.00',
    currencyCode: cardCurrency,
  },
};
const preEpochCreditInput = {
  processedAt: '1969-12-31T23:59:59Z',
  creditAmount: {
    amount: '5.00',
    currencyCode: cardCurrency,
  },
};
const validDebitInput = {
  debitAmount: {
    amount: '5.00',
    currencyCode: cardCurrency,
  },
};

const operations: Record<string, CapturedRequest> = {};

if (activeId !== null && expiredId !== null && deactivatedId !== null) {
  operations['expiredCredit'] = await capture(
    'expiredCredit',
    `#graphql
      mutation GiftCardCredit($id: ID!, $input: GiftCardCreditInput!) {
        giftCardCredit(id: $id, creditInput: $input) {
          giftCardCreditTransaction {
            ${transactionSelection}
          }
          userErrors {
            field
            code
            message
          }
        }
      }
    `,
    { id: expiredId, input: validCreditInput },
  );
  operations['deactivatedCredit'] = await capture(
    'deactivatedCredit',
    `#graphql
      mutation GiftCardCredit($id: ID!, $input: GiftCardCreditInput!) {
        giftCardCredit(id: $id, creditInput: $input) {
          giftCardCreditTransaction {
            ${transactionSelection}
          }
          userErrors {
            field
            code
            message
          }
        }
      }
    `,
    { id: deactivatedId, input: validCreditInput },
  );
  operations['mismatchCredit'] = await capture(
    'mismatchCredit',
    `#graphql
      mutation GiftCardCredit($id: ID!, $input: GiftCardCreditInput!) {
        giftCardCredit(id: $id, creditInput: $input) {
          giftCardCreditTransaction {
            ${transactionSelection}
          }
          userErrors {
            field
            code
            message
          }
        }
      }
    `,
    { id: activeId, input: mismatchCreditInput },
  );
  operations['futureCredit'] = await capture(
    'futureCredit',
    `#graphql
      mutation GiftCardCredit($id: ID!, $input: GiftCardCreditInput!) {
        giftCardCredit(id: $id, creditInput: $input) {
          giftCardCreditTransaction {
            ${transactionSelection}
          }
          userErrors {
            field
            code
            message
          }
        }
      }
    `,
    { id: activeId, input: futureCreditInput },
  );
  operations['preEpochCredit'] = await capture(
    'preEpochCredit',
    `#graphql
      mutation GiftCardCredit($id: ID!, $input: GiftCardCreditInput!) {
        giftCardCredit(id: $id, creditInput: $input) {
          giftCardCreditTransaction {
            ${transactionSelection}
          }
          userErrors {
            field
            code
            message
          }
        }
      }
    `,
    { id: activeId, input: preEpochCreditInput },
  );
  operations['deactivatedDebit'] = await capture(
    'deactivatedDebit',
    `#graphql
      mutation GiftCardDebit($id: ID!, $input: GiftCardDebitInput!) {
        giftCardDebit(id: $id, debitInput: $input) {
          giftCardDebitTransaction {
            ${transactionSelection}
          }
          userErrors {
            field
            code
            message
          }
        }
      }
    `,
    { id: deactivatedId, input: validDebitInput },
  );
  operations['successCredit'] = await capture(
    'successCredit',
    `#graphql
      mutation GiftCardCredit($id: ID!, $input: GiftCardCreditInput!) {
        giftCardCredit(id: $id, creditInput: $input) {
          giftCardCreditTransaction {
            ${transactionSelection}
          }
          userErrors {
            field
            code
            message
          }
        }
      }
    `,
    { id: activeId, input: validCreditInput },
  );
}

const cleanup: CapturedRequest[] = [];
for (const [label, id] of [
  ['cleanupActive', activeId],
  ['cleanupExpired', expiredId],
] as const) {
  if (id !== null) {
    cleanup.push(await deactivateGiftCard(label, id));
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'HAR-690 captures giftCardCredit/giftCardDebit validation branches for expired, deactivated, mismatched currency, future processedAt, pre-epoch processedAt, and typed success payload behavior.',
        'Setup creates three disposable gift cards: active, expired, and deactivated. Cleanup deactivates any setup cards that are not already deactivated.',
      ],
      proxyVariables: {
        transactionValidation: {
          activeId,
          expiredId,
          deactivatedId,
          validCreditInput,
          mismatchCreditInput,
          futureCreditInput,
          preEpochCreditInput,
          validDebitInput,
        },
      },
      setup: {
        active: setupActive,
        expired: setupExpired,
        deactivated: setupDeactivated,
        deactivate: setupDeactivate,
      },
      operations,
      cleanup,
      upstreamCalls,
      blocked:
        activeId === null || expiredId === null || deactivatedId === null
          ? {
              reason: 'One or more setup giftCardCreate calls did not return a giftCard.id; inspect setup responses.',
            }
          : null,
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
      activeId,
      expiredId,
      deactivatedId,
      operationLabels: Object.keys(operations),
      cleanupLabels: cleanup.map((entry) => entry.label),
    },
    null,
    2,
  ),
);
