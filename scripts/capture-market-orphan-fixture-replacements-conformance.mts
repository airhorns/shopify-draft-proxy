/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type CapturedCase = {
  name: string;
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
};

const fixtureStoreDomain = 'harry-test-heelo.myshopify.com';
const oldMissingId = 'gid://shopify/MarketCatalog/999999999999';
const missingMarketId = 'gid://shopify/Market/999999999999';
const missingMetafieldId = 'gid://shopify/Metafield/999999999999';
const missingProductId = 'gid://shopify/Product/0';
const missingPriceListId = 'gid://shopify/PriceList/0';
const missingVariantId = 'gid://shopify/ProductVariant/0';

function readPinnedConfig(apiVersion: string) {
  return readConformanceScriptConfig({
    defaultApiVersion: apiVersion,
    env: { ...process.env, SHOPIFY_CONFORMANCE_API_VERSION: apiVersion },
    exitOnMissing: true,
  });
}

const config2026 = readPinnedConfig('2026-04');

if (config2026.storeDomain !== fixtureStoreDomain) {
  throw new Error(
    `This recorder replaces checked-in ${fixtureStoreDomain} fixtures; got SHOPIFY_CONFORMANCE_STORE_DOMAIN=${config2026.storeDomain}.`,
  );
}

const adminAccessToken2026 = await getValidConformanceAccessToken({
  adminOrigin: config2026.adminOrigin,
  apiVersion: config2026.apiVersion,
});
const client2026 = createAdminGraphqlClient({
  adminOrigin: config2026.adminOrigin,
  apiVersion: config2026.apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken2026),
});

async function run2026(query: string, variables: JsonRecord = {}): Promise<ConformanceGraphqlResult> {
  return client2026.runGraphqlRequest(query, variables);
}

async function readDocument(relativePath: string): Promise<string> {
  return await readFile(relativePath, 'utf8');
}

async function readVariables(relativePath: string): Promise<JsonRecord> {
  const raw = await readDocument(relativePath);
  const parsed: unknown = JSON.parse(raw);
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    throw new Error(`Expected JSON object variables in ${relativePath}`);
  }
  return parsed as JsonRecord;
}

async function writeFixture(apiVersion: string, fileName: string, capture: JsonRecord): Promise<string> {
  const outputDir = path.join('fixtures', 'conformance', fixtureStoreDomain, apiVersion, 'markets');
  const outputPath = path.join(outputDir, fileName);
  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  return outputPath;
}

function payloadData(result: ConformanceGraphqlResult): JsonRecord {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null || Array.isArray(data)) {
    throw new Error(`Expected GraphQL data object: ${JSON.stringify(result.payload)}`);
  }
  return data as JsonRecord;
}

function rootPayload(result: ConformanceGraphqlResult, root: string): JsonRecord {
  const rootValue = payloadData(result)[root];
  if (typeof rootValue !== 'object' || rootValue === null || Array.isArray(rootValue)) {
    throw new Error(`Expected data.${root} object: ${JSON.stringify(result.payload)}`);
  }
  return rootValue as JsonRecord;
}

