import type { ProxyRuntimeContext } from './runtime-context.js';
import { Buffer } from 'node:buffer';
import { createHash, createHmac } from 'node:crypto';
import { getLocation, Kind, print, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { applySearchQueryTerms, matchesSearchQueryText, type SearchQueryTerm } from '../search-query-parser.js';
import type { BackupRegionRecord, ShopDomainRecord, TaxonomyCategoryRecord } from '../state/types.js';
import {
  handleB2BQuery,
  serializeCompanyAddressNodeById,
  serializeCompanyContactRoleAssignmentNodeById,
} from './b2b.js';
import { handleBulkOperationQuery } from './bulk-operations.js';
import { handleCustomerQuery } from './customers.js';
import { handleDeliveryProfileQuery, serializeDeliveryProfileNestedNodeById } from './delivery-profiles.js';
import { handleDiscountQuery } from './discounts.js';
import { handleFunctionQuery } from './functions.js';
import { handleGiftCardQuery } from './gift-cards.js';
import {
  getDocumentFragments,
  isPlainObject,
  paginateConnectionItems,
  serializeConnection,
  type FragmentMap,
} from './graphql-helpers.js';
import { handleMarketingQuery } from './marketing.js';
import { handleMarketsQuery, serializeMarketWebPresenceNodeById } from './markets.js';
import { serializeFileNodeById } from './media.js';
import { handleMetafieldDefinitionQuery } from './metafield-definitions.js';
import { serializeMetafieldSelectionSet, type MetafieldRecordCore } from './metafields.js';
import { handleMetaobjectDefinitionQuery } from './metaobject-definitions.js';
import { handleOnlineStoreQuery } from './online-store.js';
import { handleOrderQuery } from './orders/query.js';
import { handlePaymentQuery, serializePaymentTermsTemplateNodeById } from './payments.js';
import {
  handleProductQuery,
  serializeProductOptionNodeById,
  serializeProductOptionValueNodeById,
  serializeSellingPlanNodeById,
} from './products.js';
import { serializeSavedSearchNodeById } from './saved-searches.js';
import { handleSegmentsQuery } from './segments.js';
import {
  handleStorePropertiesQuery,
  serializeShopAddressNodeById,
  serializeShopNodeById,
  serializeShopPolicyNodeById,
} from './store-properties.js';
import { handleWebhookSubscriptionQuery } from './webhooks.js';

interface GraphQLResponseError {
  message: string;
  locations?: Array<{
    line: number;
    column: number;
  }>;
  path: Array<string | number>;
  extensions: {
    code: string;
    documentation?: string;
    requiredAccess?: string;
  };
}

interface SerializationContext {
  errors: GraphQLResponseError[];
}

export const ADMIN_PLATFORM_QUERY_ROOTS = new Set([
  'backupRegion',
  'domain',
  'job',
  'node',
  'nodes',
  'publicApiVersions',
  'staffMember',
  'staffMembers',
  'taxonomy',
]);

export const FLOW_UTILITY_MUTATION_ROOTS = new Set(['flowGenerateSignature', 'flowTriggerReceive']);
export const ADMIN_PLATFORM_MUTATION_ROOTS = new Set([
  'backupRegionUpdate',
  'flowGenerateSignature',
  'flowTriggerReceive',
]);

const PUBLIC_API_VERSIONS = [
  { handle: '2025-07', displayName: '2025-07', supported: true },
  { handle: '2025-10', displayName: '2025-10', supported: true },
  { handle: '2026-01', displayName: '2026-01', supported: true },
  { handle: '2026-04', displayName: '2026-04 (Latest)', supported: true },
  { handle: '2026-07', displayName: '2026-07 (Release candidate)', supported: false },
  { handle: 'unstable', displayName: 'unstable', supported: false },
] as const;

const CAPTURED_BACKUP_REGION = {
  __typename: 'MarketRegionCountry',
  id: 'gid://shopify/MarketRegionCountry/4062110417202',
  name: 'Canada',
  code: 'CA',
} as const;

// MarketRegionCountry GIDs are treated as shop-domain-scoped conformance evidence.
// Do not reuse a country mapping for another shop unless that shop/country pair was captured.
const BACKUP_REGION_BY_SHOP_DOMAIN_AND_COUNTRY_CODE: Record<string, Record<string, BackupRegionRecord>> = {
  'harry-test-heelo.myshopify.com': {
    CA: CAPTURED_BACKUP_REGION,
    AE: {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/4062110482738',
      name: 'United Arab Emirates',
      code: 'AE',
    },
    AT: {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/4062110515506',
      name: 'Austria',
      code: 'AT',
    },
    AU: {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/4062110548274',
      name: 'Australia',
      code: 'AU',
    },
    BE: {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/4062110581042',
      name: 'Belgium',
      code: 'BE',
    },
    CH: {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/4062110613810',
      name: 'Switzerland',
      code: 'CH',
    },
    CZ: {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/4062110646578',
      name: 'Czechia',
      code: 'CZ',
    },
    DE: {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/4062110679346',
      name: 'Germany',
      code: 'DE',
    },
    DK: {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/4062110712114',
      name: 'Denmark',
      code: 'DK',
    },
    ES: {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/4062110744882',
      name: 'Spain',
      code: 'ES',
    },
    FI: {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/4062110777650',
      name: 'Finland',
      code: 'FI',
    },
    MX: {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/4062111334706',
      name: 'Mexico',
      code: 'MX',
    },
  },
  'very-big-test-store.myshopify.com': {
    CA: {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/454909493481',
      name: 'Canada',
      code: 'CA',
    },
    US: {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/454910378217',
      name: 'United States',
      code: 'US',
    },
  },
};

const BACKUP_REGION_UPDATE_BY_COUNTRY_CODE: Record<string, BackupRegionRecord> = {
  CA: CAPTURED_BACKUP_REGION,
};

const LOCAL_FLOW_TRIGGER_HANDLE_PREFIXES = ['local-', 'har-374-local'];
const FLOW_TRIGGER_PAYLOAD_LIMIT_BYTES = 50_000;
const FLOW_SIGNATURE_LOCAL_SECRET = 'shopify-draft-proxy-flow-signature-local-secret-v1';

function responseKey(field: FieldNode): string {
  return field.alias?.value ?? field.name.value;
}

function typeConditionApplies(typeCondition: string | undefined, typename: string): boolean {
  return (
    !typeCondition ||
    typeCondition === typename ||
    typeCondition === 'Node' ||
    (typeCondition === 'MarketRegion' && typename === 'MarketRegionCountry')
  );
}

function serializePlainObject(
  source: Record<string, unknown>,
  selections: readonly SelectionNode[],
  typename: string,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (typeConditionApplies(selection.typeCondition?.name.value, typename)) {
        Object.assign(result, serializePlainObject(source, selection.selectionSet.selections, typename));
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = typename;
        break;
      default:
        result[key] = source[selection.name.value] ?? null;
    }
  }

  return result;
}

