import { Kind, parse, type FieldNode, type OperationDefinitionNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import {
  applySearchQuery,
  matchesSearchQueryDate,
  matchesSearchQueryString,
  normalizeSearchQueryValue,
  parseSearchQueryTermList,
  type SearchQueryTerm,
} from '../search-query-parser.js';
import { compareShopifyResourceIds } from '../shopify/resource-ids.js';
import { store } from '../state/store.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import type { BulkOperationRecord } from '../state/types.js';
import {
  getFieldResponseKey,
  getNodeLocation,
  getSelectedChildFields,
  getVariableDefinitionLocation,
  paginateConnectionItems,
  serializeConnection,
} from './graphql-helpers.js';
import {
  listProductsForBulkExport,
  listProductVariantsForBulkExport,
  listProductVariantsForProductBulkExport,
  serializeProductBulkSelection,
  serializeProductVariantBulkSelection,
} from './products.js';

type BulkOperationUserError = {
  field: string[] | null;
  message: string;
};

type GraphqlResponseBody = {
  data?: Record<string, unknown> | null;
  errors?: Array<Record<string, unknown>>;
};

type BulkOperationMutationResult = {
  response: GraphqlResponseBody;
  stagedResourceIds: string[];
  notes: string;
};

type BulkQueryValidationResult =
  | {
      ok: true;
      rootField: FieldNode;
      rootNodeField: FieldNode;
      nestedVariantField: FieldNode | null;
      nestedVariantNodeField: FieldNode | null;
    }
  | {
      ok: false;
      error: BulkOperationUserError;
    };

const BULK_OPERATION_TERMINAL_STATUSES = new Set(['CANCELED', 'COMPLETED', 'EXPIRED', 'FAILED']);

function isBulkOperationGid(id: string): boolean {
  return /^gid:\/\/shopify\/BulkOperation\/[^/]+$/u.test(id);
}

function getArgumentVariableName(field: FieldNode, argumentName: string): string | null {
  const argument = field.arguments?.find((candidate) => candidate.name.value === argumentName);
  return argument?.value.kind === 'Variable' ? argument.value.name.value : null;
}

function missingRequiredArgumentResponse(
  field: FieldNode,
  operationLabel: string,
  argumentName = 'id',
): GraphqlResponseBody {
  return {
    errors: [
      {
        message: `Field '${field.name.value}' is missing required arguments: ${argumentName}`,
        locations: getNodeLocation(field),
        path: [operationLabel, field.name.value],
        extensions: {
          code: 'missingRequiredArguments',
          className: 'Field',
          name: field.name.value,
          arguments: argumentName,
        },
      },
    ],
  };
}

function invalidBulkOperationIdResponse(document: string, field: FieldNode, id: string): GraphqlResponseBody {
  const variableName = getArgumentVariableName(field, 'id');
  const locations = variableName ? getVariableDefinitionLocation(document, variableName) : getNodeLocation(field);

  return {
    errors: [
      {
        message: variableName
          ? `Variable $${variableName} of type ID! was provided invalid value`
          : `Invalid global id '${id}'`,
        locations,
        extensions: {
          code: variableName ? 'INVALID_VARIABLE' : 'BAD_REQUEST',
          value: id,
          problems: [
            {
              path: [],
              explanation: `Invalid global id '${id}'`,
              message: `Invalid global id '${id}'`,
            },
          ],
        },
      },
    ],
  };
}

function badBulkOperationsRequestResponse(field: FieldNode, message: string): GraphqlResponseBody {
  return {
    errors: [
      {
        message,
        locations: getNodeLocation(field),
        extensions: {
          code: 'BAD_REQUEST',
        },
        path: [field.name.value],
      },
    ],
    data: null,
  };
}

function serializeBulkOperation(operation: BulkOperationRecord, field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'BulkOperation';
        break;
      case 'id':
        result[key] = operation.id;
        break;
      case 'status':
        result[key] = operation.status;
        break;
      case 'type':
        result[key] = operation.type;
        break;
      case 'errorCode':
        result[key] = operation.errorCode;
        break;
      case 'createdAt':
        result[key] = operation.createdAt;
        break;
      case 'completedAt':
        result[key] = operation.completedAt;
        break;
      case 'objectCount':
        result[key] = operation.objectCount;
        break;
      case 'rootObjectCount':
        result[key] = operation.rootObjectCount;
        break;
      case 'fileSize':
        result[key] = operation.fileSize;
        break;
      case 'url':
        result[key] = operation.url;
        break;
      case 'partialDataUrl':
        result[key] = operation.partialDataUrl;
        break;
      case 'query':
        result[key] = operation.query;
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializeBulkOperationUserErrors(
  errors: BulkOperationUserError[],
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
        default:
          result[key] = null;
          break;
      }
    }
    return result;
  });
}

