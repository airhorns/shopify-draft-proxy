/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type ProductSetup = {
  productId: string;
  variantId: string;
  inventoryItemId: string;
  locationId: string;
  available: number;
  onHand: number;
  committed: number;
  create: unknown;
  track: unknown;
  baselineRead: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'inventory-adjust-name-allowlist.json');

const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runGraphqlAllowGraphqlErrors(query: string, variables: JsonRecord = {}): Promise<unknown> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(JSON.stringify(result, null, 2));
  }

  return result.payload;
}

const createMutation = `#graphql
  mutation InventoryAdjustNameAllowlistCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        variants(first: 1) {
          nodes {
            id
            inventoryItem { id tracked requiresShipping }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const trackInventoryMutation = `#graphql
  mutation InventoryAdjustNameAllowlistTrack($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product { id tracksInventory totalInventory }
      productVariants {
        id
        inventoryItem { id tracked requiresShipping }
      }
      userErrors { field message }
    }
  }
`;

const inventoryLevelQuery = `#graphql
  query InventoryAdjustNameAllowlistInventoryLevel($inventoryItemId: ID!) {
    inventoryItem(id: $inventoryItemId) {
      id
      inventoryLevels(first: 5) {
        nodes {
          id
          location { id name }
          quantities(names: ["available", "on_hand", "committed", "incoming", "reserved", "damaged", "quality_control", "safety_stock"]) {
            name
            quantity
            updatedAt
          }
        }
      }
    }
  }
`;

const inventoryAdjustMutation = `#graphql
  mutation InventoryAdjustNameAllowlistAdjust(
    $input: InventoryAdjustQuantitiesInput!
    $idempotencyKey: String!
  ) {
    inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        reason
        referenceDocumentUri
        changes {
          name
          delta
          ledgerDocumentUri
          item { id }
          location { id name }
        }
      }
      userErrors { field message }
    }
  }
`;

const inventoryMoveMutation = `#graphql
  mutation InventoryAdjustNameAllowlistMove(
    $input: InventoryMoveQuantitiesInput!
    $idempotencyKey: String!
  ) {
    inventoryMoveQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        reason
        referenceDocumentUri
        changes {
          name
          delta
          ledgerDocumentUri
          location { id name }
        }
      }
      userErrors { field message }
    }
  }
`;

const inventorySetOnHandMutation = `#graphql
  mutation InventorySetOnHandQuantitiesAllowlist(
    $input: InventorySetOnHandQuantitiesInput!
    $idempotencyKey: String!
  ) {
    inventorySetOnHandQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        reason
        referenceDocumentUri
        changes {
          name
          delta
          ledgerDocumentUri
          item { id }
          location { id name }
        }
      }
      userErrors { field message }
    }
  }
`;

const deleteMutation = `#graphql
  mutation InventoryAdjustNameAllowlistDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

function readObject(value: unknown, context: string): JsonRecord {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }

  return value as JsonRecord;
}

function readArray(value: unknown, context: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${context} was not an array: ${JSON.stringify(value)}`);
  }

  return value;
}

function readString(value: unknown, context: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${context} was not a non-empty string: ${JSON.stringify(value)}`);
  }

  return value;
}

function readNumber(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0;
}

function readCreatedProduct(payload: unknown): Pick<ProductSetup, 'productId' | 'variantId' | 'inventoryItemId'> {
  const root = readObject(payload, 'productCreate payload');
  const data = readObject(root['data'], 'productCreate data');
  const productCreate = readObject(data['productCreate'], 'productCreate result');
  const product = readObject(productCreate['product'], 'created product');
  const variants = readObject(product['variants'], 'created product variants');
  const variant = readObject(readArray(variants['nodes'], 'created variant nodes')[0], 'created variant');
  const inventoryItem = readObject(variant['inventoryItem'], 'created inventory item');

  return {
    productId: readString(product['id'], 'created product id'),
    variantId: readString(variant['id'], 'created variant id'),
    inventoryItemId: readString(inventoryItem['id'], 'created inventory item id'),
  };
}