function serializeUserErrors(
  selections: readonly SelectionNode[],
  errors: Array<{ field: string[]; message: string; code?: string | null }>,
  typename = 'UserError',
): Array<Record<string, unknown>> {
  return errors.map((error) => serializePlainObject(error, selections, typename));
}

function serializePublicApiVersion(
  selections: readonly SelectionNode[],
  version: (typeof PUBLIC_API_VERSIONS)[number],
) {
  return serializePlainObject(version, selections, 'ApiVersion');
}

function serializeQueryRoot(selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (!selection.typeCondition?.name.value || selection.typeCondition.name.value === 'QueryRoot') {
        Object.assign(result, serializeQueryRoot(selection.selectionSet.selections));
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    result[key] = selection.name.value === '__typename' ? 'QueryRoot' : null;
  }

  return result;
}

function serializeJob(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> | null {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' && args['id'].length > 0 ? args['id'] : null;
  if (!id) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (typeConditionApplies(selection.typeCondition?.name.value, 'Job')) {
        Object.assign(result, serializeJobSelection(id, selection.selectionSet.selections));
      }
      continue;
    }

    if (selection.kind === Kind.FIELD) {
      Object.assign(result, serializeJobSelection(id, [selection]));
    }
  }

  return result;
}

function serializeJobSelection(id: string, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Job';
        break;
      case 'id':
        result[key] = id;
        break;
      case 'done':
        result[key] = true;
        break;
      case 'query':
        result[key] = serializeQueryRoot(selection.selectionSet?.selections ?? []);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeDomain(domain: ShopDomainRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (typeConditionApplies(selection.typeCondition?.name.value, 'Domain')) {
        Object.assign(result, serializeDomain(domain, selection.selectionSet.selections));
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Domain';
        break;
      case 'id':
        result[key] = domain.id;
        break;
      case 'host':
        result[key] = domain.host;
        break;
      case 'url':
        result[key] = domain.url;
        break;
      case 'sslEnabled':
        result[key] = domain.sslEnabled;
        break;
      case 'localization':
      case 'marketWebPresence':
      default:
        result[key] = null;
    }
  }

  return result;
}

function readIdArgument(field: FieldNode, variables: Record<string, unknown>): string | null {
  const args = getFieldArguments(field, variables);
  const id = args['id'];
  return typeof id === 'string' && id.length > 0 ? id : null;
}

function serializeDomainRoot(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  const id = readIdArgument(field, variables);
  const primaryDomain = runtime.store.getEffectiveShop()?.primaryDomain ?? null;
  if (!id || !primaryDomain || primaryDomain.id !== id) {
    return null;
  }

  return serializeDomain(primaryDomain, field.selectionSet?.selections ?? []);
}

function serializeMarketRegionCountryNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  selectedFields: readonly FieldNode[],
): Record<string, unknown> | null {
  const effectiveRegion = runtime.store.getEffectiveBackupRegion() ?? CAPTURED_BACKUP_REGION;
  return effectiveRegion.id === id
    ? serializePlainObject(effectiveRegion, selectedFields, 'MarketRegionCountry')
    : null;
}

function collectEffectiveMetafields(runtime: ProxyRuntimeContext): MetafieldRecordCore[] {
  const metafields = new Map<string, MetafieldRecordCore>();
  const addMetafields = (items: readonly MetafieldRecordCore[]) => {
    for (const metafield of items) {
      metafields.set(metafield.id, metafield);
    }
  };

  for (const product of runtime.store.listEffectiveProducts()) {
    addMetafields(runtime.store.getEffectiveMetafieldsByOwnerId(product.id));
    for (const variant of runtime.store.getEffectiveVariantsByProductId(product.id)) {
      addMetafields(runtime.store.getEffectiveMetafieldsByOwnerId(variant.id));
    }
  }
  for (const collection of runtime.store.listEffectiveCollections()) {
    addMetafields(runtime.store.getEffectiveMetafieldsByOwnerId(collection.id));
  }
  for (const customer of runtime.store.listEffectiveCustomers()) {
    addMetafields(runtime.store.getEffectiveMetafieldsByCustomerId(customer.id));
  }
  for (const discount of runtime.store.listEffectiveDiscounts()) {
    addMetafields(discount.metafields ?? []);
  }

  return [...metafields.values()];
}

function serializeMetafieldNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  selectedFields: readonly FieldNode[],
): Record<string, unknown> | null {
  const metafield = collectEffectiveMetafields(runtime).find((candidate) => candidate.id === id) ?? null;
  return metafield ? serializeMetafieldSelectionSet(metafield, selectedFields) : null;
}

function taxonomyCategoryCursor(category: TaxonomyCategoryRecord): string {
  return category.cursor ?? category.id;
}

function taxonomyCategoryCursorSortKey(category: TaxonomyCategoryRecord): number | null {
  if (!category.cursor) {
    return null;
  }

  try {
    const decoded = JSON.parse(Buffer.from(category.cursor, 'base64').toString('utf8')) as unknown;
    if (!isPlainObject(decoded)) {
      return null;
    }
    const id = decoded['id'];
    return typeof id === 'number' && Number.isFinite(id) ? id : null;
  } catch {
    return null;
  }
}

function sortTaxonomyHierarchyCategories(categories: TaxonomyCategoryRecord[]): TaxonomyCategoryRecord[] {
  return categories
    .map((category, index) => ({
      category,
      index,
      sortKey: taxonomyCategoryCursorSortKey(category),
    }))
    .sort((left, right) => {
      if (left.sortKey !== null && right.sortKey !== null && left.sortKey !== right.sortKey) {
        return left.sortKey - right.sortKey;
      }

      return left.index - right.index;
    })
    .map(({ category }) => category);
}

