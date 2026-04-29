/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout } from 'node:timers/promises';

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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'gift-cards');
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

function readCreatedCustomerId(createCapture: CapturedRequest): string | null {
  const data = createCapture.response.payload.data;
  if (!isObject(data)) {
    return null;
  }

  const payload = data['customerCreate'];
  if (!isObject(payload)) {
    return null;
  }

  const customer = payload['customer'];
  if (!isObject(customer)) {
    return null;
  }

  const id = customer['id'];
  return typeof id === 'string' ? id : null;
}

function giftCardTail(id: string): string {
  return id.split('/').at(-1)?.split('?')[0] ?? id;
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
  customer {
    id
  }
  recipientAttributes {
    message
    preferredName
    sendNotificationAt
    recipient {
      id
    }
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

const customerCreate = await capture(
  'customerCreate',
  `#graphql
    mutation GiftCardSearchCustomerCreate($input: CustomerInput!) {
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
  `,
  {
    input: {
      email: `har-464-gift-card-${Date.now()}@example.com`,
      firstName: 'HAR-464',
      lastName: 'Gift Card Search',
      note: 'Disposable customer for HAR-464 gift-card search conformance.',
    },
  },
);
const createdCustomerId = readCreatedCustomerId(customerCreate);

const giftCardCode = `HAR310${Date.now()}`;
const createInput: {
  initialValue: string;
  code: string;
  note: string;
  expiresOn: string;
  customerId?: string;
  recipientAttributes?: {
    id: string;
    message: string;
    preferredName: string;
  };
} = {
  initialValue: '5.00',
  code: giftCardCode,
  note: 'HAR-310 conformance gift card',
  expiresOn: '2027-04-26',
};
if (createdCustomerId !== null) {
  createInput.customerId = createdCustomerId;
  createInput.recipientAttributes = {
    id: createdCustomerId,
    message: 'HAR-464 recipient message',
    preferredName: 'HAR-464 recipient',
  };
}

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
    input: createInput,
  },
);

const createdId = readCreatedGiftCardId(create);
const lifecycle: CapturedRequest[] = [];

