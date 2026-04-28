import type { ProxyRuntimeContext } from './runtime-context.js';
import type { FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import type { JsonValue } from '../json-schemas.js';
import { normalizeSearchQueryValue, parseSearchQueryTerms, type SearchQueryTerm } from '../search-query-parser.js';
import type {
  CustomerPaymentMethodInstrumentRecord,
  CustomerPaymentMethodRecord,
  PaymentCustomizationMetafieldRecord,
  PaymentCustomizationRecord,
  PaymentTermsTemplateRecord,
} from '../state/types.js';
import {
  getFieldResponseKey,
  getSelectedChildFields,
  paginateConnectionItems,
  serializeConnectionPageInfo,
  serializeEmptyConnectionPageInfo,
} from './graphql-helpers.js';
import {
  readMetafieldInputObjects,
  serializeMetafieldSelection,
  serializeMetafieldsConnection,
  upsertOwnerMetafields,
} from './metafields.js';
import { serializeCustomerPaymentMethodSelection } from './customers.js';

interface PaymentCustomizationUserError {
  field: string[] | null;
  message: string;
  code: string | null;
}

const CAPTURED_PAYMENT_CUSTOMIZATION_APP_ID = '347082227713';

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasOwnField(value: Record<string, unknown>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function parsePaymentCustomizationQuery(rawQuery: unknown): SearchQueryTerm[] {
  if (typeof rawQuery !== 'string' || rawQuery.trim().length === 0) {
    return [];
  }

  return parseSearchQueryTerms(rawQuery.trim(), { ignoredKeywords: ['AND'] }).filter(
    (term) => normalizeSearchQueryValue(term.value).length > 0,
  );
}

function gidTail(id: string | null | undefined): string | null {
  const tail = id?.split('/').at(-1) ?? null;
  return tail && tail.length > 0 ? tail : null;
}

function isCapturedMissingFunctionId(functionId: string): boolean {
  return gidTail(functionId) === '0';
}

function isCapturedMissingFunctionHandle(functionHandle: string): boolean {
  return ['0', 'missing-function', 'missing-payment-customization-function'].includes(
    normalizeSearchQueryValue(functionHandle),
  );
}

function matchesIdentifier(id: string | null | undefined, expected: string): boolean {
  if (!id) {
    return false;
  }

  const normalizedExpected = normalizeSearchQueryValue(expected);
  const normalizedId = id.toLowerCase();
  return normalizedId === normalizedExpected || (gidTail(id)?.toLowerCase() ?? '') === normalizedExpected;
}

function matchesBoolean(value: boolean | null, expected: string): boolean {
  const normalizedExpected = normalizeSearchQueryValue(expected);
  if (normalizedExpected === 'true') return value === true;
  if (normalizedExpected === 'false') return value === false;
  return false;
}

function matchesPositivePaymentCustomizationTerm(
  customization: PaymentCustomizationRecord,
  term: SearchQueryTerm,
): boolean {
  const field = term.field?.toLowerCase() ?? 'default';
  const value = normalizeSearchQueryValue(term.value);

  switch (field) {
    case 'default':
    case 'title':
      return (customization.title ?? '').toLowerCase().includes(value);
    case 'enabled':
      return matchesBoolean(customization.enabled, term.value);
    case 'function_id':
      return matchesIdentifier(customization.functionId, term.value);
    case 'id':
      return matchesIdentifier(customization.id, term.value);
    default:
      return false;
  }
}

function matchesPaymentCustomizationTerm(customization: PaymentCustomizationRecord, term: SearchQueryTerm): boolean {
  if (!term.raw) {
    return true;
  }

  const matches = matchesPositivePaymentCustomizationTerm(customization, term);
  return term.negated ? !matches : matches;
}

function listPaymentCustomizationsForField(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): PaymentCustomizationRecord[] {
  const args = getFieldArguments(field, variables);
  const terms = parsePaymentCustomizationQuery(args['query']);
  const filtered = terms.length
    ? runtime.store
        .listEffectivePaymentCustomizations()
        .filter((customization) => terms.every((term) => matchesPaymentCustomizationTerm(customization, term)))
    : runtime.store.listEffectivePaymentCustomizations();
  return args['reverse'] === true ? [...filtered].reverse() : filtered;
}

function isJsonRecord(value: unknown): value is Record<string, JsonValue> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function serializeCapturedJsonValue(value: JsonValue | undefined, field: FieldNode): unknown {
  if (value === undefined || value === null) {
    return null;
  }

  if (Array.isArray(value)) {
    return value.map((item) => serializeCapturedJsonValue(item, field));
  }

  const selectedFields = getSelectedChildFields(field);
  if (selectedFields.length === 0 || !isJsonRecord(value)) {
    return value;
  }

  const result: Record<string, unknown> = {};
  for (const selection of selectedFields) {
    const key = getFieldResponseKey(selection);
    result[key] = serializeCapturedJsonValue(value[selection.name.value], selection);
  }
  return result;
}

function serializePaymentCustomization(
  customization: PaymentCustomizationRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'PaymentCustomization';
        break;
      case 'id':
        result[key] = customization.id;
        break;
      case 'legacyResourceId':
        result[key] = gidTail(customization.id);
        break;
      case 'title':
        result[key] = customization.title;
        break;
      case 'enabled':
        result[key] = customization.enabled;
        break;
      case 'functionId':
        result[key] = customization.functionId;
        break;
      case 'shopifyFunction':
        result[key] = serializeCapturedJsonValue(customization.shopifyFunction, selection);
        break;
      case 'errorHistory':
        result[key] = serializeCapturedJsonValue(customization.errorHistory, selection);
        break;
      case 'metafield': {
        const args = getFieldArguments(selection, variables);
        const namespace = typeof args['namespace'] === 'string' ? args['namespace'] : null;
        const metafieldKey = typeof args['key'] === 'string' ? args['key'] : null;
        const metafield =
          namespace && metafieldKey
            ? (customization.metafields ?? []).find(
                (candidate) => candidate.namespace === namespace && candidate.key === metafieldKey,
              )
            : null;
        result[key] = metafield ? serializeMetafieldSelection(metafield, selection) : null;
        break;
      }
      case 'metafields':
        result[key] = serializeMetafieldsConnection(customization.metafields ?? [], selection, variables);
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializePaymentCustomizationUserErrors(
  errors: PaymentCustomizationUserError[],
  field: FieldNode,
): Array<Record<string, unknown>> {
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

function serializePaymentCustomizationsConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const customizations = listPaymentCustomizationsForField(runtime, field, variables);
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    customizations,
    field,
    variables,
    (customization) => customization.id,
  );
  const connection: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        connection[key] = items.map((customization) =>
          serializePaymentCustomization(customization, selection, variables),
        );
        break;
      case 'edges':
        connection[key] = items.map((customization) => {
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edge[edgeKey] = `cursor:${customization.id}`;
                break;
              case 'node':
                edge[edgeKey] = serializePaymentCustomization(customization, edgeSelection, variables);
                break;
              default:
                edge[edgeKey] = null;
                break;
            }
          }
          return edge;
        });
        break;
      case 'pageInfo':
        connection[key] = serializeConnectionPageInfo(
          selection,
          items,
          hasNextPage,
          hasPreviousPage,
          (customization) => customization.id,
        );
        break;
      default:
        connection[key] = null;
        break;
    }
  }

  return connection;
}

