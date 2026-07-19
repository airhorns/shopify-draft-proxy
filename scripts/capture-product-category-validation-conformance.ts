/* oxlint-disable no-console -- CLI recorder intentionally writes capture status to stdout. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureEntry = {
  query: string;
  variables: Record<string, unknown>;
  response: {
    status: number;
    payload: unknown;
  };
};

type RecordedUpstreamCall = {
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: {
    status: number;
    body: unknown;
  };
};

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const fixturePath = path.join(outputDir, 'product-category-validation.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateDocument = `mutation ProductCategoryValidationCreate($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      title
      category {
        id
        fullName
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const productUpdateDocument = `mutation ProductCategoryValidationUpdate($product: ProductUpdateInput!) {
  productUpdate(product: $product) {
    product {
      id
      title
      category {
        id
        fullName
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const productSetDocument = `mutation ProductCategoryValidationSet($input: ProductSetInput!, $synchronous: Boolean!) {
  productSet(input: $input, synchronous: $synchronous) {
    product {
      id
      title
      category {
        id
        fullName
      }
    }
    productSetOperation {
      id
      status
      userErrors {
        field
        message
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const productDeleteDocument = `mutation ProductCategoryValidationCleanup($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}
`;

const taxonomyCategoryHydrateQuery =
  'query ProductTaxonomyCategoryHydrate($id: ID!) { node(id: $id) { __typename id ... on TaxonomyCategory { name fullName isLeaf level parentId } } }';

function readPath(value: unknown, parts: string[]): unknown {
  let current = value;
  for (const part of parts) {
    if (typeof current !== 'object' || current === null) {
      return undefined;
    }
    current = (current as JsonRecord)[part];
  }
  return current;
}

function productIdFromCreate(entry: CaptureEntry): string {
  const id = readPath(entry.response.payload, ['data', 'productCreate', 'product', 'id']);
  if (typeof id !== 'string') {
    throw new Error(`Expected productCreate product id, got ${JSON.stringify(entry.response.payload)}`);
  }
  return id;
}

function productIdFromSet(entry: CaptureEntry): string | null {
  const id = readPath(entry.response.payload, ['data', 'productSet', 'product', 'id']);
  return typeof id === 'string' ? id : null;
}

async function capture(query: string, variables: Record<string, unknown>): Promise<CaptureEntry> {
  const response = await runGraphqlRequest(query, variables);
  return {
    query,
    variables,
    response,
  };
}

async function captureUnknownCategoryLookup(id: string): Promise<RecordedUpstreamCall> {
  const lookup = await capture(taxonomyCategoryHydrateQuery, { id });
  const errors = readPath(lookup.response.payload, ['errors']);
  const node = readPath(lookup.response.payload, ['data', 'node']);
  if (lookup.response.status !== 200 || (Array.isArray(errors) && errors.length > 0) || node !== null) {
    throw new Error(`Expected an authoritative null taxonomy node for ${id}: ${JSON.stringify(lookup.response)}`);
  }
  return {
    operationName: 'ProductTaxonomyCategoryHydrate',
    variables: { id },
    query: taxonomyCategoryHydrateQuery,
    response: {
      status: lookup.response.status,
      body: lookup.response.payload,
    },
  };
}

async function cleanupProduct(productId: string | null): Promise<CaptureEntry | null> {
  if (productId === null) {
    return null;
  }
  return capture(productDeleteDocument, { input: { id: productId } });
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
// The taxonomy-shaped tail is intentional: this must prove that GID syntax does
// not establish category existence or supply hierarchy metadata.
const unknownCategory = 'gid://shopify/TaxonomyCategory/zz-999999999';
let setupProductId: string | null = null;
let productSetProductId: string | null = null;

try {
  const upstreamCalls = [
    await captureUnknownCategoryLookup(unknownCategory),
    await captureUnknownCategoryLookup(unknownCategory),
    await captureUnknownCategoryLookup(unknownCategory),
  ];
  const setup = await capture(productCreateDocument, {
    product: {
      title: `Hermes Product Category Validation Setup ${runId}`,
      status: 'DRAFT',
    },
  });
  setupProductId = productIdFromCreate(setup);

  const productCreateUnknownCategory = await capture(productCreateDocument, {
    product: {
      title: `Hermes Product Unknown Category Create ${runId}`,
      status: 'DRAFT',
      category: unknownCategory,
    },
  });
  const productUpdateUnknownCategory = await capture(productUpdateDocument, {
    product: {
      id: setupProductId,
      category: unknownCategory,
    },
  });
  const productSetUnknownCategory = await capture(productSetDocument, {
    input: {
      title: `Hermes Product Unknown Category Set ${runId}`,
      category: unknownCategory,
    },
    synchronous: true,
  });
  productSetProductId = productIdFromSet(productSetUnknownCategory);

  await writeFile(
    fixturePath,
    `${JSON.stringify(
      {
        scenarioId: 'product-category-validation',
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        setup,
        captures: {
          productCreateUnknownCategory,
          productUpdateUnknownCategory,
          productSetUnknownCategory,
        },
        upstreamCalls,
      },
      null,
      2,
    )}\n`,
  );

  console.log(
    JSON.stringify(
      {
        ok: true,
        fixturePath,
      },
      null,
      2,
    ),
  );
} finally {
  const cleanup = {
    setupProduct: await cleanupProduct(setupProductId),
    productSetProduct: await cleanupProduct(productSetProductId),
  };
  console.log(`Cleanup: ${JSON.stringify(cleanup)}`);
}
