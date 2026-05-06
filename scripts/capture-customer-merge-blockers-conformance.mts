/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CapturedRequest = {
  label: string;
  query: string;
  variables: JsonRecord;
  status: number;
  response: JsonRecord;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const outputPath = path.join(outputDir, 'customer-merge-blockers.json');

const setupDocument = await readFile('config/parity-requests/customers/customer-merge-blockers-setup.graphql', 'utf8');
const mergeDocument = await readFile('config/parity-requests/customers/customer-merge-blocker.graphql', 'utf8');
const giftCardCreateDocument = await readFile(
  'config/parity-requests/customers/customer-merge-blocker-gift-card-create.graphql',
  'utf8',
);

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function isObject(value: unknown): value is JsonRecord {
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

function readRequiredString(value: unknown, pathParts: string[], context: string): string {
  const found = readPath(value, pathParts);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`${context} did not return ${pathParts.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return found;
}

function assertNoTopLevelErrors(capture: CapturedRequest): void {
  if (capture.status < 200 || capture.status >= 300 || capture.response.errors !== undefined) {
    throw new Error(`${capture.label} failed: ${JSON.stringify(capture, null, 2)}`);
  }
}

function assertUserErrorsEmpty(capture: CapturedRequest, pathParts: string[]): void {
  const userErrors = readPath(capture.response, pathParts);
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${capture.label} returned userErrors: ${JSON.stringify(capture.response, null, 2)}`);
  }
}

function compactUserError(error: unknown): JsonRecord {
  if (!isObject(error)) {
    return {};
  }
  return {
    field: error['field'],
    message: error['message'],
    code: error['code'],
  };
}

function assertMergeErrors(capture: CapturedRequest, expected: JsonRecord[]): void {
  assertNoTopLevelErrors(capture);
  const payload = readPath(capture.response, ['data', 'customerMerge']);
  if (!isObject(payload)) {
    throw new Error(`${capture.label} did not return customerMerge: ${JSON.stringify(capture.response, null, 2)}`);
  }
  if (payload['resultingCustomerId'] !== null || payload['job'] !== null) {
    throw new Error(`${capture.label} did not short-circuit merge: ${JSON.stringify(payload, null, 2)}`);
  }
  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors)) {
    throw new Error(`${capture.label} did not return userErrors: ${JSON.stringify(payload, null, 2)}`);
  }
  const actual = userErrors.map(compactUserError);
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(
      `${capture.label} userErrors mismatch:\nexpected ${JSON.stringify(expected, null, 2)}\nactual ${JSON.stringify(
        actual,
        null,
        2,
      )}`,
    );
  }
}

async function capture(label: string, query: string, variables: JsonRecord = {}): Promise<CapturedRequest> {
  const result: ConformanceGraphqlResult = await runGraphqlRequest(query, variables);
  return {
    label,
    query,
    variables,
    status: result.status,
    response: result.payload as JsonRecord,
  };
}

function numberedTags(prefix: string, count: number): string[] {
  return Array.from({ length: count }, (_, index) => `${prefix}${index}`);
}

function customerInput(email: string, extra: JsonRecord): JsonRecord {
  return {
    email,
    firstName: 'Merge',
    lastName: 'Blocker',
    ...extra,
  };
}

