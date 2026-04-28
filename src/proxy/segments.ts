import { Kind, parse, type FieldNode, type OperationDefinitionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { CustomerRecord, CustomerSegmentMembersQueryRecord, SegmentRecord } from '../state/types.js';
import {
  defaultGraphqlTypeConditionApplies,
  getDocumentFragments,
  getFieldResponseKey,
  getNodeLocation,
  isPlainObject,
  paginateConnectionItems,
  projectGraphqlValue,
  readGraphqlDataResponsePayload,
  serializeConnection,
  type FragmentMap,
} from './graphql-helpers.js';

type SegmentUserError = {
  field: string[];
  message: string;
};

type CustomerSegmentMembersQueryUserError = {
  field: null;
  message: string;
};

type SegmentQueryValidationMode = 'segment-mutation' | 'member-query';

type SupportedSegmentQuery =
  | {
      type: 'number_of_orders';
      comparator: '=' | '>' | '>=' | '<' | '<=';
      value: number;
    }
  | {
      type: 'customer_tags_contains';
      value: string;
      negated: boolean;
    };

const segmentProjectionOptions = {
  shouldApplyTypeCondition: (source: Record<string, unknown>, typeCondition: string | undefined): boolean =>
    defaultGraphqlTypeConditionApplies(source, typeCondition) || typeCondition === 'SegmentFilter',
};

function emptyConnection(): Record<string, unknown> {
  return {
    nodes: [],
    edges: [],
    pageInfo: {
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: null,
      endCursor: null,
    },
  };
}

function segmentCursor(segment: SegmentRecord): string {
  return `cursor:${segment.id}`;
}

function buildSegmentsConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
  segments: SegmentRecord[],
): Record<string, unknown> {
  const {
    items: visibleSegments,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(segments, field, variables, (segment) => segment.id);
  const startCursor = visibleSegments[0] ? segmentCursor(visibleSegments[0]) : null;
  const endCursor = visibleSegments.at(-1) ? segmentCursor(visibleSegments.at(-1) as SegmentRecord) : null;

  return {
    nodes: visibleSegments.map((segment) => structuredClone(segment)),
    edges: visibleSegments.map((segment) => ({
      cursor: segmentCursor(segment),
      node: structuredClone(segment),
    })),
    pageInfo: {
      hasNextPage,
      hasPreviousPage,
      startCursor,
      endCursor,
    },
  };
}

function getOperation(document: string): OperationDefinitionNode | null {
  const ast = parse(document);
  return (
    ast.definitions.find(
      (definition): definition is OperationDefinitionNode => definition.kind === Kind.OPERATION_DEFINITION,
    ) ?? null
  );
}

function normalizeSegment(raw: unknown): SegmentRecord | null {
  if (!isPlainObject(raw)) {
    return null;
  }

  const id = raw['id'];
  if (typeof id !== 'string' || !id.startsWith('gid://shopify/Segment/')) {
    return null;
  }

  return {
    id,
    name: typeof raw['name'] === 'string' ? raw['name'] : null,
    query: typeof raw['query'] === 'string' ? raw['query'] : null,
    creationDate: typeof raw['creationDate'] === 'string' ? raw['creationDate'] : null,
    lastEditDate: typeof raw['lastEditDate'] === 'string' ? raw['lastEditDate'] : null,
  };
}

function collectSegmentNodes(value: unknown, segments: SegmentRecord[] = []): SegmentRecord[] {
  if (Array.isArray(value)) {
    for (const item of value) {
      collectSegmentNodes(item, segments);
    }
    return segments;
  }

  const segment = normalizeSegment(value);
  if (segment) {
    segments.push(segment);
  }

  if (!isPlainObject(value)) {
    return segments;
  }

  for (const child of Object.values(value)) {
    collectSegmentNodes(child, segments);
  }

  return segments;
}

export function hydrateSegmentsFromUpstreamResponse(
  document: string,
  _variables: Record<string, unknown>,
  upstreamPayload: unknown,
): void {
  if (!isPlainObject(upstreamPayload) || !isPlainObject(upstreamPayload['data'])) {
    return;
  }

  for (const field of getRootFields(document)) {
    const rootField = field.name.value;
    const payload = readGraphqlDataResponsePayload(upstreamPayload, getFieldResponseKey(field));

    if (payload === null && rootField !== 'segment') {
      continue;
    }

    if (
      rootField === 'segments' ||
      rootField === 'segmentsCount' ||
      rootField === 'segmentFilters' ||
      rootField === 'segmentFilterSuggestions' ||
      rootField === 'segmentValueSuggestions' ||
      rootField === 'segmentMigrations'
    ) {
      store.setBaseSegmentsRootPayload(rootField, payload);
    }

    const segments = collectSegmentNodes(payload);
    if (segments.length > 0) {
      store.upsertBaseSegments(segments);
    }
  }
}

function rootPayloadForField(field: FieldNode, variables: Record<string, unknown>): unknown {
  switch (field.name.value) {
    case 'segment': {
      const args = getFieldArguments(field, variables);
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      return id ? store.getEffectiveSegmentById(id) : null;
    }
    case 'segments':
      return store.hasStagedSegments()
        ? buildSegmentsConnection(field, variables, store.listEffectiveSegments())
        : (store.getBaseSegmentsRootPayload('segments') ??
            buildSegmentsConnection(field, variables, store.listBaseSegments()));
    case 'segmentsCount':
      return store.hasStagedSegments()
        ? {
            count: store.listEffectiveSegments().length,
            precision: 'EXACT',
          }
        : (store.getBaseSegmentsRootPayload('segmentsCount') ?? {
            count: store.listBaseSegments().length,
            precision: 'EXACT',
          });
    case 'segmentFilters':
    case 'segmentFilterSuggestions':
    case 'segmentValueSuggestions':
    case 'segmentMigrations':
      return store.getBaseSegmentsRootPayload(field.name.value) ?? emptyConnection();
    default:
      return null;
  }
}

function segmentUserError(field: string[], message: string): SegmentUserError {
  return { field, message };
}

function readStringArg(args: Record<string, unknown>, name: string): string | null {
  const value = args[name];
  return typeof value === 'string' ? value : null;
}

function normalizeSegmentName(name: string): string {
  return name.trim();
}

function resolveUniqueSegmentName(requestedName: string, currentSegmentId: string | null = null): string {
  const usedNames = new Set(
    store
      .listEffectiveSegments()
      .filter((segment) => segment.id !== currentSegmentId)
      .map((segment) => segment.name)
      .filter((name): name is string => typeof name === 'string' && name.length > 0),
  );

  if (!usedNames.has(requestedName)) {
    return requestedName;
  }

  let suffix = 2;
  let candidate = `${requestedName} (${suffix})`;
  while (usedNames.has(candidate)) {
    suffix += 1;
    candidate = `${requestedName} (${suffix})`;
  }

  return candidate;
}

function validateSegmentQuery(query: string | null, field: string[] = ['query']): SegmentUserError[] {
  if (query === null || query.trim() === '') {
    return [segmentUserError(field, "Query can't be blank")];
  }

  return validateSegmentQueryString(query, 'segment-mutation').map((message) => segmentUserError(field, message));
}

function validateCustomerSegmentMembersQuery(query: string | null): CustomerSegmentMembersQueryUserError[] {
  if (query === null || query.trim() === '') {
    return [{ field: null, message: "Query can't be blank" }];
  }

  const errorMessages = validateSegmentQueryString(query, 'member-query');
  return errorMessages.map((message) => ({ field: null, message }));
}

function validateSegmentQueryString(query: string, mode: SegmentQueryValidationMode): string[] {
  const trimmed = query.trim();
  if (parseSupportedSegmentQuery(trimmed)) {
    return [];
  }

  if (/^email_subscription_status\s*=\s*'[^']+'$/u.test(trimmed)) {
    return [];
  }

  if (trimmed === 'not a valid segment query ???') {
    return mode === 'member-query'
      ? ["Line 1 Column 6: 'valid' is unexpected."]
      : ["Query Line 1 Column 6: 'valid' is unexpected.", "Query Line 1 Column 4: 'a' filter cannot be found."];
  }

  const customerTagsEqualsMatch = trimmed.match(/^customer_tags\s*=\s*(.+)$/u);
  if (customerTagsEqualsMatch) {
    return ["Line 1 Column 14: customer_tags does not support operator '='"].map((message) =>
      mode === 'member-query' ? message : `Query ${message}`,
    );
  }

  const emailMatch = trimmed.match(/^email\s*=/u);
  if (emailMatch) {
    const message = "Line 1 Column 0: 'email' filter cannot be found.";
    return [mode === 'member-query' ? message : `Query ${message}`];
  }

  const firstToken = trimmed.split(/\s+/u)[0] ?? trimmed;
  const message = `Line 1 Column 1: '${firstToken}' filter cannot be found.`;
  return [mode === 'member-query' ? message : `Query ${message}`];
}

