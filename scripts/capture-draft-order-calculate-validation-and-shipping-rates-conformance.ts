/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const fixtureRelativePath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'orders',
  'draftOrderCalculate-validation-and-shipping-rates.json',
);
const specRelativePath = path.join(
  'config',
  'parity-specs',
  'orders',
  'draftOrderCalculate-validation-and-shipping-rates.json',
);
const requestDir = path.join(repoRoot, 'config', 'parity-requests', 'orders');

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing required capture value: ${label}`);
  }
  return value;
}

function valueAt(value: unknown, segments: string[]): unknown {
  let current = value;
  for (const segment of segments) {
    if (Array.isArray(current)) {
      current = current[Number.parseInt(segment, 10)];
    } else {
      current = asRecord(current)?.[segment];
    }
  }
  return current;
}

function assertSuccessfulRoot(response: JsonRecord, rootName: string, label: string): JsonRecord {
  const root = asRecord(valueAt(response, ['data', rootName]));
  const topLevelErrors = asArray(response['errors']);
  const userErrors = asArray(root?.['userErrors']);
  if (!root || topLevelErrors.length > 0 || userErrors.length > 0) {
    throw new Error(`${label} failed: ${JSON.stringify({ topLevelErrors, userErrors, response }, null, 2)}`);
  }
  return root;
}

async function capture(
  document: string,
  variables: JsonRecord,
): Promise<{
  document: string;
  variables: JsonRecord;
  response: JsonRecord;
  status: number;
}> {
  const result: ConformanceGraphqlResult<JsonRecord> = await runGraphqlRequest<JsonRecord>(document, variables);
  const response = asRecord(result.payload);
  if (result.status < 200 || result.status >= 300 || !response) {
    throw new Error(`GraphQL capture failed: ${JSON.stringify(result, null, 2)}`);
  }
  if (asArray(response['errors']).length > 0) {
    throw new Error(`GraphQL capture returned top-level errors: ${JSON.stringify(response['errors'], null, 2)}`);
  }
  return { document, variables, response, status: result.status };
}

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

async function writeJson(relativePath: string, value: unknown): Promise<void> {
  const absolutePath = path.join(repoRoot, relativePath);
  await mkdir(path.dirname(absolutePath), { recursive: true });
  await writeFile(absolutePath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

const productCreateDocument = `#graphql
  mutation DraftOrderShippingRateProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        variants(first: 1) { nodes { id title } }
      }
      userErrors { field message }
    }
  }
`;

const productVariantUpdateDocument = `#graphql
  mutation DraftOrderShippingRateVariantUpdate(
    $productId: ID!
    $variants: [ProductVariantsBulkInput!]!
  ) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      productVariants {
        id
        title
        sku
        taxable
        price
        inventoryItem { requiresShipping }
      }
      userErrors { field message code }
    }
  }
`;

const locationsDocument = `#graphql
  query DraftOrderShippingRateLocations {
    locationsAvailableForDeliveryProfilesConnection(first: 50) {
      nodes { id isActive isFulfillmentService }
    }
  }
`;

const deliveryProfileRemoveDocument = `#graphql
  mutation DraftOrderShippingRateProfileRemove($id: ID!) {
    deliveryProfileRemove(id: $id) {
      job { id done }
      userErrors { field message }
    }
  }
`;

const productDeleteDocument = `#graphql
  mutation DraftOrderShippingRateProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

const draftOrderDeleteDocument = `#graphql
  mutation DraftOrderShippingRateDraftDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors { field message }
    }
  }
`;

const variantHydrateDocument =
  'query OrdersDraftOrderVariantHydrate($id: ID!) {\n  productVariant(id: $id) { id title sku taxable price inventoryItem { requiresShipping } product { title } }\n}\n';
const shopPricingHydrateDocument =
  'query DraftProxyShopPricingHydrate { shop { currencyCode taxesIncluded taxShipping } }';
// Must byte-match DRAFT_ORDER_DELIVERY_PROFILES_HYDRATE_QUERY in the Rust
// draft-order runtime so parity replays the exact production read-only query.
const deliveryProfilesHydrateDocument = `query OrdersDraftOrderDeliveryProfilesHydrate {
  deliveryProfiles(first: 10) {
    nodes {
      id
      name
      default
      profileItems(first: 10) {
        nodes {
          variants(first: 10) { nodes { id } }
        }
      }
      profileLocationGroups {
        locationGroup {
          id
          locations(first: 10) { nodes { id } }
        }
        locationGroupZones(first: 10) {
          nodes {
            zone {
              id
              countries {
                code { countryCode restOfWorld }
                provinces { code }
              }
            }
            methodDefinitions(first: 10) {
              nodes {
                id
                name
                active
                rateProvider {
                  ... on DeliveryRateDefinition { id price { amount currencyCode } }
                  ... on DeliveryParticipant { id fixedFee { amount currencyCode } percentageOfRateFee }
                }
                methodConditions {
                  id
                  field
                  operator
                  conditionCriteria {
                    __typename
                    ... on MoneyV2 { amount currencyCode }
                    ... on Weight { unit value }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}`;

