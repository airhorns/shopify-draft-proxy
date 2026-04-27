import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { makeSyntheticGid } from '../state/synthetic-identity.js';
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

interface DeliveryProfileUserError {
  field: string[] | null;
  message: string;
}

interface DeliveryProfileMutationPayload {
  profile: DeliveryProfileRecord | null;
  userErrors: DeliveryProfileUserError[];
}

interface DeliveryProfileRemovePayload {
  job: { id: string; done: boolean } | null;
  userErrors: DeliveryProfileUserError[];
}

export interface DeliveryProfileMutationResult {
  response: Record<string, unknown>;
  staged: boolean;
  stagedResourceIds: string[];
  notes: string;
}

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

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function readNullableString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function readBoolean(value: unknown): boolean | null {
  return typeof value === 'boolean' ? value : null;
}

function readRecordArray(value: unknown): Array<Record<string, unknown>> {
  return Array.isArray(value) ? value.filter((item): item is Record<string, unknown> => isRecord(item)) : [];
}

function readStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

function uniqueStrings(values: string[]): string[] {
  return [...new Set(values)];
}

function blankProfileNameError(): DeliveryProfileUserError {
  return {
    field: ['profile', 'name'],
    message: 'Add a profile name',
  };
}

function normalizeMoneyAmount(value: unknown): string {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return String(value);
  }

  if (typeof value !== 'string') {
    return '0.0';
  }

  const parsed = Number(value);
  return Number.isFinite(parsed) ? String(parsed) : value;
}

const countryNamesByCode: Record<string, string> = {
  CA: 'Canada',
  GB: 'United Kingdom',
  US: 'United States',
};

function makeDeliveryCountry(input: Record<string, unknown>): DeliveryProfileCountryRecord {
  const restOfWorld = readBoolean(input['restOfWorld']) === true;
  const countryCode = restOfWorld ? null : (readString(input['code']) ?? null);
  const name = restOfWorld ? 'Rest of world' : (countryNamesByCode[countryCode ?? ''] ?? countryCode ?? 'Unknown');
  const provinces = readRecordArray(input['provinces']).map((province): DeliveryProfileProvinceRecord => {
    const code = readString(province['code']) ?? '';
    return {
      id: makeSyntheticGid('DeliveryProvince'),
      name: code,
      code,
    };
  });

  return {
    id: makeSyntheticGid('DeliveryCountry'),
    name,
    translatedName: name,
    code: {
      countryCode,
      restOfWorld,
    },
    provinces,
  };
}

function makeRateProvider(
  input: Record<string, unknown>,
  existing?: DeliveryProfileMethodDefinitionRecord,
): DeliveryProfileMethodDefinitionRecord['rateProvider'] {
  const rateDefinition = isRecord(input['rateDefinition']) ? input['rateDefinition'] : null;
  if (!rateDefinition) {
    return structuredClone(
      existing?.rateProvider ?? {
        __typename: 'DeliveryRateDefinition',
        id: makeSyntheticGid('DeliveryRateDefinition'),
        price: {
          amount: '0.0',
          currencyCode: 'USD',
        },
      },
    );
  }

  const price = isRecord(rateDefinition['price']) ? rateDefinition['price'] : {};
  return {
    __typename: 'DeliveryRateDefinition',
    id: readString(rateDefinition['id']) ?? makeSyntheticGid('DeliveryRateDefinition'),
    price: {
      amount: normalizeMoneyAmount(price['amount']),
      currencyCode: readString(price['currencyCode']) ?? 'USD',
    },
  };
}

function conditionOperatorSlug(operator: string): string {
  return operator.toLowerCase().replaceAll('_', '-').replaceAll('-', '_');
}

function makeConditionId(operator: string): string {
  return `${makeSyntheticGid('DeliveryCondition')}?operator=${conditionOperatorSlug(operator)}`;
}