function nestedId(result: ConformanceGraphqlResult, root: string, field: string): string {
  const node = rootPayload(result, root)[field];
  if (typeof node !== 'object' || node === null || Array.isArray(node)) {
    throw new Error(`Expected data.${root}.${field} object: ${JSON.stringify(result.payload)}`);
  }
  const id = (node as JsonRecord)['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`Expected data.${root}.${field}.id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function userErrors(result: ConformanceGraphqlResult, root: string): JsonRecord[] {
  const errors = rootPayload(result, root)['userErrors'];
  return Array.isArray(errors)
    ? errors.filter(
        (error): error is JsonRecord => typeof error === 'object' && error !== null && !Array.isArray(error),
      )
    : [];
}

function assertGraphqlOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertGraphqlError(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || !Array.isArray(result.payload.errors)) {
    throw new Error(`${label} did not return GraphQL errors: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(label: string, result: ConformanceGraphqlResult, root: string): void {
  const errors = userErrors(result, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function responseDataOnly(result: ConformanceGraphqlResult): JsonRecord {
  return { data: payloadData(result) };
}

function responseRootOnly(result: ConformanceGraphqlResult, root: string): JsonRecord {
  return { [root]: rootPayload(result, root) };
}

async function captureCase(name: string, query: string, variables: JsonRecord): Promise<CapturedCase> {
  const response = await run2026(query, variables);
  assertGraphqlOk(name, response);
  return { name, query, variables, response };
}

async function captureCatalogCreateMissingContext(capturedAt: string): Promise<string> {
  const query = `#graphql
mutation CatalogCreateMissingContext($input: CatalogCreateInput!) {
  catalogCreate(input: $input) {
    catalog { id }
    userErrors { field message code }
  }
}`;
  const variables = { input: { title: 'EU Catalog', status: 'ACTIVE' } };
  const response = await run2026(query, variables);
  assertGraphqlError('catalogCreate missing context', response);

  return writeFixture('2026-04', 'catalog-create-missing-context.json', {
    capturedAt,
    storeDomain: fixtureStoreDomain,
    apiVersion: '2026-04',
    cases: [{ name: 'catalogCreateMissingContext', query, variables, response }],
    upstreamCalls: [],
  });
}

async function captureCatalogLifecycleValidation(capturedAt: string): Promise<string> {
  const marketReadQuery = `#graphql
query CatalogLifecycleValidationMarketSeed($first: Int!) {
  markets(first: $first) {
    nodes { id name }
  }
}`;
  const marketRead = await run2026(marketReadQuery, { first: 10 });
  assertGraphqlOk('catalog lifecycle market seed', marketRead);
  const market = (payloadData(marketRead)['markets'] as { nodes?: Array<{ id?: string; name?: string }> }).nodes?.[0];
  if (!market?.id) {
    throw new Error('catalog lifecycle validation needs at least one existing market.');
  }

  const cases = [
    await captureCase(
      'catalogCreateBlankTitle',
      await readDocument('config/parity-requests/markets/catalog-create-blank-title-validation.graphql'),
      {
        input: {
          title: '',
          status: 'ACTIVE',
          context: { marketIds: [market.id] },
        },
      },
    ),
    await captureCase(
      'catalogUpdateUnknownId',
      await readDocument('config/parity-requests/markets/catalog-update-unknown-id-validation.graphql'),
      { id: oldMissingId, input: { title: 'Nope' } },
    ),
    await captureCase(
      'catalogContextUpdateUnknownId',
      await readDocument('config/parity-requests/markets/catalog-context-update-unknown-id-validation.graphql'),
      { catalogId: oldMissingId, contextsToAdd: { marketIds: [market.id] } },
    ),
    await captureCase(
      'catalogDeleteUnknownId',
      await readDocument('config/parity-requests/markets/catalog-delete-unknown-id-validation.graphql'),
      { id: oldMissingId },
    ),
  ];

  return writeFixture('2026-04', 'catalog-lifecycle-validation.json', {
    capturedAt,
    storeDomain: fixtureStoreDomain,
    apiVersion: '2026-04',
    data: payloadData(marketRead),
    cases,
    upstreamCalls: [],
  });
}

async function captureMarketCreateStatusEnabledMismatch(capturedAt: string): Promise<string> {
  const query = await readDocument('config/parity-requests/markets/market-create-status-enabled-mismatch.graphql');
  const variables = {
    input: {
      name: `Mismatch ${Date.now()}`,
      status: 'DRAFT',
      enabled: true,
      regions: [{ countryCode: 'US' }],
    },
  };
  const response = await run2026(query, variables);
  assertGraphqlOk('marketCreate status/enabled mismatch', response);

  return writeFixture('2026-04', 'market-create-status-enabled-mismatch.json', {
    capturedAt,
    storeDomain: fixtureStoreDomain,
    apiVersion: '2026-04',
    cases: [{ name: 'marketCreateDraftEnabledMismatch', query, variables, response }],
    upstreamCalls: [],
  });
}

async function captureMarketLocalizableEmptyRead(capturedAt: string): Promise<string> {
  const query = await readDocument('config/parity-requests/markets/market-localizable-empty-read.graphql');
  const variables = await readVariables('config/parity-requests/markets/market-localizable-empty-read.variables.json');
  const response = await run2026(query, variables);
  assertGraphqlOk('market localizable empty read', response);

  return writeFixture('2026-04', 'market-localizable-empty-read.json', {
    capturedAt,
    storeDomain: fixtureStoreDomain,
    apiVersion: '2026-04',
    query,
    variables,
    response,
    upstreamCalls: [
      {
        operationName: 'MarketLocalizableEmptyRead',
        variables,
        query: 'captured live market localizable empty read',
        response: {
          status: response.status,
          body: response.payload,
        },
      },
    ],
  });
}

async function captureMarketLocalizationValidation(capturedAt: string): Promise<string> {
  const registerQuery = await readDocument(
    'config/parity-requests/markets/market-localizations-register-unknown-resource-validation.graphql',
  );
  const removeQuery = await readDocument(
    'config/parity-requests/markets/market-localizations-remove-unknown-resource-validation.graphql',
  );
  const cases = [
    await captureCase('marketLocalizationsRegisterUnknownResource', registerQuery, {
      resourceId: missingMetafieldId,
      marketLocalizations: [
        {
          marketId: missingMarketId,
          key: 'value',
          value: 'Localized',
          marketLocalizableContentDigest: 'bad-digest',
        },
      ],
    }),
    await captureCase('marketLocalizationsRemoveUnknownResource', removeQuery, {
      resourceId: missingMetafieldId,
      marketLocalizationKeys: ['value'],
      marketIds: [missingMarketId],
    }),
  ];

  return writeFixture('2026-04', 'market-localization-validation.json', {
    capturedAt,
    storeDomain: fixtureStoreDomain,
    apiVersion: '2026-04',
    cases,
    upstreamCalls: [],
  });
}

async function captureMarketLocalizationsRegisterTooManyKeys(capturedAt: string): Promise<string> {
  const productCreateQuery = `#graphql
mutation CreateMarketLocalizationCapProduct($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      handle
      metafields(first: 5, namespace: "custom") {
        nodes { id namespace key type value compareDigest createdAt updatedAt ownerType }
      }
    }
    userErrors { field message }
  }
}`;
  const productDeleteQuery = `#graphql
mutation DeleteMarketLocalizationCapProduct($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors { field message }
  }
}`;
  const registerQuery = await readDocument(
    'config/parity-requests/markets/market-localizations-register-too-many-keys.graphql',
  );
  const suffix = Date.now().toString(36);
  const productVariables = {
    product: {
      title: `Market localizations cap ${suffix}`,
      handle: `market-localizations-cap-${suffix}`,
      status: 'DRAFT',
      metafields: [
        {
          namespace: 'custom',
          key: 'market_material',
          type: 'single_line_text_field',
          value: 'Cotton',
        },
      ],
    },
  };
  const productCreate = await run2026(productCreateQuery, productVariables);
  assertGraphqlOk('market localizations cap product create', productCreate);
  assertNoUserErrors('market localizations cap product create', productCreate, 'productCreate');
  const productId = nestedId(productCreate, 'productCreate', 'product');
  const metafields = (
    (rootPayload(productCreate, 'productCreate')['product'] as JsonRecord)['metafields'] as {
      nodes?: JsonRecord[];
    }
  ).nodes;
  const metafield = metafields?.[0];
  const metafieldId = metafield?.['id'];
  if (typeof metafieldId !== 'string') {
    throw new Error(`Could not read created metafield id: ${JSON.stringify(productCreate.payload)}`);
  }
  const digest = typeof metafield['compareDigest'] === 'string' ? metafield['compareDigest'] : 'bad-digest';
  const variables = {
    resourceId: metafieldId,
    marketLocalizations: Array.from({ length: 101 }, (_, index) => ({
      marketId: `gid://shopify/Market/${900_000_000_000 + index}`,
      key: 'value',
      value: `Localized ${index}`,
      marketLocalizableContentDigest: digest,
    })),
  };
  const response = await run2026(registerQuery, variables);
  assertGraphqlOk('market localizations too many keys', response);
  const productDelete = await run2026(productDeleteQuery, { input: { id: productId } });

  return writeFixture('2026-04', 'market-localizations-register-too-many-keys.json', {
    capturedAt,
    storeDomain: fixtureStoreDomain,
    apiVersion: '2026-04',
    scope: 'marketLocalizationsRegister >100 key cap validation',
    disposableProductId: productId,
    disposableMetafieldId: metafieldId,
    setup: {
      productCreate: {
        query: productCreateQuery,
        variables: productVariables,
        response: productCreate,
      },
    },
    cases: [{ name: 'marketLocalizationsRegisterTooManyKeys', query: registerQuery, variables, response }],
    cleanup: {
      productDelete: {
        query: productDeleteQuery,
        variables: { input: { id: productId } },
        response: productDelete,
      },
    },
    upstreamCalls: [],
  });
}