function serializeEmptyPaymentCustomizationsConnection(field: FieldNode): Record<string, unknown> {
  const connection: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
      case 'edges':
        connection[key] = [];
        break;
      case 'pageInfo':
        connection[key] = serializeEmptyConnectionPageInfo(selection);
        break;
      default:
        connection[key] = null;
        break;
    }
  }
  return connection;
}

function listPaymentTermsTemplatesForField(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): PaymentTermsTemplateRecord[] {
  const args = getFieldArguments(field, variables);
  const paymentTermsType = typeof args['paymentTermsType'] === 'string' ? args['paymentTermsType'] : null;
  const templates = runtime.store.listEffectivePaymentTermsTemplates();
  return paymentTermsType === null
    ? templates
    : templates.filter((template) => template.paymentTermsType === paymentTermsType);
}

function serializePaymentTermsTemplate(
  template: PaymentTermsTemplateRecord,
  field: FieldNode,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'PaymentTermsTemplate';
        break;
      case 'id':
        result[key] = template.id;
        break;
      case 'name':
        result[key] = template.name;
        break;
      case 'description':
        result[key] = template.description;
        break;
      case 'dueInDays':
        result[key] = template.dueInDays;
        break;
      case 'paymentTermsType':
        result[key] = template.paymentTermsType;
        break;
      case 'translatedName':
        result[key] = template.translatedName;
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializeEmptyConnectionSelection(field: FieldNode): Record<string, unknown> {
  const connection: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
      case 'edges':
        connection[key] = [];
        break;
      case 'pageInfo':
        connection[key] = serializeEmptyConnectionPageInfo(selection);
        break;
      default:
        connection[key] = null;
        break;
    }
  }
  return connection;
}

function requiredPaymentCustomizationInputError(fieldName: string): PaymentCustomizationUserError {
  return {
    field: ['paymentCustomization', fieldName],
    message: 'Required input field must be present.',
    code: 'REQUIRED_INPUT_FIELD',
  };
}

function functionNotFoundError(functionId: string): PaymentCustomizationUserError {
  return {
    field: ['paymentCustomization', 'functionId'],
    message: `Function ${functionId} not found. Ensure that it is released in the current app (${CAPTURED_PAYMENT_CUSTOMIZATION_APP_ID}), and that the app is installed.`,
    code: 'FUNCTION_NOT_FOUND',
  };
}

