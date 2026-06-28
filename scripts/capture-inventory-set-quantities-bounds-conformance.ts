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
const outputPath = path.join(outputDir, 'inventorySetQuantities-quantity-bounds.json');

const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type CapturePayload = Record<string, unknown>;

type ProductSetup = {
  productId: string;
  variantId: string;
  inventoryItemId: string;
  locationId: string;
  available: number;
  create: CapturePayload;
  track: CapturePayload;
  inventoryLevel: CapturePayload;
};

type CaseVariables = {
  input: Record<string, unknown>;
  idempotencyKey: string;
};

const createMutation = `#graphql
  mutation InventorySetQuantitiesBoundsCreate($product: ProductCreateInput!) {
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
  mutation InventorySetQuantitiesBoundsTrack(
    $productId: ID!
    $variants: [ProductVariantsBulkInput!]!
  ) {
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
  query InventorySetQuantitiesBoundsLevel($inventoryItemId: ID!) {
    inventoryItem(id: $inventoryItemId) {
      id
      inventoryLevels(first: 1) {
        nodes {
          id
          location { id name }
          quantities(names: ["available", "on_hand"]) { name quantity }
        }
      }
    }
  }
`;

const inventorySetMutation = `#graphql
  mutation InventorySetQuantitiesBounds($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
    inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        reason
        referenceDocumentUri
        changes { name delta item { id } location { id name } }
      }
      userErrors { field message code }
    }
  }
`;

const deleteMutation = `#graphql
  mutation InventorySetQuantitiesBoundsDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

async function runGraphqlAllowGraphqlErrors(
  query: string,
  variables: Record<string, unknown> = {},
): Promise<CapturePayload> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(JSON.stringify(result, null, 2));
  }

  return result.payload as CapturePayload;
}

function readCreatedProduct(payload: CapturePayload): {
  productId: string;
  variantId: string;
  inventoryItemId: string;
} {
  const productCreate = (
    payload as {
      data?: {
        productCreate?: {
          product?: {
            id?: unknown;
            variants?: { nodes?: Array<{ id?: unknown; inventoryItem?: { id?: unknown } }> };
          };
        };
      };
    }
  ).data?.productCreate;
  const product = productCreate?.product;
  const variant = product?.variants?.nodes?.[0];
  const inventoryItemId = variant?.inventoryItem?.id;
  if (typeof product?.id !== 'string' || typeof variant?.id !== 'string' || typeof inventoryItemId !== 'string') {
    throw new Error(`Product setup failed: ${JSON.stringify(productCreate)}`);
  }

  return { productId: product.id, variantId: variant.id, inventoryItemId };
}

function readInventoryLevel(payload: CapturePayload): {
  locationId: string;
  available: number;
} {
  const level = (
    payload as {
      data?: {
        inventoryItem?: {
          inventoryLevels?: {
            nodes?: Array<{
              location?: { id?: unknown };
              quantities?: Array<{ name?: unknown; quantity?: unknown }>;
            }>;
          };
        };
      };
    }
  ).data?.inventoryItem?.inventoryLevels?.nodes?.[0];
  if (!level || typeof level.location?.id !== 'string') {
    throw new Error(`Inventory level setup failed: ${JSON.stringify(payload)}`);
  }
  const available = level.quantities?.find((entry) => entry.name === 'available')?.quantity;

  return {
    locationId: level.location.id,
    available: typeof available === 'number' ? available : 0,
  };
}

async function deleteProduct(productId: string | null): Promise<CapturePayload | null> {
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

async function createSetup(runId: string, caseId: string): Promise<ProductSetup> {
  const create = await runGraphql(createMutation, {
    product: {
      title: `inventorySetQuantities bounds ${apiVersion} ${caseId} ${runId}`,
      status: 'DRAFT',
    },
  });
  const product = readCreatedProduct(create as CapturePayload);
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
  const inventoryLevel = await runGraphql(inventoryLevelQuery, {
    inventoryItemId: product.inventoryItemId,
  });
  const level = readInventoryLevel(inventoryLevel as CapturePayload);

  return {
    ...product,
    ...level,
    create: create as CapturePayload,
    track: track as CapturePayload,
    inventoryLevel: inventoryLevel as CapturePayload,
  };
}

function setVariables(
  setup: ProductSetup,
  runId: string,
  caseId: string,
  name: 'available' | 'on_hand',
  quantity: number,
): CaseVariables {
  return {
    input: {
      name,
      reason: 'correction',
      referenceDocumentUri: `logistics://inventory-set-quantities/bounds/${apiVersion}/${caseId}`,
      quantities: [
        {
          inventoryItemId: setup.inventoryItemId,
          locationId: setup.locationId,
          quantity,
          changeFromQuantity: setup.available,
        },
      ],
    },
    idempotencyKey: `inventory-set-quantities-bounds-${caseId}-${runId}`,
  };
}

async function captureCase(
  runId: string,
  caseId: string,
  variablesFor: (setup: ProductSetup) => CaseVariables,
): Promise<CapturePayload> {
  let productId: string | null = null;
  try {
    const setup = await createSetup(runId, caseId);
    productId = setup.productId;
    const variables = variablesFor(setup);
    const response = await runGraphqlAllowGraphqlErrors(inventorySetMutation, variables);
    const cleanup = await deleteProduct(setup.productId);
    productId = null;

    return {
      setup,
      query: inventorySetMutation,
      variables,
      response,
      cleanup,
    };
  } finally {
    await deleteProduct(productId);
  }
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const cases = {
  onHandNegative: await captureCase(runId, 'on-hand-negative', (setup) =>
    setVariables(setup, runId, 'on-hand-negative', 'on_hand', -5),
  ),
  availableTooLow: await captureCase(runId, 'available-too-low', (setup) =>
    setVariables(setup, runId, 'available-too-low', 'available', -2_000_000_000),
  ),
  availableNegativeWithinBounds: await captureCase(runId, 'available-negative-within-bounds', (setup) =>
    setVariables(setup, runId, 'available-negative-within-bounds', 'available', -5),
  ),
};

const capturePayload = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  cases,
  upstreamCalls: [],
};

await writeFile(outputPath, `${JSON.stringify(capturePayload, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      apiVersion,
      storeDomain,
      caseIds: Object.keys(cases),
    },
    null,
    2,
  ),
);
