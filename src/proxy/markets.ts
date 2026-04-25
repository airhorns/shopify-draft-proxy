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
import type { MarketRecord } from '../state/types.js';

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
        result[key] = serializeConnectionPageInfo(
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
    type: marketTypeFromConditions(conditions),
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
        : addIdsToConnection(
            existing?.['webPresences'],
            readStringArray(input['webPresences'] ?? input['webPresencesToAdd']),
            'MarketWebPresence',
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

function rootPayloadForField(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  switch (field.name.value) {
    case 'market': {
      const args = getFieldArguments(field, variables);
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      return id ? store.getEffectiveMarketById(id) : null;
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
      default:
        data[key] = null;
        break;
    }
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