function functionHandleNotFoundError(functionHandle: string): PaymentCustomizationUserError {
  return {
    field: ['paymentCustomization', 'functionHandle'],
    message: `Function ${functionHandle} not found. Ensure that it is released in the current app (${CAPTURED_PAYMENT_CUSTOMIZATION_APP_ID}), and that the app is installed.`,
    code: 'FUNCTION_NOT_FOUND',
  };
}

function multipleFunctionIdentifiersError(): PaymentCustomizationUserError {
  return {
    field: ['paymentCustomization', 'functionHandle'],
    message: 'Only one of function_id or function_handle can be provided, not both.',
    code: 'MULTIPLE_FUNCTION_IDENTIFIERS',
  };
}

function paymentCustomizationNotFoundError(fieldName: string, id: string): PaymentCustomizationUserError {
  return {
    field: [fieldName],
    message: `Could not find PaymentCustomization with id: ${id}`,
    code: 'PAYMENT_CUSTOMIZATION_NOT_FOUND',
  };
}

function paymentCustomizationActivationNotFoundError(ids: string[]): PaymentCustomizationUserError {
  return {
    field: ['ids'],
    message: `Could not find payment customizations with IDs: ${ids.join(', ')}`,
    code: 'PAYMENT_CUSTOMIZATION_NOT_FOUND',
  };
}

function invalidMetafieldsError(): PaymentCustomizationUserError {
  return {
    field: ['paymentCustomization', 'metafields'],
    message: 'Could not create or update metafields.',
    code: 'INVALID_METAFIELDS',
  };
}

function readFunctionId(input: Record<string, unknown>): string | null {
  return typeof input['functionId'] === 'string' && input['functionId'].trim().length > 0 ? input['functionId'] : null;
}

function readFunctionHandle(input: Record<string, unknown>): string | null {
  return typeof input['functionHandle'] === 'string' && input['functionHandle'].trim().length > 0
    ? input['functionHandle']
    : null;
}

function hasFunctionIdentifier(input: Record<string, unknown>): boolean {
  return readFunctionId(input) !== null || readFunctionHandle(input) !== null;
}

function validateFunctionIdentifier(input: Record<string, unknown>): PaymentCustomizationUserError | null {
  const functionId = readFunctionId(input);
  const functionHandle = readFunctionHandle(input);
  if (functionId !== null && functionHandle !== null) {
    return multipleFunctionIdentifiersError();
  }

  if (functionId !== null && isCapturedMissingFunctionId(functionId)) {
    return functionNotFoundError(functionId);
  }

  if (functionHandle !== null && isCapturedMissingFunctionHandle(functionHandle)) {
    return functionHandleNotFoundError(functionHandle);
  }

  return null;
}

function validateMetafieldInputs(
  input: Record<string, unknown>,
  existingMetafields: PaymentCustomizationMetafieldRecord[] = [],
): PaymentCustomizationUserError | null {
  if (!hasOwnField(input, 'metafields')) {
    return null;
  }

  const metafields = input['metafields'];
  if (!Array.isArray(metafields)) {
    return invalidMetafieldsError();
  }

  for (const metafield of metafields) {
    if (!isPlainObject(metafield)) {
      return invalidMetafieldsError();
    }

    const hasId = typeof metafield['id'] === 'string' && metafield['id'].trim().length > 0;
    const namespace = typeof metafield['namespace'] === 'string' ? metafield['namespace'].trim() : '';
    const key = typeof metafield['key'] === 'string' ? metafield['key'].trim() : '';
    const type = typeof metafield['type'] === 'string' ? metafield['type'].trim() : '';
    const value = typeof metafield['value'] === 'string' ? metafield['value'] : null;
    const existingById = hasId
      ? (existingMetafields.find((candidate) => candidate.id === metafield['id']) ?? null)
      : null;
    if (!existingById && (!namespace || !key)) {
      return invalidMetafieldsError();
    }
    if (!(type || existingById?.type) || (value === null && existingById?.value == null)) {
      return invalidMetafieldsError();
    }
  }

  return null;
}

function validateCreateInput(input: Record<string, unknown>): PaymentCustomizationUserError[] {
  if (!hasOwnField(input, 'title')) {
    return [requiredPaymentCustomizationInputError('title')];
  }

  if (!hasOwnField(input, 'enabled')) {
    return [requiredPaymentCustomizationInputError('enabled')];
  }

  if (!hasFunctionIdentifier(input)) {
    return [requiredPaymentCustomizationInputError('functionId')];
  }

  const functionError = validateFunctionIdentifier(input);
  if (functionError) return [functionError];

  const metafieldError = validateMetafieldInputs(input);
  return metafieldError ? [metafieldError] : [];
}

function applyMetafieldInputs(
  runtime: ProxyRuntimeContext,
  ownerId: string,
  input: Record<string, unknown>,
  existing: PaymentCustomizationMetafieldRecord[] = [],
): PaymentCustomizationMetafieldRecord[] {
  const inputs = readMetafieldInputObjects(input['metafields']);
  if (inputs.length === 0) {
    return existing;
  }

  return upsertOwnerMetafields(runtime, 'paymentCustomizationId', ownerId, inputs, existing, {
    allowIdLookup: true,
    ownerType: 'PAYMENT_CUSTOMIZATION',
    trimIdentity: true,
  }).metafields;
}

