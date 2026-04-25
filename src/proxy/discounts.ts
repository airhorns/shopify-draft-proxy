import { Kind, type ArgumentNode, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import type { JsonValue } from '../json-schemas.js';
import {
  matchesSearchQueryDate,
  matchesSearchQueryNumber,
  normalizeSearchQueryValue,
  parseSearchQueryTerms,
  type SearchQueryTerm,
} from '../search-query-parser.js';
import { compareNullableStrings, compareShopifyResourceIds } from '../shopify/resource-ids.js';
import { store } from '../state/store.js';
import type {
  DiscountContextRecord,
  DiscountCustomerGetsRecord,
  DiscountEventRecord,
  DiscountItemsRecord,
  DiscountMetafieldRecord,
  DiscountMinimumRequirementRecord,
  DiscountRecord,
  DiscountValueRecord,
} from '../state/types.js';
import {
  getFieldResponseKey,
  getSelectedChildFields,
  paginateConnectionItems,
  serializeConnectionPageInfo,
  serializeEmptyConnectionPageInfo,
} from './graphql-helpers.js';

type DiscountQueryIssue = {
  message: string;
  path: string[];
  extensions: {
    code: string;
    fieldName?: string;
    typeName?: string;
  };
};

type DiscountSerializationContext = {
  errors: DiscountQueryIssue[];
  path: string[];
};

type DiscountMutationUserError = {
  field: string[] | null;
  message: string;
  code: string | null;
  extraInfo: string | null;
};

const discountMutationArgumentTypes: Record<string, Record<string, string>> = {
  discountAutomaticBasicCreate: {
    automaticBasicDiscount: 'DiscountAutomaticBasicInput!',
  },
  discountAutomaticBxgyCreate: {
    automaticBxgyDiscount: 'DiscountAutomaticBxgyInput!',
  },
  discountAutomaticFreeShippingCreate: {
    freeShippingAutomaticDiscount: 'DiscountAutomaticFreeShippingInput!',
  },
  discountCodeBasicCreate: {
    basicCodeDiscount: 'DiscountCodeBasicInput!',
  },
  discountCodeBasicUpdate: {
    basicCodeDiscount: 'DiscountCodeBasicInput!',
    id: 'ID!',
  },
  discountCodeBxgyCreate: {
    bxgyCodeDiscount: 'DiscountCodeBxgyInput!',
  },
  discountCodeFreeShippingCreate: {
    freeShippingCodeDiscount: 'DiscountCodeFreeShippingInput!',
  },
};

const discountMutationNodeFieldByRoot: Record<string, string> = {
  discountAutomaticBasicCreate: 'automaticDiscountNode',
  discountAutomaticBxgyCreate: 'automaticDiscountNode',
  discountAutomaticFreeShippingCreate: 'automaticDiscountNode',
  discountCodeBasicCreate: 'codeDiscountNode',
  discountCodeBasicUpdate: 'codeDiscountNode',
  discountCodeBxgyCreate: 'codeDiscountNode',
  discountCodeFreeShippingCreate: 'codeDiscountNode',
};

function readRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readNestedRecord(value: Record<string, unknown> | null, key: string): Record<string, unknown> | null {
  return value ? readRecord(value[key]) : null;
}

function readStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

function findArgument(field: FieldNode, argumentName: string): ArgumentNode | null {
  return field.arguments?.find((argument) => argument.name.value === argumentName) ?? null;
}

function buildInvalidVariableError(variableName: string, typeName: string): Record<string, unknown> {
  return {
    message: `Variable $${variableName} of type ${typeName} was provided invalid value`,
    extensions: {
      code: 'INVALID_VARIABLE',
      value: null,
      problems: [
        {
          path: [],
          explanation: 'Expected value to not be null',
        },
      ],
    },
  };
}

function buildNullArgumentError(rootName: string, argumentName: string, typeName: string): Record<string, unknown> {
  return {
    message: `Argument '${argumentName}' on Field '${rootName}' has an invalid value (null). Expected type '${typeName}'.`,
    path: ['mutation', rootName, argumentName],
    extensions: {
      code: 'argumentLiteralsIncompatible',
      typeName: 'Field',
      argumentName,
    },
  };
}

function buildMissingArgumentError(rootName: string, argumentName: string): Record<string, unknown> {
  return {
    message: `Field '${rootName}' is missing required arguments: ${argumentName}`,
    path: ['mutation', rootName],
    extensions: {
      code: 'missingRequiredArguments',
      className: 'Field',
      name: rootName,
      arguments: argumentName,
    },
  };
}

function validateRequiredArgument(
  field: FieldNode,
  variables: Record<string, unknown>,
  argumentName: string,
  typeName: string,
): Record<string, unknown> | null {
  const argument = findArgument(field, argumentName);
  if (!argument) {
    return buildMissingArgumentError(field.name.value, argumentName);
  }

  if (argument.value.kind === Kind.NULL) {
    return buildNullArgumentError(field.name.value, argumentName, typeName);
  }

  if (argument.value.kind === Kind.VARIABLE) {
    const variableName = argument.value.name.value;
    if (variables[variableName] === null || variables[variableName] === undefined) {
      return buildInvalidVariableError(variableName, typeName);
    }
  }

  return null;
}

function serializeDiscountMutationUserErrors(
  selection: FieldNode,
  userErrors: DiscountMutationUserError[],
): Array<Record<string, unknown>> {
  return userErrors.map((userError) => {
    const result: Record<string, unknown> = {};
    for (const child of getSelectedChildFields(selection)) {
      const key = getFieldResponseKey(child);
      switch (child.name.value) {
        case 'field':
          result[key] = userError.field;
          break;
        case 'message':
          result[key] = userError.message;
          break;
        case 'code':
          result[key] = userError.code;
          break;
        case 'extraInfo':
          result[key] = userError.extraInfo;
          break;
        default:
          result[key] = null;
          break;
      }
    }
    return result;
  });
}

function serializeDiscountMutationPayload(
  field: FieldNode,
  nodeField: string | null,
  userErrors: DiscountMutationUserError[],
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    if (selection.name.value === 'userErrors') {
      payload[key] = serializeDiscountMutationUserErrors(selection, userErrors);
    } else if (selection.name.value === nodeField || selection.name.value === 'job') {
      payload[key] = null;
    } else {
      payload[key] = null;
    }
  }
  return payload;
}

