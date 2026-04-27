import type { FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type {
  CartTransformRecord,
  ShopifyFunctionRecord,
  TaxAppConfigurationRecord,
  ValidationRecord,
} from '../state/types.js';
import {
  getFieldResponseKey,
  getSelectedChildFields,
  paginateConnectionItems,
  serializeConnection,
} from './graphql-helpers.js';

export const FUNCTION_QUERY_ROOTS = new Set([
  'validation',
  'validations',
  'cartTransforms',
  'shopifyFunction',
  'shopifyFunctions',
]);

export const FUNCTION_MUTATION_ROOTS = new Set([
  'validationCreate',
  'validationUpdate',
  'validationDelete',
  'cartTransformCreate',
  'cartTransformDelete',
  'taxAppConfigure',
]);

type FunctionUserError = {
  field: string[] | null;
  message: string;
  code: string | null;
};

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function readBoolean(value: unknown): boolean | null {
  return typeof value === 'boolean' ? value : null;
}

function normalizeFunctionHandle(handle: string): string {
  return (
    handle
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9_-]+/gu, '-')
      .replace(/^-|-$/gu, '') || 'local-function'
  );
}

function shopifyFunctionIdFromHandle(handle: string): string {
  return `gid://shopify/ShopifyFunction/${normalizeFunctionHandle(handle)}`;
}

function titleFromHandle(handle: string): string {
  return handle
    .split(/[-_\s]+/u)
    .filter((part) => part.length > 0)
    .map((part) => `${part[0]?.toUpperCase() ?? ''}${part.slice(1)}`)
    .join(' ');
}

function ensureShopifyFunction(input: {
  functionId: string | null;
  functionHandle: string | null;
  apiType: string;
  fallbackTitle: string;
}): ShopifyFunctionRecord {
  const id = input.functionId ?? (input.functionHandle ? shopifyFunctionIdFromHandle(input.functionHandle) : null);
  const title = input.functionHandle ? titleFromHandle(input.functionHandle) : input.fallbackTitle;
  const shopifyFunction: ShopifyFunctionRecord = {
    id: id ?? makeSyntheticGid('ShopifyFunction'),
    title,
    handle: input.functionHandle,
    apiType: input.apiType,
  };
  store.upsertStagedShopifyFunction(shopifyFunction);
  return shopifyFunction;
}

function serializeUserErrors(errors: FunctionUserError[], field: FieldNode): Array<Record<string, unknown>> {
  return errors.map((error) => {
    const result: Record<string, unknown> = {};
    for (const selection of getSelectedChildFields(field)) {
      const key = getFieldResponseKey(selection);
      switch (selection.name.value) {
        case 'field':
          result[key] = error.field;
          break;
        case 'message':
          result[key] = error.message;
          break;
        case 'code':
          result[key] = error.code;
          break;
        default:
          result[key] = null;
          break;
      }
    }
    return result;
  });
}