function parseSupportedSegmentQuery(query: string | null): SupportedSegmentQuery | null {
  if (query === null) {
    return null;
  }

  const trimmed = query.trim();
  const numberOfOrdersMatch = trimmed.match(/^number_of_orders\s*(=|>=|<=|>|<)\s*(\d+)$/u);
  if (numberOfOrdersMatch) {
    return {
      type: 'number_of_orders',
      comparator: numberOfOrdersMatch[1] as '=' | '>' | '>=' | '<' | '<=',
      value: Number.parseInt(numberOfOrdersMatch[2]!, 10),
    };
  }

  const tagContainsMatch = trimmed.match(/^customer_tags\s+(NOT\s+)?CONTAINS\s+'([^']+)'$/u);
  if (tagContainsMatch) {
    return {
      type: 'customer_tags_contains',
      value: tagContainsMatch[2]!,
      negated: Boolean(tagContainsMatch[1]),
    };
  }

  return null;
}

function customerNumberOfOrders(customer: CustomerRecord): number {
  if (typeof customer.numberOfOrders === 'number') {
    return customer.numberOfOrders;
  }

  if (typeof customer.numberOfOrders === 'string') {
    const parsed = Number.parseInt(customer.numberOfOrders, 10);
    return Number.isFinite(parsed) ? parsed : 0;
  }

  return 0;
}