function bulkOperationCursor(operation: BulkOperationRecord): string {
  return operation.cursor ?? operation.id;
}

function matchesBulkOperationIdentifier(id: string, expected: string): boolean {
  return matchesSearchQueryString(id, expected) || matchesSearchQueryString(id.split('/').at(-1) ?? null, expected);
}

function matchesPositiveBulkOperationTerm(operation: BulkOperationRecord, term: SearchQueryTerm): boolean {
  const field = term.field?.toLowerCase() ?? 'default';

  switch (field) {
    case 'default':
    case 'id':
      return matchesBulkOperationIdentifier(operation.id, term.value);
    case 'status':
      return matchesSearchQueryString(operation.status, term.value);
    case 'operation_type':
    case 'type':
      return matchesSearchQueryString(operation.type, term.value);
    case 'created_at':
      return matchesSearchQueryDate(operation.createdAt, term);
    default:
      return false;
  }
}

function hasInvalidCreatedAtFilter(rawQuery: unknown): boolean {
  return parseSearchQueryTermList(rawQuery, { dropEmptyValues: true }).some((term) => {
    if (term.field?.toLowerCase() !== 'created_at') {
      return false;
    }

    const expectedValue = normalizeSearchQueryValue(term.value);
    return expectedValue !== 'now' && Number.isNaN(Date.parse(expectedValue));
  });
}

function sortBulkOperations(
  operations: BulkOperationRecord[],
  sortKey: unknown,
  reverse: unknown,
): BulkOperationRecord[] {
  const normalizedSortKey = typeof sortKey === 'string' ? sortKey : 'CREATED_AT';
  const sorted = [...operations].sort((left, right) => {
    switch (normalizedSortKey) {
      case 'ID':
        return compareShopifyResourceIds(left.id, right.id);
      case 'CREATED_AT':
      default:
        return right.createdAt.localeCompare(left.createdAt) || compareShopifyResourceIds(right.id, left.id);
    }
  });

  return reverse === true ? sorted.reverse() : sorted;
}

function listBulkOperationsForField(field: FieldNode, variables: Record<string, unknown>): BulkOperationRecord[] {
  const args = getFieldArguments(field, variables);
  const filtered = applySearchQuery(
    store.listEffectiveBulkOperations(),
    args['query'],
    {},
    matchesPositiveBulkOperationTerm,
  );

  return sortBulkOperations(filtered, args['sortKey'], args['reverse']);
}

function serializeBulkOperationsConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const operations = listBulkOperationsForField(field, variables);
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    operations,
    field,
    variables,
    bulkOperationCursor,
  );

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: bulkOperationCursor,
    serializeNode: (operation, nodeField) => serializeBulkOperation(operation, nodeField),
  });
}

function getCurrentBulkOperation(field: FieldNode, variables: Record<string, unknown>): BulkOperationRecord | null {
  const args = getFieldArguments(field, variables);
  const requestedType = typeof args['type'] === 'string' ? args['type'] : 'QUERY';
  const [operation] = sortBulkOperations(
    store.listEffectiveBulkOperations().filter((candidate) => candidate.type === requestedType),
    'CREATED_AT',
    false,
  );

  return operation ?? null;
}

