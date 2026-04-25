import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { logger } from '../logger.js';
import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type {
  BusinessEntityAddressRecord,
  BusinessEntityRecord,
  InventoryLevelRecord,
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

const storePropertiesLogger = logger.child({ component: 'proxy.store-properties' });

const SAFE_SHOPIFY_PAYMENTS_ACCOUNT_FIELDS = new Set(['id', 'activated', 'country', 'defaultCurrency', 'onboardable']);
const DEFAULT_INVENTORY_LEVEL_LOCATION_ID = 'gid://shopify/Location/1';

function responseKey(selection: FieldNode): string {
  return selection.alias?.value ?? selection.name.value;
}

interface LocationRecord {
  id: string;
  name: string | null;
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

function listEffectiveLocations(): LocationRecord[] {
  const locations: LocationRecord[] = [];
  const seenLocationIds = new Set<string>();

  for (const { level } of listLocationInventoryLevels()) {
    const locationId = level.location?.id;
    if (!locationId || seenLocationIds.has(locationId)) {
      continue;
    }

    seenLocationIds.add(locationId);
    locations.push({
      id: locationId,
      name: level.location?.name ?? null,
    });
  }

  return locations;
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

function serializeLocationAddress(selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'LocationAddress') {
        continue;
      }
      Object.assign(result, serializeLocationAddress(selection.selectionSet.selections));
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
        result[key] = [];
        break;
      case 'address1':
      case 'address2':
      case 'city':
      case 'country':
      case 'countryCode':
      case 'latitude':
      case 'longitude':
      case 'phone':
      case 'province':
      case 'provinceCode':
      case 'zip':
        result[key] = null;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeEmptyMetafieldsConnection(field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = [];
        break;
      case 'edges':
        result[key] = [];
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(selection, [], false, false, () => '');
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
        result[key] = location.name;
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
        result[key] = readLegacyResourceIdFromGid(location.id);
        break;
      case 'name':
        result[key] = location.name;
        break;
      case 'activatable':
        result[key] = true;
        break;
      case 'addressVerified':
        result[key] = false;
        break;
      case 'createdAt':
      case 'updatedAt':
        result[key] = null;
        break;
      case 'deactivatable':
        result[key] = false;
        break;
      case 'deactivatedAt':
        result[key] = null;
        break;
      case 'deletable':
        result[key] = false;
        break;
      case 'fulfillmentService':
        result[key] = null;
        break;
      case 'fulfillsOnlineOrders':
      case 'hasActiveInventory':
      case 'isActive':
      case 'shipsInventory':
        result[key] = true;
        break;
      case 'hasUnfulfilledOrders':
      case 'isFulfillmentService':
        result[key] = false;
        break;
      case 'address':
        result[key] = serializeLocationAddress(selection.selectionSet?.selections ?? []);
        break;
      case 'suggestedAddresses':
        result[key] = [];
        break;
      case 'metafield':
        result[key] = null;
        break;
      case 'metafields':
        result[key] = serializeEmptyMetafieldsConnection(selection);
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
