import {
  stateSnapshotSchema,
  type CustomerRecord,
  type OrderRecord,
  type ProductRecord,
} from '../../src/state/types.js';

export const performanceSmokeCounts = {
  products: 1_500,
  customers: 1_500,
  catalogs: 1_500,
  orders: 500,
  repeats: 3,
} as const;

export const performanceSmokeTargets = {
  productId: productId(1_234),
  customerId: customerId(1_234),
  catalogId: catalogId(1_234),
  orderId: orderId(404),
} as const;

function productId(index: number): string {
  return `gid://shopify/Product/${100_000 + index}`;
}

function variantId(index: number): string {
  return `gid://shopify/ProductVariant/${200_000 + index}`;
}

function inventoryItemId(index: number): string {
  return `gid://shopify/InventoryItem/${300_000 + index}`;
}

function optionId(index: number): string {
  return `gid://shopify/ProductOption/${400_000 + index}`;
}

function optionValueId(index: number): string {
  return `gid://shopify/ProductOptionValue/${500_000 + index}`;
}

function collectionId(index: number): string {
  return `gid://shopify/Collection/${600_000 + index}`;
}

function customerId(index: number): string {
  return `gid://shopify/Customer/${700_000 + index}`;
}

function catalogId(index: number): string {
  return `gid://shopify/MarketCatalog/${800_000 + index}`;
}

function orderId(index: number): string {
  return `gid://shopify/Order/${900_000 + index}`;
}

function isoDate(dayOffset: number): string {
  return new Date(Date.UTC(2026, 0, 1, 0, 0, dayOffset)).toISOString();
}

export function makePerformanceSmokeProduct(index: number): ProductRecord {
  return {
    id: productId(index),
    legacyResourceId: String(100_000 + index),
    title: `Smoke Product ${index}`,
    handle: `smoke-product-${index}`,
    status: index % 11 === 0 ? 'DRAFT' : 'ACTIVE',
    publicationIds: [],
    createdAt: isoDate(index),
    updatedAt: isoDate(index + 1),
    vendor: index % 3 === 0 ? 'Smoke Vendor' : 'Baseline Vendor',
    productType: index % 5 === 0 ? 'Snowboard' : 'Accessory',
    tags: index % 7 === 0 ? ['baseline-smoke', `bucket-${index % 10}`] : [`bucket-${index % 10}`],
    totalInventory: index % 17,
    tracksInventory: true,
    descriptionHtml: null,
    onlineStorePreviewUrl: null,
    templateSuffix: null,
    seo: { title: null, description: null },
    category: null,
  };
}

function makePerformanceSmokeCustomer(index: number): CustomerRecord {
  const email = `customer-${index}@example.com`;
  return {
    id: customerId(index),
    firstName: 'Smoke',
    lastName: `Customer ${index}`,
    displayName: `Smoke Customer ${index}`,
    email,
    legacyResourceId: String(700_000 + index),
    locale: 'en',
    note: null,
    canDelete: true,
    verifiedEmail: true,
    taxExempt: false,
    taxExemptions: [],
    state: index % 13 === 0 ? 'DISABLED' : 'ENABLED',
    tags: index % 9 === 0 ? ['baseline-smoke', `cohort-${index % 8}`] : [`cohort-${index % 8}`],
    numberOfOrders: String(index % 6),
    amountSpent: {
      amount: `${index % 100}.00`,
      currencyCode: 'USD',
    },
    defaultEmailAddress: {
      emailAddress: email,
      marketingState: 'NOT_SUBSCRIBED',
      marketingOptInLevel: 'SINGLE_OPT_IN',
      marketingUpdatedAt: null,
    },
    defaultPhoneNumber: null,
    emailMarketingConsent: {
      marketingState: 'NOT_SUBSCRIBED',
      marketingOptInLevel: 'SINGLE_OPT_IN',
      consentUpdatedAt: null,
    },
    smsMarketingConsent: null,
    defaultAddress: null,
    createdAt: isoDate(index),
    updatedAt: isoDate(index + 2),
  };
}