async function captureMarketWebPresenceValidation(capturedAt: string): Promise<string> {
  const createQuery = await readDocument(
    'config/parity-requests/markets/web-presence-create-invalid-routing-validation.graphql',
  );
  const updateQuery = await readDocument(
    'config/parity-requests/markets/web-presence-update-unknown-id-validation.graphql',
  );
  const cases = [
    await captureCase('webPresenceCreateInvalidRouting', createQuery, {
      input: {
        domainId: 'gid://shopify/Domain/93049946345',
        defaultLocale: 'english',
        alternateLocales: ['en', 'en'],
        subfolderSuffix: '../ca',
      },
    }),
    await captureCase('webPresenceUpdateUnknownId', updateQuery, {
      id: 'gid://shopify/MarketWebPresence/999999999999',
      input: { defaultLocale: 'en' },
    }),
  ];

  return writeFixture('2026-04', 'market-web-presence-validation.json', {
    capturedAt,
    storeDomain: fixtureStoreDomain,
    apiVersion: '2026-04',
    cases,
    upstreamCalls: [],
  });
}

async function captureMarketWebPresenceDelete(capturedAt: string): Promise<string> {
  const readQuery = await readDocument('config/parity-requests/markets/web-presence-delete-downstream-read.graphql');
  const createQuery = await readDocument('config/parity-requests/markets/web-presence-delete-create.graphql');
  const deleteQuery = await readDocument('config/parity-requests/markets/web-presence-delete.graphql');
  const disposableSubfolderSuffix = `harcodex${Date.now().toString(36)}`.replace(/[^a-z]/gu, '').slice(0, 20);
  const data = payloadData(await run2026(readQuery, { first: 20 }));
  const unknownDelete = await captureCase('webPresenceDeleteUnknownId', deleteQuery, {
    id: 'gid://shopify/MarketWebPresence/999999999999',
  });
  const create = await captureCase('webPresenceCreateDisposableForDelete', createQuery, {
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: disposableSubfolderSuffix,
    },
  });
  assertNoUserErrors('webPresenceCreate disposable for delete', create.response, 'webPresenceCreate');
  const createdId = nestedId(create.response, 'webPresenceCreate', 'webPresence');
  const deleted = await captureCase('webPresenceDeleteSuccess', deleteQuery, { id: createdId });
  const deletedAgain = await captureCase('webPresenceDeleteAlreadyDeleted', deleteQuery, { id: createdId });
  const readAfterDelete = await captureCase('webPresenceReadAfterDelete', readQuery, { first: 20 });

  return writeFixture('2026-04', 'market-web-presence-delete-parity.json', {
    capturedAt,
    storeDomain: fixtureStoreDomain,
    apiVersion: '2026-04',
    disposableSubfolderSuffix,
    data,
    cases: [unknownDelete, create, deleted, deletedAgain, readAfterDelete],
    upstreamCalls: [
      {
        operationName: 'MarketWebPresenceHydrate',
        variables: create.variables,
        query: 'captured live webPresenceCreate setup for delete parity',
        response: {
          status: create.response.status,
          body: create.response.payload,
        },
      },
      {
        operationName: 'WebPresenceDeleteParityRead',
        variables: readAfterDelete.variables,
        query: readQuery.replace(/\s+/gu, ' ').trim(),
        response: {
          status: readAfterDelete.response.status,
          body: readAfterDelete.response.payload,
        },
      },
    ],
  });
}

