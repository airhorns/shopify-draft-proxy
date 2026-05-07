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
const outputPath = path.join(outputDir, 'inventory-reason-validation.json');

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
  setup: CapturePayload;
};

const candidateReasons = [
  'correction',
  'cycle_count_available',
  'damaged',
  'movement_canceled',
  'movement_created',
  'movement_received',
  'movement_updated',
  'other',
  'promotion',
  'quality_control',
  'received',
  'reservation_created',
  'reservation_deleted',
  'reservation_updated',
  'restock',
  'safety_stock',
  'shrinkage',
] as const;

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

const productSetMutation = `#graphql
  mutation InventoryReasonValidationSetup($input: ProductSetInput!, $synchronous: Boolean!) {
    productSet(input: $input, synchronous: $synchronous) {
      product {
        id
        variants(first: 1) {
          nodes {
            id
            inventoryItem {
              id
              inventoryLevels(first: 1) {
                nodes {
                  location { id name }
                  quantities(names: ["available", "on_hand"]) { name quantity }
                }
              }
            }
          }
        }
      }
      userErrors { field message code }
    }
  }
`;

const locationsQuery = `#graphql
  query InventoryReasonValidationLocations {
    locations(first: 1) {
      nodes { id name }
    }
  }
`;

const inventorySetMutation = `#graphql
  mutation InventoryReasonValidationSet($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
    inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        reason
        changes { name delta item { id } location { id name } }
      }
      userErrors { field message code }
    }
  }
`;

const inventoryAdjustMutation = `#graphql
  mutation InventoryReasonValidationAdjust($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
    inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        reason
        changes { name delta item { id } location { id name } }
      }
      userErrors { field message code }
    }
  }
`;

const inventoryMoveMutation = `#graphql
  mutation InventoryReasonValidationMove($input: InventoryMoveQuantitiesInput!, $idempotencyKey: String!) {
    inventoryMoveQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        reason
        changes { name delta item { id } location { id name } }
      }
      userErrors { field message code }
    }
  }
`;

const inventorySetOnHandMutation = `#graphql
  mutation InventoryReasonValidationSetOnHand($input: InventorySetOnHandQuantitiesInput!, $idempotencyKey: String!) {
    inventorySetOnHandQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        reason
        changes { name delta item { id } location { id name } }
      }
      userErrors { field message code }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query InventoryReasonValidationDownstream($inventoryItemId: ID!) {
    inventoryItem(id: $inventoryItemId) {
      id
      inventoryLevels(first: 1) {
        nodes {
          location { id }
          quantities(names: ["available", "on_hand", "damaged"]) { name quantity }
        }
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation InventoryReasonValidationCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

function readSetup(payload: CapturePayload): ProductSetup {
  const productSet = (
    payload as {
      data?: {
        productSet?: {
          product?: {
            id?: unknown;
            variants?: {
              nodes?: Array<{
                id?: unknown;
                inventoryItem?: {
                  id?: unknown;
                  inventoryLevels?: {
                    nodes?: Array<{
                      location?: { id?: unknown };
                      quantities?: Array<{ name?: unknown; quantity?: unknown }>;
                    }>;
                  };
                };
              }>;
            };
          };
        };
      };
    }
  ).data?.productSet;
  const product = productSet?.product;
  const variant = product?.variants?.nodes?.[0];
  const level = variant?.inventoryItem?.inventoryLevels?.nodes?.[0];
  if (
    typeof product?.id !== 'string' ||
    typeof variant?.id !== 'string' ||
    typeof variant.inventoryItem?.id !== 'string' ||
    typeof level?.location?.id !== 'string'
  ) {
    throw new Error(`Inventory reason validation setup failed: ${JSON.stringify(productSet)}`);
  }

  const quantity = (name: string): number => {
    const found = level.quantities?.find((entry) => entry.name === name)?.quantity;
    return typeof found === 'number' ? found : 0;
  };

  return {
    productId: product.id,
    variantId: variant.id,
    inventoryItemId: variant.inventoryItem.id,
    locationId: level.location.id,
    available: quantity('available'),
    onHand: quantity('on_hand'),
    setup: payload,
  };
}

function readLocationId(payload: CapturePayload): string {
  const locationId = (
    payload as {
      data?: { locations?: { nodes?: Array<{ id?: unknown }> } };
    }
  ).data?.locations?.nodes?.[0]?.id;
  if (typeof locationId !== 'string') {
    throw new Error(`Inventory reason validation location lookup failed: ${JSON.stringify(payload)}`);
  }

  return locationId;
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

function setVariables(setup: ProductSetup, runId: string, reason: string, suffix: string) {
  return {
    input: {
      name: 'available',
      reason,
      referenceDocumentUri: `logistics://inventory-reason-validation/${suffix}/${runId}`,
      quantities: [
        {
          inventoryItemId: setup.inventoryItemId,
          locationId: setup.locationId,
          quantity: setup.available,
          changeFromQuantity: setup.available,
        },
      ],
    },
    idempotencyKey: `inventory-reason-validation-${suffix}-${runId}`,
  };
}

function adjustVariables(setup: ProductSetup, runId: string, reason: string, suffix: string) {
  return {
    input: {
      name: 'available',
      reason,
      referenceDocumentUri: `logistics://inventory-reason-validation/${suffix}/${runId}`,
      changes: [
        {
          inventoryItemId: setup.inventoryItemId,
          locationId: setup.locationId,
          delta: 1,
          changeFromQuantity: setup.available,
        },
      ],
    },
    idempotencyKey: `inventory-reason-validation-${suffix}-${runId}`,
  };
}

