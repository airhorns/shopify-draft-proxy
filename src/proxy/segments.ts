import { Kind, parse, type FieldNode, type FragmentDefinitionNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { store } from '../state/store.js';
import type { SegmentRecord } from '../state/types.js';

function responseKey(selection: FieldNode): string {
  return selection.alias?.value ?? selection.name.value;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

type FragmentMap = Map<string, FragmentDefinitionNode>;

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

function buildSegmentsConnection(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const first =
    typeof args['first'] === 'number' && Number.isFinite(args['first']) ? Math.max(0, Math.floor(args['first'])) : null;
  const last =
    typeof args['last'] === 'number' && Number.isFinite(args['last']) ? Math.max(0, Math.floor(args['last'])) : null;
  const allSegments = store.listBaseSegments();
  const firstWindow = first === null ? allSegments : allSegments.slice(0, first);
  const visibleSegments = last === null ? firstWindow : firstWindow.slice(Math.max(0, firstWindow.length - last));
  const hasPreviousPage = last !== null && firstWindow.length > visibleSegments.length;
  const hasNextPage = first !== null && allSegments.length > visibleSegments.length;
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
      return id ? store.getBaseSegmentById(id) : null;
    }
    case 'segments':
      return store.getBaseSegmentsRootPayload('segments') ?? buildSegmentsConnection(field, variables);
    case 'segmentsCount':
      return (
        store.getBaseSegmentsRootPayload('segmentsCount') ?? {
          count: store.listBaseSegments().length,
          precision: 'EXACT',
        }
      );
    case 'segmentFilters':
    case 'segmentFilterSuggestions':
    case 'segmentValueSuggestions':
    case 'segmentMigrations':
      return store.getBaseSegmentsRootPayload(field.name.value) ?? emptyConnection();
    default:
      return null;
  }
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
