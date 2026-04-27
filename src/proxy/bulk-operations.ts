import type { FieldNode } from 'graphql';

import type { ReadMode } from '../config.js';
import { parseOperation } from '../graphql/parse-operation.js';
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
import { isProxySyntheticGid, makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { BulkOperationRecord } from '../state/types.js';
import { getOperationCapability } from './capabilities.js';
import {
  getFieldResponseKey,
  getNodeLocation,
  getSelectedChildFields,
  getVariableDefinitionLocation,
  paginateConnectionItems,
  serializeConnection,
} from './graphql-helpers.js';
import { handleProductMutation } from './products.js';

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
  innerMutationLogs?: BulkOperationImportLogEntry[];
};

type BulkOperationMutationOptions = {
  readMode: ReadMode;
};

export type BulkOperationImportLogEntry = {
  operationName: string | null;
  rootField: string;
  query: string;
  variables: Record<string, unknown>;
  requestBody: Record<string, unknown>;
  stagedResourceIds: string[];
  bulkOperationId: string;
  lineNumber: number;
  stagedUploadPath: string;
  innerMutation: string;
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

function readStringArgument(args: Record<string, unknown>, name: string): string | null {
  const value = args[name];
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function buildBulkOperationResultUrl(operationId: string): string {
  return `https://shopify-draft-proxy.local/__meta/bulk-operations/${encodeURIComponent(operationId)}/result.jsonl`;
}

function buildMutationImportOperation(
  status: BulkOperationRecord['status'],
  mutation: string,
  resultJsonl: string,
  counts: { objectCount: number; rootObjectCount: number },
): BulkOperationRecord {
  const completedAt = makeSyntheticTimestamp();
  const id = makeSyntheticGid('BulkOperation');
  return {
    id,
    status,
    type: 'MUTATION',
    errorCode: null,
    createdAt: completedAt,
    completedAt,
    objectCount: String(counts.objectCount),
    rootObjectCount: String(counts.rootObjectCount),
    fileSize: String(Buffer.byteLength(resultJsonl, 'utf8')),
    url: buildBulkOperationResultUrl(id),
    partialDataUrl: null,
    query: mutation,
    resultJsonl,
  };
}

function withStableBulkOperationUrl(operation: BulkOperationRecord): BulkOperationRecord {
  return {
    ...operation,
    url: buildBulkOperationResultUrl(operation.id),
  };
}

function serializeRunMutationPayload(
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

function parseJsonlVariables(uploadContent: string): Array<
  | { lineNumber: number; variables: Record<string, unknown> }
  | {
      lineNumber: number;
      error: string;
    }
> {
  const lines = uploadContent.split(/\r?\n/u);
  const parsedLines: Array<
    { lineNumber: number; variables: Record<string, unknown> } | { lineNumber: number; error: string }
  > = [];

  for (const [index, line] of lines.entries()) {
    if (line.trim().length === 0) {
      continue;
    }

    try {
      const parsed = JSON.parse(line) as unknown;
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
        parsedLines.push({ lineNumber: index + 1, error: 'Bulk mutation variables line must be a JSON object.' });
        continue;
      }
      parsedLines.push({ lineNumber: index + 1, variables: parsed as Record<string, unknown> });
    } catch (error) {
      parsedLines.push({
        lineNumber: index + 1,
        error: error instanceof Error ? error.message : 'Invalid JSONL variables line.',
      });
    }
  }

  return parsedLines;
}

function collectResponseGids(value: unknown, gids = new Set<string>()): string[] {
  if (typeof value === 'string') {
    if (value.startsWith('gid://shopify/')) {
      gids.add(value);
    }
    return [...gids];
  }

  if (!value || typeof value !== 'object') {
    return [...gids];
  }

  if (Array.isArray(value)) {
    for (const item of value) {
      collectResponseGids(item, gids);
    }
    return [...gids];
  }

  for (const item of Object.values(value)) {
    collectResponseGids(item, gids);
  }
  return [...gids];
}

function hasMutationUserErrors(responseBody: unknown, rootField: string): boolean {
  if (!responseBody || typeof responseBody !== 'object') {
    return false;
  }
  const data = (responseBody as Record<string, unknown>)['data'];
  if (!data || typeof data !== 'object') {
    return false;
  }
  const payload = (data as Record<string, unknown>)[rootField];
  if (!payload || typeof payload !== 'object') {
    return false;
  }
  const userErrors = (payload as Record<string, unknown>)['userErrors'];
  return Array.isArray(userErrors) && userErrors.length > 0;
}

function isSupportedBulkImportInnerMutation(
  mutation: string,
): { operationName: string | null; rootField: string } | null {
  let parsed: ReturnType<typeof parseOperation>;
  try {
    parsed = parseOperation(mutation);
  } catch {
    return null;
  }
  if (parsed.type !== 'mutation' || parsed.rootFields.length !== 1) {
    return null;
  }

  const capability = getOperationCapability(parsed);
  if (capability.domain !== 'products' || capability.execution !== 'stage-locally') {
    return null;
  }

  return {
    operationName: capability.operationName,
    rootField: parsed.rootFields[0]!,
  };
}

function makeJsonl(rows: Array<Record<string, unknown>>): string {
  return rows.map((row) => JSON.stringify(row)).join('\n') + (rows.length > 0 ? '\n' : '');
}

function handleBulkOperationRunMutation(
  field: FieldNode,
  args: Record<string, unknown>,
  options: BulkOperationMutationOptions,
): BulkOperationMutationResult {
  const key = getFieldResponseKey(field);
  const mutation = readStringArgument(args, 'mutation');
  const stagedUploadPath = readStringArgument(args, 'stagedUploadPath');
  const emptyResult = makeJsonl([]);

  if (!mutation) {
    return {
      response: missingRequiredArgumentResponse(field, 'mutation BulkOperationRunMutation', 'mutation'),
      stagedResourceIds: [],
      notes: 'Rejected bulkOperationRunMutation locally because the required mutation argument was missing.',
    };
  }

  if (!stagedUploadPath) {
    return {
      response: missingRequiredArgumentResponse(field, 'mutation BulkOperationRunMutation', 'stagedUploadPath'),
      stagedResourceIds: [],
      notes: 'Rejected bulkOperationRunMutation locally because the required stagedUploadPath argument was missing.',
    };
  }

  const uploadContent = store.getStagedUploadContent(stagedUploadPath);
  if (uploadContent === null) {
    const failedOperation = store.stageBulkOperation(
      withStableBulkOperationUrl(
        buildMutationImportOperation('FAILED', mutation, emptyResult, { objectCount: 0, rootObjectCount: 0 }),
      ),
    );
    return {
      response: {
        data: {
          [key]: serializeRunMutationPayload(field, failedOperation, [
            {
              field: ['stagedUploadPath'],
              message: 'Staged upload content was not found for the provided stagedUploadPath.',
            },
          ]),
        },
      },
      stagedResourceIds: [failedOperation.id],
      notes: 'Rejected bulkOperationRunMutation locally because the staged upload content was missing.',
    };
  }

  const innerMutation = isSupportedBulkImportInnerMutation(mutation);
  if (!innerMutation) {
    const resultJsonl = makeJsonl([
      {
        line: null,
        errors: [
          {
            message:
              'bulkOperationRunMutation locally supports only single-root product mutations that are already staged by the proxy.',
          },
        ],
      },
    ]);
    const failedOperation = store.stageBulkOperation(
      withStableBulkOperationUrl(
        buildMutationImportOperation('FAILED', mutation, resultJsonl, { objectCount: 0, rootObjectCount: 0 }),
      ),
    );
    return {
      response: {
        data: {
          [key]: serializeRunMutationPayload(field, failedOperation, [
            {
              field: ['mutation'],
              message:
                'Unsupported bulk mutation import root. The proxy did not send this bulk import upstream at runtime.',
            },
          ]),
        },
      },
      stagedResourceIds: [failedOperation.id],
      notes:
        'Rejected bulkOperationRunMutation locally because the inner mutation root is not supported for local bulk imports.',
    };
  }

  const parsedLines = parseJsonlVariables(uploadContent);
  const rows: Array<Record<string, unknown>> = [];
  const innerMutationLogs: BulkOperationImportLogEntry[] = [];
  let objectCount = 0;
  let hasFatalLineError = false;

  for (const parsedLine of parsedLines) {
    if ('error' in parsedLine) {
      hasFatalLineError = true;
      rows.push({
        line: parsedLine.lineNumber,
        errors: [{ message: parsedLine.error }],
      });
      continue;
    }

    const responseBody = handleProductMutation(mutation, parsedLine.variables, options.readMode);
    const stagedResourceIds = collectResponseGids(responseBody).filter((id) => {
      return isProxySyntheticGid(id) || store.getEffectiveProductById(id) !== null;
    });
    const hadUserErrors = hasMutationUserErrors(responseBody, innerMutation.rootField);

    if (!hadUserErrors) {
      objectCount += 1;
    }

    rows.push({
      line: parsedLine.lineNumber,
      response: responseBody,
    });
    innerMutationLogs.push({
      operationName: innerMutation.operationName,
      rootField: innerMutation.rootField,
      query: mutation,
      variables: parsedLine.variables,
      requestBody: {
        query: mutation,
        variables: parsedLine.variables,
      },
      stagedResourceIds,
      bulkOperationId: '',
      lineNumber: parsedLine.lineNumber,
      stagedUploadPath,
      innerMutation: mutation,
    });
  }

  const resultJsonl = makeJsonl(rows);
  const operation = store.stageBulkOperation(
    withStableBulkOperationUrl(
      buildMutationImportOperation(hasFatalLineError ? 'FAILED' : 'COMPLETED', mutation, resultJsonl, {
        objectCount,
        rootObjectCount: objectCount,
      }),
    ),
  );

  return {
    response: {
      data: {
        [key]: serializeRunMutationPayload(field, operation, []),
      },
    },
    stagedResourceIds: [operation.id],
    innerMutationLogs: innerMutationLogs.map((entry) => ({ ...entry, bulkOperationId: operation.id })),
    notes: hasFatalLineError
      ? 'Handled bulkOperationRunMutation locally, but one or more JSONL variable lines failed before mutation staging.'
      : 'Handled bulkOperationRunMutation locally by replaying supported product inner mutation lines into the in-memory draft store.',
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
  options: BulkOperationMutationOptions,
): BulkOperationMutationResult | null {
  const data: Record<string, unknown> = {};
  const stagedResourceIds: string[] = [];
  let handled = false;

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);

    if (field.name.value === 'bulkOperationRunMutation') {
      handled = true;
      return handleBulkOperationRunMutation(field, args, options);
    }

    if (field.name.value !== 'bulkOperationCancel') {
      data[key] = null;
      continue;
    }

    handled = true;
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
    notes: 'Handled bulkOperationCancel locally against the in-memory BulkOperation job store.',
  };
}