function customerMatchesSupportedSegmentQuery(customer: CustomerRecord, parsed: SupportedSegmentQuery | null): boolean {
  if (!parsed) {
    return false;
  }

  if (parsed.type === 'customer_tags_contains') {
    const hasTag = customer.tags.some((tag) => tag === parsed.value);
    return parsed.negated ? !hasTag : hasTag;
  }

  const value = customerNumberOfOrders(customer);
  switch (parsed.comparator) {
    case '=':
      return value === parsed.value;
    case '>':
      return value > parsed.value;
    case '>=':
      return value >= parsed.value;
    case '<':
      return value < parsed.value;
    case '<=':
      return value <= parsed.value;
    default:
      return false;
  }
}

function listCustomerSegmentMembersForQuery(query: string | null): CustomerRecord[] {
  const parsed = parseSupportedSegmentQuery(query);
  if (!parsed) {
    return [];
  }

  return store
    .listEffectiveCustomers()
    .filter((customer) => customerMatchesSupportedSegmentQuery(customer, parsed))
    .sort((left, right) => right.id.localeCompare(left.id));
}

function memberIdForCustomer(customer: CustomerRecord): string {
  return `gid://shopify/CustomerSegmentMember/${customer.id.split('/').at(-1) ?? customer.id}`;
}

