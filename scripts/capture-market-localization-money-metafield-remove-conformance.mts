/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const definitionCreateMutation = `#graphql
  mutation MarketLocalizationMoneyDefinitionCreate($definition: MetafieldDefinitionInput!) {
    metafieldDefinitionCreate(definition: $definition) {
      createdDefinition {
        id
        namespace
        key
        ownerType
        type {
          name
          category
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
  mutation MarketLocalizationMoneyDefinitionDelete($id: ID!, $deleteAllAssociatedMetafields: Boolean!) {
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
  mutation MarketLocalizationMoneyProductCreate($product: ProductCreateInput!, $namespace: String!) {
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
  query MarketLocalizationMoneyProductRead($id: ID!, $namespace: String!) {
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
  mutation MarketLocalizationMoneyProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const readQuery = `#graphql
  query MarketLocalizationMoneyMetafieldRead($resourceId: ID!, $marketId: ID!, $marketsFirst: Int!) {
    marketLocalizableResource(resourceId: $resourceId) {
      resourceId
      marketLocalizableContent {
        key
        value
        digest
      }
      marketLocalizations(marketId: $marketId) {
        key
        value
        updatedAt
        outdated
        market {
          id
          name
        }
      }
    }
    markets(first: $marketsFirst) {
      nodes {
        id
        name
        handle
        status
        type
      }
    }
  }
`;

const registerMutation = `#graphql
  mutation MarketLocalizationMoneyMetafieldRegister(
    $resourceId: ID!
    $marketLocalizations: [MarketLocalizationRegisterInput!]!
  ) {
    marketLocalizationsRegister(resourceId: $resourceId, marketLocalizations: $marketLocalizations) {
      marketLocalizations {
        key
        value
        updatedAt
        outdated
        market {
          id
          name
        }
      }
      userErrors {
        __typename
        field
        message
        code
      }
    }
  }
`;

const removeMutation = `#graphql
  mutation MarketLocalizationMoneyMetafieldRemove($resourceId: ID!, $keys: [String!]!, $marketIds: [ID!]!) {
    marketLocalizationsRemove(
      resourceId: $resourceId
      marketLocalizationKeys: $keys
      marketIds: $marketIds
    ) {
      marketLocalizations {
        key
        value
        updatedAt
        outdated
        market {
          id
          name
        }
      }
      userErrors {
        __typename
        field
        message
        code
      }
    }
  }
