/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { readFile, mkdir, writeFile } from 'node:fs/promises';
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
const outputPath = path.join(outputDir, 'gift-card-mutation-user-error-codes.json');
const setupDocumentPath = path.join(
  'config',
  'parity-requests',
  'gift-cards',
  'gift-card-mutation-user-error-codes-setup.graphql',
);
const validationDocumentPath = path.join(
  'config',
  'parity-requests',
  'gift-cards',
  'gift-card-mutation-user-error-codes.graphql',
);
const setupDocument = await readFile(setupDocumentPath, 'utf8');
const validationDocument = await readFile(validationDocumentPath, 'utf8');

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
  return capture(
    label,
    `#graphql
      mutation GiftCardMutationUserErrorCodesCleanup($id: ID!) {
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

async function hydrateMissingGiftCard(id: string): Promise<RecordedCall> {
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

const runToken = Date.now().toString(36).slice(-10);
const setupVariables = {
  input: {
    initialValue: '5.00',
    code: `MUE${runToken}`,
    note: `Disposable gift-card mutation user-error capture ${runToken}.`,
  },
};
const missingUpdateId = `gid://shopify/GiftCard/${Date.now()}999`;
const cleanup: CapturedRequest[] = [];

let setupSmallBalance: CapturedRequest | null = null;
let mutationUserErrorCodes: CapturedRequest | null = null;
let upstreamCalls: RecordedCall[] = [];
let cardCurrency = 'CAD';

try {
  setupSmallBalance = await capture('setupSmallBalance', setupDocument, setupVariables);
  assertNoTopLevelErrors(setupSmallBalance);

  const setupCardId = readCreatedGiftCardId(setupSmallBalance);
  if (setupCardId === null) {
    throw new Error('setup giftCardCreate did not return a giftCard.id; inspect setupSmallBalance response.');
  }

  cardCurrency = readCreatedGiftCardCurrency(setupSmallBalance) ?? cardCurrency;
  upstreamCalls = [await hydrateMissingGiftCard(missingUpdateId)];

  mutationUserErrorCodes = await capture('mutationUserErrorCodes', validationDocument, {
    cardId: setupCardId,
    missingUpdateId,
    cardCurrency,
  });
  assertNoTopLevelErrors(mutationUserErrorCodes);
} finally {
  if (setupSmallBalance !== null) {
    const setupCardId = readCreatedGiftCardId(setupSmallBalance);
    if (setupCardId !== null) {
      cleanup.push(await deactivateGiftCard('cleanupDeactivate', setupCardId));
    }
  }
}

if (setupSmallBalance === null || mutationUserErrorCodes === null) {
  throw new Error('capture did not complete; no fixture was written.');
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'gift-card-mutation-user-error-codes',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      notes: [
        'Live Shopify capture for public gift-card mutation userError branches that can be represented through Admin GraphQL.',
        'GiftCardUpdatePayload.userErrors is generic UserError in the public schema, so this capture compares field/message for update and leaves local typed-code coverage to Rust integration tests.',
        'Setup creates one disposable small-balance gift card and cleanup deactivates it. The upstreamCalls cassette contains the exact GiftCardHydrate miss used when the proxy validates an unknown update id from a cold LiveHybrid state.',
      ],
      proxyVariables: {
        setupSmallBalance: setupVariables,
        mutationUserErrorCodes: {
          missingUpdateId,
          cardCurrency,
        },
      },
      operations: {
        setupSmallBalance,
        mutationUserErrorCodes,
      },
      cleanup,
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
);

console.log(`Wrote ${outputPath}`);