function serializeShopifyFunction(shopifyFunction: ShopifyFunctionRecord, field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field, { includeInlineFragments: true })) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopifyFunction';
        break;
      case 'id':
        result[key] = shopifyFunction.id;
        break;
      case 'title':
        result[key] = shopifyFunction.title;
        break;
      case 'handle':
        result[key] = shopifyFunction.handle;
        break;
      case 'apiType':
        result[key] = shopifyFunction.apiType;
        break;
      case 'description':
        result[key] = shopifyFunction.description ?? null;
        break;
      case 'appKey':
        result[key] = shopifyFunction.appKey ?? null;
        break;
      case 'app':
        result[key] = null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeValidation(validation: ValidationRecord, field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field, { includeInlineFragments: true })) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Validation';
        break;
      case 'id':
        result[key] = validation.id;
        break;
      case 'title':
        result[key] = validation.title;
        break;
      case 'enable':
      case 'enabled':
        result[key] = validation.enable;
        break;
      case 'blockOnFailure':
        result[key] = validation.blockOnFailure;
        break;
      case 'functionId':
        result[key] = validation.functionId ?? validation.shopifyFunctionId;
        break;
      case 'functionHandle':
        result[key] = validation.functionHandle ?? null;
        break;
      case 'shopifyFunction': {
        const shopifyFunction = validation.shopifyFunctionId
          ? store.getEffectiveShopifyFunctionById(validation.shopifyFunctionId)
          : null;
        result[key] = shopifyFunction ? serializeShopifyFunction(shopifyFunction, selection) : null;
        break;
      }
      case 'createdAt':
        result[key] = validation.createdAt ?? null;
        break;
      case 'updatedAt':
        result[key] = validation.updatedAt ?? null;
        break;
      case 'metafield':
        result[key] = null;
        break;
      case 'metafields':
        result[key] = serializeEmptyConnection(selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCartTransform(cartTransform: CartTransformRecord, field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field, { includeInlineFragments: true })) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'CartTransform';
        break;
      case 'id':
        result[key] = cartTransform.id;
        break;
      case 'title':
        result[key] = cartTransform.title;
        break;
      case 'blockOnFailure':
        result[key] = cartTransform.blockOnFailure;
        break;
      case 'functionId':
        result[key] = cartTransform.functionId ?? cartTransform.shopifyFunctionId;
        break;
      case 'functionHandle':
        result[key] = cartTransform.functionHandle ?? null;
        break;
      case 'createdAt':
        result[key] = cartTransform.createdAt ?? null;
        break;
      case 'updatedAt':
        result[key] = cartTransform.updatedAt ?? null;
        break;
      case 'metafield':
        result[key] = null;
        break;
      case 'metafields':
        result[key] = serializeEmptyConnection(selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeTaxAppConfiguration(
  configuration: TaxAppConfigurationRecord,
  field: FieldNode,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field, { includeInlineFragments: true })) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'TaxAppConfiguration';
        break;
      case 'id':
        result[key] = configuration.id;
        break;
      case 'ready':
        result[key] = configuration.ready;
        break;
      case 'state':
        result[key] = configuration.state;
        break;
      case 'updatedAt':
        result[key] = configuration.updatedAt ?? null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeEmptyConnection(field: FieldNode): Record<string, unknown> {
  return serializeConnection(field, {
    items: [],
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: () => '',
    serializeNode: () => null,
  });
}

function serializeValidationConnection(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const items = store.listEffectiveValidations();
  const window = paginateConnectionItems(items, field, variables, (item) => item.id);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (item) => item.id,
    serializeNode: (item, selection) => serializeValidation(item, selection),
  });
}

function serializeCartTransformConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const items = store.listEffectiveCartTransforms();
  const window = paginateConnectionItems(items, field, variables, (item) => item.id);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (item) => item.id,
    serializeNode: (item, selection) => serializeCartTransform(item, selection),
  });
}

function serializeShopifyFunctionConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const apiType = typeof args['apiType'] === 'string' ? args['apiType'] : null;
  const items = apiType
    ? store.listEffectiveShopifyFunctions().filter((shopifyFunction) => shopifyFunction.apiType === apiType)
    : store.listEffectiveShopifyFunctions();
  const window = paginateConnectionItems(items, field, variables, (item) => item.id);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (item) => item.id,
    serializeNode: (item, selection) => serializeShopifyFunction(item, selection),
  });
}

function missingFunctionError(field: string[]): FunctionUserError {
  return {
    field,
    message: 'Function handle or function ID must be provided',
    code: 'MISSING_FUNCTION',
  };
}

function notFoundError(field: string, id: string): FunctionUserError {
  return {
    field: [field],
    message: `No function-backed resource exists with id ${id}`,
    code: 'NOT_FOUND',
  };
}

function readFunctionReference(input: Record<string, unknown>): {
  functionId: string | null;
  functionHandle: string | null;
} {
  return {
    functionId: readString(input['functionId']),
    functionHandle: readString(input['functionHandle']),
  };
}