function taxonomyCategoryMatchesSearchTerm(category: TaxonomyCategoryRecord, term: SearchQueryTerm): boolean {
  switch (term.field) {
    case null:
      return (
        matchesSearchQueryText(category.name, term) ||
        matchesSearchQueryText(category.fullName, term) ||
        matchesSearchQueryText(category.id, term)
      );
    case 'id':
      return matchesSearchQueryText(category.id, term);
    case 'name':
      return matchesSearchQueryText(category.name, term);
    case 'full_name':
    case 'fullName':
      return matchesSearchQueryText(category.fullName, term);
    default:
      return false;
  }
}

function serializeTaxonomyCategory(
  category: TaxonomyCategoryRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (typeConditionApplies(selection.typeCondition?.name.value, 'TaxonomyCategory')) {
        Object.assign(result, serializeTaxonomyCategory(category, selection.selectionSet.selections));
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'TaxonomyCategory';
        break;
      case 'id':
        result[key] = category.id;
        break;
      case 'name':
        result[key] = category.name;
        break;
      case 'fullName':
        result[key] = category.fullName;
        break;
      case 'isRoot':
        result[key] = category.isRoot;
        break;
      case 'isLeaf':
        result[key] = category.isLeaf;
        break;
      case 'level':
        result[key] = category.level;
        break;
      case 'parentId':
        result[key] = category.parentId;
        break;
      case 'ancestorIds':
        result[key] = [...category.ancestorIds];
        break;
      case 'childrenIds':
        result[key] = [...category.childrenIds];
        break;
      case 'isArchived':
        result[key] = category.isArchived;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeTaxonomyCategoryNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  selectedFields: readonly FieldNode[],
): Record<string, unknown> | null {
  const category = runtime.store.getEffectiveTaxonomyCategories().find((candidate) => candidate.id === id) ?? null;
  return category ? serializeTaxonomyCategory(category, selectedFields) : null;
}

function serializeTaxonomyCategories(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const rawChildrenOf = args['childrenOf'];
  const rawDescendantsOf = args['descendantsOf'];
  const rawSiblingsOf = args['siblingsOf'];
  const rawSearch = args['search'];
  const allCategories = runtime.store.getEffectiveTaxonomyCategories();
  const hasHierarchyFilter =
    (typeof rawChildrenOf === 'string' && rawChildrenOf.length > 0) ||
    (typeof rawDescendantsOf === 'string' && rawDescendantsOf.length > 0) ||
    (typeof rawSiblingsOf === 'string' && rawSiblingsOf.length > 0);
  const hierarchyFilteredCategories =
    typeof rawChildrenOf === 'string' && rawChildrenOf.length > 0
      ? allCategories.filter((category) => category.parentId === rawChildrenOf)
      : typeof rawDescendantsOf === 'string' && rawDescendantsOf.length > 0
        ? allCategories.filter((category) => category.ancestorIds.includes(rawDescendantsOf))
        : typeof rawSiblingsOf === 'string' && rawSiblingsOf.length > 0
          ? allCategories.filter((category) => {
              const siblingSubject = allCategories.find((candidate) => candidate.id === rawSiblingsOf) ?? null;
              return (
                siblingSubject?.parentId !== undefined &&
                category.parentId === siblingSubject.parentId &&
                category.id !== rawSiblingsOf
              );
            })
          : allCategories;
  const orderedHierarchyFilteredCategories = hasHierarchyFilter
    ? sortTaxonomyHierarchyCategories(hierarchyFilteredCategories)
    : hierarchyFilteredCategories;
  const categories =
    typeof rawSearch === 'string' && rawSearch.trim().length > 0
      ? applySearchQueryTerms(
          orderedHierarchyFilteredCategories,
          rawSearch,
          { ignoredKeywords: ['AND'], dropEmptyValues: true },
          taxonomyCategoryMatchesSearchTerm,
        )
      : orderedHierarchyFilteredCategories;
  const window = paginateConnectionItems(categories, field, variables, taxonomyCategoryCursor);
  const hasPreviousPage = typeof args['last'] === 'number' ? window.hasPreviousPage : false;

  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage,
    getCursorValue: taxonomyCategoryCursor,
    pageInfoOptions: { prefixCursors: false },
    serializeNode: (category, selection) =>
      serializeTaxonomyCategory(category, selection.selectionSet?.selections ?? []),
  });
}

