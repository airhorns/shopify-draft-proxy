import { type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { compareShopifyResourceIds } from '../shopify/resource-ids.js';
import { store } from '../state/store.js';
import type {
  MetaobjectDefinitionCapabilitiesRecord,
  MetaobjectDefinitionRecord,
  MetaobjectFieldDefinitionRecord,
} from '../state/types.js';
import {
  getDocumentFragments,
  getFieldResponseKey,
  isPlainObject,
  paginateConnectionItems,
  projectGraphqlObject,
  readBooleanValue,
  readGraphqlDataResponsePayload,
  readNumberValue,
  readPlainObjectArray,
  readStringValue,
  serializeConnection,
} from './graphql-helpers.js';

function normalizeCapabilities(rawCapabilities: unknown): MetaobjectDefinitionCapabilitiesRecord {
  const capabilities = isPlainObject(rawCapabilities) ? rawCapabilities : {};
  const result: MetaobjectDefinitionCapabilitiesRecord = {};

  for (const capabilityName of ['publishable', 'translatable', 'renderable', 'onlineStore'] as const) {
    const rawCapability = capabilities[capabilityName];
    if (!isPlainObject(rawCapability)) {
      continue;
    }

    const enabled = readBooleanValue(rawCapability['enabled']);
    if (enabled !== null) {
      result[capabilityName] = { enabled };
    }
  }

  return result;
}

function normalizeFieldDefinition(rawField: Record<string, unknown>): MetaobjectFieldDefinitionRecord | null {
  const key = readStringValue(rawField['key']);
  const rawType = isPlainObject(rawField['type']) ? rawField['type'] : null;
  const typeName = rawType ? readStringValue(rawType['name']) : null;
  if (!key || !typeName) {
    return null;
  }

  return {
    key,
    name: readStringValue(rawField['name']),
    description: readStringValue(rawField['description']),
    required: readBooleanValue(rawField['required']),
    type: {
      name: typeName,
      category: rawType ? readStringValue(rawType['category']) : null,
    },
    validations: readPlainObjectArray(rawField['validations']).flatMap((validation) => {
      const name = readStringValue(validation['name']);
      return name ? [{ name, value: readStringValue(validation['value']) }] : [];
    }),
  };
}

function normalizeStandardTemplate(rawTemplate: unknown): MetaobjectDefinitionRecord['standardTemplate'] {
  if (!isPlainObject(rawTemplate)) {
    return null;
  }

  return {
    type: readStringValue(rawTemplate['type']),
    name: readStringValue(rawTemplate['name']),
  };
}

function normalizeMetaobjectDefinition(rawDefinition: unknown): MetaobjectDefinitionRecord | null {
  if (!isPlainObject(rawDefinition)) {
    return null;
  }

  const id = readStringValue(rawDefinition['id']);
  const type = readStringValue(rawDefinition['type']);
  if (!id || !type) {
    return null;
  }

  const rawAccess = isPlainObject(rawDefinition['access']) ? rawDefinition['access'] : {};
  const access = Object.fromEntries(
    Object.entries(rawAccess).filter((entry): entry is [string, string | null] => {
      const value = entry[1];
      return typeof value === 'string' || value === null;
    }),
  );

  return {
    id,
    type,
    name: readStringValue(rawDefinition['name']),
    description: readStringValue(rawDefinition['description']),
    displayNameKey: readStringValue(rawDefinition['displayNameKey']),
    access,
    capabilities: normalizeCapabilities(rawDefinition['capabilities']),
    fieldDefinitions: readPlainObjectArray(rawDefinition['fieldDefinitions']).flatMap((fieldDefinition) => {
      const normalized = normalizeFieldDefinition(fieldDefinition);
      return normalized ? [normalized] : [];
    }),
    hasThumbnailField: readBooleanValue(rawDefinition['hasThumbnailField']),
    metaobjectsCount: readNumberValue(rawDefinition['metaobjectsCount']),
    standardTemplate: normalizeStandardTemplate(rawDefinition['standardTemplate']),
    createdAt: readStringValue(rawDefinition['createdAt']),
    updatedAt: readStringValue(rawDefinition['updatedAt']),
  };
}

function buildSerializableDefinition(definition: MetaobjectDefinitionRecord): Record<string, unknown> {
  return {
    __typename: 'MetaobjectDefinition',
    id: definition.id,
    type: definition.type,
    name: definition.name,
    description: definition.description,
    displayNameKey: definition.displayNameKey,
    access: definition.access,
    capabilities: definition.capabilities,
    fieldDefinitions: definition.fieldDefinitions.map((fieldDefinition) => ({
      __typename: 'MetaobjectFieldDefinition',
      ...fieldDefinition,
      type: {
        __typename: 'MetafieldDefinitionType',
        ...fieldDefinition.type,
      },
    })),
    hasThumbnailField: definition.hasThumbnailField,
    metaobjectsCount: definition.metaobjectsCount,
    standardTemplate: definition.standardTemplate
      ? {
          __typename: 'StandardMetaobjectDefinitionTemplate',
          ...definition.standardTemplate,
        }
      : null,
    createdAt: definition.createdAt ?? null,
    updatedAt: definition.updatedAt ?? null,
  };
}

function serializeDefinitionSelection(
  definition: MetaobjectDefinitionRecord,
  selections: readonly SelectionNode[],
  document: string,
): Record<string, unknown> {
  return projectGraphqlObject(buildSerializableDefinition(definition), selections, getDocumentFragments(document));
}

function readRootStringArgument(
  field: FieldNode,
  variables: Record<string, unknown>,
  argumentName: string,
): string | null {
  const args = getFieldArguments(field, variables);
  return readStringValue(args[argumentName]);
}

function sortDefinitions(definitions: MetaobjectDefinitionRecord[], reverse: unknown): MetaobjectDefinitionRecord[] {
  const sorted = [...definitions].sort(
    (left, right) => compareShopifyResourceIds(left.id, right.id) || left.type.localeCompare(right.type),
  );
  return reverse === true ? sorted.reverse() : sorted;
}

function serializeMetaobjectDefinitionsConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
  document: string,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const definitions = sortDefinitions(store.listEffectiveMetaobjectDefinitions(), args['reverse']);
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    definitions,
    field,
    variables,
    (definition) => definition.id,
  );

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (definition) => definition.id,
    serializeNode: (definition, nodeField) =>
      serializeDefinitionSelection(definition, nodeField.selectionSet?.selections ?? [], document),
  });
}