const calculateDocument = await readRequest('draftOrderCalculate-validation-and-shipping-rates.graphql');
const createWithRateDocument = await readRequest('draftOrderCalculate-shipping-rate-create.graphql');
const updateWithRateDocument = await readRequest('draftOrderCalculate-shipping-rate-update.graphql');
const deliveryProfileCreateDocument = await readFile(
  path.join(
    repoRoot,
    'config',
    'parity-requests',
    'shipping-fulfillments',
    'delivery-profile-lifecycle-create.graphql',
  ),
  'utf8',
);

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
let productId: string | null = null;
let variantId: string | null = null;
let deliveryProfileId: string | null = null;
let draftOrderId: string | null = null;
const cleanup: JsonRecord = {};
let fixture: JsonRecord | null = null;

try {
  const productCreate = await capture(productCreateDocument, {
    product: {
      title: `Draft order shipping rate ${stamp}`,
      status: 'ACTIVE',
    },
  });
  const productRoot = assertSuccessfulRoot(productCreate.response, 'productCreate', 'productCreate');
  productId = requireString(valueAt(productRoot, ['product', 'id']), 'product id');
  variantId = requireString(valueAt(productRoot, ['product', 'variants', 'nodes', '0', 'id']), 'variant id');

  const variantUpdate = await capture(productVariantUpdateDocument, {
    productId,
    variants: [
      {
        id: variantId,
        price: '25.00',
        taxable: true,
        inventoryItem: {
          sku: `DRAFT-SHIPPING-${stamp}`,
          requiresShipping: true,
        },
      },
    ],
  });
  assertSuccessfulRoot(variantUpdate.response, 'productVariantsBulkUpdate', 'productVariantsBulkUpdate');

  const shopPricingHydrate = await capture(shopPricingHydrateDocument, {});
  const shopCurrencyCode = requireString(
    valueAt(shopPricingHydrate.response, ['data', 'shop', 'currencyCode']),
    'shop currency code',
  );

  const locations = await capture(locationsDocument, {});
  const locationNodes = asArray(
    valueAt(locations.response, ['data', 'locationsAvailableForDeliveryProfilesConnection', 'nodes']),
  );
  const location = locationNodes
    .map(asRecord)
    .find(
      (candidate) =>
        candidate?.['isActive'] !== false && candidate?.['isFulfillmentService'] !== true && candidate?.['id'],
    );
  const locationId = requireString(location?.['id'], 'active delivery-profile location id');

  const deliveryProfileCreate = await capture(deliveryProfileCreateDocument, {
    profile: {
      name: `Draft order shipping rates ${stamp}`,
      variantsToAssociate: [variantId],
      locationGroupsToCreate: [
        {
          locations: [locationId],
          zonesToCreate: [
            {
              name: 'United States',
              countries: [{ code: 'US', includeAllProvinces: true }],
              methodDefinitionsToCreate: [
                {
                  name: 'Conformance Standard',
                  description: 'Captured fixed draft-order shipping rate',
                  active: true,
                  rateDefinition: { price: { amount: '7.25', currencyCode: shopCurrencyCode } },
                },
                {
                  name: 'Conformance Express',
                  description: 'Captured expedited draft-order shipping rate',
                  active: true,
                  rateDefinition: { price: { amount: '12.00', currencyCode: shopCurrencyCode } },
                },
              ],
            },
          ],
        },
      ],
    },
  });
  const profileRoot = assertSuccessfulRoot(
    deliveryProfileCreate.response,
    'deliveryProfileCreate',
    'deliveryProfileCreate',
  );
  deliveryProfileId = requireString(valueAt(profileRoot, ['profile', 'id']), 'delivery profile id');

  const shippingAddress = {
    firstName: 'Rate',
    lastName: 'Recipient',
    address1: '11 Wall Street',
    city: 'New York',
    provinceCode: 'NY',
    countryCode: 'US',
    zip: '10005',
  };
  const lineItems = [{ variantId, quantity: 1 }];
  const calculateVariables = {
    emptyLineItems: { lineItems: [] },
    invalidEmail: {
      email: 'bad email',
      lineItems: [{ title: 'Invalid email item', quantity: 1, originalUnitPrice: '1.00' }],
    },
    availableShippingRatesEmpty: { lineItems },
    availableShippingRatesNoMatch: {
      lineItems,
      shippingAddress: { ...shippingAddress, provinceCode: 'ON', countryCode: 'CA', zip: 'M5H 2N2' },
    },
    availableShippingRatesMatching: { lineItems, shippingAddress },
    paymentTermsTemplateId: {
      paymentTerms: {
        paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
        paymentSchedules: [{ issuedAt: '2026-01-01T00:00:00Z' }],
      },
      lineItems: [{ title: 'Payment terms item', quantity: 1, originalUnitPrice: '1.00' }],
    },
  };

  let calculate = await capture(calculateDocument, calculateVariables);
  for (let attempt = 1; attempt < 6; attempt += 1) {
    const matchingRates = asArray(
      valueAt(calculate.response, [
        'data',
        'availableShippingRatesMatching',
        'calculatedDraftOrder',
        'availableShippingRates',
      ]),
    );
    if (matchingRates.length > 0) {
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 1_000));
    calculate = await capture(calculateDocument, calculateVariables);
  }
  const matchingRates = asArray(
    valueAt(calculate.response, [
      'data',
      'availableShippingRatesMatching',
      'calculatedDraftOrder',
      'availableShippingRates',
    ]),
  ).map(asRecord);
  const standardRate = matchingRates.find((rate) => rate?.['title'] === 'Conformance Standard');
  const expressRate = matchingRates.find((rate) => rate?.['title'] === 'Conformance Express');
  const standardHandle = requireString(standardRate?.['handle'], 'standard shipping-rate handle');
  const expressHandle = requireString(expressRate?.['handle'], 'express shipping-rate handle');

  const createWithRateVariables = {
    input: {
      email: `draft-shipping-${stamp}@example.com`,
      lineItems,
      shippingAddress,
      shippingLine: { shippingRateHandle: standardHandle },
    },
  };
  const createWithRate = await capture(createWithRateDocument, createWithRateVariables);
  const createRoot = assertSuccessfulRoot(createWithRate.response, 'draftOrderCreate', 'draftOrderCreate with rate');
  draftOrderId = requireString(valueAt(createRoot, ['draftOrder', 'id']), 'draft order id');

  const updateWithRateVariables = {
    id: draftOrderId,
    input: { shippingLine: { shippingRateHandle: expressHandle } },
  };
  const updateWithRate = await capture(updateWithRateDocument, updateWithRateVariables);
  assertSuccessfulRoot(updateWithRate.response, 'draftOrderUpdate', 'draftOrderUpdate with rate');

  const variantHydrate = await capture(variantHydrateDocument, { id: variantId });
  const deliveryProfilesHydrate = await capture(deliveryProfilesHydrateDocument, {});

  fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    document: calculateDocument,
    variables: calculateVariables,
    setup: {
      productCreate,
      variantUpdate,
      locations,
      deliveryProfileCreate,
      selectedLocationId: locationId,
      selectedVariantId: variantId,
      selectedDeliveryProfileId: deliveryProfileId,
      matchingAddress: shippingAddress,
      configuredRates: [
        { title: 'Conformance Standard', amount: '7.25', currencyCode: shopCurrencyCode },
        { title: 'Conformance Express', amount: '12.00', currencyCode: shopCurrencyCode },
      ],
    },
    calculate,
    createWithRate,
    updateWithRate,
    upstreamCalls: [
      {
        operationName: 'OrdersDraftOrderVariantHydrate',
        variables: { id: variantId },
        query: variantHydrateDocument,
        response: { status: variantHydrate.status, body: variantHydrate.response },
      },
      {
        operationName: 'DraftProxyShopPricingHydrate',
        variables: {},
        query: shopPricingHydrateDocument,
        response: { status: shopPricingHydrate.status, body: shopPricingHydrate.response },
      },
      {
        operationName: 'OrdersDraftOrderDeliveryProfilesHydrate',
        variables: {},
        query: deliveryProfilesHydrateDocument,
        response: { status: deliveryProfilesHydrate.status, body: deliveryProfilesHydrate.response },
      },
    ],
  };
} finally {
  if (draftOrderId) {
    cleanup['draftOrderDelete'] = await capture(draftOrderDeleteDocument, { input: { id: draftOrderId } });
  }
  if (deliveryProfileId) {
    cleanup['deliveryProfileRemove'] = await capture(deliveryProfileRemoveDocument, { id: deliveryProfileId });
  }
  if (productId) {
    cleanup['productDelete'] = await capture(productDeleteDocument, { input: { id: productId } });
  }
}

