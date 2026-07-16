/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';
import { captureGiftCardCreateConfiguration } from './support/shopify/runtime-hydration-capture.js';

type CapturedRequest = {
  label: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'gift-cards');
const outputPath = path.join(outputDir, 'gift-card-credit-debit-payload-shape.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const giftCardCreateConfigurationHydrate = await captureGiftCardCreateConfiguration((query, variables) =>
  runGraphqlRequest(query, variables),
);

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

function readCreatedGiftCardCurrency(capture: CapturedRequest): string | null {
  return readStringPath(capture.response.payload, ['data', 'giftCardCreate', 'giftCard', 'balance', 'currencyCode']);
}

function assertNoTopLevelErrors(capture: CapturedRequest): void {
  if (capture.response.payload.errors) {
    throw new Error(`${capture.label} returned top-level errors: ${JSON.stringify(capture.response.payload.errors)}`);
  }
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
  const captured = await capture(
    label,
    `#graphql
      mutation GiftCardPayloadShapeDeactivate($id: ID!) {
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
  assertNoTopLevelErrors(captured);
  return captured;
}

const createMutation = `#graphql
  mutation GiftCardCreditDebitPayloadShapeCreate($input: GiftCardCreateInput!) {
    giftCardCreate(input: $input) {
      giftCard {
        id
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
`;

const creditMutation = `#graphql
  mutation GiftCardCreditPayloadShape($id: ID!, $input: GiftCardCreditInput!) {
    giftCardCredit(id: $id, creditInput: $input) {
      giftCardCreditTransaction {
        id
        __typename
        note
        amount {
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
`;

const debitMutation = `#graphql
  mutation GiftCardDebitPayloadShape($id: ID!, $input: GiftCardDebitInput!) {
    giftCardDebit(id: $id, debitInput: $input) {
      giftCardDebitTransaction {
        id
        __typename
        note
        amount {
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
`;

const invalidCreditMutation = `#graphql
  mutation GiftCardCreditPayloadGiftCardRejected($id: ID!, $input: GiftCardCreditInput!) {
    giftCardCredit(id: $id, creditInput: $input) {
      giftCardCreditTransaction {
        id
      }
      giftCard {
        id
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
`;

const invalidDebitMutation = `#graphql
  mutation GiftCardDebitPayloadGiftCardRejected($id: ID!, $input: GiftCardDebitInput!) {
    giftCardDebit(id: $id, debitInput: $input) {
      giftCardDebitTransaction {
        id
      }
      giftCard {
        id
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
`;

const runSuffix = Date.now().toString(36).toUpperCase();
const createdGiftCardIds: string[] = [];
const cleanup: CapturedRequest[] = [];

try {
  const createInput = {
    initialValue: '20.00',
    code: `PAYLOAD${runSuffix}`,
    note: 'Disposable gift-card payload shape capture card.',
  };
  const create = await capture('giftCardPayloadShapeCreate', createMutation, { input: createInput });
  assertNoTopLevelErrors(create);

  const createdId = readCreatedGiftCardId(create);
  if (createdId === null) {
    throw new Error('Unable to create disposable gift card for credit/debit payload shape capture.');
  }
  createdGiftCardIds.push(createdId);

  const currencyCode = readCreatedGiftCardCurrency(create) ?? 'CAD';
  const creditInput = {
    creditAmount: {
      amount: '2.00',
      currencyCode,
    },
  };
  const debitInput = {
    debitAmount: {
      amount: '1.00',
      currencyCode,
    },
  };

  const validCredit = await capture('validCredit', creditMutation, {
    id: createdId,
    input: creditInput,
  });
  assertNoTopLevelErrors(validCredit);

  const validDebit = await capture('validDebit', debitMutation, {
    id: createdId,
    input: debitInput,
  });
  assertNoTopLevelErrors(validDebit);

  const invalidCreditGiftCard = await capture('invalidCreditGiftCard', invalidCreditMutation, {
    id: createdId,
    input: creditInput,
  });
  const invalidDebitGiftCard = await capture('invalidDebitGiftCard', invalidDebitMutation, {
    id: createdId,
    input: debitInput,
  });

  cleanup.push(await deactivateGiftCard('cleanupDeactivate:payloadShapeCard', createdId));

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        notes: [
          'Captures live giftCardCredit and giftCardDebit payload shapes for schema-valid typed transaction selections.',
          'The valid credit and debit inputs intentionally omit note so the captured transaction note proves Shopify returns null for the optional field.',
          'Captures Shopify top-level undefinedField validation errors when giftCard is selected on GiftCardCreditPayload or GiftCardDebitPayload.',
          'Setup creates one disposable gift card; cleanup deactivates the setup gift card.',
        ],
        proxyVariables: {
          create: { input: createInput },
          credit: { id: createdId, input: creditInput },
          debit: { id: createdId, input: debitInput },
        },
        setup: {
          create,
        },
        operations: {
          validCredit,
          validDebit,
          invalidCreditGiftCard,
          invalidDebitGiftCard,
        },
        cleanup,
        upstreamCalls: [giftCardCreateConfigurationHydrate],
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
        createdId,
        operationLabels: ['validCredit', 'validDebit', 'invalidCreditGiftCard', 'invalidDebitGiftCard'],
        cleanupLabels: cleanup.map((entry) => entry.label),
      },
      null,
      2,
    ),
  );
} catch (error) {
  for (const id of createdGiftCardIds) {
    try {
      cleanup.push(await deactivateGiftCard(`cleanupAfterError:giftCard:${id}`, id));
    } catch (cleanupError) {
      console.error(
        `Failed to deactivate gift card ${id} after capture error: ${String((cleanupError as Error).message)}`,
      );
    }
  }
  throw error;
}