async function deleteCustomer(id: string): Promise<CapturedRequest> {
  return capture(
    'customerDelete cleanup',
    `#graphql
      mutation CustomerMergeBlockerCustomerDelete($input: CustomerDeleteInput!) {
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

async function deactivateGiftCard(id: string): Promise<CapturedRequest> {
  return capture(
    'giftCardDeactivate cleanup',
    `#graphql
      mutation CustomerMergeBlockerGiftCardDeactivate($id: ID!) {
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

const stamp = Date.now();
const setupVariables: JsonRecord = {
  tagsOne: customerInput(`customer-merge-blocker-tags-one-${stamp}@example.com`, {
    tags: numberedTags('one', 126),
  }),
  tagsTwo: customerInput(`customer-merge-blocker-tags-two-${stamp}@example.com`, {
    tags: numberedTags('two', 125),
  }),
  noteOne: customerInput(`customer-merge-blocker-note-one-${stamp}@example.com`, {
    note: 'A'.repeat(3000),
  }),
  noteTwo: customerInput(`customer-merge-blocker-note-two-${stamp}@example.com`, {
    note: 'B'.repeat(2501),
  }),
  giftOne: customerInput(`customer-merge-blocker-gift-one-${stamp}@example.com`, {}),
  giftTwo: customerInput(`customer-merge-blocker-gift-two-${stamp}@example.com`, {}),
};

await mkdir(outputDir, { recursive: true });

const cleanup: CapturedRequest[] = [];
const setup = await capture('customerMerge blocker customer setup', setupDocument, setupVariables);
assertNoTopLevelErrors(setup);
for (const alias of ['tagsOne', 'tagsTwo', 'noteOne', 'noteTwo', 'giftOne', 'giftTwo']) {
  assertUserErrorsEmpty(setup, ['data', alias, 'userErrors']);
}

const tagsOneId = readRequiredString(setup.response, ['data', 'tagsOne', 'customer', 'id'], 'tagsOne setup');
const tagsTwoId = readRequiredString(setup.response, ['data', 'tagsTwo', 'customer', 'id'], 'tagsTwo setup');
const noteOneId = readRequiredString(setup.response, ['data', 'noteOne', 'customer', 'id'], 'noteOne setup');
const noteTwoId = readRequiredString(setup.response, ['data', 'noteTwo', 'customer', 'id'], 'noteTwo setup');
const giftOneId = readRequiredString(setup.response, ['data', 'giftOne', 'customer', 'id'], 'giftOne setup');
const giftTwoId = readRequiredString(setup.response, ['data', 'giftTwo', 'customer', 'id'], 'giftTwo setup');
const giftOneDisplayName = readRequiredString(
  setup.response,
  ['data', 'giftOne', 'customer', 'displayName'],
  'giftOne setup',
);

let giftCardId: string | null = null;
let tagsOverflow: CapturedRequest | null = null;
let noteOverflow: CapturedRequest | null = null;
let giftCardSetup: CapturedRequest | null = null;
let giftCardMerge: CapturedRequest | null = null;

try {
  tagsOverflow = await capture('customerMerge tags overflow', mergeDocument, {
    one: tagsOneId,
    two: tagsTwoId,
  });
  assertMergeErrors(tagsOverflow, [
    {
      field: ['customerOneId'],
      message: 'Customers must have 250 tags or less.',
      code: 'INVALID_CUSTOMER',
    },
    {
      field: ['customerTwoId'],
      message: 'Customers must have 250 tags or less.',
      code: 'INVALID_CUSTOMER',
    },
  ]);

  noteOverflow = await capture('customerMerge note overflow', mergeDocument, {
    one: noteOneId,
    two: noteTwoId,
  });
  assertMergeErrors(noteOverflow, [
    {
      field: ['customerOneId'],
      message: 'Customer notes must be 5,000 characters or less.',
      code: 'INVALID_CUSTOMER',
    },
    {
      field: ['customerTwoId'],
      message: 'Customer notes must be 5,000 characters or less.',
      code: 'INVALID_CUSTOMER',
    },
  ]);

  giftCardSetup = await capture('customerMerge gift-card setup', giftCardCreateDocument, {
    input: {
      initialValue: '5.00',
      code: `cmblock${String(stamp).slice(-12)}`,
      customerId: giftOneId,
    },
  });
  assertNoTopLevelErrors(giftCardSetup);
  assertUserErrorsEmpty(giftCardSetup, ['data', 'giftCardCreate', 'userErrors']);
  giftCardId = readRequiredString(
    giftCardSetup.response,
    ['data', 'giftCardCreate', 'giftCard', 'id'],
    'giftCardCreate setup',
  );

  giftCardMerge = await capture('customerMerge gift-card blocker', mergeDocument, {
    one: giftOneId,
    two: giftTwoId,
  });
  assertMergeErrors(giftCardMerge, [
    {
      field: ['customerOneId'],
      message: `${giftOneDisplayName} has gift cards and can’t be merged.`,
      code: 'INVALID_CUSTOMER',
    },
  ]);
} finally {
  if (giftCardId !== null) {
    cleanup.push(await deactivateGiftCard(giftCardId));
  }
  for (const id of [tagsOneId, tagsTwoId, noteOneId, noteTwoId, giftOneId, giftTwoId]) {
    cleanup.push(await deleteCustomer(id));
  }
}

if (tagsOverflow === null || noteOverflow === null || giftCardSetup === null || giftCardMerge === null) {
  throw new Error('customerMerge blocker capture did not complete all required cases.');
}

const output = {
  storeDomain,
  apiVersion,
  setup: {
    query: setupDocument,
    variables: setupVariables,
    status: setup.status,
    response: setup.response,
  },
  tagsOverflow: {
    merge: {
      query: mergeDocument,
      variables: tagsOverflow.variables,
      status: tagsOverflow.status,
      response: tagsOverflow.response,
    },
  },
  noteOverflow: {
    merge: {
      query: mergeDocument,
      variables: noteOverflow.variables,
      status: noteOverflow.status,
      response: noteOverflow.response,
    },
  },
  giftCard: {
    setup: {
      query: giftCardCreateDocument,
      variables: giftCardSetup.variables,
      status: giftCardSetup.status,
      response: giftCardSetup.response,
    },
    merge: {
      query: mergeDocument,
      variables: giftCardMerge.variables,
      status: giftCardMerge.status,
      response: giftCardMerge.response,
    },
  },
  cleanup: cleanup.map((entry) => ({
    label: entry.label,
    query: entry.query,
    variables: entry.variables,
    status: entry.status,
    response: entry.response,
  })),
  upstreamCalls: [],
};

await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`);
console.log(`Wrote ${outputPath}`);
