import { Kind, parse, type FieldNode, type FragmentDefinitionNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { parseSearchQuery, type SearchQueryNode, type SearchQueryTerm } from '../search-query-parser.js';
import { paginateConnectionItems, serializeConnection } from './graphql-helpers.js';
import { store } from '../state/store.js';
import type { MarketRecord } from '../state/types.js';

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

function shouldApplyTypeCondition(source: Record<string, unknown>, typeCondition: string | undefined): boolean {
  if (!typeCondition) {
    return true;
  }

  const sourceTypename = typeof source['__typename'] === 'string' ? source['__typename'] : null;
  return (
    !sourceTypename ||
    sourceTypename === typeCondition ||
    (typeCondition === 'Catalog' && sourceTypename === 'MarketCatalog')
  );
}

function projectValue(
  value: unknown,
  selections: readonly SelectionNode[],
  fragments: FragmentMap,
  variables: Record<string, unknown>,
): unknown {
  if (Array.isArray(value)) {
    return value.map((item) => projectValue(item, selections, fragments, variables));
  }

  if (!isPlainObject(value)) {
    return value ?? null;
  }

  return projectObject(value, selections, fragments, variables);
}

function projectObject(
  source: Record<string, unknown>,
  selections: readonly SelectionNode[],
  fragments: FragmentMap,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeCondition = selection.typeCondition?.name.value;
      if (!shouldApplyTypeCondition(source, typeCondition)) {
        continue;
      }
      Object.assign(result, projectObject(source, selection.selectionSet.selections, fragments, variables));
      continue;
    }

    if (selection.kind === Kind.FRAGMENT_SPREAD) {
      const fragment = fragments.get(selection.name.value);
      if (!fragment || !shouldApplyTypeCondition(source, fragment.typeCondition.name.value)) {
        continue;
      }
      Object.assign(result, projectObject(source, fragment.selectionSet.selections, fragments, variables));
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

    const value = source[fieldName];
    result[key] = selection.selectionSet
      ? projectSelectedFieldValue(value, selection, fragments, variables)
      : (value ?? null);
  }

  return result;
}

type ConnectionEdge = {
  cursor: string;
  node: unknown;
};

function readConnectionEdges(value: unknown): ConnectionEdge[] {
  if (!isPlainObject(value) || !Array.isArray(value['edges'])) {
    return [];
  }

  return value['edges'].flatMap((edge): ConnectionEdge[] => {
    if (!isPlainObject(edge)) {
      return [];
    }

    const rawCursor = edge['cursor'];
    const node = edge['node'] ?? null;
    const nodeId = isPlainObject(node) && typeof node['id'] === 'string' ? node['id'] : null;
    const cursor = typeof rawCursor === 'string' && rawCursor.length > 0 ? rawCursor : (nodeId ?? '');

    return cursor ? [{ cursor, node }] : [];
  });
}

function projectConnectionPayload(
  value: Record<string, unknown>,
  selection: FieldNode,
  fragments: FragmentMap,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const edges = readConnectionEdges(value);
  const window = paginateConnectionItems(edges, selection, variables, (edge) => edge.cursor);
  return serializeConnection(selection, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (edge) => edge.cursor,
    serializeNode: (edge, nodeSelection) =>
      projectValue(edge.node, nodeSelection.selectionSet?.selections ?? [], fragments, variables),
    pageInfoOptions: {
      prefixCursors: false,
    },
    serializeUnknownField: (childSelection) => value[childSelection.name.value] ?? null,
  });
}

function projectSelectedFieldValue(
  value: unknown,
  selection: FieldNode,
  fragments: FragmentMap,
  variables: Record<string, unknown>,
): unknown {
  if (isPlainObject(value) && Array.isArray(value['edges'])) {
    return projectConnectionPayload(value, selection, fragments, variables);
  }

  return projectValue(value, selection.selectionSet?.selections ?? [], fragments, variables);
}