function buildPaymentCustomizationFromInput(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
): PaymentCustomizationRecord {
  const id = runtime.syntheticIdentity.makeSyntheticGid('PaymentCustomization');
  const customization: PaymentCustomizationRecord = {
    id,
    title: typeof input['title'] === 'string' ? input['title'] : null,
    enabled: typeof input['enabled'] === 'boolean' ? input['enabled'] : null,
    functionId: readFunctionId(input),
    functionHandle: readFunctionHandle(input),
    metafields: [],
  };
  customization.metafields = applyMetafieldInputs(runtime, id, input, []);
  return customization;
}

function updatePaymentCustomizationFromInput(
  runtime: ProxyRuntimeContext,
  current: PaymentCustomizationRecord,
  input: Record<string, unknown>,
): PaymentCustomizationRecord {
  const next: PaymentCustomizationRecord = structuredClone(current);
  if (typeof input['title'] === 'string') {
    next.title = input['title'];
  }
  if (typeof input['enabled'] === 'boolean') {
    next.enabled = input['enabled'];
  }
  const functionId = readFunctionId(input);
  const functionHandle = readFunctionHandle(input);
  if (functionId !== null) {
    next.functionId = functionId;
    next.functionHandle = null;
    next.shopifyFunction = undefined;
  }
  if (functionHandle !== null) {
    next.functionHandle = functionHandle;
    next.functionId = null;
    next.shopifyFunction = undefined;
  }
  next.metafields = applyMetafieldInputs(runtime, current.id, input, current.metafields ?? []);
  return next;
}

