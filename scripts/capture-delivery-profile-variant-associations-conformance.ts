/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, 'delivery-profile-variant-associations.json');
const requestDir = path.join('config', 'parity-requests', 'shipping-fulfillments');
const missingVariantId = 'gid://shopify/ProductVariant/999999999999';

const deliveryProfileVariantHydrateQuery =
  'query ShippingDeliveryProfileVariantsHydrate($ids: [ID!]!) { nodes(ids: $ids) { ... on ProductVariant { id title product { id title handle } } } }';

const deliveryProfileRemoveMutation = `#graphql
  mutation DeliveryProfileVariantAssociationCleanupProfile($id: ID!) {
    deliveryProfileRemove(id: $id) {
      job {
        id
        done
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation DeliveryProfileVariantAssociationCleanupProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  query: string;
  variables: JsonRecord;
  result: ConformanceGraphqlResult;
};

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  return {
    query: trimGraphql(query),
    variables,
    result: await runGraphqlRequest(query, variables),
  };
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    if (Array.isArray(current) && /^\d+$/u.test(part)) {
      current = current[Number(part)];
      continue;
    }
    current = readObject(current)?.[part];
  }
  return current;
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string') {
    throw new Error(`${label} expected string, got ${JSON.stringify(value)}`);
  }
  return value;
}

function optionalProfileId(
  captureResult: GraphqlCapture,
  root: 'deliveryProfileCreate' | 'deliveryProfileUpdate',
): string | null {
  const id = readPath(captureResult.result.payload, ['data', root, 'profile', 'id']);
  return typeof id === 'string' ? id : null;
}

async function cleanupProfile(id: string): Promise<GraphqlCapture> {
  return capture(deliveryProfileRemoveMutation, { id });
}

async function cleanupProduct(id: string): Promise<GraphqlCapture> {
  return capture(productDeleteMutation, { input: { id } });
}

await mkdir(outputDir, { recursive: true });

const productCreateMutation = await readRequest('delivery-profile-variant-association-product-create.graphql');
const deliveryProfileCreateMutation = await readRequest('delivery-profile-variant-association-create.graphql');
const deliveryProfileUpdateMutation = await readRequest('delivery-profile-variant-association-update.graphql');
const deliveryProfileReadQuery = await readRequest('delivery-profile-variant-association-read.graphql');

const runStamp = Date.now();
let productId: string | null = null;
const profileIdsForCleanup: string[] = [];
const cleanup: Record<string, GraphqlCapture | null> = {
  removeValidProfile: null,
  removeMissingProfile: null,
  deleteProduct: null,
};

try {
  const productCreate = await capture(productCreateMutation, {
    product: {
      title: `Delivery profile variant association ${runStamp}`,
    },
  });
  productId = requireString(
    readPath(productCreate.result.payload, ['data', 'productCreate', 'product', 'id']),
    'created product id',
  );
  const productTitle = requireString(
    readPath(productCreate.result.payload, ['data', 'productCreate', 'product', 'title']),
    'created product title',
  );
  const variantId = requireString(
    readPath(productCreate.result.payload, ['data', 'productCreate', 'product', 'variants', 'nodes', '0', 'id']),
    'created variant id',
  );
  const variantTitle = requireString(
    readPath(productCreate.result.payload, ['data', 'productCreate', 'product', 'variants', 'nodes', '0', 'title']),
    'created variant title',
  );

  const validCreate = await capture(deliveryProfileCreateMutation, {
    profile: {
      name: `Variant association valid ${runStamp}`,
      variantsToAssociate: [variantId],
    },
  });
  const validProfileId = requireString(optionalProfileId(validCreate, 'deliveryProfileCreate'), 'valid profile id');
  profileIdsForCleanup.push(validProfileId);

  const missingVariantHydrateForCreate = await capture(deliveryProfileVariantHydrateQuery, {
    ids: [missingVariantId],
  });
  const missingCreate = await capture(deliveryProfileCreateMutation, {
    profile: {
      name: `Variant association missing ${runStamp}`,
      variantsToAssociate: [missingVariantId],
    },
  });
  const missingProfileId = optionalProfileId(missingCreate, 'deliveryProfileCreate');
  if (missingProfileId !== null) {
    profileIdsForCleanup.push(missingProfileId);
  }

  const wrongTypeCreate = await capture(deliveryProfileCreateMutation, {
    profile: {
      name: `Variant association wrong type ${runStamp}`,
      variantsToAssociate: [productId],
    },
  });

  const missingVariantHydrateForUpdate = await capture(deliveryProfileVariantHydrateQuery, {
    ids: [missingVariantId],
  });
  const missingUpdate = await capture(deliveryProfileUpdateMutation, {
    id: validProfileId,
    profile: {
      name: `Variant association missing update ${runStamp}`,
      variantsToAssociate: [missingVariantId],
    },
  });
  const readAfterMissingUpdate = await capture(deliveryProfileReadQuery, { id: validProfileId });

  const wrongTypeUpdate = await capture(deliveryProfileUpdateMutation, {
    id: validProfileId,
    profile: {
      name: `Variant association wrong type update ${runStamp}`,
      variantsToAssociate: [productId],
    },
  });
  const readAfterWrongTypeUpdate = await capture(deliveryProfileReadQuery, { id: validProfileId });

  const uniqueProfileIds = Array.from(new Set(profileIdsForCleanup));
  for (const [index, profileId] of uniqueProfileIds.entries()) {
    cleanup[index === 0 ? 'removeValidProfile' : 'removeMissingProfile'] = await cleanupProfile(profileId);
  }
  cleanup['deleteProduct'] = await cleanupProduct(productId);

  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        evidence: {
          productId,
          productTitle,
          variantId,
          variantTitle,
          validProfileId,
          missingProfileId,
          missingVariantId,
        },
        setup: {
          productCreate,
        },
        mutations: {
          validCreate,
          missingCreate,
          wrongTypeCreate,
          missingUpdate,
          readAfterMissingUpdate,
          wrongTypeUpdate,
          readAfterWrongTypeUpdate,
        },
        cleanup,
        notes: [
          'Captured with home-folder conformance auth against a disposable Shopify test store.',
          'A real productCreate provides the valid ProductVariant target; nonexistent ProductVariant association is captured as an empty profileItems/count-0 branch.',
          'The two upstreamCalls entries are real Shopify nodes(ids:) probes for the missing ProductVariant ID, recorded for proxy live-hybrid hydrate replay.',
        ],
        upstreamCalls: [
          {
            operationName: 'ShippingDeliveryProfileVariantsHydrate',
            variables: missingVariantHydrateForCreate.variables,
            query: missingVariantHydrateForCreate.query,
            response: {
              status: missingVariantHydrateForCreate.result.status,
              body: missingVariantHydrateForCreate.result.payload,
            },
          },
          {
            operationName: 'ShippingDeliveryProfileVariantsHydrate',
            variables: missingVariantHydrateForUpdate.variables,
            query: missingVariantHydrateForUpdate.query,
            response: {
              status: missingVariantHydrateForUpdate.result.status,
              body: missingVariantHydrateForUpdate.result.payload,
            },
          },
        ],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(
    JSON.stringify({ ok: true, outputPath, productId, variantId, validProfileId, missingProfileId }, null, 2),
  );
} catch (error) {
  for (const profileId of Array.from(new Set(profileIdsForCleanup))) {
    try {
      await cleanupProfile(profileId);
    } catch {
      // Best-effort cleanup after a failed capture; preserve the original error.
    }
  }
  if (productId !== null) {
    try {
      await cleanupProduct(productId);
    } catch {
      // Best-effort cleanup after a failed capture; preserve the original error.
    }
  }
  throw error;
}
