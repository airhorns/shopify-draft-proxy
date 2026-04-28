import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import {
  applySearchQuery,
  matchesSearchQueryString,
  searchQueryTermValue,
  stripSearchQueryValueQuotes,
  type SearchQueryTerm,
} from '../search-query-parser.js';
import {
  defaultGraphqlTypeConditionApplies,
  getDocumentFragments,
  getFieldResponseKey,
  getSelectedChildFields,
  isPlainObject,
  paginateConnectionItems,
  projectGraphqlValue,
  readGraphqlDataResponsePayload,
  serializeConnection,
  type FragmentMap,
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
  ProductRecord,
  ProductVariantRecord,
  WebPresenceRecord,
} from '../state/types.js';

function hasOwnProperty(value: object, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

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

const SHOPIFY_COUNTRY_CODES = new Set(
  [
    'AF',
    'AX',
    'AL',
    'DZ',
    'AD',
    'AO',
    'AI',
    'AG',
    'AR',
    'AM',
    'AW',
    'AC',
    'AU',
    'AT',
    'AZ',
    'BS',
    'BH',
    'BD',
    'BB',
    'BY',
    'BE',
    'BZ',
    'BJ',
    'BM',
    'BT',
    'BO',
    'BA',
    'BW',
    'BV',
    'BR',
    'IO',
    'BN',
    'BG',
    'BF',
    'BI',
    'KH',
    'CA',
    'CV',
    'BQ',
    'KY',
    'CF',
    'TD',
    'CL',
    'CN',
    'CX',
    'CC',
    'CO',
    'KM',
    'CG',
    'CD',
    'CK',
    'CR',
    'HR',
    'CU',
    'CW',
    'CY',
    'CZ',
    'CI',
    'DK',
    'DJ',
    'DM',
    'DO',
    'EC',
    'EG',
    'SV',
    'GQ',
    'ER',
    'EE',
    'SZ',
    'ET',
    'FK',
    'FO',
    'FJ',
    'FI',
    'FR',
    'GF',
    'PF',
    'TF',
    'GA',
    'GM',
    'GE',
    'DE',
    'GH',
    'GI',
    'GR',
    'GL',
    'GD',
    'GP',
    'GT',
    'GG',
    'GN',
    'GW',
    'GY',
    'HT',
    'HM',
    'VA',
    'HN',
    'HK',
    'HU',
    'IS',
    'IN',
    'ID',
    'IR',
    'IQ',
    'IE',
    'IM',
    'IL',
    'IT',
    'JM',
    'JP',
    'JE',
    'JO',
    'KZ',
    'KE',
    'KI',
    'KP',
    'XK',
    'KW',
    'KG',
    'LA',
    'LV',
    'LB',
    'LS',
    'LR',
    'LY',
    'LI',
    'LT',
    'LU',
    'MO',
    'MG',
    'MW',
    'MY',
    'MV',
    'ML',
    'MT',
    'MQ',
    'MR',
    'MU',
    'YT',
    'MX',
    'MD',
    'MC',
    'MN',
    'ME',
    'MS',
    'MA',
    'MZ',
    'MM',
    'NA',
    'NR',
    'NP',
    'NL',
    'AN',
    'NC',
    'NZ',
    'NI',
    'NE',
    'NG',
    'NU',
    'NF',
    'MK',
    'NO',
    'OM',
    'PK',
    'PS',
    'PA',
    'PG',
    'PY',
    'PE',
    'PH',
    'PN',
    'PL',
    'PT',
    'QA',
    'CM',
    'RE',
    'RO',
    'RU',
    'RW',
    'BL',
    'SH',
    'KN',
    'LC',
    'MF',
    'PM',
    'WS',
    'SM',
    'ST',
    'SA',
    'SN',
    'RS',
    'SC',
    'SL',
    'SG',
    'SX',
    'SK',
    'SI',
    'SB',
    'SO',
    'ZA',
    'GS',
    'KR',
    'SS',
    'ES',
    'LK',
    'VC',
    'SD',
    'SR',
    'SJ',
    'SE',
    'CH',
    'SY',
    'TW',
    'TJ',
    'TZ',
    'TH',
    'TL',
    'TG',
    'TK',
    'TO',
    'TT',
    'TA',
    'TN',
    'TR',
    'TM',
    'TC',
    'TV',
    'UG',
    'UA',
    'AE',
    'GB',
    'US',
    'UM',
    'UY',
    'UZ',
    'VU',
    'VE',
    'VN',
    'VG',
    'WF',
    'EH',
    'YE',
    'ZM',
    'ZW',
    'ZZ',
  ].sort(),
);

const SHOPIFY_COUNTRY_CODE_LIST = Array.from(SHOPIFY_COUNTRY_CODES).join(', ');

function marketsResolvedValuesPayloadKey(countryCode: string | null): string {
  return `marketsResolvedValues:${countryCode ?? '*'}`;
}

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

function connectionFromEdges(edges: ConnectionEdge[]): Record<string, unknown> {
  return {
    edges: edges.map((edge) => ({
      cursor: edge.cursor,
      node: edge.node,
    })),
    pageInfo: {
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: edges[0]?.cursor ?? null,
      endCursor: edges.at(-1)?.cursor ?? null,
    },
  };
}

function marketTypeConditionApplies(source: Record<string, unknown>, typeCondition: string | undefined): boolean {
  const sourceTypename = typeof source['__typename'] === 'string' ? source['__typename'] : null;
  return (
    defaultGraphqlTypeConditionApplies(source, typeCondition) ||
    (typeCondition === 'Catalog' && sourceTypename === 'MarketCatalog')
  );
}

function projectMarketValue(
  value: unknown,
  selections: readonly SelectionNode[],
  fragments: FragmentMap,
  variables: Record<string, unknown>,
): unknown {
  return projectGraphqlValue(value, selections, fragments, {
    shouldApplyTypeCondition: marketTypeConditionApplies,
    projectFieldValue: ({ source, field, fieldName }) => {
      if (
        fieldName === 'catalogs' &&
        typeof source['id'] === 'string' &&
        source['id'].startsWith('gid://shopify/Market/')
      ) {
        return {
          handled: true,
          value: projectConnectionPayload(
            catalogConnectionForMarket(source['id'], source['catalogs']),
            field,
            fragments,
            variables,
          ),
        };
      }

      if (source['__typename'] === 'MarketCatalog') {
        if (fieldName === 'marketsCount') {
          return {
            handled: true,
            value: serializeCountSelection(field, readConnectionEdges(source['markets']).length),
          };
        }
        if (fieldName === 'operations') {
          return {
            handled: true,
            value: projectMarketValue(
              source['operations'] ?? [],
              field.selectionSet?.selections ?? [],
              fragments,
              variables,
            ),
          };
        }
      }

      if (source['__typename'] === 'PriceList') {
        if (fieldName === 'prices') {
          return {
            handled: true,
            value: projectPriceListPricesConnection(source['prices'], field, fragments, variables),
          };
        }
        if (fieldName === 'quantityRules') {
          const quantityRules = isPlainObject(source['quantityRules']) ? source['quantityRules'] : emptyConnection();
          return {
            handled: true,
            value: projectConnectionPayload(quantityRules, field, fragments, variables),
          };
        }
      }

      const value = source[fieldName];
      if (isPlainObject(value) && Array.isArray(value['edges'])) {
        return {
          handled: true,
          value: projectConnectionPayload(value, field, fragments, variables),
        };
      }

      return { handled: false };
    },
  });
}

function priceListPriceNodeMatchesPositiveQueryTerm(node: unknown, term: SearchQueryTerm): boolean {
  if (!isPlainObject(node)) {
    return false;
  }

  const variant = isPlainObject(node['variant']) ? node['variant'] : null;
  const product = isPlainObject(variant?.['product']) ? variant['product'] : null;
  const field = term.field?.toLowerCase() ?? null;
  const value = stripSearchQueryValueQuotes(searchQueryTermValue(term));
  const variantId = typeof variant?.['id'] === 'string' ? variant['id'] : null;
  const productId = typeof product?.['id'] === 'string' ? product['id'] : null;

  if (field === 'variant_id') {
    return (
      matchesStringValue(variantId, value, 'exact') ||
      (variantId !== null && String(resourceNumericId(variantId)) === value)
    );
  }

  if (field === 'product_id') {
    return (
      matchesStringValue(productId, value, 'exact') ||
      (productId !== null && String(resourceNumericId(productId)) === value)
    );
  }

  return true;
}

function priceListPriceNodeMatchesQuery(node: unknown, rawQuery: unknown): boolean {
  return (
    applySearchQuery([node], rawQuery, { recognizeNotKeyword: true }, priceListPriceNodeMatchesPositiveQueryTerm)
      .length > 0
  );
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

  return serializeConnection(selection, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (edge) => edge.cursor,
    serializeNode: (edge, nodeSelection) =>
      projectMarketValue(edge.node, nodeSelection.selectionSet?.selections ?? [], fragments, variables),
    pageInfoOptions: {
      prefixCursors: false,
    },
    serializePageInfo: (pageInfoSelection) =>
      preservesCapturedPageInfo
        ? (projectMarketValue(
            value['pageInfo'],
            pageInfoSelection.selectionSet?.selections ?? [],
            fragments,
            variables,
          ) as Record<string, unknown>)
        : undefined,
    serializeUnknownField: (childSelection) => value[childSelection.name.value] ?? null,
  });
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

function buyerSignalCountryCode(rawBuyerSignal: unknown): string | null {
  if (!isPlainObject(rawBuyerSignal) || typeof rawBuyerSignal['countryCode'] !== 'string') {
    return null;
  }

  const countryCode = rawBuyerSignal['countryCode'];
  return SHOPIFY_COUNTRY_CODES.has(countryCode) ? countryCode : null;
}

function buyerSignalVariableName(field: FieldNode): string | null {
  const argument = field.arguments?.find((candidate) => candidate.name.value === 'buyerSignal') ?? null;
  return argument?.value.kind === Kind.VARIABLE ? argument.value.name.value : null;
}

function invalidBuyerSignalCountryCodeError(field: FieldNode, rawCountryCode: unknown): Record<string, unknown> {
  const variableName = buyerSignalVariableName(field);
  const value = typeof rawCountryCode === 'string' ? rawCountryCode : String(rawCountryCode);
  const message = variableName
    ? `Variable $${variableName} of type BuyerSignalInput! was provided invalid value for countryCode (Expected "${value}" to be one of: ${SHOPIFY_COUNTRY_CODE_LIST})`
    : `Argument 'buyerSignal' on Field 'marketsResolvedValues' has an invalid value for countryCode (Expected "${value}" to be one of: ${SHOPIFY_COUNTRY_CODE_LIST}).`;

  return {
    message,
    extensions: {
      code: variableName ? 'INVALID_VARIABLE' : 'argumentLiteralsIncompatible',
      value: variableName ? { countryCode: rawCountryCode } : undefined,
      problems: [
        {
          path: ['countryCode'],
          explanation: `Expected "${value}" to be one of: ${SHOPIFY_COUNTRY_CODE_LIST}`,
        },
      ],
    },
  };
}

function validateMarketsResolvedValuesBuyerSignal(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown>[] {
  if (!field.arguments?.some((argument) => argument.name.value === 'buyerSignal')) {
    return [];
  }

  const args = getFieldArguments(field, variables);
  const buyerSignal = args['buyerSignal'];
  const countryCode = isPlainObject(buyerSignal) ? buyerSignal['countryCode'] : undefined;

  if (typeof countryCode !== 'string' || !SHOPIFY_COUNTRY_CODES.has(countryCode)) {
    return [invalidBuyerSignalCountryCodeError(field, countryCode)];
  }

  return [];
}

function marketsResolvedValuesPayloadKeyFromDocument(document: string, variables: Record<string, unknown>): string {
  const resolvedValuesField = getRootFields(document).find((field) => field.name.value === 'marketsResolvedValues');
  if (!resolvedValuesField) {
    return marketsResolvedValuesPayloadKey(null);
  }

  const args = getFieldArguments(resolvedValuesField, variables);
  return marketsResolvedValuesPayloadKey(buyerSignalCountryCode(args['buyerSignal']));
}

export function hydrateMarketsFromUpstreamResponse(
  document: string,
  variables: Record<string, unknown>,
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
    const rootPayload = readGraphqlDataResponsePayload(upstreamPayload, rootField);
    if (rootPayload === null) {
      continue;
    }

    store.setBaseMarketsRootPayload(rootField, rootPayload);
    if (rootField === 'marketsResolvedValues') {
      store.setBaseMarketsRootPayload(marketsResolvedValuesPayloadKeyFromDocument(document, variables), rootPayload);
    }
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

  return matchesSearchQueryString(candidate, rawValue, mode);
}

function compareMarketId(marketId: number, rawValue: string): boolean {
  const match = stripSearchQueryValueQuotes(rawValue).match(/^(<=|>=|<|>|=)?\s*(?:gid:\/\/shopify\/Market\/)?(\d+)$/u);
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
    const value = stripSearchQueryValueQuotes(term.value);
    return (
      matchesStringValue(market.data['name'], value, 'includes') ||
      matchesStringValue(market.data['handle'], value, 'includes') ||
      matchesStringValue(market.id, value, 'includes')
    );
  }

  const field = term.field.toLowerCase();
  const value = searchQueryTermValue(term);

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
      const expectedTypes = stripSearchQueryValueQuotes(value)
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

function applyMarketsQuery(markets: MarketRecord[], rawQuery: unknown): MarketRecord[] {
  return applySearchQuery(markets, rawQuery, { recognizeNotKeyword: true }, matchesPositiveMarketQueryTerm);
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

function catalogReferencesMarket(catalog: CatalogRecord, marketId: string): boolean {
  return catalogMarkets(catalog).some(
    (edge) => isPlainObject(edge.node) && typeof edge.node['id'] === 'string' && edge.node['id'] === marketId,
  );
}

function catalogConnectionForMarket(marketId: string, existingConnection: unknown): Record<string, unknown> {
  const edgesById = new Map<string, ConnectionEdge>();

  for (const edge of readConnectionEdges(existingConnection)) {
    if (isPlainObject(edge.node) && typeof edge.node['id'] === 'string') {
      edgesById.set(edge.node['id'], edge);
    }
  }

  for (const catalog of store.listEffectiveCatalogs()) {
    if (!catalogReferencesMarket(catalog, marketId)) {
      continue;
    }
    edgesById.set(catalog.id, {
      cursor: catalogCursor(catalog),
      node: catalog.data,
    });
  }

  return { edges: Array.from(edgesById.values()) };
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
  const match = stripSearchQueryValueQuotes(rawValue).match(
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

  const value = searchQueryTermValue(term);
  const field = term.field?.toLowerCase() ?? null;

  if (field === null) {
    return (
      matchesStringValue(catalog.data['title'], value, 'includes') || matchesStringValue(catalog.id, value, 'includes')
    );
  }

  if (field === 'id') {
    const numericId = resourceNumericId(catalog.id);
    return matchesStringValue(catalog.id, value, 'exact') || (numericId !== null && compareCatalogId(numericId, value));
  }

  if (field === 'title') {
    return matchesStringValue(catalog.data['title'], value, 'includes');
  }

  if (field === 'status') {
    return matchesStringValue(catalog.data['status'], value, 'exact');
  }

  if (field === 'market_id') {
    return catalogMarkets(catalog).some(
      (edge) =>
        isPlainObject(edge.node) &&
        typeof edge.node['id'] === 'string' &&
        matchesStringValue(edge.node['id'], value, 'exact'),
    );
  }

  return true;
}

function applyCatalogsQuery(catalogs: CatalogRecord[], rawQuery: unknown): CatalogRecord[] {
  return applySearchQuery(catalogs, rawQuery, { recognizeNotKeyword: true }, matchesCatalogQueryTerm);
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
    store.listEffectiveCatalogs().filter((catalog) => catalogHasType(catalog, args['type'])),
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
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: catalogCursor,
    serializeNode: (catalog, selection) =>
      projectMarketValue(catalog.data, selection.selectionSet?.selections ?? [], fragments, variables),
    pageInfoOptions: {
      prefixCursors: false,
    },
  });
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
  const sortedPriceLists = [...store.listEffectivePriceLists()].sort((left, right) =>
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
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: priceListCursor,
    serializeNode: (priceList, selection) =>
      projectMarketValue(priceList.data, selection.selectionSet?.selections ?? [], fragments, variables),
    pageInfoOptions: {
      prefixCursors: false,
    },
  });
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
  return projectMarketValue(market, selections, fragments, variables);
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

    const key = getFieldResponseKey(selection);
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

      const key = getFieldResponseKey(selection);
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

    const key = getFieldResponseKey(selection);
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

function serializeMarketLocalizableResourcesConnection(
  resources: MarketLocalizableResourceRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const window = paginateConnectionItems(resources, field, variables, marketLocalizableResourceCursor);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: marketLocalizableResourceCursor,
    serializeNode: (resource, selection) =>
      serializeMarketLocalizableResource(resource, selection.selectionSet?.selections ?? [], fragments, variables),
  });
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

function removeWebPresenceFromMarkets(webPresenceId: string): void {
  for (const market of store.listEffectiveMarkets()) {
    const edges = readConnectionEdges(market.data['webPresences']);
    const nextEdges = edges.filter(
      (edge) => !(isPlainObject(edge.node) && typeof edge.node['id'] === 'string' && edge.node['id'] === webPresenceId),
    );

    if (nextEdges.length === edges.length) {
      continue;
    }

    store.stageUpdateMarket({
      ...market,
      data: {
        ...structuredClone(market.data),
        webPresences: connectionFromEdges(nextEdges) as JsonValue,
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
  if (!/^[a-z]+$/u.test(suffix)) {
    errors.push(
      marketError(
        ['input', 'subfolderSuffix'],
        'Subfolder suffix must contain only letters',
        'SUBFOLDER_SUFFIX_MUST_CONTAIN_ONLY_LETTERS',
      ),
    );
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
  return field.selectionSet
    ? projectMarketValue(payload, field.selectionSet.selections, fragments, variables)
    : payload;
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

function catalogTitleInUse(title: string, excludedCatalogId?: string): boolean {
  return store
    .listEffectiveCatalogs()
    .some((catalog) => catalog.id !== excludedCatalogId && catalog.data['title'] === title);
}

function catalogError(field: string[], message: string, code: string): MarketUserError {
  return { field, message, code };
}

function priceListError(field: string[], message: string, code: string): MarketUserError {
  return { field, message, code };
}

function catalogMarketIds(catalog: CatalogRecord): string[] {
  return catalogMarkets(catalog).flatMap((edge) =>
    isPlainObject(edge.node) && typeof edge.node['id'] === 'string' ? [edge.node['id']] : [],
  );
}

function readCatalogContextMarketIds(
  rawContext: unknown,
  fieldPrefix: string[],
  errors: MarketUserError[],
  options: { requireMarketContext: boolean },
): string[] {
  const context = readInput(rawContext);
  const companyLocationIds = readStringArray(context['companyLocationIds']);
  if (companyLocationIds.length > 0) {
    errors.push(
      catalogError(
        [...fieldPrefix, 'companyLocationIds'],
        'Only market catalog contexts are supported locally',
        'UNSUPPORTED_CONTEXT',
      ),
    );
  }

  const marketIds = readStringArray(context['marketIds']);
  if (options.requireMarketContext && marketIds.length === 0) {
    errors.push(catalogError([...fieldPrefix, 'marketIds'], 'At least one market is required', 'BLANK'));
  }

  const uniqueMarketIds: string[] = [];
  const seenMarketIds = new Set<string>();
  for (const marketId of marketIds) {
    if (seenMarketIds.has(marketId)) {
      continue;
    }
    seenMarketIds.add(marketId);
    if (!marketId.startsWith('gid://shopify/Market/')) {
      errors.push(catalogError([...fieldPrefix, 'marketIds'], 'Market does not exist', 'MARKET_NOT_FOUND'));
      continue;
    }
    if (!store.getEffectiveMarketRecordById(marketId)) {
      errors.push(catalogError([...fieldPrefix, 'marketIds'], 'Market does not exist', 'MARKET_NOT_FOUND'));
      continue;
    }
    uniqueMarketIds.push(marketId);
  }

  return uniqueMarketIds;
}

function marketNodeForCatalogConnection(marketId: string): Record<string, unknown> {
  const market = store.getEffectiveMarketRecordById(marketId);
  return market ? { __typename: 'Market', ...market.data, id: market.id } : { __typename: 'Market', id: marketId };
}

function marketConnectionFromIds(marketIds: string[]): Record<string, unknown> {
  return {
    edges: marketIds.map((marketId) => ({
      cursor: marketId,
      node: marketNodeForCatalogConnection(marketId),
    })),
    pageInfo: {
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: marketIds[0] ?? null,
      endCursor: marketIds.at(-1) ?? null,
    },
  };
}

function catalogStatusFromInput(
  input: Record<string, unknown>,
  existing: Record<string, unknown> | null,
  errors: MarketUserError[],
): string {
  const rawStatus = input['status'];
  if (rawStatus === undefined) {
    return typeof existing?.['status'] === 'string' ? existing['status'] : 'ACTIVE';
  }

  if (rawStatus === 'ACTIVE' || rawStatus === 'DRAFT' || rawStatus === 'ARCHIVED') {
    return rawStatus;
  }

  errors.push(catalogError(['input', 'status'], "Status isn't included in the list", 'INCLUSION'));
  return typeof existing?.['status'] === 'string' ? existing['status'] : 'ACTIVE';
}

function linkedPriceList(rawPriceListId: unknown, existing: Record<string, unknown> | null): unknown {
  if (rawPriceListId === null) {
    return null;
  }
  if (typeof rawPriceListId === 'string' && rawPriceListId.length > 0) {
    return store.getEffectivePriceListById(rawPriceListId) ?? { __typename: 'PriceList', id: rawPriceListId };
  }
  return existing?.['priceList'] ?? null;
}

function linkedPublication(rawPublicationId: unknown, existing: Record<string, unknown> | null): unknown {
  if (rawPublicationId === null) {
    return null;
  }
  if (typeof rawPublicationId === 'string' && rawPublicationId.length > 0) {
    const publication = store.listEffectivePublications().find((candidate) => candidate.id === rawPublicationId);
    return publication
      ? { __typename: 'Publication', ...publication }
      : { __typename: 'Publication', id: rawPublicationId };
  }
  return existing?.['publication'] ?? null;
}

function buildCatalogRecord(
  id: string,
  input: Record<string, unknown>,
  existingCatalog: CatalogRecord | null,
  errors: MarketUserError[],
  contextMarketIds: string[],
): CatalogRecord {
  const existing = existingCatalog?.data ?? null;
  const rawTitle = input['title'];
  const title =
    typeof rawTitle === 'string' ? rawTitle : typeof existing?.['title'] === 'string' ? existing['title'] : '';
  const trimmedTitle = title.trim();

  if (trimmedTitle.length === 0) {
    errors.push(catalogError(['input', 'title'], "Title can't be blank", 'BLANK'));
  } else if (trimmedTitle.length < 2) {
    errors.push(catalogError(['input', 'title'], 'Title is too short (minimum is 2 characters)', 'TOO_SHORT'));
  } else if (catalogTitleInUse(trimmedTitle, existingCatalog?.id)) {
    errors.push(catalogError(['input', 'title'], `Title '${trimmedTitle}' has already been taken`, 'TAKEN'));
  }

  const now = makeSyntheticTimestamp();
  const data: Record<string, unknown> = {
    ...(existing ? structuredClone(existing) : {}),
    __typename: 'MarketCatalog',
    id,
    title: trimmedTitle,
    status: catalogStatusFromInput(input, existing, errors),
    markets: marketConnectionFromIds(contextMarketIds),
    operations: Array.isArray(existing?.['operations']) ? structuredClone(existing['operations']) : [],
    priceList: hasOwnProperty(input, 'priceListId')
      ? linkedPriceList(input['priceListId'], existing)
      : (existing?.['priceList'] ?? null),
    publication: hasOwnProperty(input, 'publicationId')
      ? linkedPublication(input['publicationId'], existing)
      : (existing?.['publication'] ?? null),
    createdAt: typeof existing?.['createdAt'] === 'string' ? existing['createdAt'] : now,
    updatedAt: now,
  };

  return {
    id,
    cursor: existingCatalog?.cursor ?? id,
    data: data as Record<string, JsonValue>,
  };
}

function selectedCatalogPayload(catalog: CatalogRecord | null): unknown {
  return catalog ? catalog.data : null;
}

function selectedPriceListPayload(priceList: PriceListRecord | null): unknown {
  return priceList ? priceList.data : null;
}

function priceListNameInUse(name: string, excludedPriceListId?: string): boolean {
  return store
    .listEffectivePriceLists()
    .some((priceList) => priceList.data['name'] === name && priceList.id !== excludedPriceListId);
}

function priceListCurrencyFromInput(
  input: Record<string, unknown>,
  existing: Record<string, unknown> | null,
  errors: MarketUserError[],
): string {
  const rawCurrency = input['currency'];
  const existingCurrency = typeof existing?.['currency'] === 'string' ? existing['currency'] : 'USD';
  if (rawCurrency === undefined) {
    return existingCurrency;
  }

  if (typeof rawCurrency !== 'string' || !CURRENCY_NAMES[rawCurrency]) {
    errors.push(priceListError(['input', 'currency'], "Currency isn't included in the list", 'INCLUSION'));
    return existingCurrency;
  }

  return rawCurrency;
}

function priceListParentFromInput(input: Record<string, unknown>, existing: Record<string, unknown> | null): unknown {
  if (!hasOwnProperty(input, 'parent')) {
    return existing?.['parent'] ?? null;
  }

  const parent = readInput(input['parent']);
  const adjustment = readInput(parent['adjustment']);
  const type = typeof adjustment['type'] === 'string' ? adjustment['type'] : null;
  const value = typeof adjustment['value'] === 'number' ? adjustment['value'] : null;
  return type && value !== null
    ? {
        adjustment: {
          type,
          value,
        },
      }
    : null;
}

function emptyPriceListPricesConnection(): Record<string, unknown> {
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

function buildPriceListRecord(
  id: string,
  input: Record<string, unknown>,
  existingPriceList: PriceListRecord | null,
  errors: MarketUserError[],
): PriceListRecord {
  const existing = existingPriceList?.data ?? null;
  const rawName = input['name'];
  const name =
    typeof rawName === 'string' ? rawName.trim() : typeof existing?.['name'] === 'string' ? existing['name'] : '';

  if (name.length === 0) {
    errors.push(priceListError(['input', 'name'], "Name can't be blank", 'BLANK'));
  } else if (priceListNameInUse(name, existingPriceList?.id)) {
    errors.push(priceListError(['input', 'name'], `Name '${name}' has already been taken`, 'TAKEN'));
  }

  const currency = priceListCurrencyFromInput(input, existing, errors);
  const currencyChanged =
    typeof existing?.['currency'] === 'string' &&
    hasOwnProperty(input, 'currency') &&
    existing['currency'] !== currency &&
    errors.length === 0;
  const catalog =
    hasOwnProperty(input, 'catalogId') && typeof input['catalogId'] === 'string'
      ? store.getEffectiveCatalogById(input['catalogId'])
      : (existing?.['catalog'] ?? null);

  if (typeof input['catalogId'] === 'string' && !catalog) {
    errors.push(priceListError(['input', 'catalogId'], 'Catalog does not exist', 'CATALOG_NOT_FOUND'));
  }

  const existingPrices = isPlainObject(existing?.['prices']) ? existing['prices'] : emptyPriceListPricesConnection();
  const prices = currencyChanged ? emptyPriceListPricesConnection() : existingPrices;
  const fixedPricesCount = readConnectionEdges(prices).filter(
    (edge) => isPlainObject(edge.node) && edge.node['originType'] === 'FIXED',
  ).length;
  const now = makeSyntheticTimestamp();
  const data: Record<string, unknown> = {
    ...(existing ? structuredClone(existing) : {}),
    __typename: 'PriceList',
    id,
    name,
    currency,
    fixedPricesCount,
    parent: priceListParentFromInput(input, existing),
    catalog,
    prices,
    quantityRules: isPlainObject(existing?.['quantityRules']) ? existing['quantityRules'] : emptyConnection(),
    createdAt: typeof existing?.['createdAt'] === 'string' ? existing['createdAt'] : now,
    updatedAt: now,
  };

  return {
    id,
    cursor: existingPriceList?.cursor ?? id,
    data: data as Record<string, JsonValue>,
  };
}

function readPriceListIdArgument(args: Record<string, unknown>): string | null {
  const input = readInput(args['input']);
  const rawId = args['priceListId'] ?? args['id'] ?? input['priceListId'] ?? input['id'];
  return typeof rawId === 'string' && rawId.length > 0 ? rawId : null;
}

function readFixedPriceInputs(args: Record<string, unknown>, names: string[]): Record<string, unknown>[] {
  const input = readInput(args['input']);
  for (const name of names) {
    const raw = args[name] ?? input[name];
    if (Array.isArray(raw)) {
      return raw.filter(isPlainObject);
    }
  }
  return [];
}

function fixedPriceRawArgument(args: Record<string, unknown>, name: string): unknown {
  const input = readInput(args['input']);
  return args[name] ?? input[name];
}

function readFixedPriceVariantIds(args: Record<string, unknown>, names: string[]): string[] {
  const input = readInput(args['input']);
  for (const name of names) {
    const raw = args[name] ?? input[name];
    const values = readStringArray(raw);
    if (values.length > 0) {
      return values;
    }
  }
  return [];
}

function readFixedPriceProductIds(args: Record<string, unknown>, names: string[]): string[] {
  const input = readInput(args['input']);
  for (const name of names) {
    const raw = args[name] ?? input[name];
    const values = readStringArray(raw);
    if (values.length > 0 || Array.isArray(raw)) {
      return values;
    }
  }
  return [];
}

function moneyPayload(rawMoney: unknown, currencyCode: string): Record<string, unknown> | null {
  if (!isPlainObject(rawMoney)) {
    return null;
  }

  const rawAmount = rawMoney['amount'];
  const amount =
    typeof rawAmount === 'number'
      ? formatShopifyMoneyAmount(String(rawAmount))
      : typeof rawAmount === 'string' && rawAmount.trim().length > 0
        ? formatShopifyMoneyAmount(rawAmount.trim())
        : null;
  if (amount === null) {
    return null;
  }

  return {
    amount,
    currencyCode:
      typeof rawMoney['currencyCode'] === 'string' && rawMoney['currencyCode'].length > 0
        ? rawMoney['currencyCode']
        : currencyCode,
  };
}

function formatShopifyMoneyAmount(rawAmount: string): string {
  if (!/^-?\d+(?:\.\d+)?$/.test(rawAmount)) {
    return rawAmount;
  }

  const [integerPart, fractionalPart = ''] = rawAmount.split('.');
  const trimmedFraction = fractionalPart.replace(/0+$/, '');
  return `${integerPart}.${trimmedFraction.length > 0 ? trimmedFraction : '0'}`;
}

function variantPriceListNode(
  variant: ProductVariantRecord,
  product: ProductRecord | null,
  input: Record<string, unknown>,
  currencyCode: string,
  existingNode: Record<string, unknown> | null = null,
): Record<string, unknown> | null {
  const price = moneyPayload(input['price'], currencyCode);
  if (!price) {
    return null;
  }

  return {
    __typename: 'PriceListPrice',
    price,
    compareAtPrice: moneyPayload(input['compareAtPrice'], currencyCode),
    originType: 'FIXED',
    quantityPriceBreaks: isPlainObject(existingNode?.['quantityPriceBreaks'])
      ? structuredClone(existingNode['quantityPriceBreaks'])
      : emptyConnection(),
    variant: {
      __typename: 'ProductVariant',
      id: variant.id,
      sku: variant.sku,
      product: {
        __typename: 'Product',
        id: variant.productId,
        title: product?.title ?? null,
      },
    },
  };
}

function fixedPriceVariantId(edge: ConnectionEdge): string | null {
  if (!isPlainObject(edge.node) || edge.node['originType'] !== 'FIXED') {
    return null;
  }
  const variant = isPlainObject(edge.node['variant']) ? edge.node['variant'] : null;
  return typeof variant?.['id'] === 'string' ? variant['id'] : null;
}

function productPayload(product: ProductRecord): Record<string, unknown> {
  return {
    __typename: 'Product',
    id: product.id,
    title: product.title,
    handle: product.handle,
    status: product.status,
  };
}

function rebuildPriceListWithEdges(priceList: PriceListRecord, edges: ConnectionEdge[]): PriceListRecord {
  const fixedPricesCount = edges.filter((edge) => fixedPriceVariantId(edge) !== null).length;
  const data: Record<string, unknown> = {
    ...structuredClone(priceList.data),
    fixedPricesCount,
    prices: connectionFromEdges(edges),
    updatedAt: makeSyntheticTimestamp(),
  };

  return {
    id: priceList.id,
    cursor: priceList.cursor,
    data: data as Record<string, JsonValue>,
  };
}

function upsertFixedPriceNodes(
  priceList: PriceListRecord,
  inputs: Record<string, unknown>[],
  mode: 'add' | 'update' | 'upsert',
  errors: MarketUserError[],
): { priceList: PriceListRecord; changedVariantIds: string[] } {
  const currencyCode = typeof priceList.data['currency'] === 'string' ? priceList.data['currency'] : 'USD';
  const edgesByVariantId = new Map<string, ConnectionEdge>();
  const otherEdges: ConnectionEdge[] = [];
  const changedVariantIds: string[] = [];

  for (const edge of readConnectionEdges(priceList.data['prices'])) {
    const variantId = fixedPriceVariantId(edge);
    if (variantId) {
      edgesByVariantId.set(variantId, edge);
    } else {
      otherEdges.push(edge);
    }
  }

  for (const input of inputs) {
    const variantId = typeof input['variantId'] === 'string' ? input['variantId'] : null;
    if (!variantId) {
      errors.push(priceListError(['prices', 'variantId'], 'Variant does not exist', 'VARIANT_NOT_FOUND'));
      continue;
    }

    const existing = edgesByVariantId.get(variantId);
    if (mode === 'add' && existing) {
      errors.push(priceListError(['prices', 'variantId'], 'Fixed price already exists', 'TAKEN'));
      continue;
    }
    if (mode === 'update' && !existing) {
      errors.push(priceListError(['prices', 'variantId'], 'Fixed price does not exist', 'NOT_FOUND'));
      continue;
    }

    const variant = store.getEffectiveVariantById(variantId);
    if (!variant) {
      errors.push(priceListError(['prices', 'variantId'], 'Variant does not exist', 'VARIANT_NOT_FOUND'));
      continue;
    }

    const product = store.getEffectiveProductById(variant.productId);
    const existingNode = isPlainObject(existing?.node) ? existing.node : null;
    const node = variantPriceListNode(variant, product, input, currencyCode, existingNode);
    if (!node) {
      errors.push(priceListError(['prices', 'price'], "Price can't be blank", 'BLANK'));
      continue;
    }

    edgesByVariantId.set(variantId, {
      cursor: variantId,
      node,
    });
    changedVariantIds.push(variantId);
  }

  return {
    priceList: rebuildPriceListWithEdges(priceList, [...otherEdges, ...edgesByVariantId.values()]),
    changedVariantIds,
  };
}

function deleteFixedPriceNodes(
  priceList: PriceListRecord,
  variantIds: string[],
  errors: MarketUserError[],
): { priceList: PriceListRecord; deletedVariantIds: string[] } {
  const deleteIds = new Set(variantIds);
  const deletedVariantIds: string[] = [];
  const edges = readConnectionEdges(priceList.data['prices']).filter((edge) => {
    const variantId = fixedPriceVariantId(edge);
    if (!variantId || !deleteIds.has(variantId)) {
      return true;
    }
    deletedVariantIds.push(variantId);
    return false;
  });

  for (const variantId of variantIds) {
    if (!deletedVariantIds.includes(variantId)) {
      errors.push(priceListError(['variantIds'], 'Fixed price does not exist', 'NOT_FOUND'));
      break;
    }
  }

  return {
    priceList: rebuildPriceListWithEdges(priceList, edges),
    deletedVariantIds,
  };
}

function readQuantityRuleInputs(args: Record<string, unknown>, names: string[]): Record<string, unknown>[] {
  const input = readInput(args['input']);
  for (const name of names) {
    const raw = args[name] ?? input[name];
    if (Array.isArray(raw)) {
      return raw.filter(isPlainObject);
    }
  }
  return [];
}

function readQuantityVariantIds(args: Record<string, unknown>, names: string[]): string[] {
  const input = readInput(args['input']);
  for (const name of names) {
    const raw = args[name] ?? input[name];
    const values = readStringArray(raw);
    if (values.length > 0 || Array.isArray(raw)) {
      return values;
    }
  }
  return [];
}

function quantityRuleVariantId(edge: ConnectionEdge): string | null {
  if (!isPlainObject(edge.node)) {
    return null;
  }
  const variant = isPlainObject(edge.node['productVariant']) ? edge.node['productVariant'] : null;
  return typeof variant?.['id'] === 'string' ? variant['id'] : null;
}

function quantityPriceBreakId(edge: ConnectionEdge): string | null {
  return isPlainObject(edge.node) && typeof edge.node['id'] === 'string' ? edge.node['id'] : null;
}

function quantityPriceBreakVariantId(edge: ConnectionEdge): string | null {
  if (!isPlainObject(edge.node)) {
    return null;
  }
  const variant = isPlainObject(edge.node['variant']) ? edge.node['variant'] : null;
  return typeof variant?.['id'] === 'string' ? variant['id'] : null;
}

function quantityPriceBreakMinimum(edge: ConnectionEdge): number | null {
  return isPlainObject(edge.node) && typeof edge.node['minimumQuantity'] === 'number'
    ? edge.node['minimumQuantity']
    : null;
}

function quantityRuleNode(
  variant: ProductVariantRecord,
  product: ProductRecord | null,
  input: Record<string, unknown>,
): Record<string, unknown> {
  return {
    __typename: 'QuantityRule',
    minimum: typeof input['minimum'] === 'number' ? Math.floor(input['minimum']) : 1,
    maximum: typeof input['maximum'] === 'number' ? Math.floor(input['maximum']) : null,
    increment: typeof input['increment'] === 'number' ? Math.floor(input['increment']) : 1,
    isDefault: false,
    originType: 'FIXED',
    productVariant: {
      __typename: 'ProductVariant',
      id: variant.id,
      sku: variant.sku,
      product: {
        __typename: 'Product',
        id: variant.productId,
        title: product?.title ?? null,
      },
    },
  };
}

function addQuantityRuleValidationErrors(
  input: Record<string, unknown>,
  field: string[],
  errors: MarketUserError[],
  codePrefix: 'standalone' | 'pricing',
): void {
  const minimum = typeof input['minimum'] === 'number' ? Math.floor(input['minimum']) : null;
  const maximum = typeof input['maximum'] === 'number' ? Math.floor(input['maximum']) : null;
  const increment = typeof input['increment'] === 'number' ? Math.floor(input['increment']) : null;

  const addError = (suffix: string, message: string, code: string) => {
    errors.push({
      field: [...field, suffix],
      message,
      code,
    });
  };

  if (minimum === null || minimum < 1) {
    addError(
      'minimum',
      'Minimum must be greater than or equal to 1.',
      codePrefix === 'pricing' ? 'QUANTITY_RULE_ADD_MINIMUM_IS_LESS_THAN_ONE' : 'GREATER_THAN_OR_EQUAL_TO',
    );
  }
  if (increment === null || increment < 1) {
    addError(
      'increment',
      'Increment must be greater than or equal to 1.',
      codePrefix === 'pricing' ? 'QUANTITY_RULE_ADD_INCREMENT_IS_LESS_THAN_ONE' : 'GREATER_THAN_OR_EQUAL_TO',
    );
  }
  if (maximum !== null && maximum < 1) {
    addError(
      'maximum',
      'Maximum must be greater than or equal to 1.',
      codePrefix === 'pricing' ? 'QUANTITY_RULE_ADD_MAXIMUM_IS_LESS_THAN_ONE' : 'GREATER_THAN_OR_EQUAL_TO',
    );
  }
  if (minimum !== null && maximum !== null && minimum > maximum) {
    addError(
      'minimum',
      'Minimum must be lower than or equal to the maximum.',
      codePrefix === 'pricing' ? 'QUANTITY_RULE_ADD_MINIMUM_GREATER_THAN_MAXIMUM' : 'MINIMUM_IS_GREATER_THAN_MAXIMUM',
    );
  }
  if (minimum !== null && increment !== null && increment > minimum) {
    addError(
      'increment',
      'Increment must be lower than or equal to the minimum.',
      codePrefix === 'pricing'
        ? 'QUANTITY_RULE_ADD_INCREMENT_IS_GREATER_THAN_MINIMUM'
        : 'INCREMENT_IS_GREATER_THAN_MINIMUM',
    );
  }
  if (minimum !== null && increment !== null && minimum % increment !== 0) {
    addError(
      'minimum',
      'The minimum must be a multiple of the increment.',
      codePrefix === 'pricing'
        ? 'QUANTITY_RULE_ADD_MINIMUM_NOT_A_MULTIPLE_OF_INCREMENT'
        : 'MINIMUM_NOT_MULTIPLE_OF_INCREMENT',
    );
  }
  if (maximum !== null && increment !== null && maximum % increment !== 0) {
    addError(
      'maximum',
      'The maximum must be a multiple of the increment.',
      codePrefix === 'pricing'
        ? 'QUANTITY_RULE_ADD_MAXIMUM_NOT_A_MULTIPLE_OF_INCREMENT'
        : 'MAXIMUM_NOT_MULTIPLE_OF_INCREMENT',
    );
  }
}

function rebuildPriceListWithQuantityRuleEdges(priceList: PriceListRecord, edges: ConnectionEdge[]): PriceListRecord {
  const data: Record<string, unknown> = {
    ...structuredClone(priceList.data),
    quantityRules: connectionFromEdges(edges),
    updatedAt: makeSyntheticTimestamp(),
  };

  return {
    id: priceList.id,
    cursor: priceList.cursor,
    data: data as Record<string, JsonValue>,
  };
}

function upsertQuantityRuleNodes(
  priceList: PriceListRecord,
  inputs: Record<string, unknown>[],
  errors: MarketUserError[],
  options: {
    fieldPrefix: string[];
    variantNotFoundCode: string;
    duplicateCode: string;
    validationCodePrefix: 'standalone' | 'pricing';
  },
): { priceList: PriceListRecord; quantityRules: Record<string, unknown>[]; variantIds: string[] } {
  const edgesByVariantId = new Map<string, ConnectionEdge>();
  const otherEdges: ConnectionEdge[] = [];
  const quantityRules: Record<string, unknown>[] = [];
  const variantIds: string[] = [];

  for (const edge of readConnectionEdges(priceList.data['quantityRules'])) {
    const variantId = quantityRuleVariantId(edge);
    if (variantId) {
      edgesByVariantId.set(variantId, edge);
    } else {
      otherEdges.push(edge);
    }
  }

  const inputVariantIds = new Set<string>();
  for (const [index, input] of inputs.entries()) {
    const field = [...options.fieldPrefix, String(index)];
    const variantId = typeof input['variantId'] === 'string' ? input['variantId'] : null;
    if (!variantId) {
      errors.push({
        field: [...field, 'variantId'],
        code: options.variantNotFoundCode,
        message: 'Product variant ID does not exist.',
      });
      continue;
    }
    if (inputVariantIds.has(variantId)) {
      errors.push({
        field,
        code: options.duplicateCode,
        message: 'Quantity rule inputs must be unique by variant id.',
      });
      continue;
    }
    inputVariantIds.add(variantId);

    const variant = store.getEffectiveVariantById(variantId);
    if (!variant) {
      errors.push({
        field: [...field, 'variantId'],
        code: options.variantNotFoundCode,
        message: 'Product variant ID does not exist.',
      });
      continue;
    }

    addQuantityRuleValidationErrors(input, field, errors, options.validationCodePrefix);
    if (errors.length > 0) {
      continue;
    }

    const product = store.getEffectiveProductById(variant.productId);
    const node = quantityRuleNode(variant, product, input);
    edgesByVariantId.set(variantId, { cursor: variantId, node });
    quantityRules.push(node);
    variantIds.push(variantId);
  }

  return {
    priceList: rebuildPriceListWithQuantityRuleEdges(priceList, [...otherEdges, ...edgesByVariantId.values()]),
    quantityRules,
    variantIds,
  };
}

function deleteQuantityRuleNodes(
  priceList: PriceListRecord,
  variantIds: string[],
  errors: MarketUserError[],
  options: {
    fieldPrefix: string[];
    variantNotFoundCode: string;
    missingRuleCode: string;
    missingRuleMessage: string;
  },
): { priceList: PriceListRecord; deletedVariantIds: string[] } {
  const requestedIds = new Set(variantIds);
  const deletedVariantIds: string[] = [];

  const edges = readConnectionEdges(priceList.data['quantityRules']).filter((edge) => {
    const variantId = quantityRuleVariantId(edge);
    if (!variantId || !requestedIds.has(variantId)) {
      return true;
    }
    const node = isPlainObject(edge.node) ? edge.node : null;
    if (node?.['originType'] !== 'FIXED') {
      return true;
    }
    deletedVariantIds.push(variantId);
    return false;
  });

  for (const [index, variantId] of variantIds.entries()) {
    if (!store.getEffectiveVariantById(variantId)) {
      errors.push({
        field: [...options.fieldPrefix, String(index)],
        code: options.variantNotFoundCode,
        message: 'Product variant ID does not exist.',
      });
      continue;
    }
    if (!deletedVariantIds.includes(variantId)) {
      errors.push({
        field: [...options.fieldPrefix, String(index)],
        code: options.missingRuleCode,
        message: options.missingRuleMessage,
      });
    }
  }

  return {
    priceList: rebuildPriceListWithQuantityRuleEdges(priceList, edges),
    deletedVariantIds,
  };
}

function quantityPriceBreakNode(
  priceList: PriceListRecord,
  variant: ProductVariantRecord,
  input: Record<string, unknown>,
  currencyCode: string,
): Record<string, unknown> | null {
  const price = moneyPayload(input['price'], currencyCode);
  const minimumQuantity = typeof input['minimumQuantity'] === 'number' ? Math.floor(input['minimumQuantity']) : null;
  if (!price || minimumQuantity === null) {
    return null;
  }

  const product = store.getEffectiveProductById(variant.productId);
  return {
    __typename: 'QuantityPriceBreak',
    id: makeSyntheticGid('QuantityPriceBreak'),
    minimumQuantity,
    price,
    priceList: {
      __typename: 'PriceList',
      id: priceList.id,
      name: priceList.data['name'] ?? null,
      currency: priceList.data['currency'] ?? currencyCode,
    },
    variant: {
      __typename: 'ProductVariant',
      id: variant.id,
      sku: variant.sku,
      product: {
        __typename: 'Product',
        id: variant.productId,
        title: product?.title ?? null,
      },
    },
  };
}

function rebuildFixedPriceEdgeWithQuantityBreaks(
  edge: ConnectionEdge,
  quantityBreakEdges: ConnectionEdge[],
): ConnectionEdge {
  if (!isPlainObject(edge.node)) {
    return edge;
  }

  return {
    cursor: edge.cursor,
    node: {
      ...structuredClone(edge.node),
      quantityPriceBreaks: connectionFromEdges(quantityBreakEdges),
    },
  };
}

function upsertQuantityPriceBreakNodes(
  priceList: PriceListRecord,
  inputs: Record<string, unknown>[],
  errors: MarketUserError[],
): { priceList: PriceListRecord; variantIds: string[] } {
  const currencyCode = typeof priceList.data['currency'] === 'string' ? priceList.data['currency'] : 'USD';
  const priceEdges = readConnectionEdges(priceList.data['prices']);
  const updatedEdgesByVariantId = new Map<string, ConnectionEdge>();
  const changedVariantIds: string[] = [];
  const inputsByVariantId = new Map<string, Record<string, unknown>[]>();

  for (const [index, input] of inputs.entries()) {
    const variantId = typeof input['variantId'] === 'string' ? input['variantId'] : null;
    if (!variantId || !store.getEffectiveVariantById(variantId)) {
      errors.push({
        field: ['input', 'quantityPriceBreaksToAdd', String(index)],
        code: 'QUANTITY_PRICE_BREAK_ADD_VARIANT_NOT_FOUND',
        message: 'Variant not found.',
      });
      continue;
    }
    inputsByVariantId.set(variantId, [...(inputsByVariantId.get(variantId) ?? []), input]);
  }

  for (const edge of priceEdges) {
    const variantId = fixedPriceVariantId(edge);
    if (!variantId || !inputsByVariantId.has(variantId) || !isPlainObject(edge.node)) {
      continue;
    }

    const variant = store.getEffectiveVariantById(variantId);
    if (!variant) {
      continue;
    }

    const breakEdgesByMinimum = new Map<number, ConnectionEdge>();
    for (const quantityBreakEdge of readConnectionEdges(edge.node['quantityPriceBreaks'])) {
      const minimum = quantityPriceBreakMinimum(quantityBreakEdge);
      if (minimum !== null) {
        breakEdgesByMinimum.set(minimum, quantityBreakEdge);
      }
    }

    const seenMinimums = new Set<number>();
    for (const input of inputsByVariantId.get(variantId) ?? []) {
      const minimumQuantity =
        typeof input['minimumQuantity'] === 'number' ? Math.floor(input['minimumQuantity']) : null;
      if (minimumQuantity === null || minimumQuantity < 1) {
        errors.push({
          field: ['input', 'quantityPriceBreaksToAdd'],
          code: 'QUANTITY_PRICE_BREAK_ADD_INVALID',
          message: 'Invalid quantity price break.',
        });
        continue;
      }
      if (seenMinimums.has(minimumQuantity)) {
        errors.push({
          field: ['input', 'quantityPriceBreaksToAdd'],
          code: 'QUANTITY_PRICE_BREAK_ADD_DUPLICATE_INPUT_FOR_VARIANT_AND_MIN',
          message: 'Quantity price breaks to add inputs must be unique by variant id and minimum quantity.',
        });
        continue;
      }
      seenMinimums.add(minimumQuantity);

      const node = quantityPriceBreakNode(priceList, variant, input, currencyCode);
      if (!node) {
        errors.push({
          field: ['input', 'quantityPriceBreaksToAdd'],
          code: 'QUANTITY_PRICE_BREAK_ADD_INVALID',
          message: 'Invalid quantity price break.',
        });
        continue;
      }

      breakEdgesByMinimum.set(minimumQuantity, { cursor: node['id'] as string, node });
      if (!changedVariantIds.includes(variantId)) {
        changedVariantIds.push(variantId);
      }
    }

    updatedEdgesByVariantId.set(
      variantId,
      rebuildFixedPriceEdgeWithQuantityBreaks(edge, [...breakEdgesByMinimum.values()]),
    );
  }

  for (const [variantId] of inputsByVariantId) {
    if (!priceEdges.some((edge) => fixedPriceVariantId(edge) === variantId)) {
      errors.push({
        field: ['input', 'quantityPriceBreaksToAdd'],
        code: 'QUANTITY_PRICE_BREAK_ADD_PRICE_LIST_PRICE_NOT_FOUND',
        message: "Quantity price break's fixed price not found.",
      });
    }
  }

  if (errors.length > 0) {
    return { priceList, variantIds: [] };
  }

  const mergedEdges = priceEdges.map((edge) => {
    const variantId = fixedPriceVariantId(edge);
    return variantId ? (updatedEdgesByVariantId.get(variantId) ?? edge) : edge;
  });

  return {
    priceList: rebuildPriceListWithEdges(priceList, mergedEdges),
    variantIds: changedVariantIds,
  };
}

function deleteQuantityPriceBreakNodes(
  priceList: PriceListRecord,
  ids: string[],
  variantIds: string[],
  errors: MarketUserError[],
): { priceList: PriceListRecord; variantIds: string[] } {
  const deleteIds = new Set(ids);
  const deleteVariantIds = new Set(variantIds);
  const deletedIds = new Set<string>();
  const changedVariantIds: string[] = [];
  const nextEdges = readConnectionEdges(priceList.data['prices']).map((edge) => {
    if (!isPlainObject(edge.node)) {
      return edge;
    }

    const variantId = fixedPriceVariantId(edge);
    const nextBreakEdges = readConnectionEdges(edge.node['quantityPriceBreaks']).filter((quantityBreakEdge) => {
      const id = quantityPriceBreakId(quantityBreakEdge);
      const breakVariantId = quantityPriceBreakVariantId(quantityBreakEdge) ?? variantId;
      const shouldDelete =
        (id !== null && deleteIds.has(id)) || (breakVariantId !== null && deleteVariantIds.has(breakVariantId));
      if (shouldDelete && id !== null) {
        deletedIds.add(id);
        if (breakVariantId && !changedVariantIds.includes(breakVariantId)) {
          changedVariantIds.push(breakVariantId);
        }
      }
      return !shouldDelete;
    });

    return rebuildFixedPriceEdgeWithQuantityBreaks(edge, nextBreakEdges);
  });

  for (const id of ids) {
    if (!deletedIds.has(id)) {
      errors.push({
        field: ['input', 'quantityPriceBreaksToDelete'],
        code: 'QUANTITY_PRICE_BREAK_DELETE_NOT_FOUND',
        message: 'Quantity price break not found.',
      });
    }
  }

  return {
    priceList: errors.length === 0 ? rebuildPriceListWithEdges(priceList, nextEdges) : priceList,
    variantIds: errors.length === 0 ? changedVariantIds : [],
  };
}

function handleCatalogCreate(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);
  const input = readInput(args['input']);
  const errors: MarketUserError[] = [];
  const marketIds = readCatalogContextMarketIds(input['context'], ['input', 'context'], errors, {
    requireMarketContext: true,
  });
  const catalog = buildCatalogRecord(makeSyntheticGid('MarketCatalog'), input, null, errors, marketIds);

  if (errors.length === 0) {
    store.stageCreateCatalog(catalog);
  }

  return projectMutationPayload(
    {
      catalog: errors.length === 0 ? selectedCatalogPayload(catalog) : null,
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handleCatalogUpdate(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const input = readInput(args['input']);
  const errors: MarketUserError[] = [];
  const existingCatalog = id ? store.getEffectiveCatalogRecordById(id) : null;

  if (!id || !existingCatalog) {
    errors.push(catalogError(['id'], 'Catalog does not exist', 'CATALOG_NOT_FOUND'));
  } else if (!catalogHasType(existingCatalog, 'MARKET')) {
    errors.push(catalogError(['id'], 'Only market catalogs are supported locally', 'UNSUPPORTED_CONTEXT'));
  }

  const marketIds =
    existingCatalog && hasOwnProperty(input, 'context')
      ? readCatalogContextMarketIds(input['context'], ['input', 'context'], errors, { requireMarketContext: true })
      : existingCatalog
        ? catalogMarketIds(existingCatalog)
        : [];
  const catalog = id && existingCatalog ? buildCatalogRecord(id, input, existingCatalog, errors, marketIds) : null;

  if (errors.length === 0 && catalog) {
    store.stageUpdateCatalog(catalog);
  }

  return projectMutationPayload(
    {
      catalog: errors.length === 0 ? selectedCatalogPayload(catalog) : null,
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handleCatalogContextUpdate(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const args = getFieldArguments(field, variables);
  const catalogId = typeof args['catalogId'] === 'string' ? args['catalogId'] : null;
  const errors: MarketUserError[] = [];
  const existingCatalog = catalogId ? store.getEffectiveCatalogRecordById(catalogId) : null;

  if (!catalogId || !existingCatalog) {
    errors.push(catalogError(['catalogId'], 'Catalog does not exist', 'CATALOG_NOT_FOUND'));
  } else if (!catalogHasType(existingCatalog, 'MARKET')) {
    errors.push(catalogError(['catalogId'], 'Only market catalogs are supported locally', 'UNSUPPORTED_CONTEXT'));
  }

  const nextMarketIds = new Set(existingCatalog ? catalogMarketIds(existingCatalog) : []);
  for (const marketId of readCatalogContextMarketIds(args['contextsToRemove'], ['contextsToRemove'], errors, {
    requireMarketContext: false,
  })) {
    nextMarketIds.delete(marketId);
  }
  for (const marketId of readCatalogContextMarketIds(args['contextsToAdd'], ['contextsToAdd'], errors, {
    requireMarketContext: false,
  })) {
    nextMarketIds.add(marketId);
  }

  if (existingCatalog && nextMarketIds.size === 0) {
    errors.push(catalogError(['contextsToAdd', 'marketIds'], 'At least one market is required', 'BLANK'));
  }

  const catalog =
    catalogId && existingCatalog
      ? buildCatalogRecord(catalogId, {}, existingCatalog, errors, Array.from(nextMarketIds))
      : null;

  if (errors.length === 0 && catalog) {
    store.stageUpdateCatalog(catalog);
  }

  return projectMutationPayload(
    {
      catalog: errors.length === 0 ? selectedCatalogPayload(catalog) : null,
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handleCatalogDelete(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const errors: MarketUserError[] = [];
  const existingCatalog = id ? store.getEffectiveCatalogRecordById(id) : null;

  if (!id || !existingCatalog) {
    errors.push(catalogError(['id'], 'Catalog does not exist', 'CATALOG_NOT_FOUND'));
  } else if (!catalogHasType(existingCatalog, 'MARKET')) {
    errors.push(catalogError(['id'], 'Only market catalogs are supported locally', 'UNSUPPORTED_CONTEXT'));
  }

  if (errors.length === 0 && id) {
    store.stageDeleteCatalog(id);
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

function handlePriceListCreate(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);
  const input = readInput(args['input']);
  const errors: MarketUserError[] = [];
  const priceList = buildPriceListRecord(makeSyntheticGid('PriceList'), input, null, errors);

  if (errors.length === 0) {
    store.stageCreatePriceList(priceList);
  }

  return projectMutationPayload(
    {
      priceList: errors.length === 0 ? selectedPriceListPayload(priceList) : null,
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handlePriceListUpdate(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const input = readInput(args['input']);
  const errors: MarketUserError[] = [];
  const existingPriceList = id ? store.getEffectivePriceListRecordById(id) : null;

  if (!id || !existingPriceList) {
    errors.push(priceListError(['id'], 'Price list does not exist', 'PRICE_LIST_NOT_FOUND'));
  }

  const priceList = id && existingPriceList ? buildPriceListRecord(id, input, existingPriceList, errors) : null;
  if (errors.length === 0 && priceList) {
    store.stageUpdatePriceList(priceList);
  }

  return projectMutationPayload(
    {
      priceList: errors.length === 0 ? selectedPriceListPayload(priceList) : null,
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handlePriceListDelete(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const errors: MarketUserError[] = [];
  const existingPriceList = id ? store.getEffectivePriceListRecordById(id) : null;

  if (!id || !existingPriceList) {
    errors.push(priceListError(['id'], 'Price list does not exist', 'PRICE_LIST_NOT_FOUND'));
  }

  if (errors.length === 0 && id) {
    store.stageDeletePriceList(id);
  }

  return projectMutationPayload(
    {
      deletedId: errors.length === 0 ? id : null,
      priceList: errors.length === 0 ? selectedPriceListPayload(existingPriceList) : null,
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handlePriceListFixedPricesAdd(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const args = getFieldArguments(field, variables);
  const priceListId = readPriceListIdArgument(args);
  const errors: MarketUserError[] = [];
  const existingPriceList = priceListId ? store.getEffectivePriceListRecordById(priceListId) : null;

  if (!priceListId || !existingPriceList) {
    errors.push(priceListError(['priceListId'], 'Price list does not exist', 'PRICE_LIST_NOT_FOUND'));
  }

  const fixedPrices = readFixedPriceInputs(args, ['prices', 'fixedPrices', 'pricesToAdd']);
  const { priceList, changedVariantIds } = existingPriceList
    ? upsertFixedPriceNodes(existingPriceList, fixedPrices, 'add', errors)
    : { priceList: null, changedVariantIds: [] };

  if (errors.length === 0 && priceList) {
    store.stageUpdatePriceList(priceList);
  }

  return projectMutationPayload(
    {
      priceList: errors.length === 0 ? selectedPriceListPayload(priceList) : null,
      fixedPriceVariantIds: errors.length === 0 ? changedVariantIds : [],
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handlePriceListFixedPricesUpdate(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const args = getFieldArguments(field, variables);
  const priceListId = readPriceListIdArgument(args);
  const errors: MarketUserError[] = [];
  const existingPriceList = priceListId ? store.getEffectivePriceListRecordById(priceListId) : null;

  if (!priceListId || !existingPriceList) {
    errors.push(priceListError(['priceListId'], 'Price list does not exist', 'PRICE_LIST_NOT_FOUND'));
  }

  const fixedPrices = readFixedPriceInputs(args, ['prices', 'fixedPrices', 'pricesToUpdate']);
  const { priceList, changedVariantIds } = existingPriceList
    ? upsertFixedPriceNodes(existingPriceList, fixedPrices, 'update', errors)
    : { priceList: null, changedVariantIds: [] };

  if (errors.length === 0 && priceList) {
    store.stageUpdatePriceList(priceList);
  }

  return projectMutationPayload(
    {
      priceList: errors.length === 0 ? selectedPriceListPayload(priceList) : null,
      fixedPriceVariantIds: errors.length === 0 ? changedVariantIds : [],
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handlePriceListFixedPricesDelete(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const args = getFieldArguments(field, variables);
  const priceListId = readPriceListIdArgument(args);
  const errors: MarketUserError[] = [];
  const existingPriceList = priceListId ? store.getEffectivePriceListRecordById(priceListId) : null;

  if (!priceListId || !existingPriceList) {
    errors.push(priceListError(['priceListId'], 'Price list does not exist', 'PRICE_LIST_NOT_FOUND'));
  }

  const variantIds = readFixedPriceVariantIds(args, ['variantIds', 'variantsToDelete', 'fixedPriceVariantIds']);
  const { priceList, deletedVariantIds } = existingPriceList
    ? deleteFixedPriceNodes(existingPriceList, variantIds, errors)
    : { priceList: null, deletedVariantIds: [] };

  if (errors.length === 0 && priceList) {
    store.stageUpdatePriceList(priceList);
  }

  return projectMutationPayload(
    {
      priceList: errors.length === 0 ? selectedPriceListPayload(priceList) : null,
      deletedFixedPriceVariantIds: errors.length === 0 ? deletedVariantIds : [],
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handlePriceListFixedPricesByProductUpdate(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const args = getFieldArguments(field, variables);
  const input = readInput(args['input']);
  const priceListId = readPriceListIdArgument(args);
  const errors: MarketUserError[] = [];
  const existingPriceList = priceListId ? store.getEffectivePriceListRecordById(priceListId) : null;

  const productPriceInputs = readFixedPriceInputs(args, ['pricesToAdd']);
  const productIdsToDelete = readFixedPriceProductIds(args, ['pricesToDeleteByProductIds']);
  const usesProductLevelShape =
    productPriceInputs.some((priceInput) => typeof priceInput['productId'] === 'string') ||
    Array.isArray(fixedPriceRawArgument(args, 'pricesToDeleteByProductIds'));

  if (usesProductLevelShape) {
    if (!priceListId || !existingPriceList) {
      errors.push(priceListError(['priceListId'], 'Price list does not exist.', 'PRICE_LIST_DOES_NOT_EXIST'));
    }

    const fixedPrices: Record<string, unknown>[] = [];
    const variantIdsToDelete: string[] = [];
    const pricesToAddProducts: Record<string, unknown>[] = [];
    const pricesToDeleteProducts: Record<string, unknown>[] = [];

    if (existingPriceList) {
      for (const [index, priceInput] of productPriceInputs.entries()) {
        const productId = typeof priceInput['productId'] === 'string' ? priceInput['productId'] : null;
        const product = productId ? store.getEffectiveProductById(productId) : null;
        if (!product || !productId) {
          errors.push(
            priceListError(
              ['pricesToAdd', String(index), 'productId'],
              `Product ${productId ?? ''} in \`pricesToAdd\` does not exist.`,
              'PRODUCT_DOES_NOT_EXIST',
            ),
          );
          continue;
        }

        pricesToAddProducts.push(productPayload(product));
        for (const variant of store.getEffectiveVariantsByProductId(productId)) {
          fixedPrices.push({ ...priceInput, variantId: variant.id });
        }
      }

      for (const [index, productId] of productIdsToDelete.entries()) {
        const product = store.getEffectiveProductById(productId);
        if (!product) {
          errors.push(
            priceListError(
              ['pricesToDeleteByProductIds', String(index)],
              `Product ${productId} in \`pricesToDeleteByProductIds\` does not exist.`,
              'PRODUCT_DOES_NOT_EXIST',
            ),
          );
          continue;
        }

        pricesToDeleteProducts.push(productPayload(product));
        variantIdsToDelete.push(...store.getEffectiveVariantsByProductId(productId).map((variant) => variant.id));
      }
    }

    let priceList = existingPriceList;
    let changedVariantIds: string[] = [];
    let removedVariantIds: string[] = [];
    if (existingPriceList && errors.length === 0) {
      const upserted = upsertFixedPriceNodes(existingPriceList, fixedPrices, 'upsert', errors);
      const deleted = deleteFixedPriceNodes(upserted.priceList, variantIdsToDelete, errors);
      priceList = deleted.priceList;
      changedVariantIds = upserted.changedVariantIds;
      removedVariantIds = deleted.deletedVariantIds;
    }

    if (errors.length === 0 && priceList) {
      store.stageUpdatePriceList(priceList);
    }

    return projectMutationPayload(
      {
        priceList: errors.length === 0 ? selectedPriceListPayload(priceList) : null,
        pricesToAddProducts: errors.length === 0 ? pricesToAddProducts : null,
        pricesToDeleteProducts: errors.length === 0 ? pricesToDeleteProducts : null,
        fixedPriceVariantIds: errors.length === 0 ? changedVariantIds : [],
        deletedFixedPriceVariantIds: errors.length === 0 ? removedVariantIds : [],
        userErrors: errors,
      },
      field,
      fragments,
      variables,
    );
  }

  const productId =
    typeof args['productId'] === 'string'
      ? args['productId']
      : typeof input['productId'] === 'string'
        ? input['productId']
        : null;
  const product = productId ? store.getEffectiveProductById(productId) : null;

  if (!priceListId || !existingPriceList) {
    errors.push(priceListError(['priceListId'], 'Price list does not exist', 'PRICE_LIST_NOT_FOUND'));
  }
  if (!productId || !product) {
    errors.push(priceListError(['productId'], 'Product does not exist', 'PRODUCT_NOT_FOUND'));
  }

  const productVariantIds = new Set(
    productId ? store.getEffectiveVariantsByProductId(productId).map((variant) => variant.id) : [],
  );
  const fixedPrices = readFixedPriceInputs(args, ['prices', 'fixedPrices', 'pricesToAdd', 'pricesToUpdate']);
  for (const fixedPrice of fixedPrices) {
    if (typeof fixedPrice['variantId'] === 'string' && !productVariantIds.has(fixedPrice['variantId'])) {
      errors.push(priceListError(['prices', 'variantId'], 'Variant does not belong to product', 'VARIANT_NOT_FOUND'));
    }
  }

  const deletedVariantIds = readFixedPriceVariantIds(args, [
    'variantIds',
    'variantsToDelete',
    'fixedPriceVariantIds',
    'pricesToDelete',
  ]);
  for (const variantId of deletedVariantIds) {
    if (!productVariantIds.has(variantId)) {
      errors.push(priceListError(['variantIds'], 'Variant does not belong to product', 'VARIANT_NOT_FOUND'));
    }
  }

  let priceList = existingPriceList;
  let changedVariantIds: string[] = [];
  let removedVariantIds: string[] = [];
  if (existingPriceList && errors.length === 0) {
    const upserted = upsertFixedPriceNodes(existingPriceList, fixedPrices, 'upsert', errors);
    const deleted = deleteFixedPriceNodes(upserted.priceList, deletedVariantIds, errors);
    priceList = deleted.priceList;
    changedVariantIds = upserted.changedVariantIds;
    removedVariantIds = deleted.deletedVariantIds;
  }

  if (errors.length === 0 && priceList) {
    store.stageUpdatePriceList(priceList);
  }

  return projectMutationPayload(
    {
      priceList: errors.length === 0 ? selectedPriceListPayload(priceList) : null,
      fixedPriceVariantIds: errors.length === 0 ? changedVariantIds : [],
      deletedFixedPriceVariantIds: errors.length === 0 ? removedVariantIds : [],
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handleQuantityRulesAdd(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);
  const priceListId = readPriceListIdArgument(args);
  const errors: MarketUserError[] = [];
  const existingPriceList = priceListId ? store.getEffectivePriceListRecordById(priceListId) : null;

  if (!priceListId || !existingPriceList) {
    errors.push(priceListError(['priceListId'], 'Price list does not exist.', 'PRICE_LIST_DOES_NOT_EXIST'));
  }

  const quantityRuleInputs = readQuantityRuleInputs(args, ['quantityRules', 'rules', 'quantityRulesToAdd']);
  const { priceList, quantityRules } = existingPriceList
    ? upsertQuantityRuleNodes(existingPriceList, quantityRuleInputs, errors, {
        fieldPrefix: ['quantityRules'],
        variantNotFoundCode: 'PRODUCT_VARIANT_DOES_NOT_EXIST',
        duplicateCode: 'DUPLICATE_INPUT_FOR_VARIANT',
        validationCodePrefix: 'standalone',
      })
    : { priceList: null, quantityRules: [] };

  if (errors.length === 0 && priceList) {
    store.stageUpdatePriceList(priceList);
  }

  return projectMutationPayload(
    {
      quantityRules: errors.length === 0 ? quantityRules : [],
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handleQuantityRulesDelete(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const args = getFieldArguments(field, variables);
  const priceListId = readPriceListIdArgument(args);
  const errors: MarketUserError[] = [];
  const existingPriceList = priceListId ? store.getEffectivePriceListRecordById(priceListId) : null;

  if (!priceListId || !existingPriceList) {
    errors.push(priceListError(['priceListId'], 'Price list does not exist.', 'PRICE_LIST_DOES_NOT_EXIST'));
  }

  const variantIds = readQuantityVariantIds(args, ['variantIds', 'quantityRulesToDeleteByVariantId']);
  const { priceList, deletedVariantIds } = existingPriceList
    ? deleteQuantityRuleNodes(existingPriceList, variantIds, errors, {
        fieldPrefix: ['variantIds'],
        variantNotFoundCode: 'PRODUCT_VARIANT_DOES_NOT_EXIST',
        missingRuleCode: 'VARIANT_QUANTITY_RULE_DOES_NOT_EXIST',
        missingRuleMessage: 'Quantity rule for variant associated with the price list provided does not exist.',
      })
    : { priceList: null, deletedVariantIds: [] };

  if (errors.length === 0 && priceList) {
    store.stageUpdatePriceList(priceList);
  }

  return projectMutationPayload(
    {
      deletedQuantityRulesVariantIds: errors.length === 0 ? deletedVariantIds : [],
      userErrors: errors,
    },
    field,
    fragments,
    variables,
  );
}

function handleQuantityPricingByVariantUpdate(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const args = getFieldArguments(field, variables);
  const priceListId = readPriceListIdArgument(args);
  const errors: MarketUserError[] = [];
  const existingPriceList = priceListId ? store.getEffectivePriceListRecordById(priceListId) : null;

  if (!priceListId || !existingPriceList) {
    errors.push(priceListError(['priceListId'], 'Price list not found.', 'PRICE_LIST_NOT_FOUND'));
  }

  let priceList = existingPriceList;
  const changedVariantIds = new Set<string>();

  if (priceList && errors.length === 0) {
    const priceInputs = readFixedPriceInputs(args, ['pricesToAdd']);
    const upserted = upsertFixedPriceNodes(priceList, priceInputs, 'upsert', errors);
    priceList = upserted.priceList;
    for (const variantId of upserted.changedVariantIds) {
      changedVariantIds.add(variantId);
    }
  }

  if (priceList && errors.length === 0) {
    const priceVariantIdsToDelete = readFixedPriceVariantIds(args, ['pricesToDeleteByVariantId']);
    const deleted = deleteFixedPriceNodes(priceList, priceVariantIdsToDelete, errors);
    priceList = deleted.priceList;
    for (const variantId of deleted.deletedVariantIds) {
      changedVariantIds.add(variantId);
    }
  }

  if (priceList && errors.length === 0) {
    const ruleInputs = readQuantityRuleInputs(args, ['quantityRulesToAdd']);
    const upserted = upsertQuantityRuleNodes(priceList, ruleInputs, errors, {
      fieldPrefix: ['input', 'quantityRulesToAdd'],
      variantNotFoundCode: 'QUANTITY_RULE_ADD_VARIANT_NOT_FOUND',
      duplicateCode: 'QUANTITY_RULE_ADD_DUPLICATE_INPUT_FOR_VARIANT',
      validationCodePrefix: 'pricing',
    });
    priceList = upserted.priceList;
    for (const variantId of upserted.variantIds) {
      changedVariantIds.add(variantId);
    }
  }

  if (priceList && errors.length === 0) {
    const ruleVariantIdsToDelete = readQuantityVariantIds(args, ['quantityRulesToDeleteByVariantId']);
    const deleted = deleteQuantityRuleNodes(priceList, ruleVariantIdsToDelete, errors, {
      fieldPrefix: ['input', 'quantityRulesToDeleteByVariantId'],
      variantNotFoundCode: 'QUANTITY_RULE_DELETE_VARIANT_NOT_FOUND',
      missingRuleCode: 'QUANTITY_RULE_DELETE_RULE_NOT_FOUND',
      missingRuleMessage: 'Quantity rule not found.',
    });
    priceList = deleted.priceList;
    for (const variantId of deleted.deletedVariantIds) {
      changedVariantIds.add(variantId);
    }
  }

  if (priceList && errors.length === 0) {
    const priceBreakInputs = readQuantityRuleInputs(args, ['quantityPriceBreaksToAdd']);
    const upserted = upsertQuantityPriceBreakNodes(priceList, priceBreakInputs, errors);
    priceList = upserted.priceList;
    for (const variantId of upserted.variantIds) {
      changedVariantIds.add(variantId);
    }
  }

  if (priceList && errors.length === 0) {
    const quantityPriceBreakIdsToDelete = readQuantityVariantIds(args, ['quantityPriceBreaksToDelete']);
    const quantityPriceBreakVariantIdsToDelete = readQuantityVariantIds(args, [
      'quantityPriceBreaksToDeleteByVariantId',
    ]);
    const deleted = deleteQuantityPriceBreakNodes(
      priceList,
      quantityPriceBreakIdsToDelete,
      quantityPriceBreakVariantIdsToDelete,
      errors,
    );
    priceList = deleted.priceList;
    for (const variantId of deleted.variantIds) {
      changedVariantIds.add(variantId);
    }
  }

  if (errors.length === 0 && priceList) {
    store.stageUpdatePriceList(priceList);
  }

  const productVariants =
    errors.length === 0
      ? [...changedVariantIds]
          .map((variantId) => store.getEffectiveVariantById(variantId))
          .filter((variant): variant is ProductVariantRecord => variant !== null)
          .map((variant) => {
            const product = store.getEffectiveProductById(variant.productId);
            return {
              __typename: 'ProductVariant',
              id: variant.id,
              title: variant.title,
              sku: variant.sku,
              product: product ? { __typename: 'Product', id: product.id, title: product.title } : null,
            };
          })
      : null;

  return projectMutationPayload(
    {
      productVariants,
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

function handleWebPresenceDelete(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const errors: MarketUserError[] = [];
  const existingWebPresence = id ? store.getEffectiveWebPresenceRecordById(id) : null;

  if (!id || !existingWebPresence) {
    errors.push(marketError(['id'], "The market web presence wasn't found.", 'WEB_PRESENCE_NOT_FOUND'));
  }

  if (errors.length === 0 && id) {
    store.stageDeleteWebPresence(id);
    removeWebPresenceFromMarkets(id);
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
        result[key] = projectMarketValue(
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
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: marketCursor,
    serializeNode: (market, selection) =>
      projectMarketValue(market.data, selection.selectionSet?.selections ?? [], fragments, variables),
    pageInfoOptions: {
      prefixCursors: false,
    },
  });
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
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: webPresenceCursor,
    serializeNode: (webPresence, selection) =>
      projectMarketValue(webPresence.data, selection.selectionSet?.selections ?? [], fragments, variables),
    pageInfoOptions: {
      prefixCursors: false,
    },
  });
}

function marketRegionCountryEdges(market: MarketRecord): ConnectionEdge[] {
  const conditions = isPlainObject(market.data['conditions']) ? market.data['conditions'] : {};
  const regionsCondition = isPlainObject(conditions['regionsCondition'])
    ? (conditions['regionsCondition'] as Record<string, unknown>)
    : {};
  return readConnectionEdges(regionsCondition['regions']);
}

function marketRegionCountryCode(edge: ConnectionEdge): string | null {
  if (!isPlainObject(edge.node) || typeof edge.node['code'] !== 'string') {
    return null;
  }

  const countryCode = edge.node['code'].toUpperCase();
  return /^[A-Z]{2}$/u.test(countryCode) ? countryCode : null;
}

function marketMatchesBuyerCountry(market: MarketRecord, countryCode: string): boolean {
  if (market.data['status'] !== 'ACTIVE') {
    return false;
  }

  return marketRegionCountryEdges(market).some((edge) => marketRegionCountryCode(edge) === countryCode);
}

function resolveMarketForBuyerCountry(countryCode: string): MarketRecord | null {
  return store.listEffectiveMarkets().find((market) => marketMatchesBuyerCountry(market, countryCode)) ?? null;
}

function resolvedCurrencyCode(market: MarketRecord | null, countryCode: string | null): string {
  if (market) {
    for (const edge of marketRegionCountryEdges(market)) {
      if (countryCode !== null && marketRegionCountryCode(edge) !== countryCode) {
        continue;
      }

      if (
        isPlainObject(edge.node) &&
        isPlainObject(edge.node['currency']) &&
        typeof edge.node['currency']['currencyCode'] === 'string'
      ) {
        return edge.node['currency']['currencyCode'];
      }
    }

    if (
      isPlainObject(market.data['currencySettings']) &&
      isPlainObject(market.data['currencySettings']['baseCurrency']) &&
      typeof market.data['currencySettings']['baseCurrency']['currencyCode'] === 'string'
    ) {
      return market.data['currencySettings']['baseCurrency']['currencyCode'];
    }
  }

  return countryCode ? (COUNTRY_CURRENCIES[countryCode] ?? 'USD') : 'USD';
}

function resolvedPriceInclusivity(market: MarketRecord | null): Record<string, boolean> {
  const priceInclusions = market && isPlainObject(market.data['priceInclusions']) ? market.data['priceInclusions'] : {};
  const dutiesStrategy =
    typeof priceInclusions['inclusiveDutiesPricingStrategy'] === 'string'
      ? priceInclusions['inclusiveDutiesPricingStrategy']
      : null;
  const taxStrategy =
    typeof priceInclusions['inclusiveTaxPricingStrategy'] === 'string'
      ? priceInclusions['inclusiveTaxPricingStrategy']
      : null;

  return {
    dutiesIncluded: dutiesStrategy === 'INCLUDES_DUTIES_IN_PRICE',
    taxesIncluded: taxStrategy === 'INCLUDES_TAXES_IN_PRICE',
  };
}

function webPresenceReferencesMarket(webPresence: WebPresenceRecord, marketId: string): boolean {
  return readConnectionEdges(webPresence.data['markets']).some(
    (edge) => isPlainObject(edge.node) && typeof edge.node['id'] === 'string' && edge.node['id'] === marketId,
  );
}

function webPresencesForMarket(market: MarketRecord | null): WebPresenceRecord[] {
  if (!market) {
    return [];
  }

  const webPresencesById = new Map<string, WebPresenceRecord>();
  for (const edge of readConnectionEdges(market.data['webPresences'])) {
    if (!isPlainObject(edge.node) || typeof edge.node['id'] !== 'string') {
      continue;
    }

    if (store.isWebPresenceDeleted(edge.node['id'])) {
      continue;
    }

    const effectiveWebPresence = store.getEffectiveWebPresenceRecordById(edge.node['id']);
    webPresencesById.set(
      edge.node['id'],
      effectiveWebPresence ?? {
        id: edge.node['id'],
        cursor: edge.cursor,
        data: edge.node as Record<string, JsonValue>,
      },
    );
  }

  for (const webPresence of store.listEffectiveWebPresences()) {
    if (webPresenceReferencesMarket(webPresence, market.id)) {
      webPresencesById.set(webPresence.id, webPresence);
    }
  }

  return Array.from(webPresencesById.values());
}

function catalogsForMarket(market: MarketRecord | null): CatalogRecord[] {
  if (!market) {
    return [];
  }

  const catalogsById = new Map<string, CatalogRecord>();
  for (const edge of readConnectionEdges(market.data['catalogs'])) {
    if (!isPlainObject(edge.node) || typeof edge.node['id'] !== 'string') {
      continue;
    }

    const effectiveCatalog = store.getEffectiveCatalogRecordById(edge.node['id']);
    catalogsById.set(
      edge.node['id'],
      effectiveCatalog ?? {
        id: edge.node['id'],
        cursor: edge.cursor,
        data: edge.node as Record<string, JsonValue>,
      },
    );
  }

  for (const catalog of store.listEffectiveCatalogs()) {
    if (catalogReferencesMarket(catalog, market.id)) {
      catalogsById.set(catalog.id, catalog);
    }
  }

  return Array.from(catalogsById.values());
}

function connectionPayloadFromRecords<
  T extends { id: string; cursor?: string | null | undefined; data: Record<string, JsonValue> },
>(records: T[], getCursorValue: (record: T) => string): Record<string, unknown> {
  const edges = records.map((record) => ({
    cursor: getCursorValue(record),
    node: record.data,
  }));

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

function buildMarketsResolvedValuesPayload(
  market: MarketRecord | null,
  countryCode: string | null,
): Record<string, unknown> {
  return {
    currencyCode: resolvedCurrencyCode(market, countryCode),
    priceInclusivity: resolvedPriceInclusivity(market),
    catalogs: connectionPayloadFromRecords(catalogsForMarket(market), catalogCursor),
    webPresences: connectionPayloadFromRecords(webPresencesForMarket(market), webPresenceCursor),
  };
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

function serializeMarketsResolvedValues(field: FieldNode, variables: Record<string, unknown>): unknown {
  const args = getFieldArguments(field, variables);
  const countryCode = buyerSignalCountryCode(args['buyerSignal']);
  const exactBasePayload = store.getBaseMarketsRootPayload(marketsResolvedValuesPayloadKey(countryCode));
  const wildcardBasePayload = store.getBaseMarketsRootPayload(marketsResolvedValuesPayloadKey(null));
  const legacyBasePayload = store.getBaseMarketsRootPayload('marketsResolvedValues');
  const matchedMarket = countryCode ? resolveMarketForBuyerCountry(countryCode) : null;

  if (!store.hasStagedMarkets() && !store.hasStagedPriceLists() && exactBasePayload !== null) {
    return exactBasePayload;
  }

  if (matchedMarket) {
    return buildMarketsResolvedValuesPayload(matchedMarket, countryCode);
  }

  const fallbackBasePayload = exactBasePayload ?? wildcardBasePayload ?? legacyBasePayload;
  if (fallbackBasePayload !== null) {
    return overlayMarketsResolvedValuesWebPresences(fallbackBasePayload);
  }

  return buildMarketsResolvedValuesPayload(null, countryCode);
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
      return id ? store.getEffectiveCatalogById(id) : null;
    }
    case 'catalogs':
      return serializeCatalogsConnection(field, variables, fragments);
    case 'catalogsCount':
      return serializeCatalogsCount(field, variables);
    case 'priceList': {
      const args = getFieldArguments(field, variables);
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      return id ? store.getEffectivePriceListById(id) : null;
    }
    case 'priceLists':
      return serializePriceListsConnection(field, variables, fragments);
    case 'webPresences':
      return serializeWebPresencesConnection(field, variables, fragments);
    case 'marketsResolvedValues':
      return serializeMarketsResolvedValues(field, variables);
    default:
      return null;
  }
}

export function handleMarketsQuery(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  const errors: Record<string, unknown>[] = [];
  const fragments = getDocumentFragments(document);

  for (const field of getRootFields(document)) {
    if (field.name.value === 'marketsResolvedValues') {
      errors.push(...validateMarketsResolvedValuesBuyerSignal(field, variables));
      if (errors.length > 0) {
        continue;
      }
    }

    const key = getFieldResponseKey(field);
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
          ? projectMarketValue(rootPayload, field.selectionSet.selections, fragments, variables)
          : rootPayload;
  }

  if (errors.length > 0) {
    return { errors };
  }

  return { data };
}

export function handleMarketMutation(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  const fragments = getDocumentFragments(document);

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
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
      case 'catalogCreate':
        data[key] = handleCatalogCreate(field, variables, fragments);
        break;
      case 'catalogUpdate':
        data[key] = handleCatalogUpdate(field, variables, fragments);
        break;
      case 'catalogContextUpdate':
        data[key] = handleCatalogContextUpdate(field, variables, fragments);
        break;
      case 'catalogDelete':
        data[key] = handleCatalogDelete(field, variables, fragments);
        break;
      case 'priceListCreate':
        data[key] = handlePriceListCreate(field, variables, fragments);
        break;
      case 'priceListUpdate':
        data[key] = handlePriceListUpdate(field, variables, fragments);
        break;
      case 'priceListDelete':
        data[key] = handlePriceListDelete(field, variables, fragments);
        break;
      case 'priceListFixedPricesAdd':
        data[key] = handlePriceListFixedPricesAdd(field, variables, fragments);
        break;
      case 'priceListFixedPricesUpdate':
        data[key] = handlePriceListFixedPricesUpdate(field, variables, fragments);
        break;
      case 'priceListFixedPricesDelete':
        data[key] = handlePriceListFixedPricesDelete(field, variables, fragments);
        break;
      case 'priceListFixedPricesByProductUpdate':
        data[key] = handlePriceListFixedPricesByProductUpdate(field, variables, fragments);
        break;
      case 'quantityPricingByVariantUpdate':
        data[key] = handleQuantityPricingByVariantUpdate(field, variables, fragments);
        break;
      case 'quantityRulesAdd':
        data[key] = handleQuantityRulesAdd(field, variables, fragments);
        break;
      case 'quantityRulesDelete':
        data[key] = handleQuantityRulesDelete(field, variables, fragments);
        break;
      case 'webPresenceCreate':
        data[key] = handleWebPresenceCreate(field, variables, fragments);
        break;
      case 'webPresenceUpdate':
        data[key] = handleWebPresenceUpdate(field, variables, fragments);
        break;
      case 'webPresenceDelete':
        data[key] = handleWebPresenceDelete(field, variables, fragments);
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
    const payload = readGraphqlDataResponsePayload(capture, root);
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
