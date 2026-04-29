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

let createdProductId: string | null = null;
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

  cleanupResponse = await runGraphql(productDeleteMutation, { input: { id: createdProductId } });

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    disposableProductHandle: productInput.handle,
    scope: 'HAR-448 default product-metafield market localization validation parity',
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
    },
    cases: [
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
    ],
    cleanup: {
      productDelete: {
        query: productDeleteMutation,
        variables: { input: { id: createdProductId } },
        response: {
          status: 200,
          payload: cleanupResponse,
        },
      },
    },
  };

  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
  await mkdir(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, 'market-localization-metafield-lifecycle-parity.json');
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  if (createdProductId && !cleanupResponse) {
    cleanupResponse = await runGraphql(productDeleteMutation, { input: { id: createdProductId } });
    console.error(JSON.stringify({ cleanupAfterFailure: cleanupResponse }, null, 2));
  }
  throw error;
}
