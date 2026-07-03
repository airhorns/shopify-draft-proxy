/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CapturedRequest = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlPayload;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productLifecycleSlice = `
  id
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
      selectedOptions {
        name
        value
      }
    }
  }
`;

const setupProductMutation = `#graphql
  mutation ProductOptionNameDelimiterSetup($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        title
        ${productLifecycleSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteProductMutation = `#graphql
  mutation ProductOptionNameDelimiterCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const productOptionsCreateDelimiterMutation = `#graphql
  mutation ProductOptionsCreateNameDelimiter(
    $productId: ID!
    $options: [OptionCreateInput!]!
  ) {
    productOptionsCreate(productId: $productId, options: $options) {
      product {
        ${productLifecycleSlice}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const productOptionUpdateDelimiterMutation = `#graphql
  mutation ProductOptionUpdateNameDelimiter(
    $productId: ID!
    $option: OptionUpdateInput!
  ) {
    productOptionUpdate(productId: $productId, option: $option) {
      product {
        ${productLifecycleSlice}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const productCreateDelimiterMutation = `#graphql
  mutation ProductCreateOptionNameDelimiter($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productSetDelimiterMutation = `#graphql
  mutation ProductSetOptionNameDelimiter($input: ProductSetInput!, $synchronous: Boolean!) {
    productSet(input: $input, synchronous: $synchronous) {
      product {
        id
      }
      productSetOperation {
        id
        status
        userErrors {
          field
          message
          code
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query ProductOptionNameDelimiterDownstreamRead($id: ID!) {
    product(id: $id) {
      ${productLifecycleSlice}
    }
  }
`;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    if (Array.isArray(current)) {
      const index = Number.parseInt(part, 10);
      current = Number.isNaN(index) ? undefined : current[index];
    } else {
      current = isRecord(current) ? current[part] : undefined;
    }
  }
  return current;
}

function stringAt(value: unknown, pathParts: string[], context: string): string {
  const resolved = readPath(value, pathParts);
  if (typeof resolved !== 'string') {
    throw new Error(`${context} did not resolve a string at ${pathParts.join('.')}`);
  }
  return resolved;
}

async function captureRequest(query: string, variables: JsonRecord): Promise<CapturedRequest> {
  return {
    query,
    variables,
    response: await runGraphql(query, variables),
  };
}

async function cleanupProduct(productId: string): Promise<void> {
  try {
    await runGraphql(deleteProductMutation, { input: { id: productId } });
  } catch {
    // Best-effort cleanup only. The capture should surface the original failure.
  }
}

function setupProductVariables(runId: string): JsonRecord {
  return {
    product: {
      title: `Hermes Product Option Delimiter Setup ${runId}`,
      status: 'DRAFT',
      productOptions: [
        {
          name: 'Color',
          values: [{ name: 'Red' }],
        },
      ],
    },
  };
}

function productCreateDelimiterVariables(runId: string): JsonRecord {
  return {
    product: {
      title: `Hermes Product Create Delimiter ${runId}`,
      status: 'DRAFT',
      productOptions: [
        {
          name: 'Color / Shade',
          values: [{ name: 'Red' }],
        },
      ],
    },
  };
}

function productSetDelimiterVariables(runId: string): JsonRecord {
  return {
    synchronous: true,
    input: {
      title: `Hermes ProductSet Delimiter ${runId}`,
      status: 'DRAFT',
      productOptions: [
        {
          name: 'X / Y',
          values: [{ name: '1' }],
        },
      ],
      variants: [
        {
          price: '1.00',
          optionValues: [{ optionName: 'X / Y', name: '1' }],
        },
      ],
    },
  };
}

await mkdir(outputDir, { recursive: true });

const runId = Date.now().toString();
const createdProductIds: string[] = [];

try {
  const setupProduct = await captureRequest(setupProductMutation, setupProductVariables(runId));
  const setupProductId = stringAt(
    setupProduct.response,
    ['data', 'productCreate', 'product', 'id'],
    'setup product capture',
  );
  createdProductIds.push(setupProductId);
  const setupOptionId = stringAt(
    setupProduct.response,
    ['data', 'productCreate', 'product', 'options', '0', 'id'],
    'setup product option capture',
  );

  const createDelimiter = await captureRequest(productOptionsCreateDelimiterMutation, {
    productId: setupProductId,
    options: [
      {
        name: 'Size / Fit',
        values: [{ name: 'S' }],
      },
    ],
  });
  const readAfterCreateDelimiter = await captureRequest(downstreamReadQuery, {
    id: setupProductId,
  });

  const updateDelimiter = await captureRequest(productOptionUpdateDelimiterMutation, {
    productId: setupProductId,
    option: {
      id: setupOptionId,
      name: 'A / B',
    },
  });
  const readAfterUpdateDelimiter = await captureRequest(downstreamReadQuery, {
    id: setupProductId,
  });

  const productCreateDelimiter = await captureRequest(
    productCreateDelimiterMutation,
    productCreateDelimiterVariables(runId),
  );
  const maybeProductCreateId = readPath(productCreateDelimiter.response, ['data', 'productCreate', 'product', 'id']);
  if (typeof maybeProductCreateId === 'string') {
    createdProductIds.push(maybeProductCreateId);
  }

  const productSetDelimiter = await captureRequest(productSetDelimiterMutation, productSetDelimiterVariables(runId));
  const maybeProductSetId = readPath(productSetDelimiter.response, ['data', 'productSet', 'product', 'id']);
  if (typeof maybeProductSetId === 'string') {
    createdProductIds.push(maybeProductSetId);
  }

  const capture = {
    notes: [
      'Product option name delimiter validation parity capture.',
      'Shopify forbids the literal " / " sequence in product option names because variant titles are derived by joining option values with that delimiter.',
      'The read-after-failure captures prove rejected productOptionsCreate/productOptionUpdate inputs do not mutate the product option graph.',
    ],
    run: {
      runId,
      storeDomain,
      apiVersion,
      setupProductId,
      setupOptionId,
    },
    setupProduct,
    createDelimiter,
    readAfterCreateDelimiter,
    updateDelimiter,
    readAfterUpdateDelimiter,
    productCreateDelimiter,
    productSetDelimiter,
    upstreamCalls: [],
  };

  const filename = 'product-option-name-delimiter-validation.json';
  await writeFile(path.join(outputDir, filename), `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: [filename],
        setupProductId,
        setupOptionId,
      },
      null,
      2,
    ),
  );
} finally {
  for (const productId of createdProductIds.reverse()) {
    await cleanupProduct(productId);
  }
}