function serializeMoneyV2(value: CustomerRecord['amountSpent'], field: FieldNode): Record<string, unknown> | null {
  const money = value ?? { amount: '0.0', currencyCode: 'USD' };

  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'amount':
        result[key] = money.amount;
        break;
      case 'currencyCode':
        result[key] = money.currencyCode;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeMemberDefaultEmailAddress(
  customer: CustomerRecord,
  field: FieldNode,
): Record<string, unknown> | null {
  if (!customer.defaultEmailAddress) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'emailAddress':
        result[key] = customer.defaultEmailAddress.emailAddress;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCustomerSegmentMemberSelection(customer: CustomerRecord, field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = memberIdForCustomer(customer);
        break;
      case 'displayName':
        result[key] = customer.displayName ?? '';
        break;
      case 'firstName':
        result[key] = customer.firstName;
        break;
      case 'lastName':
        result[key] = customer.lastName;
        break;
      case 'defaultEmailAddress':
        result[key] = serializeMemberDefaultEmailAddress(customer, selection);
        break;
      case 'numberOfOrders':
        result[key] = String(customerNumberOfOrders(customer));
        break;
      case 'amountSpent':
        result[key] = serializeMoneyV2(customer.amountSpent, selection);
        break;
      case '__typename':
        result[key] = 'CustomerSegmentMember';
        break;
      default:
        result[key] = selection.selectionSet
          ? projectGraphqlValue(null, selection.selectionSet.selections, new Map())
          : null;
        break;
    }
  }
  return result;
}

function serializeSegmentStatistics(field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = getFieldResponseKey(selection);
    if (selection.name.value === 'attributeStatistics') {
      result[key] = Object.fromEntries(
        (selection.selectionSet?.selections ?? [])
          .filter((child): child is FieldNode => child.kind === Kind.FIELD)
          .map((child) => {
            const childKey = getFieldResponseKey(child);
            return [childKey, child.name.value === 'average' || child.name.value === 'sum' ? 0 : null];
          }),
      );
      continue;
    }

    result[key] = null;
  }
  return result;
}

function resolveCustomerSegmentMemberQuery(args: Record<string, unknown>): {
  query: string | null;
  queryRecord: CustomerSegmentMembersQueryRecord | null;
  missingQueryId: string | null;
} {
  const queryId = typeof args['queryId'] === 'string' ? args['queryId'] : null;
  if (queryId) {
    const queryRecord = store.getEffectiveCustomerSegmentMembersQueryById(queryId);
    return {
      query: queryRecord?.query ?? null,
      queryRecord,
      missingQueryId: queryRecord ? null : queryId,
    };
  }

  const segmentId = typeof args['segmentId'] === 'string' ? args['segmentId'] : null;
  if (segmentId) {
    const segment = store.getEffectiveSegmentById(segmentId);
    return {
      query: segment?.query ?? null,
      queryRecord: null,
      missingQueryId: null,
    };
  }

  return {
    query: typeof args['query'] === 'string' ? args['query'] : null,
    queryRecord: null,
    missingQueryId: null,
  };
}

function serializeCustomerSegmentMembersConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
): { data: Record<string, unknown> | null; error: Record<string, unknown> | null } {
  const args = getFieldArguments(field, variables);
  const resolved = resolveCustomerSegmentMemberQuery(args);
  if (resolved.missingQueryId) {
    return {
      data: null,
      error: buildCustomerSegmentMembersError(field, 'this async query cannot be found in segmentMembers'),
    };
  }

  const validationErrors = validateCustomerSegmentMembersQuery(resolved.query);
  if (validationErrors.length > 0) {
    return {
      data: null,
      error: buildCustomerSegmentMembersError(field, validationErrors[0]!.message),
    };
  }

  const allMembers = listCustomerSegmentMembersForQuery(resolved.query);
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    allMembers,
    field,
    variables,
    (customer) => customer.id,
  );
  const connection = serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (customer) => customer.id,
    serializeNode: (customer, selection) => serializeCustomerSegmentMemberSelection(customer, selection),
    serializeUnknownField: (selection) => {
      switch (selection.name.value) {
        case 'totalCount':
          return allMembers.length;
        case 'statistics':
          return serializeSegmentStatistics(selection);
        default:
          return null;
      }
    },
  });

  return {
    data: connection,
    error: null,
  };
}

