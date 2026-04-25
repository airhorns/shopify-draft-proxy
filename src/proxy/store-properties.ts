import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { logger } from '../logger.js';
import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { store } from '../state/store.js';
import type {
  BusinessEntityAddressRecord,
  BusinessEntityRecord,
  ShopifyPaymentsAccountRecord,
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

const storePropertiesLogger = logger.child({ component: 'proxy.store-properties' });

const SAFE_SHOPIFY_PAYMENTS_ACCOUNT_FIELDS = new Set(['id', 'activated', 'country', 'defaultCurrency', 'onboardable']);

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
