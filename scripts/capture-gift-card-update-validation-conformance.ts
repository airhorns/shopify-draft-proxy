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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'gift-cards');
const outputPath = path.join(outputDir, 'gift-card-update-validation.json');

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

function addUserErrorCode(payload: unknown, code: string): unknown {
  if (!isObject(payload)) {
    return payload;
  }

  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors)) {
    return payload;
  }

  return {
    ...payload,
    userErrors: userErrors.map((error) => {
      if (!isObject(error)) {
        return error;
      }
      return { ...error, code };
    }),
  };
}

function readTopLevelErrorMessageForPath(payload: unknown, path: string): string | null {
  const errors = readPath(payload, ['errors']);
  if (!Array.isArray(errors)) {
    return null;
  }

  for (const error of errors) {
    if (!isObject(error)) {
      continue;
    }
    const errorPath = error['path'];
    const message = error['message'];
    if (Array.isArray(errorPath) && errorPath.length === 1 && errorPath[0] === path && typeof message === 'string') {
      return message;
    }
  }

  return null;
}

function recipientLengthExpected(field: 'preferredName' | 'message', message: string): Record<string, unknown> {
  return {
    giftCard: null,
    userErrors: [
      {
        field: ['input', 'recipientAttributes', field],
        code: 'TOO_LONG',
        message,
      },
    ],
  };
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
      mutation GiftCardUpdateValidationCustomerCreate($input: CustomerInput!) {
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
      mutation GiftCardUpdateValidationGiftCardCreate($input: GiftCardCreateInput!) {
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
      mutation GiftCardUpdateValidationGiftCardDeactivate($id: ID!) {
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
      mutation GiftCardUpdateValidationCustomerDelete($input: CustomerDeleteInput!) {
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

const updateValidationMutation = `#graphql
  mutation GiftCardUpdateValidation(
    $activeId: ID!
    $deactivatedId: ID!
    $missingCustomerId: ID!
    $recipientId: ID!
    $tooLongPreferredName: String!
    $tooLongMessage: String!
    $successNote: String!
  ) {
    deactivatedExpiresOn: giftCardUpdate(id: $deactivatedId, input: { expiresOn: "2099-12-31" }) {
      giftCard {
        id
        enabled
        expiresOn
      }
      userErrors {
        field
        message
      }
    }
    emptyInput: giftCardUpdate(id: $activeId, input: {}) {
      giftCard {
        id
        note
      }
      userErrors {
        field
        message
      }
    }
    missingCustomer: giftCardUpdate(id: $activeId, input: { customerId: $missingCustomerId }) {
      giftCard {
        id
        customer {
          id
        }
      }
      userErrors {
        field
        message
      }
    }
    longRecipientName: giftCardUpdate(
      id: $activeId
      input: { recipientAttributes: { id: $recipientId, preferredName: $tooLongPreferredName } }
    ) {
      giftCard {
        id
        recipientAttributes {
          preferredName
          recipient {
            id
          }
        }
      }
      userErrors {
        field
        message
      }
    }
    longRecipientMessage: giftCardUpdate(
      id: $activeId
      input: { recipientAttributes: { id: $recipientId, message: $tooLongMessage } }
    ) {
      giftCard {
        id
        recipientAttributes {
          message
          recipient {
            id
          }
        }
      }
      userErrors {
        field
        message
      }
    }
    success: giftCardUpdate(id: $activeId, input: { note: $successNote }) {
      giftCard {
        id
        note
        updatedAt
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
  const customerA = await createCustomer(
    'customerACreate',
    {
      email: `har694-a-${stamp}@example.com`,
      firstName: 'HAR694',
      lastName: 'Customer A',
      note: 'Disposable HAR-694 gift-card update validation customer A.',
    },
    setupIds,
  );
  setup.push(customerA);
  const customerAId = readCreatedCustomerId(customerA);

  const customerB = await createCustomer(
    'customerBCreate',
    {
      email: `har694-b-${stamp}@example.com`,
      firstName: 'HAR694',
      lastName: 'Customer B',
      note: 'Disposable HAR-694 gift-card update validation customer B.',
    },
    setupIds,
  );
  setup.push(customerB);
  const customerBId = readCreatedCustomerId(customerB);

  if (customerAId === null || customerBId === null) {
    throw new Error('Unable to create disposable customers for giftCardUpdate validation capture.');
  }

  const cardA = await createGiftCard(
    'cardAActiveCreate',
    {
      initialValue: '5.00',
      code: `H694A${runSuffix}`,
      note: 'HAR-694 active update validation card.',
      customerId: customerAId,
    },
    setupIds,
  );
  setup.push(cardA);
  const cardAId = readCreatedGiftCardId(cardA);

  const cardB = await createGiftCard(
    'cardBDeactivatedCreate',
    {
      initialValue: '5.00',
      code: `H694B${runSuffix}`,
      note: 'HAR-694 deactivated update validation card.',
    },
    setupIds,
  );
  setup.push(cardB);
  const cardBId = readCreatedGiftCardId(cardB);

  if (cardAId === null || cardBId === null) {
    throw new Error('Unable to create disposable gift cards for giftCardUpdate validation capture.');
  }

  setup.push(await deactivateGiftCard('cardBDeactivate', cardBId));

  const proxyVariables = {
    activeId: 'gid://shopify/GiftCard/har694-active',
    deactivatedId: 'gid://shopify/GiftCard/har694-deactivated',
    missingCustomerId: 'gid://shopify/Customer/999999999999',
    recipientId: customerBId,
    tooLongPreferredName: 'x'.repeat(256),
    tooLongMessage: 'x'.repeat(201),
    successNote: 'HAR-694 updated note',
  };
  const liveVariables = {
    ...proxyVariables,
    activeId: cardAId,
    deactivatedId: cardBId,
  };
  const updateValidation = await capture('updateValidation', updateValidationMutation, liveVariables);

  cleanup.push(await deactivateGiftCard('cleanupDeactivate:cardA', cardAId));
  for (const id of setupIds.customers) {
    cleanup.push(await deleteCustomer(`cleanupCustomer:${id}`, id));
  }

  const liveData = readPath(updateValidation.response.payload, ['data']);
  const preferredNameMessage =
    readTopLevelErrorMessageForPath(updateValidation.response.payload, 'longRecipientName') ??
    'preferredName is too long (maximum is 255)';
  const recipientMessage =
    readTopLevelErrorMessageForPath(updateValidation.response.payload, 'longRecipientMessage') ??
    'message is too long (maximum is 200)';
  const expected = isObject(liveData)
    ? {
        data: {
          deactivatedExpiresOn: addUserErrorCode(liveData['deactivatedExpiresOn'], 'INVALID'),
          emptyInput: addUserErrorCode(liveData['emptyInput'], 'INVALID'),
          missingCustomer: addUserErrorCode(liveData['missingCustomer'], 'CUSTOMER_NOT_FOUND'),
          longRecipientName: recipientLengthExpected('preferredName', preferredNameMessage),
          longRecipientMessage: recipientLengthExpected('message', recipientMessage),
          success: liveData['success'],
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
          'HAR-694 captures live giftCardUpdate validation branches for deactivated-card protected fields, missing update arguments, missing changed customerId, recipient text length, and success.',
          'The public Admin API exposes giftCardUpdate.userErrors as generic UserError in 2025-01, so the live request records field/message only; expected replay data adds the typed code values required by the internal GiftCardErrorCode contract.',
          'Setup creates disposable Customer A/B plus active/deactivated gift cards; cleanup deactivates setup gift cards and deletes setup customers.',
        ],
        proxyVariables: {
          updateValidation: proxyVariables,
        },
        setup,
        operations: {
          updateValidation,
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