function moveVariables(setup: ProductSetup, runId: string, reason: string, suffix: string) {
  return {
    input: {
      reason,
      referenceDocumentUri: `logistics://inventory-reason-validation/${suffix}/${runId}`,
      changes: [
        {
          inventoryItemId: setup.inventoryItemId,
          quantity: 1,
          from: {
            locationId: setup.locationId,
            name: 'available',
            changeFromQuantity: setup.available,
          },
          to: {
            locationId: setup.locationId,
            name: 'damaged',
            changeFromQuantity: 0,
            ledgerDocumentUri: `logistics://inventory-reason-validation/${suffix}/${runId}`,
          },
        },
      ],
    },
    idempotencyKey: `inventory-reason-validation-${suffix}-${runId}`,
  };
}

function setOnHandVariables(setup: ProductSetup, runId: string, reason: string, suffix: string) {
  return {
    input: {
      reason,
      referenceDocumentUri: `logistics://inventory-reason-validation/${suffix}/${runId}`,
      setQuantities: [
        {
          inventoryItemId: setup.inventoryItemId,
          locationId: setup.locationId,
          quantity: setup.onHand,
          changeFromQuantity: setup.onHand,
        },
      ],
    },
    idempotencyKey: `inventory-reason-validation-${suffix}-${runId}`,
  };
}

async function createSetup(runId: string): Promise<ProductSetup> {
  const locationId = readLocationId((await runGraphql(locationsQuery)) as CapturePayload);
  const setup = (await runGraphql(productSetMutation, {
    input: {
      title: `Inventory reason validation ${runId}`,
      status: 'DRAFT',
      productOptions: [
        {
          name: 'Title',
          position: 1,
          values: [{ name: 'Default Title' }],
        },
      ],
      variants: [
        {
          optionValues: [{ optionName: 'Title', name: 'Default Title' }],
          inventoryItem: {
            tracked: true,
            requiresShipping: true,
          },
          inventoryQuantities: [
            {
              locationId,
              name: 'available',
              quantity: 1,
            },
          ],
        },
      ],
    },
    synchronous: true,
  })) as CapturePayload;

  return readSetup(setup);
}

let productId: string | null = null;
const runId = `${Date.now()}`;

try {
  await mkdir(outputDir, { recursive: true });
  const setup = await createSetup(runId);
  productId = setup.productId;

  const acceptedReasonCases: Record<string, CapturePayload> = {};
  for (const reason of candidateReasons) {
    const variables = setVariables(setup, runId, reason, `accepted-${reason}`);
    const response = await runGraphqlAllowGraphqlErrors(inventorySetMutation, variables);
    acceptedReasonCases[reason] = {
      query: inventorySetMutation,
      variables,
      response,
    };
  }

  const unknownReasonSetVariables = setVariables(setup, runId, 'completely_made_up', 'unknown-set');
  const unknownReasonSet = {
    query: inventorySetMutation,
    variables: unknownReasonSetVariables,
    response: await runGraphqlAllowGraphqlErrors(inventorySetMutation, unknownReasonSetVariables),
  };
  const emptyReasonSetVariables = setVariables(setup, runId, '', 'empty-set');
  const emptyReasonSet = {
    query: inventorySetMutation,
    variables: emptyReasonSetVariables,
    response: await runGraphqlAllowGraphqlErrors(inventorySetMutation, emptyReasonSetVariables),
  };
  const unknownReasonAdjustVariables = adjustVariables(setup, runId, 'completely_made_up', 'unknown-adjust');
  const unknownReasonAdjust = {
    query: inventoryAdjustMutation,
    variables: unknownReasonAdjustVariables,
    response: await runGraphqlAllowGraphqlErrors(inventoryAdjustMutation, unknownReasonAdjustVariables),
  };
  const unknownReasonMoveVariables = moveVariables(setup, runId, 'completely_made_up', 'unknown-move');
  const unknownReasonMove = {
    query: inventoryMoveMutation,
    variables: unknownReasonMoveVariables,
    response: await runGraphqlAllowGraphqlErrors(inventoryMoveMutation, unknownReasonMoveVariables),
  };
  const unknownReasonSetOnHandVariables = setOnHandVariables(setup, runId, 'completely_made_up', 'unknown-set-on-hand');
  const unknownReasonSetOnHand = {
    query: inventorySetOnHandMutation,
    variables: unknownReasonSetOnHandVariables,
    response: await runGraphqlAllowGraphqlErrors(inventorySetOnHandMutation, unknownReasonSetOnHandVariables),
  };
  const downstreamAfterRejected = await runGraphqlAllowGraphqlErrors(downstreamReadQuery, {
    inventoryItemId: setup.inventoryItemId,
  });
  const cleanup = await deleteProduct(setup.productId);
  productId = null;

  const capturePayload = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    candidateReasons,
    setup,
    acceptedReasonCases,
    invalidReasonCases: {
      unknownReasonSet,
      emptyReasonSet,
      unknownReasonAdjust,
      unknownReasonMove,
      unknownReasonSetOnHand,
    },
    downstreamAfterRejected,
    cleanup,
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
        acceptedReasons: candidateReasons,
        invalidCases: Object.keys(capturePayload.invalidReasonCases),
      },
      null,
      2,
    ),
  );
} finally {
  await deleteProduct(productId);
}
