import { Kind, type ArgumentNode, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import type { JsonValue } from '../json-schemas.js';
import {
  applySearchQueryTerms,
  matchesSearchQueryDate,
  matchesSearchQueryNumber,
  normalizeSearchQueryValue,
  type SearchQueryTerm,
} from '../search-query-parser.js';
import { compareNullableStrings, compareShopifyResourceIds } from '../shopify/resource-ids.js';
import { makeProxySyntheticGid, makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type {
  DiscountBulkOperationRecord,
  DiscountCombinesWithRecord,
  DiscountContextRecord,
  DiscountCustomerBuysRecord,
  DiscountCustomerGetsRecord,
  DiscountEffectRecord,
  DiscountDestinationSelectionRecord,
  DiscountEventRecord,
  DiscountItemsRecord,
  DiscountMetafieldRecord,
  DiscountMinimumRequirementRecord,
  DiscountRecord,
  DiscountValueRecord,
  ShopifyFunctionRecord,
} from '../state/types.js';
import {
  getFieldResponseKey,
  getSelectedChildFields,
  paginateConnectionItems,
  serializeConnection,
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

export type DiscountMutationHandling = {
  response: Record<string, unknown>;
  stagedResourceIds: string[];
  notes: string | null;
  staged: boolean;
};

const discountMutationArgumentTypes: Record<string, Record<string, string>> = {
  discountAutomaticActivate: {
    id: 'ID!',
  },
  discountAutomaticBasicCreate: {
    automaticBasicDiscount: 'DiscountAutomaticBasicInput!',
  },
  discountAutomaticBasicUpdate: {
    automaticBasicDiscount: 'DiscountAutomaticBasicInput!',
    id: 'ID!',
  },
  discountAutomaticDeactivate: {
    id: 'ID!',
  },
  discountAutomaticDelete: {
    id: 'ID!',
  },
  discountAutomaticBxgyCreate: {
    automaticBxgyDiscount: 'DiscountAutomaticBxgyInput!',
  },
  discountAutomaticBxgyUpdate: {
    automaticBxgyDiscount: 'DiscountAutomaticBxgyInput!',
    id: 'ID!',
  },
  discountAutomaticFreeShippingCreate: {
    freeShippingAutomaticDiscount: 'DiscountAutomaticFreeShippingInput!',
  },
  discountAutomaticFreeShippingUpdate: {
    freeShippingAutomaticDiscount: 'DiscountAutomaticFreeShippingInput!',
    id: 'ID!',
  },
  discountCodeBasicCreate: {
    basicCodeDiscount: 'DiscountCodeBasicInput!',
  },
  discountCodeBasicUpdate: {
    basicCodeDiscount: 'DiscountCodeBasicInput!',
    id: 'ID!',
  },
  discountCodeActivate: {
    id: 'ID!',
  },
  discountCodeBxgyCreate: {
    bxgyCodeDiscount: 'DiscountCodeBxgyInput!',
  },
  discountCodeBxgyUpdate: {
    bxgyCodeDiscount: 'DiscountCodeBxgyInput!',
    id: 'ID!',
  },
  discountCodeDeactivate: {
    id: 'ID!',
  },
  discountCodeDelete: {
    id: 'ID!',
  },
  discountCodeFreeShippingCreate: {
    freeShippingCodeDiscount: 'DiscountCodeFreeShippingInput!',
  },
  discountCodeFreeShippingUpdate: {
    freeShippingCodeDiscount: 'DiscountCodeFreeShippingInput!',
    id: 'ID!',
  },
  discountRedeemCodeBulkAdd: {
    discountId: 'ID!',
    codes: '[String!]!',
  },
  discountCodeRedeemCodeBulkDelete: {
    discountId: 'ID!',
  },
  discountRedeemCodeBulkDelete: {
    discountId: 'ID!',
  },
  discountCodeAppCreate: {
    codeAppDiscount: 'DiscountCodeAppInput!',
  },
  discountCodeAppUpdate: {
    codeAppDiscount: 'DiscountCodeAppInput!',
    id: 'ID!',
  },
  discountAutomaticAppCreate: {
    automaticAppDiscount: 'DiscountAutomaticAppInput!',
  },
  discountAutomaticAppUpdate: {
    automaticAppDiscount: 'DiscountAutomaticAppInput!',
    id: 'ID!',
  },
};

const discountMutationNodeFieldByRoot: Record<string, string> = {
  discountAutomaticActivate: 'automaticDiscountNode',
  discountAutomaticBasicCreate: 'automaticDiscountNode',
  discountAutomaticBasicUpdate: 'automaticDiscountNode',
  discountAutomaticDeactivate: 'automaticDiscountNode',
  discountAutomaticBxgyCreate: 'automaticDiscountNode',
  discountAutomaticBxgyUpdate: 'automaticDiscountNode',
  discountAutomaticFreeShippingCreate: 'automaticDiscountNode',
  discountAutomaticFreeShippingUpdate: 'automaticDiscountNode',
  discountCodeBasicCreate: 'codeDiscountNode',
  discountCodeBasicUpdate: 'codeDiscountNode',
  discountCodeActivate: 'codeDiscountNode',
  discountCodeBxgyCreate: 'codeDiscountNode',
  discountCodeBxgyUpdate: 'codeDiscountNode',
  discountCodeDeactivate: 'codeDiscountNode',
  discountCodeFreeShippingCreate: 'codeDiscountNode',
  discountCodeFreeShippingUpdate: 'codeDiscountNode',
};

function ownsKey(value: object, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

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

function readString(value: unknown, fallback: string): string;
function readString(value: unknown, fallback?: string | null): string | null;
function readString(value: unknown, fallback: string | null = null): string | null {
  return typeof value === 'string' ? value : fallback;
}

function readNullableString(value: unknown, fallback: string | null): string | null {
  return typeof value === 'string' || value === null ? value : fallback;
}

function readBoolean(value: unknown, fallback: boolean): boolean {
  return typeof value === 'boolean' ? value : fallback;
}

function readNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function readInteger(value: unknown, fallback: number | null = null): number | null {
  if (typeof value === 'number' && Number.isInteger(value)) {
    return value;
  }

  if (typeof value === 'string' && /^-?\d+$/u.test(value)) {
    return Number.parseInt(value, 10);
  }

  return fallback;
}

function normalizeDiscountMoneyAmount(value: string): string {
  const amount = Number.parseFloat(value);
  return Number.isFinite(amount) ? amount.toFixed(1) : value;
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
  discount: DiscountRecord | null = null,
  deletedCodeDiscountId: string | null = null,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  const context: DiscountSerializationContext = { errors: [], path: [getFieldResponseKey(field)] };
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    if (selection.name.value === 'userErrors') {
      payload[key] = serializeDiscountMutationUserErrors(selection, userErrors);
    } else if (selection.name.value === nodeField || selection.name.value === 'job') {
      payload[key] =
        userErrors.length === 0 && discount && nodeField === 'codeDiscountNode'
          ? serializeDiscountOwnerNode(discount, selection, {}, 'DiscountCodeNode', 'codeDiscount', {
              ...context,
              path: [key],
            })
          : null;
    } else if (selection.name.value === 'deletedCodeDiscountId') {
      payload[key] = userErrors.length === 0 ? deletedCodeDiscountId : null;
    } else {
      payload[key] = null;
    }
  }
  return payload;
}

function readMoneyAmount(value: unknown): string | null {
  if (typeof value === 'string' && value.length > 0) {
    return value;
  }

  if (typeof value === 'number' && Number.isFinite(value)) {
    return value.toFixed(2);
  }

  return null;
}

function readDiscountCode(input: Record<string, unknown> | null, fallback: string | null = null): string {
  const code = readString(input?.['code']) ?? fallback ?? 'DISCOUNT';
  return code.trim().length > 0 ? code.trim() : 'DISCOUNT';
}

function readDiscountTitle(input: Record<string, unknown> | null, fallback: string | null = null): string {
  const title = readString(input?.['title']) ?? fallback ?? readDiscountCode(input);
  return title.trim().length > 0 ? title.trim() : readDiscountCode(input);
}

function inferDiscountClasses(customerGets: DiscountCustomerGetsRecord | null): string[] {
  const itemTypeName = customerGets?.items.typeName;
  return itemTypeName === 'DiscountProducts' || itemTypeName === 'DiscountCollections' ? ['PRODUCT'] : ['ORDER'];
}

function buildDiscountSummary(
  customerGets: DiscountCustomerGetsRecord | null,
  minimumRequirement: DiscountMinimumRequirementRecord | null,
): string | null {
  const value = customerGets?.value ?? null;
  if (!value) {
    return null;
  }

  let baseSummary: string | null = null;
  if (value.typeName === 'DiscountPercentage' && typeof value.percentage === 'number') {
    baseSummary = `${Math.round(value.percentage * 100)}% off entire order`;
  } else if (value.typeName === 'DiscountAmount' && value.amount?.amount) {
    baseSummary = `${value.amount.amount} ${value.amount.currencyCode} off entire order`;
  }

  if (!baseSummary) {
    return null;
  }

  if (minimumRequirement?.typeName === 'DiscountMinimumSubtotal') {
    const amount = minimumRequirement.greaterThanOrEqualToSubtotal?.amount;
    return amount ? `${baseSummary} - Minimum purchase of $${amount}` : baseSummary;
  }

  if (minimumRequirement?.typeName === 'DiscountMinimumQuantity') {
    const quantity = minimumRequirement.greaterThanOrEqualToQuantity;
    return quantity ? `${baseSummary} - Minimum quantity of ${quantity}` : baseSummary;
  }

  return baseSummary;
}

function computeDiscountStatus(
  discount: Pick<DiscountRecord, 'startsAt' | 'endsAt' | 'status'>,
  nowIso: string,
): string {
  if (discount.status === 'EXPIRED') {
    return 'EXPIRED';
  }

  const now = Date.parse(nowIso);
  const startsAt = discount.startsAt ? Date.parse(discount.startsAt) : null;
  const endsAt = discount.endsAt ? Date.parse(discount.endsAt) : null;

  if (endsAt !== null && !Number.isNaN(endsAt) && endsAt <= now) {
    return 'EXPIRED';
  }

  if (startsAt !== null && !Number.isNaN(startsAt) && startsAt > now) {
    return 'SCHEDULED';
  }

  return 'ACTIVE';
}

function serializeAutomaticDiscountMutationPayload(
  field: FieldNode,
  variables: Record<string, unknown>,
  discount: DiscountRecord | null,
  userErrors: DiscountMutationUserError[],
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  const context: DiscountSerializationContext = { errors: [], path: [getFieldResponseKey(field)] };

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'automaticDiscountNode':
        payload[key] =
          discount && userErrors.length === 0
            ? serializeDiscountOwnerNode(
                discount,
                selection,
                variables,
                'DiscountAutomaticNode',
                'automaticDiscount',
                context,
              )
            : null;
        break;
      case 'userErrors':
        payload[key] = serializeDiscountMutationUserErrors(selection, userErrors);
        break;
      default:
        payload[key] = null;
        break;
    }
  }

  return payload;
}

function serializeAutomaticDiscountDeletePayload(
  field: FieldNode,
  deletedDiscountId: string | null,
  userErrors: DiscountMutationUserError[],
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'deletedAutomaticDiscountId':
        payload[key] = userErrors.length === 0 ? deletedDiscountId : null;
        break;
      case 'userErrors':
        payload[key] = serializeDiscountMutationUserErrors(selection, userErrors);
        break;
      default:
        payload[key] = null;
        break;
    }
  }
  return payload;
}

function serializeDiscountBulkOperation(
  operation: DiscountBulkOperationRecord,
  field: FieldNode,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = operation.id;
        break;
      case '__typename':
        result[key] = operation.typeName;
        break;
      case 'done':
        result[key] = operation.done;
        break;
      case 'status':
        result[key] = operation.status;
        break;
      case 'codesCount':
        result[key] = operation.codesCount ?? 0;
        break;
      case 'importedCount':
        result[key] = operation.importedCount ?? 0;
        break;
      case 'failedCount':
        result[key] = operation.failedCount ?? 0;
        break;
      case 'createdAt':
        result[key] = operation.createdAt;
        break;
      case 'completedAt':
        result[key] = operation.completedAt ?? null;
        break;
      case 'discountCode':
      case 'discount':
        result[key] = operation.discountId
          ? serializeDiscountOwnerNodeById(operation.discountId, selection, {}, { errors: [], path: [key] })
          : null;
        break;
      case 'query':
        result[key] = operation.query ?? null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDiscountBulkMutationPayload(
  field: FieldNode,
  operation: DiscountBulkOperationRecord | null,
  userErrors: DiscountMutationUserError[],
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'bulkCreation':
      case 'job':
        payload[key] =
          operation && userErrors.length === 0 ? serializeDiscountBulkOperation(operation, selection) : null;
        break;
      case 'userErrors':
        payload[key] = serializeDiscountMutationUserErrors(selection, userErrors);
        break;
      default:
        payload[key] = null;
        break;
    }
  }
  return payload;
}