function serializeValidationMutationPayload(
  field: FieldNode,
  payload: { validation: ValidationRecord | null; userErrors: FunctionUserError[] },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'validation':
        result[key] = payload.validation ? serializeValidation(payload.validation, selection) : null;
        break;
      case 'userErrors':
        result[key] = serializeUserErrors(payload.userErrors, selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCartTransformMutationPayload(
  field: FieldNode,
  payload: { cartTransform: CartTransformRecord | null; userErrors: FunctionUserError[] },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'cartTransform':
        result[key] = payload.cartTransform ? serializeCartTransform(payload.cartTransform, selection) : null;
        break;
      case 'userErrors':
        result[key] = serializeUserErrors(payload.userErrors, selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDeletePayload(
  field: FieldNode,
  payload: { deletedId: string | null; userErrors: FunctionUserError[] },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'deletedId':
        result[key] = payload.deletedId;
        break;
      case 'userErrors':
        result[key] = serializeUserErrors(payload.userErrors, selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function createValidation(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const input = isPlainObject(args['validation']) ? args['validation'] : {};
  const { functionId, functionHandle } = readFunctionReference(input);
  if (!functionId && !functionHandle) {
    return serializeValidationMutationPayload(field, {
      validation: null,
      userErrors: [missingFunctionError(['validation', 'functionHandle'])],
    });
  }

  const shopifyFunction = ensureShopifyFunction({
    functionId,
    functionHandle,
    apiType: 'VALIDATION',
    fallbackTitle: readString(input['title']) ?? 'Local validation function',
  });
  const timestamp = makeSyntheticTimestamp();
  const validation: ValidationRecord = {
    id: makeSyntheticGid('Validation'),
    title: readString(input['title']),
    enable: readBoolean(input['enable']) ?? readBoolean(input['enabled']) ?? true,
    blockOnFailure: readBoolean(input['blockOnFailure']) ?? false,
    functionId,
    ...(functionHandle ? { functionHandle } : {}),
    shopifyFunctionId: shopifyFunction.id,
    createdAt: timestamp,
    updatedAt: timestamp,
  };
  store.upsertStagedValidation(validation);
  return serializeValidationMutationPayload(field, { validation, userErrors: [] });
}

function updateValidation(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const id = readString(args['id']) ?? '';
  const current = store.getEffectiveValidationById(id);
  if (!current) {
    return serializeValidationMutationPayload(field, {
      validation: null,
      userErrors: [notFoundError('id', id)],
    });
  }

  const input = isPlainObject(args['validation']) ? args['validation'] : {};
  const { functionId, functionHandle } = readFunctionReference(input);
  const shopifyFunction =
    functionId || functionHandle
      ? ensureShopifyFunction({
          functionId,
          functionHandle,
          apiType: 'VALIDATION',
          fallbackTitle: current.title ?? 'Local validation function',
        })
      : store.getEffectiveShopifyFunctionById(current.shopifyFunctionId ?? '');
  const validation: ValidationRecord = {
    ...current,
    title: readString(input['title']) ?? current.title,
    enable: readBoolean(input['enable']) ?? readBoolean(input['enabled']) ?? current.enable,
    blockOnFailure: readBoolean(input['blockOnFailure']) ?? current.blockOnFailure,
    functionId: functionId ?? current.functionId,
    ...(functionHandle !== null ? { functionHandle } : {}),
    shopifyFunctionId: shopifyFunction?.id ?? current.shopifyFunctionId,
    updatedAt: makeSyntheticTimestamp(),
  };
  store.upsertStagedValidation(validation);
  return serializeValidationMutationPayload(field, { validation, userErrors: [] });
}

function deleteValidation(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const id = readString(args['id']) ?? '';
  if (!store.getEffectiveValidationById(id)) {
    return serializeDeletePayload(field, { deletedId: null, userErrors: [notFoundError('id', id)] });
  }

  store.deleteStagedValidation(id);
  return serializeDeletePayload(field, { deletedId: id, userErrors: [] });
}

function createCartTransform(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const input = isPlainObject(args['cartTransform']) ? args['cartTransform'] : args;
  const { functionId, functionHandle } = readFunctionReference(input);
  if (!functionId && !functionHandle) {
    return serializeCartTransformMutationPayload(field, {
      cartTransform: null,
      userErrors: [missingFunctionError(['functionHandle'])],
    });
  }

  const shopifyFunction = ensureShopifyFunction({
    functionId,
    functionHandle,
    apiType: 'CART_TRANSFORM',
    fallbackTitle: readString(input['title']) ?? 'Local cart transform function',
  });
  const timestamp = makeSyntheticTimestamp();
  const cartTransform: CartTransformRecord = {
    id: makeSyntheticGid('CartTransform'),
    title: readString(input['title']) ?? shopifyFunction.title,
    blockOnFailure: readBoolean(input['blockOnFailure']) ?? false,
    functionId,
    ...(functionHandle ? { functionHandle } : {}),
    shopifyFunctionId: shopifyFunction.id,
    createdAt: timestamp,
    updatedAt: timestamp,
  };
  store.upsertStagedCartTransform(cartTransform);
  return serializeCartTransformMutationPayload(field, { cartTransform, userErrors: [] });
}

function deleteCartTransform(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const id = readString(args['id']) ?? '';
  if (!store.getEffectiveCartTransformById(id)) {
    return serializeDeletePayload(field, { deletedId: null, userErrors: [notFoundError('id', id)] });
  }

  store.deleteStagedCartTransform(id);
  return serializeDeletePayload(field, { deletedId: id, userErrors: [] });
}

function configureTaxApp(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const ready = readBoolean(args['ready']);
  const userErrors =
    ready === null ? [{ field: ['ready'], message: 'Ready must be true or false', code: 'INVALID' }] : [];
  let configuration = store.getEffectiveTaxAppConfiguration();

  if (ready !== null) {
    configuration = {
      id: 'gid://shopify/TaxAppConfiguration/local',
      ready,
      state: ready ? 'READY' : 'NOT_READY',
      updatedAt: makeSyntheticTimestamp(),
    };
    store.setStagedTaxAppConfiguration(configuration);
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'taxAppConfiguration':
        result[key] = configuration ? serializeTaxAppConfiguration(configuration, selection) : null;
        break;
      case 'userErrors':
        result[key] = serializeUserErrors(userErrors, selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

export function handleFunctionQuery(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);
    switch (field.name.value) {
      case 'validation': {
        const id = readString(args['id']);
        const validation = id ? store.getEffectiveValidationById(id) : null;
        data[key] = validation ? serializeValidation(validation, field) : null;
        break;
      }
      case 'validations':
        data[key] = serializeValidationConnection(field, variables);
        break;
      case 'cartTransforms':
        data[key] = serializeCartTransformConnection(field, variables);
        break;
      case 'shopifyFunction': {
        const id = readString(args['id']);
        const shopifyFunction = id ? store.getEffectiveShopifyFunctionById(id) : null;
        data[key] = shopifyFunction ? serializeShopifyFunction(shopifyFunction, field) : null;
        break;
      }
      case 'shopifyFunctions':
        data[key] = serializeShopifyFunctionConnection(field, variables);
        break;
      default:
        data[key] = null;
        break;
    }
  }
  return { data };
}

export function handleFunctionMutation(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'validationCreate':
        data[key] = createValidation(field, variables);
        break;
      case 'validationUpdate':
        data[key] = updateValidation(field, variables);
        break;
      case 'validationDelete':
        data[key] = deleteValidation(field, variables);
        break;
      case 'cartTransformCreate':
        data[key] = createCartTransform(field, variables);
        break;
      case 'cartTransformDelete':
        data[key] = deleteCartTransform(field, variables);
        break;
      case 'taxAppConfigure':
        data[key] = configureTaxApp(field, variables);
        break;
      default:
        data[key] = null;
        break;
    }
  }
  return { data };
}
