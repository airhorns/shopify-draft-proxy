import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const fixturePath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'draft-order-residual-helper-roots.json',
);

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function absolutePath(relativePath: string): string {
  return path.join(repoRoot, relativePath);
}

async function writeJson(relativePath: string, value: unknown): Promise<void> {
  await mkdir(path.dirname(absolutePath(relativePath)), { recursive: true });
  await writeFile(absolutePath(relativePath), `${JSON.stringify(value, null, 2)}\n`);
}

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readRecord(value: unknown, key: string): JsonRecord | null {
  return asRecord(asRecord(value)?.[key]);
}

function readString(value: unknown, key: string): string | null {
  const fieldValue = asRecord(value)?.[key];
  return typeof fieldValue === 'string' && fieldValue.length > 0 ? fieldValue : null;
}

const stamp = Date.now();
const createVariables = {
  input: {
    email: `har-318-live-${stamp}@example.com`,
    note: 'HAR-318 residual helper capture',
    tags: ['har-318-base'],
    lineItems: [
      {
        title: 'HAR-318 custom line',
        quantity: 1,
        originalUnitPrice: '3.50',
        requiresShipping: false,
        taxable: false,
      },
    ],
  },
};

const draftOrderCreateDocument = `#graphql
  mutation CreateDraftOrder($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        name
        status
        ready
        email
        tags
        createdAt
        updatedAt
        subtotalPriceSet { shopMoney { amount currencyCode } }
        totalDiscountsSet { shopMoney { amount currencyCode } }
        totalShippingPriceSet { shopMoney { amount currencyCode } }
        totalPriceSet { shopMoney { amount currencyCode } }
        lineItems(first: 5) {
          nodes {
            id
            title
            name
            quantity
            custom
            requiresShipping
            taxable
            originalUnitPriceSet { shopMoney { amount currencyCode } }
            originalTotalSet { shopMoney { amount currencyCode } }
            discountedTotalSet { shopMoney { amount currencyCode } }
            totalDiscountSet { shopMoney { amount currencyCode } }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const draftOrderReadDocument = `#graphql
  query ReadDraftOrder($id: ID!) {
    draftOrder(id: $id) {
      id
      tags
    }
  }
`;

const calculateDocument = `#graphql
  mutation CalculateDraftOrder($input: DraftOrderInput!) {
    draftOrderCalculate(input: $input) {
      calculatedDraftOrder {
        currencyCode
        totalQuantityOfLineItems
        subtotalPriceSet { shopMoney { amount currencyCode } }
        totalDiscountsSet { shopMoney { amount currencyCode } }
        totalShippingPriceSet { shopMoney { amount currencyCode } }
        totalPriceSet { shopMoney { amount currencyCode } }
        lineItems {
          title
          quantity
          custom
          originalUnitPriceSet { shopMoney { amount currencyCode } }
          originalTotalSet { shopMoney { amount currencyCode } }
          discountedTotalSet { shopMoney { amount currencyCode } }
          totalDiscountSet { shopMoney { amount currencyCode } }
        }
        availableShippingRates { handle title price { amount currencyCode } }
      }
      userErrors { field message }
    }
  }
