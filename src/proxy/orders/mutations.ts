import { Kind, valueFromASTUntyped, type FieldNode, type ObjectValueNode } from 'graphql';

import type { ReadMode } from '../../config.js';
import { getFieldArguments, getRootFields } from '../../graphql/root-field.js';
import { store } from '../../state/store.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../../state/synthetic-identity.js';
import type {
  AbandonmentRecord,
  CalculatedOrderRecord,
  DraftOrderPaymentTermsRecord,
  DraftOrderRecord,
  MoneyV2Record,
  OrderFulfillmentEventRecord,
  OrderFulfillmentLineItemRecord,
  OrderFulfillmentOrderLineItemRecord,
  OrderFulfillmentOrderRecord,
  OrderFulfillmentRecord,
  OrderLineItemRecord,
  OrderRecord,
  OrderReverseDeliveryRecord,
  OrderReverseFulfillmentOrderLineItemRecord,
  OrderReverseFulfillmentOrderRecord,
  OrderReturnLineItemRecord,
  OrderReturnRecord,
  OrderShippingLineRecord,
} from '../../state/types.js';
import { getDocumentFragments, getFieldResponseKey, readNullableStringArgument } from '../graphql-helpers.js';
import {
  applyDraftOrdersQuery,
  buildAccessDeniedError,
  serializeAbandonmentNode,
  serializeCalculatedDraftOrder,
  serializeCalculatedOrder,
  serializeDraftOrderNode,
  serializeDraftOrderPaymentTerms,
  serializeJob,
  serializeOrderCancelPayload,
  serializeOrderCapturePayload,
  serializeOrderCreateMandatePaymentPayload,
  serializeOrderFulfillment,
  serializeOrderFulfillmentEvent,
  serializeOrderFulfillmentOrder,
  serializeOrderLineItemNode,
  serializeOrderManagementPayload,
  serializeOrderNode,
  serializeOrderReverseDelivery,
  serializeOrderReturn,
  serializeRefundCreatePayload,
  serializeTransactionVoidPayload,
} from './serializers.js';
import {
  applyRefundToOrder,
  buildCalculatedLineItemFromVariant,
  buildCalculatedOrderFromOrder,
  buildCompletedDraftOrder,
  buildDraftOrderFromInput,
  buildDraftOrderFromOrder,
  buildOrderFromCompletedDraftOrder,
  buildOrderFromInput,
  buildRefundFromInput,
  buildUpdatedDraftOrder,
  captureOrderPayment,
  createMandatePayment,
  DRAFT_ORDER_SAVED_SEARCHES,
  duplicateDraftOrder,
  findOrderWithTransaction,
  formatDecimalAmount,
  getSelectedChildFields,
  makeOrderMoneyBag,
  markOrderAsPaid,
  type MutationUserError,
  normalizeDraftOrderAddress,
  normalizeDraftOrderAttributes,
  normalizeOrderMetafields,
  orderCustomerFromCustomer,
  parseDecimalAmount,
  readDiscountCodeAttributes,
  readDiscountCodeInput,
  readMandatePaymentInput,
  readOrderCaptureInput,
  readRefundCreateInput,
  readString,
  readTransactionVoidId,
  recalculateOrderTotals,
  serializeSelectedUserErrors,
  sumRefundedAmount,
  voidOrderTransaction,
} from './shared.js';

function readOrderUpdateInput(variables: Record<string, unknown>): Record<string, unknown> {
  const input = variables['input'];
  return typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : {};
}

function readOrderCreateOptions(variables: Record<string, unknown>): Record<string, unknown> {
  const options = variables['options'];
  return typeof options === 'object' && options !== null ? (options as Record<string, unknown>) : {};
}

function readDraftOrderCompleteId(variables: Record<string, unknown>): string | null {
  return typeof variables['id'] === 'string' ? variables['id'] : null;
}

function readDraftOrderUpdateInput(variables: Record<string, unknown>): unknown {
  return variables['input'] ?? null;
}

function readDraftOrderUpdateId(variables: Record<string, unknown>): string | null {
  return typeof variables['id'] === 'string' ? variables['id'] : null;
}

function readDraftOrderDuplicateId(variables: Record<string, unknown>): string | null {
  if (typeof variables['id'] === 'string') {
    return variables['id'];
  }

  return typeof variables['draftOrderId'] === 'string' ? variables['draftOrderId'] : null;
}

function readDraftOrderDeleteInput(variables: Record<string, unknown>): Record<string, unknown> | null {
  const input = variables['input'];
  return typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : null;
}

function readInputObjectArgument(
  field: FieldNode,
  argumentName: string,
  variables: Record<string, unknown>,
  fallbackVariableName: string,
): Record<string, unknown> | null {
  const argument = field.arguments?.find((candidate) => candidate.name.value === argumentName) ?? null;
  if (argument) {
    const value =
      argument.value.kind === Kind.VARIABLE
        ? variables[argument.value.name.value]
        : valueFromASTUntyped(argument.value, variables);
    return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : null;
  }

  const fallback = variables[fallbackVariableName];
  return typeof fallback === 'object' && fallback !== null ? (fallback as Record<string, unknown>) : null;
}

function normalizePaymentScheduleAmount(
  amountSet: { shopMoney: MoneyV2Record } | null | undefined,
): MoneyV2Record | null {
  return amountSet?.shopMoney ? structuredClone(amountSet.shopMoney) : null;
}

function readBooleanValue(raw: unknown, fallback: boolean): boolean {
  return typeof raw === 'boolean' ? raw : fallback;
}

function addDaysTimestamp(rawTimestamp: string | null, days: number | null | undefined): string | null {
  if (!rawTimestamp || typeof days !== 'number') {
    return rawTimestamp ? null : null;
  }

  const timestamp = Date.parse(rawTimestamp);
  if (!Number.isFinite(timestamp)) {
    return null;
  }

  const nextTimestamp = new Date(timestamp);
  nextTimestamp.setUTCDate(nextTimestamp.getUTCDate() + days);
  return nextTimestamp.toISOString().replace('.000Z', 'Z');
}

function validationPath(prefix: string[], suffix: string[]): string[] {
  return [...prefix, ...suffix];
}

function validatePaymentTermsAttributes(
  raw: unknown,
  fieldPrefix: string[],
): { templateId: string; userErrors: [] } | { templateId: null; userErrors: MutationUserError[] } {
  const input = typeof raw === 'object' && raw !== null ? (raw as Record<string, unknown>) : {};
  const templateId = readString(input['paymentTermsTemplateId']);
  if (!templateId) {
    return {
      templateId: null,
      userErrors: [
        {
          field: validationPath(fieldPrefix, ['paymentTermsTemplateId']),
          message: 'Payment terms template id can not be empty.',
          code: 'PAYMENT_TERMS_TEMPLATE_ID_EMPTY',
        },
      ],
    };
  }

  const template = store.getEffectivePaymentTermsTemplateById(templateId);
  if (!template) {
    return {
      templateId: null,
      userErrors: [
        {
          field: validationPath(fieldPrefix, ['paymentTermsTemplateId']),
          message: 'Payment terms template does not exist.',
          code: 'PAYMENT_TERMS_TEMPLATE_NOT_FOUND',
        },
      ],
    };
  }

  const schedules = Array.isArray(input['paymentSchedules']) ? input['paymentSchedules'] : [];
  const firstSchedule = schedules[0];
  const firstScheduleRecord =
    typeof firstSchedule === 'object' && firstSchedule !== null ? (firstSchedule as Record<string, unknown>) : {};

  if (template.paymentTermsType === 'NET' && !readString(firstScheduleRecord['issuedAt'])) {
    return {
      templateId: null,
      userErrors: [
        {
          field: validationPath(fieldPrefix, ['paymentSchedules', '0', 'issuedAt']),
          message: 'Issued at must be provided for net payment terms.',
          code: 'PAYMENT_SCHEDULE_INVALID',
        },
      ],
    };
  }

  if (template.paymentTermsType === 'FIXED' && !readString(firstScheduleRecord['dueAt'])) {
    return {
      templateId: null,
      userErrors: [
        {
          field: validationPath(fieldPrefix, ['paymentSchedules', '0', 'dueAt']),
          message: 'Due at must be provided for fixed payment terms.',
          code: 'PAYMENT_SCHEDULE_INVALID',
        },
      ],
    };
  }

  return { templateId, userErrors: [] };
}

function buildPaymentTermsFromAttributes(
  raw: unknown,
  paymentTermsTemplateId: string,
  amountSet: { shopMoney: MoneyV2Record } | null | undefined,
  existing: DraftOrderPaymentTermsRecord | null = null,
): DraftOrderPaymentTermsRecord {
  const input = typeof raw === 'object' && raw !== null ? (raw as Record<string, unknown>) : {};
  const template = store.getEffectivePaymentTermsTemplateById(paymentTermsTemplateId);
  const schedules = Array.isArray(input['paymentSchedules']) ? input['paymentSchedules'] : [];
  const normalizedSchedules = schedules
    .filter((schedule): schedule is Record<string, unknown> => typeof schedule === 'object' && schedule !== null)
    .map((schedule) => {
      const issuedAt = readString(schedule['issuedAt']);
      const dueAt =
        readString(schedule['dueAt']) ??
        (template?.paymentTermsType === 'NET' ? addDaysTimestamp(issuedAt, template.dueInDays) : null);

      return {
        id: makeSyntheticGid('PaymentSchedule'),
        dueAt,
        issuedAt,
        completedAt: readString(schedule['completedAt']),
        completed: readBooleanValue(schedule['completed'], false),
        due: typeof schedule['due'] === 'boolean' ? schedule['due'] : false,
        amount: normalizePaymentScheduleAmount(amountSet),
        balanceDue: normalizePaymentScheduleAmount(amountSet),
        totalBalance: normalizePaymentScheduleAmount(amountSet),
      };
    });

  return {
    id: existing?.id ?? makeSyntheticGid('PaymentTerms'),
    due: normalizedSchedules.some((schedule) => schedule.due === true),
    overdue: false,
    dueInDays: template?.dueInDays ?? null,
    paymentTermsName: template?.name ?? 'Custom payment terms',
    paymentTermsType: template?.paymentTermsType ?? 'UNKNOWN',
    translatedName: template?.translatedName ?? template?.name ?? 'Custom payment terms',
    paymentSchedules: normalizedSchedules,
  };
}

type PaymentTermsOwner =
  | { kind: 'order'; record: OrderRecord; paymentTerms: DraftOrderPaymentTermsRecord | null }
  | { kind: 'draftOrder'; record: DraftOrderRecord; paymentTerms: DraftOrderPaymentTermsRecord | null };

function findPaymentTermsOwnerByReferenceId(referenceId: string | null): PaymentTermsOwner | null {
  if (!referenceId) {
    return null;
  }

  const order = store.getOrderById(referenceId);
  if (order) {
    return { kind: 'order', record: order, paymentTerms: order.paymentTerms ?? null };
  }

  const draftOrder = store.getDraftOrderById(referenceId);
  if (draftOrder) {
    return { kind: 'draftOrder', record: draftOrder, paymentTerms: draftOrder.paymentTerms };
  }

  return null;
}

function findPaymentTermsOwnerByPaymentTermsId(paymentTermsId: string | null): PaymentTermsOwner | null {
  if (!paymentTermsId) {
    return null;
  }

  for (const order of store.getOrders()) {
    if (order.paymentTerms?.id === paymentTermsId) {
      return { kind: 'order', record: order, paymentTerms: order.paymentTerms };
    }
  }

  for (const draftOrder of store.getDraftOrders()) {
    if (draftOrder.paymentTerms?.id === paymentTermsId) {
      return { kind: 'draftOrder', record: draftOrder, paymentTerms: draftOrder.paymentTerms };
    }
  }

  return null;
}

function storePaymentTermsOwner(owner: PaymentTermsOwner, paymentTerms: DraftOrderPaymentTermsRecord | null): void {
  if (owner.kind === 'order') {
    store.updateOrder({
      ...owner.record,
      updatedAt: makeSyntheticTimestamp(),
      paymentTerms: paymentTerms ? structuredClone(paymentTerms) : null,
    });
    return;
  }

  store.updateDraftOrder({
    ...owner.record,
    updatedAt: makeSyntheticTimestamp(),
    paymentTerms: paymentTerms ? structuredClone(paymentTerms) : null,
  });
}

function readDraftOrderInvoiceSendId(variables: Record<string, unknown>): string | null {
  return typeof variables['id'] === 'string' ? variables['id'] : null;
}

function readStringListArgument(
  field: FieldNode,
  argumentName: string,
  variables: Record<string, unknown>,
): string[] | null {
  const value = getFieldArguments(field, variables)[argumentName];
  return Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === 'string') : null;
}

function readDraftOrderSavedSearchQuery(savedSearchId: unknown): string | null {
  if (typeof savedSearchId !== 'string') {
    return null;
  }
  return DRAFT_ORDER_SAVED_SEARCHES.find((savedSearch) => savedSearch.id === savedSearchId)?.query ?? null;
}

function selectDraftOrderBulkTargets(field: FieldNode, variables: Record<string, unknown>): DraftOrderRecord[] {
  const args = getFieldArguments(field, variables);
  const ids = readStringListArgument(field, 'ids', variables);
  const savedSearchQuery = readDraftOrderSavedSearchQuery(args['savedSearchId']);
  const rawSearch = typeof args['search'] === 'string' ? args['search'] : savedSearchQuery;
  const candidates = store.getDraftOrders();

  if (ids && ids.length > 0) {
    const idSet = new Set(ids);
    return candidates.filter((draftOrder) => idSet.has(draftOrder.id));
  }

  if (typeof rawSearch === 'string' && rawSearch.length > 0) {
    return applyDraftOrdersQuery(candidates, rawSearch);
  }

  return candidates;
}

function readDraftOrderBulkTags(field: FieldNode, variables: Record<string, unknown>): string[] {
  return (readStringListArgument(field, 'tags', variables) ?? [])
    .map((tag) => tag.trim())
    .filter((tag) => tag.length > 0);
}

function updateDraftOrderTags(draftOrder: DraftOrderRecord, tags: string[], operation: 'add' | 'remove'): void {
  const nextTags =
    operation === 'add'
      ? [...new Set([...draftOrder.tags, ...tags])]
      : draftOrder.tags.filter((tag) => !tags.includes(tag));

  store.updateDraftOrder({
    ...draftOrder,
    tags: nextTags.sort((left, right) => left.localeCompare(right)),
    updatedAt: makeSyntheticTimestamp(),
  });
}

function buildDraftOrderInvoiceSendUserErrors(
  draftOrder: DraftOrderRecord | null,
): Array<{ field: string[] | null; message: string }> {
  if (!draftOrder) {
    return [{ field: null, message: 'Draft order not found' }];
  }

  const userErrors: Array<{ field: string[] | null; message: string }> = [];
  if (!draftOrder.email) {
    userErrors.push({ field: null, message: "To can't be blank" });
  }

  if (draftOrder.status === 'COMPLETED') {
    userErrors.push({
      field: null,
      message: "Draft order Invoice can't be sent. This draft order is already paid.",
    });
  }

  if (userErrors.length > 0) {
    return userErrors;
  }

  return [
    {
      field: ['id'],
      message: 'draftOrderInvoiceSend is intentionally not executed by the local proxy because it sends email.',
    },
  ];
}

function readDraftOrderCreateFromOrderId(variables: Record<string, unknown>): string | null {
  return typeof variables['orderId'] === 'string' ? variables['orderId'] : null;
}

function readNullableBooleanArgument(
  field: FieldNode,
  argumentName: string,
  variables: Record<string, unknown>,
): boolean | null {
  const argument = field.arguments?.find((candidate) => candidate.name.value === argumentName);
  if (!argument) {
    return null;
  }

  if (argument.value.kind === Kind.BOOLEAN) {
    return argument.value.value;
  }

  if (argument.value.kind === Kind.VARIABLE) {
    const rawValue = variables[argument.value.name.value];
    return typeof rawValue === 'boolean' ? rawValue : null;
  }

  return null;
}

function readDraftOrderCompleteSourceName(field: FieldNode, variables: Record<string, unknown>): string | null {
  const args = getFieldArguments(field, variables);
  return typeof args['sourceName'] === 'string' ? args['sourceName'] : null;
}

function readDraftOrderCompletePaymentPending(field: FieldNode, variables: Record<string, unknown>): boolean {
  return readNullableBooleanArgument(field, 'paymentPending', variables) ?? false;
}

function readDraftOrderCompletePaymentGatewayId(field: FieldNode, variables: Record<string, unknown>): string | null {
  const args = getFieldArguments(field, variables);
  return typeof args['paymentGatewayId'] === 'string' ? args['paymentGatewayId'] : null;
}

function readOrderEditVariantId(variables: Record<string, unknown>): string | null {
  return typeof variables['variantId'] === 'string' ? variables['variantId'] : null;
}

function readOrderEditLineItemId(variables: Record<string, unknown>): string | null {
  return typeof variables['lineItemId'] === 'string' ? variables['lineItemId'] : null;
}

function readOrderEditQuantity(variables: Record<string, unknown>): number | null {
  return typeof variables['quantity'] === 'number' ? variables['quantity'] : null;
}

function readOrderEditId(variables: Record<string, unknown>): string | null {
  return typeof variables['id'] === 'string' ? variables['id'] : null;
}

function getOrderEditInlineIdArgument(field: FieldNode) {
  return field.arguments?.find((argument) => argument.name.value === 'id') ?? null;
}

function serializeDraftOrderCreatePayloadWithUserErrors(
  field: FieldNode,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'draftOrder':
        payload[selectionKey] = null;
        break;
      case 'userErrors':
        payload[selectionKey] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }
  return payload;
}

function serializePaymentTermsMutationPayload(
  field: FieldNode,
  paymentTerms: DraftOrderPaymentTermsRecord | null,
  userErrors: MutationUserError[],
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'paymentTerms':
        payload[selectionKey] = paymentTerms ? serializeDraftOrderPaymentTerms(selection, paymentTerms) : null;
        break;
      case 'userErrors':
        payload[selectionKey] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }

  return payload;
}

function serializePaymentTermsDeletePayload(
  field: FieldNode,
  deletedId: string | null,
  userErrors: MutationUserError[],
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'deletedId':
        payload[selectionKey] = deletedId;
        break;
      case 'userErrors':
        payload[selectionKey] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }

  return payload;
}

function validateDraftOrderCreateInput(input: unknown): Array<{ field: string[] | null; message: string }> {
  const inputRecord = typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : {};
  const lineItems = inputRecord['lineItems'];
  if (!Array.isArray(lineItems) || lineItems.length === 0) {
    return [{ field: null, message: 'Add at least 1 product' }];
  }

  const userErrors: Array<{ field: string[] | null; message: string }> = [];

  if (typeof inputRecord['email'] === 'string' && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(inputRecord['email'])) {
    userErrors.push({
      field: ['email'],
      message: 'Email is invalid',
    });
  }

  if (typeof inputRecord['reserveInventoryUntil'] === 'string') {
    const reserveUntil = Date.parse(inputRecord['reserveInventoryUntil']);
    if (!Number.isNaN(reserveUntil) && reserveUntil < Date.now()) {
      userErrors.push({
        field: null,
        message: "Reserve until can't be in the past",
      });
    }
  }

  const paymentTerms = typeof inputRecord['paymentTerms'] === 'object' ? inputRecord['paymentTerms'] : null;
  if (
    paymentTerms !== null &&
    !(
      typeof paymentTerms === 'object' &&
      'paymentTermsTemplateId' in paymentTerms &&
      typeof (paymentTerms as Record<string, unknown>)['paymentTermsTemplateId'] === 'string'
    )
  ) {
    userErrors.push({
      field: null,
      message: 'Payment terms template id can not be empty.',
    });
  } else if (paymentTerms !== null) {
    userErrors.push({
      field: null,
      message: 'The user must have access to set payment terms.',
    });
  }

  lineItems.forEach((lineItem, index) => {
    if (typeof lineItem !== 'object' || lineItem === null) {
      userErrors.push({
        field: ['input', 'lineItems', String(index)],
        message: 'Line item is invalid',
      });
      return;
    }

    const lineItemRecord = lineItem as Record<string, unknown>;
    const variantId = readString(lineItemRecord['variantId']);
    const hasCustomTitle = readString(lineItemRecord['title']) !== null;
    const hasCustomPrice =
      lineItemRecord['originalUnitPrice'] !== undefined && lineItemRecord['originalUnitPrice'] !== null;
    const quantity = typeof lineItemRecord['quantity'] === 'number' ? lineItemRecord['quantity'] : null;

    if (quantity !== null && quantity < 1) {
      userErrors.push({
        field: ['lineItems', String(index), 'quantity'],
        message: 'Quantity must be greater than or equal to 1',
      });
      return;
    }

    if (variantId) {
      if (!store.getEffectiveVariantById(variantId)) {
        const numericId = variantId.split('/').at(-1) ?? variantId;
        userErrors.push({
          field: null,
          message: `Product with ID ${numericId} is no longer available.`,
        });
      }
      return;
    }

    if (!hasCustomTitle) {
      userErrors.push({
        field: null,
        message: 'Merchandise title is empty.',
      });
    }

    if (hasCustomPrice && parseDecimalAmount(lineItemRecord['originalUnitPrice']) < 0) {
      userErrors.push({
        field: null,
        message: 'Cannot send negative price for line_item',
      });
    }
  });

  return userErrors;
}

function buildOrderUpdateMissingIdError(input: Record<string, unknown>): Record<string, unknown> {
  return {
    message: 'Variable $input of type OrderInput! was provided invalid value for id (Expected value to not be null)',
    extensions: {
      code: 'INVALID_VARIABLE',
      value: input,
      problems: [{ path: ['id'], explanation: 'Expected value to not be null' }],
    },
  };
}

function buildOrderCreateMissingOrderError(): Record<string, unknown> {
  return {
    message: 'Variable $order of type OrderCreateOrderInput! was provided invalid value',
    extensions: {
      code: 'INVALID_VARIABLE',
      value: null,
      problems: [{ path: [], explanation: 'Expected value to not be null' }],
    },
  };
}

function buildOrderCreateMissingInlineOrderError(): Record<string, unknown> {
  return {
    message: "Field 'orderCreate' is missing required arguments: order",
    path: ['mutation', 'orderCreate'],
    extensions: {
      code: 'missingRequiredArguments',
      className: 'Field',
      name: 'orderCreate',
      arguments: 'order',
    },
  };
}

function buildOrderCreateNullInlineOrderError(): Record<string, unknown> {
  return {
    message:
      "Argument 'order' on Field 'orderCreate' has an invalid value (null). Expected type 'OrderCreateOrderInput!'.",
    path: ['mutation', 'orderCreate', 'order'],
    extensions: {
      code: 'argumentLiteralsIncompatible',
      typeName: 'Field',
      argumentName: 'order',
    },
  };
}

function buildDraftOrderCreateMissingInlineInputError(): Record<string, unknown> {
  return {
    message: "Field 'draftOrderCreate' is missing required arguments: input",
    path: ['mutation', 'draftOrderCreate'],
    extensions: {
      code: 'missingRequiredArguments',
      className: 'Field',
      name: 'draftOrderCreate',
      arguments: 'input',
    },
  };
}

function buildDraftOrderCreateNullInlineInputError(): Record<string, unknown> {
  return {
    message:
      "Argument 'input' on Field 'draftOrderCreate' has an invalid value (null). Expected type 'DraftOrderInput!'.",
    path: ['mutation', 'draftOrderCreate', 'input'],
    extensions: {
      code: 'argumentLiteralsIncompatible',
      typeName: 'Field',
      argumentName: 'input',
    },
  };
}

function buildDraftOrderCreateMissingInputError(): Record<string, unknown> {
  return {
    message: 'Variable $input of type DraftOrderInput! was provided invalid value',
    extensions: {
      code: 'INVALID_VARIABLE',
      value: null,
      problems: [{ path: [], explanation: 'Expected value to not be null' }],
    },
  };
}

function buildDraftOrderCompleteMissingIdError(): Record<string, unknown> {
  return {
    message: 'Variable $id of type ID! was provided invalid value',
    extensions: {
      code: 'INVALID_VARIABLE',
      value: null,
      problems: [{ path: [], explanation: 'Expected value to not be null' }],
    },
  };
}

function buildDraftOrderCompleteMissingInlineIdError(): Record<string, unknown> {
  return {
    message: "Field 'draftOrderComplete' is missing required arguments: id",
    path: ['mutation', 'draftOrderComplete'],
    extensions: {
      code: 'missingRequiredArguments',
      className: 'Field',
      name: 'draftOrderComplete',
      arguments: 'id',
    },
  };
}

function buildDraftOrderCompleteNullInlineIdError(): Record<string, unknown> {
  return {
    message: "Argument 'id' on Field 'draftOrderComplete' has an invalid value (null). Expected type 'ID!'.",
    path: ['mutation', 'draftOrderComplete', 'id'],
    extensions: {
      code: 'argumentLiteralsIncompatible',
      typeName: 'Field',
      argumentName: 'id',
    },
  };
}

function buildMissingRequiredArgumentError(operationName: string, argumentName: string): Record<string, unknown> {
  return {
    message: `Field '${operationName}' is missing required arguments: ${argumentName}`,
    path: ['mutation', operationName],
    extensions: {
      code: 'missingRequiredArguments',
      className: 'Field',
      name: operationName,
      arguments: argumentName,
    },
  };
}

function buildNullArgumentError(
  operationName: string,
  argumentName: string,
  expectedType: string,
): Record<string, unknown> {
  return {
    message: `Argument '${argumentName}' on Field '${operationName}' has an invalid value (null). Expected type '${expectedType}'.`,
    path: ['mutation', operationName, argumentName],
    extensions: {
      code: 'argumentLiteralsIncompatible',
      typeName: 'Field',
      argumentName,
    },
  };
}

function buildMissingVariableError(variableName: string, variableType: string): Record<string, unknown> {
  return {
    message: `Variable $${variableName} of type ${variableType} was provided invalid value`,
    extensions: {
      code: 'INVALID_VARIABLE',
      value: null,
      problems: [{ path: [], explanation: 'Expected value to not be null' }],
    },
  };
}

function buildFulfillmentTrackingInfoUpdateMissingIdError(): Record<string, unknown> {
  return {
    message: 'Variable $fulfillmentId of type ID! was provided invalid value',
    extensions: {
      code: 'INVALID_VARIABLE',
      value: null,
      problems: [{ path: [], explanation: 'Expected value to not be null' }],
    },
  };
}

function buildFulfillmentTrackingInfoUpdateMissingInlineIdError(): Record<string, unknown> {
  return {
    message: "Field 'fulfillmentTrackingInfoUpdate' is missing required arguments: fulfillmentId",
    path: ['mutation', 'fulfillmentTrackingInfoUpdate'],
    extensions: {
      code: 'missingRequiredArguments',
      className: 'Field',
      name: 'fulfillmentTrackingInfoUpdate',
      arguments: 'fulfillmentId',
    },
  };
}

function buildFulfillmentTrackingInfoUpdateNullInlineIdError(): Record<string, unknown> {
  return {
    message:
      "Argument 'fulfillmentId' on Field 'fulfillmentTrackingInfoUpdate' has an invalid value (null). Expected type 'ID!'.",
    path: ['mutation', 'fulfillmentTrackingInfoUpdate', 'fulfillmentId'],
    extensions: {
      code: 'argumentLiteralsIncompatible',
      typeName: 'Field',
      argumentName: 'fulfillmentId',
    },
  };
}

function buildFulfillmentCancelMissingIdError(): Record<string, unknown> {
  return {
    message: 'Variable $id of type ID! was provided invalid value',
    extensions: {
      code: 'INVALID_VARIABLE',
      value: null,
      problems: [{ path: [], explanation: 'Expected value to not be null' }],
    },
  };
}

