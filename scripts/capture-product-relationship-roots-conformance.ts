/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureStep = {
  variables: Record<string, unknown>;
  response: unknown;
};

type RawCapture = {
  label: string;
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const outputPath = path.join(outputDir, 'product-relationship-roots.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateMutation = `#graphql
  mutation ProductRelationshipCreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
        variants(first: 10) {
          nodes {
            id
            title
            selectedOptions { name value }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation ProductRelationshipDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

const collectionCreateMutation = `#graphql
  mutation ProductRelationshipCreateCollection($input: CollectionInput!) {
    collectionCreate(input: $input) {
      collection {
        id
        title
        handle
        products(first: 10) { nodes { id title handle } }
      }
      userErrors { field message }
    }
  }
`;

const collectionDeleteMutation = `#graphql
  mutation ProductRelationshipDeleteCollection($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors { field message }
    }
  }
`;

const collectionReadQuery = `#graphql
  query ProductRelationshipCollectionRead($collectionId: ID!, $productId: ID!) {
    collection(id: $collectionId) {
      id
      title
      handle
      products(first: 10) { nodes { id title handle } }
      hasProduct(id: $productId)
      productsCount { count precision }
    }
    product(id: $productId) {
      id
      title
      handle
      collections(first: 10) { nodes { id title handle } }
    }
  }
`;

const collectionAddProductsV2Mutation = `#graphql
  mutation ProductRelationshipCollectionAddProductsV2($id: ID!, $productIds: [ID!]!) {
    collectionAddProductsV2(id: $id, productIds: $productIds) {
      job { id done }
      userErrors { field message }
    }
  }
`;

const optionsCreateMutation = `#graphql
  mutation ProductRelationshipProductOptionsCreate($productId: ID!, $options: [OptionCreateInput!]!) {
    productOptionsCreate(productId: $productId, options: $options) {
      product {
        id
        options {
          id
          name
          position
          values
          optionValues { id name hasVariants }
        }
        variants(first: 10) {
          nodes { id title selectedOptions { name value } }
        }
      }
      userErrors { field message }
    }
  }
`;

const optionProductReadQuery = `#graphql
  query ProductRelationshipOptionProductRead($productId: ID!) {
    product(id: $productId) {
      id
      title
      handle
      status
      options {
        id
        name
        position
        values
        optionValues { id name hasVariants }
      }
      variants(first: 10) {
        nodes { id title selectedOptions { name value } }
      }
    }
  }
`;

const productOptionsReorderMutation = `#graphql
  mutation ProductRelationshipProductOptionsReorder($productId: ID!, $options: [OptionReorderInput!]!) {
    productOptionsReorder(productId: $productId, options: $options) {
      product {
        id
        options {
          id
          name
          position
          values
          optionValues { id name hasVariants }
        }
        variants(first: 10) {
          nodes { id title selectedOptions { name value } }
        }
      }
      userErrors { field message }
    }
  }
`;

const productCreateMediaMutation = `#graphql
  mutation ProductRelationshipProductCreateMedia($productId: ID!, $media: [CreateMediaInput!]!) {
    productCreateMedia(productId: $productId, media: $media) {
      media {
        id
        alt
        mediaContentType
        status
        ... on MediaImage { image { id url } }
      }
      mediaUserErrors { field message }
      product {
        id
        media(first: 10) {
          nodes { id alt mediaContentType status }
        }
      }
    }
  }
`;

const productMediaReadQuery = `#graphql
  query ProductRelationshipProductMediaRead($productId: ID!) {
    product(id: $productId) {
      id
      media(first: 10) {
        nodes {
          id
          alt
          mediaContentType
          status
          ... on MediaImage { image { id url } }
        }
      }
    }
  }
`;

const productVariantAppendMediaMutation = `#graphql
  mutation ProductRelationshipProductVariantAppendMedia($productId: ID!, $variantMedia: [ProductVariantAppendMediaInput!]!) {
    productVariantAppendMedia(productId: $productId, variantMedia: $variantMedia) {
      product { id }
      productVariants {
        id
        media(first: 10) { nodes { id alt mediaContentType } }
      }
      userErrors { field message }
    }
  }
`;

const productVariantDetachMediaMutation = `#graphql
  mutation ProductRelationshipProductVariantDetachMedia($productId: ID!, $variantMedia: [ProductVariantDetachMediaInput!]!) {
    productVariantDetachMedia(productId: $productId, variantMedia: $variantMedia) {
      productVariants {
        id
        media(first: 10) { nodes { id alt } }
      }
      userErrors { field message }
    }
  }
`;

const variantMediaReadQuery = `#graphql
  query ProductRelationshipVariantMediaRead($variantId: ID!) {
    productVariant(id: $variantId) {
      id
      media(first: 10) { nodes { id alt } }
    }
  }
`;

const sellingPlanGroupFields = `#graphql
  id
  appId
  name
  merchantCode
  description
  options
  position
  summary
  createdAt
  products(first: 5) { nodes { id title } }
  productVariants(first: 5) { nodes { id title product { id } } }
  sellingPlans(first: 5) {
    nodes {
      id
      name
      description
      options
      position
      category
      createdAt
    }
  }
`;

const sellingPlanGroupCreateMutation = `#graphql
  mutation ProductRelationshipSellingPlanGroupCreate($input: SellingPlanGroupInput!) {
    sellingPlanGroupCreate(input: $input) {
      sellingPlanGroup { ${sellingPlanGroupFields} }
      userErrors { field message code }
    }
  }
`;

const sellingPlanGroupDeleteMutation = `#graphql
  mutation ProductRelationshipSellingPlanGroupDelete($id: ID!) {
    sellingPlanGroupDelete(id: $id) {
      deletedSellingPlanGroupId
      userErrors { field message code }
    }
  }
`;

const productJoinSellingPlanGroupsMutation = `#graphql
  mutation ProductRelationshipProductJoinSellingPlanGroups($id: ID!, $sellingPlanGroupIds: [ID!]!) {
    productJoinSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
      product {
        id
        sellingPlanGroups(first: 5) { nodes { id name merchantCode } }
        sellingPlanGroupsCount { count precision }
      }
      userErrors { field message code }
    }
  }
`;

const productLeaveSellingPlanGroupsMutation = `#graphql
  mutation ProductRelationshipProductLeaveSellingPlanGroups($id: ID!, $sellingPlanGroupIds: [ID!]!) {
    productLeaveSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
      product {
        id
        sellingPlanGroups(first: 5) { nodes { id name merchantCode } }
        sellingPlanGroupsCount { count precision }
      }
      userErrors { field message code }
    }
  }
`;

const productVariantJoinSellingPlanGroupsMutation = `#graphql
  mutation ProductRelationshipProductVariantJoinSellingPlanGroups($id: ID!, $sellingPlanGroupIds: [ID!]!) {
    productVariantJoinSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
      productVariant {
        id
        sellingPlanGroups(first: 5) { nodes { id name merchantCode } }
        sellingPlanGroupsCount { count precision }
      }
      userErrors { field message code }
    }
  }
`;

const productVariantLeaveSellingPlanGroupsMutation = `#graphql
  mutation ProductRelationshipProductVariantLeaveSellingPlanGroups($id: ID!, $sellingPlanGroupIds: [ID!]!) {
    productVariantLeaveSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) {
      productVariant {
        id
        sellingPlanGroups(first: 5) { nodes { id name merchantCode } }
        sellingPlanGroupsCount { count precision }
      }
      userErrors { field message code }
    }
  }
`;

const sellingPlanMembershipReadQuery = `#graphql
  query ProductRelationshipSellingPlanMembershipRead($productId: ID!, $variantId: ID!) {
    product(id: $productId) {
      id
      sellingPlanGroups(first: 5) { nodes { id name merchantCode } }
      sellingPlanGroupsCount { count precision }
    }
    productVariant(id: $variantId) {
      id
      sellingPlanGroups(first: 5) { nodes { id name merchantCode } }
      sellingPlanGroupsCount { count precision }
    }
  }
`;

function readObject(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${label} was not an object.`);
  }
  return value as Record<string, unknown>;
}

function readArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${label} was not an array.`);
  }
  return value;
}

function readData(result: ConformanceGraphqlResult, label: string): Record<string, unknown> {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
  return readObject(result.payload.data, `${label}.data`);
}

async function runStep(label: string, query: string, variables: Record<string, unknown> = {}): Promise<CaptureStep> {
  const result = await runGraphqlRaw(query, variables);
  readData(result, label);
  return { variables, response: result.payload };
}

async function runRaw(label: string, query: string, variables: Record<string, unknown> = {}): Promise<RawCapture> {
  const result = await runGraphqlRaw(query, variables);
  readData(result, label);
  return { label, status: result.status, response: result.payload };
}

function readMutationPayload(step: CaptureStep, root: string): Record<string, unknown> {
  return readObject(
    readObject(readObject(step.response, `${root}.response`)['data'], `${root}.response.data`)[root],
    `${root}.payload`,
  );
}

function readCreatedProduct(step: CaptureStep): Record<string, unknown> {
  return readObject(readMutationPayload(step, 'productCreate')['product'], 'productCreate.product');
}

function readProductVariantId(product: Record<string, unknown>): string {
  const variants = readObject(product['variants'], 'product.variants');
  const nodes = readArray(variants['nodes'], 'product.variants.nodes');
  const firstVariant = readObject(nodes[0], 'product.variants.nodes[0]');
  if (typeof firstVariant['id'] !== 'string') {
    throw new Error('Created product did not include a variant id.');
  }
  return firstVariant['id'];
}

function readOptionByName(product: Record<string, unknown>, name: string): Record<string, unknown> {
  const options = readArray(product['options'], 'product.options').map((option, index) =>
    readObject(option, `product.options[${index}]`),
  );
  const option = options.find((candidate) => candidate['name'] === name);
  if (!option) {
    throw new Error(`Could not find option ${name}.`);
  }
  return option;
}

function readOptionValueByName(option: Record<string, unknown>, name: string): Record<string, unknown> {
  const values = readArray(option['optionValues'], 'option.optionValues').map((value, index) =>
    readObject(value, `option.optionValues[${index}]`),
  );
  const optionValue = values.find((candidate) => candidate['name'] === name);
  if (!optionValue) {
    throw new Error(`Could not find option value ${name}.`);
  }
  return optionValue;
}

function readId(source: Record<string, unknown>, label: string): string {
  if (typeof source['id'] !== 'string') {
    throw new Error(`${label} did not include an id.`);
  }
  return source['id'];
}

function seedMediaFromRead(productId: string, mediaRead: CaptureStep): Array<Record<string, unknown>> {
  const product = readObject(
    readObject(readObject(mediaRead.response, 'mediaRead.response')['data'], 'mediaRead.data')['product'],
    'mediaRead.product',
  );
  const mediaConnection = readObject(product['media'], 'mediaRead.product.media');
  return readArray(mediaConnection['nodes'], 'mediaRead.product.media.nodes').map((media, index) => {
    const mediaObject = readObject(media, `mediaRead.product.media.nodes[${index}]`);
    const image =
      typeof mediaObject['image'] === 'object' && mediaObject['image']
        ? (mediaObject['image'] as Record<string, unknown>)
        : {};
    return {
      productId,
      id: mediaObject['id'],
      position: index + 1,
      alt: mediaObject['alt'],
      mediaContentType: mediaObject['mediaContentType'],
      status: mediaObject['status'],
      productImageId: image['id'] ?? null,
      imageUrl: image['url'] ?? null,
    };
  });
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForReadyMedia(productId: string): Promise<CaptureStep> {
  let latestRead: CaptureStep | null = null;
  for (let attempt = 0; attempt < 12; attempt += 1) {
    if (attempt > 0) {
      await delay(5000);
    }
    latestRead = await runStep('media ready read', productMediaReadQuery, { productId });
    const seedMedia = seedMediaFromRead(productId, latestRead);
    if (seedMedia.length >= 2 && seedMedia.every((media) => media['status'] === 'READY')) {
      return latestRead;
    }
  }

  throw new Error('Timed out waiting for product media to become READY.');
}

async function waitForCollectionMembership(
  collectionId: string,
  productId: string,
  expectedProductCount: number,
): Promise<CaptureStep> {
  let latestRead: CaptureStep | null = null;
  for (let attempt = 0; attempt < 12; attempt += 1) {
    if (attempt > 0) {
      await delay(5000);
    }
    latestRead = await runStep('collection downstream read', collectionReadQuery, {
      collectionId,
      productId,
    });
    const collection = readObject(
      readObject(readObject(latestRead.response, 'collectionRead.response')['data'], 'collectionRead.data')[
        'collection'
      ],
      'collectionRead.collection',
    );
    const products = readObject(collection['products'], 'collectionRead.collection.products');
    const productNodes = readArray(products['nodes'], 'collectionRead.collection.products.nodes');
    const productsCount = readObject(collection['productsCount'], 'collectionRead.collection.productsCount');
    if (
      collection['hasProduct'] === true &&
      productNodes.length === expectedProductCount &&
      productsCount['count'] === expectedProductCount
    ) {
      return latestRead;
    }
  }

  throw new Error('Timed out waiting for collectionAddProductsV2 downstream membership to settle.');
}

const suffix = Date.now().toString(36);
const cleanup: RawCapture[] = [];
const productIds: string[] = [];
const collectionIds: string[] = [];
const sellingPlanGroupIds: string[] = [];
let fixturePayload: Record<string, unknown> | null = null;

try {
  const collectionProductOne = await runStep('collection product one setup', productCreateMutation, {
    product: { title: `Product relationship collection one ${suffix}`, status: 'DRAFT' },
  });
  const collectionProductTwo = await runStep('collection product two setup', productCreateMutation, {
    product: { title: `Product relationship collection two ${suffix}`, status: 'DRAFT' },
  });
  const collectionProductOnePayload = readCreatedProduct(collectionProductOne);
  const collectionProductTwoPayload = readCreatedProduct(collectionProductTwo);
  const collectionProductOneId = readId(collectionProductOnePayload, 'collection product one');
  const collectionProductTwoId = readId(collectionProductTwoPayload, 'collection product two');
  productIds.push(collectionProductOneId);
  productIds.push(collectionProductTwoId);

  const collectionCreate = await runStep('collection setup', collectionCreateMutation, {
    input: { title: `Product relationship collection ${suffix}` },
  });
  const collectionPayload = readObject(
    readMutationPayload(collectionCreate, 'collectionCreate')['collection'],
    'collection',
  );
  const collectionId = readId(collectionPayload, 'collection');
  collectionIds.push(collectionId);
  const initialCollectionRead = await runStep('initial collection read', collectionReadQuery, {
    collectionId,
    productId: collectionProductOneId,
  });
  const collectionAddProductsV2 = await runStep('collectionAddProductsV2 success', collectionAddProductsV2Mutation, {
    id: collectionId,
    productIds: [collectionProductOneId, collectionProductTwoId],
  });
  const collectionDownstreamRead = await waitForCollectionMembership(collectionId, collectionProductOneId, 2);

  const optionProductCreate = await runStep('option product setup', productCreateMutation, {
    product: { title: `Product relationship option reorder ${suffix}`, status: 'DRAFT' },
  });
  const optionProduct = readCreatedProduct(optionProductCreate);
  const optionProductId = readId(optionProduct, 'option product');
  productIds.push(optionProductId);
  await runStep('option setup', optionsCreateMutation, {
    productId: optionProductId,
    options: [
      { name: 'Color', position: 1, values: [{ name: 'Red' }, { name: 'Blue' }] },
      { name: 'Size', position: 2, values: [{ name: 'Small' }] },
    ],
  });
  const preMutationRead = await runStep('option pre-mutation read', optionProductReadQuery, {
    productId: optionProductId,
  });
  const preMutationProduct = readObject(
    readObject(readObject(preMutationRead.response, 'preMutationRead.response')['data'], 'preMutationRead.data')[
      'product'
    ],
    'preMutationRead.product',
  );
  const colorOption = readOptionByName(preMutationProduct, 'Color');
  const sizeOption = readOptionByName(preMutationProduct, 'Size');
  const redValue = readOptionValueByName(colorOption, 'Red');
  const blueValue = readOptionValueByName(colorOption, 'Blue');
  const smallValue = readOptionValueByName(sizeOption, 'Small');
  const productOptionsReorder = await runStep('productOptionsReorder success', productOptionsReorderMutation, {
    productId: optionProductId,
    options: [
      { id: readId(sizeOption, 'size option'), values: [{ id: readId(smallValue, 'small value') }] },
      {
        id: readId(colorOption, 'color option'),
        values: [{ id: readId(blueValue, 'blue value') }, { id: readId(redValue, 'red value') }],
      },
    ],
  });
  const optionDownstreamRead = await runStep('option downstream read', optionProductReadQuery, {
    productId: optionProductId,
  });

  const mediaProductCreate = await runStep('media product setup', productCreateMutation, {
    product: { title: `Product relationship variant media ${suffix}`, status: 'DRAFT' },
  });
  const mediaProduct = readCreatedProduct(mediaProductCreate);
  const mediaProductId = readId(mediaProduct, 'media product');
  const mediaVariantId = readProductVariantId(mediaProduct);
  productIds.push(mediaProductId);
  const productCreateMedia = await runStep('media setup', productCreateMediaMutation, {
    productId: mediaProductId,
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: 'https://placehold.co/640x480/png?text=product-relationship-front',
        alt: 'Product relationship front',
      },
      {
        mediaContentType: 'IMAGE',
        originalSource: 'https://placehold.co/640x480/png?text=product-relationship-side',
        alt: 'Product relationship side',
      },
    ],
  });
  const mediaReadyRead = await waitForReadyMedia(mediaProductId);
  const seedProductMedia = seedMediaFromRead(mediaProductId, mediaReadyRead);
  const mediaIds = seedProductMedia.map((media, index) => readId(media, `seed media ${index}`));
  const productVariantAppendMedia = await runStep(
    'productVariantAppendMedia success',
    productVariantAppendMediaMutation,
    {
      productId: mediaProductId,
      variantMedia: [{ variantId: mediaVariantId, mediaIds: [mediaIds[1]] }],
    },
  );
  const productVariantDetachMedia = await runStep(
    'productVariantDetachMedia success',
    productVariantDetachMediaMutation,
    {
      productId: mediaProductId,
      variantMedia: [{ variantId: mediaVariantId, mediaIds: [mediaIds[1]] }],
    },
  );
  const variantMediaDownstreamRead = await runStep('variant media downstream read', variantMediaReadQuery, {
    variantId: mediaVariantId,
  });

  const sellingProductCreate = await runStep('selling plan product setup', productCreateMutation, {
    product: { title: `Product relationship selling plan membership ${suffix}`, status: 'DRAFT' },
  });
  const sellingProduct = readCreatedProduct(sellingProductCreate);
  const sellingProductId = readId(sellingProduct, 'selling plan product');
  const sellingVariantId = readProductVariantId(sellingProduct);
  productIds.push(sellingProductId);
  const sellingPlanGroupCreate = await runStep('selling plan group setup', sellingPlanGroupCreateMutation, {
    input: {
      name: `Product relationship membership ${suffix}`,
      merchantCode: `product-relationship-${suffix}`,
      description: 'Temporary selling plan group for product relationship conformance capture',
      options: ['Delivery frequency'],
      position: 1,
      sellingPlansToCreate: [
        {
          name: 'Monthly delivery',
          description: 'Ships every month',
          options: ['Monthly'],
          position: 1,
          category: 'SUBSCRIPTION',
          billingPolicy: { recurring: { interval: 'MONTH', intervalCount: 1 } },
          deliveryPolicy: { recurring: { interval: 'MONTH', intervalCount: 1 } },
          inventoryPolicy: { reserve: 'ON_FULFILLMENT' },
          pricingPolicies: [{ fixed: { adjustmentType: 'PERCENTAGE', adjustmentValue: { percentage: 10 } } }],
        },
      ],
    },
  });
  const sellingPlanGroup = readObject(
    readMutationPayload(sellingPlanGroupCreate, 'sellingPlanGroupCreate')['sellingPlanGroup'],
    'sellingPlanGroupCreate.sellingPlanGroup',
  );
  const sellingPlanGroupId = readId(sellingPlanGroup, 'sellingPlanGroup');
  sellingPlanGroupIds.push(sellingPlanGroupId);
  const productJoinSellingPlanGroups = await runStep(
    'productJoinSellingPlanGroups success',
    productJoinSellingPlanGroupsMutation,
    { id: sellingProductId, sellingPlanGroupIds: [sellingPlanGroupId] },
  );
  const productVariantJoinSellingPlanGroups = await runStep(
    'productVariantJoinSellingPlanGroups success',
    productVariantJoinSellingPlanGroupsMutation,
    { id: sellingVariantId, sellingPlanGroupIds: [sellingPlanGroupId] },
  );
  const productLeaveSellingPlanGroups = await runStep(
    'productLeaveSellingPlanGroups success',
    productLeaveSellingPlanGroupsMutation,
    { id: sellingProductId, sellingPlanGroupIds: [sellingPlanGroupId] },
  );
  const productVariantLeaveSellingPlanGroups = await runStep(
    'productVariantLeaveSellingPlanGroups success',
    productVariantLeaveSellingPlanGroupsMutation,
    { id: sellingVariantId, sellingPlanGroupIds: [sellingPlanGroupId] },
  );
  const sellingPlanDownstreamRead = await runStep('selling plan downstream read', sellingPlanMembershipReadQuery, {
    productId: sellingProductId,
    variantId: sellingVariantId,
  });

  fixturePayload = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes: [
      'Product relationship root captures replayed by the parity runner.',
      'The script creates disposable products, a collection, media, and a selling-plan group, then deletes them during cleanup.',
    ],
    seedProducts: [
      collectionProductOnePayload,
      collectionProductTwoPayload,
      preMutationProduct,
      mediaProduct,
      sellingProduct,
    ],
    seedCollections: [
      readObject(
        readObject(
          readObject(initialCollectionRead.response, 'initialCollectionRead.response')['data'],
          'initialCollectionRead.data',
        )['collection'],
        'initialCollectionRead.collection',
      ),
    ],
    seedProductMedia,
    seedSellingPlanGroups: [sellingPlanGroup],
    mutation: collectionAddProductsV2,
    initialCollectionRead,
    collectionAddProductsV2,
    collectionDownstreamRead,
    preMutationRead,
    productOptionsReorder,
    optionDownstreamRead,
    productCreateMedia,
    mediaReadyRead,
    productVariantAppendMedia,
    productVariantDetachMedia,
    variantMediaDownstreamRead,
    sellingPlanGroupCreate,
    productJoinSellingPlanGroups,
    productVariantJoinSellingPlanGroups,
    productLeaveSellingPlanGroups,
    productVariantLeaveSellingPlanGroups,
    sellingPlanDownstreamRead,
    cleanup,
  };
} finally {
  for (const groupId of sellingPlanGroupIds.reverse()) {
    cleanup.push(await runRaw('cleanup sellingPlanGroupDelete', sellingPlanGroupDeleteMutation, { id: groupId }));
  }
  for (const collectionId of collectionIds.reverse()) {
    cleanup.push(await runRaw('cleanup collectionDelete', collectionDeleteMutation, { input: { id: collectionId } }));
  }
  for (const productId of productIds.reverse()) {
    cleanup.push(await runRaw('cleanup productDelete', productDeleteMutation, { input: { id: productId } }));
  }
}

if (!fixturePayload) {
  throw new Error('Product relationship capture did not produce a fixture payload.');
}

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixturePayload, null, 2)}\n`, 'utf8');
console.log(`Wrote product relationship conformance fixture to ${outputPath}`);