function getFragments(document: string): FragmentMap {
  const ast = parse(document);
  return new Map(
    ast.definitions
      .filter((definition): definition is FragmentDefinitionNode => definition.kind === Kind.FRAGMENT_DEFINITION)
      .map((definition) => [definition.name.value, definition]),
  );
}

type MarketHydrationEntry = {
  market: Record<string, unknown>;
  cursor: string | null;
};

function collectMarketNodes(
  value: unknown,
  markets: MarketHydrationEntry[] = [],
  cursor: string | null = null,
): MarketHydrationEntry[] {
  if (Array.isArray(value)) {
    for (const item of value) {
      collectMarketNodes(item, markets, cursor);
    }
    return markets;
  }

  if (!isPlainObject(value)) {
    return markets;
  }

  if (Array.isArray(value['edges'])) {
    for (const edge of value['edges']) {
      if (!isPlainObject(edge)) {
        continue;
      }

      const edgeCursor = typeof edge['cursor'] === 'string' && edge['cursor'].length > 0 ? edge['cursor'] : null;
      collectMarketNodes(edge['node'], markets, edgeCursor);
    }
  }

  const id = value['id'];
  if (typeof id === 'string' && id.startsWith('gid://shopify/Market/')) {
    markets.push({ market: value, cursor });
  }

  for (const [key, child] of Object.entries(value)) {
    if (key === 'edges') {
      continue;
    }
    collectMarketNodes(child, markets, null);
  }

  return markets;
}

function readRootPayload(upstreamPayload: unknown, rootField: string): unknown {
  if (!isPlainObject(upstreamPayload)) {
    return null;
  }

  const data = upstreamPayload['data'];
  if (!isPlainObject(data)) {
    return null;
  }

  return data[rootField] ?? null;
}

export function hydrateMarketsFromUpstreamResponse(
  _document: string,
  _variables: Record<string, unknown>,
  upstreamPayload: unknown,
): void {
  for (const rootField of ['markets', 'market', 'catalogs', 'webPresences', 'marketsResolvedValues']) {
    const rootPayload = readRootPayload(upstreamPayload, rootField);
    if (rootPayload === null) {
      continue;
    }

    store.setBaseMarketsRootPayload(rootField, rootPayload);

    if (rootField === 'markets' || rootField === 'catalogs' || rootField === 'webPresences') {
      store.upsertBaseMarkets(collectMarketNodes(rootPayload));
    } else if (rootField === 'market') {
      store.upsertBaseMarkets([rootPayload]);
    } else if (rootField === 'marketsResolvedValues') {
      store.upsertBaseMarkets(collectMarketNodes(rootPayload));
    }
  }
}

function stripSearchValueQuotes(rawValue: string): string {
  const value = rawValue.trim();
  if (
    value.length >= 2 &&
    ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'")))
  ) {
    return value.slice(1, -1);
  }

  return value;
}

function marketNumericId(market: MarketRecord): number | null {
  const match = market.id.match(/\/(\d+)$/u);
  if (!match) {
    return null;
  }

  const id = Number.parseInt(match[1] ?? '', 10);
  return Number.isFinite(id) ? id : null;
}

function matchesStringValue(candidate: unknown, rawValue: string, mode: 'exact' | 'includes' = 'exact'): boolean {
  if (typeof candidate !== 'string') {
    return false;
  }

  const value = stripSearchValueQuotes(rawValue).toLowerCase();
  const normalizedCandidate = candidate.toLowerCase();
  return mode === 'includes' ? normalizedCandidate.includes(value) : normalizedCandidate === value;
}

function searchTermValue(term: SearchQueryTerm): string {
  return term.comparator === null ? term.value : `${term.comparator}${term.value}`;
}

