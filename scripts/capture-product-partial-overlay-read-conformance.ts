import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type ProductSummary = {
  id: string;
  title: string;
  handle: string;
  status?: string;
};

type ProductCreateData = {
  productCreate?: {
    product?: ProductSummary | null;
    userErrors?: Array<{ field?: string[] | null; message?: string | null }>;
  } | null;
};

type ProductReadData = {
  product?: {
    id: string;
    variants?: {
      nodes?: Array<{ id?: string | null; sku?: string | null; barcode?: string | null } | null> | null;
    } | null;
  } | null;
};

type ProductVariantsBulkUpdateData = {
  productVariantsBulkUpdate?: {
    product?: ProductSummary | null;
    productVariants?: Array<{ id?: string | null; sku?: string | null; barcode?: string | null } | null> | null;
    userErrors?: Array<{ field?: string[] | null; message?: string | null; code?: string | null }>;
  } | null;
};

type ProductsSearchData = {
  handleCount?: { count?: number | null } | null;
  handleProducts?: { edges?: Array<{ node?: { id?: string | null } | null } | null> | null } | null;
  barcodeCount?: { count?: number | null } | null;
  barcodeProducts?: { edges?: Array<{ node?: { id?: string | null } | null } | null> | null } | null;
};

type UpstreamCall = {
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: {
    status: number;
    body: ConformanceGraphqlPayload;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'products-partial-overlay-read.json');
const hydrateDocumentPath = path.join(
  'config',
  'parity-requests',
  'products',
  'products-hydrate-nodes-observation.graphql',
);
const catalogDocumentPath = path.join(
  'config',
  'parity-requests',
  'products',
  'products-partial-overlay-catalog-read.graphql',
);
const searchDocumentPath = path.join(
  'config',
  'parity-requests',
  'products',
  'products-partial-overlay-search-read.graphql',
);
const hydrateDocument = await readFile(hydrateDocumentPath, 'utf8');
const catalogDocument = await readFile(catalogDocumentPath, 'utf8');
const searchDocument = await readFile(searchDocumentPath, 'utf8');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createProductMutation = `#graphql
  mutation ProductPartialOverlayCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productVariantReadQuery = `#graphql
  query ProductPartialOverlayVariantRead($id: ID!) {
    product(id: $id) {
      id
      variants(first: 5) {
        nodes {
          id
          sku
          barcode
        }
      }
    }
  }
`;

const variantSetupMutation = `#graphql
  mutation ProductPartialOverlayVariantSetup($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product {
        id
        title
        handle
        status
      }
      productVariants {
        id
        sku
        barcode
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const cleanupMutation = `#graphql
  mutation ProductPartialOverlayCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function expectNoUserErrors(
  label: string,
  userErrors: Array<{ field?: string[] | null; message?: string | null; code?: string | null }> | null | undefined,
): void {
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function requireProduct(label: string, payload: ConformanceGraphqlPayload<ProductCreateData>): ProductSummary {
  expectNoUserErrors(label, payload.data?.productCreate?.userErrors);
  const product = payload.data?.productCreate?.product;
  if (!product?.id || !product.title || !product.handle) {
    throw new Error(`${label} did not return a complete product: ${JSON.stringify(payload, null, 2)}`);
  }
  return product;
}

function requireDefaultVariantId(payload: ConformanceGraphqlPayload<ProductReadData>): string {
  const variantId = payload.data?.product?.variants?.nodes?.find((variant) => typeof variant?.id === 'string')?.id;
  if (!variantId) {
    throw new Error(`Setup product did not expose a default variant id: ${JSON.stringify(payload, null, 2)}`);
  }
  return variantId;
}

function recordUpstreamCall(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
  body: ConformanceGraphqlPayload,
): UpstreamCall {
  return {
    operationName,
    variables,
    query,
    response: {
      status: 200,
      body,
    },
  };
}

function productIdsFromConnection(connection: ProductsSearchData[keyof ProductsSearchData]): Set<string> {
  const ids = new Set<string>();
  if (
    typeof connection === 'object' &&
    connection !== null &&
    'edges' in connection &&
    Array.isArray(connection.edges)
  ) {
    for (const edge of connection.edges) {
      const id = edge?.node?.id;
      if (typeof id === 'string') ids.add(id);
    }
  }
  return ids;
}

async function sleep(ms: number): Promise<void> {
  await new Promise<void>((resolve) => setTimeout(resolve, ms));
}

async function waitForSearchIndex(
  expectedProductId: string,
  variables: { handleQuery: string; barcodeQuery: string },
): Promise<ConformanceGraphqlPayload<ProductsSearchData>> {
  let lastResponse: ConformanceGraphqlPayload<ProductsSearchData> | null = null;
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const response = await runGraphql<ProductsSearchData>(searchDocument, variables);
    lastResponse = response;
    const handleIds = productIdsFromConnection(response.data?.handleProducts);
    const barcodeIds = productIdsFromConnection(response.data?.barcodeProducts);
    if (
      (response.data?.handleCount?.count ?? 0) > 0 &&
      (response.data?.barcodeCount?.count ?? 0) > 0 &&
      handleIds.has(expectedProductId) &&
      barcodeIds.has(expectedProductId)
    ) {
      return response;
    }
    await sleep(3000);
  }
  throw new Error(
    `Timed out waiting for handle/barcode product search indexing: ${JSON.stringify(lastResponse, null, 2)}`,
  );
}

const stamp = `${Date.now()}`;
const createdProductIds: string[] = [];

try {
  const hydrateCreateVariables = {
    product: {
      title: `Partial Overlay Hydrate ${stamp}`,
      status: 'ACTIVE',
      vendor: 'Conformance',
      productType: 'Overlay Control',
    },
  };
  const searchCreateVariables = {
    product: {
      title: `Partial Overlay Search ${stamp}`,
      status: 'ACTIVE',
      vendor: 'Conformance',
      productType: 'Overlay Search',
    },
  };

  const hydrateCreate = await runGraphql<ProductCreateData>(createProductMutation, hydrateCreateVariables);
  const hydrateProduct = requireProduct('hydrate productCreate', hydrateCreate);
  createdProductIds.push(hydrateProduct.id);

  const searchCreate = await runGraphql<ProductCreateData>(createProductMutation, searchCreateVariables);
  const searchProduct = requireProduct('search productCreate', searchCreate);
  createdProductIds.push(searchProduct.id);

  const defaultVariantRead = await runGraphql<ProductReadData>(productVariantReadQuery, { id: searchProduct.id });
  const defaultVariantId = requireDefaultVariantId(defaultVariantRead);
  const barcode = `872555${stamp.slice(-7)}`;
  const sku = `PARTIAL-OVERLAY-${stamp.slice(-10)}`;
  const variantSetupVariables = {
    productId: searchProduct.id,
    variants: [
      {
        id: defaultVariantId,
        barcode,
        price: '19.95',
        inventoryItem: {
          sku,
          tracked: false,
          requiresShipping: false,
        },
      },
    ],
  };
  const variantSetup = await runGraphql<ProductVariantsBulkUpdateData>(variantSetupMutation, variantSetupVariables);
  expectNoUserErrors('productVariantsBulkUpdate setup', variantSetup.data?.productVariantsBulkUpdate?.userErrors);

  const hydrateVariables = { ids: [hydrateProduct.id] };
  const hydrate = await runGraphql(hydrateDocument, hydrateVariables);

  const catalogVariables = { productId: searchProduct.id, first: 5 };
  const catalog = await runGraphql(catalogDocument, catalogVariables);

  const searchVariables = {
    handleQuery: `handle:${searchProduct.handle}`,
    barcodeQuery: `barcode:${barcode}`,
  };
  const search = await waitForSearchIndex(searchProduct.id, searchVariables);

  const upstreamCalls = [
    recordUpstreamCall('ProductsHydrateNodes', hydrateDocument, hydrateVariables, hydrate),
    recordUpstreamCall('ProductsPartialOverlayCatalogRead', catalogDocument, catalogVariables, catalog),
    recordUpstreamCall('ProductsPartialOverlaySearchRead', searchDocument, searchVariables, search),
  ];

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        setup: {
          hydrateProductCreate: {
            variables: hydrateCreateVariables,
            response: hydrateCreate,
          },
          searchProductCreate: {
            variables: searchCreateVariables,
            response: searchCreate,
          },
          defaultVariantRead: {
            variables: { id: searchProduct.id },
            response: defaultVariantRead,
          },
          variantSetup: {
            variables: variantSetupVariables,
            response: variantSetup,
          },
        },
        hydrate: {
          variables: hydrateVariables,
          response: hydrate,
        },
        catalog: {
          variables: catalogVariables,
          response: catalog,
        },
        search: {
          variables: searchVariables,
          response: search,
        },
        upstreamCalls,
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  // oxlint-disable-next-line no-console -- capture scripts report their written artifact.
  console.log(
    JSON.stringify(
      { ok: true, outputPath, hydrateProductId: hydrateProduct.id, searchProductId: searchProduct.id },
      null,
      2,
    ),
  );
} finally {
  for (const productId of createdProductIds.reverse()) {
    try {
      await runGraphql(cleanupMutation, { input: { id: productId } });
    } catch {
      // Best-effort cleanup only. Preserve the original capture failure if one occurred.
    }
  }
}
