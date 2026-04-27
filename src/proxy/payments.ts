import type { FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import type { JsonValue } from '../json-schemas.js';
import { normalizeSearchQueryValue, parseSearchQueryTerms, type SearchQueryTerm } from '../search-query-parser.js';
import { makeSyntheticGid } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type {
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
  field: FieldNode,
  variables: Record<string, unknown>,
): PaymentCustomizationRecord[] {
  const args = getFieldArguments(field, variables);
  const terms = parsePaymentCustomizationQuery(args['query']);
  const filtered = terms.length
    ? store
        .listEffectivePaymentCustomizations()
        .filter((customization) => terms.every((term) => matchesPaymentCustomizationTerm(customization, term)))
    : store.listEffectivePaymentCustomizations();
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
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const customizations = listPaymentCustomizationsForField(field, variables);
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
  field: FieldNode,
  variables: Record<string, unknown>,
): PaymentTermsTemplateRecord[] {
  const args = getFieldArguments(field, variables);
  const paymentTermsType = typeof args['paymentTermsType'] === 'string' ? args['paymentTermsType'] : null;
  const templates = store.listEffectivePaymentTermsTemplates();
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
  ownerId: string,
  input: Record<string, unknown>,
  existing: PaymentCustomizationMetafieldRecord[] = [],
): PaymentCustomizationMetafieldRecord[] {
  const inputs = readMetafieldInputObjects(input['metafields']);
  if (inputs.length === 0) {
    return existing;
  }

  return upsertOwnerMetafields('paymentCustomizationId', ownerId, inputs, existing, {
    allowIdLookup: true,
    ownerType: 'PAYMENT_CUSTOMIZATION',
    trimIdentity: true,
  }).metafields;
}

function buildPaymentCustomizationFromInput(input: Record<string, unknown>): PaymentCustomizationRecord {
  const id = makeSyntheticGid('PaymentCustomization');
  const customization: PaymentCustomizationRecord = {
    id,
    title: typeof input['title'] === 'string' ? input['title'] : null,
    enabled: typeof input['enabled'] === 'boolean' ? input['enabled'] : null,
    functionId: readFunctionId(input),
    functionHandle: readFunctionHandle(input),
    metafields: [],
  };
  customization.metafields = applyMetafieldInputs(id, input, []);
  return customization;
}

function updatePaymentCustomizationFromInput(
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
  next.metafields = applyMetafieldInputs(current.id, input, current.metafields ?? []);
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

function createPaymentCustomization(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const input = isPlainObject(args['paymentCustomization']) ? args['paymentCustomization'] : {};
  const userErrors = validateCreateInput(input);
  if (userErrors.length > 0) {
    return serializePaymentCustomizationMutationPayload(field, variables, {
      paymentCustomization: null,
      userErrors,
    });
  }

  const customization = buildPaymentCustomizationFromInput(input);
  store.upsertStagedPaymentCustomization(customization);
  return serializePaymentCustomizationMutationPayload(field, variables, {
    paymentCustomization: customization,
    userErrors: [],
  });
}

function updatePaymentCustomization(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : '';
  const current = store.getEffectivePaymentCustomizationById(id);
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

  const customization = updatePaymentCustomizationFromInput(current, input);
  store.upsertStagedPaymentCustomization(customization);
  return serializePaymentCustomizationMutationPayload(field, variables, {
    paymentCustomization: customization,
    userErrors: [],
  });
}

function deletePaymentCustomization(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : '';
  const current = store.getEffectivePaymentCustomizationById(id);
  if (!current) {
    return serializePaymentCustomizationDeletePayload(field, {
      deletedId: null,
      userErrors: [paymentCustomizationNotFoundError('id', id)],
    });
  }

  store.deleteStagedPaymentCustomization(id);
  return serializePaymentCustomizationDeletePayload(field, {
    deletedId: id,
    userErrors: [],
  });
}

function activatePaymentCustomizations(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const ids = Array.isArray(args['ids'])
    ? Array.from(new Set(args['ids'].filter((id): id is string => typeof id === 'string')))
    : [];
  const enabled = args['enabled'] === true;
  const updatedIds: string[] = [];
  const missingIds: string[] = [];

  for (const id of ids) {
    const current = store.getEffectivePaymentCustomizationById(id);
    if (!current) {
      missingIds.push(id);
      continue;
    }

    store.upsertStagedPaymentCustomization({
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

export function handlePaymentQuery(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);
    switch (field.name.value) {
      case 'paymentTermsTemplates':
        data[key] = listPaymentTermsTemplatesForField(field, variables).map((template) =>
          serializePaymentTermsTemplate(template, field),
        );
        break;
      case 'paymentCustomizations':
        data[key] = store.hasPaymentCustomizations()
          ? serializePaymentCustomizationsConnection(field, variables)
          : serializeEmptyPaymentCustomizationsConnection(field);
        break;
      case 'paymentCustomization':
        data[key] =
          typeof args['id'] === 'string'
            ? (() => {
                const customization = store.getEffectivePaymentCustomizationById(args['id']);
                return customization ? serializePaymentCustomization(customization, field, variables) : null;
              })()
            : null;
        break;
      default:
        data[key] = null;
        break;
    }
  }

  return { data };
}

export function handlePaymentMutation(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'paymentCustomizationCreate':
        data[key] = createPaymentCustomization(field, variables);
        break;
      case 'paymentCustomizationUpdate':
        data[key] = updatePaymentCustomization(field, variables);
        break;
      case 'paymentCustomizationDelete':
        data[key] = deletePaymentCustomization(field, variables);
        break;
      case 'paymentCustomizationActivation':
        data[key] = activatePaymentCustomizations(field, variables);
        break;
      default:
        data[key] = null;
        break;
    }
  }

  return { data };
}
