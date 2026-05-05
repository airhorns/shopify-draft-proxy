/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-update-edge-cases.json');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const userErrorsSelection = `#graphql
  userErrors {
    field
    message
    code
    extraInfo
  }
`;

const productCreateDocument = `#graphql
  mutation DiscountUpdateEdgeProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteDocument = `#graphql
  mutation DiscountUpdateEdgeProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const createBasicDocument = await readText(
  'config/parity-requests/discounts/discount-update-edge-cases-basic-create.graphql',
);
const bulkAddDocument = await readText('config/parity-requests/discounts/discount-update-edge-cases-bulk-add.graphql');
const basicUpdateDocument = await readText(
  'config/parity-requests/discounts/discount-update-edge-cases-basic-update.graphql',
);
const createBxgyDocument = await readText(
  'config/parity-requests/discounts/discount-update-edge-cases-bxgy-create.graphql',
);
const unknownUpdateDocument = await readText(
  'config/parity-requests/discounts/discount-update-edge-cases-unknown-update.graphql',
);

const deleteDiscountDocument = `#graphql
  mutation DiscountUpdateEdgeDelete($id: ID!) {
    discountCodeDelete(id: $id) {
      deletedCodeDiscountId
      ${userErrorsSelection}
    }
  }
`;

const codeLookupDocument = `#graphql
  query DiscountUpdateEdgeCodeLookup($code: String!) {
    codeDiscountNodeByCode(code: $code) {
      id
    }
  }