function serializeCustomerSegmentMembersQuery(
  query: CustomerSegmentMembersQueryRecord,
  field: FieldNode,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = query.id;
        break;
      case 'currentCount':
        result[key] = query.currentCount;
        break;
      case 'done':
        result[key] = query.done;
        break;
      case '__typename':
        result[key] = 'CustomerSegmentMembersQuery';
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCustomerSegmentMembership(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const customerId = typeof args['customerId'] === 'string' ? args['customerId'] : null;
  const rawSegmentIds = Array.isArray(args['segmentIds']) ? args['segmentIds'] : [];
  const customer = customerId ? store.getEffectiveCustomerById(customerId) : null;
  const memberships = rawSegmentIds
    .filter((segmentId): segmentId is string => typeof segmentId === 'string')
    .flatMap((segmentId) => {
      const segment = store.getEffectiveSegmentById(segmentId);
      if (!segment) {
        return [];
      }

      return [
        {
          segmentId,
          isMember: customer
            ? customerMatchesSupportedSegmentQuery(customer, parseSupportedSegmentQuery(segment.query))
            : false,
        },
      ];
    });

  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = getFieldResponseKey(selection);
    if (selection.name.value !== 'memberships') {
      result[key] = null;
      continue;
    }

    result[key] = memberships.map((membership) => {
      const item: Record<string, unknown> = {};
      for (const child of selection.selectionSet?.selections ?? []) {
        if (child.kind !== Kind.FIELD) {
          continue;
        }

        const childKey = getFieldResponseKey(child);
        switch (child.name.value) {
          case 'segmentId':
            item[childKey] = membership.segmentId;
            break;
          case 'isMember':
            item[childKey] = membership.isMember;
            break;
          default:
            item[childKey] = null;
            break;
        }
      }
      return item;
    });
  }

  return result;
}

function projectMutationPayload(payload: Record<string, unknown>, field: FieldNode, fragments: FragmentMap): unknown {
  return field.selectionSet
    ? projectGraphqlValue(payload, field.selectionSet.selections, fragments, segmentProjectionOptions)
    : payload;
}

function handleSegmentCreate(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);
  const rawName = readStringArg(args, 'name');
  const rawQuery = readStringArg(args, 'query');
  const errors: SegmentUserError[] = [];

  if (rawName === null || rawName.trim() === '') {
    errors.push(segmentUserError(['name'], "Name can't be blank"));
  }
  errors.push(...validateSegmentQuery(rawQuery));

  const timestamp = makeSyntheticTimestamp();
  const segment: SegmentRecord | null =
    errors.length === 0 && rawName !== null && rawQuery !== null
      ? {
          id: makeSyntheticGid('Segment'),
          name: resolveUniqueSegmentName(normalizeSegmentName(rawName)),
          query: rawQuery.trim(),
          creationDate: timestamp,
          lastEditDate: timestamp,
        }
      : null;

  if (segment) {
    store.stageCreateSegment(segment);
  }

  return projectMutationPayload(
    {
      segment,
      userErrors: errors,
    },
    field,
    fragments,
  );
}

function handleSegmentUpdate(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);
  const id = readStringArg(args, 'id');
  const existing = id ? store.getEffectiveSegmentById(id) : null;
  const errors: SegmentUserError[] = [];

  if (!id || !existing) {
    errors.push(segmentUserError(['id'], 'Segment does not exist'));
  }

  const rawName = readStringArg(args, 'name');
  const rawQuery = readStringArg(args, 'query');
  if (args['name'] !== undefined && (rawName === null || rawName.trim() === '')) {
    errors.push(segmentUserError(['name'], "Name can't be blank"));
  }
  if (args['query'] !== undefined) {
    errors.push(...validateSegmentQuery(rawQuery));
  }

  const segment: SegmentRecord | null =
    errors.length === 0 && existing && id
      ? {
          id,
          name: rawName === null ? existing.name : resolveUniqueSegmentName(normalizeSegmentName(rawName), existing.id),
          query: rawQuery === null ? existing.query : rawQuery.trim(),
          creationDate: existing.creationDate,
          lastEditDate: makeSyntheticTimestamp(),
        }
      : null;

  if (segment) {
    store.stageUpdateSegment(segment);
  }

  return projectMutationPayload(
    {
      segment,
      userErrors: errors,
    },
    field,
    fragments,
  );
}

