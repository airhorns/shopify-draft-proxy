/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

import { createAdminGraphqlClient, runStorefrontGraphqlRequest } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import {
  buildAdminAuthHeaders,
  buildStorefrontRequestHeaders,
  getStoredStorefrontAccessToken,
  getValidConformanceAccessToken,
} from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

type GraphqlUpstreamCapture = {
  name: string;
  method: 'POST';
  apiSurface: 'storefront';
  apiVersion: string;
  path: string;
  endpoint: string;
  authMode: 'storefront-access-token';
  headers: Record<string, string>;
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const storedStorefrontAuth = await getStoredStorefrontAccessToken();
if (storedStorefrontAuth.shop && storedStorefrontAuth.shop !== storeDomain) {
  throw new Error(
    `Stored Storefront token is for ${storedStorefrontAuth.shop}, but the configured store is ${storeDomain}.`,
  );
}

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const adminEndpoint = `${adminOrigin}/admin/api/${apiVersion}/graphql.json`;
const adminPath = `/admin/api/${apiVersion}/graphql.json`;
const storefrontEndpoint = `https://${storeDomain}/api/${apiVersion}/graphql.json`;
const storefrontPath = `/api/${apiVersion}/graphql.json`;
const storefrontOptions = {
  storeOrigin: `https://${storeDomain}`,
  apiVersion,
  storefrontAccessToken: storedStorefrontAuth.storefront_access_token,
};
const storefrontRedactedHeaders = Object.fromEntries(
  Object.keys(buildStorefrontRequestHeaders(storedStorefrontAuth.storefront_access_token)).map((name) => [
    name,
    '<redacted:storefront-access-token>',
  ]),
);

const documents = {
  definitionCreate: 'config/parity-requests/storefront/storefront-collections-definition-create-admin.graphql',
  productsCreate: 'config/parity-requests/storefront/storefront-collections-products-create-admin.graphql',
  variantsUpdate: 'config/parity-requests/storefront/storefront-collections-variants-update-admin.graphql',
  publicationHydrate: 'config/parity-requests/storefront/storefront-catalog-publications-hydrate-admin.graphql',
  productsPublish: 'config/parity-requests/storefront/storefront-collections-products-publish-admin.graphql',
  collectionsCreate: 'config/parity-requests/storefront/storefront-collections-create-admin.graphql',
  collectionsPublish: 'config/parity-requests/storefront/storefront-collections-publish-admin.graphql',
  storefrontInitial: 'config/parity-requests/storefront/storefront-collections-read-after-admin-setup.graphql',
  reorder: 'config/parity-requests/storefront/storefront-collections-reorder-admin.graphql',
  productsRead: 'config/parity-requests/storefront/storefront-collections-products-read.graphql',
  update: 'config/parity-requests/storefront/storefront-collections-update-admin.graphql',
  removeProduct: 'config/parity-requests/storefront/storefront-collections-remove-product-admin.graphql',
  addProduct: 'config/parity-requests/storefront/storefront-collections-add-product-admin.graphql',
  unpublishProduct: 'config/parity-requests/storefront/storefront-catalog-unpublish-admin.graphql',
  publishProduct: 'config/parity-requests/storefront/storefront-catalog-publish-admin.graphql',
  deleteProduct: 'config/parity-requests/storefront/storefront-catalog-product-delete-admin.graphql',
  unpublishCollection: 'config/parity-requests/storefront/storefront-collections-unpublish-admin.graphql',
  deleteCollection: 'config/parity-requests/storefront/storefront-collections-delete-admin.graphql',
  absentRead: 'config/parity-requests/storefront/storefront-collections-absent-read.graphql',
} as const;
const documentText = Object.fromEntries(
  await Promise.all(
    Object.entries(documents).map(async ([key, documentPath]) => [key, await readFile(documentPath, 'utf8')]),
  ),
) as Record<keyof typeof documents, string>;

const suffix = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
const metafieldNamespace = `storefront_collections_${suffix}`;
const collectionHandlePrefix = `storefront-collections-${suffix}`;
const primaryHandle = `${collectionHandlePrefix}-alpha`;
const secondaryHandle = `${collectionHandlePrefix}-beta`;
const updatedHandle = `${collectionHandlePrefix}-updated`;
const collectionQuery = suffix;
const productHandles = {
  alpha: `storefront-collections-product-alpha-${suffix}`,
  beta: `storefront-collections-product-beta-${suffix}`,
  gamma: `storefront-collections-product-gamma-${suffix}`,
};
const updatedProductHandle = `storefront-collections-product-beta-updated-${suffix}`;

const adminCaptures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
const productIds: string[] = [];
const collectionIds: string[] = [];
let definitionId: string | null = null;
let primaryCollectionId: string | null = null;
let secondaryCollectionId: string | null = null;
let storefrontInitialCapture: GraphqlUpstreamCapture | null = null;
let storefrontReorderedCapture: GraphqlUpstreamCapture | null = null;
let storefrontUpdatedCapture: GraphqlUpstreamCapture | null = null;
let storefrontRemovedCapture: GraphqlUpstreamCapture | null = null;
let storefrontRestoredCapture: GraphqlUpstreamCapture | null = null;
let storefrontProductUnpublishedCapture: GraphqlUpstreamCapture | null = null;
let storefrontProductRepublishedCapture: GraphqlUpstreamCapture | null = null;
let storefrontProductDeletedCapture: GraphqlUpstreamCapture | null = null;
let storefrontCollectionUnpublishedCapture: GraphqlUpstreamCapture | null = null;
let storefrontCollectionDeletedCapture: GraphqlUpstreamCapture | null = null;

async function captureAdmin(name: string, query: string, variables: Record<string, unknown>): Promise<unknown> {
  const result = await runGraphqlRaw(query, variables);
  adminCaptures.push({
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  });
  return result.payload;
}

async function captureCleanup(name: string, query: string, variables: Record<string, unknown>): Promise<void> {
  const result = await runGraphqlRaw(query, variables);
  cleanupCaptures.push({
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  });
}

async function captureStorefront(
  name: string,
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<GraphqlUpstreamCapture> {
  const result = await runStorefrontGraphqlRequest(storefrontOptions, query, variables);
  return {
    name,
    method: 'POST',
    apiSurface: 'storefront',
    apiVersion,
    path: storefrontPath,
    endpoint: storefrontEndpoint,
    authMode: 'storefront-access-token',
    headers: storefrontRedactedHeaders,
    operationName,
    query,
    variables,
    response: { status: result.status, body: result.payload },
  };
}

function readPath(value: unknown, segments: string[]): unknown {
  return segments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) return null;
    return (current as Record<string, unknown>)[segment] ?? null;
  }, value);
}