`;

const savedSearchesDocument = `#graphql
  query DraftOrderSavedSearches {
    draftOrderSavedSearches(first: 20) {
      nodes {
        id
        legacyResourceId
        name
        query
        resourceType
        searchTerms
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
`;

const availableDeliveryOptionsDocument = `#graphql
  query DraftOrderAvailableDeliveryOptions($input: DraftOrderAvailableDeliveryOptionsInput!) {
    draftOrderAvailableDeliveryOptions(input: $input) {
      availableShippingRates { handle title code source price { amount currencyCode } }
      availableLocalDeliveryRates { handle title code source price { amount currencyCode } }
      availableLocalPickupOptions { handle title source code instructions locationId }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
`;

const invoicePreviewDocument = `#graphql
  mutation DraftOrderInvoicePreview($id: ID!, $email: EmailInput) {
    draftOrderInvoicePreview(id: $id, email: $email) {
      previewSubject
      previewHtml
      userErrors { field message }
    }
  }
`;

const bulkAddDocument = `#graphql
  mutation DraftOrderBulkAddTags($ids: [ID!], $tags: [String!]!) {
    draftOrderBulkAddTags(ids: $ids, tags: $tags) {
      job { id done }
      userErrors { field message }
    }
  }
`;

const bulkRemoveDocument = `#graphql
  mutation DraftOrderBulkRemoveTags($ids: [ID!], $tags: [String!]!) {
    draftOrderBulkRemoveTags(ids: $ids, tags: $tags) {
      job { id done }
      userErrors { field message }
    }
  }
`;

const draftOrderTagDocument = `#graphql
  query DraftOrderTag($id: ID!) {
    draftOrderTag(id: $id) {
      id
      handle
      title
    }
  }
`;

const bulkDeleteDocument = `#graphql
  mutation DraftOrderBulkDelete($ids: [ID!]) {
    draftOrderBulkDelete(ids: $ids) {
      job { id done }
      userErrors { field message }
    }
  }
`;

const createResponse = await runGraphql<JsonRecord>(draftOrderCreateDocument, createVariables);
const draftOrderId = readString(readRecord(readRecord(createResponse.data, 'draftOrderCreate'), 'draftOrder'), 'id');

if (!draftOrderId) {
  throw new Error(`Expected draftOrderCreate.draftOrder.id: ${JSON.stringify(createResponse, null, 2)}`);
}

const calculateResponse = await runGraphql<JsonRecord>(calculateDocument, createVariables);
const savedSearchesResponse = await runGraphql<JsonRecord>(savedSearchesDocument, {});
const availableDeliveryOptionsVariables = {
  input: {
    lineItems: createVariables.input.lineItems,
  },
};
const availableDeliveryOptionsResponse = await runGraphql<JsonRecord>(
  availableDeliveryOptionsDocument,
  availableDeliveryOptionsVariables,
);
const invoicePreviewVariables = {
  id: draftOrderId,
  email: {
    subject: 'HAR-318 custom subject',
    customMessage: 'Custom note',
  },
};
const invoicePreviewResponse = await runGraphql<JsonRecord>(invoicePreviewDocument, invoicePreviewVariables);
const bulkAddVariables = { ids: [draftOrderId], tags: ['har-318-added'] };
const bulkAddResponse = await runGraphql<JsonRecord>(bulkAddDocument, bulkAddVariables);
const afterBulkAddRead = await runGraphql<JsonRecord>(draftOrderReadDocument, { id: draftOrderId });
const bulkRemoveVariables = { ids: [draftOrderId], tags: ['har-318-base'] };
const bulkRemoveResponse = await runGraphql<JsonRecord>(bulkRemoveDocument, bulkRemoveVariables);
const afterBulkRemoveRead = await runGraphql<JsonRecord>(draftOrderReadDocument, { id: draftOrderId });
const draftOrderTagCandidates = [
  'har-318-added',
  'gid://shopify/DraftOrderTag/har-318-added',
  'gid://shopify/DraftOrderTag/har%2D318%2Dadded',
];
const draftOrderTagAttempts = [];
for (const candidate of draftOrderTagCandidates) {
  try {
    draftOrderTagAttempts.push({
      variables: { id: candidate },
      response: await runGraphql<JsonRecord>(draftOrderTagDocument, { id: candidate }),
    });
  } catch (error) {
    draftOrderTagAttempts.push({
      variables: { id: candidate },
      error: String(error),
    });
  }
}
const bulkDeleteVariables = { ids: [draftOrderId] };
const bulkDeleteResponse = await runGraphql<JsonRecord>(bulkDeleteDocument, bulkDeleteVariables);
const afterBulkDeleteRead = await runGraphql<JsonRecord>(draftOrderReadDocument, { id: draftOrderId });

await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  setup: {
    draftOrderCreate: {
      document: draftOrderCreateDocument,
      variables: createVariables,
      response: createResponse,
    },
  },
  draftOrderCalculate: {
    document: calculateDocument,
    variables: createVariables,
    response: calculateResponse,
  },
  draftOrderSavedSearches: {
    document: savedSearchesDocument,
    variables: {},
    response: savedSearchesResponse,
  },
  draftOrderAvailableDeliveryOptions: {
    document: availableDeliveryOptionsDocument,
    variables: availableDeliveryOptionsVariables,
    response: availableDeliveryOptionsResponse,
  },
  draftOrderInvoicePreview: {
    document: invoicePreviewDocument,
    variables: invoicePreviewVariables,
    response: invoicePreviewResponse,
  },
  draftOrderBulkAddTags: {
    document: bulkAddDocument,
    variables: bulkAddVariables,
    response: bulkAddResponse,
    downstreamRead: {
      document: draftOrderReadDocument,
      variables: { id: draftOrderId },
      response: afterBulkAddRead,
    },
  },
  draftOrderBulkRemoveTags: {
    document: bulkRemoveDocument,
    variables: bulkRemoveVariables,
    response: bulkRemoveResponse,
    downstreamRead: {
      document: draftOrderReadDocument,
      variables: { id: draftOrderId },
      response: afterBulkRemoveRead,
    },
  },
  draftOrderTag: {
    document: draftOrderTagDocument,
    attempts: draftOrderTagAttempts,
    blocker:
      'Raw tag strings are invalid ID variables and guessed gid://shopify/DraftOrderTag/<tag> identifiers returned null.',
  },
  draftOrderBulkDelete: {
    document: bulkDeleteDocument,
    variables: bulkDeleteVariables,
    response: bulkDeleteResponse,
    downstreamRead: {
      document: draftOrderReadDocument,
      variables: { id: draftOrderId },
      response: afterBulkDeleteRead,
    },
  },
});

// oxlint-disable-next-line no-console -- CLI scripts intentionally write status output to stdout.
console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, file: fixturePath }, null, 2));
