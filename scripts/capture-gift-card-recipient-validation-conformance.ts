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

type PayloadKind = 'create' | 'update';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'gift-cards');
const outputPath = path.join(outputDir, 'gift-card-recipient-validation.json');

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

function assertNoTopLevelErrors(capture: CapturedRequest): void {
  if (capture.response.payload.errors) {
    throw new Error(`${capture.label} returned top-level errors: ${JSON.stringify(capture.response.payload.errors)}`);
  }
}

function firstTopLevelErrorMessage(payload: unknown, alias: string): string | null {
  const errors = readPath(payload, ['errors']);
  if (!Array.isArray(errors)) {
    return null;
  }

  for (const error of errors) {
    if (!isObject(error)) {
      continue;
    }
    const pathValue = error['path'];
    const message = error['message'];
    if (Array.isArray(pathValue) && pathValue.includes(alias) && typeof message === 'string') {
      return message;
    }
  }

  return null;
}

function liveUserErrorMessage(liveData: unknown, alias: string): string | null {
  const errors = readPath(liveData, [alias, 'userErrors']);
  if (!Array.isArray(errors)) {
    return null;
  }
  const first = errors[0];
  if (!isObject(first)) {
    return null;
  }
  const message = first['message'];
  return typeof message === 'string' ? message : null;
}

function capturedMessage(capture: CapturedRequest, liveData: unknown, alias: string, fallback: string): string {
  return (
    liveUserErrorMessage(liveData, alias) ?? firstTopLevelErrorMessage(capture.response.payload, alias) ?? fallback
  );
}

