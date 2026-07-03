/* oxlint-disable no-console -- CLI recorder intentionally writes capture status to stdout. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const requestDir = path.join('config', 'parity-requests', 'products');
const specDir = path.join('config', 'parity-specs', 'products');

const fixturePath = path.join(outputDir, 'productCreate-dropped-inputs-parity.json');
const createDocumentPath = path.join(requestDir, 'productCreate-dropped-inputs-parity.graphql');
const readDocumentPath = path.join(requestDir, 'productCreate-dropped-inputs-downstream-read.graphql');
const specPath = path.join(specDir, 'productCreate-dropped-inputs-parity.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateDocument = `mutation ProductCreateDroppedInputsParity($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      title
      isGiftCard
      giftCardTemplateSuffix
      variants(first: 1) {
        nodes {
          id
          taxable
          inventoryItem {
            id
            requiresShipping
          }
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

const productCreateReadDocument = `query ProductCreateDroppedInputsDownstreamRead($id: ID!) {
  product(id: $id) {
    id
    title
    isGiftCard
    giftCardTemplateSuffix
    metafield(namespace: "custom", key: "occasion") {
      namespace
      key
      type
      value
      ownerType
    }
    metafields(first: 5) {
      nodes {
        namespace
        key
        type
        value
        ownerType
      }
    }
    variants(first: 1) {
      nodes {
        id
        taxable
        inventoryItem {
          id
          requiresShipping
        }
      }
    }
  }
}
`;

const productCreateWithPublicationDocument = `mutation ProductCreatePublicationsPublicSchemaProbe($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      title
      resourcePublicationsCount {
        count
        precision
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const publicationsQuery = `query ProductCreatePublicationTargetsHydrate($first: Int!) {
  publications(first: $first) {
    nodes {
      id
      name
    }
  }
}
`;

const productDeleteDocument = `mutation ProductCreateDroppedInputsCleanup($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}
`;

async function capture(query: string, variables: Record<string, unknown>): Promise<CaptureEntry> {
  const result: ConformanceGraphqlResult = await runGraphqlRequest(query, variables);
  return {
    query,
    variables,
    response: {
      status: result.status,
      payload: result.payload,
    },
  };
}

function readRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    current = readRecord(current)?.[part];
  }
  return current;
}

function readStringPath(value: unknown, pathParts: string[], context: string): string {
  const found = readPath(value, pathParts);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`Expected ${context} string at ${pathParts.join('.')}: ${JSON.stringify(value)}`);
  }
  return found;
}

function assertNoTopLevelErrors(entry: CaptureEntry, context: string): void {
  const payload = readRecord(entry.response.payload);
  if (entry.response.status < 200 || entry.response.status >= 300 || payload?.['errors'] !== undefined) {
    throw new Error(`${context} returned top-level errors: ${JSON.stringify(entry.response.payload)}`);
  }
}

function assertNoUserErrors(entry: CaptureEntry, root: string, context: string): void {
  assertNoTopLevelErrors(entry, context);
  const userErrors = readPath(entry.response.payload, ['data', root, 'userErrors']);
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(entry.response.payload)}`);
  }
}

function productIdFromCreate(entry: CaptureEntry): string {
  return readStringPath(entry.response.payload, ['data', 'productCreate', 'product', 'id'], 'productCreate id');
}

function firstPublicationId(entry: CaptureEntry): string {
  const nodes = readPath(entry.response.payload, ['data', 'publications', 'nodes']);
  if (!Array.isArray(nodes)) {
    throw new Error(`Publication probe did not return nodes: ${JSON.stringify(entry.response.payload)}`);
  }
  for (const node of nodes) {
    const id = readRecord(node)?.['id'];
    if (typeof id === 'string' && id.length > 0) return id;
  }
  throw new Error(`Publication probe returned no usable publication id: ${JSON.stringify(entry.response.payload)}`);
}

async function cleanupProduct(productId: string | null): Promise<CaptureEntry | null> {
  if (productId === null) return null;
  return capture(productDeleteDocument, { input: { id: productId } });
}

function productIdDifferences(root: string): Record<string, string>[] {
  return [
    {
      path: `${root}.id`,
      matcher: 'shopify-gid:Product',
      reason: 'Shopify and the local staging registry allocate product ids independently.',
    },
  ];
}

function variantIdDifferences(root: string): Record<string, string>[] {
  return [
    {
      path: `${root}.variants.nodes[0].id`,
      matcher: 'shopify-gid:ProductVariant',
      reason: 'Shopify and the local staging registry allocate default variant ids independently.',
    },
    {
      path: `${root}.variants.nodes[0].inventoryItem.id`,
      matcher: 'shopify-gid:InventoryItem',
      reason: 'Shopify and the local staging registry allocate inventory item ids independently.',
    },
  ];
}

function spec(): Record<string, unknown> {
  const createProductDiffs = [
    ...productIdDifferences('$.productCreate.product'),
    ...variantIdDifferences('$.productCreate.product'),
  ];
  return {
    scenarioId: 'productCreate-dropped-inputs-parity',
    operationNames: ['productCreate'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'user-errors-parity', 'downstream-read-parity'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes/products_saved_searches.rs'],
    proxyRequest: {
      documentPath: createDocumentPath,
      apiVersion,
      variablesCapturePath: '$.giftCardAndMetafields.mutation.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Captured Shopify productCreate behavior for giftCard, giftCardTemplateSuffix, claimOwnership.bundles, and product metafields with immediate product readback. The same live fixture records that the public 2025-01 schema on this conformance store rejects create-time productPublications/publications input fields before resolver execution, so that internal/Core-only branch is covered by focused local runtime tests instead of fabricated live parity.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: createProductDiffs,
      targets: [
        {
          name: 'gift-card-metafields mutation data',
          capturePath: '$.giftCardAndMetafields.mutation.response.payload.data',
          proxyPath: '$.data',
        },
        {
          name: 'gift-card-metafields downstream read',
          capturePath: '$.giftCardAndMetafields.downstreamRead.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: readDocumentPath,
            apiVersion,
            variables: {
              id: {
                fromPrimaryProxyPath: '$.data.productCreate.product.id',
              },
            },
          },
          expectedDifferences: [...productIdDifferences('$.product'), ...variantIdDifferences('$.product')],
        },
      ],
    },
  };
}

await mkdir(outputDir, { recursive: true });
await mkdir(requestDir, { recursive: true });
await mkdir(specDir, { recursive: true });

const runId = `${Date.now()}`;
let giftCardProductId: string | null = null;

try {
  const giftCardVariables = {
    product: {
      title: `Hermes Gift Card Product ${runId}`,
      status: 'DRAFT',
      giftCard: true,
      giftCardTemplateSuffix: 'birthday',
      claimOwnership: { bundles: true },
      metafields: [
        {
          namespace: 'custom',
          key: 'occasion',
          type: 'single_line_text_field',
          value: 'birthday',
        },
        {
          namespace: 'custom',
          key: 'audience',
          type: 'single_line_text_field',
          value: 'friend',
        },
      ],
    },
  };
  const giftCardMutation = await capture(productCreateDocument, giftCardVariables);
  assertNoUserErrors(giftCardMutation, 'productCreate', 'gift card and metafields productCreate');
  giftCardProductId = productIdFromCreate(giftCardMutation);
  const giftCardDownstreamRead = await capture(productCreateReadDocument, { id: giftCardProductId });
  assertNoTopLevelErrors(giftCardDownstreamRead, 'gift card downstream read');

  const publicationCatalog = await capture(publicationsQuery, { first: 50 });
  assertNoTopLevelErrors(publicationCatalog, 'publication catalog read');
  const publicationId = firstPublicationId(publicationCatalog);
  const productPublicationsSchemaProbe = await capture(productCreateWithPublicationDocument, {
    product: {
      title: `Hermes Published Product ${runId}`,
      status: 'DRAFT',
      productPublications: [{ publicationId }],
    },
  });
  const publicationsSchemaProbe = await capture(productCreateWithPublicationDocument, {
    product: {
      title: `Hermes Publications Alias Product ${runId}`,
      status: 'DRAFT',
      publications: [{ publicationId }],
    },
  });

  await writeFile(
    fixturePath,
    `${JSON.stringify(
      {
        scenarioId: 'productCreate-dropped-inputs-parity',
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        giftCardAndMetafields: {
          mutation: giftCardMutation,
          downstreamRead: giftCardDownstreamRead,
        },
        productPublications: {
          publicationCatalog,
          publicationId,
          productPublicationsSchemaProbe,
          publicationsSchemaProbe,
        },
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );

  await writeFile(createDocumentPath, productCreateDocument);
  await writeFile(readDocumentPath, productCreateReadDocument);
  await writeFile(specPath, `${JSON.stringify(spec(), null, 2)}\n`);

  console.log(
    JSON.stringify(
      {
        ok: true,
        fixtureFiles: [fixturePath],
        specFiles: [specPath],
        requestFiles: [createDocumentPath, readDocumentPath],
      },
      null,
      2,
    ),
  );
} finally {
  const cleanup = {
    giftCardProduct: await cleanupProduct(giftCardProductId),
  };
  console.log(`Cleanup: ${JSON.stringify(cleanup)}`);
}
