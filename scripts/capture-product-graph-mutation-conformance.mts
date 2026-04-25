// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

import { parseWriteScopeBlocker, renderWriteScopeBlockerNote } from './product-mutation-conformance-lib.mjs';

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'product-graph-mutation-conformance-scope-blocker.md');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function expectNoUserErrors(pathLabel, userErrors) {
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }

  throw new Error(`${pathLabel} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

const productSetMutation = readFileSync(
  path.join(repoRoot, 'config', 'parity-requests', 'productSet-parity-plan.graphql'),
  'utf8',
);
const productDuplicateMutation = readFileSync(
  path.join(repoRoot, 'config', 'parity-requests', 'productDuplicate-parity-plan.graphql'),
  'utf8',
);

const sourceAugmentQuery = `#graphql
  query ProductGraphDuplicateSource($id: ID!) {
    product(id: $id) {
      id
      title
      handle
      status
      vendor
      productType
      tags
      descriptionHtml
      templateSuffix
      seo {
        title
        description
      }
      onlineStorePreviewUrl
      options {
        id
        name
        position
        values
        optionValues {
          id
          name
          hasVariants
        }
      }
      variants(first: 10) {
        nodes {
          id
          title
          sku
          barcode
          price
          compareAtPrice
          taxable
          inventoryPolicy
          inventoryQuantity
          selectedOptions {
            name
            value
          }
          inventoryItem {
            id
            tracked
            requiresShipping
            measurement {
              weight {
                unit
                value
              }
            }
            countryCodeOfOrigin
            provinceCodeOfOrigin
            harmonizedSystemCode
          }
        }
      }
      collections(first: 10) {
        nodes {
          id
          title
          handle
        }
      }
      media(first: 10) {
        nodes {
          mediaContentType
          alt
          preview {
            image {
              url
            }
          }
        }
      }
      metafield(namespace: "custom", key: "material") {
        id
        namespace
        key
        type
        value
      }
      metafields(first: 10) {
        nodes {
          id
          namespace
          key
          type
          value
        }
      }
    }
  }
`;

const locationQuery = `#graphql
  query ProductGraphConformanceLocations {
    locations(first: 1) {
      nodes {
        id
      }
    }
  }
`;

const productsByHandleQuery = `#graphql
  query ProductGraphProductsByHandle($query: String!) {
    products(first: 25, query: $query) {
      nodes {
        id
        handle
      }
    }
  }
`;

const collectionCreateMutation = `#graphql
  mutation ProductGraphCollectionCreate($input: CollectionInput!) {
    collectionCreate(input: $input) {
      collection {
        id
        title
        handle
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const collectionDeleteMutation = `#graphql
  mutation ProductGraphCollectionDelete($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors {
        field
        message
      }
    }
  }
`;

const collectionAddProductsMutation = `#graphql
  mutation ProductGraphCollectionAddProducts($id: ID!, $productIds: [ID!]!) {
    collectionAddProducts(id: $id, productIds: $productIds) {
      collection {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productCreateMediaMutation = `#graphql
  mutation ProductGraphCreateMedia($productId: ID!, $media: [CreateMediaInput!]!) {
    productCreateMedia(productId: $productId, media: $media) {
      media {
        id
        alt
        mediaContentType
        status
        preview {
          image {
            url
          }
        }
        ... on MediaImage {
          image {
            url
          }
        }
      }
      mediaUserErrors {
        field
        message
      }
    }
  }
`;

const productMediaPollQuery = `#graphql
  query ProductGraphMediaPoll($id: ID!) {
    product(id: $id) {
      id
      media(first: 10) {
        nodes {
          ... on MediaImage {
            id
            status
            image {
              url
            }
            preview {
              image {
                url
              }
            }
          }
        }
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation ProductGraphDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function buildProductSetVariables(runId, locationId) {
  return {
    synchronous: true,
    input: {
      title: `Hermes Product Graph ${runId}`,
      status: 'DRAFT',
      vendor: 'HERMES',
      productType: 'SNOWBOARD',
      tags: ['conformance', 'product-graph', runId],
      productOptions: [
        {
          name: 'Color',
          position: 1,
          values: [{ name: 'Blue' }, { name: 'Black' }],
        },
      ],
      variants: [
        {
          optionValues: [{ optionName: 'Color', name: 'Blue' }],
          sku: `GRAPH-BLUE-${runId}`,
          price: '79.99',
          inventoryQuantities: [{ quantity: 7, locationId, name: 'available' }],
          inventoryItem: { tracked: true, requiresShipping: true },
        },
        {
          optionValues: [{ optionName: 'Color', name: 'Black' }],
          sku: `GRAPH-BLACK-${runId}`,
          price: '69.99',
          inventoryQuantities: [{ quantity: 3, locationId, name: 'available' }],
          inventoryItem: { tracked: false, requiresShipping: true },
        },
      ],
      metafields: [
        {
          namespace: 'custom',
          key: 'material',
          type: 'single_line_text_field',
          value: 'canvas',
        },
      ],
    },
  };
}

function buildCollectionCreateVariables(runId) {
  return {
    input: {
      title: `Hermes Product Graph Collection ${runId}`,
    },
  };
}

function buildCreateMediaVariables(productId, runId) {
  return {
    productId,
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: 'https://cdn.shopify.com/s/files/1/0533/2089/files/placeholder-images-image_medium.png',
        alt: `Product graph media ${runId}`,
      },
    ],
  };
}

function buildDuplicateVariables(productId, runId) {
  return {
    productId,
    newTitle: `Hermes Product Graph Copy ${runId}`,
  };
}
async function waitForReadyMedia(productId) {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const readResponse = await runGraphql(sourceAugmentQuery, { id: productId });
    const mediaNodes = readResponse.data?.product?.media?.nodes ?? [];
    const firstMediaNode = mediaNodes[0];
    if (firstMediaNode?.preview?.image?.url) {
      return;
    }

    await sleep(2000);
  }

  throw new Error(`Timed out waiting for media to reach READY for product ${productId}.`);
}

async function deleteProductsByExactHandle(handle, protectedProductIds = []) {
  const response = await runGraphql(productsByHandleQuery, { query: `handle:${handle}` });
  const candidates = response.data?.products?.nodes ?? [];
  for (const candidate of candidates) {
    const productId = candidate?.id ?? null;
    if (!productId || protectedProductIds.includes(productId)) {
      continue;
    }

    if (candidate?.handle !== handle) {
      continue;
    }

    await runGraphql(productDeleteMutation, { input: { id: productId } });
  }
}

async function writeScopeBlocker(blocker) {
  await mkdir(pendingDir, { recursive: true });

  const note = renderWriteScopeBlockerNote({
    title: 'Product graph mutation conformance blocker',
    whatFailed:
      'Attempted to capture live conformance for the staged product graph mutation family (`productDuplicate`, `productSet`).',
    operations: ['productDuplicate', 'productSet'],
    blocker,
    whyBlocked:
      'Without a write-capable token, the repo cannot capture successful live mutation payloads or immediate downstream read parity for the duplicate + set product-graph family.',
    completedSteps: [
      'added a reusable live-write capture harness for `productDuplicate` and `productSet`',
      'kept the capture payloads aligned with the existing proxy-request parity documents',
    ],
    recommendedNextStep:
      'Switch the repo conformance credential to a safe dev-store token with `write_products`, then rerun `corepack pnpm conformance:capture-product-graph-mutations`.',
  });

  await writeFile(blockerPath, `${note}\n`, 'utf8');
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
let sourceProductId = null;
let productSetCollisionProductId = null;
let duplicateProductId = null;
let duplicateCollisionProductId = null;
let collectionId = null;

try {
  const locationResponse = await runGraphql(locationQuery);
  const locationId = locationResponse.data?.locations?.nodes?.[0]?.id ?? null;
  if (!locationId) {
    throw new Error(
      'Product graph capture could not resolve a writable location id for productSet inventoryQuantities.',
    );
  }

  const productSetVariables = buildProductSetVariables(runId, locationId);
  const productSetResponse = await runGraphql(productSetMutation, productSetVariables);
  expectNoUserErrors('productSet', productSetResponse.data?.productSet?.userErrors);
  sourceProductId = productSetResponse.data?.productSet?.product?.id ?? null;
  if (!sourceProductId) {
    throw new Error('productSet capture did not return a product id.');
  }

  const productSetCollisionResponse = await runGraphql(productSetMutation, productSetVariables);
  expectNoUserErrors('productSet handle collision create', productSetCollisionResponse.data?.productSet?.userErrors);
  productSetCollisionProductId = productSetCollisionResponse.data?.productSet?.product?.id ?? null;
  if (!productSetCollisionProductId) {
    throw new Error('productSet handle collision capture did not return a product id.');
  }

  await deleteProductsByExactHandle('another-weird-handle-300', [sourceProductId, productSetCollisionProductId]);

  const productSetExplicitNormalizationVariables = {
    synchronous: true,
    identifier: { id: productSetCollisionProductId },
    input: {
      title: `Hermes Product Set Normalization ${runId}`,
      handle: '  Another Weird/Handle 300 % ',
      status: 'DRAFT',
    },
  };
  const productSetExplicitNormalizationResponse = await runGraphql(
    productSetMutation,
    productSetExplicitNormalizationVariables,
  );
  expectNoUserErrors(
    'productSet explicit handle normalization',
    productSetExplicitNormalizationResponse.data?.productSet?.userErrors,
  );

  const postSetRead = await runGraphql(sourceAugmentQuery, { id: sourceProductId });

  const collectionCreateVariables = buildCollectionCreateVariables(runId);
  const collectionCreateResponse = await runGraphql(collectionCreateMutation, collectionCreateVariables);
  expectNoUserErrors('collectionCreate', collectionCreateResponse.data?.collectionCreate?.userErrors);
  collectionId = collectionCreateResponse.data?.collectionCreate?.collection?.id ?? null;
  if (!collectionId) {
    throw new Error('Collection create for productDuplicate source capture did not return a collection id.');
  }

  const collectionAddProductsResponse = await runGraphql(collectionAddProductsMutation, {
    id: collectionId,
    productIds: [sourceProductId],
  });
  expectNoUserErrors('collectionAddProducts', collectionAddProductsResponse.data?.collectionAddProducts?.userErrors);

  const mediaCreateVariables = buildCreateMediaVariables(sourceProductId, runId);
  const mediaCreateResponse = await runGraphql(productCreateMediaMutation, mediaCreateVariables);
  expectNoUserErrors('productCreateMedia', mediaCreateResponse.data?.productCreateMedia?.mediaUserErrors);
  await waitForReadyMedia(sourceProductId);

  const duplicateSourceRead = await runGraphql(sourceAugmentQuery, { id: sourceProductId });
  const duplicateVariables = buildDuplicateVariables(sourceProductId, runId);
  const productDuplicateResponse = await runGraphql(productDuplicateMutation, duplicateVariables);
  expectNoUserErrors('productDuplicate', productDuplicateResponse.data?.productDuplicate?.userErrors);
  duplicateProductId = productDuplicateResponse.data?.productDuplicate?.newProduct?.id ?? null;
  if (!duplicateProductId) {
    throw new Error('productDuplicate capture did not return a duplicated product id.');
  }

  const duplicateCollisionResponse = await runGraphql(productDuplicateMutation, duplicateVariables);
  expectNoUserErrors(
    'productDuplicate handle collision',
    duplicateCollisionResponse.data?.productDuplicate?.userErrors,
  );
  duplicateCollisionProductId = duplicateCollisionResponse.data?.productDuplicate?.newProduct?.id ?? null;
  if (!duplicateCollisionProductId) {
    throw new Error('productDuplicate handle collision capture did not return a duplicated product id.');
  }

  const duplicateDownstreamRead = await runGraphql(sourceAugmentQuery, { id: duplicateProductId });

  const captures = {
    'product-set-parity.json': {
      mutation: {
        variables: productSetVariables,
        response: productSetResponse,
      },
      handleParity: {
        productSetCreateCollision: {
          firstCreate: productSetResponse,
          secondCreate: productSetCollisionResponse,
        },
        productSetExplicitNormalization: {
          variables: productSetExplicitNormalizationVariables,
          response: productSetExplicitNormalizationResponse,
        },
      },
      downstreamRead: postSetRead,
      liveRequirements: {
        inventoryQuantitiesLocationId: locationId,
        inventoryQuantitiesName: 'available',
      },
    },
    'product-duplicate-parity.json': {
      setup: {
        sourceProductId,
        sourceReadBeforeDuplicate: duplicateSourceRead,
        collectionCreate: {
          variables: collectionCreateVariables,
          response: collectionCreateResponse,
        },
        collectionAddProducts: {
          variables: { id: collectionId, productIds: [sourceProductId] },
          response: collectionAddProductsResponse,
        },
        productCreateMedia: {
          variables: mediaCreateVariables,
          response: mediaCreateResponse,
        },
      },
      mutation: {
        variables: duplicateVariables,
        response: productDuplicateResponse,
      },
      handleParity: {
        duplicateCollision: {
          firstDuplicate: productDuplicateResponse,
          secondDuplicate: duplicateCollisionResponse,
        },
      },
      downstreamRead: duplicateDownstreamRead,
    },
  };

  for (const [filename, payload] of Object.entries(captures)) {
    await writeFile(path.join(outputDir, filename), `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
  }

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: Object.keys(captures),
        sourceProductId,
        duplicateProductId,
        collectionId,
      },
      null,
      2,
    ),
  );
} catch (error) {
  const blocker = parseWriteScopeBlocker(error?.result ?? null);
  if (blocker) {
    await writeScopeBlocker(blocker);
    console.log(
      JSON.stringify(
        {
          ok: false,
          blocked: true,
          blockerPath,
          blocker,
        },
        null,
        2,
      ),
    );
    process.exit(1);
  }

  throw error;
} finally {
  if (duplicateCollisionProductId) {
    try {
      await runGraphql(productDeleteMutation, { input: { id: duplicateCollisionProductId } });
    } catch {
      // Best-effort cleanup only.
    }
  }

  if (duplicateProductId) {
    try {
      await runGraphql(productDeleteMutation, { input: { id: duplicateProductId } });
    } catch {
      // Best-effort cleanup only.
    }
  }

  if (productSetCollisionProductId) {
    try {
      await runGraphql(productDeleteMutation, { input: { id: productSetCollisionProductId } });
    } catch {
      // Best-effort cleanup only.
    }
  }

  if (sourceProductId) {
    try {
      await runGraphql(productDeleteMutation, { input: { id: sourceProductId } });
    } catch {
      // Best-effort cleanup only.
    }
  }

  if (collectionId) {
    try {
      await runGraphql(collectionDeleteMutation, { input: { id: collectionId } });
    } catch {
      // Best-effort cleanup only.
    }
  }
}