function makeWeightCondition(input: Record<string, unknown>): DeliveryProfileMethodConditionRecord {
  const criteria = isRecord(input['criteria']) ? input['criteria'] : {};
  const operator = readString(input['operator']) ?? 'GREATER_THAN_OR_EQUAL_TO';
  return {
    id: makeConditionId(operator),
    field: 'TOTAL_WEIGHT',
    operator,
    conditionCriteria: {
      __typename: 'Weight',
      unit: readString(criteria['unit']) ?? 'KILOGRAMS',
      value: typeof criteria['value'] === 'number' ? criteria['value'] : Number(criteria['value'] ?? 0),
    },
  };
}

function makePriceCondition(input: Record<string, unknown>): DeliveryProfileMethodConditionRecord {
  const criteria = isRecord(input['criteria']) ? input['criteria'] : {};
  const operator = readString(input['operator']) ?? 'GREATER_THAN_OR_EQUAL_TO';
  return {
    id: makeConditionId(operator),
    field: 'TOTAL_PRICE',
    operator,
    conditionCriteria: {
      __typename: 'MoneyV2',
      amount: normalizeMoneyAmount(criteria['amount']),
      currencyCode: readString(criteria['currencyCode']) ?? 'USD',
    },
  };
}

function applyConditionUpdates(
  conditions: DeliveryProfileMethodConditionRecord[],
  updates: Array<Record<string, unknown>>,
): DeliveryProfileMethodConditionRecord[] {
  const next = conditions.map((condition) => structuredClone(condition));
  for (const update of updates) {
    const id = readString(update['id']);
    if (!id) {
      continue;
    }

    const existing = next.find((condition) => condition.id === id);
    if (!existing) {
      continue;
    }

    const field = readString(update['field']);
    const operator = readString(update['operator']);
    if (field) {
      existing.field = field;
    }
    if (operator) {
      existing.operator = operator;
    }

    if (typeof update['criteria'] === 'number') {
      if (existing.field === 'TOTAL_PRICE') {
        existing.conditionCriteria = {
          __typename: 'MoneyV2',
          amount: normalizeMoneyAmount(update['criteria']),
          currencyCode: readString(update['criteriaUnit']) ?? 'USD',
        };
      } else {
        existing.conditionCriteria = {
          __typename: 'Weight',
          unit: readString(update['criteriaUnit']) ?? 'KILOGRAMS',
          value: update['criteria'],
        };
      }
    }
  }
  return next;
}

function makeMethodDefinition(
  input: Record<string, unknown>,
  existing?: DeliveryProfileMethodDefinitionRecord,
): DeliveryProfileMethodDefinitionRecord {
  const weightConditions = readRecordArray(input['weightConditionsToCreate']).map(makeWeightCondition);
  const priceConditions = readRecordArray(input['priceConditionsToCreate']).map(makePriceCondition);
  const updatedConditions = applyConditionUpdates(
    existing?.methodConditions ?? [],
    readRecordArray(input['conditionsToUpdate']),
  );

  return {
    id: readString(input['id']) ?? existing?.id ?? makeSyntheticGid('DeliveryMethodDefinition'),
    name: readNullableString(input['name']) ?? existing?.name ?? 'Standard',
    active: readBoolean(input['active']) ?? existing?.active ?? true,
    description: readNullableString(input['description']) ?? existing?.description ?? null,
    rateProvider: makeRateProvider(input, existing),
    methodConditions: [...updatedConditions, ...weightConditions, ...priceConditions],
    cursor: existing?.cursor,
  };
}

