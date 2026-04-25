import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { parseSearchQuery, type SearchQueryNode, type SearchQueryTerm } from '../search-query-parser.js';
import { compareShopifyResourceIds } from '../shopify/resource-ids.js';
import { store } from '../state/store.js';
import type {
  MetafieldDefinitionConstraintValueRecord,
  MetafieldDefinitionRecord,
  ProductMetafieldRecord,
} from '../state/types.js';
import {
  getFieldResponseKey,
  getSelectedChildFields,
  paginateConnectionItems,
  serializeConnectionPageInfo,
} from './graphql-helpers.js';
import { serializeMetafieldsConnection } from './metafields.js';

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function readBoolean(value: unknown): boolean {
  return value === true;
}

function stripSearchValueQuotes(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length >= 2) {
    const firstCharacter = trimmed[0];
    const lastCharacter = trimmed[trimmed.length - 1];
    if ((firstCharacter === '"' || firstCharacter === "'") && firstCharacter === lastCharacter) {
      return trimmed.slice(1, -1);
    }
  }

  return trimmed;
}

function searchTermValue(term: SearchQueryTerm): string {
  return term.comparator === null ? term.value : `${term.comparator}${term.value}`;
}

function matchesStringValue(candidate: string, rawValue: string, matchMode: 'includes' | 'exact'): boolean {
  const value = stripSearchValueQuotes(rawValue).trim().toLowerCase();
  if (!value) {
    return true;
  }

  const prefixMode = value.endsWith('*');
  const normalizedValue = prefixMode ? value.slice(0, -1) : value;
  if (!normalizedValue) {
    return true;
  }

  const normalizedCandidate = candidate.toLowerCase();
  if (prefixMode) {
    if (normalizedCandidate.startsWith(normalizedValue)) {
      return true;
    }

    return normalizedCandidate.split(/[^a-z0-9]+/u).some((part) => part.startsWith(normalizedValue));
  }

  return matchMode === 'exact'
    ? normalizedCandidate === normalizedValue
    : normalizedCandidate.includes(normalizedValue);
}

function matchesResourceIdValue(resourceId: string, rawValue: string): boolean {
  const normalizedValue = stripSearchValueQuotes(rawValue).trim();
  if (!normalizedValue) {
    return true;
  }

  if (normalizedValue.startsWith('gid://')) {
    return resourceId === normalizedValue;
  }

  return resourceId.split('/').at(-1) === normalizedValue;
}

function matchesResourceIdRange(resourceId: string, rawValue: string): boolean {
  const match = rawValue.match(/^(<=|>=|<|>|=)?\s*(.+)$/u);
  if (!match) {
    return matchesResourceIdValue(resourceId, rawValue);
  }

  const operator = match[1] ?? '=';
  const thresholdValue = stripSearchValueQuotes(match[2]?.trim() ?? '');
  if (!thresholdValue) {
    return true;
  }

  if (operator === '=') {
    return matchesResourceIdValue(resourceId, thresholdValue);
  }

  const resourceNumericId = Number.parseInt(resourceId.split('/').at(-1) ?? '', 10);
  const thresholdNumericId = Number.parseInt(thresholdValue.split('/').at(-1) ?? thresholdValue, 10);
  if (!Number.isFinite(resourceNumericId) || !Number.isFinite(thresholdNumericId)) {
    return true;
  }

  switch (operator) {
    case '<=':
      return resourceNumericId <= thresholdNumericId;
    case '>=':
      return resourceNumericId >= thresholdNumericId;
    case '<':
      return resourceNumericId < thresholdNumericId;
    case '>':
      return resourceNumericId > thresholdNumericId;
    default:
      return true;
  }
}

function definitionSearchTextMatches(definition: MetafieldDefinitionRecord, rawValue: string): boolean {
  const searchableValues = [
    definition.name,
    definition.namespace,
    definition.key,
    definition.description ?? '',
    definition.ownerType,
    definition.type.name,
    definition.type.category ?? '',
  ];

  return searchableValues.some((candidate) => matchesStringValue(candidate, rawValue, 'includes'));
}

