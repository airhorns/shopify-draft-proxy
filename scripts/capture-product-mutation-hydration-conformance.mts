/* oxlint-disable no-console -- CLI recorder reports capture progress/results. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonObject = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const requestDir = path.join('config', 'parity-requests', 'products');
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const hydrateQuery = await readFile(path.join(requestDir, 'product-mutation-preflight-hydrate.graphql'), 'utf8');
const productUpdateQuery = await readFile(
  path.join(requestDir, 'product-mutation-hydration-product-update.graphql'),
  'utf8',
);
const variantUpdateQuery = await readFile(
  path.join(requestDir, 'product-mutation-hydration-variant-update.graphql'),
  'utf8',
);
const optionUpdateQuery = await readFile(
  path.join(requestDir, 'product-mutation-hydration-option-update.graphql'),
  'utf8',
);
const downstreamReadQuery = await readFile(path.join(requestDir, 'product-mutation-hydration-read.graphql'), 'utf8');

const createProductQuery = `#graphql
mutation ProductMutationHydrationCaptureCreate(
  $product: ProductCreateInput!
  $media: [CreateMediaInput!]
) {
  productCreate(product: $product, media: $media) {
    product { id }
    userErrors { field message }
  }
}
`;

const createOptionsQuery = `#graphql
mutation ProductMutationHydrationCaptureOptions(
  $productId: ID!
  $options: [OptionCreateInput!]!
) {
  productOptionsCreate(
    productId: $productId
    options: $options
    variantStrategy: CREATE
  ) {
    product {
      id
      options { id name }
      variants(first: 250) { nodes { id position selectedOptions { name value } } }
    }
    userErrors { field message code }
  }
}
`;

const createCollectionQuery = `#graphql
mutation ProductMutationHydrationCaptureCollection($input: CollectionInput!) {
  collectionCreate(input: $input) {
    collection { id }
    userErrors { field message }
  }
}
`;

const addProductToCollectionQuery = `#graphql
mutation ProductMutationHydrationCaptureCollectionProduct(
  $id: ID!
  $productIds: [ID!]!
) {
  collectionAddProducts(id: $id, productIds: $productIds) {
    collection { id }
    userErrors { field message }
  }
}
`;

const deleteProductQuery = `#graphql
mutation ProductMutationHydrationCaptureProductCleanup($input: ProductDeleteInput!) {
  productDelete(input: $input) { deletedProductId userErrors { field message } }
}
`;

const deleteCollectionQuery = `#graphql
mutation ProductMutationHydrationCaptureCollectionCleanup($input: CollectionDeleteInput!) {
  collectionDelete(input: $input) { deletedCollectionId userErrors { field message } }
}
`;

function asObject(value: unknown, label: string): JsonObject {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${label} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonObject;
}

function dataObject(result: ConformanceGraphqlResult<JsonObject>, label: string): JsonObject {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
  return asObject(result.payload.data, `${label} data`);
}

function rootObject(result: ConformanceGraphqlResult<JsonObject>, rootName: string, label: string): JsonObject {
  const root = asObject(dataObject(result, label)[rootName], `${label}.${rootName}`);
  const userErrors = root['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
  return root;
}

function idFrom(value: unknown, label: string): string {
  const id = asObject(value, label)['id'];
  if (typeof id !== 'string') {
    throw new Error(`${label}.id was not a string: ${JSON.stringify(value)}`);
  }
  return id;
}

function nodesFrom(value: unknown, label: string): JsonObject[] {
  const nodes = asObject(value, label)['nodes'];
  if (!Array.isArray(nodes)) {
    throw new Error(`${label}.nodes was not an array: ${JSON.stringify(value)}`);
  }
  return nodes.map((node, index) => asObject(node, `${label}.nodes[${index}]`));
}

async function captureHydrate(productId: string) {
  const variables = {
    id: productId,
    variantsAfter: null,
    mediaAfter: null,
    collectionsAfter: null,
  };
  for (let attempt = 1; attempt <= 30; attempt += 1) {
    const response = await runGraphqlRequest<JsonObject>(hydrateQuery, variables);
    const product = asObject(dataObject(response, 'product mutation preflight hydrate')['product'], 'product');
    const media = nodesFrom(product['media'], 'hydrated product media');
    if (media.every((node) => node['status'] === 'READY')) {
      return {
        operationName: 'ProductMutationPreflightHydrate',
        query: hydrateQuery,
        variables,
        response: {
          status: response.status,
          body: response.payload,
        },
      };
    }
    if (attempt < 30) {
      await delay(1_000);
    }
  }
  throw new Error('Product mutation hydration media did not reach READY before capture.');
}

async function captureRead(productId: string) {
  const variables = { id: productId };
  const response = await runGraphqlRequest<JsonObject>(downstreamReadQuery, variables);
  dataObject(response, 'product mutation downstream read');
  return { variables, response };
}

await mkdir(fixtureDir, { recursive: true });
const runId = Date.now().toString();
const optionValues = Array.from({ length: 12 }, (_, index) => ({
  name: `Hydration value ${String(index + 1).padStart(2, '0')}`,
}));
let productId: string | null = null;
let collectionId: string | null = null;

try {
  const createVariables = {
    product: {
      title: `Product mutation hydration ${runId}`,
      status: 'DRAFT',
      vendor: 'Hydration Vendor',
      productType: 'Hydration Fixture',
      tags: ['hydration-alpha', 'hydration-beta'],
      descriptionHtml: '<p>Rich product description retained by narrow mutations.</p>',
      seo: {
        title: 'Hydration SEO title',
        description: 'Hydration SEO description',
      },
    },
    media: [
      {
        mediaContentType: 'EXTERNAL_VIDEO',
        originalSource: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
        alt: 'Hydration fixture video',
      },
    ],
  };
  const createResponse = await runGraphqlRequest<JsonObject>(createProductQuery, createVariables);
  const createdProduct = rootObject(createResponse, 'productCreate', 'product create')['product'];
  productId = idFrom(createdProduct, 'created product');

  const optionsResponse = await runGraphqlRequest<JsonObject>(createOptionsQuery, {
    productId,
    options: [{ name: 'Color', values: optionValues }],
  });
  const optionsProduct = asObject(
    rootObject(optionsResponse, 'productOptionsCreate', 'product options create')['product'],
    'options product',
  );
  const variants = nodesFrom(optionsProduct['variants'], 'options product variants');
  if (variants.length < 12) {
    throw new Error(`Expected at least 12 variants, received ${variants.length}`);
  }
  const targetVariantId = idFrom(variants[11], 'twelfth variant');
  const options = optionsProduct['options'];
  if (!Array.isArray(options) || options.length === 0) {
    throw new Error(`Expected product option state: ${JSON.stringify(optionsProduct)}`);
  }
  const optionId = idFrom(options[0], 'first option');

  const collectionResponse = await runGraphqlRequest<JsonObject>(createCollectionQuery, {
    input: { title: `Product mutation hydration collection ${runId}` },
  });
  collectionId = idFrom(
    rootObject(collectionResponse, 'collectionCreate', 'collection create')['collection'],
    'created collection',
  );
  const addResponse = await runGraphqlRequest<JsonObject>(addProductToCollectionQuery, {
    id: collectionId,
    productIds: [productId],
  });
  rootObject(addResponse, 'collectionAddProducts', 'collection add product');

  const productUpdateVariables = {
    product: { id: productId, title: `Narrow hydration update ${runId}` },
  };
  const productUpdateHydrate = await captureHydrate(productId);
  const productUpdateResponse = await runGraphqlRequest<JsonObject>(productUpdateQuery, productUpdateVariables);
  rootObject(productUpdateResponse, 'productUpdate', 'narrow product update');
  const productUpdateRead = await captureRead(productId);

  const variantUpdateVariables = {
    productId,
    variants: [{ id: targetVariantId, price: '912.34' }],
  };
  const variantUpdateHydrate = await captureHydrate(productId);
  const variantUpdateResponse = await runGraphqlRequest<JsonObject>(variantUpdateQuery, variantUpdateVariables);
  rootObject(variantUpdateResponse, 'productVariantsBulkUpdate', 'twelfth variant update');
  const variantUpdateRead = await captureRead(productId);

  const optionUpdateVariables = {
    productId,
    option: { id: optionId, name: 'Tone' },
  };
  const optionUpdateHydrate = await captureHydrate(productId);
  const optionUpdateResponse = await runGraphqlRequest<JsonObject>(optionUpdateQuery, optionUpdateVariables);
  rootObject(optionUpdateResponse, 'productOptionUpdate', 'option update');
  const optionUpdateRead = await captureRead(productId);

  const captures = {
    'product-mutation-hydration-product-update.json': {
      mutation: { variables: productUpdateVariables, response: productUpdateResponse.payload },
      downstreamRead: {
        variables: productUpdateRead.variables,
        response: productUpdateRead.response.payload,
      },
      upstreamCalls: [productUpdateHydrate],
    },
    'product-mutation-hydration-variant-update.json': {
      mutation: { variables: variantUpdateVariables, response: variantUpdateResponse.payload },
      downstreamRead: {
        variables: variantUpdateRead.variables,
        response: variantUpdateRead.response.payload,
      },
      upstreamCalls: [variantUpdateHydrate],
    },
    'product-mutation-hydration-option-update.json': {
      mutation: { variables: optionUpdateVariables, response: optionUpdateResponse.payload },
      downstreamRead: {
        variables: optionUpdateRead.variables,
        response: optionUpdateRead.response.payload,
      },
      upstreamCalls: [optionUpdateHydrate],
    },
  };

  for (const [filename, capture] of Object.entries(captures)) {
    await writeFile(path.join(fixtureDir, filename), `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  }
  console.log(JSON.stringify({ ok: true, fixtureDir, files: Object.keys(captures) }, null, 2));
} finally {
  if (productId) {
    try {
      await runGraphqlRequest<JsonObject>(deleteProductQuery, { input: { id: productId } });
    } catch (error) {
      console.error(`Product cleanup failed: ${String(error)}`);
    }
  }
  if (collectionId) {
    try {
      await runGraphqlRequest<JsonObject>(deleteCollectionQuery, { input: { id: collectionId } });
    } catch (error) {
      console.error(`Collection cleanup failed: ${String(error)}`);
    }
  }
}
