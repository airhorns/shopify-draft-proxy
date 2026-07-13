/* oxlint-disable no-console -- CLI capture scripts intentionally print machine-readable status. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

import { parseWriteScopeBlocker, renderWriteScopeBlockerNote } from './product-mutation-conformance-lib.mjs';

type Variables = Record<string, unknown>;

type RecordedGraphqlCall = {
  request: {
    query: string;
    variables: Variables;
  };
  response: ConformanceGraphqlPayload;
};

type DeleteScenario = {
  create: RecordedGraphqlCall;
  bulkCreate: RecordedGraphqlCall;
  delete: RecordedGraphqlCall;
  readAfterDelete: RecordedGraphqlCall;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-variant-delete-position-compaction.json');
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'product-variant-delete-position-compaction-scope-blocker.md');

const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createDocument = await readFile(
  'config/parity-requests/products/product-variant-position-compaction-create.graphql',
  'utf8',
);
const bulkCreateDocument = await readFile(
  'config/parity-requests/products/product-variant-position-compaction-bulk-create.graphql',
  'utf8',
);
const bulkDeleteDocument = await readFile(
  'config/parity-requests/products/product-variant-position-compaction-bulk-delete.graphql',
  'utf8',
);
const readDocument = await readFile(
  'config/parity-requests/products/product-variant-position-compaction-read.graphql',
  'utf8',
);

const productDeleteDocument = `#graphql
  mutation ProductVariantPositionCompactionCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function readPath(value: unknown, segments: Array<string | number>): unknown {
  let current = value;
  for (const segment of segments) {
    if (typeof segment === 'number') {
      if (!Array.isArray(current)) {
        return undefined;
      }
      current = current[segment];
      continue;
    }
    if (typeof current !== 'object' || current === null || Array.isArray(current)) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}

function readString(value: unknown, segments: Array<string | number>, label: string): string {
  const candidate = readPath(value, segments);
  if (typeof candidate !== 'string' || candidate.length === 0) {
    throw new Error(`${label} missing string at ${segments.join('.')}`);
  }
  return candidate;
}

function readArray(value: unknown, segments: Array<string | number>, label: string): unknown[] {
  const candidate = readPath(value, segments);
  if (!Array.isArray(candidate)) {
    throw new Error(`${label} missing array at ${segments.join('.')}`);
  }
  return candidate;
}

function expectNoUserErrors(label: string, value: unknown, segments: Array<string | number>): void {
  const userErrors = readPath(value, segments);
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function buildCreateVariables(runId: string, label: string, optionValues: string[]): Variables {
  return {
    product: {
      title: `Hermes Variant Position ${label} ${runId}`,
      status: 'DRAFT',
      productOptions: [
        {
          name: 'Color',
          values: optionValues.map((name) => ({ name })),
        },
      ],
    },
  };
}

function buildBulkCreateVariables(productId: string, runId: string, colorNames: string[]): Variables {
  return {
    productId,
    variants: colorNames.map((name, index) => ({
      optionValues: [{ optionName: 'Color', name }],
      price: `${index + 11}.00`,
      inventoryItem: {
        sku: `HERMES-POS-${runId}-${name.toUpperCase()}`,
      },
    })),
  };
}

async function recordGraphql(query: string, variables: Variables): Promise<RecordedGraphqlCall> {
  const response = await runGraphql(query, variables);
  return {
    request: { query, variables },
    response,
  };
}

async function recordDeleteScenario({
  runId,
  label,
  optionValues,
  createdVariantNames,
  deletedVariantIndexes,
  createdProductIds,
}: {
  runId: string;
  label: string;
  optionValues: string[];
  createdVariantNames: string[];
  deletedVariantIndexes: number[];
  createdProductIds: string[];
}): Promise<DeleteScenario> {
  const create = await recordGraphql(createDocument, buildCreateVariables(runId, label, optionValues));
  expectNoUserErrors(`${label} productCreate`, create.response, ['data', 'productCreate', 'userErrors']);

  const productId = readString(create.response, ['data', 'productCreate', 'product', 'id'], `${label} productCreate`);
  createdProductIds.push(productId);

  const bulkCreate = await recordGraphql(
    bulkCreateDocument,
    buildBulkCreateVariables(productId, runId, createdVariantNames),
  );
  expectNoUserErrors(`${label} productVariantsBulkCreate`, bulkCreate.response, [
    'data',
    'productVariantsBulkCreate',
    'userErrors',
  ]);

  const createdVariants = readArray(
    bulkCreate.response,
    ['data', 'productVariantsBulkCreate', 'productVariants'],
    `${label} productVariantsBulkCreate`,
  );
  const variantsIds = deletedVariantIndexes.map((index) =>
    readString(createdVariants[index], ['id'], `${label} productVariantsBulkCreate variant ${index}`),
  );

  const deleteVariables = { productId, variantsIds };
  const deleteCall = await recordGraphql(bulkDeleteDocument, deleteVariables);
  expectNoUserErrors(`${label} productVariantsBulkDelete`, deleteCall.response, [
    'data',
    'productVariantsBulkDelete',
    'userErrors',
  ]);

  const readAfterDelete = await recordGraphql(readDocument, { id: productId });

  return {
    create,
    bulkCreate,
    delete: deleteCall,
    readAfterDelete,
  };
}

async function writeScopeBlocker(blocker: NonNullable<ReturnType<typeof parseWriteScopeBlocker>>): Promise<void> {
  await mkdir(pendingDir, { recursive: true });
  const note = renderWriteScopeBlockerNote({
    title: 'Product variant delete position-compaction conformance blocker',
    whatFailed:
      'Attempted to capture live conformance for post-delete product variant position compaction on a disposable product.',
    operations: ['productCreate', 'productVariantsBulkCreate', 'productVariantsBulkDelete', 'product'],
    blocker,
    whyBlocked:
      'This ticket requires a real Shopify fixture proving middle-delete survivor positions. Without write_products access, the repo cannot record the setup product, variants, delete, and readback sequence.',
    completedSteps: [
      'added a focused capture harness and parity request/spec files for middle-delete variant position compaction',
      'retained historical productVariantDelete comparison metadata backed by equivalent live productVariantsBulkDelete evidence because the captured schema does not expose the legacy root',
    ],
    recommendedNextStep:
      'Switch the repo conformance credential to a safe dev-store token with read_products/write_products, then rerun `corepack pnpm exec tsx ./scripts/capture-product-variant-delete-position-compaction-conformance.mts`.',
  });

  await writeFile(blockerPath, `${note}\n`, 'utf8');
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const createdProductIds: string[] = [];

try {
  const singleDelete = await recordDeleteScenario({
    runId,
    label: 'Single',
    optionValues: ['Red', 'Blue', 'Green'],
    createdVariantNames: ['Blue', 'Green'],
    deletedVariantIndexes: [0],
    createdProductIds,
  });
  const bulkDelete = await recordDeleteScenario({
    runId,
    label: 'Bulk',
    optionValues: ['Red', 'Blue', 'Green', 'Yellow'],
    createdVariantNames: ['Blue', 'Green', 'Yellow'],
    deletedVariantIndexes: [0, 1],
    createdProductIds,
  });

  const capture = {
    metadata: {
      storeDomain,
      apiVersion,
      recordedAt: new Date().toISOString(),
      liveDeleteRoot: 'productVariantsBulkDelete',
      notes:
        'The historical single-delete comparison records Shopify janitor output through a one-id productVariantsBulkDelete call because the current 2025-01 live schema does not expose productVariantDelete.',
    },
    singleDelete,
    bulkDelete,
    upstreamCalls: [],
  };

  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        createdProductIds,
      },
      null,
      2,
    ),
  );
} catch (error) {
  const blocker = parseWriteScopeBlocker((error as { result?: unknown } | null)?.result ?? null);
  if (blocker) {
    await writeScopeBlocker(blocker);
    console.log(
      JSON.stringify(
        {
          ok: false,
          blocked: true,
          blockerPath,
          blocker,
        },
        null,
        2,
      ),
    );
    process.exit(1);
  }

  throw error;
} finally {
  for (const productId of createdProductIds.reverse()) {
    try {
      await runGraphql(productDeleteDocument, { input: { id: productId } });
    } catch {
      // Best-effort cleanup only. Preserve the original capture failure.
    }
  }
}