function makeLocationGroupZone(
  input: Record<string, unknown>,
  existing?: DeliveryProfileLocationGroupZoneRecord,
): DeliveryProfileLocationGroupZoneRecord {
  const updatedMethods = (existing?.methodDefinitions ?? []).map((method) => structuredClone(method));
  for (const methodInput of readRecordArray(input['methodDefinitionsToUpdate'])) {
    const id = readString(methodInput['id']);
    const index = id ? updatedMethods.findIndex((method) => method.id === id) : -1;
    if (index !== -1) {
      updatedMethods[index] = makeMethodDefinition(methodInput, updatedMethods[index]);
    }
  }

  const createdMethods = readRecordArray(input['methodDefinitionsToCreate']).map((methodInput) =>
    makeMethodDefinition(methodInput),
  );
  const countries =
    input['countries'] === undefined
      ? (existing?.zone.countries.map((country) => structuredClone(country)) ?? [])
      : readRecordArray(input['countries']).map(makeDeliveryCountry);

  return {
    zone: {
      id: readString(input['id']) ?? existing?.zone.id ?? makeSyntheticGid('DeliveryZone'),
      name: readNullableString(input['name']) ?? existing?.zone.name ?? 'Shipping zone',
      countries,
    },
    methodDefinitions: [...updatedMethods, ...createdMethods],
    cursor: existing?.cursor,
  };
}

function makeLocationGroup(
  input: Record<string, unknown>,
  existing?: DeliveryProfileLocationGroupRecord,
): DeliveryProfileLocationGroupRecord {
  const explicitLocations = readStringArray(input['locations']);
  const locations =
    explicitLocations.length > 0
      ? explicitLocations
      : uniqueStrings([...(existing?.locationIds ?? []), ...readStringArray(input['locationsToAdd'])]).filter(
          (locationId) => !readStringArray(input['locationsToRemove']).includes(locationId),
        );

  const updatedZones = (existing?.locationGroupZones ?? []).map((zone) => structuredClone(zone));
  for (const zoneInput of readRecordArray(input['zonesToUpdate'])) {
    const id = readString(zoneInput['id']);
    const index = id ? updatedZones.findIndex((zone) => zone.zone.id === id) : -1;
    if (index !== -1) {
      updatedZones[index] = makeLocationGroupZone(zoneInput, updatedZones[index]);
    }
  }

  const createdZones = readRecordArray(input['zonesToCreate']).map((zoneInput) => makeLocationGroupZone(zoneInput));

  return {
    id: readString(input['id']) ?? existing?.id ?? makeSyntheticGid('DeliveryLocationGroup'),
    locationIds: locations,
    locationCursors: existing?.locationCursors,
    countriesInAnyZone: [...updatedZones, ...createdZones].flatMap((zone) =>
      zone.zone.countries.map((country) => ({
        zone: zone.zone.name,
        country: structuredClone(country),
      })),
    ),
    locationGroupZones: [...updatedZones, ...createdZones],
  };
}

function countProfileVariants(profile: DeliveryProfileRecord): number {
  return profile.profileItems.reduce((sum, item) => sum + item.variantIds.length, 0);
}

function recomputeProfileDerivedFields(profile: DeliveryProfileRecord): DeliveryProfileRecord {
  const locationIds = new Set<string>();
  let activeMethodDefinitionsCount = 0;
  let zoneCountryCount = 0;
  let locationsWithoutRatesCount = 0;

  for (const group of profile.profileLocationGroups) {
    for (const locationId of group.locationIds) {
      locationIds.add(locationId);
    }

    const groupMethodCount = group.locationGroupZones.reduce((sum, zone) => sum + zone.methodDefinitions.length, 0);
    if (groupMethodCount === 0) {
      locationsWithoutRatesCount += group.locationIds.length;
    }

    for (const zone of group.locationGroupZones) {
      zoneCountryCount += zone.zone.countries.length;
      activeMethodDefinitionsCount += zone.methodDefinitions.filter((method) => method.active).length;
    }
  }

  return {
    ...profile,
    activeMethodDefinitionsCount,
    locationsWithoutRatesCount,
    originLocationCount: locationIds.size,
    zoneCountryCount,
    productVariantsCount: {
      count: countProfileVariants(profile),
      precision: 'EXACT',
    },
  };
}