function hasDateRangeError(input: Record<string, unknown> | null): boolean {
  const startsAt = typeof input?.['startsAt'] === 'string' ? input['startsAt'] : null;
  const endsAt = typeof input?.['endsAt'] === 'string' ? input['endsAt'] : null;
  return startsAt !== null && endsAt !== null && Date.parse(endsAt) <= Date.parse(startsAt);
}

function listInvalidIds(ids: string[], resourceName: string): DiscountMutationUserError[] {
  return ids
    .filter((id) => id.endsWith('/0'))
    .map((id) => ({
      field: ['basicCodeDiscount', 'customerGets', 'items', 'products', resourceName],
      message:
        resourceName === 'productsToAdd'
          ? `Product with id: ${id.split('/').at(-1)} is invalid`
          : `Product variant with id: ${id.split('/').at(-1)} is invalid`,
      code: 'INVALID',
      extraInfo: null,
    }));
}

function validateDiscountCodeBasicCreate(input: Record<string, unknown> | null): DiscountMutationUserError[] | null {
  if (!input) {
    return null;
  }

  if (hasDateRangeError(input)) {
    return [
      {
        field: ['basicCodeDiscount', 'endsAt'],
        message: 'Ends at needs to be after starts_at',
        code: 'INVALID',
        extraInfo: null,
      },
    ];
  }

  const code = typeof input['code'] === 'string' ? input['code'] : null;
  if (
    code &&
    store
      .listEffectiveDiscounts()
      .some((discount) => discount.method === 'code' && getDiscountCodes(discount).some((entry) => entry.code === code))
  ) {
    return [
      {
        field: ['basicCodeDiscount', 'code'],
        message: 'Code must be unique. Please try a different code.',
        code: 'TAKEN',
        extraInfo: null,
      },
    ];
  }

  const customerGets = readNestedRecord(input, 'customerGets');
  const items = readNestedRecord(customerGets, 'items');
  const collections = readNestedRecord(items, 'collections');
  const products = readNestedRecord(items, 'products');
  const productIds = readStringArray(products?.['productsToAdd']);
  const variantIds = readStringArray(products?.['productVariantsToAdd']);
  const collectionIds = readStringArray(collections?.['add']);
  const hasProductSelections = productIds.length > 0 || variantIds.length > 0;
  const userErrors: DiscountMutationUserError[] = [];

  if (collectionIds.length > 0 && hasProductSelections) {
    userErrors.push({
      field: ['basicCodeDiscount', 'customerGets', 'items', 'collections', 'add'],
      message: 'Cannot entitle collections in combination with product variants or products',
      code: 'CONFLICT',
      extraInfo: null,
    });
  }

  userErrors.push(...listInvalidIds(productIds, 'productsToAdd'));
  userErrors.push(...listInvalidIds(variantIds, 'productVariantsToAdd'));

  return userErrors.length > 0 ? userErrors : null;
}