function validateBulkOperationsWindow(
  field: FieldNode,
  variables: Record<string, unknown>,
): GraphqlResponseBody | null {
  const args = getFieldArguments(field, variables);
  const hasFirst = typeof args['first'] === 'number';
  const hasLast = typeof args['last'] === 'number';

  if (!hasFirst && !hasLast) {
    return badBulkOperationsRequestResponse(field, 'you must provide one of first or last');
  }

  if (hasFirst && hasLast) {
    return badBulkOperationsRequestResponse(field, 'providing both first and last is not supported');
  }

  if (hasInvalidCreatedAtFilter(args['query'])) {
    return badBulkOperationsRequestResponse(field, 'Invalid timestamp for query filter `created_at`.');
  }

  return null;
}

function serializeCancelPayload(
  field: FieldNode,
  operation: BulkOperationRecord | null,
  userErrors: BulkOperationUserError[],
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'bulkOperation':
        payload[key] = operation ? serializeBulkOperation(operation, selection) : null;
        break;
      case 'userErrors':
        payload[key] = serializeBulkOperationUserErrors(userErrors, selection);
        break;
      default:
        payload[key] = null;
        break;
    }
  }

  return payload;
}

function serializeRunQueryPayload(
  field: FieldNode,
  operation: BulkOperationRecord | null,
  userErrors: BulkOperationUserError[],
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'bulkOperation':
        payload[key] = operation ? serializeBulkOperation(operation, selection) : null;
        break;
      case 'userErrors':
        payload[key] = serializeBulkOperationUserErrors(userErrors, selection);
        break;
      default:
        payload[key] = null;
        break;
    }
  }

  return payload;
}

function selectedFields(selections: readonly SelectionNode[] | undefined): FieldNode[] {
  return (selections ?? []).filter((selection): selection is FieldNode => selection.kind === Kind.FIELD);
}

function getOperationDefinition(document: string): OperationDefinitionNode {
  const ast = parse(document);
  const operation = ast.definitions.find(
    (definition): definition is OperationDefinitionNode => definition.kind === Kind.OPERATION_DEFINITION,
  );
  if (!operation) {
    throw new Error('No GraphQL operation found');
  }
  return operation;
}

function findConnectionNodeField(connectionField: FieldNode): FieldNode | null {
  for (const selection of selectedFields(connectionField.selectionSet?.selections)) {
    if (selection.name.value === 'nodes') {
      return selection;
    }

    if (selection.name.value !== 'edges') {
      continue;
    }

    const nodeField = selectedFields(selection.selectionSet?.selections).find((field) => field.name.value === 'node');
    if (nodeField) {
      return nodeField;
    }
  }

  return null;
}

function fieldContainsConnectionSelection(field: FieldNode): boolean {
  const childNames = selectedFields(field.selectionSet?.selections).map((selection) => selection.name.value);
  return childNames.includes('edges') || childNames.includes('nodes');
}

function countConnectionSelections(field: FieldNode): number {
  const ownCount = fieldContainsConnectionSelection(field) ? 1 : 0;
  return (
    ownCount +
    selectedFields(field.selectionSet?.selections).reduce(
      (sum, selection) => sum + countConnectionSelections(selection),
      0,
    )
  );
}

function maxNestedConnectionDepth(field: FieldNode, depth = 0): number {
  const nextDepth = fieldContainsConnectionSelection(field) ? depth + 1 : depth;
  return selectedFields(field.selectionSet?.selections).reduce(
    (maxDepth, selection) => Math.max(maxDepth, maxNestedConnectionDepth(selection, nextDepth)),
    nextDepth,
  );
}

function productNodeSelectionsWithoutConnections(nodeField: FieldNode): SelectionNode[] {
  return (nodeField.selectionSet?.selections ?? []).filter((selection) => {
    return selection.kind !== Kind.FIELD || !fieldContainsConnectionSelection(selection);
  });
}

