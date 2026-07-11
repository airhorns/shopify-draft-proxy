/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type GraphqlResult = ConformanceGraphqlResult<JsonRecord>;

type RecordedRequest = {
  document: string;
  variables: JsonRecord;
  response: GraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-entitlement-connections.json');
const requestDir = path.join('config', 'parity-requests', 'discounts');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const productDeleteDocument = `#graphql
  mutation DiscountEntitlementConnectionsCleanupProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const collectionDeleteDocument = `#graphql
  mutation DiscountEntitlementConnectionsCleanupCollection($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors {
        field
        message
      }
    }
  }
`;

const discountDeleteDocument = `#graphql
  mutation DiscountEntitlementConnectionsCleanupDiscount($id: ID!) {
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

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

function readRunId(): string {
  const raw = process.env['SHOPIFY_CONFORMANCE_RUN_ID'];
  if (!raw) return String(Date.now());
  if (!/^[0-9]+$/u.test(raw)) {
    throw new Error(`SHOPIFY_CONFORMANCE_RUN_ID must be digits only, got ${JSON.stringify(raw)}`);
  }
  return raw;
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readPath(root: unknown, parts: string[]): unknown {
  let cursor = root;
  for (const part of parts) {
    if (Array.isArray(cursor)) {
      cursor = cursor[Number(part)];
    } else if (isRecord(cursor)) {
      cursor = cursor[part];
    } else {
      return undefined;
    }
  }
  return cursor;
}

function readArray(root: unknown, parts: string[]): unknown[] {
  const value = readPath(root, parts);
  return Array.isArray(value) ? value : [];
}

function requireString(root: unknown, parts: string[], label: string): string {
  const value = readPath(root, parts);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing ${label}: ${JSON.stringify(root, null, 2)}`);
  }
  return value;
}

function assertNoGraphqlErrors(result: GraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(result: GraphqlResult, rootName: string, label: string): void {
  assertNoGraphqlErrors(result, label);
  const userErrors = readArray(result.payload, ['data', rootName, 'userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

async function record(document: string, variables: JsonRecord, label: string): Promise<RecordedRequest> {
  const response = await runGraphqlRaw<JsonRecord>(document, variables);
  assertNoGraphqlErrors(response, label);
  return { document, variables, response };
}

function titles(root: unknown, parts: string[]): string[] {
  return readArray(root, parts).map((node) => requireString(node, ['title'], 'node title'));
}

function skus(root: unknown, parts: string[]): string[] {
  return readArray(root, parts).map((node) => requireString(node, ['sku'], 'variant sku'));
}

function catalogTitles(root: unknown): string[] {
  return readArray(root, ['data', 'catalog', 'nodes']).map((node) =>
    requireString(node, ['discount', 'title'], 'catalog discount title'),
  );
}

function assertStringArray(label: string, actual: string[], expected: string[]): void {
  if (actual.length !== expected.length || actual.some((value, index) => value !== expected[index])) {
    throw new Error(`${label}: expected ${JSON.stringify(expected)} got ${JSON.stringify(actual)}`);
  }
}

function assertInitialRead(response: GraphqlResult, expected: ExpectedState): void {
  const payload = response.payload;
  assertStringArray(
    'customerGets products',
    titles(payload, ['data', 'productDiscount', 'discount', 'customerGets', 'items', 'products', 'nodes']),
    [expected.alphaProductTitle, expected.bravoProductTitle],
  );
  assertStringArray(
    'customerBuys products',
    titles(payload, ['data', 'productDiscount', 'discount', 'customerBuys', 'items', 'products', 'nodes']),
    [expected.alphaProductTitle, expected.bravoProductTitle],
  );
  assertStringArray(
    'customerGets variants',
    skus(payload, ['data', 'productDiscount', 'discount', 'customerGets', 'items', 'productVariants', 'nodes']),
    [expected.alphaVariantSku, expected.bravoVariantSku],
  );
  assertStringArray(
    'customerGets collections',
    titles(payload, ['data', 'collectionDiscount', 'discount', 'customerGets', 'items', 'collections', 'nodes']),
    [expected.alphaCollectionTitle, expected.bravoCollectionTitle],
  );
  assertStringArray('catalog window', catalogTitles(payload), [
    expected.productDiscountTitle,
    expected.collectionDiscountTitle,
  ]);
}

function assertUpdatedRead(response: GraphqlResult, expected: ExpectedState): void {
  const payload = response.payload;
  assertStringArray(
    'updated products',
    titles(payload, ['data', 'productDiscount', 'discount', 'customerGets', 'items', 'products', 'nodes']),
    [expected.updatedProductTitle, expected.bravoProductTitle],
  );
  assertStringArray(
    'updated variants',
    skus(payload, ['data', 'productDiscount', 'discount', 'customerGets', 'items', 'productVariants', 'nodes']),
    [expected.updatedVariantSku, expected.bravoVariantSku],
  );
  assertStringArray(
    'updated collections',
    titles(payload, ['data', 'collectionDiscount', 'discount', 'customerGets', 'items', 'collections', 'nodes']),
    [expected.updatedCollectionTitle, expected.bravoCollectionTitle],
  );
}

function assertAfterVariantDelete(response: GraphqlResult, expected: ExpectedState): void {
  const payload = response.payload;
  assertStringArray(
    'variant delete leaves remaining product refs',
    titles(payload, ['data', 'productDiscount', 'discount', 'customerGets', 'items', 'products', 'nodes']),
    [expected.updatedProductTitle, expected.bravoProductTitle],
  );
  assertStringArray(
    'variant delete removes deleted variant ref',
    skus(payload, ['data', 'productDiscount', 'discount', 'customerGets', 'items', 'productVariants', 'nodes']),
    [expected.bravoVariantSku],
  );
}

function assertAfterResourceDelete(response: GraphqlResult, expected: ExpectedState): void {
  const payload = response.payload;
  assertStringArray(
    'product delete removes deleted product ref',
    titles(payload, ['data', 'productDiscount', 'discount', 'customerGets', 'items', 'products', 'nodes']),
    [expected.bravoProductTitle],
  );
  assertStringArray(
    'collection delete removes deleted collection ref',
    titles(payload, ['data', 'collectionDiscount', 'discount', 'customerGets', 'items', 'collections', 'nodes']),
    [expected.bravoCollectionTitle],
  );
  assertStringArray(
    'catalog product delete removes deleted product ref',
    titles(payload, ['data', 'catalog', 'nodes', '0', 'discount', 'customerGets', 'items', 'products', 'nodes']),
    [expected.bravoProductTitle],
  );
  assertStringArray(
    'catalog collection delete removes deleted collection ref',
    titles(payload, ['data', 'catalog', 'nodes', '1', 'discount', 'customerGets', 'items', 'collections', 'nodes']),
    [expected.bravoCollectionTitle],
  );
}

function edgeCursor(response: GraphqlResult, parts: string[], label: string): string {
  return requireString(response.payload, ['data', ...parts], label);
}

async function waitForInitialRead(
  document: string,
  variables: JsonRecord,
  expected: ExpectedState,
): Promise<RecordedRequest> {
  let lastResult: RecordedRequest | null = null;
  let lastError: unknown = null;
  for (let attempt = 0; attempt < 20; attempt += 1) {
    lastResult = await record(document, variables, 'discount entitlement initial read');
    try {
      assertInitialRead(lastResult.response, expected);
      return lastResult;
    } catch (error) {
      lastError = error;
      await sleep(750);
    }
  }
  throw new Error(
    `discount entitlement initial read did not converge: ${String(lastError)}; last=${JSON.stringify(
      lastResult,
      null,
      2,
    )}`,
  );
}

type ExpectedState = {
  alphaProductTitle: string;
  bravoProductTitle: string;
  alphaVariantProductTitle: string;
  bravoVariantProductTitle: string;
  updatedProductTitle: string;
  alphaVariantSku: string;
  bravoVariantSku: string;
  updatedVariantSku: string;
  alphaCollectionTitle: string;
  bravoCollectionTitle: string;
  updatedCollectionTitle: string;
  productDiscountTitle: string;
  collectionDiscountTitle: string;
};

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const setupDocument = await readRequest('discount-entitlement-connections-setup.graphql');
const variantCreateDocument = await readRequest('discount-entitlement-connections-variant-create.graphql');
const discountCreateDocument = await readRequest('discount-entitlement-connections-discount-create.graphql');
const readInitialDocument = await readRequest('discount-entitlement-connections-read-initial.graphql');
const readWindowDocument = await readRequest('discount-entitlement-connections-read-window.graphql');
const resourceUpdateDocument = await readRequest('discount-entitlement-connections-resource-update.graphql');
const readCurrentDocument = await readRequest('discount-entitlement-connections-read-current.graphql');
const variantDeleteDocument = await readRequest('discount-entitlement-connections-variant-delete.graphql');
const resourceDeleteDocument = await readRequest('discount-entitlement-connections-resource-delete.graphql');

const runId = readRunId();
const startsAt = new Date(Date.now() + 14 * 24 * 60 * 60 * 1000).toISOString();
const titlePrefix = `zzzzzz SDP entitlement ${runId}`;
const expected: ExpectedState = {
  alphaProductTitle: `${titlePrefix} Alpha product`,
  bravoProductTitle: `${titlePrefix} Bravo product`,
  alphaVariantProductTitle: `${titlePrefix} Alpha variant product`,
  bravoVariantProductTitle: `${titlePrefix} Bravo variant product`,
  updatedProductTitle: `${titlePrefix} Alpha product updated`,
  alphaVariantSku: `SDPENTALPHA${runId}`,
  bravoVariantSku: `SDPENTBRAVO${runId}`,
  updatedVariantSku: `SDPENTALPHAUP${runId}`,
  alphaCollectionTitle: `${titlePrefix} Alpha collection`,
  bravoCollectionTitle: `${titlePrefix} Bravo collection`,
  updatedCollectionTitle: `${titlePrefix} Alpha collection updated`,
  productDiscountTitle: `${titlePrefix} Product discount`,
  collectionDiscountTitle: `${titlePrefix} Collection discount`,
};
const catalogVariables = {
  catalogQuery: 'status:scheduled',
  catalogFirst: 2,
};
const cleanup: Array<() => Promise<unknown>> = [];
const cleanupResponses: JsonRecord = {};
let capture: JsonRecord = {};

try {
  const setupVariables = {
    alphaProduct: {
      title: expected.alphaProductTitle,
      status: 'ACTIVE',
      vendor: 'HERMES',
      productType: 'CONFORMANCE',
      tags: ['conformance', 'discount-entitlement-connections', runId],
      productOptions: [{ name: 'Color', values: [{ name: 'Seed' }] }],
    },
    bravoProduct: {
      title: expected.bravoProductTitle,
      status: 'ACTIVE',
      vendor: 'HERMES',
      productType: 'CONFORMANCE',
      tags: ['conformance', 'discount-entitlement-connections', runId],
      productOptions: [{ name: 'Color', values: [{ name: 'Seed' }] }],
    },
    alphaVariantProduct: {
      title: expected.alphaVariantProductTitle,
      status: 'ACTIVE',
      vendor: 'HERMES',
      productType: 'CONFORMANCE',
      tags: ['conformance', 'discount-entitlement-connections', runId],
      productOptions: [{ name: 'Color', values: [{ name: 'Seed' }] }],
    },
    bravoVariantProduct: {
      title: expected.bravoVariantProductTitle,
      status: 'ACTIVE',
      vendor: 'HERMES',
      productType: 'CONFORMANCE',
      tags: ['conformance', 'discount-entitlement-connections', runId],
      productOptions: [{ name: 'Color', values: [{ name: 'Seed' }] }],
    },
    alphaCollection: { title: expected.alphaCollectionTitle },
    bravoCollection: { title: expected.bravoCollectionTitle },
  };
  const setup = await record(setupDocument, setupVariables, 'discount entitlement setup');
  assertNoUserErrors(setup.response, 'alphaProduct', 'alpha productCreate');
  assertNoUserErrors(setup.response, 'bravoProduct', 'bravo productCreate');
  assertNoUserErrors(setup.response, 'alphaVariantProduct', 'alpha variant productCreate');
  assertNoUserErrors(setup.response, 'bravoVariantProduct', 'bravo variant productCreate');
  assertNoUserErrors(setup.response, 'alphaCollection', 'alpha collectionCreate');
  assertNoUserErrors(setup.response, 'bravoCollection', 'bravo collectionCreate');

  const alphaProductId = requireString(
    setup.response.payload,
    ['data', 'alphaProduct', 'product', 'id'],
    'alpha product id',
  );
  const bravoProductId = requireString(
    setup.response.payload,
    ['data', 'bravoProduct', 'product', 'id'],
    'bravo product id',
  );
  const alphaVariantProductId = requireString(
    setup.response.payload,
    ['data', 'alphaVariantProduct', 'product', 'id'],
    'alpha variant product id',
  );
  const bravoVariantProductId = requireString(
    setup.response.payload,
    ['data', 'bravoVariantProduct', 'product', 'id'],
    'bravo variant product id',
  );
  const alphaCollectionId = requireString(
    setup.response.payload,
    ['data', 'alphaCollection', 'collection', 'id'],
    'alpha collection id',
  );
  const bravoCollectionId = requireString(
    setup.response.payload,
    ['data', 'bravoCollection', 'collection', 'id'],
    'bravo collection id',
  );
  cleanup.push(() => runGraphqlRaw(productDeleteDocument, { input: { id: alphaProductId } }));
  cleanup.push(() => runGraphqlRaw(productDeleteDocument, { input: { id: bravoProductId } }));
  cleanup.push(() => runGraphqlRaw(productDeleteDocument, { input: { id: alphaVariantProductId } }));
  cleanup.push(() => runGraphqlRaw(productDeleteDocument, { input: { id: bravoVariantProductId } }));
  cleanup.push(() => runGraphqlRaw(collectionDeleteDocument, { input: { id: alphaCollectionId } }));
  cleanup.push(() => runGraphqlRaw(collectionDeleteDocument, { input: { id: bravoCollectionId } }));

  const variantCreateVariables = {
    alphaProductId: alphaVariantProductId,
    bravoProductId: bravoVariantProductId,
    alphaVariants: [
      {
        optionValues: [{ optionName: 'Color', name: 'Alpha entitlement' }],
        price: '10.00',
        inventoryItem: { sku: expected.alphaVariantSku },
      },
    ],
    bravoVariants: [
      {
        optionValues: [{ optionName: 'Color', name: 'Bravo entitlement' }],
        price: '11.00',
        inventoryItem: { sku: expected.bravoVariantSku },
      },
    ],
  };
  const variantCreate = await record(
    variantCreateDocument,
    variantCreateVariables,
    'discount entitlement variant setup',
  );
  assertNoUserErrors(variantCreate.response, 'alphaVariants', 'alpha productVariantsBulkCreate');
  assertNoUserErrors(variantCreate.response, 'bravoVariants', 'bravo productVariantsBulkCreate');
  const alphaVariantId = requireString(
    variantCreate.response.payload,
    ['data', 'alphaVariants', 'productVariants', '0', 'id'],
    'alpha variant id',
  );
  const bravoVariantId = requireString(
    variantCreate.response.payload,
    ['data', 'bravoVariants', 'productVariants', '0', 'id'],
    'bravo variant id',
  );

  const productItems = {
    products: {
      productsToAdd: [alphaProductId, bravoProductId],
      productVariantsToAdd: [alphaVariantId, bravoVariantId],
    },
  };
  const collectionItems = {
    collections: {
      add: [alphaCollectionId, bravoCollectionId],
    },
  };
  const discountCreateVariables = {
    productInput: {
      title: expected.productDiscountTitle,
      code: `SDPENTP${runId}`,
      startsAt,
      context: { all: 'ALL' },
      combinesWith: {
        productDiscounts: true,
        orderDiscounts: false,
        shippingDiscounts: false,
      },
      customerBuys: {
        value: { quantity: '1' },
        items: productItems,
      },
      customerGets: {
        value: {
          discountOnQuantity: {
            quantity: '1',
            effect: { percentage: 0.5 },
          },
        },
        items: productItems,
      },
    },
    collectionInput: {
      title: expected.collectionDiscountTitle,
      code: `SDPENTC${runId}`,
      startsAt,
      context: { all: 'ALL' },
      combinesWith: {
        productDiscounts: true,
        orderDiscounts: false,
        shippingDiscounts: false,
      },
      customerBuys: {
        value: { quantity: '1' },
        items: collectionItems,
      },
      customerGets: {
        value: {
          discountOnQuantity: {
            quantity: '1',
            effect: { percentage: 0.5 },
          },
        },
        items: collectionItems,
      },
    },
  };
  const discountCreate = await record(
    discountCreateDocument,
    discountCreateVariables,
    'discount entitlement discount setup',
  );
  assertNoUserErrors(discountCreate.response, 'productDiscount', 'product discountCodeBxgyCreate');
  assertNoUserErrors(discountCreate.response, 'collectionDiscount', 'collection discountCodeBxgyCreate');
  const productDiscountId = requireString(
    discountCreate.response.payload,
    ['data', 'productDiscount', 'codeDiscountNode', 'id'],
    'product discount id',
  );
  const collectionDiscountId = requireString(
    discountCreate.response.payload,
    ['data', 'collectionDiscount', 'codeDiscountNode', 'id'],
    'collection discount id',
  );
  cleanup.push(() => runGraphqlRaw(discountDeleteDocument, { id: productDiscountId }));
  cleanup.push(() => runGraphqlRaw(discountDeleteDocument, { id: collectionDiscountId }));

  const readInitialVariables = {
    productDiscountId,
    collectionDiscountId,
    ...catalogVariables,
  };
  const readInitial = await waitForInitialRead(readInitialDocument, readInitialVariables, expected);
  const readWindowVariables = {
    productDiscountId,
    collectionDiscountId,
    productAfter: edgeCursor(
      readInitial.response,
      ['productDiscount', 'discount', 'customerGets', 'items', 'products', 'edges', '0', 'cursor'],
      'product after cursor',
    ),
    productBefore: edgeCursor(
      readInitial.response,
      ['productDiscount', 'discount', 'customerGets', 'items', 'products', 'edges', '1', 'cursor'],
      'product before cursor',
    ),
    variantAfter: edgeCursor(
      readInitial.response,
      ['productDiscount', 'discount', 'customerGets', 'items', 'productVariants', 'edges', '0', 'cursor'],
      'variant after cursor',
    ),
    variantBefore: edgeCursor(
      readInitial.response,
      ['productDiscount', 'discount', 'customerGets', 'items', 'productVariants', 'edges', '1', 'cursor'],
      'variant before cursor',
    ),
    collectionAfter: edgeCursor(
      readInitial.response,
      ['collectionDiscount', 'discount', 'customerGets', 'items', 'collections', 'edges', '0', 'cursor'],
      'collection after cursor',
    ),
    collectionBefore: edgeCursor(
      readInitial.response,
      ['collectionDiscount', 'discount', 'customerGets', 'items', 'collections', 'edges', '1', 'cursor'],
      'collection before cursor',
    ),
  };
  const readWindow = await record(readWindowDocument, readWindowVariables, 'discount entitlement window read');

  const resourceUpdateVariables = {
    product: { id: alphaProductId, title: expected.updatedProductTitle },
    variantProductId: alphaVariantProductId,
    variants: [{ id: alphaVariantId, inventoryItem: { sku: expected.updatedVariantSku } }],
    collection: { id: alphaCollectionId, title: expected.updatedCollectionTitle },
  };
  const resourceUpdate = await record(
    resourceUpdateDocument,
    resourceUpdateVariables,
    'discount entitlement resource update',
  );
  assertNoUserErrors(resourceUpdate.response, 'productUpdate', 'productUpdate');
  assertNoUserErrors(resourceUpdate.response, 'productVariantsBulkUpdate', 'productVariantsBulkUpdate');
  assertNoUserErrors(resourceUpdate.response, 'collectionUpdate', 'collectionUpdate');

  const readUpdated = await record(
    readCurrentDocument,
    readInitialVariables,
    'discount entitlement read after resource update',
  );
  assertUpdatedRead(readUpdated.response, expected);

  const variantDeleteVariables = { productId: alphaVariantProductId, variantsIds: [alphaVariantId] };
  const variantDelete = await record(
    variantDeleteDocument,
    variantDeleteVariables,
    'discount entitlement variant delete',
  );
  assertNoUserErrors(variantDelete.response, 'productVariantsBulkDelete', 'productVariantsBulkDelete');
  const readAfterVariantDelete = await record(
    readCurrentDocument,
    readInitialVariables,
    'discount entitlement read after variant delete',
  );
  assertAfterVariantDelete(readAfterVariantDelete.response, expected);

  const resourceDeleteVariables = {
    productInput: { id: alphaProductId },
    collectionInput: { id: alphaCollectionId },
  };
  const resourceDelete = await record(
    resourceDeleteDocument,
    resourceDeleteVariables,
    'discount entitlement product and collection delete',
  );
  assertNoUserErrors(resourceDelete.response, 'productDelete', 'productDelete');
  assertNoUserErrors(resourceDelete.response, 'collectionDelete', 'collectionDelete');
  const readAfterResourceDelete = await record(
    readCurrentDocument,
    readInitialVariables,
    'discount entitlement read after product and collection delete',
  );
  assertAfterResourceDelete(readAfterResourceDelete.response, expected);

  capture = {
    metadata: {
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      runId,
    },
    requests: {
      setup,
      variantCreate,
      discountCreate,
      readInitial,
      readWindow,
      resourceUpdate,
      readUpdated,
      variantDelete,
      readAfterVariantDelete,
      resourceDelete,
      readAfterResourceDelete,
    },
  };
} finally {
  for (const [index, cleanupStep] of cleanup.reverse().entries()) {
    try {
      cleanupResponses[`cleanup${index + 1}`] = (await cleanupStep()) as JsonRecord;
    } catch (error) {
      cleanupResponses[`cleanup${index + 1}`] = { error: String(error) };
    }
  }
}

capture['cleanup'] = cleanupResponses;
await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`);
console.log(`Wrote ${outputPath}`);
