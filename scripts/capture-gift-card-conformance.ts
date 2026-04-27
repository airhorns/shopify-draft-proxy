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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const outputPath = path.join(outputDir, 'gift-card-lifecycle.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readCreatedGiftCardId(createCapture: CapturedRequest): string | null {
  const data = createCapture.response.payload.data;
  if (!isObject(data)) {
    return null;
  }

  const payload = data['giftCardCreate'];
  if (!isObject(payload)) {
    return null;
  }

  const giftCard = payload['giftCard'];
  if (!isObject(giftCard)) {
    return null;
  }

  const id = giftCard['id'];
  return typeof id === 'string' ? id : null;
}

async function capture(
  label: string,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<CapturedRequest> {
  const response = await runGraphqlRequest(query, variables);
  return {
    label,
    query,
    variables,
    response,
  };
}

const giftCardSelection = `#graphql
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
  initialValue {
    amount
    currencyCode
  }
  balance {
    amount
    currencyCode
  }
`;

const schemaAndAccess = await capture(
  'schemaAndAccess',
  `#graphql
    query GiftCardSchemaAndAccess {
      currentAppInstallation {
        accessScopes {
          handle
        }
      }
      giftCard: __type(name: "GiftCard") {
        fields {
          name
        }
      }
      giftCardCreateInput: __type(name: "GiftCardCreateInput") {
        inputFields {
          name
        }
      }
      giftCardCreditInput: __type(name: "GiftCardCreditInput") {
        inputFields {
          name
        }
      }
      giftCardDebitInput: __type(name: "GiftCardDebitInput") {
        inputFields {
          name
        }
      }
      giftCardCreditPayload: __type(name: "GiftCardCreditPayload") {
        fields {
          name
        }
      }
      giftCardDebitPayload: __type(name: "GiftCardDebitPayload") {
        fields {
          name
        }
      }
      giftCardConfiguration: __type(name: "GiftCardConfiguration") {
        fields {
          name
          type {
            kind
            name
            ofType {
              kind
              name
            }
          }
        }
      }
    }
  `,
);

const unknownId = 'gid://shopify/GiftCard/999999999999';
const emptyRead = await capture(
  'emptyRead',
  `#graphql
    query GiftCardEmptyRead($unknownId: ID!) {
      giftCard(id: $unknownId) {
        id
      }
      giftCards(first: 2, sortKey: ID) {
        nodes {
          id
          lastCharacters
        }
        edges {
          cursor
          node {
            id
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      giftCardsCount {
        count
        precision
      }
      giftCardConfiguration {
        issueLimit {
          amount
          currencyCode
        }
        purchaseLimit {
          amount
          currencyCode
        }
      }
    }
  `,
  { unknownId },
);

const filteredEmptyRead = await capture(
  'filteredEmptyRead',
  `#graphql
    query GiftCardFilteredEmptyRead($query: String!) {
      giftCards(first: 2, query: $query, sortKey: ID) {
        nodes {
          id
          lastCharacters
        }
        edges {
          cursor
          node {
            id
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      giftCardsCount(query: $query) {
        count
        precision
      }
    }
  `,
  { query: 'id:999999999999' },
);

const configurationRead = await capture(
  'configurationRead',
  `#graphql
    query GiftCardConfigurationRead {
      giftCardConfiguration {
        issueLimit {
          amount
          currencyCode
        }
        purchaseLimit {
          amount
          currencyCode
        }
      }
    }
  `,
);

const create = await capture(
  'create',
  `#graphql
    mutation GiftCardCreate($input: GiftCardCreateInput!) {
      giftCardCreate(input: $input) {
        giftCard {
          ${giftCardSelection}
        }
        giftCardCode
        userErrors {
          field
          message
        }
      }
    }
  `,
  {
    input: {
      initialValue: '5.00',
      code: `HAR310${Date.now()}`,
      note: 'HAR-310 conformance gift card',
      expiresOn: '2027-04-26',
    },
  },
);

const createdId = readCreatedGiftCardId(create);
const lifecycle: CapturedRequest[] = [];

if (createdId !== null) {
  lifecycle.push(
    await capture(
      'detailAfterCreate',
      `#graphql
        query GiftCardDetail($id: ID!) {
          giftCard(id: $id) {
            ${giftCardSelection}
            transactions(first: 5) {
              nodes {
                id
                note
                processedAt
                amount {
                  amount
                  currencyCode
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
          }
        }
      `,
      { id: createdId },
    ),
  );
  lifecycle.push(
    await capture(
      'update',
      `#graphql
        mutation GiftCardUpdate($id: ID!, $input: GiftCardUpdateInput!) {
          giftCardUpdate(id: $id, input: $input) {
            giftCard {
              ${giftCardSelection}
            }
            userErrors {
              field
              message
            }
          }
        }
      `,
      {
        id: createdId,
        input: {
          note: 'HAR-310 conformance gift card updated',
          templateSuffix: 'birthday',
          expiresOn: '2028-04-26',
        },
      },
    ),
  );
  lifecycle.push(
    await capture(
      'credit',
      `#graphql
        mutation GiftCardCredit($id: ID!, $input: GiftCardCreditInput!) {
          giftCardCredit(id: $id, creditInput: $input) {
            giftCardCreditTransaction {
              id
              note
              processedAt
              amount {
                amount
                currencyCode
              }
              giftCard {
                id
                balance {
                  amount
                  currencyCode
                }
              }
            }
            userErrors {
              field
              message
            }
          }
        }
      `,
      {
        id: createdId,
        input: {
          creditAmount: {
            amount: '2.00',
            currencyCode: 'CAD',
          },
          note: 'HAR-310 credit',
        },
      },
    ),
  );
  lifecycle.push(
    await capture(
      'debit',
      `#graphql
        mutation GiftCardDebit($id: ID!, $input: GiftCardDebitInput!) {
          giftCardDebit(id: $id, debitInput: $input) {
            giftCardDebitTransaction {
              id
              note
              processedAt
              amount {
                amount
                currencyCode
              }
              giftCard {
                id
                balance {
                  amount
                  currencyCode
                }
              }
            }
            userErrors {
              field
              message
            }
          }
        }
      `,
      {
        id: createdId,
        input: {
          debitAmount: {
            amount: '1.00',
            currencyCode: 'CAD',
          },
          note: 'HAR-310 debit',
        },
      },
    ),
  );
  lifecycle.push(
    await capture(
      'deactivate',
      `#graphql
        mutation GiftCardDeactivate($id: ID!) {
          giftCardDeactivate(id: $id) {
            giftCard {
              id
              enabled
              deactivatedAt
              balance {
                amount
                currencyCode
              }
            }
            userErrors {
              field
              message
            }
          }
        }
      `,
      { id: createdId },
    ),
  );
  lifecycle.push(
    await capture(
      'detailAfterDeactivate',
      `#graphql
        query GiftCardDetailAfterDeactivate($id: ID!) {
          giftCard(id: $id) {
            ${giftCardSelection}
          }
        }
      `,
      { id: createdId },
    ),
  );
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
        'HAR-310 captures gift-card schema/access, read/config/count behavior, and lifecycle payloads when the active conformance credential permits them.',
        'The filtered empty read uses id:999999999999 because Shopify accepts id as a gift-card search field and returns an empty connection/count for a no-match numeric id.',
        'Credit/debit transaction mutations and transaction-node reads are captured as payloads or access blockers depending on whether the active credential includes gift-card transaction scopes.',
        'Notification roots are intentionally not executed by this capture script because they are customer-visible side effects.',
      ],
      notificationRoots: {
        giftCardSendNotificationToCustomer: {
          executed: false,
          reason: 'customer-visible notification side effect',
        },
        giftCardSendNotificationToRecipient: {
          executed: false,
          reason: 'customer-visible notification side effect',
        },
      },
      schemaAndAccess,
      emptyRead,
      filteredEmptyRead,
      configurationRead,
      create,
      lifecycle,
      lifecycleBlocked:
        createdId === null
          ? {
              reason:
                'giftCardCreate did not return a giftCard.id; see create response for access or validation payload.',
            }
          : null,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ outputPath, createdId, lifecycleSteps: lifecycle.map((step) => step.label) }, null, 2));