export function makePerformanceSmokeOrder(index: number, overrides: Partial<OrderRecord> = {}): OrderRecord {
  const createdAt = isoDate(index + 4);
  return {
    id: orderId(index),
    name: `#${900_000 + index}`,
    createdAt,
    updatedAt: createdAt,
    email: `order-${index}@example.com`,
    displayFinancialStatus: index % 2 === 0 ? 'PAID' : 'PENDING',
    displayFulfillmentStatus: index % 3 === 0 ? 'FULFILLED' : 'UNFULFILLED',
    note: null,
    tags: index % 10 === 0 ? ['baseline-smoke'] : [],
    customAttributes: [],
    billingAddress: null,
    shippingAddress: null,
    subtotalPriceSet: {
      shopMoney: { amount: '10.00', currencyCode: 'USD' },
    },
    currentTotalPriceSet: {
      shopMoney: { amount: '10.00', currencyCode: 'USD' },
    },
    totalPriceSet: {
      shopMoney: { amount: '10.00', currencyCode: 'USD' },
    },
    totalRefundedSet: {
      shopMoney: { amount: '0.00', currencyCode: 'USD' },
    },
    customer: null,
    shippingLines: [],
    lineItems: [],
    transactions: [],
    refunds: [],
    returns: [],
    ...overrides,
  };
}

export function buildPerformanceSmokeSnapshot() {
  const rawSnapshot: {
    products: Record<string, unknown>;
    productVariants: Record<string, unknown>;
    productOptions: Record<string, unknown>;
    collections: Record<string, unknown>;
    customers: Record<string, unknown>;
    catalogs: Record<string, unknown>;
    catalogOrder: string[];
    productCollections: Record<string, unknown>;
    productMedia: Record<string, unknown>;
    productMetafields: Record<string, unknown>;
    deletedProductIds: Record<string, true>;
    deletedCollectionIds: Record<string, true>;
    deletedCustomerIds: Record<string, true>;
  } = {
    products: {},
    productVariants: {},
    productOptions: {},
    collections: {},
    customers: {},
    catalogs: {},
    catalogOrder: [],
    productCollections: {},
    productMedia: {},
    productMetafields: {},
    deletedProductIds: {},
    deletedCollectionIds: {},
    deletedCustomerIds: {},
  };

  for (let index = 1; index <= performanceSmokeCounts.products; index += 1) {
    const product = makePerformanceSmokeProduct(index);
    const variant = {
      id: variantId(index),
      productId: product.id,
      title: 'Default Title',
      sku: `SMOKE-${index}`,
      barcode: null,
      price: '10.00',
      compareAtPrice: null,
      taxable: true,
      inventoryPolicy: 'DENY',
      inventoryQuantity: index % 17,
      selectedOptions: [{ name: 'Title', value: 'Default Title' }],
      inventoryItem: {
        id: inventoryItemId(index),
        tracked: true,
        requiresShipping: true,
        measurement: null,
        countryCodeOfOrigin: null,
        provinceCodeOfOrigin: null,
        harmonizedSystemCode: null,
      },
    };
    const option = {
      id: optionId(index),
      productId: product.id,
      name: 'Title',
      position: 1,
      optionValues: [{ id: optionValueId(index), name: 'Default Title', hasVariants: true }],
    };
    const productCollectionId = collectionId((index % 60) + 1);
    const collection = {
      id: productCollectionId,
      title: `Smoke Collection ${(index % 60) + 1}`,
      handle: `smoke-collection-${(index % 60) + 1}`,
      publicationIds: [],
    };
    const catalog = {
      id: catalogId(index),
      cursor: `catalog-cursor-${index}`,
      data: {
        __typename: 'MarketCatalog',
        id: catalogId(index),
        title: `Smoke Catalog ${index}`,
        status: index % 4 === 0 ? 'DRAFT' : 'ACTIVE',
        priceList: null,
        publication: null,
        markets: { edges: [] },
      },
    };

    rawSnapshot.products[product.id] = product;
    rawSnapshot.productVariants[variant.id] = variant;
    rawSnapshot.productOptions[option.id] = option;
    rawSnapshot.collections[collection.id] = collection;
    rawSnapshot.productCollections[`${product.id}::${collection.id}`] = {
      ...collection,
      productId: product.id,
      position: 1,
    };

    if (index <= performanceSmokeCounts.customers) {
      const customer = makePerformanceSmokeCustomer(index);
      rawSnapshot.customers[customer.id] = customer;
    }

    rawSnapshot.catalogs[catalog.id] = catalog;
    rawSnapshot.catalogOrder.push(catalog.id);
  }

  return stateSnapshotSchema.parse(rawSnapshot);
}