async function capturePriceListCreateDkk(capturedAt: string): Promise<string> {
  const query = await readDocument('config/parity-requests/markets/price-list-create-dkk.graphql');
  const deleteQuery = `#graphql
mutation PriceListCreateDkkCleanup($id: ID!) {
  priceListDelete(id: $id) {
    deletedId
    userErrors { field message code }
  }
}`;
  const variables = {
    input: {
      name: `Denmark ${Date.now()}`,
      currency: 'DKK',
      parent: { adjustment: { type: 'PERCENTAGE_DECREASE', value: 10 } },
    },
  };
  const response = await run2026(query, variables);
  assertGraphqlOk('priceListCreate DKK', response);
  assertNoUserErrors('priceListCreate DKK', response, 'priceListCreate');
  const id = nestedId(response, 'priceListCreate', 'priceList');
  const deleteResponse = await run2026(deleteQuery, { id });
  assertGraphqlOk('priceListCreate DKK cleanup', deleteResponse);

  return writeFixture('2026-04', 'price-list-create-dkk.json', {
    capturedAt,
    storeDomain: fixtureStoreDomain,
    apiVersion: '2026-04',
    cases: [{ name: 'priceListCreate DKK success', query, variables, response }],
    cleanup: {
      mutation: 'priceListDelete',
      deletedId: rootPayload(deleteResponse, 'priceListDelete')['deletedId'],
      userErrors: rootPayload(deleteResponse, 'priceListDelete')['userErrors'],
      response: deleteResponse,
    },
    upstreamCalls: [],
  });
}

async function capturePriceListMutationValidation(capturedAt: string): Promise<string> {
  const query = await readDocument(
    'config/parity-requests/markets/price-list-create-invalid-currency-validation.graphql',
  );
  const variables = {
    input: {
      name: 'Codex Invalid Currency',
      currency: 'ZZZ',
    },
  };
  const response = await run2026(query, variables);
  assertGraphqlError('priceListCreate invalid currency', response);

  return writeFixture('2026-04', 'price-list-mutation-validation.json', {
    capturedAt,
    storeDomain: fixtureStoreDomain,
    apiVersion: '2026-04',
    cases: [{ name: 'priceListCreate invalid currency', query, variables, response }],
    upstreamCalls: [],
  });
}