function serializeAppDiscountMutationPayload(
  field: FieldNode,
  variables: Record<string, unknown>,
  discount: DiscountRecord | null,
  userErrors: DiscountMutationUserError[],
  payloadFieldName: 'codeAppDiscount' | 'automaticAppDiscount',
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  const context: DiscountSerializationContext = { errors: [], path: [getFieldResponseKey(field)] };
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'codeAppDiscount':
      case 'automaticAppDiscount':
        payload[key] =
          selection.name.value === payloadFieldName && discount && userErrors.length === 0
            ? serializeDiscountUnion(discount, selection, variables, { ...context, path: [...context.path, key] })
            : null;
        break;
      case 'userErrors':
        payload[key] = serializeDiscountMutationUserErrors(selection, userErrors);
        break;
      default:
        payload[key] = null;
        break;
    }
  }
  return payload;
}

function hasDateRangeError(input: Record<string, unknown> | null): boolean {
  const startsAt = typeof input?.['startsAt'] === 'string' ? input['startsAt'] : null;
  const endsAt = typeof input?.['endsAt'] === 'string' ? input['endsAt'] : null;
  return startsAt !== null && endsAt !== null && Date.parse(endsAt) <= Date.parse(startsAt);
}

function invalidReferenceMessage(id: string, resourceName: string): string {
  const legacyId = id.split('/').at(-1);
  if (resourceName === 'productsToAdd') {
    return `Product with id: ${legacyId} is invalid`;
  }
  if (resourceName === 'productVariantsToAdd') {
    return `Product variant with id: ${legacyId} is invalid`;
  }
  return `Collection with id: ${legacyId} is invalid`;
}

function listInvalidIdsForField(field: string[], ids: string[], resourceName: string): DiscountMutationUserError[] {
  return ids
    .filter((id) => id.endsWith('/0'))
    .map((id) => ({
      field: [...field, resourceName],
      message: invalidReferenceMessage(id, resourceName),
      code: 'INVALID',
      extraInfo: null,
    }));
}

