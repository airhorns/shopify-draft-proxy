import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type ProductSummary = { id: string; title: string; handle: string; status?: string };

type ProductCreateData = {
  productCreate?: { product?: ProductSummary | null; userErrors?: Array<{ field?: string[] | null; message?: string | null }> } | null;
};
type ProductUpdateData = {
  productUpdate?: { product?: ProductSummary | null; userErrors?: Array<{ field?: string[] | null; message?: string | null }> } | null;
};
type ProductDeleteData = {
  productDelete?: { deletedProductId?: string | null; userErrors?: Array<{ field?: string[] | null; message?: string | null }> } | null;
};
type CatalogReadData = {
  productsCount?: { count?: number | null; precision?: string | null } | null;
  products?: { nodes?: Array<{ id?: string | null; title?: string | null; handle?: string | null; status?: string | null } | null> | null } | null;
};

type UpstreamCall = {
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: { status: number; body: ConformanceGraphqlPayload };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'staged-catalog-write-overlay-read.json');

const hydrateDocPath = 'config/parity-requests/products/products-hydrate-nodes-observation.graphql';
const createDocPath = 'config/parity-requests/products/staged-catalog-write-create-product.graphql';
const updateDocPath = 'config/parity-requests/products/staged-catalog-write-update-product.graphql';
const deleteDocPath = 'config/parity-requests/products/staged-catalog-write-delete-product.graphql';
const catalogReadDocPath = 'config/parity-requests/products/staged-catalog-write-catalog-read.graphql';
const catalogNodesReadDocPath = 'config/parity-requests/products/staged-catalog-write-catalog-read-after-delete.graphql';
const catalogNodesReadAfterUpdateDocPath = 'config/parity-requests/products/staged-catalog-write-catalog-read-after-update.graphql';

const hydrateDocument = await readFile(hydrateDocPath, 'utf8');
const createDocument = await readFile(createDocPath, 'utf8');
const updateDocument = await readFile(updateDocPath, 'utf8');
const deleteDocument = await readFile(deleteDocPath, 'utf8');
const catalogReadDocument = await readFile(catalogReadDocPath, 'utf8');
const catalogNodesReadDocument = await readFile(catalogNodesReadDocPath, 'utf8');
const catalogNodesReadAfterUpdateDocument = await readFile(catalogNodesReadAfterUpdateDocPath, 'utf8');

const { runGraphql } = createAdminGraphqlClient({ adminOrigin, apiVersion, headers: buildAdminAuthHeaders(adminAccessToken) });

const stamp = `${Date.now()}`;
const createdProductIds: string[] = [];

function expectNoUserErrors(label: string, userErrors: Array<{ field?: string[] | null; message?: string | null }> | null | undefined): void {
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}
function requireProduct(label: string, payload: ConformanceGraphqlPayload): ProductSummary {
  const data = payload.data as { productCreate?: { product?: ProductSummary | null } | null; productUpdate?: { product?: ProductSummary | null } | null } | undefined;
  const product = data?.productCreate?.product ?? data?.productUpdate?.product;
  if (!product?.id || !product.title || !product.handle) throw new Error(`${label} did not return a complete product: ${JSON.stringify(payload, null, 2)}`);
  return product;
}
function recordUpstreamCall(operationName: string, query: string, variables: Record<string, unknown>, body: ConformanceGraphqlPayload): UpstreamCall {
  return { operationName, variables, query, response: { status: 200, body } };
}
async function sleep(ms: number): Promise<void> { await new Promise<void>((r) => setTimeout(r, ms)); }

async function waitForCatalog(expectedProductId: string, document: string, variables: Record<string, unknown>): Promise<ConformanceGraphqlPayload<CatalogReadData>> {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const response = await runGraphql<CatalogReadData>(document, variables);
    const nodes = response.data?.products?.nodes ?? [];
    if (nodes.some((node) => node?.id === expectedProductId)) return response;
    await sleep(3000);
  }
  throw new Error(`Timed out waiting for product ${expectedProductId} to appear in catalog`);
}
async function waitForCatalogAbsence(expectedAbsentProductId: string, document: string, variables: Record<string, unknown>): Promise<ConformanceGraphqlPayload<CatalogReadData>> {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const response = await runGraphql<CatalogReadData>(document, variables);
    const nodes = response.data?.products?.nodes ?? [];
    if (!nodes.some((node) => node?.id === expectedAbsentProductId)) return response;
    await sleep(3000);
  }
  throw new Error(`Timed out waiting for product ${expectedAbsentProductId} to disappear from catalog`);
}
async function waitForUpdatedTitle(expectedProductId: string, expectedTitle: string, document: string, variables: Record<string, unknown>): Promise<ConformanceGraphqlPayload<CatalogReadData>> {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const response = await runGraphql<CatalogReadData>(document, variables);
    const nodes = response.data?.products?.nodes ?? [];
    const found = nodes.find((node) => node?.id === expectedProductId);
    if (found && found.title === expectedTitle) return response;
    await sleep(3000);
  }
  throw new Error(`Timed out waiting for product ${expectedProductId} title to become "${expectedTitle}"`);
}