function serializeTaxonomy(
  runtime: ProxyRuntimeContext,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (!selection.typeCondition?.name.value || selection.typeCondition.name.value === 'Taxonomy') {
        Object.assign(result, serializeTaxonomy(runtime, selection.selectionSet.selections, variables));
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Taxonomy';
        break;
      case 'categories':
        result[key] = serializeTaxonomyCategories(runtime, selection, variables);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

type LocalNodeQueryHandler = (
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
) => unknown;

interface LocalNodeResolver {
  rootField: string;
  typename: string;
  handler: LocalNodeQueryHandler;
  typeConditions?: readonly string[];
}

type DirectLocalNodeResolver = Omit<LocalNodeResolver, 'handler' | 'rootField'> & {
  rootField?: undefined;
  serialize: (
    runtime: ProxyRuntimeContext,
    id: string,
    selectedFields: readonly FieldNode[],
    variables: Record<string, unknown>,
    fragments: FragmentMap,
  ) => Record<string, unknown> | null;
};

type AdminPlatformNodeResolver = LocalNodeResolver | DirectLocalNodeResolver;

const PROXY_NODE_RESPONSE_KEY = 'proxyNode';
const PROXY_NODE_ID_VARIABLE = 'proxyNodeId';
const handleProductNodeQuery: LocalNodeQueryHandler = (runtime, document, variables) =>
  handleProductQuery(runtime, document, variables, 'snapshot');

const LOCAL_NODE_RESOLVERS: Record<string, AdminPlatformNodeResolver> = {
  Product: { rootField: 'product', typename: 'Product', handler: handleProductNodeQuery },
  ProductVariant: { rootField: 'productVariant', typename: 'ProductVariant', handler: handleProductNodeQuery },
  ProductOption: {
    typename: 'ProductOption',
    serialize: (runtime, id, selectedFields) => serializeProductOptionNodeById(runtime, id, selectedFields),
  },
  ProductOptionValue: {
    typename: 'ProductOptionValue',
    serialize: (runtime, id, selectedFields) => serializeProductOptionValueNodeById(runtime, id, selectedFields),
  },
  Metafield: {
    typename: 'Metafield',
    serialize: (runtime, id, selectedFields) => serializeMetafieldNodeById(runtime, id, selectedFields),
  },
  InventoryItem: { rootField: 'inventoryItem', typename: 'InventoryItem', handler: handleProductNodeQuery },
  InventoryLevel: { rootField: 'inventoryLevel', typename: 'InventoryLevel', handler: handleProductNodeQuery },
  InventoryShipment: { rootField: 'inventoryShipment', typename: 'InventoryShipment', handler: handleProductNodeQuery },
  InventoryTransfer: { rootField: 'inventoryTransfer', typename: 'InventoryTransfer', handler: handleProductNodeQuery },
  Collection: { rootField: 'collection', typename: 'Collection', handler: handleProductNodeQuery },
  Channel: { rootField: 'channel', typename: 'Channel', handler: handleProductNodeQuery },
  Publication: { rootField: 'publication', typename: 'Publication', handler: handleProductNodeQuery },
  ProductFeed: { rootField: 'productFeed', typename: 'ProductFeed', handler: handleProductNodeQuery },
  ProductBundleOperation: {
    rootField: 'productOperation',
    typename: 'ProductBundleOperation',
    handler: handleProductNodeQuery,
    typeConditions: ['ProductOperation'],
  },
  ProductDuplicateOperation: {
    rootField: 'productOperation',
    typename: 'ProductDuplicateOperation',
    handler: handleProductNodeQuery,
    typeConditions: ['ProductOperation'],
  },
  ProductSetOperation: {
    rootField: 'productOperation',
    typename: 'ProductSetOperation',
    handler: handleProductNodeQuery,
    typeConditions: ['ProductOperation'],
  },
  SellingPlan: {
    typename: 'SellingPlan',
    serialize: (runtime, id, selectedFields) =>
      serializeSellingPlanNodeById(runtime, id, syntheticNodeField(selectedFields)),
  },
  SellingPlanGroup: { rootField: 'sellingPlanGroup', typename: 'SellingPlanGroup', handler: handleProductNodeQuery },

  Customer: { rootField: 'customer', typename: 'Customer', handler: handleCustomerQuery },
  CustomerPaymentMethod: {
    rootField: 'customerPaymentMethod',
    typename: 'CustomerPaymentMethod',
    handler: handleCustomerQuery,
  },
  StoreCreditAccount: { rootField: 'storeCreditAccount', typename: 'StoreCreditAccount', handler: handleCustomerQuery },

  Company: { rootField: 'company', typename: 'Company', handler: handleB2BQuery },
  CompanyAddress: {
    typename: 'CompanyAddress',
    serialize: (runtime, id, selectedFields) => serializeCompanyAddressNodeById(runtime, id, selectedFields),
  },
  CompanyContact: { rootField: 'companyContact', typename: 'CompanyContact', handler: handleB2BQuery },
  CompanyContactRole: { rootField: 'companyContactRole', typename: 'CompanyContactRole', handler: handleB2BQuery },
  CompanyContactRoleAssignment: {
    typename: 'CompanyContactRoleAssignment',
    serialize: (runtime, id, selectedFields) =>
      serializeCompanyContactRoleAssignmentNodeById(runtime, id, selectedFields),
  },
  CompanyLocation: { rootField: 'companyLocation', typename: 'CompanyLocation', handler: handleB2BQuery },

  BusinessEntity: { rootField: 'businessEntity', typename: 'BusinessEntity', handler: handleStorePropertiesQuery },
  Location: { rootField: 'location', typename: 'Location', handler: handleStorePropertiesQuery },
  Shop: {
    typename: 'Shop',
    serialize: (runtime, id, selectedFields) => serializeShopNodeById(runtime, id, selectedFields),
  },
  ShopAddress: {
    typename: 'ShopAddress',
    serialize: (runtime, id, selectedFields) => serializeShopAddressNodeById(runtime, id, selectedFields),
  },
  ShopPolicy: {
    typename: 'ShopPolicy',
    serialize: (runtime, id, selectedFields) => serializeShopPolicyNodeById(runtime, id, selectedFields),
  },
  DeliveryCarrierService: {
    rootField: 'carrierService',
    typename: 'DeliveryCarrierService',
    handler: handleStorePropertiesQuery,
  },

  MarketRegionCountry: {
    typename: 'MarketRegionCountry',
    typeConditions: ['MarketRegion'],
    serialize: (runtime, id, selectedFields) => serializeMarketRegionCountryNodeById(runtime, id, selectedFields),
  },
  TaxonomyCategory: {
    typename: 'TaxonomyCategory',
    serialize: (runtime, id, selectedFields) => serializeTaxonomyCategoryNodeById(runtime, id, selectedFields),
  },

  PaymentCustomization: {
    rootField: 'paymentCustomization',
    typename: 'PaymentCustomization',
    handler: handlePaymentQuery,
  },
  CashTrackingSession: {
    rootField: 'cashTrackingSession',
    typename: 'CashTrackingSession',
    handler: handlePaymentQuery,
  },
  PointOfSaleDevice: {
    rootField: 'pointOfSaleDevice',
    typename: 'PointOfSaleDevice',
    handler: handlePaymentQuery,
  },
  ShopifyPaymentsDispute: {
    rootField: 'dispute',
    typename: 'ShopifyPaymentsDispute',
    handler: handlePaymentQuery,
  },
  PaymentTermsTemplate: {
    typename: 'PaymentTermsTemplate',
    serialize: (runtime, id, selectedFields) =>
      serializePaymentTermsTemplateNodeById(runtime, id, syntheticNodeField(selectedFields)),
  },
  Validation: { rootField: 'validation', typename: 'Validation', handler: handleFunctionQuery },

  BulkOperation: { rootField: 'bulkOperation', typename: 'BulkOperation', handler: handleBulkOperationQuery },
  MetafieldDefinition: {
    rootField: 'metafieldDefinition',
    typename: 'MetafieldDefinition',
    handler: handleMetafieldDefinitionQuery,
  },
  Metaobject: { rootField: 'metaobject', typename: 'Metaobject', handler: handleMetaobjectDefinitionQuery },
  MetaobjectDefinition: {
    rootField: 'metaobjectDefinition',
    typename: 'MetaobjectDefinition',
    handler: handleMetaobjectDefinitionQuery,
  },

  Order: { rootField: 'order', typename: 'Order', handler: handleOrderQuery },
  Return: { rootField: 'return', typename: 'Return', handler: handleOrderQuery },
  Fulfillment: { rootField: 'fulfillment', typename: 'Fulfillment', handler: handleOrderQuery },
  FulfillmentOrder: { rootField: 'fulfillmentOrder', typename: 'FulfillmentOrder', handler: handleOrderQuery },
  ReverseDelivery: { rootField: 'reverseDelivery', typename: 'ReverseDelivery', handler: handleOrderQuery },
  ReverseFulfillmentOrder: {
    rootField: 'reverseFulfillmentOrder',
    typename: 'ReverseFulfillmentOrder',
    handler: handleOrderQuery,
  },
  DraftOrder: { rootField: 'draftOrder', typename: 'DraftOrder', handler: handleOrderQuery },
  Abandonment: { rootField: 'abandonment', typename: 'Abandonment', handler: handleOrderQuery },

  GiftCard: { rootField: 'giftCard', typename: 'GiftCard', handler: handleGiftCardQuery },
  DeliveryProfile: { rootField: 'deliveryProfile', typename: 'DeliveryProfile', handler: handleDeliveryProfileQuery },
  DeliveryCondition: {
    typename: 'DeliveryCondition',
    serialize: (runtime, id, selectedFields, variables) =>
      serializeDeliveryProfileNestedNodeById(runtime, id, 'DeliveryCondition', selectedFields, variables),
  },
  DeliveryCountry: {
    typename: 'DeliveryCountry',
    serialize: (runtime, id, selectedFields, variables) =>
      serializeDeliveryProfileNestedNodeById(runtime, id, 'DeliveryCountry', selectedFields, variables),
  },
  DeliveryLocationGroup: {
    typename: 'DeliveryLocationGroup',
    serialize: (runtime, id, selectedFields, variables) =>
      serializeDeliveryProfileNestedNodeById(runtime, id, 'DeliveryLocationGroup', selectedFields, variables),
  },
  DeliveryMethodDefinition: {
    typename: 'DeliveryMethodDefinition',
    serialize: (runtime, id, selectedFields, variables) =>
      serializeDeliveryProfileNestedNodeById(runtime, id, 'DeliveryMethodDefinition', selectedFields, variables),
  },
  DeliveryParticipant: {
    typename: 'DeliveryParticipant',
    serialize: (runtime, id, selectedFields, variables) =>
      serializeDeliveryProfileNestedNodeById(runtime, id, 'DeliveryParticipant', selectedFields, variables),
  },
  DeliveryProvince: {
    typename: 'DeliveryProvince',
    serialize: (runtime, id, selectedFields, variables) =>
      serializeDeliveryProfileNestedNodeById(runtime, id, 'DeliveryProvince', selectedFields, variables),
  },
  DeliveryRateDefinition: {
    typename: 'DeliveryRateDefinition',
    serialize: (runtime, id, selectedFields, variables) =>
      serializeDeliveryProfileNestedNodeById(runtime, id, 'DeliveryRateDefinition', selectedFields, variables),
  },
  DeliveryZone: {
    typename: 'DeliveryZone',
    serialize: (runtime, id, selectedFields, variables) =>
      serializeDeliveryProfileNestedNodeById(runtime, id, 'DeliveryZone', selectedFields, variables),
  },

  DiscountCodeNode: { rootField: 'codeDiscountNode', typename: 'DiscountCodeNode', handler: handleDiscountQuery },
  DiscountAutomaticNode: {
    rootField: 'automaticDiscountNode',
    typename: 'DiscountAutomaticNode',
    handler: handleDiscountQuery,
  },
  DiscountNode: { rootField: 'discountNode', typename: 'DiscountNode', handler: handleDiscountQuery },

  MarketingActivity: { rootField: 'marketingActivity', typename: 'MarketingActivity', handler: handleMarketingQuery },
  MarketingEvent: { rootField: 'marketingEvent', typename: 'MarketingEvent', handler: handleMarketingQuery },
  WebhookSubscription: {
    rootField: 'webhookSubscription',
    typename: 'WebhookSubscription',
    handler: handleWebhookSubscriptionQuery,
  },
  Segment: { rootField: 'segment', typename: 'Segment', handler: handleSegmentsQuery },
  CustomerSegmentMembersQuery: {
    rootField: 'customerSegmentMembersQuery',
    typename: 'CustomerSegmentMembersQuery',
    handler: handleSegmentsQuery,
  },

  Market: { rootField: 'market', typename: 'Market', handler: handleMarketsQuery },
  MarketWebPresence: {
    typename: 'MarketWebPresence',
    serialize: (runtime, id, selectedFields, variables, fragments) =>
      serializeMarketWebPresenceNodeById(runtime, id, syntheticNodeField(selectedFields), variables, fragments),
  },
  MarketCatalog: {
    rootField: 'catalog',
    typename: 'MarketCatalog',
    handler: handleMarketsQuery,
    typeConditions: ['Catalog'],
  },
  PriceList: { rootField: 'priceList', typename: 'PriceList', handler: handleMarketsQuery },

  Article: { rootField: 'article', typename: 'Article', handler: handleOnlineStoreQuery },
  Blog: { rootField: 'blog', typename: 'Blog', handler: handleOnlineStoreQuery },
  Page: { rootField: 'page', typename: 'Page', handler: handleOnlineStoreQuery },
  Comment: { rootField: 'comment', typename: 'Comment', handler: handleOnlineStoreQuery },
  OnlineStoreTheme: { rootField: 'theme', typename: 'OnlineStoreTheme', handler: handleOnlineStoreQuery },
  ScriptTag: { rootField: 'scriptTag', typename: 'ScriptTag', handler: handleOnlineStoreQuery },
  WebPixel: { rootField: 'webPixel', typename: 'WebPixel', handler: handleOnlineStoreQuery },
  ServerPixel: { rootField: 'serverPixel', typename: 'ServerPixel', handler: handleOnlineStoreQuery },

  GenericFile: {
    typename: 'GenericFile',
    typeConditions: ['File'],
    serialize: (runtime, id, selectedFields) => serializeFileNodeById(runtime, id, selectedFields),
  },
  MediaImage: {
    typename: 'MediaImage',
    typeConditions: ['File'],
    serialize: (runtime, id, selectedFields) => serializeFileNodeById(runtime, id, selectedFields),
  },
  Video: {
    typename: 'Video',
    typeConditions: ['File'],
    serialize: (runtime, id, selectedFields) => serializeFileNodeById(runtime, id, selectedFields),
  },
  ExternalVideo: {
    typename: 'ExternalVideo',
    typeConditions: ['File'],
    serialize: (runtime, id, selectedFields) => serializeFileNodeById(runtime, id, selectedFields),
  },
  Model3d: {
    typename: 'Model3d',
    typeConditions: ['File'],
    serialize: (runtime, id, selectedFields) => serializeFileNodeById(runtime, id, selectedFields),
  },
  SavedSearch: {
    typename: 'SavedSearch',
    serialize: (runtime, id, selectedFields, _variables, fragments) =>
      serializeSavedSearchNodeById(runtime, id, syntheticNodeField(selectedFields), fragments),
  },

};
const DISCOUNT_NODE_RESOLVER = LOCAL_NODE_RESOLVERS['DiscountNode'];

function syntheticNodeField(selectedFields: readonly FieldNode[]): FieldNode {
  return {
    kind: Kind.FIELD,
    name: {
      kind: Kind.NAME,
      value: PROXY_NODE_RESPONSE_KEY,
    },
    selectionSet: {
      kind: Kind.SELECTION_SET,
      selections: [...selectedFields],
    },
  };
}

export function listSupportedAdminPlatformNodeTypes(): string[] {
  return ['Domain', ...Object.values(LOCAL_NODE_RESOLVERS).map((resolver) => resolver.typename)].sort((left, right) =>
    left.localeCompare(right),
  );
}

export function listAdminPlatformNodeResolverEntries(): Array<{
  gidType: string;
  nodeType: string;
  rootField: string | null;
}> {
  return Object.entries(LOCAL_NODE_RESOLVERS)
    .map(([gidType, resolver]) => ({
      gidType,
      nodeType: resolver.typename,
      rootField: resolver.rootField ?? null,
    }))
    .concat({ gidType: 'Domain', nodeType: 'Domain', rootField: 'domain' })
    .sort((left, right) => left.nodeType.localeCompare(right.nodeType));
}

function readShopifyGidType(id: string): string | null {
  const match = /^gid:\/\/shopify\/([^/?#]+)\/[^?#]+(?:[?#].*)?$/u.exec(id);
  return match?.[1] ?? null;
}

function nodeTypeConditionApplies(typeCondition: string | undefined, resolver: AdminPlatformNodeResolver): boolean {
  if (!typeCondition || typeCondition === 'Node' || typeCondition === resolver.typename) {
    return true;
  }

  return resolver.typeConditions?.includes(typeCondition) ?? false;
}

function collectApplicableNodeFields(
  selections: readonly SelectionNode[],
  resolver: AdminPlatformNodeResolver,
  fragments: FragmentMap,
): FieldNode[] {
  return selections.flatMap((selection): FieldNode[] => {
    if (selection.kind === Kind.FIELD) {
      return [selection];
    }

    if (selection.kind === Kind.INLINE_FRAGMENT) {
      return nodeTypeConditionApplies(selection.typeCondition?.name.value, resolver)
        ? collectApplicableNodeFields(selection.selectionSet.selections, resolver, fragments)
        : [];
    }

    if (selection.kind === Kind.FRAGMENT_SPREAD) {
      const fragment = fragments.get(selection.name.value);
      return fragment && nodeTypeConditionApplies(fragment.typeCondition.name.value, resolver)
        ? collectApplicableNodeFields(fragment.selectionSet.selections, resolver, fragments)
        : [];
    }

    return [];
  });
}

function hasExplicitNodeTypeSelection(
  selections: readonly SelectionNode[],
  typeName: string,
  fragments: FragmentMap,
): boolean {
  return selections.some((selection) => {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      return (
        selection.typeCondition?.name.value === typeName ||
        hasExplicitNodeTypeSelection(selection.selectionSet.selections, typeName, fragments)
      );
    }

    if (selection.kind === Kind.FRAGMENT_SPREAD) {
      const fragment = fragments.get(selection.name.value);
      return (
        fragment?.typeCondition.name.value === typeName ||
        (fragment ? hasExplicitNodeTypeSelection(fragment.selectionSet.selections, typeName, fragments) : false)
      );
    }

    return false;
  });
}

function applySelectedTypename(
  payload: Record<string, unknown>,
  fields: readonly FieldNode[],
  typename: string,
): Record<string, unknown> {
  for (const field of fields) {
    if (field.name.value === '__typename') {
      payload[responseKey(field)] = typename;
    }
  }
  return payload;
}

function serializeLocalNodeWithResolver(
  runtime: ProxyRuntimeContext,
  id: string,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
  fragments: FragmentMap,
  resolver: AdminPlatformNodeResolver,
): Record<string, unknown> | null {
  const selectedFields = collectApplicableNodeFields(selections, resolver, fragments);
  if (selectedFields.length === 0) {
    return {};
  }

  if ('serialize' in resolver) {
    return resolver.serialize(runtime, id, selectedFields, variables, fragments);
  }

  const syntheticDocument = `query ProxyNodeLookup { ${PROXY_NODE_RESPONSE_KEY}: ${resolver.rootField}(id: $${PROXY_NODE_ID_VARIABLE}) { ${selectedFields.map((field) => print(field)).join('\n')} } }`;
  const response = resolver.handler(runtime, syntheticDocument, {
    ...variables,
    [PROXY_NODE_ID_VARIABLE]: id,
  });
  const payload =
    isPlainObject(response) && isPlainObject(response['data'])
      ? (response['data'][PROXY_NODE_RESPONSE_KEY] ?? null)
      : null;
  return isPlainObject(payload) ? applySelectedTypename(payload, selectedFields, resolver.typename) : null;
}

function serializeLocalNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> | null {
  if (id.startsWith('gid://shopify/Domain/')) {
    const primaryDomain = runtime.store.getEffectiveShop()?.primaryDomain ?? null;
    return primaryDomain?.id === id ? serializeDomain(primaryDomain, selections) : null;
  }

  const gidType = readShopifyGidType(id);
  const resolver = gidType ? LOCAL_NODE_RESOLVERS[gidType] : undefined;
  if (!resolver) {
    return null;
  }

  if (
    (gidType === 'DiscountCodeNode' || gidType === 'DiscountAutomaticNode') &&
    hasExplicitNodeTypeSelection(selections, 'DiscountNode', fragments)
  ) {
    return DISCOUNT_NODE_RESOLVER
      ? serializeLocalNodeWithResolver(runtime, id, selections, variables, fragments, DISCOUNT_NODE_RESOLVER)
      : null;
  }

  return serializeLocalNodeWithResolver(runtime, id, selections, variables, fragments, resolver);
}

function serializeNode(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> | null {
  const id = readIdArgument(field, variables);
  if (!id) {
    return null;
  }

  return serializeLocalNodeById(runtime, id, field.selectionSet?.selections ?? [], variables, fragments);
}

function fieldLocation(field: FieldNode): Array<{ line: number; column: number }> | undefined {
  if (!field.loc) {
    return undefined;
  }

  const location = getLocation(field.loc.source, field.loc.start);
  return [{ line: location.line, column: location.column }];
}

function staffMemberAccessDeniedError(field: FieldNode): GraphQLResponseError {
  const path = responseKey(field);
  const locations = fieldLocation(field);
  if (path === 'staffMember') {
    const error: GraphQLResponseError = {
      message:
        'Access denied for staffMember field. Required access: `read_users` access scope. Also: The app must be a finance embedded app or installed on a Shopify Plus or Advanced store. Contact Shopify Support to enable this scope for your app.',
      path: [path],
      extensions: {
        code: 'ACCESS_DENIED',
        documentation: 'https://shopify.dev/api/usage/access-scopes',
        requiredAccess:
          '`read_users` access scope. Also: The app must be a finance embedded app or installed on a Shopify Plus or Advanced store. Contact Shopify Support to enable this scope for your app.',
      },
    };
    if (locations) {
      error.locations = locations;
    }
    return error;
  }

  const error: GraphQLResponseError = {
    message: 'Access denied for staffMembers field.',
    path: [path],
    extensions: {
      code: 'ACCESS_DENIED',
      documentation: 'https://shopify.dev/api/usage/access-scopes',
    },
  };
  if (locations) {
    error.locations = locations;
  }
  return error;
}

function flowGenerateSignatureResourceNotFoundError(field: FieldNode, id: string): GraphQLResponseError {
  const error: GraphQLResponseError = {
    message: `Invalid id: ${id}`,
    path: [responseKey(field)],
    extensions: {
      code: 'RESOURCE_NOT_FOUND',
    },
  };
  const locations = fieldLocation(field);
  if (locations) {
    error.locations = locations;
  }
  return error;
}

function readIdListArgument(field: FieldNode, variables: Record<string, unknown>): string[] {
  const args = getFieldArguments(field, variables);
  const ids = args['ids'];
  return Array.isArray(ids) ? ids.filter((id): id is string => typeof id === 'string') : [];
}

function hashString(value: string): string {
  return createHash('sha256').update(value).digest('hex');
}

function stableJsonStringify(value: unknown): string {
  if (value === null || typeof value !== 'object') {
    return JSON.stringify(value);
  }

  if (Array.isArray(value)) {
    return `[${value.map((item) => stableJsonStringify(item)).join(',')}]`;
  }

  return `{${Object.entries(value)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, item]) => `${JSON.stringify(key)}:${stableJsonStringify(item)}`)
    .join(',')}}`;
}

function payloadBytes(payload: unknown): number {
  return Buffer.byteLength(stableJsonStringify(payload), 'utf8');
}

function isLocalFlowTriggerHandle(handle: string | null): boolean {
  return handle !== null && LOCAL_FLOW_TRIGGER_HANDLE_PREFIXES.some((prefix) => handle.startsWith(prefix));
}

function buildFlowSignature(flowTriggerId: string, payload: string): string {
  return createHmac('sha256', FLOW_SIGNATURE_LOCAL_SECRET)
    .update(flowTriggerId)
    .update('\0')
    .update(payload)
    .digest('hex');
}

function serializeFlowGenerateSignaturePayload(
  field: FieldNode,
  payload: string,
  signature: string,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (
        !selection.typeCondition?.name.value ||
        selection.typeCondition.name.value === 'FlowGenerateSignaturePayload'
      ) {
        Object.assign(
          result,
          serializeFlowGenerateSignaturePayload({ ...field, selectionSet: selection.selectionSet }, payload, signature),
        );
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case 'payload':
        result[key] = payload;
        break;
      case 'signature':
        result[key] = signature;
        break;
      case 'userErrors':
        result[key] = serializeUserErrors(selection.selectionSet?.selections ?? [], []);
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeFlowTriggerReceivePayload(
  field: FieldNode,
  errors: Array<{ field: string[]; message: string }>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    result[key] =
      selection.name.value === 'userErrors'
        ? serializeUserErrors(selection.selectionSet?.selections ?? [], errors)
        : null;
  }
  return result;
}

function serializeBackupRegionUpdatePayload(
  field: FieldNode,
  region: BackupRegionRecord | null,
  errors: Array<{ field: string[]; message: string; code: string }>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case 'backupRegion':
        result[key] = region
          ? serializePlainObject(region, selection.selectionSet?.selections ?? [], region.__typename)
          : null;
        break;
      case 'userErrors':
        result[key] = serializeUserErrors(selection.selectionSet?.selections ?? [], errors, 'MarketUserError');
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function normalizeCountryCode(countryCode: string | null | undefined): string | null {
  const normalized = countryCode?.trim().toUpperCase() ?? '';
  return /^[A-Z]{2}$/u.test(normalized) ? normalized : null;
}

function resolveBackupRegion(runtime: ProxyRuntimeContext): BackupRegionRecord | null {
  const stagedOrBaseRegion = runtime.store.getEffectiveBackupRegion();
  if (stagedOrBaseRegion) {
    return stagedOrBaseRegion;
  }

  const shop = runtime.store.getEffectiveShop();
  if (!shop) {
    return CAPTURED_BACKUP_REGION;
  }

  const countryCode = normalizeCountryCode(shop.shopAddress.countryCodeV2);
  if (!countryCode) {
    return null;
  }

  return BACKUP_REGION_BY_SHOP_DOMAIN_AND_COUNTRY_CODE[shop.myshopifyDomain]?.[countryCode] ?? null;
}

export interface AdminPlatformMutationResult {
  response: Record<string, unknown>;
  stagedResourceIds?: string[];
  staged: boolean;
  notes?: string;
}

export function handleAdminPlatformQuery(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const fields = getRootFields(document);
  const fragments = getDocumentFragments(document);
  const context: SerializationContext = { errors: [] };
  const data: Record<string, unknown> = {};

  for (const field of fields) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'publicApiVersions':
        data[key] = PUBLIC_API_VERSIONS.map((version) =>
          serializePublicApiVersion(field.selectionSet?.selections ?? [], version),
        );
        break;
      case 'node':
        data[key] = serializeNode(runtime, field, variables, fragments);
        break;
      case 'nodes':
        data[key] = readIdListArgument(field, variables).map((id) =>
          serializeLocalNodeById(runtime, id, field.selectionSet?.selections ?? [], variables, fragments),
        );
        break;
      case 'job':
        data[key] = serializeJob(field, variables);
        break;
      case 'domain':
        data[key] = serializeDomainRoot(runtime, field, variables);
        break;
      case 'backupRegion': {
        const region = resolveBackupRegion(runtime);
        data[key] = region
          ? serializePlainObject(region, field.selectionSet?.selections ?? [], region.__typename)
          : null;
        break;
      }
      case 'taxonomy':
        data[key] = serializeTaxonomy(runtime, field.selectionSet?.selections ?? [], variables);
        break;
      case 'staffMember':
      case 'staffMembers':
        data[key] = null;
        context.errors.push(staffMemberAccessDeniedError(field));
        break;
      default:
        data[key] = null;
    }
  }

  return context.errors.length > 0 ? { data, errors: context.errors } : { data };
}

export function handleAdminPlatformMutation(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): AdminPlatformMutationResult | null {
  const fields = getRootFields(document);
  const data: Record<string, unknown> = {};
  const errors: GraphQLResponseError[] = [];
  const stagedResourceIds: string[] = [];
  const notes: string[] = [];
  let staged = false;

  for (const field of fields) {
    const key = responseKey(field);
    const args = getFieldArguments(field, variables);
    switch (field.name.value) {
      case 'flowGenerateSignature': {
        const flowTriggerId = typeof args['id'] === 'string' ? args['id'] : '';
        const payload = typeof args['payload'] === 'string' ? args['payload'] : '';
        if (!/^gid:\/\/shopify\/FlowTrigger\/[1-9][0-9]*$/u.test(flowTriggerId)) {
          data[key] = null;
          errors.push(flowGenerateSignatureResourceNotFoundError(field, flowTriggerId));
          break;
        }

        const signature = buildFlowSignature(flowTriggerId, payload);
        const recordId = runtime.syntheticIdentity.makeSyntheticGid('FlowGenerateSignature');
        runtime.store.stageAdminPlatformFlowSignature({
          id: recordId,
          flowTriggerId,
          payloadSha256: hashString(payload),
          signatureSha256: hashString(signature),
          createdAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
        });
        stagedResourceIds.push(recordId);
        staged = true;
        notes.push(
          'Generated a deterministic proxy-local Flow signature without exposing or storing a Shopify secret.',
        );
        data[key] = serializeFlowGenerateSignaturePayload(field, payload, signature);
        break;
      }
      case 'flowTriggerReceive': {
        const handle = typeof args['handle'] === 'string' && args['handle'].length > 0 ? args['handle'] : null;
        const payload = args['payload'] ?? null;
        const size = payloadBytes(payload);
        const userErrors: Array<{ field: string[]; message: string }> = [];
        if (size > FLOW_TRIGGER_PAYLOAD_LIMIT_BYTES) {
          userErrors.push({
            field: ['body'],
            message: `Errors validating schema:\n  Properties size exceeds the limit of ${FLOW_TRIGGER_PAYLOAD_LIMIT_BYTES} bytes.\n`,
          });
        } else if (!isLocalFlowTriggerHandle(handle)) {
          userErrors.push({
            field: ['body'],
            message: `Errors validating schema:\n  Invalid handle '${handle ?? ''}'.\n`,
          });
        }

        if (userErrors.length === 0 && handle) {
          const recordId = runtime.syntheticIdentity.makeSyntheticGid('FlowTriggerReceive');
          runtime.store.stageAdminPlatformFlowTrigger({
            id: recordId,
            handle,
            payloadBytes: size,
            payloadSha256: hashString(stableJsonStringify(payload)),
            receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
          });
          stagedResourceIds.push(recordId);
          staged = true;
          notes.push('Recorded a local Flow trigger receipt without delivering any external Flow side effects.');
        }

        data[key] = serializeFlowTriggerReceivePayload(field, userErrors);
        break;
      }
      case 'backupRegionUpdate': {
        const rawRegion = args['region'];
        const countryCode =
          rawRegion && typeof rawRegion === 'object' && !Array.isArray(rawRegion)
            ? (rawRegion as Record<string, unknown>)['countryCode']
            : null;
        const region =
          typeof countryCode === 'string' ? (BACKUP_REGION_UPDATE_BY_COUNTRY_CODE[countryCode] ?? null) : null;
        if (!region) {
          data[key] = serializeBackupRegionUpdatePayload(field, null, [
            { field: ['region'], message: 'Region not found.', code: 'REGION_NOT_FOUND' },
          ]);
          break;
        }

        runtime.store.stageBackupRegion(region);
        staged = true;
        stagedResourceIds.push(region.id);
        notes.push('Staged the shop backup region locally; no market or regional setting was changed upstream.');
        data[key] = serializeBackupRegionUpdatePayload(field, region, []);
        break;
      }
      default:
        data[key] = null;
    }
  }

  const response = errors.length > 0 ? { data, errors } : { data };
  return {
    response,
    stagedResourceIds,
    staged,
    ...(notes.length > 0 ? { notes: notes.join(' ') } : {}),
  };
}