function findUnsupportedConnectionSelection(
  nodeField: FieldNode,
  allowedDirectConnection: FieldNode | null,
): FieldNode | null {
  for (const selection of selectedFields(nodeField.selectionSet?.selections)) {
    if (fieldContainsConnectionSelection(selection)) {
      if (allowedDirectConnection && selection === allowedDirectConnection) {
        const nestedNodeField = findConnectionNodeField(selection);
        const nestedUnsupportedSelection = nestedNodeField
          ? findUnsupportedConnectionSelection(nestedNodeField, null)
          : null;
        if (nestedUnsupportedSelection) {
          return nestedUnsupportedSelection;
        }
        continue;
      }

      return selection;
    }

    const nestedUnsupportedSelection = findUnsupportedConnectionSelection(selection, null);
    if (nestedUnsupportedSelection) {
      return nestedUnsupportedSelection;
    }
  }

  return null;
}

function bulkQueryUserError(message: string, field: string[] | null = ['query']): BulkOperationUserError {
  return { field, message };
}

function validateBulkRunQueryDocument(query: string): BulkQueryValidationResult {
  let operation: OperationDefinitionNode;
  try {
    operation = getOperationDefinition(query);
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Invalid GraphQL document';
    return { ok: false, error: bulkQueryUserError(`Invalid bulk query: ${message}`) };
  }

  if (operation.operation !== 'query') {
    return { ok: false, error: bulkQueryUserError('Bulk operations require a query document.') };
  }

  const rootFields = selectedFields(operation.selectionSet.selections);
  if (rootFields.length !== 1) {
    return { ok: false, error: bulkQueryUserError('Bulk queries must contain exactly one top-level field.') };
  }

  const [rootField] = rootFields;
  if (!rootField) {
    return { ok: false, error: bulkQueryUserError('Bulk queries must contain at least one connection.') };
  }

  if (rootField.name.value === 'node' || rootField.name.value === 'nodes') {
    return { ok: false, error: bulkQueryUserError("Bulk queries can't use the top-level node or nodes fields.") };
  }

  const connectionCount = countConnectionSelections(rootField);
  if (connectionCount === 0) {
    return { ok: false, error: bulkQueryUserError('Bulk queries must contain at least one connection.') };
  }

  if (rootField.name.value !== 'products' && rootField.name.value !== 'productVariants') {
    return {
      ok: false,
      error: bulkQueryUserError(`Bulk query root '${rootField.name.value}' is not supported locally.`),
    };
  }

  if (connectionCount > 5) {
    return { ok: false, error: bulkQueryUserError('Bulk queries cannot contain more than 5 connections.') };
  }

  if (maxNestedConnectionDepth(rootField) > 2) {
    return { ok: false, error: bulkQueryUserError('Bulk queries cannot contain connections more than 2 levels deep.') };
  }

  const rootNodeField = findConnectionNodeField(rootField);
  if (!rootNodeField) {
    return { ok: false, error: bulkQueryUserError('Bulk queries must select connection nodes.') };
  }

  let nestedVariantField: FieldNode | null = null;
  let nestedVariantNodeField: FieldNode | null = null;
  const allowedNestedConnection = selectedFields(rootNodeField.selectionSet?.selections).find((selection) => {
    if (rootField.name.value === 'products' && selection.name.value === 'variants') {
      nestedVariantField = selection;
      nestedVariantNodeField = findConnectionNodeField(selection);
      return true;
    }

    return false;
  });

  if (nestedVariantField && !nestedVariantNodeField) {
    return { ok: false, error: bulkQueryUserError('Bulk queries must select connection nodes.') };
  }

  const unsupportedConnection = findUnsupportedConnectionSelection(rootNodeField, allowedNestedConnection ?? null);
  if (unsupportedConnection) {
    return {
      ok: false,
      error: bulkQueryUserError(
        `Nested connection '${unsupportedConnection.name.value}' is not supported by the local bulk query executor.`,
      ),
    };
  }

  return {
    ok: true,
    rootField,
    rootNodeField,
    nestedVariantField,
    nestedVariantNodeField,
  };
}

