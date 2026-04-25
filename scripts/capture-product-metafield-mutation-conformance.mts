// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

import { parseWriteScopeBlocker, renderWriteScopeBlockerNote } from './product-mutation-conformance-lib.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'product-metafield-mutation-conformance-scope-blocker.md');
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createProductMutation = `#graphql
  mutation ProductMetafieldConformanceCreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteProductMutation = `#graphql
  mutation ProductMetafieldConformanceDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const metafieldsSetMutation = `#graphql
  mutation MetafieldsSetConformance($metafields: [MetafieldsSetInput!]!) {
    metafieldsSet(metafields: $metafields) {
      metafields {
        id
        namespace
        key
        type
        value
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const metafieldsDeleteMutation = `#graphql
  mutation MetafieldsDeleteConformance($metafields: [MetafieldIdentifierInput!]!) {
    metafieldsDelete(metafields: $metafields) {
      deletedMetafields {
        key
        namespace
        ownerId
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query ProductMetafieldDownstream($id: ID!) {
    product(id: $id) {
      id
      primarySpec: metafield(namespace: "custom", key: "material") {
        id
        namespace
        key
        type
        value
      }
      origin: metafield(namespace: "details", key: "origin") {
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
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
    }
  }
`;

const productMetafieldsReadQuery = `#graphql
  query ProductMetafieldsRead($id: ID!, $namespace: String!, $key: String!, $after: String) {
    product(id: $id) {
      id
      title
      primarySpec: metafield(namespace: $namespace, key: $key) {
        id
        namespace
        key
        type
        value
        compareDigest
        jsonValue
        createdAt
        updatedAt
        ownerType
        definition {
          id
          name
        }
      }
      metafields(first: 1) {
        nodes {
          id
          namespace
          key
          type
          value
          compareDigest
          jsonValue
          createdAt
          updatedAt
          ownerType
        }
        edges {
          cursor
          node {
            id
            namespace
            key
            type
            value
            compareDigest
            jsonValue
            createdAt
            updatedAt
            ownerType
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      nextMetafields: metafields(first: 1, after: $after) {
        nodes {
          id
          namespace
          key
          type
          value
          compareDigest
          jsonValue
          createdAt
          updatedAt
          ownerType
        }
        edges {
          cursor
          node {
            id
            namespace
            key
            type
            value
            compareDigest
            jsonValue
            createdAt
            updatedAt
            ownerType
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
    }
  }
`;

function buildCreateProductVariables(runId) {
  return {
    product: {
      title: `Hermes Product Metafield Conformance ${runId}`,
      status: 'DRAFT',
    },
  };
}

function buildMetafieldsSetVariables(productId) {
  return {
    metafields: [
      {
        ownerId: productId,
        namespace: 'custom',
        key: 'material',
        type: 'single_line_text_field',
        value: 'Canvas',
      },
      {
        ownerId: productId,
        namespace: 'details',
        key: 'origin',
        type: 'single_line_text_field',
        value: 'VN',
      },
    ],
  };
}

function buildMetafieldsDeleteVariables(productId) {
  return {
    metafields: [
      {
        ownerId: productId,
        namespace: 'custom',
        key: 'material',
      },
      {
        ownerId: productId,
        namespace: 'custom',
        key: 'missing',
      },
    ],
  };
}

function buildMetafieldsDeleteNonexistentOwnerVariables() {
  return {
    metafields: [
      {
        ownerId: 'gid://shopify/Product/999999999999999',
        namespace: 'custom',
        key: 'material',
      },
    ],
  };
}

function buildMetafieldsDeleteMissingKeyVariables(productId) {
  return {
    metafields: [
      {
        ownerId: productId,
        namespace: 'custom',
      },
    ],
  };
}

async function writeScopeBlocker(blocker) {
  await mkdir(pendingDir, { recursive: true });
  const note = renderWriteScopeBlockerNote({
    title: 'Product metafield mutation conformance blocker',
    whatFailed: 'Attempted to capture live conformance for the product-scoped metafield write slice (`metafieldsSet`).',
    operations: ['metafieldsSet'],
    blocker,
    whyBlocked:
      'Without a write-capable token, the repo cannot capture successful live metafield payload shape, userErrors behavior, or immediate downstream `product.metafield(...)` / `product.metafields` parity for staged metafield writes.',
    completedSteps: [
      'added a reusable live-write capture harness for staged product metafield writes',
      'aligned the metafield write mutation and downstream read slices with the parity-request scaffold so future runs capture the same owner-scoped metafield shape directly',
    ],
    recommendedNextStep:
      'Switch the repo conformance credential to a safe dev-store token with product write permissions, then rerun `corepack pnpm conformance:capture-product-metafield-mutations`.',
  });

  await writeFile(blockerPath, `${note}\n`, 'utf8');
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const createProductVariables = buildCreateProductVariables(runId);
let createdProductId = null;

try {
  const createProductResponse = await runGraphql(createProductMutation, createProductVariables);
  createdProductId = createProductResponse.data?.productCreate?.product?.id ?? null;
  if (!createdProductId) {
    throw new Error('Product metafield capture did not return a product id.');
  }

  const metafieldsSetVariables = buildMetafieldsSetVariables(createdProductId);
  const metafieldsSetResponse = await runGraphql(metafieldsSetMutation, metafieldsSetVariables);
  const postSetRead = await runGraphql(downstreamReadQuery, { id: createdProductId });
  const firstMetafieldCursor = postSetRead.data?.product?.metafields?.pageInfo?.startCursor ?? null;
  const productMetafieldsReadVariables = {
    id: createdProductId,
    namespace: 'custom',
    key: 'material',
    after: typeof firstMetafieldCursor === 'string' ? firstMetafieldCursor : null,
  };
  const productMetafieldsRead = await runGraphql(productMetafieldsReadQuery, productMetafieldsReadVariables);
  const metafieldsDeleteVariables = buildMetafieldsDeleteVariables(createdProductId);
  const metafieldsDeleteResponse = await runGraphql(metafieldsDeleteMutation, metafieldsDeleteVariables);
  const postDeleteRead = await runGraphql(downstreamReadQuery, { id: createdProductId });
  const nonexistentOwnerDeleteVariables = buildMetafieldsDeleteNonexistentOwnerVariables();
  const nonexistentOwnerDeleteResponse = await runGraphqlRaw(metafieldsDeleteMutation, nonexistentOwnerDeleteVariables);
  const emptyDeleteVariables = { metafields: [] };
  const emptyDeleteResponse = await runGraphqlRaw(metafieldsDeleteMutation, emptyDeleteVariables);
  const missingKeyDeleteVariables = buildMetafieldsDeleteMissingKeyVariables(createdProductId);
  const missingKeyDeleteResponse = await runGraphqlRaw(metafieldsDeleteMutation, missingKeyDeleteVariables);

  const setCaptureFile = 'metafields-set-parity.json';
  await writeFile(
    path.join(outputDir, setCaptureFile),
    `${JSON.stringify(
      {
        mutation: {
          variables: metafieldsSetVariables,
          response: metafieldsSetResponse,
        },
        downstreamRead: postSetRead,
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  const deleteCaptureFile = 'metafields-delete-parity.json';
  await writeFile(
    path.join(outputDir, deleteCaptureFile),
    `${JSON.stringify(
      {
        mutation: {
          variables: metafieldsDeleteVariables,
          response: metafieldsDeleteResponse,
        },
        downstreamRead: postDeleteRead,
        edgeCases: {
          nonexistentOwner: {
            variables: nonexistentOwnerDeleteVariables,
            response: nonexistentOwnerDeleteResponse,
          },
          emptyInput: {
            variables: emptyDeleteVariables,
            response: emptyDeleteResponse,
          },
          missingKey: {
            variables: missingKeyDeleteVariables,
            response: missingKeyDeleteResponse,
          },
        },
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  const readCaptureFile = 'product-metafields.json';
  await writeFile(
    path.join(outputDir, readCaptureFile),
    `${JSON.stringify(
      {
        variables: productMetafieldsReadVariables,
        response: productMetafieldsRead,
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
        outputDir,
        files: [setCaptureFile, deleteCaptureFile, readCaptureFile],
        productId: createdProductId,
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
  if (createdProductId) {
    try {
      await runGraphql(deleteProductMutation, { input: { id: createdProductId } });
    } catch (cleanupError) {
      console.warn(
        JSON.stringify(
          {
            ok: false,
            cleanup: 'productDelete',
            productId: createdProductId,
            error: cleanupError instanceof Error ? cleanupError.message : String(cleanupError),
          },
          null,
          2,
        ),
      );
    }
  }
}
