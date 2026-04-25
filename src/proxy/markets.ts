import { Kind, parse, type FieldNode, type FragmentDefinitionNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { store } from '../state/store.js';

function responseKey(selection: FieldNode): string {
  return selection.alias?.value ?? selection.name.value;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

type FragmentMap = Map<string, FragmentDefinitionNode>;

function emptyConnection(): Record<string, unknown> {
  return {
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

    const value = source[fieldName];
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

function collectMarketNodes(value: unknown, markets: unknown[] = []): unknown[] {
  if (Array.isArray(value)) {
    for (const item of value) {
      collectMarketNodes(item, markets);
    }
    return markets;
  }

  if (!isPlainObject(value)) {
    return markets;
  }

  const id = value['id'];
  if (typeof id === 'string' && id.startsWith('gid://shopify/Market/')) {
    markets.push(value);
  }

  for (const child of Object.values(value)) {
    collectMarketNodes(child, markets);
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

function rootPayloadForField(field: FieldNode, variables: Record<string, unknown>): unknown {
  switch (field.name.value) {
    case 'market': {
      const args = getFieldArguments(field, variables);
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      return id ? store.getBaseMarketById(id) : null;
    }
    case 'markets':
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
    const rootPayload = rootPayloadForField(field, variables);
    data[key] = field.selectionSet ? projectValue(rootPayload, field.selectionSet.selections, fragments) : rootPayload;
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
