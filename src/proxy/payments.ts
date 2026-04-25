import type { FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import type { JsonValue } from '../json-schemas.js';
import { normalizeSearchQueryValue, parseSearchQueryTerms, type SearchQueryTerm } from '../search-query-parser.js';
import { store } from '../state/store.js';
import type { PaymentCustomizationRecord } from '../state/types.js';
import {
  getFieldResponseKey,
  getSelectedChildFields,
  paginateConnectionItems,
  serializeConnectionPageInfo,
  serializeEmptyConnectionPageInfo,
} from './graphql-helpers.js';
import { serializeMetafieldSelection, serializeMetafieldsConnection } from './metafields.js';

function parsePaymentCustomizationQuery(rawQuery: unknown): SearchQueryTerm[] {
  if (typeof rawQuery !== 'string' || rawQuery.trim().length === 0) {
    return [];
  }

  return parseSearchQueryTerms(rawQuery.trim(), { ignoredKeywords: ['AND'] }).filter(
    (term) => normalizeSearchQueryValue(term.value).length > 0,
  );
}

function gidTail(id: string | null | undefined): string | null {
  const tail = id?.split('/').at(-1) ?? null;
  return tail && tail.length > 0 ? tail : null;
}

function matchesIdentifier(id: string | null | undefined, expected: string): boolean {
  if (!id) {
    return false;
  }

  const normalizedExpected = normalizeSearchQueryValue(expected);
  const normalizedId = id.toLowerCase();
  return normalizedId === normalizedExpected || (gidTail(id)?.toLowerCase() ?? '') === normalizedExpected;
}

function matchesBoolean(value: boolean | null, expected: string): boolean {
  const normalizedExpected = normalizeSearchQueryValue(expected);
  if (normalizedExpected === 'true') return value === true;
  if (normalizedExpected === 'false') return value === false;
  return false;
}

function matchesPositivePaymentCustomizationTerm(
  customization: PaymentCustomizationRecord,
  term: SearchQueryTerm,
): boolean {
  const field = term.field?.toLowerCase() ?? 'default';
  const value = normalizeSearchQueryValue(term.value);

  switch (field) {
    case 'default':
    case 'title':
      return (customization.title ?? '').toLowerCase().includes(value);
    case 'enabled':
      return matchesBoolean(customization.enabled, term.value);
    case 'function_id':
      return matchesIdentifier(customization.functionId, term.value);
    case 'id':
      return matchesIdentifier(customization.id, term.value);
    default:
      return false;
  }
}

function matchesPaymentCustomizationTerm(customization: PaymentCustomizationRecord, term: SearchQueryTerm): boolean {
  if (!term.raw) {
    return true;
  }

  const matches = matchesPositivePaymentCustomizationTerm(customization, term);
  return term.negated ? !matches : matches;
}

function listPaymentCustomizationsForField(
  field: FieldNode,
  variables: Record<string, unknown>,
): PaymentCustomizationRecord[] {
  const args = getFieldArguments(field, variables);
  const terms = parsePaymentCustomizationQuery(args['query']);
  const filtered = terms.length
    ? store
        .listEffectivePaymentCustomizations()
        .filter((customization) => terms.every((term) => matchesPaymentCustomizationTerm(customization, term)))
    : store.listEffectivePaymentCustomizations();
  return args['reverse'] === true ? [...filtered].reverse() : filtered;
}

function isJsonRecord(value: unknown): value is Record<string, JsonValue> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function serializeCapturedJsonValue(value: JsonValue | undefined, field: FieldNode): unknown {
  if (value === undefined || value === null) {
    return null;
  }

  if (Array.isArray(value)) {
    return value.map((item) => serializeCapturedJsonValue(item, field));
  }

  const selectedFields = getSelectedChildFields(field);
  if (selectedFields.length === 0 || !isJsonRecord(value)) {
    return value;
  }

  const result: Record<string, unknown> = {};
  for (const selection of selectedFields) {
    const key = getFieldResponseKey(selection);
    result[key] = serializeCapturedJsonValue(value[selection.name.value], selection);
  }
  return result;
}

function serializePaymentCustomization(
  customization: PaymentCustomizationRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'PaymentCustomization';
        break;
      case 'id':
        result[key] = customization.id;
        break;
      case 'legacyResourceId':
        result[key] = gidTail(customization.id);
        break;
      case 'title':
        result[key] = customization.title;
        break;
      case 'enabled':
        result[key] = customization.enabled;
        break;
      case 'functionId':
        result[key] = customization.functionId;
        break;
      case 'shopifyFunction':
        result[key] = serializeCapturedJsonValue(customization.shopifyFunction, selection);
        break;
      case 'errorHistory':
        result[key] = serializeCapturedJsonValue(customization.errorHistory, selection);
        break;
      case 'metafield': {
        const args = getFieldArguments(selection, variables);
        const namespace = typeof args['namespace'] === 'string' ? args['namespace'] : null;
        const metafieldKey = typeof args['key'] === 'string' ? args['key'] : null;
        const metafield =
          namespace && metafieldKey
            ? (customization.metafields ?? []).find(
                (candidate) => candidate.namespace === namespace && candidate.key === metafieldKey,
              )
            : null;
        result[key] = metafield ? serializeMetafieldSelection(metafield, selection) : null;
        break;
      }
      case 'metafields':
        result[key] = serializeMetafieldsConnection(customization.metafields ?? [], selection, variables);
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializePaymentCustomizationsConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const customizations = listPaymentCustomizationsForField(field, variables);
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    customizations,
    field,
    variables,
    (customization) => customization.id,
  );
  const connection: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        connection[key] = items.map((customization) =>
          serializePaymentCustomization(customization, selection, variables),
        );
        break;
      case 'edges':
        connection[key] = items.map((customization) => {
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edge[edgeKey] = `cursor:${customization.id}`;
                break;
              case 'node':
                edge[edgeKey] = serializePaymentCustomization(customization, edgeSelection, variables);
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
        connection[key] = serializeConnectionPageInfo(
          selection,
          items,
          hasNextPage,
          hasPreviousPage,
          (customization) => customization.id,
        );
        break;
      default:
        connection[key] = null;
        break;
    }
  }

  return connection;
}

function serializeEmptyPaymentCustomizationsConnection(field: FieldNode): Record<string, unknown> {
  const connection: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
      case 'edges':
        connection[key] = [];
        break;
      case 'pageInfo':
        connection[key] = serializeEmptyConnectionPageInfo(selection);
        break;
      default:
        connection[key] = null;
        break;
    }
  }
  return connection;
}

export function handlePaymentQuery(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);
    switch (field.name.value) {
      case 'paymentCustomizations':
        data[key] = store.hasPaymentCustomizations()
          ? serializePaymentCustomizationsConnection(field, variables)
          : serializeEmptyPaymentCustomizationsConnection(field);
        break;
      case 'paymentCustomization':
        data[key] =
          typeof args['id'] === 'string'
            ? (() => {
                const customization = store.getEffectivePaymentCustomizationById(args['id']);
                return customization ? serializePaymentCustomization(customization, field, variables) : null;
              })()
            : null;
        break;
      default:
        data[key] = null;
        break;
    }
  }

  return { data };
}