function compareMarketId(marketId: number, rawValue: string): boolean {
  const match = stripSearchValueQuotes(rawValue).match(/^(<=|>=|<|>|=)?\s*(?:gid:\/\/shopify\/Market\/)?(\d+)$/u);
  if (!match) {
    return false;
  }

  const operator = match[1] ?? '=';
  const value = Number.parseInt(match[2] ?? '', 10);
  switch (operator) {
    case '<=':
      return marketId <= value;
    case '>=':
      return marketId >= value;
    case '<':
      return marketId < value;
    case '>':
      return marketId > value;
    case '=':
      return marketId === value;
    default:
      return false;
  }
}

function marketConditionTypes(market: MarketRecord): string[] {
  const conditions = market.data['conditions'];
  if (!isPlainObject(conditions) || !Array.isArray(conditions['conditionTypes'])) {
    return [];
  }

  return conditions['conditionTypes'].filter((condition): condition is string => typeof condition === 'string');
}

function matchesPositiveMarketQueryTerm(market: MarketRecord, term: SearchQueryTerm): boolean {
  if (term.field === null) {
    const value = stripSearchValueQuotes(term.value);
    return (
      matchesStringValue(market.data['name'], value, 'includes') ||
      matchesStringValue(market.data['handle'], value, 'includes') ||
      matchesStringValue(market.id, value, 'includes')
    );
  }

  const field = term.field.toLowerCase();
  const value = searchTermValue(term);

  switch (field) {
    case 'id': {
      if (matchesStringValue(market.id, value, 'exact')) {
        return true;
      }

      const numericId = marketNumericId(market);
      return numericId === null ? false : compareMarketId(numericId, value);
    }
    case 'name':
      return matchesStringValue(market.data['name'], value, 'includes');
    case 'status':
      return matchesStringValue(market.data['status'], value, 'exact');
    case 'market_type':
    case 'type':
      return matchesStringValue(market.data['type'], value, 'exact');
    case 'market_condition_types': {
      const expectedTypes = stripSearchValueQuotes(value)
        .split(',')
        .map((entry) => entry.trim().toUpperCase())
        .filter(Boolean);
      const actualTypes = new Set(marketConditionTypes(market).map((entry) => entry.toUpperCase()));
      return expectedTypes.every((entry) => actualTypes.has(entry));
    }
    default:
      return true;
  }
}

function matchesMarketQueryTerm(market: MarketRecord, term: SearchQueryTerm): boolean {
  if (!term.raw) {
    return true;
  }

  const matches = matchesPositiveMarketQueryTerm(market, term);
  return term.negated ? !matches : matches;
}

function matchesMarketQueryNode(market: MarketRecord, node: SearchQueryNode): boolean {
  switch (node.type) {
    case 'term':
      return matchesMarketQueryTerm(market, node.term);
    case 'and':
      return node.children.every((child) => matchesMarketQueryNode(market, child));
    case 'or':
      return node.children.some((child) => matchesMarketQueryNode(market, child));
    case 'not':
      return !matchesMarketQueryNode(market, node.child);
  }
}

function applyMarketsQuery(markets: MarketRecord[], rawQuery: unknown): MarketRecord[] {
  if (typeof rawQuery !== 'string' || !rawQuery.trim()) {
    return markets;
  }

  const parsedQuery = parseSearchQuery(rawQuery, { recognizeNotKeyword: true });
  if (!parsedQuery) {
    return markets;
  }

  return markets.filter((market) => matchesMarketQueryNode(market, parsedQuery));
}

function applyRootMarketFilters(markets: MarketRecord[], args: Record<string, unknown>): MarketRecord[] {
  return markets.filter((market) => {
    const rawType = args['type'];
    const rawStatus = args['status'];

    return (
      (typeof rawType !== 'string' || matchesStringValue(market.data['type'], rawType, 'exact')) &&
      (typeof rawStatus !== 'string' || matchesStringValue(market.data['status'], rawStatus, 'exact'))
    );
  });
}

function compareNullableStrings(left: unknown, right: unknown): number {
  return (typeof left === 'string' ? left : '').localeCompare(typeof right === 'string' ? right : '');
}

