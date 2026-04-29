import type { ProxyRuntimeContext } from './runtime-context.js';
import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { logger } from '../logger.js';
import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { parseSearchQueryTerms, normalizeSearchQueryValue, type SearchQueryTerm } from '../search-query-parser.js';
import { compareShopifyResourceIds } from '../shopify/resource-ids.js';
import type {
  BusinessEntityAddressRecord,
  BusinessEntityRecord,
  CarrierServiceRecord,
  DeliveryLocalPickupSettingsRecord,
  FulfillmentServiceRecord,
  InventoryLevelRecord,
  LocationAddressRecord,
  LocationFulfillmentServiceRecord,
  LocationRecord,
  LocationSuggestedAddressRecord,
  OnlineStoreIntegrationRecord,
  PaymentSettingsRecord,
  ProductVariantRecord,
  ShopifyPaymentsAccountRecord,
  ShippingPackageDimensionsRecord,
  ShippingPackageRecord,
  ShippingPackageWeightRecord,
  ShopAddressRecord,
  ShopBundlesFeatureRecord,
  ShopCartTransformEligibleOperationsRecord,
  ShopCartTransformFeatureRecord,
  ShopDomainRecord,
  ShopFeaturesRecord,
  ShopPlanRecord,
  ShopPolicyRecord,
  ShopRecord,
  ShopResourceLimitsRecord,
} from '../state/types.js';
import {
  buildMissingIdempotencyKeyError,
  getNodeLocation,
  paginateConnectionItems,
  readIdempotencyKey,
  serializeConnection,
} from './graphql-helpers.js';
import {
  readMetafieldInputObjects,
  serializeMetafieldSelection,
  serializeMetafieldsConnection,
  upsertOwnerMetafields,
} from './metafields.js';

interface GraphQLResponseError {
  message: string;
  locations?: Array<{
    line: number;
    column: number;
  }>;
  path?: Array<string | number>;
  extensions: {
    code: string;
    reason?: string;
    inputObjectType?: string;
  };
}

interface SerializationContext {
  errors: GraphQLResponseError[];
  fatalErrors: GraphQLResponseError[];
}

interface ShopPolicyUserErrorRecord {
  field: string[] | null;
  message: string;
  code: 'TOO_BIG' | null;
}

interface LocationUserErrorRecord {
  field: string[] | null;
  message: string;
  code?: string | null;
}

interface FulfillmentServiceUserErrorRecord {
  field: string[] | null;
  message: string;
}

interface CarrierServiceUserErrorRecord {
  field: string[] | null;
  message: string;
}

interface LocalPickupUserErrorRecord {
  field: string[] | null;
  message: string;
  code: string | null;
}

interface ShippingPackageUserErrorRecord {
  field: string[] | null;
  message: string;
}

const storePropertiesLogger = logger.child({ component: 'proxy.store-properties' });

const SAFE_SHOPIFY_PAYMENTS_ACCOUNT_FIELDS = new Set(['id', 'activated', 'country', 'defaultCurrency', 'onboardable']);
const DEFAULT_INVENTORY_LEVEL_LOCATION_ID = 'gid://shopify/Location/1';

const SHOP_POLICY_BODY_LIMIT_BYTES = 512 * 1024;

const SHOP_POLICY_TYPE_ORDER = [
  'CONTACT_INFORMATION',
  'LEGAL_NOTICE',
  'PRIVACY_POLICY',
  'REFUND_POLICY',
  'SHIPPING_POLICY',
  'SUBSCRIPTION_POLICY',
  'TERMS_OF_SALE',
  'TERMS_OF_SERVICE',
] as const;

const SHOP_POLICY_TYPES = new Set<string>(SHOP_POLICY_TYPE_ORDER);

const SHOP_POLICY_TITLES_BY_TYPE: Record<string, string> = {
  CONTACT_INFORMATION: 'Contact',
  LEGAL_NOTICE: 'Legal notice',
  PRIVACY_POLICY: 'Privacy policy',
  REFUND_POLICY: 'Refund policy',
  SHIPPING_POLICY: 'Shipping policy',
  SUBSCRIPTION_POLICY: 'Cancellation policy',
  TERMS_OF_SALE: 'Terms of sale',
  TERMS_OF_SERVICE: 'Terms of service',
};

function responseKey(selection: FieldNode): string {
  return selection.alias?.value ?? selection.name.value;
}

interface LocationInventoryLevelRecord {
  variant: ProductVariantRecord;
  level: InventoryLevelRecord;
}

function readLegacyResourceIdFromGid(id: string): string | null {
  const tail = id.split('/').at(-1);
  return tail && /^\d+$/u.test(tail) ? tail : null;
}

function buildStableSyntheticInventoryLevelId(inventoryItemId: string, locationId: string): string {
  const inventoryItemTail = inventoryItemId.split('/').at(-1) ?? encodeURIComponent(inventoryItemId);
  const locationTail = locationId.split('/').at(-1) ?? encodeURIComponent(locationId);

  return `gid://shopify/InventoryLevel/${inventoryItemTail}-${locationTail}?inventory_item_id=${encodeURIComponent(
    inventoryItemId,
  )}`;
}

function buildSyntheticInventoryLevel(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
): InventoryLevelRecord | null {
  if (!variant.inventoryItem) {
    return null;
  }

  const availableQuantity = variant.inventoryQuantity ?? 0;
  const availableUpdatedAt = runtime.syntheticIdentity.makeSyntheticTimestamp();

  return {
    id: buildStableSyntheticInventoryLevelId(variant.inventoryItem.id, DEFAULT_INVENTORY_LEVEL_LOCATION_ID),
    cursor: null,
    location: { id: DEFAULT_INVENTORY_LEVEL_LOCATION_ID, name: null },
    quantities: [
      {
        name: 'available',
        quantity: availableQuantity,
        updatedAt: availableUpdatedAt,
      },
      {
        name: 'on_hand',
        quantity: availableQuantity,
        updatedAt: null,
      },
      {
        name: 'incoming',
        quantity: 0,
        updatedAt: null,
      },
      {
        name: 'committed',
        quantity: 0,
        updatedAt: null,
      },
      {
        name: 'reserved',
        quantity: 0,
        updatedAt: null,
      },
    ],
  };
}

function getEffectiveInventoryLevels(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
): InventoryLevelRecord[] {
  const hydratedLevels = variant.inventoryItem?.inventoryLevels;
  if (!hydratedLevels || hydratedLevels.length === 0) {
    const syntheticLevel = buildSyntheticInventoryLevel(runtime, variant);
    return syntheticLevel && !runtime.store.isLocationDeleted(syntheticLevel.location?.id ?? '')
      ? [syntheticLevel]
      : [];
  }

  return structuredClone(hydratedLevels).filter((level) => !runtime.store.isLocationDeleted(level.location?.id ?? ''));
}

function listLocationInventoryLevels(runtime: ProxyRuntimeContext): LocationInventoryLevelRecord[] {
  return runtime.store
    .listEffectiveProducts()
    .flatMap((product) =>
      runtime.store
        .getEffectiveVariantsByProductId(product.id)
        .flatMap((variant) => getEffectiveInventoryLevels(runtime, variant).map((level) => ({ variant, level }))),
    );
}

function locationRecordFromInventoryLocation(location: NonNullable<InventoryLevelRecord['location']>): LocationRecord {
  return {
    id: location.id,
    name: location.name,
  };
}

function mergeLocationRecord(base: LocationRecord, inventoryLocation: LocationRecord | null): LocationRecord {
  return {
    ...base,
    name: base.name ?? inventoryLocation?.name ?? null,
  };
}

function listEffectiveLocations(runtime: ProxyRuntimeContext): LocationRecord[] {
  const locationsById = new Map<string, LocationRecord>();
  const locations: LocationRecord[] = [];
  const seenLocationIds = new Set<string>();

  for (const location of runtime.store.listEffectiveLocations()) {
    locationsById.set(location.id, location);
    seenLocationIds.add(location.id);
    locations.push(location);
  }

  for (const { level } of listLocationInventoryLevels(runtime)) {
    if (!level.location) {
      continue;
    }

    const location = locationRecordFromInventoryLocation(level.location);
    const existing = locationsById.get(location.id);
    if (existing) {
      locationsById.set(location.id, mergeLocationRecord(existing, location));
      continue;
    }

    if (seenLocationIds.has(location.id)) {
      continue;
    }

    seenLocationIds.add(location.id);
    locationsById.set(location.id, location);
    locations.push(location);
  }

  return locations.map((location) => locationsById.get(location.id) ?? location);
}

function findEffectiveLocationById(runtime: ProxyRuntimeContext, id: string): LocationRecord | null {
  if (runtime.store.isLocationDeleted(id)) {
    return null;
  }

  return listEffectiveLocations(runtime).find((location) => location.id === id) ?? null;
}

function getPrimaryLocation(runtime: ProxyRuntimeContext): LocationRecord | null {
  return listEffectiveLocations(runtime)[0] ?? null;
}

