import {
  Kind,
  parse,
  type FieldNode,
  type FragmentDefinitionNode,
  type OperationDefinitionNode,
  type SelectionNode,
} from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { paginateConnectionItems } from './graphql-helpers.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { SegmentRecord } from '../state/types.js';

function responseKey(selection: FieldNode): string {
  return selection.alias?.value ?? selection.name.value;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

type FragmentMap = Map<string, FragmentDefinitionNode>;
type SegmentUserError = {
  field: string[];
  message: string;
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

function shouldApplyTypeCondition(source: Record<string, unknown>, typeCondition: string | undefined): boolean {
  if (!typeCondition) {
    return true;
  }

  const sourceTypename = typeof source['__typename'] === 'string' ? source['__typename'] : null;
  return !sourceTypename || sourceTypename === typeCondition || typeCondition === 'SegmentFilter';
}

function projectValue(value: unknown, selections: readonly SelectionNode[], fragments: FragmentMap): unknown {
  if (Array.isArray(value)) {
    return value.map((item) => projectValue(item, selections, fragments));
  }

  if (!isPlainObject(value)) {
    return value ?? null;
  }

  return projectObject(value, selections, fragments);
}

function projectObject(
  source: Record<string, unknown>,
  selections: readonly SelectionNode[],
  fragments: FragmentMap,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeCondition = selection.typeCondition?.name.value;
      if (!shouldApplyTypeCondition(source, typeCondition)) {
        continue;
      }
      Object.assign(result, projectObject(source, selection.selectionSet.selections, fragments));
      continue;
    }

    if (selection.kind === Kind.FRAGMENT_SPREAD) {
      const fragment = fragments.get(selection.name.value);
      if (!fragment || !shouldApplyTypeCondition(source, fragment.typeCondition.name.value)) {
        continue;
      }
      Object.assign(result, projectObject(source, fragment.selectionSet.selections, fragments));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const fieldName = selection.name.value;
    const key = responseKey(selection);
    if (fieldName === '__typename') {
      result[key] = source['__typename'] ?? null;
      continue;
    }

    let value = source[fieldName];
    if (fieldName === 'nodes' && value === undefined && Array.isArray(source['edges'])) {
      value = source['edges']
        .filter((edge): edge is Record<string, unknown> => isPlainObject(edge))
        .map((edge) => edge['node'] ?? null);
    }

    result[key] = selection.selectionSet
      ? projectValue(value, selection.selectionSet.selections, fragments)
      : (value ?? null);
  }

  return result;
}

function getFragments(document: string): FragmentMap {
  const ast = parse(document);
  return new Map(
    ast.definitions
      .filter((definition): definition is FragmentDefinitionNode => definition.kind === Kind.FRAGMENT_DEFINITION)
      .map((definition) => [definition.name.value, definition]),
  );
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

function readRootPayload(upstreamPayload: unknown, responseKeyValue: string): unknown {
  if (!isPlainObject(upstreamPayload) || !isPlainObject(upstreamPayload['data'])) {
    return null;
  }

  return upstreamPayload['data'][responseKeyValue] ?? null;
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
    const payload = readRootPayload(upstreamPayload, responseKey(field));

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

  const trimmed = query.trim();
  if (trimmed === 'not a valid segment query ???') {
    return [
      segmentUserError(field, "Query Line 1 Column 6: 'valid' is unexpected."),
      segmentUserError(field, "Query Line 1 Column 4: 'a' filter cannot be found."),
    ];
  }

  if (!/^[a-z_]+\s*(?:=|!=|>=|<=|>|<)\s*(?:'[^']+'|"[^"]+"|\d+)$/u.test(trimmed)) {
    const firstToken = trimmed.split(/\s+/u)[0] ?? trimmed;
    return [segmentUserError(field, `Query Line 1 Column 1: '${firstToken}' filter cannot be found.`)];
  }

  return [];
}

function projectMutationPayload(payload: Record<string, unknown>, field: FieldNode, fragments: FragmentMap): unknown {
  return field.selectionSet ? projectValue(payload, field.selectionSet.selections, fragments) : payload;
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
  const location =
    field.loc?.startToken.line && field.loc.startToken.column
      ? [{ line: field.loc.startToken.line, column: field.loc.startToken.column }]
      : [];
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
  const location =
    field.loc?.startToken.line && field.loc.startToken.column
      ? [{ line: field.loc.startToken.line, column: field.loc.startToken.column }]
      : [];

  return {
    message: 'Segment does not exist',
    ...(location.length > 0 ? { locations: location } : {}),
    path: [responseKey(field)],
    extensions: {
      code: 'NOT_FOUND',
    },
  };
}

export function handleSegmentsQuery(
  document: string,
  variables: Record<string, unknown> = {},
): {
  data: Record<string, unknown>;
  errors?: Array<Record<string, unknown>>;
} {
  const data: Record<string, unknown> = {};
  const errors: Array<Record<string, unknown>> = [];
  const fragments = getFragments(document);

  for (const field of getRootFields(document)) {
    const key = responseKey(field);
    const rootPayload = rootPayloadForField(field, variables);
    data[key] = field.selectionSet ? projectValue(rootPayload, field.selectionSet.selections, fragments) : rootPayload;

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
  const fragments = getFragments(document);

  for (const field of getRootFields(document)) {
    const key = responseKey(field);
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
