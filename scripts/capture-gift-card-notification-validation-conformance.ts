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

type SetupIds = {
  customers: string[];
  giftCards: string[];
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

type OperationKey =
  | 'customerDeactivated'
  | 'recipientDeactivated'
  | 'customerNoCustomer'
  | 'recipientNoContact'
  | 'customerExpired'
  | 'recipientExpired';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'gift-cards');
const outputPath = path.join(outputDir, 'gift-card-notification-validation.json');

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

function readCreatedCustomerId(capture: CapturedRequest): string | null {
  return readStringPath(capture.response.payload, ['data', 'customerCreate', 'customer', 'id']);
}

function readCreatedGiftCardId(capture: CapturedRequest): string | null {
  return readStringPath(capture.response.payload, ['data', 'giftCardCreate', 'giftCard', 'id']);
}

async function capture(
  label: string,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<CapturedRequest> {
  const response = await runGraphqlRequest(query, variables);
  return { label, query, variables, response };
}

async function createCustomer(
  label: string,
  input: Record<string, unknown>,
  setupIds: SetupIds,
): Promise<CapturedRequest> {
  const captured = await capture(
    label,
    `#graphql
      mutation GiftCardNotificationCustomerCreate($input: CustomerInput!) {
        customerCreate(input: $input) {
          customer {
            id
            email
            phone
            defaultEmailAddress {
              emailAddress
            }
            defaultPhoneNumber {
              phoneNumber
            }
          }
          userErrors {
            field
            message
          }
        }
      }
    `,
    { input },
  );
  const id = readCreatedCustomerId(captured);
  if (id !== null) {
    setupIds.customers.push(id);
  }
  return captured;
}

async function createGiftCard(
  label: string,
  input: Record<string, unknown>,
  setupIds: SetupIds,
): Promise<CapturedRequest> {
  const captured = await capture(
    label,
    `#graphql
      mutation GiftCardNotificationGiftCardCreate($input: GiftCardCreateInput!) {
        giftCardCreate(input: $input) {
          giftCard {
            id
            enabled
            deactivatedAt
            expiresOn
            customer {
              id
              email
              defaultEmailAddress {
                emailAddress
              }
              defaultPhoneNumber {
                phoneNumber
              }
            }
            recipientAttributes {
              recipient {
                id
                email
                defaultEmailAddress {
                  emailAddress
                }
                defaultPhoneNumber {
                  phoneNumber
                }
              }
            }
          }
          giftCardCode
          userErrors {
            field
            message
          }
        }
      }
    `,
    { input },
  );
  const id = readCreatedGiftCardId(captured);
  if (id !== null) {
    setupIds.giftCards.push(id);
  }
  return captured;
}

async function deactivateGiftCard(label: string, id: string): Promise<CapturedRequest> {
  return capture(
    label,
    `#graphql
      mutation GiftCardNotificationGiftCardDeactivate($id: ID!) {
        giftCardDeactivate(id: $id) {
          giftCard {
            id
            enabled
            deactivatedAt
          }
          userErrors {
            field
            message
          }
        }
      }
    `,
    { id },
  );
}

async function deleteCustomer(label: string, id: string): Promise<CapturedRequest> {
  return capture(
    label,
    `#graphql
      mutation GiftCardNotificationCustomerDelete($input: CustomerDeleteInput!) {
        customerDelete(input: $input) {
          deletedCustomerId
          userErrors {
            field
            message
          }
        }
      }
    `,
    { input: { id } },
  );
}

async function sendToCustomer(label: string, id: string): Promise<CapturedRequest> {
  return capture(
    label,
    `#graphql
      mutation GiftCardNotificationToCustomerValidation($id: ID!) {
        giftCardSendNotificationToCustomer(id: $id) {
          giftCard {
            id
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

async function sendToRecipient(label: string, id: string): Promise<CapturedRequest> {
  return capture(
    label,
    `#graphql
      mutation GiftCardNotificationToRecipientValidation($id: ID!) {
        giftCardSendNotificationToRecipient(id: $id) {
          giftCard {
            id
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
          pageInfo {
            hasNextPage
            hasPreviousPage
            startCursor
            endCursor
          }
        }
      }
      giftCardConfiguration {
        issueLimit { amount currencyCode }
        purchaseLimit { amount currencyCode }
      }
    }
  `;
  const variables = { id };
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

const setupIds: SetupIds = { customers: [], giftCards: [] };
const setup: CapturedRequest[] = [];
const operations: Partial<Record<OperationKey, CapturedRequest>> = {};
const cleanup: CapturedRequest[] = [];
const upstreamCalls: RecordedCall[] = [];
const stamp = Date.now();

try {
  const contactCustomer = await createCustomer(
    'contactCustomerCreate',
    {
      email: `har688-contact-${stamp}@example.com`,
      firstName: 'HAR688',
      lastName: 'Notification Contact',
      note: 'Disposable HAR-688 gift-card notification validation customer.',
    },
    setupIds,
  );
  setup.push(contactCustomer);
  const contactCustomerId = readCreatedCustomerId(contactCustomer);

  const noContactCustomer = await createCustomer(
    'noContactCustomerCreate',
    {
      firstName: 'HAR688',
      lastName: 'Notification No Contact',
      note: 'Disposable HAR-688 no-contact recipient validation customer.',
    },
    setupIds,
  );
  setup.push(noContactCustomer);
  const noContactCustomerId = readCreatedCustomerId(noContactCustomer);

  if (contactCustomerId === null || noContactCustomerId === null) {
    throw new Error('Unable to create disposable customers for notification validation capture.');
  }

  const deactivatedCard = await createGiftCard(
    'cardADeactivatedCreate',
    {
      initialValue: '5.00',
      code: `HAR688A${String(stamp).slice(-8)}`,
      note: 'HAR-688 deactivated notification validation.',
      customerId: contactCustomerId,
    },
    setupIds,
  );
  setup.push(deactivatedCard);
  const deactivatedCardId = readCreatedGiftCardId(deactivatedCard);
  if (deactivatedCardId === null) {
    throw new Error('Unable to create deactivated-branch gift card.');
  }
  setup.push(await deactivateGiftCard('cardADeactivate', deactivatedCardId));
  upstreamCalls.push(await hydrateGiftCard(deactivatedCardId));

  const noCustomerCard = await createGiftCard(
    'cardBNoCustomerCreate',
    {
      initialValue: '5.00',
      code: `HAR688B${String(stamp).slice(-8)}`,
      note: 'HAR-688 no-customer notification validation.',
      recipientAttributes: {
        id: contactCustomerId,
        preferredName: 'HAR-688 recipient',
        message: 'Validation-only notification branch.',
      },
    },
    setupIds,
  );
  setup.push(noCustomerCard);
  const noCustomerCardId = readCreatedGiftCardId(noCustomerCard);
  if (noCustomerCardId === null) {
    throw new Error('Unable to create no-customer-branch gift card.');
  }
  upstreamCalls.push(await hydrateGiftCard(noCustomerCardId));

  const noContactRecipientCard = await createGiftCard(
    'cardCNoContactRecipientCreate',
    {
      initialValue: '5.00',
      code: `HAR688C${String(stamp).slice(-8)}`,
      note: 'HAR-688 no-contact recipient notification validation.',
      recipientAttributes: {
        id: noContactCustomerId,
        preferredName: 'HAR-688 no-contact recipient',
        message: 'Validation-only notification branch.',
      },
    },
    setupIds,
  );
  setup.push(noContactRecipientCard);
  const noContactRecipientCardId = readCreatedGiftCardId(noContactRecipientCard);
  if (noContactRecipientCardId === null) {
    throw new Error('Unable to create no-contact-recipient-branch gift card.');
  }
  upstreamCalls.push(await hydrateGiftCard(noContactRecipientCardId));

  const expiredCard = await createGiftCard(
    'cardEExpiredCreate',
    {
      initialValue: '5.00',
      code: `HAR688E${String(stamp).slice(-8)}`,
      note: 'HAR-688 expired notification validation.',
      expiresOn: '2000-01-01',
      customerId: contactCustomerId,
      recipientAttributes: {
        id: contactCustomerId,
        preferredName: 'HAR-688 expired recipient',
        message: 'Validation-only notification branch.',
      },
    },
    setupIds,
  );
  setup.push(expiredCard);
  const expiredCardId = readCreatedGiftCardId(expiredCard);
  if (expiredCardId === null) {
    throw new Error('Unable to create expired-branch gift card.');
  }
  upstreamCalls.push(await hydrateGiftCard(expiredCardId));

  operations.customerDeactivated = await sendToCustomer('customerDeactivated', deactivatedCardId);
  operations.recipientDeactivated = await sendToRecipient('recipientDeactivated', deactivatedCardId);
  operations.customerNoCustomer = await sendToCustomer('customerNoCustomer', noCustomerCardId);
  operations.recipientNoContact = await sendToRecipient('recipientNoContact', noContactRecipientCardId);
  operations.customerExpired = await sendToCustomer('customerExpired', expiredCardId);
  operations.recipientExpired = await sendToRecipient('recipientExpired', expiredCardId);
} finally {
  for (const id of setupIds.giftCards) {
    cleanup.push(await deactivateGiftCard(`cleanupDeactivate:${id}`, id));
  }
  for (const id of setupIds.customers) {
    cleanup.push(await deleteCustomer(`cleanupCustomer:${id}`, id));
  }
}

const proxyVariables = {
  customerDeactivated: { id: operations.customerDeactivated?.variables['id'] },
  recipientDeactivated: { id: operations.recipientDeactivated?.variables['id'] },
  customerNoCustomer: { id: operations.customerNoCustomer?.variables['id'] },
  recipientNoContact: { id: operations.recipientNoContact?.variables['id'] },
  customerExpired: { id: operations.customerExpired?.variables['id'] },
  recipientExpired: { id: operations.recipientExpired?.variables['id'] },
};

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      scenarioId: 'gift-card-notification-validation',
      notes: [
        'HAR-688 captures validation-failing gift-card notification roots only, avoiding successful customer-visible notification dispatch.',
        'Live Admin GraphQL 2025-01 serializes base-scoped notification userErrors with field: null; runtime tests keep the HAR-688 requested local field ["base"] contract for those branches.',
        'Public Admin GraphQL 2025-01 does not expose a GiftCard notify field or GiftCardCreate/Update notify input, so notify-disabled validation remains covered by local runtime tests rather than live setup.',
      ],
      proxyVariables,
      setup,
      operations,
      cleanup,
      notifyDisabledCapture: {
        captured: false,
        reason:
          'Public Admin GraphQL 2025-01 exposes no GiftCard notify field or GiftCardCreate/Update notify input to construct a notify=false gift card through this conformance harness.',
      },
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ outputPath, operations: Object.keys(operations), cleanup: cleanup.length }, null, 2));
