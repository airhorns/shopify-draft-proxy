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
const outputPath = path.join(outputDir, 'inventory-adjust-ledger-document-validation.json');

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

const locationsQuery = `#graphql
  query InventoryAdjustLedgerDocumentLocations {
    locations(first: 1) {
      nodes { id name }
    }
  }
`;

const createMutation = `#graphql
  mutation InventoryAdjustLedgerDocumentCreate($product: ProductCreateInput!) {
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
  mutation InventoryAdjustLedgerDocumentTrack($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
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

const inventoryItemLevelQuery = `#graphql
  query InventoryAdjustLedgerDocumentInventoryItemLevel($inventoryItemId: ID!) {
    inventoryItem(id: $inventoryItemId) {
      id
      inventoryLevels(first: 5) {
        nodes {
          location { id name }
          quantities(names: ["available", "damaged"]) { name quantity }
        }
      }
    }
  }
`;

const inventoryAdjustMutation = `#graphql
  mutation InventoryAdjustLedgerDocumentValidation($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
    inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        id
        createdAt
        reason
        changes {
          name
          delta
          quantityAfterChange
          ledgerDocumentUri
          item { id }
          location { id }
        }
      }
      userErrors { field message code }
    }
  }
`;

const deleteMutation = `#graphql
  mutation InventoryAdjustLedgerDocumentDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

type LocationRef = {
  id: string;
  name?: string | null;
};

type TrackedProduct = {
  productId: string;
  variantId: string;
  inventoryItemId: string;
  location: LocationRef;
  create: unknown;
  track: unknown;
  inventoryItemLevel: unknown;
};

type ExpectedUserError = {
  field: string[];
  message: string;
  code: string;
};

function asRecord(value: unknown, label: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${label} was not an object.`);
  }

  return value as Record<string, unknown>;
}

function asOptionalRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} was not a non-empty string.`);
  }

  return value;
}

function requireArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${label} was not an array.`);
  }

  return value;
}

function extractFirstLocation(payload: unknown): LocationRef {
  const root = asRecord(payload, 'locations response');
  const data = asRecord(root['data'], 'locations data');
  const locations = asRecord(data['locations'], 'locations connection');
  const nodes = requireArray(locations['nodes'], 'locations nodes');
  const first = asRecord(nodes[0], 'first location');

  return {
    id: requireString(first['id'], 'first location id'),
    name: typeof first['name'] === 'string' ? first['name'] : null,
  };
}

function extractCreatedProduct(
  payload: unknown,
): Omit<TrackedProduct, 'location' | 'create' | 'track' | 'inventoryItemLevel'> {
  const root = asRecord(payload, 'productCreate response');
  const data = asRecord(root['data'], 'productCreate data');
  const productCreate = asRecord(data['productCreate'], 'productCreate payload');
  const userErrors = requireArray(productCreate['userErrors'], 'productCreate userErrors');
  if (userErrors.length !== 0) {
    throw new Error(`productCreate returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
  const product = asRecord(productCreate['product'], 'created product');
  const variants = asRecord(product['variants'], 'created product variants');
  const nodes = requireArray(variants['nodes'], 'created product variant nodes');
  const variant = asRecord(nodes[0], 'created product default variant');
  const inventoryItem = asRecord(variant['inventoryItem'], 'created variant inventory item');

  return {
    productId: requireString(product['id'], 'created product id'),
    variantId: requireString(variant['id'], 'created variant id'),
    inventoryItemId: requireString(inventoryItem['id'], 'created inventory item id'),
  };
}

function extractInventoryItemLocation(payload: unknown, fallback: LocationRef): LocationRef {
  const root = asRecord(payload, 'inventory item level response');
  const data = asRecord(root['data'], 'inventory item level data');
  const inventoryItem = asOptionalRecord(data['inventoryItem']);
  const inventoryLevels = asOptionalRecord(inventoryItem?.['inventoryLevels']);
  const nodes = Array.isArray(inventoryLevels?.['nodes']) ? inventoryLevels['nodes'] : [];
  const firstNode = asOptionalRecord(nodes[0]);
  const location = asOptionalRecord(firstNode?.['location']);

  if (!location) {
    return fallback;
  }

  return {
    id: requireString(location['id'], 'inventory item level location id'),
    name: typeof location['name'] === 'string' ? location['name'] : null,
  };
}

function inventoryAdjustPayload(payload: unknown): Record<string, unknown> {
  const root = asRecord(payload, 'inventoryAdjustQuantities response');
  const data = asRecord(root['data'], 'inventoryAdjustQuantities data');

  return asRecord(data['inventoryAdjustQuantities'], 'inventoryAdjustQuantities payload');
}

