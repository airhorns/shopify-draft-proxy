/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CreatedProduct = {
  response: ConformanceGraphqlPayload;
  productId: string;
  variantId: string;
};

type CreatedCollection = {
  response: ConformanceGraphqlPayload;
  collectionId: string;
};

const requestPaths = {
  setMutation: 'config/parity-requests/products/metafieldsSet-parity-plan-no-compare-digest.graphql',
  setRead: 'config/parity-requests/products/metafieldsSet-downstream-read-no-compare-digest.graphql',
  ownerExpansionMutation: 'config/parity-requests/products/metafieldsSet-owner-expansion-no-compare-digest.graphql',
  ownerExpansionRead:
    'config/parity-requests/products/metafieldsSet-owner-expansion-downstream-read-no-compare-digest.graphql',
} as const;

const createProductMutation = `#graphql
  mutation ProductMetafieldNonCasCreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        variants(first: 1) {
          nodes {
            id
            title
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

const deleteProductMutation = `#graphql
  mutation ProductMetafieldNonCasDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const createCollectionMutation = `#graphql
  mutation ProductMetafieldNonCasCreateCollection($input: CollectionInput!) {
    collectionCreate(input: $input) {
      collection {
        id
        title
        handle
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteCollectionMutation = `#graphql
  mutation ProductMetafieldNonCasDeleteCollection($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors {
        field
        message
      }
    }
  }
`;

const ownerMetafieldsHydrateQuery = `query OwnerMetafieldsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { __typename id ... on Product { id title handle status totalInventory tracksInventory createdAt updatedAt metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } } ... on ProductVariant { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } product { id title handle status totalInventory tracksInventory createdAt updatedAt } metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Collection { id title handle metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Customer { id displayName email metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Order { id name metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Company { id name metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } } }`;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function getPath(value: unknown, pathParts: string[]): unknown {
  let cursor = value;
  for (const part of pathParts) {
    if (!isRecord(cursor)) return undefined;
    cursor = cursor[part];
  }
  return cursor;
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return a string: ${JSON.stringify(value)}`);
  }
  return value;
}

