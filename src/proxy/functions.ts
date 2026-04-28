import type { ProxyRuntimeContext } from './runtime-context.js';
import type { FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
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
  projectGraphqlValue,
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

function findExistingShopifyFunction(
  runtime: ProxyRuntimeContext,
  input: {
    functionId: string | null;
    functionHandle: string | null;
  },
): ShopifyFunctionRecord | null {
  if (input.functionId) {
    return runtime.store.getEffectiveShopifyFunctionById(input.functionId);
  }

  if (!input.functionHandle) {
    return null;
  }

  const functionHandle = input.functionHandle;
  const normalizedHandle = normalizeFunctionHandle(functionHandle);
  const handleBasedId = shopifyFunctionIdFromHandle(functionHandle);
  return (
    runtime.store
      .listEffectiveShopifyFunctions()
      .find(
        (candidate) =>
          candidate.handle === functionHandle ||
          candidate.handle === normalizedHandle ||
          candidate.id === handleBasedId,
      ) ?? null
  );
}

function ensureShopifyFunction(
  runtime: ProxyRuntimeContext,
  input: {
    functionId: string | null;
    functionHandle: string | null;
    apiType: string;
    fallbackTitle: string;
  },
): ShopifyFunctionRecord {
  const existing = findExistingShopifyFunction(runtime, input);
  const id =
    existing?.id ??
    input.functionId ??
    (input.functionHandle ? shopifyFunctionIdFromHandle(input.functionHandle) : null);
  const handle = input.functionHandle ?? existing?.handle ?? null;
  const title = existing?.title ?? (handle ? titleFromHandle(handle) : input.fallbackTitle);
  const shopifyFunction: ShopifyFunctionRecord = {
    id: id ?? runtime.syntheticIdentity.makeSyntheticGid('ShopifyFunction'),
    title,
    handle,
    apiType: input.apiType,
    ...(existing?.description !== undefined ? { description: existing.description } : {}),
    ...(existing?.appKey !== undefined ? { appKey: existing.appKey } : {}),
    ...(existing?.app !== undefined ? { app: existing.app } : {}),
  };
  runtime.store.upsertStagedShopifyFunction(shopifyFunction);
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
        result[key] = shopifyFunction.app
          ? projectGraphqlValue(shopifyFunction.app, selection.selectionSet?.selections ?? [], new Map())
          : null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeValidation(
  runtime: ProxyRuntimeContext,
  validation: ValidationRecord,
  field: FieldNode,
): Record<string, unknown> {
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
          ? runtime.store.getEffectiveShopifyFunctionById(validation.shopifyFunctionId)
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

function serializeValidationConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const items = runtime.store.listEffectiveValidations();
  const window = paginateConnectionItems(items, field, variables, (item) => item.id);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (item) => item.id,
    serializeNode: (item, selection) => serializeValidation(runtime, item, selection),
  });
}

function serializeCartTransformConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const items = runtime.store.listEffectiveCartTransforms();
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
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const apiType = typeof args['apiType'] === 'string' ? args['apiType'] : null;
  const items = apiType
    ? runtime.store.listEffectiveShopifyFunctions().filter((shopifyFunction) => shopifyFunction.apiType === apiType)
    : runtime.store.listEffectiveShopifyFunctions();
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
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  payload: { validation: ValidationRecord | null; userErrors: FunctionUserError[] },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'validation':
        result[key] = payload.validation ? serializeValidation(runtime, payload.validation, selection) : null;
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

function createValidation(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const input = isPlainObject(args['validation']) ? args['validation'] : {};
  const { functionId, functionHandle } = readFunctionReference(input);
  if (!functionId && !functionHandle) {
    return serializeValidationMutationPayload(runtime, field, {
      validation: null,
      userErrors: [missingFunctionError(['validation', 'functionHandle'])],
    });
  }

  const shopifyFunction = ensureShopifyFunction(runtime, {
    functionId,
    functionHandle,
    apiType: 'VALIDATION',
    fallbackTitle: readString(input['title']) ?? 'Local validation function',
  });
  const timestamp = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const validation: ValidationRecord = {
    id: runtime.syntheticIdentity.makeSyntheticGid('Validation'),
    title: readString(input['title']),
    enable: readBoolean(input['enable']) ?? readBoolean(input['enabled']) ?? true,
    blockOnFailure: readBoolean(input['blockOnFailure']) ?? false,
    functionId,
    functionHandle: functionHandle ?? shopifyFunction.handle ?? undefined,
    shopifyFunctionId: shopifyFunction.id,
    createdAt: timestamp,
    updatedAt: timestamp,
  };
  runtime.store.upsertStagedValidation(validation);
  return serializeValidationMutationPayload(runtime, field, { validation, userErrors: [] });
}