function readRequiredString(value: unknown, segments: string[], label: string): string {
  const result = readPath(value, segments);
  if (typeof result !== 'string' || result.length === 0) {
    throw new Error(`${label} did not return a string at ${segments.join('.')}: ${JSON.stringify(value)}`);
  }
  return result;
}

function assertNoTopLevelErrors(value: unknown, label: string): void {
  const errors = readPath(value, ['errors']);
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertNoUserErrors(value: unknown, segments: string[], label: string): void {
  const errors = readPath(value, segments);
  if (Array.isArray(errors) && errors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
}

function collectionProductIds(value: unknown): string[] {
  const directNodes = readPath(value, ['data', 'byId', 'products', 'nodes']);
  const edges = readPath(value, ['data', 'byId', 'products', 'edges']);
  const nodes = Array.isArray(directNodes)
    ? directNodes
    : Array.isArray(edges)
      ? edges.map((edge) => readPath(edge, ['node']))
      : [];
  if (!Array.isArray(nodes)) return [];
  return nodes.flatMap((node) => {
    if (typeof node !== 'object' || node === null) return [];
    const id = (node as Record<string, unknown>)['id'];
    return typeof id === 'string' ? [id] : [];
  });
}

function initialStorefrontReady(value: unknown): boolean {
  return (
    readPath(value, ['data', 'byId', 'handle']) === primaryHandle &&
    readPath(value, ['data', 'byHandleArgument', 'handle']) === primaryHandle &&
    readPath(value, ['data', 'deprecatedByHandle', 'handle']) === primaryHandle &&
    readPath(value, ['data', 'byId', 'metafield', 'value']) === `Visible collection metafield ${suffix}` &&
    collectionProductIds(value).length === 2 &&
    readPath(value, ['data', 'firstPage', 'pageInfo', 'hasNextPage']) === true
  );
}

async function waitForStorefront(
  name: string,
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
  ready: (value: unknown) => boolean,
): Promise<GraphqlUpstreamCapture> {
  let latest: GraphqlUpstreamCapture | null = null;
  for (let attempt = 1; attempt <= 45; attempt += 1) {
    latest = await captureStorefront(name, operationName, query, variables);
    assertNoTopLevelErrors(latest.response.body, `${name} attempt ${attempt}`);
    if (ready(latest.response.body)) return latest;
    await delay(2000);
  }
  throw new Error(`${name} did not reach the expected state: ${JSON.stringify(latest?.response.body, null, 2)}`);
}

function adminCapture(name: string): Capture | undefined {
  return adminCaptures.find((capture) => capture.name === name);
}

function adminUpstreamCall(capture: Capture, operationName: string): Record<string, unknown> {
  return {
    name: capture.name,
    method: 'POST',
    apiSurface: 'admin',
    apiVersion,
    path: adminPath,
    endpoint: adminEndpoint,
    authMode: 'admin-access-token',
    operationName,
    query: capture.request.query,
    variables: capture.request.variables,
    response: { status: capture.status, body: capture.response },
  };
}

const productCreateVariables = {
  alpha: {
    title: `Storefront Collections Alpha ${suffix}`,
    handle: productHandles.alpha,
    status: 'ACTIVE',
    vendor: 'Hermes North',
    productType: 'Collection Fixture',
    tags: ['alpha', 'storefront-collections'],
    productOptions: [{ name: 'Color', values: [{ name: 'Red' }] }],
  },
  beta: {
    title: `Storefront Collections Beta ${suffix}`,
    handle: productHandles.beta,
    status: 'ACTIVE',
    vendor: 'Hermes South',
    productType: 'Collection Fixture',
    tags: ['beta', 'storefront-collections'],
    productOptions: [{ name: 'Color', values: [{ name: 'Blue' }] }],
  },
  gamma: {
    title: `Storefront Collections Gamma ${suffix}`,
    handle: productHandles.gamma,
    status: 'ACTIVE',
    vendor: 'Hermes North',
    productType: 'Collection Fixture',
    tags: ['gamma', 'storefront-collections'],
    productOptions: [{ name: 'Color', values: [{ name: 'Green' }] }],
  },
} satisfies Record<string, unknown>;

const metafieldDefinitionDeleteMutation = `#graphql
  mutation StorefrontCollectionsDefinitionCleanup($id: ID!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
      deletedDefinitionId
      userErrors { field message code }
    }
  }
`;

try {
  const definitionCreate = await captureAdmin('admin-definition-create', documentText.definitionCreate, {
    definition: {
      ownerType: 'COLLECTION',
      namespace: metafieldNamespace,
      key: 'visible',
      name: `Storefront collection visible ${suffix}`,
      type: 'single_line_text_field',
      access: { storefront: 'PUBLIC_READ' },
    },
  });
  assertNoTopLevelErrors(definitionCreate, 'metafield definition create');
  assertNoUserErrors(
    definitionCreate,
    ['data', 'metafieldDefinitionCreate', 'userErrors'],
    'metafieldDefinitionCreate',
  );
  definitionId = readRequiredString(
    definitionCreate,
    ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id'],
    'metafield definition id',
  );

  const productsCreate = await captureAdmin(
    'admin-products-create',
    documentText.productsCreate,
    productCreateVariables,
  );
  assertNoTopLevelErrors(productsCreate, 'product creates');
  const productState = ['alpha', 'beta', 'gamma'].map((key) => {
    assertNoUserErrors(productsCreate, ['data', key, 'userErrors'], `${key} productCreate`);
    const id = readRequiredString(productsCreate, ['data', key, 'product', 'id'], `${key} product id`);
    const variantId = readRequiredString(
      productsCreate,
      ['data', key, 'product', 'variants', 'nodes', '0', 'id'],
      `${key} variant id`,
    );
    productIds.push(id);
    return { key, id, variantId };
  });
  const [alpha, beta, gamma] = productState;
  if (alpha === undefined || beta === undefined || gamma === undefined) {
    throw new Error('Product setup did not return all three products.');
  }

  const variantsUpdateVariables = {
    alphaProductId: alpha.id,
    alphaVariants: [
      {
        id: alpha.variantId,
        inventoryPolicy: 'CONTINUE',
        price: '10.00',
        inventoryItem: { tracked: true },
      },
    ],
    betaProductId: beta.id,
    betaVariants: [
      {
        id: beta.variantId,
        inventoryPolicy: 'DENY',
        price: '20.00',
        inventoryItem: { tracked: true },
      },
    ],
    gammaProductId: gamma.id,
    gammaVariants: [
      {
        id: gamma.variantId,
        inventoryPolicy: 'CONTINUE',
        price: '30.00',
        inventoryItem: { tracked: true },
      },
    ],
  } satisfies Record<string, unknown>;
  const variantsUpdate = await captureAdmin(
    'admin-variants-update',
    documentText.variantsUpdate,
    variantsUpdateVariables,
  );
  assertNoTopLevelErrors(variantsUpdate, 'variant updates');
  for (const key of ['alpha', 'beta', 'gamma']) {
    assertNoUserErrors(variantsUpdate, ['data', key, 'userErrors'], `${key} productVariantsBulkUpdate`);
  }

  const publicationHydrate = await captureAdmin('admin-publication-hydrate', documentText.publicationHydrate, {});
  assertNoTopLevelErrors(publicationHydrate, 'publication hydrate');
  const publicationNodes = readPath(publicationHydrate, ['data', 'publications', 'nodes']);
  if (!Array.isArray(publicationNodes)) throw new Error('Publication hydrate did not return nodes.');
  const storefrontPublication = publicationNodes.find((node) => {
    return typeof node === 'object' && node !== null && (node as Record<string, unknown>)['name'] === 'Online Store';
  });
  const publicationId = readRequiredString(storefrontPublication, ['id'], 'Online Store publication id');
  const publicationInput = [{ publicationId }];

  const productsPublish = await captureAdmin('admin-products-publish', documentText.productsPublish, {
    alphaId: alpha.id,
    betaId: beta.id,
    gammaId: gamma.id,
    input: publicationInput,
    publicationId,
  });
  assertNoTopLevelErrors(productsPublish, 'product publish');
  for (const key of ['alpha', 'beta', 'gamma']) {
    assertNoUserErrors(productsPublish, ['data', key, 'userErrors'], `${key} publishablePublish`);
  }

  const collectionsCreateVariables = {
    primary: {
      title: `Storefront Collections Alpha ${suffix}`,
      handle: primaryHandle,
      descriptionHtml: `<p>Storefront collection description ${suffix}</p>`,
      sortOrder: 'MANUAL',
      products: [alpha.id, beta.id, gamma.id],
      image: {
        src: 'https://placehold.co/64x64/png',
        altText: `Storefront collection image ${suffix}`,
      },
      seo: {
        title: `Storefront Collection SEO ${suffix}`,
        description: `Storefront collection SEO description ${suffix}`,
      },
      metafields: [
        {
          namespace: metafieldNamespace,
          key: 'visible',
          type: 'single_line_text_field',
          value: `Visible collection metafield ${suffix}`,
        },
      ],
    },
    secondary: {
      title: `Storefront Collections Beta ${suffix}`,
      handle: secondaryHandle,
      sortOrder: 'MANUAL',
    },
  } satisfies Record<string, unknown>;
  const collectionsCreate = await captureAdmin(
    'admin-collections-create',
    documentText.collectionsCreate,
    collectionsCreateVariables,
  );
  assertNoTopLevelErrors(collectionsCreate, 'collection creates');
  assertNoUserErrors(collectionsCreate, ['data', 'primary', 'userErrors'], 'primary collectionCreate');
  assertNoUserErrors(collectionsCreate, ['data', 'secondary', 'userErrors'], 'secondary collectionCreate');
  primaryCollectionId = readRequiredString(
    collectionsCreate,
    ['data', 'primary', 'collection', 'id'],
    'primary collection id',
  );
  secondaryCollectionId = readRequiredString(
    collectionsCreate,
    ['data', 'secondary', 'collection', 'id'],
    'secondary collection id',
  );
  collectionIds.push(primaryCollectionId, secondaryCollectionId);

  const collectionsPublish = await captureAdmin('admin-collections-publish', documentText.collectionsPublish, {
    primaryId: primaryCollectionId,
    secondaryId: secondaryCollectionId,
    input: publicationInput,
    publicationId,
  });
  assertNoTopLevelErrors(collectionsPublish, 'collection publish');
  assertNoUserErrors(collectionsPublish, ['data', 'primary', 'userErrors'], 'primary collection publish');
  assertNoUserErrors(collectionsPublish, ['data', 'secondary', 'userErrors'], 'secondary collection publish');

  const initialStorefrontVariables = {
    id: primaryCollectionId,
    handle: primaryHandle,
    query: collectionQuery,
    metafieldNamespace,
    country: 'CA',
  };
  storefrontInitialCapture = await waitForStorefront(
    'storefront-collections-read-after-admin-setup',
    'StorefrontCollectionsReadAfterAdminSetup',
    documentText.storefrontInitial,
    initialStorefrontVariables,
    initialStorefrontReady,
  );

  const reorder = await captureAdmin('admin-collection-reorder', documentText.reorder, {
    id: primaryCollectionId,
    moves: [{ id: gamma.id, newPosition: '0' }],
  });
  assertNoTopLevelErrors(reorder, 'collection reorder');
  assertNoUserErrors(reorder, ['data', 'collectionReorderProducts', 'userErrors'], 'collectionReorderProducts');
  const readVariables = { id: primaryCollectionId, handle: primaryHandle, query: collectionQuery };
  storefrontReorderedCapture = await waitForStorefront(
    'storefront-collections-reordered-read',
    'StorefrontCollectionsProductsRead',
    documentText.productsRead,
    readVariables,
    (value) => collectionProductIds(value)[0] === gamma.id,
  );

  const updatedTitle = `Storefront Collections Updated ${suffix}`;
  const updatedProductTitle = `Storefront Collections Product Updated ${suffix}`;
  const update = await captureAdmin('admin-collection-and-product-update', documentText.update, {
    collection: {
      id: primaryCollectionId,
      title: updatedTitle,
      handle: updatedHandle,
      descriptionHtml: `<p>Updated Storefront collection description ${suffix}</p>`,
      image: {
        src: 'https://placehold.co/80x80/png',
        altText: `Updated Storefront collection image ${suffix}`,
      },
      seo: {
        title: `Updated Storefront Collection SEO ${suffix}`,
        description: `Updated Storefront collection SEO description ${suffix}`,
      },
    },
    product: { id: beta.id, title: updatedProductTitle, handle: updatedProductHandle },
  });
  assertNoTopLevelErrors(update, 'collection and product update');
  assertNoUserErrors(update, ['data', 'collectionUpdate', 'userErrors'], 'collectionUpdate');
  assertNoUserErrors(update, ['data', 'productUpdate', 'userErrors'], 'productUpdate');
  const updatedReadVariables = {
    id: primaryCollectionId,
    handle: updatedHandle,
    query: collectionQuery,
  };
  storefrontUpdatedCapture = await waitForStorefront(
    'storefront-collections-updated-read',
    'StorefrontCollectionsProductsRead',
    documentText.productsRead,
    updatedReadVariables,
    (value) =>
      readPath(value, ['data', 'byId', 'title']) === updatedTitle &&
      readPath(value, ['data', 'byHandle', 'handle']) === updatedHandle &&
      readPath(value, ['data', 'byId', 'products', 'nodes', '2', 'title']) === updatedProductTitle,
  );

  const removeProduct = await captureAdmin('admin-collection-remove-product', documentText.removeProduct, {
    id: primaryCollectionId,
    productIds: [beta.id],
  });
  assertNoTopLevelErrors(removeProduct, 'collection remove product');
  assertNoUserErrors(removeProduct, ['data', 'collectionRemoveProducts', 'userErrors'], 'collectionRemoveProducts');
  storefrontRemovedCapture = await waitForStorefront(
    'storefront-collections-product-removed-read',
    'StorefrontCollectionsProductsRead',
    documentText.productsRead,
    updatedReadVariables,
    (value) => !collectionProductIds(value).includes(beta.id) && collectionProductIds(value).length === 2,
  );

  const addProduct = await captureAdmin('admin-collection-add-product', documentText.addProduct, {
    id: primaryCollectionId,
    productIds: [beta.id],
  });
  assertNoTopLevelErrors(addProduct, 'collection add product');
  assertNoUserErrors(addProduct, ['data', 'collectionAddProducts', 'userErrors'], 'collectionAddProducts');
  storefrontRestoredCapture = await waitForStorefront(
    'storefront-collections-product-restored-read',
    'StorefrontCollectionsProductsRead',
    documentText.productsRead,
    updatedReadVariables,
    (value) => collectionProductIds(value).includes(beta.id) && collectionProductIds(value).length === 3,
  );

  const unpublishProduct = await captureAdmin('admin-product-unpublish', documentText.unpublishProduct, {
    id: gamma.id,
    input: publicationInput,
    publicationId,
  });
  assertNoTopLevelErrors(unpublishProduct, 'product unpublish');
  assertNoUserErrors(unpublishProduct, ['data', 'publishableUnpublish', 'userErrors'], 'product publishableUnpublish');
  storefrontProductUnpublishedCapture = await waitForStorefront(
    'storefront-collections-product-unpublished-read',
    'StorefrontCollectionsProductsRead',
    documentText.productsRead,
    updatedReadVariables,
    (value) => !collectionProductIds(value).includes(gamma.id) && collectionProductIds(value).length === 2,
  );

  const republishProduct = await captureAdmin('admin-product-republish', documentText.publishProduct, {
    id: gamma.id,
    input: publicationInput,
    publicationId,
  });
  assertNoTopLevelErrors(republishProduct, 'product republish');
  assertNoUserErrors(republishProduct, ['data', 'publishablePublish', 'userErrors'], 'product publishablePublish');
  storefrontProductRepublishedCapture = await waitForStorefront(
    'storefront-collections-product-republished-read',
    'StorefrontCollectionsProductsRead',
    documentText.productsRead,
    updatedReadVariables,
    (value) => collectionProductIds(value).includes(gamma.id) && collectionProductIds(value).length === 3,
  );

  const deleteProduct = await captureAdmin('admin-product-delete', documentText.deleteProduct, {
    input: { id: alpha.id },
  });
  assertNoTopLevelErrors(deleteProduct, 'product delete');
  assertNoUserErrors(deleteProduct, ['data', 'productDelete', 'userErrors'], 'productDelete');
  productIds.splice(productIds.indexOf(alpha.id), 1);
  storefrontProductDeletedCapture = await waitForStorefront(
    'storefront-collections-product-deleted-read',
    'StorefrontCollectionsProductsRead',
    documentText.productsRead,
    updatedReadVariables,
    (value) => !collectionProductIds(value).includes(alpha.id) && collectionProductIds(value).length === 2,
  );

  const unpublishCollection = await captureAdmin('admin-collection-unpublish', documentText.unpublishCollection, {
    id: primaryCollectionId,
    input: publicationInput,
    publicationId,
  });
  assertNoTopLevelErrors(unpublishCollection, 'collection unpublish');
  assertNoUserErrors(
    unpublishCollection,
    ['data', 'publishableUnpublish', 'userErrors'],
    'collection publishableUnpublish',
  );
  const absentVariables = { id: primaryCollectionId, handle: updatedHandle, query: collectionQuery };
  storefrontCollectionUnpublishedCapture = await waitForStorefront(
    'storefront-collections-unpublished-read',
    'StorefrontCollectionsAbsentRead',
    documentText.absentRead,
    absentVariables,
    (value) =>
      readPath(value, ['data', 'byId']) === null &&
      readPath(value, ['data', 'byHandle']) === null &&
      Array.isArray(readPath(value, ['data', 'catalog', 'nodes'])) &&
      !(readPath(value, ['data', 'catalog', 'nodes']) as unknown[]).some(
        (node) => readPath(node, ['id']) === primaryCollectionId,
      ),
  );

  const deleteCollection = await captureAdmin('admin-collection-delete', documentText.deleteCollection, {
    input: { id: primaryCollectionId },
  });
  assertNoTopLevelErrors(deleteCollection, 'collection delete');
  assertNoUserErrors(deleteCollection, ['data', 'collectionDelete', 'userErrors'], 'collectionDelete');
  collectionIds.splice(collectionIds.indexOf(primaryCollectionId), 1);
  storefrontCollectionDeletedCapture = await waitForStorefront(
    'storefront-collections-deleted-read',
    'StorefrontCollectionsAbsentRead',
    documentText.absentRead,
    absentVariables,
    (value) => readPath(value, ['data', 'byId']) === null && readPath(value, ['data', 'byHandle']) === null,
  );
} finally {
  for (const id of collectionIds.reverse()) {
    await captureCleanup('collectionDelete-cleanup', documentText.deleteCollection, { input: { id } });
  }
  for (const id of productIds.reverse()) {
    await captureCleanup('productDelete-cleanup', documentText.deleteProduct, { input: { id } });
  }
  if (definitionId !== null) {
    await captureCleanup('metafieldDefinitionDelete-cleanup', metafieldDefinitionDeleteMutation, {
      id: definitionId,
    });
  }
}

if (
  primaryCollectionId === null ||
  secondaryCollectionId === null ||
  storefrontInitialCapture === null ||
  storefrontReorderedCapture === null ||
  storefrontUpdatedCapture === null ||
  storefrontRemovedCapture === null ||
  storefrontRestoredCapture === null ||
  storefrontProductUnpublishedCapture === null ||
  storefrontProductRepublishedCapture === null ||
  storefrontProductDeletedCapture === null ||
  storefrontCollectionUnpublishedCapture === null ||
  storefrontCollectionDeletedCapture === null
) {
  throw new Error('Storefront collections capture did not complete.');
}
const publicationHydrateCapture = adminCapture('admin-publication-hydrate');
if (publicationHydrateCapture === undefined) {
  throw new Error('Storefront collections capture did not retain the publication hydrate.');
}

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'storefront');
await mkdir(outputDir, { recursive: true });
const outputPath = path.join(outputDir, 'storefront-collections-read-after-admin-setup.json');
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'storefront-collections-read-after-admin-setup',
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      apiSurface: 'storefront',
      endpoint: storefrontEndpoint,
      authMode: 'storefront-access-token',
      storefrontToken: {
        id: storedStorefrontAuth.storefront_token_id || '<unknown>',
        title: storedStorefrontAuth.storefront_token_title || '<unknown>',
        accessScopes: storedStorefrontAuth.storefront_access_scopes,
        obtainedAt: storedStorefrontAuth.obtained_at || '<unknown>',
      },
      adminDefinitionCreate: adminCapture('admin-definition-create'),
      adminProductsCreate: adminCapture('admin-products-create'),
      adminVariantsUpdate: adminCapture('admin-variants-update'),
      adminPublicationHydrate: adminCapture('admin-publication-hydrate'),
      adminProductsPublish: adminCapture('admin-products-publish'),
      adminCollectionsCreate: adminCapture('admin-collections-create'),
      adminCollectionsPublish: adminCapture('admin-collections-publish'),
      storefrontInitial: storefrontInitialCapture,
      adminReorder: adminCapture('admin-collection-reorder'),
      storefrontReordered: storefrontReorderedCapture,
      adminUpdate: adminCapture('admin-collection-and-product-update'),
      storefrontUpdated: storefrontUpdatedCapture,
      adminRemoveProduct: adminCapture('admin-collection-remove-product'),
      storefrontRemoved: storefrontRemovedCapture,
      adminAddProduct: adminCapture('admin-collection-add-product'),
      storefrontRestored: storefrontRestoredCapture,
      adminProductUnpublish: adminCapture('admin-product-unpublish'),
      storefrontProductUnpublished: storefrontProductUnpublishedCapture,
      adminProductRepublish: adminCapture('admin-product-republish'),
      storefrontProductRepublished: storefrontProductRepublishedCapture,
      adminProductDelete: adminCapture('admin-product-delete'),
      storefrontProductDeleted: storefrontProductDeletedCapture,
      adminCollectionUnpublish: adminCapture('admin-collection-unpublish'),
      storefrontCollectionUnpublished: storefrontCollectionUnpublishedCapture,
      adminCollectionDelete: adminCapture('admin-collection-delete'),
      storefrontCollectionDeleted: storefrontCollectionDeletedCapture,
      cleanup: cleanupCaptures,
      upstreamCalls: [
        adminUpstreamCall(publicationHydrateCapture, 'StorePropertiesPublishableInputValidationHydrate'),
        storefrontInitialCapture,
      ],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
console.log(`Captured authenticated Storefront collections status ${storefrontInitialCapture.response.status}`);
