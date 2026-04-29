import type { ProxyRuntimeContext } from './runtime-context.js';
import { Kind, parse, type FieldNode, type OperationDefinitionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import {
  matchesSearchQueryText,
  normalizeSearchQueryValue,
  parseSearchQueryTerms,
  type SearchQueryTerm,
} from '../search-query-parser.js';
import { compareShopifyResourceIds } from '../shopify/resource-ids.js';
import type { WebhookSubscriptionRecord } from '../state/types.js';
import {
  defaultGraphqlTypeConditionApplies,
  getDocumentFragments,
  getFieldResponseKey,
  isPlainObject,
  paginateConnectionItems,
  projectGraphqlValue,
  readGraphqlDataResponsePayload,
  serializeConnection,
  type FragmentMap,
} from './graphql-helpers.js';

const webhookProjectionOptions = {
  shouldApplyTypeCondition: (source: Record<string, unknown>, typeCondition: string | undefined): boolean =>
    defaultGraphqlTypeConditionApplies(source, typeCondition) || typeCondition === 'WebhookSubscription',
  projectFieldValue: ({
    source,
    fieldName,
  }: {
    source: Record<string, unknown>;
    fieldName: string;
  }): { handled: true; value: unknown } | { handled: false } => {
    if (fieldName !== 'uri') {
      return { handled: false };
    }

    return { handled: true, value: webhookSubscriptionUri(source as WebhookSubscriptionRecord) };
  },
};

type WebhookSubscriptionUserError = {
  field: string[];
  message: string;
};

export type WebhookSubscriptionMutationResult = {
  response: Record<string, unknown>;
  staged: boolean;
  stagedResourceIds: string[];
  notes: string;
};

function normalizeStringArray(raw: unknown): string[] {
  return Array.isArray(raw) ? raw.filter((value): value is string => typeof value === 'string') : [];
}

function webhookSubscriptionUserError(field: string[], message: string): WebhookSubscriptionUserError {
  return { field, message };
}

function getOperationPathLabel(document: string): string {
  const operation = parse(document).definitions.find(
    (definition): definition is OperationDefinitionNode => definition.kind === Kind.OPERATION_DEFINITION,
  );
  const operationType = operation?.operation ?? 'mutation';
  return operation?.name ? `${operationType} ${operation.name.value}` : operationType;
}

function getFieldLocation(field: FieldNode): Array<{ line: number; column: number }> | undefined {
  return field.loc ? [{ line: field.loc.startToken.line, column: field.loc.startToken.column }] : undefined;
}

function buildMissingRequiredArgumentError(
  operationName: string,
  argumentName: string,
  field?: FieldNode,
  operationPath?: string,
): Record<string, unknown> {
  const locations = field ? getFieldLocation(field) : undefined;
  return {
    message: `Field '${operationName}' is missing required arguments: ${argumentName}`,
    ...(locations ? { locations } : {}),
    path: [operationPath ?? 'mutation', operationName],
    extensions: {
      code: 'missingRequiredArguments',
      className: 'Field',
      name: operationName,
      arguments: argumentName,
    },
  };
}

function buildNullArgumentError(
  operationName: string,
  argumentName: string,
  expectedType: string,
  field?: FieldNode,
  operationPath?: string,
): Record<string, unknown> {
  const locations = field ? getFieldLocation(field) : undefined;
  return {
    message: `Argument '${argumentName}' on Field '${operationName}' has an invalid value (null). Expected type '${expectedType}'.`,
    ...(locations ? { locations } : {}),
    path: [operationPath ?? 'mutation', operationName, argumentName],
    extensions: {
      code: 'argumentLiteralsIncompatible',
      typeName: 'Field',
      argumentName,
    },
  };
}

function buildMissingVariableError(variableName: string, variableType: string): Record<string, unknown> {
  return {
    message: `Variable $${variableName} of type ${variableType} was provided invalid value`,
    extensions: {
      code: 'INVALID_VARIABLE',
      value: null,
      problems: [{ path: [], explanation: 'Expected value to not be null' }],
    },
  };
}

type RequiredArgumentSpec = {
  name: string;
  expectedType: string;
};

function validateRequiredFieldArguments(
  field: FieldNode,
  variables: Record<string, unknown>,
  operationName: string,
  requiredArguments: RequiredArgumentSpec[],
  operationPath: string,
): Record<string, unknown>[] {
  const missingArgumentNames: string[] = [];
  const errors: Record<string, unknown>[] = [];

  for (const requiredArgument of requiredArguments) {
    const argument = field.arguments?.find((candidate) => candidate.name.value === requiredArgument.name) ?? null;
    if (!argument) {
      missingArgumentNames.push(requiredArgument.name);
      continue;
    }

    if (argument.value.kind === Kind.NULL) {
      errors.push(
        buildNullArgumentError(
          operationName,
          requiredArgument.name,
          requiredArgument.expectedType,
          field,
          operationPath,
        ),
      );
      continue;
    }

    if (argument.value.kind === Kind.VARIABLE) {
      const variableName = argument.value.name.value;
      const value = variables[variableName];
      if (value === null || value === undefined) {
        errors.push(buildMissingVariableError(variableName, requiredArgument.expectedType));
      }
    }
  }

  if (missingArgumentNames.length > 0) {
    errors.unshift(
      buildMissingRequiredArgumentError(operationName, missingArgumentNames.join(', '), field, operationPath),
    );
  }

  return errors;
}

function readWebhookSubscriptionInput(args: Record<string, unknown>): Record<string, unknown> | null {
  const input = args['webhookSubscription'];
  return isPlainObject(input) ? input : null;
}

function readOptionalString(input: Record<string, unknown>, fieldName: string): string | null | undefined {
  if (!Object.prototype.hasOwnProperty.call(input, fieldName)) {
    return undefined;
  }

  const value = input[fieldName];
  return typeof value === 'string' || value === null ? value : undefined;
}

function readOptionalStringArray(input: Record<string, unknown>, fieldName: string): string[] | undefined {
  if (!Object.prototype.hasOwnProperty.call(input, fieldName)) {
    return undefined;
  }

  return normalizeStringArray(input[fieldName]);
}

function endpointFromUri(uri: string): WebhookSubscriptionRecord['endpoint'] {
  if (uri.startsWith('pubsub://')) {
    const pubSubUri = uri.slice('pubsub://'.length);
    const separatorIndex = pubSubUri.indexOf(':');
    return {
      __typename: 'WebhookPubSubEndpoint',
      pubSubProject: separatorIndex >= 0 ? pubSubUri.slice(0, separatorIndex) : pubSubUri,
      pubSubTopic: separatorIndex >= 0 ? pubSubUri.slice(separatorIndex + 1) : '',
    };
  }

  if (uri.startsWith('arn:aws:events:')) {
    return {
      __typename: 'WebhookEventBridgeEndpoint',
      arn: uri,
    };
  }

  return {
    __typename: 'WebhookHttpEndpoint',
    callbackUrl: uri,
  };
}

function uriFromEndpoint(endpoint: WebhookSubscriptionRecord['endpoint']): string | null {
  if (!endpoint) {
    return null;
  }

  switch (endpoint.__typename) {
    case 'WebhookHttpEndpoint':
      return endpoint.callbackUrl ?? null;
    case 'WebhookEventBridgeEndpoint':
      return endpoint.arn ?? null;
    case 'WebhookPubSubEndpoint':
      return endpoint.pubSubProject && endpoint.pubSubTopic
        ? `pubsub://${endpoint.pubSubProject}:${endpoint.pubSubTopic}`
        : null;
    default:
      return null;
  }
}

function webhookSubscriptionUri(webhookSubscription: WebhookSubscriptionRecord): string | null {
  return webhookSubscription.uri ?? uriFromEndpoint(webhookSubscription.endpoint);
}

function normalizeUri(input: Record<string, unknown> | null): string | null {
  if (!input) {
    return null;
  }

  const uri = input['uri'];
  return typeof uri === 'string' && uri.trim().length > 0 ? uri.trim() : null;
}

function projectMutationPayload(payload: Record<string, unknown>, field: FieldNode, fragments: FragmentMap): unknown {
  return field.selectionSet
    ? projectGraphqlValue(payload, field.selectionSet.selections, fragments, webhookProjectionOptions)
    : payload;
}

function buildWebhookSubscriptionFromCreateInput(
  runtime: ProxyRuntimeContext,
  topic: unknown,
  input: Record<string, unknown>,
): WebhookSubscriptionRecord {
  const timestamp = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const uri = normalizeUri(input) as string;
  return {
    id: runtime.syntheticIdentity.makeProxySyntheticGid('WebhookSubscription'),
    topic: typeof topic === 'string' ? topic : null,
    uri,
    name: readOptionalString(input, 'name') ?? null,
    format: readOptionalString(input, 'format') ?? 'JSON',
    includeFields: readOptionalStringArray(input, 'includeFields') ?? [],
    metafieldNamespaces: readOptionalStringArray(input, 'metafieldNamespaces') ?? [],
    filter: readOptionalString(input, 'filter') ?? '',
    createdAt: timestamp,
    updatedAt: timestamp,
    endpoint: endpointFromUri(uri),
  };
}

function applyWebhookSubscriptionUpdateInput(
  runtime: ProxyRuntimeContext,
  existing: WebhookSubscriptionRecord,
  input: Record<string, unknown>,
): WebhookSubscriptionRecord {
  const uri = normalizeUri(input);
  return {
    ...existing,
    uri: uri ?? existing.uri ?? uriFromEndpoint(existing.endpoint),
    name: readOptionalString(input, 'name') ?? existing.name,
    format: readOptionalString(input, 'format') ?? existing.format,
    includeFields: readOptionalStringArray(input, 'includeFields') ?? existing.includeFields,
    metafieldNamespaces: readOptionalStringArray(input, 'metafieldNamespaces') ?? existing.metafieldNamespaces,
    filter: readOptionalString(input, 'filter') ?? existing.filter,
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    endpoint: uri ? endpointFromUri(uri) : existing.endpoint,
  };
}

function handleWebhookSubscriptionCreate(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
  operationPath: string,
): { payload: unknown; stagedResourceIds: string[]; errors: Record<string, unknown>[] } {
  const validationErrors = validateRequiredFieldArguments(
    field,
    variables,
    'webhookSubscriptionCreate',
    [
      { name: 'topic', expectedType: 'WebhookSubscriptionTopic!' },
      { name: 'webhookSubscription', expectedType: 'WebhookSubscriptionInput!' },
    ],
    operationPath,
  );
  if (validationErrors.length > 0) {
    return {
      payload: null,
      stagedResourceIds: [],
      errors: validationErrors,
    };
  }

  const args = getFieldArguments(field, variables);
  const input = readWebhookSubscriptionInput(args);
  const errors: WebhookSubscriptionUserError[] = [];

  if (!normalizeUri(input)) {
    errors.push(webhookSubscriptionUserError(['webhookSubscription', 'callbackUrl'], "Address can't be blank"));
  }

  const webhookSubscription =
    errors.length === 0 && input ? buildWebhookSubscriptionFromCreateInput(runtime, args['topic'], input) : null;

  if (webhookSubscription) {
    runtime.store.upsertStagedWebhookSubscription(webhookSubscription);
  }

  return {
    payload: projectMutationPayload(
      {
        webhookSubscription,
        userErrors: errors,
      },
      field,
      fragments,
    ),
    stagedResourceIds: webhookSubscription ? [webhookSubscription.id] : [],
    errors: [],
  };
}

function handleWebhookSubscriptionUpdate(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
  operationPath: string,
): { payload: unknown; stagedResourceIds: string[]; errors: Record<string, unknown>[] } {
  const validationErrors = validateRequiredFieldArguments(
    field,
    variables,
    'webhookSubscriptionUpdate',
    [
      { name: 'id', expectedType: 'ID!' },
      { name: 'webhookSubscription', expectedType: 'WebhookSubscriptionInput!' },
    ],
    operationPath,
  );
  if (validationErrors.length > 0) {
    return {
      payload: null,
      stagedResourceIds: [],
      errors: validationErrors,
    };
  }

  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const input = readWebhookSubscriptionInput(args);
  const existing = id ? runtime.store.getEffectiveWebhookSubscriptionById(id) : null;
  const errors: WebhookSubscriptionUserError[] = [];

  if (!id || !existing) {
    errors.push(webhookSubscriptionUserError(['id'], 'Webhook subscription does not exist'));
  }

  const webhookSubscription =
    errors.length === 0 && existing && input ? applyWebhookSubscriptionUpdateInput(runtime, existing, input) : null;

  if (webhookSubscription) {
    runtime.store.upsertStagedWebhookSubscription(webhookSubscription);
  }

  return {
    payload: projectMutationPayload(
      {
        webhookSubscription,
        userErrors: errors,
      },
      field,
      fragments,
    ),
    stagedResourceIds: webhookSubscription ? [webhookSubscription.id] : [],
    errors: [],
  };
}

function validateWebhookSubscriptionDeleteId(
  field: FieldNode,
  variables: Record<string, unknown>,
  operationPath: string,
): { id: string | null; errors: Record<string, unknown>[] } {
  const idArgument = field.arguments?.find((argument) => argument.name.value === 'id') ?? null;
  if (!idArgument) {
    return {
      id: null,
      errors: [buildMissingRequiredArgumentError('webhookSubscriptionDelete', 'id', field, operationPath)],
    };
  }

  if (idArgument.value.kind === Kind.NULL) {
    return {
      id: null,
      errors: [buildNullArgumentError('webhookSubscriptionDelete', 'id', 'ID!', field, operationPath)],
    };
  }

  if (idArgument.value.kind === Kind.VARIABLE) {
    const variableName = idArgument.value.name.value;
    const id = variables[variableName];
    if (id === null || id === undefined) {
      return {
        id: null,
        errors: [buildMissingVariableError(variableName, 'ID!')],
      };
    }

    return {
      id: typeof id === 'string' ? id : null,
      errors: [],
    };
  }

  const args = getFieldArguments(field, variables);
  return {
    id: typeof args['id'] === 'string' ? args['id'] : null,
    errors: [],
  };
}

function handleWebhookSubscriptionDelete(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
  operationPath: string,
): { payload: unknown; stagedResourceIds: string[]; errors: Record<string, unknown>[] } {
  const validatedId = validateWebhookSubscriptionDeleteId(field, variables, operationPath);
  if (validatedId.errors.length > 0) {
    return {
      payload: null,
      stagedResourceIds: [],
      errors: validatedId.errors,
    };
  }

  const existing = validatedId.id ? runtime.store.getEffectiveWebhookSubscriptionById(validatedId.id) : null;
  const userErrors: WebhookSubscriptionUserError[] = [];
  if (!validatedId.id || !existing) {
    userErrors.push(webhookSubscriptionUserError(['id'], 'Webhook subscription does not exist'));
  }

  const deletedWebhookSubscriptionId = userErrors.length === 0 ? validatedId.id : null;
  if (deletedWebhookSubscriptionId) {
    runtime.store.deleteStagedWebhookSubscription(deletedWebhookSubscriptionId);
  }

  return {
    payload: projectMutationPayload(
      {
        deletedWebhookSubscriptionId,
        userErrors,
      },
      field,
      fragments,
    ),
    stagedResourceIds: deletedWebhookSubscriptionId ? [deletedWebhookSubscriptionId] : [],
    errors: [],
  };
}

function normalizeEndpoint(raw: unknown): WebhookSubscriptionRecord['endpoint'] {
  if (!isPlainObject(raw)) {
    return null;
  }

  const typename = raw['__typename'];
  if (
    typename !== 'WebhookHttpEndpoint' &&
    typename !== 'WebhookEventBridgeEndpoint' &&
    typename !== 'WebhookPubSubEndpoint'
  ) {
    return null;
  }

  return {
    __typename: typename,
    ...(typeof raw['callbackUrl'] === 'string' || raw['callbackUrl'] === null
      ? { callbackUrl: raw['callbackUrl'] }
      : {}),
    ...(typeof raw['arn'] === 'string' || raw['arn'] === null ? { arn: raw['arn'] } : {}),
    ...(typeof raw['pubSubProject'] === 'string' || raw['pubSubProject'] === null
      ? { pubSubProject: raw['pubSubProject'] }
      : {}),
    ...(typeof raw['pubSubTopic'] === 'string' || raw['pubSubTopic'] === null
      ? { pubSubTopic: raw['pubSubTopic'] }
      : {}),
  };
}

function normalizeWebhookSubscription(raw: unknown): WebhookSubscriptionRecord | null {
  if (!isPlainObject(raw)) {
    return null;
  }

  const id = raw['id'];
  if (typeof id !== 'string' || !id.startsWith('gid://shopify/WebhookSubscription/')) {
    return null;
  }

  const endpoint = normalizeEndpoint(raw['endpoint']);
  return {
    id,
    topic: typeof raw['topic'] === 'string' ? raw['topic'] : null,
    uri: typeof raw['uri'] === 'string' ? raw['uri'] : uriFromEndpoint(endpoint),
    name: typeof raw['name'] === 'string' || raw['name'] === null ? raw['name'] : null,
    format: typeof raw['format'] === 'string' ? raw['format'] : null,
    includeFields: normalizeStringArray(raw['includeFields']),
    metafieldNamespaces: normalizeStringArray(raw['metafieldNamespaces']),
    filter: typeof raw['filter'] === 'string' || raw['filter'] === null ? raw['filter'] : null,
    createdAt: typeof raw['createdAt'] === 'string' || raw['createdAt'] === null ? raw['createdAt'] : null,
    updatedAt: typeof raw['updatedAt'] === 'string' || raw['updatedAt'] === null ? raw['updatedAt'] : null,
    endpoint,
  };
}

function collectWebhookSubscriptions(
  value: unknown,
  records: WebhookSubscriptionRecord[] = [],
): WebhookSubscriptionRecord[] {
  if (Array.isArray(value)) {
    for (const item of value) {
      collectWebhookSubscriptions(item, records);
    }
    return records;
  }

  const webhookSubscription = normalizeWebhookSubscription(value);
  if (webhookSubscription) {
    records.push(webhookSubscription);
  }

  if (!isPlainObject(value)) {
    return records;
  }

  for (const child of Object.values(value)) {
    collectWebhookSubscriptions(child, records);
  }

  return records;
}

export function hydrateWebhookSubscriptionsFromUpstreamResponse(
  runtime: ProxyRuntimeContext,
  document: string,
  _variables: Record<string, unknown>,
  upstreamPayload: unknown,
): void {
  if (!isPlainObject(upstreamPayload) || !isPlainObject(upstreamPayload['data'])) {
    return;
  }

  const records: WebhookSubscriptionRecord[] = [];
  for (const field of getRootFields(document)) {
    collectWebhookSubscriptions(readGraphqlDataResponsePayload(upstreamPayload, getFieldResponseKey(field)), records);
  }

  if (records.length > 0) {
    runtime.store.upsertBaseWebhookSubscriptions(records);
  }
}

function webhookSubscriptionLegacyId(webhookSubscription: WebhookSubscriptionRecord): string {
  return webhookSubscription.id.split('/').at(-1) ?? webhookSubscription.id;
}

function matchesWebhookTerm(webhookSubscription: WebhookSubscriptionRecord, term: SearchQueryTerm): boolean {
  const field = term.field?.toLowerCase() ?? null;
  const uri = webhookSubscriptionUri(webhookSubscription);
  let matches = false;

  switch (field) {
    case null:
      matches =
        matchesSearchQueryText(webhookSubscription.id, term) ||
        matchesSearchQueryText(webhookSubscription.topic, term) ||
        matchesSearchQueryText(webhookSubscription.format, term);
      break;
    case 'id': {
      const expected = normalizeSearchQueryValue(term.value);
      matches =
        normalizeSearchQueryValue(webhookSubscription.id) === expected ||
        normalizeSearchQueryValue(webhookSubscriptionLegacyId(webhookSubscription)) === expected;
      break;
    }
    case 'topic':
      matches = matchesSearchQueryText(webhookSubscription.topic, term);
      break;
    case 'format':
      matches = matchesSearchQueryText(webhookSubscription.format, term);
      break;
    case 'uri':
    case 'callbackurl':
    case 'callback_url':
    case 'endpoint':
      matches = matchesSearchQueryText(uri, term);
      break;
    case 'created_at':
    case 'createdat':
      matches = matchesSearchQueryText(webhookSubscription.createdAt, term);
      break;
    case 'updated_at':
    case 'updatedat':
      matches = matchesSearchQueryText(webhookSubscription.updatedAt, term);
      break;
    default:
      matches = false;
      break;
  }

  return term.negated ? !matches : matches;
}

function filterWebhookSubscriptionsByFieldArguments(
  webhookSubscriptions: WebhookSubscriptionRecord[],
  args: Record<string, unknown>,
): WebhookSubscriptionRecord[] {
  const format = typeof args['format'] === 'string' ? args['format'] : null;
  const uri =
    typeof args['uri'] === 'string'
      ? args['uri']
      : typeof args['callbackUrl'] === 'string'
        ? args['callbackUrl']
        : null;
  const topics = Array.isArray(args['topics'])
    ? args['topics'].filter((topic): topic is string => typeof topic === 'string')
    : [];

  return webhookSubscriptions
    .filter((webhookSubscription) => !format || webhookSubscription.format === format)
    .filter((webhookSubscription) => !uri || webhookSubscriptionUri(webhookSubscription) === uri)
    .filter(
      (webhookSubscription) =>
        topics.length === 0 || (webhookSubscription.topic !== null && topics.includes(webhookSubscription.topic)),
    );
}

function filterWebhookSubscriptionsByQuery(
  webhookSubscriptions: WebhookSubscriptionRecord[],
  rawQuery: unknown,
): WebhookSubscriptionRecord[] {
  if (typeof rawQuery !== 'string' || rawQuery.trim().length === 0) {
    return webhookSubscriptions;
  }

  const terms = parseSearchQueryTerms(rawQuery.trim(), { ignoredKeywords: ['AND'] });
  if (terms.length === 0) {
    return webhookSubscriptions;
  }

  return webhookSubscriptions.filter((webhookSubscription) =>
    terms.every((term) => matchesWebhookTerm(webhookSubscription, term)),
  );
}

function sortWebhookSubscriptionsForConnection(
  webhookSubscriptions: WebhookSubscriptionRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
): WebhookSubscriptionRecord[] {
  const args = getFieldArguments(field, variables);
  const sortKey = typeof args['sortKey'] === 'string' ? args['sortKey'] : 'ID';
  const reverse = args['reverse'] === true;

  const sorted = [...webhookSubscriptions].sort((left, right) => {
    switch (sortKey) {
      case 'CREATED_AT':
        return (
          (left.createdAt ?? '').localeCompare(right.createdAt ?? '') || compareShopifyResourceIds(left.id, right.id)
        );
      case 'UPDATED_AT':
        return (
          (left.updatedAt ?? '').localeCompare(right.updatedAt ?? '') || compareShopifyResourceIds(left.id, right.id)
        );
      case 'TOPIC':
        return (left.topic ?? '').localeCompare(right.topic ?? '') || compareShopifyResourceIds(left.id, right.id);
      case 'ID':
      default:
        return compareShopifyResourceIds(left.id, right.id);
    }
  });

  return reverse ? sorted.reverse() : sorted;
}

function serializeWebhookSubscriptionNode(
  selection: FieldNode,
  webhookSubscription: WebhookSubscriptionRecord,
  fragments: FragmentMap,
): unknown {
  return selection.selectionSet
    ? projectGraphqlValue(webhookSubscription, selection.selectionSet.selections, fragments, webhookProjectionOptions)
    : webhookSubscription.id;
}

function serializeWebhookSubscriptionsConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const filteredWebhookSubscriptions = filterWebhookSubscriptionsByQuery(
    filterWebhookSubscriptionsByFieldArguments(runtime.store.listEffectiveWebhookSubscriptions(), args),
    args['query'],
  );
  const sortedWebhookSubscriptions = sortWebhookSubscriptionsForConnection(
    filteredWebhookSubscriptions,
    field,
    variables,
  );
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    sortedWebhookSubscriptions,
    field,
    variables,
    (webhookSubscription) => webhookSubscription.id,
  );

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (webhookSubscription) => webhookSubscription.id,
    serializeNode: (webhookSubscription, selection) =>
      serializeWebhookSubscriptionNode(selection, webhookSubscription, fragments),
    selectedFieldOptions: { includeInlineFragments: true },
    pageInfoOptions: { includeInlineFragments: true },
  });
}

