/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type ProductCreateData = {
  productCreate?: {
    product?: {
      id?: unknown;
      title?: unknown;
      variants?: { nodes?: Array<{ id?: unknown; title?: unknown }> } | null;
    } | null;
    userErrors?: Array<{ field?: unknown; message?: unknown }> | null;
  } | null;
};

type CollectionCreateData = {
  collectionCreate?: {
    collection?: {
      id?: unknown;
      title?: unknown;
    } | null;
    userErrors?: Array<{ field?: unknown; message?: unknown }> | null;
  } | null;
};

type ProductRecord = {
  id: string;
  title: string;
  variantId: string;
  variantTitle: string;
};

type CollectionRecord = {
  id: string;
  title: string;
};

type RawGraphqlResult = {
  status?: number;
  payload?: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-update-reference-validation.json');
const setupRequestPath = 'config/parity-requests/discounts/discount-update-reference-validation-setup.graphql';
const updateRequestPath = 'config/parity-requests/discounts/discount-update-reference-validation-basic-update.graphql';
const hydrateRequestPath = 'config/parity-requests/discounts/discount-item-refs-hydrate.graphql';
const uniquenessRequestPath = 'config/parity-requests/discounts/discount-uniqueness-check.graphql';

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const productCreateMutation = `#graphql
  mutation DiscountUpdateReferenceProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        variants(first: 1) {
          nodes {
            id
            title
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation DiscountUpdateReferenceProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const collectionCreateMutation = `#graphql
  mutation DiscountUpdateReferenceCollectionCreate($input: CollectionInput!) {
    collectionCreate(input: $input) {
      collection {
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

const collectionDeleteMutation = `#graphql
  mutation DiscountUpdateReferenceCollectionDelete($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors {
        field
        message
      }
    }
  }
`;

const discountDeleteMutation = `#graphql
  mutation DiscountUpdateReferenceDiscountDelete($id: ID!) {
    discountCodeDelete(id: $id) {
      deletedCodeDiscountId
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
  }
`;

async function readText(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

function assertNoUserErrors(label: string, userErrors: unknown): void {
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }

  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function readProduct(label: string, response: ConformanceGraphqlPayload<ProductCreateData>): ProductRecord {
  const create = response.data?.productCreate;
  assertNoUserErrors(label, create?.userErrors);
  const product = create?.product;
  const id = product?.id;
  const title = product?.title;
  const variant = product?.variants?.nodes?.[0];
  const variantId = variant?.id;
  const variantTitle = variant?.title;
  if (
    typeof id !== 'string' ||
    typeof title !== 'string' ||
    typeof variantId !== 'string' ||
    typeof variantTitle !== 'string'
  ) {
    throw new Error(`${label} did not return product and variant ids: ${JSON.stringify(response)}`);
  }

  return { id, title, variantId, variantTitle };
}

function readCollection(label: string, response: ConformanceGraphqlPayload<CollectionCreateData>): CollectionRecord {
  const create = response.data?.collectionCreate;
  assertNoUserErrors(label, create?.userErrors);
  const id = create?.collection?.id;
  const title = create?.collection?.title;
  if (typeof id !== 'string' || typeof title !== 'string') {
    throw new Error(`${label} did not return collection id/title: ${JSON.stringify(response)}`);
  }

  return { id, title };
}

function readCreatedDiscountId(response: RawGraphqlResult, root: 'first' | 'second'): string {
  const id = (
    response.payload as {
      data?: Record<string, { codeDiscountNode?: { id?: unknown } | null } | null>;
    }
  ).data?.[root]?.codeDiscountNode?.id;
  if (typeof id !== 'string') {
    throw new Error(`${root} setup did not return a discount id: ${JSON.stringify(response.payload)}`);
  }

  return id;
}

function basicCreateInput(stamp: number, suffix: string, code: string): Record<string, unknown> {
  return {
    title: `update reference ${suffix} ${stamp}`,
    code,
    startsAt: '2026-04-25T00:00:00Z',
    customerSelection: {
      all: true,
    },
    customerGets: {
      value: { percentage: 0.1 },
      items: { all: true },
    },
  };
}

function basicUpdateInput(title: string, items: Record<string, unknown>, code?: string): Record<string, unknown> {
  return {
    title,
    ...(code === undefined ? {} : { code }),
    startsAt: '2026-04-25T00:00:00Z',
    customerGets: {
      value: { percentage: 0.1 },
      items,
    },
  };
}

function codeLookupFound(response: RawGraphqlResult): boolean {
  const node = (response.payload as { data?: { codeDiscountNodeByCode?: { id?: unknown } | null } }).data
    ?.codeDiscountNodeByCode;
  return typeof node?.id === 'string';
}

async function waitForCodeLookup(query: string, code: string): Promise<RawGraphqlResult> {
  let lastResponse: RawGraphqlResult = {};
  for (let attempt = 0; attempt < 20; attempt += 1) {
    lastResponse = (await runGraphqlRaw(query, { code })) as RawGraphqlResult;
    if (codeLookupFound(lastResponse)) {
      return lastResponse;
    }
    await sleep(500);
  }

  return lastResponse;
}

function shopifyGidTail(id: string): string {
  return id.split('?')[0]?.split('/').at(-1) ?? id;
}

function compareShopifyResourceIds(left: string, right: string): number {
  const leftTail = shopifyGidTail(left);
  const rightTail = shopifyGidTail(right);
  const leftNumeric = /^\d+$/.test(leftTail) ? BigInt(leftTail) : undefined;
  const rightNumeric = /^\d+$/.test(rightTail) ? BigInt(rightTail) : undefined;
  if (leftNumeric !== undefined && rightNumeric !== undefined && leftNumeric !== rightNumeric) {
    return leftNumeric < rightNumeric ? -1 : 1;
  }

  return left.localeCompare(right);
}

async function captureUpstreamCall(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const response = (await runGraphqlRaw(query, variables)) as RawGraphqlResult;
  return {
    operationName,
    variables,
    query,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

async function captureHydrateCall(query: string, ids: string[]): Promise<Record<string, unknown>> {
  const variables = { ids: [...ids].sort(compareShopifyResourceIds) };
  return captureUpstreamCall('ProductsHydrateNodes', query, variables);
}

async function runCleanup(cleanup: Array<() => Promise<unknown>>): Promise<unknown[]> {
  const results: unknown[] = [];
  for (const cleanupAction of [...cleanup].reverse()) {
    try {
      results.push(await cleanupAction());
    } catch (error) {
      results.push({ error: error instanceof Error ? error.message : String(error) });
    }
  }

  return results;
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const setupDocument = await readText(setupRequestPath);
const updateDocument = await readText(updateRequestPath);
const hydrateDocument = await readText(hydrateRequestPath);
const uniquenessDocument = await readText(uniquenessRequestPath);
const stamp = Date.now();
const firstCode = `UPDREF${stamp}A`;
const secondCode = `UPDREF${stamp}B`;
const cleanup: Array<() => Promise<unknown>> = [];
let cleanupResults: unknown[] = [];

try {
  const product = readProduct(
    'product create',
    await runGraphql<ProductCreateData>(productCreateMutation, {
      product: {
        title: `discount update reference product ${stamp}`,
        status: 'ACTIVE',
        vendor: 'HERMES',
        productType: 'CONFORMANCE',
        tags: ['conformance', 'discount-update-reference-validation', String(stamp)],
      },
    }),
  );
  cleanup.push(() => runGraphqlRaw(productDeleteMutation, { input: { id: product.id } }));

  const collection = readCollection(
    'collection create',
    await runGraphql<CollectionCreateData>(collectionCreateMutation, {
      input: {
        title: `discount update reference collection ${stamp}`,
      },
    }),
  );
  cleanup.push(() => runGraphqlRaw(collectionDeleteMutation, { input: { id: collection.id } }));

  const setupVariables = {
    first: basicCreateInput(stamp, 'first', firstCode),
    second: basicCreateInput(stamp, 'second', secondCode),
  };
  const upstreamCalls: unknown[] = [
    await captureUpstreamCall('DiscountUniquenessCheck', uniquenessDocument, { code: firstCode }),
    await captureUpstreamCall('DiscountUniquenessCheck', uniquenessDocument, { code: secondCode }),
    await captureHydrateCall(hydrateDocument, ['gid://shopify/Product/0']),
    await captureHydrateCall(hydrateDocument, [product.id, 'gid://shopify/Product/999999999999']),
    await captureHydrateCall(hydrateDocument, [product.variantId, 'gid://shopify/ProductVariant/999999999999']),
    await captureHydrateCall(hydrateDocument, [collection.id, 'gid://shopify/Collection/999999999999']),
  ];

  const setup = (await runGraphqlRaw(setupDocument, setupVariables)) as RawGraphqlResult;
  const firstDiscountId = readCreatedDiscountId(setup, 'first');
  const secondDiscountId = readCreatedDiscountId(setup, 'second');
  cleanup.push(() => runGraphqlRaw(discountDeleteMutation, { id: firstDiscountId }));
  cleanup.push(() => runGraphqlRaw(discountDeleteMutation, { id: secondDiscountId }));
  const firstCodeLookup = await waitForCodeLookup(uniquenessDocument, firstCode);

  const takenCodeVariables = {
    id: secondDiscountId,
    input: basicUpdateInput(`update reference taken ${stamp}`, { all: true }, firstCode),
  };
  const takenCode = (await runGraphqlRaw(updateDocument, takenCodeVariables)) as RawGraphqlResult;

  const ownCodeVariables = {
    id: firstDiscountId,
    input: basicUpdateInput(`update reference own ${stamp}`, { all: true }, firstCode),
  };
  const ownCode = (await runGraphqlRaw(updateDocument, ownCodeVariables)) as RawGraphqlResult;

  const productZeroVariables = {
    id: firstDiscountId,
    input: basicUpdateInput(`update reference product zero ${stamp}`, {
      products: { productsToAdd: ['gid://shopify/Product/0'] },
    }),
  };
  const productZero = (await runGraphqlRaw(updateDocument, productZeroVariables)) as RawGraphqlResult;

  const unknownProductVariables = {
    id: firstDiscountId,
    input: basicUpdateInput(`update reference product unknown ${stamp}`, {
      products: { productsToAdd: [product.id, 'gid://shopify/Product/999999999999'] },
    }),
  };
  const unknownProduct = (await runGraphqlRaw(updateDocument, unknownProductVariables)) as RawGraphqlResult;

  const unknownVariantVariables = {
    id: firstDiscountId,
    input: basicUpdateInput(`update reference variant unknown ${stamp}`, {
      products: { productVariantsToAdd: [product.variantId, 'gid://shopify/ProductVariant/999999999999'] },
    }),
  };
  const unknownVariant = (await runGraphqlRaw(updateDocument, unknownVariantVariables)) as RawGraphqlResult;

  const unknownCollectionVariables = {
    id: firstDiscountId,
    input: basicUpdateInput(`update reference collection unknown ${stamp}`, {
      collections: { add: [collection.id, 'gid://shopify/Collection/999999999999'] },
    }),
  };
  const unknownCollection = (await runGraphqlRaw(updateDocument, unknownCollectionVariables)) as RawGraphqlResult;

  const productCollectionConflictVariables = {
    id: firstDiscountId,
    input: basicUpdateInput(`update reference product collection conflict ${stamp}`, {
      products: { productsToAdd: [product.id] },
      collections: { add: [collection.id] },
    }),
  };
  const productCollectionConflict = (await runGraphqlRaw(
    updateDocument,
    productCollectionConflictVariables,
  )) as RawGraphqlResult;

  cleanupResults = await runCleanup(cleanup);

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    accessScopes: scopeProbe,
    setupResources: {
      product,
      collection,
    },
    variables: {
      setup: setupVariables,
      takenCode: takenCodeVariables,
      ownCode: ownCodeVariables,
      productZero: productZeroVariables,
      unknownProduct: unknownProductVariables,
      unknownVariant: unknownVariantVariables,
      unknownCollection: unknownCollectionVariables,
      productCollectionConflict: productCollectionConflictVariables,
    },
    requests: {
      setup: { query: setupDocument, variables: setupVariables },
      update: { query: updateDocument },
    },
    setup,
    firstCodeLookup,
    takenCode,
    ownCode,
    productZero,
    unknownProduct,
    unknownVariant,
    unknownCollection,
    productCollectionConflict,
    cleanup: cleanupResults,
    upstreamCalls,
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        firstDiscountId,
        secondDiscountId,
        productId: product.id,
        collectionId: collection.id,
      },
      null,
      2,
    ),
  );
} catch (error) {
  cleanupResults = await runCleanup(cleanup);
  console.error(`Cleanup after capture failure: ${JSON.stringify(cleanupResults)}`);
  throw error;
}