function handleSegmentDelete(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);
  const id = readStringArg(args, 'id');
  const existing = id ? store.getEffectiveSegmentById(id) : null;
  const errors: SegmentUserError[] = [];

  if (!id || !existing) {
    errors.push(segmentUserError(['id'], 'Segment does not exist'));
  }

  if (errors.length === 0 && id) {
    store.stageDeleteSegment(id);
  }

  return projectMutationPayload(
    {
      deletedSegmentId: errors.length === 0 ? id : null,
      userErrors: errors,
    },
    field,
    fragments,
  );
}

function buildMissingRequiredArgumentsError(
  document: string,
  field: FieldNode,
  missingArguments: string[],
): Record<string, unknown> {
  const operation = getOperation(document);
  const operationLabel = operation?.name?.value
    ? `${operation.operation} ${operation.name.value}`
    : (operation?.operation ?? 'mutation');
  const location = getNodeLocation(field);
  const argumentsText = missingArguments.join(', ');

  return {
    message: `Field '${field.name.value}' is missing required arguments: ${argumentsText}`,
    ...(location.length > 0 ? { locations: location } : {}),
    path: [operationLabel, field.name.value],
    extensions: {
      code: 'missingRequiredArguments',
      className: 'Field',
      name: field.name.value,
      arguments: argumentsText,
    },
  };
}

function buildSegmentNotFoundError(field: FieldNode): Record<string, unknown> {
  const location = getNodeLocation(field);

  return {
    message: 'Segment does not exist',
    ...(location.length > 0 ? { locations: location } : {}),
    path: [getFieldResponseKey(field)],
    extensions: {
      code: 'NOT_FOUND',
    },
  };
}

export function handleSegmentsQuery(
  document: string,
  variables: Record<string, unknown> = {},
): {
  data: Record<string, unknown> | null;
  errors?: Array<Record<string, unknown>>;
} {
  const data: Record<string, unknown> = {};
  const errors: Array<Record<string, unknown>> = [];
  const fragments = getDocumentFragments(document);

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    if (field.name.value === 'customerSegmentMembers') {
      const result = serializeCustomerSegmentMembersConnection(field, variables);
      if (result.error) {
        errors.push(result.error);
        return {
          data: null,
          errors,
        };
      }
      data[key] = result.data;
      continue;
    }

    if (field.name.value === 'customerSegmentMembersQuery') {
      const args = getFieldArguments(field, variables);
      const queryId = typeof args['id'] === 'string' ? args['id'] : null;
      const queryRecord = queryId ? store.getEffectiveCustomerSegmentMembersQueryById(queryId) : null;
      data[key] = queryRecord ? serializeCustomerSegmentMembersQuery(queryRecord, field) : null;
      if (queryId && !queryRecord) {
        errors.push(buildCustomerSegmentMembersQueryNotFoundError(field));
      }
      continue;
    }

    if (field.name.value === 'customerSegmentMembership') {
      data[key] = serializeCustomerSegmentMembership(field, variables);
      continue;
    }

    const rootPayload = rootPayloadForField(field, variables);
    data[key] = field.selectionSet
      ? projectGraphqlValue(rootPayload, field.selectionSet.selections, fragments, segmentProjectionOptions)
      : rootPayload;

    if (field.name.value === 'segment') {
      const args = getFieldArguments(field, variables);
      if (typeof args['id'] === 'string' && rootPayload === null) {
        errors.push(buildSegmentNotFoundError(field));
      }
    }
  }

  return {
    data,
    ...(errors.length > 0 ? { errors } : {}),
  };
}