`;

async function readText(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

function basicInput(title: string, code: string, percentage: number): Record<string, unknown> {
  return {
    title,
    code,
    startsAt: '2026-04-25T00:00:00Z',
    context: {
      all: 'ALL',
    },
    customerGets: {
      value: { percentage },
      items: { all: true },
    },
  };
}

function bxgyInput(title: string, code: string, buyProductId: string, getProductId: string): Record<string, unknown> {
  return {
    title,
    code,
    startsAt: '2026-04-25T00:00:00Z',
    context: {
      all: 'ALL',
    },
    customerBuys: {
      value: { quantity: '1' },
      items: {
        products: {
          productsToAdd: [buyProductId],
        },
      },
    },
    customerGets: {
      value: {
        discountOnQuantity: {
          quantity: '1',
          effect: { percentage: 0.5 },
        },
      },
      items: {
        products: {
          productsToAdd: [getProductId],
        },
      },
    },
  };
}

function assertNoUserErrors(label: string, userErrors: unknown): void {
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }

  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function readProductId(response: unknown): string {
  const create = (response as { data?: { productCreate?: { product?: { id?: unknown }; userErrors?: unknown } } }).data
    ?.productCreate;
  assertNoUserErrors('productCreate', create?.userErrors);
  const id = create?.product?.id;
  if (typeof id !== 'string') {
    throw new Error(`productCreate did not return an id: ${JSON.stringify(response)}`);
  }
  return id;
}

function readCreatedDiscountId(response: unknown, root: 'discountCodeBasicCreate' | 'discountCodeBxgyCreate'): string {
  const payload = (response as { payload?: { data?: Record<string, { codeDiscountNode?: { id?: unknown } }> } }).payload
    ?.data?.[root];
  const id = payload?.codeDiscountNode?.id;
  if (typeof id !== 'string') {
    throw new Error(`${root} did not return an id: ${JSON.stringify(response)}`);
  }
  return id;
}

function hasCodeLookup(response: unknown): boolean {
  const node = (response as { payload?: { data?: { codeDiscountNodeByCode?: { id?: unknown } | null } } }).payload?.data
    ?.codeDiscountNodeByCode;
  return typeof node?.id === 'string';
}

async function waitForCodeLookup(code: string): Promise<unknown> {
  let lastResponse: unknown = null;
  for (let attempt = 0; attempt < 20; attempt += 1) {
    lastResponse = await runGraphqlRaw(codeLookupDocument, { code });
    if (hasCodeLookup(lastResponse)) {
      return lastResponse;
    }
    await sleep(500);
  }
  return lastResponse;
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const runId = Date.now();
const cleanup: Array<() => Promise<unknown>> = [];
let cleanupResults: unknown[] = [];

try {
  const buyProductPayload = await runGraphql(productCreateDocument, {
    product: { title: `HAR-605 buy product ${runId}` },
  });
  const buyProductId = readProductId(buyProductPayload);
  cleanup.push(() => runGraphqlRaw(productDeleteDocument, { input: { id: buyProductId } }));

  const getProductPayload = await runGraphql(productCreateDocument, {
    product: { title: `HAR-605 get product ${runId}` },
  });
  const getProductId = readProductId(getProductPayload);
  cleanup.push(() => runGraphqlRaw(productDeleteDocument, { input: { id: getProductId } }));

  const createBasicVariables = {
    input: basicInput(`HAR-605 bulk rule ${runId}`, `HAR605BULK${runId}`, 0.1),
  };
  const createBasic = await runGraphqlRaw(createBasicDocument, createBasicVariables);
  const basicDiscountId = readCreatedDiscountId(createBasic, 'discountCodeBasicCreate');
  cleanup.push(() => runGraphqlRaw(deleteDiscountDocument, { id: basicDiscountId }));

  const bulkAddVariables = {
    discountId: basicDiscountId,
    codes: [1, 2, 3, 4, 5].map((index) => ({ code: `HAR605BULK${runId}_${index}` })),
  };
  const bulkAdd = await runGraphqlRaw(bulkAddDocument, bulkAddVariables);
  const readAfterBulkAdd = await waitForCodeLookup(`HAR605BULK${runId}_5`);

  const bulkCodeChangeVariables = {
    id: basicDiscountId,
    input: basicInput(`HAR-605 bulk renamed ${runId}`, `HAR605BULKNEW${runId}`, 0.2),
  };
  const bulkCodeChange = await runGraphqlRaw(basicUpdateDocument, bulkCodeChangeVariables);

  const createBxgyVariables = {
    input: bxgyInput(`HAR-605 BXGY ${runId}`, `HAR605BXGY${runId}`, buyProductId, getProductId),
  };
  const createBxgy = await runGraphqlRaw(createBxgyDocument, createBxgyVariables);
  const bxgyDiscountId = readCreatedDiscountId(createBxgy, 'discountCodeBxgyCreate');
  cleanup.push(() => runGraphqlRaw(deleteDiscountDocument, { id: bxgyDiscountId }));

  const bxgyToBasicVariables = {
    id: bxgyDiscountId,
    input: basicInput(`HAR-605 coerced basic ${runId}`, `HAR605BXGY${runId}`, 0.25),
  };
  const bxgyToBasic = await runGraphqlRaw(basicUpdateDocument, bxgyToBasicVariables);

  const unknownUpdateVariables = {
    id: 'gid://shopify/DiscountCodeNode/0',
    input: basicInput(`HAR-605 unknown ${runId}`, `HAR605UNKNOWN${runId}`, 0.1),
  };
  const unknownUpdate = await runGraphqlRaw(unknownUpdateDocument, unknownUpdateVariables);

  cleanupResults = await runCleanup(cleanup);

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    accessScopes: scopeProbe,
    variables: {
      createBasic: createBasicVariables,
      bulkAdd: bulkAddVariables,
      bulkCodeChange: bulkCodeChangeVariables,
      createBxgy: createBxgyVariables,
      bxgyToBasic: bxgyToBasicVariables,
      unknownUpdate: unknownUpdateVariables,
    },
    requests: {
      createBasic: { query: createBasicDocument, variables: createBasicVariables },
      bulkAdd: { query: bulkAddDocument, variables: bulkAddVariables },
      basicUpdate: { query: basicUpdateDocument },
      createBxgy: { query: createBxgyDocument, variables: createBxgyVariables },
      unknownUpdate: { query: unknownUpdateDocument, variables: unknownUpdateVariables },
      cleanup: { query: deleteDiscountDocument },
    },
    createBasic,
    bulkAdd,
    readAfterBulkAdd,
    bulkCodeChange,
    createBxgy,
    bxgyToBasic,
    unknownUpdate,
    cleanup: cleanupResults,
    upstreamCalls: [],
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        basicDiscountId,
        bxgyDiscountId,
      },
      null,
      2,
    ),
  );
} catch (error) {
  cleanupResults = await runCleanup(cleanup);
  console.error(`Cleanup after capture failure: ${JSON.stringify(cleanupResults)}`);
  throw error;
}

async function runCleanup(cleanups: Array<() => Promise<unknown>>): Promise<unknown[]> {
  const results: unknown[] = [];
  for (const cleanupAction of [...cleanups].reverse()) {
    try {
      results.push(await cleanupAction());
    } catch (error) {
      results.push({ error: String(error) });
    }
  }
  return results;
}
