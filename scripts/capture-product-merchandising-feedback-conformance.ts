/* oxlint-disable no-console -- CLI capture scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonObject = Record<string, unknown>;

type Probe = {
  name: string;
  operationNames: string[];
  query: string;
  variables: JsonObject;
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const merchandisingOutputPath = path.join(outputDir, 'product-merchandising-mutation-probes.json');
const feedbackAccessOutputPath = path.join(outputDir, 'product-feedback-mutation-access-blockers.json');

async function readRequest(name: string): Promise<string> {
  return readFile(path.join('config', 'parity-requests', 'products', name), 'utf8');
}

async function capture(name: string, operationNames: string[], query: string, variables: JsonObject): Promise<Probe> {
  const result: ConformanceGraphqlResult = await runGraphqlRaw(query, variables);
  return {
    name,
    operationNames,
    query,
    variables,
    status: result.status,
    response: result.payload,
  };
}

const productFeedCreate = await readRequest('product-merchandising-product-feed-create.graphql');
const productFeedDelete = await readRequest('product-merchandising-product-feed-delete.graphql');
const productFullSync = await readRequest('product-merchandising-product-full-sync.graphql');
const productBundleCreate = await readRequest('product-merchandising-product-bundle-create.graphql');
const productBundleUpdate = await readRequest('product-merchandising-product-bundle-update.graphql');
const combinedListingUpdate = await readRequest('product-merchandising-combined-listing-update.graphql');
const variantRelationshipBulkUpdate = await readRequest(
  'product-merchandising-variant-relationship-bulk-update.graphql',
);
const productFeedbackInvalidState = await readRequest('product-feedback-invalid-state.graphql');
const shopFeedbackInvalidState = await readRequest('shop-feedback-invalid-state.graphql');

const bulkProductFeedbackAccess = `#graphql
  mutation BulkProductResourceFeedbackAccessBlocker($feedbackInput: [ProductResourceFeedbackInput!]!) {
    bulkProductResourceFeedbackCreate(feedbackInput: $feedbackInput) {
      feedback {
        productId
        state
        messages
        feedbackGeneratedAt
        productUpdatedAt
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const shopFeedbackAccess = `#graphql
  mutation ShopResourceFeedbackAccessBlocker($input: ResourceFeedbackCreateInput!) {
    shopResourceFeedbackCreate(input: $input) {
      feedback {
        state
        messages {
          message
        }
        feedbackGeneratedAt
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const merchandisingProbes: Probe[] = [
  await capture('productFeedCreate-channel-or-webhook-blocker', ['productFeedCreate'], productFeedCreate, {
    input: {
      country: 'US',
      language: 'EN',
    },
  }),
  await capture('productFeedDelete-unknown-id', ['productFeedDelete'], productFeedDelete, {
    id: 'gid://shopify/ProductFeed/999999999',
  }),
  await capture('productFullSync-unknown-id', ['productFullSync'], productFullSync, {
    id: 'gid://shopify/ProductFeed/999999999',
  }),
  await capture('productBundleCreate-empty-components', ['productBundleCreate'], productBundleCreate, {
    input: {
      title: 'Product merchandising bundle probe',
      components: [],
    },
  }),
  await capture('productBundleUpdate-unknown-product', ['productBundleUpdate'], productBundleUpdate, {
    input: {
      productId: 'gid://shopify/Product/999999999',
      title: 'Product merchandising bundle update probe',
      components: [],
    },
  }),
  await capture('combinedListingUpdate-unknown-parent', ['combinedListingUpdate'], combinedListingUpdate, {
    parentProductId: 'gid://shopify/Product/999999999',
  }),
  await capture(
    'productVariantRelationshipBulkUpdate-unknown-variants',
    ['productVariantRelationshipBulkUpdate'],
    variantRelationshipBulkUpdate,
    {
      input: [
        {
          parentProductVariantId: 'gid://shopify/ProductVariant/999999999',
          productVariantRelationshipsToCreate: [
            {
              id: 'gid://shopify/ProductVariant/999999998',
              quantity: 1,
            },
          ],
        },
      ],
    },
  ),
  await capture(
    'bulkProductResourceFeedbackCreate-invalid-state',
    ['bulkProductResourceFeedbackCreate'],
    productFeedbackInvalidState,
    {},
  ),
  await capture(
    'shopResourceFeedbackCreate-invalid-state',
    ['shopResourceFeedbackCreate'],
    shopFeedbackInvalidState,
    {},
  ),
];

const feedbackAccessProbes: Probe[] = [
  await capture(
    'bulkProductResourceFeedbackCreate-access-denied',
    ['bulkProductResourceFeedbackCreate'],
    bulkProductFeedbackAccess,
    {
      feedbackInput: [
        {
          productId: 'gid://shopify/Product/999999999',
          state: 'REQUIRES_ACTION',
          feedbackGeneratedAt: '2024-01-01T00:00:00Z',
          productUpdatedAt: '2024-01-01T00:00:00Z',
          messages: ['missing'],
        },
      ],
    },
  ),
  await capture('shopResourceFeedbackCreate-access-denied', ['shopResourceFeedbackCreate'], shopFeedbackAccess, {
    input: {
      state: 'ACCEPTED',
      feedbackGeneratedAt: '2024-01-01T00:00:00Z',
      messages: ['ready'],
    },
  }),
];

await mkdir(outputDir, { recursive: true });
await writeFile(
  merchandisingOutputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'Live product merchandising and feedback validation probes recorded by the dedicated product merchandising/feedback conformance script.',
        'The configured store currently rejects productFeedCreate with a payload userError because the channel/spec manages feeds automatically and requires feed webhooks; local productFeedCreate success remains runtime-test-backed.',
        'Feedback success remains blocked by missing write_resource_feedbacks plus Storefront API / Sales Channel configuration, so this fixture captures schema-level invalid-state parity for the feedback mutation roots.',
      ],
      schemaEvidence: {
        ProductFeed: {
          fields: ['id', 'country', 'language', 'status'],
          statusValues: ['ACTIVE', 'INACTIVE'],
        },
        ProductFeedCreatePayload: {
          fields: ['productFeed', 'userErrors'],
        },
        ProductFeedDeletePayload: {
          fields: ['deletedId', 'userErrors'],
        },
        ProductFullSyncPayload: {
          fields: ['id', 'userErrors'],
        },
        ProductBundleOperation: {
          fields: ['id', 'product', 'status', 'userErrors'],
          statusValues: ['CREATED', 'ACTIVE', 'COMPLETE'],
        },
        ProductBundleCreatePayload: {
          fields: ['productBundleOperation', 'userErrors'],
        },
        ProductBundleUpdatePayload: {
          fields: ['productBundleOperation', 'userErrors'],
        },
        CombinedListingUpdatePayload: {
          fields: ['product', 'userErrors'],
        },
      },
      probes: merchandisingProbes,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);
await writeFile(
  feedbackAccessOutputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'Live blocker evidence for product and shop feedback mutation success capture.',
        'The configured app token can probe Shopify but lacks write_resource_feedbacks and Storefront API / Sales Channel configuration required by both feedback mutations.',
      ],
      probes: feedbackAccessProbes,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      outputs: [merchandisingOutputPath, feedbackAccessOutputPath],
      probeCount: merchandisingProbes.length + feedbackAccessProbes.length,
      storeDomain,
      apiVersion,
    },
    null,
    2,
  ),
);