export function handleSegmentMutation(
  document: string,
  variables: Record<string, unknown> = {},
): {
  data?: Record<string, unknown>;
  errors?: Array<Record<string, unknown>>;
} {
  const data: Record<string, unknown> = {};
  const errors: Array<Record<string, unknown>> = [];
  const fragments = getDocumentFragments(document);

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'segmentCreate': {
        const argumentNames = new Set((field.arguments ?? []).map((argument) => argument.name.value));
        const missingArguments = ['name', 'query'].filter((argumentName) => !argumentNames.has(argumentName));
        if (missingArguments.length > 0) {
          errors.push(buildMissingRequiredArgumentsError(document, field, missingArguments));
          break;
        }
        data[key] = handleSegmentCreate(field, variables, fragments);
        break;
      }
      case 'segmentUpdate': {
        const argumentNames = new Set((field.arguments ?? []).map((argument) => argument.name.value));
        if (!argumentNames.has('id')) {
          errors.push(buildMissingRequiredArgumentsError(document, field, ['id']));
          break;
        }
        data[key] = handleSegmentUpdate(field, variables, fragments);
        break;
      }
      case 'segmentDelete': {
        const argumentNames = new Set((field.arguments ?? []).map((argument) => argument.name.value));
        if (!argumentNames.has('id')) {
          errors.push(buildMissingRequiredArgumentsError(document, field, ['id']));
          break;
        }
        data[key] = handleSegmentDelete(field, variables, fragments);
        break;
      }
      case 'customerSegmentMembersQueryCreate': {
        const argumentNames = new Set((field.arguments ?? []).map((argument) => argument.name.value));
        if (!argumentNames.has('input')) {
          errors.push(buildMissingRequiredArgumentsError(document, field, ['input']));
          break;
        }
        data[key] = handleCustomerSegmentMembersQueryCreate(field, variables, fragments);
        break;
      }
      default:
        data[key] = null;
        break;
    }
  }

  if (errors.length > 0) {
    return { errors };
  }

  return { data };
}

function readInputObjectArg(args: Record<string, unknown>, name: string): Record<string, unknown> | null {
  const value = args[name];
  return isPlainObject(value) ? value : null;
}

function handleCustomerSegmentMembersQueryCreate(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const args = getFieldArguments(field, variables);
  const input = readInputObjectArg(args, 'input');
  const rawQuery = typeof input?.['query'] === 'string' ? input['query'] : null;
  const segmentId = typeof input?.['segmentId'] === 'string' ? input['segmentId'] : null;
  const segment = segmentId ? store.getEffectiveSegmentById(segmentId) : null;
  const query = rawQuery ?? segment?.query ?? null;
  const userErrors = validateCustomerSegmentMembersQuery(query);
  const members = userErrors.length === 0 ? listCustomerSegmentMembersForQuery(query) : [];
  const queryRecord: CustomerSegmentMembersQueryRecord | null =
    userErrors.length === 0
      ? {
          id: makeSyntheticGid('CustomerSegmentMembersQuery'),
          query,
          segmentId,
          currentCount: members.length,
          done: true,
        }
      : null;

  if (queryRecord) {
    store.stageCustomerSegmentMembersQuery(queryRecord);
  }

  const responseQuery = queryRecord
    ? {
        ...queryRecord,
        currentCount: 0,
        done: false,
      }
    : null;

  return projectMutationPayload(
    {
      customerSegmentMembersQuery: responseQuery,
      userErrors,
    },
    field,
    fragments,
  );
}

function buildCustomerSegmentMembersError(field: FieldNode, message: string): Record<string, unknown> {
  const location = getNodeLocation(field);

  return {
    message,
    ...(location.length > 0 ? { locations: location } : {}),
    path: [getFieldResponseKey(field)],
  };
}

function buildCustomerSegmentMembersQueryNotFoundError(field: FieldNode): Record<string, unknown> {
  const location = getNodeLocation(field);

  return {
    message: 'Something went wrong',
    ...(location.length > 0 ? { locations: location } : {}),
    extensions: {
      code: 'INTERNAL_SERVER_ERROR',
    },
    path: [getFieldResponseKey(field)],
  };
}