function matchesPositiveDefinitionQueryTerm(definition: MetafieldDefinitionRecord, term: SearchQueryTerm): boolean {
  if (term.field === null) {
    return definitionSearchTextMatches(definition, term.value);
  }

  const field = term.field.toLowerCase();
  const value = searchTermValue(term);

  switch (field) {
    case 'id':
      return matchesResourceIdRange(definition.id, value);
    case 'key':
      return matchesStringValue(definition.key, value, 'exact');
    case 'namespace':
      return matchesStringValue(definition.namespace, value, 'exact');
    case 'owner_type':
      return matchesStringValue(definition.ownerType, value, 'exact');
    case 'type':
      return matchesStringValue(definition.type.name, value, 'exact');
    default:
      return true;
  }
}

function matchesDefinitionQueryTerm(definition: MetafieldDefinitionRecord, term: SearchQueryTerm): boolean {
  if (!term.raw) {
    return true;
  }

  const matches = matchesPositiveDefinitionQueryTerm(definition, term);
  return term.negated ? !matches : matches;
}

function matchesDefinitionQueryNode(definition: MetafieldDefinitionRecord, node: SearchQueryNode): boolean {
  switch (node.type) {
    case 'term':
      return matchesDefinitionQueryTerm(definition, node.term);
    case 'and':
      return node.children.every((child) => matchesDefinitionQueryNode(definition, child));
    case 'or':
      return node.children.some((child) => matchesDefinitionQueryNode(definition, child));
    case 'not':
      return !matchesDefinitionQueryNode(definition, node.child);
  }
}

function applyDefinitionQuery(
  definitions: MetafieldDefinitionRecord[],
  rawQuery: unknown,
): MetafieldDefinitionRecord[] {
  if (typeof rawQuery !== 'string' || !rawQuery.trim()) {
    return definitions;
  }

  const parsedQuery = parseSearchQuery(rawQuery, { recognizeNotKeyword: true });
  if (!parsedQuery) {
    return definitions;
  }

  return definitions.filter((definition) => matchesDefinitionQueryNode(definition, parsedQuery));
}

function isDefinitionConstrained(definition: MetafieldDefinitionRecord): boolean {
  return Boolean(definition.constraints?.key) || (definition.constraints?.values.length ?? 0) > 0;
}

function matchesConstraintSubtype(definition: MetafieldDefinitionRecord, rawConstraintSubtype: unknown): boolean {
  if (!isObject(rawConstraintSubtype)) {
    return true;
  }

  const key = readString(rawConstraintSubtype['key']);
  const value = readString(rawConstraintSubtype['value']);
  if (!key && !value) {
    return true;
  }

  if (key && definition.constraints?.key !== key) {
    return false;
  }

  if (!value) {
    return true;
  }

  return (definition.constraints?.values ?? []).some((candidate) => candidate.value === value);
}

function applyDefinitionFilters(
  definitions: MetafieldDefinitionRecord[],
  args: Record<string, unknown>,
): MetafieldDefinitionRecord[] {
  const ownerType = readString(args['ownerType']);
  const namespace = readString(args['namespace']);
  const key = readString(args['key']);
  const pinnedStatus = readString(args['pinnedStatus']) ?? 'ANY';
  const constraintStatus = readString(args['constraintStatus']) ?? 'CONSTRAINED_AND_UNCONSTRAINED';

  return applyDefinitionQuery(definitions, args['query']).filter((definition) => {
    if (ownerType && definition.ownerType !== ownerType) {
      return false;
    }
    if (namespace && definition.namespace !== namespace) {
      return false;
    }
    if (key && definition.key !== key) {
      return false;
    }
    if (pinnedStatus === 'PINNED' && definition.pinnedPosition === null) {
      return false;
    }
    if (pinnedStatus === 'UNPINNED' && definition.pinnedPosition !== null) {
      return false;
    }
    if (constraintStatus === 'CONSTRAINED_ONLY' && !isDefinitionConstrained(definition)) {
      return false;
    }
    if (constraintStatus === 'UNCONSTRAINED_ONLY' && isDefinitionConstrained(definition)) {
      return false;
    }

    return matchesConstraintSubtype(definition, args['constraintSubtype']);
  });
}

