/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'inventory-quantity-contracts-2026-04.json');

const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runGraphqlAllowGraphqlErrors(query: string, variables: Record<string, unknown> = {}): Promise<unknown> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(JSON.stringify(result, null, 2));
  }

  return result.payload;
}

const inputShapeQuery = `#graphql
  query InventoryQuantityContractInputShapes {
    inventoryChangeInput: __type(name: "InventoryChangeInput") {
      inputFields { name }
    }
    inventorySetQuantityInput: __type(name: "InventorySetQuantityInput") {
      inputFields { name }
    }
    inventorySetQuantitiesInput: __type(name: "InventorySetQuantitiesInput") {
      inputFields { name }
    }
  }
`;

const locationsQuery = `#graphql
  query InventoryQuantityContractLocations {
    locations(first: 1) {
      nodes { id name }
    }
  }
`;

const createMutation = `#graphql
  mutation InventoryQuantityContractCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        variants(first: 1) {
          nodes {
            id
            inventoryQuantity
            inventoryItem { id tracked requiresShipping }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const trackInventoryMutation = `#graphql
  mutation InventoryQuantityContractTrack($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product { id tracksInventory totalInventory }
      productVariants {
        id
        inventoryQuantity
        inventoryItem { id tracked requiresShipping }
      }
      userErrors { field message }
    }
  }
`;

const inventorySetMutation = `#graphql
  mutation InventoryQuantityContractSet($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
    inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        id
        createdAt
        reason
        referenceDocumentUri
        changes {
          name
          delta
          quantityAfterChange
          item { id }
          location { id name }
        }
      }
      userErrors { field message }
    }
  }
`;

const inventorySetMissingIdempotencyMutation = `#graphql
  mutation InventoryQuantityContractSetMissingIdempotency($input: InventorySetQuantitiesInput!) {
    inventorySetQuantities(input: $input) {
      inventoryAdjustmentGroup { id }
      userErrors { field message }
    }
  }
`;

const inventoryAdjustMutation = `#graphql
  mutation InventoryQuantityContractAdjust($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
    inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        id
        createdAt
        reason
        referenceDocumentUri
        changes {
          name
          delta
          quantityAfterChange
          item { id }
          location { id name }
        }
      }
      userErrors { field message }
    }
  }
