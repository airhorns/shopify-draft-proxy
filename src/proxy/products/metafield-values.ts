import type { ProductMetafieldRecord } from '../../state/types.js';

export function parseMetafieldJsonValue(
  type: string | null,
  value: string | null,
): ProductMetafieldRecord['jsonValue'] {
  if (value === null) {
    return null;
  }

  if (type === 'date_time') {
    return normalizeDateTimeValue(value);
  }

  if (type === 'number_decimal' || type === 'float') {
    return value;
  }

  if (type && MEASUREMENT_METAFIELD_TYPES.has(type)) {
    return parseMeasurementJsonValue(type, value);
  }

  if (type === 'rating') {
    return parseRatingJsonValue(value);
  }

  if (type?.startsWith('list.')) {
    return parseListMetafieldJsonValue(type.slice('list.'.length), value);
  }

  if (shouldParseMetafieldJsonValue(type)) {
    try {
      return JSON.parse(value) as ProductMetafieldRecord['jsonValue'];
    } catch {
      return value;
    }
  }

  if (type === 'number_integer' || type === 'integer') {
    const parsed = Number.parseInt(value, 10);
    return Number.isFinite(parsed) ? parsed : value;
  }

  if (type === 'boolean') {
    return value === 'true';
  }

  return value;
}

export function normalizeMetafieldValue(type: string | null, value: string | null): string | null {
  if (value === null) {
    return null;
  }

  if (type === 'date_time') {
    return normalizeDateTimeValue(value);
  }

  if (type && MEASUREMENT_METAFIELD_TYPES.has(type)) {
    return normalizeMeasurementValueString(type, value);
  }

  if (type === 'rating') {
    return normalizeRatingValueString(value);
  }

  if (type?.startsWith('list.')) {
    return normalizeListMetafieldValueString(type.slice('list.'.length), value);
  }

  return value;
}

export function isMeasurementMetafieldType(type: string | null): boolean {
  const baseType = type?.startsWith('list.') ? type.slice('list.'.length) : type;
  return !!baseType && MEASUREMENT_METAFIELD_TYPES.has(baseType);
}

const JSON_OBJECT_METAFIELD_TYPES = new Set([
  'antenna_gain',
  'area',
  'battery_charge_capacity',
  'battery_energy_capacity',
  'capacitance',
  'concentration',
  'data_storage_capacity',
  'data_transfer_rate',
  'dimension',
  'display_density',
  'distance',
  'duration',
  'electric_current',
  'electrical_resistance',
  'energy',
  'frequency',
  'illuminance',
  'inductance',
  'json',
  'json_string',
  'link',
  'luminous_flux',
  'mass_flow_rate',
  'money',
  'power',
  'pressure',
  'rating',
  'resolution',
  'rich_text_field',
  'rotational_speed',
  'sound_level',
  'speed',
  'temperature',
  'thermal_power',
  'voltage',
  'volume',
  'volumetric_flow_rate',
  'weight',
]);

const MEASUREMENT_METAFIELD_TYPES = new Set([
  'antenna_gain',
  'area',
  'battery_charge_capacity',
  'battery_energy_capacity',
  'capacitance',
  'concentration',
  'data_storage_capacity',
  'data_transfer_rate',
  'dimension',
  'display_density',
  'distance',
  'duration',
  'electric_current',
  'electrical_resistance',
  'energy',
  'frequency',
  'illuminance',
  'inductance',
  'luminous_flux',
  'mass_flow_rate',
  'power',
  'pressure',
  'resolution',
  'rotational_speed',
  'sound_level',
  'speed',
  'temperature',
  'thermal_power',
  'voltage',
  'volume',
  'volumetric_flow_rate',
  'weight',
]);

const LIST_MEASUREMENT_JSON_UNIT_OVERRIDES = new Map([
  ['dimension', new Map([['centimeters', 'cm']])],
  ['volume', new Map([['milliliters', 'ml']])],
  ['weight', new Map([['kilograms', 'kg']])],
]);

function shouldParseMetafieldJsonValue(type: string | null): boolean {
  return !!type && (type.startsWith('list.') || JSON_OBJECT_METAFIELD_TYPES.has(type));
}

function parseJsonValue(value: string): unknown {
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return value;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readMeasurementValue(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }

  if (typeof value === 'string') {
    const parsed = Number.parseFloat(value);
    return Number.isFinite(parsed) ? parsed : null;
  }

  return null;
}

function formatMeasurementNumber(value: number): string {
  return Number.isInteger(value) ? `${value}.0` : String(value);
}

function normalizeDateTimeValue(value: string): string {
  if (/[zZ]$/u.test(value)) {
    return `${value.slice(0, -1)}+00:00`;
  }

  return /(?:[+-]\d{2}:\d{2})$/u.test(value) ? value : `${value}+00:00`;
}

function normalizeMeasurementUnitForValue(unit: unknown): string | null {
  return typeof unit === 'string' && unit.length > 0 ? unit.toUpperCase() : null;
}

function normalizeMeasurementUnitForListJson(type: string, unit: string): string {
  const normalized = unit.toLowerCase();
  return LIST_MEASUREMENT_JSON_UNIT_OVERRIDES.get(type)?.get(normalized) ?? normalized;
}