function buildFulfillmentCancelMissingInlineIdError(): Record<string, unknown> {
  return {
    message: "Field 'fulfillmentCancel' is missing required arguments: id",
    path: ['mutation', 'fulfillmentCancel'],
    extensions: {
      code: 'missingRequiredArguments',
      className: 'Field',
      name: 'fulfillmentCancel',
      arguments: 'id',
    },
  };
}

function buildFulfillmentCancelNullInlineIdError(): Record<string, unknown> {
  return {
    message: "Argument 'id' on Field 'fulfillmentCancel' has an invalid value (null). Expected type 'ID!'.",
    path: ['mutation', 'fulfillmentCancel', 'id'],
    extensions: {
      code: 'argumentLiteralsIncompatible',
      typeName: 'Field',
      argumentName: 'id',
    },
  };
}

function buildOrderEditMissingIdError(): Record<string, unknown> {
  return {
    message: 'Variable $id of type ID! was provided invalid value',
    extensions: {
      code: 'INVALID_VARIABLE',
      value: null,
      problems: [{ path: [], explanation: 'Expected value to not be null' }],
    },
  };
}

function buildOrderEditBeginMissingIdError(): Record<string, unknown> {
  return buildOrderEditMissingIdError();
}

function buildOrderEditAddVariantMissingIdError(): Record<string, unknown> {
  return buildOrderEditMissingIdError();
}

function buildOrderEditSetQuantityMissingIdError(): Record<string, unknown> {
  return buildOrderEditMissingIdError();
}

function buildOrderEditCommitMissingIdError(): Record<string, unknown> {
  return buildOrderEditMissingIdError();
}

function serializeOrderEditSession(field: FieldNode, calculatedOrder: CalculatedOrderRecord): Record<string, unknown> {
  const sessionId = calculatedOrder.id.replace('/CalculatedOrder/', '/OrderEditSession/');
  return Object.fromEntries(
    getSelectedChildFields(field).map((selection) => {
      const key = getFieldResponseKey(selection);
      switch (selection.name.value) {
        case 'id':
          return [key, sessionId];
        default:
          return [key, null];
      }
    }),
  );
}

function serializeMoneySetSelection(
  field: FieldNode,
  moneySet: { shopMoney: MoneyV2Record } | null,
): Record<string, unknown> | null {
  if (!moneySet) {
    return null;
  }
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'shopMoney':
      case 'presentmentMoney':
        result[key] = Object.fromEntries(
          getSelectedChildFields(selection).map((moneySelection) => {
            const moneyKey = getFieldResponseKey(moneySelection);
            switch (moneySelection.name.value) {
              case 'amount':
                return [moneyKey, moneySet.shopMoney.amount];
              case 'currencyCode':
                return [moneyKey, moneySet.shopMoney.currencyCode];
              default:
                return [moneyKey, null];
            }
          }),
        );
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCalculatedShippingLinePayload(
  field: FieldNode,
  shippingLine: OrderShippingLineRecord,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = shippingLine.id ?? null;
        break;
      case 'title':
        result[key] = shippingLine.title;
        break;
      case 'price':
        result[key] = serializeMoneySetSelection(selection, shippingLine.originalPriceSet);
        break;
      case 'stagedStatus':
        result[key] = shippingLine.stagedStatus ?? 'ADDED';
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function buildOrderEditInvalidVariantUserErrors(): Array<{ field: string[]; message: string }> {
  return [
    {
      field: ['variantId'],
      message: "can't convert Integer[0] to a positive Integer to use as an untrusted id",
    },
  ];
}

function readMoneyInputAmount(raw: unknown): { amount: string; currencyCode: string } | null {
  if (typeof raw !== 'object' || raw === null) {
    return null;
  }
  const input = raw as Record<string, unknown>;
  const rawAmount = input['amount'];
  const rawCurrencyCode = input['currencyCode'];
  if ((typeof rawAmount !== 'string' && typeof rawAmount !== 'number') || typeof rawCurrencyCode !== 'string') {
    return null;
  }
  return {
    amount: formatDecimalAmount(parseDecimalAmount(rawAmount)),
    currencyCode: rawCurrencyCode,
  };
}

function recalculateCalculatedOrder(calculatedOrder: CalculatedOrderRecord): CalculatedOrderRecord {
  const currencyCode =
    calculatedOrder.currentTotalPriceSet?.shopMoney.currencyCode ??
    calculatedOrder.subtotalPriceSet?.shopMoney.currencyCode ??
    'CAD';
  const totalDiscount = calculatedOrder.lineItems.reduce(
    (sum, lineItem) => sum + parseDecimalAmount(lineItem.totalDiscountSet?.shopMoney.amount),
    0,
  );
  return recalculateOrderTotals({
    ...calculatedOrder,
    totalDiscountsSet: makeOrderMoneyBag(totalDiscount, currencyCode),
    currentTotalDiscountsSet: makeOrderMoneyBag(totalDiscount, currencyCode),
  }) as CalculatedOrderRecord;
}

function buildCalculatedCustomLineItem(args: Record<string, unknown>): OrderLineItemRecord | null {
  const title = typeof args['title'] === 'string' ? args['title'] : null;
  const quantity = typeof args['quantity'] === 'number' ? args['quantity'] : null;
  const price = readMoneyInputAmount(args['price']);
  if (!title || quantity === null || quantity <= 0 || !price) {
    return null;
  }
  const currencyCode = price.currencyCode;
  return {
    id: makeSyntheticGid('CalculatedLineItem'),
    originalLineItemId: null,
    isAdded: true,
    title,
    quantity,
    currentQuantity: quantity,
    sku: null,
    variantId: null,
    variantTitle: null,
    originalUnitPriceSet: makeOrderMoneyBag(price.amount, currencyCode),
    discountedUnitPriceSet: makeOrderMoneyBag(price.amount, currencyCode),
    totalDiscountSet: makeOrderMoneyBag(0, currencyCode),
    calculatedDiscountAllocations: [],
    taxLines: [],
    requiresShipping: typeof args['requiresShipping'] === 'boolean' ? args['requiresShipping'] : true,
    taxable: typeof args['taxable'] === 'boolean' ? args['taxable'] : true,
  };
}

function buildCalculatedShippingLine(args: Record<string, unknown>): OrderShippingLineRecord | null {
  const shippingLineInput =
    typeof args['shippingLine'] === 'object' && args['shippingLine'] !== null
      ? (args['shippingLine'] as Record<string, unknown>)
      : null;
  const title = typeof shippingLineInput?.['title'] === 'string' ? shippingLineInput['title'] : null;
  const price = readMoneyInputAmount(shippingLineInput?.['price']);
  if (!title || !price) {
    return null;
  }
  return {
    id: makeSyntheticGid('CalculatedShippingLine'),
    title,
    code: title,
    source: 'shopify-draft-proxy',
    originalPriceSet: makeOrderMoneyBag(price.amount, price.currencyCode),
    taxLines: [],
    stagedStatus: 'ADDED',
  };
}

function applyLineItemDiscount(
  calculatedOrder: CalculatedOrderRecord,
  lineItemId: string,
  discountInput: Record<string, unknown>,
): { calculatedOrder: CalculatedOrderRecord; calculatedLineItem: OrderLineItemRecord } | null {
  const targetLineItem = calculatedOrder.lineItems.find((lineItem) => lineItem.id === lineItemId) ?? null;
  if (!targetLineItem) {
    return null;
  }
  const currencyCode = targetLineItem.originalUnitPriceSet?.shopMoney.currencyCode ?? 'CAD';
  const fixedValue = readMoneyInputAmount(discountInput['fixedValue']);
  const percentValue = typeof discountInput['percentValue'] === 'number' ? discountInput['percentValue'] : null;
  const subtotal = parseDecimalAmount(targetLineItem.originalUnitPriceSet?.shopMoney.amount) * targetLineItem.quantity;
  const discountAmount = fixedValue
    ? parseDecimalAmount(fixedValue.amount) * targetLineItem.quantity
    : percentValue !== null
      ? (subtotal * percentValue) / 100
      : 0;
  const boundedDiscount = Math.min(subtotal, Math.max(0, discountAmount));
  const allocation = {
    id: makeSyntheticGid('CalculatedDiscountApplication'),
    description: typeof discountInput['description'] === 'string' ? discountInput['description'] : null,
    allocatedAmountSet: makeOrderMoneyBag(boundedDiscount, fixedValue?.currencyCode ?? currencyCode),
  };
  const updatedLineItem: OrderLineItemRecord = {
    ...targetLineItem,
    calculatedDiscountAllocations: [...(targetLineItem.calculatedDiscountAllocations ?? []), allocation],
    totalDiscountSet: makeOrderMoneyBag(
      parseDecimalAmount(targetLineItem.totalDiscountSet?.shopMoney.amount) + boundedDiscount,
      currencyCode,
    ),
    discountedUnitPriceSet: makeOrderMoneyBag(
      Math.max(
        0,
        parseDecimalAmount(targetLineItem.originalUnitPriceSet?.shopMoney.amount) -
          boundedDiscount / Math.max(1, targetLineItem.quantity),
      ),
      currencyCode,
    ),
  };
  return {
    calculatedLineItem: updatedLineItem,
    calculatedOrder: recalculateCalculatedOrder({
      ...calculatedOrder,
      lineItems: calculatedOrder.lineItems.map((lineItem) => (lineItem.id === lineItemId ? updatedLineItem : lineItem)),
    }),
  };
}

function removeCalculatedDiscount(
  calculatedOrder: CalculatedOrderRecord,
  discountApplicationId: string,
): CalculatedOrderRecord | null {
  let removed = false;
  const lineItems = calculatedOrder.lineItems.map((lineItem) => {
    const allocations = lineItem.calculatedDiscountAllocations ?? [];
    const nextAllocations = allocations.filter((allocation) => allocation.id !== discountApplicationId);
    if (nextAllocations.length === allocations.length) {
      return lineItem;
    }
    removed = true;
    const currencyCode = lineItem.originalUnitPriceSet?.shopMoney.currencyCode ?? 'CAD';
    const totalDiscount = nextAllocations.reduce(
      (sum, allocation) => sum + parseDecimalAmount(allocation.allocatedAmountSet.shopMoney.amount),
      0,
    );
    return {
      ...lineItem,
      calculatedDiscountAllocations: nextAllocations,
      totalDiscountSet: makeOrderMoneyBag(totalDiscount, currencyCode),
      discountedUnitPriceSet: makeOrderMoneyBag(
        Math.max(
          0,
          parseDecimalAmount(lineItem.originalUnitPriceSet?.shopMoney.amount) -
            totalDiscount / Math.max(1, lineItem.quantity),
        ),
        currencyCode,
      ),
    };
  });
  return removed ? recalculateCalculatedOrder({ ...calculatedOrder, lineItems }) : null;
}

function buildCommittedOrderLineItems(
  originalOrder: OrderRecord,
  calculatedOrder: CalculatedOrderRecord,
): OrderLineItemRecord[] {
  return calculatedOrder.lineItems.map((lineItem) => {
    const originalLineItem = lineItem.originalLineItemId
      ? (originalOrder.lineItems.find((candidate) => candidate.id === lineItem.originalLineItemId) ?? null)
      : null;

    if (lineItem.quantity === 0 && originalLineItem) {
      return {
        ...structuredClone(originalLineItem),
        currentQuantity: 0,
      };
    }

    return {
      ...structuredClone(lineItem),
      currentQuantity: lineItem.quantity,
      isAdded: false,
      originalLineItemId: lineItem.originalLineItemId ?? lineItem.id,
    };
  });
}

function buildOrderUpdateMissingInlineIdError(): Record<string, unknown> {
  return {
    message: "Argument 'id' on InputObject 'OrderInput' is required. Expected type ID!",
    path: ['mutation', 'orderUpdate', 'input', 'id'],
    extensions: {
      code: 'missingRequiredInputObjectAttribute',
      argumentName: 'id',
      argumentType: 'ID!',
      inputObjectType: 'OrderInput',
    },
  };
}

function buildOrderUpdateNullInlineIdError(): Record<string, unknown> {
  return {
    message: "Argument 'id' on InputObject 'OrderInput' has an invalid value (null). Expected type 'ID!'.",
    path: ['mutation', 'orderUpdate', 'input', 'id'],
    extensions: {
      code: 'argumentLiteralsIncompatible',
      typeName: 'InputObject',
      argumentName: 'id',
    },
  };
}

function getOrderUpdateInlineInput(field: FieldNode): ObjectValueNode | null {
  const inputArg = field.arguments?.find((argument) => argument.name.value === 'input') ?? null;
  return inputArg?.value.kind === Kind.OBJECT ? inputArg.value : null;
}

function getOrderCreateInlineArgument(field: FieldNode) {
  return field.arguments?.find((argument) => argument.name.value === 'order') ?? null;
}

function readVariableBackedInputArgument(
  field: FieldNode,
  argumentName: string,
  variables: Record<string, unknown>,
  fallbackVariableName: string,
): Record<string, unknown> | null {
  const argument = field.arguments?.find((candidate) => candidate.name.value === argumentName) ?? null;
  if (argument?.value.kind === Kind.VARIABLE) {
    const value = variables[argument.value.name.value];
    return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : null;
  }

  const fallback = variables[fallbackVariableName];
  return typeof fallback === 'object' && fallback !== null ? (fallback as Record<string, unknown>) : null;
}

function getDraftOrderCompleteInlineIdArgument(field: FieldNode) {
  return field.arguments?.find((argument) => argument.name.value === 'id') ?? null;
}

function getInlineArgument(field: FieldNode, argumentName: string) {
  return field.arguments?.find((argument) => argument.name.value === argumentName) ?? null;
}

function readNullableEnumArgument(
  field: FieldNode,
  argumentName: string,
  variables: Record<string, unknown>,
): string | null {
  const argument = getInlineArgument(field, argumentName);
  if (!argument) {
    return null;
  }

  if (argument.value.kind === Kind.ENUM || argument.value.kind === Kind.STRING) {
    return argument.value.value;
  }

  if (argument.value.kind === Kind.VARIABLE) {
    const rawValue = variables[argument.value.name.value];
    return typeof rawValue === 'string' ? rawValue : null;
  }

  return null;
}

function getFulfillmentTrackingInfoUpdateInlineIdArgument(field: FieldNode) {
  return field.arguments?.find((argument) => argument.name.value === 'fulfillmentId') ?? null;
}

function getFulfillmentCancelInlineIdArgument(field: FieldNode) {
  return field.arguments?.find((argument) => argument.name.value === 'id') ?? null;
}

function readFulfillmentCreateInput(variables: Record<string, unknown>): Record<string, unknown> {
  const fulfillment = variables['fulfillment'];
  return typeof fulfillment === 'object' && fulfillment !== null ? (fulfillment as Record<string, unknown>) : {};
}

function readFulfillmentTrackingInfoUpdateId(variables: Record<string, unknown>): string | null {
  return typeof variables['fulfillmentId'] === 'string' ? variables['fulfillmentId'] : null;
}

function readFulfillmentTrackingInfoInput(
  variables: Record<string, unknown>,
): NonNullable<OrderFulfillmentRecord['trackingInfo']>[number] | null {
  const trackingInfoInput = variables['trackingInfoInput'];
  if (typeof trackingInfoInput !== 'object' || trackingInfoInput === null) {
    return null;
  }

  const input = trackingInfoInput as Record<string, unknown>;
  return {
    number: typeof input['number'] === 'string' ? input['number'] : null,
    url: typeof input['url'] === 'string' ? input['url'] : null,
    company: typeof input['company'] === 'string' ? input['company'] : null,
  };
}

function readFulfillmentCreateTrackingInfoInput(
  fulfillment: Record<string, unknown>,
): NonNullable<OrderFulfillmentRecord['trackingInfo']>[number] | null {
  const trackingInfoInput = fulfillment['trackingInfo'];
  if (typeof trackingInfoInput !== 'object' || trackingInfoInput === null) {
    return null;
  }

  const input = trackingInfoInput as Record<string, unknown>;
  return {
    number: typeof input['number'] === 'string' ? input['number'] : null,
    url: typeof input['url'] === 'string' ? input['url'] : null,
    company: typeof input['company'] === 'string' ? input['company'] : null,
  };
}

function readFulfillmentCancelId(variables: Record<string, unknown>): string | null {
  return typeof variables['id'] === 'string' ? variables['id'] : null;
}

function findOrderWithFulfillment(
  fulfillmentId: string,
): { order: OrderRecord; fulfillment: OrderFulfillmentRecord } | null {
  for (const order of store.getOrders()) {
    const fulfillment = (order.fulfillments ?? []).find((candidate) => candidate.id === fulfillmentId);
    if (fulfillment) {
      return { order, fulfillment };
    }
  }

  return null;
}

function findOrderWithFulfillmentOrder(
  fulfillmentOrderId: string,
): { order: OrderRecord; fulfillmentOrder: OrderFulfillmentOrderRecord } | null {
  for (const order of store.getOrders()) {
    const fulfillmentOrder = (order.fulfillmentOrders ?? []).find((candidate) => candidate.id === fulfillmentOrderId);
    if (fulfillmentOrder) {
      return { order, fulfillmentOrder };
    }
  }

  return null;
}

function readFulfillmentOrderId(variables: Record<string, unknown>): string | null {
  return typeof variables['id'] === 'string' ? variables['id'] : null;
}

function readFulfillmentOrderLineItemInputs(
  variables: Record<string, unknown>,
): Array<{ id: string; quantity: number }> {
  const raw = variables['fulfillmentOrderLineItems'];
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .filter((item): item is Record<string, unknown> => typeof item === 'object' && item !== null)
    .map((item) => ({
      id: typeof item['id'] === 'string' ? item['id'] : '',
      quantity: typeof item['quantity'] === 'number' ? item['quantity'] : 0,
    }))
    .filter((item) => item.id.length > 0 && item.quantity > 0);
}

function readFulfillmentHoldInput(variables: Record<string, unknown>): Record<string, unknown> {
  const input = variables['fulfillmentHold'];
  return typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : {};
}

function fulfillmentOrderSupportsSplit(lineItems: OrderFulfillmentOrderLineItemRecord[] | undefined): boolean {
  return (lineItems ?? []).some((lineItem) => Math.max(lineItem.totalQuantity, lineItem.remainingQuantity) > 1);
}

function fulfillmentOrderSupportedActions(
  status: string | null | undefined,
  lineItems?: OrderFulfillmentOrderLineItemRecord[],
): string[] {
  switch (status) {
    case 'ON_HOLD':
      return ['RELEASE_HOLD', 'HOLD', 'MOVE'];
    case 'IN_PROGRESS':
      return ['CREATE_FULFILLMENT', 'REPORT_PROGRESS', 'HOLD', 'MARK_AS_OPEN'];
    case 'CLOSED':
      return [];
    case 'OPEN':
    default:
      return fulfillmentOrderSupportsSplit(lineItems)
        ? ['CREATE_FULFILLMENT', 'REPORT_PROGRESS', 'MOVE', 'HOLD', 'SPLIT']
        : ['CREATE_FULFILLMENT', 'REPORT_PROGRESS', 'MOVE', 'HOLD'];
  }
}

function updateOrderFulfillmentOrders(
  order: OrderRecord,
  updater: (fulfillmentOrders: OrderFulfillmentOrderRecord[]) => OrderFulfillmentOrderRecord[],
): OrderRecord {
  return store.updateOrder({
    ...order,
    updatedAt: makeSyntheticTimestamp(),
    fulfillmentOrders: updater(order.fulfillmentOrders ?? []),
  });
}

function serializeFulfillmentOrderMutationPayload(
  field: FieldNode,
  values: Record<string, OrderFulfillmentOrderRecord | OrderFulfillmentOrderRecord[] | null>,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'fulfillmentOrder':
      case 'remainingFulfillmentOrder':
      case 'movedFulfillmentOrder':
      case 'originalFulfillmentOrder':
      case 'replacementFulfillmentOrder': {
        const fulfillmentOrder = values[selection.name.value];
        payload[selectionKey] =
          fulfillmentOrder && !Array.isArray(fulfillmentOrder)
            ? serializeOrderFulfillmentOrder(selection, fulfillmentOrder)
            : null;
        break;
      }
      case 'movedFulfillmentOrders': {
        const fulfillmentOrders = values[selection.name.value];
        payload[selectionKey] = Array.isArray(fulfillmentOrders)
          ? fulfillmentOrders.map((fulfillmentOrder) => serializeOrderFulfillmentOrder(selection, fulfillmentOrder))
          : [];
        break;
      }
      case 'fulfillmentHold': {
        const fulfillmentOrder = values['fulfillmentOrder'];
        const hold =
          fulfillmentOrder && !Array.isArray(fulfillmentOrder)
            ? ((fulfillmentOrder.fulfillmentHolds ?? [])[0] ?? null)
            : null;
        payload[selectionKey] = hold
          ? Object.fromEntries(
              getSelectedChildFields(selection).map((holdSelection) => {
                const holdKey = getFieldResponseKey(holdSelection);
                switch (holdSelection.name.value) {
                  case 'id':
                    return [holdKey, hold.id];
                  case 'handle':
                    return [holdKey, hold.handle ?? null];
                  case 'reason':
                    return [holdKey, hold.reason ?? null];
                  case 'reasonNotes':
                    return [holdKey, hold.reasonNotes ?? null];
                  case 'displayReason':
                    return [holdKey, hold.displayReason ?? null];
                  case 'heldByApp':
                    return [holdKey, null];
                  case 'heldByRequestingApp':
                    return [holdKey, hold.heldByRequestingApp ?? null];
                  default:
                    return [holdKey, null];
                }
              }),
            )
          : null;
        break;
      }
      case 'userErrors':
        payload[selectionKey] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }
  return payload;
}

function splitFulfillmentOrderLineItems(
  fulfillmentOrder: OrderFulfillmentOrderRecord,
  inputs: Array<{ id: string; quantity: number }>,
): {
  selectedLineItems: OrderFulfillmentOrderLineItemRecord[];
  remainingLineItems: OrderFulfillmentOrderLineItemRecord[];
} {
  if (inputs.length === 0) {
    return {
      selectedLineItems: structuredClone(fulfillmentOrder.lineItems ?? []),
      remainingLineItems: [],
    };
  }

  const selectedLineItems: OrderFulfillmentOrderLineItemRecord[] = [];
  const remainingLineItems: OrderFulfillmentOrderLineItemRecord[] = [];
  for (const lineItem of fulfillmentOrder.lineItems ?? []) {
    const input = inputs.find((candidate) => candidate.id === lineItem.id);
    if (!input) {
      remainingLineItems.push(structuredClone(lineItem));
      continue;
    }

    const selectedQuantity = Math.min(input.quantity, lineItem.remainingQuantity, lineItem.totalQuantity);
    if (selectedQuantity > 0) {
      selectedLineItems.push({
        ...lineItem,
        totalQuantity: selectedQuantity,
        remainingQuantity: selectedQuantity,
        lineItemFulfillableQuantity: selectedQuantity,
      });
    }

    const remainingQuantity = lineItem.totalQuantity - selectedQuantity;
    if (remainingQuantity > 0) {
      remainingLineItems.push({
        ...lineItem,
        id: makeSyntheticGid('FulfillmentOrderLineItem'),
        totalQuantity: remainingQuantity,
        remainingQuantity,
        lineItemFulfillableQuantity: remainingQuantity,
      });
    }
  }

  return { selectedLineItems, remainingLineItems };
}

function buildReplacementFulfillmentOrder(
  fulfillmentOrder: OrderFulfillmentOrderRecord,
  lineItems: OrderFulfillmentOrderLineItemRecord[],
  overrides: Partial<OrderFulfillmentOrderRecord> = {},
): OrderFulfillmentOrderRecord {
  const status = overrides.status ?? 'OPEN';
  return {
    ...fulfillmentOrder,
    id: makeSyntheticGid('FulfillmentOrder'),
    updatedAt: makeSyntheticTimestamp(),
    status,
    requestStatus: overrides.requestStatus ?? fulfillmentOrder.requestStatus ?? 'UNSUBMITTED',
    supportedActions: fulfillmentOrderSupportedActions(status, lineItems),
    fulfillmentHolds: [],
    lineItems,
    ...overrides,
  };
}

function applyFulfillmentOrderHold(
  order: OrderRecord,
  fulfillmentOrder: OrderFulfillmentOrderRecord,
  variables: Record<string, unknown>,
): {
  order: OrderRecord;
  fulfillmentOrder: OrderFulfillmentOrderRecord;
  remainingFulfillmentOrder: OrderFulfillmentOrderRecord | null;
} {
  const input = readFulfillmentHoldInput(variables);
  const { selectedLineItems, remainingLineItems } = splitFulfillmentOrderLineItems(
    fulfillmentOrder,
    Array.isArray(input['fulfillmentOrderLineItems'])
      ? (input['fulfillmentOrderLineItems'] as Record<string, unknown>[])
          .map((item) => ({
            id: typeof item['id'] === 'string' ? item['id'] : '',
            quantity: typeof item['quantity'] === 'number' ? item['quantity'] : 0,
          }))
          .filter((item) => item.id.length > 0 && item.quantity > 0)
      : [],
  );
  const hold = {
    id: makeSyntheticGid('FulfillmentHold'),
    handle: readString(input['handle']),
    reason: readString(input['reason']) ?? 'OTHER',
    reasonNotes: readString(input['reasonNotes']),
    displayReason:
      readString(input['reason']) === 'OTHER' || !readString(input['reason']) ? 'Other' : readString(input['reason']),
    heldByRequestingApp: true,
  };
  const heldFulfillmentOrder: OrderFulfillmentOrderRecord = {
    ...fulfillmentOrder,
    status: 'ON_HOLD',
    updatedAt: makeSyntheticTimestamp(),
    supportedActions: fulfillmentOrderSupportedActions('ON_HOLD', selectedLineItems),
    fulfillmentHolds: [hold],
    lineItems: selectedLineItems,
  };
  const remainingFulfillmentOrder =
    remainingLineItems.length > 0
      ? buildReplacementFulfillmentOrder(fulfillmentOrder, remainingLineItems, {
          assignedLocation: fulfillmentOrder.assignedLocation,
        })
      : null;
  const updatedOrder = updateOrderFulfillmentOrders(order, (fulfillmentOrders) =>
    fulfillmentOrders.flatMap((candidate) => {
      if (candidate.id !== fulfillmentOrder.id) {
        return [candidate];
      }
      return remainingFulfillmentOrder ? [heldFulfillmentOrder, remainingFulfillmentOrder] : [heldFulfillmentOrder];
    }),
  );
  return {
    order: updatedOrder,
    fulfillmentOrder: heldFulfillmentOrder,
    remainingFulfillmentOrder,
  };
}

function applyFulfillmentOrderReleaseHold(
  order: OrderRecord,
  fulfillmentOrder: OrderFulfillmentOrderRecord,
): { order: OrderRecord; fulfillmentOrder: OrderFulfillmentOrderRecord } {
  const releasedLineItems = (fulfillmentOrder.lineItems ?? []).map((lineItem) => ({ ...lineItem }));
  const releasedLineItemsByLineItemId = new Map(
    releasedLineItems
      .filter((lineItem) => lineItem.lineItemId)
      .map((lineItem) => [lineItem.lineItemId as string, lineItem]),
  );
  const closedSiblingIds = new Set<string>();
  const closedSiblingLineItems = new Map<string, OrderFulfillmentOrderLineItemRecord[]>();

  for (const sibling of order.fulfillmentOrders ?? []) {
    if (sibling.id === fulfillmentOrder.id || sibling.status === 'CLOSED') {
      continue;
    }
    const siblingLineItems = sibling.lineItems ?? [];
    const isMatchingSplitSibling = siblingLineItems.some(
      (lineItem) => lineItem.lineItemId && releasedLineItemsByLineItemId.has(lineItem.lineItemId),
    );
    if (!isMatchingSplitSibling) {
      continue;
    }

    closedSiblingIds.add(sibling.id);
    closedSiblingLineItems.set(
      sibling.id,
      siblingLineItems.map((lineItem) => {
        const releasedLineItem = lineItem.lineItemId ? releasedLineItemsByLineItemId.get(lineItem.lineItemId) : null;
        if (releasedLineItem) {
          const currentFulfillableQuantity =
            releasedLineItem.lineItemFulfillableQuantity ?? releasedLineItem.remainingQuantity;
          releasedLineItem.totalQuantity += lineItem.totalQuantity;
          releasedLineItem.remainingQuantity += lineItem.remainingQuantity;
          releasedLineItem.lineItemFulfillableQuantity =
            currentFulfillableQuantity + (lineItem.lineItemFulfillableQuantity ?? lineItem.remainingQuantity);
        }
        return {
          ...lineItem,
          totalQuantity: 0,
          remainingQuantity: 0,
          lineItemFulfillableQuantity:
            releasedLineItem?.lineItemFulfillableQuantity ?? lineItem.lineItemFulfillableQuantity,
        };
      }),
    );
  }

  const releasedFulfillmentOrder: OrderFulfillmentOrderRecord = {
    ...fulfillmentOrder,
    status: 'OPEN',
    updatedAt: makeSyntheticTimestamp(),
    supportedActions: fulfillmentOrderSupportedActions('OPEN', releasedLineItems),
    fulfillmentHolds: [],
    lineItems: releasedLineItems,
  };
  const updatedOrder = updateOrderFulfillmentOrders(order, (fulfillmentOrders) =>
    fulfillmentOrders.map((candidate) => {
      if (candidate.id === fulfillmentOrder.id) {
        return releasedFulfillmentOrder;
      }
      if (!closedSiblingIds.has(candidate.id)) {
        return candidate;
      }
      return {
        ...candidate,
        status: 'CLOSED',
        updatedAt: makeSyntheticTimestamp(),
        supportedActions: [],
        fulfillmentHolds: [],
        lineItems: closedSiblingLineItems.get(candidate.id) ?? [],
      };
    }),
  );
  return { order: updatedOrder, fulfillmentOrder: releasedFulfillmentOrder };
}

function applyFulfillmentOrderMove(
  order: OrderRecord,
  fulfillmentOrder: OrderFulfillmentOrderRecord,
  variables: Record<string, unknown>,
): {
  order: OrderRecord;
  movedFulfillmentOrder: OrderFulfillmentOrderRecord;
  originalFulfillmentOrder: OrderFulfillmentOrderRecord;
  remainingFulfillmentOrder: OrderFulfillmentOrderRecord | null;
} {
  const splitLineItems = splitFulfillmentOrderLineItems(
    fulfillmentOrder,
    readFulfillmentOrderLineItemInputs(variables),
  );
  const restoreLineItemFulfillableQuantities = (
    lineItems: OrderFulfillmentOrderLineItemRecord[],
  ): OrderFulfillmentOrderLineItemRecord[] =>
    lineItems.map((lineItem) => {
      const originalLineItem =
        (fulfillmentOrder.lineItems ?? []).find((candidate) => candidate.lineItemId === lineItem.lineItemId) ?? null;
      return {
        ...lineItem,
        lineItemFulfillableQuantity:
          originalLineItem?.lineItemFulfillableQuantity ??
          originalLineItem?.lineItemQuantity ??
          lineItem.lineItemFulfillableQuantity,
      };
    });
  const selectedLineItems = restoreLineItemFulfillableQuantities(splitLineItems.selectedLineItems);
  const remainingLineItems = restoreLineItemFulfillableQuantities(splitLineItems.remainingLineItems);
  const newLocationId = typeof variables['newLocationId'] === 'string' ? variables['newLocationId'] : null;
  const movedFulfillmentOrder = buildReplacementFulfillmentOrder(fulfillmentOrder, selectedLineItems, {
    assignedLocation: {
      name:
        newLocationId === fulfillmentOrder.assignedLocation?.locationId
          ? fulfillmentOrder.assignedLocation.name
          : 'Shop location',
      locationId: newLocationId,
    },
  });
  const originalFulfillmentOrder: OrderFulfillmentOrderRecord = {
    ...fulfillmentOrder,
    updatedAt: makeSyntheticTimestamp(),
    supportedActions: fulfillmentOrderSupportedActions(fulfillmentOrder.status, remainingLineItems),
    lineItems: remainingLineItems.length > 0 ? remainingLineItems : [],
  };
  const remainingFulfillmentOrder = remainingLineItems.length > 0 ? originalFulfillmentOrder : null;
  const updatedOrder = updateOrderFulfillmentOrders(order, (fulfillmentOrders) =>
    fulfillmentOrders.flatMap((candidate) => {
      if (candidate.id !== fulfillmentOrder.id) {
        return [candidate];
      }
      return remainingFulfillmentOrder ? [originalFulfillmentOrder, movedFulfillmentOrder] : [movedFulfillmentOrder];
    }),
  );
  return { order: updatedOrder, movedFulfillmentOrder, originalFulfillmentOrder, remainingFulfillmentOrder };
}

function applyFulfillmentOrderStatus(
  order: OrderRecord,
  fulfillmentOrder: OrderFulfillmentOrderRecord,
  status: string,
): { order: OrderRecord; fulfillmentOrder: OrderFulfillmentOrderRecord } {
  const updatedFulfillmentOrder: OrderFulfillmentOrderRecord = {
    ...fulfillmentOrder,
    status,
    updatedAt: makeSyntheticTimestamp(),
    supportedActions: fulfillmentOrderSupportedActions(status, fulfillmentOrder.lineItems),
  };
  const updatedOrder = updateOrderFulfillmentOrders(order, (fulfillmentOrders) =>
    fulfillmentOrders.map((candidate) => (candidate.id === fulfillmentOrder.id ? updatedFulfillmentOrder : candidate)),
  );
  if (status === 'IN_PROGRESS' || status === 'OPEN') {
    const displayFulfillmentStatus = status === 'IN_PROGRESS' ? 'IN_PROGRESS' : 'UNFULFILLED';
    return {
      order: store.updateOrder({
        ...updatedOrder,
        displayFulfillmentStatus,
      }),
      fulfillmentOrder: updatedFulfillmentOrder,
    };
  }
  return { order: updatedOrder, fulfillmentOrder: updatedFulfillmentOrder };
}

function applyFulfillmentOrderCancel(
  order: OrderRecord,
  fulfillmentOrder: OrderFulfillmentOrderRecord,
): {
  order: OrderRecord;
  fulfillmentOrder: OrderFulfillmentOrderRecord;
  replacementFulfillmentOrder: OrderFulfillmentOrderRecord;
} {
  const cancelledFulfillmentOrder: OrderFulfillmentOrderRecord = {
    ...fulfillmentOrder,
    status: 'CLOSED',
    updatedAt: makeSyntheticTimestamp(),
    supportedActions: [],
    lineItems: [],
  };
  const replacementFulfillmentOrder = buildReplacementFulfillmentOrder(
    fulfillmentOrder,
    structuredClone(fulfillmentOrder.lineItems ?? []),
  );
  const updatedOrder = updateOrderFulfillmentOrders(order, (fulfillmentOrders) =>
    fulfillmentOrders.flatMap((candidate) =>
      candidate.id === fulfillmentOrder.id ? [cancelledFulfillmentOrder, replacementFulfillmentOrder] : [candidate],
    ),
  );
  return { order: updatedOrder, fulfillmentOrder: cancelledFulfillmentOrder, replacementFulfillmentOrder };
}

type FulfillmentOrderSplitResult = {
  fulfillmentOrder: OrderFulfillmentOrderRecord;
  remainingFulfillmentOrder: OrderFulfillmentOrderRecord;
  replacementFulfillmentOrder: OrderFulfillmentOrderRecord | null;
};

type FulfillmentOrderMergeResult = {
  fulfillmentOrder: OrderFulfillmentOrderRecord;
};

function readFulfillmentOrderSplitInputs(
  variables: Record<string, unknown>,
): Array<{ fulfillmentOrderId: string; fulfillmentOrderLineItems: Array<{ id: string; quantity: number }> }> {
  const raw = variables['fulfillmentOrderSplits'];
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .filter((input): input is Record<string, unknown> => typeof input === 'object' && input !== null)
    .map((input) => ({
      fulfillmentOrderId: typeof input['fulfillmentOrderId'] === 'string' ? input['fulfillmentOrderId'] : '',
      fulfillmentOrderLineItems: Array.isArray(input['fulfillmentOrderLineItems'])
        ? input['fulfillmentOrderLineItems']
            .filter(
              (lineItem): lineItem is Record<string, unknown> => typeof lineItem === 'object' && lineItem !== null,
            )
            .map((lineItem) => ({
              id: typeof lineItem['id'] === 'string' ? lineItem['id'] : '',
              quantity:
                typeof lineItem['quantity'] === 'number' && Number.isInteger(lineItem['quantity'])
                  ? lineItem['quantity']
                  : 0,
            }))
            .filter((lineItem) => lineItem.id.length > 0 && lineItem.quantity > 0)
        : [],
    }))
    .filter((input) => input.fulfillmentOrderId.length > 0);
}

function readFulfillmentOrderMergeInputs(variables: Record<string, unknown>): Array<{
  mergeIntents: Array<{
    fulfillmentOrderId: string;
    fulfillmentOrderLineItems: Array<{ id: string; quantity: number }>;
  }>;
}> {
  const raw = variables['fulfillmentOrderMergeInputs'];
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .filter((input): input is Record<string, unknown> => typeof input === 'object' && input !== null)
    .map((input) => ({
      mergeIntents: Array.isArray(input['mergeIntents'])
        ? input['mergeIntents']
            .filter((intent): intent is Record<string, unknown> => typeof intent === 'object' && intent !== null)
            .map((intent) => ({
              fulfillmentOrderId: typeof intent['fulfillmentOrderId'] === 'string' ? intent['fulfillmentOrderId'] : '',
              fulfillmentOrderLineItems: Array.isArray(intent['fulfillmentOrderLineItems'])
                ? intent['fulfillmentOrderLineItems']
                    .filter(
                      (lineItem): lineItem is Record<string, unknown> =>
                        typeof lineItem === 'object' && lineItem !== null,
                    )
                    .map((lineItem) => ({
                      id: typeof lineItem['id'] === 'string' ? lineItem['id'] : '',
                      quantity:
                        typeof lineItem['quantity'] === 'number' && Number.isInteger(lineItem['quantity'])
                          ? lineItem['quantity']
                          : 0,
                    }))
                    .filter((lineItem) => lineItem.id.length > 0 && lineItem.quantity > 0)
                : [],
            }))
            .filter((intent) => intent.fulfillmentOrderId.length > 0)
        : [],
    }))
    .filter((input) => input.mergeIntents.length > 0);
}

function readFulfillmentOrdersSetDeadlineInput(variables: Record<string, unknown>): {
  fulfillmentOrderIds: string[];
  fulfillmentDeadline: string | null;
} {
  const rawFulfillmentDeadline =
    typeof variables['fulfillmentDeadline'] === 'string' ? variables['fulfillmentDeadline'] : null;
  return {
    fulfillmentOrderIds: Array.isArray(variables['fulfillmentOrderIds'])
      ? variables['fulfillmentOrderIds'].filter((id): id is string => typeof id === 'string')
      : [],
    fulfillmentDeadline: rawFulfillmentDeadline?.replace(/\.\d{3}Z$/u, 'Z') ?? null,
  };
}

function buildFulfillmentOrderInvalidIdError(operationName: string, responseKey: string): Record<string, unknown> {
  return {
    message: 'invalid id',
    extensions: {
      code: 'RESOURCE_NOT_FOUND',
    },
    path: [responseKey || operationName],
  };
}

function fulfillmentOrderSupportedActionsWithMerge(
  status: string | null | undefined,
  lineItems: OrderFulfillmentOrderLineItemRecord[] | undefined,
  includeMerge: boolean,
): string[] {
  const actions = fulfillmentOrderSupportedActions(status, lineItems);
  return includeMerge && !actions.includes('MERGE') ? [...actions, 'MERGE'] : actions;
}

function applyFulfillmentOrderSplit(
  order: OrderRecord,
  fulfillmentOrder: OrderFulfillmentOrderRecord,
  requestedLineItems: Array<{ id: string; quantity: number }>,
): { order: OrderRecord; result: FulfillmentOrderSplitResult } {
  const requestedById = new Map(requestedLineItems.map((lineItem) => [lineItem.id, lineItem.quantity]));
  const originalLineItems: OrderFulfillmentOrderLineItemRecord[] = [];
  const splitLineItems: OrderFulfillmentOrderLineItemRecord[] = [];

  for (const lineItem of fulfillmentOrder.lineItems ?? []) {
    const requestedQuantity = Math.min(requestedById.get(lineItem.id) ?? 0, lineItem.remainingQuantity);
    if (requestedQuantity <= 0) {
      originalLineItems.push(structuredClone(lineItem));
      continue;
    }

    const originalQuantity = lineItem.totalQuantity - requestedQuantity;
    if (originalQuantity > 0) {
      originalLineItems.push({
        ...lineItem,
        totalQuantity: originalQuantity,
        remainingQuantity: Math.min(originalQuantity, lineItem.remainingQuantity - requestedQuantity),
        lineItemFulfillableQuantity:
          lineItem.lineItemFulfillableQuantity ?? lineItem.lineItemQuantity ?? lineItem.remainingQuantity,
      });
    }

    splitLineItems.push({
      ...lineItem,
      id: originalQuantity > 0 ? makeSyntheticGid('FulfillmentOrderLineItem') : lineItem.id,
      totalQuantity: requestedQuantity,
      remainingQuantity: requestedQuantity,
      lineItemFulfillableQuantity:
        lineItem.lineItemFulfillableQuantity ?? lineItem.lineItemQuantity ?? lineItem.remainingQuantity,
    });
  }

  const updatedAt = makeSyntheticTimestamp();
  const originalFulfillmentOrder: OrderFulfillmentOrderRecord = {
    ...fulfillmentOrder,
    updatedAt,
    supportedActions: fulfillmentOrderSupportedActionsWithMerge(fulfillmentOrder.status, originalLineItems, true),
    lineItems: originalLineItems,
  };
  const remainingFulfillmentOrder: OrderFulfillmentOrderRecord = {
    ...buildReplacementFulfillmentOrder(fulfillmentOrder, splitLineItems, {
      assignedLocation: fulfillmentOrder.assignedLocation,
      updatedAt,
    }),
    supportedActions: fulfillmentOrderSupportedActionsWithMerge(fulfillmentOrder.status, splitLineItems, true).filter(
      (action) => action !== 'SPLIT' || fulfillmentOrderSupportsSplit(splitLineItems),
    ),
  };

  const updatedOrder = updateOrderFulfillmentOrders(order, (fulfillmentOrders) =>
    fulfillmentOrders.flatMap((candidate) =>
      candidate.id === fulfillmentOrder.id ? [originalFulfillmentOrder, remainingFulfillmentOrder] : [candidate],
    ),
  );

  return {
    order: updatedOrder,
    result: {
      fulfillmentOrder: originalFulfillmentOrder,
      remainingFulfillmentOrder,
      replacementFulfillmentOrder: null,
    },
  };
}

function applyFulfillmentOrderMerge(
  order: OrderRecord,
  fulfillmentOrders: OrderFulfillmentOrderRecord[],
): { order: OrderRecord; result: FulfillmentOrderMergeResult } {
  const target = fulfillmentOrders[0] as OrderFulfillmentOrderRecord;
  const mergedLineItemsByLineItemId = new Map<string, OrderFulfillmentOrderLineItemRecord>();

  for (const fulfillmentOrder of fulfillmentOrders) {
    for (const lineItem of fulfillmentOrder.lineItems ?? []) {
      const key = lineItem.lineItemId ?? lineItem.id;
      const existing = mergedLineItemsByLineItemId.get(key);
      if (!existing) {
        mergedLineItemsByLineItemId.set(key, { ...lineItem });
        continue;
      }
      existing.totalQuantity += lineItem.totalQuantity;
      existing.remainingQuantity += lineItem.remainingQuantity;
      existing.lineItemFulfillableQuantity =
        existing.lineItemFulfillableQuantity ?? lineItem.lineItemFulfillableQuantity ?? existing.remainingQuantity;
    }
  }

  const mergedLineItems = [...mergedLineItemsByLineItemId.values()];
  const mergedFulfillmentOrder: OrderFulfillmentOrderRecord = {
    ...target,
    fulfillBy: fulfillmentOrders.find((candidate) => candidate.fulfillBy)?.fulfillBy ?? target.fulfillBy,
    updatedAt: makeSyntheticTimestamp(),
    supportedActions: fulfillmentOrderSupportedActions(target.status, mergedLineItems),
    lineItems: mergedLineItems,
  };
  const mergedIds = new Set(fulfillmentOrders.map((candidate) => candidate.id));
  const updatedOrder = updateOrderFulfillmentOrders(order, (existingFulfillmentOrders) =>
    existingFulfillmentOrders.map((candidate) => {
      if (candidate.id === target.id) {
        return mergedFulfillmentOrder;
      }
      if (!mergedIds.has(candidate.id)) {
        return candidate;
      }
      return {
        ...candidate,
        status: 'CLOSED',
        updatedAt: makeSyntheticTimestamp(),
        supportedActions: [],
        lineItems: zeroFulfillmentOrderLineItems(candidate.lineItems),
      };
    }),
  );

  return { order: updatedOrder, result: { fulfillmentOrder: mergedFulfillmentOrder } };
}

type FulfillmentOrderUserError = { field: string[] | null; message: string; code?: string | null };

type FulfillmentOrderMutationIdRead = { kind: 'ok'; id: string } | { kind: 'error'; error: Record<string, unknown> };

function readArgumentValue(field: FieldNode, argumentName: string, variables: Record<string, unknown>): unknown {
  const argument = getInlineArgument(field, argumentName);
  if (!argument) {
    return undefined;
  }

  switch (argument.value.kind) {
    case Kind.VARIABLE:
      return variables[argument.value.name.value];
    case Kind.STRING:
    case Kind.BOOLEAN:
    case Kind.INT:
    case Kind.FLOAT:
      return argument.value.value;
    case Kind.NULL:
      return null;
    default:
      return undefined;
  }
}

function readNullableStringMutationArgument(
  field: FieldNode,
  argumentName: string,
  variables: Record<string, unknown>,
): string | null {
  const value = readArgumentValue(field, argumentName, variables);
  return typeof value === 'string' ? value : null;
}

function readNullableBooleanMutationArgument(
  field: FieldNode,
  argumentName: string,
  variables: Record<string, unknown>,
): boolean | null {
  const value = readArgumentValue(field, argumentName, variables);
  return typeof value === 'boolean' ? value : null;
}

function readNullableArrayMutationArgument(
  field: FieldNode,
  argumentName: string,
  variables: Record<string, unknown>,
): unknown[] | null {
  const value = readArgumentValue(field, argumentName, variables);
  return Array.isArray(value) ? value : null;
}

function readFulfillmentOrderMutationId(
  field: FieldNode,
  operationName: string,
  variables: Record<string, unknown>,
): FulfillmentOrderMutationIdRead {
  const inlineIdArgument = getInlineArgument(field, 'id');
  if (!inlineIdArgument) {
    return { kind: 'error', error: buildMissingRequiredArgumentError(operationName, 'id') };
  }

  if (inlineIdArgument.value.kind === Kind.NULL) {
    return { kind: 'error', error: buildNullArgumentError(operationName, 'id', 'ID!') };
  }

  if (inlineIdArgument.value.kind === Kind.STRING) {
    return { kind: 'ok', id: inlineIdArgument.value.value };
  }

  if (inlineIdArgument.value.kind === Kind.VARIABLE) {
    const id = variables[inlineIdArgument.value.name.value];
    if (typeof id === 'string') {
      return { kind: 'ok', id };
    }
    return {
      kind: 'error',
      error: buildMissingVariableError(inlineIdArgument.value.name.value, 'ID!'),
    };
  }

  return { kind: 'error', error: buildNullArgumentError(operationName, 'id', 'ID!') };
}

function buildInvalidFulfillmentOrderIdError(
  operationName: string,
  responseKey: string,
  id: string,
): Record<string, unknown> {
  return {
    message: `Invalid id: ${id}`,
    extensions: {
      code: 'RESOURCE_NOT_FOUND',
    },
    path: [responseKey || operationName],
  };
}

function serializeUserErrors(
  field: FieldNode,
  userErrors: FulfillmentOrderUserError[],
): Array<Record<string, unknown>> {
  return userErrors.map((userError) =>
    Object.fromEntries(
      getSelectedChildFields(field).map((selection) => {
        const key = getFieldResponseKey(selection);
        switch (selection.name.value) {
          case 'field':
            return [key, userError.field];
          case 'message':
            return [key, userError.message];
          case 'code':
            return [key, userError.code ?? null];
          default:
            return [key, null];
        }
      }),
    ),
  );
}

function serializeFulfillmentOrderSplitResult(
  field: FieldNode,
  result: FulfillmentOrderSplitResult,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'fulfillmentOrder':
        payload[key] = serializeOrderFulfillmentOrder(selection, result.fulfillmentOrder, variables);
        break;
      case 'remainingFulfillmentOrder':
        payload[key] = serializeOrderFulfillmentOrder(selection, result.remainingFulfillmentOrder, variables);
        break;
      case 'replacementFulfillmentOrder':
        payload[key] = result.replacementFulfillmentOrder
          ? serializeOrderFulfillmentOrder(selection, result.replacementFulfillmentOrder, variables)
          : null;
        break;
      default:
        payload[key] = null;
        break;
    }
  }
  return payload;
}

