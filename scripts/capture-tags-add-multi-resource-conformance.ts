/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonObject = Record<string, unknown>;

type CaptureCase = {
  query: string;
  variables: JsonObject;
  status: number;
  response: unknown;
};

type UpstreamCall = {
  operationName: string;
  variables: JsonObject;
  query: string;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'tags-add-multi-resource.json');
const specPath = path.join('config', 'parity-specs', 'products', 'tagsAdd-multi-resource.json');
const addDocumentPath = path.join('config', 'parity-requests', 'products', 'tagsAdd-multi-resource.graphql');
const removeDocumentPath = path.join('config', 'parity-requests', 'products', 'tagsRemove-multi-resource.graphql');
const readDocumentPath = path.join('config', 'parity-requests', 'products', 'tagsAdd-multi-resource-reads.graphql');

const tagsAddDocument = await readFile(addDocumentPath, 'utf8');
const tagsRemoveDocument = await readFile(removeDocumentPath, 'utf8');
const downstreamReadDocument = await readFile(readDocumentPath, 'utf8');

const runId = `${Date.now()}`;
const baseTag = `hermes-tags-base-${runId}`;
const addTag = `hermes-tags-added-${runId}`;
const removeTag = `hermes-tags-remove-${runId}`;

const productCreateMutation = `#graphql
  mutation TagsMultiResourceProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product { id title tags }
      userErrors { field message }
    }
  }
`;

const customerCreateMutation = `#graphql
  mutation TagsMultiResourceCustomerCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer { id email displayName tags }
      userErrors { field message }
    }
  }
`;

const orderCreateMutation = `#graphql
  mutation TagsMultiResourceOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order { id name tags }
      userErrors { field message }
    }
  }
`;

const draftOrderCreateMutation = `#graphql
  mutation TagsMultiResourceDraftOrderCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder { id name tags }
      userErrors { field message }
    }
  }
`;

const shopCurrencyQuery = `#graphql
  query TagsMultiResourceShopCurrency {
    shop {
      currencyCode
    }
  }
`;

const blogCreateMutation = `#graphql
  mutation TagsMultiResourceBlogCreate($blog: BlogCreateInput!) {
    blogCreate(blog: $blog) {
      blog { id title handle }
      userErrors { field message code }
    }
  }
`;

const articleCreateMutation = `#graphql
  mutation TagsMultiResourceArticleCreate($article: ArticleCreateInput!) {
    articleCreate(article: $article) {
      article { id title handle tags blog { id } }
      userErrors { field message code }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation TagsMultiResourceProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

const customerDeleteMutation = `#graphql
  mutation TagsMultiResourceCustomerDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors { field message }
    }
  }
`;

const orderDeleteMutation = `#graphql
  mutation TagsMultiResourceOrderDelete($id: ID!) {
    orderDelete(orderId: $id) {
      deletedId
      userErrors { field message }
    }
  }
`;

const draftOrderDeleteMutation = `#graphql
  mutation TagsMultiResourceDraftOrderDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors { field message }
    }
  }
`;

const articleDeleteMutation = `#graphql
  mutation TagsMultiResourceArticleDelete($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message code }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation TagsMultiResourceBlogDelete($id: ID!) {
    blogDelete(id: $id) {
      deletedBlogId
      userErrors { field message code }
    }
  }
`;

const productsHydrateNodesQuery = `
query ProductsHydrateNodes($ids: [ID!]!) {
  nodes(ids: $ids) {
    __typename
    id
    ... on Product {
      legacyResourceId
      title
      handle
      status
      vendor
      productType
      tags
      totalInventory
      tracksInventory
      createdAt
      updatedAt
      publishedAt
      descriptionHtml
      onlineStorePreviewUrl
      templateSuffix
      seo { title description }
    }
  }
}`;

const ordersOrderHydrateQuery = `query OrdersOrderHydrate($id: ID!) {
  order(id: $id) { id name tags }
}`;

const ordersDraftOrderHydrateQuery = `query OrdersDraftOrderHydrate($id: ID!) {
  draftOrder(id: $id) { id name tags }
}`;

const customerHydrateQuery = `query CustomerHydrate($id: ID!) {
  customer(id: $id) {
    id firstName lastName displayName email legacyResourceId locale note
    canDelete verifiedEmail dataSaleOptOut taxExempt taxExemptions state tags
    numberOfOrders createdAt updatedAt
    amountSpent { amount currencyCode }
    defaultEmailAddress { emailAddress marketingState marketingOptInLevel marketingUpdatedAt }
    defaultPhoneNumber { phoneNumber marketingState marketingOptInLevel marketingUpdatedAt marketingCollectedFrom }
    emailMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt }
    smsMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt consentCollectedFrom }
    defaultAddress { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea }
    addressesV2(first: 250) { nodes { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea } }
    metafields(first: 250) { nodes { id namespace key type value compareDigest createdAt updatedAt } }
    orders(first: 10, sortKey: CREATED_AT, reverse: true) { nodes { id name email createdAt currentTotalPriceSet { shopMoney { amount currencyCode } } } pageInfo { startCursor endCursor } }
    storeCreditAccounts(first: 50) { nodes { id balance { amount currencyCode } } }
  }
}`;

const articleHydrateQuery = `query TagsArticleHydrate($id: ID!) {
  article(id: $id) {
    __typename
    id
    title
    handle
    tags
    createdAt
    updatedAt
    blog { id }
  }
}`;

async function capture(query: string, variables: JsonObject): Promise<CaptureCase> {
  const result = await runGraphqlRaw(query, variables);
  return {
    query,
    variables,
    status: result.status,
    response: result.payload,
  };
}

async function runRequired(query: string, variables: JsonObject, label: string): Promise<ConformanceGraphqlResult> {
  const result = await runGraphqlRaw(query, variables);
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return result;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  return pathSegments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) {
      return null;
    }
    return (current as JsonObject)[segment] ?? null;
  }, value);
}

function readRequiredId(value: unknown, pathSegments: string[], label: string): string {
  const id = readPath(value, pathSegments);
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${label} did not return an id: ${JSON.stringify(value, null, 2)}`);
  }
  return id;
}

