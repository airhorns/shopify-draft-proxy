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
const outputPath = path.join(outputDir, 'gift-card-update-noop.json');

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
      mutation GiftCardUpdateNoopGiftCardDeactivate($id: ID!) {
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
  mutation GiftCardUpdateNoopCreate($input: GiftCardCreateInput!) {
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

const updateNoopMutation = `#graphql
  mutation GiftCardUpdateNoop(
    $id: ID!
    $note: String!
    $expiresOn: Date!
    $templateSuffix: String!
  ) {
    noteNoop: giftCardUpdate(id: $id, input: { note: $note }) {
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
    expiresNoop: giftCardUpdate(id: $id, input: { expiresOn: $expiresOn }) {
      giftCard {
        id
        expiresOn
        updatedAt
      }
      userErrors {
        field
        message
      }
    }
    templateNoop: giftCardUpdate(id: $id, input: { templateSuffix: $templateSuffix }) {
      giftCard {
        id
        templateSuffix
        updatedAt
      }
      userErrors {
        field
        message
      }
    }
    emptyInput: giftCardUpdate(id: $id, input: {}) {
      giftCard {
        id
        note
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
const setup: CapturedRequest[] = [];
const cleanup: CapturedRequest[] = [];
const createdGiftCardIds: string[] = [];

try {
  const createInput = {
    initialValue: '5.00',
    code: `H766N${runSuffix}`,
    note: 'HAR-766 no-op current note',
    expiresOn: '2030-01-01',
    templateSuffix: 'birthday',
  };

  const create = await capture('noopCardCreate', createMutation, { input: createInput });
  assertNoTopLevelErrors(create);
  setup.push(create);

  const createdId = readCreatedGiftCardId(create);
  if (createdId === null) {
    throw new Error('Unable to create disposable gift card for giftCardUpdate no-op capture.');
  }
  createdGiftCardIds.push(createdId);

  const noopVariables = {
    id: createdId,
    note: createInput.note,
    expiresOn: createInput.expiresOn,
    templateSuffix: createInput.templateSuffix,
  };
  const noop = await capture('updateNoop', updateNoopMutation, noopVariables);
  assertNoTopLevelErrors(noop);

  cleanup.push(await deactivateGiftCard('cleanupDeactivate:noopCard', createdId));

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        notes: [
          'HAR-766 captures live giftCardUpdate no-op behavior for note, expiresOn, and templateSuffix inputs whose values already equal the current gift card.',
          'Shopify accepts present editable keys even when the values do not change, touches updatedAt, and still rejects an input object with no editable keys.',
          'The public Admin API exposes giftCardUpdate.userErrors as generic UserError in 2025-01, so parity compares the recorded public field/message payload directly.',
          'Setup creates one disposable gift card with known editable fields; cleanup deactivates the setup gift card.',
        ],
        proxyVariables: {
          create: { input: createInput },
          noop: {
            note: createInput.note,
            expiresOn: createInput.expiresOn,
            templateSuffix: createInput.templateSuffix,
          },
        },
        setup,
        operations: {
          noop,
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
