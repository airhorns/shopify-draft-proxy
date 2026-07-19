/* oxlint-disable no-console -- CLI recorder intentionally writes capture status to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  operationName: string;
  query: string;
  variables: JsonRecord;
  result: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
if (apiVersion !== '2026-04') {
  throw new Error(`This recorder requires Admin GraphQL 2026-04, received ${apiVersion}.`);
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const outputPath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'orders',
  'draft-order-available-delivery-options.json',
);

const locationsQuery = `#graphql
  query DraftOrderDeliveryOptionLocations {
    shop {
      currencyCode
    }
    locationsAvailableForDeliveryProfilesConnection(first: 250) {
      nodes {
        id
        name
        isActive
        isFulfillmentService
        fulfillsOnlineOrders
        shipsInventory
        hasActiveInventory
        address {
          address1
          city
          provinceCode
          countryCode
          zip
        }
        localPickupSettingsV2 {
          pickupTime
          instructions
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    deliveryProfiles(first: 10) {
      nodes {
        id
        name
        default
      }
    }
  }
`;

const productCreateMutation = `#graphql
  mutation DraftOrderDeliveryOptionProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        status
        variants(first: 1) {
          nodes {
            id
            title
            price
            inventoryPolicy
            inventoryItem {
              id
              tracked
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

const customerCreateMutation = `#graphql
  mutation DraftOrderDeliveryOptionCustomerCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        id
        firstName
        lastName
        email
        defaultAddress {
          address1
          city
          provinceCode
          countryCodeV2
          zip
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const variantUpdateMutation = `#graphql
  mutation DraftOrderDeliveryOptionVariantUpdate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product {
        id
      }
      productVariants {
        id
        price
        inventoryPolicy
        inventoryItem {
          id
          tracked
          requiresShipping
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const inventorySetQuantitiesMutation = `#graphql
  mutation DraftOrderDeliveryOptionInventorySet($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
    inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        id
        changes {
          name
          delta
          quantityAfterChange
          item {
            id
          }
          location {
            id
            name
          }
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

const deliveryProfileUpdateMutation = `#graphql
  mutation DraftOrderDeliveryOptionProfileUpdate($id: ID!, $profile: DeliveryProfileInput!) {
    deliveryProfileUpdate(id: $id, profile: $profile) {
      profile {
        id
        name
        default
        profileItems(first: 1) {
          nodes {
            variants(first: 1) {
              nodes {
                id
              }
            }
          }
        }
        profileLocationGroups {
          locationGroup {
            id
            locations(first: 10) {
              nodes {
                id
                name
              }
            }
          }
          locationGroupZones(first: 10) {
            nodes {
              zone {
                id
                name
                countries {
                  code {
                    countryCode
                    restOfWorld
                  }
                }
              }
              methodDefinitions(first: 10) {
                nodes {
                  id
                  name
                  active
                  description
                  rateProvider {
                    ... on DeliveryRateDefinition {
                      id
                      price {
                        amount
                        currencyCode
                      }
                    }
                  }
                }
              }
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

const pickupEnableMutation = `#graphql
  mutation DraftOrderDeliveryOptionPickupEnable($localPickupSettings: DeliveryLocationLocalPickupEnableInput!) {
    locationLocalPickupEnable(localPickupSettings: $localPickupSettings) {
      localPickupSettings {
        pickupTime
        instructions
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const pickupDisableMutation = `#graphql
  mutation DraftOrderDeliveryOptionPickupDisable($locationId: ID!) {
    locationLocalPickupDisable(locationId: $locationId) {
      locationId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const setupReadQuery = `#graphql
  query DraftOrderDeliveryOptionSetupRead($profileId: ID!, $variantId: ID!, $customerId: ID!) {
    deliveryProfile(id: $profileId) {
      id
      name
      default
      profileLocationGroups {
        locationGroup {
          id
          locations(first: 10) {
            nodes {
              id
              name
              isActive
              fulfillsOnlineOrders
              shipsInventory
              localPickupSettingsV2 {
                pickupTime
                instructions
              }
            }
          }
        }
        locationGroupZones(first: 10) {
          nodes {
            zone {
              id
              name
              countries {
                code {
                  countryCode
                  restOfWorld
                }
              }
            }
            methodDefinitions(first: 10) {
              nodes {
                id
                name
                active
                description
                rateProvider {
                  ... on DeliveryRateDefinition {
                    id
                    price {
                      amount
                      currencyCode
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
    productVariant(id: $variantId) {
      id
      price
      inventoryQuantity
      inventoryItem {
        id
        tracked
        requiresShipping
        inventoryLevels(first: 250, includeInactive: true) {
          nodes {
            id
            isActive
            location {
              id
              name
              isActive
              fulfillsOnlineOrders
              shipsInventory
            }
            quantities(names: ["available", "on_hand"]) {
              name
              quantity
            }
          }
        }
      }
    }
    customer(id: $customerId) {
      id
      defaultAddress {
        address1
        city
        provinceCode
        countryCodeV2
        zip
      }
    }
  }
`;

const availableDeliveryOptionsQuery = `#graphql
  query DraftOrderDeliveryOptions(
    $input: DraftOrderAvailableDeliveryOptionsInput!
    $localPickupCount: Int
    $localPickupFrom: Int
    $search: String
    $sessionToken: String
  ) {
    draftOrderAvailableDeliveryOptions(
      input: $input
      localPickupCount: $localPickupCount
      localPickupFrom: $localPickupFrom
      search: $search
      sessionToken: $sessionToken
    ) {
      availableShippingRates {
        handle
        title
        code
        source
        price {
          amount
          currencyCode
        }
      }
      availableLocalDeliveryRates {
        handle
        title
        code
        source
        price {
          amount
          currencyCode
        }
      }
      availableLocalPickupOptions {
        handle
        title
        code
        source
        instructions
        locationId
        distanceFromBuyer {
          value
          unit
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const deliveryProfileCleanupMutation = `#graphql
  mutation DraftOrderDeliveryOptionProfileCleanup($id: ID!, $profile: DeliveryProfileInput!) {
    deliveryProfileUpdate(id: $id, profile: $profile) {
      profile {
        id
        name
        default
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation DraftOrderDeliveryOptionProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const customerDeleteMutation = `#graphql
  mutation DraftOrderDeliveryOptionCustomerDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(operationName: string, query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  return {
    operationName,
    query: trimGraphql(query),
    variables,
    result: await runGraphqlRequest(query, variables),
  };
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    if (Array.isArray(current) && /^\d+$/u.test(part)) {
      current = current[Number(part)];
      continue;
    }
    current = readObject(current)?.[part];
  }
  return current;
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} expected a non-empty string, got ${JSON.stringify(value)}.`);
  }
  return value;
}

function requireArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${label} expected an array, got ${JSON.stringify(value)}.`);
  }
  return value;
}

function assertNoErrors(captureResult: GraphqlCapture, root: string): void {
  const topLevelErrors = readPath(captureResult.result.payload, ['errors']);
  if (Array.isArray(topLevelErrors) && topLevelErrors.length > 0) {
    throw new Error(`${captureResult.operationName} returned top-level errors: ${JSON.stringify(topLevelErrors)}.`);
  }
  const userErrors = readPath(captureResult.result.payload, ['data', root, 'userErrors']);
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`${captureResult.operationName} returned userErrors: ${JSON.stringify(userErrors)}.`);
  }
}

function deliveryOptionList(captureResult: GraphqlCapture, field: string): unknown[] {
  return requireArray(
    readPath(captureResult.result.payload, ['data', 'draftOrderAvailableDeliveryOptions', field]),
    `${captureResult.operationName}.${field}`,
  );
}

function chooseLocations(locations: unknown[]): JsonRecord[] {
  const eligible = locations.flatMap((value): JsonRecord[] => {
    const location = readObject(value);
    const address = readObject(location?.['address']);
    if (
      location?.['isActive'] !== true ||
      location['isFulfillmentService'] === true ||
      location['localPickupSettingsV2'] !== null ||
      typeof address?.['countryCode'] !== 'string' ||
      typeof address['city'] !== 'string' ||
      typeof address['zip'] !== 'string'
    ) {
      return [];
    }
    return [location];
  });
  const canadian = eligible.find((location) => readObject(location['address'])?.['countryCode'] === 'CA');
  const american = eligible.find((location) => readObject(location['address'])?.['countryCode'] === 'US');
  if (!canadian || !american) {
    throw new Error('The disposable store needs one unused active CA location and one unused active US location.');
  }
  return [canadian, american];
}

function mailingAddress(location: JsonRecord): JsonRecord {
  const address = readObject(location['address']);
  if (!address) {
    throw new Error('Selected location has no address.');
  }
  return {
    address1: address['address1'] ?? '1 Commerce Street',
    city: address['city'],
    firstName: 'Delivery',
    lastName: 'Options',
    provinceCode: address['provinceCode'],
    countryCode: address['countryCode'],
    zip: address['zip'],
  };
}

const runStamp = Date.now();
let productId: string | null = null;
let customerId: string | null = null;
let profileId: string | null = null;
let profileLocationGroupId: string | null = null;
const enabledPickupLocationIds: string[] = [];
const cleanup: Record<string, GraphqlCapture | null> = {
  pickupDisableCanadian: null,
  pickupDisableAmerican: null,
  profileCleanup: null,
  productDelete: null,
  customerDelete: null,
};

try {
  const locations = await capture('DraftOrderDeliveryOptionLocations', locationsQuery);
  const locationNodes = requireArray(
    readPath(locations.result.payload, ['data', 'locationsAvailableForDeliveryProfilesConnection', 'nodes']),
    'delivery-profile locations',
  );
  const [canadianLocation, americanLocation] = chooseLocations(locationNodes);
  if (!canadianLocation || !americanLocation) {
    throw new Error('Location selection unexpectedly returned fewer than two locations.');
  }
  const canadianLocationId = requireString(canadianLocation['id'], 'Canadian location id');
  const americanLocationId = requireString(americanLocation['id'], 'US location id');
  const currencyCode = requireString(
    readPath(locations.result.payload, ['data', 'shop', 'currencyCode']),
    'shop currency',
  );
  const deliveryProfiles = requireArray(
    readPath(locations.result.payload, ['data', 'deliveryProfiles', 'nodes']),
    'delivery profiles',
  );
  const defaultProfile = deliveryProfiles
    .map(readObject)
    .find((profile): profile is JsonRecord => profile?.['default'] === true);
  profileId = requireString(defaultProfile?.['id'], 'default delivery profile id');

  const customerCreate = await capture('DraftOrderDeliveryOptionCustomerCreate', customerCreateMutation, {
    input: {
      firstName: 'Delivery',
      lastName: 'Options',
      email: `draft-delivery-options-${runStamp}@example.com`,
      addresses: [mailingAddress(canadianLocation)],
    },
  });
  assertNoErrors(customerCreate, 'customerCreate');
  customerId = requireString(
    readPath(customerCreate.result.payload, ['data', 'customerCreate', 'customer', 'id']),
    'customer id',
  );

  const productCreate = await capture('DraftOrderDeliveryOptionProductCreate', productCreateMutation, {
    product: {
      title: `Draft order delivery options ${runStamp}`,
      status: 'ACTIVE',
    },
  });
  assertNoErrors(productCreate, 'productCreate');
  productId = requireString(
    readPath(productCreate.result.payload, ['data', 'productCreate', 'product', 'id']),
    'product id',
  );
  const variantId = requireString(
    readPath(productCreate.result.payload, ['data', 'productCreate', 'product', 'variants', 'nodes', '0', 'id']),
    'variant id',
  );
  const inventoryItemId = requireString(
    readPath(productCreate.result.payload, [
      'data',
      'productCreate',
      'product',
      'variants',
      'nodes',
      '0',
      'inventoryItem',
      'id',
    ]),
    'inventory item id',
  );

  const variantUpdate = await capture('DraftOrderDeliveryOptionVariantUpdate', variantUpdateMutation, {
    productId,
    variants: [
      {
        id: variantId,
        price: '25.00',
        inventoryPolicy: 'DENY',
        inventoryItem: {
          tracked: true,
          requiresShipping: true,
        },
      },
    ],
  });
  assertNoErrors(variantUpdate, 'productVariantsBulkUpdate');

  const inventorySetBothLocations = await capture(
    'DraftOrderDeliveryOptionInventorySet',
    inventorySetQuantitiesMutation,
    {
      input: {
        name: 'available',
        reason: 'correction',
        referenceDocumentUri: `gid://draft-order-delivery-options/Capture/${runStamp}`,
        quantities: [
          { inventoryItemId, locationId: canadianLocationId, quantity: 20, changeFromQuantity: null },
          { inventoryItemId, locationId: americanLocationId, quantity: 20, changeFromQuantity: null },
        ],
      },
      idempotencyKey: `draft-delivery-set-both-${runStamp}`,
    },
  );
  assertNoErrors(inventorySetBothLocations, 'inventorySetQuantities');

  const profileUpdate = await capture('DraftOrderDeliveryOptionProfileUpdate', deliveryProfileUpdateMutation, {
    id: profileId,
    profile: {
      locationGroupsToCreate: [
        {
          locations: [canadianLocationId, americanLocationId],
          zonesToCreate: [
            {
              name: 'North America',
              countries: [
                { code: 'CA', includeAllProvinces: true },
                { code: 'US', includeAllProvinces: true },
              ],
              methodDefinitionsToCreate: [
                {
                  name: 'Ground delivery',
                  description: 'Fixed merchant rate',
                  active: true,
                  rateDefinition: {
                    price: { amount: '7.25', currencyCode },
                  },
                },
              ],
            },
          ],
        },
      ],
    },
  });
  assertNoErrors(profileUpdate, 'deliveryProfileUpdate');
  profileLocationGroupId = requireString(
    readPath(profileUpdate.result.payload, [
      'data',
      'deliveryProfileUpdate',
      'profile',
      'profileLocationGroups',
      '0',
      'locationGroup',
      'id',
    ]),
    'temporary profile location group id',
  );

  const pickupEnableCanadian = await capture('DraftOrderDeliveryOptionPickupEnable', pickupEnableMutation, {
    localPickupSettings: {
      locationId: canadianLocationId,
      pickupTime: 'TWO_HOURS',
      instructions: 'Bring the order confirmation to the Canadian counter.',
    },
  });
  assertNoErrors(pickupEnableCanadian, 'locationLocalPickupEnable');
  enabledPickupLocationIds.push(canadianLocationId);
  const pickupEnableAmerican = await capture('DraftOrderDeliveryOptionPickupEnable', pickupEnableMutation, {
    localPickupSettings: {
      locationId: americanLocationId,
      pickupTime: 'FOUR_HOURS',
      instructions: 'Bring the order confirmation to the US counter.',
    },
  });
  assertNoErrors(pickupEnableAmerican, 'locationLocalPickupEnable');
  enabledPickupLocationIds.push(americanLocationId);

  const setupRead = await capture('DraftOrderDeliveryOptionSetupRead', setupReadQuery, {
    profileId,
    variantId,
    customerId,
  });
  console.log(JSON.stringify({ effectiveSetup: readPath(setupRead.result.payload, ['data']) }, null, 2));

  const baseInput = {
    lineItems: [{ variantId, quantity: 1 }],
    shippingAddress: mailingAddress(canadianLocation),
    marketRegionCountryCode: 'CA',
    purchasingEntity: { customerId },
  };
  const availableAllVariables = {
    input: baseInput,
    localPickupCount: 250,
    localPickupFrom: 0,
    search: null,
    sessionToken: `draft-delivery-options-${runStamp}-settle-0`,
  };
  let availableAll = await capture('DraftOrderDeliveryOptions', availableDeliveryOptionsQuery, availableAllVariables);
  let shippingRates = deliveryOptionList(availableAll, 'availableShippingRates');
  let localDeliveryRates = deliveryOptionList(availableAll, 'availableLocalDeliveryRates');
  let pickupOptions = deliveryOptionList(availableAll, 'availableLocalPickupOptions');
  for (let attempt = 1; attempt <= 5 && (shippingRates.length === 0 || pickupOptions.length < 2); attempt += 1) {
    console.log(
      JSON.stringify({
        settlingAttempt: attempt,
        shippingRateCount: shippingRates.length,
        localDeliveryRateCount: localDeliveryRates.length,
        pickupOptionCount: pickupOptions.length,
      }),
    );
    await delay(2_000);
    availableAllVariables.sessionToken = `draft-delivery-options-${runStamp}-settle-${attempt}`;
    availableAll = await capture('DraftOrderDeliveryOptions', availableDeliveryOptionsQuery, availableAllVariables);
    shippingRates = deliveryOptionList(availableAll, 'availableShippingRates');
    localDeliveryRates = deliveryOptionList(availableAll, 'availableLocalDeliveryRates');
    pickupOptions = deliveryOptionList(availableAll, 'availableLocalPickupOptions');
  }

  const firstPickupTitle = requireString(readObject(pickupOptions[0])?.['title'], 'first pickup title');
  const pickupSearchTerm = firstPickupTitle.split(/\s+/u)[0] ?? firstPickupTitle;
  const pickupFirstPage = await capture('DraftOrderDeliveryOptions', availableDeliveryOptionsQuery, {
    input: baseInput,
    localPickupCount: 1,
    localPickupFrom: 0,
    search: null,
    sessionToken: `draft-delivery-options-${runStamp}-first-page`,
  });
  const pickupSecondPage = await capture('DraftOrderDeliveryOptions', availableDeliveryOptionsQuery, {
    input: baseInput,
    localPickupCount: 1,
    localPickupFrom: 1,
    search: null,
    sessionToken: `draft-delivery-options-${runStamp}-second-page`,
  });
  const pickupSearch = await capture('DraftOrderDeliveryOptions', availableDeliveryOptionsQuery, {
    input: baseInput,
    localPickupCount: 250,
    localPickupFrom: 0,
    search: pickupSearchTerm,
    sessionToken: `draft-delivery-options-${runStamp}-search`,
  });
  const discounted = await capture('DraftOrderDeliveryOptions', availableDeliveryOptionsQuery, {
    input: {
      ...baseInput,
      appliedDiscount: {
        title: 'Ten percent',
        description: 'Representative order-level discount',
        value: 10,
        valueType: 'PERCENTAGE',
      },
      discountCodes: ['NOT-A-REAL-DISCOUNT'],
      acceptAutomaticDiscounts: true,
    },
    localPickupCount: 250,
    localPickupFrom: 0,
    search: null,
    sessionToken: `draft-delivery-options-${runStamp}-discounted`,
  });
  const changedAddress = await capture('DraftOrderDeliveryOptions', availableDeliveryOptionsQuery, {
    input: {
      ...baseInput,
      shippingAddress: mailingAddress(americanLocation),
      marketRegionCountryCode: 'US',
    },
    localPickupCount: 250,
    localPickupFrom: 0,
    search: null,
    sessionToken: `draft-delivery-options-${runStamp}-address-us`,
  });
  const outsideZone = await capture('DraftOrderDeliveryOptions', availableDeliveryOptionsQuery, {
    input: {
      ...baseInput,
      shippingAddress: {
        address1: '1 King Street',
        city: 'London',
        countryCode: 'GB',
        firstName: 'Delivery',
        lastName: 'Options',
        zip: 'SW1A 1AA',
      },
      marketRegionCountryCode: 'GB',
    },
    localPickupCount: 250,
    localPickupFrom: 0,
    search: null,
    sessionToken: `draft-delivery-options-${runStamp}-outside-zone`,
  });
  const noEligibleOptions = await capture('DraftOrderDeliveryOptions', availableDeliveryOptionsQuery, {
    input: {
      lineItems: [
        {
          title: 'Digital service',
          quantity: 1,
          originalUnitPrice: '3.50',
          requiresShipping: false,
          taxable: false,
        },
      ],
    },
    localPickupCount: 250,
    localPickupFrom: 0,
    search: null,
    sessionToken: `draft-delivery-options-${runStamp}-no-eligible`,
  });

  const pickupDisableAmerican = await capture('DraftOrderDeliveryOptionPickupDisable', pickupDisableMutation, {
    locationId: americanLocationId,
  });
  assertNoErrors(pickupDisableAmerican, 'locationLocalPickupDisable');
  enabledPickupLocationIds.splice(enabledPickupLocationIds.indexOf(americanLocationId), 1);
  const afterPickupDisable = await capture('DraftOrderDeliveryOptions', availableDeliveryOptionsQuery, {
    input: baseInput,
    localPickupCount: 250,
    localPickupFrom: 0,
    search: null,
    sessionToken: `draft-delivery-options-${runStamp}-after-pickup-disable`,
  });

  const inventorySetAmericanZero = await capture(
    'DraftOrderDeliveryOptionInventorySet',
    inventorySetQuantitiesMutation,
    {
      input: {
        name: 'available',
        reason: 'correction',
        referenceDocumentUri: `gid://draft-order-delivery-options/Capture/${runStamp}-us-zero`,
        quantities: [{ inventoryItemId, locationId: americanLocationId, quantity: 0, changeFromQuantity: null }],
      },
      idempotencyKey: `draft-delivery-set-us-zero-${runStamp}`,
    },
  );
  assertNoErrors(inventorySetAmericanZero, 'inventorySetQuantities');
  const afterInventoryDeactivate = await capture('DraftOrderDeliveryOptions', availableDeliveryOptionsQuery, {
    input: baseInput,
    localPickupCount: 250,
    localPickupFrom: 0,
    search: null,
    sessionToken: `draft-delivery-options-${runStamp}-after-inventory-zero`,
  });

  if (shippingRates.length === 0) {
    throw new Error('Shopify did not return a non-empty availableShippingRates list after fixed-rate setup.');
  }
  if (pickupOptions.length < 2) {
    throw new Error(
      `Shopify returned ${pickupOptions.length} pickup options; at least two are required for offset coverage.`,
    );
  }
  if (localDeliveryRates.length === 0) {
    throw new Error(
      'Shopify did not return a non-empty availableLocalDeliveryRates list. The store needs local delivery configured before this evidence can be recorded.',
    );
  }

  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        evidence: {
          productId,
          customerId,
          variantId,
          inventoryItemId,
          profileId,
          profileLocationGroupId,
          locationIds: [canadianLocationId, americanLocationId],
          pickupSearchTerm,
        },
        setup: {
          locations,
          customerCreate,
          productCreate,
          variantUpdate,
          inventorySetBothLocations,
          profileUpdate,
          pickupEnableCanadian,
          pickupEnableAmerican,
          setupRead,
        },
        queries: {
          availableAll,
          pickupFirstPage,
          pickupSecondPage,
          pickupSearch,
          discounted,
          changedAddress,
          outsideZone,
          noEligibleOptions,
          afterPickupDisable,
          afterInventoryDeactivate,
        },
        stagedChanges: {
          pickupDisableAmerican,
          inventorySetAmericanZero,
        },
        upstreamCalls: [
          {
            operationName: locations.operationName,
            query: locations.query,
            variables: locations.variables,
            response: {
              status: locations.result.status,
              body: locations.result.payload,
            },
          },
        ],
        notes: [
          'Captured from real Shopify Admin GraphQL 2026-04 using only public setup and cleanup operations.',
          'The disposable product, delivery profile, inventory levels, and pickup settings are removed or restored after recording.',
        ],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
} finally {
  for (const locationId of [...enabledPickupLocationIds].reverse()) {
    const cleanupKey = locationId === enabledPickupLocationIds[0] ? 'pickupDisableCanadian' : 'pickupDisableAmerican';
    cleanup[cleanupKey] = await capture('DraftOrderDeliveryOptionPickupDisable', pickupDisableMutation, { locationId });
  }
  if (profileId && profileLocationGroupId) {
    cleanup['profileCleanup'] = await capture(
      'DraftOrderDeliveryOptionProfileCleanup',
      deliveryProfileCleanupMutation,
      {
        id: profileId,
        profile: { locationGroupsToDelete: [profileLocationGroupId] },
      },
    );
  }
  if (productId) {
    cleanup['productDelete'] = await capture('DraftOrderDeliveryOptionProductDelete', productDeleteMutation, {
      input: { id: productId },
    });
  }
  if (customerId) {
    cleanup['customerDelete'] = await capture('DraftOrderDeliveryOptionCustomerDelete', customerDeleteMutation, {
      input: { id: customerId },
    });
  }
}

console.log(JSON.stringify({ ok: true, outputPath, cleanup }, null, 2));