function assertUserError(payload: unknown, label: string, expected: ExpectedUserError): void {
  const root = inventoryAdjustPayload(payload);
  if (root['inventoryAdjustmentGroup'] !== null) {
    throw new Error(`${label} returned an adjustment group: ${JSON.stringify(root, null, 2)}`);
  }

  const userErrors = requireArray(root['userErrors'], `${label} userErrors`);
  if (userErrors.length !== 1) {
    throw new Error(`${label} expected one userError: ${JSON.stringify(userErrors, null, 2)}`);
  }

  const userError = asRecord(userErrors[0], `${label} userError`);
  const field = requireArray(userError['field'], `${label} userError field`).map((segment) =>
    requireString(segment, `${label} userError field segment`),
  );
  if (
    JSON.stringify(field) !== JSON.stringify(expected.field) ||
    userError['message'] !== expected.message ||
    userError['code'] !== expected.code
  ) {
    throw new Error(
      `${label} userError mismatch: ${JSON.stringify({ field, message: userError['message'], code: userError['code'] }, null, 2)}`,
    );
  }
}

function assertNoUserErrors(payload: Record<string, unknown>, label: string): void {
  const userErrors = requireArray(payload['userErrors'], `${label} userErrors`);
  if (userErrors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertValidNonAvailable(
  payload: unknown,
  expectedInventoryItemId: string,
  expectedLocationId: string,
  expectedLedgerDocumentUri: string,
): void {
  const root = inventoryAdjustPayload(payload);
  assertNoUserErrors(root, 'valid non-available ledger adjustment');
  const group = asRecord(root['inventoryAdjustmentGroup'], 'valid non-available inventory adjustment group');
  const changes = requireArray(group['changes'], 'valid non-available changes');
  if (changes.length !== 1) {
    throw new Error(`Expected one valid non-available change: ${JSON.stringify(changes, null, 2)}`);
  }

  const change = asRecord(changes[0], 'valid non-available change');
  const item = asRecord(change['item'], 'valid non-available item');
  const location = asRecord(change['location'], 'valid non-available location');
  if (
    group['reason'] !== 'received' ||
    change['name'] !== 'incoming' ||
    change['delta'] !== 5 ||
    change['quantityAfterChange'] !== null ||
    change['ledgerDocumentUri'] !== expectedLedgerDocumentUri ||
    item['id'] !== expectedInventoryItemId ||
    location['id'] !== expectedLocationId
  ) {
    throw new Error(`Unexpected valid non-available payload: ${JSON.stringify(root, null, 2)}`);
  }
}

async function deleteProduct(productId: string): Promise<unknown> {
  try {
    return await runGraphqlAllowGraphqlErrors(deleteMutation, { input: { id: productId } });
  } catch (error) {
    console.warn(`Cleanup failed for ${productId}: ${error instanceof Error ? error.message : String(error)}`);
    return null;
  }
}

async function cleanupProducts(productIds: Set<string>): Promise<unknown[]> {
  const responses: unknown[] = [];
  for (const productId of Array.from(productIds)) {
    responses.push(await deleteProduct(productId));
    productIds.delete(productId);
  }

  return responses;
}

async function createTrackedProduct(
  label: string,
  fallbackLocation: LocationRef,
  productIds: Set<string>,
): Promise<TrackedProduct> {
  const create = await runGraphql(createMutation, {
    product: {
      title: label,
      status: 'DRAFT',
    },
  });
  const product = extractCreatedProduct(create);
  productIds.add(product.productId);

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
  const inventoryItemLevel = await runGraphql(inventoryItemLevelQuery, {
    inventoryItemId: product.inventoryItemId,
  });

  return {
    ...product,
    location: extractInventoryItemLocation(inventoryItemLevel, fallbackLocation),
    create,
    track,
    inventoryItemLevel,
  };
}

await mkdir(outputDir, { recursive: true });

const productIds = new Set<string>();
const runId = `${Date.now()}`;

try {
  const fallbackLocation = extractFirstLocation(await runGraphql(locationsQuery));
  const primaryProduct = await createTrackedProduct(
    `Hermes Inventory Adjust Ledger Validation ${runId}`,
    fallbackLocation,
    productIds,
  );
  const secondaryProduct = await createTrackedProduct(
    `Hermes Inventory Adjust Ledger Validation Secondary ${runId}`,
    fallbackLocation,
    productIds,
  );

  const missingLedgerDocumentVariables = {
    input: {
      name: 'damaged',
      reason: 'damaged',
      changes: [
        {
          inventoryItemId: primaryProduct.inventoryItemId,
          locationId: primaryProduct.location.id,
          delta: 5,
          changeFromQuantity: 0,
        },
      ],
    },
    idempotencyKey: `inventory-adjust-ledger-required-${runId}`,
  };
  const missingLedgerDocument = await runGraphql(inventoryAdjustMutation, missingLedgerDocumentVariables);
  assertUserError(missingLedgerDocument, 'missing ledger document', {
    field: ['input', 'changes', '0', 'ledgerDocumentUri'],
    message: 'A ledger document URI is required except when adjusting available.',
    code: 'INVALID_QUANTITY_DOCUMENT',
  });

  const availableLedgerDocumentVariables = {
    input: {
      name: 'available',
      reason: 'correction',
      changes: [
        {
          inventoryItemId: primaryProduct.inventoryItemId,
          locationId: primaryProduct.location.id,
          delta: 5,
          changeFromQuantity: 0,
          ledgerDocumentUri: `https://example.com/inventory-ledger/available/${runId}`,
        },
      ],
    },
    idempotencyKey: `inventory-adjust-ledger-available-${runId}`,
  };
  const availableLedgerDocument = await runGraphql(inventoryAdjustMutation, availableLedgerDocumentVariables);
  assertUserError(availableLedgerDocument, 'available ledger document', {
    field: ['input', 'changes', '0', 'ledgerDocumentUri'],
    message: 'A ledger document URI is not allowed when adjusting available.',
    code: 'INVALID_AVAILABLE_DOCUMENT',
  });

  const internalLedgerDocumentVariables = {
    input: {
      name: 'reserved',
      reason: 'correction',
      changes: [
        {
          inventoryItemId: primaryProduct.inventoryItemId,
          locationId: primaryProduct.location.id,
          delta: 5,
          changeFromQuantity: 0,
          ledgerDocumentUri: 'gid://shopify/Order/123',
        },
      ],
    },
    idempotencyKey: `inventory-adjust-ledger-internal-${runId}`,
  };
  const internalLedgerDocument = await runGraphql(inventoryAdjustMutation, internalLedgerDocumentVariables);
  assertUserError(internalLedgerDocument, 'internal ledger document', {
    field: ['input', 'changes', '0', 'ledgerDocumentUri'],
    message: 'Internal (gid://shopify/) ledger documents are not allowed to be adjusted via API.',
    code: 'INTERNAL_LEDGER_DOCUMENT',
  });

  const maxOneLedgerDocumentVariables = {
    input: {
      name: 'damaged',
      reason: 'damaged',
      changes: [
        {
          inventoryItemId: primaryProduct.inventoryItemId,
          locationId: primaryProduct.location.id,
          delta: 5,
          changeFromQuantity: 0,
          ledgerDocumentUri: `https://example.com/inventory-ledger/first/${runId}`,
        },
        {
          inventoryItemId: secondaryProduct.inventoryItemId,
          locationId: secondaryProduct.location.id,
          delta: 6,
          changeFromQuantity: 0,
          ledgerDocumentUri: `https://example.com/inventory-ledger/second/${runId}`,
        },
      ],
    },
    idempotencyKey: `inventory-adjust-ledger-max-one-${runId}`,
  };
  const maxOneLedgerDocument = await runGraphql(inventoryAdjustMutation, maxOneLedgerDocumentVariables);
  assertUserError(maxOneLedgerDocument, 'max-one ledger document', {
    field: ['input', 'changes'],
    message:
      'All changes must have the same ledger document URI or, in the case of adjusting available, no ledger document URI.',
    code: 'MAX_ONE_LEDGER_DOCUMENT',
  });

  const validLedgerDocumentUri = `https://example.com/inventory-ledger/valid/${runId}`;
  const validNonAvailableLedgerDocumentVariables = {
    input: {
      name: 'incoming',
      reason: 'received',
      changes: [
        {
          inventoryItemId: primaryProduct.inventoryItemId,
          locationId: primaryProduct.location.id,
          delta: 5,
          changeFromQuantity: 0,
          ledgerDocumentUri: validLedgerDocumentUri,
        },
      ],
    },
    idempotencyKey: `inventory-adjust-ledger-valid-${runId}`,
  };
  const validNonAvailableLedgerDocument = await runGraphql(
    inventoryAdjustMutation,
    validNonAvailableLedgerDocumentVariables,
  );
  assertValidNonAvailable(
    validNonAvailableLedgerDocument,
    primaryProduct.inventoryItemId,
    primaryProduct.location.id,
    validLedgerDocumentUri,
  );

  const cleanup = await cleanupProducts(productIds);

  const capturePayload = {
    storeDomain,
    apiVersion,
    setup: {
      fallbackLocation,
      primaryProduct,
      secondaryProduct,
    },
    missingLedgerDocument: {
      variables: missingLedgerDocumentVariables,
      response: missingLedgerDocument,
    },
    availableLedgerDocument: {
      variables: availableLedgerDocumentVariables,
      response: availableLedgerDocument,
    },
    internalLedgerDocument: {
      variables: internalLedgerDocumentVariables,
      response: internalLedgerDocument,
    },
    maxOneLedgerDocument: {
      variables: maxOneLedgerDocumentVariables,
      response: maxOneLedgerDocument,
    },
    validNonAvailableLedgerDocument: {
      variables: validNonAvailableLedgerDocumentVariables,
      response: validNonAvailableLedgerDocument,
    },
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
        primaryInventoryItemId: primaryProduct.inventoryItemId,
        secondaryInventoryItemId: secondaryProduct.inventoryItemId,
      },
      null,
      2,
    ),
  );
} finally {
  await cleanupProducts(productIds);
}