async function capturePriceListFixedPricesByProductUpdate(capturedAt: string): Promise<string> {
  const updateQuery = await readDocument(
    'config/parity-requests/markets/price-list-fixed-prices-by-product-update.graphql',
  );
  const readQuery = await readDocument(
    'config/parity-requests/markets/price-list-fixed-prices-by-product-read.graphql',
  );
  const priceListId = process.env['SHOPIFY_CONFORMANCE_PRICE_LIST_ID'] ?? 'gid://shopify/PriceList/31575376178';
  const productId = process.env['SHOPIFY_CONFORMANCE_PRODUCT_ID'] ?? 'gid://shopify/Product/9801098789170';
  const priceQuery = `product_id:${productId.split('/').at(-1) ?? ''}`;
  const preflightQuery = `#graphql
query MarketsMutationPreflightHydrate($priceListId: ID!, $productIds: [ID!]!, $priceQuery: String) {
  priceList(id: $priceListId) {
    __typename
    id
    name
    currency
    fixedPricesCount
    prices(first: 10, query: $priceQuery, originType: FIXED) {
      edges {
        cursor
        node {
          price { amount currencyCode }
          compareAtPrice { amount currencyCode }
          originType
          variant { id sku product { id title } }
        }
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
  productNodes: nodes(ids: $productIds) {
    __typename
    ... on Product {
      id
      title
      handle
      status
      variants(first: 10) {
        nodes { id title sku price compareAtPrice }
      }
    }
  }
}`;
  const preflightVariables = { priceListId, productIds: [productId], priceQuery };
  const preflight = await run2026(preflightQuery, preflightVariables);
  assertGraphqlOk('price list by product preflight', preflight);
  const fixedPrice = { amount: '881.25', currencyCode: 'CAD' };
  const fixedCompareAtPrice = { amount: '999.99', currencyCode: 'CAD' };
  const successPath = [
    {
      name: 'add product fixed price',
      request: {
        variables: {
          priceListId,
          priceQuery,
          pricesToAdd: [{ productId, price: fixedPrice, compareAtPrice: fixedCompareAtPrice }],
          pricesToDeleteByProductIds: [],
        },
      },
      response: await run2026(updateQuery, {
        priceListId,
        priceQuery,
        pricesToAdd: [{ productId, price: fixedPrice, compareAtPrice: fixedCompareAtPrice }],
        pricesToDeleteByProductIds: [],
      }),
    },
    {
      name: 'read after add',
      request: { variables: { priceListId, priceQuery, originType: 'FIXED' } },
      response: await run2026(readQuery, { priceListId, priceQuery, originType: 'FIXED' }),
    },
    {
      name: 'cleanup delete product fixed price',
      request: { variables: { priceListId, priceQuery, pricesToAdd: [], pricesToDeleteByProductIds: [productId] } },
      response: await run2026(updateQuery, {
        priceListId,
        priceQuery,
        pricesToAdd: [],
        pricesToDeleteByProductIds: [productId],
      }),
    },
    {
      name: 'read after cleanup',
      request: { variables: { priceListId, priceQuery, originType: 'FIXED' } },
      response: await run2026(readQuery, { priceListId, priceQuery, originType: 'FIXED' }),
    },
  ];
  for (const entry of successPath) {
    assertGraphqlOk(entry.name, entry.response);
  }
  const validationBranches = [
    {
      name: 'unknown product add',
      request: {
        variables: {
          priceListId,
          pricesToAdd: [{ productId: missingProductId, price: { amount: '1.00', currencyCode: 'CAD' } }],
          pricesToDeleteByProductIds: [],
        },
      },
      response: await run2026(updateQuery, {
        priceListId,
        priceQuery: null,
        pricesToAdd: [{ productId: missingProductId, price: { amount: '1.00', currencyCode: 'CAD' } }],
        pricesToDeleteByProductIds: [],
      }),
    },
    {
      name: 'unknown product delete',
      request: { variables: { priceListId, pricesToAdd: [], pricesToDeleteByProductIds: [missingProductId] } },
      response: await run2026(updateQuery, {
        priceListId,
        priceQuery: null,
        pricesToAdd: [],
        pricesToDeleteByProductIds: [missingProductId],
      }),
    },
    {
      name: 'unknown price list',
      request: {
        variables: {
          priceListId: missingPriceListId,
          pricesToAdd: [{ productId, price: { amount: '1.00', currencyCode: 'CAD' } }],
          pricesToDeleteByProductIds: [],
        },
      },
      response: await run2026(updateQuery, {
        priceListId: missingPriceListId,
        priceQuery: null,
        pricesToAdd: [{ productId, price: { amount: '1.00', currencyCode: 'CAD' } }],
        pricesToDeleteByProductIds: [],
      }),
    },
  ];
  for (const entry of validationBranches) {
    assertGraphqlOk(entry.name, entry.response);
  }

  return writeFixture('2026-04', 'price-list-fixed-prices-by-product-update-parity.json', {
    storeDomain: fixtureStoreDomain,
    apiVersion: '2026-04',
    capturedAt,
    scope: 'priceListFixedPricesByProductUpdate product-level fixed-price parity',
    setup: {
      priceListId,
      productId,
      priceQuery,
      cleanup: 'The success path deletes the product-level fixed price after the add/read capture.',
    },
    data: payloadData(preflight),
    schemaEvidence: {
      mutationArgs: ['priceListId', 'pricesToAdd', 'pricesToDeleteByProductIds'],
      priceListProductPriceInputFields: ['productId', 'price', 'compareAtPrice'],
      payloadFields: ['priceList', 'pricesToAddProducts', 'pricesToDeleteProducts', 'userErrors'],
      downstreamReadTargets: ['PriceList.prices(query: product_id, originType: FIXED)'],
    },
    successPath,
    validationBranches,
    upstreamCalls: [
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: preflightVariables,
        query: preflightQuery.replace(/\s+/gu, ' ').trim(),
        response: {
          status: preflight.status,
          body: preflight.payload,
        },
      },
    ],
  });
}

