/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

type CapturedGraphqlResult = {
  status: number;
  payload: unknown;
};

type ProductMutationRoot = 'productCreate' | 'productDuplicate' | 'productSet' | 'productUpdate';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-handle-dedup-parity.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateMutation = `#graphql
  mutation ProductHandleLifecycleCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
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

const productUpdateMutation = `#graphql
  mutation ProductHandleLifecycleUpdate($product: ProductUpdateInput!) {
    productUpdate(product: $product) {
      product {
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

const productSetMutation = `#graphql
  mutation ProductHandleLifecycleSet($input: ProductSetInput!) {
    productSet(input: $input, synchronous: true) {
      product {
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

const productDuplicateMutation = `#graphql
  mutation ProductHandleLifecycleDuplicate($productId: ID!, $newTitle: String!) {
    productDuplicate(productId: $productId, newTitle: $newTitle) {
      newProduct {
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

const productDeleteMutation = `#graphql
  mutation ProductHandleLifecycleCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

async function capture(query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = (await runGraphqlRaw(query, variables)) as CapturedGraphqlResult;
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  return pathSegments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) {
      return null;
    }
    return (current as Record<string, unknown>)[segment] ?? null;
  }, value);
}

function assertGraphqlOk(result: Capture, label: string): void {
  if (readPath(result.response, ['errors'])) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result.response, null, 2)}`);
  }
}

function readProductId(result: Capture, root: ProductMutationRoot): string | null {
  const productKey = root === 'productDuplicate' ? 'newProduct' : 'product';
  const id = readPath(result.response, ['data', root, productKey, 'id']);
  return typeof id === 'string' && id.length > 0 ? id : null;
}

function requireProductId(result: Capture, root: ProductMutationRoot, label: string): string {
  assertGraphqlOk(result, label);
  const id = readProductId(result, root);
  if (!id) {
    throw new Error(`${label} did not return a product id.`);
  }
  return id;
}

function requireProductHandle(result: Capture, root: ProductMutationRoot, label: string): string {
  const productKey = root === 'productDuplicate' ? 'newProduct' : 'product';
  const handle = readPath(result.response, ['data', root, productKey, 'handle']);
  if (typeof handle !== 'string' || handle.length === 0) {
    throw new Error(`${label} did not return a product handle.`);
  }
  return handle;
}

const runId = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const generatedTitle = `Product Handle Lifecycle ${runId}`;
const updateTitle = `Product Handle Update ${runId}`;
const productSetTitle = `Product Set Handle ${runId}`;
const duplicateTitle = `Product Duplicate Handle ${runId}`;

const operations: Record<string, Capture> = {};
const cleanup: Capture[] = [];
const productIds = new Set<string>();

function trackProduct(result: Capture, root: ProductMutationRoot, label: string): string {
  const id = requireProductId(result, root, label);
  productIds.add(id);
  return id;
}

try {
  operations.generatedCreateFirst = await capture(productCreateMutation, {
    product: { title: generatedTitle, status: 'DRAFT' },
  });
  const sourceProductId = trackProduct(operations.generatedCreateFirst, 'productCreate', 'generatedCreateFirst');
  const sourceHandle = requireProductHandle(operations.generatedCreateFirst, 'productCreate', 'generatedCreateFirst');

  operations.generatedCreateSecond = await capture(productCreateMutation, {
    product: { title: generatedTitle, status: 'DRAFT' },
  });
  trackProduct(operations.generatedCreateSecond, 'productCreate', 'generatedCreateSecond');

  operations.normalizedUnicodeCreate = await capture(productCreateMutation, {
    product: {
      title: `Normalized Unicode Handle ${runId}`,
      handle: '  Mixed CASE / 東京 100 % ',
      status: 'DRAFT',
    },
  });
  trackProduct(operations.normalizedUnicodeCreate, 'productCreate', 'normalizedUnicodeCreate');

  operations.punctuationCreateFirst = await capture(productCreateMutation, {
    product: { title: `Punctuation Handle One ${runId}`, handle: '%%%', status: 'DRAFT' },
  });
  trackProduct(operations.punctuationCreateFirst, 'productCreate', 'punctuationCreateFirst');

  operations.punctuationCreateSecond = await capture(productCreateMutation, {
    product: { title: `Punctuation Handle Two ${runId}`, handle: '%%%', status: 'DRAFT' },
  });
  trackProduct(operations.punctuationCreateSecond, 'productCreate', 'punctuationCreateSecond');

  operations.explicitCreateCollision = await capture(productCreateMutation, {
    product: {
      title: `Explicit Handle Collision ${runId}`,
      handle: `  ${sourceHandle.toUpperCase()}  `,
      status: 'DRAFT',
    },
  });
  assertGraphqlOk(operations.explicitCreateCollision, 'explicitCreateCollision');

  operations.updateTargetCreate = await capture(productCreateMutation, {
    product: {
      title: updateTitle,
      handle: `product-handle-update-${runId}`,
      status: 'DRAFT',
    },
  });
  const updateProductId = trackProduct(operations.updateTargetCreate, 'productCreate', 'updateTargetCreate');

  operations.normalizedUnicodeUpdate = await capture(productUpdateMutation, {
    product: { id: updateProductId, handle: '  Updated / 大阪 200 % ' },
  });
  assertGraphqlOk(operations.normalizedUnicodeUpdate, 'normalizedUnicodeUpdate');

  operations.blankUpdate = await capture(productUpdateMutation, {
    product: { id: updateProductId, handle: '   ' },
  });
  assertGraphqlOk(operations.blankUpdate, 'blankUpdate');

  operations.titleOnlyUpdate = await capture(productUpdateMutation, {
    product: { id: updateProductId, title: `${updateTitle} Renamed` },
  });
  assertGraphqlOk(operations.titleOnlyUpdate, 'titleOnlyUpdate');

  operations.explicitUpdateCollision = await capture(productUpdateMutation, {
    product: { id: updateProductId, handle: `  ${sourceHandle.toUpperCase()}  ` },
  });
  assertGraphqlOk(operations.explicitUpdateCollision, 'explicitUpdateCollision');

  operations.punctuationUpdate = await capture(productUpdateMutation, {
    product: { id: updateProductId, handle: '%%%' },
  });
  assertGraphqlOk(operations.punctuationUpdate, 'punctuationUpdate');

  operations.generatedProductSetFirst = await capture(productSetMutation, {
    input: { title: productSetTitle, status: 'DRAFT' },
  });
  const productSetId = trackProduct(operations.generatedProductSetFirst, 'productSet', 'generatedProductSetFirst');

  operations.generatedProductSetSecond = await capture(productSetMutation, {
    input: { title: productSetTitle, status: 'DRAFT' },
  });
  trackProduct(operations.generatedProductSetSecond, 'productSet', 'generatedProductSetSecond');
  const secondProductSetHandle = requireProductHandle(
    operations.generatedProductSetSecond,
    'productSet',
    'generatedProductSetSecond',
  );

  operations.normalizedProductSetUpdate = await capture(productSetMutation, {
    input: { id: productSetId, handle: '  Set / 東京 300 % ' },
  });
  assertGraphqlOk(operations.normalizedProductSetUpdate, 'normalizedProductSetUpdate');

  operations.blankProductSetUpdate = await capture(productSetMutation, {
    input: { id: productSetId, handle: '   ' },
  });
  assertGraphqlOk(operations.blankProductSetUpdate, 'blankProductSetUpdate');

  operations.explicitProductSetCollision = await capture(productSetMutation, {
    input: { id: productSetId, handle: `  ${secondProductSetHandle.toUpperCase()}  ` },
  });
  assertGraphqlOk(operations.explicitProductSetCollision, 'explicitProductSetCollision');

  operations.duplicateHandleOwnerCreate = await capture(productCreateMutation, {
    product: { title: duplicateTitle, status: 'DRAFT' },
  });
  trackProduct(operations.duplicateHandleOwnerCreate, 'productCreate', 'duplicateHandleOwnerCreate');

  operations.productDuplicate = await capture(productDuplicateMutation, {
    productId: sourceProductId,
    newTitle: duplicateTitle,
  });
  trackProduct(operations.productDuplicate, 'productDuplicate', 'productDuplicate');
} finally {
  for (const id of [...productIds].reverse()) {
    cleanup.push(await capture(productDeleteMutation, { input: { id } }));
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'product-handle-dedup',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      operations,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
