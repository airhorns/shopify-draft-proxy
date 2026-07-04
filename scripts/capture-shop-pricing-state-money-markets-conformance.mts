/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';
import type { ConformanceGraphqlResult } from './conformance-graphql-client.js';

type Operation = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

const SHOP_PRICING_HYDRATE_QUERY =
  'query DraftProxyShopPricingHydrate { shop { currencyCode taxesIncluded taxShipping } }';

const sellingPlanGroupDeleteMutation = `#graphql
  mutation ShopPricingStateSellingPlanGroupDelete($id: ID!) {
    sellingPlanGroupDelete(id: $id) {
      deletedSellingPlanGroupId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const marketDeleteMutation = `#graphql
  mutation ShopPricingStateMarketDelete($id: ID!) {
    marketDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation ShopPricingStateProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const capture = await createConformanceCapture();
const productCreateMutation = await capture.readRequest('products', 'shop-pricing-state-product-create.graphql');
const sellingPlanGroupCreateMutation = await capture.readRequest(
  'selling-plans',
  'shop-pricing-state-selling-plan-group-create.graphql',
);
const marketCreateMutation = await capture.readRequest('markets', 'shop-pricing-state-market-create.graphql');
const marketsResolvedValuesQuery = await capture.readRequest(
  'markets',
  'shop-pricing-state-markets-resolved-values.graphql',
);

function assertGraphqlSuccess(operation: Operation, label: string): void {
  if (operation.response.status < 200 || operation.response.status >= 300 || operation.response.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(operation.response, null, 2)}`);
  }
}

async function runOperation(label: string, query: string, variables: JsonRecord): Promise<Operation> {
  const operation = {
    query,
    variables,
    response: await capture.runGraphqlRequest<JsonRecord>(query, variables),
  };
  assertGraphqlSuccess(operation, label);
  return operation;
}

function operationRoot(operation: Operation, rootName: string, label: string): JsonRecord {
  const root = readRecord(readRecord(operation.response.payload.data)?.[rootName]);
  if (!root) {
    throw new Error(`${label} missing ${rootName}: ${JSON.stringify(operation.response, null, 2)}`);
  }
  return root;
}

function assertNoUserErrors(root: JsonRecord, label: string): void {
  const userErrors = readArray(root['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertMoneyCurrency(value: unknown, expectedCurrency: string, label: string): void {
  const money = readRecord(value);
  const currencyCode = money?.['currencyCode'];
  if (currencyCode !== expectedCurrency) {
    throw new Error(`${label} expected ${expectedCurrency}, got ${JSON.stringify(value)}`);
  }
}

function requireShopCurrency(operation: Operation): string {
  const shop = readRecord(readRecord(operation.response.payload.data)?.['shop']);
  const currencyCode = requireString(shop?.['currencyCode'], 'shop.currencyCode');
  if (currencyCode === 'USD') {
    throw new Error('shop pricing-state parity must be recorded against a non-USD shop.');
  }
  return currencyCode;
}

function assertProductCreate(operation: Operation, expectedCurrency: string): string {
  const root = operationRoot(operation, 'productCreate', 'productCreate');
  assertNoUserErrors(root, 'productCreate');
  const product = readRecord(root['product']);
  if (!product) {
    throw new Error(`productCreate did not return a product: ${JSON.stringify(root, null, 2)}`);
  }
  const priceRangeV2 = readRecord(product['priceRangeV2']);
  const priceRange = readRecord(product['priceRange']);
  assertMoneyCurrency(
    priceRangeV2?.['minVariantPrice'],
    expectedCurrency,
    'productCreate.priceRangeV2.minVariantPrice',
  );
  assertMoneyCurrency(
    priceRangeV2?.['maxVariantPrice'],
    expectedCurrency,
    'productCreate.priceRangeV2.maxVariantPrice',
  );
  assertMoneyCurrency(priceRange?.['minVariantPrice'], expectedCurrency, 'productCreate.priceRange.minVariantPrice');
  assertMoneyCurrency(priceRange?.['maxVariantPrice'], expectedCurrency, 'productCreate.priceRange.maxVariantPrice');
  return requireString(product['id'], 'productCreate.product.id');
}

function assertSellingPlanGroupCreate(operation: Operation, expectedCurrency: string): string {
  const root = operationRoot(operation, 'sellingPlanGroupCreate', 'sellingPlanGroupCreate');
  assertNoUserErrors(root, 'sellingPlanGroupCreate');
  const group = readRecord(root['sellingPlanGroup']);
  if (!group) {
    throw new Error(`sellingPlanGroupCreate did not return a group: ${JSON.stringify(root, null, 2)}`);
  }
  const product = readRecord(readArray(readRecord(group['products'])?.['nodes'])[0]);
  if (!product) {
    throw new Error(`sellingPlanGroupCreate did not return an attached product: ${JSON.stringify(group, null, 2)}`);
  }
  const priceRangeV2 = readRecord(product['priceRangeV2']);
  assertMoneyCurrency(
    priceRangeV2?.['minVariantPrice'],
    expectedCurrency,
    'sellingPlanGroup.products[0].priceRangeV2.minVariantPrice',
  );
  assertMoneyCurrency(
    priceRangeV2?.['maxVariantPrice'],
    expectedCurrency,
    'sellingPlanGroup.products[0].priceRangeV2.maxVariantPrice',
  );

  const sellingPlans = readArray(readRecord(group['sellingPlans'])?.['nodes']);
  const fixedCurrencies = sellingPlans.flatMap((plan) =>
    readArray(readRecord(plan)?.['pricingPolicies']).flatMap((policy) => {
      const adjustmentValue = readRecord(readRecord(policy)?.['adjustmentValue']);
      const currencyCode = adjustmentValue?.['currencyCode'];
      return typeof currencyCode === 'string' ? [currencyCode] : [];
    }),
  );
  if (fixedCurrencies.length < 2 || fixedCurrencies.some((currencyCode) => currencyCode !== expectedCurrency)) {
    throw new Error(
      `sellingPlanGroup fixed pricing currencies mismatch: expected ${expectedCurrency}, got ${JSON.stringify(
        fixedCurrencies,
      )}`,
    );
  }
  return requireString(group['id'], 'sellingPlanGroupCreate.sellingPlanGroup.id');
}

function assertMarketCreate(operation: Operation, expectedCurrency: string): string {
  const root = operationRoot(operation, 'marketCreate', 'marketCreate');
  assertNoUserErrors(root, 'marketCreate');
  const market = readRecord(root['market']);
  if (!market) {
    throw new Error(`marketCreate did not return a market: ${JSON.stringify(root, null, 2)}`);
  }
  const baseCurrency = readRecord(readRecord(market['currencySettings'])?.['baseCurrency']);
  if (baseCurrency?.['currencyCode'] !== expectedCurrency) {
    throw new Error(`marketCreate baseCurrency mismatch: ${JSON.stringify(market['currencySettings'], null, 2)}`);
  }
  const priceInclusions = readRecord(market['priceInclusions']);
  if (
    priceInclusions?.['inclusiveTaxPricingStrategy'] !== 'INCLUDES_TAXES_IN_PRICE' ||
    priceInclusions?.['inclusiveDutiesPricingStrategy'] !== 'INCLUDE_DUTIES_IN_PRICE'
  ) {
    throw new Error(`marketCreate priceInclusions mismatch: ${JSON.stringify(priceInclusions, null, 2)}`);
  }
  return requireString(market['id'], 'marketCreate.market.id');
}

function assertMarketsResolvedValues(operation: Operation, expectedCurrency: string): void {
  const resolved = readRecord(readRecord(operation.response.payload.data)?.['marketsResolvedValues']);
  if (!resolved) {
    throw new Error(`marketsResolvedValues missing response: ${JSON.stringify(operation.response, null, 2)}`);
  }
  const priceInclusivity = readRecord(resolved['priceInclusivity']);
  if (
    resolved['currencyCode'] !== expectedCurrency ||
    priceInclusivity?.['taxesIncluded'] !== true ||
    priceInclusivity?.['dutiesIncluded'] !== false
  ) {
    throw new Error(`marketsResolvedValues mismatch: ${JSON.stringify(resolved, null, 2)}`);
  }
}

const cleanup: Record<string, ConformanceGraphqlResult<JsonRecord> | string> = {};
const operations: Record<string, Operation> = {};
let productId: string | null = null;
let groupId: string | null = null;
let marketId: string | null = null;

try {
  operations['shopPricingHydrate'] = await runOperation('shopPricingHydrate', SHOP_PRICING_HYDRATE_QUERY, {});
  const expectedCurrency = requireShopCurrency(operations['shopPricingHydrate']);

  operations['productCreate'] = await runOperation('productCreate', productCreateMutation, {
    product: {
      title: `Draft Proxy Pricing State ${capture.stamp}`,
      status: 'DRAFT',
      productOptions: [
        {
          name: 'Color',
          values: [{ name: 'Red' }],
        },
      ],
    },
  });
  productId = assertProductCreate(operations['productCreate'], expectedCurrency);

  operations['sellingPlanGroupCreate'] = await runOperation('sellingPlanGroupCreate', sellingPlanGroupCreateMutation, {
    input: {
      name: `Pricing state group ${capture.stamp}`,
      options: ['Delivery frequency', 'Billing cadence'],
      sellingPlansToCreate: [
        {
          name: 'Monthly fixed',
          options: ['Monthly fixed', 'Monthly billing'],
          category: 'SUBSCRIPTION',
          billingPolicy: { recurring: { interval: 'MONTH', intervalCount: 1 } },
          deliveryPolicy: { recurring: { interval: 'MONTH', intervalCount: 1 } },
          pricingPolicies: [
            {
              fixed: {
                adjustmentType: 'FIXED_AMOUNT',
                adjustmentValue: { fixedValue: '5.0' },
              },
            },
          ],
        },
        {
          name: 'Annual fixed',
          options: ['Annual fixed', 'Annual billing'],
          category: 'SUBSCRIPTION',
          billingPolicy: { recurring: { interval: 'YEAR', intervalCount: 1 } },
          deliveryPolicy: { recurring: { interval: 'YEAR', intervalCount: 1 } },
          pricingPolicies: [
            {
              fixed: {
                adjustmentType: 'FIXED_AMOUNT',
                adjustmentValue: { fixedValue: '8.0' },
              },
            },
          ],
        },
      ],
    },
    resources: { productIds: [productId] },
  });
  groupId = assertSellingPlanGroupCreate(operations['sellingPlanGroupCreate'], expectedCurrency);

  operations['marketCreate'] = await runOperation('marketCreate', marketCreateMutation, {
    input: {
      name: `Pricing State Brazil ${capture.stamp}`,
      status: 'ACTIVE',
      conditions: {
        regionsCondition: {
          regions: [{ countryCode: 'BR' }],
        },
      },
      currencySettings: {
        localCurrencies: true,
      },
      priceInclusions: {
        taxPricingStrategy: 'INCLUDES_TAXES_IN_PRICE',
        dutiesPricingStrategy: 'INCLUDE_DUTIES_IN_PRICE',
      },
    },
  });
  marketId = assertMarketCreate(operations['marketCreate'], expectedCurrency);

  operations['marketsResolvedValues'] = await runOperation('marketsResolvedValues', marketsResolvedValuesQuery, {
    buyerSignal: { countryCode: 'BR' },
  });
  assertMarketsResolvedValues(operations['marketsResolvedValues'], expectedCurrency);
} finally {
  if (groupId) {
    cleanup['sellingPlanGroupDelete'] = await capture.runGraphqlRequest<JsonRecord>(sellingPlanGroupDeleteMutation, {
      id: groupId,
    });
  }
  if (marketId) {
    cleanup['marketDelete'] = await capture.runGraphqlRequest<JsonRecord>(marketDeleteMutation, { id: marketId });
  }
  if (productId) {
    cleanup['productDelete'] = await capture.runGraphqlRequest<JsonRecord>(productDeleteMutation, {
      input: { id: productId },
    });
  }
}

const expectedShopCurrency = requireShopCurrency(operations['shopPricingHydrate']);
const outputPath = capture.fixturePath('markets', 'shop-pricing-state-money-and-markets.json');
await capture.writeJson(outputPath, {
  metadata: {
    scenario: 'Shop pricing state drives money fields and market price inclusivity',
    storeDomain: capture.storeDomain,
    apiVersion: capture.apiVersion,
    capturedAt: new Date().toISOString(),
    expectedShopCurrency,
    buyerCountryCode: 'BR',
    expectedResolvedPriceInclusivity: {
      dutiesIncluded: false,
      taxesIncluded: true,
    },
  },
  operations,
  cleanup,
  upstreamCalls: [
    {
      operationName: 'DraftProxyShopPricingHydrate',
      query: SHOP_PRICING_HYDRATE_QUERY,
      variables: {},
      response: {
        status: operations['shopPricingHydrate'].response.status,
        body: operations['shopPricingHydrate'].response.payload,
      },
    },
  ],
});

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      storeDomain: capture.storeDomain,
      apiVersion: capture.apiVersion,
      expectedShopCurrency,
      productId,
      groupId,
      marketId,
    },
    null,
    2,
  ),
);
