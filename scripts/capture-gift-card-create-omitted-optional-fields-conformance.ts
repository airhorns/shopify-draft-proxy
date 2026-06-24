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
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'gift-cards');
const outputPath = path.join(outputDir, 'gift-card-create-omitted-optional-fields.json');

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

function assertNullPath(capture: CapturedRequest, pathParts: string[]): void {
  const actual = readPath(capture.response.payload, pathParts);
  if (actual !== null) {
    throw new Error(`${capture.label} expected ${pathParts.join('.')} to be null; got ${JSON.stringify(actual)}.`);
  }
}

function assertNoTopLevelErrors(capture: CapturedRequest): void {
  if (capture.response.payload.errors) {
    throw new Error(`${capture.label} returned top-level errors: ${JSON.stringify(capture.response.payload.errors)}`);
  }
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

async function deactivateGiftCard(label: string, id: string): Promise<CapturedRequest> {
  const captured = await capture(
    label,
    `#graphql
      mutation GiftCardCreateOmittedOptionalFieldsDeactivate($id: ID!) {
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

const createMutation = `#graphql
  mutation GiftCardCreateOmittedOptionalFields($input: GiftCardCreateInput!) {
    giftCardCreate(input: $input) {
      giftCard {
        id
        note
        expiresOn
        customer {
          id
        }
        templateSuffix
        recipientAttributes {
          message
          preferredName
          sendNotificationAt
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
  }
`;

const readAfterCreateQuery = `#graphql
  query GiftCardCreateOmittedOptionalFieldsRead($id: ID!) {
    giftCard(id: $id) {
      id
      note
      expiresOn
      customer {
        id
      }
      templateSuffix
      recipientAttributes {
        message
        preferredName
        sendNotificationAt
        recipient {
          id
        }
      }
    }
  }
`;

const setup: CapturedRequest[] = [];
const cleanup: CapturedRequest[] = [];
const createdGiftCardIds: string[] = [];

try {
  const createInput = {
    initialValue: '25.00',
  };

  const create = await capture('createOmittedOptionalFields', createMutation, { input: createInput });
  assertNoTopLevelErrors(create);
  for (const field of ['note', 'expiresOn', 'customer', 'templateSuffix', 'recipientAttributes']) {
    assertNullPath(create, ['data', 'giftCardCreate', 'giftCard', field]);
  }
  setup.push(create);

  const createdId = readCreatedGiftCardId(create);
  if (createdId === null) {
    throw new Error('Unable to create disposable gift card for omitted optional fields capture.');
  }
  createdGiftCardIds.push(createdId);

  const readAfterCreate = await capture('readAfterCreate', readAfterCreateQuery, { id: createdId });
  assertNoTopLevelErrors(readAfterCreate);
  for (const field of ['note', 'expiresOn', 'customer', 'templateSuffix', 'recipientAttributes']) {
    assertNullPath(readAfterCreate, ['data', 'giftCard', field]);
  }

  cleanup.push(await deactivateGiftCard('cleanupDeactivate:omittedOptionalFieldsCard', createdId));

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        notes: [
          'Captures live giftCardCreate behavior when the input contains only initialValue.',
          'Shopify returns null note, expiresOn, customer, templateSuffix, and recipientAttributes in the create payload and immediate giftCard(id:) readback.',
          'Setup creates one disposable gift card with only initialValue; cleanup deactivates the setup gift card.',
        ],
        proxyVariables: {
          create: { input: createInput },
        },
        setup,
        operations: {
          readAfterCreate,
        },
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
  for (const id of createdGiftCardIds) {
    try {
      cleanup.push(await deactivateGiftCard(`cleanupAfterError:giftCard:${id}`, id));
    } catch {
      // best-effort cleanup
    }
  }
  throw error;
}