function removeVariantsFromProfile(profile: DeliveryProfileRecord, variantIds: string[]): DeliveryProfileRecord {
  if (variantIds.length === 0) {
    return profile;
  }

  return recomputeProfileDerivedFields({
    ...profile,
    profileItems: profile.profileItems
      .map((item) => ({
        ...item,
        variantIds: item.variantIds.filter((variantId) => !variantIds.includes(variantId)),
        variantCursors: item.variantCursors
          ? Object.fromEntries(
              Object.entries(item.variantCursors).filter(([variantId]) => !variantIds.includes(variantId)),
            )
          : undefined,
      }))
      .filter((item) => item.variantIds.length > 0),
  });
}

function associateVariantsWithProfile(profile: DeliveryProfileRecord, variantIds: string[]): DeliveryProfileRecord {
  const next = structuredClone(profile);
  for (const variantId of variantIds) {
    const variant = store.getEffectiveVariantById(variantId);
    if (!variant) {
      continue;
    }

    let item = next.profileItems.find((candidate) => candidate.productId === variant.productId);
    if (!item) {
      item = {
        productId: variant.productId,
        variantIds: [],
        cursor: variant.productId,
        variantCursors: {},
      };
      next.profileItems.push(item);
    }

    if (!item.variantIds.includes(variantId)) {
      item.variantIds.push(variantId);
    }
    item.variantCursors = {
      ...item.variantCursors,
      [variantId]: item.variantCursors?.[variantId] ?? variantId,
    };
  }

  return recomputeProfileDerivedFields(next);
}