function serializeWebhookSubscriptionsCount(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const filteredWebhookSubscriptions = filterWebhookSubscriptionsByQuery(
    runtime.store.listEffectiveWebhookSubscriptions(),
    args['query'],
  );
  const rawLimit = args['limit'];
  const limit =
    typeof rawLimit === 'number' && Number.isFinite(rawLimit) && rawLimit >= 0 ? Math.floor(rawLimit) : null;
  const count =
    limit === null ? filteredWebhookSubscriptions.length : Math.min(filteredWebhookSubscriptions.length, limit);
  const precision = limit !== null && filteredWebhookSubscriptions.length > limit ? 'AT_LEAST' : 'EXACT';
  const result: Record<string, unknown> = {};

  for (const selection of (field.selectionSet?.selections ?? []).filter(
    (candidate): candidate is FieldNode => candidate.kind === Kind.FIELD,
  )) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'count':
        result[key] = count;
        break;
      case 'precision':
        result[key] = precision;
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function rootPayloadForField(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const args = getFieldArguments(field, variables);
  switch (field.name.value) {
    case 'webhookSubscription': {
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      const webhookSubscription = id ? runtime.store.getEffectiveWebhookSubscriptionById(id) : null;
      return webhookSubscription && field.selectionSet
        ? projectGraphqlValue(webhookSubscription, field.selectionSet.selections, fragments, webhookProjectionOptions)
        : webhookSubscription;
    }
    case 'webhookSubscriptions':
      return serializeWebhookSubscriptionsConnection(runtime, field, variables, fragments);
    case 'webhookSubscriptionsCount':
      return serializeWebhookSubscriptionsCount(runtime, field, variables);
    default:
      return null;
  }
}

export function handleWebhookSubscriptionQuery(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): { data: Record<string, unknown> } {
  const fragments = getDocumentFragments(document);
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    data[getFieldResponseKey(field)] = rootPayloadForField(runtime, field, variables, fragments);
  }

  return { data };
}

