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
const outputPath = path.join(outputDir, 'gift-card-update-clear-nullable.json');

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
      mutation GiftCardUpdateClearNullableGiftCardDeactivate($id: ID!) {
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
  mutation GiftCardUpdateClearNullableCreate($input: GiftCardCreateInput!) {
    giftCardCreate(input: $input) {
      giftCard {
        id
        note
        expiresOn
        templateSuffix
      }
      giftCardCode
      userErrors {
        field
        message
      }
    }
  }
`;

const clearMutation = `#graphql
  mutation GiftCardUpdateClearNullable($id: ID!) {
    noteClear: giftCardUpdate(id: $id, input: { note: null }) {
      giftCard {
        id
        note
        expiresOn
        templateSuffix
        updatedAt
      }
      userErrors {
        field
        message
      }
    }
    expiresClear: giftCardUpdate(id: $id, input: { expiresOn: null }) {
      giftCard {
        id
        note
        expiresOn
        templateSuffix
        updatedAt
      }
      userErrors {
        field
        message
      }
    }
    templateClear: giftCardUpdate(id: $id, input: { templateSuffix: null }) {
      giftCard {
        id
        note
        expiresOn
        templateSuffix
        updatedAt
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const readAfterClearQuery = `#graphql
  query GiftCardUpdateClearNullableRead($id: ID!) {
    giftCard(id: $id) {
      id
      note
      expiresOn
      templateSuffix
      updatedAt
    }
  }
`;

const stamp = Date.now();
const runSuffix = stamp.toString(36).slice(-8);
const setup: CapturedRequest[] = [];
const cleanup: CapturedRequest[] = [];
const createdGiftCardIds: string[] = [];

try {
  const createInput = {
    initialValue: '5.00',
    code: `CLR${runSuffix}`,
    note: 'nullable clear current note',
    expiresOn: '2030-01-01',
    templateSuffix: 'birthday',
  };

  const create = await capture('clearNullableCardCreate', createMutation, { input: createInput });
  assertNoTopLevelErrors(create);
  setup.push(create);

  const createdId = readCreatedGiftCardId(create);
  if (createdId === null) {
    throw new Error('Unable to create disposable gift card for giftCardUpdate nullable clear capture.');
  }
  createdGiftCardIds.push(createdId);

  const clear = await capture('updateClearNullable', clearMutation, { id: createdId });
  assertNoTopLevelErrors(clear);

  const readAfterClear = await capture('readAfterClear', readAfterClearQuery, { id: createdId });
  assertNoTopLevelErrors(readAfterClear);

  cleanup.push(await deactivateGiftCard('cleanupDeactivate:clearNullableCard', createdId));

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        notes: [
          'Captures live giftCardUpdate behavior for explicit null note, expiresOn, and templateSuffix inputs against a gift card with populated values.',
          'Shopify accepts each null-valued editable key, clears the corresponding nullable field, returns empty userErrors, and read-after-write returns null for all cleared fields.',
          'Setup creates one disposable gift card with known editable fields; cleanup deactivates the setup gift card.',
        ],
        proxyVariables: {
          create: { input: createInput },
        },
        setup,
        operations: {
          clear,
          readAfterClear,
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