function errorPayload(kind: PayloadKind, field: string, code: string, message: string): Record<string, unknown> {
  const base = {
    giftCard: null,
    userErrors: [
      {
        field: ['input', 'recipientAttributes', field],
        code,
        message,
      },
    ],
  };

  return kind === 'create' ? { ...base, giftCardCode: null } : base;
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
      mutation GiftCardRecipientValidationCustomerCreate($input: CustomerInput!) {
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
    { input },
  );
  assertNoTopLevelErrors(captured);
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
      mutation GiftCardRecipientValidationGiftCardCreate($input: GiftCardCreateInput!) {
        giftCardCreate(input: $input) {
          giftCard {
            id
            enabled
            customer {
              id
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
  assertNoTopLevelErrors(captured);
  const id = readCreatedGiftCardId(captured);
  if (id !== null) {
    setupIds.giftCards.push(id);
  }
  return captured;
}

async function deactivateGiftCard(label: string, id: string): Promise<CapturedRequest> {
  const captured = await capture(
    label,
    `#graphql
      mutation GiftCardRecipientValidationGiftCardDeactivate($id: ID!) {
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
  assertNoTopLevelErrors(captured);
  return captured;
}

async function deleteCustomer(label: string, id: string): Promise<CapturedRequest> {
  const captured = await capture(
    label,
    `#graphql
      mutation GiftCardRecipientValidationCustomerDelete($input: CustomerDeleteInput!) {
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
  assertNoTopLevelErrors(captured);
  return captured;
}

const createMissingRecipientIdMutation = `#graphql
  mutation GiftCardRecipientValidationCreateMissingId {
    createMissingRecipientId: giftCardCreate(
      input: { initialValue: "10", recipientAttributes: { message: "missing id" } }
    ) {
      giftCard {
        id
      }
      giftCardCode
      userErrors {
        field
        code
        message
      }
    }
  }
`;

const updateMissingRecipientIdMutation = `#graphql
  mutation GiftCardRecipientValidationUpdateMissingId($activeId: ID!) {
    updateMissingRecipientId: giftCardUpdate(
      id: $activeId
      input: { recipientAttributes: { message: "missing id" } }
    ) {
      giftCard {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const recipientValidationMutation = `#graphql
  mutation GiftCardRecipientValidation(
    $activeId: ID!
    $recipientId: ID!
    $tooLongPreferredName: String!
    $tooLongMessage: String!
    $htmlPreferredName: String!
    $htmlMessage: String!
    $futureSendAt: DateTime!
    $pastSendAt: DateTime!
  ) {
    createLongPreferredName: giftCardCreate(
      input: {
        initialValue: "10"
        recipientAttributes: { id: $recipientId, preferredName: $tooLongPreferredName }
      }
    ) {
      giftCard {
        id
        recipientAttributes {
          preferredName
        }
      }
      giftCardCode
      userErrors {
        field
        code
        message
      }
    }
    createLongMessage: giftCardCreate(
      input: {
        initialValue: "10"
        recipientAttributes: { id: $recipientId, message: $tooLongMessage }
      }
    ) {
      giftCard {
        id
        recipientAttributes {
          message
        }
      }
      giftCardCode
      userErrors {
        field
        code
        message
      }
    }
    createHtmlPreferredName: giftCardCreate(
      input: {
        initialValue: "10"
        recipientAttributes: { id: $recipientId, preferredName: $htmlPreferredName }
      }
    ) {
      giftCard {
        id
        recipientAttributes {
          preferredName
        }
      }
      giftCardCode
      userErrors {
        field
        code
        message
      }
    }
    createHtmlMessage: giftCardCreate(
      input: { initialValue: "10", recipientAttributes: { id: $recipientId, message: $htmlMessage } }
    ) {
      giftCard {
        id
        recipientAttributes {
          message
        }
      }
      giftCardCode
      userErrors {
        field
        code
        message
      }
    }
    createFutureSendAt: giftCardCreate(
      input: {
        initialValue: "10"
        recipientAttributes: { id: $recipientId, sendNotificationAt: $futureSendAt }
      }
    ) {
      giftCard {
        id
        recipientAttributes {
          sendNotificationAt
        }
      }
      giftCardCode
      userErrors {
        field
        code
        message
      }
    }
    updateLongPreferredName: giftCardUpdate(
      id: $activeId
      input: { recipientAttributes: { id: $recipientId, preferredName: $tooLongPreferredName } }
    ) {
      giftCard {
        id
        recipientAttributes {
          preferredName
        }
      }
      userErrors {
        field
        message
      }
    }
    updateLongMessage: giftCardUpdate(
      id: $activeId
      input: { recipientAttributes: { id: $recipientId, message: $tooLongMessage } }
    ) {
      giftCard {
        id
        recipientAttributes {
          message
        }
      }
      userErrors {
        field
        message
      }
    }
    updateHtmlPreferredName: giftCardUpdate(
      id: $activeId
      input: { recipientAttributes: { id: $recipientId, preferredName: $htmlPreferredName } }
    ) {
      giftCard {
        id
        recipientAttributes {
          preferredName
        }
      }
      userErrors {
        field
        message
      }
    }
    updateHtmlMessage: giftCardUpdate(
      id: $activeId
      input: { recipientAttributes: { id: $recipientId, message: $htmlMessage } }
    ) {
      giftCard {
        id
        recipientAttributes {
          message
        }
      }
      userErrors {
        field
        message
      }
    }
    updatePastSendAt: giftCardUpdate(
      id: $activeId
      input: { recipientAttributes: { id: $recipientId, sendNotificationAt: $pastSendAt } }
    ) {
      giftCard {
        id
        recipientAttributes {
          sendNotificationAt
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const stamp = Date.now();
const runSuffix = stamp.toString(36).slice(-8);
const setupIds: SetupIds = { customers: [], giftCards: [] };
const setup: CapturedRequest[] = [];
const cleanup: CapturedRequest[] = [];

try {
  const customer = await createCustomer(
    'recipientCustomerCreate',
    {
      email: `recipient-validation-${stamp}@example.com`,
      firstName: 'Recipient',
      lastName: 'Validation',
      note: 'Disposable gift-card recipient validation customer.',
    },
    setupIds,
  );
  setup.push(customer);
  const customerId = readCreatedCustomerId(customer);
  if (customerId === null) {
    throw new Error('Unable to create disposable customer for gift-card recipient validation capture.');
  }

  const card = await createGiftCard(
    'activeGiftCardCreate',
    {
      initialValue: '5.00',
      code: `GCRV${runSuffix}`,
      note: 'Disposable gift-card recipient validation card.',
      customerId,
    },
    setupIds,
  );
  setup.push(card);
  const cardId = readCreatedGiftCardId(card);
  if (cardId === null) {
    throw new Error('Unable to create disposable gift card for gift-card recipient validation capture.');
  }

  const proxyVariables = {
    recipientValidation: {
      activeId: 'gid://shopify/GiftCard/recipient-validation-active',
      recipientId: customerId,
      tooLongPreferredName: 'x'.repeat(256),
      tooLongMessage: 'x'.repeat(201),
      htmlPreferredName: '<b>Recipient</b>',
      htmlMessage: '<script>alert(1)</script>',
      futureSendAt: '2099-01-01T00:00:00Z',
      pastSendAt: '1990-01-01T00:00:00Z',
    },
  };
  const liveVariables = {
    ...proxyVariables.recipientValidation,
    activeId: cardId,
  };

  const createMissingRecipientId = await capture('createMissingRecipientId', createMissingRecipientIdMutation);
  const updateMissingRecipientId = await capture('updateMissingRecipientId', updateMissingRecipientIdMutation, {
    activeId: cardId,
  });
  const recipientValidation = await capture('recipientValidation', recipientValidationMutation, liveVariables);

  cleanup.push(await deactivateGiftCard('cleanupDeactivate:activeGiftCard', cardId));
  for (const id of setupIds.customers) {
    cleanup.push(await deleteCustomer(`cleanupCustomer:${id}`, id));
  }

  const liveData = readPath(recipientValidation.response.payload, ['data']);
  const expected = isObject(liveData)
    ? {
        data: {
          createLongPreferredName: errorPayload(
            'create',
            'preferredName',
            'TOO_LONG',
            capturedMessage(
              recipientValidation,
              liveData,
              'createLongPreferredName',
              'preferredName is too long (maximum is 255)',
            ),
          ),
          createLongMessage: errorPayload(
            'create',
            'message',
            'TOO_LONG',
            capturedMessage(recipientValidation, liveData, 'createLongMessage', 'message is too long (maximum is 200)'),
          ),
          createHtmlPreferredName: errorPayload(
            'create',
            'preferredName',
            'INVALID',
            capturedMessage(
              recipientValidation,
              liveData,
              'createHtmlPreferredName',
              'preferredName contains HTML tags',
            ),
          ),
          createHtmlMessage: errorPayload(
            'create',
            'message',
            'INVALID',
            capturedMessage(recipientValidation, liveData, 'createHtmlMessage', 'message contains HTML tags'),
          ),
          createFutureSendAt: errorPayload(
            'create',
            'sendNotificationAt',
            'INVALID',
            capturedMessage(
              recipientValidation,
              liveData,
              'createFutureSendAt',
              'sendNotificationAt must be within 90 days from now',
            ),
          ),
          updateLongPreferredName: errorPayload(
            'update',
            'preferredName',
            'TOO_LONG',
            capturedMessage(
              recipientValidation,
              liveData,
              'updateLongPreferredName',
              'preferredName is too long (maximum is 255)',
            ),
          ),
          updateLongMessage: errorPayload(
            'update',
            'message',
            'TOO_LONG',
            capturedMessage(recipientValidation, liveData, 'updateLongMessage', 'message is too long (maximum is 200)'),
          ),
          updateHtmlPreferredName: errorPayload(
            'update',
            'preferredName',
            'INVALID',
            capturedMessage(
              recipientValidation,
              liveData,
              'updateHtmlPreferredName',
              'preferredName contains HTML tags',
            ),
          ),
          updateHtmlMessage: errorPayload(
            'update',
            'message',
            'INVALID',
            capturedMessage(recipientValidation, liveData, 'updateHtmlMessage', 'message contains HTML tags'),
          ),
          updatePastSendAt: errorPayload(
            'update',
            'sendNotificationAt',
            'INVALID',
            capturedMessage(
              recipientValidation,
              liveData,
              'updatePastSendAt',
              'sendNotificationAt must be within 90 days from now',
            ),
          ),
        },
      }
    : { data: {} };

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        notes: [
          'Captures giftCardCreate and giftCardUpdate recipientAttributes validation for missing recipient id, text length caps, HTML-tag rejection, and sendNotificationAt range bounds.',
          'Setup creates one disposable customer and one active gift card; cleanup deactivates the setup gift card and deletes the setup customer.',
          'The public giftCardUpdate userErrors type in Admin API 2025-01 exposes field/message only; replay expectations add local typed code values.',
        ],
        proxyVariables,
        setup,
        operations: {
          createMissingRecipientId,
          updateMissingRecipientId,
          recipientValidation,
        },
        expected,
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(`wrote ${outputPath}`);
} catch (error) {
  for (const id of setupIds.giftCards) {
    try {
      cleanup.push(await deactivateGiftCard(`cleanupAfterError:giftCard:${id}`, id));
    } catch {
      // best-effort cleanup
    }
  }
  for (const id of setupIds.customers) {
    try {
      cleanup.push(await deleteCustomer(`cleanupAfterError:customer:${id}`, id));
    } catch {
      // best-effort cleanup
    }
  }
  throw error;
}
