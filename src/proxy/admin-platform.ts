import { createHash, createHmac } from 'node:crypto';
import { getLocation, Kind, print, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { BackupRegionRecord, ShopDomainRecord } from '../state/types.js';
import { handleB2BQuery } from './b2b.js';
import { handleBulkOperationQuery } from './bulk-operations.js';
import { handleCustomerQuery } from './customers.js';
import { handleDeliveryProfileQuery } from './delivery-profiles.js';
import { handleDiscountQuery } from './discounts.js';
import { handleFunctionQuery } from './functions.js';
import { handleGiftCardQuery } from './gift-cards.js';
import { getDocumentFragments, isPlainObject, serializeConnection, type FragmentMap } from './graphql-helpers.js';
import { handleMarketingQuery } from './marketing.js';
import { handleMarketsQuery } from './markets.js';
import { serializeFileNodeById } from './media.js';
import { handleMetafieldDefinitionQuery } from './metafield-definitions.js';
import { handleMetaobjectDefinitionQuery } from './metaobject-definitions.js';
import { handleOnlineStoreQuery } from './online-store.js';
import { handleOrderQuery } from './orders/query.js';
import { handlePaymentQuery, serializePaymentTermsTemplateNodeById } from './payments.js';
import { handleProductQuery, serializeProductOptionNodeById, serializeProductOptionValueNodeById } from './products.js';
import { serializeSavedSearchNodeById } from './saved-searches.js';
import { handleSegmentsQuery } from './segments.js';
import { handleStorePropertiesQuery, serializeShopNodeById } from './store-properties.js';
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

const BACKUP_REGION_BY_COUNTRY_CODE: Record<string, BackupRegionRecord> = {
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

function serializeDomainRoot(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> | null {
  const id = readIdArgument(field, variables);
  const primaryDomain = store.getEffectiveShop()?.primaryDomain ?? null;
  if (!id || !primaryDomain || primaryDomain.id !== id) {
    return null;
  }

  return serializeDomain(primaryDomain, field.selectionSet?.selections ?? []);
}

function serializeEmptyConnection(field: FieldNode): Record<string, unknown> {
  return serializeConnection(field, {
    items: [],
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: () => '',
    serializeNode: () => null,
  });
}

function serializeTaxonomy(selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (!selection.typeCondition?.name.value || selection.typeCondition.name.value === 'Taxonomy') {
        Object.assign(result, serializeTaxonomy(selection.selectionSet.selections));
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
        result[key] = serializeEmptyConnection(selection);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

type LocalNodeQueryHandler = (document: string, variables: Record<string, unknown>) => unknown;

interface LocalNodeResolver {
  rootField: string;
  typename: string;
  handler: LocalNodeQueryHandler;
  typeConditions?: readonly string[];
}

type DirectLocalNodeResolver = Omit<LocalNodeResolver, 'handler' | 'rootField'> & {
  rootField?: undefined;
  serialize: (
    id: string,
    selectedFields: readonly FieldNode[],
    variables: Record<string, unknown>,
    fragments: FragmentMap,
  ) => Record<string, unknown> | null;
};

type AdminPlatformNodeResolver = LocalNodeResolver | DirectLocalNodeResolver;

const PROXY_NODE_RESPONSE_KEY = 'proxyNode';
const PROXY_NODE_ID_VARIABLE = 'proxyNodeId';
const handleProductNodeQuery: LocalNodeQueryHandler = (document, variables) =>
  handleProductQuery(document, variables, 'snapshot');

const LOCAL_NODE_RESOLVERS: Record<string, AdminPlatformNodeResolver> = {
  Product: { rootField: 'product', typename: 'Product', handler: handleProductNodeQuery },
  ProductVariant: { rootField: 'productVariant', typename: 'ProductVariant', handler: handleProductNodeQuery },
  ProductOption: {
    typename: 'ProductOption',
    serialize: (id, selectedFields) => serializeProductOptionNodeById(id, selectedFields),
  },
  ProductOptionValue: {
    typename: 'ProductOptionValue',
    serialize: (id, selectedFields) => serializeProductOptionValueNodeById(id, selectedFields),
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
  SellingPlanGroup: { rootField: 'sellingPlanGroup', typename: 'SellingPlanGroup', handler: handleProductNodeQuery },

  Customer: { rootField: 'customer', typename: 'Customer', handler: handleCustomerQuery },
  CustomerPaymentMethod: {
    rootField: 'customerPaymentMethod',
    typename: 'CustomerPaymentMethod',
    handler: handleCustomerQuery,
  },
  StoreCreditAccount: { rootField: 'storeCreditAccount', typename: 'StoreCreditAccount', handler: handleCustomerQuery },

  Company: { rootField: 'company', typename: 'Company', handler: handleB2BQuery },
  CompanyContact: { rootField: 'companyContact', typename: 'CompanyContact', handler: handleB2BQuery },
  CompanyContactRole: { rootField: 'companyContactRole', typename: 'CompanyContactRole', handler: handleB2BQuery },
  CompanyLocation: { rootField: 'companyLocation', typename: 'CompanyLocation', handler: handleB2BQuery },

  BusinessEntity: { rootField: 'businessEntity', typename: 'BusinessEntity', handler: handleStorePropertiesQuery },
  Location: { rootField: 'location', typename: 'Location', handler: handleStorePropertiesQuery },
  Shop: {
    typename: 'Shop',
    serialize: (id, selectedFields) => serializeShopNodeById(id, selectedFields),
  },
  DeliveryCarrierService: {
    rootField: 'carrierService',
    typename: 'DeliveryCarrierService',
    handler: handleStorePropertiesQuery,
  },

  PaymentCustomization: {
    rootField: 'paymentCustomization',
    typename: 'PaymentCustomization',
    handler: handlePaymentQuery,
  },
  PaymentTermsTemplate: {
    typename: 'PaymentTermsTemplate',
    serialize: (id, selectedFields) => serializePaymentTermsTemplateNodeById(id, syntheticNodeField(selectedFields)),
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

  DiscountCodeNode: { rootField: 'codeDiscountNode', typename: 'DiscountCodeNode', handler: handleDiscountQuery },
  DiscountAutomaticNode: {
    rootField: 'automaticDiscountNode',
    typename: 'DiscountAutomaticNode',
    handler: handleDiscountQuery,
  },

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
    serialize: (id, selectedFields) => serializeFileNodeById(id, selectedFields),
  },
  MediaImage: {
    typename: 'MediaImage',
    typeConditions: ['File'],
    serialize: (id, selectedFields) => serializeFileNodeById(id, selectedFields),
  },
  Video: {
    typename: 'Video',
    typeConditions: ['File'],
    serialize: (id, selectedFields) => serializeFileNodeById(id, selectedFields),
  },
  ExternalVideo: {
    typename: 'ExternalVideo',
    typeConditions: ['File'],
    serialize: (id, selectedFields) => serializeFileNodeById(id, selectedFields),
  },
  Model3d: {
    typename: 'Model3d',
    typeConditions: ['File'],
    serialize: (id, selectedFields) => serializeFileNodeById(id, selectedFields),
  },
  SavedSearch: {
    typename: 'SavedSearch',
    serialize: (id, selectedFields, _variables, fragments) =>
      serializeSavedSearchNodeById(id, syntheticNodeField(selectedFields), fragments),
  },
};

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

function serializeLocalNodeById(
  id: string,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> | null {
  if (id.startsWith('gid://shopify/Domain/')) {
    const primaryDomain = store.getEffectiveShop()?.primaryDomain ?? null;
    return primaryDomain?.id === id ? serializeDomain(primaryDomain, selections) : null;
  }

  const gidType = readShopifyGidType(id);
  const resolver = gidType ? LOCAL_NODE_RESOLVERS[gidType] : undefined;
  if (!resolver) {
    return null;
  }

  const selectedFields = collectApplicableNodeFields(selections, resolver, fragments);
  if (selectedFields.length === 0) {
    return {};
  }

  if ('serialize' in resolver) {
    return resolver.serialize(id, selectedFields, variables, fragments);
  }

  const syntheticDocument = `query ProxyNodeLookup { ${PROXY_NODE_RESPONSE_KEY}: ${resolver.rootField}(id: $${PROXY_NODE_ID_VARIABLE}) { ${selectedFields.map((field) => print(field)).join('\n')} } }`;
  const response = resolver.handler(syntheticDocument, {
    ...variables,
    [PROXY_NODE_ID_VARIABLE]: id,
  });
  const payload =
    isPlainObject(response) && isPlainObject(response['data'])
      ? (response['data'][PROXY_NODE_RESPONSE_KEY] ?? null)
      : null;
  return isPlainObject(payload) ? applySelectedTypename(payload, selectedFields, resolver.typename) : null;
}

function serializeNode(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> | null {
  const id = readIdArgument(field, variables);
  if (!id) {
    return null;
  }

  return serializeLocalNodeById(id, field.selectionSet?.selections ?? [], variables, fragments);
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

export interface AdminPlatformMutationResult {
  response: Record<string, unknown>;
  stagedResourceIds?: string[];
  staged: boolean;
  notes?: string;
}

export function handleAdminPlatformQuery(
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
        data[key] = serializeNode(field, variables, fragments);
        break;
      case 'nodes':
        data[key] = readIdListArgument(field, variables).map((id) =>
          serializeLocalNodeById(id, field.selectionSet?.selections ?? [], variables, fragments),
        );
        break;
      case 'job':
        data[key] = serializeJob(field, variables);
        break;
      case 'domain':
        data[key] = serializeDomainRoot(field, variables);
        break;
      case 'backupRegion':
        data[key] = serializePlainObject(
          store.getEffectiveBackupRegion() ?? CAPTURED_BACKUP_REGION,
          field.selectionSet?.selections ?? [],
          'MarketRegionCountry',
        );
        break;
      case 'taxonomy':
        data[key] = serializeTaxonomy(field.selectionSet?.selections ?? []);
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
        const recordId = makeSyntheticGid('FlowGenerateSignature');
        store.stageAdminPlatformFlowSignature({
          id: recordId,
          flowTriggerId,
          payloadSha256: hashString(payload),
          signatureSha256: hashString(signature),
          createdAt: makeSyntheticTimestamp(),
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
          const recordId = makeSyntheticGid('FlowTriggerReceive');
          store.stageAdminPlatformFlowTrigger({
            id: recordId,
            handle,
            payloadBytes: size,
            payloadSha256: hashString(stableJsonStringify(payload)),
            receivedAt: makeSyntheticTimestamp(),
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
        const region = typeof countryCode === 'string' ? (BACKUP_REGION_BY_COUNTRY_CODE[countryCode] ?? null) : null;
        if (!region) {
          data[key] = serializeBackupRegionUpdatePayload(field, null, [
            { field: ['region'], message: 'Region not found.', code: 'REGION_NOT_FOUND' },
          ]);
          break;
        }

        store.stageBackupRegion(region);
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