function serializePaymentCustomizationMutationPayload(
  field: FieldNode,
  variables: Record<string, unknown>,
  payload: { paymentCustomization: PaymentCustomizationRecord | null; userErrors: PaymentCustomizationUserError[] },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'paymentCustomization':
        result[key] = payload.paymentCustomization
          ? serializePaymentCustomization(payload.paymentCustomization, selection, variables)
          : null;
        break;
      case 'userErrors':
        result[key] = serializePaymentCustomizationUserErrors(payload.userErrors, selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializePaymentCustomizationDeletePayload(
  field: FieldNode,
  payload: { deletedId: string | null; userErrors: PaymentCustomizationUserError[] },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'deletedId':
        result[key] = payload.deletedId;
        break;
      case 'userErrors':
        result[key] = serializePaymentCustomizationUserErrors(payload.userErrors, selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializePaymentCustomizationActivationPayload(
  field: FieldNode,
  payload: { ids: string[]; userErrors: PaymentCustomizationUserError[] },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'ids':
        result[key] = payload.ids;
        break;
      case 'userErrors':
        result[key] = serializePaymentCustomizationUserErrors(payload.userErrors, selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function createPaymentCustomization(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const input = isPlainObject(args['paymentCustomization']) ? args['paymentCustomization'] : {};
  const userErrors = validateCreateInput(input);
  if (userErrors.length > 0) {
    return serializePaymentCustomizationMutationPayload(field, variables, {
      paymentCustomization: null,
      userErrors,
    });
  }

  const customization = buildPaymentCustomizationFromInput(runtime, input);
  runtime.store.upsertStagedPaymentCustomization(customization);
  return serializePaymentCustomizationMutationPayload(field, variables, {
    paymentCustomization: customization,
    userErrors: [],
  });
}

function updatePaymentCustomization(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : '';
  const current = runtime.store.getEffectivePaymentCustomizationById(id);
  if (!current) {
    return serializePaymentCustomizationMutationPayload(field, variables, {
      paymentCustomization: null,
      userErrors: [paymentCustomizationNotFoundError('id', id)],
    });
  }

  const input = isPlainObject(args['paymentCustomization']) ? args['paymentCustomization'] : {};
  const functionError = validateFunctionIdentifier(input);
  if (functionError) {
    return serializePaymentCustomizationMutationPayload(field, variables, {
      paymentCustomization: null,
      userErrors: [functionError],
    });
  }

  const metafieldError = validateMetafieldInputs(input, current.metafields ?? []);
  if (metafieldError) {
    return serializePaymentCustomizationMutationPayload(field, variables, {
      paymentCustomization: null,
      userErrors: [metafieldError],
    });
  }

  const customization = updatePaymentCustomizationFromInput(runtime, current, input);
  runtime.store.upsertStagedPaymentCustomization(customization);
  return serializePaymentCustomizationMutationPayload(field, variables, {
    paymentCustomization: customization,
    userErrors: [],
  });
}

function deletePaymentCustomization(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : '';
  const current = runtime.store.getEffectivePaymentCustomizationById(id);
  if (!current) {
    return serializePaymentCustomizationDeletePayload(field, {
      deletedId: null,
      userErrors: [paymentCustomizationNotFoundError('id', id)],
    });
  }

  runtime.store.deleteStagedPaymentCustomization(id);
  return serializePaymentCustomizationDeletePayload(field, {
    deletedId: id,
    userErrors: [],
  });
}

function activatePaymentCustomizations(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const ids = Array.isArray(args['ids'])
    ? Array.from(new Set(args['ids'].filter((id): id is string => typeof id === 'string')))
    : [];
  const enabled = args['enabled'] === true;
  const updatedIds: string[] = [];
  const missingIds: string[] = [];

  for (const id of ids) {
    const current = runtime.store.getEffectivePaymentCustomizationById(id);
    if (!current) {
      missingIds.push(id);
      continue;
    }

    runtime.store.upsertStagedPaymentCustomization({
      ...current,
      enabled,
    });
    updatedIds.push(id);
  }

  return serializePaymentCustomizationActivationPayload(field, {
    ids: updatedIds,
    userErrors: missingIds.length > 0 ? [paymentCustomizationActivationNotFoundError(missingIds)] : [],
  });
}

interface CustomerPaymentMethodUserError {
  field: string[] | null;
  message: string;
  code?: string | null;
}

const DUPLICATION_DATA_PREFIX = 'shopify-draft-proxy:customer-payment-method-duplication:';

function isShopifyGid(value: string | null | undefined, resourceType: string): boolean {
  return typeof value === 'string' && value.startsWith(`gid://shopify/${resourceType}/`);
}

function paymentMethodDoesNotExistError(fieldName = 'customerPaymentMethodId'): CustomerPaymentMethodUserError {
  return {
    field: [fieldName],
    message: 'Customer payment method does not exist',
    code: 'PAYMENT_METHOD_DOES_NOT_EXIST',
  };
}

function customerDoesNotExistError(fieldName = 'customerId'): CustomerPaymentMethodUserError {
  return {
    field: [fieldName],
    message: 'Customer does not exist',
    code: 'CUSTOMER_DOES_NOT_EXIST',
  };
}

function invalidDuplicationDataError(): CustomerPaymentMethodUserError {
  return {
    field: ['encryptedDuplicationData'],
    message: 'Encrypted duplication data is invalid',
    code: 'INVALID_ENCRYPTED_DUPLICATION_DATA',
  };
}

function paymentReminderSendError(): CustomerPaymentMethodUserError {
  return {
    field: ['paymentScheduleId'],
    message: 'Payment reminder could not be sent',
    code: 'PAYMENT_REMINDER_SEND_UNSUCCESSFUL',
  };
}

function exactlyOneRemoteReferenceError(): CustomerPaymentMethodUserError {
  return {
    field: ['remoteReference'],
    message: 'Exactly one remote reference is required',
    code: 'EXACTLY_ONE_REMOTE_REFERENCE_REQUIRED',
  };
}

function serializeCustomerPaymentMethodUserErrors(
  errors: CustomerPaymentMethodUserError[],
  field: FieldNode,
): Array<Record<string, unknown>> {
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
          result[key] = error.code ?? null;
          break;
        default:
          result[key] = null;
          break;
      }
    }
    return result;
  });
}

function serializeCustomerPaymentMethodMutationPayload(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  payload: {
    customerPaymentMethod?: CustomerPaymentMethodRecord | null;
    encryptedDuplicationData?: string | null;
    processing?: boolean | null;
    revokedCustomerPaymentMethodId?: string | null;
    success?: boolean | null;
    updatePaymentMethodUrl?: string | null;
    userErrors: CustomerPaymentMethodUserError[];
  },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'customerPaymentMethod':
        result[key] = payload.customerPaymentMethod
          ? serializeCustomerPaymentMethodSelection(runtime, payload.customerPaymentMethod, selection, variables)
          : null;
        break;
      case 'encryptedDuplicationData':
        result[key] = payload.encryptedDuplicationData ?? null;
        break;
      case 'processing':
        result[key] = payload.processing ?? false;
        break;
      case 'revokedCustomerPaymentMethodId':
        result[key] = payload.revokedCustomerPaymentMethodId ?? null;
        break;
      case 'success':
        result[key] = payload.success ?? null;
        break;
      case 'updatePaymentMethodUrl':
        result[key] = payload.updatePaymentMethodUrl ?? null;
        break;
      case 'userErrors':
        result[key] = serializeCustomerPaymentMethodUserErrors(payload.userErrors, selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function scrubbedCreditCardInstrument(): CustomerPaymentMethodInstrumentRecord {
  return {
    typeName: 'CustomerCreditCard',
    data: {
      __typename: 'CustomerCreditCard',
      brand: null,
      lastDigits: null,
      expiryMonth: null,
      expiryYear: null,
      name: null,
      maskedNumber: null,
    },
  };
}

function scrubbedPaypalInstrument(inactive: boolean): CustomerPaymentMethodInstrumentRecord {
  return {
    typeName: 'CustomerPaypalBillingAgreement',
    data: {
      __typename: 'CustomerPaypalBillingAgreement',
      paypalAccountEmail: null,
      inactive,
    },
  };
}

function createCustomerPaymentMethod(
  runtime: ProxyRuntimeContext,
  customerId: string,
  instrument: CustomerPaymentMethodInstrumentRecord | null,
): CustomerPaymentMethodRecord {
  return runtime.store.stageUpsertCustomerPaymentMethod({
    id: runtime.syntheticIdentity.makeSyntheticGid('CustomerPaymentMethod'),
    customerId,
    instrument,
    revokedAt: null,
    revokedReason: null,
    subscriptionContracts: [],
  });
}

function updateCustomerPaymentMethod(
  runtime: ProxyRuntimeContext,
  current: CustomerPaymentMethodRecord,
  instrument: CustomerPaymentMethodInstrumentRecord | null,
): CustomerPaymentMethodRecord {
  return runtime.store.stageUpsertCustomerPaymentMethod({
    ...current,
    instrument,
  });
}

function activeCustomerPaymentMethodById(
  runtime: ProxyRuntimeContext,
  paymentMethodId: string | null,
  fieldName = 'customerPaymentMethodId',
): { paymentMethod: CustomerPaymentMethodRecord | null; error: CustomerPaymentMethodUserError | null } {
  if (typeof paymentMethodId !== 'string' || !isShopifyGid(paymentMethodId, 'CustomerPaymentMethod')) {
    return { paymentMethod: null, error: paymentMethodDoesNotExistError(fieldName) };
  }

  const paymentMethod = runtime.store.getEffectiveCustomerPaymentMethodById(paymentMethodId);
  return paymentMethod
    ? { paymentMethod, error: null }
    : { paymentMethod: null, error: paymentMethodDoesNotExistError(fieldName) };
}

function countRemoteReferenceKinds(remoteReference: unknown): number {
  if (!isPlainObject(remoteReference)) {
    return 0;
  }

  return Object.values(remoteReference).filter((value) => isPlainObject(value)).length;
}

function encodeDuplicationData(payload: Record<string, string>): string {
  return `${DUPLICATION_DATA_PREFIX}${Buffer.from(JSON.stringify(payload), 'utf8').toString('base64url')}`;
}

function decodeDuplicationData(raw: unknown): Record<string, string> | null {
  if (typeof raw !== 'string' || !raw.startsWith(DUPLICATION_DATA_PREFIX)) {
    return null;
  }

  try {
    const decoded = JSON.parse(
      Buffer.from(raw.slice(DUPLICATION_DATA_PREFIX.length), 'base64url').toString('utf8'),
    ) as unknown;
    if (!isPlainObject(decoded)) {
      return null;
    }

    const result: Record<string, string> = {};
    for (const [key, value] of Object.entries(decoded)) {
      if (typeof value !== 'string') {
        return null;
      }
      result[key] = value;
    }
    return result;
  } catch {
    return null;
  }
}

function buildPaymentMethodUpdateUrl(paymentMethodId: string): string {
  const tail = gidTail(paymentMethodId) ?? 'unknown';
  return `https://shopify-draft-proxy.local/customer-payment-methods/${encodeURIComponent(
    tail,
  )}/update?token=local-only`;
}

function createCreditCardPaymentMethod(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const customerId = typeof args['customerId'] === 'string' ? args['customerId'] : null;
  const customer =
    customerId && isShopifyGid(customerId, 'Customer') ? runtime.store.getEffectiveCustomerById(customerId) : null;
  if (!customerId || !customer) {
    return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
      customerPaymentMethod: null,
      processing: false,
      userErrors: [customerDoesNotExistError()],
    });
  }

  const paymentMethod = createCustomerPaymentMethod(runtime, customerId, scrubbedCreditCardInstrument());
  return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
    customerPaymentMethod: paymentMethod,
    processing: false,
    userErrors: [],
  });
}