function serializeFulfillmentOrderSplitPayload(
  field: FieldNode,
  results: FulfillmentOrderSplitResult[],
  userErrors: FulfillmentOrderUserError[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'fulfillmentOrderSplits':
        payload[key] = results.map((result) => serializeFulfillmentOrderSplitResult(selection, result, variables));
        break;
      case 'userErrors':
        payload[key] = serializeUserErrors(selection, userErrors);
        break;
      default:
        payload[key] = null;
        break;
    }
  }
  return payload;
}

function serializeFulfillmentOrderMergeResult(
  field: FieldNode,
  result: FulfillmentOrderMergeResult,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'fulfillmentOrder':
        payload[key] = serializeOrderFulfillmentOrder(selection, result.fulfillmentOrder, variables);
        break;
      default:
        payload[key] = null;
        break;
    }
  }
  return payload;
}

function serializeFulfillmentOrderMergePayload(
  field: FieldNode,
  results: FulfillmentOrderMergeResult[],
  userErrors: FulfillmentOrderUserError[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'fulfillmentOrderMerges':
        payload[key] = results.map((result) => serializeFulfillmentOrderMergeResult(selection, result, variables));
        break;
      case 'userErrors':
        payload[key] = serializeUserErrors(selection, userErrors);
        break;
      default:
        payload[key] = null;
        break;
    }
  }
  return payload;
}

function serializeFulfillmentOrdersSetDeadlinePayload(
  field: FieldNode,
  success: boolean | null,
  userErrors: FulfillmentOrderUserError[],
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'success':
        payload[key] = success;
        break;
      case 'userErrors':
        payload[key] = serializeUserErrors(selection, userErrors);
        break;
      default:
        payload[key] = null;
        break;
    }
  }
  return payload;
}

function serializeFulfillmentOrderPayload(
  field: FieldNode,
  fulfillmentOrder: OrderFulfillmentOrderRecord | null,
  userErrors: FulfillmentOrderUserError[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'fulfillmentOrder':
        payload[key] = fulfillmentOrder ? serializeOrderFulfillmentOrder(selection, fulfillmentOrder, variables) : null;
        break;
      case 'userErrors':
        payload[key] = serializeUserErrors(selection, userErrors);
        break;
      default:
        payload[key] = null;
        break;
    }
  }
  return payload;
}

function serializeSubmitFulfillmentRequestPayload(
  field: FieldNode,
  fulfillmentOrders: {
    originalFulfillmentOrder: OrderFulfillmentOrderRecord | null;
    submittedFulfillmentOrder: OrderFulfillmentOrderRecord | null;
    unsubmittedFulfillmentOrder: OrderFulfillmentOrderRecord | null;
  },
  userErrors: FulfillmentOrderUserError[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'originalFulfillmentOrder':
      case 'submittedFulfillmentOrder':
      case 'unsubmittedFulfillmentOrder': {
        const fulfillmentOrder = fulfillmentOrders[selection.name.value];
        payload[key] = fulfillmentOrder ? serializeOrderFulfillmentOrder(selection, fulfillmentOrder, variables) : null;
        break;
      }
      case 'userErrors':
        payload[key] = serializeUserErrors(selection, userErrors);
        break;
      default:
        payload[key] = null;
        break;
    }
  }
  return payload;
}

function replaceOrderFulfillmentOrder(
  order: OrderRecord,
  fulfillmentOrderId: string,
  updatedFulfillmentOrder: OrderFulfillmentOrderRecord,
  additionalFulfillmentOrders: OrderFulfillmentOrderRecord[] = [],
): void {
  store.updateOrder({
    ...order,
    updatedAt: makeSyntheticTimestamp(),
    fulfillmentOrders: [
      ...(order.fulfillmentOrders ?? []).map((fulfillmentOrder) =>
        fulfillmentOrder.id === fulfillmentOrderId ? updatedFulfillmentOrder : fulfillmentOrder,
      ),
      ...additionalFulfillmentOrders,
    ],
  });
}

function makeFulfillmentOrderMerchantRequest(
  kind: string,
  message: string | null,
  requestOptions: Record<string, unknown> = {},
): NonNullable<OrderFulfillmentOrderRecord['merchantRequests']>[number] {
  return {
    id: makeSyntheticGid('FulfillmentOrderMerchantRequest'),
    kind,
    message,
    requestOptions,
    responseData: null,
    sentAt: makeSyntheticTimestamp(),
  };
}

function readFulfillmentOrderLineItemRequests(
  field: FieldNode,
  variables: Record<string, unknown>,
): Array<{ id: string; quantity: number }> {
  const rawLineItems = readNullableArrayMutationArgument(field, 'fulfillmentOrderLineItems', variables) ?? [];
  return rawLineItems
    .filter((lineItem): lineItem is Record<string, unknown> => typeof lineItem === 'object' && lineItem !== null)
    .map((lineItem) => ({
      id: readNullableInputString(lineItem, 'id') ?? '',
      quantity:
        typeof lineItem['quantity'] === 'number' && Number.isInteger(lineItem['quantity']) ? lineItem['quantity'] : 0,
    }))
    .filter((lineItem) => lineItem.id.length > 0);
}

