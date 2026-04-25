import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { makeSyntheticGid } from '../state/synthetic-identity.js';
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
};

type OwnerScopedMetafieldRecord<OwnerKey extends string> = MetafieldRecordCore & Record<OwnerKey, string>;

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function readOptionalString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
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

  return buildOwnerScopedMetafield(ownerKey, ownerId, {
    id,
    namespace,
    key,
    type: readOptionalString(raw['type']),
    value: readOptionalString(raw['value']),
  });
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
  options: { allowIdLookup?: boolean; trimIdentity?: boolean } = {},
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
    const nextMetafield = buildOwnerScopedMetafield(ownerKey, ownerId, {
      id: existing?.id ?? makeSyntheticGid('Metafield'),
      namespace,
      key,
      type: (options.trimIdentity ? rawType?.trim() : rawType) ?? existing?.type ?? null,
      value: readOptionalString(input['value']) ?? existing?.value ?? null,
    });

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