function requireNoUserErrors(payload: ConformanceGraphqlPayload, pathParts: string[], label: string): void {
  const errors = getPath(payload.data, pathParts);
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function buildCreateProductVariables(runId: string, label: string): JsonRecord {
  return {
    product: {
      title: `Hermes Metafield Non-CAS ${label} ${runId}`,
      status: 'DRAFT',
    },
  };
}

function buildCreateCollectionVariables(runId: string, label: string): JsonRecord {
  return {
    input: {
      title: `Hermes Metafield Non-CAS ${label} Collection ${runId}`,
    },
  };
}

function buildMetafieldsSetVariables(productId: string): JsonRecord {
  return {
    metafields: [
      {
        ownerId: productId,
        namespace: 'custom',
        key: 'material',
        type: 'single_line_text_field',
        value: 'Canvas',
      },
      {
        ownerId: productId,
        namespace: 'details',
        key: 'origin',
        type: 'single_line_text_field',
        value: 'VN',
      },
    ],
  };
}

function buildMissingNamespaceVariables(productId: string): JsonRecord {
  return {
    metafields: [
      {
        ownerId: productId,
        key: 'missing_namespace',
        type: 'single_line_text_field',
        value: 'Missing namespace',
      },
    ],
  };
}

function buildOwnerExpansionVariables(variantId: string, collectionId: string): JsonRecord {
  return {
    metafields: [
      {
        ownerId: variantId,
        namespace: 'custom',
        key: 'variant_care',
        type: 'single_line_text_field',
        value: 'Spot clean',
      },
      {
        ownerId: collectionId,
        namespace: 'custom',
        key: 'collection_season',
        type: 'single_line_text_field',
        value: 'Winter',
      },
    ],
  };
}

async function readGraphql(relativePath: string): Promise<string> {
  return await readFile(relativePath, 'utf8');
}

async function writeJson(filePath: string, value: unknown): Promise<void> {
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const [setMutation, setReadQuery, ownerExpansionMutation, ownerExpansionReadQuery] = await Promise.all([
  readGraphql(requestPaths.setMutation),
  readGraphql(requestPaths.setRead),
  readGraphql(requestPaths.ownerExpansionMutation),
  readGraphql(requestPaths.ownerExpansionRead),
]);

async function createProduct(runId: string, label: string): Promise<CreatedProduct> {
  const response = await runGraphql(createProductMutation, buildCreateProductVariables(runId, label));
  requireNoUserErrors(response, ['productCreate', 'userErrors'], `${label} productCreate`);
  const productId = requireString(getPath(response.data, ['productCreate', 'product', 'id']), `${label} product id`);
  const variantNodes = getPath(response.data, ['productCreate', 'product', 'variants', 'nodes']);
  const firstVariant = Array.isArray(variantNodes) ? variantNodes[0] : undefined;
  const variantId = requireString(
    isRecord(firstVariant) ? firstVariant['id'] : undefined,
    `${label} default variant id`,
  );
  return { response, productId, variantId };
}

async function createCollection(runId: string, label: string): Promise<CreatedCollection> {
  const response = await runGraphql(createCollectionMutation, buildCreateCollectionVariables(runId, label));
  requireNoUserErrors(response, ['collectionCreate', 'userErrors'], `${label} collectionCreate`);
  const collectionId = requireString(
    getPath(response.data, ['collectionCreate', 'collection', 'id']),
    `${label} collection id`,
  );
  return { response, collectionId };
}

async function cleanupProduct(productId: string | null): Promise<void> {
  if (!productId) return;
  try {
    await runGraphql(deleteProductMutation, { input: { id: productId } });
  } catch (error) {
    console.warn(
      JSON.stringify({
        ok: false,
        cleanup: 'productDelete',
        productId,
        error: error instanceof Error ? error.message : String(error),
      }),
    );
  }
}

async function cleanupCollection(collectionId: string | null): Promise<void> {
  if (!collectionId) return;
  try {
    await runGraphql(deleteCollectionMutation, { input: { id: collectionId } });
  } catch (error) {
    console.warn(
      JSON.stringify({
        ok: false,
        cleanup: 'collectionDelete',
        collectionId,
        error: error instanceof Error ? error.message : String(error),
      }),
    );
  }
}

async function captureOwnerMetafieldsHydrateCall(ids: string[]): Promise<JsonRecord> {
  const sortedIds = [...ids].sort();
  const variables = { ids: sortedIds };
  const body = await runGraphql(ownerMetafieldsHydrateQuery, variables);
  return {
    operationName: 'OwnerMetafieldsHydrateNodes',
    variables,
    query: ownerMetafieldsHydrateQuery,
    response: {
      status: 200,
      body,
    },
  };
}

async function captureProductScenario(
  runId: string,
  label: string,
  fileName: string,
  buildVariables: (productId: string) => JsonRecord,
): Promise<string> {
  let productId: string | null = null;
  try {
    const product = await createProduct(runId, label);
    productId = product.productId;
    const variables = buildVariables(productId);
    const mutation = await runGraphql(setMutation, variables);
    const downstreamReadVariables = { id: productId };
    const downstreamRead = await runGraphql(setReadQuery, downstreamReadVariables);
    const upstreamCalls = [await captureOwnerMetafieldsHydrateCall([productId])];
    await writeJson(path.join(outputDir, fileName), {
      mutation: {
        variables,
        response: mutation,
      },
      downstreamReadVariables,
      downstreamRead,
      upstreamCalls,
    });
    return fileName;
  } finally {
    await cleanupProduct(productId);
  }
}

async function captureOwnerExpansionScenario(runId: string): Promise<string> {
  const fileName = 'metafields-set-owner-expansion-parity.json';
  let productId: string | null = null;
  let collectionId: string | null = null;
  try {
    const product = await createProduct(runId, 'owner-expansion');
    productId = product.productId;
    const collection = await createCollection(runId, 'owner-expansion');
    collectionId = collection.collectionId;
    const variables = buildOwnerExpansionVariables(product.variantId, collectionId);
    const mutation = await runGraphql(ownerExpansionMutation, variables);
    const downstreamReadVariables = {
      productId,
      variantId: product.variantId,
      collectionId,
    };
    const downstreamRead = await runGraphql(ownerExpansionReadQuery, downstreamReadVariables);
    const upstreamCalls = [await captureOwnerMetafieldsHydrateCall([product.variantId, collectionId])];
    await writeJson(path.join(outputDir, fileName), {
      seedProduct: getPath(product.response.data, ['productCreate', 'product']) ?? null,
      seedCollection: getPath(collection.response.data, ['collectionCreate', 'collection']) ?? null,
      mutation: {
        variables,
        response: mutation,
      },
      downstreamReadVariables,
      downstreamRead,
      upstreamCalls,
    });
    return fileName;
  } finally {
    await cleanupCollection(collectionId);
    await cleanupProduct(productId);
  }
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const files = [
  await captureProductScenario(runId, 'set-parity', 'metafields-set-parity.json', buildMetafieldsSetVariables),
  await captureProductScenario(
    runId,
    'missing-namespace',
    'metafields-set-missing-namespace-parity.json',
    buildMissingNamespaceVariables,
  ),
  await captureOwnerExpansionScenario(runId),
];

console.log(
  JSON.stringify(
    {
      ok: true,
      outputDir,
      files,
    },
    null,
    2,
  ),
);
