import type { ProxyRuntimeContext } from './runtime-context.js';
import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
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

function makeDeliveryCountry(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
): DeliveryProfileCountryRecord {
  const restOfWorld = readBoolean(input['restOfWorld']) === true;
  const countryCode = restOfWorld ? null : (readString(input['code']) ?? null);
  const name = restOfWorld ? 'Rest of world' : (countryNamesByCode[countryCode ?? ''] ?? countryCode ?? 'Unknown');
  const provinces = readRecordArray(input['provinces']).map((province): DeliveryProfileProvinceRecord => {
    const code = readString(province['code']) ?? '';
    return {
      id: runtime.syntheticIdentity.makeSyntheticGid('DeliveryProvince'),
      name: code,
      code,
    };
  });

  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('DeliveryCountry'),
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
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing?: DeliveryProfileMethodDefinitionRecord,
): DeliveryProfileMethodDefinitionRecord['rateProvider'] {
  const rateDefinition = isRecord(input['rateDefinition']) ? input['rateDefinition'] : null;
  if (!rateDefinition) {
    return structuredClone(
      existing?.rateProvider ?? {
        __typename: 'DeliveryRateDefinition',
        id: runtime.syntheticIdentity.makeSyntheticGid('DeliveryRateDefinition'),
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
    id: readString(rateDefinition['id']) ?? runtime.syntheticIdentity.makeSyntheticGid('DeliveryRateDefinition'),
    price: {
      amount: normalizeMoneyAmount(price['amount']),
      currencyCode: readString(price['currencyCode']) ?? 'USD',
    },
  };
}

function conditionOperatorSlug(operator: string): string {
  return operator.toLowerCase().replaceAll('_', '-').replaceAll('-', '_');
}

function makeConditionId(runtime: ProxyRuntimeContext, operator: string): string {
  return `${runtime.syntheticIdentity.makeSyntheticGid('DeliveryCondition')}?operator=${conditionOperatorSlug(operator)}`;
}

function makeWeightCondition(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
): DeliveryProfileMethodConditionRecord {
  const criteria = isRecord(input['criteria']) ? input['criteria'] : {};
  const operator = readString(input['operator']) ?? 'GREATER_THAN_OR_EQUAL_TO';
  return {
    id: makeConditionId(runtime, operator),
    field: 'TOTAL_WEIGHT',
    operator,
    conditionCriteria: {
      __typename: 'Weight',
      unit: readString(criteria['unit']) ?? 'KILOGRAMS',
      value: typeof criteria['value'] === 'number' ? criteria['value'] : Number(criteria['value'] ?? 0),
    },
  };
}

function makePriceCondition(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
): DeliveryProfileMethodConditionRecord {
  const criteria = isRecord(input['criteria']) ? input['criteria'] : {};
  const operator = readString(input['operator']) ?? 'GREATER_THAN_OR_EQUAL_TO';
  return {
    id: makeConditionId(runtime, operator),
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
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing?: DeliveryProfileMethodDefinitionRecord,
): DeliveryProfileMethodDefinitionRecord {
  const weightConditions = readRecordArray(input['weightConditionsToCreate']).map((conditionInput) =>
    makeWeightCondition(runtime, conditionInput),
  );
  const priceConditions = readRecordArray(input['priceConditionsToCreate']).map((conditionInput) =>
    makePriceCondition(runtime, conditionInput),
  );
  const updatedConditions = applyConditionUpdates(
    existing?.methodConditions ?? [],
    readRecordArray(input['conditionsToUpdate']),
  );

  return {
    id:
      readString(input['id']) ?? existing?.id ?? runtime.syntheticIdentity.makeSyntheticGid('DeliveryMethodDefinition'),
    name: readNullableString(input['name']) ?? existing?.name ?? 'Standard',
    active: readBoolean(input['active']) ?? existing?.active ?? true,
    description: readNullableString(input['description']) ?? existing?.description ?? null,
    rateProvider: makeRateProvider(runtime, input, existing),
    methodConditions: [...updatedConditions, ...weightConditions, ...priceConditions],
    cursor: existing?.cursor,
  };
}

function makeLocationGroupZone(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing?: DeliveryProfileLocationGroupZoneRecord,
): DeliveryProfileLocationGroupZoneRecord {
  const updatedMethods = (existing?.methodDefinitions ?? []).map((method) => structuredClone(method));
  for (const methodInput of readRecordArray(input['methodDefinitionsToUpdate'])) {
    const id = readString(methodInput['id']);
    const index = id ? updatedMethods.findIndex((method) => method.id === id) : -1;
    if (index !== -1) {
      updatedMethods[index] = makeMethodDefinition(runtime, methodInput, updatedMethods[index]);
    }
  }

  const createdMethods = readRecordArray(input['methodDefinitionsToCreate']).map((methodInput) =>
    makeMethodDefinition(runtime, methodInput),
  );
  const countries =
    input['countries'] === undefined
      ? (existing?.zone.countries.map((country) => structuredClone(country)) ?? [])
      : readRecordArray(input['countries']).map((countryInput) => makeDeliveryCountry(runtime, countryInput));

  return {
    zone: {
      id: readString(input['id']) ?? existing?.zone.id ?? runtime.syntheticIdentity.makeSyntheticGid('DeliveryZone'),
      name: readNullableString(input['name']) ?? existing?.zone.name ?? 'Shipping zone',
      countries,
    },
    methodDefinitions: [...updatedMethods, ...createdMethods],
    cursor: existing?.cursor,
  };
}

function makeLocationGroup(
  runtime: ProxyRuntimeContext,
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
      updatedZones[index] = makeLocationGroupZone(runtime, zoneInput, updatedZones[index]);
    }
  }

  const createdZones = readRecordArray(input['zonesToCreate']).map((zoneInput) =>
    makeLocationGroupZone(runtime, zoneInput),
  );

  return {
    id: readString(input['id']) ?? existing?.id ?? runtime.syntheticIdentity.makeSyntheticGid('DeliveryLocationGroup'),
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

function associateVariantsWithProfile(
  runtime: ProxyRuntimeContext,
  profile: DeliveryProfileRecord,
  variantIds: string[],
): DeliveryProfileRecord {
  const next = structuredClone(profile);
  for (const variantId of variantIds) {
    const variant = runtime.store.getEffectiveVariantById(variantId);
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

function stageVariantReassignment(runtime: ProxyRuntimeContext, targetProfileId: string, variantIds: string[]): void {
  if (variantIds.length === 0) {
    return;
  }

  for (const profile of runtime.store.listEffectiveDeliveryProfiles()) {
    if (profile.id === targetProfileId) {
      continue;
    }

    const updatedProfile = removeVariantsFromProfile(profile, variantIds);
    if (countProfileVariants(updatedProfile) !== countProfileVariants(profile)) {
      runtime.store.stageUpdateDeliveryProfile(updatedProfile);
    }
  }
}

function serializeProduct(
  runtime: ProxyRuntimeContext,
  productId: string,
  selections: readonly SelectionNode[],
): Record<string, unknown> | null {
  const product = runtime.store.getEffectiveProductById(productId);
  const fallback: Pick<ProductRecord, 'id' | 'title'> = { id: productId, title: '' };
  const source = product ?? fallback;
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'Product') {
        continue;
      }
      Object.assign(result, serializeProduct(runtime, productId, selection.selectionSet.selections));
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
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'ProductVariant') {
        continue;
      }
      Object.assign(result, serializeVariant(runtime, variant, selection.selectionSet.selections));
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
        result[key] = serializeProduct(runtime, variant.productId, selection.selectionSet?.selections ?? []);
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
      case '__typename':
        result[key] = 'DeliveryProvince';
        break;
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
      case '__typename':
        result[key] = 'DeliveryCountry';
        break;
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
      case '__typename':
        result[key] = 'DeliveryZone';
        break;
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
      case '__typename':
        result[key] = 'DeliveryCondition';
        break;
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
      case '__typename':
        result[key] = 'DeliveryMethodDefinition';
        break;
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

function locationsForGroup(runtime: ProxyRuntimeContext, group: DeliveryProfileLocationGroupRecord): LocationRecord[] {
  return group.locationIds
    .map((locationId) => runtime.store.getEffectiveLocationById(locationId))
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
  runtime: ProxyRuntimeContext,
  group: DeliveryProfileLocationGroupRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const locations = locationsForGroup(runtime, group);
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case '__typename':
        result[key] = 'DeliveryLocationGroup';
        break;
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

type DeliveryProfileNodeType =
  | 'DeliveryCondition'
  | 'DeliveryCountry'
  | 'DeliveryLocationGroup'
  | 'DeliveryMethodDefinition'
  | 'DeliveryParticipant'
  | 'DeliveryProvince'
  | 'DeliveryRateDefinition'
  | 'DeliveryZone';

function findDeliveryProfileCountryById(
  profiles: DeliveryProfileRecord[],
  id: string,
): DeliveryProfileCountryRecord | null {
  for (const profile of profiles) {
    for (const group of profile.profileLocationGroups) {
      for (const countryAndZone of group.countriesInAnyZone) {
        if (countryAndZone.country.id === id) {
          return countryAndZone.country;
        }
      }

      for (const zone of group.locationGroupZones) {
        const country = zone.zone.countries.find((candidate) => candidate.id === id);
        if (country) {
          return country;
        }
      }
    }
  }

  return null;
}

function findDeliveryProfileProvinceById(
  profiles: DeliveryProfileRecord[],
  id: string,
): DeliveryProfileProvinceRecord | null {
  for (const profile of profiles) {
    for (const group of profile.profileLocationGroups) {
      for (const countryAndZone of group.countriesInAnyZone) {
        const province = countryAndZone.country.provinces.find((candidate) => candidate.id === id);
        if (province) {
          return province;
        }
      }

      for (const zone of group.locationGroupZones) {
        for (const country of zone.zone.countries) {
          const province = country.provinces.find((candidate) => candidate.id === id);
          if (province) {
            return province;
          }
        }
      }
    }
  }

  return null;
}

function findDeliveryProfileMethodDefinitionById(
  profiles: DeliveryProfileRecord[],
  id: string,
): DeliveryProfileMethodDefinitionRecord | null {
  for (const profile of profiles) {
    for (const group of profile.profileLocationGroups) {
      for (const zone of group.locationGroupZones) {
        const method = zone.methodDefinitions.find((candidate) => candidate.id === id);
        if (method) {
          return method;
        }
      }
    }
  }

  return null;
}

function findDeliveryProfileMethodConditionById(
  profiles: DeliveryProfileRecord[],
  id: string,
): DeliveryProfileMethodConditionRecord | null {
  for (const profile of profiles) {
    for (const group of profile.profileLocationGroups) {
      for (const zone of group.locationGroupZones) {
        for (const method of zone.methodDefinitions) {
          const condition = method.methodConditions.find((candidate) => candidate.id === id);
          if (condition) {
            return condition;
          }
        }
      }
    }
  }

  return null;
}

function findDeliveryProfileRateProviderById(
  profiles: DeliveryProfileRecord[],
  id: string,
  typeName: 'DeliveryParticipant' | 'DeliveryRateDefinition',
): Record<string, unknown> | null {
  for (const profile of profiles) {
    for (const group of profile.profileLocationGroups) {
      for (const zone of group.locationGroupZones) {
        for (const method of zone.methodDefinitions) {
          const rateProvider = method.rateProvider;
          if (rateProvider['id'] === id && rateProvider['__typename'] === typeName) {
            return rateProvider;
          }
        }
      }
    }
  }

  return null;
}

export function serializeDeliveryProfileNestedNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  typeName: DeliveryProfileNodeType,
  selectedFields: readonly FieldNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  const profiles = runtime.store.listEffectiveDeliveryProfiles();

  switch (typeName) {
    case 'DeliveryCondition': {
      const condition = findDeliveryProfileMethodConditionById(profiles, id);
      return condition ? serializeMethodCondition(condition, selectedFields) : null;
    }
    case 'DeliveryCountry': {
      const country = findDeliveryProfileCountryById(profiles, id);
      return country ? serializeCountry(country, selectedFields) : null;
    }
    case 'DeliveryLocationGroup': {
      for (const profile of profiles) {
        const group = profile.profileLocationGroups.find((candidate) => candidate.id === id);
        if (group) {
          return serializeLocationGroup(runtime, group, selectedFields, variables);
        }
      }
      return null;
    }
    case 'DeliveryMethodDefinition': {
      const method = findDeliveryProfileMethodDefinitionById(profiles, id);
      return method ? serializeMethodDefinition(method, selectedFields) : null;
    }
    case 'DeliveryParticipant':
    case 'DeliveryRateDefinition': {
      const rateProvider = findDeliveryProfileRateProviderById(profiles, id, typeName);
      return rateProvider ? (projectPlainValue(rateProvider, selectedFields) as Record<string, unknown>) : null;
    }
    case 'DeliveryProvince': {
      const province = findDeliveryProfileProvinceById(profiles, id);
      return province ? serializeProvince(province, selectedFields) : null;
    }
    case 'DeliveryZone': {
      for (const profile of profiles) {
        for (const group of profile.profileLocationGroups) {
          const zone = group.locationGroupZones.find((candidate) => candidate.zone.id === id);
          if (zone) {
            return serializeZone(zone.zone, selectedFields);
          }
        }
      }
      return null;
    }
  }
}

function serializeProfileLocationGroup(
  runtime: ProxyRuntimeContext,
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
        result[key] = serializeLocationGroup(runtime, group, field.selectionSet?.selections ?? [], variables);
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

function variantsForProfileItem(runtime: ProxyRuntimeContext, item: DeliveryProfileItemRecord): ProductVariantRecord[] {
  return item.variantIds
    .map((variantId) => runtime.store.getEffectiveVariantById(variantId))
    .filter((variant): variant is ProductVariantRecord => variant !== null);
}

function serializeVariantsConnection(
  runtime: ProxyRuntimeContext,
  item: DeliveryProfileItemRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const items = maybeReverse(variantsForProfileItem(runtime, item), field, variables);
  const getCursor = (variant: ProductVariantRecord) => variantCursor(item, variant);
  const window = paginateConnectionItems(items, field, variables, getCursor, {
    parseCursor: (raw) => raw,
  });

  return serializeConnection(field, {
    ...window,
    getCursorValue: getCursor,
    serializeNode: (variant, selection) => serializeVariant(runtime, variant, selection.selectionSet?.selections ?? []),
    pageInfoOptions: { prefixCursors: false },
  });
}

function serializeProfileItem(
  runtime: ProxyRuntimeContext,
  item: DeliveryProfileItemRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'product':
        result[key] = serializeProduct(runtime, item.productId, field.selectionSet?.selections ?? []);
        break;
      case 'variants':
        result[key] = serializeVariantsConnection(runtime, item, field, variables);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeProfileItemsConnection(
  runtime: ProxyRuntimeContext,
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
    serializeNode: (item, selection) =>
      serializeProfileItem(runtime, item, selection.selectionSet?.selections ?? [], variables),
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

function unassignedLocations(runtime: ProxyRuntimeContext, profile: DeliveryProfileRecord): LocationRecord[] {
  return profile.unassignedLocationIds
    .map((locationId) => runtime.store.getEffectiveLocationById(locationId))
    .filter((location): location is LocationRecord => location !== null);
}

function serializeDeliveryProfile(
  runtime: ProxyRuntimeContext,
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
      Object.assign(result, serializeDeliveryProfile(runtime, profile, selection.selectionSet.selections, variables));
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
        result[key] = serializeProfileItemsConnection(runtime, profile, selection, variables);
        break;
      case 'profileLocationGroups': {
        const args = getFieldArguments(selection, variables);
        const locationGroupId = typeof args['locationGroupId'] === 'string' ? args['locationGroupId'] : null;
        const groups = locationGroupId
          ? profile.profileLocationGroups.filter((group) => group.id === locationGroupId)
          : profile.profileLocationGroups;
        result[key] = groups.map((group) =>
          serializeProfileLocationGroup(runtime, group, selection.selectionSet?.selections ?? [], variables),
        );
        break;
      }
      case 'sellingPlanGroups':
        result[key] = serializePlainConnection(profile.sellingPlanGroups, selection, variables);
        break;
      case 'unassignedLocations':
        result[key] = unassignedLocations(runtime, profile).map((location) =>
          serializeLocation(location, selection.selectionSet?.selections ?? []),
        );
        break;
      case 'unassignedLocationsPaginated':
        result[key] = serializeLocationsConnection(
          unassignedLocations(runtime, profile),
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

function profilesForConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): DeliveryProfileRecord[] {
  const merchantOwnedOnly = readBooleanArgument(field, 'merchantOwnedOnly', variables);
  const profiles = runtime.store.listEffectiveDeliveryProfiles().filter((profile) => {
    if (merchantOwnedOnly === true) {
      return profile.merchantOwned;
    }

    return true;
  });

  return maybeReverse(profiles, field, variables);
}

function serializeDeliveryProfilesConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const profiles = profilesForConnection(runtime, field, variables);
  const window = paginateConnectionItems(profiles, field, variables, deliveryProfileCursor, {
    parseCursor: (raw) => raw,
  });

  return serializeConnection(field, {
    ...window,
    getCursorValue: deliveryProfileCursor,
    serializeNode: (profile, selection) =>
      serializeDeliveryProfile(runtime, profile, selection.selectionSet?.selections ?? [], variables),
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
  runtime: ProxyRuntimeContext,
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
        serializeDeliveryProfileMutationPayload(
          runtime,
          payload,
          typeName,
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
        result[key] = typeName;
        break;
      case 'profile':
        result[key] = payload.profile
          ? serializeDeliveryProfile(runtime, payload.profile, selection.selectionSet?.selections ?? [], variables)
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

function makeEmptyProfile(runtime: ProxyRuntimeContext, input: Record<string, unknown>): DeliveryProfileRecord {
  const groups = [
    ...readRecordArray(input['profileLocationGroups']),
    ...readRecordArray(input['locationGroupsToCreate']),
  ].map((groupInput) => makeLocationGroup(runtime, groupInput));
  const profile = recomputeProfileDerivedFields({
    id: runtime.syntheticIdentity.makeSyntheticGid('DeliveryProfile'),
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

  return associateVariantsWithProfile(runtime, profile, readStringArray(input['variantsToAssociate']));
}

function applyProfileInput(
  runtime: ProxyRuntimeContext,
  existing: DeliveryProfileRecord,
  input: Record<string, unknown>,
): DeliveryProfileRecord {
  let next = structuredClone(existing);
  const name = readNullableString(input['name']);
  if (name !== null) {
    next.name = name;
  }

  next = associateVariantsWithProfile(runtime, next, readStringArray(input['variantsToAssociate']));
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
      next.profileLocationGroups[index] = makeLocationGroup(runtime, groupInput, next.profileLocationGroups[index]);
    }
  }

  next.profileLocationGroups.push(
    ...readRecordArray(input['locationGroupsToCreate']).map((groupInput) => makeLocationGroup(runtime, groupInput)),
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

function stageDeliveryProfileCreate(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): DeliveryProfileMutationPayload {
  const input = readProfileInput(args);
  const name = input ? readString(input['name']) : null;
  if (!input || !name) {
    return {
      profile: null,
      userErrors: [blankProfileNameError()],
    };
  }

  const profile = makeEmptyProfile(runtime, input);
  stageVariantReassignment(runtime, profile.id, readStringArray(input['variantsToAssociate']));
  return {
    profile: runtime.store.stageCreateDeliveryProfile(profile),
    userErrors: [],
  };
}

function stageDeliveryProfileUpdate(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): DeliveryProfileMutationPayload {
  const id = readString(args['id']);
  const input = readProfileInput(args);
  const existing = id ? runtime.store.getEffectiveDeliveryProfileById(id) : null;
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

  stageVariantReassignment(runtime, existing.id, readStringArray(input['variantsToAssociate']));
  return {
    profile: runtime.store.stageUpdateDeliveryProfile(applyProfileInput(runtime, existing, input)),
    userErrors: [],
  };
}

function stageDeliveryProfileRemove(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): DeliveryProfileRemovePayload {
  const id = readString(args['id']);
  const existing = id ? runtime.store.getEffectiveDeliveryProfileById(id) : null;
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

  runtime.store.stageDeleteDeliveryProfile(existing.id);
  return {
    job: {
      id: runtime.syntheticIdentity.makeSyntheticGid('Job'),
      done: false,
    },
    userErrors: [],
  };
}

export function handleDeliveryProfileMutation(
  runtime: ProxyRuntimeContext,
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
        const payload = stageDeliveryProfileCreate(runtime, args);
        data[key] = serializeDeliveryProfileMutationPayload(
          runtime,
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
        const payload = stageDeliveryProfileUpdate(runtime, args);
        data[key] = serializeDeliveryProfileMutationPayload(
          runtime,
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
        const payload = stageDeliveryProfileRemove(runtime, args);
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
  runtime: ProxyRuntimeContext,
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
        const profile = id ? runtime.store.getEffectiveDeliveryProfileById(id) : null;
        data[key] = profile
          ? serializeDeliveryProfile(runtime, profile, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'deliveryProfiles':
        data[key] = serializeDeliveryProfilesConnection(runtime, field, variables);
        break;
      default:
        data[key] = null;
        break;
    }
  }

  return { data };
}
