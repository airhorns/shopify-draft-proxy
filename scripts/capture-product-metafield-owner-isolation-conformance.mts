/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'metafields-set-owner-isolation-parity.json');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type UserError = {
  field: string[] | null;
  message: string;
  code?: string | null;
  elementIndex?: number | null;
};

type ProductCreateData = {
  productCreate: {
    product: { id: string; title: string } | null;
    userErrors: UserError[];
  };
};

type ProductDeleteData = {
  productDelete: {
    deletedProductId: string | null;
    userErrors: UserError[];
  };
};

type MetafieldsSetData = {
  metafieldsSet: {
    metafields: Array<{
      namespace: string;
      key: string;
      type: string;
      value: string;
    }>;
    userErrors: UserError[];
  };
};

type ProductMetafieldsReadData = {
  product: {
    id: string;
    metafield: null | {
      namespace: string;
      key: string;
      type: string;
      value: string;
    };
    metafields: {
      nodes: Array<{
        namespace: string;
        key: string;
        type: string;
        value: string;
      }>;
      edges: Array<{
        cursor: string;
        node: {
          namespace: string;
          key: string;
          type: string;
          value: string;
        };
      }>;
      pageInfo: {
        hasNextPage: boolean;
        hasPreviousPage: boolean;
        startCursor: string | null;
        endCursor: string | null;
      };
    };
  } | null;
};

type MetafieldsSetVariables = {
  metafields: [
    {
      ownerId: string;
      namespace: string;
      key: string;
      type: string;
      value: string;
    },
  ];
};

type ProductMetafieldsReadVariables = {
  id: string;
  namespace: string;
  key: string;
};

const createProductMutation = `#graphql
  mutation ProductMetafieldOwnerIsolationCreateProduct($product: ProductCreateInput!) {
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
  mutation ProductMetafieldOwnerIsolationDeleteProduct($input: ProductDeleteInput!) {
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
  mutation MetafieldDefinitionLifecycleMetafieldsSet($metafields: [MetafieldsSetInput!]!) {
    metafieldsSet(metafields: $metafields) {
      metafields {
        namespace
        key
        type
        value
      }
      userErrors {
        field
        message
        code
        elementIndex
      }
    }
  }
`;

const isolatedOwnerReadQuery = `#graphql
  query MetafieldDefinitionLifecycleReadProductMetafield($id: ID!, $namespace: String!, $key: String!) {
    product(id: $id) {
      id
      metafield(namespace: $namespace, key: $key) {
        namespace
        key
        type
        value
      }
      metafields(first: 10, namespace: $namespace) {
        nodes {
          namespace
          key
          type
          value
        }
        edges {
          cursor
          node {
            namespace
            key
            type
            value
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

function assertNoUserErrors(errors: UserError[] | undefined, label: string): void {
  if (Array.isArray(errors) && errors.length === 0) {
    return;
  }

  throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
}

async function createProduct(title: string): Promise<{ id: string; title: string }> {
  const response = await runGraphql<ProductCreateData>(createProductMutation, {
    product: {
      title,
      status: 'DRAFT',
    },
  });
  assertNoUserErrors(response.data?.productCreate.userErrors, `productCreate ${title}`);
  const product = response.data?.productCreate.product ?? null;
  if (!product) {
    throw new Error(`productCreate ${title} did not return a product: ${JSON.stringify(response)}`);
  }
  return product;
}

async function deleteProduct(id: string): Promise<ConformanceGraphqlPayload<ProductDeleteData>> {
  return await runGraphql<ProductDeleteData>(deleteProductMutation, {
    input: { id },
  });
}

await mkdir(outputDir, { recursive: true });

const runId = Date.now().toString(36);
let ownerWithoutMetafields: { id: string; title: string } | null = null;
let ownerWithMetafields: { id: string; title: string } | null = null;
const cleanup: Record<string, ConformanceGraphqlPayload<ProductDeleteData> | { error: string }> = {};

try {
  ownerWithoutMetafields = await createProduct(`Owner metafield isolation empty ${runId}`);
  ownerWithMetafields = await createProduct(`Owner metafield isolation populated ${runId}`);

  const namespace = `owner_isolation_${runId}`;
  const key = 'tier';
  const metafieldsSetVariables: MetafieldsSetVariables = {
    metafields: [
      {
        ownerId: ownerWithMetafields.id,
        namespace,
        key,
        type: 'single_line_text_field',
        value: 'gold',
      },
    ],
  };
  const metafieldsSetResponse = await runGraphql<MetafieldsSetData>(metafieldsSetMutation, metafieldsSetVariables);
  assertNoUserErrors(metafieldsSetResponse.data?.metafieldsSet.userErrors, 'metafieldsSet populated owner');

  const isolatedOwnerReadVariables: ProductMetafieldsReadVariables = {
    id: ownerWithoutMetafields.id,
    namespace,
    key,
  };
  const isolatedOwnerRead = await runGraphql<ProductMetafieldsReadData>(
    isolatedOwnerReadQuery,
    isolatedOwnerReadVariables,
  );

  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId: 'metafields-set-owner-isolation',
        storeDomain,
        apiVersion,
        capturedAt: new Date().toISOString(),
        owners: {
          ownerWithoutMetafields,
          ownerWithMetafields,
        },
        mutation: {
          variables: metafieldsSetVariables,
          response: metafieldsSetResponse,
        },
        isolatedOwnerReadVariables,
        isolatedOwnerRead,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
} finally {
  for (const [label, product] of [
    ['ownerWithoutMetafields', ownerWithoutMetafields],
    ['ownerWithMetafields', ownerWithMetafields],
  ] as const) {
    if (!product) {
      continue;
    }
    try {
      cleanup[label] = await deleteProduct(product.id);
    } catch (error) {
      cleanup[label] = { error: error instanceof Error ? error.message : String(error) };
      console.warn(JSON.stringify({ cleanup: label, productId: product.id, error: cleanup[label] }, null, 2));
    }
  }
}

if (!ownerWithoutMetafields || !ownerWithMetafields) {
  throw new Error('Product metafield owner isolation capture did not create both product owners.');
}

console.log(
  JSON.stringify(
    {
      ok: true,
      fixture: outputPath,
      ownerWithoutMetafields: ownerWithoutMetafields.id,
      ownerWithMetafields: ownerWithMetafields.id,
      cleanup,
    },
    null,
    2,
  ),
);
