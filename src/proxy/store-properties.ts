import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { logger } from '../logger.js';
import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type {
  BusinessEntityAddressRecord,
  BusinessEntityRecord,
  PaymentSettingsRecord,
  ShopifyPaymentsAccountRecord,
  ShopAddressRecord,
  ShopBundlesFeatureRecord,
  ShopCartTransformEligibleOperationsRecord,
  ShopCartTransformFeatureRecord,
  ShopDomainRecord,
  ShopFeaturesRecord,
  ShopPlanRecord,
  ShopPolicyRecord,
  ShopRecord,
  ShopResourceLimitsRecord,
} from '../state/types.js';

interface GraphQLResponseError {
  message: string;
  path: string[];
  extensions: {
    code: 'UNSUPPORTED_FIELD';
    reason: string;
  };
}

interface SerializationContext {
  errors: GraphQLResponseError[];
}

interface ShopPolicyUserErrorRecord {
  field: string[] | null;
  message: string;
  code: 'TOO_BIG' | null;
}

const storePropertiesLogger = logger.child({ component: 'proxy.store-properties' });

const SAFE_SHOPIFY_PAYMENTS_ACCOUNT_FIELDS = new Set(['id', 'activated', 'country', 'defaultCurrency', 'onboardable']);

const SHOP_POLICY_BODY_LIMIT_BYTES = 512 * 1024;

const SHOP_POLICY_TYPE_ORDER = [
  'CONTACT_INFORMATION',
  'LEGAL_NOTICE',
  'PRIVACY_POLICY',
  'REFUND_POLICY',
  'SHIPPING_POLICY',
  'SUBSCRIPTION_POLICY',
  'TERMS_OF_SALE',
  'TERMS_OF_SERVICE',
] as const;

const SHOP_POLICY_TYPES = new Set<string>(SHOP_POLICY_TYPE_ORDER);

const SHOP_POLICY_TITLES_BY_TYPE: Record<string, string> = {
  CONTACT_INFORMATION: 'Contact',
  LEGAL_NOTICE: 'Legal notice',
  PRIVACY_POLICY: 'Privacy policy',
  REFUND_POLICY: 'Refund policy',
  SHIPPING_POLICY: 'Shipping policy',
  SUBSCRIPTION_POLICY: 'Cancellation policy',
  TERMS_OF_SALE: 'Terms of sale',
  TERMS_OF_SERVICE: 'Terms of service',
};

function responseKey(selection: FieldNode): string {
  return selection.alias?.value ?? selection.name.value;
}