`;

const inventoryAdjustMissingIdempotencyMutation = `#graphql
  mutation InventoryQuantityContractAdjustMissingIdempotency($input: InventoryAdjustQuantitiesInput!) {
    inventoryAdjustQuantities(input: $input) {
      inventoryAdjustmentGroup { id }
      userErrors { field message }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query InventoryQuantityContractDownstream($productId: ID!, $inventoryItemId: ID!) {
    product(id: $productId) {
      id
      totalInventory
      tracksInventory
    }
    inventoryItem(id: $inventoryItemId) {
      id
      tracked
      variant { id inventoryQuantity product { id totalInventory } }
      inventoryLevels(first: 5) {
        nodes {
          id
          location { id name }
          quantities(names: ["available", "on_hand"]) { name quantity updatedAt }
        }
      }
    }
  }
`;

const inventoryItemLevelQuery = `#graphql
  query InventoryQuantityContractInventoryItemLevel($inventoryItemId: ID!) {
    inventoryItem(id: $inventoryItemId) {
      id
      inventoryLevels(first: 5) {
        nodes {
          location { id name }
          quantities(names: ["available", "on_hand"]) { name quantity }
        }
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation InventoryQuantityContractDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

function extractCreatedProduct(payload: unknown): {
  productId: string;
  variantId: string;
  inventoryItemId: string;
} {
  const product = (payload as { data?: { productCreate?: { product?: unknown } } }).data?.productCreate?.product as
    | {
        id?: unknown;
        variants?: { nodes?: Array<{ id?: unknown; inventoryItem?: { id?: unknown } }> };
      }
    | undefined;
  const variant = product?.variants?.nodes?.[0];
  const inventoryItem = variant?.inventoryItem;

  if (typeof product?.id !== 'string' || typeof variant?.id !== 'string' || typeof inventoryItem?.id !== 'string') {
    throw new Error('Inventory quantity contract capture did not return a default variant inventory item.');
  }

  return {
    productId: product.id,
    variantId: variant.id,
    inventoryItemId: inventoryItem.id,
  };
}

async function deleteProduct(productId: string | null): Promise<unknown> {
  if (!productId) {
    return null;
  }

  try {
    return await runGraphqlAllowGraphqlErrors(deleteMutation, { input: { id: productId } });
  } catch (error) {
    console.warn(`Cleanup failed for ${productId}: ${error instanceof Error ? error.message : String(error)}`);
    return null;
  }
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
let productId: string | null = null;

try {
  const schema = await runGraphql(inputShapeQuery);
  const locations = (await runGraphql(locationsQuery)) as {
    data?: { locations?: { nodes?: Array<{ id?: string; name?: string | null }> } };
  };
  const fallbackLocation = locations.data?.locations?.nodes?.[0] ?? null;
  if (!fallbackLocation?.id) {
    throw new Error('Could not resolve a location id for inventory quantity contract capture.');
  }

  const create = await runGraphql(createMutation, {
    product: {
      title: `Hermes Inventory Quantity Contracts 2026-04 ${runId}`,
      status: 'DRAFT',
    },
  });
  const product = extractCreatedProduct(create);
  productId = product.productId;

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
  const inventoryItemLevel = (await runGraphql(inventoryItemLevelQuery, {
    inventoryItemId: product.inventoryItemId,
  })) as {
    data?: {
      inventoryItem?: {
        inventoryLevels?: {
          nodes?: Array<{ location?: { id?: string; name?: string | null }; quantities?: unknown[] }>;
        };
      };
    };
  };
  const location = inventoryItemLevel.data?.inventoryItem?.inventoryLevels?.nodes?.[0]?.location ?? fallbackLocation;
  if (!location?.id) {
    throw new Error('Could not resolve an inventory level location id for inventory quantity contract capture.');
  }

  const setVariables = {
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: `logistics://har-408/set/${runId}`,
      quantities: [
        {
          inventoryItemId: product.inventoryItemId,
          locationId: location.id,
          quantity: 5,
          changeFromQuantity: 0,
        },
      ],
    },
    idempotencyKey: `har-408-set-${runId}`,
  };
  const inventorySet = await runGraphqlAllowGraphqlErrors(inventorySetMutation, setVariables);

  const missingSetIdempotency = await runGraphqlAllowGraphqlErrors(
    inventorySetMissingIdempotencyMutation,
    setVariables,
  );

  const missingSetChangeFromQuantityVariables = {
    input: {
      name: 'available',
      reason: 'correction',
      quantities: [
        {
          inventoryItemId: product.inventoryItemId,
          locationId: location.id,
          quantity: 5,
        },
      ],
    },
    idempotencyKey: `har-408-set-missing-change-from-${runId}`,
  };
  const missingSetChangeFromQuantity = await runGraphqlAllowGraphqlErrors(
    inventorySetMutation,
    missingSetChangeFromQuantityVariables,
  );

  const legacySetVariables = {
    input: {
      name: 'available',
      reason: 'correction',
      ignoreCompareQuantity: true,
      quantities: [
        {
          inventoryItemId: product.inventoryItemId,
          locationId: location.id,
          quantity: 5,
        },
      ],
    },
    idempotencyKey: `har-408-set-legacy-${runId}`,
  };
  const legacySetShape = await runGraphqlAllowGraphqlErrors(inventorySetMutation, legacySetVariables);

  const adjustVariables = {
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: `logistics://har-408/adjust/${runId}`,
      changes: [
        {
          inventoryItemId: product.inventoryItemId,
          locationId: location.id,
          delta: 2,
          changeFromQuantity: 5,
        },
      ],
    },
    idempotencyKey: `har-408-adjust-${runId}`,
  };
  const inventoryAdjust = await runGraphqlAllowGraphqlErrors(inventoryAdjustMutation, adjustVariables);

  const missingAdjustIdempotency = await runGraphqlAllowGraphqlErrors(
    inventoryAdjustMissingIdempotencyMutation,
    adjustVariables,
  );

  const missingAdjustChangeFromQuantityVariables = {
    input: {
      name: 'available',
      reason: 'correction',
      changes: [
        {
          inventoryItemId: product.inventoryItemId,
          locationId: location.id,
          delta: 1,
        },
      ],
    },
  };
  const missingAdjustChangeFromQuantity = await runGraphqlAllowGraphqlErrors(
    inventoryAdjustMissingIdempotencyMutation,
    missingAdjustChangeFromQuantityVariables,
  );

  const downstreamRead = await runGraphql(downstreamReadQuery, {
    productId: product.productId,
    inventoryItemId: product.inventoryItemId,
  });
  const cleanup = await deleteProduct(product.productId);
  productId = null;

  const capturePayload = {
    storeDomain,
    apiVersion,
    schema,
    setup: {
      location,
      create,
      track,
      inventoryItemLevel,
      product,
    },
    inventorySetQuantities: {
      variables: setVariables,
      response: inventorySet,
    },
    missingSetIdempotency: {
      variables: setVariables,
      response: missingSetIdempotency,
    },
    missingSetChangeFromQuantity: {
      variables: missingSetChangeFromQuantityVariables,
      response: missingSetChangeFromQuantity,
    },
    legacySetShape: {
      variables: legacySetVariables,
      response: legacySetShape,
    },
    inventoryAdjustQuantities: {
      variables: adjustVariables,
      response: inventoryAdjust,
    },
    missingAdjustIdempotency: {
      variables: adjustVariables,
      response: missingAdjustIdempotency,
    },
    missingAdjustChangeFromQuantity: {
      variables: missingAdjustChangeFromQuantityVariables,
      response: missingAdjustChangeFromQuantity,
    },
    downstreamRead,
    cleanup,
  };

  await writeFile(outputPath, `${JSON.stringify(capturePayload, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        apiVersion,
        storeDomain,
        productId: product.productId,
        locationId: location.id,
      },
      null,
      2,
    ),
  );
} finally {
  await deleteProduct(productId);
}