function listInvalidIds(ids: string[], resourceName: string): DiscountMutationUserError[] {
  return listInvalidIdsForField(['basicCodeDiscount', 'customerGets', 'items', 'products'], ids, resourceName);
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

function validateCodeUniqueness(codes: string[], field: string[]): DiscountMutationUserError[] | null {
  const normalizedCodes = codes.map((code) => code.trim()).filter((code) => code.length > 0);
  const duplicateInputCodes = normalizedCodes.filter((code, index) => normalizedCodes.indexOf(code) !== index);
  const existingCodes = new Set(
    store
      .listEffectiveDiscounts()
      .flatMap((discount) =>
        discount.method === 'code' ? getDiscountCodes(discount).map((entry) => entry.code.trim()) : [],
      ),
  );
  const duplicateExistingCodes = normalizedCodes.filter((code) => existingCodes.has(code));

  if (duplicateInputCodes.length === 0 && duplicateExistingCodes.length === 0) {
    return null;
  }

  return [
    {
      field,
      message: 'Code must be unique. Please try a different code.',
      code: 'TAKEN',
      extraInfo: null,
    },
  ];
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

function validateAutomaticDiscountExists(
  id: unknown,
  typeName: string | null = null,
): DiscountMutationUserError[] | null {
  if (typeof id === 'string' && findAutomaticDiscountById(id, typeName)) {
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

function validateAutomaticBxgyDiscountExists(id: unknown): DiscountMutationUserError[] | null {
  if (typeof id === 'string' && findAutomaticBxgyDiscountById(id)) {
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

function validateDiscountBxgyCreate(
  input: Record<string, unknown> | null,
  argumentName: 'bxgyCodeDiscount' | 'automaticBxgyDiscount',
): DiscountMutationUserError[] | null {
  if (!input) {
    return null;
  }

  const userErrors: DiscountMutationUserError[] = [];

  if (hasDateRangeError(input)) {
    userErrors.push({
      field: [argumentName, 'endsAt'],
      message: 'Ends at needs to be after starts_at',
      code: 'INVALID',
      extraInfo: null,
    });
  }

  if (input['title'] === '') {
    userErrors.push(
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
    );
  }

  if (argumentName === 'bxgyCodeDiscount') {
    const code = typeof input['code'] === 'string' ? input['code'] : null;
    if (
      code &&
      store
        .listEffectiveDiscounts()
        .some(
          (discount) => discount.method === 'code' && getDiscountCodes(discount).some((entry) => entry.code === code),
        )
    ) {
      userErrors.push({
        field: [argumentName, 'code'],
        message: 'Code must be unique. Please try a different code.',
        code: 'TAKEN',
        extraInfo: null,
      });
    }
  }

  for (const sectionName of ['customerBuys', 'customerGets'] as const) {
    const section = readNestedRecord(input, sectionName);
    const items = readNestedRecord(section, 'items');
    const products = readNestedRecord(items, 'products');
    const collections = readNestedRecord(items, 'collections');
    const productIds = readStringArray(products?.['productsToAdd']);
    const variantIds = readStringArray(products?.['productVariantsToAdd']);
    const collectionIds = readStringArray(collections?.['add']);

    userErrors.push(
      ...listInvalidIdsForField([argumentName, sectionName, 'items', 'products'], productIds, 'productsToAdd'),
      ...listInvalidIdsForField([argumentName, sectionName, 'items', 'products'], variantIds, 'productVariantsToAdd'),
      ...listInvalidIdsForField([argumentName, sectionName, 'items', 'collections'], collectionIds, 'add'),
    );
  }

  return userErrors.length > 0 ? userErrors : null;
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

function validateDiscountFreeShippingInput(
  input: Record<string, unknown> | null,
  argumentName: 'freeShippingCodeDiscount' | 'freeShippingAutomaticDiscount',
): DiscountMutationUserError[] | null {
  const baseErrors = validateDiscountFreeShippingCreate(input, argumentName) ?? [];
  const minimumRequirement = readNestedRecord(input, 'minimumRequirement');
  const hasSubtotal = readNestedRecord(minimumRequirement, 'subtotal') !== null;
  const hasQuantity = readNestedRecord(minimumRequirement, 'quantity') !== null;

  if (hasSubtotal && hasQuantity) {
    baseErrors.push(
      {
        field: [argumentName, 'minimumRequirement', 'subtotal', 'greaterThanOrEqualToSubtotal'],
        message: 'Minimum subtotal cannot be defined when minimum quantity is.',
        code: 'CONFLICT',
        extraInfo: null,
      },
      {
        field: [argumentName, 'minimumRequirement', 'quantity', 'greaterThanOrEqualToQuantity'],
        message: 'Minimum quantity cannot be defined when minimum subtotal is.',
        code: 'CONFLICT',
        extraInfo: null,
      },
    );
  }

  return baseErrors.length > 0 ? baseErrors : null;
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

function validateKnownCodeDiscountId(id: unknown): DiscountMutationUserError[] | null {
  if (
    typeof id === 'string' &&
    store.listEffectiveDiscounts().some((discount) => discount.id === id && discount.method === 'code')
  ) {
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

function validateKnownCodeBxgyDiscountId(id: unknown): DiscountMutationUserError[] | null {
  if (typeof id === 'string' && findCodeBxgyDiscountById(id)) {
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

function validateKnownTypedCodeDiscountId(id: unknown, typeName: string): DiscountMutationUserError[] | null {
  if (
    typeof id === 'string' &&
    store
      .listEffectiveDiscounts()
      .some((discount) => discount.id === id && discount.method === 'code' && discount.typeName === typeName)
  ) {
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

function findCodeDiscountById(id: unknown): DiscountRecord | null {
  if (typeof id !== 'string') {
    return null;
  }

  return store.listEffectiveDiscounts().find((discount) => discount.id === id && discount.method === 'code') ?? null;
}

function readRedeemCodeInputs(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((item) => {
      if (typeof item === 'string') {
        return item.trim();
      }

      const record = readRecord(item);
      return readString(record?.['code'])?.trim() ?? '';
    })
    .filter((code) => code.length > 0);
}

function buildDiscountBulkOperation(
  operation: DiscountBulkOperationRecord['operation'],
  discountId: string,
  values: {
    id: string;
    typeName: string;
    query?: string | null;
    codesCount?: number;
    importedCount?: number;
    failedCount?: number;
    redeemCodeIds?: string[];
    discountIds?: string[];
    selector?: Record<string, JsonValue>;
  },
): DiscountBulkOperationRecord {
  const now = makeSyntheticTimestamp();
  return {
    id: values.id,
    typeName: values.typeName,
    operation,
    discountId,
    status: 'COMPLETED',
    done: true,
    createdAt: now,
    completedAt: now,
    ...(values.query === undefined ? {} : { query: values.query }),
    ...(values.codesCount === undefined ? {} : { codesCount: values.codesCount }),
    ...(values.importedCount === undefined ? {} : { importedCount: values.importedCount }),
    ...(values.failedCount === undefined ? {} : { failedCount: values.failedCount }),
    ...(values.redeemCodeIds === undefined ? {} : { redeemCodeIds: values.redeemCodeIds }),
    ...(values.discountIds === undefined ? {} : { discountIds: values.discountIds }),
    ...(values.selector === undefined ? {} : { selector: values.selector }),
  };
}

function stageRedeemCodeBulkAdd(discountId: string, codes: string[]): DiscountBulkOperationRecord {
  const existing = findCodeDiscountById(discountId);
  if (!existing) {
    throw new Error(`Cannot add redeem codes to unknown code discount ${discountId}`);
  }

  const existingRedeemCodes = getDiscountCodes(existing);
  const bulkCreationId = makeProxySyntheticGid('DiscountRedeemCodeBulkCreation');
  const addedRedeemCodes = codes.map((code) => ({
    id: makeProxySyntheticGid('DiscountRedeemCode'),
    code,
    asyncUsageCount: 0,
  }));
  const nextRedeemCodes = [...existingRedeemCodes, ...addedRedeemCodes];
  store.stageCreateDiscount({
    ...structuredClone(existing),
    codes: nextRedeemCodes.map((redeemCode) => redeemCode.code),
    redeemCodes: nextRedeemCodes,
    updatedAt: makeSyntheticTimestamp(),
  });

  return store.stageDiscountBulkOperation(
    buildDiscountBulkOperation('discountRedeemCodeBulkAdd', discountId, {
      id: bulkCreationId,
      typeName: 'DiscountRedeemCodeBulkCreation',
      codesCount: codes.length,
      importedCount: codes.length,
      failedCount: 0,
      redeemCodeIds: addedRedeemCodes.map((code) => code.id),
    }),
  );
}

function stageRedeemCodeBulkDelete(discountId: string, redeemCodeIds: string[]): DiscountBulkOperationRecord {
  const existing = findCodeDiscountById(discountId);
  if (!existing) {
    throw new Error(`Cannot delete redeem codes from unknown code discount ${discountId}`);
  }

  const ids = new Set(redeemCodeIds);
  const nextRedeemCodes = getDiscountCodes(existing).filter((code) => !ids.has(code.id));
  store.stageCreateDiscount({
    ...structuredClone(existing),
    codes: nextRedeemCodes.map((redeemCode) => redeemCode.code),
    redeemCodes: nextRedeemCodes,
    updatedAt: makeSyntheticTimestamp(),
  });

  return store.stageDiscountBulkOperation(
    buildDiscountBulkOperation('discountCodeRedeemCodeBulkDelete', discountId, {
      id: makeProxySyntheticGid('Job'),
      typeName: 'Job',
      redeemCodeIds,
    }),
  );
}

function readDiscountClasses(input: Record<string, unknown> | null, fallback: string[] = ['ORDER']): string[] {
  const classes = readStringArray(input?.['discountClasses']);
  return classes.length > 0 ? classes : fallback;
}

function readAppFunctionReference(input: Record<string, unknown> | null): {
  field: string[];
  value: string | null;
  shopifyFunction: ShopifyFunctionRecord | null;
} {
  const functionId = readString(input?.['functionId']);
  const functionHandle = readString(input?.['functionHandle']);
  const lookup = functionId ?? functionHandle;
  const field = functionId ? ['functionId'] : ['functionHandle'];
  if (!lookup) {
    return { field: ['functionId'], value: null, shopifyFunction: null };
  }

  const shopifyFunction =
    store
      .listEffectiveShopifyFunctions()
      .find(
        (candidate) =>
          candidate.id === lookup ||
          candidate.handle === lookup ||
          candidate.id.endsWith(`/${lookup}`) ||
          (functionHandle !== null && candidate.handle === functionHandle),
      ) ?? null;

  return { field, value: lookup, shopifyFunction };
}

function validateAppFunctionEvidence(input: Record<string, unknown> | null): DiscountMutationUserError[] | null {
  const functionReference = readAppFunctionReference(input);
  if (!functionReference.value) {
    return [
      {
        field: functionReference.field,
        message: 'Function ID or function handle is required for local app-discount staging.',
        code: 'BLANK',
        extraInfo: null,
      },
    ];
  }

  if (!functionReference.shopifyFunction) {
    return [
      {
        field: functionReference.field,
        message: `Function ${functionReference.value} not found. Ensure that it is released in the current app, and that the app is installed.`,
        code: 'INVALID',
        extraInfo: null,
      },
    ];
  }

  return null;
}

function validateDiscountAppCreate(
  input: Record<string, unknown> | null,
  inputFieldName: string,
  isCodeDiscount: boolean,
): DiscountMutationUserError[] | null {
  if (!input) {
    return [
      {
        field: [inputFieldName],
        message: 'Input is required',
        code: 'BLANK',
        extraInfo: null,
      },
    ];
  }

  const errors: DiscountMutationUserError[] = [];
  const title = readString(input['title']);
  if (!title || title.trim().length === 0) {
    errors.push({
      field: [inputFieldName, 'title'],
      message: "Title can't be blank",
      code: 'BLANK',
      extraInfo: null,
    });
  }

  if (isCodeDiscount) {
    const code = readString(input['code']);
    if (!code || code.trim().length === 0) {
      errors.push({
        field: [inputFieldName, 'code'],
        message: "Code can't be blank",
        code: 'BLANK',
        extraInfo: null,
      });
    }
  }

  const functionErrors = validateAppFunctionEvidence(input);
  if (functionErrors) {
    errors.push(...functionErrors.map((error) => ({ ...error, field: [inputFieldName, ...(error.field ?? [])] })));
  }

  if (hasDateRangeError(input)) {
    errors.push({
      field: [inputFieldName, 'endsAt'],
      message: 'End date must be after start date',
      code: 'INVALID',
      extraInfo: null,
    });
  }

  return errors.length > 0 ? errors : null;
}

function findAppDiscountById(id: unknown, typeName: string): DiscountRecord | null {
  return typeof id === 'string'
    ? (store.listEffectiveDiscounts().find((discount) => discount.id === id && discount.typeName === typeName) ?? null)
    : null;
}

function validateKnownAppDiscountId(id: unknown, typeName: string): DiscountMutationUserError[] | null {
  if (findAppDiscountById(id, typeName)) {
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

function normalizeDiscountMetafields(
  input: Record<string, unknown> | null,
  _ownerId: string,
  existing: DiscountMetafieldRecord[] = [],
): DiscountMetafieldRecord[] {
  if (!ownsKey(input ?? {}, 'metafields')) {
    return structuredClone(existing);
  }

  const metafields = Array.isArray(input?.['metafields']) ? input['metafields'] : [];
  return metafields
    .map((item): DiscountMetafieldRecord | null => {
      const metafield = readRecord(item);
      const namespace = readString(metafield?.['namespace']);
      const key = readString(metafield?.['key']);
      if (!namespace || !key) {
        return null;
      }

      const type = readString(metafield?.['type']);
      const value = readString(metafield?.['value']);
      return {
        id: makeProxySyntheticGid('Metafield'),
        namespace,
        key,
        type: type ?? 'single_line_text_field',
        value: value ?? '',
        compareDigest: null,
        createdAt: makeSyntheticTimestamp(),
        updatedAt: makeSyntheticTimestamp(),
        ownerType: 'DISCOUNT',
      };
    })
    .filter((metafield): metafield is DiscountMetafieldRecord => metafield !== null);
}

function makeAppDiscountType(
  shopifyFunction: ShopifyFunctionRecord,
  functionReference: string,
  discountClasses: string[],
): Record<string, JsonValue> {
  return {
    __typename: 'AppDiscountType',
    appKey: shopifyFunction.appKey ?? null,
    title: shopifyFunction.title ?? 'Local app discount',
    description: shopifyFunction.description ?? null,
    functionId: functionReference,
    discountClass: discountClasses[0] ?? 'ORDER',
    targetType: null,
    app: shopifyFunction.app ?? null,
  };
}

function buildAppDiscount(
  input: Record<string, unknown>,
  isCodeDiscount: boolean,
  existing: DiscountRecord | null = null,
): DiscountRecord {
  const now = makeSyntheticTimestamp();
  const id = existing?.id ?? makeProxySyntheticGid(isCodeDiscount ? 'DiscountCodeNode' : 'DiscountAutomaticNode');
  const functionReference = readAppFunctionReference(input);
  if (!functionReference.value || !functionReference.shopifyFunction) {
    throw new Error('Cannot build app discount without Function evidence');
  }

  const discountClasses = readDiscountClasses(input, existing?.discountClasses ?? ['ORDER']);
  const code = isCodeDiscount ? readDiscountCode(input, existing?.codes[0] ?? null) : null;
  const startsAt = readNullableString(input['startsAt'], existing?.startsAt ?? now);
  const endsAt = readNullableString(input['endsAt'], existing?.endsAt ?? null);
  const discount: DiscountRecord = {
    id,
    typeName: isCodeDiscount ? 'DiscountCodeApp' : 'DiscountAutomaticApp',
    method: isCodeDiscount ? 'code' : 'automatic',
    title: readDiscountTitle(input, existing?.title ?? null),
    status: existing?.status ?? null,
    summary: existing?.summary ?? null,
    startsAt,
    endsAt,
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
    asyncUsageCount: existing?.asyncUsageCount ?? 0,
    usageLimit: typeof input['usageLimit'] === 'number' ? input['usageLimit'] : (existing?.usageLimit ?? null),
    discountClasses,
    combinesWith: readCombinesWith(input['combinesWith'], existing?.combinesWith),
    codes: code ? [code] : [],
    redeemCodes: code
      ? [
          {
            id: existing?.redeemCodes?.[0]?.id ?? makeProxySyntheticGid('DiscountRedeemCode'),
            code,
            asyncUsageCount: existing?.redeemCodes?.[0]?.asyncUsageCount ?? 0,
          },
        ]
      : [],
    context: ownsKey(input, 'context') ? readDiscountContext(input['context'], existing?.context) : existing?.context,
    customerGets: existing?.customerGets ?? null,
    minimumRequirement: existing?.minimumRequirement ?? null,
    metafields: normalizeDiscountMetafields(input, id, existing?.metafields ?? []),
    events: existing?.events ? structuredClone(existing.events) : [],
    discountType: 'app',
    appId: functionReference.shopifyFunction.appKey ?? null,
    appDiscountType: makeAppDiscountType(functionReference.shopifyFunction, functionReference.value, discountClasses),
    discountId: id,
    errorHistory: null,
    appliesOncePerCustomer: isCodeDiscount
      ? readBoolean(input['appliesOncePerCustomer'], existing?.appliesOncePerCustomer ?? false)
      : (existing?.appliesOncePerCustomer ?? null),
    appliesOnOneTimePurchase: readBoolean(
      input['appliesOnOneTimePurchase'],
      existing?.appliesOnOneTimePurchase ?? true,
    ),
    appliesOnSubscription: readBoolean(input['appliesOnSubscription'], existing?.appliesOnSubscription ?? false),
    recurringCycleLimit: readInteger(input['recurringCycleLimit'], existing?.recurringCycleLimit ?? 1),
  };

  discount.status = computeDiscountStatus(discount, now);
  return discount;
}

function stageCodeAppCreate(input: Record<string, unknown>): DiscountRecord {
  return store.stageCreateDiscount(buildAppDiscount(input, true));
}

function stageCodeAppUpdate(id: string, input: Record<string, unknown>): DiscountRecord {
  return store.stageCreateDiscount(buildAppDiscount(input, true, findAppDiscountById(id, 'DiscountCodeApp')));
}

function stageAutomaticAppCreate(input: Record<string, unknown>): DiscountRecord {
  return store.stageCreateDiscount(buildAppDiscount(input, false));
}

function stageAutomaticAppUpdate(id: string, input: Record<string, unknown>): DiscountRecord {
  return store.stageCreateDiscount(buildAppDiscount(input, false, findAppDiscountById(id, 'DiscountAutomaticApp')));
}

function normalizeDiscountCombinesWith(
  input: Record<string, unknown> | null,
  fallback?: DiscountRecord['combinesWith'],
) {
  const combinesWith = readNestedRecord(input, 'combinesWith');
  return {
    productDiscounts: readBoolean(combinesWith?.['productDiscounts'], fallback?.productDiscounts ?? false),
    orderDiscounts: readBoolean(combinesWith?.['orderDiscounts'], fallback?.orderDiscounts ?? false),
    shippingDiscounts: readBoolean(combinesWith?.['shippingDiscounts'], fallback?.shippingDiscounts ?? false),
  };
}

function normalizeDiscountContext(
  input: Record<string, unknown> | null,
  fallback?: DiscountContextRecord | null,
): DiscountContextRecord {
  const context = readNestedRecord(input, 'context');
  const customers = readNestedRecord(context, 'customers');
  const customerSegments = readNestedRecord(context, 'customerSegments');
  const customerIds = readStringArray(customers?.['add']);
  const customerSegmentIds = readStringArray(customerSegments?.['add']);

  if (customerIds.length > 0) {
    return {
      typeName: 'DiscountCustomers',
      customerIds,
    };
  }

  if (customerSegmentIds.length > 0) {
    return {
      typeName: 'DiscountCustomerSegments',
      customerSegmentIds,
    };
  }

  return fallback ?? { typeName: 'DiscountBuyerSelectionAll', all: 'ALL' };
}

function normalizeDiscountItems(
  input: Record<string, unknown> | null,
  fallback?: DiscountItemsRecord | null,
): DiscountItemsRecord {
  const customerGets = readNestedRecord(input, 'customerGets');
  const items = readNestedRecord(customerGets, 'items');
  const products = readNestedRecord(items, 'products');
  const collections = readNestedRecord(items, 'collections');
  const productIds = readStringArray(products?.['productsToAdd']);
  const productVariantIds = readStringArray(products?.['productVariantsToAdd']);
  const collectionIds = readStringArray(collections?.['add']);

  if (collectionIds.length > 0) {
    return {
      typeName: 'DiscountCollections',
      collectionIds,
    };
  }

  if (productIds.length > 0 || productVariantIds.length > 0) {
    return {
      typeName: 'DiscountProducts',
      productIds,
      productVariantIds,
    };
  }

  return fallback ?? { typeName: 'AllDiscountItems', allItems: true };
}

function normalizeDiscountValue(
  input: Record<string, unknown> | null,
  fallback?: DiscountValueRecord | null,
): DiscountValueRecord {
  const customerGets = readNestedRecord(input, 'customerGets');
  const value = readNestedRecord(customerGets, 'value');
  const percentage = readNumber(value?.['percentage']);
  if (percentage !== null) {
    return {
      typeName: 'DiscountPercentage',
      percentage,
    };
  }

  const fixedAmount = readNestedRecord(value, 'discountAmount') ?? readNestedRecord(value, 'fixedAmount');
  const amount = readMoneyAmount(fixedAmount?.['amount']);
  if (amount !== null) {
    return {
      typeName: 'DiscountAmount',
      amount: {
        amount,
        currencyCode: readString(fixedAmount?.['currencyCode']) ?? 'USD',
      },
      appliesOnEachItem: readBoolean(fixedAmount?.['appliesOnEachItem'], false),
    };
  }

  return fallback ?? { typeName: 'DiscountPercentage', percentage: 0 };
}

function normalizeCustomerGets(
  input: Record<string, unknown> | null,
  fallback?: DiscountCustomerGetsRecord | null,
): DiscountCustomerGetsRecord {
  const customerGets = readNestedRecord(input, 'customerGets');
  return {
    value: normalizeDiscountValue(input, fallback?.value),
    items: normalizeDiscountItems(input, fallback?.items),
    appliesOnOneTimePurchase: readBoolean(
      customerGets?.['appliesOnOneTimePurchase'],
      fallback?.appliesOnOneTimePurchase ?? true,
    ),
    appliesOnSubscription: readBoolean(
      customerGets?.['appliesOnSubscription'],
      fallback?.appliesOnSubscription ?? false,
    ),
  };
}

function normalizeMinimumRequirement(
  input: Record<string, unknown> | null,
  fallback?: DiscountMinimumRequirementRecord | null,
): DiscountMinimumRequirementRecord | null {
  const minimumRequirement = readNestedRecord(input, 'minimumRequirement');
  const subtotal = readNestedRecord(minimumRequirement, 'subtotal');
  const subtotalAmount = readMoneyAmount(subtotal?.['greaterThanOrEqualToSubtotal']);
  if (subtotalAmount !== null) {
    return {
      typeName: 'DiscountMinimumSubtotal',
      greaterThanOrEqualToSubtotal: {
        amount: subtotalAmount,
        currencyCode: readString(subtotal?.['currencyCode']) ?? 'USD',
      },
    };
  }

  const quantity = readNestedRecord(minimumRequirement, 'quantity');
  const minimumQuantity = readString(quantity?.['greaterThanOrEqualToQuantity']);
  if (minimumQuantity !== null) {
    return {
      typeName: 'DiscountMinimumQuantity',
      greaterThanOrEqualToQuantity: minimumQuantity,
    };
  }

  return fallback ?? null;
}

function buildCodeBasicDiscount(
  input: Record<string, unknown>,
  existing: DiscountRecord | null = null,
): DiscountRecord {
  const now = makeSyntheticTimestamp();
  const code = readDiscountCode(input, existing?.codes[0] ?? existing?.redeemCodes?.[0]?.code ?? null);
  const customerGets = normalizeCustomerGets(input, existing?.customerGets ?? null);
  const minimumRequirement = normalizeMinimumRequirement(input, existing?.minimumRequirement ?? null);
  const discount: DiscountRecord = {
    id: existing?.id ?? makeProxySyntheticGid('DiscountCodeNode'),
    typeName: 'DiscountCodeBasic',
    method: 'code',
    title: readDiscountTitle(input, existing?.title ?? null),
    status: existing?.status ?? null,
    summary: null,
    startsAt: readString(input['startsAt']) ?? existing?.startsAt ?? now,
    endsAt: input['endsAt'] === null ? null : (readString(input['endsAt']) ?? existing?.endsAt ?? null),
    createdAt: existing?.createdAt ?? now,
    updatedAt: existing ? now : now,
    asyncUsageCount: existing?.asyncUsageCount ?? 0,
    discountClasses: inferDiscountClasses(customerGets),
    combinesWith: normalizeDiscountCombinesWith(input, existing?.combinesWith),
    codes: [code],
    redeemCodes: [
      {
        id: existing?.redeemCodes?.[0]?.id ?? makeProxySyntheticGid('DiscountRedeemCode'),
        code,
        asyncUsageCount: existing?.redeemCodes?.[0]?.asyncUsageCount ?? 0,
      },
    ],
    context: normalizeDiscountContext(input, existing?.context ?? null),
    customerGets,
    minimumRequirement,
    metafields: existing?.metafields ?? [],
    events: existing?.events ?? [],
    discountType: customerGets.value.typeName === 'DiscountAmount' ? 'fixed_amount' : 'percentage',
    appId: existing?.appId ?? null,
  };

  discount.summary = buildDiscountSummary(customerGets, minimumRequirement);
  discount.status = computeDiscountStatus(discount, now);
  return discount;
}

function stageCodeBasicCreate(input: Record<string, unknown>): DiscountRecord {
  return store.stageCreateDiscount(buildCodeBasicDiscount(input));
}

function stageCodeBasicUpdate(id: string, input: Record<string, unknown>): DiscountRecord {
  const existing = store.listEffectiveDiscounts().find((discount) => discount.id === id && discount.method === 'code');
  return store.stageCreateDiscount(buildCodeBasicDiscount(input, existing ?? null));
}

function buildCodeFreeShippingDiscount(
  input: Record<string, unknown>,
  existing: DiscountRecord | null = null,
): DiscountRecord {
  const now = makeSyntheticTimestamp();
  const code = readDiscountCode(input, existing?.codes[0] ?? existing?.redeemCodes?.[0]?.code ?? null);
  const startsAt = readNullableString(input['startsAt'], existing?.startsAt ?? now);
  const endsAt = readNullableString(input['endsAt'], existing?.endsAt ?? null);
  const discount: DiscountRecord = {
    id: existing?.id ?? makeProxySyntheticGid('DiscountCodeNode'),
    typeName: 'DiscountCodeFreeShipping',
    method: 'code',
    title: readDiscountTitle(input, existing?.title ?? null),
    status: existing?.status ?? null,
    summary: null,
    startsAt,
    endsAt,
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
    asyncUsageCount: existing?.asyncUsageCount ?? 0,
    discountClasses: ['SHIPPING'],
    combinesWith: readCombinesWith(input['combinesWith'], existing?.combinesWith),
    codes: [code],
    redeemCodes: [
      {
        id: existing?.redeemCodes?.[0]?.id ?? makeProxySyntheticGid('DiscountRedeemCode'),
        code,
        asyncUsageCount: existing?.redeemCodes?.[0]?.asyncUsageCount ?? 0,
      },
    ],
    context: ownsKey(input, 'context') ? readDiscountContext(input['context'], existing?.context) : existing?.context,
    customerGets: null,
    minimumRequirement: ownsKey(input, 'minimumRequirement')
      ? readMinimumRequirement(input['minimumRequirement'], existing?.minimumRequirement)
      : existing?.minimumRequirement,
    destinationSelection: ownsKey(input, 'destination')
      ? readDiscountDestinationSelection(input['destination'], existing?.destinationSelection)
      : (existing?.destinationSelection ?? { typeName: 'DiscountCountryAll', allCountries: true }),
    maximumShippingPrice: ownsKey(input, 'maximumShippingPrice')
      ? readShippingPrice(input['maximumShippingPrice'], existing?.maximumShippingPrice)
      : (existing?.maximumShippingPrice ?? null),
    appliesOncePerCustomer: readBoolean(input['appliesOncePerCustomer'], existing?.appliesOncePerCustomer ?? false),
    appliesOnOneTimePurchase: readBoolean(
      input['appliesOnOneTimePurchase'],
      existing?.appliesOnOneTimePurchase ?? true,
    ),
    appliesOnSubscription: readBoolean(input['appliesOnSubscription'], existing?.appliesOnSubscription ?? false),
    recurringCycleLimit:
      typeof input['recurringCycleLimit'] === 'number'
        ? input['recurringCycleLimit']
        : (existing?.recurringCycleLimit ?? null),
    usageLimit: typeof input['usageLimit'] === 'number' ? input['usageLimit'] : (existing?.usageLimit ?? null),
    metafields: existing?.metafields ? structuredClone(existing.metafields) : [],
    events: existing?.events ? structuredClone(existing.events) : [],
    discountType: 'free_shipping',
    appId: existing?.appId ?? null,
  };

  discount.status = computeDiscountStatus(discount, now);
  discount.summary = buildFreeShippingSummary(discount);
  return discount;
}

function stageCodeFreeShippingCreate(input: Record<string, unknown>): DiscountRecord {
  return store.stageCreateDiscount(buildCodeFreeShippingDiscount(input));
}

function stageCodeFreeShippingUpdate(id: string, input: Record<string, unknown>): DiscountRecord {
  const existing = store
    .listEffectiveDiscounts()
    .find(
      (discount) =>
        discount.id === id && discount.method === 'code' && discount.typeName === 'DiscountCodeFreeShipping',
    );
  return store.stageCreateDiscount(buildCodeFreeShippingDiscount(input, existing ?? null));
}

function stageCodeStatus(id: string, status: 'ACTIVE' | 'EXPIRED'): DiscountRecord {
  const existing = store.listEffectiveDiscounts().find((discount) => discount.id === id && discount.method === 'code');
  if (!existing) {
    throw new Error(`Cannot stage status for unknown code discount ${id}`);
  }

  const now = makeSyntheticTimestamp();
  const updated: DiscountRecord = {
    ...structuredClone(existing),
    status,
    updatedAt: now,
  } as DiscountRecord;
  return store.stageCreateDiscount(updated);
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

function validateBroadBulkSelector(
  args: Record<string, unknown>,
  conflictMessage: string,
): DiscountMutationUserError[] | null {
  const conflict = validateBulkSelectorConflict(args, conflictMessage);
  if (conflict) {
    return conflict;
  }

  const ids = readStringArray(args['ids']);
  const search = args['search'];
  const savedSearchId = args['savedSearchId'];
  const hasSavedSearch = typeof savedSearchId === 'string' && savedSearchId.trim().length > 0;
  if (ids.length > 0 || hasSavedSearch) {
    return null;
  }

  if (typeof search === 'string') {
    return search.trim().length === 0
      ? [
          {
            field: ['search'],
            message: 'Local proxy refuses blank bulk search selectors to avoid broad destructive discount writes.',
            code: 'INVALID',
            extraInfo: null,
          },
        ]
      : null;
  }

  return [
    {
      field: null,
      message:
        'Local proxy refuses discount bulk mutations without ids, search, or savedSearchId to avoid broad destructive writes.',
      code: 'INVALID',
      extraInfo: null,
    },
  ];
}

function validateRedeemCodeBulkAdd(args: Record<string, unknown>): DiscountMutationUserError[] | null {
  const discountError = validateKnownCodeDiscountId(args['discountId']);
  if (discountError) {
    return discountError.map((error) => ({ ...error, field: ['discountId'] }));
  }

  const codes = readRedeemCodeInputs(args['codes']);
  if (codes.length === 0) {
    return [
      {
        field: ['codes'],
        message: "Codes can't be blank",
        code: 'BLANK',
        extraInfo: null,
      },
    ];
  }

  return validateCodeUniqueness(codes, ['codes']);
}

function validateRedeemCodeBulkDelete(args: Record<string, unknown>): DiscountMutationUserError[] | null {
  const discountError = validateKnownCodeDiscountId(args['discountId']);
  if (discountError) {
    return discountError.map((error) => ({ ...error, field: ['discountId'] }));
  }

  const conflict = validateBulkSelectorConflict(args, "Only one of 'ids', 'search' or 'saved_search_id' is allowed.");
  if (conflict) {
    return conflict;
  }

  if (typeof args['search'] === 'string' || typeof args['savedSearchId'] === 'string') {
    return [
      {
        field: typeof args['search'] === 'string' ? ['search'] : ['savedSearchId'],
        message:
          'Local proxy only supports id-scoped redeem-code bulk delete and refuses search selectors to avoid broad destructive writes.',
        code: 'INVALID',
        extraInfo: null,
      },
    ];
  }

  const ids = readStringArray(args['ids']);
  if (ids.length === 0) {
    return [
      {
        field: ['ids'],
        message: 'Redeem-code bulk delete requires one or more redeem code IDs for local staging.',
        code: 'BLANK',
        extraInfo: null,
      },
    ];
  }

  const existingDiscount = findCodeDiscountById(args['discountId']);
  const existingIds = new Set(existingDiscount ? getDiscountCodes(existingDiscount).map((code) => code.id) : []);
  if (ids.some((id) => !existingIds.has(id))) {
    return [
      {
        field: ['ids'],
        message: 'Redeem code does not exist',
        code: null,
        extraInfo: null,
      },
    ];
  }

  return null;
}

type DiscountBulkSelectorResolution = {
  targets: DiscountRecord[];
  query: string | null;
  selector: Record<string, JsonValue>;
  userErrors: DiscountMutationUserError[] | null;
};

function resolveDiscountBulkSelector(
  args: Record<string, unknown>,
  method: DiscountRecord['method'],
): DiscountBulkSelectorResolution {
  const ids = readStringArray(args['ids']);
  if (ids.length > 0) {
    const idSet = new Set(ids);
    const targets = store
      .listEffectiveDiscounts()
      .filter((discount) => discount.method === method && idSet.has(discount.id));
    const foundIds = new Set(targets.map((discount) => discount.id));
    const missingId = ids.find((id) => !foundIds.has(id));
    if (missingId) {
      return {
        targets: [],
        query: null,
        selector: { ids },
        userErrors: [
          {
            field: ['ids'],
            message: `Discount ${missingId} does not exist`,
            code: null,
            extraInfo: null,
          },
        ],
      };
    }

    return { targets, query: null, selector: { ids }, userErrors: null };
  }

  const search = readString(args['search']);
  if (search !== null) {
    const candidates = store.listEffectiveDiscounts().filter((discount) => discount.method === method);
    return {
      targets: filterDiscountsByQuery(candidates, search),
      query: search,
      selector: { search },
      userErrors: null,
    };
  }

  const savedSearchId = readString(args['savedSearchId']);
  if (savedSearchId !== null) {
    const savedSearch = store.getEffectiveSavedSearchById(savedSearchId);
    if (!savedSearch) {
      return {
        targets: [],
        query: null,
        selector: { savedSearchId },
        userErrors: [
          {
            field: ['savedSearchId'],
            message: 'Saved search does not exist',
            code: null,
            extraInfo: null,
          },
        ],
      };
    }

    const candidates = store.listEffectiveDiscounts().filter((discount) => discount.method === method);
    return {
      targets: filterDiscountsByQuery(candidates, savedSearch.query),
      query: savedSearch.query,
      selector: { savedSearchId, query: savedSearch.query },
      userErrors: null,
    };
  }

  return { targets: [], query: null, selector: {}, userErrors: null };
}

function stageDiscountBulkMutation(
  operation: Extract<
    DiscountBulkOperationRecord['operation'],
    'discountCodeBulkActivate' | 'discountCodeBulkDeactivate' | 'discountCodeBulkDelete' | 'discountAutomaticBulkDelete'
  >,
  args: Record<string, unknown>,
): { operation: DiscountBulkOperationRecord; targetIds: string[]; userErrors: DiscountMutationUserError[] | null } {
  const method: DiscountRecord['method'] = operation === 'discountAutomaticBulkDelete' ? 'automatic' : 'code';
  const resolution = resolveDiscountBulkSelector(args, method);
  if (resolution.userErrors) {
    return {
      operation: buildDiscountBulkOperation(operation, '', {
        id: makeProxySyntheticGid('Job'),
        typeName: 'Job',
        query: resolution.query,
        selector: resolution.selector,
      }),
      targetIds: [],
      userErrors: resolution.userErrors,
    };
  }

  const targetIds = resolution.targets.map((discount) => discount.id);
  const now = makeSyntheticTimestamp();
  for (const discount of resolution.targets) {
    if (operation === 'discountCodeBulkActivate') {
      store.stageCreateDiscount({ ...structuredClone(discount), status: 'ACTIVE', updatedAt: now });
    } else if (operation === 'discountCodeBulkDeactivate') {
      store.stageCreateDiscount({ ...structuredClone(discount), status: 'EXPIRED', updatedAt: now });
    } else {
      store.stageDeleteDiscount(discount.id);
    }
  }

  return {
    operation: store.stageDiscountBulkOperation(
      buildDiscountBulkOperation(operation, targetIds[0] ?? '', {
        id: makeProxySyntheticGid('Job'),
        typeName: 'Job',
        query: resolution.query,
        discountIds: targetIds,
        selector: resolution.selector,
      }),
    ),
    targetIds,
    userErrors: null,
  };
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

function filterDiscountsByQuery(discounts: DiscountRecord[], rawQuery: unknown): DiscountRecord[] {
  return applySearchQueryTerms(
    discounts,
    rawQuery,
    { ignoredKeywords: ['AND'], dropEmptyValues: true },
    matchesPositiveDiscountTerm,
  );
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

function listAutomaticDiscountsForField(field: FieldNode, variables: Record<string, unknown>): DiscountRecord[] {
  return listDiscountsForField(field, variables).filter((discount) => discount.method === 'automatic');
}

function findDiscountById(id: unknown): DiscountRecord | null {
  if (typeof id !== 'string' || id.length === 0) {
    return null;
  }

  return store.listEffectiveDiscounts().find((discount) => discount.id === id) ?? null;
}

function findAutomaticDiscountById(id: unknown, typeName: string | null = null): DiscountRecord | null {
  const discount = findDiscountById(id);
  return discount?.method === 'automatic' && (typeName === null || discount.typeName === typeName) ? discount : null;
}

function findAutomaticBasicDiscountById(id: unknown): DiscountRecord | null {
  return findAutomaticDiscountById(id, 'DiscountAutomaticBasic');
}

function resolveDiscountStatus(startsAt: string | null, endsAt: string | null): string {
  const nowTime = Date.now();
  const startsAtTime = startsAt ? Date.parse(startsAt) : Number.NaN;
  const endsAtTime = endsAt ? Date.parse(endsAt) : Number.NaN;

  if (Number.isFinite(endsAtTime) && endsAtTime <= nowTime) {
    return 'EXPIRED';
  }

  if (Number.isFinite(startsAtTime) && startsAtTime > nowTime) {
    return 'SCHEDULED';
  }

  return 'ACTIVE';
}

function readCombinesWith(
  value: unknown,
  fallback: DiscountCombinesWithRecord = {
    productDiscounts: false,
    orderDiscounts: false,
    shippingDiscounts: false,
  },
): DiscountCombinesWithRecord {
  const input = readRecord(value);
  if (!input) {
    return structuredClone(fallback);
  }

  return {
    productDiscounts: readBoolean(input['productDiscounts'], fallback.productDiscounts),
    orderDiscounts: readBoolean(input['orderDiscounts'], fallback.orderDiscounts),
    shippingDiscounts: readBoolean(input['shippingDiscounts'], fallback.shippingDiscounts),
  };
}

function readDiscountContext(
  value: unknown,
  fallback: DiscountContextRecord | null | undefined,
): DiscountContextRecord | null {
  const input = readRecord(value);
  if (!input) {
    return fallback ? structuredClone(fallback) : { typeName: 'DiscountBuyerSelectionAll', all: 'ALL' };
  }

  if (input['all'] !== undefined) {
    return { typeName: 'DiscountBuyerSelectionAll', all: 'ALL' };
  }

  const customers = readNestedRecord(input, 'customers');
  const segments = readNestedRecord(input, 'customerSegments');
  const customerIds = readStringArray(customers?.['add']);
  const customerSegmentIds = readStringArray(segments?.['add']);
  if (customerIds.length > 0) {
    return { typeName: 'DiscountCustomers', customerIds };
  }
  if (customerSegmentIds.length > 0) {
    return { typeName: 'DiscountCustomerSegments', customerSegmentIds };
  }

  return fallback ? structuredClone(fallback) : { typeName: 'DiscountBuyerSelectionAll', all: 'ALL' };
}

function readDiscountItems(value: unknown, fallback: DiscountItemsRecord | null | undefined): DiscountItemsRecord {
  const input = readRecord(value);
  if (!input) {
    return fallback ? structuredClone(fallback) : { typeName: 'AllDiscountItems', allItems: true };
  }

  if (input['all'] === true) {
    return { typeName: 'AllDiscountItems', allItems: true };
  }

  const products = readNestedRecord(input, 'products');
  const collections = readNestedRecord(input, 'collections');
  const productIds = readStringArray(products?.['productsToAdd']);
  const productVariantIds = readStringArray(products?.['productVariantsToAdd']);
  const collectionIds = readStringArray(collections?.['add']);

  if (collectionIds.length > 0) {
    return { typeName: 'DiscountCollections', collectionIds };
  }

  if (productIds.length > 0 || productVariantIds.length > 0) {
    return { typeName: 'DiscountProducts', productIds, productVariantIds };
  }

  return fallback ? structuredClone(fallback) : { typeName: 'AllDiscountItems', allItems: true };
}

function readDiscountValue(value: unknown, fallback: DiscountValueRecord | null | undefined): DiscountValueRecord {
  const input = readRecord(value);
  const fallbackValue = fallback ? structuredClone(fallback) : null;
  if (!input) {
    return fallbackValue ?? { typeName: 'DiscountPercentage', percentage: 0 };
  }

  const percentage = readNumber(input['percentage']);
  if (percentage !== null) {
    return {
      typeName: 'DiscountPercentage',
      percentage,
    };
  }

  const discountAmount = readNestedRecord(input, 'discountAmount');
  if (discountAmount) {
    return {
      typeName: 'DiscountAmount',
      amount: {
        amount: normalizeDiscountMoneyAmount(
          readString(discountAmount['amount'], fallbackValue?.amount?.amount ?? '0.0'),
        ),
        currencyCode: fallbackValue?.amount?.currencyCode ?? 'CAD',
      },
      appliesOnEachItem: readBoolean(discountAmount['appliesOnEachItem'], fallbackValue?.appliesOnEachItem ?? false),
    };
  }

  return fallbackValue ?? { typeName: 'DiscountPercentage', percentage: 0 };
}

function readCustomerGets(
  value: unknown,
  fallback: DiscountCustomerGetsRecord | null | undefined,
): DiscountCustomerGetsRecord {
  const input = readRecord(value);
  if (!input) {
    return (
      structuredClone(fallback) ?? {
        value: { typeName: 'DiscountPercentage', percentage: 0 },
        items: { typeName: 'AllDiscountItems', allItems: true },
        appliesOnOneTimePurchase: true,
        appliesOnSubscription: false,
      }
    );
  }

  return {
    value: readDiscountValue(input['value'], fallback?.value),
    items: readDiscountItems(input['items'], fallback?.items),
    appliesOnOneTimePurchase: readBoolean(
      input['appliesOnOneTimePurchase'],
      fallback?.appliesOnOneTimePurchase ?? true,
    ),
    appliesOnSubscription: readBoolean(input['appliesOnSubscription'], fallback?.appliesOnSubscription ?? false),
  };
}

function readDiscountEffect(value: unknown, fallback: DiscountEffectRecord | null | undefined): DiscountEffectRecord {
  const input = readRecord(value);
  const fallbackEffect = fallback ? structuredClone(fallback) : null;
  if (!input) {
    return fallbackEffect ?? { typeName: 'DiscountPercentage', percentage: 0 };
  }

  const percentage = readNumber(input['percentage']);
  if (percentage !== null) {
    return {
      typeName: 'DiscountPercentage',
      percentage,
    };
  }

  const fixedAmount = readNestedRecord(input, 'fixedAmount') ?? readNestedRecord(input, 'discountAmount');
  if (fixedAmount) {
    return {
      typeName: 'DiscountAmount',
      amount: {
        amount: normalizeDiscountMoneyAmount(
          readString(fixedAmount['amount'], fallbackEffect?.amount?.amount ?? '0.0'),
        ),
        currencyCode: readString(fixedAmount['currencyCode'], fallbackEffect?.amount?.currencyCode ?? 'USD'),
      },
      appliesOnEachItem: readBoolean(fixedAmount['appliesOnEachItem'], fallbackEffect?.appliesOnEachItem ?? false),
    };
  }

  return fallbackEffect ?? { typeName: 'DiscountPercentage', percentage: 0 };
}

function readDiscountQuantityValue(
  value: unknown,
  fallback: DiscountValueRecord | null | undefined,
  typeName: 'DiscountQuantity' | 'DiscountPurchaseAmount' = 'DiscountQuantity',
): DiscountValueRecord {
  const input = readRecord(value);
  if (!input) {
    return structuredClone(fallback) ?? { typeName, quantity: '1' };
  }

  const amount = readMoneyAmount(input['amount']);
  if (typeName === 'DiscountPurchaseAmount' || amount !== null) {
    return {
      typeName: 'DiscountPurchaseAmount',
      amount: {
        amount: amount ?? fallback?.amount?.amount ?? '0.0',
        currencyCode: readString(input['currencyCode'], fallback?.amount?.currencyCode ?? 'USD'),
      },
    };
  }

  return {
    typeName: 'DiscountQuantity',
    quantity: readString(input['quantity'], fallback?.quantity ?? '1'),
  };
}

function readBxgyCustomerBuys(
  value: unknown,
  fallback: DiscountCustomerBuysRecord | null | undefined,
): DiscountCustomerBuysRecord {
  const input = readRecord(value);
  if (!input) {
    return (
      structuredClone(fallback) ?? {
        value: { typeName: 'DiscountQuantity', quantity: '1' },
        items: { typeName: 'AllDiscountItems', allItems: true },
      }
    );
  }

  return {
    value: readDiscountQuantityValue(input['value'], fallback?.value),
    items: readDiscountItems(input['items'], fallback?.items),
  };
}

function readBxgyCustomerGets(
  value: unknown,
  fallback: DiscountCustomerGetsRecord | null | undefined,
): DiscountCustomerGetsRecord {
  const input = readRecord(value);
  if (!input) {
    return (
      structuredClone(fallback) ?? {
        value: {
          typeName: 'DiscountOnQuantity',
          quantity: '1',
          effect: { typeName: 'DiscountPercentage', percentage: 1 },
        },
        items: { typeName: 'AllDiscountItems', allItems: true },
        appliesOnOneTimePurchase: true,
        appliesOnSubscription: false,
      }
    );
  }

  const valueInput = readNestedRecord(input, 'value');
  const discountOnQuantity = readNestedRecord(valueInput, 'discountOnQuantity');
  const fallbackValue = fallback?.value;
  return {
    value: {
      typeName: 'DiscountOnQuantity',
      quantity: readString(discountOnQuantity?.['quantity'], fallbackValue?.quantity ?? '1'),
      effect: readDiscountEffect(discountOnQuantity?.['effect'], fallbackValue?.effect),
    },
    items: readDiscountItems(input['items'], fallback?.items),
    appliesOnOneTimePurchase: readBoolean(
      input['appliesOnOneTimePurchase'],
      fallback?.appliesOnOneTimePurchase ?? true,
    ),
    appliesOnSubscription: readBoolean(input['appliesOnSubscription'], fallback?.appliesOnSubscription ?? false),
  };
}

function readMinimumRequirement(
  value: unknown,
  fallback: DiscountMinimumRequirementRecord | null | undefined,
): DiscountMinimumRequirementRecord | null {
  const input = readRecord(value);
  if (!input) {
    return fallback ? structuredClone(fallback) : null;
  }

  const quantity = readNestedRecord(input, 'quantity');
  if (quantity) {
    const greaterThanOrEqualToQuantity = readNullableString(quantity['greaterThanOrEqualToQuantity'], null);
    return greaterThanOrEqualToQuantity === null
      ? null
      : {
          typeName: 'DiscountMinimumQuantity',
          greaterThanOrEqualToQuantity,
        };
  }

  const subtotal = readNestedRecord(input, 'subtotal');
  if (subtotal) {
    const greaterThanOrEqualToSubtotal = readNullableString(subtotal['greaterThanOrEqualToSubtotal'], null);
    return greaterThanOrEqualToSubtotal === null
      ? null
      : {
          typeName: 'DiscountMinimumSubtotal',
          greaterThanOrEqualToSubtotal: {
            amount: normalizeDiscountMoneyAmount(greaterThanOrEqualToSubtotal),
            currencyCode: fallback?.greaterThanOrEqualToSubtotal?.currencyCode ?? 'CAD',
          },
        };
  }

  return fallback ? structuredClone(fallback) : null;
}

function readDiscountDestinationSelection(
  value: unknown,
  fallback: DiscountDestinationSelectionRecord | null | undefined,
): DiscountDestinationSelectionRecord {
  const input = readRecord(value);
  if (!input) {
    return fallback ? structuredClone(fallback) : { typeName: 'DiscountCountryAll', allCountries: true };
  }

  if (input['all'] === true) {
    return { typeName: 'DiscountCountryAll', allCountries: true };
  }

  const countries = readNestedRecord(input, 'countries');
  const countryCodes = readStringArray(countries?.['add']).sort();
  if (countryCodes.length > 0 || countries?.['includeRestOfWorld'] !== undefined) {
    return {
      typeName: 'DiscountCountries',
      countries: countryCodes,
      includeRestOfWorld: readBoolean(countries?.['includeRestOfWorld'], fallback?.includeRestOfWorld ?? false),
    };
  }

  return fallback ? structuredClone(fallback) : { typeName: 'DiscountCountryAll', allCountries: true };
}

function readShippingPrice(
  value: unknown,
  fallback: DiscountRecord['maximumShippingPrice'],
): DiscountRecord['maximumShippingPrice'] {
  const amount = readMoneyAmount(value);
  if (amount === null) {
    return fallback ? structuredClone(fallback) : null;
  }

  return {
    amount: normalizeDiscountMoneyAmount(amount),
    currencyCode: fallback?.currencyCode ?? 'CAD',
  };
}

function formatMoneyAmountForSummary(
  money: { amount: string; currencyCode: string } | null | undefined,
): string | null {
  if (!money?.amount) {
    return null;
  }

  const amount = Number.parseFloat(money.amount);
  return Number.isFinite(amount) ? amount.toFixed(2) : money.amount;
}

function buildFreeShippingSummary(discount: DiscountRecord): string {
  const productScope =
    discount.appliesOnOneTimePurchase === false && discount.appliesOnSubscription === true
      ? 'subscription products'
      : discount.method === 'code' &&
          discount.appliesOnOneTimePurchase === true &&
          discount.appliesOnSubscription !== true
        ? 'one-time purchase products'
        : 'all products';
  const parts = [`Free shipping on ${productScope}`];
  const subtotal = formatMoneyAmountForSummary(discount.minimumRequirement?.greaterThanOrEqualToSubtotal);
  if (subtotal) {
    parts.push(`Minimum purchase of $${subtotal}`);
  } else if (discount.minimumRequirement?.greaterThanOrEqualToQuantity) {
    parts.push(`Minimum quantity of ${discount.minimumRequirement.greaterThanOrEqualToQuantity}`);
  }

  const destination = discount.destinationSelection;
  if (destination?.typeName === 'DiscountCountryAll') {
    parts.push('For all countries');
  } else if (destination?.typeName === 'DiscountCountries') {
    const countries = destination.countries ?? [];
    if (countries.length === 1) {
      parts.push(countries[0] === 'US' ? 'For United States' : `For ${countries[0]}`);
    } else if (countries.length > 1) {
      parts.push(`For ${countries.length} countries`);
    }
  }

  const maximumShippingPrice = formatMoneyAmountForSummary(discount.maximumShippingPrice);
  if (maximumShippingPrice) {
    parts.push(`Applies to shipping rates under $${maximumShippingPrice}`);
  }

  if (discount.method === 'code' && discount.appliesOncePerCustomer === true) {
    parts.push('One use per customer');
  }

  return parts.join(' • ');
}

function formatDiscountPercentage(value: number | null | undefined): string {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return '0%';
  }

  const percentage = value * 100;
  return `${Number.isInteger(percentage) ? percentage.toFixed(0) : percentage.toFixed(2)}%`;
}

function buildAutomaticDiscountSummary(discount: DiscountRecord): string {
  const value = discount.customerGets?.value;
  const discountText =
    value?.typeName === 'DiscountAmount'
      ? `${value.amount?.amount ?? '0.00'} ${value.amount?.currencyCode ?? 'USD'} off`
      : `${formatDiscountPercentage(value?.percentage)} off`;
  const requirement = discount.minimumRequirement;
  const requirementText =
    requirement?.typeName === 'DiscountMinimumQuantity'
      ? ` - Minimum quantity of ${requirement.greaterThanOrEqualToQuantity ?? ''}`
      : requirement?.typeName === 'DiscountMinimumSubtotal'
        ? ` - Minimum purchase of ${requirement.greaterThanOrEqualToSubtotal?.amount ?? ''} ${
            requirement.greaterThanOrEqualToSubtotal?.currencyCode ?? ''
          }`.trimEnd()
        : '';

  return `${discountText} entire order${requirementText}`;
}

function buildAutomaticBasicDiscount(
  input: Record<string, unknown>,
  now: string,
  existing?: DiscountRecord | null,
): DiscountRecord {
  const startsAt = readNullableString(input['startsAt'], existing?.startsAt ?? now);
  const endsAt = readNullableString(input['endsAt'], existing?.endsAt ?? null);
  const discount: DiscountRecord = {
    id: existing?.id ?? makeProxySyntheticGid('DiscountAutomaticNode'),
    typeName: 'DiscountAutomaticBasic',
    method: 'automatic',
    title: readString(input['title'], existing?.title ?? ''),
    status: resolveDiscountStatus(startsAt, endsAt),
    summary: existing?.summary ?? null,
    startsAt,
    endsAt,
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
    asyncUsageCount: existing?.asyncUsageCount ?? 0,
    discountClasses: existing?.discountClasses ?? ['ORDER'],
    combinesWith: readCombinesWith(input['combinesWith'], existing?.combinesWith),
    codes: [],
    context: ownsKey(input, 'context') ? readDiscountContext(input['context'], existing?.context) : existing?.context,
    customerGets: ownsKey(input, 'customerGets')
      ? readCustomerGets(input['customerGets'], existing?.customerGets)
      : existing?.customerGets,
    minimumRequirement: ownsKey(input, 'minimumRequirement')
      ? readMinimumRequirement(input['minimumRequirement'], existing?.minimumRequirement)
      : existing?.minimumRequirement,
    metafields: existing?.metafields ? structuredClone(existing.metafields) : [],
    events: existing?.events ? structuredClone(existing.events) : [],
    discountType:
      (ownsKey(input, 'customerGets')
        ? readDiscountValue(readNestedRecord(input, 'customerGets')?.['value'], null)
        : null
      )?.typeName === 'DiscountAmount' || existing?.discountType === 'fixed_amount'
        ? 'fixed_amount'
        : 'percentage',
  };
  discount.summary = buildAutomaticDiscountSummary(discount);

  return discount;
}

function formatBxgyEffect(effect: DiscountEffectRecord | null | undefined): string {
  if (effect?.typeName === 'DiscountAmount') {
    const amount = effect.amount?.amount ?? '0.00';
    const currencyCode = effect.amount?.currencyCode ?? 'USD';
    return `${amount} ${currencyCode} off`;
  }

  const percentage = effect?.percentage ?? 0;
  return percentage >= 1 ? 'free' : `at ${formatDiscountPercentage(percentage)} off`;
}

function buildBxgyDiscountSummary(discount: DiscountRecord): string {
  const buysQuantity = discount.customerBuys?.value.quantity ?? '1';
  const getsQuantity = discount.customerGets?.value.quantity ?? '1';
  return `Buy ${buysQuantity} item${buysQuantity === '1' ? '' : 's'}, get ${getsQuantity} item${
    getsQuantity === '1' ? '' : 's'
  } ${formatBxgyEffect(discount.customerGets?.value.effect)}`;
}

function buildBxgyDiscount(
  input: Record<string, unknown>,
  method: 'code' | 'automatic',
  now: string,
  existing?: DiscountRecord | null,
): DiscountRecord {
  const startsAt = readNullableString(input['startsAt'], existing?.startsAt ?? now);
  const endsAt = readNullableString(input['endsAt'], existing?.endsAt ?? null);
  const isCodeDiscount = method === 'code';
  const code = isCodeDiscount ? readDiscountCode(input, existing?.codes[0] ?? existing?.redeemCodes?.[0]?.code) : null;
  const customerBuys = ownsKey(input, 'customerBuys')
    ? readBxgyCustomerBuys(input['customerBuys'], existing?.customerBuys)
    : (existing?.customerBuys ?? null);
  const customerGets = ownsKey(input, 'customerGets')
    ? readBxgyCustomerGets(input['customerGets'], existing?.customerGets)
    : (existing?.customerGets ?? null);
  const discount: DiscountRecord = {
    id: existing?.id ?? makeProxySyntheticGid(isCodeDiscount ? 'DiscountCodeNode' : 'DiscountAutomaticNode'),
    typeName: isCodeDiscount ? 'DiscountCodeBxgy' : 'DiscountAutomaticBxgy',
    method,
    title: readString(input['title'], existing?.title ?? code ?? 'BXGY discount'),
    status: resolveDiscountStatus(startsAt, endsAt),
    summary: existing?.summary ?? null,
    startsAt,
    endsAt,
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
    asyncUsageCount: existing?.asyncUsageCount ?? 0,
    usageLimit: readInteger(input['usageLimit'], existing?.usageLimit ?? null),
    usesPerOrderLimit: readInteger(input['usesPerOrderLimit'], existing?.usesPerOrderLimit ?? null),
    discountClasses: ['PRODUCT'],
    combinesWith: readCombinesWith(input['combinesWith'], existing?.combinesWith),
    codes: code ? [code] : [],
    redeemCodes: code
      ? [
          {
            id: existing?.redeemCodes?.[0]?.id ?? makeProxySyntheticGid('DiscountRedeemCode'),
            code,
            asyncUsageCount: existing?.redeemCodes?.[0]?.asyncUsageCount ?? 0,
          },
        ]
      : [],
    context: ownsKey(input, 'context') ? readDiscountContext(input['context'], existing?.context) : existing?.context,
    customerBuys,
    customerGets,
    minimumRequirement: existing?.minimumRequirement ?? null,
    metafields: existing?.metafields ? structuredClone(existing.metafields) : [],
    events: existing?.events ? structuredClone(existing.events) : [],
    discountType: 'bogo',
  };
  discount.summary = buildBxgyDiscountSummary(discount);

  return discount;
}

function findCodeBxgyDiscountById(id: unknown): DiscountRecord | null {
  const discount = findDiscountById(id);
  return discount?.method === 'code' && discount.typeName === 'DiscountCodeBxgy' ? discount : null;
}

function findAutomaticBxgyDiscountById(id: unknown): DiscountRecord | null {
  return findAutomaticDiscountById(id, 'DiscountAutomaticBxgy');
}

function stageCodeBxgyCreate(input: Record<string, unknown>): DiscountRecord {
  return store.stageCreateDiscount(buildBxgyDiscount(input, 'code', makeSyntheticTimestamp()));
}

function stageCodeBxgyUpdate(id: string, input: Record<string, unknown>): DiscountRecord {
  const existing = findCodeBxgyDiscountById(id);
  return store.stageCreateDiscount(buildBxgyDiscount(input, 'code', makeSyntheticTimestamp(), existing));
}

function stageAutomaticBxgyCreate(input: Record<string, unknown>): DiscountRecord {
  const now = makeSyntheticTimestamp();
  const discount = stageAutomaticDiscountEvent(buildBxgyDiscount(input, 'automatic', now), 'create', now);
  return store.stageCreateDiscount(discount);
}

function stageAutomaticBxgyUpdate(id: string, input: Record<string, unknown>): DiscountRecord {
  const existing = findAutomaticBxgyDiscountById(id);
  const now = makeSyntheticTimestamp();
  const discount = stageAutomaticDiscountEvent(buildBxgyDiscount(input, 'automatic', now, existing), 'update', now);
  return store.stageCreateDiscount(discount);
}

function buildAutomaticFreeShippingDiscount(
  input: Record<string, unknown>,
  now: string,
  existing?: DiscountRecord | null,
): DiscountRecord {
  const startsAt = readNullableString(input['startsAt'], existing?.startsAt ?? now);
  const endsAt = readNullableString(input['endsAt'], existing?.endsAt ?? null);
  const discount: DiscountRecord = {
    id: existing?.id ?? makeProxySyntheticGid('DiscountAutomaticNode'),
    typeName: 'DiscountAutomaticFreeShipping',
    method: 'automatic',
    title: readString(input['title'], existing?.title ?? ''),
    status: resolveDiscountStatus(startsAt, endsAt),
    summary: existing?.summary ?? null,
    startsAt,
    endsAt,
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
    asyncUsageCount: existing?.asyncUsageCount ?? 0,
    discountClasses: ['SHIPPING'],
    combinesWith: readCombinesWith(input['combinesWith'], existing?.combinesWith),
    codes: [],
    context: ownsKey(input, 'context') ? readDiscountContext(input['context'], existing?.context) : existing?.context,
    customerGets: null,
    minimumRequirement: ownsKey(input, 'minimumRequirement')
      ? readMinimumRequirement(input['minimumRequirement'], existing?.minimumRequirement)
      : existing?.minimumRequirement,
    destinationSelection: ownsKey(input, 'destination')
      ? readDiscountDestinationSelection(input['destination'], existing?.destinationSelection)
      : (existing?.destinationSelection ?? { typeName: 'DiscountCountryAll', allCountries: true }),
    maximumShippingPrice: ownsKey(input, 'maximumShippingPrice')
      ? readShippingPrice(input['maximumShippingPrice'], existing?.maximumShippingPrice)
      : (existing?.maximumShippingPrice ?? null),
    appliesOnOneTimePurchase: readBoolean(
      input['appliesOnOneTimePurchase'],
      existing?.appliesOnOneTimePurchase ?? true,
    ),
    appliesOnSubscription: readBoolean(input['appliesOnSubscription'], existing?.appliesOnSubscription ?? false),
    recurringCycleLimit:
      typeof input['recurringCycleLimit'] === 'number'
        ? input['recurringCycleLimit']
        : (existing?.recurringCycleLimit ?? 0),
    metafields: existing?.metafields ? structuredClone(existing.metafields) : [],
    events: existing?.events ? structuredClone(existing.events) : [],
    discountType: 'free_shipping',
    appId: existing?.appId ?? null,
  };

  discount.summary = buildFreeShippingSummary(discount);
  return discount;
}

function stageAutomaticDiscountEvent(discount: DiscountRecord, action: string, now: string): DiscountRecord {
  return {
    ...discount,
    events: [
      ...(discount.events ?? []),
      {
        id: makeSyntheticGid('BasicEvent'),
        typeName: 'BasicEvent',
        action,
        message: `shopify-draft-proxy ${action}d this discount.`,
        createdAt: now,
        subjectId: discount.id,
        subjectType: 'PRICE_RULE',
      },
    ],
  };
}

function stageAutomaticBasicCreate(input: Record<string, unknown>): DiscountRecord {
  const now = makeSyntheticTimestamp();
  const discount = stageAutomaticDiscountEvent(buildAutomaticBasicDiscount(input, now), 'create', now);
  return store.stageCreateDiscount(discount);
}

function stageAutomaticBasicUpdate(id: string, input: Record<string, unknown>): DiscountRecord {
  const existing = findAutomaticBasicDiscountById(id);
  const now = makeSyntheticTimestamp();
  const discount = stageAutomaticDiscountEvent(buildAutomaticBasicDiscount(input, now, existing), 'update', now);
  return store.stageCreateDiscount(discount);
}

function stageAutomaticFreeShippingCreate(input: Record<string, unknown>): DiscountRecord {
  const now = makeSyntheticTimestamp();
  const discount = stageAutomaticDiscountEvent(buildAutomaticFreeShippingDiscount(input, now), 'create', now);
  return store.stageCreateDiscount(discount);
}

function stageAutomaticFreeShippingUpdate(id: string, input: Record<string, unknown>): DiscountRecord {
  const existing = findAutomaticDiscountById(id, 'DiscountAutomaticFreeShipping');
  const now = makeSyntheticTimestamp();
  const discount = stageAutomaticDiscountEvent(buildAutomaticFreeShippingDiscount(input, now, existing), 'update', now);
  return store.stageCreateDiscount(discount);
}

function stageAutomaticActivate(id: string): DiscountRecord {
  const existing = findAutomaticDiscountById(id);
  if (!existing) {
    throw new Error(`Cannot activate unknown automatic discount ${id}`);
  }

  const now = makeSyntheticTimestamp();
  const statusNow = Date.now();
  const startsAt = existing.startsAt && Date.parse(existing.startsAt) > statusNow ? now : existing.startsAt;
  const endsAt = existing.endsAt && Date.parse(existing.endsAt) <= statusNow ? null : existing.endsAt;
  const discount = stageAutomaticDiscountEvent(
    {
      ...existing,
      startsAt,
      endsAt,
      status: 'ACTIVE',
      updatedAt: now,
    },
    'activate',
    now,
  );
  return store.stageCreateDiscount(discount);
}

function stageAutomaticDeactivate(id: string): DiscountRecord {
  const existing = findAutomaticDiscountById(id);
  if (!existing) {
    throw new Error(`Cannot deactivate unknown automatic discount ${id}`);
  }

  const now = makeSyntheticTimestamp();
  const discount = stageAutomaticDiscountEvent(
    {
      ...existing,
      startsAt: existing.startsAt && Date.parse(existing.startsAt) > Date.now() ? now : existing.startsAt,
      endsAt: now,
      status: 'EXPIRED',
      updatedAt: now,
    },
    'deactivate',
    now,
  );
  return store.stageCreateDiscount(discount);
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
  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (code) => code.code,
    serializeNode: serializeCodeNode,
  });
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
      case 'quantity':
        result[key] =
          value.typeName === 'DiscountOnQuantity'
            ? serializeDiscountQuantity(value.quantity ?? null, selection)
            : (value.quantity ?? null);
        break;
      case 'effect':
        result[key] = serializeDiscountEffect(value.effect, selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDiscountEffect(
  effect: DiscountEffectRecord | null | undefined,
  field: FieldNode,
): Record<string, unknown> | null {
  if (!effect) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of selectedFieldsForConcreteType(field, effect.typeName)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = effect.typeName;
        break;
      case 'percentage':
        result[key] = effect.percentage ?? null;
        break;
      case 'amount':
        result[key] = serializeMoney(effect.amount, selection);
        break;
      case 'appliesOnEachItem':
        result[key] = effect.appliesOnEachItem ?? null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDiscountQuantity(quantity: string | null, field: FieldNode): Record<string, unknown> | null {
  if (quantity === null) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    result[key] = selection.name.value === 'quantity' ? quantity : null;
  }
  return result;
}

function serializeCustomerBuys(
  customerBuys: DiscountCustomerBuysRecord | null | undefined,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  if (!customerBuys) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'value':
        result[key] = serializeDiscountValue(customerBuys.value, selection);
        break;
      case 'items':
        result[key] = serializeDiscountItems(customerBuys.items, selection, variables);
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

function serializeDiscountDestinationSelection(
  destination: DiscountDestinationSelectionRecord | null | undefined,
  field: FieldNode,
): Record<string, unknown> | null {
  if (!destination) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of selectedFieldsForConcreteType(field, destination.typeName)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = destination.typeName;
        break;
      case 'allCountries':
        result[key] = destination.allCountries ?? true;
        break;
      case 'countries':
        result[key] = structuredClone(destination.countries ?? []);
        break;
      case 'includeRestOfWorld':
        result[key] = destination.includeRestOfWorld ?? false;
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
      case 'id':
        result[key] = discount.id;
        break;
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
        result[key] = discount.usageLimit ?? null;
        break;
      case 'recurringCycleLimit':
        result[key] =
          discount.recurringCycleLimit ?? (discount.typeName === 'DiscountAutomaticFreeShipping' ? 0 : null);
        break;
      case 'appliesOncePerCustomer':
        result[key] = discount.appliesOncePerCustomer ?? false;
        break;
      case 'appliesOnOneTimePurchase':
        result[key] = discount.appliesOnOneTimePurchase ?? true;
        break;
      case 'appliesOnSubscription':
        result[key] = discount.appliesOnSubscription ?? false;
        break;
      case 'destinationSelection':
        result[key] = serializeDiscountDestinationSelection(discount.destinationSelection, selection);
        break;
      case 'maximumShippingPrice':
        result[key] = serializeMoney(discount.maximumShippingPrice, selection);
        break;
      case 'usesPerOrderLimit':
        result[key] = discount.usesPerOrderLimit ?? null;
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
      case 'customerBuys':
        result[key] = serializeCustomerBuys(discount.customerBuys, selection, variables);
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
  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (discount) => discount.id,
    serializeNode: (discount, selection, _index, nodeContext) =>
      serializeDiscountNode(discount, selection, variables, {
        ...context,
        path: [...context.path, ...nodeContext.path],
      }),
  });
}

function serializeAutomaticDiscountNodesConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
  context: DiscountSerializationContext,
): Record<string, unknown> {
  const discounts = listAutomaticDiscountsForField(field, variables);
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
          serializeDiscountOwnerNode(discount, selection, variables, 'DiscountAutomaticNode', 'automaticDiscount', {
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
                edge[edgeKey] = serializeDiscountOwnerNode(
                  discount,
                  edgeSelection,
                  variables,
                  'DiscountAutomaticNode',
                  'automaticDiscount',
                  {
                    ...context,
                    path: [...context.path, key, String(index), edgeKey],
                  },
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

function serializeDiscountOwnerNodeById(
  discountId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
  context: DiscountSerializationContext,
): Record<string, unknown> | null {
  const discount = findDiscountById(discountId);
  if (!discount) {
    return null;
  }

  return discount.method === 'code'
    ? serializeDiscountOwnerNode(discount, field, variables, 'DiscountCodeNode', 'codeDiscount', context)
    : serializeDiscountOwnerNode(discount, field, variables, 'DiscountAutomaticNode', 'automaticDiscount', context);
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
      case 'automaticDiscountNodes':
        data[key] = serializeAutomaticDiscountNodesConnection(field, variables, { ...context, path: [key] });
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
): DiscountMutationHandling | null {
  const data: Record<string, unknown> = {};
  let handled = false;
  let staged = false;
  const stagedResourceIds: string[] = [];

  for (const field of getRootFields(document)) {
    const rootName = field.name.value;
    const key = getFieldResponseKey(field);
    const requiredArguments = discountMutationArgumentTypes[rootName] ?? {};
    for (const [argumentName, typeName] of Object.entries(requiredArguments)) {
      const validationError = validateRequiredArgument(field, variables, argumentName, typeName);
      if (validationError) {
        return {
          response: { errors: [validationError] },
          stagedResourceIds: [],
          notes: null,
          staged: false,
        };
      }
    }

    const args = getFieldArguments(field, variables);
    const nodeField = discountMutationNodeFieldByRoot[rootName] ?? null;
    let userErrors: DiscountMutationUserError[] | null = null;
    let discount: DiscountRecord | null = null;
    let deletedCodeDiscountId: string | null = null;

    switch (rootName) {
      case 'discountCodeAppCreate': {
        const input = readRecord(args['codeAppDiscount']);
        userErrors = validateDiscountAppCreate(input, 'codeAppDiscount', true);
        if (input && userErrors === null) {
          const discount = stageCodeAppCreate(input);
          handled = true;
          staged = true;
          stagedResourceIds.push(discount.id);
          data[key] = serializeAppDiscountMutationPayload(field, variables, discount, [], 'codeAppDiscount');
        }
        break;
      }
      case 'discountCodeAppUpdate': {
        const input = readRecord(args['codeAppDiscount']);
        userErrors =
          validateKnownAppDiscountId(args['id'], 'DiscountCodeApp') ??
          validateDiscountAppCreate(input, 'codeAppDiscount', true);
        if (typeof args['id'] === 'string' && input && userErrors === null) {
          const discount = stageCodeAppUpdate(args['id'], input);
          handled = true;
          staged = true;
          stagedResourceIds.push(discount.id);
          data[key] = serializeAppDiscountMutationPayload(field, variables, discount, [], 'codeAppDiscount');
        }
        break;
      }
      case 'discountAutomaticAppCreate': {
        const input = readRecord(args['automaticAppDiscount']);
        userErrors = validateDiscountAppCreate(input, 'automaticAppDiscount', false);
        if (input && userErrors === null) {
          const discount = stageAutomaticAppCreate(input);
          handled = true;
          staged = true;
          stagedResourceIds.push(discount.id);
          data[key] = serializeAppDiscountMutationPayload(field, variables, discount, [], 'automaticAppDiscount');
        }
        break;
      }
      case 'discountAutomaticAppUpdate': {
        const input = readRecord(args['automaticAppDiscount']);
        userErrors =
          validateKnownAppDiscountId(args['id'], 'DiscountAutomaticApp') ??
          validateDiscountAppCreate(input, 'automaticAppDiscount', false);
        if (typeof args['id'] === 'string' && input && userErrors === null) {
          const discount = stageAutomaticAppUpdate(args['id'], input);
          handled = true;
          staged = true;
          stagedResourceIds.push(discount.id);
          data[key] = serializeAppDiscountMutationPayload(field, variables, discount, [], 'automaticAppDiscount');
        }
        break;
      }
      case 'discountCodeBasicCreate': {
        const input = readRecord(args['basicCodeDiscount']);
        userErrors = validateDiscountCodeBasicCreate(input);
        if (input && userErrors === null) {
          discount = stageCodeBasicCreate(input);
          staged = true;
          stagedResourceIds.push(discount.id);
          userErrors = [];
        }
        break;
      }
      case 'discountAutomaticBasicCreate':
        userErrors = validateDiscountAutomaticBasicCreate(readRecord(args['automaticBasicDiscount']));
        if (userErrors === null) {
          const discount = stageAutomaticBasicCreate(readRecord(args['automaticBasicDiscount']) ?? {});
          handled = true;
          staged = true;
          stagedResourceIds.push(discount.id);
          data[key] = serializeAutomaticDiscountMutationPayload(field, variables, discount, []);
        }
        break;
      case 'discountAutomaticBasicUpdate':
        userErrors =
          validateAutomaticDiscountExists(args['id'], 'DiscountAutomaticBasic') ??
          validateDiscountAutomaticBasicCreate(readRecord(args['automaticBasicDiscount']));
        if (userErrors === null && typeof args['id'] === 'string') {
          const discount = stageAutomaticBasicUpdate(args['id'], readRecord(args['automaticBasicDiscount']) ?? {});
          handled = true;
          staged = true;
          stagedResourceIds.push(discount.id);
          data[key] = serializeAutomaticDiscountMutationPayload(field, variables, discount, []);
        }
        break;
      case 'discountAutomaticActivate':
        userErrors = validateAutomaticDiscountExists(args['id']);
        if (userErrors === null && typeof args['id'] === 'string') {
          const discount = stageAutomaticActivate(args['id']);
          handled = true;
          staged = true;
          stagedResourceIds.push(discount.id);
          data[key] = serializeAutomaticDiscountMutationPayload(field, variables, discount, []);
        }
        break;
      case 'discountAutomaticDeactivate':
        userErrors = validateAutomaticDiscountExists(args['id']);
        if (userErrors === null && typeof args['id'] === 'string') {
          const discount = stageAutomaticDeactivate(args['id']);
          handled = true;
          staged = true;
          stagedResourceIds.push(discount.id);
          data[key] = serializeAutomaticDiscountMutationPayload(field, variables, discount, []);
        }
        break;
      case 'discountAutomaticDelete':
        userErrors = validateAutomaticDiscountExists(args['id']);
        if (userErrors === null && typeof args['id'] === 'string') {
          store.stageDeleteDiscount(args['id']);
          handled = true;
          staged = true;
          stagedResourceIds.push(args['id']);
          data[key] = serializeAutomaticDiscountDeletePayload(field, args['id'], []);
        }
        break;
      case 'discountCodeBasicUpdate': {
        const input = readRecord(args['basicCodeDiscount']);
        userErrors = validateDiscountCodeBasicUpdate(args['id'], input);
        if (typeof args['id'] === 'string' && input && userErrors === null) {
          discount = stageCodeBasicUpdate(args['id'], input);
          staged = true;
          stagedResourceIds.push(discount.id);
          userErrors = [];
        }
        break;
      }
      case 'discountCodeActivate':
        userErrors = validateKnownCodeDiscountId(args['id']);
        if (typeof args['id'] === 'string' && userErrors === null) {
          discount = stageCodeStatus(args['id'], 'ACTIVE');
          staged = true;
          stagedResourceIds.push(discount.id);
          userErrors = [];
        }
        break;
      case 'discountCodeDeactivate':
        userErrors = validateKnownCodeDiscountId(args['id']);
        if (typeof args['id'] === 'string' && userErrors === null) {
          discount = stageCodeStatus(args['id'], 'EXPIRED');
          staged = true;
          stagedResourceIds.push(discount.id);
          userErrors = [];
        }
        break;
      case 'discountCodeDelete':
        userErrors = validateKnownCodeDiscountId(args['id']);
        if (typeof args['id'] === 'string' && userErrors === null) {
          store.stageDeleteDiscount(args['id']);
          deletedCodeDiscountId = args['id'];
          staged = true;
          stagedResourceIds.push(args['id']);
          userErrors = [];
        }
        break;
      case 'discountRedeemCodeBulkAdd': {
        const codes = readRedeemCodeInputs(args['codes']);
        userErrors = validateRedeemCodeBulkAdd(args);
        if (typeof args['discountId'] === 'string' && userErrors === null) {
          const operation = stageRedeemCodeBulkAdd(args['discountId'], codes);
          handled = true;
          staged = true;
          stagedResourceIds.push(operation.id);
          data[key] = serializeDiscountBulkMutationPayload(field, operation, []);
        }
        break;
      }
      case 'discountCodeRedeemCodeBulkDelete':
      case 'discountRedeemCodeBulkDelete': {
        const ids = readStringArray(args['ids']);
        userErrors = validateRedeemCodeBulkDelete(args);
        if (typeof args['discountId'] === 'string' && userErrors === null) {
          const operation = stageRedeemCodeBulkDelete(args['discountId'], ids);
          handled = true;
          staged = true;
          stagedResourceIds.push(operation.id);
          data[key] = serializeDiscountBulkMutationPayload(field, operation, []);
        }
        break;
      }
      case 'discountCodeBxgyCreate':
        userErrors = validateDiscountBxgyCreate(readRecord(args['bxgyCodeDiscount']), 'bxgyCodeDiscount');
        if (userErrors === null) {
          discount = stageCodeBxgyCreate(readRecord(args['bxgyCodeDiscount']) ?? {});
          staged = true;
          stagedResourceIds.push(discount.id);
          userErrors = [];
        }
        break;
      case 'discountCodeBxgyUpdate':
        userErrors =
          validateKnownCodeBxgyDiscountId(args['id']) ??
          validateDiscountBxgyCreate(readRecord(args['bxgyCodeDiscount']), 'bxgyCodeDiscount');
        if (typeof args['id'] === 'string' && userErrors === null) {
          discount = stageCodeBxgyUpdate(args['id'], readRecord(args['bxgyCodeDiscount']) ?? {});
          staged = true;
          stagedResourceIds.push(discount.id);
          userErrors = [];
        }
        break;
      case 'discountAutomaticBxgyCreate':
        userErrors = validateDiscountBxgyCreate(readRecord(args['automaticBxgyDiscount']), 'automaticBxgyDiscount');
        if (userErrors === null) {
          const discount = stageAutomaticBxgyCreate(readRecord(args['automaticBxgyDiscount']) ?? {});
          handled = true;
          staged = true;
          stagedResourceIds.push(discount.id);
          data[key] = serializeAutomaticDiscountMutationPayload(field, variables, discount, []);
        }
        break;
      case 'discountAutomaticBxgyUpdate':
        userErrors =
          validateAutomaticBxgyDiscountExists(args['id']) ??
          validateDiscountBxgyCreate(readRecord(args['automaticBxgyDiscount']), 'automaticBxgyDiscount');
        if (userErrors === null && typeof args['id'] === 'string') {
          const discount = stageAutomaticBxgyUpdate(args['id'], readRecord(args['automaticBxgyDiscount']) ?? {});
          handled = true;
          staged = true;
          stagedResourceIds.push(discount.id);
          data[key] = serializeAutomaticDiscountMutationPayload(field, variables, discount, []);
        }
        break;
      case 'discountCodeFreeShippingCreate':
        userErrors = validateDiscountFreeShippingInput(
          readRecord(args['freeShippingCodeDiscount']),
          'freeShippingCodeDiscount',
        );
        if (userErrors === null) {
          const input = readRecord(args['freeShippingCodeDiscount']);
          if (input) {
            discount = stageCodeFreeShippingCreate(input);
            staged = true;
            stagedResourceIds.push(discount.id);
            userErrors = [];
          }
        }
        break;
      case 'discountCodeFreeShippingUpdate':
        userErrors =
          validateKnownTypedCodeDiscountId(args['id'], 'DiscountCodeFreeShipping') ??
          validateDiscountFreeShippingInput(readRecord(args['freeShippingCodeDiscount']), 'freeShippingCodeDiscount');
        if (typeof args['id'] === 'string' && userErrors === null) {
          const input = readRecord(args['freeShippingCodeDiscount']);
          if (input) {
            discount = stageCodeFreeShippingUpdate(args['id'], input);
            staged = true;
            stagedResourceIds.push(discount.id);
            userErrors = [];
          }
        }
        break;
      case 'discountAutomaticFreeShippingCreate':
        userErrors = validateDiscountFreeShippingInput(
          readRecord(args['freeShippingAutomaticDiscount']),
          'freeShippingAutomaticDiscount',
        );
        if (userErrors === null) {
          const discount = stageAutomaticFreeShippingCreate(readRecord(args['freeShippingAutomaticDiscount']) ?? {});
          handled = true;
          staged = true;
          stagedResourceIds.push(discount.id);
          data[key] = serializeAutomaticDiscountMutationPayload(field, variables, discount, []);
        }
        break;
      case 'discountAutomaticFreeShippingUpdate':
        userErrors =
          validateAutomaticDiscountExists(args['id'], 'DiscountAutomaticFreeShipping') ??
          validateDiscountFreeShippingInput(
            readRecord(args['freeShippingAutomaticDiscount']),
            'freeShippingAutomaticDiscount',
          );
        if (userErrors === null && typeof args['id'] === 'string') {
          const discount = stageAutomaticFreeShippingUpdate(
            args['id'],
            readRecord(args['freeShippingAutomaticDiscount']) ?? {},
          );
          handled = true;
          staged = true;
          stagedResourceIds.push(discount.id);
          data[key] = serializeAutomaticDiscountMutationPayload(field, variables, discount, []);
        }
        break;
      case 'discountCodeBulkActivate':
        userErrors = validateBroadBulkSelector(args, "Only one of 'ids', 'search' or 'saved_search_id' is allowed.");
        if (userErrors === null) {
          const result = stageDiscountBulkMutation('discountCodeBulkActivate', args);
          if (result.userErrors === null) {
            handled = true;
            staged = true;
            stagedResourceIds.push(result.operation.id, ...result.targetIds);
            data[key] = serializeDiscountBulkMutationPayload(field, result.operation, []);
          } else {
            userErrors = result.userErrors;
          }
        }
        break;
      case 'discountCodeBulkDelete':
        userErrors = validateBroadBulkSelector(args, "Only one of 'ids', 'search' or 'saved_search_id' is allowed.");
        if (userErrors === null) {
          const result = stageDiscountBulkMutation('discountCodeBulkDelete', args);
          if (result.userErrors === null) {
            handled = true;
            staged = true;
            stagedResourceIds.push(result.operation.id, ...result.targetIds);
            data[key] = serializeDiscountBulkMutationPayload(field, result.operation, []);
          } else {
            userErrors = result.userErrors;
          }
        }
        break;
      case 'discountCodeBulkDeactivate':
        userErrors = validateBroadBulkSelector(args, "Only one of 'ids', 'search' or 'saved_search_id' is allowed.");
        if (userErrors === null) {
          const result = stageDiscountBulkMutation('discountCodeBulkDeactivate', args);
          if (result.userErrors === null) {
            handled = true;
            staged = true;
            stagedResourceIds.push(result.operation.id, ...result.targetIds);
            data[key] = serializeDiscountBulkMutationPayload(field, result.operation, []);
          } else {
            userErrors = result.userErrors;
          }
        }
        break;
      case 'discountAutomaticBulkDelete':
        userErrors = validateBroadBulkSelector(args, 'Only one of IDs, search argument or saved search ID is allowed.');
        if (userErrors === null) {
          const result = stageDiscountBulkMutation('discountAutomaticBulkDelete', args);
          if (result.userErrors === null) {
            handled = true;
            staged = true;
            stagedResourceIds.push(result.operation.id, ...result.targetIds);
            data[key] = serializeDiscountBulkMutationPayload(field, result.operation, []);
          } else {
            userErrors = result.userErrors;
          }
        }
        break;
      default:
        break;
    }

    if (userErrors === null) {
      continue;
    }

    handled = true;
    if (rootName === 'discountAutomaticDelete') {
      data[key] = serializeAutomaticDiscountDeletePayload(field, null, userErrors);
    } else if (
      rootName === 'discountAutomaticBasicCreate' ||
      rootName === 'discountAutomaticBasicUpdate' ||
      rootName === 'discountAutomaticBxgyCreate' ||
      rootName === 'discountAutomaticBxgyUpdate' ||
      rootName === 'discountAutomaticFreeShippingCreate' ||
      rootName === 'discountAutomaticFreeShippingUpdate' ||
      rootName === 'discountAutomaticActivate' ||
      rootName === 'discountAutomaticDeactivate'
    ) {
      data[key] = serializeAutomaticDiscountMutationPayload(field, variables, null, userErrors);
    } else if (
      rootName === 'discountRedeemCodeBulkAdd' ||
      rootName === 'discountCodeRedeemCodeBulkDelete' ||
      rootName === 'discountRedeemCodeBulkDelete' ||
      rootName === 'discountCodeBulkActivate' ||
      rootName === 'discountCodeBulkDeactivate' ||
      rootName === 'discountCodeBulkDelete' ||
      rootName === 'discountAutomaticBulkDelete'
    ) {
      data[key] = serializeDiscountBulkMutationPayload(field, null, userErrors);
    } else if (rootName === 'discountCodeAppCreate' || rootName === 'discountCodeAppUpdate') {
      data[key] = serializeAppDiscountMutationPayload(field, variables, null, userErrors, 'codeAppDiscount');
    } else if (rootName === 'discountAutomaticAppCreate' || rootName === 'discountAutomaticAppUpdate') {
      data[key] = serializeAppDiscountMutationPayload(field, variables, null, userErrors, 'automaticAppDiscount');
    } else {
      data[key] = serializeDiscountMutationPayload(field, nodeField, userErrors, discount, deletedCodeDiscountId);
    }
  }

  return handled
    ? {
        response: { data },
        stagedResourceIds,
        notes: staged
          ? 'Staged locally in the in-memory discount draft store.'
          : 'Returned captured discount validation response locally.',
        staged,
      }
    : null;
}