if (createdId !== null) {
  const updateInput = {
    note: 'HAR-310 conformance gift card updated',
    templateSuffix: 'birthday',
    expiresOn: '2028-04-26',
  };
  const creditInput = {
    creditAmount: {
      amount: '2.00',
      currencyCode: 'CAD',
    },
    note: 'HAR-310 credit',
  };
  const debitInput = {
    debitAmount: {
      amount: '3.00',
      currencyCode: 'CAD',
    },
    note: 'HAR-310 debit',
  };
  const giftCardIdQuery = `id:${giftCardTail(createdId)}`;
  const codeFragmentQuery = `${giftCardIdQuery} AND ${createInput.code.slice(-4)}`;
  const captureDate = new Date().toISOString().slice(0, 10);
  const customerIdTail = createdCustomerId ? giftCardTail(createdCustomerId) : null;

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
        input: updateInput,
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
        input: creditInput,
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
        input: debitInput,
      },
    ),
  );
  await setTimeout(10_000);
  lifecycle.push(
    await capture(
      'searchFilterRead',
      `#graphql
        query GiftCardSearchFilterRead(
          $partialQuery: String!
          $fullQuery: String!
          $emptyQuery: String!
          $nonEmptyQuery: String!
          $codeQuery: String!
        ) {
          partialBalanceGiftCards: giftCards(first: 2, query: $partialQuery, sortKey: ID) {
            nodes {
              id
              lastCharacters
              enabled
              balance {
                amount
                currencyCode
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          partialBalanceGiftCardsCount: giftCardsCount(query: $partialQuery) {
            count
            precision
          }
          fullBalanceGiftCards: giftCards(first: 2, query: $fullQuery, sortKey: ID) {
            nodes {
              id
              lastCharacters
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          emptyBalanceGiftCards: giftCards(first: 2, query: $emptyQuery, sortKey: ID) {
            nodes {
              id
              lastCharacters
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          nonEmptyBalanceGiftCardsCount: giftCardsCount(query: $nonEmptyQuery) {
            count
            precision
          }
          codeSearchGiftCards: giftCards(first: 2, query: $codeQuery, sortKey: ID) {
            nodes {
              id
              lastCharacters
              enabled
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          codeSearchGiftCardsCount: giftCardsCount(query: $codeQuery) {
            count
            precision
          }
        }
      `,
      {
        partialQuery: `${giftCardIdQuery} AND balance_status:partial`,
        fullQuery: `${giftCardIdQuery} AND balance_status:full`,
        emptyQuery: `${giftCardIdQuery} AND balance_status:empty`,
        nonEmptyQuery: `${giftCardIdQuery} AND balance_status:full_or_partial`,
        codeQuery: codeFragmentQuery,
      },
    ),
  );
  lifecycle.push(
    await capture(
      'advancedSearchFilterRead',
      `#graphql
        query GiftCardAdvancedSearchFilterRead(
          $createdAfterQuery: String!
          $createdFutureQuery: String!
          $updatedAfterQuery: String!
          $updatedFutureQuery: String!
          $expiresAfterQuery: String!
          $expiresBeforeQuery: String!
          $customerQuery: String!
          $recipientQuery: String!
          $sourceQuery: String!
          $sourceNoMatchQuery: String!
          $initialValueQuery: String!
        ) {
          createdAfterGiftCards: giftCards(first: 2, query: $createdAfterQuery, sortKey: ID) {
            nodes {
              id
              createdAt
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          createdAfterGiftCardsCount: giftCardsCount(query: $createdAfterQuery) {
            count
            precision
          }
          createdFutureGiftCards: giftCards(first: 2, query: $createdFutureQuery, sortKey: ID) {
            nodes {
              id
              createdAt
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          updatedAfterGiftCards: giftCards(first: 2, query: $updatedAfterQuery, sortKey: ID) {
            nodes {
              id
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          updatedFutureGiftCards: giftCards(first: 2, query: $updatedFutureQuery, sortKey: ID) {
            nodes {
              id
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          expiresAfterGiftCards: giftCards(first: 2, query: $expiresAfterQuery, sortKey: ID) {
            nodes {
              id
              expiresOn
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          expiresBeforeGiftCards: giftCards(first: 2, query: $expiresBeforeQuery, sortKey: ID) {
            nodes {
              id
              expiresOn
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          customerGiftCards: giftCards(first: 2, query: $customerQuery, sortKey: ID) {
            nodes {
              id
              customer {
                id
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          recipientGiftCards: giftCards(first: 2, query: $recipientQuery, sortKey: ID) {
            nodes {
              id
              recipientAttributes {
                recipient {
                  id
                }
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          sourceGiftCards: giftCards(first: 2, query: $sourceQuery, sortKey: ID) {
            nodes {
              id
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          sourceNoMatchGiftCards: giftCards(first: 2, query: $sourceNoMatchQuery, sortKey: ID) {
            nodes {
              id
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          initialValueGiftCards: giftCards(first: 2, query: $initialValueQuery, sortKey: ID) {
            nodes {
              id
              initialValue {
                amount
                currencyCode
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
        }
      `,
      {
        createdAfterQuery: `${giftCardIdQuery} AND created_at:>=${captureDate}`,
        createdFutureQuery: `${giftCardIdQuery} AND created_at:>=2099-01-01`,
        updatedAfterQuery: `${giftCardIdQuery} AND updated_at:>=${captureDate}`,
        updatedFutureQuery: `${giftCardIdQuery} AND updated_at:>=2099-01-01`,
        expiresAfterQuery: `${giftCardIdQuery} AND expires_on:>=2028-01-01`,
        expiresBeforeQuery: `${giftCardIdQuery} AND expires_on:<2028-01-01`,
        customerQuery: `${giftCardIdQuery} AND customer_id:${customerIdTail ?? '999999999999'}`,
        recipientQuery: `${giftCardIdQuery} AND recipient_id:${customerIdTail ?? '999999999999'}`,
        sourceQuery: `${giftCardIdQuery} AND source:api_client`,
        sourceNoMatchQuery: `${giftCardIdQuery} AND source:manual`,
        initialValueQuery: `${giftCardIdQuery} AND initial_value:>=5`,
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
            transactions(first: 5) {
              nodes {
                note
                amount {
                  amount
                  currencyCode
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
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
      'readAfterLifecycle',
      `#graphql
        query GiftCardReadAfterLifecycle($id: ID!, $query: String!) {
          giftCard(id: $id) {
            note
            templateSuffix
            expiresOn
            enabled
            balance {
              amount
              currencyCode
            }
            transactions(first: 5) {
              nodes {
                note
                amount {
                  amount
                  currencyCode
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
              }
            }
          }
          giftCards(first: 2, query: $query, sortKey: ID) {
            nodes {
              id
              lastCharacters
              enabled
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
          }
          giftCardsCount(query: $query) {
            count
            precision
          }
        }
      `,
      { id: createdId, query: `id:${giftCardTail(createdId)}` },
    ),
  );
  if (createdCustomerId !== null) {
    lifecycle.push(
      await capture(
        'customerCleanup',
        `#graphql
          mutation GiftCardSearchCustomerCleanup($input: CustomerDeleteInput!) {
            customerDelete(input: $input) {
              deletedCustomerId
              userErrors {
                field
                message
              }
            }
          }
        `,
        { input: { id: createdCustomerId } },
      ),
    );
  }

  const operations = Object.fromEntries(lifecycle.map((step) => [step.label, step]));

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
          'Credit/debit transaction mutations and transaction-node reads are captured with read_gift_card_transactions and write_gift_card_transactions.',
          'HAR-464 extends the fixture with a disposable customer-backed gift card and populated-data search filters for date/range, customer_id, recipient_id, source, and initial_value behavior.',
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
        proxyVariables: {
          lifecycle: {
            id: createdId,
            updateInput,
            creditInput,
            debitInput,
          },
          readEvidence: {
            unknownId,
            query: 'id:999999999999',
          },
          readAfterLifecycle: {
            id: createdId,
            query: `id:${giftCardTail(createdId)}`,
          },
          searchFilters: {
            partialQuery: `${giftCardIdQuery} AND balance_status:partial`,
            fullQuery: `${giftCardIdQuery} AND balance_status:full`,
            emptyQuery: `${giftCardIdQuery} AND balance_status:empty`,
            nonEmptyQuery: `${giftCardIdQuery} AND balance_status:full_or_partial`,
            codeQuery: codeFragmentQuery,
          },
          advancedSearchFilters: {
            createdAfterQuery: `${giftCardIdQuery} AND created_at:>=${captureDate}`,
            createdFutureQuery: `${giftCardIdQuery} AND created_at:>=2099-01-01`,
            updatedAfterQuery: `${giftCardIdQuery} AND updated_at:>=${captureDate}`,
            updatedFutureQuery: `${giftCardIdQuery} AND updated_at:>=2099-01-01`,
            expiresAfterQuery: `${giftCardIdQuery} AND expires_on:>=2028-01-01`,
            expiresBeforeQuery: `${giftCardIdQuery} AND expires_on:<2028-01-01`,
            customerQuery: `${giftCardIdQuery} AND customer_id:${customerIdTail ?? '999999999999'}`,
            recipientQuery: `${giftCardIdQuery} AND recipient_id:${customerIdTail ?? '999999999999'}`,
            sourceQuery: `${giftCardIdQuery} AND source:api_client`,
            sourceNoMatchQuery: `${giftCardIdQuery} AND source:manual`,
            initialValueQuery: `${giftCardIdQuery} AND initial_value:>=5`,
          },
        },
        operations: {
          schemaAndAccess,
          emptyRead,
          filteredEmptyRead,
          configurationRead,
          customerCreate,
          create,
          ...operations,
        },
        schemaAndAccess,
        emptyRead,
        filteredEmptyRead,
        configurationRead,
        customerCreate,
        create,
        lifecycle,
        lifecycleBlocked: null,
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(JSON.stringify({ outputPath, createdId, lifecycleSteps: lifecycle.map((step) => step.label) }, null, 2));
  process.exit(0);
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
        'HAR-464 attempts to create a disposable customer-backed gift card so populated-data search filters can be captured when giftCardCreate returns a usable id.',
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
      customerCreate,
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