function validateDiscountAutomaticBasicCreate(
  input: Record<string, unknown> | null,
): DiscountMutationUserError[] | null {
  if (!hasDateRangeError(input)) {
    return null;
  }

  return [
    {
      field: ['automaticBasicDiscount', 'endsAt'],
      message: 'Ends at needs to be after starts_at',
      code: 'INVALID',
      extraInfo: null,
    },
  ];
}

function validateDiscountBxgyCreate(
  input: Record<string, unknown> | null,
  argumentName: 'bxgyCodeDiscount' | 'automaticBxgyDiscount',
): DiscountMutationUserError[] | null {
  if (!input || input['title'] !== '') {
    return null;
  }

  return [
    {
      field: [argumentName, 'customerGets'],
      message: "Items in 'customer get' cannot be set to all",
      code: 'INVALID',
      extraInfo: null,
    },
    {
      field: [argumentName, 'title'],
      message: "Title can't be blank",
      code: 'BLANK',
      extraInfo: null,
    },
    {
      field: [argumentName, 'customerBuys', 'items'],
      message: "Items in 'customer buys' must be defined",
      code: 'BLANK',
      extraInfo: null,
    },
  ];
}

function validateDiscountFreeShippingCreate(
  input: Record<string, unknown> | null,
  argumentName: 'freeShippingCodeDiscount' | 'freeShippingAutomaticDiscount',
): DiscountMutationUserError[] | null {
  const combinesWith = readNestedRecord(input, 'combinesWith');
  const invalidCombinesWith =
    combinesWith?.['productDiscounts'] === true &&
    combinesWith['orderDiscounts'] === true &&
    combinesWith['shippingDiscounts'] === true;
  if (!invalidCombinesWith && input?.['title'] !== '') {
    return null;
  }

  const userErrors: DiscountMutationUserError[] = [];
  if (invalidCombinesWith) {
    userErrors.push({
      field: [argumentName, 'combinesWith'],
      message: 'The combinesWith settings are not valid for the discount class.',
      code: 'INVALID_COMBINES_WITH_FOR_DISCOUNT_CLASS',
      extraInfo: null,
    });
  }

  if (argumentName === 'freeShippingCodeDiscount' && input?.['title'] === '') {
    userErrors.push({
      field: [argumentName, 'title'],
      message: "Title can't be blank",
      code: 'BLANK',
      extraInfo: null,
    });
  }

  return userErrors.length > 0 ? userErrors : null;
}

function validateDiscountCodeBasicUpdate(
  id: unknown,
  input: Record<string, unknown> | null,
): DiscountMutationUserError[] | null {
  if (typeof id !== 'string' || !input || store.listEffectiveDiscounts().some((discount) => discount.id === id)) {
    return null;
  }

  return [
    {
      field: ['id'],
      message: 'Discount does not exist',
      code: null,
      extraInfo: null,
    },
  ];
}