if (!fixture) {
  throw new Error('Capture did not produce a fixture.');
}
fixture['cleanup'] = cleanup;

const paritySpec = {
  scenarioId: 'draftOrderCalculate-validation-and-shipping-rates',
  operationNames: ['draftOrderCalculate', 'draftOrderCreate', 'draftOrderUpdate'],
  scenarioStatus: 'captured',
  assertionKinds: [
    'payload-shape',
    'user-errors-parity',
    'nullability-parity',
    'runtime-staging',
    'downstream-read-parity',
  ],
  liveCaptureFiles: [fixtureRelativePath],
  proxyRequest: {
    documentPath: 'config/parity-requests/orders/draftOrderCalculate-shipping-rate-create.graphql',
    variablesCapturePath: '$.createWithRate.variables',
    apiVersion,
  },
  comparisonMode: 'captured-vs-proxy-request',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [
      {
        path: '$.draftOrder.id',
        matcher: 'shopify-gid:DraftOrder',
        reason:
          'The proxy stages a deterministic local DraftOrder while Shopify returns the disposable live DraftOrder id.',
      },
      {
        path: '$.draftOrder.shippingLine.shippingRateHandle',
        matcher: 'non-empty-string',
        reason:
          'Shopify signs its opaque rate handle with a shop-owned key; the proxy emits a deterministic local JWT-shaped handle and validates its decoded rate fields against effective delivery-profile state.',
      },
    ],
    targets: [
      {
        name: 'create-with-calculated-shipping-rate',
        capturePath: '$.createWithRate.response.data.draftOrderCreate',
        proxyPath: '$.data.draftOrderCreate',
      },
      {
        name: 'update-with-calculated-shipping-rate',
        capturePath: '$.updateWithRate.response.data.draftOrderUpdate',
        proxyPath: '$.data.draftOrderUpdate',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/draftOrderCalculate-shipping-rate-update.graphql',
          variables: {
            id: { fromPrimaryProxyPath: '$.data.draftOrderCreate.draftOrder.id' },
            input: { fromCapturePath: '$.updateWithRate.variables.input' },
          },
          apiVersion,
        },
      },
      ...[
        ['calculate-empty-line-items-validation', 'emptyLineItems'],
        ['calculate-invalid-email-validation', 'invalidEmail'],
        ['calculate-no-address-shipping-rates-empty', 'availableShippingRatesEmpty'],
        ['calculate-no-match-shipping-rates-empty', 'availableShippingRatesNoMatch'],
        ['calculate-matching-shipping-rates', 'availableShippingRatesMatching'],
        ['calculate-payment-terms-template', 'paymentTermsTemplateId'],
      ].map(([name, responseKey]) => ({
        name,
        capturePath: `$.calculate.response.data.${responseKey}`,
        proxyPath: `$.data.${responseKey}`,
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/draftOrderCalculate-validation-and-shipping-rates.graphql',
          variablesCapturePath: '$.calculate.variables',
          apiVersion,
        },
        ...(responseKey === 'availableShippingRatesMatching'
          ? {
              expectedDifferences: [
                {
                  path: '$.calculatedDraftOrder.availableShippingRates[0].handle',
                  matcher: 'non-empty-string',
                  reason:
                    'Shopify signs opaque rate handles with a shop-owned key; the proxy emits deterministic local JWT-shaped handles.',
                },
                {
                  path: '$.calculatedDraftOrder.availableShippingRates[1].handle',
                  matcher: 'non-empty-string',
                  reason:
                    'Shopify signs opaque rate handles with a shop-owned key; the proxy emits deterministic local JWT-shaped handles.',
                },
              ],
            }
          : {}),
      })),
    ],
  },
  notes:
    'Live Admin GraphQL 2026-04 evidence creates a disposable shippable variant and US-only delivery profile with two active fixed rates, captures matching/no-address/no-match calculation branches, then feeds the returned opaque handles into draftOrderCreate and draftOrderUpdate before cleanup. Proxy replay hydrates the exact read-only delivery-profile slice from the recorded cassette; normal supported mutations remain local.',
};

await writeJson(fixtureRelativePath, fixture);
await writeJson(specRelativePath, paritySpec);

process.stdout.write(
  `${JSON.stringify(
    {
      fixturePath: fixtureRelativePath,
      specPath: specRelativePath,
      matchingRates: valueAt(fixture, [
        'calculate',
        'response',
        'data',
        'availableShippingRatesMatching',
        'calculatedDraftOrder',
        'availableShippingRates',
      ]),
      cleanup,
    },
    null,
    2,
  )}\n`,
);
