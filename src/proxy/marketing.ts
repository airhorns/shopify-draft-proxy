import type { FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import {
  matchesSearchQueryDate,
  matchesSearchQueryNumber,
  matchesSearchQueryText,
  normalizeSearchQueryValue,
  parseSearchQuery,
  type SearchQueryNode,
  type SearchQueryTerm,
} from '../search-query-parser.js';
import { store } from '../state/store.js';
import type { MarketingRecord } from '../state/types.js';
import {
  buildSyntheticCursor,
  getDocumentFragments,
  getFieldResponseKey,
  isPlainObject,
  paginateConnectionItems,
  projectGraphqlValue,
  readGraphqlDataResponsePayload,
  serializeConnection,
  type FragmentMap,
} from './graphql-helpers.js';

type MarketingKind = 'activity' | 'event';

type MarketingConnectionItem = {
  node: Record<string, unknown>;
  paginationCursor: string;
  outputCursor: string;
};

const ACTIVITY_ID_PREFIX = 'gid://shopify/MarketingActivity/';
const EVENT_ID_PREFIX = 'gid://shopify/MarketingEvent/';

function collectConnectionCandidates(value: unknown): Array<{ data: unknown; cursor?: string | null }> {
  if (!isPlainObject(value) || !Array.isArray(value['edges'])) {
    return [];
  }

  return value['edges'].flatMap((edge): Array<{ data: unknown; cursor?: string | null }> => {
    if (!isPlainObject(edge)) {
      return [];
    }

    const node = edge['node'];
    if (!isPlainObject(node)) {
      return [];
    }

    const cursor = typeof edge['cursor'] === 'string' && edge['cursor'].length > 0 ? edge['cursor'] : null;
    return [{ data: node, cursor }];
  });
}

function collectMarketingNodes(
  value: unknown,
  result: {
    activities: Array<{ data: unknown; cursor?: string | null }>;
    events: Array<{ data: unknown; cursor?: string | null }>;
  } = {
    activities: [],
    events: [],
  },
): {
  activities: Array<{ data: unknown; cursor?: string | null }>;
  events: Array<{ data: unknown; cursor?: string | null }>;
} {
  if (Array.isArray(value)) {
    for (const item of value) {
      collectMarketingNodes(item, result);
    }
    return result;
  }

  if (!isPlainObject(value)) {
    return result;
  }

  const id = value['id'];
  if (typeof id === 'string') {
    if (id.startsWith(ACTIVITY_ID_PREFIX)) {
      result.activities.push({ data: value });
    } else if (id.startsWith(EVENT_ID_PREFIX)) {
      result.events.push({ data: value });
    }
  }

  for (const child of Object.values(value)) {
    collectMarketingNodes(child, result);
  }

  return result;
}

export function hydrateMarketingFromUpstreamResponse(
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

    const collected = collectMarketingNodes(payload);
    if (rootField === 'marketingActivities') {
      collected.activities.unshift(...collectConnectionCandidates(payload));
    }
    if (rootField === 'marketingEvents') {
      collected.events.unshift(...collectConnectionCandidates(payload));
    }

    if (collected.activities.length > 0) {
      store.upsertBaseMarketingActivities(collected.activities);
    }
    if (collected.events.length > 0) {
      store.upsertBaseMarketingEvents(collected.events);
    }
  }
}

function readString(source: Record<string, unknown>, field: string): string | null {
  const value = source[field];
  return typeof value === 'string' ? value : null;
}

function idNumber(id: string): number | null {
  const value = id.split('/').at(-1);
  if (!value) {
    return null;
  }

  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? parsed : null;
}

function matchesIdTerm(id: string, term: SearchQueryTerm): boolean {
  const expected = normalizeSearchQueryValue(term.value);
  const numericId = idNumber(id);
  if (term.comparator && numericId !== null) {
    return matchesSearchQueryNumber(numericId, term);
  }

  return id.toLowerCase().includes(expected) || String(numericId ?? '').includes(expected);
}

function appName(source: Record<string, unknown>): string | null {
  const app = source['app'];
  return isPlainObject(app) ? (readString(app, 'name') ?? readString(app, 'title')) : null;
}

function matchesActivityTerm(source: Record<string, unknown>, term: SearchQueryTerm): boolean {
  const field = term.field ?? 'default';
  switch (field) {
    case 'default':
      return (
        matchesSearchQueryText(readString(source, 'title'), term) ||
        matchesSearchQueryText(readString(source, 'sourceAndMedium'), term) ||
        matchesSearchQueryText(appName(source), term)
      );
    case 'app_name':
      return matchesSearchQueryText(appName(source), term);
    case 'created_at':
      return matchesSearchQueryDate(readString(source, 'createdAt'), term);
    case 'id':
      return matchesIdTerm(String(source['id'] ?? ''), term);
    case 'scheduled_to_end_at':
      return matchesSearchQueryDate(readString(source, 'scheduledToEndAt'), term);
    case 'scheduled_to_start_at':
      return matchesSearchQueryDate(readString(source, 'scheduledToStartAt'), term);
    case 'tactic':
      return normalizeSearchQueryValue(readString(source, 'tactic') ?? '') === normalizeSearchQueryValue(term.value);
    case 'title':
      return matchesSearchQueryText(readString(source, 'title'), term);
    case 'updated_at':
      return matchesSearchQueryDate(readString(source, 'updatedAt'), term);
    default:
      return false;
  }
}