export function handleMetaobjectDefinitionQuery(
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const responseKey = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'metaobjectDefinition': {
        const id = readRootStringArgument(field, variables, 'id');
        const definition = id ? store.getEffectiveMetaobjectDefinitionById(id) : null;
        data[responseKey] = definition
          ? serializeDefinitionSelection(definition, field.selectionSet?.selections ?? [], document)
          : null;
        break;
      }
      case 'metaobjectDefinitionByType': {
        const type = readRootStringArgument(field, variables, 'type');
        const definition = type ? store.findEffectiveMetaobjectDefinitionByType(type) : null;
        data[responseKey] = definition
          ? serializeDefinitionSelection(definition, field.selectionSet?.selections ?? [], document)
          : null;
        break;
      }
      case 'metaobjectDefinitions':
        data[responseKey] = serializeMetaobjectDefinitionsConnection(field, variables, document);
        break;
      default:
        data[responseKey] = null;
        break;
    }
  }

  return { data };
}

function collectDefinitionsFromConnection(connection: unknown): MetaobjectDefinitionRecord[] {
  if (!isPlainObject(connection)) {
    return [];
  }

  const byId = new Map<string, MetaobjectDefinitionRecord>();
  for (const rawDefinition of readPlainObjectArray(connection['nodes'])) {
    const definition = normalizeMetaobjectDefinition(rawDefinition);
    if (definition) {
      byId.set(definition.id, definition);
    }
  }

  for (const edge of readPlainObjectArray(connection['edges'])) {
    const definition = normalizeMetaobjectDefinition(edge['node']);
    if (definition) {
      byId.set(definition.id, definition);
    }
  }

  return [...byId.values()];
}

export function hydrateMetaobjectDefinitionsFromUpstreamResponse(
  document: string,
  _variables: Record<string, unknown>,
  upstreamPayload: unknown,
): void {
  const definitions: MetaobjectDefinitionRecord[] = [];

  for (const field of getRootFields(document)) {
    const responseKey = getFieldResponseKey(field);
    const payload = readGraphqlDataResponsePayload(upstreamPayload, responseKey);
    switch (field.name.value) {
      case 'metaobjectDefinition':
      case 'metaobjectDefinitionByType': {
        const definition = normalizeMetaobjectDefinition(payload);
        if (definition) {
          definitions.push(definition);
        }
        break;
      }
      case 'metaobjectDefinitions':
        definitions.push(...collectDefinitionsFromConnection(payload));
        break;
      default:
        break;
    }
  }

  if (definitions.length > 0) {
    store.upsertBaseMetaobjectDefinitions(definitions);
  }
}