export function handleWebhookSubscriptionMutation(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): WebhookSubscriptionMutationResult | null {
  const fragments = getDocumentFragments(document);
  const operationPath = getOperationPathLabel(document);
  const data: Record<string, unknown> = {};
  const stagedResourceIds = new Set<string>();
  const errors: Record<string, unknown>[] = [];

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'webhookSubscriptionCreate': {
        const result = handleWebhookSubscriptionCreate(runtime, field, variables, fragments, operationPath);
        if (result.errors.length > 0) {
          errors.push(...result.errors);
          break;
        }

        data[key] = result.payload;
        for (const id of result.stagedResourceIds) {
          stagedResourceIds.add(id);
        }
        break;
      }
      case 'webhookSubscriptionUpdate': {
        const result = handleWebhookSubscriptionUpdate(runtime, field, variables, fragments, operationPath);
        if (result.errors.length > 0) {
          errors.push(...result.errors);
          break;
        }

        data[key] = result.payload;
        for (const id of result.stagedResourceIds) {
          stagedResourceIds.add(id);
        }
        break;
      }
      case 'webhookSubscriptionDelete': {
        const result = handleWebhookSubscriptionDelete(runtime, field, variables, fragments, operationPath);
        if (result.errors.length > 0) {
          errors.push(...result.errors);
          break;
        }

        data[key] = result.payload;
        for (const id of result.stagedResourceIds) {
          stagedResourceIds.add(id);
        }
        break;
      }
      default:
        return null;
    }
  }

  if (errors.length > 0) {
    return {
      response: { errors },
      staged: false,
      stagedResourceIds: [],
      notes:
        'Returned captured Shopify-like webhook subscription GraphQL validation locally; deregistration was not staged.',
    };
  }

  return {
    response: { data },
    staged: stagedResourceIds.size > 0,
    stagedResourceIds: [...stagedResourceIds],
    notes:
      'Staged locally in the in-memory webhook subscription draft store; registration, update, and deregistration do not call Shopify or deliver webhook payloads.',
  };
}