function matchesEventTerm(source: Record<string, unknown>, term: SearchQueryTerm): boolean {
  const field = term.field ?? 'default';
  switch (field) {
    case 'default':
      return (
        matchesSearchQueryText(readString(source, 'description'), term) ||
        matchesSearchQueryText(readString(source, 'sourceAndMedium'), term) ||
        matchesSearchQueryText(readString(source, 'remoteId'), term)
      );
    case 'description':
      return matchesSearchQueryText(readString(source, 'description'), term);
    case 'id':
      return matchesIdTerm(String(source['id'] ?? ''), term);
    case 'started_at':
      return matchesSearchQueryDate(readString(source, 'startedAt'), term);
    case 'type':
      return normalizeSearchQueryValue(readString(source, 'type') ?? '') === normalizeSearchQueryValue(term.value);
    default:
      return false;
  }
}

function matchesSearchNode(
  node: SearchQueryNode | null,
  source: Record<string, unknown>,
  kind: MarketingKind,
): boolean {
  if (node === null) {
    return true;
  }

  switch (node.type) {
    case 'term': {
      const termMatch =
        kind === 'activity' ? matchesActivityTerm(source, node.term) : matchesEventTerm(source, node.term);
      return node.term.negated ? !termMatch : termMatch;
    }
    case 'and':
      return node.children.every((child) => matchesSearchNode(child, source, kind));
    case 'or':
      return node.children.some((child) => matchesSearchNode(child, source, kind));
    case 'not':
      return !matchesSearchNode(node.child, source, kind);
  }
}

function compareNullableString(left: string | null, right: string | null): number {
  return (left ?? '').localeCompare(right ?? '');
}

function sortRecords(records: MarketingRecord[], sortKey: unknown, kind: MarketingKind): MarketingRecord[] {
  const normalizedSortKey = typeof sortKey === 'string' ? sortKey : kind === 'activity' ? 'CREATED_AT' : 'ID';
  const sorted = [...records];

  sorted.sort((left, right) => {
    const leftData = left.data;
    const rightData = right.data;
    switch (normalizedSortKey) {
      case 'CREATED_AT':
        return compareNullableString(readString(leftData, 'createdAt'), readString(rightData, 'createdAt'));
      case 'STARTED_AT':
        return compareNullableString(readString(leftData, 'startedAt'), readString(rightData, 'startedAt'));
      case 'TITLE':
        return compareNullableString(readString(leftData, 'title'), readString(rightData, 'title'));
      case 'ID':
      default:
        return left.id.localeCompare(right.id);
    }
  });

  return sorted;
}

function filterRecords(
  records: MarketingRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
  kind: MarketingKind,
): MarketingRecord[] {
  const args = getFieldArguments(field, variables);
  let filtered = records;

  if (kind === 'activity') {
    const activityIds = Array.isArray(args['marketingActivityIds']) ? args['marketingActivityIds'] : [];
    if (activityIds.length > 0) {
      const ids = new Set(activityIds.filter((id): id is string => typeof id === 'string'));
      filtered = filtered.filter((record) => ids.has(record.id));
    }

    const remoteIds = Array.isArray(args['remoteIds']) ? args['remoteIds'] : [];
    if (remoteIds.length > 0) {
      const ids = new Set(remoteIds.filter((id): id is string => typeof id === 'string'));
      filtered = filtered.filter((record) => {
        const remoteId = readString(record.data, 'remoteId');
        return remoteId !== null && ids.has(remoteId);
      });
    }
  }

  const query = typeof args['query'] === 'string' ? args['query'] : null;
  if (query) {
    const search = parseSearchQuery(query);
    filtered = filtered.filter((record) => matchesSearchNode(search, record.data, kind));
  }

  filtered = sortRecords(filtered, args['sortKey'], kind);
  return args['reverse'] === true ? filtered.reverse() : filtered;
}

function connectionItems(records: MarketingRecord[]): MarketingConnectionItem[] {
  return records.map((record) => {
    const id = record.id;
    const capturedCursor = typeof record.cursor === 'string' && record.cursor.length > 0 ? record.cursor : null;
    return {
      node: structuredClone(record.data),
      paginationCursor: capturedCursor ?? id,
      outputCursor: capturedCursor ?? buildSyntheticCursor(id),
    };
  });
}

function buildConnection(
  records: MarketingRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const items = connectionItems(records);
  const window = paginateConnectionItems(items, field, variables, (item) => item.paginationCursor);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (item) => item.outputCursor,
    serializeNode: (item, selection) =>
      projectGraphqlValue(item.node, selection.selectionSet?.selections ?? [], fragments),
    pageInfoOptions: {
      prefixCursors: false,
    },
  });
}

function rootPayloadForField(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);

  switch (field.name.value) {
    case 'marketingActivity': {
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      return id ? store.getBaseMarketingActivityById(id) : null;
    }
    case 'marketingActivities':
      return buildConnection(
        filterRecords(store.listBaseMarketingActivities(), field, variables, 'activity'),
        field,
        variables,
        fragments,
      );
    case 'marketingEvent': {
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      return id ? store.getBaseMarketingEventById(id) : null;
    }
    case 'marketingEvents':
      return buildConnection(
        filterRecords(store.listBaseMarketingEvents(), field, variables, 'event'),
        field,
        variables,
        fragments,
      );
    default:
      return null;
  }
}

export function handleMarketingQuery(
  document: string,
  variables: Record<string, unknown> = {},
): {
  data: Record<string, unknown>;
} {
  const data: Record<string, unknown> = {};
  const fragments = getDocumentFragments(document);

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const rootPayload = rootPayloadForField(field, variables, fragments);
    data[key] = field.selectionSet
      ? projectGraphqlValue(rootPayload, field.selectionSet.selections, fragments)
      : rootPayload;
  }

  return { data };
}