function readInventoryLevel(payload: unknown): Pick<ProductSetup, 'locationId' | 'available' | 'onHand' | 'committed'> {
  const root = readObject(payload, 'inventory level payload');
  const data = readObject(root['data'], 'inventory level data');
  const inventoryItem = readObject(data['inventoryItem'], 'inventory item');
  const inventoryLevels = readObject(inventoryItem['inventoryLevels'], 'inventory levels');
  const level = readObject(readArray(inventoryLevels['nodes'], 'inventory level nodes')[0], 'inventory level');
  const location = readObject(level['location'], 'inventory level location');
  const quantities = readArray(level['quantities'], 'inventory level quantities').map((entry) =>
    readObject(entry, 'inventory quantity'),
  );
  const quantity = (name: string): number =>
    readNumber(quantities.find((entry) => entry['name'] === name)?.['quantity']);

  return {
    locationId: readString(location['id'], 'inventory level location id'),
    available: quantity('available'),
    onHand: quantity('on_hand'),
    committed: quantity('committed'),
  };
}

async function deleteProduct(productId: string | null): Promise<unknown | null> {
  if (productId === null) {
    return null;
  }

  try {
    return await runGraphqlAllowGraphqlErrors(deleteMutation, { input: { id: productId } });
  } catch (error) {
    console.warn(`Cleanup failed for ${productId}: ${error instanceof Error ? error.message : String(error)}`);
    return null;
  }
}

async function createSetup(runId: string): Promise<ProductSetup> {
  const create = await runGraphql(createMutation, {
    product: {
      title: `Inventory name allowlist ${apiVersion} ${runId}`,
      status: 'DRAFT',
    },
  });
  const product = readCreatedProduct(create);
  const track = await runGraphql(trackInventoryMutation, {
    productId: product.productId,
    variants: [
      {
        id: product.variantId,
        inventoryItem: {
          tracked: true,
          requiresShipping: true,
        },
      },
    ],
  });
  const baselineRead = await runGraphql(inventoryLevelQuery, {
    inventoryItemId: product.inventoryItemId,
  });
  const level = readInventoryLevel(baselineRead);

  return {
    ...product,
    ...level,
    create,
    track,
    baselineRead,
  };
}

function adjustVariables(setup: ProductSetup, runId: string, name: 'on_hand' | 'committed'): JsonRecord {
  const currentQuantity = name === 'on_hand' ? setup.onHand : setup.committed;
  const caseName = name.replace('_', '-');

  return {
    input: {
      name,
      reason: 'correction',
      referenceDocumentUri: `logistics://inventory-name-allowlist/${apiVersion}/${caseName}/${runId}`,
      changes: [
        {
          inventoryItemId: setup.inventoryItemId,
          locationId: setup.locationId,
          delta: 1,
          changeFromQuantity: currentQuantity,
          ledgerDocumentUri: `ledger://inventory-name-allowlist/${caseName}/${runId}`,
        },
      ],
    },
    idempotencyKey: `inventory-name-allowlist-${caseName}-${runId}`,
  };
}

function moveVariables(setup: ProductSetup, runId: string, name: 'on_hand' | 'committed'): JsonRecord {
  const caseName = name.replace('_', '-');
  const currentQuantity = name === 'on_hand' ? setup.onHand : setup.committed;
  const destinationName = name === 'on_hand' ? 'damaged' : 'reserved';

  return {
    input: {
      reason: 'correction',
      referenceDocumentUri: `logistics://inventory-name-allowlist/${apiVersion}/move-${caseName}/${runId}`,
      changes: [
        {
          inventoryItemId: setup.inventoryItemId,
          quantity: 1,
          from: {
            locationId: setup.locationId,
            name,
            changeFromQuantity: currentQuantity,
            ledgerDocumentUri: `ledger://inventory-name-allowlist/move-${caseName}-from/${runId}`,
          },
          to: {
            locationId: setup.locationId,
            name: destinationName,
            changeFromQuantity: 0,
            ledgerDocumentUri: `ledger://inventory-name-allowlist/move-${destinationName}-to/${runId}`,
          },
        },
      ],
    },
    idempotencyKey: `inventory-name-allowlist-move-${caseName}-${runId}`,
  };
}