try {
  const catalogVariables = { first: 50 };

  // Phase 0: Baseline catalog (before the product exists) — upstream cassette
  // for the create scenario. The proxy overlays the synthetic staged create on
  // top of this baseline; the comparison target is the catalog with the product.
  const baselineCatalog = await runGraphql<CatalogReadData>(catalogReadDocument, catalogVariables);

  // Phase 1: Create the product.
  const createVariables = { product: { title: `Staged Catalog Write ${stamp}`, status: 'ACTIVE', vendor: 'Conformance', productType: 'Overlay' } };
  const create = await runGraphql<ProductCreateData>(createDocument, createVariables);
  expectNoUserErrors('productCreate', create.data?.productCreate?.userErrors);
  const createdProduct = requireProduct('productCreate', create);
  createdProductIds.push(createdProduct.id);

  // Catalog with the created product present — comparison target for the create
  // scenario, and upstream cassette for the update scenario (the product has
  // its original title here, so the proxy's staged update overlay is meaningful).
  const catalogAfterCreate = await waitForCatalog(createdProduct.id, catalogReadDocument, catalogVariables);

  // Phase 2: Update the product title.
  const updatedTitle = `Staged Catalog Write Updated ${stamp}`;
  const updateVariables = { product: { id: createdProduct.id, title: updatedTitle } };
  const update = await runGraphql<ProductUpdateData>(updateDocument, updateVariables);
  expectNoUserErrors('productUpdate', update.data?.productUpdate?.userErrors);

  // Catalog with the updated title — comparison target for the update scenario,
  // and upstream cassette for the delete scenario (the product is still present,
  // so the proxy's staged delete overlay is meaningful).
  const catalogAfterUpdate = await waitForUpdatedTitle(createdProduct.id, updatedTitle, catalogNodesReadAfterUpdateDocument, catalogVariables);

  // Hydrate the product (the proxy does this before productUpdate/productDelete
  // on a product it hasn't observed).
  const hydrateVariables = { ids: [createdProduct.id] };
  const hydrate = await runGraphql(hydrateDocument, hydrateVariables);

  // Phase 3: Delete the product.
  const deleteVariables = { input: { id: createdProduct.id } };
  const del = await runGraphql<ProductDeleteData>(deleteDocument, deleteVariables);
  expectNoUserErrors('productDelete', del.data?.productDelete?.userErrors);

  // Catalog without the product — comparison target for the delete scenario.
  const catalogAfterDelete = await waitForCatalogAbsence(createdProduct.id, catalogNodesReadDocument, catalogVariables);

  // Build the upstreamCalls cassette. Each entry's query must be the exact
  // GraphQL document text the proxy sends upstream.
  const upstreamCalls = [
    // Hydrate call the proxy makes before productUpdate/productDelete.
    recordUpstreamCall('ProductsHydrateNodes', hydrateDocument, hydrateVariables, hydrate),
    // Create scenario: proxy forwards the baseline (without the product) and
    // overlays the synthetic staged create.
    recordUpstreamCall('StagedCatalogWriteCatalogRead', catalogReadDocument, catalogVariables, baselineCatalog),
    // Update scenario: proxy forwards the post-create catalog (original title)
    // and overlays the staged update (fresh title).
    recordUpstreamCall('StagedCatalogWriteCatalogReadAfterUpdate', catalogNodesReadAfterUpdateDocument, catalogVariables, catalogAfterCreate),
    // Delete scenario: proxy forwards the post-update catalog (product present)
    // and drops the tombstoned row.
    recordUpstreamCall('StagedCatalogWriteCatalogReadAfterDelete', catalogNodesReadDocument, catalogVariables, catalogAfterUpdate),
  ];

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId: 'staged-catalog-write-overlay-read',
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        createdProduct,
        updatedTitle,
        create: { query: createDocument, variables: createVariables, response: create },
        catalogAfterCreate: { query: catalogReadDocument, variables: catalogVariables, response: catalogAfterCreate },
        update: { query: updateDocument, variables: updateVariables, response: update },
        catalogAfterUpdate: { query: catalogNodesReadAfterUpdateDocument, variables: catalogVariables, response: catalogAfterUpdate },
        del: { query: deleteDocument, variables: deleteVariables, response: del },
        catalogAfterDelete: { query: catalogNodesReadDocument, variables: catalogVariables, response: catalogAfterDelete },
        upstreamCalls,
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  // oxlint-disable-next-line no-console -- capture scripts report their written artifact.
  console.log(JSON.stringify({ ok: true, outputPath, createdProductId: createdProduct.id, createdProductTitle: createdProduct.title, updatedTitle }, null, 2));
} finally {
  for (const productId of createdProductIds.reverse()) {
    try { await runGraphql(deleteDocument, { input: { id: productId } }); } catch { /* best-effort */ }
  }
}
