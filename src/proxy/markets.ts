import { Kind, parse, type FieldNode, type FragmentDefinitionNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { parseSearchQuery, type SearchQueryNode, type SearchQueryTerm } from '../search-query-parser.js';
import {
  getFieldResponseKey,
  getSelectedChildFields,
  paginateConnectionItems,
  serializeConnectionPageInfo,
} from './graphql-helpers.js';
import { store } from '../state/store.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import type { JsonValue } from '../json-schemas.js';
import type {
  CatalogRecord,
  MarketLocalizationRecord,
  MarketRecord,
  PriceListRecord,
  ProductMetafieldRecord,
  WebPresenceRecord,
} from '../state/types.js';

function responseKey(selection: FieldNode): string {
  return selection.alias?.value ?? selection.name.value;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

type FragmentMap = Map<string, FragmentDefinitionNode>;
type MarketUserError = {
  field: string[];
  message: string;
  code: string;
};
type MarketLocalizableResourceRecord = {
  resourceId: string;
  content: Array<{
    key: string;
    value: string | null;
    digest: string | null;
  }>;
};

const CURRENCY_NAMES: Record<string, string> = {
  AUD: 'Australian Dollar',
  CAD: 'Canadian Dollar',
  EUR: 'Euro',
  GBP: 'British Pound',
  JPY: 'Japanese Yen',
  NZD: 'New Zealand Dollar',
  USD: 'US Dollar',
};

const COUNTRY_NAMES: Record<string, string> = {
  AU: 'Australia',
  CA: 'Canada',
  DE: 'Germany',
  FR: 'France',
  GB: 'United Kingdom',
  JP: 'Japan',
  NZ: 'New Zealand',
  US: 'United States',
};

const COUNTRY_CURRENCIES: Record<string, string> = {
  AU: 'AUD',
  CA: 'CAD',
  DE: 'EUR',
  FR: 'EUR',
  GB: 'GBP',
  JP: 'JPY',
  NZ: 'NZD',
  US: 'USD',
};

const LOCALE_NAMES: Record<string, string> = {
  de: 'German',
  en: 'English',
  es: 'Spanish',
  fr: 'French',
  it: 'Italian',
  ja: 'Japanese',
  nl: 'Dutch',
  pt: 'Portuguese',
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

function serializeCountSelection(field: FieldNode, count: number, precision = 'EXACT'): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
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

function connectionFromNodes(nodes: unknown[]): Record<string, unknown> {
  const edges = nodes.map((node) => {
    const id = isPlainObject(node) && typeof node['id'] === 'string' ? node['id'] : makeSyntheticGid('Cursor');
    return {
      cursor: id,
      node,
    };
  });

  return {
    edges,
    pageInfo: {
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: edges[0]?.cursor ?? null,
      endCursor: edges.at(-1)?.cursor ?? null,
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

    if (source['__typename'] === 'MarketCatalog') {
      if (fieldName === 'marketsCount') {
        result[key] = serializeCountSelection(selection, readConnectionEdges(source['markets']).length);
        continue;
      }
      if (fieldName === 'operations') {
        result[key] = projectValue(
          source['operations'] ?? [],
          selection.selectionSet?.selections ?? [],
          fragments,
          variables,
        );
        continue;
      }
    }

    if (source['__typename'] === 'PriceList') {
      if (fieldName === 'prices') {
        result[key] = projectPriceListPricesConnection(source['prices'], selection, fragments, variables);
        continue;
      }
      if (fieldName === 'quantityRules') {
        const quantityRules = isPlainObject(source['quantityRules']) ? source['quantityRules'] : emptyConnection();
        result[key] = projectConnectionPayload(quantityRules, selection, fragments, variables);
        continue;
      }
    }

    const value = source[fieldName];
    result[key] = selection.selectionSet
      ? projectSelectedFieldValue(value, selection, fragments, variables)
      : (value ?? null);
  }

  return result;
}

function priceListPriceNodeMatchesQuery(node: unknown, rawQuery: unknown): boolean {
  if (typeof rawQuery !== 'string' || !rawQuery.trim()) {
    return true;
  }

  const parsedQuery = parseSearchQuery(rawQuery, { recognizeNotKeyword: true });
  if (!parsedQuery) {
    return true;
  }

  const matchesTerm = (term: SearchQueryTerm): boolean => {
    if (!term.raw) {
      return true;
    }

    if (!isPlainObject(node)) {
      return false;
    }

    const variant = isPlainObject(node['variant']) ? node['variant'] : null;
    const product = isPlainObject(variant?.['product']) ? variant['product'] : null;
    const field = term.field?.toLowerCase() ?? null;
    const value = stripSearchValueQuotes(searchTermValue(term));
    const variantId = typeof variant?.['id'] === 'string' ? variant['id'] : null;
    const productId = typeof product?.['id'] === 'string' ? product['id'] : null;
    const matches =
      field === 'variant_id'
        ? matchesStringValue(variantId, value, 'exact') ||
          (variantId !== null && String(resourceNumericId(variantId)) === value)
        : field === 'product_id'
          ? matchesStringValue(productId, value, 'exact') ||
            (productId !== null && String(resourceNumericId(productId)) === value)
          : true;

    return term.negated ? !matches : matches;
  };

  const matchesNode = (queryNode: SearchQueryNode): boolean => {
    switch (queryNode.type) {
      case 'term':
        return matchesTerm(queryNode.term);
      case 'and':
        return queryNode.children.every((child) => matchesNode(child));
      case 'or':
        return queryNode.children.some((child) => matchesNode(child));
      case 'not':
        return !matchesNode(queryNode.child);
    }
  };

  return matchesNode(parsedQuery);
}

function projectPriceListPricesConnection(
  value: unknown,
  selection: FieldNode,
  fragments: FragmentMap,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(selection, variables);
  const originType = typeof args['originType'] === 'string' ? args['originType'] : null;
  if (originType === null && (typeof args['query'] !== 'string' || !args['query'].trim()) && isPlainObject(value)) {
    return projectConnectionPayload(value, selection, fragments, variables);
  }

  const edges = readConnectionEdges(value).filter((edge) => {
    if (!isPlainObject(edge.node)) {
      return false;
    }
    return (
      (originType === null || edge.node['originType'] === originType) &&
      priceListPriceNodeMatchesQuery(edge.node, args['query'])
    );
  });
  return projectConnectionPayload({ edges }, selection, fragments, variables);
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
  const args = getFieldArguments(selection, variables);
  const first = typeof args['first'] === 'number' && Number.isInteger(args['first']) ? args['first'] : null;
  const preservesCapturedPageInfo =
    isPlainObject(value['pageInfo']) &&
    args['after'] === undefined &&
    args['before'] === undefined &&
    args['last'] === undefined &&
    (first === null || first >= edges.length) &&
    window.items.length === edges.length;
  const result: Record<string, unknown> = {};

  for (const childSelection of getSelectedChildFields(selection)) {
    const key = getFieldResponseKey(childSelection);
    switch (childSelection.name.value) {
      case 'nodes':
        result[key] = window.items.map((edge) =>
          projectValue(edge.node, childSelection.selectionSet?.selections ?? [], fragments, variables),
        );
        break;
      case 'edges':
        result[key] = window.items.map((edge) => projectEdge(edge, childSelection, fragments, variables));
        break;
      case 'pageInfo':
        result[key] = preservesCapturedPageInfo
          ? projectValue(value['pageInfo'], childSelection.selectionSet?.selections ?? [], fragments, variables)
          : serializeConnectionPageInfo(
              childSelection,
              window.items,
              window.hasNextPage,
              window.hasPreviousPage,
              (edge) => edge.cursor,
              { prefixCursors: false },
            );
        break;
      default:
        result[key] = value[childSelection.name.value] ?? null;
    }
  }

  return result;
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

function projectEdge(
  edge: ConnectionEdge,
  selection: FieldNode,
  fragments: FragmentMap,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const edgeSelection of getSelectedChildFields(selection)) {
    const key = getFieldResponseKey(edgeSelection);
    switch (edgeSelection.name.value) {
      case 'cursor':
        result[key] = edge.cursor;
        break;
      case 'node':
        result[key] = projectValue(edge.node, edgeSelection.selectionSet?.selections ?? [], fragments, variables);
        break;
      default:
        result[key] = null;
    }
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

type MarketHydrationEntry = {
  market: Record<string, unknown>;
  cursor: string | null;
};

type WebPresenceHydrationEntry = {
  webPresence: Record<string, unknown>;
  cursor: string | null;
};

type CatalogHydrationEntry = {
  catalog: Record<string, unknown>;
  cursor: string | null;
};

type PriceListHydrationEntry = {
  priceList: Record<string, unknown>;
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

function collectWebPresenceNodes(
  value: unknown,
  webPresences: WebPresenceHydrationEntry[] = [],
  cursor: string | null = null,
): WebPresenceHydrationEntry[] {
  if (Array.isArray(value)) {
    for (const item of value) {
      collectWebPresenceNodes(item, webPresences, cursor);
    }
    return webPresences;
  }

  if (!isPlainObject(value)) {
    return webPresences;
  }

  if (Array.isArray(value['edges'])) {
    for (const edge of value['edges']) {
      if (!isPlainObject(edge)) {
        continue;
      }

      const edgeCursor = typeof edge['cursor'] === 'string' && edge['cursor'].length > 0 ? edge['cursor'] : null;
      collectWebPresenceNodes(edge['node'], webPresences, edgeCursor);
    }
  }

  const id = value['id'];
  if (typeof id === 'string' && id.startsWith('gid://shopify/MarketWebPresence/')) {
    webPresences.push({ webPresence: value, cursor });
  }

  for (const [key, child] of Object.entries(value)) {
    if (key === 'edges') {
      continue;
    }
    collectWebPresenceNodes(child, webPresences, null);
  }

  return webPresences;
}

function collectCatalogNodes(
  value: unknown,
  catalogs: CatalogHydrationEntry[] = [],
  cursor: string | null = null,
): CatalogHydrationEntry[] {
  if (Array.isArray(value)) {
    for (const item of value) {
      collectCatalogNodes(item, catalogs, cursor);
    }
    return catalogs;
  }

  if (!isPlainObject(value)) {
    return catalogs;
  }

  if (Array.isArray(value['edges'])) {
    for (const edge of value['edges']) {
      if (!isPlainObject(edge)) {
        continue;
      }

      const edgeCursor = typeof edge['cursor'] === 'string' && edge['cursor'].length > 0 ? edge['cursor'] : null;
      collectCatalogNodes(edge['node'], catalogs, edgeCursor);
    }
  }

  const id = value['id'];
  if (
    typeof id === 'string' &&
    /gid:\/\/shopify\/(?:MarketCatalog|CompanyLocationCatalog|AppCatalog|Catalog)\//u.test(id)
  ) {
    const catalog = { __typename: 'MarketCatalog', ...value };
    catalogs.push({ catalog, cursor });
  }

  for (const [key, child] of Object.entries(value)) {
    if (key === 'edges') {
      continue;
    }
    collectCatalogNodes(child, catalogs, null);
  }

  return catalogs;
}

function collectPriceListNodes(
  value: unknown,
  priceLists: PriceListHydrationEntry[] = [],
  cursor: string | null = null,
): PriceListHydrationEntry[] {
  if (Array.isArray(value)) {
    for (const item of value) {
      collectPriceListNodes(item, priceLists, cursor);
    }
    return priceLists;
  }

  if (!isPlainObject(value)) {
    return priceLists;
  }

  if (Array.isArray(value['edges'])) {
    for (const edge of value['edges']) {
      if (!isPlainObject(edge)) {
        continue;
      }

      const edgeCursor = typeof edge['cursor'] === 'string' && edge['cursor'].length > 0 ? edge['cursor'] : null;
      collectPriceListNodes(edge['node'], priceLists, edgeCursor);
    }
  }

  const id = value['id'];
  if (typeof id === 'string' && id.startsWith('gid://shopify/PriceList/')) {
    priceLists.push({ priceList: { __typename: 'PriceList', ...value }, cursor });
  }

  for (const [key, child] of Object.entries(value)) {
    if (key === 'edges') {
      continue;
    }
    collectPriceListNodes(child, priceLists, null);
  }

  return priceLists;
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
  for (const rootField of [
    'markets',
    'market',
    'catalog',
    'catalogs',
    'catalogsCount',
    'priceList',
    'priceLists',
    'webPresences',
    'marketsResolvedValues',
    'marketLocalizableResource',
    'marketLocalizableResources',
    'marketLocalizableResourcesByIds',
  ]) {
    const rootPayload = readRootPayload(upstreamPayload, rootField);
    if (rootPayload === null) {
      continue;
    }

    store.setBaseMarketsRootPayload(rootField, rootPayload);
    store.upsertBaseWebPresences(collectWebPresenceNodes(rootPayload));
    store.upsertBaseCatalogs(collectCatalogNodes(rootPayload));
    store.upsertBasePriceLists(collectPriceListNodes(rootPayload));

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

function resourceNumericId(resourceId: string): number | null {
  const match = resourceId.match(/\/(\d+)$/u);
  if (!match) {
    return null;
  }

  const id = Number.parseInt(match[1] ?? '', 10);
  return Number.isFinite(id) ? id : null;
}

function catalogMarkets(catalog: CatalogRecord): ConnectionEdge[] {
  return readConnectionEdges(catalog.data['markets']);
}

function catalogHasType(catalog: CatalogRecord, rawType: unknown): boolean {
  if (typeof rawType !== 'string' || rawType.length === 0) {
    return true;
  }

  if (rawType === 'MARKET') {
    return catalog.data['__typename'] === 'MarketCatalog' || catalog.id.startsWith('gid://shopify/MarketCatalog/');
  }

  return matchesStringValue(catalog.data['__typename'], `${rawType[0]}${rawType.slice(1).toLowerCase()}Catalog`);
}

function compareCatalogId(catalogId: number, rawValue: string): boolean {
  const match = stripSearchValueQuotes(rawValue).match(
    /^(<=|>=|<|>|=)?\s*(?:gid:\/\/shopify\/(?:MarketCatalog|CompanyLocationCatalog|AppCatalog|Catalog)\/)?(\d+)$/u,
  );
  if (!match) {
    return false;
  }

  const operator = match[1] ?? '=';
  const value = Number.parseInt(match[2] ?? '', 10);
  switch (operator) {
    case '<=':
      return catalogId <= value;
    case '>=':
      return catalogId >= value;
    case '<':
      return catalogId < value;
    case '>':
      return catalogId > value;
    case '=':
      return catalogId === value;
    default:
      return false;
  }
}

function matchesCatalogQueryTerm(catalog: CatalogRecord, term: SearchQueryTerm): boolean {
  if (!term.raw) {
    return true;
  }

  const value = searchTermValue(term);
  const field = term.field?.toLowerCase() ?? null;
  const matches =
    field === null
      ? matchesStringValue(catalog.data['title'], value, 'includes') ||
        matchesStringValue(catalog.id, value, 'includes')
      : field === 'id'
        ? matchesStringValue(catalog.id, value, 'exact') ||
          (resourceNumericId(catalog.id) !== null && compareCatalogId(resourceNumericId(catalog.id)!, value))
        : field === 'title'
          ? matchesStringValue(catalog.data['title'], value, 'includes')
          : field === 'status'
            ? matchesStringValue(catalog.data['status'], value, 'exact')
            : field === 'market_id'
              ? catalogMarkets(catalog).some(
                  (edge) =>
                    isPlainObject(edge.node) &&
                    typeof edge.node['id'] === 'string' &&
                    matchesStringValue(edge.node['id'], value, 'exact'),
                )
              : true;

  return term.negated ? !matches : matches;
}

function matchesCatalogQueryNode(catalog: CatalogRecord, node: SearchQueryNode): boolean {
  switch (node.type) {
    case 'term':
      return matchesCatalogQueryTerm(catalog, node.term);
    case 'and':
      return node.children.every((child) => matchesCatalogQueryNode(catalog, child));
    case 'or':
      return node.children.some((child) => matchesCatalogQueryNode(catalog, child));
    case 'not':
      return !matchesCatalogQueryNode(catalog, node.child);
  }
}

function applyCatalogsQuery(catalogs: CatalogRecord[], rawQuery: unknown): CatalogRecord[] {
  if (typeof rawQuery !== 'string' || !rawQuery.trim()) {
    return catalogs;
  }

  const parsedQuery = parseSearchQuery(rawQuery, { recognizeNotKeyword: true });
  if (!parsedQuery) {
    return catalogs;
  }

  return catalogs.filter((catalog) => matchesCatalogQueryNode(catalog, parsedQuery));
}

function compareCatalogsBySortKey(left: CatalogRecord, right: CatalogRecord, rawSortKey: unknown): number {
  const sortKey = typeof rawSortKey === 'string' ? rawSortKey : 'ID';
  switch (sortKey) {
    case 'TITLE':
      return compareNullableStrings(left.data['title'], right.data['title']) || left.id.localeCompare(right.id);
    case 'STATUS':
      return compareNullableStrings(left.data['status'], right.data['status']) || left.id.localeCompare(right.id);
    case 'ID':
    default:
      return (resourceNumericId(left.id) ?? 0) - (resourceNumericId(right.id) ?? 0) || left.id.localeCompare(right.id);
  }
}

function listCatalogsForConnection(field: FieldNode, variables: Record<string, unknown>): CatalogRecord[] {
  const args = getFieldArguments(field, variables);
  const filteredCatalogs = applyCatalogsQuery(
    store.listBaseCatalogs().filter((catalog) => catalogHasType(catalog, args['type'])),
    args['query'],
  );
  const sortedCatalogs = [...filteredCatalogs].sort((left, right) =>
    compareCatalogsBySortKey(left, right, args['sortKey']),
  );

  return args['reverse'] === true ? sortedCatalogs.reverse() : sortedCatalogs;
}

function catalogCursor(catalog: CatalogRecord): string {
  return catalog.cursor ?? catalog.id;
}

function serializeCatalogsConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const catalogs = listCatalogsForConnection(field, variables);
  const window = paginateConnectionItems(catalogs, field, variables, catalogCursor);
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = window.items.map((catalog) =>
          projectValue(catalog.data, selection.selectionSet?.selections ?? [], fragments, variables),
        );
        break;
      case 'edges':
        result[key] = window.items.map((catalog) =>
          projectEdge({ cursor: catalogCursor(catalog), node: catalog.data }, selection, fragments, variables),
        );
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(
          selection,
          window.items,
          window.hasNextPage,
          window.hasPreviousPage,
          catalogCursor,
          { prefixCursors: false },
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeCatalogsCount(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const count = listCatalogsForConnection(field, variables).length;
  const rawLimit = args['limit'];
  const limit = typeof rawLimit === 'number' && Number.isFinite(rawLimit) && rawLimit >= 0 ? rawLimit : null;
  const visibleCount = limit === null ? count : Math.min(count, limit);
  const precision = limit !== null && count > limit ? 'AT_LEAST' : 'EXACT';
  return serializeCountSelection(field, visibleCount, precision);
}

function comparePriceListsBySortKey(left: PriceListRecord, right: PriceListRecord, rawSortKey: unknown): number {
  const sortKey = typeof rawSortKey === 'string' ? rawSortKey : 'ID';
  switch (sortKey) {
    case 'NAME':
      return compareNullableStrings(left.data['name'], right.data['name']) || left.id.localeCompare(right.id);
    case 'ID':
    default:
      return (resourceNumericId(left.id) ?? 0) - (resourceNumericId(right.id) ?? 0) || left.id.localeCompare(right.id);
  }
}

function listPriceListsForConnection(field: FieldNode, variables: Record<string, unknown>): PriceListRecord[] {
  const args = getFieldArguments(field, variables);
  const sortedPriceLists = [...store.listBasePriceLists()].sort((left, right) =>
    comparePriceListsBySortKey(left, right, args['sortKey']),
  );
  return args['reverse'] === true ? sortedPriceLists.reverse() : sortedPriceLists;
}

function priceListCursor(priceList: PriceListRecord): string {
  return priceList.cursor ?? priceList.id;
}

function serializePriceListsConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const priceLists = listPriceListsForConnection(field, variables);
  const window = paginateConnectionItems(priceLists, field, variables, priceListCursor);
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = window.items.map((priceList) =>
          projectValue(priceList.data, selection.selectionSet?.selections ?? [], fragments, variables),
        );
        break;
      case 'edges':
        result[key] = window.items.map((priceList) =>
          projectEdge({ cursor: priceListCursor(priceList), node: priceList.data }, selection, fragments, variables),
        );
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(
          selection,
          window.items,
          window.hasNextPage,
          window.hasPreviousPage,
          priceListCursor,
          { prefixCursors: false },
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function normalizeHandleParts(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

function marketError(field: string[], message: string, code: string): MarketUserError {
  return { field, message, code };
}

function translationError(field: string[], message: string, code: string): MarketUserError {
  return { field, message, code };
}

function listMarketLocalizableMetafields(): ProductMetafieldRecord[] {
  return store
    .listEffectiveProducts()
    .flatMap((product) => store.getEffectiveMetafieldsByProductId(product.id))
    .sort((left, right) => left.id.localeCompare(right.id));
}

function findMarketLocalizableMetafield(resourceId: string): ProductMetafieldRecord | null {
  return listMarketLocalizableMetafields().find((metafield) => metafield.id === resourceId) ?? null;
}

function localizableResourceFromMetafield(metafield: ProductMetafieldRecord): MarketLocalizableResourceRecord {
  return {
    resourceId: metafield.id,
    content: [
      {
        key: 'value',
        value: metafield.value,
        digest: metafield.compareDigest ?? null,
      },
    ],
  };
}

function readMarketLocalizableResource(resourceId: string): MarketLocalizableResourceRecord | null {
  const metafield = findMarketLocalizableMetafield(resourceId);
  return metafield ? localizableResourceFromMetafield(metafield) : null;
}

function serializeMarketLocalizationMarket(
  marketId: string,
  selections: readonly SelectionNode[],
  fragments: FragmentMap,
  variables: Record<string, unknown>,
): unknown {
  const market = store.getEffectiveMarketById(marketId);
  return projectValue(market, selections, fragments, variables);
}

function serializeMarketLocalization(
  localization: MarketLocalizationRecord,
  selections: readonly SelectionNode[],
  fragments: FragmentMap,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case 'key':
        result[key] = localization.key;
        break;
      case 'value':
        result[key] = localization.value;
        break;
      case 'updatedAt':
        result[key] = localization.updatedAt;
        break;
      case 'outdated':
        result[key] = localization.outdated;
        break;
      case 'market':
        result[key] = serializeMarketLocalizationMarket(
          localization.marketId,
          selection.selectionSet?.selections ?? [],
          fragments,
          variables,
        );
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeMarketLocalizableContent(
  resource: MarketLocalizableResourceRecord,
  selections: readonly SelectionNode[],
): Array<Record<string, unknown>> {
  return resource.content.map((content) => {
    const result: Record<string, unknown> = {};
    for (const selection of selections) {
      if (selection.kind !== Kind.FIELD) {
        continue;
      }

      const key = responseKey(selection);
      switch (selection.name.value) {
        case 'key':
          result[key] = content.key;
          break;
        case 'value':
          result[key] = content.value;
          break;
        case 'digest':
          result[key] = content.digest;
          break;
        default:
          result[key] = null;
      }
    }
    return result;
  });
}

function serializeMarketLocalizableResource(
  resource: MarketLocalizableResourceRecord | null,
  selections: readonly SelectionNode[],
  fragments: FragmentMap,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  if (!resource) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case 'resourceId':
        result[key] = resource.resourceId;
        break;
      case 'marketLocalizableContent':
        result[key] = serializeMarketLocalizableContent(resource, selection.selectionSet?.selections ?? []);
        break;
      case 'marketLocalizations': {
        const args = getFieldArguments(selection, variables);
        const marketId = typeof args['marketId'] === 'string' ? args['marketId'] : null;
        const localizations = marketId ? store.listEffectiveMarketLocalizations(resource.resourceId, marketId) : [];
        result[key] = localizations.map((localization) =>
          serializeMarketLocalization(localization, selection.selectionSet?.selections ?? [], fragments, variables),
        );
        break;
      }
      default:
        result[key] = null;
    }
  }
  return result;
}

function listMarketLocalizableResources(
  field: FieldNode,
  variables: Record<string, unknown>,
): MarketLocalizableResourceRecord[] {
  const args = getFieldArguments(field, variables);
  const resourceType = args['resourceType'];
  if (resourceType !== 'METAFIELD') {
    return [];
  }

  const resources = listMarketLocalizableMetafields().map(localizableResourceFromMetafield);
  return args['reverse'] === true ? resources.reverse() : resources;
}

function listMarketLocalizableResourcesByIds(
  field: FieldNode,
  variables: Record<string, unknown>,
): MarketLocalizableResourceRecord[] {
  const args = getFieldArguments(field, variables);
  const resourceIds = Array.isArray(args['resourceIds'])
    ? args['resourceIds'].filter((id): id is string => typeof id === 'string')
    : [];
  const resourcesById = new Map(
    listMarketLocalizableMetafields().map((metafield) => [metafield.id, localizableResourceFromMetafield(metafield)]),
  );
  const resources = resourceIds.flatMap((resourceId) => {
    const resource = resourcesById.get(resourceId);
    return resource ? [resource] : [];
  });
  return args['reverse'] === true ? resources.reverse() : resources;
}

function marketLocalizableResourceCursor(resource: MarketLocalizableResourceRecord): string {
  return resource.resourceId;
}

function serializedMarketLocalizableResourceCursor(resource: MarketLocalizableResourceRecord): string {
  return `cursor:${marketLocalizableResourceCursor(resource)}`;
}

function serializeMarketLocalizableResourcesConnection(
  resources: MarketLocalizableResourceRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const window = paginateConnectionItems(resources, field, variables, marketLocalizableResourceCursor);
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = window.items.map((resource) =>
          serializeMarketLocalizableResource(resource, selection.selectionSet?.selections ?? [], fragments, variables),
        );
        break;
      case 'edges':
        result[key] = window.items.map((resource) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = serializedMarketLocalizableResourceCursor(resource);
                break;
              case 'node':
                edgeResult[edgeKey] = serializeMarketLocalizableResource(
                  resource,
                  edgeSelection.selectionSet?.selections ?? [],
                  fragments,
                  variables,
                );
                break;
              default:
                edgeResult[edgeKey] = null;
            }
          }
          return edgeResult;
        });
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(
          selection,
          window.items,
          window.hasNextPage,
          window.hasPreviousPage,
          marketLocalizableResourceCursor,
          { prefixCursors: true },
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function readInput(raw: unknown): Record<string, unknown> {
  return isPlainObject(raw) ? raw : {};
}

function marketHandleInUse(handle: string, excludedMarketId?: string): boolean {
  return store
    .listEffectiveMarkets()
    .some((market) => market.data['handle'] === handle && market.id !== excludedMarketId);
}

function normalizeMarketHandle(
  input: Record<string, unknown>,
  excludedMarketId?: string,
): { handle: string; errors: MarketUserError[] } {
  const rawHandle = input['handle'];
  const fallbackName = typeof input['name'] === 'string' ? input['name'] : 'market';
  const handle = typeof rawHandle === 'string' ? rawHandle.trim() : normalizeHandleParts(fallbackName);
  const normalizedHandle = normalizeHandleParts(handle) || 'market';
  const errors: MarketUserError[] = [];

  if (typeof rawHandle === 'string' && rawHandle.trim() && rawHandle.trim() !== normalizedHandle) {
    errors.push(marketError(['input', 'handle'], 'Handle is invalid', 'INVALID'));
  }

  if (marketHandleInUse(normalizedHandle, excludedMarketId)) {
    errors.push(marketError(['input', 'handle'], `Handle '${normalizedHandle}' has already been taken`, 'TAKEN'));
  }

  return { handle: normalizedHandle, errors };
}

function readStatusAndEnabled(
  input: Record<string, unknown>,
  existing?: Record<string, unknown>,
): { status: string; enabled: boolean; errors: MarketUserError[] } {
  const rawStatus = input['status'];
  const rawEnabled = input['enabled'];
  const existingStatus = typeof existing?.['status'] === 'string' ? existing['status'] : 'ACTIVE';
  let status = rawStatus === 'ACTIVE' || rawStatus === 'DRAFT' ? rawStatus : existingStatus;

  if (typeof rawEnabled === 'boolean' && rawStatus !== 'ACTIVE' && rawStatus !== 'DRAFT') {
    status = rawEnabled ? 'ACTIVE' : 'DRAFT';
  }

  const enabled = status === 'ACTIVE';
  const errors: MarketUserError[] = [];
  if (typeof rawStatus === 'string' && rawStatus !== 'ACTIVE' && rawStatus !== 'DRAFT') {
    errors.push(marketError(['input', 'status'], "Status isn't included in the list", 'INCLUSION'));
  }

  if (typeof rawEnabled === 'boolean' && rawEnabled !== enabled) {
    errors.push(
      marketError(
        ['input', 'enabled'],
        'Invalid combination of status and enabled',
        'INVALID_STATUS_AND_ENABLED_COMBINATION',
      ),
    );
  }

  return { status, enabled, errors };
}

function currencySetting(currencyCode: string): Record<string, unknown> {
  return {
    currencyCode,
    currencyName: CURRENCY_NAMES[currencyCode] ?? currencyCode,
    enabled: true,
  };
}

function readStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === 'string') : [];
}

function buildRegionsCondition(
  input: Record<string, unknown>,
  errors: MarketUserError[],
): Record<string, unknown> | null {
  const applicationLevel =
    typeof input['applicationLevel'] === 'string'
      ? input['applicationLevel']
      : input['regionIds']
        ? 'SPECIFIED'
        : 'SPECIFIED';
  const rawRegions = Array.isArray(input['regions']) ? input['regions'] : [];
  const regionIds = readStringArray(input['regionIds']);

  if (applicationLevel === 'SPECIFIED' && rawRegions.length === 0 && regionIds.length === 0) {
    errors.push(
      marketError(
        ['input', 'conditions', 'regionsCondition', 'regions'],
        'Specified conditions cannot be empty',
        'SPECIFIED_CONDITIONS_CANNOT_BE_EMPTY',
      ),
    );
  }

  const regionNodes = rawRegions.flatMap((region): Record<string, unknown>[] => {
    if (!isPlainObject(region) || typeof region['countryCode'] !== 'string' || !region['countryCode']) {
      errors.push(
        marketError(
          ['input', 'conditions', 'regionsCondition', 'regions', 'countryCode'],
          'The country code is missing',
          'MISSING_COUNTRY_CODE',
        ),
      );
      return [];
    }

    const countryCode = region['countryCode'].toUpperCase();
    const currencyCode = COUNTRY_CURRENCIES[countryCode] ?? 'USD';
    return [
      {
        __typename: 'MarketRegionCountry',
        id: makeSyntheticGid('MarketRegionCountry'),
        name: COUNTRY_NAMES[countryCode] ?? countryCode,
        code: countryCode,
        currency: currencySetting(currencyCode),
      },
    ];
  });

  for (const id of regionIds) {
    regionNodes.push({
      __typename: 'MarketRegionCountry',
      id,
      name: id.split('/').at(-1) ?? id,
    });
  }

  return {
    applicationLevel,
    regions: connectionFromNodes(regionNodes),
  };
}

function buildIdCondition(
  input: Record<string, unknown>,
  idField: 'companyLocationIds' | 'locationIds',
  nodeType: 'CompanyLocation' | 'Location',
): Record<string, unknown> {
  const applicationLevel = typeof input['applicationLevel'] === 'string' ? input['applicationLevel'] : 'SPECIFIED';
  const nodes = readStringArray(input[idField]).map((id) => ({
    __typename: nodeType,
    id,
  }));

  return {
    applicationLevel,
    [nodeType === 'CompanyLocation' ? 'companyLocations' : 'locations']: connectionFromNodes(nodes),
  };
}

function buildConditions(
  rawConditions: unknown,
  existing: Record<string, unknown> | null,
  errors: MarketUserError[],
): Record<string, unknown> {
  const existingConditions = isPlainObject(existing?.['conditions'])
    ? structuredClone(existing['conditions'] as Record<string, unknown>)
    : {};
  const conditionsInput = readInput(rawConditions);
  const updateInput =
    isPlainObject(conditionsInput['conditionsToAdd']) || isPlainObject(conditionsInput['conditionsToDelete']);
  const directInput = updateInput ? readInput(conditionsInput['conditionsToAdd']) : conditionsInput;
  const deleteInput = updateInput ? readInput(conditionsInput['conditionsToDelete']) : {};
  const result: Record<string, unknown> = {
    conditionTypes: Array.isArray(existingConditions['conditionTypes'])
      ? structuredClone(existingConditions['conditionTypes'])
      : [],
  };

  for (const key of ['regionsCondition', 'companyLocationsCondition', 'locationsCondition']) {
    if (existingConditions[key] !== undefined) {
      result[key] = structuredClone(existingConditions[key]);
    }
  }

  if (isPlainObject(directInput['regionsCondition'])) {
    result['regionsCondition'] = buildRegionsCondition(directInput['regionsCondition'], errors);
  }
  if (isPlainObject(directInput['companyLocationsCondition'])) {
    result['companyLocationsCondition'] = buildIdCondition(
      directInput['companyLocationsCondition'],
      'companyLocationIds',
      'CompanyLocation',
    );
  }
  if (isPlainObject(directInput['locationsCondition'])) {
    result['locationsCondition'] = buildIdCondition(directInput['locationsCondition'], 'locationIds', 'Location');
  }

  if (isPlainObject(deleteInput['regionsCondition'])) {
    delete result['regionsCondition'];
  }
  if (isPlainObject(deleteInput['companyLocationsCondition'])) {
    delete result['companyLocationsCondition'];
  }
  if (isPlainObject(deleteInput['locationsCondition'])) {
    delete result['locationsCondition'];
  }

  const possibleConditionEntries: Array<[string, string]> = [
    ['regionsCondition', 'REGION'],
    ['companyLocationsCondition', 'COMPANY_LOCATION'],
    ['locationsCondition', 'LOCATION'],
  ];
  const conditionEntries = possibleConditionEntries.filter(([key]) => result[key] !== undefined);

  if (conditionEntries.length > 1) {
    errors.push(
      marketError(
        ['input', 'conditions'],
        'The specified conditions are not compatible with each other',
        'INCOMPATIBLE_CONDITIONS',
      ),
    );
  }

  result['conditionTypes'] = conditionEntries.map(([, type]) => type);
  return result;
}

function marketTypeFromConditions(conditions: Record<string, unknown>): string {
  const conditionTypes = Array.isArray(conditions['conditionTypes']) ? conditions['conditionTypes'] : [];
  const [firstType] = conditionTypes;
  return typeof firstType === 'string' ? firstType : 'NONE';
}

function buildCurrencySettings(
  input: Record<string, unknown>,
  existing: Record<string, unknown> | null,
  conditions: Record<string, unknown>,
  errors: MarketUserError[],
): Record<string, unknown> | null {
  if (input['removeCurrencySettings'] === true) {
    return null;
  }

  const rawCurrencySettings = readInput(input['currencySettings']);
  const existingCurrencySettings = isPlainObject(existing?.['currencySettings'])
    ? (existing['currencySettings'] as Record<string, unknown>)
    : {};
  const regionsCondition = isPlainObject(conditions['regionsCondition'])
    ? (conditions['regionsCondition'] as Record<string, unknown>)
    : null;
  const regionEdges =
    regionsCondition &&
    isPlainObject(regionsCondition['regions']) &&
    Array.isArray(regionsCondition['regions']['edges'])
      ? regionsCondition['regions']['edges']
      : [];
  const firstRegionCurrency =
    isPlainObject(regionEdges[0]) &&
    isPlainObject(regionEdges[0]['node']) &&
    isPlainObject(regionEdges[0]['node']['currency']) &&
    typeof regionEdges[0]['node']['currency']['currencyCode'] === 'string'
      ? regionEdges[0]['node']['currency']['currencyCode']
      : null;
  const previousBaseCurrency =
    isPlainObject(existingCurrencySettings['baseCurrency']) &&
    typeof existingCurrencySettings['baseCurrency']['currencyCode'] === 'string'
      ? existingCurrencySettings['baseCurrency']['currencyCode']
      : null;
  const requestedCurrency =
    typeof rawCurrencySettings['baseCurrency'] === 'string'
      ? rawCurrencySettings['baseCurrency'].toUpperCase()
      : (previousBaseCurrency ?? firstRegionCurrency ?? 'USD');

  if (!CURRENCY_NAMES[requestedCurrency]) {
    errors.push(
      marketError(
        ['input', 'currencySettings', 'baseCurrency'],
        'The specified currency is not supported',
        'UNSUPPORTED_CURRENCY',
      ),
    );
  }

  return {
    baseCurrency: currencySetting(requestedCurrency),
    localCurrencies:
      typeof rawCurrencySettings['localCurrencies'] === 'boolean'
        ? rawCurrencySettings['localCurrencies']
        : typeof existingCurrencySettings['localCurrencies'] === 'boolean'
          ? existingCurrencySettings['localCurrencies']
          : false,
    roundingEnabled:
      typeof rawCurrencySettings['roundingEnabled'] === 'boolean'
        ? rawCurrencySettings['roundingEnabled']
        : typeof existingCurrencySettings['roundingEnabled'] === 'boolean'
          ? existingCurrencySettings['roundingEnabled']
          : true,
  };
}

function buildPriceInclusions(input: Record<string, unknown>, existing: Record<string, unknown> | null): unknown {
  if (input['removePriceInclusions'] === true) {
    return null;
  }

  if (!isPlainObject(input['priceInclusions'])) {
    return existing?.['priceInclusions'] ?? null;
  }

  const priceInclusions = input['priceInclusions'];
  return {
    inclusiveDutiesPricingStrategy:
      typeof priceInclusions['dutiesPricingStrategy'] === 'string'
        ? priceInclusions['dutiesPricingStrategy']
        : 'ADD_DUTIES_AT_CHECKOUT',
    inclusiveTaxPricingStrategy:
      typeof priceInclusions['taxPricingStrategy'] === 'string'
        ? priceInclusions['taxPricingStrategy']
        : 'ADD_TAXES_AT_CHECKOUT',
  };
}

function addIdsToConnection(existing: unknown, ids: string[], typeName: string): Record<string, unknown> {
  const edges = readConnectionEdges(existing);
  const knownIds = new Set(
    edges.flatMap((edge) => (isPlainObject(edge.node) && typeof edge.node['id'] === 'string' ? [edge.node['id']] : [])),
  );
  const nodes = edges.map((edge) => edge.node);
  for (const id of ids) {
    if (knownIds.has(id)) {
      continue;
    }
    nodes.push({ __typename: typeName, id });
  }
  return connectionFromNodes(nodes);
}

function addWebPresenceIdsToConnection(existing: unknown, ids: string[]): Record<string, unknown> {
  const edges = readConnectionEdges(existing);
  const knownIds = new Set(
    edges.flatMap((edge) => (isPlainObject(edge.node) && typeof edge.node['id'] === 'string' ? [edge.node['id']] : [])),
  );
  const nodes = edges.map((edge) => {
    if (!isPlainObject(edge.node) || typeof edge.node['id'] !== 'string') {
      return edge.node;
    }

    return store.getEffectiveWebPresenceById(edge.node['id']) ?? edge.node;
  });

  for (const id of ids) {
    if (knownIds.has(id)) {
      continue;
    }

    nodes.push(store.getEffectiveWebPresenceById(id) ?? { __typename: 'MarketWebPresence', id });
  }

  return connectionFromNodes(nodes);
}

function removeIdsFromConnection(existing: unknown, ids: string[]): Record<string, unknown> {
  const deletedIds = new Set(ids);
  const nodes = readConnectionEdges(existing)
    .map((edge) => edge.node)
    .filter((node) => !(isPlainObject(node) && typeof node['id'] === 'string' && deletedIds.has(node['id'])));
  return connectionFromNodes(nodes);
}

function buildMarketRecord(
  id: string,
  input: Record<string, unknown>,
  existingMarket: MarketRecord | null,
  errors: MarketUserError[],
): MarketRecord {
  const existing = existingMarket?.data ?? null;
  const handleResolution = normalizeMarketHandle(
    { name: existing?.['name'] ?? input['name'], ...input },
    existingMarket?.id,
  );
  errors.push(...handleResolution.errors);

  const statusResolution = readStatusAndEnabled(input, existing ?? undefined);
  errors.push(...statusResolution.errors);

  const now = makeSyntheticTimestamp();
  const conditions =
    input['conditions'] !== undefined
      ? buildConditions(input['conditions'], existing, errors)
      : isPlainObject(existing?.['conditions'])
        ? structuredClone(existing['conditions'] as Record<string, unknown>)
        : buildConditions({}, null, errors);
  const data: Record<string, unknown> = {
    ...(existing ? structuredClone(existing) : {}),
    id,
    name:
      typeof input['name'] === 'string'
        ? input['name']
        : typeof existing?.['name'] === 'string'
          ? existing['name']
          : '',
    handle: handleResolution.handle,
    status: statusResolution.status,
    enabled: statusResolution.enabled,
    type:
      input['conditions'] === undefined && typeof existing?.['type'] === 'string'
        ? existing['type']
        : marketTypeFromConditions(conditions),
    conditions,
    currencySettings: buildCurrencySettings(input, existing, conditions, errors),
    priceInclusions: buildPriceInclusions(input, existing),
    catalogs:
      input['catalogsToDelete'] !== undefined
        ? removeIdsFromConnection(existing?.['catalogs'], readStringArray(input['catalogsToDelete']))
        : addIdsToConnection(
            existing?.['catalogs'],
            readStringArray(input['catalogs'] ?? input['catalogsToAdd']),
            'MarketCatalog',
          ),
    webPresences:
      input['webPresencesToDelete'] !== undefined
        ? removeIdsFromConnection(existing?.['webPresences'], readStringArray(input['webPresencesToDelete']))
        : addWebPresenceIdsToConnection(
            existing?.['webPresences'],
            readStringArray(input['webPresences'] ?? input['webPresencesToAdd']),
          ),
    createdAt: typeof existing?.['createdAt'] === 'string' ? existing['createdAt'] : now,
    updatedAt: now,
  };

  return {
    id,
    cursor: existingMarket?.cursor ?? id,
    data: data as Record<string, JsonValue>,
  };
}

function selectedMarketPayload(market: MarketRecord | null): unknown {
  return market ? market.data : null;
}

function marketSummaryForWebPresence(market: MarketRecord): Record<string, unknown> {
  return {
    __typename: 'Market',
    id: market.id,
    name: market.data['name'] ?? null,
    handle: market.data['handle'] ?? null,
    status: market.data['status'] ?? null,
    type: market.data['type'] ?? null,
  };
}

function syncWebPresenceMarketLinks(market: MarketRecord): void {
  const edges = readConnectionEdges(market.data['webPresences']);
  const marketSummary = marketSummaryForWebPresence(market);
  for (const edge of edges) {
    if (!isPlainObject(edge.node) || typeof edge.node['id'] !== 'string') {
      continue;
    }

    const existing = store.getEffectiveWebPresenceRecordById(edge.node['id']);
    if (!existing) {
      continue;
    }

    store.stageUpdateWebPresence({
      ...existing,
      data: {
        ...structuredClone(existing.data),
        markets: connectionFromNodes([
          ...readConnectionEdges(existing.data['markets'])
            .map((marketEdge) => marketEdge.node)
            .filter((node) => !(isPlainObject(node) && typeof node['id'] === 'string' && node['id'] === market.id)),
          marketSummary,
        ]) as JsonValue,
      },
    });
  }
}

function syncMarketWebPresenceNodes(webPresence: WebPresenceRecord): void {
  for (const market of store.listEffectiveMarkets()) {
    const edges = readConnectionEdges(market.data['webPresences']);
    if (
      !edges.some(
        (edge) => isPlainObject(edge.node) && typeof edge.node['id'] === 'string' && edge.node['id'] === webPresence.id,
      )
    ) {
      continue;
    }

    store.stageUpdateMarket({
      ...market,
      data: {
        ...structuredClone(market.data),
        webPresences: connectionFromNodes(
          edges.map((edge) =>
            isPlainObject(edge.node) && typeof edge.node['id'] === 'string' && edge.node['id'] === webPresence.id
              ? webPresence.data
              : edge.node,
          ),
        ) as JsonValue,
      },
    });
  }
}

function normalizeLocale(rawLocale: unknown): string | null {
  if (typeof rawLocale !== 'string') {
    return null;
  }

  const locale = rawLocale.trim();
  return locale.length > 0 ? locale : null;
}

function isValidLocale(locale: string): boolean {
  return /^[a-z]{2}(?:-[A-Z]{2})?$/u.test(locale);
}

function invalidLocaleMessage(locale: string, label = 'locale codes'): string {
  return `Invalid ${label}: ${locale}`;
}

function localePayload(locale: string, primary: boolean): Record<string, unknown> {
  const language = locale.split('-')[0] ?? locale;
  return {
    locale,
    name: LOCALE_NAMES[language] ?? locale,
    primary,
    published: true,
  };
}

function normalizeAlternateLocales(rawLocales: unknown, defaultLocale: string, errors: MarketUserError[]): string[] {
  if (rawLocales === undefined || rawLocales === null) {
    return [];
  }

  if (!Array.isArray(rawLocales)) {
    errors.push(marketError(['input', 'alternateLocales'], 'Alternate locales must be an array', 'INVALID'));
    return [];
  }

  const seen = new Set<string>();
  const locales: string[] = [];
  for (const rawLocale of rawLocales) {
    const locale = normalizeLocale(rawLocale);
    if (!locale || !isValidLocale(locale)) {
      errors.push(
        marketError(['input', 'alternateLocales'], invalidLocaleMessage(locale ?? String(rawLocale)), 'INVALID'),
      );
      continue;
    }

    if (locale === defaultLocale) {
      errors.push(
        marketError(['input', 'alternateLocales'], "Alternate locales can't include the default locale", 'INVALID'),
      );
      continue;
    }

    if (seen.has(locale)) {
      errors.push(marketError(['input', 'alternateLocales'], 'Alternate locales must be unique', 'TAKEN'));
      continue;
    }

    seen.add(locale);
    locales.push(locale);
  }

  return locales;
}

function normalizeSubfolderSuffix(rawSuffix: unknown, errors: MarketUserError[]): string | null {
  if (rawSuffix === undefined || rawSuffix === null) {
    return null;
  }

  if (typeof rawSuffix !== 'string' || rawSuffix.trim() === '') {
    errors.push(marketError(['input', 'subfolderSuffix'], "Subfolder suffix can't be blank", 'BLANK'));
    return null;
  }

  const suffix = rawSuffix.trim().toLowerCase();
  const isAscii = [...suffix].every((character) => character.charCodeAt(0) <= 0x7f);
  if (!isAscii || !/^[a-z0-9][a-z0-9-]*$/u.test(suffix)) {
    errors.push(marketError(['input', 'subfolderSuffix'], 'Subfolder suffix is invalid', 'INVALID'));
    return suffix;
  }

  return suffix;
}

function domainIdFromInput(input: Record<string, unknown>): string | null {
  return typeof input['domainId'] === 'string' && input['domainId'].trim() ? input['domainId'].trim() : null;
}

function domainIdExists(domainId: string): boolean {
  return store.listEffectiveWebPresences().some((webPresence) => {
    const domain = webPresence.data['domain'];
    return isPlainObject(domain) && domain['id'] === domainId;
  });
}

function webPresenceDomainFromId(domainId: string | null): Record<string, unknown> | null {
  if (!domainId) {
    return null;
  }

  const tail = domainId.split('/').at(-1) ?? 'domain';
  const host = `domain-${tail.toLowerCase()}.example.com`;
  return {
    id: domainId,
    host,
    url: `https://${host}`,
    sslEnabled: true,
  };
}

function primaryWebPresenceBaseUrl(existing?: Record<string, unknown> | null): string {
  const existingDomain = isPlainObject(existing?.['domain']) ? existing['domain'] : null;
  if (isPlainObject(existingDomain) && typeof existingDomain['url'] === 'string') {
    return existingDomain['url'].replace(/\/$/u, '');
  }

  const shop = store.getEffectiveShop();
  if (shop?.url) {
    return shop.url.replace(/\/$/u, '');
  }

  const capturedDomain = store
    .listEffectiveWebPresences()
    .map((webPresence) => webPresence.data['domain'])
    .find((domain): domain is Record<string, JsonValue> => isPlainObject(domain) && typeof domain['url'] === 'string');
  if (capturedDomain && typeof capturedDomain['url'] === 'string') {
    return capturedDomain['url'].replace(/\/$/u, '');
  }

  return 'https://example.myshopify.com';
}

function buildRootUrls(
  defaultLocale: string,
  alternateLocales: string[],
  subfolderSuffix: string | null,
  domain: Record<string, unknown> | null,
  existing?: Record<string, unknown> | null,
): Array<Record<string, unknown>> {
  const baseUrl =
    domain && typeof domain['url'] === 'string'
      ? domain['url'].replace(/\/$/u, '')
      : primaryWebPresenceBaseUrl(existing);
  return [defaultLocale, ...alternateLocales].map((locale, index) => ({
    locale,
    url: subfolderSuffix
      ? `${baseUrl}/${locale}-${subfolderSuffix}`
      : index === 0
        ? `${baseUrl}/`
        : `${baseUrl}/${locale}`,
  }));
}

function webPresenceIdentifierInUse(
  input: { domainId: string | null; subfolderSuffix: string | null },
  excludedWebPresenceId?: string,
): MarketUserError[] {
  const errors: MarketUserError[] = [];
  for (const webPresence of store.listEffectiveWebPresences()) {
    if (webPresence.id === excludedWebPresenceId) {
      continue;
    }

    const domain = isPlainObject(webPresence.data['domain']) ? webPresence.data['domain'] : null;
    if (input.domainId && domain && domain['id'] === input.domainId) {
      errors.push(marketError(['input', 'domainId'], 'Domain has already been taken', 'TAKEN'));
    }

    if (input.subfolderSuffix && webPresence.data['subfolderSuffix'] === input.subfolderSuffix) {
      errors.push(marketError(['input', 'subfolderSuffix'], 'Subfolder suffix has already been taken', 'TAKEN'));
    }
  }

  return errors;
}

function buildWebPresenceRecord(
  id: string,
  input: Record<string, unknown>,
  existingWebPresence: WebPresenceRecord | null,
  errors: MarketUserError[],
): WebPresenceRecord {
  const existing = existingWebPresence?.data ?? null;
  const existingDefaultLocale =
    isPlainObject(existing?.['defaultLocale']) && typeof existing['defaultLocale']['locale'] === 'string'
      ? existing['defaultLocale']['locale']
      : null;
  const domainId = domainIdFromInput(input);
  const domainExists = domainId ? domainIdExists(domainId) : false;
  if (domainId && !domainExists) {
    errors.push(marketError(['input', 'domainId'], 'Domain does not exist', 'DOMAIN_NOT_FOUND'));
  }

  const rawDefaultLocale = normalizeLocale(input['defaultLocale']);
  const defaultLocale = rawDefaultLocale ?? existingDefaultLocale ?? '';
  if (!defaultLocale) {
    errors.push(marketError(['input', 'defaultLocale'], "Default locale can't be blank", 'BLANK'));
  } else if (!isValidLocale(defaultLocale)) {
    errors.push(marketError(['input', 'defaultLocale'], invalidLocaleMessage(defaultLocale), 'INVALID'));
  }

  const defaultLocaleIsUsable = !!defaultLocale && isValidLocale(defaultLocale);
  const alternateLocales = !defaultLocaleIsUsable
    ? []
    : input['alternateLocales'] === undefined && Array.isArray(existing?.['alternateLocales'])
      ? (existing['alternateLocales'] as unknown[]).flatMap((locale) =>
          isPlainObject(locale) && typeof locale['locale'] === 'string' ? [locale['locale']] : [],
        )
      : normalizeAlternateLocales(input['alternateLocales'], defaultLocale, errors);
  const subfolderSuffix =
    domainId && !domainExists
      ? null
      : input['subfolderSuffix'] === undefined
        ? typeof existing?.['subfolderSuffix'] === 'string'
          ? existing['subfolderSuffix']
          : null
        : normalizeSubfolderSuffix(input['subfolderSuffix'], errors);

  if (domainId && domainExists && subfolderSuffix) {
    errors.push(
      marketError(
        ['input', 'domainId'],
        "Domain ID must be null when subfolder suffix isn't null",
        'DOMAIN_AND_SUBFOLDER_MUTUALLY_EXCLUSIVE',
      ),
    );
  }

  if (input['subfolderSuffix'] !== undefined && existing && existing['subfolderSuffix'] === null) {
    errors.push(
      marketError(
        ['input', 'subfolderSuffix'],
        'Subfolder suffix can only be updated for a subfolder web presence',
        'INVALID',
      ),
    );
  }

  if (!domainId || domainExists) {
    errors.push(...webPresenceIdentifierInUse({ domainId, subfolderSuffix }, existingWebPresence?.id));
  }

  const now = makeSyntheticTimestamp();
  const domain = domainId
    ? webPresenceDomainFromId(domainId)
    : isPlainObject(existing?.['domain'])
      ? existing['domain']
      : null;
  const markets = existing?.['markets'] ?? emptyConnection();
  const data: Record<string, unknown> = {
    ...(existing ? structuredClone(existing) : {}),
    __typename: 'MarketWebPresence',
    id,
    subfolderSuffix,
    domain: subfolderSuffix ? null : domain,
    rootUrls: defaultLocale ? buildRootUrls(defaultLocale, alternateLocales, subfolderSuffix, domain, existing) : [],
    defaultLocale: defaultLocale ? localePayload(defaultLocale, true) : null,
    alternateLocales: alternateLocales.map((locale) => localePayload(locale, false)),
    markets,
    createdAt: typeof existing?.['createdAt'] === 'string' ? existing['createdAt'] : now,
    updatedAt: now,
  };

  return {
    id,
    cursor: existingWebPresence?.cursor ?? id,
    data: data as Record<string, JsonValue>,
  };
}

function selectedWebPresencePayload(webPresence: WebPresenceRecord | null): unknown {
  return webPresence ? webPresence.data : null;
}

function projectMutationPayload(
  payload: Record<string, unknown>,
  field: FieldNode,
  fragments: FragmentMap,
  variables: Record<string, unknown>,
): unknown {
  return field.selectionSet ? projectValue(payload, field.selectionSet.selections, fragments, variables) : payload;
}

function handleMarketCreate(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);
  const input = readInput(args['input']);
  const errors: MarketUserError[] = [];

  if (typeof input['name'] !== 'string' || input['name'].trim() === '') {
    errors.push(marketError(['input', 'name'], "Name can't be blank", 'BLANK'));
    errors.push(marketError(['input', 'name'], 'Name is too short (minimum is 2 characters)', 'TOO_SHORT'));
  } else if (input['name'].trim().length < 2) {
    errors.push(marketError(['input', 'name'], 'Name is too short (minimum is 2 characters)', 'TOO_SHORT'));
  }

  const market = buildMarketRecord(makeSyntheticGid('Market'), input, null, errors);
  if (errors.length === 0) {
    store.stageCreateMarket(market);
    syncWebPresenceMarketLinks(market);
  }

  return projectMutationPayload(
    {
      market: errors.length === 0 ? selectedMarketPayload(market) : null,
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handleMarketUpdate(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const input = readInput(args['input']);
  const errors: MarketUserError[] = [];

  if (!id) {
    errors.push(marketError(['id'], 'Market does not exist', 'MARKET_NOT_FOUND'));
  }

  const existingMarket = id ? store.getEffectiveMarketRecordById(id) : null;
  if (id && !existingMarket) {
    errors.push(marketError(['id'], 'Market does not exist', 'MARKET_NOT_FOUND'));
  }

  const market = id && existingMarket ? buildMarketRecord(id, input, existingMarket, errors) : null;
  if (errors.length === 0 && market) {
    store.stageUpdateMarket(market);
    syncWebPresenceMarketLinks(market);
  }

  return projectMutationPayload(
    {
      market: errors.length === 0 ? selectedMarketPayload(market) : null,
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function countActiveRegionMarkets(excludedMarketId?: string): number {
  return store
    .listEffectiveMarkets()
    .filter(
      (market) =>
        market.id !== excludedMarketId && market.data['type'] === 'REGION' && market.data['status'] === 'ACTIVE',
    ).length;
}

function handleMarketDelete(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const errors: MarketUserError[] = [];
  const existingMarket = id ? store.getEffectiveMarketRecordById(id) : null;

  if (!id || !existingMarket) {
    errors.push(marketError(['id'], 'Market does not exist', 'MARKET_NOT_FOUND'));
  } else if (existingMarket.data['primary'] === true) {
    errors.push(marketError(['id'], "Can't delete the primary market", 'CANNOT_DELETE_PRIMARY_MARKET'));
  } else if (
    existingMarket.data['type'] === 'REGION' &&
    existingMarket.data['status'] === 'ACTIVE' &&
    countActiveRegionMarkets(id) === 0
  ) {
    errors.push(
      marketError(
        ['id'],
        "Can't delete, disable, or change the type of the last region market",
        'MUST_HAVE_AT_LEAST_ONE_ACTIVE_REGION_MARKET',
      ),
    );
  }

  if (errors.length === 0 && id) {
    store.stageDeleteMarket(id);
  }

  return projectMutationPayload(
    {
      deletedId: errors.length === 0 ? id : null,
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handleWebPresenceCreate(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const args = getFieldArguments(field, variables);
  const input = readInput(args['input']);
  const errors: MarketUserError[] = [];
  const webPresence = buildWebPresenceRecord(makeSyntheticGid('MarketWebPresence'), input, null, errors);

  if (errors.length === 0) {
    store.stageCreateWebPresence(webPresence);
  }

  return projectMutationPayload(
    {
      webPresence: errors.length === 0 ? selectedWebPresencePayload(webPresence) : null,
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handleWebPresenceUpdate(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const input = readInput(args['input']);
  const errors: MarketUserError[] = [];

  if (!id) {
    errors.push(marketError(['id'], "The market web presence wasn't found.", 'WEB_PRESENCE_NOT_FOUND'));
  }

  const existingWebPresence = id ? store.getEffectiveWebPresenceRecordById(id) : null;
  if (id && !existingWebPresence) {
    errors.push(marketError(['id'], "The market web presence wasn't found.", 'WEB_PRESENCE_NOT_FOUND'));
  }

  const webPresence = id && existingWebPresence ? buildWebPresenceRecord(id, input, existingWebPresence, errors) : null;
  if (errors.length === 0 && webPresence) {
    store.stageUpdateWebPresence(webPresence);
    syncMarketWebPresenceNodes(webPresence);
  }

  return projectMutationPayload(
    {
      webPresence: errors.length === 0 ? selectedWebPresencePayload(webPresence) : null,
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function validateMarketLocalizationResource(resourceId: unknown): {
  resource: MarketLocalizableResourceRecord | null;
  errors: MarketUserError[];
} {
  if (typeof resourceId !== 'string' || !resourceId) {
    return {
      resource: null,
      errors: [translationError(['resourceId'], 'Resource does not exist', 'RESOURCE_NOT_FOUND')],
    };
  }

  const resource = readMarketLocalizableResource(resourceId);
  if (!resource) {
    return {
      resource: null,
      errors: [translationError(['resourceId'], `Resource ${resourceId} does not exist`, 'RESOURCE_NOT_FOUND')],
    };
  }

  return { resource, errors: [] };
}

function validateMarketLocalizationKey(
  resource: MarketLocalizableResourceRecord,
  rawKey: unknown,
  fieldPrefix: string[],
): { key: string | null; contentDigest: string | null; errors: MarketUserError[] } {
  const key = typeof rawKey === 'string' ? rawKey : '';
  const content = resource.content.find((entry) => entry.key === key) ?? null;
  if (!content) {
    return {
      key: key || null,
      contentDigest: null,
      errors: [
        translationError(
          fieldPrefix,
          `Key ${key || String(rawKey)} is not market localizable for this resource`,
          'INVALID_KEY_FOR_MODEL',
        ),
      ],
    };
  }

  return { key, contentDigest: content.digest, errors: [] };
}

function validateMarketId(
  rawMarketId: unknown,
  fieldPrefix: string[],
): { marketId: string | null; errors: MarketUserError[] } {
  const marketId = typeof rawMarketId === 'string' ? rawMarketId : '';
  if (!marketId || !store.getEffectiveMarketRecordById(marketId)) {
    return {
      marketId: marketId || null,
      errors: [
        translationError(
          fieldPrefix,
          `Market ${marketId || String(rawMarketId)} does not exist`,
          'MARKET_DOES_NOT_EXIST',
        ),
      ],
    };
  }

  return { marketId, errors: [] };
}

function projectMarketLocalizationMutationPayload(
  payload: Record<string, unknown>,
  field: FieldNode,
  fragments: FragmentMap,
  variables: Record<string, unknown>,
): unknown {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'marketLocalizations':
        result[key] = Array.isArray(payload['marketLocalizations'])
          ? payload['marketLocalizations'].map((localization) =>
              serializeMarketLocalization(
                localization as MarketLocalizationRecord,
                selection.selectionSet?.selections ?? [],
                fragments,
                variables,
              ),
            )
          : null;
        break;
      case 'userErrors':
        result[key] = projectValue(
          payload['userErrors'],
          selection.selectionSet?.selections ?? [],
          fragments,
          variables,
        );
        break;
      default:
        result[key] = payload[selection.name.value] ?? null;
    }
  }
  return result;
}

function handleMarketLocalizationsRegister(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const args = getFieldArguments(field, variables);
  const resourceValidation = validateMarketLocalizationResource(args['resourceId']);
  const errors = [...resourceValidation.errors];
  const inputs = Array.isArray(args['marketLocalizations'])
    ? args['marketLocalizations'].filter((input): input is Record<string, unknown> => isPlainObject(input))
    : [];

  if (inputs.length === 0) {
    errors.push(translationError(['marketLocalizations'], 'At least one market localization is required', 'BLANK'));
  }

  const localizations: MarketLocalizationRecord[] = [];
  const resource = resourceValidation.resource;
  if (resource) {
    inputs.forEach((input, index) => {
      const indexPath = ['marketLocalizations', String(index)];
      const marketValidation = validateMarketId(input['marketId'], [...indexPath, 'marketId']);
      const keyValidation = validateMarketLocalizationKey(resource, input['key'], [...indexPath, 'key']);
      errors.push(...marketValidation.errors, ...keyValidation.errors);

      if (typeof input['value'] !== 'string' || input['value'] === '') {
        errors.push(translationError([...indexPath, 'value'], "Value can't be blank", 'BLANK'));
      }

      if (
        keyValidation.contentDigest !== null &&
        input['marketLocalizableContentDigest'] !== keyValidation.contentDigest
      ) {
        errors.push(
          translationError(
            [...indexPath, 'marketLocalizableContentDigest'],
            'Market localizable content digest does not match the resource content',
            'INVALID_MARKET_LOCALIZABLE_CONTENT',
          ),
        );
      }

      if (errors.length === 0 && marketValidation.marketId && keyValidation.key && typeof input['value'] === 'string') {
        localizations.push({
          resourceId: resource.resourceId,
          marketId: marketValidation.marketId,
          key: keyValidation.key,
          value: input['value'],
          updatedAt: makeSyntheticTimestamp(),
          outdated: false,
        });
      }
    });
  }

  if (errors.length === 0) {
    for (const localization of localizations) {
      store.stageMarketLocalization(localization);
    }
  }

  return projectMarketLocalizationMutationPayload(
    {
      marketLocalizations: errors.length === 0 ? localizations : null,
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handleMarketLocalizationsRemove(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const args = getFieldArguments(field, variables);
  const resourceValidation = validateMarketLocalizationResource(args['resourceId']);
  const errors = [...resourceValidation.errors];
  const rawKeys = Array.isArray(args['marketLocalizationKeys']) ? args['marketLocalizationKeys'] : [];
  const rawMarketIds = Array.isArray(args['marketIds']) ? args['marketIds'] : [];

  if (rawKeys.length === 0) {
    errors.push(
      translationError(['marketLocalizationKeys'], 'At least one market localization key is required', 'BLANK'),
    );
  }
  if (rawMarketIds.length === 0) {
    errors.push(translationError(['marketIds'], 'At least one market ID is required', 'BLANK'));
  }

  const keys: string[] = [];
  const marketIds: string[] = [];
  const resource = resourceValidation.resource;
  if (resource) {
    rawKeys.forEach((rawKey, index) => {
      const keyValidation = validateMarketLocalizationKey(resource, rawKey, ['marketLocalizationKeys', String(index)]);
      errors.push(...keyValidation.errors);
      if (keyValidation.key) {
        keys.push(keyValidation.key);
      }
    });

    rawMarketIds.forEach((rawMarketId, index) => {
      const marketValidation = validateMarketId(rawMarketId, ['marketIds', String(index)]);
      errors.push(...marketValidation.errors);
      if (marketValidation.marketId) {
        marketIds.push(marketValidation.marketId);
      }
    });
  }

  const removedLocalizations: MarketLocalizationRecord[] = [];
  if (errors.length === 0 && resource) {
    for (const marketId of marketIds) {
      for (const key of keys) {
        const removed = store.removeMarketLocalization(resource.resourceId, marketId, key);
        if (removed) {
          removedLocalizations.push(removed);
        }
      }
    }
  }

  return projectMarketLocalizationMutationPayload(
    {
      marketLocalizations: errors.length === 0 ? removedLocalizations : null,
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function listMarketsForConnection(field: FieldNode, variables: Record<string, unknown>): MarketRecord[] {
  const args = getFieldArguments(field, variables);
  const filteredMarkets = applyMarketsQuery(applyRootMarketFilters(store.listEffectiveMarkets(), args), args['query']);
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
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = window.items.map((market) =>
          projectValue(market.data, selection.selectionSet?.selections ?? [], fragments, variables),
        );
        break;
      case 'edges':
        result[key] = window.items.map((market) =>
          projectEdge({ cursor: marketCursor(market), node: market.data }, selection, fragments, variables),
        );
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(
          selection,
          window.items,
          window.hasNextPage,
          window.hasPreviousPage,
          marketCursor,
          { prefixCursors: false },
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function webPresenceCursor(webPresence: WebPresenceRecord): string {
  return webPresence.cursor ?? webPresence.id;
}

function serializeWebPresencesConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const webPresences = store.listEffectiveWebPresences();
  const args = getFieldArguments(field, variables);
  const sortedWebPresences = args['reverse'] === true ? [...webPresences].reverse() : webPresences;
  const window = paginateConnectionItems(sortedWebPresences, field, variables, webPresenceCursor);
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = window.items.map((webPresence) =>
          projectValue(webPresence.data, selection.selectionSet?.selections ?? [], fragments, variables),
        );
        break;
      case 'edges':
        result[key] = window.items.map((webPresence) =>
          projectEdge(
            { cursor: webPresenceCursor(webPresence), node: webPresence.data },
            selection,
            fragments,
            variables,
          ),
        );
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(
          selection,
          window.items,
          window.hasNextPage,
          window.hasPreviousPage,
          webPresenceCursor,
          { prefixCursors: false },
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function overlayMarketsResolvedValuesWebPresences(rootPayload: unknown): unknown {
  if (!isPlainObject(rootPayload)) {
    return rootPayload;
  }

  const webPresences = store.listEffectiveWebPresences();
  if (webPresences.length === 0) {
    return rootPayload;
  }

  return {
    ...structuredClone(rootPayload),
    webPresences: {
      edges: webPresences.map((webPresence) => ({
        cursor: webPresenceCursor(webPresence),
        node: webPresence.data,
      })),
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: webPresenceCursor(webPresences[0] as WebPresenceRecord),
        endCursor: webPresenceCursor(webPresences.at(-1) as WebPresenceRecord),
      },
    },
  };
}

function rootPayloadForField(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  switch (field.name.value) {
    case 'market': {
      const args = getFieldArguments(field, variables);
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      return id ? store.getEffectiveMarketById(id) : null;
    }
    case 'markets':
      return serializeMarketsConnection(field, variables, fragments);
    case 'marketLocalizableResource': {
      const args = getFieldArguments(field, variables);
      const resourceId = typeof args['resourceId'] === 'string' ? args['resourceId'] : null;
      return resourceId
        ? serializeMarketLocalizableResource(
            readMarketLocalizableResource(resourceId),
            field.selectionSet?.selections ?? [],
            fragments,
            variables,
          )
        : null;
    }
    case 'marketLocalizableResources':
      return serializeMarketLocalizableResourcesConnection(
        listMarketLocalizableResources(field, variables),
        field,
        variables,
        fragments,
      );
    case 'marketLocalizableResourcesByIds':
      return serializeMarketLocalizableResourcesConnection(
        listMarketLocalizableResourcesByIds(field, variables),
        field,
        variables,
        fragments,
      );
    case 'catalog': {
      const args = getFieldArguments(field, variables);
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      return id ? store.getBaseCatalogById(id) : null;
    }
    case 'catalogs':
      return serializeCatalogsConnection(field, variables, fragments);
    case 'catalogsCount':
      return serializeCatalogsCount(field, variables);
    case 'priceList': {
      const args = getFieldArguments(field, variables);
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      return id ? store.getBasePriceListById(id) : null;
    }
    case 'priceLists':
      return serializePriceListsConnection(field, variables, fragments);
    case 'webPresences':
      return serializeWebPresencesConnection(field, variables, fragments);
    case 'marketsResolvedValues':
      return overlayMarketsResolvedValuesWebPresences(store.getBaseMarketsRootPayload(field.name.value));
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
      field.name.value === 'markets' ||
      field.name.value === 'marketLocalizableResource' ||
      field.name.value === 'marketLocalizableResources' ||
      field.name.value === 'marketLocalizableResourcesByIds' ||
      field.name.value === 'catalogs' ||
      field.name.value === 'catalogsCount' ||
      field.name.value === 'priceLists' ||
      field.name.value === 'webPresences'
        ? rootPayload
        : field.selectionSet
          ? projectValue(rootPayload, field.selectionSet.selections, fragments, variables)
          : rootPayload;
  }

  return { data };
}

export function handleMarketMutation(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  const fragments = getFragments(document);

  for (const field of getRootFields(document)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'marketCreate':
        data[key] = handleMarketCreate(field, variables, fragments);
        break;
      case 'marketUpdate':
        data[key] = handleMarketUpdate(field, variables, fragments);
        break;
      case 'marketDelete':
        data[key] = handleMarketDelete(field, variables, fragments);
        break;
      case 'webPresenceCreate':
        data[key] = handleWebPresenceCreate(field, variables, fragments);
        break;
      case 'webPresenceUpdate':
        data[key] = handleWebPresenceUpdate(field, variables, fragments);
        break;
      case 'marketLocalizationsRegister':
        data[key] = handleMarketLocalizationsRegister(field, variables, fragments);
        break;
      case 'marketLocalizationsRemove':
        data[key] = handleMarketLocalizationsRemove(field, variables, fragments);
        break;
      default:
        data[key] = null;
        break;
    }
  }

  return { data };
}

export function seedMarketsFromCapture(capture: unknown): boolean {
  const roots = [
    'markets',
    'market',
    'catalog',
    'catalogs',
    'catalogsCount',
    'priceList',
    'priceLists',
    'webPresences',
    'marketsResolvedValues',
    'marketLocalizableResource',
    'marketLocalizableResources',
    'marketLocalizableResourcesByIds',
  ];
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
