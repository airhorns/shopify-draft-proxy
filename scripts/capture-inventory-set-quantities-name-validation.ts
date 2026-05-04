/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'inventorySetQuantities-name-validation.json');

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
  onHand: number;
  create: CapturePayload;
  track: CapturePayload;
  inventoryLevel: CapturePayload;
};

type CaseVariables = {
  input: Record<string, unknown>;
  idempotencyKey?: string;
};

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

const createMutation = `#graphql
  mutation InventorySetQuantitiesNameValidationCreate($product: ProductCreateInput!) {
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
  mutation InventorySetQuantitiesNameValidationTrack(
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
  query InventorySetQuantitiesNameValidationLevel($inventoryItemId: ID!) {
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

const inventorySetMutation202501 = `#graphql
  mutation InventorySetQuantitiesNameValidation($input: InventorySetQuantitiesInput!) {
    inventorySetQuantities(input: $input) {
      inventoryAdjustmentGroup {
        reason
        referenceDocumentUri
        changes { name delta item { id } location { id name } }
      }
      userErrors { field message code }
    }
  }
`;

const inventorySetMutation202604 = `#graphql
  mutation InventorySetQuantitiesNameValidation(
    $input: InventorySetQuantitiesInput!
    $idempotencyKey: String!
  ) {
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
  mutation InventorySetQuantitiesNameValidationDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

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
  onHand: number;
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
  const locationId = level.location.id;

  const quantity = (name: string): number => {
    const found = level.quantities?.find((entry) => entry.name === name)?.quantity;
    return typeof found === 'number' ? found : 0;
  };

  return {
    locationId,
    available: quantity('available'),
    onHand: quantity('on_hand'),
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

function isVersion202604(): boolean {
  return apiVersion === '2026-04';
}

function inventorySetMutation(): string {
  return isVersion202604() ? inventorySetMutation202604 : inventorySetMutation202501;
}

function setVariables(
  setup: ProductSetup,
  runId: string,
  caseId: string,
  name: string,
  quantity: number,
  compareQuantity: number,
): CaseVariables {
  const quantityInput = {
    inventoryItemId: setup.inventoryItemId,
    locationId: setup.locationId,
    quantity,
    ...(isVersion202604() ? { changeFromQuantity: compareQuantity } : {}),
  };
  return {
    input: {
      name,
      reason: 'correction',
      referenceDocumentUri: `logistics://har-568/${apiVersion}/${caseId}/${runId}`,
      ...(!isVersion202604() ? { ignoreCompareQuantity: true } : {}),
      quantities: [quantityInput],
    },
    ...(isVersion202604() ? { idempotencyKey: `har-568-${caseId}-${runId}` } : {}),
  };
}

function duplicateVariables(setup: ProductSetup, runId: string): CaseVariables {
  const quantityInput = (quantity: number) => ({
    inventoryItemId: setup.inventoryItemId,
    locationId: setup.locationId,
    quantity,
    ...(isVersion202604() ? { changeFromQuantity: setup.available } : {}),
  });
  return {
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: `logistics://har-568/${apiVersion}/duplicate/${runId}`,
      ...(!isVersion202604() ? { ignoreCompareQuantity: true } : {}),
      quantities: [quantityInput(setup.available + 1), quantityInput(setup.available + 2)],
    },
    ...(isVersion202604() ? { idempotencyKey: `har-568-duplicate-${runId}` } : {}),
  };
}

async function createSetup(runId: string, caseId: string): Promise<ProductSetup> {
  const create = await runGraphql(createMutation, {
    product: {
      title: `HAR-568 inventorySetQuantities ${apiVersion} ${caseId} ${runId}`,
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
    const response = await runGraphqlAllowGraphqlErrors(inventorySetMutation(), variables);
    const cleanup = await deleteProduct(setup.productId);
    productId = null;

    return {
      setup,
      query: inventorySetMutation(),
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
  damaged: await captureCase(runId, 'damaged', (setup) =>
    setVariables(setup, runId, 'damaged', 'damaged', setup.available + 5, 0),
  ),
  committed: await captureCase(runId, 'committed', (setup) =>
    setVariables(setup, runId, 'committed', 'committed', setup.available + 5, 0),
  ),
  onHand: await captureCase(runId, 'on-hand', (setup) =>
    setVariables(setup, runId, 'on-hand', 'on_hand', setup.onHand + 4, setup.onHand),
  ),
  availableTooHigh: await captureCase(runId, 'available-too-high', (setup) =>
    setVariables(setup, runId, 'available-too-high', 'available', 1_000_000_001, setup.available),
  ),
  duplicatePair: await captureCase(runId, 'duplicate-pair', (setup) => duplicateVariables(setup, runId)),
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
