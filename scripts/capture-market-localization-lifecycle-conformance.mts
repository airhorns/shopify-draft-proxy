/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const productCreateMutation = `#graphql
  mutation MarketLocalizationProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        handle
        title
        status
        metafields(first: 5, namespace: "custom") {
          nodes {
            id
            namespace
            key
            type
            value
            compareDigest
            createdAt
            updatedAt
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
  query MarketLocalizationProductRead($id: ID!) {
    product(id: $id) {
      id
      handle
      title
      status
      metafields(first: 5, namespace: "custom") {
        nodes {
          id
          namespace
          key
          type
          value
          compareDigest
          createdAt
          updatedAt
          ownerType
        }
      }
    }
  }
`;

const productReadByNamespaceQuery = `#graphql
  query MarketLocalizationProductReadByNamespace($id: ID!, $namespace: String!) {
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
          createdAt
          updatedAt
          ownerType
        }
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation MarketLocalizationProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const metafieldDefinitionCreateMutation = `#graphql
  mutation MarketLocalizationMetafieldDefinitionCreate($definition: MetafieldDefinitionInput!) {
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

const metafieldDefinitionDeleteMutation = `#graphql
  mutation MarketLocalizationMetafieldDefinitionDelete($id: ID!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
      deletedDefinitionId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const metafieldsSetMutation = `#graphql
  mutation MarketLocalizationMetafieldsSet($metafields: [MetafieldsSetInput!]!) {
    metafieldsSet(metafields: $metafields) {
      metafields {
        id
        namespace
        key
        type
        value
        compareDigest
        createdAt
        updatedAt
        ownerType
        owner {
          ... on Product {
            id
          }
        }
      }
      userErrors {
        field
        message
        code
        elementIndex
      }
    }
  }
`;

const marketsReadQuery = `#graphql
  query MarketLocalizationMarketsRead($first: Int!) {
    markets(first: $first) {
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

const localizableReadQuery = `#graphql
  query MarketLocalizationMetafieldRead($resourceId: ID!, $marketId: ID!) {
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
  }
`;

const registerMutation = `#graphql
  mutation MarketLocalizationMetafieldRegister(
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
        field
        message
        code
      }
    }
  }
`;

const removeMutation = `#graphql
  mutation MarketLocalizationMetafieldRemove($resourceId: ID!, $keys: [String!]!, $marketIds: [ID!]!) {
    marketLocalizationsRemove(
      resourceId: $resourceId
      marketLocalizationKeys: $keys
      marketIds: $marketIds
    ) {
      marketLocalizations {
        key
        value
        outdated
        market {
          id
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

function dataObject(response: unknown): Record<string, unknown> {
  const data = response && typeof response === 'object' ? (response as { data?: unknown }).data : null;
  return data && typeof data === 'object' ? (data as Record<string, unknown>) : {};
}

function assertNoUserErrors(payload: unknown, root: string, label: string): void {
  const rootPayload = dataObject(payload)[root];
  const userErrors =
    rootPayload && typeof rootPayload === 'object' ? (rootPayload as { userErrors?: unknown }).userErrors : null;
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function objectOrNull(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function caseResponsePayload(entry: { response: { payload: unknown } }): unknown {
  return entry.response.payload;
}

function marketLocalizableResourceFrom(response: unknown): unknown {
  return dataObject(response)['marketLocalizableResource'];
}

function resourceNodeFromProduct(product: unknown, resourceId: string): Record<string, unknown> | null {
  const productObject = objectOrNull(product);
  const metafields = objectOrNull(productObject?.['metafields']);
  const nodes = metafields?.['nodes'];
  if (!Array.isArray(nodes)) {
    return null;
  }
  const metafield = nodes.find((node) => objectOrNull(node)?.['id'] === resourceId);
  const metafieldObject = objectOrNull(metafield);
  if (!metafieldObject || !productObject) {
    return null;
  }
  return {
    __typename: 'Metafield',
    ...metafieldObject,
    owner: {
      __typename: 'Product',
      id: productObject['id'],
      handle: productObject['handle'],
      title: productObject['title'],
      status: productObject['status'],
      metafields: productObject['metafields'],
    },
  };
}

function preflightBody(product: unknown, resourceId: string, localizableResource: unknown, markets: unknown): unknown {
  return {
    data: {
      resource: resourceNodeFromProduct(product, resourceId),
      marketLocalizableResource: localizableResource,
      markets,
    },
  };
}

function upstreamReadCall(entry: {
  query: string;
  variables: Record<string, unknown>;
  response: { payload: unknown };
}) {
  return {
    operationName: 'MarketLocalizationMetafieldRead',
    variables: entry.variables,
    query: entry.query,
    response: {
      status: 200,
      body: caseResponsePayload(entry),
    },
  };
}

function upstreamPreflightCall(
  entry: { variables: Record<string, unknown> },
  product: unknown,
  resourceId: string,
  localizableResource: unknown,
  markets: unknown,
) {
  return {
    operationName: 'MarketsMutationPreflightHydrate',
    variables: entry.variables,
    query: 'synthesized from live capture setup before disposable cleanup',
    response: {
      status: 200,
      body: preflightBody(product, resourceId, localizableResource, markets),
    },
  };
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const captureToken = randomSuffix();
const productInput = {
  title: `HAR market localization ${captureToken}`,
  handle: `har-market-localization-${captureToken}`,
  status: 'DRAFT',
  metafields: [
    {
      namespace: 'custom',
      key: 'market_material',
      type: 'single_line_text_field',
      value: `Maple ${captureToken}`,
    },
  ],
};
const moneyDefinitionInput = {
  name: 'Market localization money',
  namespace: `har_market_money_${captureToken}`,
  key: 'localized_price',
  ownerType: 'PRODUCT',
  type: 'money',
};
const moneyMetafieldValue = JSON.stringify({
  amount: '12.34',
  currency_code: 'CAD',
});
const localizedMoneyValue = JSON.stringify({
  amount: '15.67',
  currency_code: 'CAD',
});

let createdProductId: string | null = null;
let createdDefinitionId: string | null = null;
let cleanupDefinitionResponse: unknown = null;
let cleanupResponse: unknown = null;

try {
  const setupProductCreate = await runGraphql(productCreateMutation, { product: productInput });
  const productCreatePayload = setupProductCreate.data?.productCreate;
  const product = productCreatePayload?.product;
  createdProductId = typeof product?.id === 'string' ? product.id : null;
  if (!createdProductId) {
    throw new Error(`Product setup failed: ${firstUserErrorMessage(productCreatePayload) ?? 'missing product id'}`);
  }

  const setupProductRead = await runGraphql(productReadQuery, { id: createdProductId });
  const seedProduct = setupProductRead.data?.product;
  const metafield = seedProduct?.metafields?.nodes?.[0];
  const resourceId = typeof metafield?.id === 'string' ? metafield.id : null;
  const digest = typeof metafield?.compareDigest === 'string' ? metafield.compareDigest : null;
  if (!resourceId || !digest) {
    throw new Error('Product metafield setup did not return a market-localizable metafield id and compareDigest.');
  }

  const marketsRead = await runGraphql(marketsReadQuery, { first: 1 });
  const market = marketsRead.data?.markets?.nodes?.[0];
  const marketId = typeof market?.id === 'string' ? market.id : null;
  if (!marketId) {
    throw new Error('Market setup failed: no market returned from markets(first: 1).');
  }

  const readVariables = { resourceId, marketId };
  const registerVariables = {
    resourceId,
    marketLocalizations: [
      {
        key: 'value',
        value: `Erable ${captureToken}`,
        marketId,
        marketLocalizableContentDigest: digest,
      },
    ],
  };
  const removeVariables = {
    resourceId,
    keys: ['value'],
    marketIds: [marketId],
  };

  const readBeforeRegister = await runGraphql(localizableReadQuery, readVariables);
  const registerResponse = await runGraphql(registerMutation, registerVariables);
  const readAfterRegister = await runGraphql(localizableReadQuery, readVariables);
  const removeResponse = await runGraphql(removeMutation, removeVariables);
  const readAfterRemove = await runGraphql(localizableReadQuery, readVariables);

  const definitionCreateResponse = await runGraphql(metafieldDefinitionCreateMutation, {
    definition: moneyDefinitionInput,
  });
  assertNoUserErrors(definitionCreateResponse, 'metafieldDefinitionCreate', 'money metafield definition setup');
  createdDefinitionId =
    typeof definitionCreateResponse.data?.metafieldDefinitionCreate?.createdDefinition?.id === 'string'
      ? definitionCreateResponse.data.metafieldDefinitionCreate.createdDefinition.id
      : null;
  if (!createdDefinitionId) {
    throw new Error('Money metafield definition setup did not return a definition id.');
  }

  const moneyMetafieldsSetResponse = await runGraphql(metafieldsSetMutation, {
    metafields: [
      {
        ownerId: createdProductId,
        namespace: moneyDefinitionInput.namespace,
        key: moneyDefinitionInput.key,
        type: moneyDefinitionInput.type,
        value: moneyMetafieldValue,
      },
    ],
  });
  assertNoUserErrors(moneyMetafieldsSetResponse, 'metafieldsSet', 'money metafield setup');
  const moneyMetafield = moneyMetafieldsSetResponse.data?.metafieldsSet?.metafields?.[0];
  const moneyResourceId = typeof moneyMetafield?.id === 'string' ? moneyMetafield.id : null;
  if (!moneyResourceId) {
    throw new Error('Money metafield setup did not return a metafield id.');
  }

  const moneyProductRead = await runGraphql(productReadByNamespaceQuery, {
    id: createdProductId,
    namespace: moneyDefinitionInput.namespace,
  });
  const seededMoneyProduct = moneyProductRead.data?.product;
  const moneyReadVariables = { resourceId: moneyResourceId, marketId };
  const moneyReadBeforeRegister = await runGraphql(localizableReadQuery, moneyReadVariables);
  const moneyContent = moneyReadBeforeRegister.data?.marketLocalizableResource?.marketLocalizableContent?.[0];
  const moneyContentKey = typeof moneyContent?.key === 'string' ? moneyContent.key : null;
  const moneyContentDigest = typeof moneyContent?.digest === 'string' ? moneyContent.digest : null;
  if (!moneyContentKey || !moneyContentDigest) {
    throw new Error('Money metafield did not expose marketLocalizableContent.');
  }
  const moneyRegisterVariables = {
    resourceId: moneyResourceId,
    marketLocalizations: [
      {
        key: moneyContentKey,
        value: localizedMoneyValue,
        marketId,
        marketLocalizableContentDigest: moneyContentDigest,
      },
    ],
  };
  const moneyUnknownMarketVariables = {
    resourceId: moneyResourceId,
    keys: [moneyContentKey],
    marketIds: ['gid://shopify/Market/999999999999'],
  };
  const moneyEmptyKeysVariables = {
    resourceId: moneyResourceId,
    keys: [],
    marketIds: [marketId],
  };
  const moneyRemoveVariables = {
    resourceId: moneyResourceId,
    keys: [moneyContentKey],
    marketIds: [marketId],
  };
  const moneyRegisterResponse = await runGraphql(registerMutation, moneyRegisterVariables);
  const moneyReadAfterRegister = await runGraphql(localizableReadQuery, moneyReadVariables);
  const moneyUnknownMarketRemoveResponse = await runGraphql(removeMutation, moneyUnknownMarketVariables);
  const moneyEmptyKeysRemoveResponse = await runGraphql(removeMutation, moneyEmptyKeysVariables);
  const moneyRemoveResponse = await runGraphql(removeMutation, moneyRemoveVariables);
  const moneyReadAfterRemove = await runGraphql(localizableReadQuery, moneyReadVariables);

  cleanupDefinitionResponse = await runGraphql(metafieldDefinitionDeleteMutation, { id: createdDefinitionId });
  cleanupResponse = await runGraphql(productDeleteMutation, { input: { id: createdProductId } });

  const cases = [
    {
      name: 'marketLocalizableMetafieldReadBeforeRegister',
      query: localizableReadQuery,
      variables: readVariables,
      response: {
        status: 200,
        payload: readBeforeRegister,
      },
    },
    {
      name: 'marketLocalizationsRegisterDefaultMetafieldValidation',
      query: registerMutation,
      variables: registerVariables,
      response: {
        status: 200,
        payload: registerResponse,
      },
    },
    {
      name: 'marketLocalizableMetafieldReadAfterRejectedRegister',
      query: localizableReadQuery,
      variables: readVariables,
      response: {
        status: 200,
        payload: readAfterRegister,
      },
    },
    {
      name: 'marketLocalizationsRemoveDefaultMetafieldValidation',
      query: removeMutation,
      variables: removeVariables,
      response: {
        status: 200,
        payload: removeResponse,
      },
    },
    {
      name: 'marketLocalizableMetafieldReadAfterEmptyRemove',
      query: localizableReadQuery,
      variables: readVariables,
      response: {
        status: 200,
        payload: readAfterRemove,
      },
    },
    {
      name: 'marketLocalizableMoneyMetafieldReadBeforeRegister',
      query: localizableReadQuery,
      variables: moneyReadVariables,
      response: {
        status: 200,
        payload: moneyReadBeforeRegister,
      },
    },
    {
      name: 'marketLocalizationsRegisterMoneyMetafieldSuccess',
      query: registerMutation,
      variables: moneyRegisterVariables,
      response: {
        status: 200,
        payload: moneyRegisterResponse,
      },
    },
    {
      name: 'marketLocalizableMoneyMetafieldReadAfterRegister',
      query: localizableReadQuery,
      variables: moneyReadVariables,
      response: {
        status: 200,
        payload: moneyReadAfterRegister,
      },
    },
    {
      name: 'marketLocalizationsRemoveMoneyMetafieldUnknownMarket',
      query: removeMutation,
      variables: moneyUnknownMarketVariables,
      response: {
        status: 200,
        payload: moneyUnknownMarketRemoveResponse,
      },
    },
    {
      name: 'marketLocalizationsRemoveMoneyMetafieldEmptyKeys',
      query: removeMutation,
      variables: moneyEmptyKeysVariables,
      response: {
        status: 200,
        payload: moneyEmptyKeysRemoveResponse,
      },
    },
    {
      name: 'marketLocalizationsRemoveMoneyMetafieldSuccess',
      query: removeMutation,
      variables: moneyRemoveVariables,
      response: {
        status: 200,
        payload: moneyRemoveResponse,
      },
    },
    {
      name: 'marketLocalizableMoneyMetafieldReadAfterRemove',
      query: localizableReadQuery,
      variables: moneyReadVariables,
      response: {
        status: 200,
        payload: moneyReadAfterRemove,
      },
    },
  ];
  const marketsPreflightPayload = marketsRead.data?.markets;
  const defaultLocalizableResource = marketLocalizableResourceFrom(readBeforeRegister);
  const moneyLocalizableResource = marketLocalizableResourceFrom(moneyReadBeforeRegister);

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    disposableProductHandle: productInput.handle,
    scope:
      'Default product-metafield market localization validation plus definition-backed money metafield successful remove parity',
    data: {
      markets: marketsRead.data?.markets,
    },
    response: {
      data: {
        product: seedProduct,
      },
    },
    setup: {
      productCreate: {
        query: productCreateMutation,
        variables: { product: productInput },
        response: {
          status: 200,
          payload: setupProductCreate,
        },
      },
      productRead: {
        query: productReadQuery,
        variables: { id: createdProductId },
        response: {
          status: 200,
          payload: setupProductRead,
        },
      },
      moneyDefinitionCreate: {
        query: metafieldDefinitionCreateMutation,
        variables: { definition: moneyDefinitionInput },
        response: {
          status: 200,
          payload: definitionCreateResponse,
        },
      },
      moneyMetafieldsSet: {
        query: metafieldsSetMutation,
        variables: {
          metafields: [
            {
              ownerId: createdProductId,
              namespace: moneyDefinitionInput.namespace,
              key: moneyDefinitionInput.key,
              type: moneyDefinitionInput.type,
              value: moneyMetafieldValue,
            },
          ],
        },
        response: {
          status: 200,
          payload: moneyMetafieldsSetResponse,
        },
      },
      moneyProductRead: {
        query: productReadByNamespaceQuery,
        variables: {
          id: createdProductId,
          namespace: moneyDefinitionInput.namespace,
        },
        response: {
          status: 200,
          payload: moneyProductRead,
        },
      },
    },
    cases,
    cleanup: {
      moneyDefinitionDelete: {
        query: metafieldDefinitionDeleteMutation,
        variables: { id: createdDefinitionId },
        response: {
          status: 200,
          payload: cleanupDefinitionResponse,
        },
      },
      productDelete: {
        query: productDeleteMutation,
        variables: { input: { id: createdProductId } },
        response: {
          status: 200,
          payload: cleanupResponse,
        },
      },
    },
    upstreamCalls: [
      upstreamReadCall(cases[0]),
      upstreamPreflightCall(cases[1], seedProduct, resourceId, defaultLocalizableResource, marketsPreflightPayload),
      upstreamPreflightCall(cases[3], seedProduct, resourceId, defaultLocalizableResource, marketsPreflightPayload),
      upstreamReadCall(cases[5]),
      upstreamPreflightCall(
        cases[6],
        seededMoneyProduct,
        moneyResourceId,
        moneyLocalizableResource,
        marketsPreflightPayload,
      ),
      upstreamPreflightCall(
        cases[8],
        seededMoneyProduct,
        moneyResourceId,
        moneyLocalizableResource,
        marketsPreflightPayload,
      ),
      upstreamPreflightCall(
        cases[9],
        seededMoneyProduct,
        moneyResourceId,
        moneyLocalizableResource,
        marketsPreflightPayload,
      ),
      upstreamPreflightCall(
        cases[10],
        seededMoneyProduct,
        moneyResourceId,
        moneyLocalizableResource,
        marketsPreflightPayload,
      ),
    ],
  };

  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
  await mkdir(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, 'market-localization-metafield-lifecycle-parity.json');
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  if (createdDefinitionId && !cleanupDefinitionResponse) {
    cleanupDefinitionResponse = await runGraphql(metafieldDefinitionDeleteMutation, { id: createdDefinitionId });
    console.error(JSON.stringify({ cleanupDefinitionAfterFailure: cleanupDefinitionResponse }, null, 2));
  }
  if (createdProductId && !cleanupResponse) {
    cleanupResponse = await runGraphql(productDeleteMutation, { input: { id: createdProductId } });
    console.error(JSON.stringify({ cleanupAfterFailure: cleanupResponse }, null, 2));
  }
  throw error;
}
