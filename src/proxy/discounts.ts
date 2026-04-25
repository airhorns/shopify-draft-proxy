import { Kind, type FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import {
  matchesSearchQueryDate,
  matchesSearchQueryNumber,
  normalizeSearchQueryValue,
  parseSearchQueryTerms,
  type SearchQueryTerm,
} from '../search-query-parser.js';
import { compareNullableStrings, compareShopifyResourceIds } from '../shopify/resource-ids.js';
import { store } from '../state/store.js';
import type { DiscountRecord } from '../state/types.js';
import {
  getFieldResponseKey,
  getSelectedChildFields,
  paginateConnectionItems,
  serializeConnectionPageInfo,
} from './graphql-helpers.js';

function parseDiscountQuery(rawQuery: unknown): SearchQueryTerm[] {
  if (typeof rawQuery !== 'string' || rawQuery.trim().length === 0) {
    return [];
  }

  return parseSearchQueryTerms(rawQuery.trim(), { ignoredKeywords: ['AND'] }).filter(
    (term) => normalizeSearchQueryValue(term.value).length > 0,
  );
}

function inferDiscountType(discount: DiscountRecord): string | null {
  if (discount.discountType) {
    return discount.discountType.toLowerCase();
  }

  if (discount.typeName.toLowerCase().includes('freeshipping')) {
    return 'free_shipping';
  }

  if (discount.typeName.toLowerCase().includes('bxgy')) {
    return 'bogo';
  }

  if (discount.summary?.includes('%')) {
    return 'percentage';
  }

  return null;
}

function matchesPositiveDiscountTerm(discount: DiscountRecord, term: SearchQueryTerm): boolean {
  const field = term.field?.toLowerCase() ?? 'default';
  const value = normalizeSearchQueryValue(term.value);

  switch (field) {
    case 'default':
    case 'title':
      return discount.title.toLowerCase().includes(value);
    case 'code':
      // Captured 2026-04 behavior: native code discounts found through codeDiscountNodes
      // did not match discountNodes(query: "code:<code>").
      return false;
    case 'combines_with':
      if (value === 'product_discounts') return discount.combinesWith.productDiscounts;
      if (value === 'order_discounts') return discount.combinesWith.orderDiscounts;
      if (value === 'shipping_discounts') return discount.combinesWith.shippingDiscounts;
      return false;
    case 'discount_class':
      return discount.discountClasses.some((discountClass) => discountClass.toLowerCase() === value);
    case 'discount_type':
    case 'type': {
      if (value === 'all') return true;
      if (value === 'all_with_app') return true;
      if (value === 'app') return discount.typeName.toLowerCase().includes('app');
      return inferDiscountType(discount) === value;
    }
    case 'method':
      return discount.method === value;
    case 'status':
      return discount.status?.toLowerCase() === value;
    case 'starts_at':
      return matchesSearchQueryDate(discount.startsAt, term);
    case 'ends_at':
      return matchesSearchQueryDate(discount.endsAt, term);
    case 'created_at':
      return matchesSearchQueryDate(discount.createdAt, term);
    case 'updated_at':
      return matchesSearchQueryDate(discount.updatedAt, term);
    case 'times_used':
      return matchesSearchQueryNumber(discount.asyncUsageCount, term);
    case 'app_id':
      return discount.appId?.toLowerCase() === value;
    case 'id':
      return discount.id.endsWith(`/${value}`) || discount.id === value;
    default:
      return false;
  }
}

function matchesDiscountTerm(discount: DiscountRecord, term: SearchQueryTerm): boolean {
  if (!term.raw) {
    return true;
  }

  const matches = matchesPositiveDiscountTerm(discount, term);
  return term.negated ? !matches : matches;
}

function filterDiscountsByQuery(discounts: DiscountRecord[], rawQuery: unknown): DiscountRecord[] {
  const terms = parseDiscountQuery(rawQuery);
  if (terms.length === 0) {
    return discounts;
  }

  return discounts.filter((discount) => terms.every((term) => matchesDiscountTerm(discount, term)));
}

function sortDiscounts(discounts: DiscountRecord[], rawSortKey: unknown, rawReverse: unknown): DiscountRecord[] {
  const sortKey = typeof rawSortKey === 'string' ? rawSortKey : 'ID';
  const sorted = [...discounts].sort((left, right) => {
    switch (sortKey) {
      case 'CREATED_AT':
        return compareNullableStrings(left.createdAt, right.createdAt) || compareShopifyResourceIds(left.id, right.id);
      case 'ENDS_AT':
        return compareNullableStrings(left.endsAt, right.endsAt) || compareShopifyResourceIds(left.id, right.id);
      case 'STARTS_AT':
        return compareNullableStrings(left.startsAt, right.startsAt) || compareShopifyResourceIds(left.id, right.id);
      case 'TITLE':
      case 'RELEVANCE':
        return left.title.localeCompare(right.title) || compareShopifyResourceIds(left.id, right.id);
      case 'UPDATED_AT':
        return compareNullableStrings(left.updatedAt, right.updatedAt) || compareShopifyResourceIds(left.id, right.id);
      case 'ID':
      default:
        return compareShopifyResourceIds(left.id, right.id);
    }
  });

  return rawReverse === true ? sorted.reverse() : sorted;
}

function listDiscountsForField(field: FieldNode, variables: Record<string, unknown>): DiscountRecord[] {
  const args = getFieldArguments(field, variables);
  return sortDiscounts(
    filterDiscountsByQuery(store.listEffectiveDiscounts(), args['query']),
    args['sortKey'],
    args['reverse'],
  );
}

function serializeCodesConnection(
  discount: DiscountRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const codes = discount.codes.map((code) => ({
    code,
    id: `gid://shopify/DiscountRedeemCode/${discount.id.split('/').at(-1) ?? code}`,
  }));
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(codes, field, variables, (code) => code.code);
  const connection: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        connection[key] = items.map((code) => serializeCodeNode(code, selection));
        break;
      case 'edges':
        connection[key] = items.map((code) => {
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            if (edgeSelection.name.value === 'cursor') {
              edge[edgeKey] = `cursor:${code.code}`;
            } else if (edgeSelection.name.value === 'node') {
              edge[edgeKey] = serializeCodeNode(code, edgeSelection);
            } else {
              edge[edgeKey] = null;
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
          (code) => code.code,
        );
        break;
      default:
        connection[key] = null;
        break;
    }
  }

  return connection;
}

function serializeCodeNode(code: { code: string; id: string }, field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'code':
        result[key] = code.code;
        break;
      case 'id':
        result[key] = code.id;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCombinesWith(discount: DiscountRecord, field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'productDiscounts':
        result[key] = discount.combinesWith.productDiscounts;
        break;
      case 'orderDiscounts':
        result[key] = discount.combinesWith.orderDiscounts;
        break;
      case 'shippingDiscounts':
        result[key] = discount.combinesWith.shippingDiscounts;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDiscountUnion(
  discount: DiscountRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field, { includeInlineFragments: true })) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = discount.typeName;
        break;
      case 'title':
        result[key] = discount.title;
        break;
      case 'status':
        result[key] = discount.status;
        break;
      case 'summary':
        result[key] = discount.summary;
        break;
      case 'startsAt':
        result[key] = discount.startsAt;
        break;
      case 'endsAt':
        result[key] = discount.endsAt;
        break;
      case 'createdAt':
        result[key] = discount.createdAt;
        break;
      case 'updatedAt':
        result[key] = discount.updatedAt;
        break;
      case 'asyncUsageCount':
        result[key] = discount.asyncUsageCount;
        break;
      case 'discountClasses':
        result[key] = structuredClone(discount.discountClasses);
        break;
      case 'combinesWith':
        result[key] = serializeCombinesWith(discount, selection);
        break;
      case 'codes':
        result[key] = serializeCodesConnection(discount, selection, variables);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDiscountNode(
  discount: DiscountRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = discount.id;
        break;
      case '__typename':
        result[key] = 'DiscountNode';
        break;
      case 'discount':
        result[key] = serializeDiscountUnion(discount, selection, variables);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDiscountNodesConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const discounts = listDiscountsForField(field, variables);
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    discounts,
    field,
    variables,
    (discount) => discount.id,
  );
  const connection: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        connection[key] = items.map((discount) => serializeDiscountNode(discount, selection, variables));
        break;
      case 'edges':
        connection[key] = items.map((discount) => {
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edge[edgeKey] = `cursor:${discount.id}`;
                break;
              case 'node':
                edge[edgeKey] = serializeDiscountNode(discount, edgeSelection, variables);
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
          (discount) => discount.id,
        );
        break;
      default:
        connection[key] = null;
        break;
    }
  }

  return connection;
}

function serializeDiscountNodesCount(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const discounts = filterDiscountsByQuery(store.listEffectiveDiscounts(), args['query']);
  const limit =
    args['limit'] === null ? null : typeof args['limit'] === 'number' ? Math.max(0, Math.floor(args['limit'])) : 10000;
  const count = limit === null ? discounts.length : Math.min(discounts.length, limit);
  const precision = limit !== null && discounts.length > limit ? 'AT_LEAST' : 'EXACT';
  const result: Record<string, unknown> = {};

  for (const selection of (field.selectionSet?.selections ?? []).filter(
    (candidate): candidate is FieldNode => candidate.kind === Kind.FIELD,
  )) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'count':
        result[key] = count;
        break;
      case 'precision':
        result[key] = precision;
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

export function handleDiscountQuery(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'discountNodes':
        data[key] = serializeDiscountNodesConnection(field, variables);
        break;
      case 'discountNodesCount':
        data[key] = serializeDiscountNodesCount(field, variables);
        break;
      default:
        data[key] = null;
        break;
    }
  }

  return { data };
}