function updateCreditCardPaymentMethod(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const { paymentMethod, error } = activeCustomerPaymentMethodById(runtime, id, 'id');
  if (!paymentMethod || error) {
    return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
      customerPaymentMethod: null,
      processing: false,
      userErrors: [error ?? paymentMethodDoesNotExistError('id')],
    });
  }

  const updated = updateCustomerPaymentMethod(runtime, paymentMethod, scrubbedCreditCardInstrument());
  return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
    customerPaymentMethod: updated,
    processing: false,
    userErrors: [],
  });
}

function createRemotePaymentMethod(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const customerId = typeof args['customerId'] === 'string' ? args['customerId'] : null;
  const customer =
    customerId && isShopifyGid(customerId, 'Customer') ? runtime.store.getEffectiveCustomerById(customerId) : null;
  if (!customerId || !customer) {
    return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
      customerPaymentMethod: null,
      userErrors: [customerDoesNotExistError()],
    });
  }

  if (countRemoteReferenceKinds(args['remoteReference']) !== 1) {
    return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
      customerPaymentMethod: null,
      userErrors: [exactlyOneRemoteReferenceError()],
    });
  }

  const paymentMethod = createCustomerPaymentMethod(runtime, customerId, null);
  return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
    customerPaymentMethod: paymentMethod,
    userErrors: [],
  });
}