function buildSubmitFulfillmentRequestResult(
  fulfillmentOrder: OrderFulfillmentOrderRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): {
  submittedFulfillmentOrder: OrderFulfillmentOrderRecord | null;
  unsubmittedFulfillmentOrder: OrderFulfillmentOrderRecord | null;
  userErrors: FulfillmentOrderUserError[];
} {
  if (fulfillmentOrder.requestStatus && fulfillmentOrder.requestStatus !== 'UNSUBMITTED') {
    return {
      submittedFulfillmentOrder: null,
      unsubmittedFulfillmentOrder: null,
      userErrors: [{ field: null, message: 'Cannot request fulfillment for the fulfillment order.' }],
    };
  }

  const lineItems = fulfillmentOrder.lineItems ?? [];
  const requestedLineItems = readFulfillmentOrderLineItemRequests(field, variables);
  const requestedById = new Map(requestedLineItems.map((lineItem) => [lineItem.id, lineItem.quantity]));
  const requestAll = requestedById.size === 0;
  const requestedIds = new Set(requestedById.keys());

  for (const [lineItemId, quantity] of requestedById) {
    const lineItem = lineItems.find((candidate) => candidate.id === lineItemId);
    if (!lineItem || quantity < 1 || quantity > lineItem.remainingQuantity) {
      return {
        submittedFulfillmentOrder: null,
        unsubmittedFulfillmentOrder: null,
        userErrors: [
          {
            field: ['fulfillmentOrderLineItems'],
            message: 'Quantity must be greater than 0 and less than or equal to the remaining quantity.',
          },
        ],
      };
    }
  }

  const submittedLineItems = lineItems
    .filter((lineItem) => requestAll || requestedIds.has(lineItem.id))
    .map((lineItem) => {
      const quantity = requestAll ? lineItem.remainingQuantity : (requestedById.get(lineItem.id) ?? 0);
      return {
        ...lineItem,
        totalQuantity: quantity,
        remainingQuantity: quantity,
      };
    });
  const unsubmittedLineItems = lineItems
    .map((lineItem) => {
      const requestedQuantity = requestAll ? lineItem.remainingQuantity : (requestedById.get(lineItem.id) ?? 0);
      const remainingQuantity = lineItem.remainingQuantity - requestedQuantity;
      return remainingQuantity > 0
        ? {
            ...lineItem,
            id: makeSyntheticGid('FulfillmentOrderLineItem'),
            totalQuantity: remainingQuantity,
            remainingQuantity,
          }
        : null;
    })
    .filter((lineItem): lineItem is OrderFulfillmentOrderLineItemRecord => lineItem !== null);
  const notifyCustomer = readNullableBooleanMutationArgument(field, 'notifyCustomer', variables);
  const requestOptions = notifyCustomer === null ? {} : { notify_customer: notifyCustomer };
  const merchantRequest = makeFulfillmentOrderMerchantRequest(
    'FULFILLMENT_REQUEST',
    readNullableStringMutationArgument(field, 'message', variables),
    requestOptions,
  );
  const submittedFulfillmentOrder: OrderFulfillmentOrderRecord = {
    ...fulfillmentOrder,
    status: 'OPEN',
    requestStatus: 'SUBMITTED',
    merchantRequests: [...(fulfillmentOrder.merchantRequests ?? []), merchantRequest],
    lineItems: submittedLineItems,
  };
  const unsubmittedFulfillmentOrder: OrderFulfillmentOrderRecord | null =
    unsubmittedLineItems.length > 0
      ? {
          ...fulfillmentOrder,
          id: makeSyntheticGid('FulfillmentOrder'),
          status: 'OPEN',
          requestStatus: 'UNSUBMITTED',
          merchantRequests: [],
          lineItems: unsubmittedLineItems,
        }
      : null;

  return {
    submittedFulfillmentOrder,
    unsubmittedFulfillmentOrder,
    userErrors: [],
  };
}

function zeroFulfillmentOrderLineItems(
  lineItems: OrderFulfillmentOrderLineItemRecord[] | undefined,
): OrderFulfillmentOrderLineItemRecord[] {
  return (lineItems ?? []).map((lineItem) => ({
    ...lineItem,
    totalQuantity: 0,
    remainingQuantity: 0,
  }));
}

function readFulfillmentEventInput(variables: Record<string, unknown>): Record<string, unknown> | null {
  const input = variables['fulfillmentEvent'];
  return typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : null;
}

function readNullableInputString(input: Record<string, unknown>, key: string): string | null {
  return typeof input[key] === 'string' ? input[key] : null;
}

function readNullableInputNumber(input: Record<string, unknown>, key: string): number | null {
  return typeof input[key] === 'number' && Number.isFinite(input[key]) ? input[key] : null;
}

function buildFulfillmentEventFromInput(input: Record<string, unknown>): OrderFulfillmentEventRecord {
  const createdAt = makeSyntheticTimestamp();
  return {
    id: makeSyntheticGid('FulfillmentEvent'),
    status: readNullableInputString(input, 'status'),
    message: readNullableInputString(input, 'message'),
    happenedAt: readNullableInputString(input, 'happenedAt') ?? createdAt,
    createdAt,
    estimatedDeliveryAt: readNullableInputString(input, 'estimatedDeliveryAt'),
    city: readNullableInputString(input, 'city'),
    province: readNullableInputString(input, 'province'),
    country: readNullableInputString(input, 'country'),
    zip: readNullableInputString(input, 'zip'),
    address1: readNullableInputString(input, 'address1'),
    latitude: readNullableInputNumber(input, 'latitude'),
    longitude: readNullableInputNumber(input, 'longitude'),
  };
}

function withFulfillmentEventDerivedFields(
  fulfillment: OrderFulfillmentRecord,
  event: OrderFulfillmentEventRecord,
): OrderFulfillmentRecord {
  return {
    ...fulfillment,
    displayStatus: event.status ?? fulfillment.displayStatus,
    updatedAt: event.createdAt ?? makeSyntheticTimestamp(),
    estimatedDeliveryAt: event.estimatedDeliveryAt ?? fulfillment.estimatedDeliveryAt,
    inTransitAt: event.status === 'IN_TRANSIT' ? event.happenedAt : fulfillment.inTransitAt,
    deliveredAt: event.status === 'DELIVERED' ? event.happenedAt : fulfillment.deliveredAt,
    events: [...(fulfillment.events ?? []), event],
  };
}

function serializeFulfillmentEventCreatePayload(
  field: FieldNode,
  event: OrderFulfillmentEventRecord | null,
  userErrors: Array<{ field: string[]; message: string }>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'fulfillmentEvent':
        payload[selectionKey] = event ? serializeOrderFulfillmentEvent(selection, event) : null;
        break;
      case 'userErrors':
        payload[selectionKey] = userErrors.map((userError) =>
          Object.fromEntries(
            getSelectedChildFields(selection).map((userErrorSelection) => {
              const userErrorKey = getFieldResponseKey(userErrorSelection);
              switch (userErrorSelection.name.value) {
                case 'field':
                  return [userErrorKey, userError.field];
                case 'message':
                  return [userErrorKey, userError.message];
                default:
                  return [userErrorKey, null];
              }
            }),
          ),
        );
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }
  return payload;
}

function serializeFulfillmentMutationPayload(
  field: FieldNode,
  fulfillment: OrderFulfillmentRecord | null,
  userErrors: Array<{ field: string[]; message: string }>,
  variables: Record<string, unknown> = {},
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'fulfillment':
        payload[selectionKey] = fulfillment ? serializeOrderFulfillment(selection, fulfillment, variables) : null;
        break;
      case 'userErrors':
        payload[selectionKey] = userErrors.map((userError) =>
          Object.fromEntries(
            getSelectedChildFields(selection).map((userErrorSelection) => {
              const userErrorKey = getFieldResponseKey(userErrorSelection);
              switch (userErrorSelection.name.value) {
                case 'field':
                  return [userErrorKey, userError.field];
                case 'message':
                  return [userErrorKey, userError.message];
                default:
                  return [userErrorKey, null];
              }
            }),
          ),
        );
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }
  return payload;
}

function serializeSelectedOrderMutationPayload(field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'order':
        result[key] = null;
        break;
      case 'userErrors':
        result[key] = [{ field: ['id'], message: 'Order does not exist' }];
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function applyOrderUpdateInput(existingOrder: OrderRecord, input: Record<string, unknown>): OrderRecord {
  const updatedOrder: OrderRecord = {
    ...existingOrder,
    updatedAt: makeSyntheticTimestamp(),
    email: typeof input['email'] === 'string' ? input['email'] : (existingOrder.email ?? null),
    phone: typeof input['phone'] === 'string' ? input['phone'] : (existingOrder.phone ?? null),
    poNumber: typeof input['poNumber'] === 'string' ? input['poNumber'] : (existingOrder.poNumber ?? null),
    note: typeof input['note'] === 'string' ? input['note'] : existingOrder.note,
    tags: Array.isArray(input['tags'])
      ? input['tags']
          .filter((tag): tag is string => typeof tag === 'string')
          .sort((left, right) => left.localeCompare(right))
      : existingOrder.tags,
    customAttributes: Array.isArray(input['customAttributes'])
      ? normalizeDraftOrderAttributes(input['customAttributes'])
      : existingOrder.customAttributes,
    metafields: Array.isArray(input['metafields'])
      ? normalizeOrderMetafields(existingOrder.id, input['metafields'], existingOrder.metafields ?? [])
      : (existingOrder.metafields ?? []),
    shippingAddress:
      typeof input['shippingAddress'] === 'object' && input['shippingAddress'] !== null
        ? normalizeDraftOrderAddress(input['shippingAddress'])
        : existingOrder.shippingAddress,
    customer: existingOrder.customer ? structuredClone(existingOrder.customer) : null,
  };

  return recalculateOrderTotals(updatedOrder);
}

function validateOrderCreateInput(input: unknown): Array<{ field: string[] | null; message: string }> {
  if (typeof input !== 'object' || input === null) {
    return [{ field: ['order'], message: 'Order input is required.' }];
  }

  const inputRecord = input as Record<string, unknown>;
  const lineItems = Array.isArray(inputRecord['lineItems']) ? inputRecord['lineItems'] : [];
  if (lineItems.length === 0) {
    return [{ field: ['order', 'lineItems'], message: 'Line items must have at least one line item' }];
  }

  const hasOrderTaxLines = Array.isArray(inputRecord['taxLines']) && inputRecord['taxLines'].length > 0;
  const hasNestedLineTaxLines = lineItems.some(
    (lineItem) =>
      typeof lineItem === 'object' &&
      lineItem !== null &&
      Array.isArray((lineItem as Record<string, unknown>)['taxLines']) &&
      ((lineItem as Record<string, unknown>)['taxLines'] as unknown[]).length > 0,
  );
  const shippingLines = Array.isArray(inputRecord['shippingLines']) ? inputRecord['shippingLines'] : [];
  const hasNestedShippingTaxLines = shippingLines.some(
    (shippingLine) =>
      typeof shippingLine === 'object' &&
      shippingLine !== null &&
      Array.isArray((shippingLine as Record<string, unknown>)['taxLines']) &&
      ((shippingLine as Record<string, unknown>)['taxLines'] as unknown[]).length > 0,
  );
  if (hasOrderTaxLines && (hasNestedLineTaxLines || hasNestedShippingTaxLines)) {
    return [
      {
        field: ['order', 'taxLines'],
        message: 'Tax lines can be specified on the order or on line items and shipping lines, but not both.',
      },
    ];
  }

  const discountCode = readDiscountCodeInput(inputRecord);
  if (discountCode) {
    const supportedDiscountKeys = [
      'freeShippingDiscountCode',
      'itemFixedDiscountCode',
      'itemPercentageDiscountCode',
    ].filter((key) => readDiscountCodeAttributes(discountCode, key) !== null);
    if (supportedDiscountKeys.length > 1) {
      return [
        {
          field: ['order', 'discountCode'],
          message: 'Only one discount code type can be applied to an order.',
        },
      ];
    }
  }

  return [];
}

function validateOrderCreateOptions(
  options: Record<string, unknown>,
): Array<{ field: string[] | null; message: string }> {
  const inventoryBehaviour = options['inventoryBehaviour'];
  if (
    inventoryBehaviour !== undefined &&
    inventoryBehaviour !== 'BYPASS' &&
    inventoryBehaviour !== 'DECREMENT_IGNORING_POLICY' &&
    inventoryBehaviour !== 'DECREMENT_OBEYING_POLICY'
  ) {
    return [{ field: ['options', 'inventoryBehaviour'], message: 'Inventory behaviour is not supported.' }];
  }

  return [];
}

function serializeDraftOrderCompletePayload(
  field: FieldNode,
  draftOrder: DraftOrderRecord | null,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'draftOrder':
        payload[selectionKey] = draftOrder ? serializeDraftOrderNode(selection, draftOrder) : null;
        break;
      case 'userErrors':
        payload[selectionKey] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }

  return payload;
}

function serializeDraftOrderMutationPayload(
  field: FieldNode,
  draftOrder: DraftOrderRecord | null,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'draftOrder':
        payload[selectionKey] = draftOrder ? serializeDraftOrderNode(selection, draftOrder) : null;
        break;
      case 'userErrors':
        payload[selectionKey] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }

  return payload;
}

function serializeAbandonmentMutationPayload(
  field: FieldNode,
  abandonment: AbandonmentRecord | null,
  variables: Record<string, unknown>,
  fragments: ReturnType<typeof getDocumentFragments>,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'abandonment':
        payload[selectionKey] = abandonment
          ? serializeAbandonmentNode(selection, abandonment, variables, fragments)
          : null;
        break;
      case 'userErrors':
        payload[selectionKey] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }

  return payload;
}

function serializeDraftOrderDeletePayload(
  field: FieldNode,
  deletedId: string | null,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'deletedId':
        payload[selectionKey] = deletedId;
        break;
      case 'userErrors':
        payload[selectionKey] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }

  return payload;
}

function serializeDraftOrderBulkPayload(
  field: FieldNode,
  jobId: string | null,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'job':
        payload[selectionKey] = jobId ? serializeJob(selection, jobId, false) : null;
        break;
      case 'userErrors':
        payload[selectionKey] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }
  return payload;
}

function serializeDraftOrderCalculatePayload(
  field: FieldNode,
  calculatedDraftOrder: DraftOrderRecord | null,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'calculatedDraftOrder':
        payload[selectionKey] = calculatedDraftOrder
          ? serializeCalculatedDraftOrder(selection, calculatedDraftOrder)
          : null;
        break;
      case 'userErrors':
        payload[selectionKey] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }
  return payload;
}

function serializeDraftOrderInvoicePreviewPayload(
  field: FieldNode,
  draftOrder: DraftOrderRecord | null,
  args: Record<string, unknown>,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Record<string, unknown> {
  const email =
    typeof args['email'] === 'object' && args['email'] !== null ? (args['email'] as Record<string, unknown>) : {};
  const subject =
    typeof email['subject'] === 'string' && email['subject'].length > 0 ? email['subject'] : 'Complete your purchase';
  const customMessage = typeof email['customMessage'] === 'string' ? email['customMessage'] : '';
  const payload: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'previewSubject':
        payload[selectionKey] = draftOrder ? subject : null;
        break;
      case 'previewHtml':
        payload[selectionKey] = draftOrder
          ? `<!DOCTYPE html><html><body><h1>${subject}</h1><p>${customMessage}</p><p>Draft order ${draftOrder.name}</p></body></html>`
          : null;
        break;
      case 'userErrors':
        payload[selectionKey] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }

  return payload;
}

type ReturnUserError = { field: string[] | null; message: string };

function readReturnInput(
  variables: Record<string, unknown>,
  key: 'returnInput' | 'input',
): Record<string, unknown> | null {
  const input = variables[key];
  return typeof input === 'object' && input !== null && !Array.isArray(input)
    ? (input as Record<string, unknown>)
    : null;
}

function readReturnMutationId(field: FieldNode, variables: Record<string, unknown>): string | null {
  const args = getFieldArguments(field, variables);
  return typeof args['id'] === 'string' ? args['id'] : null;
}

function findOrderWithReturn(returnId: string): { order: OrderRecord; orderReturn: OrderReturnRecord } | null {
  for (const order of store.getOrders()) {
    const orderReturn = order.returns.find((candidate) => candidate.id === returnId) ?? null;
    if (orderReturn) {
      return { order, orderReturn };
    }
  }
  return null;
}

function findOrderWithReverseFulfillmentOrder(reverseFulfillmentOrderId: string): {
  order: OrderRecord;
  orderReturn: OrderReturnRecord;
  reverseFulfillmentOrder: OrderReverseFulfillmentOrderRecord;
} | null {
  for (const order of store.getOrders()) {
    for (const orderReturn of order.returns) {
      const reverseFulfillmentOrder =
        (orderReturn.reverseFulfillmentOrders ?? []).find((candidate) => candidate.id === reverseFulfillmentOrderId) ??
        null;
      if (reverseFulfillmentOrder) {
        return { order, orderReturn, reverseFulfillmentOrder };
      }
    }
  }
  return null;
}

function findOrderWithReverseDelivery(reverseDeliveryId: string): {
  order: OrderRecord;
  orderReturn: OrderReturnRecord;
  reverseFulfillmentOrder: OrderReverseFulfillmentOrderRecord;
  reverseDelivery: OrderReverseDeliveryRecord;
} | null {
  for (const order of store.getOrders()) {
    for (const orderReturn of order.returns) {
      for (const reverseFulfillmentOrder of orderReturn.reverseFulfillmentOrders ?? []) {
        const reverseDelivery =
          (reverseFulfillmentOrder.reverseDeliveries ?? []).find((candidate) => candidate.id === reverseDeliveryId) ??
          null;
        if (reverseDelivery) {
          return { order, orderReturn, reverseFulfillmentOrder, reverseDelivery };
        }
      }
    }
  }
  return null;
}

function findFulfillmentLineItem(
  order: OrderRecord,
  fulfillmentLineItemId: string,
): OrderFulfillmentLineItemRecord | null {
  for (const fulfillment of order.fulfillments ?? []) {
    const lineItem =
      (fulfillment.fulfillmentLineItems ?? []).find((candidate) => candidate.id === fulfillmentLineItemId) ?? null;
    if (lineItem) {
      return lineItem;
    }
  }
  return null;
}

function buildReverseFulfillmentOrderFromReturn(
  order: OrderRecord,
  orderReturn: OrderReturnRecord,
): OrderReverseFulfillmentOrderRecord {
  const lineItems: OrderReverseFulfillmentOrderLineItemRecord[] = (orderReturn.returnLineItems ?? []).map(
    (lineItem) => ({
      id: makeSyntheticGid('ReverseFulfillmentOrderLineItem'),
      returnLineItemId: lineItem.id,
      fulfillmentLineItemId: lineItem.fulfillmentLineItemId,
      lineItemId: lineItem.lineItemId,
      title: lineItem.title,
      totalQuantity: lineItem.quantity,
      remainingQuantity: Math.max(0, lineItem.quantity - (lineItem.processedQuantity ?? 0)),
      disposedQuantity: 0,
    }),
  );

  return {
    id: makeSyntheticGid('ReverseFulfillmentOrder'),
    orderId: order.id,
    returnId: orderReturn.id,
    status: 'OPEN',
    lineItems,
    reverseDeliveries: [],
  };
}

function ensureReturnReverseFulfillmentOrders(order: OrderRecord, orderReturn: OrderReturnRecord): OrderReturnRecord {
  if ((orderReturn.reverseFulfillmentOrders ?? []).length > 0) {
    return orderReturn;
  }

  return {
    ...orderReturn,
    reverseFulfillmentOrders: [buildReverseFulfillmentOrderFromReturn(order, orderReturn)],
  };
}

function buildReturnLineItemsFromInput(
  order: OrderRecord,
  rawLineItems: unknown,
): { lineItems: OrderReturnLineItemRecord[] } | { userErrors: ReturnUserError[] } {
  if (!Array.isArray(rawLineItems) || rawLineItems.length === 0) {
    return { userErrors: [{ field: ['returnLineItems'], message: 'Return must include at least one line item.' }] };
  }

  const lineItems: OrderReturnLineItemRecord[] = [];
  const userErrors: ReturnUserError[] = [];
  for (const [index, rawLineItem] of rawLineItems.entries()) {
    if (typeof rawLineItem !== 'object' || rawLineItem === null || Array.isArray(rawLineItem)) {
      userErrors.push({ field: ['returnLineItems', String(index)], message: 'Return line item is invalid.' });
      continue;
    }

    const input = rawLineItem as Record<string, unknown>;
    const fulfillmentLineItemId =
      typeof input['fulfillmentLineItemId'] === 'string' ? input['fulfillmentLineItemId'] : null;
    const quantity =
      typeof input['quantity'] === 'number' && Number.isFinite(input['quantity']) ? input['quantity'] : 0;
    const fulfillmentLineItem = fulfillmentLineItemId ? findFulfillmentLineItem(order, fulfillmentLineItemId) : null;

    if (!fulfillmentLineItem) {
      userErrors.push({
        field: ['returnLineItems', String(index), 'fulfillmentLineItemId'],
        message: 'Fulfillment line item does not exist.',
      });
      continue;
    }

    if (quantity <= 0 || quantity > fulfillmentLineItem.quantity) {
      userErrors.push({
        field: ['returnLineItems', String(index), 'quantity'],
        message: 'Quantity is not available for return.',
      });
      continue;
    }

    lineItems.push({
      id: makeSyntheticGid('ReturnLineItem'),
      fulfillmentLineItemId: fulfillmentLineItem.id,
      lineItemId: fulfillmentLineItem.lineItemId,
      title: fulfillmentLineItem.title,
      quantity,
      processedQuantity: 0,
      returnReason: typeof input['returnReason'] === 'string' ? input['returnReason'] : 'UNKNOWN',
      returnReasonNote: typeof input['returnReasonNote'] === 'string' ? input['returnReasonNote'] : '',
      customerNote: null,
    });
  }

  return userErrors.length > 0 ? { userErrors } : { lineItems };
}

function buildOrderReturnFromInput(
  order: OrderRecord,
  input: Record<string, unknown>,
  status: 'OPEN' | 'REQUESTED',
): { orderReturn: OrderReturnRecord } | { userErrors: ReturnUserError[] } {
  const lineItemResult = buildReturnLineItemsFromInput(order, input['returnLineItems']);
  if ('userErrors' in lineItemResult) {
    return lineItemResult;
  }

  const createdAt = typeof input['requestedAt'] === 'string' ? input['requestedAt'] : makeSyntheticTimestamp();
  const totalQuantity = lineItemResult.lineItems.reduce((total, lineItem) => total + lineItem.quantity, 0);
  const orderReturn: OrderReturnRecord = {
    id: makeSyntheticGid('Return'),
    orderId: order.id,
    name: `${order.name}-R${order.returns.length + 1}`,
    status,
    createdAt,
    closedAt: null,
    decline: null,
    totalQuantity,
    returnLineItems: lineItemResult.lineItems,
    reverseFulfillmentOrders: [],
  };

  return {
    orderReturn: status === 'OPEN' ? ensureReturnReverseFulfillmentOrders(order, orderReturn) : orderReturn,
  };
}

function applyReturnCreate(
  input: Record<string, unknown> | null,
  status: 'OPEN' | 'REQUESTED',
): { order: OrderRecord | null; orderReturn: OrderReturnRecord | null; userErrors: ReturnUserError[] } {
  if (!input) {
    return { order: null, orderReturn: null, userErrors: [{ field: ['input'], message: 'Input is required.' }] };
  }

  const orderId = typeof input['orderId'] === 'string' ? input['orderId'] : null;
  const order = orderId ? store.getOrderById(orderId) : null;
  if (!order) {
    return {
      order: null,
      orderReturn: null,
      userErrors: [{ field: ['orderId'], message: 'Order does not exist.' }],
    };
  }

  const result = buildOrderReturnFromInput(order, input, status);
  if ('userErrors' in result) {
    return { order, orderReturn: null, userErrors: result.userErrors };
  }

  const updatedOrder = store.updateOrder({
    ...order,
    updatedAt: makeSyntheticTimestamp(),
    returns: [result.orderReturn, ...order.returns],
  });
  const stagedReturn =
    updatedOrder.returns.find((candidate) => candidate.id === result.orderReturn.id) ?? result.orderReturn;
  return { order: updatedOrder, orderReturn: stagedReturn, userErrors: [] };
}

function applyReturnStatusUpdate(
  returnId: string | null,
  status: 'OPEN' | 'CANCELED' | 'CLOSED',
): { order: OrderRecord | null; orderReturn: OrderReturnRecord | null; userErrors: ReturnUserError[] } {
  if (!returnId) {
    return { order: null, orderReturn: null, userErrors: [{ field: ['id'], message: 'Return does not exist.' }] };
  }

  const match = findOrderWithReturn(returnId);
  if (!match) {
    return { order: null, orderReturn: null, userErrors: [{ field: ['id'], message: 'Return does not exist.' }] };
  }

  const updatedReturn: OrderReturnRecord = {
    ...match.orderReturn,
    status,
    closedAt: status === 'CLOSED' ? makeSyntheticTimestamp() : null,
  };
  const updatedOrder = store.updateOrder({
    ...match.order,
    updatedAt: makeSyntheticTimestamp(),
    returns: match.order.returns.map((candidate) => (candidate.id === returnId ? updatedReturn : candidate)),
  });
  const stagedReturn = updatedOrder.returns.find((candidate) => candidate.id === returnId) ?? updatedReturn;
  return { order: updatedOrder, orderReturn: stagedReturn, userErrors: [] };
}

function readReturnRequestInputId(input: Record<string, unknown> | null): string | null {
  return typeof input?.['id'] === 'string' ? input['id'] : null;
}

function applyReturnApproveRequest(input: Record<string, unknown> | null): {
  order: OrderRecord | null;
  orderReturn: OrderReturnRecord | null;
  userErrors: ReturnUserError[];
} {
  const returnId = readReturnRequestInputId(input);
  if (!returnId) {
    return {
      order: null,
      orderReturn: null,
      userErrors: [{ field: ['input', 'id'], message: 'Return does not exist.' }],
    };
  }

  const match = findOrderWithReturn(returnId);
  if (!match) {
    return {
      order: null,
      orderReturn: null,
      userErrors: [{ field: ['input', 'id'], message: 'Return does not exist.' }],
    };
  }

  if (match.orderReturn.status !== 'REQUESTED') {
    return {
      order: match.order,
      orderReturn: null,
      userErrors: [
        {
          field: ['input', 'id'],
          message: 'Return is not approvable. Only returns with status REQUESTED can be approved.',
        },
      ],
    };
  }

  const approvedReturn = ensureReturnReverseFulfillmentOrders(match.order, {
    ...match.orderReturn,
    status: 'OPEN',
    decline: null,
  });
  const updatedOrder = store.updateOrder({
    ...match.order,
    updatedAt: makeSyntheticTimestamp(),
    returns: match.order.returns.map((candidate) => (candidate.id === returnId ? approvedReturn : candidate)),
  });
  return {
    order: updatedOrder,
    orderReturn: updatedOrder.returns.find((candidate) => candidate.id === returnId) ?? approvedReturn,
    userErrors: [],
  };
}

function applyReturnDeclineRequest(input: Record<string, unknown> | null): {
  order: OrderRecord | null;
  orderReturn: OrderReturnRecord | null;
  userErrors: ReturnUserError[];
} {
  const returnId = readReturnRequestInputId(input);
  if (!returnId) {
    return {
      order: null,
      orderReturn: null,
      userErrors: [{ field: ['input', 'id'], message: 'Return does not exist.' }],
    };
  }

  const match = findOrderWithReturn(returnId);
  if (!match) {
    return {
      order: null,
      orderReturn: null,
      userErrors: [{ field: ['input', 'id'], message: 'Return does not exist.' }],
    };
  }

  if (match.orderReturn.status !== 'REQUESTED') {
    return {
      order: match.order,
      orderReturn: null,
      userErrors: [
        {
          field: ['input', 'id'],
          message: 'Return is not declinable. Only non-refunded returns with status REQUESTED can be declined.',
        },
      ],
    };
  }

  const declinedReturn: OrderReturnRecord = {
    ...match.orderReturn,
    status: 'DECLINED',
    decline: {
      reason: typeof input?.['declineReason'] === 'string' ? input['declineReason'] : null,
      note: typeof input?.['declineNote'] === 'string' ? input['declineNote'] : null,
    },
  };
  const updatedOrder = store.updateOrder({
    ...match.order,
    updatedAt: makeSyntheticTimestamp(),
    returns: match.order.returns.map((candidate) => (candidate.id === returnId ? declinedReturn : candidate)),
  });
  return {
    order: updatedOrder,
    orderReturn: updatedOrder.returns.find((candidate) => candidate.id === returnId) ?? declinedReturn,
    userErrors: [],
  };
}

