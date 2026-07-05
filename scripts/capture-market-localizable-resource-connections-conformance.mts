/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const definitionCreateMutation = `#graphql
  mutation MarketLocalizableConnectionsDefinitionCreate($definition: MetafieldDefinitionInput!) {
    metafieldDefinitionCreate(definition: $definition) {
      createdDefinition {
        id
        namespace
        key
        ownerType
        type {
          name
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

const definitionDeleteMutation = `#graphql
  mutation MarketLocalizableConnectionsDefinitionDelete($id: ID!, $deleteAllAssociatedMetafields: Boolean!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: $deleteAllAssociatedMetafields) {
      deletedDefinitionId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const productCreateMutation = `#graphql
  mutation MarketLocalizableConnectionsProductCreate($product: ProductCreateInput!, $namespace: String!) {
    productCreate(product: $product) {
      product {
        id
        handle
        title
        status
        metafields(first: 5, namespace: $namespace) {
          nodes {
            id
            namespace
            key
            type
            value
            compareDigest
            ownerType
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

const productReadQuery = `#graphql
  query MarketLocalizableConnectionsProductRead($id: ID!, $namespace: String!) {
    product(id: $id) {
      id
      handle
      title
      status
      metafields(first: 5, namespace: $namespace) {
        nodes {
          id
          namespace
          key
          type
          value
          compareDigest
          ownerType
        }
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation MarketLocalizableConnectionsProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function randomSuffix(): string {
  return Math.random().toString(36).slice(2, 10);
}

async function readDocument(relativePath: string): Promise<string> {
  return await readFile(relativePath, 'utf8');
}

function firstUserErrorMessage(response: unknown): string | null {
  if (typeof response !== 'object' || response === null) return null;
  const errors = (response as { userErrors?: unknown }).userErrors;
  if (!Array.isArray(errors) || errors.length === 0) return null;
  const first = errors[0];
  return typeof first === 'object' && first !== null && 'message' in first
    ? String((first as { message?: unknown }).message)
    : null;
}

function assertNoUserErrors(payload: unknown, root: string, label: string): void {
  const data = payload && typeof payload === 'object' ? (payload as { data?: unknown }).data : null;
  const rootPayload = data && typeof data === 'object' ? (data as Record<string, unknown>)[root] : null;
  const userErrors =
    rootPayload && typeof rootPayload === 'object' ? (rootPayload as { userErrors?: unknown }).userErrors : null;
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const singularReadQuery = await readDocument(
  'config/parity-requests/markets/market-localization-money-metafield-read.graphql',
);
const connectionsReadQuery = await readDocument(
  'config/parity-requests/markets/market-localizable-resource-connections-read.graphql',
);

const captureToken = randomSuffix();
const namespace = `codex_localizable_${captureToken}`;
const key = 'money';
const missingMetafieldResourceId = 'gid://shopify/Metafield/999999999999';
const productInput = {
  title: `Market localizable connections ${captureToken}`,
  handle: `market-localizable-connections-${captureToken}`,
  status: 'DRAFT',
  metafields: [
    {
      namespace,
      key,
      type: 'money',
      value: JSON.stringify({ amount: '5.99', currency_code: 'CAD' }),
    },
  ],
};

let createdDefinitionId: string | null = null;
let createdProductId: string | null = null;
let cleanupProductResponse: unknown = null;
let cleanupDefinitionResponse: unknown = null;

try {
  const definitionCreate = await runGraphql(definitionCreateMutation, {
    definition: {
      name: `Market localizable connections ${captureToken}`,
      namespace,
      key,
      ownerType: 'PRODUCT',
      type: 'money',
      description: 'Disposable market-localizable connection capture.',
    },
  });
  const definitionPayload = definitionCreate.data?.metafieldDefinitionCreate;
  createdDefinitionId =
    typeof definitionPayload?.createdDefinition?.id === 'string' ? definitionPayload.createdDefinition.id : null;
  if (!createdDefinitionId) {
    throw new Error(`Definition setup failed: ${firstUserErrorMessage(definitionPayload) ?? 'missing definition id'}`);
  }
  assertNoUserErrors(definitionCreate, 'metafieldDefinitionCreate', 'definition setup');

  const productCreate = await runGraphql(productCreateMutation, { product: productInput, namespace });
  const productPayload = productCreate.data?.productCreate;
  const product = productPayload?.product;
  createdProductId = typeof product?.id === 'string' ? product.id : null;
  if (!createdProductId) {
    throw new Error(`Product setup failed: ${firstUserErrorMessage(productPayload) ?? 'missing product id'}`);
  }
  assertNoUserErrors(productCreate, 'productCreate', 'product setup');

  const productRead = await runGraphql(productReadQuery, { id: createdProductId, namespace });
  const seedProduct = productRead.data?.product;
  const metafield = seedProduct?.metafields?.nodes?.[0];
  const resourceId = typeof metafield?.id === 'string' ? metafield.id : null;
  if (!resourceId) {
    throw new Error('Product metafield setup did not return a metafield id.');
  }

  const marketProbe = await runGraphql(singularReadQuery, {
    resourceId,
    marketId: 'gid://shopify/Market/0',
    marketsFirst: 1,
  });
  const market = marketProbe.data?.markets?.nodes?.[0];
  const marketId = typeof market?.id === 'string' ? market.id : null;
  if (!marketId) {
    throw new Error('Market probe failed: markets(first: 1) returned no market.');
  }

  const singularVariables = { resourceId, marketId, marketsFirst: 1 };
  const singularRead = await runGraphql(singularReadQuery, singularVariables);
  const connectionVariables = {
    resourceId,
    resourceIds: [resourceId, missingMetafieldResourceId],
    marketId,
    first: 10,
  };
  const connectionRead = await runGraphql(connectionsReadQuery, connectionVariables);

  cleanupProductResponse = await runGraphql(productDeleteMutation, { input: { id: createdProductId } });
  cleanupDefinitionResponse = await runGraphql(definitionDeleteMutation, {
    id: createdDefinitionId,
    deleteAllAssociatedMetafields: true,
  });

  const cases = [
    {
      name: 'marketLocalizableResourceObservation',
      query: singularReadQuery,
      variables: singularVariables,
      response: { status: 200, payload: singularRead },
    },
    {
      name: 'marketLocalizableResourceConnectionsAfterObservation',
      query: connectionsReadQuery,
      variables: connectionVariables,
      response: { status: 200, payload: connectionRead },
    },
  ];

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    disposableProductHandle: productInput.handle,
    disposableMetafieldDefinition: { namespace, key },
    scope: 'MarketLocalizableResource plural and byIds connection read-after-observation parity',
    setup: {
      definitionCreate: {
        query: definitionCreateMutation,
        variables: {
          definition: {
            name: `Market localizable connections ${captureToken}`,
            namespace,
            key,
            ownerType: 'PRODUCT',
            type: 'money',
            description: 'Disposable market-localizable connection capture.',
          },
        },
        response: { status: 200, payload: definitionCreate },
      },
      productCreate: {
        query: productCreateMutation,
        variables: { product: productInput, namespace },
        response: { status: 200, payload: productCreate },
      },
      productRead: {
        query: productReadQuery,
        variables: { id: createdProductId, namespace },
        response: { status: 200, payload: productRead },
      },
      marketProbe: {
        query: singularReadQuery,
        variables: {
          resourceId,
          marketId: 'gid://shopify/Market/0',
          marketsFirst: 1,
        },
        response: { status: 200, payload: marketProbe },
      },
    },
    cases,
    cleanup: {
      productDelete: {
        query: productDeleteMutation,
        variables: { input: { id: createdProductId } },
        response: { status: 200, payload: cleanupProductResponse },
      },
      definitionDelete: {
        query: definitionDeleteMutation,
        variables: { id: createdDefinitionId, deleteAllAssociatedMetafields: true },
        response: { status: 200, payload: cleanupDefinitionResponse },
      },
    },
    upstreamCalls: [
      {
        operationName: 'MarketLocalizationMoneyMetafieldRead',
        variables: singularVariables,
        query: singularReadQuery,
        response: { status: 200, body: singularRead },
      },
    ],
    notes:
      'Live Admin GraphQL capture creates a disposable product-owned money metafield definition and metafield. The singular read records the real market-localizable content/digest; the follow-up plural and byIds read proves Shopify returns the observed metafield resource and omits a never-created ByIds metafield ID.',
  };

  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
  await mkdir(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, 'market-localizable-resource-connections.json');
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  if (createdProductId && !cleanupProductResponse) {
    cleanupProductResponse = await runGraphql(productDeleteMutation, { input: { id: createdProductId } });
    console.error(JSON.stringify({ cleanupProductAfterFailure: cleanupProductResponse }, null, 2));
  }
  if (createdDefinitionId && !cleanupDefinitionResponse) {
    cleanupDefinitionResponse = await runGraphql(definitionDeleteMutation, {
      id: createdDefinitionId,
      deleteAllAssociatedMetafields: true,
    });
    console.error(JSON.stringify({ cleanupDefinitionAfterFailure: cleanupDefinitionResponse }, null, 2));
  }
  throw error;
}