function validateBulkSelectorConflict(
  args: Record<string, unknown>,
  message: string,
): DiscountMutationUserError[] | null {
  const presentSelectors = [args['ids'], args['search'], args['savedSearchId']].filter((value) => {
    if (Array.isArray(value)) {
      return value.length > 0;
    }
    return value !== null && value !== undefined;
  });

  if (presentSelectors.length <= 1) {
    return null;
  }

  return [
    {
      field: null,
      message,
      code: 'TOO_MANY_ARGUMENTS',
      extraInfo: null,
    },
  ];
}

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

function findDiscountById(id: unknown): DiscountRecord | null {
  if (typeof id !== 'string' || id.length === 0) {
    return null;
  }

  return store.listEffectiveDiscounts().find((discount) => discount.id === id) ?? null;
}

function getDiscountCodes(
  discount: DiscountRecord,
): Array<{ code: string; id: string; asyncUsageCount: number | null }> {
  const redeemCodes = discount.redeemCodes ?? [];
  if (redeemCodes.length > 0) {
    return redeemCodes.map((code) => ({
      code: code.code,
      id: code.id,
      asyncUsageCount: code.asyncUsageCount,
    }));
  }

  return discount.codes.map((code) => ({
    code,
    id: `gid://shopify/DiscountRedeemCode/${discount.id.split('/').at(-1) ?? code}`,
    asyncUsageCount: 0,
  }));
}

function findCodeDiscountByCode(code: unknown): DiscountRecord | null {
  if (typeof code !== 'string' || code.length === 0) {
    return null;
  }

  return (
    store
      .listEffectiveDiscounts()
      .find(
        (discount) => discount.method === 'code' && getDiscountCodes(discount).some((entry) => entry.code === code),
      ) ?? null
  );
}

function selectedFieldsForConcreteType(field: FieldNode, typeName: string): FieldNode[] {
  return (field.selectionSet?.selections ?? []).flatMap((selection: SelectionNode) => {
    if (selection.kind === Kind.FIELD) {
      return [selection];
    }

    if (selection.kind === Kind.INLINE_FRAGMENT && selection.typeCondition?.name.value === typeName) {
      return selection.selectionSet.selections.filter(
        (inlineSelection): inlineSelection is FieldNode => inlineSelection.kind === Kind.FIELD,
      );
    }

    return [];
  });
}

