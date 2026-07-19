import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type UserError = { field?: string[] | null; message?: string | null };
type ProductNode = {
  id?: string | null;
  title?: string | null;
  handle?: string | null;
  status?: string | null;
  tags?: string[] | null;
};
type ProductMutationData = {
  productCreate?: { product?: ProductNode | null; userErrors?: UserError[] | null } | null;
  productUpdate?: { product?: ProductNode | null; userErrors?: UserError[] | null } | null;
  productDelete?: { deletedProductId?: string | null; userErrors?: UserError[] | null } | null;
};
type SavedSearchMutationData = {
  savedSearchCreate?: {
    savedSearch?: {
      id?: string | null;
      name?: string | null;
      query?: string | null;
      resourceType?: string | null;
    } | null;
    userErrors?: UserError[] | null;
  } | null;
  savedSearchDelete?: { deletedSavedSearchId?: string | null; userErrors?: UserError[] | null } | null;
};
type ProductConnection = {
  nodes?: ProductNode[] | null;
  pageInfo?: {
    hasNextPage?: boolean | null;
    hasPreviousPage?: boolean | null;
    startCursor?: string | null;
    endCursor?: string | null;
  } | null;
};
type OverlayReadData = {
  catalog?: ProductConnection | null;
  reverse?: ProductConnection | null;
  firstPage?: ProductConnection | null;
  savedSearch?: ProductConnection | null;
  deleted?: ProductConnection | null;
  unrelated?: ProductConnection | null;
  matches?: { count?: number | null; precision?: string | null } | null;
  limitedTotal?: { count?: number | null; precision?: string | null } | null;
  products?: ProductConnection | null;
};
type CatalogHydrateData = {
  products?: ProductConnection | null;
};
type UpstreamCall = {
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: { status: number; body: ConformanceGraphqlPayload };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql: runGraphqlOnce } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const fixturePath = path.join(fixtureDir, 'product-live-hybrid-overlay.json');
const specPath = path.join('config', 'parity-specs', 'products', 'product-live-hybrid-overlay.json');

const requestPath = (...segments: string[]): string => path.join('config', 'parity-requests', 'products', ...segments);
const createDocument = await readFile(requestPath('product-live-hybrid-overlay-create.graphql'), 'utf8');
const updateDocument = await readFile(requestPath('productUpdate-parity-plan.graphql'), 'utf8');
const deleteDocument = await readFile(requestPath('product-live-hybrid-overlay-delete.graphql'), 'utf8');
const nodesHydrateDocument = await readFile(requestPath('products-hydrate-nodes-observation.graphql'), 'utf8');
const catalogHydrateDocument = await readFile(requestPath('product-live-hybrid-catalog-hydrate.graphql'), 'utf8');
const savedSearchHydrateDocument = await readFile(
  requestPath('product-live-hybrid-saved-search-hydrate.graphql'),
  'utf8',
);
const readDocument = await readFile(requestPath('product-live-hybrid-overlay-read.graphql'), 'utf8');
const pageDocument = await readFile(requestPath('product-live-hybrid-overlay-page.graphql'), 'utf8');

async function runGraphql<TData = unknown>(
  query: string,
  variables: Record<string, unknown> = {},
): Promise<ConformanceGraphqlPayload<TData>> {
  let lastError: unknown;
  for (let attempt = 0; attempt < 8; attempt += 1) {
    try {
      return await runGraphqlOnce<TData>(query, variables);
    } catch (error) {
      lastError = error;
      if (!(error instanceof Error) || error.message !== 'Throttled') throw error;
      await sleep(3000);
    }
  }
  throw lastError;
}

const savedSearchCreateDocument = `mutation ProductLiveHybridSavedSearchCreate($input: SavedSearchCreateInput!) {
  savedSearchCreate(input: $input) {
    savedSearch { id name query resourceType }
    userErrors { field message }
  }
}`;
const savedSearchDeleteDocument = `mutation ProductLiveHybridSavedSearchDelete($input: SavedSearchDeleteInput!) {
  savedSearchDelete(input: $input) {
    deletedSavedSearchId
    userErrors { field message }
  }
}`;

function expectNoTopLevelErrors(label: string, payload: ConformanceGraphqlPayload): void {
  if (!Array.isArray(payload.errors) || payload.errors.length === 0) return;
  throw new Error(`${label} returned top-level errors: ${JSON.stringify(payload.errors, null, 2)}`);
}

