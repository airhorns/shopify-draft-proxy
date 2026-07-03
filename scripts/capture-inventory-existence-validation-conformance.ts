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
const outputPath = path.join(outputDir, 'inventory-existence-validation.json');
const unknownInventoryItemId = 'gid://shopify/InventoryItem/42';
const unknownLocationId = 'gid://shopify/Location/999999999999';

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const inventoryAdjustUnknownItemMutation = `#graphql
  mutation InventoryExistenceAdjustUnknownItem(
    $input: InventoryAdjustQuantitiesInput!
    $idempotencyKey: String!
  ) {
    inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        id
        reason
        changes {
          name
          delta
          item { id }
          location { id name }
        }
      }
      userErrors { field message code }
    }
  }
`;

async function runGraphqlAllowGraphqlErrors(query: string, variables: Record<string, unknown> = {}): Promise<unknown> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(JSON.stringify(result, null, 2));
  }

  return result.payload;
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const adjustUnknownInventoryItemVariables = {
  input: {
    name: 'available',
    reason: 'correction',
    referenceDocumentUri: `logistics://inventory-existence/adjust-unknown-item/${runId}`,
    changes: [
      {
        inventoryItemId: unknownInventoryItemId,
        locationId: unknownLocationId,
        delta: 1,
        changeFromQuantity: 0,
      },
    ],
  },
  idempotencyKey: `inventory-existence-adjust-unknown-item-${runId}`,
};

const adjustUnknownInventoryItem = await runGraphqlAllowGraphqlErrors(
  inventoryAdjustUnknownItemMutation,
  adjustUnknownInventoryItemVariables,
);

const capturePayload = {
  storeDomain,
  apiVersion,
  summary:
    'Inventory existence validation for a non-sentinel well-formed but unbacked InventoryItem GID in inventoryAdjustQuantities.',
  unknownInventoryItemId,
  unknownLocationId,
  adjustUnknownInventoryItem: {
    query: inventoryAdjustUnknownItemMutation,
    variables: adjustUnknownInventoryItemVariables,
    response: adjustUnknownInventoryItem,
  },
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
      unknownInventoryItemId,
      unknownLocationId,
    },
    null,
    2,
  ),
);