function compareMarketsBySortKey(left: MarketRecord, right: MarketRecord, rawSortKey: unknown): number {
  const sortKey = typeof rawSortKey === 'string' ? rawSortKey : 'NAME';
  switch (sortKey) {
    case 'CREATED_AT':
      return compareNullableStrings(left.data['createdAt'], right.data['createdAt']) || left.id.localeCompare(right.id);
    case 'ID':
      return (marketNumericId(left) ?? 0) - (marketNumericId(right) ?? 0) || left.id.localeCompare(right.id);
    case 'MARKET_CONDITION_TYPES':
      return (
        marketConditionTypes(left).join(',').localeCompare(marketConditionTypes(right).join(',')) ||
        left.id.localeCompare(right.id)
      );
    case 'MARKET_TYPE':
      return compareNullableStrings(left.data['type'], right.data['type']) || left.id.localeCompare(right.id);
    case 'STATUS':
      return compareNullableStrings(left.data['status'], right.data['status']) || left.id.localeCompare(right.id);
    case 'UPDATED_AT':
      return compareNullableStrings(left.data['updatedAt'], right.data['updatedAt']) || left.id.localeCompare(right.id);
    case 'NAME':
    default:
      return compareNullableStrings(left.data['name'], right.data['name']) || left.id.localeCompare(right.id);
  }
}

function listMarketsForConnection(field: FieldNode, variables: Record<string, unknown>): MarketRecord[] {
  const args = getFieldArguments(field, variables);
  const filteredMarkets = applyMarketsQuery(applyRootMarketFilters(store.listBaseMarkets(), args), args['query']);
  const sortedMarkets = [...filteredMarkets].sort((left, right) =>
    compareMarketsBySortKey(left, right, args['sortKey']),
  );

  return args['reverse'] === true ? sortedMarkets.reverse() : sortedMarkets;
}

function marketCursor(market: MarketRecord): string {
  return market.cursor ?? market.id;
}

function serializeMarketsConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const markets = listMarketsForConnection(field, variables);
  const window = paginateConnectionItems(markets, field, variables, marketCursor);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: marketCursor,
    serializeNode: (market, selection) =>
      projectValue(market.data, selection.selectionSet?.selections ?? [], fragments, variables),
    pageInfoOptions: {
      prefixCursors: false,
    },
  });
}

function rootPayloadForField(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  switch (field.name.value) {
    case 'market': {
      const args = getFieldArguments(field, variables);
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      return id ? store.getBaseMarketById(id) : null;
    }
    case 'markets':
      return serializeMarketsConnection(field, variables, fragments);
    case 'catalogs':
    case 'webPresences':
      return store.getBaseMarketsRootPayload(field.name.value) ?? emptyConnection();
    case 'marketsResolvedValues':
      return store.getBaseMarketsRootPayload(field.name.value);
    default:
      return null;
  }
}

export function handleMarketsQuery(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  const fragments = getFragments(document);

  for (const field of getRootFields(document)) {
    const key = responseKey(field);
    const rootPayload = rootPayloadForField(field, variables, fragments);
    data[key] =
      field.name.value === 'markets'
        ? rootPayload
        : field.selectionSet
          ? projectValue(rootPayload, field.selectionSet.selections, fragments, variables)
          : rootPayload;
  }

  return { data };
}

export function seedMarketsFromCapture(capture: unknown): boolean {
  const roots = ['markets', 'market', 'catalogs', 'webPresences', 'marketsResolvedValues'];
  const seededPayload: Record<string, unknown> = { data: {} };
  const data = seededPayload['data'] as Record<string, unknown>;
  let seeded = false;

  for (const root of roots) {
    const payload = readRootPayload(capture, root);
    if (payload === null) {
      continue;
    }

    data[root] = payload;
    seeded = true;
  }

  if (seeded) {
    hydrateMarketsFromUpstreamResponse('query MarketsSeed { __typename }', {}, seededPayload);
  }

  return seeded;
}