function stageVariantReassignment(targetProfileId: string, variantIds: string[]): void {
  if (variantIds.length === 0) {
    return;
  }

  for (const profile of store.listEffectiveDeliveryProfiles()) {
    if (profile.id === targetProfileId) {
      continue;
    }

    const updatedProfile = removeVariantsFromProfile(profile, variantIds);
    if (countProfileVariants(updatedProfile) !== countProfileVariants(profile)) {
      store.stageUpdateDeliveryProfile(updatedProfile);
    }
  }
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
  const profiles = store.listEffectiveDeliveryProfiles().filter((profile) => {
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

function serializeUserError(
  userError: DeliveryProfileUserError,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'field':
        result[key] = userError.field;
        break;
      case 'message':
        result[key] = userError.message;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDeliveryProfileMutationPayload(
  payload: DeliveryProfileMutationPayload,
  typeName: string,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== typeName) {
        continue;
      }
      Object.assign(
        result,
        serializeDeliveryProfileMutationPayload(payload, typeName, selection.selectionSet.selections, variables),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = typeName;
        break;
      case 'profile':
        result[key] = payload.profile
          ? serializeDeliveryProfile(payload.profile, selection.selectionSet?.selections ?? [], variables)
          : null;
        break;
      case 'userErrors':
        result[key] = payload.userErrors.map((userError) =>
          serializeUserError(userError, selection.selectionSet?.selections ?? []),
        );
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeJob(
  job: NonNullable<DeliveryProfileRemovePayload['job']>,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case '__typename':
        result[key] = 'Job';
        break;
      case 'id':
        result[key] = job.id;
        break;
      case 'done':
        result[key] = job.done;
        break;
      case 'query':
        result[key] = null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDeliveryProfileRemovePayload(
  payload: DeliveryProfileRemovePayload,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (
        selection.typeCondition?.name.value &&
        selection.typeCondition.name.value !== 'DeliveryProfileRemovePayload'
      ) {
        continue;
      }
      Object.assign(result, serializeDeliveryProfileRemovePayload(payload, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'DeliveryProfileRemovePayload';
        break;
      case 'job':
        result[key] = payload.job ? serializeJob(payload.job, selection.selectionSet?.selections ?? []) : null;
        break;
      case 'userErrors':
        result[key] = payload.userErrors.map((userError) =>
          serializeUserError(userError, selection.selectionSet?.selections ?? []),
        );
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function readProfileInput(args: Record<string, unknown>): Record<string, unknown> | null {
  return isRecord(args['profile']) ? args['profile'] : null;
}

function makeEmptyProfile(input: Record<string, unknown>): DeliveryProfileRecord {
  const groups = [
    ...readRecordArray(input['profileLocationGroups']),
    ...readRecordArray(input['locationGroupsToCreate']),
  ].map((groupInput) => makeLocationGroup(groupInput));
  const profile = recomputeProfileDerivedFields({
    id: makeSyntheticGid('DeliveryProfile'),
    name: readString(input['name']) ?? '',
    default: false,
    merchantOwned: true,
    version: 1,
    activeMethodDefinitionsCount: 0,
    locationsWithoutRatesCount: 0,
    originLocationCount: 0,
    zoneCountryCount: 0,
    productVariantsCount: { count: 0, precision: 'EXACT' },
    profileItems: [],
    profileLocationGroups: groups,
    unassignedLocationIds: [],
    sellingPlanGroups: [],
  });

  return associateVariantsWithProfile(profile, readStringArray(input['variantsToAssociate']));
}

function applyProfileInput(existing: DeliveryProfileRecord, input: Record<string, unknown>): DeliveryProfileRecord {
  let next = structuredClone(existing);
  const name = readNullableString(input['name']);
  if (name !== null) {
    next.name = name;
  }

  next = associateVariantsWithProfile(next, readStringArray(input['variantsToAssociate']));
  next = removeVariantsFromProfile(next, readStringArray(input['variantsToDissociate']));

  const groupsToDelete = readStringArray(input['locationGroupsToDelete']);
  if (groupsToDelete.length > 0) {
    next.profileLocationGroups = next.profileLocationGroups.filter((group) => !groupsToDelete.includes(group.id));
  }

  const zonesToDelete = readStringArray(input['zonesToDelete']);
  const methodDefinitionsToDelete = readStringArray(input['methodDefinitionsToDelete']);
  const conditionsToDelete = readStringArray(input['conditionsToDelete']);
  if (zonesToDelete.length > 0 || methodDefinitionsToDelete.length > 0 || conditionsToDelete.length > 0) {
    next.profileLocationGroups = next.profileLocationGroups.map((group) => ({
      ...group,
      locationGroupZones: group.locationGroupZones
        .filter((zone) => !zonesToDelete.includes(zone.zone.id))
        .map((zone) => ({
          ...zone,
          methodDefinitions: zone.methodDefinitions
            .filter((method) => !methodDefinitionsToDelete.includes(method.id))
            .map((method) => ({
              ...method,
              methodConditions: method.methodConditions.filter(
                (condition) => !conditionsToDelete.includes(condition.id),
              ),
            })),
        })),
    }));
  }

  for (const groupInput of readRecordArray(input['locationGroupsToUpdate'])) {
    const id = readString(groupInput['id']);
    const index = id ? next.profileLocationGroups.findIndex((group) => group.id === id) : -1;
    if (index !== -1) {
      next.profileLocationGroups[index] = makeLocationGroup(groupInput, next.profileLocationGroups[index]);
    }
  }

  next.profileLocationGroups.push(
    ...readRecordArray(input['locationGroupsToCreate']).map((groupInput) => makeLocationGroup(groupInput)),
  );

  const sellingPlanGroupsToAssociate = readStringArray(input['sellingPlanGroupsToAssociate']);
  const sellingPlanGroupsToDissociate = readStringArray(input['sellingPlanGroupsToDissociate']);
  if (sellingPlanGroupsToAssociate.length > 0 || sellingPlanGroupsToDissociate.length > 0) {
    const current = next.sellingPlanGroups.filter((group) => {
      const id = readString(group['id']);
      return id ? !sellingPlanGroupsToDissociate.includes(id) : true;
    });
    for (const id of sellingPlanGroupsToAssociate) {
      if (!current.some((group) => group['id'] === id)) {
        current.push({ id, name: null });
      }
    }
    next.sellingPlanGroups = current;
  }

  return recomputeProfileDerivedFields({
    ...next,
    version: next.version + 1,
  });
}

function stageDeliveryProfileCreate(args: Record<string, unknown>): DeliveryProfileMutationPayload {
  const input = readProfileInput(args);
  const name = input ? readString(input['name']) : null;
  if (!input || !name) {
    return {
      profile: null,
      userErrors: [blankProfileNameError()],
    };
  }

  const profile = makeEmptyProfile(input);
  stageVariantReassignment(profile.id, readStringArray(input['variantsToAssociate']));
  return {
    profile: store.stageCreateDeliveryProfile(profile),
    userErrors: [],
  };
}

function stageDeliveryProfileUpdate(args: Record<string, unknown>): DeliveryProfileMutationPayload {
  const id = readString(args['id']);
  const input = readProfileInput(args);
  const existing = id ? store.getEffectiveDeliveryProfileById(id) : null;
  if (!existing || !input) {
    return {
      profile: null,
      userErrors: [{ field: null, message: 'Profile could not be updated.' }],
    };
  }

  if (readNullableString(input['name']) === '') {
    return {
      profile: null,
      userErrors: [blankProfileNameError()],
    };
  }

  stageVariantReassignment(existing.id, readStringArray(input['variantsToAssociate']));
  return {
    profile: store.stageUpdateDeliveryProfile(applyProfileInput(existing, input)),
    userErrors: [],
  };
}

function stageDeliveryProfileRemove(args: Record<string, unknown>): DeliveryProfileRemovePayload {
  const id = readString(args['id']);
  const existing = id ? store.getEffectiveDeliveryProfileById(id) : null;
  if (!existing) {
    return {
      job: null,
      userErrors: [{ field: null, message: 'The Delivery Profile cannot be found for the shop.' }],
    };
  }

  if (existing.default) {
    return {
      job: null,
      userErrors: [{ field: null, message: 'Cannot delete the default profile.' }],
    };
  }

  store.stageDeleteDeliveryProfile(existing.id);
  return {
    job: {
      id: makeSyntheticGid('Job'),
      done: false,
    },
    userErrors: [],
  };
}

export function handleDeliveryProfileMutation(
  document: string,
  variables: Record<string, unknown>,
): DeliveryProfileMutationResult | null {
  const data: Record<string, unknown> = {};
  let staged = false;
  const stagedResourceIds: string[] = [];
  const notes: string[] = [];

  for (const field of getRootFields(document)) {
    const key = responseKey(field);
    const args = getFieldArguments(field, variables);
    switch (field.name.value) {
      case 'deliveryProfileCreate': {
        const payload = stageDeliveryProfileCreate(args);
        data[key] = serializeDeliveryProfileMutationPayload(
          payload,
          'DeliveryProfileCreatePayload',
          field.selectionSet?.selections ?? [],
          variables,
        );
        if (payload.profile) {
          staged = true;
          stagedResourceIds.push(payload.profile.id);
          notes.push('created delivery profile');
        }
        break;
      }
      case 'deliveryProfileUpdate': {
        const payload = stageDeliveryProfileUpdate(args);
        data[key] = serializeDeliveryProfileMutationPayload(
          payload,
          'DeliveryProfileUpdatePayload',
          field.selectionSet?.selections ?? [],
          variables,
        );
        if (payload.profile) {
          staged = true;
          stagedResourceIds.push(payload.profile.id);
          notes.push('updated delivery profile');
        }
        break;
      }
      case 'deliveryProfileRemove': {
        const id = readString(args['id']);
        const payload = stageDeliveryProfileRemove(args);
        data[key] = serializeDeliveryProfileRemovePayload(payload, field.selectionSet?.selections ?? []);
        if (payload.job) {
          staged = true;
          stagedResourceIds.push(...[id, payload.job.id].filter((value): value is string => typeof value === 'string'));
          notes.push('removed delivery profile');
        }
        break;
      }
      default:
        return null;
    }
  }

  return {
    response: { data },
    staged,
    stagedResourceIds,
    notes:
      notes.length > 0
        ? `Staged locally in the in-memory delivery profile draft store: ${notes.join(', ')}.`
        : 'Handled locally as a delivery profile validation branch without staging state.',
  };
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
        const profile = id ? store.getEffectiveDeliveryProfileById(id) : null;
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