function syncReverseFulfillmentLineItems(order: OrderRecord, orderReturn: OrderReturnRecord): OrderReturnRecord {
  const reverseFulfillmentOrders = orderReturn.reverseFulfillmentOrders ?? [];
  if (reverseFulfillmentOrders.length === 0) {
    return orderReturn.status === 'OPEN' ? ensureReturnReverseFulfillmentOrders(order, orderReturn) : orderReturn;
  }

  const returnLineItems = orderReturn.returnLineItems ?? [];
  const syncedReverseFulfillmentOrders = reverseFulfillmentOrders.map((reverseFulfillmentOrder) => ({
    ...reverseFulfillmentOrder,
    lineItems: returnLineItems.map((returnLineItem) => {
      const existing =
        reverseFulfillmentOrder.lineItems.find((lineItem) => lineItem.returnLineItemId === returnLineItem.id) ?? null;
      const processedQuantity = returnLineItem.processedQuantity ?? 0;
      return {
        id: existing?.id ?? makeSyntheticGid('ReverseFulfillmentOrderLineItem'),
        returnLineItemId: returnLineItem.id,
        fulfillmentLineItemId: returnLineItem.fulfillmentLineItemId,
        lineItemId: returnLineItem.lineItemId,
        title: returnLineItem.title,
        totalQuantity: returnLineItem.quantity,
        remainingQuantity: Math.max(0, returnLineItem.quantity - processedQuantity),
        disposedQuantity: existing?.disposedQuantity ?? 0,
        dispositionType: existing?.dispositionType ?? null,
        dispositionLocationId: existing?.dispositionLocationId ?? null,
      };
    }),
  }));

  return {
    ...orderReturn,
    reverseFulfillmentOrders: syncedReverseFulfillmentOrders,
  };
}

function applyRemoveFromReturn(
  field: FieldNode,
  variables: Record<string, unknown>,
): { order: OrderRecord | null; orderReturn: OrderReturnRecord | null; userErrors: ReturnUserError[] } {
  const args = getFieldArguments(field, variables);
  const returnId = typeof args['returnId'] === 'string' ? args['returnId'] : null;
  const match = returnId ? findOrderWithReturn(returnId) : null;
  if (!match) {
    return { order: null, orderReturn: null, userErrors: [{ field: ['returnId'], message: 'Return does not exist.' }] };
  }

  const rawReturnLineItems = Array.isArray(args['returnLineItems']) ? args['returnLineItems'] : [];
  const rawExchangeLineItems = Array.isArray(args['exchangeLineItems']) ? args['exchangeLineItems'] : [];
  if (rawReturnLineItems.length === 0 && rawExchangeLineItems.length === 0) {
    return {
      order: match.order,
      orderReturn: null,
      userErrors: [{ field: ['returnLineItems'], message: 'Return line items or exchange line items are required.' }],
    };
  }

  if (rawExchangeLineItems.length > 0) {
    return {
      order: match.order,
      orderReturn: null,
      userErrors: [
        {
          field: ['exchangeLineItems'],
          message: 'Exchange line item removal is not supported by the local return model yet.',
        },
      ],
    };
  }

  const nextLineItems = [...(match.orderReturn.returnLineItems ?? [])];
  const userErrors: ReturnUserError[] = [];
  for (const [index, rawLineItem] of rawReturnLineItems.entries()) {
    const input =
      typeof rawLineItem === 'object' && rawLineItem !== null ? (rawLineItem as Record<string, unknown>) : {};
    const lineItemId = typeof input['returnLineItemId'] === 'string' ? input['returnLineItemId'] : null;
    const quantity =
      typeof input['quantity'] === 'number' && Number.isFinite(input['quantity']) ? input['quantity'] : 0;
    const lineItemIndex = lineItemId ? nextLineItems.findIndex((lineItem) => lineItem.id === lineItemId) : -1;
    const lineItem = lineItemIndex >= 0 ? nextLineItems[lineItemIndex] : null;

    if (!lineItem) {
      userErrors.push({
        field: ['returnLineItems', String(index), 'returnLineItemId'],
        message: 'Return line item does not exist.',
      });
      continue;
    }

    const removableQuantity = lineItem.quantity - (lineItem.processedQuantity ?? 0);
    if (quantity <= 0 || quantity > removableQuantity) {
      userErrors.push({
        field: ['returnLineItems', String(index), 'quantity'],
        message: 'Quantity is not removable from return.',
      });
      continue;
    }

    const nextQuantity = lineItem.quantity - quantity;
    if (nextQuantity <= 0) {
      nextLineItems.splice(lineItemIndex, 1);
    } else {
      nextLineItems[lineItemIndex] = { ...lineItem, quantity: nextQuantity };
    }
  }

  if (userErrors.length > 0) {
    return { order: match.order, orderReturn: null, userErrors };
  }

  const updatedReturn = syncReverseFulfillmentLineItems(match.order, {
    ...match.orderReturn,
    totalQuantity: nextLineItems.reduce((total, lineItem) => total + lineItem.quantity, 0),
    returnLineItems: nextLineItems,
  });
  const updatedOrder = store.updateOrder({
    ...match.order,
    updatedAt: makeSyntheticTimestamp(),
    returns: match.order.returns.map((candidate) =>
      candidate.id === match.orderReturn.id ? updatedReturn : candidate,
    ),
  });
  return {
    order: updatedOrder,
    orderReturn: updatedOrder.returns.find((candidate) => candidate.id === match.orderReturn.id) ?? updatedReturn,
    userErrors: [],
  };
}

function applyReturnProcess(input: Record<string, unknown> | null): {
  order: OrderRecord | null;
  orderReturn: OrderReturnRecord | null;
  userErrors: ReturnUserError[];
} {
  const returnId = typeof input?.['returnId'] === 'string' ? input['returnId'] : null;
  const match = returnId ? findOrderWithReturn(returnId) : null;
  if (!match) {
    return {
      order: null,
      orderReturn: null,
      userErrors: [{ field: ['input', 'returnId'], message: 'Return does not exist.' }],
    };
  }

  if (match.orderReturn.status !== 'OPEN') {
    return {
      order: match.order,
      orderReturn: null,
      userErrors: [{ field: ['input', 'returnId'], message: 'Only OPEN returns can be processed.' }],
    };
  }

  const rawReturnLineItems = Array.isArray(input?.['returnLineItems']) ? input['returnLineItems'] : [];
  if (rawReturnLineItems.length === 0) {
    return {
      order: match.order,
      orderReturn: null,
      userErrors: [{ field: ['input', 'returnLineItems'], message: 'Return line items are required.' }],
    };
  }

  const nextLineItems = [...(match.orderReturn.returnLineItems ?? [])];
  const userErrors: ReturnUserError[] = [];
  for (const [index, rawLineItem] of rawReturnLineItems.entries()) {
    const item =
      typeof rawLineItem === 'object' && rawLineItem !== null ? (rawLineItem as Record<string, unknown>) : {};
    const lineItemId = typeof item['id'] === 'string' ? item['id'] : null;
    const quantity = typeof item['quantity'] === 'number' && Number.isFinite(item['quantity']) ? item['quantity'] : 0;
    const lineItemIndex = lineItemId ? nextLineItems.findIndex((lineItem) => lineItem.id === lineItemId) : -1;
    const lineItem = lineItemIndex >= 0 ? nextLineItems[lineItemIndex] : null;

    if (!lineItem) {
      userErrors.push({
        field: ['input', 'returnLineItems', String(index), 'id'],
        message: 'Return line item does not exist.',
      });
      continue;
    }

    const unprocessedQuantity = lineItem.quantity - (lineItem.processedQuantity ?? 0);
    if (quantity <= 0 || quantity > unprocessedQuantity) {
      userErrors.push({
        field: ['input', 'returnLineItems', String(index), 'quantity'],
        message: 'Quantity is not processable.',
      });
      continue;
    }

    nextLineItems[lineItemIndex] = {
      ...lineItem,
      processedQuantity: (lineItem.processedQuantity ?? 0) + quantity,
    };
  }

  if (userErrors.length > 0) {
    return { order: match.order, orderReturn: null, userErrors };
  }

  const allProcessed = nextLineItems.every((lineItem) => (lineItem.processedQuantity ?? 0) >= lineItem.quantity);
  const updatedReturn = syncReverseFulfillmentLineItems(match.order, {
    ...ensureReturnReverseFulfillmentOrders(match.order, match.orderReturn),
    status: allProcessed ? 'CLOSED' : match.orderReturn.status,
    closedAt: allProcessed ? makeSyntheticTimestamp() : match.orderReturn.closedAt,
    returnLineItems: nextLineItems,
  });
  const updatedOrder = store.updateOrder({
    ...match.order,
    updatedAt: makeSyntheticTimestamp(),
    returns: match.order.returns.map((candidate) =>
      candidate.id === match.orderReturn.id ? updatedReturn : candidate,
    ),
  });
  return {
    order: updatedOrder,
    orderReturn: updatedOrder.returns.find((candidate) => candidate.id === match.orderReturn.id) ?? updatedReturn,
    userErrors: [],
  };
}

function serializeReturnMutationPayload(
  field: FieldNode,
  orderReturn: OrderReturnRecord | null,
  order: OrderRecord | null,
  userErrors: ReturnUserError[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'return':
        payload[selectionKey] =
          orderReturn && order ? serializeOrderReturn(selection, orderReturn, variables, order) : null;
        break;
      case 'userErrors':
        payload[selectionKey] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }

  return payload;
}

function serializeReverseDeliveryMutationPayload(
  field: FieldNode,
  reverseDelivery: OrderReverseDeliveryRecord | null,
  reverseFulfillmentOrder: OrderReverseFulfillmentOrderRecord | null,
  orderReturn: OrderReturnRecord | null,
  order: OrderRecord | null,
  userErrors: ReturnUserError[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'reverseDelivery':
        payload[key] =
          reverseDelivery && reverseFulfillmentOrder && orderReturn && order
            ? serializeOrderReverseDelivery(
                selection,
                reverseDelivery,
                reverseFulfillmentOrder,
                orderReturn,
                order,
                variables,
              )
            : null;
        break;
      case 'userErrors':
        payload[key] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[key] = null;
        break;
    }
  }
  return payload;
}

function serializeReverseFulfillmentOrderDisposePayload(
  field: FieldNode,
  lineItems: OrderReverseFulfillmentOrderLineItemRecord[],
  userErrors: ReturnUserError[],
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'reverseFulfillmentOrderLineItems':
        payload[key] = lineItems.map((lineItem) =>
          Object.fromEntries(
            getSelectedChildFields(selection).map((lineItemSelection) => {
              const lineItemKey = getFieldResponseKey(lineItemSelection);
              switch (lineItemSelection.name.value) {
                case 'id':
                  return [lineItemKey, lineItem.id];
                case 'totalQuantity':
                case 'quantity':
                  return [lineItemKey, lineItem.totalQuantity];
                case 'remainingQuantity':
                  return [lineItemKey, lineItem.remainingQuantity];
                case 'dispositionType':
                  return [lineItemKey, lineItem.dispositionType ?? null];
                default:
                  return [lineItemKey, null];
              }
            }),
          ),
        );
        break;
      case 'userErrors':
        payload[key] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[key] = null;
        break;
    }
  }
  return payload;
}

function normalizeReverseDeliveryTracking(raw: unknown): OrderReverseDeliveryRecord['tracking'] {
  const input = typeof raw === 'object' && raw !== null ? (raw as Record<string, unknown>) : null;
  if (!input) {
    return null;
  }
  return {
    number:
      typeof input['number'] === 'string'
        ? input['number']
        : typeof input['trackingNumber'] === 'string'
          ? input['trackingNumber']
          : null,
    url:
      typeof input['url'] === 'string'
        ? input['url']
        : typeof input['trackingUrl'] === 'string'
          ? input['trackingUrl']
          : null,
    company:
      typeof input['company'] === 'string'
        ? input['company']
        : typeof input['carrierName'] === 'string'
          ? input['carrierName']
          : null,
  };
}

function normalizeReverseDeliveryLabel(raw: unknown): OrderReverseDeliveryRecord['label'] {
  const input = typeof raw === 'object' && raw !== null ? (raw as Record<string, unknown>) : null;
  if (!input) {
    return null;
  }
  return {
    publicFileUrl:
      typeof input['publicFileUrl'] === 'string'
        ? input['publicFileUrl']
        : typeof input['url'] === 'string'
          ? input['url']
          : null,
  };
}

function applyReverseDeliveryCreateWithShipping(
  field: FieldNode,
  variables: Record<string, unknown>,
): {
  order: OrderRecord | null;
  orderReturn: OrderReturnRecord | null;
  reverseFulfillmentOrder: OrderReverseFulfillmentOrderRecord | null;
  reverseDelivery: OrderReverseDeliveryRecord | null;
  userErrors: ReturnUserError[];
} {
  const args = getFieldArguments(field, variables);
  const reverseFulfillmentOrderId =
    typeof args['reverseFulfillmentOrderId'] === 'string' ? args['reverseFulfillmentOrderId'] : null;
  const match = reverseFulfillmentOrderId ? findOrderWithReverseFulfillmentOrder(reverseFulfillmentOrderId) : null;
  if (!match) {
    return {
      order: null,
      orderReturn: null,
      reverseFulfillmentOrder: null,
      reverseDelivery: null,
      userErrors: [{ field: ['reverseFulfillmentOrderId'], message: 'Reverse fulfillment order does not exist.' }],
    };
  }

  const rawLineItems = Array.isArray(args['reverseDeliveryLineItems']) ? args['reverseDeliveryLineItems'] : [];
  if (rawLineItems.length === 0) {
    return {
      order: match.order,
      orderReturn: match.orderReturn,
      reverseFulfillmentOrder: match.reverseFulfillmentOrder,
      reverseDelivery: null,
      userErrors: [{ field: ['reverseDeliveryLineItems'], message: 'Reverse delivery line items are required.' }],
    };
  }

  const userErrors: ReturnUserError[] = [];
  const lineItems = rawLineItems.flatMap(
    (rawLineItem, index): OrderReverseDeliveryRecord['reverseDeliveryLineItems'] => {
      const input =
        typeof rawLineItem === 'object' && rawLineItem !== null ? (rawLineItem as Record<string, unknown>) : {};
      const lineItemId =
        typeof input['reverseFulfillmentOrderLineItemId'] === 'string'
          ? input['reverseFulfillmentOrderLineItemId']
          : null;
      const quantity =
        typeof input['quantity'] === 'number' && Number.isFinite(input['quantity']) ? input['quantity'] : 0;
      const lineItem = lineItemId
        ? (match.reverseFulfillmentOrder.lineItems.find((candidate) => candidate.id === lineItemId) ?? null)
        : null;
      if (!lineItem) {
        userErrors.push({
          field: ['reverseDeliveryLineItems', String(index), 'reverseFulfillmentOrderLineItemId'],
          message: 'Reverse fulfillment order line item does not exist.',
        });
        return [];
      }
      if (quantity <= 0 || quantity > lineItem.totalQuantity) {
        userErrors.push({
          field: ['reverseDeliveryLineItems', String(index), 'quantity'],
          message: 'Quantity is not available for reverse delivery.',
        });
        return [];
      }
      return [
        {
          id: makeSyntheticGid('ReverseDeliveryLineItem'),
          reverseFulfillmentOrderLineItemId: lineItem.id,
          quantity,
        },
      ];
    },
  );

  if (userErrors.length > 0) {
    return {
      order: match.order,
      orderReturn: match.orderReturn,
      reverseFulfillmentOrder: match.reverseFulfillmentOrder,
      reverseDelivery: null,
      userErrors,
    };
  }

  const reverseDelivery: OrderReverseDeliveryRecord = {
    id: makeSyntheticGid('ReverseDelivery'),
    reverseFulfillmentOrderId: match.reverseFulfillmentOrder.id,
    reverseDeliveryLineItems: lineItems,
    tracking: normalizeReverseDeliveryTracking(args['trackingInput']),
    label: normalizeReverseDeliveryLabel(args['labelInput']),
  };
  const updatedReverseFulfillmentOrder: OrderReverseFulfillmentOrderRecord = {
    ...match.reverseFulfillmentOrder,
    reverseDeliveries: [reverseDelivery, ...(match.reverseFulfillmentOrder.reverseDeliveries ?? [])],
  };
  const updatedReturn: OrderReturnRecord = {
    ...match.orderReturn,
    reverseFulfillmentOrders: (match.orderReturn.reverseFulfillmentOrders ?? []).map((candidate) =>
      candidate.id === updatedReverseFulfillmentOrder.id ? updatedReverseFulfillmentOrder : candidate,
    ),
  };
  const updatedOrder = store.updateOrder({
    ...match.order,
    updatedAt: makeSyntheticTimestamp(),
    returns: match.order.returns.map((candidate) => (candidate.id === updatedReturn.id ? updatedReturn : candidate)),
  });
  const stagedReturn = updatedOrder.returns.find((candidate) => candidate.id === updatedReturn.id) ?? updatedReturn;
  const stagedReverseFulfillmentOrder =
    (stagedReturn.reverseFulfillmentOrders ?? []).find(
      (candidate) => candidate.id === updatedReverseFulfillmentOrder.id,
    ) ?? updatedReverseFulfillmentOrder;
  const stagedReverseDelivery =
    (stagedReverseFulfillmentOrder.reverseDeliveries ?? []).find((candidate) => candidate.id === reverseDelivery.id) ??
    reverseDelivery;
  return {
    order: updatedOrder,
    orderReturn: stagedReturn,
    reverseFulfillmentOrder: stagedReverseFulfillmentOrder,
    reverseDelivery: stagedReverseDelivery,
    userErrors: [],
  };
}

function applyReverseDeliveryShippingUpdate(
  field: FieldNode,
  variables: Record<string, unknown>,
): {
  order: OrderRecord | null;
  orderReturn: OrderReturnRecord | null;
  reverseFulfillmentOrder: OrderReverseFulfillmentOrderRecord | null;
  reverseDelivery: OrderReverseDeliveryRecord | null;
  userErrors: ReturnUserError[];
} {
  const args = getFieldArguments(field, variables);
  const reverseDeliveryId = typeof args['reverseDeliveryId'] === 'string' ? args['reverseDeliveryId'] : null;
  const match = reverseDeliveryId ? findOrderWithReverseDelivery(reverseDeliveryId) : null;
  if (!match) {
    return {
      order: null,
      orderReturn: null,
      reverseFulfillmentOrder: null,
      reverseDelivery: null,
      userErrors: [{ field: ['reverseDeliveryId'], message: 'Reverse delivery does not exist.' }],
    };
  }

  const updatedReverseDelivery: OrderReverseDeliveryRecord = {
    ...match.reverseDelivery,
    tracking: normalizeReverseDeliveryTracking(args['trackingInput']) ?? match.reverseDelivery.tracking ?? null,
    label: normalizeReverseDeliveryLabel(args['labelInput']) ?? match.reverseDelivery.label ?? null,
  };
  const updatedReverseFulfillmentOrder: OrderReverseFulfillmentOrderRecord = {
    ...match.reverseFulfillmentOrder,
    reverseDeliveries: (match.reverseFulfillmentOrder.reverseDeliveries ?? []).map((candidate) =>
      candidate.id === updatedReverseDelivery.id ? updatedReverseDelivery : candidate,
    ),
  };
  const updatedReturn: OrderReturnRecord = {
    ...match.orderReturn,
    reverseFulfillmentOrders: (match.orderReturn.reverseFulfillmentOrders ?? []).map((candidate) =>
      candidate.id === updatedReverseFulfillmentOrder.id ? updatedReverseFulfillmentOrder : candidate,
    ),
  };
  const updatedOrder = store.updateOrder({
    ...match.order,
    updatedAt: makeSyntheticTimestamp(),
    returns: match.order.returns.map((candidate) => (candidate.id === updatedReturn.id ? updatedReturn : candidate)),
  });
  return {
    order: updatedOrder,
    orderReturn: updatedReturn,
    reverseFulfillmentOrder: updatedReverseFulfillmentOrder,
    reverseDelivery: updatedReverseDelivery,
    userErrors: [],
  };
}

function applyReverseFulfillmentOrderDispose(
  field: FieldNode,
  variables: Record<string, unknown>,
): { lineItems: OrderReverseFulfillmentOrderLineItemRecord[]; userErrors: ReturnUserError[] } {
  const args = getFieldArguments(field, variables);
  const rawInputs = Array.isArray(args['dispositionInputs']) ? args['dispositionInputs'] : [];
  if (rawInputs.length === 0) {
    return {
      lineItems: [],
      userErrors: [{ field: ['dispositionInputs'], message: 'Disposition inputs are required.' }],
    };
  }

  const updates: Array<{
    match: NonNullable<ReturnType<typeof findOrderWithReverseFulfillmentOrder>>;
    lineItem: OrderReverseFulfillmentOrderLineItemRecord;
    quantity: number;
    dispositionType: string | null;
    locationId: string | null;
  }> = [];
  const userErrors: ReturnUserError[] = [];

  for (const [index, rawInput] of rawInputs.entries()) {
    const input = typeof rawInput === 'object' && rawInput !== null ? (rawInput as Record<string, unknown>) : {};
    const lineItemId =
      typeof input['reverseFulfillmentOrderLineItemId'] === 'string'
        ? input['reverseFulfillmentOrderLineItemId']
        : null;
    const quantity =
      typeof input['quantity'] === 'number' && Number.isFinite(input['quantity']) ? input['quantity'] : 0;
    let found: {
      match: NonNullable<ReturnType<typeof findOrderWithReverseFulfillmentOrder>>;
      lineItem: OrderReverseFulfillmentOrderLineItemRecord;
    } | null = null;
    if (lineItemId) {
      for (const order of store.getOrders()) {
        for (const orderReturn of order.returns) {
          for (const reverseFulfillmentOrder of orderReturn.reverseFulfillmentOrders ?? []) {
            const lineItem = reverseFulfillmentOrder.lineItems.find((candidate) => candidate.id === lineItemId) ?? null;
            if (lineItem) {
              found = { match: { order, orderReturn, reverseFulfillmentOrder }, lineItem };
              break;
            }
          }
          if (found) break;
        }
        if (found) break;
      }
    }

    if (!found) {
      userErrors.push({
        field: ['dispositionInputs', String(index), 'reverseFulfillmentOrderLineItemId'],
        message: 'Reverse fulfillment order line item does not exist.',
      });
      continue;
    }

    const disposableQuantity = found.lineItem.totalQuantity - (found.lineItem.disposedQuantity ?? 0);
    if (quantity <= 0 || quantity > disposableQuantity) {
      userErrors.push({
        field: ['dispositionInputs', String(index), 'quantity'],
        message: 'Quantity is not disposable.',
      });
      continue;
    }

    updates.push({
      ...found,
      quantity,
      dispositionType: typeof input['dispositionType'] === 'string' ? input['dispositionType'] : null,
      locationId: typeof input['locationId'] === 'string' ? input['locationId'] : null,
    });
  }

  if (userErrors.length > 0) {
    return { lineItems: [], userErrors };
  }

  const disposedLineItems: OrderReverseFulfillmentOrderLineItemRecord[] = [];
  for (const update of updates) {
    const updatedLineItem: OrderReverseFulfillmentOrderLineItemRecord = {
      ...update.lineItem,
      remainingQuantity: Math.max(0, update.lineItem.remainingQuantity - update.quantity),
      disposedQuantity: (update.lineItem.disposedQuantity ?? 0) + update.quantity,
      dispositionType: update.dispositionType,
      dispositionLocationId: update.locationId,
    };
    const updatedReverseFulfillmentOrder: OrderReverseFulfillmentOrderRecord = {
      ...update.match.reverseFulfillmentOrder,
      status: update.match.reverseFulfillmentOrder.lineItems.every((lineItem) =>
        lineItem.id === updatedLineItem.id ? updatedLineItem.remainingQuantity === 0 : lineItem.remainingQuantity === 0,
      )
        ? 'CLOSED'
        : update.match.reverseFulfillmentOrder.status,
      lineItems: update.match.reverseFulfillmentOrder.lineItems.map((lineItem) =>
        lineItem.id === updatedLineItem.id ? updatedLineItem : lineItem,
      ),
    };
    const updatedReturn: OrderReturnRecord = {
      ...update.match.orderReturn,
      reverseFulfillmentOrders: (update.match.orderReturn.reverseFulfillmentOrders ?? []).map((candidate) =>
        candidate.id === updatedReverseFulfillmentOrder.id ? updatedReverseFulfillmentOrder : candidate,
      ),
    };
    store.updateOrder({
      ...update.match.order,
      updatedAt: makeSyntheticTimestamp(),
      returns: update.match.order.returns.map((candidate) =>
        candidate.id === updatedReturn.id ? updatedReturn : candidate,
      ),
    });
    disposedLineItems.push(updatedLineItem);
  }

  return { lineItems: disposedLineItems, userErrors: [] };
}