function serializeAddress(
  address: BusinessEntityAddressRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'BusinessEntityAddress') {
        continue;
      }
      Object.assign(result, serializeAddress(address, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'BusinessEntityAddress';
        break;
      case 'address1':
        result[key] = address.address1;
        break;
      case 'address2':
        result[key] = address.address2;
        break;
      case 'city':
        result[key] = address.city;
        break;
      case 'countryCode':
        result[key] = address.countryCode;
        break;
      case 'province':
        result[key] = address.province;
        break;
      case 'zip':
        result[key] = address.zip;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopDomain(domain: ShopDomainRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'Domain') {
        continue;
      }
      Object.assign(result, serializeShopDomain(domain, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Domain';
        break;
      case 'id':
        result[key] = domain.id;
        break;
      case 'host':
        result[key] = domain.host;
        break;
      case 'url':
        result[key] = domain.url;
        break;
      case 'sslEnabled':
        result[key] = domain.sslEnabled;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopAddress(
  address: ShopAddressRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopAddress') {
        continue;
      }
      Object.assign(result, serializeShopAddress(address, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopAddress';
        break;
      case 'id':
        result[key] = address.id;
        break;
      case 'address1':
        result[key] = address.address1;
        break;
      case 'address2':
        result[key] = address.address2;
        break;
      case 'city':
        result[key] = address.city;
        break;
      case 'company':
        result[key] = address.company;
        break;
      case 'coordinatesValidated':
        result[key] = address.coordinatesValidated;
        break;
      case 'country':
        result[key] = address.country;
        break;
      case 'countryCodeV2':
        result[key] = address.countryCodeV2;
        break;
      case 'formatted':
        result[key] = structuredClone(address.formatted);
        break;
      case 'formattedArea':
        result[key] = address.formattedArea;
        break;
      case 'latitude':
        result[key] = address.latitude;
        break;
      case 'longitude':
        result[key] = address.longitude;
        break;
      case 'phone':
        result[key] = address.phone;
        break;
      case 'province':
        result[key] = address.province;
        break;
      case 'provinceCode':
        result[key] = address.provinceCode;
        break;
      case 'zip':
        result[key] = address.zip;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopPlan(plan: ShopPlanRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopPlan') {
        continue;
      }
      Object.assign(result, serializeShopPlan(plan, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopPlan';
        break;
      case 'partnerDevelopment':
        result[key] = plan.partnerDevelopment;
        break;
      case 'publicDisplayName':
        result[key] = plan.publicDisplayName;
        break;
      case 'shopifyPlus':
        result[key] = plan.shopifyPlus;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopResourceLimits(
  resourceLimits: ShopResourceLimitsRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopResourceLimits') {
        continue;
      }
      Object.assign(result, serializeShopResourceLimits(resourceLimits, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopResourceLimits';
        break;
      case 'locationLimit':
        result[key] = resourceLimits.locationLimit;
        break;
      case 'maxProductOptions':
        result[key] = resourceLimits.maxProductOptions;
        break;
      case 'maxProductVariants':
        result[key] = resourceLimits.maxProductVariants;
        break;
      case 'redirectLimitReached':
        result[key] = resourceLimits.redirectLimitReached;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopBundlesFeature(
  bundles: ShopBundlesFeatureRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'BundlesFeature') {
        continue;
      }
      Object.assign(result, serializeShopBundlesFeature(bundles, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'BundlesFeature';
        break;
      case 'eligibleForBundles':
        result[key] = bundles.eligibleForBundles;
        break;
      case 'ineligibilityReason':
        result[key] = bundles.ineligibilityReason;
        break;
      case 'sellsBundles':
        result[key] = bundles.sellsBundles;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopCartTransformEligibleOperations(
  operations: ShopCartTransformEligibleOperationsRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (
        selection.typeCondition?.name.value &&
        selection.typeCondition.name.value !== 'CartTransformEligibleOperations'
      ) {
        continue;
      }
      Object.assign(
        result,
        serializeShopCartTransformEligibleOperations(operations, selection.selectionSet.selections),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'CartTransformEligibleOperations';
        break;
      case 'expandOperation':
        result[key] = operations.expandOperation;
        break;
      case 'mergeOperation':
        result[key] = operations.mergeOperation;
        break;
      case 'updateOperation':
        result[key] = operations.updateOperation;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopCartTransformFeature(
  cartTransform: ShopCartTransformFeatureRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'CartTransformFeature') {
        continue;
      }
      Object.assign(result, serializeShopCartTransformFeature(cartTransform, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'CartTransformFeature';
        break;
      case 'eligibleOperations':
        result[key] = serializeShopCartTransformEligibleOperations(
          cartTransform.eligibleOperations,
          selection.selectionSet?.selections ?? [],
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopFeatures(
  features: ShopFeaturesRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopFeatures') {
        continue;
      }
      Object.assign(result, serializeShopFeatures(features, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopFeatures';
        break;
      case 'avalaraAvatax':
        result[key] = features.avalaraAvatax;
        break;
      case 'branding':
        result[key] = features.branding;
        break;
      case 'bundles':
        result[key] = serializeShopBundlesFeature(features.bundles, selection.selectionSet?.selections ?? []);
        break;
      case 'captcha':
        result[key] = features.captcha;
        break;
      case 'cartTransform':
        result[key] = serializeShopCartTransformFeature(
          features.cartTransform,
          selection.selectionSet?.selections ?? [],
        );
        break;
      case 'dynamicRemarketing':
        result[key] = features.dynamicRemarketing;
        break;
      case 'eligibleForSubscriptionMigration':
        result[key] = features.eligibleForSubscriptionMigration;
        break;
      case 'eligibleForSubscriptions':
        result[key] = features.eligibleForSubscriptions;
        break;
      case 'giftCards':
        result[key] = features.giftCards;
        break;
      case 'harmonizedSystemCode':
        result[key] = features.harmonizedSystemCode;
        break;
      case 'legacySubscriptionGatewayEnabled':
        result[key] = features.legacySubscriptionGatewayEnabled;
        break;
      case 'liveView':
        result[key] = features.liveView;
        break;
      case 'paypalExpressSubscriptionGatewayStatus':
        result[key] = features.paypalExpressSubscriptionGatewayStatus;
        break;
      case 'reports':
        result[key] = features.reports;
        break;
      case 'sellsSubscriptions':
        result[key] = features.sellsSubscriptions;
        break;
      case 'showMetrics':
        result[key] = features.showMetrics;
        break;
      case 'storefront':
        result[key] = features.storefront;
        break;
      case 'unifiedMarkets':
        result[key] = features.unifiedMarkets;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializePaymentSettings(
  paymentSettings: PaymentSettingsRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'PaymentSettings') {
        continue;
      }
      Object.assign(result, serializePaymentSettings(paymentSettings, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'PaymentSettings';
        break;
      case 'supportedDigitalWallets':
        result[key] = structuredClone(paymentSettings.supportedDigitalWallets);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopPolicy(policy: ShopPolicyRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopPolicy') {
        continue;
      }
      Object.assign(result, serializeShopPolicy(policy, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopPolicy';
        break;
      case 'id':
        result[key] = policy.id;
        break;
      case 'title':
        result[key] = policy.title;
        break;
      case 'body':
        result[key] = policy.body;
        break;
      case 'type':
        result[key] = policy.type;
        break;
      case 'url':
        result[key] = policy.url;
        break;
      case 'createdAt':
        result[key] = policy.createdAt;
        break;
      case 'updatedAt':
        result[key] = policy.updatedAt;
        break;
      case 'translations':
        result[key] = [];
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopPolicyUserError(
  userError: ShopPolicyUserErrorRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopPolicyUserError') {
        continue;
      }
      Object.assign(result, serializeShopPolicyUserError(userError, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopPolicyUserError';
        break;
      case 'field':
        result[key] = userError.field ? structuredClone(userError.field) : null;
        break;
      case 'message':
        result[key] = userError.message;
        break;
      case 'code':
        result[key] = userError.code;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShop(shop: ShopRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'Shop') {
        continue;
      }
      Object.assign(result, serializeShop(shop, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Shop';
        break;
      case 'id':
        result[key] = shop.id;
        break;
      case 'name':
        result[key] = shop.name;
        break;
      case 'myshopifyDomain':
        result[key] = shop.myshopifyDomain;
        break;
      case 'url':
        result[key] = shop.url;
        break;
      case 'primaryDomain':
        result[key] = serializeShopDomain(shop.primaryDomain, selection.selectionSet?.selections ?? []);
        break;
      case 'contactEmail':
        result[key] = shop.contactEmail;
        break;
      case 'email':
        result[key] = shop.email;
        break;
      case 'currencyCode':
        result[key] = shop.currencyCode;
        break;
      case 'enabledPresentmentCurrencies':
        result[key] = structuredClone(shop.enabledPresentmentCurrencies);
        break;
      case 'ianaTimezone':
        result[key] = shop.ianaTimezone;
        break;
      case 'timezoneAbbreviation':
        result[key] = shop.timezoneAbbreviation;
        break;
      case 'timezoneOffset':
        result[key] = shop.timezoneOffset;
        break;
      case 'timezoneOffsetMinutes':
        result[key] = shop.timezoneOffsetMinutes;
        break;
      case 'taxesIncluded':
        result[key] = shop.taxesIncluded;
        break;
      case 'taxShipping':
        result[key] = shop.taxShipping;
        break;
      case 'unitSystem':
        result[key] = shop.unitSystem;
        break;
      case 'weightUnit':
        result[key] = shop.weightUnit;
        break;
      case 'shopAddress':
        result[key] = serializeShopAddress(shop.shopAddress, selection.selectionSet?.selections ?? []);
        break;
      case 'plan':
        result[key] = serializeShopPlan(shop.plan, selection.selectionSet?.selections ?? []);
        break;
      case 'resourceLimits':
        result[key] = serializeShopResourceLimits(shop.resourceLimits, selection.selectionSet?.selections ?? []);
        break;
      case 'features':
        result[key] = serializeShopFeatures(shop.features, selection.selectionSet?.selections ?? []);
        break;
      case 'paymentSettings':
        result[key] = serializePaymentSettings(shop.paymentSettings, selection.selectionSet?.selections ?? []);
        break;
      case 'shopPolicies':
        result[key] = shop.shopPolicies.map((policy) =>
          serializeShopPolicy(policy, selection.selectionSet?.selections ?? []),
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function unsupportedShopifyPaymentsFieldError(
  businessEntity: BusinessEntityRecord,
  fieldName: string,
): GraphQLResponseError {
  return {
    message: `Field ShopifyPaymentsAccount.${fieldName} is not exposed by the local snapshot because it can contain account-specific payment data. Capture and model it explicitly before relying on it.`,
    path: ['businessEntity', 'shopifyPaymentsAccount', fieldName],
    extensions: {
      code: 'UNSUPPORTED_FIELD',
      reason: 'shopify-payments-account-sensitive-field',
    },
  };
}

function serializeShopifyPaymentsAccount(
  businessEntity: BusinessEntityRecord,
  account: ShopifyPaymentsAccountRecord,
  selections: readonly SelectionNode[],
  context: SerializationContext,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopifyPaymentsAccount') {
        continue;
      }
      Object.assign(
        result,
        serializeShopifyPaymentsAccount(businessEntity, account, selection.selectionSet.selections, context),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopifyPaymentsAccount';
        break;
      case 'id':
        result[key] = account.id;
        break;
      case 'activated':
        result[key] = account.activated;
        break;
      case 'country':
        result[key] = account.country;
        break;
      case 'defaultCurrency':
        result[key] = account.defaultCurrency;
        break;
      case 'onboardable':
        result[key] = account.onboardable;
        break;
      default: {
        if (!SAFE_SHOPIFY_PAYMENTS_ACCOUNT_FIELDS.has(selection.name.value)) {
          storePropertiesLogger.warn(
            {
              businessEntityId: businessEntity.id,
              fieldName: selection.name.value,
            },
            'unsupported Shopify Payments account field requested from snapshot business entity',
          );
          context.errors.push(unsupportedShopifyPaymentsFieldError(businessEntity, selection.name.value));
        }
        result[key] = null;
      }
    }
  }

  return result;
}

function serializeBusinessEntity(
  businessEntity: BusinessEntityRecord,
  selections: readonly SelectionNode[],
  context: SerializationContext,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'BusinessEntity') {
        continue;
      }
      Object.assign(result, serializeBusinessEntity(businessEntity, selection.selectionSet.selections, context));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'BusinessEntity';
        break;
      case 'id':
        result[key] = businessEntity.id;
        break;
      case 'displayName':
        result[key] = businessEntity.displayName;
        break;
      case 'companyName':
        result[key] = businessEntity.companyName;
        break;
      case 'primary':
        result[key] = businessEntity.primary;
        break;
      case 'archived':
        result[key] = businessEntity.archived;
        break;
      case 'address':
        result[key] = serializeAddress(businessEntity.address, selection.selectionSet?.selections ?? []);
        break;
      case 'shopifyPaymentsAccount':
        result[key] = businessEntity.shopifyPaymentsAccount
          ? serializeShopifyPaymentsAccount(
              businessEntity,
              businessEntity.shopifyPaymentsAccount,
              selection.selectionSet?.selections ?? [],
              context,
            )
          : null;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function readShopPolicyInput(args: Record<string, unknown>): Record<string, unknown> {
  const shopPolicy = args['shopPolicy'];
  if (shopPolicy && typeof shopPolicy === 'object' && !Array.isArray(shopPolicy)) {
    return shopPolicy as Record<string, unknown>;
  }

  const input = args['input'];
  if (input && typeof input === 'object' && !Array.isArray(input)) {
    return input as Record<string, unknown>;
  }

  return {};
}

function readNumericGidTail(id: string): string | null {
  const tail = id.split('/').at(-1)?.split('?')[0] ?? '';
  return /^\d+$/.test(tail) ? tail : null;
}

function buildShopPolicyUrl(shop: ShopRecord, policyId: string, type: string): string {
  const shopTail = readNumericGidTail(shop.id);
  const policyTail = readNumericGidTail(policyId);
  if (shopTail && policyTail) {
    return `https://checkout.shopify.com/${shopTail}/policies/${policyTail}.html?locale=en`;
  }

  return `${shop.url.replace(/\/$/, '')}/policies/${type.toLowerCase().replaceAll('_', '-')}`;
}

function compareShopPoliciesByType(left: ShopPolicyRecord, right: ShopPolicyRecord): number {
  const leftIndex = SHOP_POLICY_TYPE_ORDER.indexOf(left.type as (typeof SHOP_POLICY_TYPE_ORDER)[number]);
  const rightIndex = SHOP_POLICY_TYPE_ORDER.indexOf(right.type as (typeof SHOP_POLICY_TYPE_ORDER)[number]);
  const normalizedLeftIndex = leftIndex === -1 ? SHOP_POLICY_TYPE_ORDER.length : leftIndex;
  const normalizedRightIndex = rightIndex === -1 ? SHOP_POLICY_TYPE_ORDER.length : rightIndex;
  return normalizedLeftIndex - normalizedRightIndex || left.type.localeCompare(right.type);
}

function validateShopPolicyInput(input: Record<string, unknown>): {
  body: string | null;
  type: string | null;
  userErrors: ShopPolicyUserErrorRecord[];
} {
  const rawType = input['type'];
  const rawBody = input['body'];
  const type = typeof rawType === 'string' ? rawType : null;
  const body = typeof rawBody === 'string' ? rawBody : null;
  const userErrors: ShopPolicyUserErrorRecord[] = [];

  if (!type || !SHOP_POLICY_TYPES.has(type)) {
    userErrors.push({
      field: ['shopPolicy', 'type'],
      message: 'Type is invalid',
      code: null,
    });
  }

  if (body === null) {
    userErrors.push({
      field: ['shopPolicy', 'body'],
      message: 'Body is required',
      code: null,
    });
  } else if (Buffer.byteLength(body, 'utf8') > SHOP_POLICY_BODY_LIMIT_BYTES) {
    userErrors.push({
      field: ['shopPolicy', 'body'],
      message: 'Body is too big (maximum is 512 KB)',
      code: 'TOO_BIG',
    });
  }

  return {
    body,
    type,
    userErrors,
  };
}

function stageShopPolicyUpdate(input: Record<string, unknown>): {
  shopPolicy: ShopPolicyRecord | null;
  userErrors: ShopPolicyUserErrorRecord[];
} {
  const validation = validateShopPolicyInput(input);
  if (validation.userErrors.length > 0 || !validation.type || validation.body === null) {
    return {
      shopPolicy: null,
      userErrors: validation.userErrors,
    };
  }

  const shop = store.getEffectiveShop();
  if (!shop) {
    return {
      shopPolicy: null,
      userErrors: [
        {
          field: ['shopPolicy'],
          message: 'Shop baseline is required to stage a shop policy update',
          code: null,
        },
      ],
    };
  }

  const existingPolicy = shop.shopPolicies.find((policy) => policy.type === validation.type) ?? null;
  const now = makeSyntheticTimestamp();
  const id = existingPolicy?.id ?? makeSyntheticGid('ShopPolicy');
  const policy: ShopPolicyRecord = {
    id,
    title: existingPolicy?.title ?? SHOP_POLICY_TITLES_BY_TYPE[validation.type] ?? validation.type,
    body: validation.body,
    type: validation.type,
    url: existingPolicy?.url ?? buildShopPolicyUrl(shop, id, validation.type),
    createdAt: existingPolicy?.createdAt ?? now,
    updatedAt: now,
  };
  const otherPolicies = shop.shopPolicies.filter((candidate) => candidate.type !== policy.type);
  const updatedShop: ShopRecord = {
    ...shop,
    shopPolicies: [...otherPolicies, policy].sort(compareShopPoliciesByType),
  };

  store.stageShop(updatedShop);

  return {
    shopPolicy: policy,
    userErrors: [],
  };
}

function serializeShopPolicyUpdatePayload(
  payload: { shopPolicy: ShopPolicyRecord | null; userErrors: ShopPolicyUserErrorRecord[] },
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopPolicyUpdatePayload') {
        continue;
      }
      Object.assign(result, serializeShopPolicyUpdatePayload(payload, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopPolicyUpdatePayload';
        break;
      case 'shopPolicy':
        result[key] = payload.shopPolicy
          ? serializeShopPolicy(payload.shopPolicy, selection.selectionSet?.selections ?? [])
          : null;
        break;
      case 'userErrors':
        result[key] = payload.userErrors.map((userError) =>
          serializeShopPolicyUserError(userError, selection.selectionSet?.selections ?? []),
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

export function handleStorePropertiesMutation(
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const fields = getRootFields(document);
  const data: Record<string, unknown> = {};

  for (const field of fields) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'shopPolicyUpdate': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeShopPolicyUpdatePayload(
          stageShopPolicyUpdate(readShopPolicyInput(args)),
          field.selectionSet?.selections ?? [],
        );
        break;
      }
      default:
        data[key] = null;
    }
  }

  return { data };
}

export function handleStorePropertiesQuery(
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const fields = getRootFields(document);
  const context: SerializationContext = { errors: [] };
  const data: Record<string, unknown> = {};

  for (const field of fields) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'shop': {
        const shop = store.getEffectiveShop();
        data[key] = shop ? serializeShop(shop, field.selectionSet?.selections ?? []) : null;
        break;
      }
      case 'businessEntities':
        data[key] = store
          .listEffectiveBusinessEntities()
          .map((businessEntity) =>
            serializeBusinessEntity(businessEntity, field.selectionSet?.selections ?? [], context),
          );
        break;
      case 'businessEntity': {
        const args = getFieldArguments(field, variables);
        const rawId = args['id'];
        const id = typeof rawId === 'string' && rawId.length > 0 ? rawId : null;
        const businessEntity = id ? store.getBusinessEntityById(id) : store.getPrimaryBusinessEntity();
        data[key] = businessEntity
          ? serializeBusinessEntity(businessEntity, field.selectionSet?.selections ?? [], context)
          : null;
        break;
      }
      default:
        data[key] = null;
    }
  }

  return context.errors.length > 0 ? { data, errors: context.errors } : { data };
}