function expectNoUserErrors(label: string, userErrors: UserError[] | null | undefined): void {
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function requireProduct(
  label: string,
  payload: ConformanceGraphqlPayload<ProductMutationData>,
  root: 'productCreate' | 'productUpdate',
): ProductNode & { id: string; title: string; handle: string } {
  expectNoTopLevelErrors(label, payload);
  const mutation = payload.data?.[root];
  expectNoUserErrors(label, mutation?.userErrors);
  const product = mutation?.product;
  if (typeof product?.id !== 'string' || typeof product.title !== 'string' || typeof product.handle !== 'string') {
    throw new Error(`${label} did not return a complete product: ${JSON.stringify(payload, null, 2)}`);
  }
  return { ...product, id: product.id, title: product.title, handle: product.handle };
}

function upstreamCall(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
  body: ConformanceGraphqlPayload,
): UpstreamCall {
  return { operationName, query, variables, response: { status: 200, body } };
}

async function captureCatalogHydration(): Promise<{
  calls: UpstreamCall[];
  responses: Array<{ variables: Record<string, unknown>; response: ConformanceGraphqlPayload<CatalogHydrateData> }>;
}> {
  const calls: UpstreamCall[] = [];
  const responses: Array<{
    variables: Record<string, unknown>;
    response: ConformanceGraphqlPayload<CatalogHydrateData>;
  }> = [];
  let after: string | null = null;
  const seenCursors = new Set<string>();
  while (true) {
    const variables: { after: string | null } = { after };
    const response: ConformanceGraphqlPayload<CatalogHydrateData> = await runGraphql<CatalogHydrateData>(
      catalogHydrateDocument,
      variables,
    );
    expectNoTopLevelErrors('product catalog hydrate', response);
    calls.push(upstreamCall('DraftProxyProductCatalogHydration', catalogHydrateDocument, variables, response));
    responses.push({ variables, response });
    const pageInfo: ProductConnection['pageInfo'] = response.data?.products?.pageInfo;
    if (pageInfo?.hasNextPage !== true) break;
    if (typeof pageInfo.endCursor !== 'string' || !seenCursors.add(pageInfo.endCursor)) {
      throw new Error(`Product catalog hydrate returned an invalid next cursor: ${JSON.stringify(pageInfo, null, 2)}`);
    }
    after = pageInfo.endCursor;
  }
  return { calls, responses };
}

function titles(connection: ProductConnection | null | undefined): string[] {
  return (connection?.nodes ?? []).flatMap((node) => (typeof node.title === 'string' ? [node.title] : []));
}

async function sleep(ms: number): Promise<void> {
  await new Promise<void>((resolve) => setTimeout(resolve, ms));
}

async function waitForOverlayRead(
  variables: Record<string, unknown>,
  expectedTitles: string[],
  unrelatedTitle: string,
): Promise<ConformanceGraphqlPayload<OverlayReadData>> {
  let lastResponse: ConformanceGraphqlPayload<OverlayReadData> | null = null;
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const response = await runGraphql<OverlayReadData>(readDocument, variables);
    expectNoTopLevelErrors('product overlay read', response);
    lastResponse = response;
    const data = response.data;
    if (
      JSON.stringify(titles(data?.catalog)) === JSON.stringify(expectedTitles) &&
      JSON.stringify(titles(data?.savedSearch)) === JSON.stringify(expectedTitles) &&
      titles(data?.deleted).length === 0 &&
      titles(data?.unrelated).includes(unrelatedTitle) &&
      data?.matches?.count === expectedTitles.length &&
      data.limitedTotal?.count === 1 &&
      data.limitedTotal.precision === 'AT_LEAST'
    ) {
      return response;
    }
    await sleep(3000);
  }
  throw new Error(`Timed out waiting for product overlay indexing: ${JSON.stringify(lastResponse, null, 2)}`);
}

const stamp = String(Date.now());
const matchTag = `overlay-${stamp}`;
const controlTag = `control-${stamp}`;
const deleteTag = `delete-${stamp}`;
const updatedTitle = `A Overlay Updated ${stamp}`;
const createdTitle = `B Overlay Created ${stamp}`;
const unrelatedTitle = `Z Overlay Unrelated ${stamp}`;
const productIdsForCleanup: string[] = [];
let savedSearchIdForCleanup: string | null = null;