async function captureQuantityPricingRules(capturedAt: string): Promise<string> {
  const config2025 = readPinnedConfig('2025-01');
  const adminAccessToken2025 = await getValidConformanceAccessToken({
    adminOrigin: config2025.adminOrigin,
    apiVersion: config2025.apiVersion,
  });
  const client2025 = createAdminGraphqlClient({
    adminOrigin: config2025.adminOrigin,
    apiVersion: config2025.apiVersion,
    headers: buildAdminAuthHeaders(adminAccessToken2025),
  });
  const run2025 = (query: string, variables: JsonRecord = {}) => client2025.runGraphqlRequest(query, variables);
  const updateQuery = await readDocument('config/parity-requests/markets/quantity-pricing-by-variant-update.graphql');
  const readQuery = await readDocument('config/parity-requests/markets/quantity-pricing-price-list-read.graphql');
  const addRulesQuery = await readDocument('config/parity-requests/markets/quantity-rules-add.graphql');
  const deleteRulesQuery = await readDocument('config/parity-requests/markets/quantity-rules-delete.graphql');
  const priceListId =
    process.env['SHOPIFY_CONFORMANCE_QUANTITY_PRICE_LIST_ID'] ?? 'gid://shopify/PriceList/32128106802';
  const variantId =
    process.env['SHOPIFY_CONFORMANCE_PRODUCT_VARIANT_ID'] ?? 'gid://shopify/ProductVariant/49875425296690';
  const preflightQuery = `#graphql
query MarketsMutationPreflightHydrate($priceListId: ID!) {
  priceList(id: $priceListId) {
    __typename
    id
    name
    currency
    fixedPricesCount
    quantityRules(first: 20) {
      edges {
        cursor
        node {
          minimum
          maximum
          increment
          isDefault
          originType
          productVariant { id }
        }
      }
    }
    prices(first: 20, originType: FIXED) {
      edges {
        cursor
        node {
          price { amount currencyCode }
          compareAtPrice { amount currencyCode }
          originType
          variant { id sku product { id title } }
          quantityPriceBreaks(first: 20) {
            edges {
              cursor
              node {
                minimumQuantity
                price { amount currencyCode }
                variant { id }
              }
            }
          }
        }
      }
    }
  }
  products(first: 10) {
    nodes {
      id
      title
      variants(first: 20) {
        nodes { id title sku }
      }
    }
  }
}`;
  const input = {
    pricesToAdd: [{ variantId, price: { amount: '17.00', currencyCode: 'CAD' } }],
    pricesToDeleteByVariantId: [],
    quantityRulesToAdd: [{ variantId, minimum: 2, maximum: 10, increment: 2 }],
    quantityRulesToDeleteByVariantId: [],
    quantityPriceBreaksToAdd: [
      {
        variantId,
        minimumQuantity: 4,
        price: { amount: '15.00', currencyCode: 'CAD' },
      },
    ],
    quantityPriceBreaksToDelete: [],
  };
  const updateVariables = { priceListId, input };
  const setup = await run2025(preflightQuery, updateVariables);
  assertGraphqlOk('quantity pricing setup', setup);
  const update = await run2025(updateQuery, updateVariables);
  assertGraphqlOk('quantity pricing update', update);
  const downstream = await run2025(readQuery, { priceListId });
  assertGraphqlOk('quantity pricing downstream read', downstream);
  const cleanupRuleVariables = { priceListId, variantIds: [variantId] };
  const cleanupRules = await run2025(deleteRulesQuery, cleanupRuleVariables);
  assertGraphqlOk('quantity pricing cleanup quantityRulesDelete', cleanupRules);
  const cleanupPrices = await run2025(updateQuery, {
    priceListId,
    input: {
      pricesToAdd: [],
      pricesToDeleteByVariantId: [variantId],
      quantityRulesToAdd: [],
      quantityRulesToDeleteByVariantId: [],
      quantityPriceBreaksToAdd: [],
      quantityPriceBreaksToDelete: [],
    },
  });
  assertGraphqlOk('quantity pricing cleanup price delete', cleanupPrices);
  const missingPriceListUpdateVariables = { priceListId: missingPriceListId, input };
  const missingPriceListRulesAddVariables = {
    priceListId: missingPriceListId,
    quantityRules: [{ variantId, minimum: 2, maximum: 10, increment: 2 }],
  };
  const missingPriceListRulesDeleteVariables = {
    priceListId: missingPriceListId,
    variantIds: [variantId],
  };
  const missingVariantRulesAddVariables = {
    priceListId,
    quantityRules: [{ variantId: missingVariantId, minimum: 2, maximum: 10, increment: 2 }],
  };
  const missingVariantRulesDeleteVariables = {
    priceListId,
    variantIds: [missingVariantId],
  };
  const cleanupRulePreflight = await run2025(preflightQuery, cleanupRuleVariables);
  assertGraphqlOk('quantity pricing cleanup rule preflight', cleanupRulePreflight);
  const missingPriceListUpdatePreflight = await run2025(preflightQuery, missingPriceListUpdateVariables);
  assertGraphqlOk('quantity pricing missing price list update preflight', missingPriceListUpdatePreflight);
  const missingPriceListRulesAddPreflight = await run2025(preflightQuery, missingPriceListRulesAddVariables);
  assertGraphqlOk('quantity pricing missing price list rules add preflight', missingPriceListRulesAddPreflight);
  const missingPriceListRulesDeletePreflight = await run2025(preflightQuery, missingPriceListRulesDeleteVariables);
  assertGraphqlOk('quantity pricing missing price list rules delete preflight', missingPriceListRulesDeletePreflight);
  const missingVariantRulesAddPreflight = await run2025(preflightQuery, missingVariantRulesAddVariables);
  assertGraphqlOk('quantity pricing missing variant rules add preflight', missingVariantRulesAddPreflight);
  const missingVariantRulesDeletePreflight = await run2025(preflightQuery, missingVariantRulesDeleteVariables);
  assertGraphqlOk('quantity pricing missing variant rules delete preflight', missingVariantRulesDeletePreflight);
  const validationBranches = [
    {
      name: 'quantityPricingByVariantUpdate-unknown-price-list',
      response: responseRootOnly(
        await run2025(updateQuery, missingPriceListUpdateVariables),
        'quantityPricingByVariantUpdate',
      ),
    },
    {
      name: 'quantityRulesAdd-unknown-price-list',
      response: responseRootOnly(await run2025(addRulesQuery, missingPriceListRulesAddVariables), 'quantityRulesAdd'),
    },
    {
      name: 'quantityRulesDelete-unknown-price-list',
      response: responseRootOnly(
        await run2025(deleteRulesQuery, missingPriceListRulesDeleteVariables),
        'quantityRulesDelete',
      ),
    },
    {
      name: 'quantityRulesAdd-unknown-variant',
      response: responseRootOnly(await run2025(addRulesQuery, missingVariantRulesAddVariables), 'quantityRulesAdd'),
    },
    {
      name: 'quantityPricingByVariantUpdate-unknown-variant',
      response: responseRootOnly(
        await run2025(updateQuery, {
          priceListId,
          input: {
            ...input,
            pricesToAdd: [{ variantId: missingVariantId, price: { amount: '17.00', currencyCode: 'CAD' } }],
            quantityRulesToAdd: [],
            quantityPriceBreaksToAdd: [],
          },
        }),
        'quantityPricingByVariantUpdate',
      ),
    },
    {
      name: 'quantityRulesDelete-unknown-variant',
      response: responseRootOnly(
        await run2025(deleteRulesQuery, missingVariantRulesDeleteVariables),
        'quantityRulesDelete',
      ),
    },
  ];

  return writeFixture('2025-01', 'quantity-pricing-rules-parity.json', {
    storeDomain: fixtureStoreDomain,
    apiVersion: '2025-01',
    capturedAt,
    scope: 'quantity pricing/rules live capture',
    setup: {
      priceListId,
      variantId,
      priceListContext: 'existing CompanyLocationCatalog-backed price list in disposable conformance shop',
    },
    data: payloadData(setup),
    schemaEvidence: {
      quantityPricingByVariantUpdateArgs: ['priceListId', 'input'],
      quantityRulesAddArgs: ['priceListId', 'quantityRules'],
      quantityRulesDeleteArgs: ['priceListId', 'variantIds'],
      quantityPricingByVariantUpdateInputFields: [
        'quantityPriceBreaksToAdd',
        'quantityPriceBreaksToDelete',
        'quantityPriceBreaksToDeleteByVariantId',
        'quantityRulesToAdd',
        'quantityRulesToDeleteByVariantId',
        'pricesToAdd',
        'pricesToDeleteByVariantId',
      ],
      downstreamReadTargets: ['PriceList.quantityRules', 'PriceList.prices(originType: FIXED).quantityPriceBreaks'],
    },
    successPath: [
      {
        name: 'quantityPricingByVariantUpdate-add',
        request: { variables: { priceListId, input } },
        response: responseDataOnly(update),
      },
      {
        name: 'downstream-price-list-read',
        request: { variables: { priceListId } },
        response: responseDataOnly(downstream),
      },
      {
        name: 'cleanup',
        response: {
          quantityRulesDelete: rootPayload(cleanupRules, 'quantityRulesDelete'),
          quantityPricingByVariantUpdate: rootPayload(cleanupPrices, 'quantityPricingByVariantUpdate'),
        },
      },
    ],
    validationBranches,
    upstreamCalls: [
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: updateVariables,
        query: preflightQuery.replace(/\s+/gu, ' ').trim(),
        response: { status: setup.status, body: setup.payload },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: cleanupRuleVariables,
        query: preflightQuery.replace(/\s+/gu, ' ').trim(),
        response: { status: cleanupRulePreflight.status, body: cleanupRulePreflight.payload },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: missingPriceListUpdateVariables,
        query: preflightQuery.replace(/\s+/gu, ' ').trim(),
        response: { status: missingPriceListUpdatePreflight.status, body: missingPriceListUpdatePreflight.payload },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: missingPriceListRulesAddVariables,
        query: preflightQuery.replace(/\s+/gu, ' ').trim(),
        response: { status: missingPriceListRulesAddPreflight.status, body: missingPriceListRulesAddPreflight.payload },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: missingPriceListRulesDeleteVariables,
        query: preflightQuery.replace(/\s+/gu, ' ').trim(),
        response: {
          status: missingPriceListRulesDeletePreflight.status,
          body: missingPriceListRulesDeletePreflight.payload,
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: missingVariantRulesAddVariables,
        query: preflightQuery.replace(/\s+/gu, ' ').trim(),
        response: { status: missingVariantRulesAddPreflight.status, body: missingVariantRulesAddPreflight.payload },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: missingVariantRulesDeleteVariables,
        query: preflightQuery.replace(/\s+/gu, ' ').trim(),
        response: {
          status: missingVariantRulesDeletePreflight.status,
          body: missingVariantRulesDeletePreflight.payload,
        },
      },
    ],
  });
}

const capturedAt = new Date().toISOString();
const outputPaths = [
  await captureQuantityPricingRules(capturedAt),
  await captureCatalogCreateMissingContext(capturedAt),
  await captureCatalogLifecycleValidation(capturedAt),
  await captureMarketCreateStatusEnabledMismatch(capturedAt),
  await captureMarketLocalizableEmptyRead(capturedAt),
  await captureMarketLocalizationValidation(capturedAt),
  await captureMarketLocalizationsRegisterTooManyKeys(capturedAt),
  await captureMarketWebPresenceDelete(capturedAt),
  await captureMarketWebPresenceValidation(capturedAt),
  await capturePriceListCreateDkk(capturedAt),
  await capturePriceListFixedPricesByProductUpdate(capturedAt),
  await capturePriceListMutationValidation(capturedAt),
];

console.log(
  JSON.stringify(
    {
      ok: true,
      storeDomain: fixtureStoreDomain,
      outputPaths,
    },
    null,
    2,
  ),
);