function setOnHandVariables(setup: ProductSetup, runId: string): JsonRecord {
  return {
    input: {
      reason: 'correction',
      referenceDocumentUri: `logistics://inventory-name-allowlist/${apiVersion}/set-on-hand/${runId}`,
      setQuantities: [
        {
          inventoryItemId: setup.inventoryItemId,
          locationId: setup.locationId,
          quantity: setup.onHand + 3,
          changeFromQuantity: setup.onHand,
        },
      ],
    },
    idempotencyKey: `inventory-name-allowlist-set-on-hand-${runId}`,
  };
}

async function capture(): Promise<JsonRecord> {
  const runId = `${Date.now()}`;
  let productId: string | null = null;
  try {
    const setup = await createSetup(runId);
    productId = setup.productId;

    const adjustOnHandVariables = adjustVariables(setup, runId, 'on_hand');
    const adjustOnHandResponse = await runGraphqlAllowGraphqlErrors(inventoryAdjustMutation, adjustOnHandVariables);

    const adjustCommittedVariables = adjustVariables(setup, runId, 'committed');
    const adjustCommittedResponse = await runGraphqlAllowGraphqlErrors(
      inventoryAdjustMutation,
      adjustCommittedVariables,
    );

    const moveOnHandVariables = moveVariables(setup, runId, 'on_hand');
    const moveOnHandResponse = await runGraphqlAllowGraphqlErrors(inventoryMoveMutation, moveOnHandVariables);

    const moveCommittedVariables = moveVariables(setup, runId, 'committed');
    const moveCommittedResponse = await runGraphqlAllowGraphqlErrors(inventoryMoveMutation, moveCommittedVariables);

    const afterInvalidsRead = await runGraphql(inventoryLevelQuery, {
      inventoryItemId: setup.inventoryItemId,
    });

    const setOnHandVariablesValue = setOnHandVariables(setup, runId);
    const setOnHandResponse = await runGraphqlAllowGraphqlErrors(inventorySetOnHandMutation, setOnHandVariablesValue);

    const afterSetOnHandRead = await runGraphql(inventoryLevelQuery, {
      inventoryItemId: setup.inventoryItemId,
    });

    const cleanup = await deleteProduct(setup.productId);
    productId = null;

    return {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      setup,
      cases: {
        adjustOnHand: {
          query: inventoryAdjustMutation,
          variables: adjustOnHandVariables,
          response: adjustOnHandResponse,
        },
        adjustCommitted: {
          query: inventoryAdjustMutation,
          variables: adjustCommittedVariables,
          response: adjustCommittedResponse,
        },
        moveOnHand: {
          query: inventoryMoveMutation,
          variables: moveOnHandVariables,
          response: moveOnHandResponse,
        },
        moveCommitted: {
          query: inventoryMoveMutation,
          variables: moveCommittedVariables,
          response: moveCommittedResponse,
        },
        setOnHand: {
          query: inventorySetOnHandMutation,
          variables: setOnHandVariablesValue,
          response: setOnHandResponse,
        },
      },
      afterInvalidsRead,
      afterSetOnHandRead,
      cleanup,
      upstreamCalls: [
        {
          operationName: 'InventorySetOnHandQuantitiesAllowlist',
          variables: setOnHandVariablesValue,
          query: inventorySetOnHandMutation,
          response: { status: 200, body: setOnHandResponse },
        },
      ],
    };
  } finally {
    await deleteProduct(productId);
  }
}

await mkdir(outputDir, { recursive: true });
const payload = await capture();
await writeFile(outputPath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      apiVersion,
      storeDomain,
      cases: Object.keys(readObject(payload['cases'], 'cases')),
    },
    null,
    2,
  ),
);
