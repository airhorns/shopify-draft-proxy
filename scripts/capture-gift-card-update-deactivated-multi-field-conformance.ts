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
const outputPath = path.join(outputDir, 'gift-card-update-deactivated-multi-field.json');

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
      mutation GiftCardUpdateDeactivatedMultiFieldCustomerCreate($input: CustomerInput!) {
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
      mutation GiftCardUpdateDeactivatedMultiFieldGiftCardCreate($input: GiftCardCreateInput!) {
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
      mutation GiftCardUpdateDeactivatedMultiFieldGiftCardDeactivate($id: ID!) {
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
      mutation GiftCardUpdateDeactivatedMultiFieldCustomerDelete($input: CustomerDeleteInput!) {
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

const updateMutation = `#graphql
  mutation GiftCardUpdateDeactivatedMultiField($deactivatedId: ID!, $customerId: ID!, $recipientId: ID!) {
    expiresAndCustomer: giftCardUpdate(
      id: $deactivatedId
      input: { expiresOn: "2099-12-31", customerId: $customerId }
    ) {
      giftCard {
        id
      }
      userErrors {
        field
        message
      }
    }
    customerAndRecipient: giftCardUpdate(
      id: $deactivatedId
      input: { customerId: $customerId, recipientAttributes: { id: $recipientId } }
    ) {
      giftCard {
        id
      }
      userErrors {
        field
        message
      }
    }
    customerRecipientAndExpires: giftCardUpdate(
      id: $deactivatedId
      input: {
        customerId: $customerId
        recipientAttributes: { id: $recipientId }
        expiresOn: "2099-12-31"
      }
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

const stamp = Date.now();
const runSuffix = stamp.toString(36).slice(-8);
const setupIds: SetupIds = { customers: [], giftCards: [] };
const setup: CapturedRequest[] = [];
const cleanup: CapturedRequest[] = [];

try {
  const customer = await createCustomer(
    'customerCreate',
    {
      email: `gift-card-update-deactivated-${stamp}@example.com`,
      firstName: 'GiftCardUpdate',
      lastName: 'DeactivatedMultiField',
      note: 'Disposable giftCardUpdate deactivated multi-field validation customer.',
    },
    setupIds,
  );
  setup.push(customer);
  const customerId = readCreatedCustomerId(customer);
  if (customerId === null) {
    throw new Error('Unable to create disposable customer for giftCardUpdate deactivated multi-field capture.');
  }

  const giftCard = await createGiftCard(
    'giftCardCreate',
    {
      initialValue: '5.00',
      code: `GUPDMF${runSuffix}`,
      note: 'Disposable giftCardUpdate deactivated multi-field validation card.',
    },
    setupIds,
  );
  setup.push(giftCard);
  const giftCardId = readCreatedGiftCardId(giftCard);
  if (giftCardId === null) {
    throw new Error('Unable to create disposable gift card for giftCardUpdate deactivated multi-field capture.');
  }

  setup.push(await deactivateGiftCard('giftCardDeactivate', giftCardId));

  const proxyVariables = {
    deactivatedId: 'gid://shopify/GiftCard/deactivated-multi-field',
    customerId,
    recipientId: customerId,
  };
  const liveVariables = {
    ...proxyVariables,
    deactivatedId: giftCardId,
  };
  const multiFieldUpdate = await capture('multiFieldUpdate', updateMutation, liveVariables);

  for (const id of setupIds.customers) {
    cleanup.push(await deleteCustomer(`cleanupCustomer:${id}`, id));
  }

  const liveData = readPath(multiFieldUpdate.response.payload, ['data']);
  const expected = isObject(liveData)
    ? {
        data: Object.fromEntries(
          Object.entries(liveData).map(([key, value]) => [key, addUserErrorCode(value, 'INVALID')]),
        ),
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
          'Captures live giftCardUpdate deactivated-card validation when multiple public blocked fields are supplied in the same input.',
          'The public Admin API exposes giftCardUpdate.userErrors as generic UserError in 2025-01, so the live request records field/message and expected replay data adds the typed INVALID code.',
          'Setup creates one disposable customer and one disposable gift card, deactivates the gift card, records two-field and three-field blocked-input updates, then deletes the setup customer.',
        ],
        proxyVariables: {
          multiFieldUpdate: proxyVariables,
        },
        setup,
        operations: {
          multiFieldUpdate,
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