function updateValidation(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const id = readString(args['id']) ?? '';
  const current = runtime.store.getEffectiveValidationById(id);
  if (!current) {
    return serializeValidationMutationPayload(runtime, field, {
      validation: null,
      userErrors: [notFoundError('id', id)],
    });
  }

  const input = isPlainObject(args['validation']) ? args['validation'] : {};
  const { functionId, functionHandle } = readFunctionReference(input);
  const shopifyFunction =
    functionId || functionHandle
      ? ensureShopifyFunction(runtime, {
          functionId,
          functionHandle,
          apiType: 'VALIDATION',
          fallbackTitle: current.title ?? 'Local validation function',
        })
      : runtime.store.getEffectiveShopifyFunctionById(current.shopifyFunctionId ?? '');
  const validation: ValidationRecord = {
    ...current,
    title: readString(input['title']) ?? current.title,
    enable: readBoolean(input['enable']) ?? readBoolean(input['enabled']) ?? current.enable,
    blockOnFailure: readBoolean(input['blockOnFailure']) ?? current.blockOnFailure,
    functionId: functionId ?? (functionHandle !== null ? null : current.functionId),
    functionHandle:
      functionHandle ?? (functionId !== null ? (shopifyFunction?.handle ?? undefined) : current.functionHandle),
    shopifyFunctionId: shopifyFunction?.id ?? current.shopifyFunctionId,
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
  };
  runtime.store.upsertStagedValidation(validation);
  return serializeValidationMutationPayload(runtime, field, { validation, userErrors: [] });
}

function deleteValidation(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const id = readString(args['id']) ?? '';
  if (!runtime.store.getEffectiveValidationById(id)) {
    return serializeDeletePayload(field, { deletedId: null, userErrors: [notFoundError('id', id)] });
  }

  runtime.store.deleteStagedValidation(id);
  return serializeDeletePayload(field, { deletedId: id, userErrors: [] });
}

function createCartTransform(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const input = isPlainObject(args['cartTransform']) ? args['cartTransform'] : args;
  const { functionId, functionHandle } = readFunctionReference(input);
  if (!functionId && !functionHandle) {
    return serializeCartTransformMutationPayload(field, {
      cartTransform: null,
      userErrors: [missingFunctionError(['functionHandle'])],
    });
  }

  const shopifyFunction = ensureShopifyFunction(runtime, {
    functionId,
    functionHandle,
    apiType: 'CART_TRANSFORM',
    fallbackTitle: readString(input['title']) ?? 'Local cart transform function',
  });
  const timestamp = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const cartTransform: CartTransformRecord = {
    id: runtime.syntheticIdentity.makeSyntheticGid('CartTransform'),
    title: readString(input['title']) ?? shopifyFunction.title,
    blockOnFailure: readBoolean(input['blockOnFailure']) ?? false,
    functionId,
    functionHandle: functionHandle ?? shopifyFunction.handle ?? undefined,
    shopifyFunctionId: shopifyFunction.id,
    createdAt: timestamp,
    updatedAt: timestamp,
  };
  runtime.store.upsertStagedCartTransform(cartTransform);
  return serializeCartTransformMutationPayload(field, { cartTransform, userErrors: [] });
}

function deleteCartTransform(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const id = readString(args['id']) ?? '';
  if (!runtime.store.getEffectiveCartTransformById(id)) {
    return serializeDeletePayload(field, { deletedId: null, userErrors: [notFoundError('id', id)] });
  }

  runtime.store.deleteStagedCartTransform(id);
  return serializeDeletePayload(field, { deletedId: id, userErrors: [] });
}

function configureTaxApp(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const ready = readBoolean(args['ready']);
  const userErrors =
    ready === null ? [{ field: ['ready'], message: 'Ready must be true or false', code: 'INVALID' }] : [];
  let configuration = runtime.store.getEffectiveTaxAppConfiguration();

  if (ready !== null) {
    configuration = {
      id: 'gid://shopify/TaxAppConfiguration/local',
      ready,
      state: ready ? 'READY' : 'NOT_READY',
      updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    };
    runtime.store.setStagedTaxAppConfiguration(configuration);
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

export function handleFunctionQuery(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);
    switch (field.name.value) {
      case 'validation': {
        const id = readString(args['id']);
        const validation = id ? runtime.store.getEffectiveValidationById(id) : null;
        data[key] = validation ? serializeValidation(runtime, validation, field) : null;
        break;
      }
      case 'validations':
        data[key] = serializeValidationConnection(runtime, field, variables);
        break;
      case 'cartTransforms':
        data[key] = serializeCartTransformConnection(runtime, field, variables);
        break;
      case 'shopifyFunction': {
        const id = readString(args['id']);
        const shopifyFunction = id ? runtime.store.getEffectiveShopifyFunctionById(id) : null;
        data[key] = shopifyFunction ? serializeShopifyFunction(shopifyFunction, field) : null;
        break;
      }
      case 'shopifyFunctions':
        data[key] = serializeShopifyFunctionConnection(runtime, field, variables);
        break;
      default:
        data[key] = null;
        break;
    }
  }
  return { data };
}

export function handleFunctionMutation(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'validationCreate':
        data[key] = createValidation(runtime, field, variables);
        break;
      case 'validationUpdate':
        data[key] = updateValidation(runtime, field, variables);
        break;
      case 'validationDelete':
        data[key] = deleteValidation(runtime, field, variables);
        break;
      case 'cartTransformCreate':
        data[key] = createCartTransform(runtime, field, variables);
        break;
      case 'cartTransformDelete':
        data[key] = deleteCartTransform(runtime, field, variables);
        break;
      case 'taxAppConfigure':
        data[key] = configureTaxApp(runtime, field, variables);
        break;
      default:
        data[key] = null;
        break;
    }
  }
  return { data };
}