function makeLocalBulkResultUrl(operation: BulkOperationRecord): string {
  const operationId = operation.id.split('/').at(-1) ?? operation.id;
  return `https://shopify-draft-proxy.local/__bulk_operations/${operationId}/result.jsonl`;
}

function serializeJsonl(records: Array<Record<string, unknown>>): string {
  if (records.length === 0) {
    return '';
  }

  return `${records.map((record) => JSON.stringify(record)).join('\n')}\n`;
}

function executeProductBulkQuery(
  validation: Extract<BulkQueryValidationResult, { ok: true }>,
  variables: Record<string, unknown>,
): { jsonl: string; objectCount: number; rootObjectCount: number } {
  const records: Array<Record<string, unknown>> = [];

  if (validation.rootField.name.value === 'productVariants') {
    const variants = listProductVariantsForBulkExport(validation.rootField, variables);
    for (const variant of variants) {
      records.push(
        serializeProductVariantBulkSelection(
          variant,
          validation.rootNodeField.selectionSet?.selections ?? [],
          variables,
        ),
      );
    }
    return {
      jsonl: serializeJsonl(records),
      objectCount: records.length,
      rootObjectCount: variants.length,
    };
  }

  const products = listProductsForBulkExport(validation.rootField, variables);
  const productSelections = productNodeSelectionsWithoutConnections(validation.rootNodeField);
  for (const product of products) {
    records.push(serializeProductBulkSelection(product, productSelections, variables));

    if (validation.nestedVariantField && validation.nestedVariantNodeField) {
      const variants = listProductVariantsForProductBulkExport(product.id, validation.nestedVariantField, variables);
      for (const variant of variants) {
        records.push({
          ...serializeProductVariantBulkSelection(
            variant,
            validation.nestedVariantNodeField.selectionSet?.selections ?? [],
            variables,
          ),
          __parentId: product.id,
        });
      }
    }
  }

  return {
    jsonl: serializeJsonl(records),
    objectCount: records.length,
    rootObjectCount: products.length,
  };
}

function stageCompletedBulkQueryOperation(query: string, jsonl: string, objectCount: number, rootObjectCount: number) {
  const createdAt = makeSyntheticTimestamp();
  const completedAt = makeSyntheticTimestamp();
  const operation: BulkOperationRecord = {
    id: makeSyntheticGid('BulkOperation'),
    status: 'COMPLETED',
    type: 'QUERY',
    errorCode: null,
    createdAt,
    completedAt,
    objectCount: String(objectCount),
    rootObjectCount: String(rootObjectCount),
    fileSize: String(Buffer.byteLength(jsonl, 'utf8')),
    url: null,
    partialDataUrl: null,
    query,
  };

  operation.url = makeLocalBulkResultUrl(operation);
  return store.stageBulkOperationResult(operation, jsonl);
}

function terminalCancelError(operation: BulkOperationRecord): BulkOperationUserError {
  return {
    field: null,
    message: `A bulk operation cannot be canceled when it is ${operation.status.toLowerCase()}`,
  };
}

function missingBulkOperationUserError(): BulkOperationUserError {
  return {
    field: ['id'],
    message: 'Bulk operation does not exist',
  };
}

