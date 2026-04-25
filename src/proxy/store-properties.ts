import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { logger } from '../logger.js';
import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { makeProxySyntheticGid, makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type {
  BusinessEntityAddressRecord,
  BusinessEntityRecord,
  InventoryLevelRecord,
  LocationAddressRecord,
  LocationFulfillmentServiceRecord,
  LocationRecord,
  LocationSuggestedAddressRecord,
  PaymentSettingsRecord,
  ProductVariantRecord,
  ShopifyPaymentsAccountRecord,
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
import { paginateConnectionItems, serializeConnectionPageInfo } from './graphql-helpers.js';
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

function buildSyntheticInventoryLevel(variant: ProductVariantRecord): InventoryLevelRecord | null {
  if (!variant.inventoryItem) {
    return null;
  }

  const availableQuantity = variant.inventoryQuantity ?? 0;
  const availableUpdatedAt = makeSyntheticTimestamp();

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

function getEffectiveInventoryLevels(variant: ProductVariantRecord): InventoryLevelRecord[] {
  const hydratedLevels = variant.inventoryItem?.inventoryLevels;
  if (!hydratedLevels || hydratedLevels.length === 0) {
    const syntheticLevel = buildSyntheticInventoryLevel(variant);
    return syntheticLevel ? [syntheticLevel] : [];
  }

  return structuredClone(hydratedLevels);
}

function listLocationInventoryLevels(): LocationInventoryLevelRecord[] {
  return store
    .listEffectiveProducts()
    .flatMap((product) =>
      store
        .getEffectiveVariantsByProductId(product.id)
        .flatMap((variant) => getEffectiveInventoryLevels(variant).map((level) => ({ variant, level }))),
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

function listEffectiveLocations(): LocationRecord[] {
  const locationsById = new Map<string, LocationRecord>();
  const locations: LocationRecord[] = [];
  const seenLocationIds = new Set<string>();

  for (const location of store.listEffectiveLocations()) {
    locationsById.set(location.id, location);
    seenLocationIds.add(location.id);
    locations.push(location);
  }

  for (const { level } of listLocationInventoryLevels()) {
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

function findEffectiveLocationById(id: string): LocationRecord | null {
  return listEffectiveLocations().find((location) => location.id === id) ?? null;
}

function getPrimaryLocation(): LocationRecord | null {
  return listEffectiveLocations()[0] ?? null;
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
  service: LocationFulfillmentServiceRecord,
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
  location: NonNullable<InventoryLevelRecord['location']>,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const effectiveLocation = findEffectiveLocationById(location.id);

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'Location') {
        continue;
      }
      Object.assign(result, serializeInventoryLevelLocation(location, selection.selectionSet.selections));
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
      Object.assign(result, serializeInventoryLevel(entry, selection.selectionSet.selections, variables));
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
          ? serializeInventoryLevelLocation(entry.level.location, selection.selectionSet?.selections ?? [])
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

function listInventoryLevelsForLocation(locationId: string): LocationInventoryLevelRecord[] {
  return listLocationInventoryLevels().filter((entry) => entry.level.location?.id === locationId);
}

function serializeLocationInventoryLevelsConnection(
  location: LocationRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const allLevels = listInventoryLevelsForLocation(location.id);
  if (args['reverse'] === true) {
    allLevels.reverse();
  }
  const {
    items: levels,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allLevels, field, variables, getInventoryLevelCursor);
  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = levels.map((level) =>
          serializeInventoryLevel(level, selection.selectionSet?.selections ?? [], variables),
        );
        break;
      case 'edges':
        result[key] = levels.map((level) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of selection.selectionSet?.selections ?? []) {
            if (edgeSelection.kind !== Kind.FIELD) {
              continue;
            }

            const edgeKey = responseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = getInventoryLevelCursor(level);
                break;
              case 'node':
                edgeResult[edgeKey] = serializeInventoryLevel(
                  level,
                  edgeSelection.selectionSet?.selections ?? [],
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
          levels,
          hasNextPage,
          hasPreviousPage,
          getInventoryLevelCursor,
          {
            prefixCursors: false,
          },
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function findInventoryLevelForLocation(
  locationId: string,
  inventoryItemId: string,
): LocationInventoryLevelRecord | null {
  return (
    listInventoryLevelsForLocation(locationId).find((entry) => entry.variant.inventoryItem?.id === inventoryItemId) ??
    null
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

function serializeLocation(
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
      Object.assign(result, serializeLocation(location, selection.selectionSet.selections, variables));
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
          ? serializeLocationFulfillmentService(location.fulfillmentService, selection.selectionSet?.selections ?? [])
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
        const level = inventoryItemId ? findInventoryLevelForLocation(location.id, inventoryItemId) : null;
        result[key] = level
          ? serializeInventoryLevel(level, selection.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'inventoryLevels':
        result[key] = serializeLocationInventoryLevelsConnection(location, selection, variables);
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

function serializeShop(shop: ShopRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'Shop') {
        continue;
      }
      Object.assign(result, serializeShop(shop, selection.selectionSet.selections));
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
      case 'paymentSettings':
        result[key] = serializePaymentSettings(shop.paymentSettings, selection.selectionSet?.selections ?? []);
        break;
      case 'shopPolicies':
        result[key] = shop.shopPolicies.map((policy) =>
          serializeShopPolicy(policy, selection.selectionSet?.selections ?? []),
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function unsupportedShopifyPaymentsFieldError(
  businessEntity: BusinessEntityRecord,
  fieldName: string,
): GraphQLResponseError {
  return {
    message: `Field ShopifyPaymentsAccount.${fieldName} is not exposed by the local snapshot because it can contain account-specific payment data. Capture and model it explicitly before relying on it.`,
    path: ['businessEntity', 'shopifyPaymentsAccount', fieldName],
    extensions: {
      code: 'UNSUPPORTED_FIELD',
      reason: 'shopify-payments-account-sensitive-field',
    },
  };
}

function serializeShopifyPaymentsAccount(
  businessEntity: BusinessEntityRecord,
  account: ShopifyPaymentsAccountRecord,
  selections: readonly SelectionNode[],
  context: SerializationContext,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ShopifyPaymentsAccount') {
        continue;
      }
      Object.assign(
        result,
        serializeShopifyPaymentsAccount(businessEntity, account, selection.selectionSet.selections, context),
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
      default: {
        if (!SAFE_SHOPIFY_PAYMENTS_ACCOUNT_FIELDS.has(selection.name.value)) {
          storePropertiesLogger.warn(
            {
              businessEntityId: businessEntity.id,
              fieldName: selection.name.value,
            },
            'unsupported Shopify Payments account field requested from snapshot business entity',
          );
          context.errors.push(unsupportedShopifyPaymentsFieldError(businessEntity, selection.name.value));
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
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'BusinessEntity') {
        continue;
      }
      Object.assign(result, serializeBusinessEntity(businessEntity, selection.selectionSet.selections, context));
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
            )
          : null;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
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

function stageLocationAdd(input: Record<string, unknown>): {
  location: LocationRecord | null;
  userErrors: LocationUserErrorRecord[];
} {
  const userErrors = validateLocationInput(input, { requireCountry: true });
  const name = readOptionalInputString(input, 'name');
  const address = buildLocationAddressRecord(readLocationAddressInput(input));
  if (userErrors.length > 0 || !name || !address) {
    return { location: null, userErrors };
  }

  const now = makeSyntheticTimestamp();
  const location: LocationRecord = {
    id: makeProxySyntheticGid('Location'),
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
    location.metafields = upsertOwnerMetafields('locationId', location.id, metafieldInputs, [], {
      allowIdLookup: true,
      ownerType: 'LOCATION',
      trimIdentity: true,
    }).metafields;
  }

  return { location: store.stageCreateLocation(location), userErrors: [] };
}

function stageLocationEdit(
  id: string | null,
  input: Record<string, unknown>,
): { location: LocationRecord | null; userErrors: LocationUserErrorRecord[] } {
  if (!id) {
    return { location: null, userErrors: [{ field: ['id'], message: 'Location not found.' }] };
  }

  const existing = store.getEffectiveLocationById(id) ?? findEffectiveLocationById(id);
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
    updatedAt: makeSyntheticTimestamp(),
    address: addressInput ? buildLocationAddressRecord(addressInput, existing.address ?? null) : existing.address,
  };

  const metafieldInputs = readMetafieldInputObjects(input['metafields']);
  if (metafieldInputs.length > 0) {
    nextLocation.metafields = upsertOwnerMetafields(
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

  return { location: store.stageUpdateLocation(nextLocation), userErrors: [] };
}

function serializeLocationMutationPayload(
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
          ? serializeLocation(payload.location, selection.selectionSet?.selections ?? [], variables)
          : null;
        break;
      case 'userErrors':
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

function stageShopPolicyUpdate(input: Record<string, unknown>): {
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

  const shop = store.getEffectiveShop();
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
  const now = makeSyntheticTimestamp();
  const id = existingPolicy?.id ?? makeSyntheticGid('ShopPolicy');
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

  store.stageShop(updatedShop);

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
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const fields = getRootFields(document);
  const data: Record<string, unknown> = {};

  for (const field of fields) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'locationAdd': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeLocationMutationPayload(
          stageLocationAdd(readLocationInput(args)),
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
          stageLocationEdit(id, readLocationInput(args)),
          'LocationEditPayload',
          'LocationEditUserError',
          field.selectionSet?.selections ?? [],
          variables,
        );
        break;
      }
      case 'shopPolicyUpdate': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeShopPolicyUpdatePayload(
          stageShopPolicyUpdate(readShopPolicyInput(args)),
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

function resolveLocationIdentifier(
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
    return findEffectiveLocationById(identifierRecord['id']);
  }

  return null;
}

export function handleStorePropertiesQuery(
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
          typeof rawId === 'string' && rawId.length > 0 ? findEffectiveLocationById(rawId) : getPrimaryLocation();
        data[key] = location ? serializeLocation(location, field.selectionSet?.selections ?? [], variables) : null;
        break;
      }
      case 'locationByIdentifier': {
        const location = resolveLocationIdentifier(field, variables, context);
        data[key] = location ? serializeLocation(location, field.selectionSet?.selections ?? [], variables) : null;
        break;
      }
      case 'shop': {
        const shop = store.getEffectiveShop();
        data[key] = shop ? serializeShop(shop, field.selectionSet?.selections ?? []) : null;
        break;
      }
      case 'businessEntities':
        data[key] = store
          .listEffectiveBusinessEntities()
          .map((businessEntity) =>
            serializeBusinessEntity(businessEntity, field.selectionSet?.selections ?? [], context),
          );
        break;
      case 'businessEntity': {
        const args = getFieldArguments(field, variables);
        const rawId = args['id'];
        const id = typeof rawId === 'string' && rawId.length > 0 ? rawId : null;
        const businessEntity = id ? store.getBusinessEntityById(id) : store.getPrimaryBusinessEntity();
        data[key] = businessEntity
          ? serializeBusinessEntity(businessEntity, field.selectionSet?.selections ?? [], context)
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