function requireNoUserErrors(value: unknown, pathSegments: string[], label: string): void {
  const errors = readPath(value, pathSegments);
  if (Array.isArray(errors) && errors.length === 0) {
    return;
  }
  throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
}

function readShopCurrency(value: unknown): string {
  const currency = readPath(value, ['data', 'shop', 'currencyCode']);
  if (typeof currency !== 'string' || currency.length === 0) {
    throw new Error(`shop currency probe failed: ${JSON.stringify(value, null, 2)}`);
  }
  return currency;
}

function upstreamCall(operationName: string, variables: JsonObject, query: string, response: unknown): UpstreamCall {
  return {
    operationName,
    variables,
    query,
    response: {
      status: 200,
      body: response,
    },
  };
}

function downstreamVariables(ids: {
  productId: string;
  orderId: string;
  customerId: string;
  draftOrderId: string;
  articleId: string;
}): JsonObject {
  return {
    productId: ids.productId,
    orderId: ids.orderId,
    customerId: ids.customerId,
    draftOrderId: ids.draftOrderId,
    articleId: ids.articleId,
  };
}

await mkdir(outputDir, { recursive: true });

let productId: string | null = null;
let customerId: string | null = null;
let orderId: string | null = null;
let draftOrderId: string | null = null;
let blogId: string | null = null;
let articleId: string | null = null;
const cleanup: CaptureCase[] = [];

