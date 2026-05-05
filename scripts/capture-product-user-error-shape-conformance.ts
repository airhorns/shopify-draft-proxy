/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureEntry = {
  document: string;
  variables: Record<string, unknown>;
  result: ConformanceGraphqlResult;
};

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-user-error-shape-parity.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function capture(document: string, variables: Record<string, unknown>): Promise<CaptureEntry> {
  const result = await runGraphqlRequest(document, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(JSON.stringify(result, null, 2));
  }

  return { document, variables, result };
}

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathSegments: readonly string[]): unknown {
  let current: unknown = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      current = current[Number(segment)];
      continue;
    }
    const record = readRecord(current);
    if (!record) {
      return undefined;
    }
    current = record[segment];
  }
  return current;
}

async function readFirstLocationId(): Promise<string> {
  const locations = await capture(
    `#graphql
      query ProductUserErrorShapeLocations {
        locations(first: 1) {
          nodes { id }
        }
      }
    `,
    {},
  );
  const id = readPath(locations.result.payload, ['data', 'locations', 'nodes', '0', 'id']);
  if (typeof id !== 'string') {
    throw new Error('Unable to resolve a location id for inventoryActivate validation capture.');
  }
  return id;
}

const productCreateBlankDocument = `#graphql
  mutation ProductUserErrorShapeProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product { id }
      userErrors { field message }
    }
  }
`;

const productOptionsCreateUnknownDocument = `#graphql
  mutation ProductUserErrorShapeProductOptionsCreate($productId: ID!, $options: [OptionCreateInput!]!) {
    productOptionsCreate(productId: $productId, options: $options) {
      product { id }
      userErrors { field message code }
    }
  }
`;

const productOptionsDeleteUnknownDocument = `#graphql
  mutation ProductUserErrorShapeProductOptionsDelete($productId: ID!, $options: [ID!]!) {
    productOptionsDelete(productId: $productId, options: $options) {
      deletedOptionsIds
      product { id }
      userErrors { field message code }
    }
  }
`;

const productVariantsBulkReorderUnknownDocument = `#graphql
  mutation ProductUserErrorShapeProductVariantsBulkReorder(
    $productId: ID!
    $positions: [ProductVariantPositionInput!]!
  ) {
    productVariantsBulkReorder(productId: $productId, positions: $positions) {
      product { id }
      userErrors { field message code }
    }
  }
`;

const collectionCreateBlankDocument = `#graphql
  mutation ProductUserErrorShapeCollectionCreate($input: CollectionInput!) {
    collectionCreate(input: $input) {
      collection { id }
      userErrors { field message }
    }
  }
`;

const inventoryActivateUnknownDocument = `#graphql
  mutation ProductUserErrorShapeInventoryActivate($inventoryItemId: ID!, $locationId: ID!) {
    inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId) {
      inventoryLevel { id }
      userErrors { field message }
    }
  }
`;

const missingProductId = 'gid://shopify/Product/999999999999999';
const missingInventoryItemId = 'gid://shopify/InventoryItem/999999999999999';
const firstLocationId = await readFirstLocationId();

const captures = {
  productCreateBlank: await capture(productCreateBlankDocument, {
    product: { title: '' },
  }),
  productOptionsCreateUnknown: await capture(productOptionsCreateUnknownDocument, {
    productId: missingProductId,
    options: [{ name: 'Color', values: [{ name: 'Red' }] }],
  }),
  productOptionsDeleteUnknown: await capture(productOptionsDeleteUnknownDocument, {
    productId: missingProductId,
    options: ['gid://shopify/ProductOption/999999999999999'],
  }),
  productVariantsBulkReorderUnknown: await capture(productVariantsBulkReorderUnknownDocument, {
    productId: missingProductId,
    positions: [],
  }),
  collectionCreateBlank: await capture(collectionCreateBlankDocument, {
    input: { title: '' },
  }),
  inventoryActivateUnknownItem: await capture(inventoryActivateUnknownDocument, {
    inventoryItemId: missingInventoryItemId,
    locationId: firstLocationId,
  }),
};

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'product-user-error-shape-parity',
      apiVersion,
      storeDomain,
      notes: [
        'HAR-586 live validation capture for product-domain userError field/message/code shape.',
        'Current Admin GraphQL exposes code on typed product option and bulk reorder userError objects, but productCreate, productUpdate, collectionCreate, and inventoryActivate still expose the generic UserError type without code in public introspection.',
      ],
      captures,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);

console.log(`Wrote ${outputPath}`);