function normalizeMeasurementJsonObject(
  type: string,
  raw: unknown,
  options: { listJsonUnit: boolean },
): Record<string, unknown> | null {
  if (!isRecord(raw)) {
    return null;
  }

  const value = readMeasurementValue(raw['value']);
  const unit = typeof raw['unit'] === 'string' ? raw['unit'] : null;
  if (value === null || !unit) {
    return null;
  }

  return {
    value,
    unit: options.listJsonUnit
      ? normalizeMeasurementUnitForListJson(type, unit)
      : normalizeMeasurementUnitForValue(unit),
  };
}

function serializeMeasurementValueObject(raw: unknown): string | null {
  if (!isRecord(raw)) {
    return null;
  }

  const value = readMeasurementValue(raw['value']);
  const unit = normalizeMeasurementUnitForValue(raw['unit']);
  if (value === null || !unit) {
    return null;
  }

  return `{"value":${formatMeasurementNumber(value)},"unit":"${unit}"}`;
}

function parseMeasurementJsonValue(type: string, value: string): ProductMetafieldRecord['jsonValue'] {
  const normalized = normalizeMeasurementJsonObject(type, parseJsonValue(value), { listJsonUnit: false });
  return (normalized ?? parseJsonValue(value)) as ProductMetafieldRecord['jsonValue'];
}

function normalizeMeasurementValueString(type: string, value: string): string {
  return serializeMeasurementValueObject(parseJsonValue(value)) ?? value;
}

function normalizeRatingObject(raw: unknown): Record<string, string> | null {
  if (!isRecord(raw)) {
    return null;
  }

  const scaleMin = typeof raw['scale_min'] === 'string' ? raw['scale_min'] : null;
  const scaleMax = typeof raw['scale_max'] === 'string' ? raw['scale_max'] : null;
  const ratingValue = typeof raw['value'] === 'string' ? raw['value'] : null;
  if (!scaleMin || !scaleMax || !ratingValue) {
    return null;
  }

  return {
    scale_min: scaleMin,
    scale_max: scaleMax,
    value: ratingValue,
  };
}

function parseRatingJsonValue(value: string): ProductMetafieldRecord['jsonValue'] {
  return (normalizeRatingObject(parseJsonValue(value)) ?? parseJsonValue(value)) as ProductMetafieldRecord['jsonValue'];
}

function normalizeRatingValueString(value: string): string {
  const normalized = normalizeRatingObject(parseJsonValue(value));
  return normalized ? JSON.stringify(normalized) : value;
}

function parseListMetafieldJsonValue(type: string, value: string): ProductMetafieldRecord['jsonValue'] {
  const parsed = parseJsonValue(value);
  if (!Array.isArray(parsed)) {
    return parsed as ProductMetafieldRecord['jsonValue'];
  }

  if (type === 'date_time') {
    return parsed.map((item) => (typeof item === 'string' ? normalizeDateTimeValue(item) : item));
  }

  if (type === 'number_decimal' || type === 'float') {
    return parsed.map((item) => (typeof item === 'number' || typeof item === 'string' ? String(item) : item));
  }

  if (MEASUREMENT_METAFIELD_TYPES.has(type)) {
    return parsed.map((item) => normalizeMeasurementJsonObject(type, item, { listJsonUnit: true }) ?? item);
  }

  if (type === 'rating') {
    return parsed.map((item) => normalizeRatingObject(item) ?? item);
  }

  return parsed as ProductMetafieldRecord['jsonValue'];
}

function normalizeListMetafieldValueString(type: string, value: string): string {
  const parsed = parseJsonValue(value);
  if (!Array.isArray(parsed)) {
    return value;
  }

  if (type === 'date_time') {
    return JSON.stringify(parsed.map((item) => (typeof item === 'string' ? normalizeDateTimeValue(item) : item)));
  }

  if (type === 'number_decimal' || type === 'float') {
    return JSON.stringify(
      parsed.map((item) => (typeof item === 'number' || typeof item === 'string' ? String(item) : item)),
    );
  }

  if (MEASUREMENT_METAFIELD_TYPES.has(type)) {
    const serialized = parsed.map(serializeMeasurementValueObject);
    return serialized.every((item): item is string => item !== null) ? `[${serialized.join(',')}]` : value;
  }

  if (type === 'rating') {
    const normalized = parsed.map(normalizeRatingObject);
    return normalized.every((item): item is Record<string, string> => item !== null)
      ? JSON.stringify(normalized)
      : value;
  }

  return value;
}

export function makeMetafieldCompareDigest(metafield: {
  namespace: string;
  key: string;
  type: string | null;
  value: string | null;
  jsonValue?: unknown;
  updatedAt?: string | null | undefined;
}): string {
  return `draft:${Buffer.from(
    JSON.stringify([
      metafield.namespace,
      metafield.key,
      metafield.type,
      metafield.value,
      metafield.jsonValue ?? null,
      metafield.updatedAt ?? null,
    ]),
  ).toString('base64url')}`;
}