try {
  const shopCurrencyResult = await runRequired(shopCurrencyQuery, {}, 'shop currency setup');
  const shopCurrency = readShopCurrency(shopCurrencyResult.payload);

  const productCreate = await runRequired(
    productCreateMutation,
    {
      product: {
        title: `Hermes Tags Product ${runId}`,
        tags: [baseTag],
      },
    },
    'productCreate setup',
  );
  productId = readRequiredId(productCreate.payload, ['data', 'productCreate', 'product', 'id'], 'productCreate setup');
  requireNoUserErrors(productCreate.payload, ['data', 'productCreate', 'userErrors'], 'productCreate setup');

  const customerCreate = await runRequired(
    customerCreateMutation,
    {
      input: {
        firstName: 'Hermes',
        lastName: `Tags ${runId}`,
        email: `hermes-tags-${runId}@example.com`,
        tags: [baseTag],
      },
    },
    'customerCreate setup',
  );
  customerId = readRequiredId(
    customerCreate.payload,
    ['data', 'customerCreate', 'customer', 'id'],
    'customerCreate setup',
  );
  requireNoUserErrors(customerCreate.payload, ['data', 'customerCreate', 'userErrors'], 'customerCreate setup');

  const orderCreate = await runRequired(
    orderCreateMutation,
    {
      order: {
        email: `hermes-tags-order-${runId}@example.com`,
        currency: shopCurrency,
        tags: [baseTag],
        lineItems: [
          {
            title: `Hermes Tags Item ${runId}`,
            priceSet: {
              shopMoney: { amount: '12.00', currencyCode: shopCurrency },
            },
            quantity: 1,
          },
        ],
        transactions: [
          {
            kind: 'SALE',
            status: 'SUCCESS',
            amountSet: {
              shopMoney: { amount: '12.00', currencyCode: shopCurrency },
            },
          },
        ],
      },
      options: { inventoryBehaviour: 'BYPASS' },
    },
    'orderCreate setup',
  );
  orderId = readRequiredId(orderCreate.payload, ['data', 'orderCreate', 'order', 'id'], 'orderCreate setup');
  requireNoUserErrors(orderCreate.payload, ['data', 'orderCreate', 'userErrors'], 'orderCreate setup');

  const draftOrderCreate = await runRequired(
    draftOrderCreateMutation,
    {
      input: {
        email: `hermes-tags-draft-${runId}@example.com`,
        tags: [baseTag, removeTag],
        lineItems: [
          {
            title: `Hermes Draft Tags Item ${runId}`,
            originalUnitPrice: '11.00',
            quantity: 1,
          },
        ],
      },
    },
    'draftOrderCreate setup',
  );
  draftOrderId = readRequiredId(
    draftOrderCreate.payload,
    ['data', 'draftOrderCreate', 'draftOrder', 'id'],
    'draftOrderCreate setup',
  );
  requireNoUserErrors(draftOrderCreate.payload, ['data', 'draftOrderCreate', 'userErrors'], 'draftOrderCreate setup');

  const blogCreate = await runRequired(
    blogCreateMutation,
    { blog: { title: `Hermes Tags Blog ${runId}` } },
    'blogCreate setup',
  );
  blogId = readRequiredId(blogCreate.payload, ['data', 'blogCreate', 'blog', 'id'], 'blogCreate setup');
  requireNoUserErrors(blogCreate.payload, ['data', 'blogCreate', 'userErrors'], 'blogCreate setup');

  const articleCreate = await runRequired(
    articleCreateMutation,
    {
      article: {
        blogId,
        title: `Hermes Tags Article ${runId}`,
        body: 'Tags conformance article',
        author: { name: 'Hermes Conformance' },
        tags: [baseTag],
      },
    },
    'articleCreate setup',
  );
  articleId = readRequiredId(articleCreate.payload, ['data', 'articleCreate', 'article', 'id'], 'articleCreate setup');
  requireNoUserErrors(articleCreate.payload, ['data', 'articleCreate', 'userErrors'], 'articleCreate setup');

  const productHydrate = await runRequired(productsHydrateNodesQuery, { ids: [productId] }, 'product hydrate cassette');
  const orderHydrate = await runRequired(ordersOrderHydrateQuery, { id: orderId }, 'order hydrate cassette');
  const customerHydrate = await runRequired(customerHydrateQuery, { id: customerId }, 'customer hydrate cassette');
  const articleHydrate = await runRequired(articleHydrateQuery, { id: articleId }, 'article hydrate cassette');
  const draftOrderHydrate = await runRequired(
    ordersDraftOrderHydrateQuery,
    { id: draftOrderId },
    'draft order hydrate cassette',
  );

  const productAdd = await capture(tagsAddDocument, { id: productId, tags: [addTag] });
  const orderAdd = await capture(tagsAddDocument, { id: orderId, tags: [addTag] });
  const customerAdd = await capture(tagsAddDocument, { id: customerId, tags: [addTag] });
  const articleAdd = await capture(tagsAddDocument, { id: articleId, tags: [addTag] });
  const draftOrderRemove = await capture(tagsRemoveDocument, { id: draftOrderId, tags: [removeTag] });
  const unsupportedAdd = await capture(tagsAddDocument, {
    id: 'gid://shopify/SomethingUnsupported/1',
    tags: [addTag],
  });
  const downstreamRead = await capture(
    downstreamReadDocument,
    downstreamVariables({ productId, orderId, customerId, draftOrderId, articleId }),
  );

  const fixture = {
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    cases: {
      productAdd,
      orderAdd,
      customerAdd,
      articleAdd,
      draftOrderRemove,
      unsupportedAdd,
    },
    downstreamRead,
    upstreamCalls: [
      upstreamCall('ProductsHydrateNodes', { ids: [productId] }, productsHydrateNodesQuery, productHydrate.payload),
      upstreamCall('OrdersOrderHydrate', { id: orderId }, ordersOrderHydrateQuery, orderHydrate.payload),
      upstreamCall('CustomerHydrate', { id: customerId }, customerHydrateQuery, customerHydrate.payload),
      upstreamCall('TagsArticleHydrate', { id: articleId }, articleHydrateQuery, articleHydrate.payload),
      upstreamCall(
        'OrdersDraftOrderHydrate',
        { id: draftOrderId },
        ordersDraftOrderHydrateQuery,
        draftOrderHydrate.payload,
      ),
    ],
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputPath, specPath, runId }, null, 2));
} finally {
  if (articleId) {
    cleanup.push(await capture(articleDeleteMutation, { id: articleId }));
  }
  if (blogId) {
    cleanup.push(await capture(blogDeleteMutation, { id: blogId }));
  }
  if (draftOrderId) {
    cleanup.push(await capture(draftOrderDeleteMutation, { input: { id: draftOrderId } }));
  }
  if (orderId) {
    cleanup.push(await capture(orderDeleteMutation, { id: orderId }));
  }
  if (customerId) {
    cleanup.push(await capture(customerDeleteMutation, { input: { id: customerId } }));
  }
  if (productId) {
    cleanup.push(await capture(productDeleteMutation, { input: { id: productId } }));
  }
  if (cleanup.length > 0) {
    await writeFile(
      path.join(outputDir, `tags-add-multi-resource-cleanup-${runId}.json`),
      `${JSON.stringify({ storeDomain, apiVersion, runId, cleanup }, null, 2)}\n`,
      'utf8',
    );
  }
}
