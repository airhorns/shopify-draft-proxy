/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  accessToken: adminAccessToken,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const setupDocumentPath = 'config/parity-requests/discounts/discount-redeem-code-bulk-delete-setup.graphql';
const validationDocumentPath = 'config/parity-requests/discounts/discount-redeem-code-bulk-delete-validation.graphql';
const happyDocumentPath = 'config/parity-requests/discounts/discount-redeem-code-bulk-delete-happy.graphql';

const setupDocument = await readFile(setupDocumentPath, 'utf8');
const validationDocument = await readFile(validationDocumentPath, 'utf8');
const happyDocument = await readFile(happyDocumentPath, 'utf8');

const deleteDiscountDocument = `#graphql
  mutation DiscountRedeemCodeBulkDeleteValidationCleanup($id: ID!) {
    discountCodeDelete(id: $id) {
      deletedCodeDiscountId
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
  }
`;

function readRunId(): number {
  const raw = process.env['SHOPIFY_CONFORMANCE_RUN_ID'];
  if (!raw) {
    return Date.now();
  }

  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`SHOPIFY_CONFORMANCE_RUN_ID must be a positive integer, got ${JSON.stringify(raw)}`);
  }
  return parsed;
}

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function readPath(value: unknown, segments: string[]): unknown {
  let current: unknown = value;
  for (const segment of segments) {
    const record = readRecord(current);
    if (!record) {
      return null;
    }
    current = record[segment];
  }
  return current;
}

function assertHttpOk(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} returned HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function readCreatedDiscountId(response: unknown): string | null {
  return readString(readPath(response, ['data', 'discountCodeBasicCreate', 'codeDiscountNode', 'id']));
}

function readSeedRedeemCodeId(response: unknown): string | null {
  const nodes = readPath(response, [
    'data',
    'discountCodeBasicCreate',
    'codeDiscountNode',
    'codeDiscount',
    'codes',
    'nodes',
  ]);
  if (!Array.isArray(nodes)) {
    return null;
  }
  const first = readRecord(nodes[0]);
  return readString(first?.['id']);
}

const runId = readRunId();
const seedCode = `SDPDELBASE${runId}`;
const startsAt = '2026-05-07T00:00:00Z';
const setupVariables = {
  input: {
    title: `Discount redeem delete validation ${runId}`,
    code: seedCode,
    startsAt,
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    customerGets: {
      value: {
        percentage: 0.1,
      },
      items: {
        all: true,
      },
    },
  },
};

let discountId: string | null = null;
let cleanup: unknown = null;

try {
  const setup = await runGraphqlRaw(setupDocument, setupVariables);
  assertHttpOk(setup, 'discountCodeBasicCreate setup');
  discountId = readCreatedDiscountId(setup.payload);
  const seedRedeemCodeId = readSeedRedeemCodeId(setup.payload);
  if (!discountId || !seedRedeemCodeId) {
    throw new Error(`Setup did not create a discount with a redeem code: ${JSON.stringify(setup, null, 2)}`);
  }

  const variables = {
    discountId,
    unknownDiscountId: 'gid://shopify/DiscountCodeNode/0',
    ids: [seedRedeemCodeId],
    emptyIds: [],
    search: 'code:ANY',
    blankSearch: '   ',
    savedSearchId: 'gid://shopify/SavedSearch/0',
  };
  const validation = await runGraphqlRaw(validationDocument, variables);
  assertHttpOk(validation, 'discountCodeRedeemCodeBulkDelete validation');

  const happyVariables = {
    discountId,
    ids: [seedRedeemCodeId],
  };
  const happy = await runGraphqlRaw(happyDocument, happyVariables);
  assertHttpOk(happy, 'discountCodeRedeemCodeBulkDelete happy path');

  cleanup = await runGraphqlRaw(deleteDiscountDocument, { id: discountId });

  const output = {
    storeDomain,
    apiVersion,
    variables,
    requests: {
      setup: {
        query: setupDocument,
        variables: setupVariables,
      },
      validation: {
        query: validationDocument,
        variables,
      },
      happy: {
        query: happyDocument,
        variables: happyVariables,
      },
      cleanup: {
        query: deleteDiscountDocument,
        variables: { id: discountId },
      },
    },
    scopeProbe,
    setup,
    validation,
    happy,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join(outputDir, 'discount-redeem-code-bulk-delete-validation.json');
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        discountId,
        seedRedeemCodeId,
      },
      null,
      2,
    ),
  );
} catch (error) {
  if (discountId && !cleanup) {
    try {
      cleanup = await runGraphqlRaw(deleteDiscountDocument, { id: discountId });
      console.error(`Cleaned up ${discountId} after capture failure: ${JSON.stringify(cleanup)}`);
    } catch (cleanupError) {
      console.error(`Failed to clean up ${discountId}: ${String(cleanupError)}`);
    }
  }
  throw error;
}