function sortDefinitions(
  definitions: MetafieldDefinitionRecord[],
  sortKey: unknown,
  reverse: unknown,
): MetafieldDefinitionRecord[] {
  const normalizedSortKey = typeof sortKey === 'string' ? sortKey : 'ID';
  const sorted = [...definitions].sort((left, right) => {
    switch (normalizedSortKey) {
      case 'NAME':
        return left.name.localeCompare(right.name) || compareShopifyResourceIds(left.id, right.id);
      case 'PINNED_POSITION': {
        const leftPosition = left.pinnedPosition ?? Number.NEGATIVE_INFINITY;
        const rightPosition = right.pinnedPosition ?? Number.NEGATIVE_INFINITY;
        return rightPosition - leftPosition || compareShopifyResourceIds(right.id, left.id);
      }
      case 'RELEVANCE':
      case 'ID':
      default:
        return compareShopifyResourceIds(left.id, right.id);
    }
  });

  return readBoolean(reverse) ? sorted.reverse() : sorted;
}

function getProductMetafieldsForDefinition(definition: MetafieldDefinitionRecord): ProductMetafieldRecord[] {
  if (definition.ownerType !== 'PRODUCT') {
    return [];
  }

  return store
    .listEffectiveProducts()
    .flatMap((product) => store.getEffectiveMetafieldsByProductId(product.id))
    .filter((metafield) => metafield.namespace === definition.namespace && metafield.key === definition.key)
    .sort((left, right) => compareShopifyResourceIds(left.id, right.id));
}

function serializeUnknownValue(value: unknown, selections: readonly SelectionNode[]): unknown {
  if (Array.isArray(value)) {
    return value.map((item) => serializeUnknownValue(item, selections));
  }

  if (!isObject(value)) {
    return value ?? null;
  }

  return Object.fromEntries(
    selections
      .filter((selection): selection is FieldNode => selection.kind === Kind.FIELD)
      .map((selection) => {
        const key = getFieldResponseKey(selection);
        return [key, serializeUnknownValue(value[selection.name.value], selection.selectionSet?.selections ?? [])];
      }),
  );
}