export function handleBulkOperationQuery(document: string, variables: Record<string, unknown>): GraphqlResponseBody {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);

    switch (field.name.value) {
      case 'bulkOperation': {
        const id = typeof args['id'] === 'string' ? args['id'] : null;
        if (!id) {
          return missingRequiredArgumentResponse(field, 'query BulkOperation');
        }
        if (!isBulkOperationGid(id)) {
          return invalidBulkOperationIdResponse(document, field, id);
        }
        const operation = store.getEffectiveBulkOperationById(id);
        data[key] = operation ? serializeBulkOperation(operation, field) : null;
        break;
      }
      case 'bulkOperations': {
        const validationError = validateBulkOperationsWindow(field, variables);
        if (validationError) {
          return validationError;
        }
        data[key] = serializeBulkOperationsConnection(field, variables);
        break;
      }
      case 'currentBulkOperation': {
        const operation = getCurrentBulkOperation(field, variables);
        data[key] = operation ? serializeBulkOperation(operation, field) : null;
        break;
      }
      default:
        data[key] = null;
        break;
    }
  }

  return { data };
}

export function handleBulkOperationMutation(
  document: string,
  variables: Record<string, unknown>,
): BulkOperationMutationResult | null {
  const data: Record<string, unknown> = {};
  const stagedResourceIds: string[] = [];
  let handled = false;

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);

    if (field.name.value !== 'bulkOperationCancel' && field.name.value !== 'bulkOperationRunQuery') {
      data[key] = null;
      continue;
    }

    handled = true;

    if (field.name.value === 'bulkOperationRunQuery') {
      const query = typeof args['query'] === 'string' ? args['query'] : null;
      if (!query) {
        return {
          response: missingRequiredArgumentResponse(field, 'mutation BulkOperationRunQuery', 'query'),
          stagedResourceIds,
          notes: 'Rejected bulkOperationRunQuery locally because the required query argument was missing.',
        };
      }

      if (args['groupObjects'] === true) {
        data[key] = serializeRunQueryPayload(field, null, [
          bulkQueryUserError('groupObjects is not supported by the local bulk query executor.', ['groupObjects']),
        ]);
        continue;
      }

      const validation = validateBulkRunQueryDocument(query);
      if (!validation.ok) {
        data[key] = serializeRunQueryPayload(field, null, [validation.error]);
        continue;
      }

      const result = executeProductBulkQuery(validation, variables);
      const operation = stageCompletedBulkQueryOperation(
        query,
        result.jsonl,
        result.objectCount,
        result.rootObjectCount,
      );
      data[key] = serializeRunQueryPayload(field, operation, []);
      stagedResourceIds.push(operation.id);
      continue;
    }

    const id = typeof args['id'] === 'string' ? args['id'] : null;
    if (!id) {
      return {
        response: missingRequiredArgumentResponse(field, 'mutation BulkOperationCancel'),
        stagedResourceIds,
        notes: 'Rejected bulkOperationCancel locally because the required id argument was missing.',
      };
    }

    if (!isBulkOperationGid(id)) {
      return {
        response: invalidBulkOperationIdResponse(document, field, id),
        stagedResourceIds,
        notes: 'Rejected bulkOperationCancel locally because the id was not a BulkOperation GID.',
      };
    }

    const stagedOperation = store.getStagedBulkOperationById(id);
    const effectiveOperation = stagedOperation ?? store.getEffectiveBulkOperationById(id);

    if (!effectiveOperation) {
      data[key] = serializeCancelPayload(field, null, [missingBulkOperationUserError()]);
      continue;
    }

    if (BULK_OPERATION_TERMINAL_STATUSES.has(effectiveOperation.status)) {
      data[key] = serializeCancelPayload(field, effectiveOperation, [terminalCancelError(effectiveOperation)]);
      stagedResourceIds.push(effectiveOperation.id);
      continue;
    }

    if (!stagedOperation) {
      data[key] = serializeCancelPayload(field, null, [missingBulkOperationUserError()]);
      continue;
    }

    const canceledOperation = store.cancelStagedBulkOperation(id) ?? stagedOperation;
    data[key] = serializeCancelPayload(field, canceledOperation, []);
    stagedResourceIds.push(canceledOperation.id);
  }

  if (!handled) {
    return null;
  }

  return {
    response: { data },
    stagedResourceIds,
    notes: 'Handled BulkOperation mutation locally against the in-memory BulkOperation job store.',
  };
}