function createPaypalPaymentMethod(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const customerId = typeof args['customerId'] === 'string' ? args['customerId'] : null;
  const customer =
    customerId && isShopifyGid(customerId, 'Customer') ? runtime.store.getEffectiveCustomerById(customerId) : null;
  if (!customerId || !customer) {
    return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
      customerPaymentMethod: null,
      userErrors: [customerDoesNotExistError()],
    });
  }

  const paymentMethod = createCustomerPaymentMethod(
    runtime,
    customerId,
    scrubbedPaypalInstrument(args['inactive'] === true),
  );
  return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
    customerPaymentMethod: paymentMethod,
    userErrors: [],
  });
}

function updatePaypalPaymentMethod(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const { paymentMethod, error } = activeCustomerPaymentMethodById(runtime, id, 'id');
  if (!paymentMethod || error) {
    return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
      customerPaymentMethod: null,
      userErrors: [error ?? paymentMethodDoesNotExistError('id')],
    });
  }

  const inactive =
    paymentMethod.instrument?.typeName === 'CustomerPaypalBillingAgreement'
      ? paymentMethod.instrument.data['inactive'] === true
      : false;
  const updated = updateCustomerPaymentMethod(runtime, paymentMethod, scrubbedPaypalInstrument(inactive));
  return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
    customerPaymentMethod: updated,
    userErrors: [],
  });
}

function getPaymentMethodDuplicationData(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const paymentMethodId = typeof args['customerPaymentMethodId'] === 'string' ? args['customerPaymentMethodId'] : null;
  const { paymentMethod, error } = activeCustomerPaymentMethodById(runtime, paymentMethodId);
  if (!paymentMethod || error) {
    return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
      encryptedDuplicationData: null,
      userErrors: [error ?? paymentMethodDoesNotExistError()],
    });
  }

  const targetCustomerId = typeof args['targetCustomerId'] === 'string' ? args['targetCustomerId'] : null;
  const targetCustomer =
    targetCustomerId && isShopifyGid(targetCustomerId, 'Customer')
      ? runtime.store.getEffectiveCustomerById(targetCustomerId)
      : null;
  if (!targetCustomerId || !targetCustomer) {
    return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
      encryptedDuplicationData: null,
      userErrors: [customerDoesNotExistError('targetCustomerId')],
    });
  }

  const targetShopId = typeof args['targetShopId'] === 'string' ? args['targetShopId'] : '';
  return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
    encryptedDuplicationData: encodeDuplicationData({
      customerPaymentMethodId: paymentMethod.id,
      targetCustomerId,
      targetShopId,
    }),
    userErrors: [],
  });
}

function createPaymentMethodFromDuplicationData(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const customerId = typeof args['customerId'] === 'string' ? args['customerId'] : null;
  const customer =
    customerId && isShopifyGid(customerId, 'Customer') ? runtime.store.getEffectiveCustomerById(customerId) : null;
  if (!customerId || !customer) {
    return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
      customerPaymentMethod: null,
      userErrors: [customerDoesNotExistError()],
    });
  }

  const decoded = decodeDuplicationData(args['encryptedDuplicationData']);
  const sourcePaymentMethodId = decoded?.['customerPaymentMethodId'] ?? null;
  const sourcePaymentMethod =
    sourcePaymentMethodId && isShopifyGid(sourcePaymentMethodId, 'CustomerPaymentMethod')
      ? runtime.store.getEffectiveCustomerPaymentMethodById(sourcePaymentMethodId, { showRevoked: true })
      : null;
  if (!decoded || !sourcePaymentMethod || decoded['targetCustomerId'] !== customerId) {
    return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
      customerPaymentMethod: null,
      userErrors: [invalidDuplicationDataError()],
    });
  }

  const paymentMethod = createCustomerPaymentMethod(runtime, customerId, sourcePaymentMethod.instrument);
  return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
    customerPaymentMethod: paymentMethod,
    userErrors: [],
  });
}

function getPaymentMethodUpdateUrl(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const paymentMethodId = typeof args['customerPaymentMethodId'] === 'string' ? args['customerPaymentMethodId'] : null;
  const { paymentMethod, error } = activeCustomerPaymentMethodById(runtime, paymentMethodId);
  if (!paymentMethod || error) {
    return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
      updatePaymentMethodUrl: null,
      userErrors: [error ?? paymentMethodDoesNotExistError()],
    });
  }

  const updateUrl = runtime.store.stageCustomerPaymentMethodUpdateUrl({
    id: runtime.syntheticIdentity.makeSyntheticGid('CustomerPaymentMethodUpdateUrl'),
    customerPaymentMethodId: paymentMethod.id,
    updatePaymentMethodUrl: buildPaymentMethodUpdateUrl(paymentMethod.id),
    createdAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
  });
  return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
    updatePaymentMethodUrl: updateUrl.updatePaymentMethodUrl,
    userErrors: [],
  });
}