function serializeConstraintValuesConnection(
  values: MetafieldDefinitionConstraintValueRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    values,
    field,
    variables,
    (value) => value.value,
  );
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = items.map((value) => serializeUnknownValue(value, selection.selectionSet?.selections ?? []));
        break;
      case 'edges':
        result[key] = items.map((value) => {
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edge[edgeKey] = `cursor:${value.value}`;
                break;
              case 'node':
                edge[edgeKey] = serializeUnknownValue(value, edgeSelection.selectionSet?.selections ?? []);
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
          items,
          hasNextPage,
          hasPreviousPage,
          (value) => value.value,
        );
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializeConstraints(
  definition: MetafieldDefinitionRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  if (!definition.constraints) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'key':
        result[key] = definition.constraints.key;
        break;
      case 'values':
        result[key] = serializeConstraintValuesConnection(definition.constraints.values, selection, variables);
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializeDefinitionMetafieldsConnection(
  definition: MetafieldDefinitionRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const metafields = getProductMetafieldsForDefinition(definition);
  const args = getFieldArguments(field, variables);
  const orderedMetafields = readBoolean(args['reverse']) ? [...metafields].reverse() : metafields;
  return serializeMetafieldsConnection(orderedMetafields, field, variables);
}

function serializeDefinitionSelectionSet(
  definition: MetafieldDefinitionRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = definition.id;
        break;
      case 'name':
        result[key] = definition.name;
        break;
      case 'namespace':
        result[key] = definition.namespace;
        break;
      case 'key':
        result[key] = definition.key;
        break;
      case 'ownerType':
        result[key] = definition.ownerType;
        break;
      case 'type':
        result[key] = serializeUnknownValue(definition.type, selection.selectionSet?.selections ?? []);
        break;
      case 'description':
        result[key] = definition.description;
        break;
      case 'validations':
        result[key] = definition.validations.map((validation) =>
          serializeUnknownValue(validation, selection.selectionSet?.selections ?? []),
        );
        break;
      case 'access':
        result[key] = serializeUnknownValue(definition.access, selection.selectionSet?.selections ?? []);
        break;
      case 'capabilities':
        result[key] = serializeUnknownValue(definition.capabilities, selection.selectionSet?.selections ?? []);
        break;
      case 'constraints':
        result[key] = serializeConstraints(definition, selection, variables);
        break;
      case 'pinnedPosition':
        result[key] = definition.pinnedPosition;
        break;
      case 'validationStatus':
        result[key] = definition.validationStatus;
        break;
      case 'metafieldsCount':
        result[key] = getProductMetafieldsForDefinition(definition).length;
        break;
      case 'metafields':
        result[key] = serializeDefinitionMetafieldsConnection(definition, selection, variables);
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function readDefinitionIdentifier(
  args: Record<string, unknown>,
): { ownerType: string; namespace: string; key: string } | null {
  const identifier = isObject(args['identifier']) ? args['identifier'] : null;
  if (!identifier) {
    return null;
  }

  const ownerType = readString(identifier['ownerType']);
  const namespace = readString(identifier['namespace']);
  const key = readString(identifier['key']);
  if (!ownerType || !namespace || !key) {
    return null;
  }

  return { ownerType, namespace, key };
}

function getDefinitionReferenceField(args: Record<string, unknown>): string[] {
  if (typeof args['definitionId'] === 'string') {
    return ['definitionId'];
  }

  return ['identifier'];
}

function findDefinitionFromMutationArgs(args: Record<string, unknown>): MetafieldDefinitionRecord | null {
  const definitionId = readString(args['definitionId']);
  if (definitionId) {
    return store.getEffectiveMetafieldDefinitionById(definitionId);
  }

  const identifier = readDefinitionIdentifier(args);
  return identifier ? store.findEffectiveMetafieldDefinition(identifier) : null;
}

function serializeUserError(
  selections: readonly SelectionNode[],
  error: { field: string[]; message: string; code: string },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'field':
        result[key] = error.field;
        break;
      case 'message':
        result[key] = error.message;
        break;
      case 'code':
        result[key] = error.code;
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function listPinnedDefinitions(ownerType: string): MetafieldDefinitionRecord[] {
  return store
    .listEffectiveMetafieldDefinitions()
    .filter((definition) => definition.ownerType === ownerType && definition.pinnedPosition !== null);
}

function updatePinnedDefinitions(definitions: MetafieldDefinitionRecord[]): void {
  if (definitions.length > 0) {
    store.upsertStagedMetafieldDefinitions(definitions);
  }
}

function pinDefinition(definition: MetafieldDefinitionRecord): MetafieldDefinitionRecord {
  if (definition.pinnedPosition !== null) {
    return definition;
  }

  const highestPinnedPosition = listPinnedDefinitions(definition.ownerType).reduce(
    (highest, candidate) => Math.max(highest, candidate.pinnedPosition ?? 0),
    0,
  );
  const pinnedDefinition = {
    ...definition,
    pinnedPosition: highestPinnedPosition + 1,
  };
  store.upsertStagedMetafieldDefinitions([pinnedDefinition]);

  return pinnedDefinition;
}

function unpinDefinition(definition: MetafieldDefinitionRecord): MetafieldDefinitionRecord {
  if (definition.pinnedPosition === null) {
    return definition;
  }

  const removedPosition = definition.pinnedPosition;
  const unpinnedDefinition = {
    ...definition,
    pinnedPosition: null,
  };
  const compactedDefinitions = listPinnedDefinitions(definition.ownerType)
    .filter((candidate) => candidate.id !== definition.id && (candidate.pinnedPosition ?? 0) > removedPosition)
    .map((candidate) => ({
      ...candidate,
      pinnedPosition: (candidate.pinnedPosition ?? 1) - 1,
    }));

  updatePinnedDefinitions([unpinnedDefinition, ...compactedDefinitions]);

  return unpinnedDefinition;
}

function serializePinningPayload(
  payloadFieldName: 'pinnedDefinition' | 'unpinnedDefinition',
  definition: MetafieldDefinitionRecord | null,
  userErrors: Array<{ field: string[]; message: string; code: string }>,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case payloadFieldName:
        result[key] = definition
          ? serializeDefinitionSelectionSet(definition, selection.selectionSet?.selections ?? [], variables)
          : null;
        break;
      case 'userErrors':
        result[key] = userErrors.map((error) => serializeUserError(selection.selectionSet?.selections ?? [], error));
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializeDefinitionPinRoot(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const definition = findDefinitionFromMutationArgs(args);
  if (!definition) {
    return serializePinningPayload(
      'pinnedDefinition',
      null,
      [
        {
          field: getDefinitionReferenceField(args),
          message: 'Definition not found.',
          code: 'NOT_FOUND',
        },
      ],
      field,
      variables,
    );
  }

  return serializePinningPayload('pinnedDefinition', pinDefinition(definition), [], field, variables);
}

function serializeDefinitionUnpinRoot(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const definition = findDefinitionFromMutationArgs(args);
  if (!definition) {
    return serializePinningPayload(
      'unpinnedDefinition',
      null,
      [
        {
          field: getDefinitionReferenceField(args),
          message: 'Definition not found.',
          code: 'NOT_FOUND',
        },
      ],
      field,
      variables,
    );
  }

  return serializePinningPayload('unpinnedDefinition', unpinDefinition(definition), [], field, variables);
}

function serializeMetafieldDefinitionRoot(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  const args = getFieldArguments(field, variables);
  const id = readString(args['id']);
  const definition = id
    ? store.getEffectiveMetafieldDefinitionById(id)
    : (() => {
        const identifier = readDefinitionIdentifier(args);
        return identifier ? store.findEffectiveMetafieldDefinition(identifier) : null;
      })();

  return definition
    ? serializeDefinitionSelectionSet(definition, field.selectionSet?.selections ?? [], variables)
    : null;
}

function serializeMetafieldDefinitionsConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const definitions = sortDefinitions(
    applyDefinitionFilters(store.listEffectiveMetafieldDefinitions(), args),
    args['sortKey'],
    args['reverse'],
  );
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    definitions,
    field,
    variables,
    (definition) => definition.id,
  );
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = items.map((definition) =>
          serializeDefinitionSelectionSet(definition, selection.selectionSet?.selections ?? [], variables),
        );
        break;
      case 'edges':
        result[key] = items.map((definition) => {
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edge[edgeKey] = `cursor:${definition.id}`;
                break;
              case 'node':
                edge[edgeKey] = serializeDefinitionSelectionSet(
                  definition,
                  edgeSelection.selectionSet?.selections ?? [],
                  variables,
                );
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
          items,
          hasNextPage,
          hasPreviousPage,
          (definition) => definition.id,
        );
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

export function handleMetafieldDefinitionQuery(
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const responseKey = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'metafieldDefinition':
        data[responseKey] = serializeMetafieldDefinitionRoot(field, variables);
        break;
      case 'metafieldDefinitions':
        data[responseKey] = serializeMetafieldDefinitionsConnection(field, variables);
        break;
      default:
        data[responseKey] = null;
        break;
    }
  }

  return { data };
}

export function handleMetafieldDefinitionMutation(
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const responseKey = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'metafieldDefinitionPin':
        data[responseKey] = serializeDefinitionPinRoot(field, variables);
        break;
      case 'metafieldDefinitionUnpin':
        data[responseKey] = serializeDefinitionUnpinRoot(field, variables);
        break;
      default:
        data[responseKey] = null;
        break;
    }
  }

  return { data };
}