function serializeAddress(
  address: BusinessEntityAddressRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'BusinessEntityAddress') {
        continue;
      }
      Object.assign(result, serializeAddress(address, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'BusinessEntityAddress';
        break;
      case 'address1':
        result[key] = address.address1;
        break;
      case 'address2':
        result[key] = address.address2;
        break;
      case 'city':
        result[key] = address.city;
        break;
      case 'countryCode':
        result[key] = address.countryCode;
        break;
      case 'province':
        result[key] = address.province;
        break;
      case 'zip':
        result[key] = address.zip;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeLocationAddress(
  address: LocationAddressRecord | null,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'LocationAddress') {
        continue;
      }
      Object.assign(result, serializeLocationAddress(address, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'LocationAddress';
        break;
      case 'formatted':
        result[key] = address?.formatted ? structuredClone(address.formatted) : [];
        break;
      case 'address1':
        result[key] = address?.address1 ?? null;
        break;
      case 'address2':
        result[key] = address?.address2 ?? null;
        break;
      case 'city':
        result[key] = address?.city ?? null;
        break;
      case 'country':
        result[key] = address?.country ?? null;
        break;
      case 'countryCode':
        result[key] = address?.countryCode ?? null;
        break;
      case 'latitude':
        result[key] = address?.latitude ?? null;
        break;
      case 'longitude':
        result[key] = address?.longitude ?? null;
        break;
      case 'phone':
        result[key] = address?.phone ?? null;
        break;
      case 'province':
        result[key] = address?.province ?? null;
        break;
      case 'provinceCode':
        result[key] = address?.provinceCode ?? null;
        break;
      case 'zip':
        result[key] = address?.zip ?? null;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeLocationSuggestedAddress(
  address: LocationSuggestedAddressRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'LocationSuggestedAddress';
        break;
      case 'address1':
        result[key] = address.address1;
        break;
      case 'countryCode':
        result[key] = address.countryCode;
        break;
      case 'formatted':
        result[key] = structuredClone(address.formatted);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeLocationFulfillmentService(
  runtime: ProxyRuntimeContext,
  service: LocationFulfillmentServiceRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  if (typeof service.id === 'string') {
    const fullService = runtime.store.getEffectiveFulfillmentServiceById(service.id);
    if (fullService) {
      return serializeFulfillmentService(runtime, fullService, selections, variables);
    }
  }

  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'FulfillmentService';
        break;
      case 'id':
        result[key] = service.id;
        break;
      case 'handle':
        result[key] = service.handle;
        break;
      case 'serviceName':
        result[key] = service.serviceName;
        break;
      case 'callbackUrl':
        result[key] = service.callbackUrl ?? null;
        break;
      case 'inventoryManagement':
        result[key] = service.inventoryManagement ?? false;
        break;
      case 'location':
        result[key] =
          typeof service.locationId === 'string'
            ? serializeFulfillmentServiceLocation(
                runtime,
                service.locationId,
                selection.selectionSet?.selections ?? [],
                variables,
              )
            : null;
        break;
      case 'requiresShippingMethod':
        result[key] = service.requiresShippingMethod ?? true;
        break;
      case 'trackingSupport':
        result[key] = service.trackingSupport ?? false;
        break;
      case 'type':
        result[key] = service.type ?? 'THIRD_PARTY';
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeFulfillmentServiceLocation(
  runtime: ProxyRuntimeContext,
  locationId: string,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  const location = findEffectiveLocationById(runtime, locationId);
  return location ? serializeLocation(runtime, location, selections, variables) : null;
}

function serializeFulfillmentService(
  runtime: ProxyRuntimeContext,
  service: FulfillmentServiceRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'FulfillmentService') {
        continue;
      }
      Object.assign(
        result,
        serializeFulfillmentService(runtime, service, selection.selectionSet.selections, variables),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'FulfillmentService';
        break;
      case 'id':
        result[key] = service.id;
        break;
      case 'handle':
        result[key] = service.handle;
        break;
      case 'serviceName':
        result[key] = service.serviceName;
        break;
      case 'callbackUrl':
        result[key] = service.callbackUrl;
        break;
      case 'inventoryManagement':
        result[key] = service.inventoryManagement;
        break;
      case 'location':
        result[key] = service.locationId
          ? serializeFulfillmentServiceLocation(
              runtime,
              service.locationId,
              selection.selectionSet?.selections ?? [],
              variables,
            )
          : null;
        break;
      case 'requiresShippingMethod':
        result[key] = service.requiresShippingMethod;
        break;
      case 'trackingSupport':
        result[key] = service.trackingSupport;
        break;
      case 'type':
        result[key] = service.type;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeCarrierService(
  service: CarrierServiceRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'DeliveryCarrierService') {
        continue;
      }
      Object.assign(result, serializeCarrierService(service, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'DeliveryCarrierService';
        break;
      case 'id':
        result[key] = service.id;
        break;
      case 'name':
        result[key] = service.name;
        break;
      case 'formattedName':
        result[key] = service.formattedName;
        break;
      case 'callbackUrl':
        result[key] = service.callbackUrl;
        break;
      case 'active':
        result[key] = service.active;
        break;
      case 'supportsServiceDiscovery':
        result[key] = service.supportsServiceDiscovery;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeInventoryLevelQuantities(
  level: InventoryLevelRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Array<Record<string, unknown>> {
  const args = getFieldArguments(field, variables);
  const requestedNames = Array.isArray(args['names'])
    ? args['names'].filter((value): value is string => typeof value === 'string')
    : [];
  const visibleQuantities =
    requestedNames.length > 0
      ? requestedNames.map(
          (name) =>
            level.quantities.find((quantity) => quantity.name === name) ?? { name, quantity: 0, updatedAt: null },
        )
      : level.quantities;

  return visibleQuantities.map((quantity) => {
    const result: Record<string, unknown> = {};
    for (const selection of field.selectionSet?.selections ?? []) {
      if (selection.kind !== Kind.FIELD) {
        continue;
      }

      const key = responseKey(selection);
      switch (selection.name.value) {
        case 'name':
          result[key] = quantity.name;
          break;
        case 'quantity':
          result[key] = quantity.quantity;
          break;
        case 'updatedAt':
          result[key] = quantity.updatedAt;
          break;
        default:
          result[key] = null;
      }
    }
    return result;
  });
}

function serializeInventoryLevelLocation(
  runtime: ProxyRuntimeContext,
  location: NonNullable<InventoryLevelRecord['location']>,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const effectiveLocation = findEffectiveLocationById(runtime, location.id);

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'Location') {
        continue;
      }
      Object.assign(result, serializeInventoryLevelLocation(runtime, location, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Location';
        break;
      case 'id':
        result[key] = location.id;
        break;
      case 'name':
        result[key] = effectiveLocation?.name ?? location.name;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeInventoryItem(
  variant: ProductVariantRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> | null {
  if (!variant.inventoryItem) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'InventoryItem';
        break;
      case 'id':
        result[key] = variant.inventoryItem.id;
        break;
      case 'sku':
        result[key] = variant.sku;
        break;
      case 'tracked':
        result[key] = variant.inventoryItem.tracked;
        break;
      case 'requiresShipping':
        result[key] = variant.inventoryItem.requiresShipping;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeInventoryLevel(
  runtime: ProxyRuntimeContext,
  entry: LocationInventoryLevelRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'InventoryLevel') {
        continue;
      }
      Object.assign(result, serializeInventoryLevel(runtime, entry, selection.selectionSet.selections, variables));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'InventoryLevel';
        break;
      case 'id':
        result[key] = entry.level.id;
        break;
      case 'item':
        result[key] = serializeInventoryItem(entry.variant, selection.selectionSet?.selections ?? []);
        break;
      case 'location':
        result[key] = entry.level.location
          ? serializeInventoryLevelLocation(runtime, entry.level.location, selection.selectionSet?.selections ?? [])
          : null;
        break;
      case 'quantities':
        result[key] = serializeInventoryLevelQuantities(entry.level, selection, variables);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function getInventoryLevelCursor(entry: LocationInventoryLevelRecord): string {
  return entry.level.cursor ?? entry.level.id;
}

function listInventoryLevelsForLocation(
  runtime: ProxyRuntimeContext,
  locationId: string,
): LocationInventoryLevelRecord[] {
  return listLocationInventoryLevels(runtime).filter((entry) => entry.level.location?.id === locationId);
}

function serializeLocationInventoryLevelsConnection(
  runtime: ProxyRuntimeContext,
  location: LocationRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const allLevels = listInventoryLevelsForLocation(runtime, location.id);
  if (args['reverse'] === true) {
    allLevels.reverse();
  }
  const {
    items: levels,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allLevels, field, variables, getInventoryLevelCursor);
  return serializeConnection(field, {
    items: levels,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: getInventoryLevelCursor,
    serializeNode: (level, selection) =>
      serializeInventoryLevel(runtime, level, selection.selectionSet?.selections ?? [], variables),
    pageInfoOptions: {
      prefixCursors: false,
    },
  });
}

function serializeLocalPickupSettings(
  settings: DeliveryLocalPickupSettingsRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'DeliveryLocalPickupSettings') {
        continue;
      }
      Object.assign(result, serializeLocalPickupSettings(settings, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'DeliveryLocalPickupSettings';
        break;
      case 'pickupTime':
        result[key] = settings.pickupTime;
        break;
      case 'instructions':
        result[key] = settings.instructions;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function findInventoryLevelForLocation(
  runtime: ProxyRuntimeContext,
  locationId: string,
  inventoryItemId: string,
): LocationInventoryLevelRecord | null {
  return (
    listInventoryLevelsForLocation(runtime, locationId).find(
      (entry) => entry.variant.inventoryItem?.id === inventoryItemId,
    ) ?? null
  );
}

function serializeLocationMetafield(
  location: LocationRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  const args = getFieldArguments(field, variables);
  const namespace = typeof args['namespace'] === 'string' ? args['namespace'] : null;
  const key = typeof args['key'] === 'string' ? args['key'] : null;
  if (!namespace || !key) {
    return null;
  }

  const metafield =
    (location.metafields ?? []).find((candidate) => candidate.namespace === namespace && candidate.key === key) ?? null;
  return metafield ? serializeMetafieldSelection(metafield, field, { includeInlineFragments: true }) : null;
}

export function serializeLocation(
  runtime: ProxyRuntimeContext,
  location: LocationRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'Location') {
        continue;
      }
      Object.assign(result, serializeLocation(runtime, location, selection.selectionSet.selections, variables));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Location';
        break;
      case 'id':
        result[key] = location.id;
        break;
      case 'legacyResourceId':
        result[key] = location.legacyResourceId ?? readLegacyResourceIdFromGid(location.id);
        break;
      case 'name':
        result[key] = location.name;
        break;
      case 'activatable':
        result[key] = location.activatable ?? true;
        break;
      case 'addressVerified':
        result[key] = location.addressVerified ?? false;
        break;
      case 'createdAt':
        result[key] = location.createdAt ?? null;
        break;
      case 'updatedAt':
        result[key] = location.updatedAt ?? null;
        break;
      case 'deactivatable':
        result[key] = location.deactivatable ?? false;
        break;
      case 'deactivatedAt':
        result[key] = location.deactivatedAt ?? null;
        break;
      case 'deletable':
        result[key] = location.deletable ?? false;
        break;
      case 'fulfillmentService':
        result[key] = location.fulfillmentService
          ? serializeLocationFulfillmentService(
              runtime,
              location.fulfillmentService,
              selection.selectionSet?.selections ?? [],
              variables,
            )
          : null;
        break;
      case 'fulfillsOnlineOrders':
        result[key] = location.fulfillsOnlineOrders ?? true;
        break;
      case 'hasActiveInventory':
        result[key] = location.hasActiveInventory ?? true;
        break;
      case 'isActive':
        result[key] = location.isActive ?? true;
        break;
      case 'shipsInventory':
        result[key] = location.shipsInventory ?? true;
        break;
      case 'hasUnfulfilledOrders':
        result[key] = location.hasUnfulfilledOrders ?? false;
        break;
      case 'isFulfillmentService':
        result[key] = location.isFulfillmentService ?? false;
        break;
      case 'address':
        result[key] = serializeLocationAddress(location.address ?? null, selection.selectionSet?.selections ?? []);
        break;
      case 'suggestedAddresses':
        result[key] = (location.suggestedAddresses ?? []).map((address) =>
          serializeLocationSuggestedAddress(address, selection.selectionSet?.selections ?? []),
        );
        break;
      case 'metafield':
        result[key] = serializeLocationMetafield(location, selection, variables);
        break;
      case 'metafields':
        result[key] = serializeMetafieldsConnection(location.metafields ?? [], selection, variables, {
          includeInlineFragments: true,
        });
        break;
      case 'inventoryLevel': {
        const args = getFieldArguments(selection, variables);
        const inventoryItemId = typeof args['inventoryItemId'] === 'string' ? args['inventoryItemId'] : null;
        const level = inventoryItemId ? findInventoryLevelForLocation(runtime, location.id, inventoryItemId) : null;
        result[key] = level
          ? serializeInventoryLevel(runtime, level, selection.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'inventoryLevels':
        result[key] = serializeLocationInventoryLevelsConnection(runtime, location, selection, variables);
        break;
      case 'localPickupSettingsV2':
        result[key] = location.localPickupSettings
          ? serializeLocalPickupSettings(location.localPickupSettings, selection.selectionSet?.selections ?? [])
          : null;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopDomain(domain: ShopDomainRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'Domain') {
        continue;
      }
      Object.assign(result, serializeShopDomain(domain, selection.selectionSet.selections));
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
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopAddress(
  address: ShopAddressRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopAddress') {
        continue;
      }
      Object.assign(result, serializeShopAddress(address, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopAddress';
        break;
      case 'id':
        result[key] = address.id;
        break;
      case 'address1':
        result[key] = address.address1;
        break;
      case 'address2':
        result[key] = address.address2;
        break;
      case 'city':
        result[key] = address.city;
        break;
      case 'company':
        result[key] = address.company;
        break;
      case 'coordinatesValidated':
        result[key] = address.coordinatesValidated;
        break;
      case 'country':
        result[key] = address.country;
        break;
      case 'countryCodeV2':
        result[key] = address.countryCodeV2;
        break;
      case 'formatted':
        result[key] = structuredClone(address.formatted);
        break;
      case 'formattedArea':
        result[key] = address.formattedArea;
        break;
      case 'latitude':
        result[key] = address.latitude;
        break;
      case 'longitude':
        result[key] = address.longitude;
        break;
      case 'phone':
        result[key] = address.phone;
        break;
      case 'province':
        result[key] = address.province;
        break;
      case 'provinceCode':
        result[key] = address.provinceCode;
        break;
      case 'zip':
        result[key] = address.zip;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopPlan(plan: ShopPlanRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopPlan') {
        continue;
      }
      Object.assign(result, serializeShopPlan(plan, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopPlan';
        break;
      case 'partnerDevelopment':
        result[key] = plan.partnerDevelopment;
        break;
      case 'publicDisplayName':
        result[key] = plan.publicDisplayName;
        break;
      case 'shopifyPlus':
        result[key] = plan.shopifyPlus;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopResourceLimits(
  resourceLimits: ShopResourceLimitsRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopResourceLimits') {
        continue;
      }
      Object.assign(result, serializeShopResourceLimits(resourceLimits, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopResourceLimits';
        break;
      case 'locationLimit':
        result[key] = resourceLimits.locationLimit;
        break;
      case 'maxProductOptions':
        result[key] = resourceLimits.maxProductOptions;
        break;
      case 'maxProductVariants':
        result[key] = resourceLimits.maxProductVariants;
        break;
      case 'redirectLimitReached':
        result[key] = resourceLimits.redirectLimitReached;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopBundlesFeature(
  bundles: ShopBundlesFeatureRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'BundlesFeature') {
        continue;
      }
      Object.assign(result, serializeShopBundlesFeature(bundles, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'BundlesFeature';
        break;
      case 'eligibleForBundles':
        result[key] = bundles.eligibleForBundles;
        break;
      case 'ineligibilityReason':
        result[key] = bundles.ineligibilityReason;
        break;
      case 'sellsBundles':
        result[key] = bundles.sellsBundles;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopCartTransformEligibleOperations(
  operations: ShopCartTransformEligibleOperationsRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (
        selection.typeCondition?.name.value &&
        selection.typeCondition.name.value !== 'CartTransformEligibleOperations'
      ) {
        continue;
      }
      Object.assign(
        result,
        serializeShopCartTransformEligibleOperations(operations, selection.selectionSet.selections),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'CartTransformEligibleOperations';
        break;
      case 'expandOperation':
        result[key] = operations.expandOperation;
        break;
      case 'mergeOperation':
        result[key] = operations.mergeOperation;
        break;
      case 'updateOperation':
        result[key] = operations.updateOperation;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopCartTransformFeature(
  cartTransform: ShopCartTransformFeatureRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'CartTransformFeature') {
        continue;
      }
      Object.assign(result, serializeShopCartTransformFeature(cartTransform, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'CartTransformFeature';
        break;
      case 'eligibleOperations':
        result[key] = serializeShopCartTransformEligibleOperations(
          cartTransform.eligibleOperations,
          selection.selectionSet?.selections ?? [],
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopFeatures(
  features: ShopFeaturesRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopFeatures') {
        continue;
      }
      Object.assign(result, serializeShopFeatures(features, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopFeatures';
        break;
      case 'avalaraAvatax':
        result[key] = features.avalaraAvatax;
        break;
      case 'branding':
        result[key] = features.branding;
        break;
      case 'bundles':
        result[key] = serializeShopBundlesFeature(features.bundles, selection.selectionSet?.selections ?? []);
        break;
      case 'captcha':
        result[key] = features.captcha;
        break;
      case 'cartTransform':
        result[key] = serializeShopCartTransformFeature(
          features.cartTransform,
          selection.selectionSet?.selections ?? [],
        );
        break;
      case 'dynamicRemarketing':
        result[key] = features.dynamicRemarketing;
        break;
      case 'eligibleForSubscriptionMigration':
        result[key] = features.eligibleForSubscriptionMigration;
        break;
      case 'eligibleForSubscriptions':
        result[key] = features.eligibleForSubscriptions;
        break;
      case 'giftCards':
        result[key] = features.giftCards;
        break;
      case 'harmonizedSystemCode':
        result[key] = features.harmonizedSystemCode;
        break;
      case 'legacySubscriptionGatewayEnabled':
        result[key] = features.legacySubscriptionGatewayEnabled;
        break;
      case 'liveView':
        result[key] = features.liveView;
        break;
      case 'paypalExpressSubscriptionGatewayStatus':
        result[key] = features.paypalExpressSubscriptionGatewayStatus;
        break;
      case 'reports':
        result[key] = features.reports;
        break;
      case 'sellsSubscriptions':
        result[key] = features.sellsSubscriptions;
        break;
      case 'showMetrics':
        result[key] = features.showMetrics;
        break;
      case 'storefront':
        result[key] = features.storefront;
        break;
      case 'unifiedMarkets':
        result[key] = features.unifiedMarkets;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializePaymentSettings(
  paymentSettings: PaymentSettingsRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'PaymentSettings') {
        continue;
      }
      Object.assign(result, serializePaymentSettings(paymentSettings, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'PaymentSettings';
        break;
      case 'supportedDigitalWallets':
        result[key] = structuredClone(paymentSettings.supportedDigitalWallets);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShopPolicy(policy: ShopPolicyRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopPolicy') {
        continue;
      }
      Object.assign(result, serializeShopPolicy(policy, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopPolicy';
        break;
      case 'id':
        result[key] = policy.id;
        break;
      case 'title':
        result[key] = policy.title;
        break;
      case 'body':
        result[key] = policy.body;
        break;
      case 'type':
        result[key] = policy.type;
        break;
      case 'url':
        result[key] = policy.url;
        break;
      case 'createdAt':
        result[key] = policy.createdAt;
        break;
      case 'updatedAt':
        result[key] = policy.updatedAt;
        break;
      case 'translations':
        result[key] = [];
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

export function serializeShopAddressNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  selectedFields: readonly FieldNode[],
): Record<string, unknown> | null {
  const shop = runtime.store.getEffectiveShop();
  return shop?.shopAddress.id === id ? serializeShopAddress(shop.shopAddress, selectedFields) : null;
}

export function serializeShopPolicyNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  selectedFields: readonly FieldNode[],
): Record<string, unknown> | null {
  const shop = runtime.store.getEffectiveShop();
  const policy = shop?.shopPolicies.find((candidate) => candidate.id === id) ?? null;
  return policy ? serializeShopPolicy(policy, selectedFields) : null;
}

function serializeShopPolicyUserError(
  userError: ShopPolicyUserErrorRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopPolicyUserError') {
        continue;
      }
      Object.assign(result, serializeShopPolicyUserError(userError, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopPolicyUserError';
        break;
      case 'field':
        result[key] = userError.field ? structuredClone(userError.field) : null;
        break;
      case 'message':
        result[key] = userError.message;
        break;
      case 'code':
        result[key] = userError.code;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeAccessScope(scope: unknown, selections: readonly SelectionNode[]): Record<string, unknown> {
  const scopeRecord = typeof scope === 'object' && scope !== null ? (scope as Record<string, unknown>) : {};
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'AccessScope') {
        continue;
      }
      Object.assign(result, serializeAccessScope(scope, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'AccessScope';
        break;
      case 'handle':
        result[key] = typeof scopeRecord['handle'] === 'string' ? scopeRecord['handle'] : null;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeStorefrontAccessToken(
  token: OnlineStoreIntegrationRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'StorefrontAccessToken') {
        continue;
      }
      Object.assign(result, serializeStorefrontAccessToken(token, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'StorefrontAccessToken';
        break;
      case 'id':
        result[key] = token.id;
        break;
      case 'title':
        result[key] = typeof token.data['title'] === 'string' ? token.data['title'] : '';
        break;
      case 'accessToken':
        result[key] = 'shpat_redacted';
        break;
      case 'accessScopes': {
        const scopes = Array.isArray(token.data['accessScopes']) ? token.data['accessScopes'] : [];
        result[key] = scopes.map((scope) => serializeAccessScope(scope, selection.selectionSet?.selections ?? []));
        break;
      }
      case 'createdAt':
        result[key] = typeof token.data['createdAt'] === 'string' ? token.data['createdAt'] : token.createdAt;
        break;
      case 'updatedAt':
        result[key] = typeof token.data['updatedAt'] === 'string' ? token.data['updatedAt'] : token.updatedAt;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeStorefrontAccessTokensConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const tokens = runtime.store.listEffectiveOnlineStoreIntegrations('storefrontAccessToken');
  const window = paginateConnectionItems(tokens, field, variables, (token) => token.id);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (token) => token.id,
    serializeNode: (token, nodeField) =>
      serializeStorefrontAccessToken(token, nodeField.selectionSet?.selections ?? []),
  });
}

function serializeShop(
  runtime: ProxyRuntimeContext,
  shop: ShopRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'Shop') {
        continue;
      }
      Object.assign(result, serializeShop(runtime, shop, selection.selectionSet.selections, variables));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Shop';
        break;
      case 'id':
        result[key] = shop.id;
        break;
      case 'name':
        result[key] = shop.name;
        break;
      case 'myshopifyDomain':
        result[key] = shop.myshopifyDomain;
        break;
      case 'url':
        result[key] = shop.url;
        break;
      case 'primaryDomain':
        result[key] = serializeShopDomain(shop.primaryDomain, selection.selectionSet?.selections ?? []);
        break;
      case 'contactEmail':
        result[key] = shop.contactEmail;
        break;
      case 'email':
        result[key] = shop.email;
        break;
      case 'currencyCode':
        result[key] = shop.currencyCode;
        break;
      case 'enabledPresentmentCurrencies':
        result[key] = structuredClone(shop.enabledPresentmentCurrencies);
        break;
      case 'ianaTimezone':
        result[key] = shop.ianaTimezone;
        break;
      case 'timezoneAbbreviation':
        result[key] = shop.timezoneAbbreviation;
        break;
      case 'timezoneOffset':
        result[key] = shop.timezoneOffset;
        break;
      case 'timezoneOffsetMinutes':
        result[key] = shop.timezoneOffsetMinutes;
        break;
      case 'taxesIncluded':
        result[key] = shop.taxesIncluded;
        break;
      case 'taxShipping':
        result[key] = shop.taxShipping;
        break;
      case 'unitSystem':
        result[key] = shop.unitSystem;
        break;
      case 'weightUnit':
        result[key] = shop.weightUnit;
        break;
      case 'shopAddress':
        result[key] = serializeShopAddress(shop.shopAddress, selection.selectionSet?.selections ?? []);
        break;
      case 'plan':
        result[key] = serializeShopPlan(shop.plan, selection.selectionSet?.selections ?? []);
        break;
      case 'resourceLimits':
        result[key] = serializeShopResourceLimits(shop.resourceLimits, selection.selectionSet?.selections ?? []);
        break;
      case 'features':
        result[key] = serializeShopFeatures(shop.features, selection.selectionSet?.selections ?? []);
        break;
      case 'fulfillmentServices':
        result[key] = runtime.store
          .listEffectiveFulfillmentServices()
          .map((service) =>
            serializeFulfillmentService(runtime, service, selection.selectionSet?.selections ?? [], {}),
          );
        break;
      case 'paymentSettings':
        result[key] = serializePaymentSettings(shop.paymentSettings, selection.selectionSet?.selections ?? []);
        break;
      case 'shopPolicies':
        result[key] = shop.shopPolicies.map((policy) =>
          serializeShopPolicy(policy, selection.selectionSet?.selections ?? []),
        );
        break;
      case 'storefrontAccessTokens':
        result[key] = serializeStorefrontAccessTokensConnection(runtime, selection, variables);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

export function serializeShopNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  selections: readonly SelectionNode[],
): Record<string, unknown> | null {
  const shop = runtime.store.getEffectiveShop();
  return shop?.id === id ? serializeShop(runtime, shop, selections, {}) : null;
}

function unsupportedShopifyPaymentsFieldError(fieldName: string, path: Array<string | number>): GraphQLResponseError {
  return {
    message: `Field ShopifyPaymentsAccount.${fieldName} is not exposed by the local snapshot because it can contain account-specific payment data. Capture and model it explicitly before relying on it.`,
    path,
    extensions: {
      code: 'UNSUPPORTED_FIELD',
      reason: 'shopify-payments-account-sensitive-field',
    },
  };
}

function serializeEmptyShopifyPaymentsConnection(field: FieldNode): Record<string, unknown> {
  return serializeConnection(field, {
    items: [],
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: () => '',
    serializeNode: () => null,
  });
}

function serializeShopifyPaymentsAccount(
  businessEntity: BusinessEntityRecord,
  account: ShopifyPaymentsAccountRecord,
  selections: readonly SelectionNode[],
  context: SerializationContext,
  path: Array<string | number>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopifyPaymentsAccount') {
        continue;
      }
      Object.assign(
        result,
        serializeShopifyPaymentsAccount(businessEntity, account, selection.selectionSet.selections, context, path),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopifyPaymentsAccount';
        break;
      case 'id':
        result[key] = account.id;
        break;
      case 'activated':
        result[key] = account.activated;
        break;
      case 'country':
        result[key] = account.country;
        break;
      case 'defaultCurrency':
        result[key] = account.defaultCurrency;
        break;
      case 'onboardable':
        result[key] = account.onboardable;
        break;
      case 'balanceTransactions':
      case 'disputes':
      case 'payouts':
        result[key] = serializeEmptyShopifyPaymentsConnection(selection);
        break;
      default: {
        if (!SAFE_SHOPIFY_PAYMENTS_ACCOUNT_FIELDS.has(selection.name.value)) {
          storePropertiesLogger.warn(
            {
              businessEntityId: businessEntity.id,
              fieldName: selection.name.value,
            },
            'unsupported Shopify Payments account field requested from snapshot business entity',
          );
          context.errors.push(unsupportedShopifyPaymentsFieldError(selection.name.value, [...path, key]));
        }
        result[key] = null;
      }
    }
  }

  return result;
}

function serializeBusinessEntity(
  businessEntity: BusinessEntityRecord,
  selections: readonly SelectionNode[],
  context: SerializationContext,
  path: Array<string | number>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'BusinessEntity') {
        continue;
      }
      Object.assign(result, serializeBusinessEntity(businessEntity, selection.selectionSet.selections, context, path));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'BusinessEntity';
        break;
      case 'id':
        result[key] = businessEntity.id;
        break;
      case 'displayName':
        result[key] = businessEntity.displayName;
        break;
      case 'companyName':
        result[key] = businessEntity.companyName;
        break;
      case 'primary':
        result[key] = businessEntity.primary;
        break;
      case 'archived':
        result[key] = businessEntity.archived;
        break;
      case 'address':
        result[key] = serializeAddress(businessEntity.address, selection.selectionSet?.selections ?? []);
        break;
      case 'shopifyPaymentsAccount':
        result[key] = businessEntity.shopifyPaymentsAccount
          ? serializeShopifyPaymentsAccount(
              businessEntity,
              businessEntity.shopifyPaymentsAccount,
              selection.selectionSet?.selections ?? [],
              context,
              [...path, key],
            )
          : null;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function getShopifyPaymentsAccountOwner(runtime: ProxyRuntimeContext): {
  businessEntity: BusinessEntityRecord;
  account: ShopifyPaymentsAccountRecord;
} | null {
  const primaryBusinessEntity = runtime.store.getPrimaryBusinessEntity();
  if (primaryBusinessEntity?.shopifyPaymentsAccount) {
    return {
      businessEntity: primaryBusinessEntity,
      account: primaryBusinessEntity.shopifyPaymentsAccount,
    };
  }

  const firstAccountBusinessEntity =
    runtime.store
      .listEffectiveBusinessEntities()
      .find((businessEntity) => businessEntity.shopifyPaymentsAccount !== null) ?? null;

  return firstAccountBusinessEntity?.shopifyPaymentsAccount
    ? {
        businessEntity: firstAccountBusinessEntity,
        account: firstAccountBusinessEntity.shopifyPaymentsAccount,
      }
    : null;
}

function readShopPolicyInput(args: Record<string, unknown>): Record<string, unknown> {
  const shopPolicy = args['shopPolicy'];
  if (shopPolicy && typeof shopPolicy === 'object' && !Array.isArray(shopPolicy)) {
    return shopPolicy as Record<string, unknown>;
  }

  const input = args['input'];
  if (input && typeof input === 'object' && !Array.isArray(input)) {
    return input as Record<string, unknown>;
  }

  return {};
}

function readLocationInput(args: Record<string, unknown>): Record<string, unknown> {
  const input = args['input'];
  return input && typeof input === 'object' && !Array.isArray(input) ? (input as Record<string, unknown>) : {};
}

function readLocationAddressInput(input: Record<string, unknown>): Record<string, unknown> | null {
  const address = input['address'];
  return address && typeof address === 'object' && !Array.isArray(address)
    ? (address as Record<string, unknown>)
    : null;
}

function hasInputField(input: Record<string, unknown>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(input, key);
}

function readOptionalInputString(input: Record<string, unknown>, key: string): string | null {
  const value = input[key];
  return typeof value === 'string' ? value : null;
}

function buildLocationFormattedAddress(address: LocationAddressRecord): string[] {
  const lines = [
    address.address1,
    address.address2,
    [address.city, address.provinceCode ?? address.province, address.zip].filter(Boolean).join(' '),
    address.countryCode ?? address.country,
  ].filter((line): line is string => typeof line === 'string' && line.trim().length > 0);

  return lines.length > 0 ? lines : [];
}

function buildLocationAddressRecord(
  input: Record<string, unknown> | null,
  base: LocationAddressRecord | null = null,
): LocationAddressRecord | null {
  if (!input && !base) {
    return null;
  }

  const address: LocationAddressRecord = {
    address1:
      input && hasInputField(input, 'address1') ? readOptionalInputString(input, 'address1') : (base?.address1 ?? null),
    address2:
      input && hasInputField(input, 'address2') ? readOptionalInputString(input, 'address2') : (base?.address2 ?? null),
    city: input && hasInputField(input, 'city') ? readOptionalInputString(input, 'city') : (base?.city ?? null),
    country: base?.country ?? null,
    countryCode:
      input && hasInputField(input, 'countryCode')
        ? readOptionalInputString(input, 'countryCode')
        : (base?.countryCode ?? null),
    formatted: [],
    latitude: base?.latitude ?? null,
    longitude: base?.longitude ?? null,
    phone: input && hasInputField(input, 'phone') ? readOptionalInputString(input, 'phone') : (base?.phone ?? null),
    province: base?.province ?? null,
    provinceCode:
      input && hasInputField(input, 'provinceCode')
        ? readOptionalInputString(input, 'provinceCode')
        : (base?.provinceCode ?? null),
    zip: input && hasInputField(input, 'zip') ? readOptionalInputString(input, 'zip') : (base?.zip ?? null),
  };

  address.country = address.country ?? address.countryCode;
  address.formatted = buildLocationFormattedAddress(address);
  return address;
}

function serializeLocationUserErrors(
  userErrors: LocationUserErrorRecord[],
  typename: string,
  selections: readonly SelectionNode[],
): Array<Record<string, unknown>> {
  return userErrors.map((userError) => {
    const result: Record<string, unknown> = {};

    for (const selection of selections) {
      if (selection.kind === Kind.INLINE_FRAGMENT) {
        Object.assign(
          result,
          serializeLocationUserErrors([userError], typename, selection.selectionSet.selections)[0] ?? {},
        );
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
        case 'field':
          result[key] = userError.field ? structuredClone(userError.field) : null;
          break;
        case 'message':
          result[key] = userError.message;
          break;
        case 'code':
          result[key] = userError.code ?? null;
          break;
        default:
          result[key] = null;
      }
    }

    return result;
  });
}

function validateLocationInput(
  input: Record<string, unknown>,
  options: { requireCountry: boolean },
): LocationUserErrorRecord[] {
  const userErrors: LocationUserErrorRecord[] = [];
  const rawName = input['name'];
  const address = readLocationAddressInput(input);

  if (typeof rawName === 'string' && rawName.trim().length === 0) {
    userErrors.push({ field: ['input', 'name'], message: 'Add a location name' });
  }

  if (options.requireCountry) {
    const countryCode = address ? readOptionalInputString(address, 'countryCode') : null;
    if (!countryCode || countryCode.trim().length === 0) {
      userErrors.push({ field: ['input', 'address', 'countryCode'], message: 'Country is required' });
    }
  }

  return userErrors;
}

function stageLocationAdd(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
): {
  location: LocationRecord | null;
  userErrors: LocationUserErrorRecord[];
} {
  const userErrors = validateLocationInput(input, { requireCountry: true });
  const name = readOptionalInputString(input, 'name');
  const address = buildLocationAddressRecord(readLocationAddressInput(input));
  if (userErrors.length > 0 || !name || !address) {
    return { location: null, userErrors };
  }

  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const location: LocationRecord = {
    id: runtime.syntheticIdentity.makeProxySyntheticGid('Location'),
    name,
    legacyResourceId: null,
    activatable: false,
    addressVerified: false,
    createdAt: now,
    deactivatable: true,
    deactivatedAt: null,
    deletable: true,
    fulfillmentService: null,
    fulfillsOnlineOrders: typeof input['fulfillsOnlineOrders'] === 'boolean' ? input['fulfillsOnlineOrders'] : true,
    hasActiveInventory: false,
    hasUnfulfilledOrders: false,
    isActive: true,
    isFulfillmentService: false,
    shipsInventory: true,
    updatedAt: now,
    address,
    suggestedAddresses: [],
    metafields: [],
  };

  const metafieldInputs = readMetafieldInputObjects(input['metafields']);
  if (metafieldInputs.length > 0) {
    location.metafields = upsertOwnerMetafields(runtime, 'locationId', location.id, metafieldInputs, [], {
      allowIdLookup: true,
      ownerType: 'LOCATION',
      trimIdentity: true,
    }).metafields;
  }

  return { location: runtime.store.stageCreateLocation(location), userErrors: [] };
}

function stageLocationEdit(
  runtime: ProxyRuntimeContext,
  id: string | null,
  input: Record<string, unknown>,
): { location: LocationRecord | null; userErrors: LocationUserErrorRecord[] } {
  if (!id) {
    return { location: null, userErrors: [{ field: ['id'], message: 'Location not found.' }] };
  }

  const existing = runtime.store.getEffectiveLocationById(id) ?? findEffectiveLocationById(runtime, id);
  if (!existing) {
    return { location: null, userErrors: [{ field: ['id'], message: 'Location not found.' }] };
  }

  if (existing.isFulfillmentService === true) {
    return {
      location: null,
      userErrors: [
        {
          field: ['id'],
          message: 'Only the app that created a fulfillment service can edit its associated location.',
        },
      ],
    };
  }

  const userErrors = validateLocationInput(input, { requireCountry: false });
  if (userErrors.length > 0) {
    return { location: null, userErrors };
  }

  const addressInput = readLocationAddressInput(input);
  const nextLocation: LocationRecord = {
    ...existing,
    name: hasInputField(input, 'name') ? readOptionalInputString(input, 'name') : existing.name,
    fulfillsOnlineOrders:
      typeof input['fulfillsOnlineOrders'] === 'boolean'
        ? input['fulfillsOnlineOrders']
        : existing.fulfillsOnlineOrders,
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    address: addressInput ? buildLocationAddressRecord(addressInput, existing.address ?? null) : existing.address,
  };

  const metafieldInputs = readMetafieldInputObjects(input['metafields']);
  if (metafieldInputs.length > 0) {
    nextLocation.metafields = upsertOwnerMetafields(
      runtime,
      'locationId',
      nextLocation.id,
      metafieldInputs,
      nextLocation.metafields ?? [],
      {
        allowIdLookup: true,
        ownerType: 'LOCATION',
        trimIdentity: true,
      },
    ).metafields;
  }

  return { location: runtime.store.stageUpdateLocation(nextLocation), userErrors: [] };
}

type LocationLifecyclePayload = { location: LocationRecord | null; userErrors: LocationUserErrorRecord[] };
type LocationDeletePayload = { deletedLocationId: string | null; userErrors: LocationUserErrorRecord[] };

function locationNotFoundError(field: string): LocationUserErrorRecord {
  return { field: [field], message: 'Location not found.', code: 'LOCATION_NOT_FOUND' };
}

function isLocationActive(location: LocationRecord): boolean {
  return location.isActive ?? true;
}

function locationHasInventory(runtime: ProxyRuntimeContext, locationId: string): boolean {
  return listInventoryLevelsForLocation(runtime, locationId).length > 0;
}

function mergeInventoryQuantities(
  destination: InventoryLevelRecord['quantities'],
  source: InventoryLevelRecord['quantities'],
): InventoryLevelRecord['quantities'] {
  const quantitiesByName = new Map(destination.map((quantity) => [quantity.name, structuredClone(quantity)]));

  for (const sourceQuantity of source) {
    const existing = quantitiesByName.get(sourceQuantity.name) ?? {
      name: sourceQuantity.name,
      quantity: 0,
      updatedAt: null,
    };
    quantitiesByName.set(sourceQuantity.name, {
      ...existing,
      quantity: (existing.quantity ?? 0) + (sourceQuantity.quantity ?? 0),
      updatedAt: sourceQuantity.updatedAt ?? existing.updatedAt,
    });
  }

  return [...quantitiesByName.values()];
}

function transferLocationInventory(
  runtime: ProxyRuntimeContext,
  sourceLocationId: string,
  destinationLocation: LocationRecord,
): void {
  for (const product of runtime.store.listEffectiveProducts()) {
    const variants = runtime.store.getEffectiveVariantsByProductId(product.id);
    let changed = false;
    const nextVariants = variants.map((variant) => {
      if (!variant.inventoryItem) {
        return variant;
      }

      const levels = getEffectiveInventoryLevels(runtime, variant);
      const sourceLevels = levels.filter((level) => level.location?.id === sourceLocationId);
      if (sourceLevels.length === 0) {
        return variant;
      }

      changed = true;
      const nextLevels = levels.filter((level) => level.location?.id !== sourceLocationId);
      let destinationIndex = nextLevels.findIndex((level) => level.location?.id === destinationLocation.id);
      for (const sourceLevel of sourceLevels) {
        if (destinationIndex === -1) {
          const nextDestinationLevel: InventoryLevelRecord = {
            ...structuredClone(sourceLevel),
            id: buildStableSyntheticInventoryLevelId(variant.inventoryItem.id, destinationLocation.id),
            cursor: null,
            location: { id: destinationLocation.id, name: destinationLocation.name },
          };
          nextLevels.push(nextDestinationLevel);
          destinationIndex = nextLevels.length - 1;
          continue;
        }

        const destinationLevel = nextLevels[destinationIndex];
        if (!destinationLevel) {
          continue;
        }

        nextLevels[destinationIndex] = {
          ...destinationLevel,
          quantities: mergeInventoryQuantities(destinationLevel.quantities, sourceLevel.quantities),
        };
      }

      return {
        ...structuredClone(variant),
        inventoryItem: {
          ...structuredClone(variant.inventoryItem),
          inventoryLevels: nextLevels,
        },
      };
    });

    if (changed) {
      runtime.store.replaceStagedVariantsForProduct(product.id, nextVariants);
    }
  }
}

function stageLocationDeactivate(
  runtime: ProxyRuntimeContext,
  locationId: string | null,
  destinationLocationId: string | null,
): LocationLifecyclePayload {
  if (!locationId) {
    return { location: null, userErrors: [locationNotFoundError('locationId')] };
  }

  const existing = runtime.store.getEffectiveLocationById(locationId) ?? findEffectiveLocationById(runtime, locationId);
  if (!existing) {
    return { location: null, userErrors: [locationNotFoundError('locationId')] };
  }

  if (!isLocationActive(existing)) {
    return { location: existing, userErrors: [] };
  }

  if (existing.hasUnfulfilledOrders === true) {
    return {
      location: existing,
      userErrors: [
        {
          field: ['locationId'],
          message: 'Location could not be deactivated because it has pending orders.',
          code: 'HAS_FULFILLMENT_ORDERS_ERROR',
        },
      ],
    };
  }

  if (locationHasInventory(runtime, existing.id)) {
    if (!destinationLocationId) {
      return {
        location: existing,
        userErrors: [
          {
            field: ['locationId'],
            message:
              'Location could not be deactivated without specifying where to relocate inventory stocked at the location.',
            code: 'HAS_ACTIVE_INVENTORY_ERROR',
          },
        ],
      };
    }

    if (destinationLocationId === existing.id) {
      return {
        location: existing,
        userErrors: [
          {
            field: ['destinationLocationId'],
            message: 'Destination location must be different from the location being deactivated.',
            code: 'GENERIC_ERROR',
          },
        ],
      };
    }

    const destinationLocation =
      runtime.store.getEffectiveLocationById(destinationLocationId) ??
      findEffectiveLocationById(runtime, destinationLocationId);
    if (!destinationLocation || !isLocationActive(destinationLocation)) {
      return {
        location: existing,
        userErrors: [locationNotFoundError('destinationLocationId')],
      };
    }

    transferLocationInventory(runtime, existing.id, destinationLocation);
  }

  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const nextLocation: LocationRecord = {
    ...existing,
    activatable: true,
    deactivatable: false,
    deactivatedAt: now,
    deletable: true,
    fulfillsOnlineOrders: false,
    hasActiveInventory: false,
    isActive: false,
    shipsInventory: false,
    updatedAt: now,
  };

  return { location: runtime.store.stageUpdateLocation(nextLocation), userErrors: [] };
}

function stageLocationActivate(runtime: ProxyRuntimeContext, locationId: string | null): LocationLifecyclePayload {
  if (!locationId) {
    return { location: null, userErrors: [locationNotFoundError('locationId')] };
  }

  const existing = runtime.store.getEffectiveLocationById(locationId) ?? findEffectiveLocationById(runtime, locationId);
  if (!existing) {
    return { location: null, userErrors: [locationNotFoundError('locationId')] };
  }

  if (isLocationActive(existing)) {
    return { location: existing, userErrors: [] };
  }

  if (existing.activatable === false) {
    return {
      location: existing,
      userErrors: [{ field: ['locationId'], message: 'Location cannot be activated.', code: 'GENERIC_ERROR' }],
    };
  }

  const duplicateActiveName = listEffectiveLocations(runtime).some(
    (location) => location.id !== existing.id && isLocationActive(location) && location.name === existing.name,
  );
  if (duplicateActiveName) {
    return {
      location: existing,
      userErrors: [
        {
          field: ['locationId'],
          message: 'A location with this name already exists.',
          code: 'HAS_NON_UNIQUE_NAME',
        },
      ],
    };
  }

  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const nextLocation: LocationRecord = {
    ...existing,
    activatable: false,
    deactivatable: true,
    deactivatedAt: null,
    deletable: false,
    fulfillsOnlineOrders: existing.fulfillsOnlineOrders ?? true,
    isActive: true,
    shipsInventory: true,
    updatedAt: now,
  };

  return { location: runtime.store.stageUpdateLocation(nextLocation), userErrors: [] };
}

function stageLocationDelete(runtime: ProxyRuntimeContext, locationId: string | null): LocationDeletePayload {
  if (!locationId) {
    return { deletedLocationId: null, userErrors: [locationNotFoundError('locationId')] };
  }

  const existing = runtime.store.getEffectiveLocationById(locationId) ?? findEffectiveLocationById(runtime, locationId);
  if (!existing) {
    return { deletedLocationId: null, userErrors: [locationNotFoundError('locationId')] };
  }

  const userErrors: LocationUserErrorRecord[] = [];

  if (isLocationActive(existing)) {
    userErrors.push({
      field: ['locationId'],
      message: 'The location cannot be deleted while it is active.',
      code: 'LOCATION_IS_ACTIVE',
    });
  }

  if (locationHasInventory(runtime, existing.id)) {
    userErrors.push({
      field: ['locationId'],
      message: 'The location cannot be deleted while it has inventory.',
      code: 'LOCATION_HAS_INVENTORY',
    });
  }

  if (existing.hasUnfulfilledOrders === true) {
    userErrors.push({
      field: ['locationId'],
      message: 'The location cannot be deleted while it has pending orders.',
      code: 'LOCATION_HAS_PENDING_ORDERS',
    });
  }

  if (userErrors.length > 0) {
    return { deletedLocationId: null, userErrors };
  }

  runtime.store.stageUpdateLocation({
    ...existing,
    deleted: true,
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
  });

  return { deletedLocationId: existing.id, userErrors: [] };
}

function serializeLocationMutationPayload(
  runtime: ProxyRuntimeContext,
  payload: { location: LocationRecord | null; userErrors: LocationUserErrorRecord[] },
  payloadTypename: string,
  userErrorTypename: string,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== payloadTypename) {
        continue;
      }
      Object.assign(
        result,
        serializeLocationMutationPayload(
          runtime,
          payload,
          payloadTypename,
          userErrorTypename,
          selection.selectionSet.selections,
          variables,
        ),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = payloadTypename;
        break;
      case 'location':
        result[key] = payload.location
          ? serializeLocation(runtime, payload.location, selection.selectionSet?.selections ?? [], variables)
          : null;
        break;
      case 'userErrors':
      case 'locationActivateUserErrors':
      case 'locationDeactivateUserErrors':
        result[key] = serializeLocationUserErrors(
          payload.userErrors,
          userErrorTypename,
          selection.selectionSet?.selections ?? [],
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeLocationDeletePayload(
  payload: LocationDeletePayload,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'LocationDeletePayload') {
        continue;
      }
      Object.assign(result, serializeLocationDeletePayload(payload, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'LocationDeletePayload';
        break;
      case 'deletedLocationId':
        result[key] = payload.deletedLocationId;
        break;
      case 'userErrors':
      case 'locationDeleteUserErrors':
        result[key] = serializeLocationUserErrors(
          payload.userErrors,
          'LocationDeleteUserError',
          selection.selectionSet?.selections ?? [],
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function normalizeFulfillmentServiceHandle(name: string): string {
  const handle = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/gu, '-')
    .replace(/^-+|-+$/gu, '');
  return handle.length > 0 ? handle : 'fulfillment-service';
}

function carrierServiceNumericId(id: string): string {
  return id.split('/').at(-1)?.split('?')[0] ?? id;
}

function carrierServiceFormattedName(name: string | null): string | null {
  return name ? `${name} (Rates provided by app)` : null;
}

function carrierServiceMatchesTerm(service: CarrierServiceRecord, term: SearchQueryTerm): boolean {
  const normalizedValue = normalizeSearchQueryValue(term.value);
  let matches = true;

  switch (term.field) {
    case 'active':
      matches = normalizedValue === String(service.active);
      break;
    case 'id':
      matches =
        normalizedValue === normalizeSearchQueryValue(service.id) ||
        normalizedValue === carrierServiceNumericId(service.id);
      break;
    default:
      matches = true;
      break;
  }

  return term.negated ? !matches : matches;
}

function filterCarrierServicesByQuery(services: CarrierServiceRecord[], rawQuery: unknown): CarrierServiceRecord[] {
  if (typeof rawQuery !== 'string' || rawQuery.trim().length === 0) {
    return services;
  }

  const terms = parseSearchQueryTerms(rawQuery.trim(), { ignoredKeywords: ['AND'] }).filter(
    (term) => term.field === 'active' || term.field === 'id',
  );
  return terms.length === 0
    ? services
    : services.filter((service) => terms.every((term) => carrierServiceMatchesTerm(service, term)));
}

function compareCarrierServices(left: CarrierServiceRecord, right: CarrierServiceRecord, sortKey: unknown): number {
  switch (sortKey) {
    case 'CREATED_AT':
      return Date.parse(left.createdAt) - Date.parse(right.createdAt) || compareShopifyResourceIds(left.id, right.id);
    case 'UPDATED_AT':
      return Date.parse(left.updatedAt) - Date.parse(right.updatedAt) || compareShopifyResourceIds(left.id, right.id);
    case 'ID':
    default:
      return compareShopifyResourceIds(left.id, right.id);
  }
}

function listCarrierServicesForConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): CarrierServiceRecord[] {
  const args = getFieldArguments(field, variables);
  const reverse = args['reverse'] === true;
  const sortedServices = filterCarrierServicesByQuery(runtime.store.listEffectiveCarrierServices(), args['query']).sort(
    (left, right) => compareCarrierServices(left, right, args['sortKey']),
  );

  return reverse ? sortedServices.reverse() : sortedServices;
}

function serializeCarrierServicesConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const services = listCarrierServicesForConnection(runtime, field, variables);
  const getCursor = (service: CarrierServiceRecord): string => service.id;
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(services, field, variables, getCursor);

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: getCursor,
    serializeNode: (service, selection) => serializeCarrierService(service, selection.selectionSet?.selections ?? []),
  });
}

function listLocationsAvailableForDeliveryProfiles(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): LocationRecord[] {
  const args = getFieldArguments(field, variables);
  const locations = listEffectiveLocations(runtime)
    .filter((location) => location.deleted !== true && location.isActive !== false)
    .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

  return args['reverse'] === true ? locations.reverse() : locations;
}

function serializeLocationsAvailableForDeliveryProfilesConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const locations = listLocationsAvailableForDeliveryProfiles(runtime, field, variables);
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    locations,
    field,
    variables,
    (location) => location.id,
  );

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (location) => location.id,
    serializeNode: (location, selection) =>
      serializeLocation(runtime, location, selection.selectionSet?.selections ?? [], variables),
  });
}

function serializeAvailableCarrierServiceLocation(
  runtime: ProxyRuntimeContext,
  location: LocationRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  return serializeLocation(runtime, location, selections, variables);
}

function serializeAvailableCarrierServicePair(
  runtime: ProxyRuntimeContext,
  service: CarrierServiceRecord,
  locations: LocationRecord[],
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (
        selection.typeCondition?.name.value &&
        selection.typeCondition.name.value !== 'DeliveryCarrierServiceAndLocations'
      ) {
        continue;
      }
      Object.assign(
        result,
        serializeAvailableCarrierServicePair(runtime, service, locations, selection.selectionSet.selections, variables),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'DeliveryCarrierServiceAndLocations';
        break;
      case 'carrierService':
        result[key] = serializeCarrierService(service, selection.selectionSet?.selections ?? []);
        break;
      case 'locations':
        result[key] = locations.map((location) =>
          serializeAvailableCarrierServiceLocation(
            runtime,
            location,
            selection.selectionSet?.selections ?? [],
            variables,
          ),
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeAvailableCarrierServices(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Array<Record<string, unknown>> {
  const locations = listEffectiveLocations(runtime).filter(
    (location) => location.deleted !== true && location.isActive !== false && location.isFulfillmentService !== true,
  );
  const services = runtime.store.listEffectiveCarrierServices().filter((service) => service.active);

  return services.map((service) =>
    serializeAvailableCarrierServicePair(runtime, service, locations, field.selectionSet?.selections ?? [], variables),
  );
}

function readCarrierServiceInput(args: Record<string, unknown>): Record<string, unknown> {
  const input = args['input'];
  return input && typeof input === 'object' && !Array.isArray(input) ? (input as Record<string, unknown>) : {};
}

function readCarrierServiceCallbackUrl(input: Record<string, unknown>): string | null {
  const value = input['callbackUrl'];
  return typeof value === 'string' && value.trim().length > 0 ? value : null;
}

function validateCarrierServiceName(name: string | null): CarrierServiceUserErrorRecord[] {
  if (typeof name !== 'string' || name.trim().length === 0) {
    return [{ field: null, message: "Shipping rate provider name can't be blank" }];
  }

  return [];
}

function stageCarrierServiceCreate(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): {
  carrierService: CarrierServiceRecord | null;
  userErrors: CarrierServiceUserErrorRecord[];
} {
  const input = readCarrierServiceInput(args);
  const name = typeof input['name'] === 'string' ? input['name'].trim() : null;
  const userErrors = validateCarrierServiceName(name);
  if (userErrors.length > 0 || !name) {
    return { carrierService: null, userErrors };
  }

  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const service: CarrierServiceRecord = {
    id: runtime.syntheticIdentity.makeProxySyntheticGid('DeliveryCarrierService'),
    name,
    formattedName: carrierServiceFormattedName(name),
    callbackUrl: readCarrierServiceCallbackUrl(input),
    active: input['active'] === true,
    supportsServiceDiscovery: input['supportsServiceDiscovery'] === true,
    createdAt: now,
    updatedAt: now,
  };

  return { carrierService: runtime.store.stageCreateCarrierService(service), userErrors: [] };
}

function carrierServiceNotFoundForUpdate(): CarrierServiceUserErrorRecord {
  return { field: null, message: 'The carrier or app could not be found.' };
}

function carrierServiceNotFoundForDelete(): CarrierServiceUserErrorRecord {
  return { field: ['id'], message: 'The carrier or app could not be found.' };
}

function stageCarrierServiceUpdate(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): {
  carrierService: CarrierServiceRecord | null;
  userErrors: CarrierServiceUserErrorRecord[];
} {
  const input = readCarrierServiceInput(args);
  const id = typeof input['id'] === 'string' ? input['id'] : null;
  const existing = id ? runtime.store.getEffectiveCarrierServiceById(id) : null;
  if (!id || !existing) {
    return { carrierService: null, userErrors: [carrierServiceNotFoundForUpdate()] };
  }

  const nextName = typeof input['name'] === 'string' ? input['name'].trim() : existing.name;
  const userErrors = validateCarrierServiceName(nextName);
  if (userErrors.length > 0) {
    return { carrierService: null, userErrors };
  }

  const carrierService: CarrierServiceRecord = {
    ...existing,
    name: nextName,
    formattedName: carrierServiceFormattedName(nextName),
    callbackUrl: Object.prototype.hasOwnProperty.call(input, 'callbackUrl')
      ? readCarrierServiceCallbackUrl(input)
      : existing.callbackUrl,
    active: typeof input['active'] === 'boolean' ? input['active'] : existing.active,
    supportsServiceDiscovery:
      typeof input['supportsServiceDiscovery'] === 'boolean'
        ? input['supportsServiceDiscovery']
        : existing.supportsServiceDiscovery,
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
  };

  return { carrierService: runtime.store.stageUpdateCarrierService(carrierService), userErrors: [] };
}

function stageCarrierServiceDelete(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): {
  deletedId: string | null;
  userErrors: CarrierServiceUserErrorRecord[];
} {
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const existing = id ? runtime.store.getEffectiveCarrierServiceById(id) : null;
  if (!id || !existing) {
    return { deletedId: null, userErrors: [carrierServiceNotFoundForDelete()] };
  }

  runtime.store.stageDeleteCarrierService(id);
  return { deletedId: id, userErrors: [] };
}

function serializeCarrierServiceUserErrors(
  userErrors: CarrierServiceUserErrorRecord[],
  selections: readonly SelectionNode[],
): Array<Record<string, unknown>> {
  return userErrors.map((userError) => {
    const result: Record<string, unknown> = {};

    for (const selection of selections) {
      if (selection.kind === Kind.INLINE_FRAGMENT) {
        Object.assign(result, serializeCarrierServiceUserErrors([userError], selection.selectionSet.selections)[0]);
        continue;
      }

      if (selection.kind !== Kind.FIELD) {
        continue;
      }

      const key = responseKey(selection);
      switch (selection.name.value) {
        case '__typename':
          result[key] = 'UserError';
          break;
        case 'field':
          result[key] = userError.field ? structuredClone(userError.field) : null;
          break;
        case 'message':
          result[key] = userError.message;
          break;
        default:
          result[key] = null;
      }
    }

    return result;
  });
}

function serializeCarrierServiceMutationPayload(
  payload: { carrierService: CarrierServiceRecord | null; userErrors: CarrierServiceUserErrorRecord[] },
  payloadTypename: string,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== payloadTypename) {
        continue;
      }
      Object.assign(
        result,
        serializeCarrierServiceMutationPayload(payload, payloadTypename, selection.selectionSet.selections),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = payloadTypename;
        break;
      case 'carrierService':
        result[key] = payload.carrierService
          ? serializeCarrierService(payload.carrierService, selection.selectionSet?.selections ?? [])
          : null;
        break;
      case 'userErrors':
        result[key] = serializeCarrierServiceUserErrors(payload.userErrors, selection.selectionSet?.selections ?? []);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeCarrierServiceDeletePayload(
  payload: { deletedId: string | null; userErrors: CarrierServiceUserErrorRecord[] },
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'CarrierServiceDeletePayload') {
        continue;
      }
      Object.assign(result, serializeCarrierServiceDeletePayload(payload, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'CarrierServiceDeletePayload';
        break;
      case 'deletedId':
        result[key] = payload.deletedId;
        break;
      case 'userErrors':
        result[key] = serializeCarrierServiceUserErrors(payload.userErrors, selection.selectionSet?.selections ?? []);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function localPickupLocationNotFound(
  field: 'localPickupSettings' | 'locationId',
  locationId: string | null,
): LocalPickupUserErrorRecord {
  const legacyId = locationId ? (readLegacyResourceIdFromGid(locationId) ?? locationId) : '';
  return {
    field: [field],
    message: `Unable to find an active location for location ID ${legacyId}`,
    code: 'ACTIVE_LOCATION_NOT_FOUND',
  };
}

function serializeLocalPickupUserErrors(
  userErrors: LocalPickupUserErrorRecord[],
  selections: readonly SelectionNode[],
): Array<Record<string, unknown>> {
  return userErrors.map((userError) => {
    const result: Record<string, unknown> = {};

    for (const selection of selections) {
      if (selection.kind === Kind.INLINE_FRAGMENT) {
        Object.assign(result, serializeLocalPickupUserErrors([userError], selection.selectionSet.selections)[0] ?? {});
        continue;
      }

      if (selection.kind !== Kind.FIELD) {
        continue;
      }

      const key = responseKey(selection);
      switch (selection.name.value) {
        case '__typename':
          result[key] = 'DeliveryLocationLocalPickupSettingsError';
          break;
        case 'field':
          result[key] = userError.field ? structuredClone(userError.field) : null;
          break;
        case 'message':
          result[key] = userError.message;
          break;
        case 'code':
          result[key] = userError.code;
          break;
        default:
          result[key] = null;
      }
    }

    return result;
  });
}

function readLocalPickupSettings(args: Record<string, unknown>): Record<string, unknown> {
  const input = args['localPickupSettings'];
  return input && typeof input === 'object' && !Array.isArray(input) ? (input as Record<string, unknown>) : {};
}

function stageLocationLocalPickupEnable(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): {
  localPickupSettings: DeliveryLocalPickupSettingsRecord | null;
  userErrors: LocalPickupUserErrorRecord[];
} {
  const input = readLocalPickupSettings(args);
  const locationId = typeof input['locationId'] === 'string' ? input['locationId'] : null;
  const location = locationId ? findEffectiveLocationById(runtime, locationId) : null;
  if (!location || location.isActive === false) {
    return { localPickupSettings: null, userErrors: [localPickupLocationNotFound('localPickupSettings', locationId)] };
  }

  const settings: DeliveryLocalPickupSettingsRecord = {
    pickupTime: typeof input['pickupTime'] === 'string' ? input['pickupTime'] : 'ONE_HOUR',
    instructions: typeof input['instructions'] === 'string' ? input['instructions'] : '',
  };
  runtime.store.stageUpdateLocation({
    ...location,
    localPickupSettings: settings,
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
  });
  return { localPickupSettings: settings, userErrors: [] };
}

function stageLocationLocalPickupDisable(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): {
  locationId: string | null;
  userErrors: LocalPickupUserErrorRecord[];
} {
  const locationId = typeof args['locationId'] === 'string' ? args['locationId'] : null;
  const location = locationId ? findEffectiveLocationById(runtime, locationId) : null;
  if (!location || location.isActive === false) {
    return { locationId: null, userErrors: [localPickupLocationNotFound('locationId', locationId)] };
  }

  runtime.store.stageUpdateLocation({
    ...location,
    localPickupSettings: null,
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
  });
  return { locationId, userErrors: [] };
}

function serializeLocationLocalPickupEnablePayload(
  payload: { localPickupSettings: DeliveryLocalPickupSettingsRecord | null; userErrors: LocalPickupUserErrorRecord[] },
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (
        selection.typeCondition?.name.value &&
        selection.typeCondition.name.value !== 'LocationLocalPickupEnablePayload'
      ) {
        continue;
      }
      Object.assign(result, serializeLocationLocalPickupEnablePayload(payload, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'LocationLocalPickupEnablePayload';
        break;
      case 'localPickupSettings':
        result[key] = payload.localPickupSettings
          ? serializeLocalPickupSettings(payload.localPickupSettings, selection.selectionSet?.selections ?? [])
          : null;
        break;
      case 'userErrors':
        result[key] = serializeLocalPickupUserErrors(payload.userErrors, selection.selectionSet?.selections ?? []);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeLocationLocalPickupDisablePayload(
  payload: { locationId: string | null; userErrors: LocalPickupUserErrorRecord[] },
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (
        selection.typeCondition?.name.value &&
        selection.typeCondition.name.value !== 'LocationLocalPickupDisablePayload'
      ) {
        continue;
      }
      Object.assign(result, serializeLocationLocalPickupDisablePayload(payload, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'LocationLocalPickupDisablePayload';
        break;
      case 'locationId':
        result[key] = payload.locationId;
        break;
      case 'userErrors':
        result[key] = serializeLocalPickupUserErrors(payload.userErrors, selection.selectionSet?.selections ?? []);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function readRecordValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readOptionalInputNumber(input: Record<string, unknown>, key: string): number | null {
  const value = input[key];
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function readShippingPackageWeight(
  input: Record<string, unknown>,
  base: ShippingPackageWeightRecord | null,
): ShippingPackageWeightRecord | null {
  if (!hasInputField(input, 'weight')) {
    return base;
  }

  const weight = readRecordValue(input['weight']);
  if (!weight) {
    return null;
  }

  return {
    value: readOptionalInputNumber(weight, 'value'),
    unit: readOptionalInputString(weight, 'unit'),
  };
}

function readShippingPackageDimensions(
  input: Record<string, unknown>,
  base: ShippingPackageDimensionsRecord | null,
): ShippingPackageDimensionsRecord | null {
  if (!hasInputField(input, 'dimensions')) {
    return base;
  }

  const dimensions = readRecordValue(input['dimensions']);
  if (!dimensions) {
    return null;
  }

  return {
    length: readOptionalInputNumber(dimensions, 'length'),
    width: readOptionalInputNumber(dimensions, 'width'),
    height: readOptionalInputNumber(dimensions, 'height'),
    unit: readOptionalInputString(dimensions, 'unit'),
  };
}

function shippingPackageInvalidIdError(field: FieldNode): GraphQLResponseError {
  const error: GraphQLResponseError = {
    message: 'invalid id',
    path: [responseKey(field)],
    extensions: {
      code: 'RESOURCE_NOT_FOUND',
    },
  };

  if (field.loc) {
    error.locations = [{ line: field.loc.startToken.line, column: field.loc.startToken.column }];
  }

  return error;
}

function serializeShippingPackageUserErrors(
  userErrors: ShippingPackageUserErrorRecord[],
  selections: readonly SelectionNode[],
): Array<Record<string, unknown>> {
  return userErrors.map((userError) => {
    const result: Record<string, unknown> = {};

    for (const selection of selections) {
      if (selection.kind === Kind.INLINE_FRAGMENT) {
        Object.assign(
          result,
          serializeShippingPackageUserErrors([userError], selection.selectionSet.selections)[0] ?? {},
        );
        continue;
      }

      if (selection.kind !== Kind.FIELD) {
        continue;
      }

      const key = responseKey(selection);
      switch (selection.name.value) {
        case '__typename':
          result[key] = 'UserError';
          break;
        case 'field':
          result[key] = userError.field ? structuredClone(userError.field) : null;
          break;
        case 'message':
          result[key] = userError.message;
          break;
        default:
          result[key] = null;
      }
    }

    return result;
  });
}

function stageShippingPackageUpdate(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): ShippingPackageRecord | null {
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const existing = id ? runtime.store.getEffectiveShippingPackageById(id) : null;
  if (!id || !existing) {
    return null;
  }

  const input = readRecordValue(args['shippingPackage']) ?? {};
  const nextPackage: ShippingPackageRecord = {
    ...existing,
    name: hasInputField(input, 'name') ? readOptionalInputString(input, 'name') : existing.name,
    type: hasInputField(input, 'type') ? readOptionalInputString(input, 'type') : existing.type,
    default: typeof input['default'] === 'boolean' ? input['default'] : existing.default,
    weight: readShippingPackageWeight(input, existing.weight),
    dimensions: readShippingPackageDimensions(input, existing.dimensions),
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
  };

  if (nextPackage.default) {
    for (const shippingPackage of runtime.store.listEffectiveShippingPackages()) {
      if (shippingPackage.id !== nextPackage.id && shippingPackage.default) {
        runtime.store.stageUpdateShippingPackage({
          ...shippingPackage,
          default: false,
          updatedAt: nextPackage.updatedAt,
        });
      }
    }
  }

  return runtime.store.stageUpdateShippingPackage(nextPackage);
}

function stageShippingPackageMakeDefault(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): ShippingPackageRecord | null {
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const existing = id ? runtime.store.getEffectiveShippingPackageById(id) : null;
  if (!id || !existing) {
    return null;
  }

  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  for (const shippingPackage of runtime.store.listEffectiveShippingPackages()) {
    runtime.store.stageUpdateShippingPackage({
      ...shippingPackage,
      default: shippingPackage.id === id,
      updatedAt: now,
    });
  }

  return runtime.store.getEffectiveShippingPackageById(id);
}

function stageShippingPackageDelete(runtime: ProxyRuntimeContext, args: Record<string, unknown>): string | null {
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const existing = id ? runtime.store.getEffectiveShippingPackageById(id) : null;
  if (!id || !existing) {
    return null;
  }

  runtime.store.stageDeleteShippingPackage(id);
  return id;
}

function serializeShippingPackageUserErrorsOnlyPayload(
  selections: readonly SelectionNode[],
  payloadTypename: 'ShippingPackageUpdatePayload' | 'ShippingPackageMakeDefaultPayload',
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== payloadTypename) {
        continue;
      }
      Object.assign(
        result,
        serializeShippingPackageUserErrorsOnlyPayload(selection.selectionSet.selections, payloadTypename),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = payloadTypename;
        break;
      case 'userErrors':
        result[key] = serializeShippingPackageUserErrors([], selection.selectionSet?.selections ?? []);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeShippingPackageDeletePayload(
  deletedId: string,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (
        selection.typeCondition?.name.value &&
        selection.typeCondition.name.value !== 'ShippingPackageDeletePayload'
      ) {
        continue;
      }
      Object.assign(result, serializeShippingPackageDeletePayload(deletedId, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShippingPackageDeletePayload';
        break;
      case 'deletedId':
        result[key] = deletedId;
        break;
      case 'userErrors':
        result[key] = serializeShippingPackageUserErrors([], selection.selectionSet?.selections ?? []);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function isAllowedFulfillmentServiceCallbackUrl(callbackUrl: string | null): boolean {
  if (callbackUrl === null || callbackUrl.trim().length === 0) {
    return true;
  }

  try {
    const url = new URL(callbackUrl);
    return url.protocol === 'https:' && url.hostname === 'mock.shop';
  } catch {
    return false;
  }
}

function fulfillmentServiceLocationReference(service: FulfillmentServiceRecord): LocationFulfillmentServiceRecord {
  return {
    id: service.id,
    handle: service.handle,
    serviceName: service.serviceName,
    callbackUrl: service.callbackUrl,
    inventoryManagement: service.inventoryManagement,
    locationId: service.locationId,
    requiresShippingMethod: service.requiresShippingMethod,
    trackingSupport: service.trackingSupport,
    type: service.type,
  };
}

function buildFulfillmentServiceLocation(
  runtime: ProxyRuntimeContext,
  service: FulfillmentServiceRecord,
  existing?: LocationRecord | null,
): LocationRecord {
  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  return {
    id: service.locationId ?? runtime.syntheticIdentity.makeProxySyntheticGid('Location'),
    name: service.serviceName,
    legacyResourceId: existing?.legacyResourceId ?? null,
    activatable: existing?.activatable ?? false,
    addressVerified: existing?.addressVerified ?? false,
    createdAt: existing?.createdAt ?? now,
    deactivatable: existing?.deactivatable ?? false,
    deactivatedAt: existing?.deactivatedAt ?? null,
    deletable: existing?.deletable ?? false,
    fulfillmentService: fulfillmentServiceLocationReference(service),
    fulfillsOnlineOrders: true,
    hasActiveInventory: existing?.hasActiveInventory ?? false,
    hasUnfulfilledOrders: existing?.hasUnfulfilledOrders ?? false,
    isActive: true,
    isFulfillmentService: true,
    shipsInventory: false,
    updatedAt: now,
    address: existing?.address ?? null,
    suggestedAddresses: existing?.suggestedAddresses ?? [],
    metafields: existing?.metafields ?? [],
  };
}

function readFulfillmentServiceCallbackUrl(args: Record<string, unknown>): string | null {
  const value = args['callbackUrl'];
  return typeof value === 'string' && value.trim().length > 0 ? value : null;
}

function validateFulfillmentServiceName(name: string | null): FulfillmentServiceUserErrorRecord[] {
  if (typeof name !== 'string' || name.trim().length === 0) {
    return [{ field: ['name'], message: "Name can't be blank" }];
  }

  return [];
}

function validateFulfillmentServiceCallbackUrl(callbackUrl: string | null): FulfillmentServiceUserErrorRecord[] {
  if (!isAllowedFulfillmentServiceCallbackUrl(callbackUrl)) {
    return [{ field: ['callbackUrl'], message: 'Callback url is not allowed' }];
  }

  return [];
}

function stageFulfillmentServiceCreate(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): {
  fulfillmentService: FulfillmentServiceRecord | null;
  userErrors: FulfillmentServiceUserErrorRecord[];
} {
  const name = typeof args['name'] === 'string' ? args['name'].trim() : null;
  const callbackUrl = readFulfillmentServiceCallbackUrl(args);
  const userErrors = [...validateFulfillmentServiceName(name), ...validateFulfillmentServiceCallbackUrl(callbackUrl)];
  if (userErrors.length > 0 || !name) {
    return { fulfillmentService: null, userErrors };
  }

  const locationId = runtime.syntheticIdentity.makeProxySyntheticGid('Location');
  const service: FulfillmentServiceRecord = {
    id: runtime.syntheticIdentity.makeProxySyntheticGid('FulfillmentService'),
    handle: normalizeFulfillmentServiceHandle(name),
    serviceName: name,
    callbackUrl,
    inventoryManagement: typeof args['inventoryManagement'] === 'boolean' ? args['inventoryManagement'] : false,
    locationId,
    requiresShippingMethod: typeof args['requiresShippingMethod'] === 'boolean' ? args['requiresShippingMethod'] : true,
    trackingSupport: typeof args['trackingSupport'] === 'boolean' ? args['trackingSupport'] : false,
    type: 'THIRD_PARTY',
  };

  const stagedService = runtime.store.stageCreateFulfillmentService(service);
  runtime.store.stageCreateLocation(buildFulfillmentServiceLocation(runtime, stagedService));
  return { fulfillmentService: stagedService, userErrors: [] };
}

function stageFulfillmentServiceUpdate(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): {
  fulfillmentService: FulfillmentServiceRecord | null;
  userErrors: FulfillmentServiceUserErrorRecord[];
} {
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const existing = id ? runtime.store.getEffectiveFulfillmentServiceById(id) : null;
  if (!id || !existing) {
    return {
      fulfillmentService: null,
      userErrors: [{ field: ['id'], message: 'Fulfillment service could not be found.' }],
    };
  }

  const nextName = typeof args['name'] === 'string' ? args['name'].trim() : existing.serviceName;
  const callbackUrl = Object.prototype.hasOwnProperty.call(args, 'callbackUrl')
    ? readFulfillmentServiceCallbackUrl(args)
    : existing.callbackUrl;
  const userErrors = [
    ...validateFulfillmentServiceName(nextName),
    ...validateFulfillmentServiceCallbackUrl(callbackUrl),
  ];
  if (userErrors.length > 0) {
    return { fulfillmentService: null, userErrors };
  }

  const service: FulfillmentServiceRecord = {
    ...existing,
    serviceName: nextName,
    callbackUrl,
    inventoryManagement:
      typeof args['inventoryManagement'] === 'boolean' ? args['inventoryManagement'] : existing.inventoryManagement,
    requiresShippingMethod:
      typeof args['requiresShippingMethod'] === 'boolean'
        ? args['requiresShippingMethod']
        : existing.requiresShippingMethod,
    trackingSupport: typeof args['trackingSupport'] === 'boolean' ? args['trackingSupport'] : existing.trackingSupport,
  };

  const stagedService = runtime.store.stageUpdateFulfillmentService(service);
  if (stagedService.locationId) {
    runtime.store.stageUpdateLocation(
      buildFulfillmentServiceLocation(
        runtime,
        stagedService,
        runtime.store.getEffectiveLocationById(stagedService.locationId),
      ),
    );
  }

  return { fulfillmentService: stagedService, userErrors: [] };
}

function stripQueryFromGid(id: string): string {
  return id.split('?')[0] ?? id;
}

function stageFulfillmentServiceDelete(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): {
  deletedId: string | null;
  userErrors: FulfillmentServiceUserErrorRecord[];
} {
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const existing = id ? runtime.store.getEffectiveFulfillmentServiceById(id) : null;
  if (!id || !existing) {
    return {
      deletedId: null,
      userErrors: [{ field: ['id'], message: 'Fulfillment service could not be found.' }],
    };
  }

  const inventoryAction = typeof args['inventoryAction'] === 'string' ? args['inventoryAction'] : 'DELETE';
  if (inventoryAction === 'TRANSFER') {
    const destinationLocationId =
      typeof args['destinationLocationId'] === 'string' ? args['destinationLocationId'] : null;
    if (!destinationLocationId || !findEffectiveLocationById(runtime, destinationLocationId)) {
      return {
        deletedId: null,
        userErrors: [{ field: ['destinationLocationId'], message: 'Destination location could not be found.' }],
      };
    }
  }

  runtime.store.stageDeleteFulfillmentService(id);
  if (existing.locationId) {
    if (inventoryAction === 'KEEP') {
      const location = findEffectiveLocationById(runtime, existing.locationId);
      if (location) {
        runtime.store.stageUpdateLocation({
          ...location,
          fulfillmentService: null,
          isFulfillmentService: false,
          shipsInventory: true,
          updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
        });
      }
    } else {
      runtime.store.stageDeleteLocation(existing.locationId);
    }
  }

  return { deletedId: stripQueryFromGid(id), userErrors: [] };
}

function serializeFulfillmentServiceUserErrors(
  userErrors: FulfillmentServiceUserErrorRecord[],
  selections: readonly SelectionNode[],
): Array<Record<string, unknown>> {
  return userErrors.map((userError) => {
    const result: Record<string, unknown> = {};

    for (const selection of selections) {
      if (selection.kind === Kind.INLINE_FRAGMENT) {
        Object.assign(result, serializeFulfillmentServiceUserErrors([userError], selection.selectionSet.selections)[0]);
        continue;
      }

      if (selection.kind !== Kind.FIELD) {
        continue;
      }

      const key = responseKey(selection);
      switch (selection.name.value) {
        case '__typename':
          result[key] = 'UserError';
          break;
        case 'field':
          result[key] = userError.field ? structuredClone(userError.field) : null;
          break;
        case 'message':
          result[key] = userError.message;
          break;
        default:
          result[key] = null;
      }
    }

    return result;
  });
}

function serializeFulfillmentServiceMutationPayload(
  runtime: ProxyRuntimeContext,
  payload: { fulfillmentService: FulfillmentServiceRecord | null; userErrors: FulfillmentServiceUserErrorRecord[] },
  payloadTypename: string,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== payloadTypename) {
        continue;
      }
      Object.assign(
        result,
        serializeFulfillmentServiceMutationPayload(
          runtime,
          payload,
          payloadTypename,
          selection.selectionSet.selections,
        ),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = payloadTypename;
        break;
      case 'fulfillmentService':
        result[key] = payload.fulfillmentService
          ? serializeFulfillmentService(
              runtime,
              payload.fulfillmentService,
              selection.selectionSet?.selections ?? [],
              {},
            )
          : null;
        break;
      case 'userErrors':
        result[key] = serializeFulfillmentServiceUserErrors(
          payload.userErrors,
          selection.selectionSet?.selections ?? [],
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeFulfillmentServiceDeletePayload(
  payload: { deletedId: string | null; userErrors: FulfillmentServiceUserErrorRecord[] },
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (
        selection.typeCondition?.name.value &&
        selection.typeCondition.name.value !== 'FulfillmentServiceDeletePayload'
      ) {
        continue;
      }
      Object.assign(result, serializeFulfillmentServiceDeletePayload(payload, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'FulfillmentServiceDeletePayload';
        break;
      case 'deletedId':
        result[key] = payload.deletedId;
        break;
      case 'userErrors':
        result[key] = serializeFulfillmentServiceUserErrors(
          payload.userErrors,
          selection.selectionSet?.selections ?? [],
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function readNumericGidTail(id: string): string | null {
  const tail = id.split('/').at(-1)?.split('?')[0] ?? '';
  return /^\d+$/.test(tail) ? tail : null;
}

function buildShopPolicyUrl(shop: ShopRecord, policyId: string, type: string): string {
  const shopTail = readNumericGidTail(shop.id);
  const policyTail = readNumericGidTail(policyId);
  if (shopTail && policyTail) {
    return `https://checkout.shopify.com/${shopTail}/policies/${policyTail}.html?locale=en`;
  }

  return `${shop.url.replace(/\/$/, '')}/policies/${type.toLowerCase().replaceAll('_', '-')}`;
}

function compareShopPoliciesByType(left: ShopPolicyRecord, right: ShopPolicyRecord): number {
  const leftIndex = SHOP_POLICY_TYPE_ORDER.indexOf(left.type as (typeof SHOP_POLICY_TYPE_ORDER)[number]);
  const rightIndex = SHOP_POLICY_TYPE_ORDER.indexOf(right.type as (typeof SHOP_POLICY_TYPE_ORDER)[number]);
  const normalizedLeftIndex = leftIndex === -1 ? SHOP_POLICY_TYPE_ORDER.length : leftIndex;
  const normalizedRightIndex = rightIndex === -1 ? SHOP_POLICY_TYPE_ORDER.length : rightIndex;
  return normalizedLeftIndex - normalizedRightIndex || left.type.localeCompare(right.type);
}

function validateShopPolicyInput(input: Record<string, unknown>): {
  body: string | null;
  type: string | null;
  userErrors: ShopPolicyUserErrorRecord[];
} {
  const rawType = input['type'];
  const rawBody = input['body'];
  const type = typeof rawType === 'string' ? rawType : null;
  const body = typeof rawBody === 'string' ? rawBody : null;
  const userErrors: ShopPolicyUserErrorRecord[] = [];

  if (!type || !SHOP_POLICY_TYPES.has(type)) {
    userErrors.push({
      field: ['shopPolicy', 'type'],
      message: 'Type is invalid',
      code: null,
    });
  }

  if (body === null) {
    userErrors.push({
      field: ['shopPolicy', 'body'],
      message: 'Body is required',
      code: null,
    });
  } else if (Buffer.byteLength(body, 'utf8') > SHOP_POLICY_BODY_LIMIT_BYTES) {
    userErrors.push({
      field: ['shopPolicy', 'body'],
      message: 'Body is too big (maximum is 512 KB)',
      code: 'TOO_BIG',
    });
  }

  return {
    body,
    type,
    userErrors,
  };
}

function stageShopPolicyUpdate(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
): {
  shopPolicy: ShopPolicyRecord | null;
  userErrors: ShopPolicyUserErrorRecord[];
} {
  const validation = validateShopPolicyInput(input);
  if (validation.userErrors.length > 0 || !validation.type || validation.body === null) {
    return {
      shopPolicy: null,
      userErrors: validation.userErrors,
    };
  }

  const shop = runtime.store.getEffectiveShop();
  if (!shop) {
    return {
      shopPolicy: null,
      userErrors: [
        {
          field: ['shopPolicy'],
          message: 'Shop baseline is required to stage a shop policy update',
          code: null,
        },
      ],
    };
  }

  const existingPolicy = shop.shopPolicies.find((policy) => policy.type === validation.type) ?? null;
  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const id = existingPolicy?.id ?? runtime.syntheticIdentity.makeSyntheticGid('ShopPolicy');
  const policy: ShopPolicyRecord = {
    id,
    title: existingPolicy?.title ?? SHOP_POLICY_TITLES_BY_TYPE[validation.type] ?? validation.type,
    body: validation.body,
    type: validation.type,
    url: existingPolicy?.url ?? buildShopPolicyUrl(shop, id, validation.type),
    createdAt: existingPolicy?.createdAt ?? now,
    updatedAt: now,
  };
  const otherPolicies = shop.shopPolicies.filter((candidate) => candidate.type !== policy.type);
  const updatedShop: ShopRecord = {
    ...shop,
    shopPolicies: [...otherPolicies, policy].sort(compareShopPoliciesByType),
  };

  runtime.store.stageShop(updatedShop);

  return {
    shopPolicy: policy,
    userErrors: [],
  };
}

function serializeShopPolicyUpdatePayload(
  payload: { shopPolicy: ShopPolicyRecord | null; userErrors: ShopPolicyUserErrorRecord[] },
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopPolicyUpdatePayload') {
        continue;
      }
      Object.assign(result, serializeShopPolicyUpdatePayload(payload, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ShopPolicyUpdatePayload';
        break;
      case 'shopPolicy':
        result[key] = payload.shopPolicy
          ? serializeShopPolicy(payload.shopPolicy, selection.selectionSet?.selections ?? [])
          : null;
        break;
      case 'userErrors':
        result[key] = payload.userErrors.map((userError) =>
          serializeShopPolicyUserError(userError, selection.selectionSet?.selections ?? []),
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

export function handleStorePropertiesMutation(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const fields = getRootFields(document);
  const data: Record<string, unknown> = {};

  for (const field of fields) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'carrierServiceCreate': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeCarrierServiceMutationPayload(
          stageCarrierServiceCreate(runtime, args),
          'CarrierServiceCreatePayload',
          field.selectionSet?.selections ?? [],
        );
        break;
      }
      case 'carrierServiceUpdate': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeCarrierServiceMutationPayload(
          stageCarrierServiceUpdate(runtime, args),
          'CarrierServiceUpdatePayload',
          field.selectionSet?.selections ?? [],
        );
        break;
      }
      case 'carrierServiceDelete': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeCarrierServiceDeletePayload(
          stageCarrierServiceDelete(runtime, args),
          field.selectionSet?.selections ?? [],
        );
        break;
      }
      case 'locationLocalPickupEnable': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeLocationLocalPickupEnablePayload(
          stageLocationLocalPickupEnable(runtime, args),
          field.selectionSet?.selections ?? [],
        );
        break;
      }
      case 'locationLocalPickupDisable': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeLocationLocalPickupDisablePayload(
          stageLocationLocalPickupDisable(runtime, args),
          field.selectionSet?.selections ?? [],
        );
        break;
      }
      case 'shippingPackageUpdate': {
        const args = getFieldArguments(field, variables);
        const updatedPackage = stageShippingPackageUpdate(runtime, args);
        if (!updatedPackage) {
          return { errors: [shippingPackageInvalidIdError(field)], data: { [key]: null } };
        }
        data[key] = serializeShippingPackageUserErrorsOnlyPayload(
          field.selectionSet?.selections ?? [],
          'ShippingPackageUpdatePayload',
        );
        break;
      }
      case 'shippingPackageMakeDefault': {
        const args = getFieldArguments(field, variables);
        const defaultPackage = stageShippingPackageMakeDefault(runtime, args);
        if (!defaultPackage) {
          return { errors: [shippingPackageInvalidIdError(field)], data: { [key]: null } };
        }
        data[key] = serializeShippingPackageUserErrorsOnlyPayload(
          field.selectionSet?.selections ?? [],
          'ShippingPackageMakeDefaultPayload',
        );
        break;
      }
      case 'shippingPackageDelete': {
        const args = getFieldArguments(field, variables);
        const deletedId = stageShippingPackageDelete(runtime, args);
        if (!deletedId) {
          return { errors: [shippingPackageInvalidIdError(field)], data: { [key]: null } };
        }
        data[key] = serializeShippingPackageDeletePayload(deletedId, field.selectionSet?.selections ?? []);
        break;
      }
      case 'fulfillmentServiceCreate': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeFulfillmentServiceMutationPayload(
          runtime,
          stageFulfillmentServiceCreate(runtime, args),
          'FulfillmentServiceCreatePayload',
          field.selectionSet?.selections ?? [],
        );
        break;
      }
      case 'fulfillmentServiceUpdate': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeFulfillmentServiceMutationPayload(
          runtime,
          stageFulfillmentServiceUpdate(runtime, args),
          'FulfillmentServiceUpdatePayload',
          field.selectionSet?.selections ?? [],
        );
        break;
      }
      case 'fulfillmentServiceDelete': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeFulfillmentServiceDeletePayload(
          stageFulfillmentServiceDelete(runtime, args),
          field.selectionSet?.selections ?? [],
        );
        break;
      }
      case 'locationAdd': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeLocationMutationPayload(
          runtime,
          stageLocationAdd(runtime, readLocationInput(args)),
          'LocationAddPayload',
          'LocationAddUserError',
          field.selectionSet?.selections ?? [],
          variables,
        );
        break;
      }
      case 'locationEdit': {
        const args = getFieldArguments(field, variables);
        const id = typeof args['id'] === 'string' ? args['id'] : null;
        data[key] = serializeLocationMutationPayload(
          runtime,
          stageLocationEdit(runtime, id, readLocationInput(args)),
          'LocationEditPayload',
          'LocationEditUserError',
          field.selectionSet?.selections ?? [],
          variables,
        );
        break;
      }
      case 'locationActivate': {
        if (!readIdempotencyKey(field, variables)) {
          return { errors: [buildMissingIdempotencyKeyError(field)], data: { [key]: null } };
        }

        const args = getFieldArguments(field, variables);
        const locationId = typeof args['locationId'] === 'string' ? args['locationId'] : null;
        data[key] = serializeLocationMutationPayload(
          runtime,
          stageLocationActivate(runtime, locationId),
          'LocationActivatePayload',
          'LocationActivateUserError',
          field.selectionSet?.selections ?? [],
          variables,
        );
        break;
      }
      case 'locationDeactivate': {
        if (!readIdempotencyKey(field, variables)) {
          return { errors: [buildMissingIdempotencyKeyError(field)], data: { [key]: null } };
        }

        const args = getFieldArguments(field, variables);
        const locationId = typeof args['locationId'] === 'string' ? args['locationId'] : null;
        const destinationLocationId =
          typeof args['destinationLocationId'] === 'string' ? args['destinationLocationId'] : null;
        data[key] = serializeLocationMutationPayload(
          runtime,
          stageLocationDeactivate(runtime, locationId, destinationLocationId),
          'LocationDeactivatePayload',
          'LocationDeactivateUserError',
          field.selectionSet?.selections ?? [],
          variables,
        );
        break;
      }
      case 'locationDelete': {
        const args = getFieldArguments(field, variables);
        const locationId = typeof args['locationId'] === 'string' ? args['locationId'] : null;
        data[key] = serializeLocationDeletePayload(
          stageLocationDelete(runtime, locationId),
          field.selectionSet?.selections ?? [],
        );
        break;
      }
      case 'shopPolicyUpdate': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeShopPolicyUpdatePayload(
          stageShopPolicyUpdate(runtime, readShopPolicyInput(args)),
          field.selectionSet?.selections ?? [],
        );
        break;
      }
      default:
        data[key] = null;
    }
  }

  return { data };
}

function invalidLocationIdentifierError(field: FieldNode): GraphQLResponseError {
  return {
    message: "OneOf Input Object 'LocationIdentifierInput' must specify exactly one key.",
    path: [field.name.value, 'identifier'],
    extensions: {
      code: 'invalidOneOfInputObject',
      inputObjectType: 'LocationIdentifierInput',
    },
  };
}

function buildLocationCustomIdDefinitionMissingError(field: FieldNode): GraphQLResponseError {
  return {
    message: "Metafield definition of type 'id' is required when using custom ids.",
    locations: getNodeLocation(field),
    path: [responseKey(field)],
    extensions: {
      code: 'NOT_FOUND',
    },
  };
}

function resolveLocationIdentifier(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  context: SerializationContext,
): LocationRecord | null {
  const args = getFieldArguments(field, variables);
  const identifier = args['identifier'];
  if (!identifier || typeof identifier !== 'object' || Array.isArray(identifier)) {
    context.fatalErrors.push(invalidLocationIdentifierError(field));
    return null;
  }

  const identifierRecord = identifier as Record<string, unknown>;
  const populatedKeys = ['id', 'customId'].filter(
    (key) => identifierRecord[key] !== undefined && identifierRecord[key] !== null,
  );
  if (populatedKeys.length !== 1) {
    context.fatalErrors.push(invalidLocationIdentifierError(field));
    return null;
  }

  if (typeof identifierRecord['id'] === 'string' && identifierRecord['id'].length > 0) {
    return findEffectiveLocationById(runtime, identifierRecord['id']);
  }

  if (identifierRecord['customId'] !== undefined && identifierRecord['customId'] !== null) {
    context.errors.push(buildLocationCustomIdDefinitionMissingError(field));
    return null;
  }

  return null;
}

export function handleStorePropertiesQuery(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const fields = getRootFields(document);
  const context: SerializationContext = { errors: [], fatalErrors: [] };
  const data: Record<string, unknown> = {};

  for (const field of fields) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'location': {
        const args = getFieldArguments(field, variables);
        const rawId = args['id'];
        const location =
          typeof rawId === 'string' && rawId.length > 0
            ? findEffectiveLocationById(runtime, rawId)
            : getPrimaryLocation(runtime);
        data[key] = location
          ? serializeLocation(runtime, location, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'locationByIdentifier': {
        const location = resolveLocationIdentifier(runtime, field, variables, context);
        data[key] = location
          ? serializeLocation(runtime, location, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'shop': {
        const shop = runtime.store.getEffectiveShop();
        data[key] = shop ? serializeShop(runtime, shop, field.selectionSet?.selections ?? [], variables) : null;
        break;
      }
      case 'businessEntities':
        data[key] = runtime.store
          .listEffectiveBusinessEntities()
          .map((businessEntity, index) =>
            serializeBusinessEntity(businessEntity, field.selectionSet?.selections ?? [], context, [key, index]),
          );
        break;
      case 'businessEntity': {
        const args = getFieldArguments(field, variables);
        const rawId = args['id'];
        const id = typeof rawId === 'string' && rawId.length > 0 ? rawId : null;
        const businessEntity = id ? runtime.store.getBusinessEntityById(id) : runtime.store.getPrimaryBusinessEntity();
        data[key] = businessEntity
          ? serializeBusinessEntity(businessEntity, field.selectionSet?.selections ?? [], context, [key])
          : null;
        break;
      }
      case 'shopifyPaymentsAccount': {
        const owner = getShopifyPaymentsAccountOwner(runtime);
        data[key] = owner
          ? serializeShopifyPaymentsAccount(
              owner.businessEntity,
              owner.account,
              field.selectionSet?.selections ?? [],
              context,
              [key],
            )
          : null;
        break;
      }
      case 'carrierService': {
        const args = getFieldArguments(field, variables);
        const rawId = args['id'];
        const id = typeof rawId === 'string' && rawId.length > 0 ? rawId : null;
        const service = id ? runtime.store.getEffectiveCarrierServiceById(id) : null;
        data[key] = service ? serializeCarrierService(service, field.selectionSet?.selections ?? []) : null;
        break;
      }
      case 'carrierServices':
        data[key] = serializeCarrierServicesConnection(runtime, field, variables);
        break;
      case 'availableCarrierServices':
        data[key] = serializeAvailableCarrierServices(runtime, field, variables);
        break;
      case 'locationsAvailableForDeliveryProfilesConnection':
        data[key] = serializeLocationsAvailableForDeliveryProfilesConnection(runtime, field, variables);
        break;
      case 'fulfillmentService': {
        const args = getFieldArguments(field, variables);
        const rawId = args['id'];
        const id = typeof rawId === 'string' && rawId.length > 0 ? rawId : null;
        const service = id ? runtime.store.getEffectiveFulfillmentServiceById(id) : null;
        data[key] = service
          ? serializeFulfillmentService(runtime, service, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      default:
        data[key] = null;
    }
  }

  if (context.fatalErrors.length > 0) {
    return { errors: context.fatalErrors };
  }

  return context.errors.length > 0 ? { data, errors: context.errors } : { data };
}