function revokePaymentMethod(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const paymentMethodId = typeof args['customerPaymentMethodId'] === 'string' ? args['customerPaymentMethodId'] : null;
  const { paymentMethod, error } = activeCustomerPaymentMethodById(runtime, paymentMethodId);
  if (!paymentMethod || error) {
    return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
      revokedCustomerPaymentMethodId: null,
      userErrors: [error ?? paymentMethodDoesNotExistError()],
    });
  }

  runtime.store.stageUpsertCustomerPaymentMethod({
    ...paymentMethod,
    revokedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    revokedReason: 'CUSTOMER_REVOKED',
  });
  return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
    revokedCustomerPaymentMethodId: paymentMethod.id,
    userErrors: [],
  });
}

function sendPaymentReminder(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const paymentScheduleId = typeof args['paymentScheduleId'] === 'string' ? args['paymentScheduleId'] : null;
  if (typeof paymentScheduleId !== 'string' || !isShopifyGid(paymentScheduleId, 'PaymentSchedule')) {
    return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
      success: false,
      userErrors: [paymentReminderSendError()],
    });
  }

  runtime.store.stagePaymentReminderSend({
    id: runtime.syntheticIdentity.makeSyntheticGid('PaymentReminderSend'),
    paymentScheduleId,
    sentAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
  });
  return serializeCustomerPaymentMethodMutationPayload(runtime, field, variables, {
    success: true,
    userErrors: [],
  });
}

export function handlePaymentQuery(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);
    switch (field.name.value) {
      case 'paymentTermsTemplates':
        data[key] = listPaymentTermsTemplatesForField(runtime, field, variables).map((template) =>
          serializePaymentTermsTemplate(template, field),
        );
        break;
      case 'paymentCustomizations':
        data[key] = runtime.store.hasPaymentCustomizations()
          ? serializePaymentCustomizationsConnection(runtime, field, variables)
          : serializeEmptyPaymentCustomizationsConnection(field);
        break;
      case 'paymentCustomization':
        data[key] =
          typeof args['id'] === 'string'
            ? (() => {
                const customization = runtime.store.getEffectivePaymentCustomizationById(args['id']);
                return customization ? serializePaymentCustomization(customization, field, variables) : null;
              })()
            : null;
        break;
      case 'cashTrackingSession':
      case 'pointOfSaleDevice':
      case 'dispute':
      case 'disputeEvidence':
      case 'shopPayPaymentRequestReceipt':
        data[key] = null;
        break;
      case 'cashTrackingSessions':
      case 'disputes':
      case 'shopPayPaymentRequestReceipts':
        data[key] = serializeEmptyConnectionSelection(field);
        break;
      default:
        data[key] = null;
        break;
    }
  }

  return { data };
}

export function handlePaymentMutation(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'paymentCustomizationCreate':
        data[key] = createPaymentCustomization(runtime, field, variables);
        break;
      case 'paymentCustomizationUpdate':
        data[key] = updatePaymentCustomization(runtime, field, variables);
        break;
      case 'paymentCustomizationDelete':
        data[key] = deletePaymentCustomization(runtime, field, variables);
        break;
      case 'paymentCustomizationActivation':
        data[key] = activatePaymentCustomizations(runtime, field, variables);
        break;
      case 'customerPaymentMethodCreditCardCreate':
        data[key] = createCreditCardPaymentMethod(runtime, field, variables);
        break;
      case 'customerPaymentMethodCreditCardUpdate':
        data[key] = updateCreditCardPaymentMethod(runtime, field, variables);
        break;
      case 'customerPaymentMethodRemoteCreate':
        data[key] = createRemotePaymentMethod(runtime, field, variables);
        break;
      case 'customerPaymentMethodPaypalBillingAgreementCreate':
        data[key] = createPaypalPaymentMethod(runtime, field, variables);
        break;
      case 'customerPaymentMethodPaypalBillingAgreementUpdate':
        data[key] = updatePaypalPaymentMethod(runtime, field, variables);
        break;
      case 'customerPaymentMethodGetDuplicationData':
        data[key] = getPaymentMethodDuplicationData(runtime, field, variables);
        break;
      case 'customerPaymentMethodCreateFromDuplicationData':
        data[key] = createPaymentMethodFromDuplicationData(runtime, field, variables);
        break;
      case 'customerPaymentMethodGetUpdateUrl':
        data[key] = getPaymentMethodUpdateUrl(runtime, field, variables);
        break;
      case 'customerPaymentMethodRevoke':
        data[key] = revokePaymentMethod(runtime, field, variables);
        break;
      case 'paymentReminderSend':
        data[key] = sendPaymentReminder(runtime, field, variables);
        break;
      default:
        data[key] = null;
        break;
    }
  }

  return { data };
}