function serializeMoney(
  money: { amount: string; currencyCode: string } | null | undefined,
  field: FieldNode,
): Record<string, unknown> | null {
  if (!money) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'amount':
        result[key] = money.amount;
        break;
      case 'currencyCode':
        result[key] = money.currencyCode;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCount(count: number, field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'count':
        result[key] = count;
        break;
      case 'precision':
        result[key] = 'EXACT';
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCodesConnection(
  discount: DiscountRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const codes = getDiscountCodes(discount);
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

function serializeCodeNode(
  code: { code: string; id: string; asyncUsageCount: number | null },
  field: FieldNode,
): Record<string, unknown> {
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
      case 'asyncUsageCount':
        result[key] = code.asyncUsageCount ?? 0;
        break;
      case 'createdBy':
        result[key] = null;
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

function serializeResourceNode(
  id: string,
  field: FieldNode,
  resourceType: 'product' | 'variant' | 'collection' | 'customer' | 'segment',
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const product = resourceType === 'product' ? store.getEffectiveProductById(id) : null;
  const variant = resourceType === 'variant' ? store.getEffectiveVariantById(id) : null;
  const collection = resourceType === 'collection' ? store.getEffectiveCollectionById(id) : null;
  const customer = resourceType === 'customer' ? store.getEffectiveCustomerById(id) : null;

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = id;
        break;
      case 'title':
        result[key] = product?.title ?? variant?.title ?? collection?.title ?? null;
        break;
      case 'displayName':
        result[key] = customer?.displayName ?? null;
        break;
      case 'name':
        result[key] = null;
        break;
      case '__typename':
        result[key] =
          resourceType === 'product'
            ? 'Product'
            : resourceType === 'variant'
              ? 'ProductVariant'
              : resourceType === 'collection'
                ? 'Collection'
                : resourceType === 'customer'
                  ? 'Customer'
                  : 'Segment';
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializeIdConnection(
  ids: string[],
  field: FieldNode,
  variables: Record<string, unknown>,
  resourceType: 'product' | 'variant' | 'collection' | 'customer' | 'segment',
): Record<string, unknown> {
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(ids, field, variables, (id) => id);
  const connection: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        connection[key] = items.map((id) => serializeResourceNode(id, selection, resourceType));
        break;
      case 'edges':
        connection[key] = items.map((id) => {
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            if (edgeSelection.name.value === 'cursor') {
              edge[edgeKey] = `cursor:${id}`;
            } else if (edgeSelection.name.value === 'node') {
              edge[edgeKey] = serializeResourceNode(id, edgeSelection, resourceType);
            } else {
              edge[edgeKey] = null;
            }
          }
          return edge;
        });
        break;
      case 'pageInfo':
        connection[key] = serializeConnectionPageInfo(selection, items, hasNextPage, hasPreviousPage, (id) => id);
        break;
      default:
        connection[key] = null;
        break;
    }
  }

  return connection;
}

function serializeDiscountContext(
  context: DiscountContextRecord | null | undefined,
  field: FieldNode,
): Record<string, unknown> | null {
  if (!context) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of selectedFieldsForConcreteType(field, context.typeName)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = context.typeName;
        break;
      case 'all':
        result[key] = context.all ?? 'ALL';
        break;
      case 'customers':
        result[key] = (context.customerIds ?? []).map((id) => serializeResourceNode(id, selection, 'customer'));
        break;
      case 'segments':
        result[key] = (context.customerSegmentIds ?? []).map((id) => serializeResourceNode(id, selection, 'segment'));
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDiscountItems(
  items: DiscountItemsRecord | null | undefined,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  if (!items) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of selectedFieldsForConcreteType(field, items.typeName)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = items.typeName;
        break;
      case 'allItems':
        result[key] = items.allItems ?? true;
        break;
      case 'products':
        result[key] = serializeIdConnection(items.productIds ?? [], selection, variables, 'product');
        break;
      case 'productVariants':
        result[key] = serializeIdConnection(items.productVariantIds ?? [], selection, variables, 'variant');
        break;
      case 'collections':
        result[key] = serializeIdConnection(items.collectionIds ?? [], selection, variables, 'collection');
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDiscountValue(
  value: DiscountValueRecord | null | undefined,
  field: FieldNode,
): Record<string, unknown> | null {
  if (!value) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of selectedFieldsForConcreteType(field, value.typeName)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = value.typeName;
        break;
      case 'percentage':
        result[key] = value.percentage ?? null;
        break;
      case 'amount':
        result[key] = serializeMoney(value.amount, selection);
        break;
      case 'appliesOnEachItem':
        result[key] = value.appliesOnEachItem ?? null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCustomerGets(
  customerGets: DiscountCustomerGetsRecord | null | undefined,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  if (!customerGets) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'value':
        result[key] = serializeDiscountValue(customerGets.value, selection);
        break;
      case 'items':
        result[key] = serializeDiscountItems(customerGets.items, selection, variables);
        break;
      case 'appliesOnOneTimePurchase':
        result[key] = customerGets.appliesOnOneTimePurchase;
        break;
      case 'appliesOnSubscription':
        result[key] = customerGets.appliesOnSubscription;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeMinimumRequirement(
  requirement: DiscountMinimumRequirementRecord | null | undefined,
  field: FieldNode,
): Record<string, unknown> | null {
  if (!requirement) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of selectedFieldsForConcreteType(field, requirement.typeName)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = requirement.typeName;
        break;
      case 'greaterThanOrEqualToQuantity':
        result[key] = requirement.greaterThanOrEqualToQuantity ?? null;
        break;
      case 'greaterThanOrEqualToSubtotal':
        result[key] = serializeMoney(requirement.greaterThanOrEqualToSubtotal, selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeMetafield(metafield: DiscountMetafieldRecord, field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
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
        result[key] = metafield.compareDigest ?? null;
        break;
      case 'jsonValue':
        result[key] = metafield.jsonValue ?? null;
        break;
      case 'createdAt':
        result[key] = metafield.createdAt ?? null;
        break;
      case 'updatedAt':
        result[key] = metafield.updatedAt ?? null;
        break;
      case 'ownerType':
        result[key] = metafield.ownerType ?? 'DISCOUNT';
        break;
      case 'definition':
      case 'reference':
      case 'references':
        result[key] = null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeMetafieldsConnection(
  metafields: DiscountMetafieldRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    metafields,
    field,
    variables,
    (item) => item.id,
  );
  const connection: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        connection[key] = items.map((metafield) => serializeMetafield(metafield, selection));
        break;
      case 'edges':
        connection[key] = items.map((metafield) => {
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            if (edgeSelection.name.value === 'cursor') {
              edge[edgeKey] = `cursor:${metafield.id}`;
            } else if (edgeSelection.name.value === 'node') {
              edge[edgeKey] = serializeMetafield(metafield, edgeSelection);
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
          (item) => item.id,
        );
        break;
      default:
        connection[key] = null;
        break;
    }
  }

  return connection;
}

function isRecord(value: JsonValue | undefined): value is Record<string, JsonValue> {
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
  if (selectedFields.length === 0 || !isRecord(value)) {
    return value;
  }

  const result: Record<string, unknown> = {};
  for (const selection of selectedFields) {
    const key = getFieldResponseKey(selection);
    result[key] = serializeCapturedJsonValue(value[selection.name.value], selection);
  }

  return result;
}

function serializeEvent(event: DiscountEventRecord, field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selectedFieldsForConcreteType(field, event.typeName)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = event.typeName;
        break;
      case 'id':
        result[key] = event.id;
        break;
      case 'action':
        result[key] = event.action ?? null;
        break;
      case 'message':
        result[key] = event.message ?? null;
        break;
      case 'createdAt':
        result[key] = event.createdAt ?? null;
        break;
      case 'subjectId':
        result[key] = event.subjectId ?? null;
        break;
      case 'subjectType':
        result[key] = event.subjectType ?? null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeEventsConnection(
  events: DiscountEventRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(events, field, variables, (item) => item.id);
  const connection: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        connection[key] = items.map((event) => serializeEvent(event, selection));
        break;
      case 'edges':
        connection[key] = items.map((event) => {
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            if (edgeSelection.name.value === 'cursor') {
              edge[edgeKey] = `cursor:${event.id}`;
            } else if (edgeSelection.name.value === 'node') {
              edge[edgeKey] = serializeEvent(event, edgeSelection);
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
          (item) => item.id,
        );
        break;
      default:
        connection[key] = null;
        break;
    }
  }

  return connection;
}

function serializeEmptyConnection(field: FieldNode): Record<string, unknown> {
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

function serializeDiscountUnion(
  discount: DiscountRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
  context: DiscountSerializationContext,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selectedFieldsForConcreteType(field, discount.typeName)) {
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
      case 'shortSummary':
        result[key] = discount.summary ?? '';
        break;
      case 'usageLimit':
      case 'recurringCycleLimit':
        result[key] = null;
        break;
      case 'tags':
        result[key] = [];
        break;
      case 'totalSales':
        result[key] = null;
        break;
      case 'hasTimelineComment':
        result[key] = false;
        break;
      case 'discountClasses':
        result[key] = structuredClone(discount.discountClasses);
        break;
      case 'combinesWith':
        result[key] = serializeCombinesWith(discount, selection);
        break;
      case 'appDiscountType':
        result[key] = serializeCapturedJsonValue(discount.appDiscountType, selection);
        if (
          result[key] === null &&
          (discount.typeName === 'DiscountCodeApp' || discount.typeName === 'DiscountAutomaticApp')
        ) {
          context.errors.push({
            message: `Local discount detail does not have captured app-managed field ${selection.name.value}.`,
            path: [...context.path, key],
            extensions: {
              code: 'UNSUPPORTED_APP_DISCOUNT_FIELD',
              fieldName: selection.name.value,
              typeName: discount.typeName,
            },
          });
        }
        break;
      case 'discountId':
        result[key] = discount.discountId ?? null;
        if (
          result[key] === null &&
          (discount.typeName === 'DiscountCodeApp' || discount.typeName === 'DiscountAutomaticApp')
        ) {
          context.errors.push({
            message: `Local discount detail does not have captured app-managed field ${selection.name.value}.`,
            path: [...context.path, key],
            extensions: {
              code: 'UNSUPPORTED_APP_DISCOUNT_FIELD',
              fieldName: selection.name.value,
              typeName: discount.typeName,
            },
          });
        }
        break;
      case 'errorHistory':
        result[key] = serializeCapturedJsonValue(discount.errorHistory, selection);
        break;
      case 'codes':
        result[key] = serializeCodesConnection(discount, selection, variables);
        break;
      case 'codesCount':
        result[key] = serializeCount(getDiscountCodes(discount).length, selection);
        break;
      case 'context':
        result[key] = serializeDiscountContext(discount.context, selection);
        break;
      case 'customerGets':
        result[key] = serializeCustomerGets(discount.customerGets, selection, variables);
        break;
      case 'minimumRequirement':
        result[key] = serializeMinimumRequirement(discount.minimumRequirement, selection);
        break;
      case 'shareableUrls':
        result[key] = [];
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
  context: DiscountSerializationContext,
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
        result[key] = serializeDiscountUnion(discount, selection, variables, {
          ...context,
          path: [...context.path, key],
        });
        break;
      case 'metafield':
        result[key] = null;
        break;
      case 'metafields':
        result[key] =
          (discount.metafields ?? []).length > 0
            ? serializeMetafieldsConnection(discount.metafields ?? [], selection, variables)
            : serializeEmptyConnection(selection);
        break;
      case 'events':
        result[key] =
          (discount.events ?? []).length > 0
            ? serializeEventsConnection(discount.events ?? [], selection, variables)
            : serializeEmptyConnection(selection);
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
  context: DiscountSerializationContext,
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
        connection[key] = items.map((discount, index) =>
          serializeDiscountNode(discount, selection, variables, {
            ...context,
            path: [...context.path, key, String(index)],
          }),
        );
        break;
      case 'edges':
        connection[key] = items.map((discount, index) => {
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edge[edgeKey] = `cursor:${discount.id}`;
                break;
              case 'node':
                edge[edgeKey] = serializeDiscountNode(discount, edgeSelection, variables, {
                  ...context,
                  path: [...context.path, key, String(index), edgeKey],
                });
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

function serializeDiscountOwnerNode(
  discount: DiscountRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
  nodeTypeName: 'DiscountCodeNode' | 'DiscountAutomaticNode',
  unionFieldName: 'codeDiscount' | 'automaticDiscount',
  context: DiscountSerializationContext,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = discount.id;
        break;
      case '__typename':
        result[key] = nodeTypeName;
        break;
      case 'codeDiscount':
      case 'automaticDiscount':
        if (selection.name.value === unionFieldName) {
          result[key] = serializeDiscountUnion(discount, selection, variables, {
            ...context,
            path: [...context.path, key],
          });
        } else {
          result[key] = null;
        }
        break;
      case 'metafield':
        result[key] = null;
        break;
      case 'metafields':
        result[key] =
          (discount.metafields ?? []).length > 0
            ? serializeMetafieldsConnection(discount.metafields ?? [], selection, variables)
            : serializeEmptyConnection(selection);
        break;
      case 'events':
        result[key] =
          (discount.events ?? []).length > 0
            ? serializeEventsConnection(discount.events ?? [], selection, variables)
            : serializeEmptyConnection(selection);
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
  const context: DiscountSerializationContext = { errors: [], path: [] };

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);
    switch (field.name.value) {
      case 'discountNodes':
        data[key] = serializeDiscountNodesConnection(field, variables, { ...context, path: [key] });
        break;
      case 'discountNodesCount':
        data[key] = serializeDiscountNodesCount(field, variables);
        break;
      case 'discountNode': {
        const discount = findDiscountById(args['id']);
        data[key] = discount ? serializeDiscountNode(discount, field, variables, { ...context, path: [key] }) : null;
        break;
      }
      case 'codeDiscountNode': {
        const discount = findDiscountById(args['id']);
        data[key] =
          discount?.method === 'code'
            ? serializeDiscountOwnerNode(discount, field, variables, 'DiscountCodeNode', 'codeDiscount', {
                ...context,
                path: [key],
              })
            : null;
        break;
      }
      case 'codeDiscountNodeByCode': {
        const discount = findCodeDiscountByCode(args['code']);
        data[key] = discount
          ? serializeDiscountOwnerNode(discount, field, variables, 'DiscountCodeNode', 'codeDiscount', {
              ...context,
              path: [key],
            })
          : null;
        break;
      }
      case 'automaticDiscountNode': {
        const discount = findDiscountById(args['id']);
        data[key] =
          discount?.method === 'automatic'
            ? serializeDiscountOwnerNode(discount, field, variables, 'DiscountAutomaticNode', 'automaticDiscount', {
                ...context,
                path: [key],
              })
            : null;
        break;
      }
      default:
        data[key] = null;
        break;
    }
  }

  return context.errors.length > 0 ? { data, errors: context.errors } : { data };
}

export function handleDiscountMutation(
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  const data: Record<string, unknown> = {};
  let handled = false;

  for (const field of getRootFields(document)) {
    const rootName = field.name.value;
    const key = getFieldResponseKey(field);
    const requiredArguments = discountMutationArgumentTypes[rootName] ?? {};
    for (const [argumentName, typeName] of Object.entries(requiredArguments)) {
      const validationError = validateRequiredArgument(field, variables, argumentName, typeName);
      if (validationError) {
        return { errors: [validationError] };
      }
    }

    const args = getFieldArguments(field, variables);
    const nodeField = discountMutationNodeFieldByRoot[rootName] ?? null;
    let userErrors: DiscountMutationUserError[] | null = null;

    switch (rootName) {
      case 'discountCodeBasicCreate':
        userErrors = validateDiscountCodeBasicCreate(readRecord(args['basicCodeDiscount']));
        break;
      case 'discountAutomaticBasicCreate':
        userErrors = validateDiscountAutomaticBasicCreate(readRecord(args['automaticBasicDiscount']));
        break;
      case 'discountCodeBasicUpdate':
        userErrors = validateDiscountCodeBasicUpdate(args['id'], readRecord(args['basicCodeDiscount']));
        break;
      case 'discountCodeBxgyCreate':
        userErrors = validateDiscountBxgyCreate(readRecord(args['bxgyCodeDiscount']), 'bxgyCodeDiscount');
        break;
      case 'discountAutomaticBxgyCreate':
        userErrors = validateDiscountBxgyCreate(readRecord(args['automaticBxgyDiscount']), 'automaticBxgyDiscount');
        break;
      case 'discountCodeFreeShippingCreate':
        userErrors = validateDiscountFreeShippingCreate(
          readRecord(args['freeShippingCodeDiscount']),
          'freeShippingCodeDiscount',
        );
        break;
      case 'discountAutomaticFreeShippingCreate':
        userErrors = validateDiscountFreeShippingCreate(
          readRecord(args['freeShippingAutomaticDiscount']),
          'freeShippingAutomaticDiscount',
        );
        break;
      case 'discountCodeBulkDeactivate':
        userErrors = validateBulkSelectorConflict(args, "Only one of 'ids', 'search' or 'saved_search_id' is allowed.");
        break;
      case 'discountAutomaticBulkDelete':
        userErrors = validateBulkSelectorConflict(
          args,
          'Only one of IDs, search argument or saved search ID is allowed.',
        );
        break;
      default:
        break;
    }

    if (userErrors === null) {
      continue;
    }

    handled = true;
    data[key] = serializeDiscountMutationPayload(field, nodeField, userErrors);
  }

  return handled ? { data } : null;
}