`;

const preflightQuery = 'query MarketsMutationPreflightHydrate { __typename }';

function randomSuffix(): string {
  return Math.random().toString(36).slice(2, 10);
}

function firstUserErrorMessage(response: unknown): string | null {
  if (typeof response !== 'object' || response === null) {
    return null;
  }
  const errors = (response as { userErrors?: unknown }).userErrors;
  if (!Array.isArray(errors) || errors.length === 0) {
    return null;
  }
  const first = errors[0];
  return typeof first === 'object' && first !== null && 'message' in first
    ? String((first as { message?: unknown }).message)
    : null;
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

const captureToken = randomSuffix();
const namespace = `codex_market_${captureToken}`;
const key = 'money';
const shopCurrencyCode = 'CAD';
const productInput = {
  title: `Market localization money ${captureToken}`,
  handle: `market-localization-money-${captureToken}`,
  status: 'DRAFT',
  metafields: [
    {
      namespace,
      key,
      type: 'money',
      value: JSON.stringify({ amount: '5.99', currency_code: shopCurrencyCode }),
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
      name: `Market localization money ${captureToken}`,
      namespace,
      key,
      ownerType: 'PRODUCT',
      type: 'money',
      description: 'Disposable market localization money metafield capture.',
    },
  });
  const definitionPayload = definitionCreate.data?.metafieldDefinitionCreate;
  createdDefinitionId =
    typeof definitionPayload?.createdDefinition?.id === 'string' ? definitionPayload.createdDefinition.id : null;
  if (!createdDefinitionId) {
    throw new Error(`Definition setup failed: ${firstUserErrorMessage(definitionPayload) ?? 'missing definition id'}`);
  }

  const setupProductCreate = await runGraphql(productCreateMutation, { product: productInput, namespace });
  const productCreatePayload = setupProductCreate.data?.productCreate;
  const product = productCreatePayload?.product;
  createdProductId = typeof product?.id === 'string' ? product.id : null;
  if (!createdProductId) {
    throw new Error(`Product setup failed: ${firstUserErrorMessage(productCreatePayload) ?? 'missing product id'}`);
  }

  const setupProductRead = await runGraphql(productReadQuery, { id: createdProductId, namespace });
  const seedProduct = setupProductRead.data?.product;
  const metafield = seedProduct?.metafields?.nodes?.[0];
  const resourceId = typeof metafield?.id === 'string' ? metafield.id : null;
  if (!resourceId) {
    throw new Error('Product metafield setup did not return a metafield id.');
  }

  const marketsRead = await runGraphql(readQuery, {
    resourceId,
    marketId: 'gid://shopify/Market/0',
    marketsFirst: 2,
  });
  const markets = marketsRead.data?.markets?.nodes ?? [];
  const firstMarket = markets[0];
  const secondMarket = markets[1];
  const firstMarketId = typeof firstMarket?.id === 'string' ? firstMarket.id : null;
  const secondMarketId = typeof secondMarket?.id === 'string' ? secondMarket.id : null;
  if (!firstMarketId || !secondMarketId) {
    throw new Error('Market setup failed: fewer than two markets returned from markets(first: 2).');
  }

  const readBeforeVariables = { resourceId, marketId: firstMarketId, marketsFirst: 2 };
  const readBeforeRegister = await runGraphql(readQuery, readBeforeVariables);
  const content = readBeforeRegister.data?.marketLocalizableResource?.marketLocalizableContent?.[0];
  const digest = typeof content?.digest === 'string' ? content.digest : null;
  const contentKey = typeof content?.key === 'string' ? content.key : null;
  if (contentKey !== 'value' || !digest) {
    throw new Error('Money metafield did not expose market localizable value content.');
  }

  const registerVariables = {
    resourceId,
    marketLocalizations: [
      {
        key: 'value',
        value: JSON.stringify({ amount: '6.99', currency_code: shopCurrencyCode }),
        marketId: firstMarketId,
        marketLocalizableContentDigest: digest,
      },
      {
        key: 'value',
        value: JSON.stringify({ amount: '7.99', currency_code: 'MXN' }),
        marketId: secondMarketId,
        marketLocalizableContentDigest: digest,
      },
    ],
  };
  const registerResponse = await runGraphql(registerMutation, registerVariables);
  const registerPreflightResponse = await runGraphql(preflightQuery, registerVariables);
  const readFirstAfterRegister = await runGraphql(readQuery, {
    resourceId,
    marketId: firstMarketId,
    marketsFirst: 2,
  });
  const readSecondAfterRegister = await runGraphql(readQuery, {
    resourceId,
    marketId: secondMarketId,
    marketsFirst: 2,
  });

  const removeFirstVariables = {
    resourceId,
    keys: ['value'],
    marketIds: [firstMarketId],
  };
  const removeFirstResponse = await runGraphql(removeMutation, removeFirstVariables);
  const removeFirstPreflightResponse = await runGraphql(preflightQuery, removeFirstVariables);
  const readFirstAfterPartialRemove = await runGraphql(readQuery, {
    resourceId,
    marketId: firstMarketId,
    marketsFirst: 2,
  });
  const readSecondAfterPartialRemove = await runGraphql(readQuery, {
    resourceId,
    marketId: secondMarketId,
    marketsFirst: 2,
  });

  const removeSecondVariables = {
    resourceId,
    keys: ['value'],
    marketIds: [secondMarketId],
  };
  const removeSecondResponse = await runGraphql(removeMutation, removeSecondVariables);
  const removeSecondPreflightResponse = await runGraphql(preflightQuery, removeSecondVariables);
  const readSecondAfterFinalRemove = await runGraphql(readQuery, {
    resourceId,
    marketId: secondMarketId,
    marketsFirst: 2,
  });

  cleanupProductResponse = await runGraphql(productDeleteMutation, { input: { id: createdProductId } });
  cleanupDefinitionResponse = await runGraphql(definitionDeleteMutation, {
    id: createdDefinitionId,
    deleteAllAssociatedMetafields: true,
  });

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    disposableProductHandle: productInput.handle,
    disposableMetafieldDefinition: { namespace, key },
    scope: 'Money metafield market localization register/remove parity',
    setup: {
      definitionCreate: {
        query: definitionCreateMutation,
        variables: {
          definition: {
            name: `Market localization money ${captureToken}`,
            namespace,
            key,
            ownerType: 'PRODUCT',
            type: 'money',
            description: 'Disposable market localization money metafield capture.',
          },
        },
        response: { status: 200, payload: definitionCreate },
      },
      productCreate: {
        query: productCreateMutation,
        variables: { product: productInput, namespace },
        response: { status: 200, payload: setupProductCreate },
      },
      productRead: {
        query: productReadQuery,
        variables: { id: createdProductId, namespace },
        response: { status: 200, payload: setupProductRead },
      },
      marketsRead: {
        query: readQuery,
        variables: {
          resourceId,
          marketId: 'gid://shopify/Market/0',
          marketsFirst: 2,
        },
        response: { status: 200, payload: marketsRead },
      },
    },
    cases: [
      {
        name: 'moneyMetafieldReadBeforeRegister',
        query: readQuery,
        variables: readBeforeVariables,
        response: { status: 200, payload: readBeforeRegister },
      },
      {
        name: 'moneyMetafieldRegisterTwoMarkets',
        query: registerMutation,
        variables: registerVariables,
        response: { status: 200, payload: registerResponse },
      },
      {
        name: 'moneyMetafieldReadFirstAfterRegister',
        query: readQuery,
        variables: { resourceId, marketId: firstMarketId, marketsFirst: 2 },
        response: { status: 200, payload: readFirstAfterRegister },
      },
      {
        name: 'moneyMetafieldReadSecondAfterRegister',
        query: readQuery,
        variables: { resourceId, marketId: secondMarketId, marketsFirst: 2 },
        response: { status: 200, payload: readSecondAfterRegister },
      },
      {
        name: 'moneyMetafieldRemoveFirstMarket',
        query: removeMutation,
        variables: removeFirstVariables,
        response: { status: 200, payload: removeFirstResponse },
      },
      {
        name: 'moneyMetafieldReadFirstAfterPartialRemove',
        query: readQuery,
        variables: { resourceId, marketId: firstMarketId, marketsFirst: 2 },
        response: { status: 200, payload: readFirstAfterPartialRemove },
      },
      {
        name: 'moneyMetafieldReadSecondAfterPartialRemove',
        query: readQuery,
        variables: { resourceId, marketId: secondMarketId, marketsFirst: 2 },
        response: { status: 200, payload: readSecondAfterPartialRemove },
      },
      {
        name: 'moneyMetafieldRemoveSecondMarket',
        query: removeMutation,
        variables: removeSecondVariables,
        response: { status: 200, payload: removeSecondResponse },
      },
      {
        name: 'moneyMetafieldReadSecondAfterFinalRemove',
        query: readQuery,
        variables: { resourceId, marketId: secondMarketId, marketsFirst: 2 },
        response: { status: 200, payload: readSecondAfterFinalRemove },
      },
    ],
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
        variables: readBeforeVariables,
        query: readQuery,
        response: { status: 200, body: readBeforeRegister },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: registerVariables,
        query: preflightQuery,
        response: { status: 200, body: registerPreflightResponse },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: removeFirstVariables,
        query: preflightQuery,
        response: { status: 200, body: removeFirstPreflightResponse },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: removeSecondVariables,
        query: preflightQuery,
        response: { status: 200, body: removeSecondPreflightResponse },
      },
    ],
    notes:
      'Live Admin GraphQL capture creates a disposable money metafield definition and product metafield. Shopify exposes only the money metafield value as market-localizable content; the capture registers two market-specific money values, removes one market tuple, verifies the other remains visible, then removes the remaining tuple.',
  };

  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
  await mkdir(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, 'market-localization-money-metafield-remove-parity.json');
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
