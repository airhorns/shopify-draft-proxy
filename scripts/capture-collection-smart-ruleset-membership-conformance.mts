/* oxlint-disable no-console -- CLI capture scripts intentionally report written artifacts. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlPayload = Record<string, unknown>;
type JsonPathPart = string | number;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productsDir = path.join('config', 'parity-requests', 'products');
const specsDir = path.join('config', 'parity-specs', 'products');
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');

const productSetDocumentPath = path.join(productsDir, 'collection-smart-ruleset-membership-product-set.graphql');
const createDocumentPath = path.join(productsDir, 'collection-smart-ruleset-membership-create.graphql');
const readDocumentPath = path.join(productsDir, 'collection-smart-ruleset-membership-read.graphql');
const specPath = path.join(specsDir, 'collection-smart-ruleset-membership-parity.json');
const fixturePath = path.join(fixtureDir, 'collection-smart-ruleset-membership-parity.json');

const productSetDocument = `mutation CollectionSmartRuleSetMembershipProductSet($input: ProductSetInput!, $synchronous: Boolean!) {
  productSet(input: $input, synchronous: $synchronous) {
    product {
      id
      title
      handle
      status
      vendor
      productType
      tags
      variants(first: 10) {
        nodes {
          id
          price
        }
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

const createDocument = `mutation CollectionSmartRuleSetMembershipCreate($input: CollectionInput!) {
  collectionCreate(input: $input) {
    collection {
      id
      title
      handle
      sortOrder
      ruleSet {
        appliedDisjunctively
        rules {
          column
          relation
          condition
        }
      }
      productsCount {
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

const readDocument = `query CollectionSmartRuleSetMembershipRead($id: ID!) {
  collection(id: $id) {
    id
    title
    handle
    productsCount {
      count
      precision
    }
    products(first: 10, sortKey: TITLE) {
      nodes {
        id
        title
        vendor
        productType
        tags
        variants(first: 10) {
          nodes {
            price
          }
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
}
`;

const collectionDeleteDocument = `#graphql
mutation CollectionSmartRuleSetMembershipCollectionCleanup($input: CollectionDeleteInput!) {
  collectionDelete(input: $input) {
    deletedCollectionId
    userErrors {
      field
      message
    }
  }
}
`;

const productDeleteDocument = `#graphql
mutation CollectionSmartRuleSetMembershipProductCleanup($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}
`;

function readPath(value: unknown, pathParts: JsonPathPart[]): unknown {
  let cursor = value;
  for (const part of pathParts) {
    if (typeof part === 'number' && Array.isArray(cursor)) {
      cursor = cursor[part];
    } else if (typeof part === 'string' && typeof cursor === 'object' && cursor !== null) {
      cursor = (cursor as Record<string, unknown>)[part];
    } else {
      return undefined;
    }
  }
  return cursor;
}

function requireString(value: unknown, label: string): string {
  if (typeof value === 'string' && value.length > 0) return value;
  throw new Error(`Missing ${label}: ${JSON.stringify(value)}`);
}

function assertNoUserErrors(payload: GraphqlPayload, root: string, label: string): void {
  const userErrors = readPath(payload, ['data', root, 'userErrors']);
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

async function runGraphqlPayload(query: string, variables?: Record<string, unknown>): Promise<GraphqlPayload> {
  try {
    return (await runGraphql(query, variables)) as GraphqlPayload;
  } catch (error) {
    const payload = (error as { result?: { payload?: GraphqlPayload } }).result?.payload;
    if (payload) return payload;
    throw error;
  }
}

async function cleanupCollection(id: string | null): Promise<void> {
  if (id === null) return;
  try {
    await runGraphqlPayload(collectionDeleteDocument, { input: { id } });
  } catch {
    // Best-effort cleanup only; preserve the original capture result.
  }
}

async function cleanupProduct(id: string | null): Promise<void> {
  if (id === null) return;
  try {
    await runGraphqlPayload(productDeleteDocument, { input: { id } });
  } catch {
    // Best-effort cleanup only; preserve the original capture result.
  }
}

async function waitForMembership(
  collectionId: string,
  productTitle: string,
): Promise<{
  variables: Record<string, unknown>;
  response: GraphqlPayload;
  attempts: number;
}> {
  const variables = { id: collectionId };
  let lastResponse: GraphqlPayload | null = null;
  for (let attempt = 1; attempt <= 30; attempt += 1) {
    const response = await runGraphqlPayload(readDocument, variables);
    lastResponse = response;
    const count = readPath(response, ['data', 'collection', 'productsCount', 'count']);
    const nodes = readPath(response, ['data', 'collection', 'products', 'nodes']);
    const hasProduct = Array.isArray(nodes) && nodes.some((node) => readPath(node, ['title']) === productTitle);
    if (count === 1 && hasProduct) {
      return { variables, response, attempts: attempt };
    }
    await delay(2_000);
  }
  throw new Error(`Timed out waiting for smart collection membership: ${JSON.stringify(lastResponse)}`);
}

await mkdir(productsDir, { recursive: true });
await mkdir(specsDir, { recursive: true });
await mkdir(fixtureDir, { recursive: true });

const runId = `${Date.now()}`;
const productTitle = `Smart RuleSet Product ${runId}`;
const productVendor = 'Hermes Smart Rule Vendor';
const productType = 'Smart Rule Shirt';
const productTag = `smart-ruleset-${runId}`;
let productId: string | null = null;
let collectionId: string | null = null;

try {
  const productSetVariables = {
    synchronous: true,
    input: {
      title: productTitle,
      status: 'ACTIVE',
      vendor: productVendor,
      productType,
      tags: [productTag],
      productOptions: [{ name: 'Color', values: [{ name: 'Blue' }] }],
      variants: [
        {
          optionValues: [{ optionName: 'Color', name: 'Blue' }],
          price: '7.50',
          inventoryItem: { tracked: false, requiresShipping: true },
        },
      ],
    },
  };
  const productSet = await runGraphqlPayload(productSetDocument, productSetVariables);
  assertNoUserErrors(productSet, 'productSet', 'productSet setup');
  productId = requireString(readPath(productSet, ['data', 'productSet', 'product', 'id']), 'productSet.product.id');

  const smartCreateVariables = {
    input: {
      title: `Smart RuleSet Collection ${runId}`,
      ruleSet: {
        appliedDisjunctively: false,
        rules: [
          { column: 'TITLE', relation: 'CONTAINS', condition: `Product ${runId}` },
          { column: 'TYPE', relation: 'EQUALS', condition: productType },
          { column: 'VENDOR', relation: 'EQUALS', condition: productVendor },
          { column: 'TAG', relation: 'EQUALS', condition: productTag },
          { column: 'VARIANT_PRICE', relation: 'LESS_THAN', condition: '10' },
        ],
      },
    },
  };
  const smartCreate = await runGraphqlPayload(createDocument, smartCreateVariables);
  assertNoUserErrors(smartCreate, 'collectionCreate', 'collectionCreate smart ruleSet setup');
  collectionId = requireString(
    readPath(smartCreate, ['data', 'collectionCreate', 'collection', 'id']),
    'collectionCreate.collection.id',
  );

  const downstreamRead = await waitForMembership(collectionId, productTitle);

  const fixture = {
    storeDomain,
    apiVersion,
    productSet: { variables: productSetVariables, response: productSet },
    smartCreate: { variables: smartCreateVariables, response: smartCreate },
    downstreamRead,
    upstreamCalls: [],
  };

  const spec = {
    scenarioId: 'collection-smart-ruleset-membership-parity',
    operationNames: ['productSet', 'collectionCreate', 'collection'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'downstream-read-parity', 'rule-set-membership'],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: productSetDocumentPath,
      variablesCapturePath: '$.productSet.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Captured against a disposable product and smart collection. The scenario proves a smart collection with TITLE, TYPE, VENDOR, TAG, and VARIANT_PRICE rules reports matching products/productsCount on downstream collection reads; setup and replay use public GraphQL mutations only.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [
        {
          path: '$.productSet.product.id',
          matcher: 'shopify-gid:Product',
          reason: 'Shopify and the proxy allocate setup product ids independently.',
        },
        {
          path: '$.productSet.product.variants.nodes[0].id',
          matcher: 'shopify-gid:ProductVariant',
          reason: 'Shopify and the proxy allocate setup variant ids independently.',
        },
        {
          path: '$.collectionCreate.collection.id',
          matcher: 'shopify-gid:Collection',
          reason: 'Shopify and the proxy allocate smart collection ids independently.',
        },
        {
          path: '$.collection.id',
          matcher: 'shopify-gid:Collection',
          reason: 'The downstream read uses the collection id allocated by each replay.',
        },
        {
          path: '$.collection.products.nodes[0].id',
          matcher: 'shopify-gid:Product',
          reason: 'The downstream smart collection member is the product allocated by each replay.',
        },
      ],
      targets: [
        {
          name: 'product-set-setup',
          capturePath: '$.productSet.response.data',
          proxyPath: '$.data',
        },
        {
          name: 'smart-create',
          capturePath: '$.smartCreate.response.data',
          proxyRequest: {
            documentPath: createDocumentPath,
            variablesCapturePath: '$.smartCreate.variables',
            apiVersion,
          },
          proxyPath: '$.data',
        },
        {
          name: 'smart-downstream-read',
          capturePath: '$.downstreamRead.response.data',
          proxyRequest: {
            documentPath: readDocumentPath,
            apiVersion,
            variables: {
              id: {
                fromProxyResponse: 'smart-create',
                path: '$.data.collectionCreate.collection.id',
              },
            },
          },
          proxyPath: '$.data',
        },
      ],
    },
  };

  await writeFile(productSetDocumentPath, productSetDocument, 'utf8');
  await writeFile(createDocumentPath, createDocument, 'utf8');
  await writeFile(readDocumentPath, readDocument, 'utf8');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        fixturePath,
        specPath,
        requestFiles: [productSetDocumentPath, createDocumentPath, readDocumentPath],
        attempts: downstreamRead.attempts,
      },
      null,
      2,
    ),
  );
} finally {
  await cleanupCollection(collectionId);
  await cleanupProduct(productId);
}
