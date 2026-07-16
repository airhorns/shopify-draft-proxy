/* oxlint-disable no-console -- CLI recorder intentionally writes status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

import { createAdminGraphqlClient, runStorefrontGraphqlRequest } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import {
  buildAdminAuthHeaders,
  buildStorefrontRequestHeaders,
  getStoredStorefrontAccessToken,
  getValidConformanceAccessToken,
} from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlRecord = {
  name: string;
  method: 'POST';
  apiSurface: 'admin' | 'storefront';
  apiVersion: string;
  path: string;
  endpoint: string;
  authMode: 'admin-access-token' | 'storefront-access-token';
  headers?: Record<string, string>;
  operationName: string;
  query: string;
  variables: JsonRecord;
  response: { status: number; body: unknown };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const storefrontAuth = await getStoredStorefrontAccessToken();
if (storefrontAuth.shop && storefrontAuth.shop !== storeDomain) {
  throw new Error(`Stored Storefront credential targets ${storefrontAuth.shop}, not ${storeDomain}.`);
}

const adminClient = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const adminPath = `/admin/api/${apiVersion}/graphql.json`;
const adminEndpoint = `${adminOrigin}${adminPath}`;
const storefrontPath = `/api/${apiVersion}/graphql.json`;
const storefrontEndpoint = `https://${storeDomain}${storefrontPath}`;
const storefrontHeaders = Object.fromEntries(
  Object.keys(buildStorefrontRequestHeaders(storefrontAuth.storefront_access_token)).map((name) => [
    name,
    '<redacted:storefront-access-token>',
  ]),
);

const documentPaths = {
  productCreate: 'config/parity-requests/storefront/storefront-catalog-product-create-admin.graphql',
  variantUpdate: 'config/parity-requests/storefront/storefront-catalog-variant-update-admin.graphql',
  locationAdd: 'config/parity-requests/storefront/storefront-catalog-location-add-admin.graphql',
  stockLocationHydrate: 'config/parity-requests/storefront/storefront-catalog-stock-location-hydrate-admin.graphql',
  inventorySet: 'config/parity-requests/storefront/storefront-catalog-inventory-set-admin.graphql',
  publicationHydrate: 'config/parity-requests/storefront/storefront-catalog-publications-hydrate-admin.graphql',
  publish: 'config/parity-requests/storefront/storefront-catalog-publish-admin.graphql',
  productDelete: 'config/parity-requests/storefront/storefront-catalog-product-delete-admin.graphql',
  catalogRead: 'config/parity-requests/storefront/storefront-catalog-read-after-admin-setup.graphql',
  create: 'config/parity-requests/storefront/storefront-cart-create.graphql',
  read: 'config/parity-requests/storefront/storefront-cart-read.graphql',
  linesAdd: 'config/parity-requests/storefront/storefront-cart-lines-add.graphql',
  linesUpdate: 'config/parity-requests/storefront/storefront-cart-lines-update.graphql',
  linesRemove: 'config/parity-requests/storefront/storefront-cart-lines-remove.graphql',
  attributesUpdate: 'config/parity-requests/storefront/storefront-cart-attributes-update.graphql',
  buyerIdentityUpdate: 'config/parity-requests/storefront/storefront-cart-buyer-identity-update.graphql',
  noteUpdate: 'config/parity-requests/storefront/storefront-cart-note-update.graphql',
  discountCodesUpdate: 'config/parity-requests/storefront/storefront-cart-discount-codes-update.graphql',
  giftCardCodesAdd: 'config/parity-requests/storefront/storefront-cart-gift-card-codes-add.graphql',
  giftCardCodesRemove: 'config/parity-requests/storefront/storefront-cart-gift-card-codes-remove.graphql',
  giftCardCodesUpdate: 'config/parity-requests/storefront/storefront-cart-gift-card-codes-update.graphql',
  metafieldsSet: 'config/parity-requests/storefront/storefront-cart-metafields-set.graphql',
  metafieldDelete: 'config/parity-requests/storefront/storefront-cart-metafield-delete.graphql',
  discountCreate: 'config/parity-requests/storefront/storefront-cart-discount-create-admin.graphql',
  discountDelete: 'config/parity-requests/storefront/storefront-cart-discount-delete-admin.graphql',
  giftCardCreate: 'config/parity-requests/storefront/storefront-cart-gift-card-create-admin.graphql',
  giftCardDeactivate: 'config/parity-requests/storefront/storefront-cart-gift-card-deactivate-admin.graphql',
  customerCreate: 'config/parity-requests/storefront/storefront-customer-auth-create.graphql',
  customerTokenCreate: 'config/parity-requests/storefront/storefront-customer-auth-token-create.graphql',
  customerDelete: 'config/parity-requests/storefront/storefront-customer-auth-admin-delete.graphql',
} as const;
const documents = Object.fromEntries(
  await Promise.all(
    Object.entries(documentPaths).map(async ([key, documentPath]) => [key, await readFile(documentPath, 'utf8')]),
  ),
) as Record<keyof typeof documentPaths, string>;

const cartSecrets = new Set<string>();
const redactedCartSecret = '<redacted:storefront-cart-secret>';
const secretReplacements = new Map<string, string>();
const redactedDiscountCode = '<redacted:storefront-cart-active-discount-code>';
const redactedExpiredDiscountCode = '<redacted:storefront-cart-expired-discount-code>';
const redactedInapplicableDiscountCode = '<redacted:storefront-cart-inapplicable-discount-code>';
const redactedGiftCardCode = '<redacted:storefront-cart-gift-card-code>';
const redactedGiftCardSuffix = '<redacted:storefront-cart-gift-card-suffix>';
const redactedCustomerAuth = '<redacted:storefront-cart-customer-auth>';

function registerSecret(secret: string, marker: string): void {
  if (secret.length === 0) return;
  secretReplacements.set(secret, marker);
  secretReplacements.set(secret.toLocaleLowerCase('en-US'), marker);
  secretReplacements.set(secret.toLocaleUpperCase('en-US'), marker);
}

function registerCartSecrets(value: unknown): void {
  if (typeof value === 'string') {
    for (const pattern of [
      /gid:\/\/shopify\/Cart\/([^?&#/]+)(?:\?key=([^&#]+))?/gu,
      /\/cart\/c\/([^?&#/]+)(?:\?key=([^&#]+))?/gu,
    ]) {
      for (const match of value.matchAll(pattern)) {
        if (match[1]) cartSecrets.add(match[1]);
        if (match[2]) cartSecrets.add(match[2]);
      }
    }
    return;
  }
  if (Array.isArray(value)) {
    for (const entry of value) registerCartSecrets(entry);
    return;
  }
  if (typeof value === 'object' && value !== null) {
    for (const child of Object.values(value)) registerCartSecrets(child);
  }
}

function redactCartSecrets(value: unknown, key?: string): unknown {
  if (
    typeof value === 'string' &&
    key !== undefined &&
    ['accessToken', 'customerAccessToken', 'password'].includes(key)
  ) {
    return redactedCustomerAuth;
  }
  if (typeof value === 'string' && key === 'giftCardCode') return redactedGiftCardCode;
  if (typeof value === 'string' && ['lastCharacters', 'maskedCode'].includes(key ?? '')) {
    return redactedGiftCardSuffix;
  }
  if (typeof value === 'string') {
    let redacted = value;
    for (const secret of cartSecrets) redacted = redacted.replaceAll(secret, redactedCartSecret);
    for (const [secret, marker] of secretReplacements) redacted = redacted.replaceAll(secret, marker);
    return redacted;
  }
  if (Array.isArray(value)) return value.map((entry) => redactCartSecrets(entry, key));
  if (typeof value === 'object' && value !== null) {
    return Object.fromEntries(
      Object.entries(value).map(([childKey, child]) => [childKey, redactCartSecrets(child, childKey)]),
    );
  }
  return value;
}

function pathValue(root: unknown, segments: string[]): unknown {
  return segments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) return undefined;
    if (Array.isArray(current)) return current[Number(segment)];
    return (current as JsonRecord)[segment];
  }, root);
}

function requiredString(root: unknown, segments: string[], label: string): string {
  const value = pathValue(root, segments);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(
      `${label} missing at ${segments.join('.')}: ${JSON.stringify(redactCartSecrets(root)).slice(0, 2000)}`,
    );
  }
  return value;
}

function requiredArray(root: unknown, segments: string[], label: string): unknown[] {
  const value = pathValue(root, segments);
  if (!Array.isArray(value)) {
    throw new Error(
      `${label} missing at ${segments.join('.')}: ${JSON.stringify(redactCartSecrets(root)).slice(0, 2000)}`,
    );
  }
  return value;
}

function assertNoTopLevelErrors(payload: unknown, label: string): void {
  const errors = pathValue(payload, ['errors']);
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(redactCartSecrets(errors), null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, segments: string[], label: string): void {
  const errors = requiredArray(payload, segments, `${label} userErrors`);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(redactCartSecrets(errors), null, 2)}`);
  }
}

function publicationId(payload: unknown): string {
  const nodes = requiredArray(payload, ['data', 'publications', 'nodes'], 'publications');
  const onlineStore = nodes.find(
    (node) => typeof node === 'object' && node !== null && (node as JsonRecord)['name'] === 'Online Store',
  );
  return requiredString(onlineStore, ['id'], 'Online Store publication ID');
}

function stockLocationId(payload: unknown): string {
  const nodes = requiredArray(payload, ['data', 'locations', 'nodes'], 'locations');
  const location = nodes.find((node) => {
    if (typeof node !== 'object' || node === null) return false;
    const record = node as JsonRecord;
    return record['isActive'] === true && record['fulfillsOnlineOrders'] === true && record['shipsInventory'] === true;
  });
  return requiredString(location, ['id'], 'active stock location ID');
}

async function recordAdmin(
  name: string,
  operationName: string,
  query: string,
  variables: JsonRecord,
): Promise<{ record: GraphqlRecord; raw: unknown }> {
  const response = await adminClient.runGraphqlRequest(query, variables);
  return {
    record: {
      name,
      method: 'POST',
      apiSurface: 'admin',
      apiVersion,
      path: adminPath,
      endpoint: adminEndpoint,
      authMode: 'admin-access-token',
      operationName,
      query,
      variables: redactCartSecrets(variables) as JsonRecord,
      response: { status: response.status, body: redactCartSecrets(response.payload) },
    },
    raw: response.payload,
  };
}

async function recordStorefront(
  name: string,
  operationName: string,
  query: string,
  variables: JsonRecord,
): Promise<{ record: GraphqlRecord; raw: unknown }> {
  const response = await runStorefrontGraphqlRequest(
    {
      storeOrigin: `https://${storeDomain}`,
      apiVersion,
      storefrontAccessToken: storefrontAuth.storefront_access_token,
    },
    query,
    variables,
  );
  registerCartSecrets(response.payload);
  return {
    record: {
      name,
      method: 'POST',
      apiSurface: 'storefront',
      apiVersion,
      path: storefrontPath,
      endpoint: storefrontEndpoint,
      authMode: 'storefront-access-token',
      headers: storefrontHeaders,
      operationName,
      query,
      variables: redactCartSecrets(variables) as JsonRecord,
      response: { status: response.status, body: redactCartSecrets(response.payload) },
    },
    raw: response.payload,
  };
}

const suffix = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
const discountCode = `CARTD${suffix}`;
const expiredDiscountCode = `CARTX${suffix}`;
const inapplicableDiscountCode = `CARTI${suffix}`;
const giftCardCode = `CARTG${suffix}`;
const customerEmail = `storefront-cart-${suffix}@example.com`;
const customerPassword = `Cart-${suffix}-Aa1!`;
registerSecret(discountCode, redactedDiscountCode);
registerSecret(expiredDiscountCode, redactedExpiredDiscountCode);
registerSecret(inapplicableDiscountCode, redactedInapplicableDiscountCode);
registerSecret(giftCardCode, redactedGiftCardCode);
const productVariables = {
  product: {
    title: `Storefront Cart Product ${suffix}`,
    handle: `storefront-cart-product-${suffix}`,
    status: 'ACTIVE',
    vendor: 'Hermes',
    productType: 'Cart Fixture',
    tags: ['storefront-cart'],
    descriptionHtml: `<p>Storefront cart fixture ${suffix}</p>`,
    productOptions: [{ name: 'Color', values: [{ name: 'Blue' }] }],
  },
};
const adminRecords: Record<string, GraphqlRecord> = {};
const storefrontRecords: Record<string, GraphqlRecord> = {};
const cleanup: GraphqlRecord[] = [];
let productId: string | undefined;
let disposableLocationId: string | undefined;
const discountIds: string[] = [];
let giftCardId: string | undefined;
let customerId: string | undefined;

const locationDeactivateMutation = `#graphql
  mutation StorefrontCartLocationDeactivateCleanup($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location { id isActive }
      locationDeactivateUserErrors { field message code }
    }
  }
`;
const locationDeleteMutation = `#graphql
  mutation StorefrontCartLocationDeleteCleanup($locationId: ID!) {
    locationDelete(locationId: $locationId) {
      deletedLocationId
      locationDeleteUserErrors { field message code }
    }
  }
`;

try {
  const created = await recordAdmin(
    'adminProductCreate',
    'StorefrontCatalogProductCreate',
    documents.productCreate,
    productVariables,
  );
  adminRecords['adminProductCreate'] = created.record;
  assertNoTopLevelErrors(created.raw, 'productCreate');
  assertNoUserErrors(created.raw, ['data', 'productCreate', 'userErrors'], 'productCreate');
  productId = requiredString(created.raw, ['data', 'productCreate', 'product', 'id'], 'product ID');
  const variantId = requiredString(
    created.raw,
    ['data', 'productCreate', 'product', 'variants', 'nodes', '0', 'id'],
    'variant ID',
  );

  const variantUpdate = await recordAdmin(
    'adminVariantUpdate',
    'StorefrontCatalogVariantUpdate',
    documents.variantUpdate,
    {
      productId,
      variants: [
        {
          id: variantId,
          price: '12.50',
          compareAtPrice: '15.00',
          inventoryItem: { sku: `CART-${suffix}`, tracked: true, requiresShipping: false },
        },
      ],
    },
  );
  adminRecords['adminVariantUpdate'] = variantUpdate.record;
  assertNoTopLevelErrors(variantUpdate.raw, 'variant update');
  assertNoUserErrors(variantUpdate.raw, ['data', 'updateVariant', 'userErrors'], 'variant update');
  const inventoryItemId = requiredString(
    variantUpdate.raw,
    ['data', 'updateVariant', 'productVariants', '0', 'inventoryItem', 'id'],
    'inventory item ID',
  );

  const locationAdd = await recordAdmin('adminLocationAdd', 'StorefrontCatalogLocationAdd', documents.locationAdd, {
    input: { name: `Storefront Cart ${suffix}`, address: { countryCode: 'US' } },
  });
  adminRecords['adminLocationAdd'] = locationAdd.record;
  assertNoTopLevelErrors(locationAdd.raw, 'location add');
  assertNoUserErrors(locationAdd.raw, ['data', 'locationAdd', 'userErrors'], 'location add');
  disposableLocationId = requiredString(locationAdd.raw, ['data', 'locationAdd', 'location', 'id'], 'location ID');

  const stockLocation = await recordAdmin(
    'adminStockLocationHydrate',
    'StorefrontCatalogStockLocationHydrate',
    documents.stockLocationHydrate,
    {},
  );
  adminRecords['adminStockLocationHydrate'] = stockLocation.record;
  assertNoTopLevelErrors(stockLocation.raw, 'stock location hydrate');
  const locationId = stockLocationId(stockLocation.raw);

  const inventorySet = await recordAdmin('adminInventorySet', 'StorefrontCatalogInventorySet', documents.inventorySet, {
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: `logistics://storefront-cart/${suffix}`,
      quantities: [{ inventoryItemId, locationId, quantity: 5, changeFromQuantity: 0 }],
    },
    idempotencyKey: `storefront-cart-inventory-${suffix}`,
  });
  adminRecords['adminInventorySet'] = inventorySet.record;
  assertNoTopLevelErrors(inventorySet.raw, 'inventory set');
  assertNoUserErrors(inventorySet.raw, ['data', 'inventorySetQuantities', 'userErrors'], 'inventory set');

  const publications = await recordAdmin(
    'adminPublicationHydrate',
    'StorePropertiesPublishableInputValidationHydrate',
    documents.publicationHydrate,
    {},
  );
  adminRecords['adminPublicationHydrate'] = publications.record;
  assertNoTopLevelErrors(publications.raw, 'publication hydrate');
  const storefrontPublicationId = publicationId(publications.raw);

  const publish = await recordAdmin('adminPublish', 'StorefrontCatalogPublish', documents.publish, {
    id: productId,
    input: [{ publicationId: storefrontPublicationId }],
    publicationId: storefrontPublicationId,
  });
  adminRecords['adminPublish'] = publish.record;
  assertNoTopLevelErrors(publish.raw, 'publishablePublish');
  assertNoUserErrors(publish.raw, ['data', 'publishablePublish', 'userErrors'], 'publishablePublish');

  const activeStartsAt = new Date(Date.now() - 60_000).toISOString();
  const expiredStartsAt = new Date(Date.now() - 7_200_000).toISOString();
  const expiredEndsAt = new Date(Date.now() - 3_600_000).toISOString();
  const discountInputs = [
    {
      key: 'adminDiscountCreate',
      title: `Storefront cart active ${suffix}`,
      code: discountCode,
      startsAt: activeStartsAt,
      minimumRequirement: { subtotal: { greaterThanOrEqualToSubtotal: '1.00' } },
    },
    {
      key: 'adminExpiredDiscountCreate',
      title: `Storefront cart expired ${suffix}`,
      code: expiredDiscountCode,
      startsAt: expiredStartsAt,
      endsAt: expiredEndsAt,
      minimumRequirement: { subtotal: { greaterThanOrEqualToSubtotal: '1.00' } },
    },
    {
      key: 'adminInapplicableDiscountCreate',
      title: `Storefront cart inapplicable ${suffix}`,
      code: inapplicableDiscountCode,
      startsAt: activeStartsAt,
      minimumRequirement: { subtotal: { greaterThanOrEqualToSubtotal: '1000.00' } },
    },
  ] as const;
  for (const discountInput of discountInputs) {
    const createdDiscount = await recordAdmin(
      discountInput.key,
      'StorefrontCartDiscountCreate',
      documents.discountCreate,
      {
        input: {
          title: discountInput.title,
          code: discountInput.code,
          startsAt: discountInput.startsAt,
          ...('endsAt' in discountInput ? { endsAt: discountInput.endsAt } : {}),
          combinesWith: { productDiscounts: false, orderDiscounts: true, shippingDiscounts: false },
          context: { all: 'ALL' },
          minimumRequirement: discountInput.minimumRequirement,
          customerGets: { value: { percentage: 0.2 }, items: { all: true } },
        },
      },
    );
    adminRecords[discountInput.key] = createdDiscount.record;
    assertNoTopLevelErrors(createdDiscount.raw, discountInput.key);
    assertNoUserErrors(createdDiscount.raw, ['data', 'discountCodeBasicCreate', 'userErrors'], discountInput.key);
    discountIds.push(
      requiredString(
        createdDiscount.raw,
        ['data', 'discountCodeBasicCreate', 'codeDiscountNode', 'id'],
        `${discountInput.key} id`,
      ),
    );
  }

  const createdGiftCard = await recordAdmin(
    'adminGiftCardCreate',
    'StorefrontCartGiftCardCreate',
    documents.giftCardCreate,
    { input: { initialValue: '40.00', code: giftCardCode, note: `Storefront cart ${suffix}` } },
  );
  adminRecords['adminGiftCardCreate'] = createdGiftCard.record;
  assertNoTopLevelErrors(createdGiftCard.raw, 'giftCardCreate');
  assertNoUserErrors(createdGiftCard.raw, ['data', 'giftCardCreate', 'userErrors'], 'giftCardCreate');
  giftCardId = requiredString(createdGiftCard.raw, ['data', 'giftCardCreate', 'giftCard', 'id'], 'gift card id');

  const createdCustomer = await recordStorefront(
    'storefrontCustomerCreate',
    'StorefrontCustomerAuthCreate',
    documents.customerCreate,
    {
      input: {
        email: customerEmail,
        password: customerPassword,
        firstName: 'Cart',
        lastName: 'Buyer',
      },
    },
  );
  storefrontRecords['storefrontCustomerCreate'] = createdCustomer.record;
  assertNoTopLevelErrors(createdCustomer.raw, 'customerCreate');
  assertNoUserErrors(createdCustomer.raw, ['data', 'customerCreate', 'userErrors'], 'customerCreate');
  customerId = requiredString(createdCustomer.raw, ['data', 'customerCreate', 'customer', 'id'], 'customer id');

  const customerToken = await recordStorefront(
    'storefrontCustomerTokenCreate',
    'StorefrontCustomerAuthTokenCreate',
    documents.customerTokenCreate,
    { input: { email: customerEmail, password: customerPassword } },
  );
  storefrontRecords['storefrontCustomerTokenCreate'] = customerToken.record;
  assertNoTopLevelErrors(customerToken.raw, 'customerAccessTokenCreate');
  assertNoUserErrors(
    customerToken.raw,
    ['data', 'customerAccessTokenCreate', 'userErrors'],
    'customerAccessTokenCreate',
  );
  const customerAccessToken = requiredString(
    customerToken.raw,
    ['data', 'customerAccessTokenCreate', 'customerAccessToken', 'accessToken'],
    'customer access token',
  );

  let catalogReady: { record: GraphqlRecord; raw: unknown } | undefined;
  for (let attempt = 1; attempt <= 45; attempt += 1) {
    const candidate = await recordStorefront(
      'storefrontCatalogReady',
      'StorefrontCatalogReadAfterAdminSetup',
      documents.catalogRead,
      {
        id: productId,
        handle: productVariables.product.handle,
        query: 'tag:storefront-cart',
        selectedOptions: [{ name: 'Color', value: 'Blue' }],
      },
    );
    assertNoTopLevelErrors(candidate.raw, `Storefront catalog readiness attempt ${attempt}`);
    if (
      pathValue(candidate.raw, ['data', 'byId', 'variants', 'edges', '0', 'node', 'quantityAvailable']) === 5 &&
      pathValue(candidate.raw, ['data', 'byId', 'variants', 'edges', '0', 'node', 'availableForSale']) === true
    ) {
      catalogReady = candidate;
      break;
    }
    await delay(2000);
  }
  if (!catalogReady) throw new Error('Storefront cart merchandise did not become available before capture.');
  storefrontRecords['storefrontCatalogReady'] = catalogReady.record;

  const create = await recordStorefront('cartCreate', 'StorefrontCartCreate', documents.create, {
    input: {
      attributes: [{ key: 'channel', value: 'conformance' }],
      note: 'Initial cart note',
      buyerIdentity: { countryCode: 'CA' },
      lines: [
        {
          merchandiseId: variantId,
          quantity: 2,
          attributes: [{ key: 'engraving', value: 'A' }],
        },
      ],
    },
  });
  storefrontRecords['cartCreate'] = create.record;
  assertNoTopLevelErrors(create.raw, 'cartCreate');
  assertNoUserErrors(create.raw, ['data', 'cartCreate', 'userErrors'], 'cartCreate');
  const cartId = requiredString(create.raw, ['data', 'cartCreate', 'cart', 'id'], 'cart ID');

  const capture = async (
    key: string,
    operationName: string,
    query: string,
    variables: JsonRecord,
  ): Promise<unknown> => {
    const result = await recordStorefront(key, operationName, query, variables);
    storefrontRecords[key] = result.record;
    return result.raw;
  };

  await capture('cartReadAfterCreate', 'StorefrontCartRead', documents.read, { id: cartId });
  await capture('cartBuyerIdentityUpdate', 'StorefrontCartBuyerIdentityUpdate', documents.buyerIdentityUpdate, {
    cartId,
    buyerIdentity: {
      countryCode: 'CA',
      email: customerEmail,
      phone: '+12025550123',
      customerAccessToken,
    },
  });
  await capture(
    'cartBuyerIdentityUpdateInvalidToken',
    'StorefrontCartBuyerIdentityUpdate',
    documents.buyerIdentityUpdate,
    {
      cartId,
      buyerIdentity: { countryCode: 'CA', customerAccessToken: `invalid-${suffix}` },
    },
  );
  await capture('cartDiscountCodesUpdateValid', 'StorefrontCartDiscountCodesUpdate', documents.discountCodesUpdate, {
    cartId,
    discountCodes: [discountCode.toLocaleLowerCase('en-US')],
  });
  await capture('cartDiscountCodesUpdateMixed', 'StorefrontCartDiscountCodesUpdate', documents.discountCodesUpdate, {
    cartId,
    discountCodes: [
      discountCode.toLocaleUpperCase('en-US'),
      discountCode.toLocaleLowerCase('en-US'),
      expiredDiscountCode,
      inapplicableDiscountCode,
      ' NOT-A-REAL-CODE ',
    ],
  });
  const giftCardAdd = await capture(
    'cartGiftCardCodesAdd',
    'StorefrontCartGiftCardCodesAdd',
    documents.giftCardCodesAdd,
    {
      cartId,
      giftCardCodes: [giftCardCode.toLocaleLowerCase('en-US'), 'NOT-A-REAL-GIFT-CARD'],
    },
  );
  const appliedGiftCardId = requiredString(
    giftCardAdd,
    ['data', 'cartGiftCardCodesAdd', 'cart', 'appliedGiftCards', '0', 'id'],
    'applied gift card id',
  );
  await capture('cartGiftCardCodesRemove', 'StorefrontCartGiftCardCodesRemove', documents.giftCardCodesRemove, {
    cartId,
    appliedGiftCardIds: [appliedGiftCardId],
  });
  await capture('cartGiftCardCodesUpdate', 'StorefrontCartGiftCardCodesUpdate', documents.giftCardCodesUpdate, {
    cartId,
    giftCardCodes: [giftCardCode.toLocaleUpperCase('en-US'), giftCardCode.toLocaleLowerCase('en-US')],
  });
  await capture('cartMetafieldsSet', 'StorefrontCartMetafieldsSet', documents.metafieldsSet, {
    metafields: [
      { ownerId: cartId, key: 'custom.note', type: 'single_line_text_field', value: 'Cart note value' },
      { ownerId: cartId, key: 'custom.count', type: 'number_integer', value: '2' },
    ],
  });
  await capture('cartMetafieldsSetInvalidValue', 'StorefrontCartMetafieldsSet', documents.metafieldsSet, {
    metafields: [{ ownerId: cartId, key: 'custom.count', type: 'number_integer', value: 'not-a-number' }],
  });
  await capture('cartReadAfterAdjustments', 'StorefrontCartRead', documents.read, { id: cartId });
  await capture('cartMetafieldDelete', 'StorefrontCartMetafieldDelete', documents.metafieldDelete, {
    input: { ownerId: cartId, key: 'custom.note' },
  });
  await capture('cartMetafieldDeleteMissing', 'StorefrontCartMetafieldDelete', documents.metafieldDelete, {
    input: { ownerId: cartId, key: 'custom.missing' },
  });
  await capture('cartReadAfterMetafieldDelete', 'StorefrontCartRead', documents.read, { id: cartId });
  await capture('cartLinesAddMerge', 'StorefrontCartLinesAdd', documents.linesAdd, {
    cartId,
    lines: [
      {
        merchandiseId: variantId,
        quantity: 1,
        attributes: [{ key: 'engraving', value: 'A' }],
      },
    ],
  });
  const distinct = await capture('cartLinesAddDistinct', 'StorefrontCartLinesAdd', documents.linesAdd, {
    cartId,
    lines: [
      {
        merchandiseId: variantId,
        quantity: 1,
        attributes: [{ key: 'engraving', value: 'B' }],
      },
    ],
  });
  const distinctLines = requiredArray(
    distinct,
    ['data', 'cartLinesAdd', 'cart', 'lines', 'nodes'],
    'distinct cart lines',
  );
  const firstLineId = requiredString(distinctLines[0], ['id'], 'first line ID');
  const secondLineId = requiredString(distinctLines[1], ['id'], 'second line ID');

  await capture('cartLinesUpdate', 'StorefrontCartLinesUpdate', documents.linesUpdate, {
    cartId,
    lines: [
      {
        id: firstLineId,
        quantity: 4,
        attributes: [{ key: 'engraving', value: 'Updated' }],
      },
    ],
  });
  await capture('cartAttributesUpdate', 'StorefrontCartAttributesUpdate', documents.attributesUpdate, {
    cartId,
    attributes: [
      { key: 'gift', value: 'yes' },
      { key: 'channel', value: 'updated' },
      { key: 'gift', value: 'no' },
    ],
  });
  await capture('cartNoteUpdate', 'StorefrontCartNoteUpdate', documents.noteUpdate, {
    cartId,
    note: 'Updated cart note',
  });
  await capture('cartLinesRemove', 'StorefrontCartLinesRemove', documents.linesRemove, {
    cartId,
    lineIds: [secondLineId],
  });
  await capture('cartLinesUpdateStale', 'StorefrontCartLinesUpdate', documents.linesUpdate, {
    cartId,
    lines: [{ id: secondLineId, quantity: 2 }],
  });
  await capture('cartLinesAddInvalidMerchandise', 'StorefrontCartLinesAdd', documents.linesAdd, {
    cartId,
    lines: [{ merchandiseId: 'gid://shopify/ProductVariant/0', quantity: 1 }],
  });
  await capture('cartLinesAddInvalidSellingPlan', 'StorefrontCartLinesAdd', documents.linesAdd, {
    cartId,
    lines: [{ merchandiseId: variantId, sellingPlanId: 'gid://shopify/SellingPlan/0', quantity: 1 }],
  });
  await capture('cartLinesAddInventoryWarning', 'StorefrontCartLinesAdd', documents.linesAdd, {
    cartId,
    lines: [
      {
        merchandiseId: variantId,
        quantity: 10,
        attributes: [{ key: 'stock-probe', value: 'true' }],
      },
    ],
  });
  await capture('cartLinesAddOutOfStock', 'StorefrontCartLinesAdd', documents.linesAdd, {
    cartId,
    lines: [
      {
        merchandiseId: variantId,
        quantity: 1,
        attributes: [{ key: 'out-of-stock-probe', value: 'true' }],
      },
    ],
  });
  await capture('cartLinesAddZeroQuantity', 'StorefrontCartLinesAdd', documents.linesAdd, {
    cartId,
    lines: [{ merchandiseId: variantId, quantity: 0 }],
  });
  await capture('cartNoteTooLong', 'StorefrontCartNoteUpdate', documents.noteUpdate, {
    cartId,
    note: 'n'.repeat(5001),
  });
  await capture('cartAttributesTooMany', 'StorefrontCartAttributesUpdate', documents.attributesUpdate, {
    cartId,
    attributes: Array.from({ length: 251 }, (_, index) => ({ key: `key-${index}`, value: `value-${index}` })),
  });
  await capture('cartLinesTooMany', 'StorefrontCartLinesAdd', documents.linesAdd, {
    cartId,
    lines: Array.from({ length: 251 }, (_, index) => ({
      merchandiseId: variantId,
      quantity: 1,
      attributes: [{ key: 'line', value: String(index) }],
    })),
  });
  await capture('cartReadMissing', 'StorefrontCartRead', documents.read, {
    id: 'gid://shopify/Cart/missing?key=missing',
  });
  await capture('cartNoteUpdateMissing', 'StorefrontCartNoteUpdate', documents.noteUpdate, {
    cartId: 'gid://shopify/Cart/missing?key=missing',
    note: 'Missing cart',
  });

  const beforeCleanup = await capture('cartReadBeforeCleanup', 'StorefrontCartRead', documents.read, { id: cartId });
  const remainingLines = requiredArray(beforeCleanup, ['data', 'cart', 'lines', 'nodes'], 'remaining cart lines');
  const remainingLineIds = remainingLines.map((line, index) => requiredString(line, ['id'], `remaining line ${index}`));
  if (remainingLineIds.length > 0) {
    await capture('cartLinesRemoveAll', 'StorefrontCartLinesRemove', documents.linesRemove, {
      cartId,
      lineIds: remainingLineIds,
    });
  }
  await capture('cartReadEmpty', 'StorefrontCartRead', documents.read, { id: cartId });
} finally {
  for (const id of discountIds) {
    const deleted = await recordAdmin(
      'adminDiscountDeleteCleanup',
      'StorefrontCartDiscountDelete',
      documents.discountDelete,
      { id },
    );
    cleanup.push(deleted.record);
  }
  if (giftCardId) {
    const deactivated = await recordAdmin(
      'adminGiftCardDeactivateCleanup',
      'StorefrontCartGiftCardDeactivate',
      documents.giftCardDeactivate,
      { id: giftCardId },
    );
    cleanup.push(deactivated.record);
  }
  if (customerId) {
    const deleted = await recordAdmin(
      'adminCustomerDeleteCleanup',
      'StorefrontCustomerAuthAdminDelete',
      documents.customerDelete,
      { input: { id: customerId } },
    );
    cleanup.push(deleted.record);
  }
  if (productId) {
    const deleted = await recordAdmin(
      'adminProductDeleteCleanup',
      'StorefrontCatalogProductDelete',
      documents.productDelete,
      { input: { id: productId } },
    );
    cleanup.push(deleted.record);
  }
  if (disposableLocationId) {
    const deactivated = await recordAdmin(
      'adminLocationDeactivateCleanup',
      'StorefrontCartLocationDeactivateCleanup',
      locationDeactivateMutation,
      {
        locationId: disposableLocationId,
        idempotencyKey: `storefront-cart-location-deactivate-${suffix}`,
      },
    );
    cleanup.push(deactivated.record);
    const deleted = await recordAdmin(
      'adminLocationDeleteCleanup',
      'StorefrontCartLocationDeleteCleanup',
      locationDeleteMutation,
      { locationId: disposableLocationId },
    );
    cleanup.push(deleted.record);
  }
}

const fixture = {
  scenarioId: 'storefront-cart-lifecycle',
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  apiSurface: 'storefront',
  endpoint: storefrontEndpoint,
  authMode: 'storefront-access-token',
  storefrontToken: {
    id: storefrontAuth.storefront_token_id || '<unknown>',
    title: storefrontAuth.storefront_token_title || '<unknown>',
    accessScopes: storefrontAuth.storefront_access_scopes,
    obtainedAt: storefrontAuth.obtained_at || '<unknown>',
  },
  redactions: {
    marker: redactedCartSecret,
    fields: [
      'Cart.id token',
      'Cart.id key',
      'Cart.checkoutUrl token',
      'Cart.checkoutUrl key',
      'discount codes',
      'gift card codes and suffixes',
      'customer passwords and access tokens',
    ],
    markers: {
      cart: redactedCartSecret,
      discountCode: redactedDiscountCode,
      expiredDiscountCode: redactedExpiredDiscountCode,
      inapplicableDiscountCode: redactedInapplicableDiscountCode,
      giftCardCode: redactedGiftCardCode,
      giftCardSuffix: redactedGiftCardSuffix,
      customerAuth: redactedCustomerAuth,
    },
  },
  ...adminRecords,
  ...storefrontRecords,
  cleanup,
  upstreamCalls: [adminRecords['adminStockLocationHydrate'], adminRecords['adminPublicationHydrate']],
  notes: [
    'All Storefront documents and variables were sent exactly as recorded; live cart tokens and keys are replaced consistently in the checked-in artifact.',
    'Discount codes, gift card codes and suffixes, customer passwords, and customer access tokens are redacted before the artifact is written.',
    'The recorder creates disposable products, discounts, gift cards, customers, and carts. Admin resources are deleted or deactivated during cleanup; Storefront exposes no cart deletion mutation, so cart secrets are discarded.',
  ],
};
const fixtureText = `${JSON.stringify(fixture, null, 2)}\n`;
for (const secret of cartSecrets) {
  if (fixtureText.includes(secret)) throw new Error('A live cart secret survived fixture redaction.');
}
for (const secret of secretReplacements.keys()) {
  if (fixtureText.includes(secret)) throw new Error('A live adjustment secret survived fixture redaction.');
}
if (cartSecrets.size === 0) throw new Error('The capture returned no cart secrets to redact.');

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'storefront');
await mkdir(outputDir, { recursive: true });
const outputPath = path.join(outputDir, 'storefront-cart-lifecycle.json');
await writeFile(outputPath, fixtureText, 'utf8');
console.log(`Wrote ${outputPath}`);
console.log(`Captured ${Object.keys(storefrontRecords).length} secret-redacted Storefront cart interactions.`);