try {
  const updateSetupVariables = {
    product: {
      title: `A Overlay Original ${stamp}`,
      status: 'ACTIVE',
      vendor: 'Conformance',
      productType: 'Overlay Base',
      tags: [`update-original-${stamp}`],
    },
  };
  const deleteSetupVariables = {
    product: {
      title: `Delete Overlay ${stamp}`,
      status: 'ACTIVE',
      vendor: 'Conformance',
      productType: 'Overlay Base',
      tags: [deleteTag],
    },
  };
  const unrelatedSetupVariables = {
    product: {
      title: unrelatedTitle,
      status: 'ACTIVE',
      vendor: 'Conformance',
      productType: 'Overlay Control',
      tags: [controlTag],
    },
  };
  const updateSetupResponse = await runGraphql<ProductMutationData>(createDocument, updateSetupVariables);
  const updateProduct = requireProduct('update setup productCreate', updateSetupResponse, 'productCreate');
  productIdsForCleanup.push(updateProduct.id);
  const deleteSetupResponse = await runGraphql<ProductMutationData>(createDocument, deleteSetupVariables);
  const deleteProduct = requireProduct('delete setup productCreate', deleteSetupResponse, 'productCreate');
  productIdsForCleanup.push(deleteProduct.id);
  const unrelatedSetupResponse = await runGraphql<ProductMutationData>(createDocument, unrelatedSetupVariables);
  const unrelatedProduct = requireProduct('unrelated setup productCreate', unrelatedSetupResponse, 'productCreate');
  productIdsForCleanup.push(unrelatedProduct.id);

  const savedSearchCreateVariables = {
    input: {
      resourceType: 'PRODUCT',
      name: `Overlay products ${stamp}`,
      query: `tag:${matchTag}`,
    },
  };
  const savedSearchCreateResponse = await runGraphql<SavedSearchMutationData>(
    savedSearchCreateDocument,
    savedSearchCreateVariables,
  );
  expectNoTopLevelErrors('product overlay savedSearchCreate', savedSearchCreateResponse);
  expectNoUserErrors(
    'product overlay savedSearchCreate',
    savedSearchCreateResponse.data?.savedSearchCreate?.userErrors,
  );
  const savedSearchId = savedSearchCreateResponse.data?.savedSearchCreate?.savedSearch?.id;
  if (typeof savedSearchId !== 'string') {
    throw new Error(`savedSearchCreate did not return an id: ${JSON.stringify(savedSearchCreateResponse, null, 2)}`);
  }
  savedSearchIdForCleanup = savedSearchId;

  const updateHydrateVariables = { ids: [updateProduct.id] };
  const updateHydrateResponse = await runGraphql(nodesHydrateDocument, updateHydrateVariables);
  expectNoTopLevelErrors('update product node hydrate', updateHydrateResponse);
  const deleteHydrateVariables = { ids: [deleteProduct.id] };
  const deleteHydrateResponse = await runGraphql(nodesHydrateDocument, deleteHydrateVariables);
  expectNoTopLevelErrors('delete product node hydrate', deleteHydrateResponse);
  const firstCatalogHydration = await captureCatalogHydration();
  const savedSearchHydrateVariables = { id: savedSearchId };
  const savedSearchHydrateResponse = await runGraphql(savedSearchHydrateDocument, savedSearchHydrateVariables);
  expectNoTopLevelErrors('product saved-search hydrate', savedSearchHydrateResponse);

  const createVariables = {
    product: {
      title: createdTitle,
      handle: `b-overlay-created-${stamp}`,
      status: 'ACTIVE',
      vendor: 'Conformance',
      productType: 'Overlay Staged',
      tags: [matchTag],
    },
  };
  const createResponse = await runGraphql<ProductMutationData>(createDocument, createVariables);
  const createdProduct = requireProduct('overlay productCreate', createResponse, 'productCreate');
  productIdsForCleanup.push(createdProduct.id);

  const updateVariables = {
    product: {
      id: updateProduct.id,
      title: updatedTitle,
      handle: `a-overlay-updated-${stamp}`,
      tags: [matchTag],
    },
  };
  const updateResponse = await runGraphql<ProductMutationData>(updateDocument, updateVariables);
  requireProduct('overlay productUpdate', updateResponse, 'productUpdate');

  const deleteVariables = { input: { id: deleteProduct.id } };
  const deleteResponse = await runGraphql<ProductMutationData>(deleteDocument, deleteVariables);
  expectNoTopLevelErrors('overlay productDelete', deleteResponse);
  expectNoUserErrors('overlay productDelete', deleteResponse.data?.productDelete?.userErrors);
  if (deleteResponse.data?.productDelete?.deletedProductId !== deleteProduct.id) {
    throw new Error(`productDelete did not delete the expected product: ${JSON.stringify(deleteResponse, null, 2)}`);
  }

  const readVariables = {
    matchQuery: `tag:${matchTag}`,
    deletedQuery: `tag:${deleteTag}`,
    controlQuery: `tag:${controlTag}`,
    savedSearchId,
  };
  const readResponse = await waitForOverlayRead(readVariables, [updatedTitle, createdTitle], unrelatedTitle);
  const pageCursor = readResponse.data?.firstPage?.pageInfo?.endCursor;
  if (typeof pageCursor !== 'string') {
    throw new Error(`First overlay page did not return an endCursor: ${JSON.stringify(readResponse, null, 2)}`);
  }
  const pageVariables = { matchQuery: `tag:${matchTag}`, after: pageCursor };
  const pageResponse = await runGraphql<OverlayReadData>(pageDocument, pageVariables);
  expectNoTopLevelErrors('product overlay second page', pageResponse);
  if (JSON.stringify(titles(pageResponse.data?.products)) !== JSON.stringify([createdTitle])) {
    throw new Error(`Second overlay page did not return the created product: ${JSON.stringify(pageResponse, null, 2)}`);
  }

  const upstreamCalls: UpstreamCall[] = [
    upstreamCall('ProductsHydrateNodes', nodesHydrateDocument, updateHydrateVariables, updateHydrateResponse),
    upstreamCall('ProductsHydrateNodes', nodesHydrateDocument, deleteHydrateVariables, deleteHydrateResponse),
    ...firstCatalogHydration.calls,
    upstreamCall(
      'DraftProxyProductSavedSearchHydration',
      savedSearchHydrateDocument,
      savedSearchHydrateVariables,
      savedSearchHydrateResponse,
    ),
  ];

  await mkdir(fixtureDir, { recursive: true });
  await writeFile(
    fixturePath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        setup: {
          updateProduct: { variables: updateSetupVariables, response: updateSetupResponse },
          deleteProduct: { variables: deleteSetupVariables, response: deleteSetupResponse },
          unrelatedProduct: { variables: unrelatedSetupVariables, response: unrelatedSetupResponse },
          savedSearchCreate: { variables: savedSearchCreateVariables, response: savedSearchCreateResponse },
        },
        hydration: {
          updateProduct: { variables: updateHydrateVariables, response: updateHydrateResponse },
          deleteProduct: { variables: deleteHydrateVariables, response: deleteHydrateResponse },
          catalogPages: firstCatalogHydration.responses.map(({ variables, response }) => ({
            variables,
            nodeCount: response.data?.products?.nodes?.length ?? 0,
            pageInfo: response.data?.products?.pageInfo ?? null,
          })),
          savedSearch: { variables: savedSearchHydrateVariables, response: savedSearchHydrateResponse },
        },
        mutations: {
          create: { variables: createVariables, response: createResponse },
          update: { variables: updateVariables, response: updateResponse },
          delete: { variables: deleteVariables, response: deleteResponse },
        },
        read: { variables: readVariables, response: readResponse },
        page: { variables: pageVariables, response: pageResponse },
        upstreamCalls,
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  const spec = {
    scenarioId: 'product-live-hybrid-overlay',
    operationNames: ['productCreate', 'productUpdate', 'productDelete', 'products', 'productsCount', 'node'],
    scenarioStatus: 'captured',
    assertionKinds: [
      'runtime-staging',
      'downstream-read-parity',
      'search-filter-semantics',
      'connection-window-parity',
      'count-precision-parity',
    ],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes/products_saved_searches.rs'],
    proxyRequest: {
      documentPath: requestPath('product-live-hybrid-overlay-create.graphql'),
      variablesCapturePath: '$.mutations.create.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'staged-create',
          capturePath: '$.mutations.create.response.data.productCreate',
          proxyPath: '$.data.productCreate',
          selectedPaths: [
            '$.product.title',
            '$.product.handle',
            '$.product.status',
            '$.product.vendor',
            '$.product.productType',
            '$.product.tags',
            '$.product.descriptionHtml',
            '$.product.templateSuffix',
            '$.userErrors',
          ],
        },
        {
          name: 'staged-update-wins-by-id',
          capturePath: '$.mutations.update.response.data.productUpdate',
          proxyPath: '$.data.productUpdate',
          preserveProxyState: true,
          proxyRequest: {
            documentPath: requestPath('productUpdate-parity-plan.graphql'),
            variablesCapturePath: '$.mutations.update.variables',
            apiVersion,
          },
          selectedPaths: ['$.product.title', '$.product.handle', '$.product.status', '$.product.tags', '$.userErrors'],
        },
        {
          name: 'staged-delete-tombstone',
          capturePath: '$.mutations.delete.response.data.productDelete',
          proxyPath: '$.data.productDelete',
          preserveProxyState: true,
          proxyRequest: {
            documentPath: requestPath('product-live-hybrid-overlay-delete.graphql'),
            variablesCapturePath: '$.mutations.delete.variables',
            apiVersion,
          },
          selectedPaths: ['$.deletedProductId', '$.userErrors'],
        },
        {
          name: 'effective-catalog-search-sort-reverse-saved-search-count-and-unrelated-retention',
          capturePath: '$.read.response.data',
          proxyPath: '$.data',
          preserveProxyState: true,
          proxyRequest: {
            documentPath: requestPath('product-live-hybrid-overlay-read.graphql'),
            variablesCapturePath: '$.read.variables',
            apiVersion,
          },
          selectedPaths: [
            '$.catalog.nodes',
            '$.reverse.nodes',
            '$.firstPage.nodes',
            '$.firstPage.pageInfo.hasNextPage',
            '$.firstPage.pageInfo.hasPreviousPage',
            '$.savedSearch.nodes',
            '$.deleted.nodes',
            '$.unrelated.nodes',
            '$.matches',
            '$.limitedTotal',
          ],
        },
        {
          name: 'effective-catalog-cursor-second-page',
          capturePath: '$.page.response.data',
          proxyPath: '$.data',
          preserveProxyState: true,
          proxyRequest: {
            documentPath: requestPath('product-live-hybrid-overlay-page.graphql'),
            apiVersion,
            variables: {
              matchQuery: { fromCapturePath: '$.page.variables.matchQuery' },
              after: {
                fromProxyResponse: 'effective-catalog-search-sort-reverse-saved-search-count-and-unrelated-retention',
                path: '$.data.firstPage.pageInfo.endCursor',
              },
            },
          },
          selectedPaths: ['$.products.nodes', '$.products.pageInfo.hasNextPage', '$.products.pageInfo.hasPreviousPage'],
        },
      ],
    },
    notes:
      'Live Shopify capture records the exact pre-mutation product catalog, product-node, and PRODUCT saved-search hydrates used by LiveHybrid. Public productCreate/productUpdate/productDelete requests are replayed locally before strict effective-catalog comparisons cover create/update/delete precedence, unrelated retention, search, title sort, reverse, cursor windowing, saved searches, count deltas, and limited-count precision.',
  };
  await mkdir(path.dirname(specPath), { recursive: true });
  await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- capture scripts report written evidence paths.
  console.log(JSON.stringify({ ok: true, fixturePath, specPath, upstreamCalls: upstreamCalls.length }, null, 2));
} finally {
  if (savedSearchIdForCleanup !== null) {
    try {
      await runGraphql<SavedSearchMutationData>(savedSearchDeleteDocument, {
        input: { id: savedSearchIdForCleanup },
      });
    } catch {
      // Best-effort cleanup preserves the original capture failure.
    }
  }
  for (const productId of productIdsForCleanup.reverse()) {
    try {
      await runGraphql<ProductMutationData>(deleteDocument, { input: { id: productId } });
    } catch {
      // Best-effort cleanup preserves the original capture failure.
    }
  }
}