export function handleOrderMutation(
  document: string,
  variables: Record<string, unknown> = {},
  readMode: ReadMode,
  shopifyAdminOrigin = 'https://example.myshopify.com',
): { data?: Record<string, unknown>; errors?: Array<Record<string, unknown>> } | null {
  const data: Record<string, unknown> = {};
  const errors: Array<Record<string, unknown>> = [];
  const fragments = getDocumentFragments(document);
  let handled = false;

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);

    if (field.name.value === 'paymentTermsCreate') {
      handled = true;
      const referenceId = readNullableStringArgument(field, 'referenceId', variables);
      const attributes = readInputObjectArgument(field, 'paymentTermsAttributes', variables, 'paymentTermsAttributes');
      const validation = validatePaymentTermsAttributes(attributes, ['paymentTermsAttributes']);
      if (validation.userErrors.length > 0 || validation.templateId === null) {
        data[key] = serializePaymentTermsMutationPayload(field, null, validation.userErrors);
        continue;
      }

      const owner = findPaymentTermsOwnerByReferenceId(referenceId);
      if (!owner) {
        data[key] = serializePaymentTermsMutationPayload(field, null, [
          {
            field: ['referenceId'],
            message: 'Reference order or draft order does not exist.',
            code: 'NOT_FOUND',
          },
        ]);
        continue;
      }

      if (owner.paymentTerms) {
        data[key] = serializePaymentTermsMutationPayload(field, null, [
          {
            field: ['referenceId'],
            message: 'Payment terms already exist.',
            code: 'PAYMENT_TERMS_ALREADY_EXISTS',
          },
        ]);
        continue;
      }

      const paymentTerms = buildPaymentTermsFromAttributes(
        attributes,
        validation.templateId,
        owner.record.totalPriceSet,
      );
      storePaymentTermsOwner(owner, paymentTerms);
      data[key] = serializePaymentTermsMutationPayload(field, paymentTerms, []);
      continue;
    }

    if (field.name.value === 'paymentTermsUpdate') {
      handled = true;
      const input = readInputObjectArgument(field, 'input', variables, 'input');
      const paymentTermsId = typeof input?.['paymentTermsId'] === 'string' ? input['paymentTermsId'] : null;
      const attributes =
        typeof input?.['paymentTermsAttributes'] === 'object' && input['paymentTermsAttributes'] !== null
          ? (input['paymentTermsAttributes'] as Record<string, unknown>)
          : null;
      const validation = validatePaymentTermsAttributes(attributes, ['paymentTermsAttributes']);
      if (validation.userErrors.length > 0 || validation.templateId === null) {
        data[key] = serializePaymentTermsMutationPayload(field, null, validation.userErrors);
        continue;
      }

      const owner = findPaymentTermsOwnerByPaymentTermsId(paymentTermsId);
      if (!owner || !owner.paymentTerms) {
        data[key] = serializePaymentTermsMutationPayload(field, null, [
          {
            field: ['paymentTermsId'],
            message: 'Payment terms do not exist.',
            code: 'PAYMENT_TERMS_NOT_FOUND',
          },
        ]);
        continue;
      }

      const paymentTerms = buildPaymentTermsFromAttributes(
        attributes,
        validation.templateId,
        owner.record.totalPriceSet,
        owner.paymentTerms,
      );
      storePaymentTermsOwner(owner, paymentTerms);
      data[key] = serializePaymentTermsMutationPayload(field, paymentTerms, []);
      continue;
    }

    if (field.name.value === 'paymentTermsDelete') {
      handled = true;
      const input = readInputObjectArgument(field, 'input', variables, 'input');
      const paymentTermsId = typeof input?.['paymentTermsId'] === 'string' ? input['paymentTermsId'] : null;
      const owner = findPaymentTermsOwnerByPaymentTermsId(paymentTermsId);
      if (!paymentTermsId || !owner || !owner.paymentTerms) {
        data[key] = serializePaymentTermsDeletePayload(field, null, [
          {
            field: ['paymentTermsId'],
            message: 'Payment terms do not exist.',
            code: 'PAYMENT_TERMS_DELETE_UNSUCCESSFUL',
          },
        ]);
        continue;
      }

      storePaymentTermsOwner(owner, null);
      data[key] = serializePaymentTermsDeletePayload(field, paymentTermsId, []);
      continue;
    }

    if (field.name.value === 'returnCreate' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const result = applyReturnCreate(readReturnInput(variables, 'returnInput'), 'OPEN');
      data[key] = serializeReturnMutationPayload(field, result.orderReturn, result.order, result.userErrors, variables);
      continue;
    }

    if (field.name.value === 'returnRequest' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const result = applyReturnCreate(readReturnInput(variables, 'input'), 'REQUESTED');
      data[key] = serializeReturnMutationPayload(field, result.orderReturn, result.order, result.userErrors, variables);
      continue;
    }

    if (field.name.value === 'returnApproveRequest' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const result = applyReturnApproveRequest(readReturnInput(variables, 'input'));
      data[key] = serializeReturnMutationPayload(field, result.orderReturn, result.order, result.userErrors, variables);
      continue;
    }

    if (field.name.value === 'returnDeclineRequest' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const result = applyReturnDeclineRequest(readReturnInput(variables, 'input'));
      data[key] = serializeReturnMutationPayload(field, result.orderReturn, result.order, result.userErrors, variables);
      continue;
    }

    if (field.name.value === 'removeFromReturn' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const result = applyRemoveFromReturn(field, variables);
      data[key] = serializeReturnMutationPayload(field, result.orderReturn, result.order, result.userErrors, variables);
      continue;
    }

    if (field.name.value === 'returnProcess' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const result = applyReturnProcess(readReturnInput(variables, 'input'));
      data[key] = serializeReturnMutationPayload(field, result.orderReturn, result.order, result.userErrors, variables);
      continue;
    }

    if (
      field.name.value === 'reverseDeliveryCreateWithShipping' &&
      (readMode === 'snapshot' || readMode === 'live-hybrid')
    ) {
      handled = true;
      const result = applyReverseDeliveryCreateWithShipping(field, variables);
      data[key] = serializeReverseDeliveryMutationPayload(
        field,
        result.reverseDelivery,
        result.reverseFulfillmentOrder,
        result.orderReturn,
        result.order,
        result.userErrors,
        variables,
      );
      continue;
    }

    if (
      field.name.value === 'reverseDeliveryShippingUpdate' &&
      (readMode === 'snapshot' || readMode === 'live-hybrid')
    ) {
      handled = true;
      const result = applyReverseDeliveryShippingUpdate(field, variables);
      data[key] = serializeReverseDeliveryMutationPayload(
        field,
        result.reverseDelivery,
        result.reverseFulfillmentOrder,
        result.orderReturn,
        result.order,
        result.userErrors,
        variables,
      );
      continue;
    }

    if (
      field.name.value === 'reverseFulfillmentOrderDispose' &&
      (readMode === 'snapshot' || readMode === 'live-hybrid')
    ) {
      handled = true;
      const result = applyReverseFulfillmentOrderDispose(field, variables);
      data[key] = serializeReverseFulfillmentOrderDisposePayload(field, result.lineItems, result.userErrors);
      continue;
    }

    if (
      (field.name.value === 'returnCancel' ||
        field.name.value === 'returnClose' ||
        field.name.value === 'returnReopen') &&
      (readMode === 'snapshot' || readMode === 'live-hybrid')
    ) {
      handled = true;
      const statusByRoot: Record<string, 'OPEN' | 'CANCELED' | 'CLOSED'> = {
        returnCancel: 'CANCELED',
        returnClose: 'CLOSED',
        returnReopen: 'OPEN',
      };
      const status = statusByRoot[field.name.value] ?? 'OPEN';
      const result = applyReturnStatusUpdate(readReturnMutationId(field, variables), status);
      data[key] = serializeReturnMutationPayload(field, result.orderReturn, result.order, result.userErrors, variables);
      continue;
    }

    if (field.name.value === 'orderCapture' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const input = readOrderCaptureInput(variables);
      if (!input) {
        data[key] = serializeOrderCapturePayload(field, null, null, [
          { field: ['input'], message: 'Input is required.' },
        ]);
        continue;
      }

      const orderId =
        typeof input['id'] === 'string' ? input['id'] : typeof input['orderId'] === 'string' ? input['orderId'] : null;
      const parentTransactionId =
        typeof input['parentTransactionId'] === 'string'
          ? input['parentTransactionId']
          : typeof input['transactionId'] === 'string'
            ? input['transactionId']
            : null;
      const order = orderId ? store.getOrderById(orderId) : null;
      const authorization =
        order && parentTransactionId
          ? (order.transactions.find((transaction) => transaction.id === parentTransactionId) ?? null)
          : null;

      if (!order) {
        data[key] = serializeOrderCapturePayload(field, null, null, [
          { field: ['input', 'id'], message: 'Order does not exist' },
        ]);
        continue;
      }

      if (!authorization) {
        data[key] = serializeOrderCapturePayload(field, null, order, [
          { field: ['input', 'parentTransactionId'], message: 'Transaction does not exist' },
        ]);
        continue;
      }

      const result = captureOrderPayment(order, authorization, input);
      if ('userErrors' in result) {
        data[key] = serializeOrderCapturePayload(field, null, order, result.userErrors);
        continue;
      }

      const updatedOrder = store.updateOrder(result.order);
      data[key] = serializeOrderCapturePayload(field, result.transaction, updatedOrder, []);
      continue;
    }

    if (field.name.value === 'transactionVoid' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const transactionId = readTransactionVoidId(variables);
      const match = transactionId ? findOrderWithTransaction(transactionId) : null;

      if (!match) {
        data[key] = serializeTransactionVoidPayload(field, null, [
          { field: ['id'], message: 'Transaction does not exist' },
        ]);
        continue;
      }

      const result = voidOrderTransaction(match.order, match.transaction);
      if ('userErrors' in result) {
        data[key] = serializeTransactionVoidPayload(field, null, result.userErrors);
        continue;
      }

      store.updateOrder(result.order);
      data[key] = serializeTransactionVoidPayload(field, result.transaction, []);
      continue;
    }

    if (field.name.value === 'orderCreateMandatePayment' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const input = readMandatePaymentInput(variables);
      const orderId =
        typeof input['id'] === 'string' ? input['id'] : typeof input['orderId'] === 'string' ? input['orderId'] : null;
      const order = orderId ? store.getOrderById(orderId) : null;

      if (!order) {
        data[key] = serializeOrderCreateMandatePaymentPayload(field, null, null, [
          { field: ['id'], message: 'Order does not exist' },
        ]);
        continue;
      }

      const result = createMandatePayment(order, input);
      if ('userErrors' in result) {
        data[key] = serializeOrderCreateMandatePaymentPayload(field, null, order, result.userErrors);
        continue;
      }

      const updatedOrder = store.updateOrder(result.order);
      data[key] = serializeOrderCreateMandatePaymentPayload(field, result.mandatePayment, updatedOrder, []);
      continue;
    }

    if (field.name.value === 'refundCreate' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const input = readRefundCreateInput(variables);
      if (!input) {
        data[key] = serializeRefundCreatePayload(field, null, null, [
          { field: ['input'], message: 'Input is required.' },
        ]);
        continue;
      }

      const orderId = typeof input['orderId'] === 'string' ? input['orderId'] : null;
      const order = orderId ? store.getOrderById(orderId) : null;
      if (!order) {
        data[key] = serializeRefundCreatePayload(field, null, null, [
          { field: ['input', 'orderId'], message: 'Order does not exist' },
        ]);
        continue;
      }

      const refund = buildRefundFromInput(order, input);
      const refundAmount = parseDecimalAmount(refund.totalRefundedSet?.shopMoney.amount);
      const alreadyRefunded = sumRefundedAmount(order);
      const refundableAmount = parseDecimalAmount(order.totalPriceSet?.shopMoney.amount) - alreadyRefunded;
      const allowOverRefunding = input['allowOverRefunding'] === true;

      if (!allowOverRefunding && refundAmount > refundableAmount) {
        data[key] = serializeRefundCreatePayload(field, null, order, [
          {
            field: null,
            message: `Refund amount $${refundAmount.toFixed(2)} is greater than net payment received $${refundableAmount.toFixed(2)}`,
          },
        ]);
        continue;
      }

      const updatedOrder = store.updateOrder(applyRefundToOrder(order, refund));
      const stagedRefund = updatedOrder.refunds.find((candidate) => candidate.id === refund.id) ?? refund;
      data[key] = serializeRefundCreatePayload(field, stagedRefund, updatedOrder, []);
      continue;
    }

    if (field.name.value === 'orderUpdate') {
      handled = true;
      const inlineInput = getOrderUpdateInlineInput(field);
      if (inlineInput) {
        const idField = inlineInput.fields.find((objectField) => objectField.name.value === 'id') ?? null;
        if (!idField) {
          errors.push(buildOrderUpdateMissingInlineIdError());
          continue;
        }

        if (idField.value.kind === Kind.NULL) {
          errors.push(buildOrderUpdateNullInlineIdError());
          continue;
        }
      }

      const input = readOrderUpdateInput(variables);
      const id = input['id'];

      if (typeof id !== 'string' || id.length === 0) {
        errors.push(buildOrderUpdateMissingIdError(input));
        continue;
      }

      const existingOrder = store.getOrderById(id);
      if (existingOrder && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
        const updatedOrder = store.updateOrder(applyOrderUpdateInput(existingOrder, input));

        const payload: Record<string, unknown> = {};
        for (const selection of getSelectedChildFields(field)) {
          const selectionKey = getFieldResponseKey(selection);
          switch (selection.name.value) {
            case 'order':
              payload[selectionKey] = serializeOrderNode(selection, updatedOrder);
              break;
            case 'userErrors':
              payload[selectionKey] = [];
              break;
            default:
              payload[selectionKey] = null;
              break;
          }
        }
        data[key] = payload;
        continue;
      }

      data[key] = {
        order: null,
        userErrors: [{ field: ['id'], message: 'Order does not exist' }],
        ...serializeSelectedOrderMutationPayload(field),
      };

      continue;
    }

    if (field.name.value === 'orderClose' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const input = variables['input'];
      const id =
        typeof input === 'object' && input !== null && typeof (input as Record<string, unknown>)['id'] === 'string'
          ? ((input as Record<string, unknown>)['id'] as string)
          : null;
      const order = id ? store.getOrderById(id) : null;

      if (!order) {
        data[key] = serializeOrderManagementPayload(field, null, [{ field: ['id'], message: 'Order does not exist' }]);
        continue;
      }

      if (id && store.hasStagedOrder(id) && order.closed) {
        data[key] = serializeOrderManagementPayload(field, order, [
          { field: ['id'], message: 'Order is already closed' },
        ]);
        continue;
      }

      const closedAt = makeSyntheticTimestamp();
      const updatedOrder = store.updateOrder({
        ...order,
        updatedAt: closedAt,
        closed: true,
        closedAt,
      });
      data[key] = serializeOrderManagementPayload(field, updatedOrder, []);
      continue;
    }

    if (field.name.value === 'orderOpen' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const input = variables['input'];
      const id =
        typeof input === 'object' && input !== null && typeof (input as Record<string, unknown>)['id'] === 'string'
          ? ((input as Record<string, unknown>)['id'] as string)
          : null;
      const order = id ? store.getOrderById(id) : null;

      if (!order) {
        data[key] = serializeOrderManagementPayload(field, null, [{ field: ['id'], message: 'Order does not exist' }]);
        continue;
      }

      if (id && store.hasStagedOrder(id) && order.cancelledAt) {
        data[key] = serializeOrderManagementPayload(field, order, [
          { field: ['id'], message: 'Canceled orders cannot be opened' },
        ]);
        continue;
      }

      if (id && store.hasStagedOrder(id) && !order.closed) {
        data[key] = serializeOrderManagementPayload(field, order, [
          { field: ['id'], message: 'Order is already open' },
        ]);
        continue;
      }

      const updatedOrder = store.updateOrder({
        ...order,
        updatedAt: makeSyntheticTimestamp(),
        closed: false,
        closedAt: null,
      });
      data[key] = serializeOrderManagementPayload(field, updatedOrder, []);
      continue;
    }

    if (field.name.value === 'orderMarkAsPaid' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const input = variables['input'];
      const id =
        typeof input === 'object' && input !== null && typeof (input as Record<string, unknown>)['id'] === 'string'
          ? ((input as Record<string, unknown>)['id'] as string)
          : null;
      const order = id ? store.getOrderById(id) : null;

      if (!order) {
        data[key] = serializeOrderManagementPayload(field, null, [{ field: ['id'], message: 'Order does not exist' }]);
        continue;
      }

      if (
        id &&
        store.hasStagedOrder(id) &&
        (order.displayFinancialStatus === 'PAID' ||
          parseDecimalAmount(order.totalOutstandingSet?.shopMoney.amount) <= 0)
      ) {
        data[key] = serializeOrderManagementPayload(field, order, [
          { field: ['id'], message: 'Order is already paid' },
        ]);
        continue;
      }

      const updatedOrder = store.updateOrder(order.displayFinancialStatus === 'PAID' ? order : markOrderAsPaid(order));
      data[key] = serializeOrderManagementPayload(field, updatedOrder, []);
      continue;
    }

    if (field.name.value === 'orderCreateManualPayment' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      data[key] = null;
      errors.push(
        buildAccessDeniedError(
          'orderCreateManualPayment',
          '`write_orders` access scope. Also: The user must have mark_orders_as_paid permission. The API client must be installed on a Shopify Plus store to use the amount field.',
        ),
      );
      continue;
    }

    if (field.name.value === 'orderCustomerSet' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const orderId = typeof variables['orderId'] === 'string' ? variables['orderId'] : null;
      const customerId = typeof variables['customerId'] === 'string' ? variables['customerId'] : null;
      const order = orderId ? store.getOrderById(orderId) : null;

      if (!order) {
        data[key] = serializeOrderManagementPayload(field, null, [
          { field: ['orderId'], message: 'Order does not exist' },
        ]);
        continue;
      }

      const customer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      const stagedOrder = orderId ? store.hasStagedOrder(orderId) : false;
      if (!customer && stagedOrder) {
        data[key] = serializeOrderManagementPayload(field, order, [
          { field: ['customerId'], message: 'Customer does not exist' },
        ]);
        continue;
      }

      if (customer && stagedOrder && order.customer?.id === customer.id) {
        data[key] = serializeOrderManagementPayload(field, order, [
          { field: ['customerId'], message: 'Order already has this customer' },
        ]);
        continue;
      }

      const orderCustomer =
        customer !== null
          ? orderCustomerFromCustomer(customer)
          : order.customer?.id === customerId
            ? structuredClone(order.customer)
            : { id: customerId ?? '', email: null, displayName: null };
      const updatedOrder = store.updateOrder({
        ...order,
        updatedAt: makeSyntheticTimestamp(),
        customer: orderCustomer,
      });
      data[key] = serializeOrderManagementPayload(field, updatedOrder, []);
      continue;
    }

    if (field.name.value === 'orderCustomerRemove' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const orderId = typeof variables['orderId'] === 'string' ? variables['orderId'] : null;
      const order = orderId ? store.getOrderById(orderId) : null;

      if (!order) {
        data[key] = serializeOrderManagementPayload(field, null, [
          { field: ['orderId'], message: 'Order does not exist' },
        ]);
        continue;
      }

      if (orderId && store.hasStagedOrder(orderId) && order.customer === null) {
        data[key] = serializeOrderManagementPayload(field, order, [
          { field: ['orderId'], message: 'Order does not have a customer' },
        ]);
        continue;
      }

      const updatedOrder = store.updateOrder({
        ...order,
        updatedAt: makeSyntheticTimestamp(),
        customer: null,
      });
      data[key] = serializeOrderManagementPayload(field, updatedOrder, []);
      continue;
    }

    if (field.name.value === 'orderInvoiceSend' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const id = typeof variables['id'] === 'string' ? variables['id'] : null;
      const order = id ? store.getOrderById(id) : null;
      data[key] = serializeOrderManagementPayload(
        field,
        order,
        order ? [] : [{ field: ['id'], message: 'Order does not exist' }],
      );
      continue;
    }

    if (field.name.value === 'taxSummaryCreate' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      data[key] = null;
      errors.push(
        buildAccessDeniedError(
          'taxSummaryCreate',
          '`write_taxes` access scope. Also: The caller must be a tax calculations app and the relevant feature must be on.',
        ),
      );
      continue;
    }

    if (field.name.value === 'orderCancel' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const orderId = typeof variables['orderId'] === 'string' ? variables['orderId'] : null;
      const order = orderId ? store.getOrderById(orderId) : null;

      if (!order) {
        data[key] = serializeOrderCancelPayload(field, [{ field: ['orderId'], message: 'Order does not exist' }]);
        continue;
      }

      if (orderId && store.hasStagedOrder(orderId) && order.cancelledAt) {
        data[key] = serializeOrderCancelPayload(field, [{ field: ['orderId'], message: 'Order is already canceled' }]);
        continue;
      }

      const cancelledAt = makeSyntheticTimestamp();
      store.updateOrder({
        ...order,
        updatedAt: cancelledAt,
        closed: true,
        closedAt: cancelledAt,
        cancelledAt,
        cancelReason: typeof variables['reason'] === 'string' ? variables['reason'] : 'OTHER',
      });
      data[key] = serializeOrderCancelPayload(field, []);
      continue;
    }

    if (field.name.value === 'orderDelete' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const args = getFieldArguments(field, variables);
      const orderId = typeof args['orderId'] === 'string' ? args['orderId'] : null;
      const order = orderId ? store.getOrderById(orderId) : null;
      if (orderId && order) {
        store.deleteOrder(orderId);
      }
      const payload: Record<string, unknown> = {};
      for (const selection of getSelectedChildFields(field)) {
        const selectionKey = getFieldResponseKey(selection);
        switch (selection.name.value) {
          case 'deletedId':
            payload[selectionKey] = order ? orderId : null;
            break;
          case 'userErrors':
            payload[selectionKey] = order ? [] : [{ field: ['orderId'], message: 'Order does not exist' }];
            break;
          default:
            payload[selectionKey] = null;
            break;
        }
      }
      data[key] = payload;
      continue;
    }

    if (field.name.value === 'orderCreate') {
      const inlineOrderArgument = getOrderCreateInlineArgument(field);

      if (!inlineOrderArgument) {
        handled = true;
        errors.push(buildOrderCreateMissingInlineOrderError());
        continue;
      }

      if (inlineOrderArgument.value.kind === Kind.NULL) {
        handled = true;
        errors.push(buildOrderCreateNullInlineOrderError());
        continue;
      }

      const order = readVariableBackedInputArgument(field, 'order', variables, 'order');

      if (order === null) {
        handled = true;
        errors.push(buildOrderCreateMissingOrderError());
        continue;
      }

      if (readMode === 'snapshot' || readMode === 'live-hybrid') {
        handled = true;
        const options = readOrderCreateOptions(variables);
        const validationErrors = [...validateOrderCreateInput(order), ...validateOrderCreateOptions(options)];
        if (validationErrors.length > 0) {
          const payload: Record<string, unknown> = {};
          for (const selection of getSelectedChildFields(field)) {
            const selectionKey = getFieldResponseKey(selection);
            switch (selection.name.value) {
              case 'order':
                payload[selectionKey] = null;
                break;
              case 'userErrors':
                payload[selectionKey] = serializeSelectedUserErrors(selection, validationErrors);
                break;
              default:
                payload[selectionKey] = null;
                break;
            }
          }
          data[key] = payload;
          continue;
        }

        const stagedOrder = store.stageCreateOrder(buildOrderFromInput(order));
        const payload: Record<string, unknown> = {};
        for (const selection of getSelectedChildFields(field)) {
          const selectionKey = getFieldResponseKey(selection);
          switch (selection.name.value) {
            case 'order':
              payload[selectionKey] = serializeOrderNode(selection, stagedOrder);
              break;
            case 'userErrors':
              payload[selectionKey] = [];
              break;
            default:
              payload[selectionKey] = null;
              break;
          }
        }
        data[key] = payload;
      }

      continue;
    }

    if (
      field.name.value === 'abandonmentUpdateActivitiesDeliveryStatuses' &&
      (readMode === 'snapshot' || readMode === 'live-hybrid')
    ) {
      handled = true;
      const inlineAbandonmentIdArgument = getInlineArgument(field, 'abandonmentId');
      const inlineMarketingActivityIdArgument = getInlineArgument(field, 'marketingActivityId');
      const inlineDeliveryStatusArgument = getInlineArgument(field, 'deliveryStatus');

      if (!inlineAbandonmentIdArgument) {
        errors.push(buildMissingRequiredArgumentError('abandonmentUpdateActivitiesDeliveryStatuses', 'abandonmentId'));
        continue;
      }

      if (inlineAbandonmentIdArgument.value.kind === Kind.NULL) {
        errors.push(buildNullArgumentError('abandonmentUpdateActivitiesDeliveryStatuses', 'abandonmentId', 'ID!'));
        continue;
      }

      if (!inlineMarketingActivityIdArgument) {
        errors.push(
          buildMissingRequiredArgumentError('abandonmentUpdateActivitiesDeliveryStatuses', 'marketingActivityId'),
        );
        continue;
      }

      if (inlineMarketingActivityIdArgument.value.kind === Kind.NULL) {
        errors.push(
          buildNullArgumentError('abandonmentUpdateActivitiesDeliveryStatuses', 'marketingActivityId', 'ID!'),
        );
        continue;
      }

      if (!inlineDeliveryStatusArgument) {
        errors.push(buildMissingRequiredArgumentError('abandonmentUpdateActivitiesDeliveryStatuses', 'deliveryStatus'));
        continue;
      }

      if (inlineDeliveryStatusArgument.value.kind === Kind.NULL) {
        errors.push(
          buildNullArgumentError(
            'abandonmentUpdateActivitiesDeliveryStatuses',
            'deliveryStatus',
            'AbandonmentDeliveryState!',
          ),
        );
        continue;
      }

      const abandonmentId = readNullableStringArgument(field, 'abandonmentId', variables);
      if (inlineAbandonmentIdArgument.value.kind === Kind.VARIABLE && abandonmentId === null) {
        errors.push(buildMissingVariableError(inlineAbandonmentIdArgument.value.name.value, 'ID!'));
        continue;
      }

      const marketingActivityId = readNullableStringArgument(field, 'marketingActivityId', variables);
      if (inlineMarketingActivityIdArgument.value.kind === Kind.VARIABLE && marketingActivityId === null) {
        errors.push(buildMissingVariableError(inlineMarketingActivityIdArgument.value.name.value, 'ID!'));
        continue;
      }

      const deliveryStatus = readNullableEnumArgument(field, 'deliveryStatus', variables);
      if (inlineDeliveryStatusArgument.value.kind === Kind.VARIABLE && deliveryStatus === null) {
        errors.push(
          buildMissingVariableError(inlineDeliveryStatusArgument.value.name.value, 'AbandonmentDeliveryState!'),
        );
        continue;
      }

      const abandonment = abandonmentId ? store.getAbandonmentById(abandonmentId) : null;
      if (!abandonment || !abandonmentId || !marketingActivityId || !deliveryStatus) {
        data[key] = serializeAbandonmentMutationPayload(field, null, variables, fragments, [
          { field: ['abandonmentId'], message: 'abandonment_not_found' },
        ]);
        continue;
      }

      const updatedAbandonment = store.stageAbandonmentDeliveryActivity(abandonmentId, {
        marketingActivityId,
        deliveryStatus,
        deliveredAt: readNullableStringArgument(field, 'deliveredAt', variables),
        deliveryStatusChangeReason: readNullableStringArgument(field, 'deliveryStatusChangeReason', variables),
      });
      data[key] = serializeAbandonmentMutationPayload(field, updatedAbandonment, variables, fragments, []);
      continue;
    }

    if (field.name.value === 'draftOrderCreate') {
      handled = true;
      const inlineInputArgument = field.arguments?.find((argument) => argument.name.value === 'input') ?? null;

      if (!inlineInputArgument) {
        errors.push(buildDraftOrderCreateMissingInlineInputError());
        continue;
      }

      if (inlineInputArgument.value.kind === Kind.NULL) {
        errors.push(buildDraftOrderCreateNullInlineInputError());
        continue;
      }

      const input = readVariableBackedInputArgument(field, 'input', variables, 'input');

      if (input === null) {
        errors.push(buildDraftOrderCreateMissingInputError());
        continue;
      }

      const userErrors = validateDraftOrderCreateInput(input);
      if (userErrors.length > 0) {
        data[key] = serializeDraftOrderCreatePayloadWithUserErrors(field, userErrors);
        continue;
      }

      const draftOrder = store.stageCreateDraftOrder(buildDraftOrderFromInput(input, shopifyAdminOrigin));
      const payload: Record<string, unknown> = {};
      for (const selection of getSelectedChildFields(field)) {
        const selectionKey = getFieldResponseKey(selection);
        switch (selection.name.value) {
          case 'draftOrder':
            payload[selectionKey] = serializeDraftOrderNode(selection, draftOrder);
            break;
          case 'userErrors':
            payload[selectionKey] = [];
            break;
          default:
            payload[selectionKey] = null;
            break;
        }
      }
      data[key] = payload;
      continue;
    }

    if (field.name.value === 'draftOrderCalculate') {
      handled = true;
      const inlineInputArgument = getInlineArgument(field, 'input');

      if (!inlineInputArgument) {
        errors.push(buildMissingRequiredArgumentError('draftOrderCalculate', 'input'));
        continue;
      }

      if (inlineInputArgument.value.kind === Kind.NULL) {
        errors.push(buildNullArgumentError('draftOrderCalculate', 'input', 'DraftOrderInput!'));
        continue;
      }

      const input = readVariableBackedInputArgument(field, 'input', variables, 'input');
      if (inlineInputArgument.value.kind === Kind.VARIABLE && input === null) {
        errors.push(buildMissingVariableError(inlineInputArgument.value.name.value, 'DraftOrderInput!'));
        continue;
      }

      const userErrors = validateDraftOrderCreateInput(input);
      const calculatedDraftOrder = userErrors.length === 0 ? buildDraftOrderFromInput(input, shopifyAdminOrigin) : null;
      data[key] = serializeDraftOrderCalculatePayload(field, calculatedDraftOrder, userErrors);
      continue;
    }

    if (field.name.value === 'draftOrderUpdate') {
      handled = true;
      const inlineIdArgument = getInlineArgument(field, 'id');
      const inlineInputArgument = getInlineArgument(field, 'input');

      if (!inlineIdArgument) {
        errors.push(buildMissingRequiredArgumentError('draftOrderUpdate', 'id'));
        continue;
      }

      if (inlineIdArgument.value.kind === Kind.NULL) {
        errors.push(buildNullArgumentError('draftOrderUpdate', 'id', 'ID!'));
        continue;
      }

      if (!inlineInputArgument) {
        errors.push(buildMissingRequiredArgumentError('draftOrderUpdate', 'input'));
        continue;
      }

      if (inlineInputArgument.value.kind === Kind.NULL) {
        errors.push(buildNullArgumentError('draftOrderUpdate', 'input', 'DraftOrderInput!'));
        continue;
      }

      const id = readDraftOrderUpdateId(variables);
      if (inlineIdArgument.value.kind === Kind.VARIABLE && id === null) {
        errors.push(buildMissingVariableError(inlineIdArgument.value.name.value, 'ID!'));
        continue;
      }

      const input = readDraftOrderUpdateInput(variables);
      if (inlineInputArgument.value.kind === Kind.VARIABLE && input === null) {
        errors.push(buildMissingVariableError(inlineInputArgument.value.name.value, 'DraftOrderInput!'));
        continue;
      }

      const inputRecord = typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : {};
      const paymentTerms = typeof inputRecord['paymentTerms'] === 'object' ? inputRecord['paymentTerms'] : null;
      if (paymentTerms !== null) {
        const hasTemplateId = typeof (paymentTerms as Record<string, unknown>)['paymentTermsTemplateId'] === 'string';
        data[key] = serializeDraftOrderMutationPayload(field, null, [
          {
            field: null,
            message: hasTemplateId
              ? 'The user must have access to set payment terms.'
              : 'Payment terms template id can not be empty.',
          },
        ]);
        continue;
      }

      const draftOrder = id ? store.getDraftOrderById(id) : null;
      if (!draftOrder) {
        data[key] = serializeDraftOrderMutationPayload(field, null, [
          { field: ['id'], message: 'Draft order does not exist' },
        ]);
        continue;
      }

      const updatedDraftOrder = store.updateDraftOrder(buildUpdatedDraftOrder(draftOrder, input, shopifyAdminOrigin));
      data[key] = serializeDraftOrderMutationPayload(field, updatedDraftOrder, []);
      continue;
    }

    if (field.name.value === 'draftOrderDuplicate') {
      handled = true;
      const id = readDraftOrderDuplicateId(variables);
      const draftOrder = id ? store.getDraftOrderById(id) : null;

      if (!draftOrder) {
        data[key] = serializeDraftOrderMutationPayload(field, null, [
          { field: ['id'], message: 'Draft order does not exist' },
        ]);
        continue;
      }

      const duplicatedDraftOrder = store.stageCreateDraftOrder(duplicateDraftOrder(draftOrder, shopifyAdminOrigin));
      data[key] = serializeDraftOrderMutationPayload(field, duplicatedDraftOrder, []);
      continue;
    }

    if (field.name.value === 'draftOrderBulkAddTags') {
      handled = true;
      const tags = readDraftOrderBulkTags(field, variables);
      const userErrors = tags.length === 0 ? [{ field: ['tags'], message: "Tags can't be blank" }] : [];
      if (userErrors.length === 0) {
        for (const draftOrder of selectDraftOrderBulkTargets(field, variables)) {
          updateDraftOrderTags(draftOrder, tags, 'add');
        }
      }
      data[key] = serializeDraftOrderBulkPayload(
        field,
        userErrors.length === 0 ? makeSyntheticGid('Job') : null,
        userErrors,
      );
      continue;
    }

    if (field.name.value === 'draftOrderBulkRemoveTags') {
      handled = true;
      const tags = readDraftOrderBulkTags(field, variables);
      const userErrors = tags.length === 0 ? [{ field: ['tags'], message: "Tags can't be blank" }] : [];
      if (userErrors.length === 0) {
        for (const draftOrder of selectDraftOrderBulkTargets(field, variables)) {
          updateDraftOrderTags(draftOrder, tags, 'remove');
        }
      }
      data[key] = serializeDraftOrderBulkPayload(
        field,
        userErrors.length === 0 ? makeSyntheticGid('Job') : null,
        userErrors,
      );
      continue;
    }

    if (field.name.value === 'draftOrderBulkDelete') {
      handled = true;
      const targets = selectDraftOrderBulkTargets(field, variables);
      for (const draftOrder of targets) {
        store.deleteDraftOrder(draftOrder.id);
      }
      data[key] = serializeDraftOrderBulkPayload(field, makeSyntheticGid('Job'), []);
      continue;
    }

    if (field.name.value === 'draftOrderDelete') {
      handled = true;
      const inlineInputArgument = getInlineArgument(field, 'input');

      if (!inlineInputArgument) {
        errors.push(buildMissingRequiredArgumentError('draftOrderDelete', 'input'));
        continue;
      }

      if (inlineInputArgument.value.kind === Kind.NULL) {
        errors.push(buildNullArgumentError('draftOrderDelete', 'input', 'DraftOrderDeleteInput!'));
        continue;
      }

      const input = readDraftOrderDeleteInput(variables);
      if (inlineInputArgument.value.kind === Kind.VARIABLE && input === null) {
        errors.push(buildMissingVariableError(inlineInputArgument.value.name.value, 'DraftOrderDeleteInput!'));
        continue;
      }

      const id = typeof input?.['id'] === 'string' ? input['id'] : null;
      const draftOrder = id ? store.getDraftOrderById(id) : null;
      if (!id || !draftOrder) {
        data[key] = serializeDraftOrderDeletePayload(field, null, [
          { field: ['id'], message: 'Draft order does not exist' },
        ]);
        continue;
      }

      store.deleteDraftOrder(id);
      data[key] = serializeDraftOrderDeletePayload(field, id, []);
      continue;
    }

    if (field.name.value === 'draftOrderInvoiceSend') {
      handled = true;
      const inlineIdArgument = getInlineArgument(field, 'id');

      if (!inlineIdArgument) {
        errors.push(buildMissingRequiredArgumentError('draftOrderInvoiceSend', 'id'));
        continue;
      }

      if (inlineIdArgument.value.kind === Kind.NULL) {
        errors.push(buildNullArgumentError('draftOrderInvoiceSend', 'id', 'ID!'));
        continue;
      }

      const id = readDraftOrderInvoiceSendId(variables);
      if (inlineIdArgument.value.kind === Kind.VARIABLE && id === null) {
        errors.push(buildMissingVariableError(inlineIdArgument.value.name.value, 'ID!'));
        continue;
      }

      const draftOrder = id ? store.getDraftOrderById(id) : null;
      data[key] = serializeDraftOrderMutationPayload(
        field,
        draftOrder,
        buildDraftOrderInvoiceSendUserErrors(draftOrder),
      );
      continue;
    }

    if (field.name.value === 'draftOrderInvoicePreview') {
      handled = true;
      const inlineIdArgument = getInlineArgument(field, 'id');

      if (!inlineIdArgument) {
        errors.push(buildMissingRequiredArgumentError('draftOrderInvoicePreview', 'id'));
        continue;
      }

      if (inlineIdArgument.value.kind === Kind.NULL) {
        errors.push(buildNullArgumentError('draftOrderInvoicePreview', 'id', 'ID!'));
        continue;
      }

      const args = getFieldArguments(field, variables);
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      if (inlineIdArgument.value.kind === Kind.VARIABLE && id === null) {
        errors.push(buildMissingVariableError(inlineIdArgument.value.name.value, 'ID!'));
        continue;
      }

      const draftOrder = id ? store.getDraftOrderById(id) : null;
      const userErrors = draftOrder ? [] : [{ field: ['id'], message: 'Draft order does not exist' }];
      data[key] = serializeDraftOrderInvoicePreviewPayload(field, draftOrder, args, userErrors);
      continue;
    }

    if (field.name.value === 'draftOrderCreateFromOrder') {
      handled = true;
      const inlineOrderIdArgument = getInlineArgument(field, 'orderId');

      if (!inlineOrderIdArgument) {
        errors.push(buildMissingRequiredArgumentError('draftOrderCreateFromOrder', 'orderId'));
        continue;
      }

      if (inlineOrderIdArgument.value.kind === Kind.NULL) {
        errors.push(buildNullArgumentError('draftOrderCreateFromOrder', 'orderId', 'ID!'));
        continue;
      }

      const orderId = readDraftOrderCreateFromOrderId(variables);
      if (inlineOrderIdArgument.value.kind === Kind.VARIABLE && orderId === null) {
        errors.push(buildMissingVariableError(inlineOrderIdArgument.value.name.value, 'ID!'));
        continue;
      }

      const order = orderId ? store.getOrderById(orderId) : null;
      if (!order) {
        data[key] = serializeDraftOrderMutationPayload(field, null, [
          { field: ['orderId'], message: 'Order does not exist' },
        ]);
        continue;
      }

      const draftOrder = store.stageCreateDraftOrder(buildDraftOrderFromOrder(order, shopifyAdminOrigin));
      data[key] = serializeDraftOrderMutationPayload(field, draftOrder, []);
      continue;
    }

    if (field.name.value === 'draftOrderComplete') {
      const inlineIdArgument = getDraftOrderCompleteInlineIdArgument(field);

      if (!inlineIdArgument) {
        handled = true;
        errors.push(buildDraftOrderCompleteMissingInlineIdError());
        continue;
      }

      if (inlineIdArgument.value.kind === Kind.NULL) {
        handled = true;
        errors.push(buildDraftOrderCompleteNullInlineIdError());
        continue;
      }

      const id = readDraftOrderCompleteId(variables);

      if (id === null) {
        handled = true;
        errors.push(buildDraftOrderCompleteMissingIdError());
        continue;
      }

      const draftOrder = store.getDraftOrderById(id);
      if (draftOrder && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
        handled = true;

        if (readDraftOrderCompletePaymentGatewayId(field, variables)) {
          data[key] = serializeDraftOrderCompletePayload(field, null, [
            {
              field: null,
              message: 'Invalid payment gateway',
            },
          ]);
          continue;
        }

        const completedDraftOrder = buildCompletedDraftOrder(draftOrder);
        const order = store.stageCreateOrder(
          buildOrderFromCompletedDraftOrder(completedDraftOrder, {
            sourceName: readDraftOrderCompleteSourceName(field, variables),
            paymentPending: readDraftOrderCompletePaymentPending(field, variables),
          }),
        );
        const linkedDraftOrder = store.updateDraftOrder({
          ...completedDraftOrder,
          orderId: order.id,
        });
        const payload = serializeDraftOrderCompletePayload(field, linkedDraftOrder, []);
        data[key] = payload;
        continue;
      }

      if (readMode === 'snapshot') {
        handled = true;
      }

      continue;
    }

    if (field.name.value === 'fulfillmentOrderSubmitFulfillmentRequest') {
      handled = true;
      const idRead = readFulfillmentOrderMutationId(field, field.name.value, variables);
      if (idRead.kind === 'error') {
        errors.push(idRead.error);
        continue;
      }

      const match = findOrderWithFulfillmentOrder(idRead.id);
      if (!match) {
        data[key] = null;
        errors.push(buildInvalidFulfillmentOrderIdError(field.name.value, key, idRead.id));
        continue;
      }

      const result = buildSubmitFulfillmentRequestResult(match.fulfillmentOrder, field, variables);
      if (!result.submittedFulfillmentOrder) {
        data[key] = serializeSubmitFulfillmentRequestPayload(
          field,
          {
            originalFulfillmentOrder: null,
            submittedFulfillmentOrder: null,
            unsubmittedFulfillmentOrder: null,
          },
          result.userErrors,
          variables,
        );
        continue;
      }

      replaceOrderFulfillmentOrder(
        match.order,
        idRead.id,
        result.submittedFulfillmentOrder,
        result.unsubmittedFulfillmentOrder ? [result.unsubmittedFulfillmentOrder] : [],
      );
      data[key] = serializeSubmitFulfillmentRequestPayload(
        field,
        {
          originalFulfillmentOrder: result.submittedFulfillmentOrder,
          submittedFulfillmentOrder: result.submittedFulfillmentOrder,
          unsubmittedFulfillmentOrder: result.unsubmittedFulfillmentOrder,
        },
        [],
        variables,
      );
      continue;
    }

    if (field.name.value === 'fulfillmentOrderAcceptFulfillmentRequest') {
      handled = true;
      const idRead = readFulfillmentOrderMutationId(field, field.name.value, variables);
      if (idRead.kind === 'error') {
        errors.push(idRead.error);
        continue;
      }

      const match = findOrderWithFulfillmentOrder(idRead.id);
      if (!match) {
        data[key] = null;
        errors.push(buildInvalidFulfillmentOrderIdError(field.name.value, key, idRead.id));
        continue;
      }

      if (match.fulfillmentOrder.requestStatus !== 'SUBMITTED') {
        data[key] = serializeFulfillmentOrderPayload(
          field,
          null,
          [{ field: null, message: 'Cannot accept fulfillment request for the fulfillment order.' }],
          variables,
        );
        continue;
      }

      const acceptedFulfillmentOrder: OrderFulfillmentOrderRecord = {
        ...match.fulfillmentOrder,
        status: 'IN_PROGRESS',
        requestStatus: 'ACCEPTED',
      };
      replaceOrderFulfillmentOrder(match.order, idRead.id, acceptedFulfillmentOrder);
      data[key] = serializeFulfillmentOrderPayload(field, acceptedFulfillmentOrder, [], variables);
      continue;
    }

    if (field.name.value === 'fulfillmentOrderRejectFulfillmentRequest') {
      handled = true;
      const idRead = readFulfillmentOrderMutationId(field, field.name.value, variables);
      if (idRead.kind === 'error') {
        errors.push(idRead.error);
        continue;
      }

      const match = findOrderWithFulfillmentOrder(idRead.id);
      if (!match) {
        data[key] = null;
        errors.push(buildInvalidFulfillmentOrderIdError(field.name.value, key, idRead.id));
        continue;
      }

      if (match.fulfillmentOrder.requestStatus !== 'SUBMITTED') {
        data[key] = serializeFulfillmentOrderPayload(
          field,
          null,
          [{ field: null, message: 'Cannot reject fulfillment request for the fulfillment order.' }],
          variables,
        );
        continue;
      }

      const rejectedFulfillmentOrder: OrderFulfillmentOrderRecord = {
        ...match.fulfillmentOrder,
        status: 'OPEN',
        requestStatus: 'REJECTED',
      };
      replaceOrderFulfillmentOrder(match.order, idRead.id, rejectedFulfillmentOrder);
      data[key] = serializeFulfillmentOrderPayload(field, rejectedFulfillmentOrder, [], variables);
      continue;
    }

    if (field.name.value === 'fulfillmentOrderSubmitCancellationRequest') {
      handled = true;
      const idRead = readFulfillmentOrderMutationId(field, field.name.value, variables);
      if (idRead.kind === 'error') {
        errors.push(idRead.error);
        continue;
      }

      const match = findOrderWithFulfillmentOrder(idRead.id);
      if (!match) {
        data[key] = null;
        errors.push(buildInvalidFulfillmentOrderIdError(field.name.value, key, idRead.id));
        continue;
      }

      if (match.fulfillmentOrder.requestStatus !== 'ACCEPTED') {
        data[key] = serializeFulfillmentOrderPayload(
          field,
          null,
          [{ field: null, message: 'Cannot request cancellation for the fulfillment order.' }],
          variables,
        );
        continue;
      }

      const cancellationRequest = makeFulfillmentOrderMerchantRequest(
        'CANCELLATION_REQUEST',
        readNullableStringMutationArgument(field, 'message', variables),
      );
      const cancellationRequestedFulfillmentOrder: OrderFulfillmentOrderRecord = {
        ...match.fulfillmentOrder,
        merchantRequests: [...(match.fulfillmentOrder.merchantRequests ?? []), cancellationRequest],
      };
      replaceOrderFulfillmentOrder(match.order, idRead.id, cancellationRequestedFulfillmentOrder);
      data[key] = serializeFulfillmentOrderPayload(field, cancellationRequestedFulfillmentOrder, [], variables);
      continue;
    }

    if (field.name.value === 'fulfillmentOrderAcceptCancellationRequest') {
      handled = true;
      const idRead = readFulfillmentOrderMutationId(field, field.name.value, variables);
      if (idRead.kind === 'error') {
        errors.push(idRead.error);
        continue;
      }

      const match = findOrderWithFulfillmentOrder(idRead.id);
      if (!match) {
        data[key] = null;
        errors.push(buildInvalidFulfillmentOrderIdError(field.name.value, key, idRead.id));
        continue;
      }

      const hasCancellationRequest = (match.fulfillmentOrder.merchantRequests ?? []).some(
        (merchantRequest) => merchantRequest.kind === 'CANCELLATION_REQUEST',
      );
      if (!hasCancellationRequest || match.fulfillmentOrder.requestStatus !== 'ACCEPTED') {
        data[key] = serializeFulfillmentOrderPayload(
          field,
          null,
          [{ field: null, message: 'Cannot accept cancellation request for the fulfillment order.' }],
          variables,
        );
        continue;
      }

      const cancelledFulfillmentOrder: OrderFulfillmentOrderRecord = {
        ...match.fulfillmentOrder,
        status: 'CLOSED',
        requestStatus: 'CANCELLATION_ACCEPTED',
        lineItems: zeroFulfillmentOrderLineItems(match.fulfillmentOrder.lineItems),
      };
      replaceOrderFulfillmentOrder(match.order, idRead.id, cancelledFulfillmentOrder);
      data[key] = serializeFulfillmentOrderPayload(field, cancelledFulfillmentOrder, [], variables);
      continue;
    }

    if (field.name.value === 'fulfillmentOrderRejectCancellationRequest') {
      handled = true;
      const idRead = readFulfillmentOrderMutationId(field, field.name.value, variables);
      if (idRead.kind === 'error') {
        errors.push(idRead.error);
        continue;
      }

      const match = findOrderWithFulfillmentOrder(idRead.id);
      if (!match) {
        data[key] = null;
        errors.push(buildInvalidFulfillmentOrderIdError(field.name.value, key, idRead.id));
        continue;
      }

      const hasCancellationRequest = (match.fulfillmentOrder.merchantRequests ?? []).some(
        (merchantRequest) => merchantRequest.kind === 'CANCELLATION_REQUEST',
      );
      if (!hasCancellationRequest || match.fulfillmentOrder.requestStatus !== 'ACCEPTED') {
        data[key] = serializeFulfillmentOrderPayload(
          field,
          null,
          [{ field: null, message: 'Cannot reject cancellation request for the fulfillment order.' }],
          variables,
        );
        continue;
      }

      const cancellationRejectedFulfillmentOrder: OrderFulfillmentOrderRecord = {
        ...match.fulfillmentOrder,
        status: 'IN_PROGRESS',
        requestStatus: 'CANCELLATION_REJECTED',
      };
      replaceOrderFulfillmentOrder(match.order, idRead.id, cancellationRejectedFulfillmentOrder);
      data[key] = serializeFulfillmentOrderPayload(field, cancellationRejectedFulfillmentOrder, [], variables);
      continue;
    }

    if (field.name.value === 'fulfillmentEventCreate' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const input = readFulfillmentEventInput(variables);
      const fulfillmentId = input ? readNullableInputString(input, 'fulfillmentId') : null;
      const match = fulfillmentId ? findOrderWithFulfillment(fulfillmentId) : null;

      if (!input || !fulfillmentId || !match) {
        data[key] = serializeFulfillmentEventCreatePayload(field, null, [
          { field: ['fulfillmentEvent', 'fulfillmentId'], message: 'Fulfillment does not exist.' },
        ]);
        continue;
      }

      const event = buildFulfillmentEventFromInput(input);
      const updatedFulfillment = withFulfillmentEventDerivedFields(match.fulfillment, event);
      store.updateOrder({
        ...match.order,
        updatedAt: makeSyntheticTimestamp(),
        fulfillments: (match.order.fulfillments ?? []).map((fulfillment) =>
          fulfillment.id === fulfillmentId ? updatedFulfillment : fulfillment,
        ),
      });
      data[key] = serializeFulfillmentEventCreatePayload(field, event, []);
      continue;
    }

    if (
      field.name.value === 'fulfillmentTrackingInfoUpdate' &&
      (readMode === 'snapshot' || readMode === 'live-hybrid')
    ) {
      handled = true;
      const inlineIdArgument = getFulfillmentTrackingInfoUpdateInlineIdArgument(field);

      if (!inlineIdArgument) {
        errors.push(buildFulfillmentTrackingInfoUpdateMissingInlineIdError());
        continue;
      }

      if (inlineIdArgument.value.kind === Kind.NULL) {
        errors.push(buildFulfillmentTrackingInfoUpdateNullInlineIdError());
        continue;
      }

      const fulfillmentId = readFulfillmentTrackingInfoUpdateId(variables);
      if (inlineIdArgument.value.kind === Kind.VARIABLE && fulfillmentId === null) {
        errors.push(buildFulfillmentTrackingInfoUpdateMissingIdError());
        continue;
      }

      if (fulfillmentId) {
        const trackingInfo = readFulfillmentTrackingInfoInput(variables);
        const match = findOrderWithFulfillment(fulfillmentId);
        if (!match || !trackingInfo) {
          data[key] = serializeFulfillmentMutationPayload(
            field,
            null,
            [{ field: ['fulfillmentId'], message: 'Fulfillment does not exist.' }],
            variables,
          );
          continue;
        }

        const updatedFulfillment: OrderFulfillmentRecord = {
          ...match.fulfillment,
          updatedAt: makeSyntheticTimestamp(),
          trackingInfo: [trackingInfo],
        };
        store.updateOrder({
          ...match.order,
          updatedAt: makeSyntheticTimestamp(),
          fulfillments: (match.order.fulfillments ?? []).map((fulfillment) =>
            fulfillment.id === fulfillmentId ? updatedFulfillment : fulfillment,
          ),
        });
        data[key] = serializeFulfillmentMutationPayload(field, updatedFulfillment, [], variables);
      }
      continue;
    }

    if (field.name.value === 'fulfillmentOrderHold' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const fulfillmentOrderId = readFulfillmentOrderId(variables);
      const match = fulfillmentOrderId ? findOrderWithFulfillmentOrder(fulfillmentOrderId) : null;
      if (!match) {
        errors.push({
          message: `Invalid id: ${fulfillmentOrderId ?? ''}`,
          extensions: { code: 'RESOURCE_NOT_FOUND' },
          path: [key],
        });
        continue;
      }

      const result = applyFulfillmentOrderHold(match.order, match.fulfillmentOrder, variables);
      data[key] = serializeFulfillmentOrderMutationPayload(
        field,
        {
          fulfillmentOrder: result.fulfillmentOrder,
          remainingFulfillmentOrder: result.remainingFulfillmentOrder,
        },
        [],
      );
      continue;
    }

    if (field.name.value === 'fulfillmentOrderReleaseHold' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const fulfillmentOrderId = readFulfillmentOrderId(variables);
      const match = fulfillmentOrderId ? findOrderWithFulfillmentOrder(fulfillmentOrderId) : null;
      if (!match) {
        errors.push({
          message: `Invalid id: ${fulfillmentOrderId ?? ''}`,
          extensions: { code: 'RESOURCE_NOT_FOUND' },
          path: [key],
        });
        continue;
      }

      const result = applyFulfillmentOrderReleaseHold(match.order, match.fulfillmentOrder);
      data[key] = serializeFulfillmentOrderMutationPayload(field, { fulfillmentOrder: result.fulfillmentOrder }, []);
      continue;
    }

    if (field.name.value === 'fulfillmentOrderMove' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const fulfillmentOrderId = readFulfillmentOrderId(variables);
      const match = fulfillmentOrderId ? findOrderWithFulfillmentOrder(fulfillmentOrderId) : null;
      if (!match) {
        errors.push({
          message: `Invalid id: ${fulfillmentOrderId ?? ''}`,
          extensions: { code: 'RESOURCE_NOT_FOUND' },
          path: [key],
        });
        continue;
      }

      const result = applyFulfillmentOrderMove(match.order, match.fulfillmentOrder, variables);
      data[key] = serializeFulfillmentOrderMutationPayload(
        field,
        {
          movedFulfillmentOrder: result.movedFulfillmentOrder,
          originalFulfillmentOrder: result.originalFulfillmentOrder,
          remainingFulfillmentOrder: result.remainingFulfillmentOrder,
        },
        [],
      );
      continue;
    }

    if (
      field.name.value === 'fulfillmentOrderReportProgress' &&
      (readMode === 'snapshot' || readMode === 'live-hybrid')
    ) {
      handled = true;
      const fulfillmentOrderId = readFulfillmentOrderId(variables);
      const match = fulfillmentOrderId ? findOrderWithFulfillmentOrder(fulfillmentOrderId) : null;
      if (!match) {
        errors.push({
          message: `Invalid id: ${fulfillmentOrderId ?? ''}`,
          extensions: { code: 'RESOURCE_NOT_FOUND' },
          path: [key],
        });
        continue;
      }

      const result = applyFulfillmentOrderStatus(match.order, match.fulfillmentOrder, 'IN_PROGRESS');
      data[key] = serializeFulfillmentOrderMutationPayload(field, { fulfillmentOrder: result.fulfillmentOrder }, []);
      continue;
    }

    if (field.name.value === 'fulfillmentOrderOpen' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const fulfillmentOrderId = readFulfillmentOrderId(variables);
      const match = fulfillmentOrderId ? findOrderWithFulfillmentOrder(fulfillmentOrderId) : null;
      if (!match) {
        errors.push({
          message: `Invalid id: ${fulfillmentOrderId ?? ''}`,
          extensions: { code: 'RESOURCE_NOT_FOUND' },
          path: [key],
        });
        continue;
      }

      const result = applyFulfillmentOrderStatus(match.order, match.fulfillmentOrder, 'OPEN');
      data[key] = serializeFulfillmentOrderMutationPayload(field, { fulfillmentOrder: result.fulfillmentOrder }, []);
      continue;
    }

    if (field.name.value === 'fulfillmentOrderCancel' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const fulfillmentOrderId = readFulfillmentOrderId(variables);
      const match = fulfillmentOrderId ? findOrderWithFulfillmentOrder(fulfillmentOrderId) : null;
      if (!match) {
        errors.push({
          message: `Invalid id: ${fulfillmentOrderId ?? ''}`,
          extensions: { code: 'RESOURCE_NOT_FOUND' },
          path: [key],
        });
        continue;
      }

      const result = applyFulfillmentOrderCancel(match.order, match.fulfillmentOrder);
      data[key] = serializeFulfillmentOrderMutationPayload(
        field,
        {
          fulfillmentOrder: result.fulfillmentOrder,
          replacementFulfillmentOrder: result.replacementFulfillmentOrder,
        },
        [],
      );
      continue;
    }

    if (field.name.value === 'fulfillmentOrderSplit' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const splitInputs = readFulfillmentOrderSplitInputs(variables);
      const splitResults: FulfillmentOrderSplitResult[] = [];
      let invalidId = false;

      for (const splitInput of splitInputs) {
        const match = findOrderWithFulfillmentOrder(splitInput.fulfillmentOrderId);
        if (!match) {
          invalidId = true;
          break;
        }
        const result = applyFulfillmentOrderSplit(
          match.order,
          match.fulfillmentOrder,
          splitInput.fulfillmentOrderLineItems,
        );
        splitResults.push(result.result);
      }

      if (invalidId || splitInputs.length === 0) {
        data[key] = null;
        errors.push(buildFulfillmentOrderInvalidIdError(field.name.value, key));
        continue;
      }

      data[key] = serializeFulfillmentOrderSplitPayload(field, splitResults, [], variables);
      continue;
    }

    if (field.name.value === 'fulfillmentOrderMerge' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const mergeInputs = readFulfillmentOrderMergeInputs(variables);
      const mergeResults: FulfillmentOrderMergeResult[] = [];
      let invalidId = false;

      for (const mergeInput of mergeInputs) {
        const matches = mergeInput.mergeIntents.map((intent) =>
          findOrderWithFulfillmentOrder(intent.fulfillmentOrderId),
        );
        const firstMatch = matches[0];
        if (!firstMatch || matches.some((match) => !match || match.order.id !== firstMatch.order.id)) {
          invalidId = true;
          break;
        }
        const result = applyFulfillmentOrderMerge(
          firstMatch.order,
          matches
            .filter(
              (
                match,
              ): match is {
                order: OrderRecord;
                fulfillmentOrder: OrderFulfillmentOrderRecord;
              } => match !== null,
            )
            .map((match) => match.fulfillmentOrder),
        );
        mergeResults.push(result.result);
      }

      if (invalidId || mergeInputs.length === 0) {
        data[key] = null;
        errors.push(buildFulfillmentOrderInvalidIdError(field.name.value, key));
        continue;
      }

      data[key] = serializeFulfillmentOrderMergePayload(field, mergeResults, [], variables);
      continue;
    }

    if (
      field.name.value === 'fulfillmentOrdersSetFulfillmentDeadline' &&
      (readMode === 'snapshot' || readMode === 'live-hybrid')
    ) {
      handled = true;
      const deadlineInput = readFulfillmentOrdersSetDeadlineInput(variables);
      const matches = deadlineInput.fulfillmentOrderIds.map((id) => findOrderWithFulfillmentOrder(id));
      if (!deadlineInput.fulfillmentDeadline || matches.length === 0 || matches.some((match) => match === null)) {
        data[key] = null;
        errors.push(buildFulfillmentOrderInvalidIdError(field.name.value, key));
        continue;
      }

      for (const match of matches) {
        const currentMatch = match ? findOrderWithFulfillmentOrder(match.fulfillmentOrder.id) : null;
        if (!currentMatch) {
          continue;
        }
        const updatedFulfillmentOrder: OrderFulfillmentOrderRecord = {
          ...currentMatch.fulfillmentOrder,
          fulfillBy: deadlineInput.fulfillmentDeadline,
          updatedAt: makeSyntheticTimestamp(),
        };
        replaceOrderFulfillmentOrder(currentMatch.order, currentMatch.fulfillmentOrder.id, updatedFulfillmentOrder);
      }
      data[key] = serializeFulfillmentOrdersSetDeadlinePayload(field, true, []);
      continue;
    }

    if (field.name.value === 'fulfillmentOrderReschedule' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      data[key] = serializeFulfillmentOrderMutationPayload(field, { fulfillmentOrder: null }, [
        { field: null, message: 'Fulfillment order must be scheduled.' },
      ]);
      continue;
    }

    if (field.name.value === 'fulfillmentOrderClose' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      data[key] = serializeFulfillmentOrderMutationPayload(field, { fulfillmentOrder: null }, [
        { field: null, message: "The fulfillment order's assigned fulfillment service must be of api type" },
      ]);
      continue;
    }

    if (field.name.value === 'fulfillmentOrdersReroute' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      errors.push({
        message: 'Internal error. Looks like something went wrong on our end.',
        extensions: {
          code: 'INTERNAL_SERVER_ERROR',
        },
      });
      continue;
    }

    if (field.name.value === 'fulfillmentCancel' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const inlineIdArgument = getFulfillmentCancelInlineIdArgument(field);

      if (!inlineIdArgument) {
        errors.push(buildFulfillmentCancelMissingInlineIdError());
        continue;
      }

      if (inlineIdArgument.value.kind === Kind.NULL) {
        errors.push(buildFulfillmentCancelNullInlineIdError());
        continue;
      }

      const fulfillmentId = readFulfillmentCancelId(variables);
      if (inlineIdArgument.value.kind === Kind.VARIABLE && fulfillmentId === null) {
        errors.push(buildFulfillmentCancelMissingIdError());
        continue;
      }

      if (fulfillmentId) {
        const match = findOrderWithFulfillment(fulfillmentId);
        if (!match) {
          data[key] = serializeFulfillmentMutationPayload(
            field,
            null,
            [{ field: ['id'], message: 'Fulfillment not found.' }],
            variables,
          );
          continue;
        }

        const cancelledFulfillment: OrderFulfillmentRecord = {
          ...match.fulfillment,
          status: 'CANCELLED',
          displayStatus: 'CANCELED',
          updatedAt: makeSyntheticTimestamp(),
        };
        store.updateOrder({
          ...match.order,
          updatedAt: makeSyntheticTimestamp(),
          fulfillments: (match.order.fulfillments ?? []).map((fulfillment) =>
            fulfillment.id === fulfillmentId ? cancelledFulfillment : fulfillment,
          ),
        });
        data[key] = serializeFulfillmentMutationPayload(field, cancelledFulfillment, [], variables);
      }
      continue;
    }

    if (field.name.value === 'orderEditBegin' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      const inlineIdArgument = getOrderEditInlineIdArgument(field);
      const id = readOrderEditId(variables);

      if (inlineIdArgument?.value.kind === Kind.VARIABLE && id === null) {
        handled = true;
        errors.push(buildOrderEditBeginMissingIdError());
        continue;
      }

      const order = id ? store.getOrderById(id) : null;
      if (!order) {
        continue;
      }

      handled = true;
      const calculatedOrder = store.stageCalculatedOrder(buildCalculatedOrderFromOrder(order));
      const payload: Record<string, unknown> = {};
      for (const selection of getSelectedChildFields(field)) {
        const selectionKey = getFieldResponseKey(selection);
        switch (selection.name.value) {
          case 'calculatedOrder':
            payload[selectionKey] = serializeCalculatedOrder(selection, calculatedOrder);
            break;
          case 'orderEditSession':
            payload[selectionKey] = serializeOrderEditSession(selection, calculatedOrder);
            break;
          case 'userErrors':
            payload[selectionKey] = [];
            break;
          default:
            payload[selectionKey] = null;
            break;
        }
      }
      data[key] = payload;
      continue;
    }

    if (field.name.value === 'orderEditAddVariant' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      const inlineIdArgument = getOrderEditInlineIdArgument(field);
      const calculatedOrderId = readOrderEditId(variables);
      const variantId = readOrderEditVariantId(variables);
      const quantity = readOrderEditQuantity(variables);

      if (inlineIdArgument?.value.kind === Kind.VARIABLE && calculatedOrderId === null) {
        handled = true;
        errors.push(buildOrderEditAddVariantMissingIdError());
        continue;
      }

      const calculatedOrder = calculatedOrderId ? store.getCalculatedOrderById(calculatedOrderId) : null;
      if (!calculatedOrder || !variantId || quantity === null) {
        continue;
      }

      const calculatedLineItem = buildCalculatedLineItemFromVariant(variantId, quantity);
      if (!calculatedLineItem) {
        handled = true;
        const userErrors = buildOrderEditInvalidVariantUserErrors();
        const payload: Record<string, unknown> = {};
        for (const selection of getSelectedChildFields(field)) {
          const selectionKey = getFieldResponseKey(selection);
          switch (selection.name.value) {
            case 'calculatedOrder':
            case 'calculatedLineItem':
            case 'orderEditSession':
              payload[selectionKey] = null;
              break;
            case 'userErrors':
              payload[selectionKey] = userErrors;
              break;
            default:
              payload[selectionKey] = null;
              break;
          }
        }
        data[key] = payload;
        continue;
      }

      handled = true;
      const updatedCalculatedOrder = store.updateCalculatedOrder(
        recalculateCalculatedOrder({
          ...calculatedOrder,
          lineItems: [...calculatedOrder.lineItems, calculatedLineItem],
        }),
      );
      const payload: Record<string, unknown> = {};
      for (const selection of getSelectedChildFields(field)) {
        const selectionKey = getFieldResponseKey(selection);
        switch (selection.name.value) {
          case 'calculatedOrder':
            payload[selectionKey] = serializeCalculatedOrder(selection, updatedCalculatedOrder);
            break;
          case 'calculatedLineItem':
            payload[selectionKey] = serializeOrderLineItemNode(selection, calculatedLineItem);
            break;
          case 'orderEditSession':
            payload[selectionKey] = serializeOrderEditSession(selection, updatedCalculatedOrder);
            break;
          case 'userErrors':
            payload[selectionKey] = [];
            break;
          default:
            payload[selectionKey] = null;
            break;
        }
      }
      data[key] = payload;
      continue;
    }

    if (field.name.value === 'orderEditAddCustomItem' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      const args = getFieldArguments(field, variables);
      const calculatedOrderId = typeof args['id'] === 'string' ? args['id'] : null;
      const calculatedOrder = calculatedOrderId ? store.getCalculatedOrderById(calculatedOrderId) : null;
      if (!calculatedOrder) {
        continue;
      }

      handled = true;
      const calculatedLineItem = buildCalculatedCustomLineItem(args);
      const updatedCalculatedOrder = calculatedLineItem
        ? store.updateCalculatedOrder(
            recalculateCalculatedOrder({
              ...calculatedOrder,
              lineItems: [...calculatedOrder.lineItems, calculatedLineItem],
            }),
          )
        : calculatedOrder;
      const userErrors = calculatedLineItem ? [] : [{ field: ['price'], message: 'Price must be present' }];
      const payload: Record<string, unknown> = {};
      for (const selection of getSelectedChildFields(field)) {
        const selectionKey = getFieldResponseKey(selection);
        switch (selection.name.value) {
          case 'calculatedOrder':
            payload[selectionKey] = calculatedLineItem
              ? serializeCalculatedOrder(selection, updatedCalculatedOrder)
              : null;
            break;
          case 'calculatedLineItem':
            payload[selectionKey] = calculatedLineItem
              ? serializeOrderLineItemNode(selection, calculatedLineItem)
              : null;
            break;
          case 'userErrors':
            payload[selectionKey] = userErrors;
            break;
          default:
            payload[selectionKey] = null;
            break;
        }
      }
      data[key] = payload;
      continue;
    }

    if (
      field.name.value === 'orderEditAddLineItemDiscount' &&
      (readMode === 'snapshot' || readMode === 'live-hybrid')
    ) {
      const args = getFieldArguments(field, variables);
      const calculatedOrderId = typeof args['id'] === 'string' ? args['id'] : null;
      const lineItemId = typeof args['lineItemId'] === 'string' ? args['lineItemId'] : null;
      const discountInput =
        typeof args['discount'] === 'object' && args['discount'] !== null
          ? (args['discount'] as Record<string, unknown>)
          : null;
      const calculatedOrder = calculatedOrderId ? store.getCalculatedOrderById(calculatedOrderId) : null;
      if (!calculatedOrder || !lineItemId || !discountInput) {
        continue;
      }

      handled = true;
      const result = applyLineItemDiscount(calculatedOrder, lineItemId, discountInput);
      const updatedCalculatedOrder = result ? store.updateCalculatedOrder(result.calculatedOrder) : calculatedOrder;
      const payload: Record<string, unknown> = {};
      for (const selection of getSelectedChildFields(field)) {
        const selectionKey = getFieldResponseKey(selection);
        switch (selection.name.value) {
          case 'calculatedOrder':
            payload[selectionKey] = result ? serializeCalculatedOrder(selection, updatedCalculatedOrder) : null;
            break;
          case 'calculatedLineItem':
            payload[selectionKey] = result ? serializeOrderLineItemNode(selection, result.calculatedLineItem) : null;
            break;
          case 'addedDiscountStagedChange':
            payload[selectionKey] = null;
            break;
          case 'userErrors':
            payload[selectionKey] = result ? [] : [{ field: ['lineItemId'], message: 'Line item does not exist' }];
            break;
          default:
            payload[selectionKey] = null;
            break;
        }
      }
      data[key] = payload;
      continue;
    }

    if (field.name.value === 'orderEditRemoveDiscount' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      const args = getFieldArguments(field, variables);
      const calculatedOrderId = typeof args['id'] === 'string' ? args['id'] : null;
      const discountApplicationId =
        typeof args['discountApplicationId'] === 'string' ? args['discountApplicationId'] : null;
      const calculatedOrder = calculatedOrderId ? store.getCalculatedOrderById(calculatedOrderId) : null;
      if (!calculatedOrder || !discountApplicationId) {
        continue;
      }

      handled = true;
      const result = removeCalculatedDiscount(calculatedOrder, discountApplicationId);
      const updatedCalculatedOrder = result ? store.updateCalculatedOrder(result) : calculatedOrder;
      const payload: Record<string, unknown> = {};
      for (const selection of getSelectedChildFields(field)) {
        const selectionKey = getFieldResponseKey(selection);
        switch (selection.name.value) {
          case 'calculatedOrder':
            payload[selectionKey] = serializeCalculatedOrder(selection, updatedCalculatedOrder);
            break;
          case 'userErrors':
            payload[selectionKey] = result
              ? []
              : [{ field: ['discountApplicationId'], message: 'Discount does not exist' }];
            break;
          default:
            payload[selectionKey] = null;
            break;
        }
      }
      data[key] = payload;
      continue;
    }

    if (field.name.value === 'orderEditAddShippingLine' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      const args = getFieldArguments(field, variables);
      const calculatedOrderId = typeof args['id'] === 'string' ? args['id'] : null;
      const calculatedOrder = calculatedOrderId ? store.getCalculatedOrderById(calculatedOrderId) : null;
      if (!calculatedOrder) {
        continue;
      }

      handled = true;
      const shippingLine = buildCalculatedShippingLine(args);
      const updatedCalculatedOrder = shippingLine
        ? store.updateCalculatedOrder(
            recalculateCalculatedOrder({
              ...calculatedOrder,
              shippingLines: [...calculatedOrder.shippingLines, shippingLine],
            }),
          )
        : calculatedOrder;
      const payload: Record<string, unknown> = {};
      for (const selection of getSelectedChildFields(field)) {
        const selectionKey = getFieldResponseKey(selection);
        switch (selection.name.value) {
          case 'calculatedOrder':
            payload[selectionKey] = shippingLine ? serializeCalculatedOrder(selection, updatedCalculatedOrder) : null;
            break;
          case 'calculatedShippingLine':
            payload[selectionKey] = shippingLine
              ? serializeCalculatedShippingLinePayload(selection, shippingLine)
              : null;
            break;
          case 'userErrors':
            payload[selectionKey] = shippingLine
              ? []
              : [{ field: ['shippingLine'], message: 'Shipping line is invalid' }];
            break;
          default:
            payload[selectionKey] = null;
            break;
        }
      }
      data[key] = payload;
      continue;
    }

    if (field.name.value === 'orderEditRemoveShippingLine' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      const args = getFieldArguments(field, variables);
      const calculatedOrderId = typeof args['id'] === 'string' ? args['id'] : null;
      const shippingLineId = typeof args['shippingLineId'] === 'string' ? args['shippingLineId'] : null;
      const calculatedOrder = calculatedOrderId ? store.getCalculatedOrderById(calculatedOrderId) : null;
      if (!calculatedOrder || !shippingLineId) {
        continue;
      }

      handled = true;
      const hadShippingLine = calculatedOrder.shippingLines.some((shippingLine) => shippingLine.id === shippingLineId);
      const updatedCalculatedOrder = hadShippingLine
        ? store.updateCalculatedOrder(
            recalculateCalculatedOrder({
              ...calculatedOrder,
              shippingLines: calculatedOrder.shippingLines.filter((shippingLine) => shippingLine.id !== shippingLineId),
            }),
          )
        : calculatedOrder;
      const payload: Record<string, unknown> = {};
      for (const selection of getSelectedChildFields(field)) {
        const selectionKey = getFieldResponseKey(selection);
        switch (selection.name.value) {
          case 'calculatedOrder':
            payload[selectionKey] = serializeCalculatedOrder(selection, updatedCalculatedOrder);
            break;
          case 'userErrors':
            payload[selectionKey] = hadShippingLine
              ? []
              : [{ field: ['shippingLineId'], message: 'Shipping line does not exist' }];
            break;
          default:
            payload[selectionKey] = null;
            break;
        }
      }
      data[key] = payload;
      continue;
    }

    if (field.name.value === 'orderEditUpdateShippingLine' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      const args = getFieldArguments(field, variables);
      const calculatedOrderId = typeof args['id'] === 'string' ? args['id'] : null;
      const shippingLineId = typeof args['shippingLineId'] === 'string' ? args['shippingLineId'] : null;
      const shippingLineInput =
        typeof args['shippingLine'] === 'object' && args['shippingLine'] !== null
          ? (args['shippingLine'] as Record<string, unknown>)
          : {};
      const calculatedOrder = calculatedOrderId ? store.getCalculatedOrderById(calculatedOrderId) : null;
      if (!calculatedOrder || !shippingLineId) {
        continue;
      }

      handled = true;
      const price = readMoneyInputAmount(shippingLineInput['price']);
      const hadShippingLine = calculatedOrder.shippingLines.some((shippingLine) => shippingLine.id === shippingLineId);
      const updatedCalculatedOrder = hadShippingLine
        ? store.updateCalculatedOrder(
            recalculateCalculatedOrder({
              ...calculatedOrder,
              shippingLines: calculatedOrder.shippingLines.map((shippingLine) =>
                shippingLine.id === shippingLineId
                  ? {
                      ...shippingLine,
                      title:
                        typeof shippingLineInput['title'] === 'string'
                          ? shippingLineInput['title']
                          : shippingLine.title,
                      code:
                        typeof shippingLineInput['title'] === 'string' ? shippingLineInput['title'] : shippingLine.code,
                      originalPriceSet: price
                        ? makeOrderMoneyBag(price.amount, price.currencyCode)
                        : shippingLine.originalPriceSet,
                      stagedStatus: shippingLine.stagedStatus === 'ADDED' ? 'ADDED' : 'UPDATED',
                    }
                  : shippingLine,
              ),
            }),
          )
        : calculatedOrder;
      const payload: Record<string, unknown> = {};
      for (const selection of getSelectedChildFields(field)) {
        const selectionKey = getFieldResponseKey(selection);
        switch (selection.name.value) {
          case 'calculatedOrder':
            payload[selectionKey] = serializeCalculatedOrder(selection, updatedCalculatedOrder);
            break;
          case 'userErrors':
            payload[selectionKey] = hadShippingLine
              ? []
              : [{ field: ['shippingLineId'], message: 'Shipping line does not exist' }];
            break;
          default:
            payload[selectionKey] = null;
            break;
        }
      }
      data[key] = payload;
      continue;
    }

    if (field.name.value === 'orderEditSetQuantity' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      const inlineIdArgument = getOrderEditInlineIdArgument(field);
      const calculatedOrderId = readOrderEditId(variables);
      const lineItemId = readOrderEditLineItemId(variables);
      const quantity = readOrderEditQuantity(variables);

      if (inlineIdArgument?.value.kind === Kind.VARIABLE && calculatedOrderId === null) {
        handled = true;
        errors.push(buildOrderEditSetQuantityMissingIdError());
        continue;
      }

      const calculatedOrder = calculatedOrderId ? store.getCalculatedOrderById(calculatedOrderId) : null;
      if (!calculatedOrder || !lineItemId || quantity === null) {
        continue;
      }

      const targetLineItem = calculatedOrder.lineItems.find((lineItem) => lineItem.id === lineItemId) ?? null;
      if (!targetLineItem) {
        continue;
      }

      handled = true;
      const updatedCalculatedOrder = store.updateCalculatedOrder(
        recalculateCalculatedOrder({
          ...calculatedOrder,
          lineItems: calculatedOrder.lineItems.map((lineItem) =>
            lineItem.id === lineItemId ? { ...lineItem, quantity, currentQuantity: quantity } : lineItem,
          ),
        }),
      );
      const updatedCalculatedLineItem =
        updatedCalculatedOrder.lineItems.find((lineItem) => lineItem.id === lineItemId) ?? targetLineItem;
      const payload: Record<string, unknown> = {};
      for (const selection of getSelectedChildFields(field)) {
        const selectionKey = getFieldResponseKey(selection);
        switch (selection.name.value) {
          case 'calculatedOrder':
            payload[selectionKey] = serializeCalculatedOrder(selection, updatedCalculatedOrder);
            break;
          case 'calculatedLineItem':
            payload[selectionKey] = serializeOrderLineItemNode(selection, updatedCalculatedLineItem);
            break;
          case 'orderEditSession':
            payload[selectionKey] = serializeOrderEditSession(selection, updatedCalculatedOrder);
            break;
          case 'userErrors':
            payload[selectionKey] = [];
            break;
          default:
            payload[selectionKey] = null;
            break;
        }
      }
      data[key] = payload;
      continue;
    }

    if (field.name.value === 'orderEditCommit' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      const inlineIdArgument = getOrderEditInlineIdArgument(field);
      const calculatedOrderId = readOrderEditId(variables);

      if (inlineIdArgument?.value.kind === Kind.VARIABLE && calculatedOrderId === null) {
        handled = true;
        errors.push(buildOrderEditCommitMissingIdError());
        continue;
      }

      const calculatedOrder = calculatedOrderId ? store.getCalculatedOrderById(calculatedOrderId) : null;
      if (!calculatedOrder) {
        continue;
      }

      handled = true;
      const originalOrder = store.getOrderById(calculatedOrder.originalOrderId)!;
      const committedOrder = store.updateOrder(
        recalculateOrderTotals({
          ...originalOrder,
          updatedAt: makeSyntheticTimestamp(),
          lineItems: buildCommittedOrderLineItems(originalOrder, calculatedOrder),
          shippingLines: structuredClone(calculatedOrder.shippingLines),
        }),
      );
      store.discardCalculatedOrder(calculatedOrder.id);

      const payload: Record<string, unknown> = {};
      for (const selection of getSelectedChildFields(field)) {
        const selectionKey = getFieldResponseKey(selection);
        switch (selection.name.value) {
          case 'order':
            payload[selectionKey] = serializeOrderNode(selection, committedOrder);
            break;
          case 'userErrors':
            payload[selectionKey] = [];
            break;
          case 'successMessages':
            payload[selectionKey] = ['Order updated'];
            break;
          default:
            payload[selectionKey] = null;
            break;
        }
      }
      data[key] = payload;
      continue;
    }

    if (field.name.value === 'fulfillmentCreate' && (readMode === 'snapshot' || readMode === 'live-hybrid')) {
      handled = true;
      const fulfillment = readFulfillmentCreateInput(variables);
      const lineItemsByFulfillmentOrder = fulfillment['lineItemsByFulfillmentOrder'];
      const fulfillmentOrderRequest = Array.isArray(lineItemsByFulfillmentOrder)
        ? (lineItemsByFulfillmentOrder[0] as Record<string, unknown> | undefined)
        : undefined;
      const fulfillmentOrderId = fulfillmentOrderRequest?.['fulfillmentOrderId'];

      data[key] = null;

      if (typeof fulfillmentOrderId === 'string' && fulfillmentOrderId.length > 0) {
        const match = findOrderWithFulfillmentOrder(fulfillmentOrderId);
        if (match) {
          const createdAt = makeSyntheticTimestamp();
          const rawRequestedLineItems = fulfillmentOrderRequest?.['fulfillmentOrderLineItems'];
          const requestedLineItems = Array.isArray(rawRequestedLineItems)
            ? rawRequestedLineItems.filter(
                (lineItem): lineItem is Record<string, unknown> => typeof lineItem === 'object' && lineItem !== null,
              )
            : [];
          const requestedById = new Map(
            requestedLineItems
              .map((lineItem) => [readNullableInputString(lineItem, 'id'), lineItem] as const)
              .filter((entry): entry is readonly [string, Record<string, unknown>] => entry[0] !== null),
          );
          const fulfillmentLineItems = (match.fulfillmentOrder.lineItems ?? [])
            .filter((lineItem) => requestedById.size === 0 || requestedById.has(lineItem.id))
            .map((lineItem) => {
              const requestedLineItem = requestedById.get(lineItem.id);
              const requestedQuantity =
                typeof requestedLineItem?.['quantity'] === 'number' ? requestedLineItem['quantity'] : null;
              return {
                id: makeSyntheticGid('FulfillmentLineItem'),
                lineItemId: lineItem.lineItemId,
                title: lineItem.title,
                quantity: requestedQuantity ?? lineItem.remainingQuantity ?? lineItem.totalQuantity,
              };
            });
          const trackingInfo = readFulfillmentCreateTrackingInfoInput(fulfillment);
          const createdFulfillment: OrderFulfillmentRecord = {
            id: makeSyntheticGid('Fulfillment'),
            status: 'SUCCESS',
            displayStatus: 'FULFILLED',
            createdAt,
            updatedAt: createdAt,
            deliveredAt: null,
            estimatedDeliveryAt: null,
            inTransitAt: null,
            trackingInfo: trackingInfo ? [trackingInfo] : [],
            events: [],
            fulfillmentLineItems,
            service: null,
            location: match.fulfillmentOrder.assignedLocation
              ? {
                  name: match.fulfillmentOrder.assignedLocation.name,
                }
              : null,
            originAddress: null,
          };
          const updatedFulfillmentOrder: OrderFulfillmentOrderRecord = {
            ...match.fulfillmentOrder,
            status: 'CLOSED',
            lineItems: (match.fulfillmentOrder.lineItems ?? []).map((lineItem) => {
              const fulfilledLineItem = fulfillmentLineItems.find(
                (candidate) => candidate.lineItemId === lineItem.lineItemId,
              );
              return fulfilledLineItem
                ? {
                    ...lineItem,
                    remainingQuantity: Math.max(0, lineItem.remainingQuantity - fulfilledLineItem.quantity),
                  }
                : lineItem;
            }),
          };
          const updatedFulfillmentOrders = (match.order.fulfillmentOrders ?? []).map((candidate) =>
            candidate.id === fulfillmentOrderId ? updatedFulfillmentOrder : candidate,
          );
          const hasOpenFulfillmentOrder = updatedFulfillmentOrders.some(
            (candidate) =>
              candidate.status !== 'CLOSED' &&
              (candidate.lineItems ?? []).some((lineItem) => lineItem.remainingQuantity > 0),
          );
          store.updateOrder({
            ...match.order,
            updatedAt: makeSyntheticTimestamp(),
            displayFulfillmentStatus: hasOpenFulfillmentOrder ? 'PARTIALLY_FULFILLED' : 'FULFILLED',
            fulfillments: [createdFulfillment, ...(match.order.fulfillments ?? [])],
            fulfillmentOrders: updatedFulfillmentOrders,
          });
          data[key] = serializeFulfillmentMutationPayload(field, createdFulfillment, [], variables);
          continue;
        }

        errors.push({
          message: 'invalid id',
          extensions: {
            code: 'RESOURCE_NOT_FOUND',
          },
          path: [key],
        });
      }
    }
  }

  if (!handled) {
    return null;
  }

  if (errors.length > 0) {
    return Object.keys(data).length > 0 ? { data, errors } : { errors };
  }

  return { data };
}
