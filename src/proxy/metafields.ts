import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import type { JsonValue } from '../json-schemas.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import {
  getFieldResponseKey,
  getSelectedChildFields,
  paginateConnectionItems,
  serializeConnectionPageInfo,
} from './graphql-helpers.js';

export type MetafieldRecordCore = {
  id: string;
  namespace: string;
  key: string;
  type: string | null;
  value: string | null;
  compareDigest?: string | null | undefined;
  jsonValue?: JsonValue | undefined;
  createdAt?: string | null | undefined;
  updatedAt?: string | null | undefined;
  ownerType?: string | null | undefined;
};

type OwnerScopedMetafieldRecord<OwnerKey extends string> = MetafieldRecordCore & Record<OwnerKey, string>;
type OwnerMetafieldOptions = {
  allowIdLookup?: boolean;
  trimIdentity?: boolean;
  ownerType?: string;
};

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function readOptionalString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function hasOwnField(value: Record<string, unknown>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function parseMetafieldJsonValue(type: string | null, value: string | null): JsonValue {
  if (value === null) {
    return null;
  }

  if (type === 'json' || type?.startsWith('list.')) {
    try {
      return JSON.parse(value) as JsonValue;
    } catch {
      return value;
    }
  }

  if (type === 'number_integer') {
    const parsed = Number.parseInt(value, 10);
    return Number.isFinite(parsed) ? parsed : value;
  }

  if (type === 'number_decimal') {
    const parsed = Number.parseFloat(value);
    return Number.isFinite(parsed) ? parsed : value;
  }

  if (type === 'boolean') {
    return value === 'true';
  }

  return value;
}

function makeMetafieldCompareDigest(metafield: MetafieldRecordCore): string {
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

function buildOwnerScopedMetafield<OwnerKey extends string>(
  ownerKey: OwnerKey,
  ownerId: string,
  metafield: MetafieldRecordCore,
): OwnerScopedMetafieldRecord<OwnerKey> {
  return {
    ...metafield,
    [ownerKey]: ownerId,
  } as OwnerScopedMetafieldRecord<OwnerKey>;
}

export function readMetafieldInputObjects(raw: unknown): Record<string, unknown>[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw.filter((value): value is Record<string, unknown> => isObject(value));
}

export function normalizeOwnerMetafield<OwnerKey extends string>(
  ownerKey: OwnerKey,
  ownerId: string,
  raw: unknown,
  options: Pick<OwnerMetafieldOptions, 'ownerType'> = {},
): OwnerScopedMetafieldRecord<OwnerKey> | null {
  if (!isObject(raw)) {
    return null;
  }

  const id = readOptionalString(raw['id']);
  const namespace = readOptionalString(raw['namespace']);
  const key = readOptionalString(raw['key']);
  if (!id || !namespace || !key) {
    return null;
  }

  const type = readOptionalString(raw['type']);
  const value = readOptionalString(raw['value']);
  const metafield: MetafieldRecordCore = {
    id,
    namespace,
    key,
    type,
    value,
  };

  if (options.ownerType) {
    metafield.compareDigest = readOptionalString(raw['compareDigest']);
    metafield.jsonValue = hasOwnField(raw, 'jsonValue')
      ? (raw['jsonValue'] as JsonValue)
      : parseMetafieldJsonValue(type, value);
    metafield.createdAt = readOptionalString(raw['createdAt']);
    metafield.updatedAt = readOptionalString(raw['updatedAt']);
    metafield.ownerType = readOptionalString(raw['ownerType']) ?? options.ownerType;
  }

  return buildOwnerScopedMetafield(ownerKey, ownerId, metafield);
}

export function mergeMetafieldRecords<T extends MetafieldRecordCore>(existing: T[], next: T[]): T[] {
  const byIdentity = new Map(existing.map((metafield) => [`${metafield.namespace}:${metafield.key}`, metafield]));
  for (const metafield of next) {
    byIdentity.set(`${metafield.namespace}:${metafield.key}`, metafield);
  }

  return Array.from(byIdentity.values());
}

export function upsertOwnerMetafields<OwnerKey extends string>(
  ownerKey: OwnerKey,
  ownerId: string,
  inputs: Record<string, unknown>[],
  existingMetafields: OwnerScopedMetafieldRecord<OwnerKey>[],
  options: OwnerMetafieldOptions = {},
): {
  metafields: Array<OwnerScopedMetafieldRecord<OwnerKey>>;
  createdOrUpdated: Array<OwnerScopedMetafieldRecord<OwnerKey>>;
} {
  const metafieldsById = new Map(existingMetafields.map((metafield) => [metafield.id, metafield]));
  const metafieldsByIdentity = new Map(
    existingMetafields.map((metafield) => [`${metafield.namespace}:${metafield.key}`, structuredClone(metafield)]),
  );
  const createdOrUpdated: Array<OwnerScopedMetafieldRecord<OwnerKey>> = [];

  for (const input of inputs) {
    const existingById =
      options.allowIdLookup && typeof input['id'] === 'string' ? (metafieldsById.get(input['id']) ?? null) : null;
    const rawNamespace = readOptionalString(input['namespace']);
    const rawKey = readOptionalString(input['key']);
    const rawType = readOptionalString(input['type']);
    const namespace = options.trimIdentity
      ? (rawNamespace?.trim() ?? existingById?.namespace ?? '')
      : (rawNamespace ?? existingById?.namespace ?? '');
    const key = options.trimIdentity
      ? (rawKey?.trim() ?? existingById?.key ?? '')
      : (rawKey ?? existingById?.key ?? '');

    if (!existingById && (!namespace || !key)) {
      continue;
    }

    const identityKey = `${namespace}:${key}`;
    const existing = existingById ?? metafieldsByIdentity.get(identityKey);
    const type = (options.trimIdentity ? rawType?.trim() : rawType) ?? existing?.type ?? null;
    const value = readOptionalString(input['value']) ?? existing?.value ?? null;
    const nextCore: MetafieldRecordCore = {
      id: existing?.id ?? makeSyntheticGid('Metafield'),
      namespace,
      key,
      type,
      value,
    };

    if (options.ownerType) {
      const createdAt = existing?.createdAt ?? makeSyntheticTimestamp();
      const updatedAt = existing
        ? value === existing.value && type === existing.type
          ? (existing.updatedAt ?? createdAt)
          : makeSyntheticTimestamp()
        : createdAt;

      nextCore.jsonValue = parseMetafieldJsonValue(type, value);
      nextCore.createdAt = createdAt;
      nextCore.updatedAt = updatedAt;
      nextCore.ownerType = options.ownerType;
      nextCore.compareDigest = makeMetafieldCompareDigest(nextCore);
    }

    const nextMetafield = buildOwnerScopedMetafield(ownerKey, ownerId, nextCore);

    if (existingById && (existingById.namespace !== namespace || existingById.key !== key)) {
      metafieldsByIdentity.delete(`${existingById.namespace}:${existingById.key}`);
    }
    metafieldsById.set(nextMetafield.id, nextMetafield);
    metafieldsByIdentity.set(identityKey, nextMetafield);
    createdOrUpdated.push(structuredClone(nextMetafield));
  }

  return {
    metafields: Array.from(metafieldsByIdentity.values()).sort(
      (left, right) =>
        left.namespace.localeCompare(right.namespace) ||
        left.key.localeCompare(right.key) ||
        left.id.localeCompare(right.id),
    ),
    createdOrUpdated,
  };
}

export function serializeMetafieldSelectionSet(
  metafield: MetafieldRecordCore,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = metafield.id;
        break;
      case 'namespace':
        result[key] = metafield.namespace;
        break;
      case 'key':
        result[key] = metafield.key;
        break;
      case 'type':
        result[key] = metafield.type;
        break;
      case 'value':
        result[key] = metafield.value;
        break;
      case 'compareDigest':
        result[key] = metafield.compareDigest ?? makeMetafieldCompareDigest(metafield);
        break;
      case 'jsonValue':
        result[key] = metafield.jsonValue ?? parseMetafieldJsonValue(metafield.type, metafield.value);
        break;
      case 'createdAt':
        result[key] = metafield.createdAt ?? null;
        break;
      case 'updatedAt':
        result[key] = metafield.updatedAt ?? metafield.createdAt ?? null;
        break;
      case 'ownerType':
        result[key] = metafield.ownerType ?? null;
        break;
      case 'definition':
        result[key] = null;
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

export function serializeMetafieldSelection(
  metafield: MetafieldRecordCore,
  field: FieldNode,
  options: { includeInlineFragments?: boolean } = {},
): Record<string, unknown> {
  return serializeMetafieldSelectionSet(metafield, getSelectedChildFields(field, options));
}

export function serializeMetafieldsConnection(
  metafields: MetafieldRecordCore[],
  field: FieldNode,
  variables: Record<string, unknown> = {},
  options: { includeInlineFragments?: boolean } = {},
): Record<string, unknown> {
  const {
    items: pageMetafields,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(metafields, field, variables, (metafield) => metafield.id);
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field, options)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = pageMetafields.map((metafield) => serializeMetafieldSelection(metafield, selection, options));
        break;
      case 'edges':
        result[key] = pageMetafields.map((metafield) => {
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection, options)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edge[edgeKey] = `cursor:${metafield.id}`;
                break;
              case 'node':
                edge[edgeKey] = serializeMetafieldSelection(metafield, edgeSelection, options);
                break;
              default:
                edge[edgeKey] = null;
                break;
            }
          }
          return edge;
        });
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(
          selection,
          pageMetafields,
          hasNextPage,
          hasPreviousPage,
          (metafield) => metafield.id,
        );
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}
