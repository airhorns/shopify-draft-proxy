import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import {
  matchesSearchQueryDate,
  matchesSearchQueryText,
  normalizeSearchQueryValue,
  parseSearchQuery,
  type SearchQueryNode,
  type SearchQueryTerm,
} from '../search-query-parser.js';
import { compareShopifyResourceIds } from '../shopify/resource-ids.js';
import { makeProxySyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type {
  MetaobjectDefinitionCapabilitiesRecord,
  MetaobjectDefinitionRecord,
  MetaobjectFieldRecord,
  MetaobjectFieldDefinitionRecord,
  MetaobjectRecord,
  MetaobjectFieldDefinitionValidationRecord,
} from '../state/types.js';
import {
  getDocumentFragments,
  getFieldResponseKey,
  getSelectedChildFields,
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

type MetaobjectUserError = {
  field: string[] | null;
  message: string;
  code: string;
  elementKey?: string | null;
  elementIndex?: number | null;
};

type StandardMetaobjectDefinitionTemplate = {
  type: string;
  name: string;
  displayNameKey: string;
  fieldDefinitions: MetaobjectFieldDefinitionRecord[];
};

const DEFAULT_METAOBJECT_DEFINITION_ACCESS: Record<string, string> = {
  admin: 'PUBLIC_READ_WRITE',
  storefront: 'NONE',
};

const DEFAULT_METAOBJECT_DEFINITION_CAPABILITIES: Required<MetaobjectDefinitionCapabilitiesRecord> = {
  publishable: { enabled: false },
  translatable: { enabled: false },
  renderable: { enabled: false },
  onlineStore: { enabled: false },
};

const STANDARD_METAOBJECT_DEFINITION_TEMPLATES: StandardMetaobjectDefinitionTemplate[] = [
  {
    type: 'shopify--qa-pair',
    name: 'Q&A pair',
    displayNameKey: 'question',
    fieldDefinitions: [
      {
        key: 'question',
        name: 'Question',
        description: null,
        required: true,
        type: { name: 'single_line_text_field', category: 'TEXT' },
        validations: [],
      },
      {
        key: 'answer',
        name: 'Answer',
        description: null,
        required: true,
        type: { name: 'multi_line_text_field', category: 'TEXT' },
        validations: [],
      },
    ],
  },
];

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

function readNullableBooleanValue(value: unknown): boolean | null {
  return typeof value === 'boolean' ? value : null;
}

function hasOwnProperty(value: Record<string, unknown>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function isAppReservedMetaobjectType(type: string): boolean {
  return type.startsWith('$app:');
}

function inferFieldTypeCategory(typeName: string): string | null {
  if (typeName.includes('text') || typeName === 'url' || typeName === 'color') {
    return 'TEXT';
  }

  if (typeName.includes('number') || typeName === 'rating' || typeName === 'volume' || typeName === 'weight') {
    return 'NUMBER';
  }

  if (typeName.includes('reference')) {
    return 'REFERENCE';
  }

  if (typeName === 'boolean') {
    return 'TRUE_FALSE';
  }

  if (typeName === 'date' || typeName === 'date_time') {
    return 'DATE_TIME';
  }

  if (typeName === 'json') {
    return 'JSON';
  }

  return null;
}

function readValidationInput(rawValidation: Record<string, unknown>): MetaobjectFieldDefinitionValidationRecord | null {
  const name = readStringValue(rawValidation['name']);
  if (!name) {
    return null;
  }

  return {
    name,
    value: readStringValue(rawValidation['value']),
  };
}

function readValidationInputs(rawValidations: unknown): MetaobjectFieldDefinitionValidationRecord[] {
  return readPlainObjectArray(rawValidations).flatMap((validation) => {
    const normalized = readValidationInput(validation);
    return normalized ? [normalized] : [];
  });
}

function readFieldDefinitionInput(rawField: unknown): MetaobjectFieldDefinitionRecord | null {
  if (!isPlainObject(rawField)) {
    return null;
  }

  const key = readStringValue(rawField['key']);
  const rawType = rawField['type'];
  const typeName = readStringValue(rawType) ?? (isPlainObject(rawType) ? readStringValue(rawType['name']) : null);
  if (!key || !typeName) {
    return null;
  }

  return {
    key,
    name: readStringValue(rawField['name']),
    description: readStringValue(rawField['description']),
    required: readNullableBooleanValue(rawField['required']) ?? false,
    type: {
      name: typeName,
      category: isPlainObject(rawType) ? readStringValue(rawType['category']) : inferFieldTypeCategory(typeName),
    },
    validations: readValidationInputs(rawField['validations']),
  };
}

function mergeFieldDefinitionInput(
  existingField: MetaobjectFieldDefinitionRecord,
  rawField: Record<string, unknown>,
): MetaobjectFieldDefinitionRecord {
  const rawType = rawField['type'];
  const typeName = readStringValue(rawType) ?? (isPlainObject(rawType) ? readStringValue(rawType['name']) : null);

  return {
    key: readStringValue(rawField['key']) ?? existingField.key,
    name: hasOwnProperty(rawField, 'name') ? readStringValue(rawField['name']) : existingField.name,
    description: hasOwnProperty(rawField, 'description')
      ? readStringValue(rawField['description'])
      : existingField.description,
    required: hasOwnProperty(rawField, 'required')
      ? readNullableBooleanValue(rawField['required'])
      : existingField.required,
    type: typeName
      ? {
          name: typeName,
          category: isPlainObject(rawType)
            ? readStringValue(rawType['category'])
            : (inferFieldTypeCategory(typeName) ?? existingField.type.category),
        }
      : existingField.type,
    validations: hasOwnProperty(rawField, 'validations')
      ? readValidationInputs(rawField['validations'])
      : existingField.validations,
  };
}

function normalizeCapabilitiesFromInput(
  rawCapabilities: unknown,
  baseCapabilities: MetaobjectDefinitionCapabilitiesRecord = DEFAULT_METAOBJECT_DEFINITION_CAPABILITIES,
): MetaobjectDefinitionCapabilitiesRecord {
  const capabilities = isPlainObject(rawCapabilities) ? rawCapabilities : {};
  const result: MetaobjectDefinitionCapabilitiesRecord = structuredClone(baseCapabilities);

  for (const capabilityName of ['publishable', 'translatable', 'renderable', 'onlineStore'] as const) {
    const rawCapability = capabilities[capabilityName];
    if (!isPlainObject(rawCapability)) {
      continue;
    }

    const enabled = readNullableBooleanValue(rawCapability['enabled']);
    if (enabled !== null) {
      result[capabilityName] = { enabled };
    }
  }

  return result;
}

function buildDefinitionAccess(
  rawAccess: unknown,
  baseAccess: Record<string, unknown> = DEFAULT_METAOBJECT_DEFINITION_ACCESS,
): Record<string, string | null> {
  const access = isPlainObject(rawAccess) ? rawAccess : {};
  const result: Record<string, string | null> = {};

  for (const [key, value] of Object.entries(baseAccess)) {
    if (typeof value === 'string' || value === null) {
      result[key] = value;
    }
  }

  for (const [key, value] of Object.entries(access)) {
    if (typeof value === 'string' || value === null) {
      result[key] = value;
    }
  }

  return result;
}

function buildCreateDefinitionUserErrors(input: Record<string, unknown>): MetaobjectUserError[] {
  const type = readStringValue(input['type']);
  const access = isPlainObject(input['access']) ? input['access'] : null;
  const userErrors: MetaobjectUserError[] = [];

  if (!type) {
    userErrors.push({
      field: ['definition', 'type'],
      message: "Type can't be blank",
      code: 'BLANK',
    });
  }

  if (!readStringValue(input['name'])) {
    userErrors.push({
      field: ['definition', 'name'],
      message: "Name can't be blank",
      code: 'BLANK',
    });
  }

  if (type && !isAppReservedMetaobjectType(type) && access && hasOwnProperty(access, 'admin')) {
    userErrors.push({
      field: ['definition', 'access', 'admin'],
      message: 'Admin access can only be specified on metaobject definitions that have an app-reserved type.',
      code: 'ADMIN_ACCESS_INPUT_NOT_ALLOWED',
    });
  }

  if (type && store.findEffectiveMetaobjectDefinitionByType(type)) {
    userErrors.push({
      field: ['definition', 'type'],
      message: 'Type has already been taken',
      code: 'TAKEN',
    });
  }

  return userErrors;
}

function buildMetaobjectDefinitionFromCreateInput(input: Record<string, unknown>): MetaobjectDefinitionRecord {
  const now = makeSyntheticTimestamp();
  const type = readStringValue(input['type']) ?? 'metaobject_definition';

  return {
    id: makeProxySyntheticGid('MetaobjectDefinition'),
    type,
    name: readStringValue(input['name']),
    description: readStringValue(input['description']),
    displayNameKey: readStringValue(input['displayNameKey']),
    access: buildDefinitionAccess(input['access']),
    capabilities: normalizeCapabilitiesFromInput(input['capabilities']),
    fieldDefinitions: readPlainObjectArray(input['fieldDefinitions']).flatMap((rawField) => {
      const field = readFieldDefinitionInput(rawField);
      return field ? [field] : [];
    }),
    hasThumbnailField: false,
    metaobjectsCount: 0,
    standardTemplate: null,
    createdAt: now,
    updatedAt: now,
  };
}

function applyDefinitionScalarUpdates(
  existing: MetaobjectDefinitionRecord,
  input: Record<string, unknown>,
): MetaobjectDefinitionRecord {
  return {
    ...existing,
    name: hasOwnProperty(input, 'name') ? readStringValue(input['name']) : existing.name,
    description: hasOwnProperty(input, 'description') ? readStringValue(input['description']) : existing.description,
    displayNameKey: hasOwnProperty(input, 'displayNameKey')
      ? readStringValue(input['displayNameKey'])
      : existing.displayNameKey,
    access: hasOwnProperty(input, 'access') ? buildDefinitionAccess(input['access'], existing.access) : existing.access,
    capabilities: hasOwnProperty(input, 'capabilities')
      ? normalizeCapabilitiesFromInput(input['capabilities'], existing.capabilities)
      : existing.capabilities,
    updatedAt: makeSyntheticTimestamp(),
  };
}

function readFieldOperationPayload(rawOperation: Record<string, unknown>): {
  kind: 'create' | 'update' | 'delete' | 'upsert';
  payload: unknown;
} {
  if (hasOwnProperty(rawOperation, 'create')) {
    return { kind: 'create', payload: rawOperation['create'] };
  }

  if (hasOwnProperty(rawOperation, 'update')) {
    return { kind: 'update', payload: rawOperation['update'] };
  }

  if (hasOwnProperty(rawOperation, 'delete')) {
    return { kind: 'delete', payload: rawOperation['delete'] };
  }

  return { kind: 'upsert', payload: rawOperation };
}

function readDeleteFieldKey(payload: unknown): string | null {
  if (typeof payload === 'string') {
    return payload;
  }

  return isPlainObject(payload) ? readStringValue(payload['key']) : null;
}

function applyFieldDefinitionOperations(
  existingFields: MetaobjectFieldDefinitionRecord[],
  rawOperations: unknown,
): {
  fieldDefinitions: MetaobjectFieldDefinitionRecord[];
  userErrors: MetaobjectUserError[];
  orderedOperationKeys: string[];
} {
  const fieldDefinitions = structuredClone(existingFields);
  const userErrors: MetaobjectUserError[] = [];
  const orderedOperationKeys: string[] = [];

  for (const [index, rawOperation] of readPlainObjectArray(rawOperations).entries()) {
    const { kind, payload } = readFieldOperationPayload(rawOperation);
    const payloadObject = isPlainObject(payload) ? payload : null;
    const key =
      kind === 'delete' ? readDeleteFieldKey(payload) : payloadObject ? readStringValue(payloadObject['key']) : null;

    if (!key) {
      userErrors.push({
        field: ['definition', 'fieldDefinitions', String(index), 'key'],
        message: "Key can't be blank",
        code: 'BLANK',
        elementIndex: index,
      });
      continue;
    }

    orderedOperationKeys.push(key);
    const existingIndex = fieldDefinitions.findIndex((fieldDefinition) => fieldDefinition.key === key);

    if (kind === 'delete') {
      if (existingIndex < 0) {
        userErrors.push({
          field: ['definition', 'fieldDefinitions', String(index), 'delete'],
          message: 'Field definition not found.',
          code: 'NOT_FOUND',
          elementKey: key,
          elementIndex: index,
        });
      } else {
        fieldDefinitions.splice(existingIndex, 1);
      }
      continue;
    }

    if (!payloadObject) {
      userErrors.push({
        field: ['definition', 'fieldDefinitions', String(index)],
        message: 'Field definition input is invalid.',
        code: 'INVALID',
        elementKey: key,
        elementIndex: index,
      });
      continue;
    }

    if (kind === 'create') {
      if (existingIndex >= 0) {
        userErrors.push({
          field: ['definition', 'fieldDefinitions', String(index), 'create'],
          message: 'Field definition already exists.',
          code: 'TAKEN',
          elementKey: key,
          elementIndex: index,
        });
        continue;
      }

      const nextField = readFieldDefinitionInput(payloadObject);
      if (nextField) {
        fieldDefinitions.push(nextField);
      }
      continue;
    }

    if (kind === 'update' && existingIndex < 0) {
      userErrors.push({
        field: ['definition', 'fieldDefinitions', String(index), 'update'],
        message: 'Field definition not found.',
        code: 'NOT_FOUND',
        elementKey: key,
        elementIndex: index,
      });
      continue;
    }

    if (existingIndex >= 0) {
      const existingField = fieldDefinitions[existingIndex];
      if (existingField) {
        fieldDefinitions[existingIndex] = mergeFieldDefinitionInput(existingField, payloadObject);
      }
    } else {
      const nextField = readFieldDefinitionInput(payloadObject);
      if (nextField) {
        fieldDefinitions.push(nextField);
      }
    }
  }

  return { fieldDefinitions, userErrors, orderedOperationKeys };
}

function reorderFieldDefinitions(
  fieldDefinitions: MetaobjectFieldDefinitionRecord[],
  orderedOperationKeys: string[],
): MetaobjectFieldDefinitionRecord[] {
  const byKey = new Map(fieldDefinitions.map((fieldDefinition) => [fieldDefinition.key, fieldDefinition]));
  const ordered: MetaobjectFieldDefinitionRecord[] = [];

  for (const key of orderedOperationKeys) {
    const fieldDefinition = byKey.get(key);
    if (!fieldDefinition || ordered.some((candidate) => candidate.key === key)) {
      continue;
    }
    ordered.push(fieldDefinition);
  }

  for (const fieldDefinition of fieldDefinitions) {
    if (!ordered.some((candidate) => candidate.key === fieldDefinition.key)) {
      ordered.push(fieldDefinition);
    }
  }

  return ordered;
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

function normalizeMetaobjectCapabilities(rawCapabilities: unknown): MetaobjectRecord['capabilities'] {
  const capabilities = isPlainObject(rawCapabilities) ? rawCapabilities : {};
  const result: MetaobjectRecord['capabilities'] = {};

  const rawPublishable = capabilities['publishable'];
  if (isPlainObject(rawPublishable)) {
    result.publishable = {
      status: readStringValue(rawPublishable['status']),
    };
  }

  result.onlineStore = isPlainObject(capabilities['onlineStore'])
    ? {
        templateSuffix: readStringValue(capabilities['onlineStore']['templateSuffix']),
      }
    : null;

  return result;
}

function normalizeMetaobjectFieldDefinition(rawDefinition: unknown): MetaobjectFieldRecord['definition'] {
  if (!isPlainObject(rawDefinition)) {
    return null;
  }

  const key = readStringValue(rawDefinition['key']);
  const rawType = isPlainObject(rawDefinition['type']) ? rawDefinition['type'] : null;
  const typeName = rawType ? readStringValue(rawType['name']) : null;
  if (!key || !typeName) {
    return null;
  }

  return {
    key,
    name: readStringValue(rawDefinition['name']),
    required: readBooleanValue(rawDefinition['required']),
    type: {
      name: typeName,
      category: rawType ? readStringValue(rawType['category']) : null,
    },
  };
}

function normalizeMetaobjectField(rawField: Record<string, unknown>): MetaobjectFieldRecord | null {
  const key = readStringValue(rawField['key']);
  if (!key) {
    return null;
  }

  return {
    key,
    type: readStringValue(rawField['type']),
    value: readStringValue(rawField['value']),
    jsonValue: (rawField['jsonValue'] === undefined
      ? null
      : rawField['jsonValue']) as MetaobjectFieldRecord['jsonValue'],
    definition: normalizeMetaobjectFieldDefinition(rawField['definition']),
  };
}

function normalizeMetaobject(rawMetaobject: unknown): MetaobjectRecord | null {
  if (!isPlainObject(rawMetaobject)) {
    return null;
  }

  const id = readStringValue(rawMetaobject['id']);
  const handle = readStringValue(rawMetaobject['handle']);
  const type = readStringValue(rawMetaobject['type']);
  if (!id || !handle || !type) {
    return null;
  }

  const fieldsByKey = new Map<string, MetaobjectFieldRecord>();
  for (const rawField of readPlainObjectArray(rawMetaobject['fields'])) {
    const field = normalizeMetaobjectField(rawField);
    if (field) {
      fieldsByKey.set(field.key, field);
    }
  }

  for (const value of Object.values(rawMetaobject)) {
    const field = normalizeMetaobjectField(isPlainObject(value) ? value : {});
    if (field && !fieldsByKey.has(field.key)) {
      fieldsByKey.set(field.key, field);
    }
  }

  return {
    id,
    handle,
    type,
    displayName: readStringValue(rawMetaobject['displayName']),
    fields: [...fieldsByKey.values()],
    capabilities: normalizeMetaobjectCapabilities(rawMetaobject['capabilities']),
    createdAt: readStringValue(rawMetaobject['createdAt']),
    updatedAt: readStringValue(rawMetaobject['updatedAt']),
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

function buildSerializableMetaobjectField(field: MetaobjectFieldRecord): Record<string, unknown> {
  return {
    __typename: 'MetaobjectField',
    key: field.key,
    type: field.type,
    value: field.value,
    jsonValue: field.jsonValue,
    definition: field.definition
      ? {
          __typename: 'MetaobjectFieldDefinition',
          ...field.definition,
          type: {
            __typename: 'MetafieldDefinitionType',
            ...field.definition.type,
          },
        }
      : null,
  };
}

function buildSerializableMetaobject(metaobject: MetaobjectRecord): Record<string, unknown> {
  const definition = store.findEffectiveMetaobjectDefinitionByType(metaobject.type);
  return {
    __typename: 'Metaobject',
    id: metaobject.id,
    handle: metaobject.handle,
    type: metaobject.type,
    displayName: metaobject.displayName,
    createdAt: metaobject.createdAt ?? null,
    updatedAt: metaobject.updatedAt ?? null,
    capabilities: {
      publishable: metaobject.capabilities.publishable ?? null,
      onlineStore: metaobject.capabilities.onlineStore ?? null,
    },
    definition: definition ? buildSerializableDefinition(definition) : null,
    fields: metaobject.fields.map(buildSerializableMetaobjectField),
  };
}

function serializeDefinitionSelection(
  definition: MetaobjectDefinitionRecord,
  selections: readonly SelectionNode[],
  document: string,
): Record<string, unknown> {
  return projectGraphqlObject(buildSerializableDefinition(definition), selections, getDocumentFragments(document));
}

function serializeEmptyReferencedByConnection(field: FieldNode): Record<string, unknown> {
  return serializeConnection(field, {
    items: [],
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: () => '',
    serializeNode: () => null,
  });
}

function serializeMetaobjectSelection(
  metaobject: MetaobjectRecord,
  selections: readonly SelectionNode[],
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const fragments = getDocumentFragments(document);

  return projectGraphqlObject(buildSerializableMetaobject(metaobject), selections, fragments, {
    projectFieldValue: ({ source, field, fieldName }) => {
      if (source['__typename'] !== 'Metaobject') {
        return { handled: false };
      }

      if (fieldName === 'field') {
        const key = readStringValue(getFieldArguments(field, variables)['key']);
        const selectedField = key ? metaobject.fields.find((candidate) => candidate.key === key) : null;
        return {
          handled: true,
          value: selectedField
            ? projectGraphqlObject(
                buildSerializableMetaobjectField(selectedField),
                field.selectionSet?.selections ?? [],
                fragments,
              )
            : null,
        };
      }

      if (fieldName === 'referencedBy') {
        return {
          handled: true,
          value: serializeEmptyReferencedByConnection(field),
        };
      }

      return { handled: false };
    },
  });
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

function compareNullableStrings(left: string | null | undefined, right: string | null | undefined): number {
  return (left ?? '').localeCompare(right ?? '');
}

function sortMetaobjects(metaobjects: MetaobjectRecord[], sortKey: unknown, reverse: unknown): MetaobjectRecord[] {
  const normalizedSortKey = typeof sortKey === 'string' ? sortKey.toLowerCase() : 'id';
  const sorted = [...metaobjects].sort((left, right) => {
    switch (normalizedSortKey) {
      case 'display_name':
        return (
          compareNullableStrings(left.displayName, right.displayName) || compareShopifyResourceIds(left.id, right.id)
        );
      case 'type':
        return (
          left.type.localeCompare(right.type) ||
          left.handle.localeCompare(right.handle) ||
          compareShopifyResourceIds(left.id, right.id)
        );
      case 'updated_at':
        return compareNullableStrings(left.updatedAt, right.updatedAt) || compareShopifyResourceIds(left.id, right.id);
      case 'id':
      default:
        return compareShopifyResourceIds(left.id, right.id);
    }
  });

  return reverse === true ? sorted.reverse() : sorted;
}

function matchesStringValue(value: string | null | undefined, expected: string): boolean {
  if (!value) {
    return false;
  }

  return normalizeSearchQueryValue(value).includes(normalizeSearchQueryValue(expected));
}

function metaobjectSearchTextMatches(metaobject: MetaobjectRecord, rawValue: string): boolean {
  const searchableValues = [
    metaobject.id,
    metaobject.handle,
    metaobject.type,
    metaobject.displayName ?? '',
    ...metaobject.fields.flatMap((field) => [
      field.key,
      field.type ?? '',
      field.value ?? '',
      typeof field.jsonValue === 'string' ? field.jsonValue : JSON.stringify(field.jsonValue),
    ]),
  ];

  return searchableValues.some((candidate) => matchesStringValue(candidate, rawValue));
}

function matchesMetaobjectFieldQuery(metaobject: MetaobjectRecord, fieldKey: string, term: SearchQueryTerm): boolean {
  const field = metaobject.fields.find((candidate) => candidate.key === fieldKey);
  if (!field) {
    return false;
  }

  const values = [field.value, typeof field.jsonValue === 'string' ? field.jsonValue : JSON.stringify(field.jsonValue)];
  return values.some((value) => matchesSearchQueryText(value, term));
}

function matchesPositiveMetaobjectQueryTerm(metaobject: MetaobjectRecord, term: SearchQueryTerm): boolean {
  if (term.field === null) {
    return metaobjectSearchTextMatches(metaobject, term.value);
  }

  const field = term.field.toLowerCase();
  switch (field) {
    case 'id':
      return matchesStringValue(metaobject.id, term.value);
    case 'handle':
      return normalizeSearchQueryValue(metaobject.handle) === normalizeSearchQueryValue(term.value);
    case 'type':
      return normalizeSearchQueryValue(metaobject.type) === normalizeSearchQueryValue(term.value);
    case 'display_name':
      return matchesSearchQueryText(metaobject.displayName, term);
    case 'created_at':
      return matchesSearchQueryDate(metaobject.createdAt, term);
    case 'updated_at':
      return matchesSearchQueryDate(metaobject.updatedAt, term);
    default:
      if (field.startsWith('fields.')) {
        return matchesMetaobjectFieldQuery(metaobject, field.slice('fields.'.length), term);
      }
      return true;
  }
}

function matchesMetaobjectQueryTerm(metaobject: MetaobjectRecord, term: SearchQueryTerm): boolean {
  if (!term.raw) {
    return true;
  }

  const matches = matchesPositiveMetaobjectQueryTerm(metaobject, term);
  return term.negated ? !matches : matches;
}

function matchesMetaobjectQueryNode(metaobject: MetaobjectRecord, node: SearchQueryNode): boolean {
  switch (node.type) {
    case 'term':
      return matchesMetaobjectQueryTerm(metaobject, node.term);
    case 'and':
      return node.children.every((child) => matchesMetaobjectQueryNode(metaobject, child));
    case 'or':
      return node.children.some((child) => matchesMetaobjectQueryNode(metaobject, child));
    case 'not':
      return !matchesMetaobjectQueryNode(metaobject, node.child);
  }
}

function applyMetaobjectQuery(metaobjects: MetaobjectRecord[], rawQuery: unknown): MetaobjectRecord[] {
  if (typeof rawQuery !== 'string' || !rawQuery.trim()) {
    return metaobjects;
  }

  const parsedQuery = parseSearchQuery(rawQuery, { recognizeNotKeyword: true });
  if (!parsedQuery) {
    return metaobjects;
  }

  return metaobjects.filter((metaobject) => matchesMetaobjectQueryNode(metaobject, parsedQuery));
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

function serializeMetaobjectsConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
  document: string,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const type = readStringValue(args['type']);
  const metaobjects = sortMetaobjects(
    applyMetaobjectQuery(type ? store.listEffectiveMetaobjectsByType(type) : [], args['query']),
    args['sortKey'],
    args['reverse'],
  );
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    metaobjects,
    field,
    variables,
    (metaobject) => metaobject.id,
  );

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (metaobject) => metaobject.id,
    serializeNode: (metaobject, nodeField) =>
      serializeMetaobjectSelection(metaobject, nodeField.selectionSet?.selections ?? [], document, variables),
  });
}

function serializeUserError(
  selections: readonly SelectionNode[],
  userError: MetaobjectUserError,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'field':
        result[key] = userError.field;
        break;
      case 'message':
        result[key] = userError.message;
        break;
      case 'code':
        result[key] = userError.code;
        break;
      case 'elementKey':
        result[key] = userError.elementKey ?? null;
        break;
      case 'elementIndex':
        result[key] = userError.elementIndex ?? null;
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializeDefinitionMutationPayload(
  field: FieldNode,
  document: string,
  definition: MetaobjectDefinitionRecord | null,
  userErrors: MetaobjectUserError[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'metaobjectDefinition':
      case 'createdDefinition':
        result[key] = definition
          ? serializeDefinitionSelection(definition, selection.selectionSet?.selections ?? [], document)
          : null;
        break;
      case 'userErrors':
        result[key] = userErrors.map((userError) =>
          serializeUserError(selection.selectionSet?.selections ?? [], userError),
        );
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializeDeleteDefinitionPayload(
  field: FieldNode,
  deletedId: string | null,
  userErrors: MetaobjectUserError[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'deletedId':
        result[key] = deletedId;
        break;
      case 'userErrors':
        result[key] = userErrors.map((userError) =>
          serializeUserError(selection.selectionSet?.selections ?? [], userError),
        );
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializeMetaobjectDefinitionCreateMutation(
  field: FieldNode,
  variables: Record<string, unknown>,
  document: string,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const input = isPlainObject(args['definition']) ? args['definition'] : {};
  const userErrors = buildCreateDefinitionUserErrors(input);

  if (userErrors.length > 0) {
    return serializeDefinitionMutationPayload(field, document, null, userErrors);
  }

  const definition = buildMetaobjectDefinitionFromCreateInput(input);
  store.upsertStagedMetaobjectDefinitions([definition]);
  return serializeDefinitionMutationPayload(field, document, definition, []);
}

function serializeMetaobjectDefinitionUpdateMutation(
  field: FieldNode,
  variables: Record<string, unknown>,
  document: string,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const id = readStringValue(args['id']);
  const existingDefinition = id ? store.getEffectiveMetaobjectDefinitionById(id) : null;
  if (!id || !existingDefinition) {
    return serializeDefinitionMutationPayload(field, document, null, [
      {
        field: ['id'],
        message: 'Metaobject definition not found.',
        code: 'NOT_FOUND',
      },
    ]);
  }

  const input = isPlainObject(args['definition']) ? args['definition'] : {};
  let definition = applyDefinitionScalarUpdates(existingDefinition, input);
  const fieldDefinitionUpdate = hasOwnProperty(input, 'fieldDefinitions')
    ? applyFieldDefinitionOperations(definition.fieldDefinitions, input['fieldDefinitions'])
    : null;

  if (fieldDefinitionUpdate) {
    if (fieldDefinitionUpdate.userErrors.length > 0) {
      return serializeDefinitionMutationPayload(field, document, null, fieldDefinitionUpdate.userErrors);
    }

    definition = {
      ...definition,
      fieldDefinitions:
        args['resetFieldOrder'] === true || input['resetFieldOrder'] === true
          ? reorderFieldDefinitions(fieldDefinitionUpdate.fieldDefinitions, fieldDefinitionUpdate.orderedOperationKeys)
          : fieldDefinitionUpdate.fieldDefinitions,
    };
  }

  store.upsertStagedMetaobjectDefinitions([definition]);
  return serializeDefinitionMutationPayload(field, document, definition, []);
}

function serializeMetaobjectDefinitionDeleteMutation(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const id = readStringValue(args['id']);
  const definition = id ? store.getEffectiveMetaobjectDefinitionById(id) : null;

  if (!id || !definition) {
    return serializeDeleteDefinitionPayload(field, null, [
      {
        field: ['id'],
        message: 'Metaobject definition not found.',
        code: 'NOT_FOUND',
      },
    ]);
  }

  if ((definition.metaobjectsCount ?? 0) > 0) {
    return serializeDeleteDefinitionPayload(field, null, [
      {
        field: ['id'],
        message:
          'Local proxy cannot delete a metaobject definition with associated metaobjects until entry cascade behavior is modeled.',
        code: 'UNSUPPORTED',
      },
    ]);
  }

  store.deleteStagedMetaobjectDefinition(id);
  return serializeDeleteDefinitionPayload(field, id, []);
}

function serializeStandardMetaobjectDefinitionEnableMutation(
  field: FieldNode,
  variables: Record<string, unknown>,
  document: string,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const type = readStringValue(args['type']);

  if (!type) {
    return serializeDefinitionMutationPayload(field, document, null, [
      {
        field: ['type'],
        message: "Type can't be blank",
        code: 'BLANK',
      },
    ]);
  }

  const template = STANDARD_METAOBJECT_DEFINITION_TEMPLATES.find((candidate) => candidate.type === type) ?? null;
  if (!template) {
    return serializeDefinitionMutationPayload(field, document, null, [
      {
        field: ['type'],
        message: "A standard metaobject definition wasn't found for the specified type.",
        code: 'TEMPLATE_NOT_FOUND',
      },
    ]);
  }

  const existingDefinition = store.findEffectiveMetaobjectDefinitionByType(type);
  const now = makeSyntheticTimestamp();
  const definition: MetaobjectDefinitionRecord = existingDefinition ?? {
    id: makeProxySyntheticGid('MetaobjectDefinition'),
    type: template.type,
    name: template.name,
    description: null,
    displayNameKey: template.displayNameKey,
    access: DEFAULT_METAOBJECT_DEFINITION_ACCESS,
    capabilities: DEFAULT_METAOBJECT_DEFINITION_CAPABILITIES,
    fieldDefinitions: structuredClone(template.fieldDefinitions),
    hasThumbnailField: false,
    metaobjectsCount: 0,
    standardTemplate: {
      type: template.type,
      name: template.name,
    },
    createdAt: now,
    updatedAt: now,
  };

  store.upsertStagedMetaobjectDefinitions([definition]);
  return serializeDefinitionMutationPayload(field, document, definition, []);
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
      case 'metaobject': {
        const id = readRootStringArgument(field, variables, 'id');
        const metaobject = id ? store.getEffectiveMetaobjectById(id) : null;
        data[responseKey] = metaobject
          ? serializeMetaobjectSelection(metaobject, field.selectionSet?.selections ?? [], document, variables)
          : null;
        break;
      }
      case 'metaobjectByHandle': {
        const args = getFieldArguments(field, variables);
        const handle = isPlainObject(args['handle']) ? args['handle'] : null;
        const type = handle ? readStringValue(handle['type']) : null;
        const handleValue = handle ? readStringValue(handle['handle']) : null;
        const metaobject =
          type && handleValue ? store.findEffectiveMetaobjectByHandle({ type, handle: handleValue }) : null;
        data[responseKey] = metaobject
          ? serializeMetaobjectSelection(metaobject, field.selectionSet?.selections ?? [], document, variables)
          : null;
        break;
      }
      case 'metaobjects':
        data[responseKey] = serializeMetaobjectsConnection(field, variables, document);
        break;
      default:
        data[responseKey] = null;
        break;
    }
  }

  return { data };
}

export const handleMetaobjectQuery = handleMetaobjectDefinitionQuery;

export function handleMetaobjectDefinitionMutation(
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const responseKey = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'metaobjectDefinitionCreate':
        data[responseKey] = serializeMetaobjectDefinitionCreateMutation(field, variables, document);
        break;
      case 'metaobjectDefinitionUpdate':
        data[responseKey] = serializeMetaobjectDefinitionUpdateMutation(field, variables, document);
        break;
      case 'metaobjectDefinitionDelete':
        data[responseKey] = serializeMetaobjectDefinitionDeleteMutation(field, variables);
        break;
      case 'standardMetaobjectDefinitionEnable':
        data[responseKey] = serializeStandardMetaobjectDefinitionEnableMutation(field, variables, document);
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

function collectMetaobjectsFromConnection(connection: unknown): MetaobjectRecord[] {
  if (!isPlainObject(connection)) {
    return [];
  }

  const byId = new Map<string, MetaobjectRecord>();
  for (const rawMetaobject of readPlainObjectArray(connection['nodes'])) {
    const metaobject = normalizeMetaobject(rawMetaobject);
    if (metaobject) {
      byId.set(metaobject.id, metaobject);
    }
  }

  for (const edge of readPlainObjectArray(connection['edges'])) {
    const metaobject = normalizeMetaobject(edge['node']);
    if (metaobject) {
      byId.set(metaobject.id, metaobject);
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
  const metaobjects: MetaobjectRecord[] = [];

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
      case 'metaobject':
      case 'metaobjectByHandle': {
        const metaobject = normalizeMetaobject(payload);
        if (metaobject) {
          metaobjects.push(metaobject);
        }
        break;
      }
      case 'metaobjects':
        metaobjects.push(...collectMetaobjectsFromConnection(payload));
        break;
      default:
        break;
    }
  }

  if (definitions.length > 0) {
    store.upsertBaseMetaobjectDefinitions(definitions);
  }

  if (metaobjects.length > 0) {
    store.upsertBaseMetaobjects(metaobjects);
  }
}

export const hydrateMetaobjectsFromUpstreamResponse = hydrateMetaobjectDefinitionsFromUpstreamResponse;
