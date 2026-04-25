import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { store } from '../state/store.js';
import type {
  DeliveryProfileCountryAndZoneRecord,
  DeliveryProfileCountryCodeRecord,
  DeliveryProfileCountryRecord,
  DeliveryProfileItemRecord,
  DeliveryProfileLocationGroupRecord,
  DeliveryProfileLocationGroupZoneRecord,
  DeliveryProfileMethodConditionRecord,
  DeliveryProfileMethodDefinitionRecord,
  DeliveryProfileProvinceRecord,
  DeliveryProfileRecord,
  DeliveryProfileZoneRecord,
  LocationRecord,
  ProductRecord,
  ProductVariantRecord,
} from '../state/types.js';
import { paginateConnectionItems, serializeConnection } from './graphql-helpers.js';

function responseKey(selection: FieldNode): string {
  return selection.alias?.value ?? selection.name.value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function selectedFields(selections: readonly SelectionNode[]): FieldNode[] {
  return selections.filter((selection): selection is FieldNode => selection.kind === Kind.FIELD);
}

function projectPlainValue(value: unknown, selections: readonly SelectionNode[]): unknown {
  if (Array.isArray(value)) {
    return value.map((item) => projectPlainValue(item, selections));
  }

  if (!isRecord(value)) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeName = value['__typename'];
      if (
        selection.typeCondition?.name.value &&
        typeof typeName === 'string' &&
        selection.typeCondition.name.value !== typeName
      ) {
        continue;
      }
      Object.assign(result, projectPlainValue(value, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    if (selection.name.value === '__typename') {
      result[key] = value['__typename'] ?? null;
      continue;
    }

    const child = value[selection.name.value];
    result[key] = selection.selectionSet
      ? projectPlainValue(child, selection.selectionSet.selections)
      : child === undefined
        ? null
        : structuredClone(child);
  }

  return result;
}

function readBooleanArgument(
  field: FieldNode,
  argumentName: string,
  variables: Record<string, unknown>,
): boolean | null {
  const args = getFieldArguments(field, variables);
  const value = args[argumentName];
  return typeof value === 'boolean' ? value : null;
}

function maybeReverse<T>(items: T[], field: FieldNode, variables: Record<string, unknown>): T[] {
  return readBooleanArgument(field, 'reverse', variables) === true ? [...items].reverse() : items;
}

function deliveryProfileCursor(profile: DeliveryProfileRecord): string {
  return profile.cursor ?? profile.id;
}

function deliveryProfileItemCursor(item: DeliveryProfileItemRecord): string {
  return item.cursor ?? item.productId;
}

function methodDefinitionCursor(methodDefinition: DeliveryProfileMethodDefinitionRecord): string {
  return methodDefinition.cursor ?? methodDefinition.id;
}

function locationGroupZoneCursor(zone: DeliveryProfileLocationGroupZoneRecord): string {
  return zone.cursor ?? zone.zone.id;
}

function variantCursor(item: DeliveryProfileItemRecord, variant: ProductVariantRecord): string {
  return item.variantCursors?.[variant.id] ?? variant.id;
}

function serializeCount(
  count: { count: number; precision?: string | null | undefined } | null,
  selections: readonly SelectionNode[],
): Record<string, unknown> | null {
  if (!count) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'count':
        result[key] = count.count;
        break;
      case 'precision':
        result[key] = count.precision ?? 'EXACT';
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeProduct(productId: string, selections: readonly SelectionNode[]): Record<string, unknown> | null {
  const product = store.getEffectiveProductById(productId);
  const fallback: Pick<ProductRecord, 'id' | 'title'> = { id: productId, title: '' };
  const source = product ?? fallback;
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'Product') {
        continue;
      }
      Object.assign(result, serializeProduct(productId, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Product';
        break;
      case 'id':
        result[key] = source.id;
        break;
      case 'title':
        result[key] = source.title;
        break;
      case 'handle':
        result[key] = product?.handle ?? null;
        break;
      case 'status':
        result[key] = product?.status ?? null;
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializeVariant(
  variant: ProductVariantRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ProductVariant') {
        continue;
      }
      Object.assign(result, serializeVariant(variant, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ProductVariant';
        break;
      case 'id':
        result[key] = variant.id;
        break;
      case 'title':
        result[key] = variant.title;
        break;
      case 'sku':
        result[key] = variant.sku;
        break;
      case 'product':
        result[key] = serializeProduct(variant.productId, selection.selectionSet?.selections ?? []);
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializeLocation(location: LocationRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'Location') {
        continue;
      }
      Object.assign(result, serializeLocation(location, selection.selectionSet.selections));
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
      case 'isActive':
        result[key] = location.isActive ?? true;
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializeCountryCode(
  code: DeliveryProfileCountryCodeRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'countryCode':
        result[key] = code.countryCode;
        break;
      case 'restOfWorld':
        result[key] = code.restOfWorld;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeProvince(
  province: DeliveryProfileProvinceRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'id':
        result[key] = province.id;
        break;
      case 'name':
        result[key] = province.name;
        break;
      case 'code':
        result[key] = province.code;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCountry(
  country: DeliveryProfileCountryRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'id':
        result[key] = country.id;
        break;
      case 'name':
        result[key] = country.name;
        break;
      case 'translatedName':
        result[key] = country.translatedName ?? country.name;
        break;
      case 'code':
        result[key] = serializeCountryCode(country.code, field.selectionSet?.selections ?? []);
        break;
      case 'provinces':
        result[key] = country.provinces.map((province) =>
          serializeProvince(province, field.selectionSet?.selections ?? []),
        );
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCountryAndZone(
  countryAndZone: DeliveryProfileCountryAndZoneRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'zone':
        result[key] = countryAndZone.zone;
        break;
      case 'country':
        result[key] = serializeCountry(countryAndZone.country, field.selectionSet?.selections ?? []);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeZone(zone: DeliveryProfileZoneRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'id':
        result[key] = zone.id;
        break;
      case 'name':
        result[key] = zone.name;
        break;
      case 'countries':
        result[key] = zone.countries.map((country) => serializeCountry(country, field.selectionSet?.selections ?? []));
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeMethodCondition(
  condition: DeliveryProfileMethodConditionRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'id':
        result[key] = condition.id;
        break;
      case 'field':
        result[key] = condition.field;
        break;
      case 'operator':
        result[key] = condition.operator;
        break;
      case 'conditionCriteria':
        result[key] = projectPlainValue(condition.conditionCriteria, field.selectionSet?.selections ?? []);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeMethodDefinition(
  methodDefinition: DeliveryProfileMethodDefinitionRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'id':
        result[key] = methodDefinition.id;
        break;
      case 'name':
        result[key] = methodDefinition.name;
        break;
      case 'active':
        result[key] = methodDefinition.active;
        break;
      case 'description':
        result[key] = methodDefinition.description;
        break;
      case 'rateProvider':
        result[key] = projectPlainValue(methodDefinition.rateProvider, field.selectionSet?.selections ?? []);
        break;
      case 'methodConditions':
        result[key] = methodDefinition.methodConditions.map((condition) =>
          serializeMethodCondition(condition, field.selectionSet?.selections ?? []),
        );
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeMethodDefinitionsConnection(
  methodDefinitions: DeliveryProfileMethodDefinitionRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const items = maybeReverse(methodDefinitions, field, variables);
  const window = paginateConnectionItems(items, field, variables, methodDefinitionCursor, {
    parseCursor: (raw) => raw,
  });

  return serializeConnection(field, {
    ...window,
    getCursorValue: methodDefinitionCursor,
    serializeNode: (methodDefinition, selection) =>
      serializeMethodDefinition(methodDefinition, selection.selectionSet?.selections ?? []),
    pageInfoOptions: { prefixCursors: false },
  });
}

function serializeLocationGroupZonesConnection(
  zones: DeliveryProfileLocationGroupZoneRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const items = maybeReverse(zones, field, variables);
  const window = paginateConnectionItems(items, field, variables, locationGroupZoneCursor, {
    parseCursor: (raw) => raw,
  });

  return serializeConnection(field, {
    ...window,
    getCursorValue: locationGroupZoneCursor,
    serializeNode: (zone, selection) =>
      serializeLocationGroupZone(zone, selection.selectionSet?.selections ?? [], variables),
    pageInfoOptions: { prefixCursors: false },
  });
}

function serializeLocationGroupZone(
  zone: DeliveryProfileLocationGroupZoneRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'zone':
        result[key] = serializeZone(zone.zone, field.selectionSet?.selections ?? []);
        break;
      case 'methodDefinitions':
        result[key] = serializeMethodDefinitionsConnection(zone.methodDefinitions, field, variables);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function locationsForGroup(group: DeliveryProfileLocationGroupRecord): LocationRecord[] {
  return group.locationIds
    .map((locationId) => store.getEffectiveLocationById(locationId))
    .filter((location): location is LocationRecord => location !== null);
}

function serializeLocationsConnection(
  locations: LocationRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
  cursorByLocationId: Record<string, string> = {},
): Record<string, unknown> {
  const items = maybeReverse(locations, field, variables);
  const getCursor = (location: LocationRecord) => cursorByLocationId[location.id] ?? location.id;
  const window = paginateConnectionItems(items, field, variables, getCursor, {
    parseCursor: (raw) => raw,
  });

  return serializeConnection(field, {
    ...window,
    getCursorValue: getCursor,
    serializeNode: (location, selection) => serializeLocation(location, selection.selectionSet?.selections ?? []),
    pageInfoOptions: { prefixCursors: false },
  });
}

function serializeLocationGroup(
  group: DeliveryProfileLocationGroupRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const locations = locationsForGroup(group);
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'id':
        result[key] = group.id;
        break;
      case 'locations':
        result[key] = serializeLocationsConnection(locations, field, variables, group.locationCursors);
        break;
      case 'locationsCount':
        result[key] = serializeCount(
          { count: locations.length, precision: 'EXACT' },
          field.selectionSet?.selections ?? [],
        );
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeProfileLocationGroup(
  group: DeliveryProfileLocationGroupRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'countriesInAnyZone':
        result[key] = group.countriesInAnyZone.map((countryAndZone) =>
          serializeCountryAndZone(countryAndZone, field.selectionSet?.selections ?? []),
        );
        break;
      case 'locationGroup':
        result[key] = serializeLocationGroup(group, field.selectionSet?.selections ?? [], variables);
        break;
      case 'locationGroupZones':
        result[key] = serializeLocationGroupZonesConnection(group.locationGroupZones, field, variables);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function variantsForProfileItem(item: DeliveryProfileItemRecord): ProductVariantRecord[] {
  return item.variantIds
    .map((variantId) => store.getEffectiveVariantById(variantId))
    .filter((variant): variant is ProductVariantRecord => variant !== null);
}

function serializeVariantsConnection(
  item: DeliveryProfileItemRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const items = maybeReverse(variantsForProfileItem(item), field, variables);
  const getCursor = (variant: ProductVariantRecord) => variantCursor(item, variant);
  const window = paginateConnectionItems(items, field, variables, getCursor, {
    parseCursor: (raw) => raw,
  });

  return serializeConnection(field, {
    ...window,
    getCursorValue: getCursor,
    serializeNode: (variant, selection) => serializeVariant(variant, selection.selectionSet?.selections ?? []),
    pageInfoOptions: { prefixCursors: false },
  });
}

function serializeProfileItem(
  item: DeliveryProfileItemRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'product':
        result[key] = serializeProduct(item.productId, field.selectionSet?.selections ?? []);
        break;
      case 'variants':
        result[key] = serializeVariantsConnection(item, field, variables);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeProfileItemsConnection(
  profile: DeliveryProfileRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const items = maybeReverse(profile.profileItems, field, variables);
  const window = paginateConnectionItems(items, field, variables, deliveryProfileItemCursor, {
    parseCursor: (raw) => raw,
  });

  return serializeConnection(field, {
    ...window,
    getCursorValue: deliveryProfileItemCursor,
    serializeNode: (item, selection) => serializeProfileItem(item, selection.selectionSet?.selections ?? [], variables),
    pageInfoOptions: { prefixCursors: false },
  });
}

function serializePlainConnection(
  values: Array<Record<string, unknown>>,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const items = maybeReverse(values, field, variables);
  const cursor = (item: Record<string, unknown>, index: number) =>
    typeof item['cursor'] === 'string' ? item['cursor'] : String(item['id'] ?? index);
  const window = paginateConnectionItems(items, field, variables, cursor, {
    parseCursor: (raw) => raw,
  });

  return serializeConnection(field, {
    ...window,
    getCursorValue: cursor,
    serializeNode: (item, selection) => projectPlainValue(item, selection.selectionSet?.selections ?? []),
    pageInfoOptions: { prefixCursors: false },
  });
}

function unassignedLocations(profile: DeliveryProfileRecord): LocationRecord[] {
  return profile.unassignedLocationIds
    .map((locationId) => store.getEffectiveLocationById(locationId))
    .filter((location): location is LocationRecord => location !== null);
}

function serializeDeliveryProfile(
  profile: DeliveryProfileRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'DeliveryProfile') {
        continue;
      }
      Object.assign(result, serializeDeliveryProfile(profile, selection.selectionSet.selections, variables));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'DeliveryProfile';
        break;
      case 'id':
        result[key] = profile.id;
        break;
      case 'name':
        result[key] = profile.name;
        break;
      case 'default':
        result[key] = profile.default;
        break;
      case 'version':
        result[key] = profile.version;
        break;
      case 'activeMethodDefinitionsCount':
        result[key] = profile.activeMethodDefinitionsCount;
        break;
      case 'locationsWithoutRatesCount':
        result[key] = profile.locationsWithoutRatesCount;
        break;
      case 'originLocationCount':
        result[key] = profile.originLocationCount;
        break;
      case 'zoneCountryCount':
        result[key] = profile.zoneCountryCount;
        break;
      case 'productVariantsCount':
        result[key] = serializeCount(profile.productVariantsCount, selection.selectionSet?.selections ?? []);
        break;
      case 'profileItems':
        result[key] = serializeProfileItemsConnection(profile, selection, variables);
        break;
      case 'profileLocationGroups': {
        const args = getFieldArguments(selection, variables);
        const locationGroupId = typeof args['locationGroupId'] === 'string' ? args['locationGroupId'] : null;
        const groups = locationGroupId
          ? profile.profileLocationGroups.filter((group) => group.id === locationGroupId)
          : profile.profileLocationGroups;
        result[key] = groups.map((group) =>
          serializeProfileLocationGroup(group, selection.selectionSet?.selections ?? [], variables),
        );
        break;
      }
      case 'sellingPlanGroups':
        result[key] = serializePlainConnection(profile.sellingPlanGroups, selection, variables);
        break;
      case 'unassignedLocations':
        result[key] = unassignedLocations(profile).map((location) =>
          serializeLocation(location, selection.selectionSet?.selections ?? []),
        );
        break;
      case 'unassignedLocationsPaginated':
        result[key] = serializeLocationsConnection(
          unassignedLocations(profile),
          selection,
          variables,
          profile.unassignedLocationCursors,
        );
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function profilesForConnection(field: FieldNode, variables: Record<string, unknown>): DeliveryProfileRecord[] {
  const merchantOwnedOnly = readBooleanArgument(field, 'merchantOwnedOnly', variables);
  const profiles = store.listBaseDeliveryProfiles().filter((profile) => {
    if (merchantOwnedOnly === true) {
      return profile.merchantOwned;
    }

    return true;
  });

  return maybeReverse(profiles, field, variables);
}

function serializeDeliveryProfilesConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const profiles = profilesForConnection(field, variables);
  const window = paginateConnectionItems(profiles, field, variables, deliveryProfileCursor, {
    parseCursor: (raw) => raw,
  });

  return serializeConnection(field, {
    ...window,
    getCursorValue: deliveryProfileCursor,
    serializeNode: (profile, selection) =>
      serializeDeliveryProfile(profile, selection.selectionSet?.selections ?? [], variables),
    pageInfoOptions: { prefixCursors: false },
  });
}

export function handleDeliveryProfileQuery(
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'deliveryProfile': {
        const args = getFieldArguments(field, variables);
        const id = typeof args['id'] === 'string' ? args['id'] : null;
        const profile = id ? store.getBaseDeliveryProfileById(id) : null;
        data[key] = profile ? serializeDeliveryProfile(profile, field.selectionSet?.selections ?? [], variables) : null;
        break;
      }
      case 'deliveryProfiles':
        data[key] = serializeDeliveryProfilesConnection(field, variables);
        break;
      default:
        data[key] = null;
        break;
    }
  }

  return { data };
}
