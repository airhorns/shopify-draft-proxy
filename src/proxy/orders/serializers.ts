import { Kind, type FieldNode } from 'graphql';

import { getFieldArguments } from '../../graphql/root-field.js';
import {
  applySearchQueryTerms,
  matchesSearchQueryString,
  parseSearchQueryTermList,
  searchQueryTermValue,
  stripSearchQueryValueQuotes,
  type SearchQueryTerm,
} from '../../search-query-parser.js';
import { store } from '../../state/store.js';
import { makeSyntheticTimestamp } from '../../state/synthetic-identity.js';
import type {
  AbandonedCheckoutRecord,
  AbandonmentRecord,
  CalculatedOrderRecord,
  DraftOrderAddressRecord,
  DraftOrderAppliedDiscountRecord,
  DraftOrderAttributeRecord,
  DraftOrderCustomerRecord,
  DraftOrderLineItemRecord,
  DraftOrderPaymentTermsRecord,
  DraftOrderRecord,
  DraftOrderShippingLineRecord,
  MoneyV2Record,
  PaymentScheduleRecord,
  OrderCustomerRecord,
  OrderDiscountApplicationRecord,
  OrderFulfillmentEventRecord,
  OrderFulfillmentLineItemRecord,
  OrderFulfillmentLocationRecord,
  OrderFulfillmentOrderLineItemRecord,
  OrderFulfillmentOrderRecord,
  OrderFulfillmentOriginAddressRecord,
  OrderFulfillmentRecord,
  OrderFulfillmentServiceRecord,
  OrderLineItemRecord,
  OrderMandatePaymentRecord,
  OrderRecord,
  OrderRefundLineItemRecord,
  OrderRefundRecord,
  OrderReturnLineItemRecord,
  OrderReturnRecord,
  OrderShippingLineRecord,
  OrderTaxLineRecord,
  OrderTransactionRecord,
} from '../../state/types.js';
import {
  buildSyntheticCursor,
  getDocumentFragments,
  getFieldResponseKey,
  paginateConnectionItems,
  projectGraphqlObject,
  readNullableIntArgument,
  readNullableStringArgument,
  serializeConnection,
} from '../graphql-helpers.js';
import { serializeMetafieldsConnection, serializeMetafieldSelection } from '../metafields.js';
import type { DraftOrderSavedSearchRecord } from './shared.js';
import {
  buildDraftOrderTagId,
  DRAFT_ORDER_SAVED_SEARCHES,
  findOrderTransactionById,
  formatDecimalAmount,
  getSelectedChildFields,
  makeOrderMoneyBag,
  normalizeDraftOrderTagHandle,
  normalizeMoney,
  normalizeZeroMoneyBag,
  parseDecimalAmount,
  readOrderCurrencyCode,
  serializeSelectedUserErrors,
  subtractMoney,
  totalCapturableAmount,
} from './shared.js';

export function serializeOrderManagementPayload(
  field: FieldNode,
  order: OrderRecord | null,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'order':
        payload[selectionKey] = order ? serializeOrderNode(selection, order) : null;
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

function serializeSelectedTransactionUserErrors(
  field: FieldNode,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Array<Record<string, unknown>> {
  return serializeSelectedUserErrors(field, userErrors);
}

export function serializeOrderCapturePayload(
  field: FieldNode,
  transaction: OrderTransactionRecord | null,
  order: OrderRecord | null,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'transaction':
        payload[selectionKey] = transaction ? serializeOrderTransaction(selection, transaction) : null;
        break;
      case 'order':
        payload[selectionKey] = order ? serializeOrderNode(selection, order) : null;
        break;
      case 'userErrors':
        payload[selectionKey] = serializeSelectedTransactionUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }
  return payload;
}

export function serializeTransactionVoidPayload(
  field: FieldNode,
  transaction: OrderTransactionRecord | null,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'transaction':
        payload[selectionKey] = transaction ? serializeOrderTransaction(selection, transaction) : null;
        break;
      case 'userErrors':
        payload[selectionKey] = serializeSelectedTransactionUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }
  return payload;
}

export function serializeJob(field: FieldNode, jobId: string, done = true): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = jobId;
        break;
      case 'done':
        result[key] = done;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDraftOrderSavedSearch(
  field: FieldNode,
  savedSearch: DraftOrderSavedSearchRecord,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = savedSearch.id;
        break;
      case 'legacyResourceId':
        result[key] = savedSearch.legacyResourceId;
        break;
      case 'name':
        result[key] = savedSearch.name;
        break;
      case 'query':
        result[key] = savedSearch.query;
        break;
      case 'resourceType':
        result[key] = savedSearch.resourceType;
        break;
      case 'searchTerms':
        result[key] = savedSearch.searchTerms;
        break;
      case 'filters':
        result[key] = [];
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

export function serializeDraftOrderSavedSearchesConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const items = args['reverse'] === true ? [...DRAFT_ORDER_SAVED_SEARCHES].reverse() : DRAFT_ORDER_SAVED_SEARCHES;
  const window = paginateConnectionItems(items, field, variables, (savedSearch) => savedSearch.id);

  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (savedSearch) => savedSearch.id,
    serializeNode: (savedSearch, selection) => serializeDraftOrderSavedSearch(selection, savedSearch),
  });
}

function serializeEmptyPageInfo(field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'hasNextPage':
      case 'hasPreviousPage':
        result[key] = false;
        break;
      case 'startCursor':
      case 'endCursor':
        result[key] = null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

export function serializeDraftOrderAvailableDeliveryOptions(field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'availableShippingRates':
      case 'availableLocalDeliveryRates':
      case 'availableLocalPickupOptions':
        result[key] = [];
        break;
      case 'pageInfo':
        result[key] = serializeEmptyPageInfo(selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

export function findDraftOrderTagById(id: string): { id: string; handle: string; title: string } | null {
  for (const draftOrder of store.getDraftOrders()) {
    const tag = draftOrder.tags.find((candidate) => buildDraftOrderTagId(candidate) === id);
    if (tag) {
      return {
        id,
        handle: normalizeDraftOrderTagHandle(tag),
        title: tag,
      };
    }
  }
  return null;
}

export function serializeDraftOrderTag(
  field: FieldNode,
  tag: { id: string; handle: string; title: string },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = tag.id;
        break;
      case 'handle':
        result[key] = tag.handle;
        break;
      case 'title':
        result[key] = tag.title;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

export function serializeOrderCreateMandatePaymentPayload(
  field: FieldNode,
  mandatePayment: OrderMandatePaymentRecord | null,
  order: OrderRecord | null,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'job':
        payload[selectionKey] = mandatePayment ? serializeJob(selection, mandatePayment.jobId) : null;
        break;
      case 'paymentReferenceId':
        payload[selectionKey] = mandatePayment?.paymentReferenceId ?? null;
        break;
      case 'order':
        payload[selectionKey] = order ? serializeOrderNode(selection, order) : null;
        break;
      case 'userErrors':
        payload[selectionKey] = serializeSelectedTransactionUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }
  return payload;
}

export function serializeOrderCancelPayload(
  field: FieldNode,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'job':
        payload[selectionKey] = null;
        break;
      case 'orderCancelUserErrors':
        payload[selectionKey] = serializeSelectedUserErrors(selection, userErrors);
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }
  return payload;
}

export function buildAccessDeniedError(operationName: string, requiredAccess: string): Record<string, unknown> {
  return {
    message: `Access denied for ${operationName} field. Required access: ${requiredAccess}`,
    extensions: {
      code: 'ACCESS_DENIED',
      documentation: 'https://shopify.dev/api/usage/access-scopes',
      requiredAccess,
    },
    path: [operationName],
  };
}

export function serializeRefundCreatePayload(
  field: FieldNode,
  refund: OrderRefundRecord | null,
  order: OrderRecord | null,
  userErrors: Array<Record<string, unknown>>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'refund':
        payload[selectionKey] = refund ? serializeOrderRefund(selection, refund) : null;
        break;
      case 'order':
        payload[selectionKey] = order ? serializeOrderNode(selection, order) : null;
        break;
      case 'userErrors':
        payload[selectionKey] = userErrors;
        break;
      default:
        payload[selectionKey] = null;
        break;
    }
  }
  return payload;
}

function serializeMoneyField(field: FieldNode, money: MoneyV2Record | null): Record<string, unknown> | null {
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

function serializeShopMoneySet(
  field: FieldNode,
  money:
    | MoneyV2Record
    | {
        shopMoney: MoneyV2Record | null;
        presentmentMoney?: MoneyV2Record | null | undefined;
      }
    | null,
): Record<string, unknown> | null {
  const shopMoney = money && 'shopMoney' in money ? money.shopMoney : money;
  const presentmentMoney = money && 'shopMoney' in money ? (money.presentmentMoney ?? null) : null;
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'shopMoney':
        result[key] = serializeMoneyField(selection, shopMoney);
        break;
      case 'presentmentMoney':
        result[key] = serializeMoneyField(selection, presentmentMoney);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDraftOrderAddress(
  field: FieldNode,
  address: DraftOrderAddressRecord | null,
): Record<string, unknown> | null {
  if (!address) {
    return null;
  }
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'firstName':
        result[key] = address.firstName;
        break;
      case 'lastName':
        result[key] = address.lastName;
        break;
      case 'address1':
        result[key] = address.address1;
        break;
      case 'address2':
        result[key] = address.address2 ?? null;
        break;
      case 'company':
        result[key] = address.company ?? null;
        break;
      case 'city':
        result[key] = address.city;
        break;
      case 'province':
        result[key] = address.province ?? null;
        break;
      case 'provinceCode':
        result[key] = address.provinceCode;
        break;
      case 'country':
        result[key] = address.country ?? null;
        break;
      case 'countryCodeV2':
        result[key] = address.countryCodeV2;
        break;
      case 'zip':
        result[key] = address.zip;
        break;
      case 'phone':
        result[key] = address.phone;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDraftOrderAttributes(
  field: FieldNode,
  attributes: DraftOrderAttributeRecord[],
): Array<Record<string, unknown>> {
  return attributes.map((attribute) => {
    const result: Record<string, unknown> = {};
    for (const selection of getSelectedChildFields(field)) {
      const key = getFieldResponseKey(selection);
      switch (selection.name.value) {
        case 'key':
          result[key] = attribute.key;
          break;
        case 'value':
          result[key] = attribute.value;
          break;
        default:
          result[key] = null;
          break;
      }
    }
    return result;
  });
}

function serializeDraftOrderShippingLine(
  field: FieldNode,
  shippingLine: DraftOrderShippingLineRecord | null,
): Record<string, unknown> | null {
  if (!shippingLine) {
    return null;
  }
  const orderShippingLine = shippingLine as DraftOrderShippingLineRecord & Partial<OrderShippingLineRecord>;
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'title':
        result[key] = shippingLine.title;
        break;
      case 'code':
        result[key] = shippingLine.code;
        break;
      case 'source':
        result[key] = orderShippingLine.source ?? null;
        break;
      case 'originalPriceSet':
        result[key] = serializeShopMoneySet(selection, shippingLine.originalPriceSet ?? null);
        break;
      case 'taxLines':
        result[key] = serializeOrderTaxLines(selection, orderShippingLine.taxLines ?? []);
        break;
      case 'discountedPriceSet':
      case 'currentDiscountedPriceSet':
        result[key] = serializeShopMoneySet(selection, shippingLine.originalPriceSet?.shopMoney ?? null);
        break;
      case 'custom':
        result[key] = true;
        break;
      case 'carrierIdentifier':
      case 'deliveryCategory':
      case 'phone':
      case 'shippingRateHandle':
        result[key] = null;
        break;
      case 'isRemoved':
        result[key] = false;
        break;
      case 'discountAllocations':
        result[key] = [];
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDraftOrderAppliedDiscount(
  field: FieldNode,
  discount: DraftOrderAppliedDiscountRecord | null,
): Record<string, unknown> | null {
  if (!discount) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'title':
        result[key] = discount.title;
        break;
      case 'description':
        result[key] = discount.description ?? '';
        break;
      case 'value':
        result[key] = discount.value;
        break;
      case 'valueType':
        result[key] = discount.valueType;
        break;
      case 'amountSet':
        result[key] = serializeShopMoneySet(selection, discount.amountSet?.shopMoney ?? null);
        break;
      case 'amountV2':
        result[key] = serializeMoneyField(selection, discount.amountSet?.shopMoney ?? null);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDraftOrderCustomer(
  field: FieldNode,
  customer: DraftOrderCustomerRecord | null,
): Record<string, unknown> | null {
  if (!customer) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = customer.id;
        break;
      case 'email':
        result[key] = customer.email;
        break;
      case 'displayName':
        result[key] = customer.displayName;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDraftOrderPaymentTerms(
  field: FieldNode,
  paymentTerms: DraftOrderPaymentTermsRecord | null,
): Record<string, unknown> | null {
  if (!paymentTerms) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = paymentTerms.id;
        break;
      case 'due':
        result[key] = paymentTerms.due;
        break;
      case 'overdue':
        result[key] = paymentTerms.overdue;
        break;
      case 'dueInDays':
        result[key] = paymentTerms.dueInDays;
        break;
      case 'paymentTermsName':
        result[key] = paymentTerms.paymentTermsName;
        break;
      case 'paymentTermsType':
        result[key] = paymentTerms.paymentTermsType;
        break;
      case 'translatedName':
        result[key] = paymentTerms.translatedName;
        break;
      case 'paymentSchedules':
        result[key] = serializePaymentSchedulesConnection(selection, paymentTerms.paymentSchedules ?? []);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializePaymentScheduleNode(field: FieldNode, schedule: PaymentScheduleRecord): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = schedule.id;
        break;
      case 'dueAt':
        result[key] = schedule.dueAt;
        break;
      case 'issuedAt':
        result[key] = schedule.issuedAt;
        break;
      case 'completedAt':
        result[key] = schedule.completedAt;
        break;
      case 'completed':
        result[key] = schedule.completed ?? false;
        break;
      case 'due':
        result[key] = schedule.due ?? false;
        break;
      case 'amount':
        result[key] = serializeMoneyField(selection, schedule.amount ?? null);
        break;
      case 'balanceDue':
        result[key] = serializeMoneyField(selection, schedule.balanceDue ?? null);
        break;
      case 'totalBalance':
        result[key] = serializeMoneyField(selection, schedule.totalBalance ?? null);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializePaymentSchedulesConnection(
  field: FieldNode,
  schedules: PaymentScheduleRecord[],
): Record<string, unknown> {
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    schedules,
    field,
    {},
    (schedule) => schedule.id,
  );
  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (schedule) => schedule.id,
    serializeNode: (schedule, selection) => serializePaymentScheduleNode(selection, schedule),
  });
}

function serializeDraftOrderLineItemNode(
  field: FieldNode,
  lineItem: DraftOrderLineItemRecord,
): Record<string, unknown> {
  const nodeResult: Record<string, unknown> = {};
  for (const nodeSelection of getSelectedChildFields(field)) {
    const nodeKey = getFieldResponseKey(nodeSelection);
    switch (nodeSelection.name.value) {
      case 'id':
        nodeResult[nodeKey] = lineItem.id;
        break;
      case 'title':
        nodeResult[nodeKey] = lineItem.title;
        break;
      case 'name':
        nodeResult[nodeKey] = lineItem.name;
        break;
      case 'quantity':
        nodeResult[nodeKey] = lineItem.quantity;
        break;
      case 'sku':
        nodeResult[nodeKey] = lineItem.sku;
        break;
      case 'variantTitle':
        nodeResult[nodeKey] = lineItem.variantTitle === 'Default Title' ? null : lineItem.variantTitle;
        break;
      case 'custom':
        nodeResult[nodeKey] = lineItem.custom;
        break;
      case 'requiresShipping':
        nodeResult[nodeKey] = lineItem.requiresShipping;
        break;
      case 'taxable':
        nodeResult[nodeKey] = lineItem.taxable;
        break;
      case 'customAttributes':
        nodeResult[nodeKey] = serializeDraftOrderAttributes(nodeSelection, lineItem.customAttributes);
        break;
      case 'appliedDiscount':
        nodeResult[nodeKey] = serializeDraftOrderAppliedDiscount(nodeSelection, lineItem.appliedDiscount);
        break;
      case 'originalUnitPriceSet':
        nodeResult[nodeKey] = serializeShopMoneySet(nodeSelection, lineItem.originalUnitPriceSet ?? null);
        break;
      case 'originalTotalSet':
        nodeResult[nodeKey] = serializeShopMoneySet(nodeSelection, lineItem.originalTotalSet?.shopMoney ?? null);
        break;
      case 'discountedTotalSet':
        nodeResult[nodeKey] = serializeShopMoneySet(nodeSelection, lineItem.discountedTotalSet?.shopMoney ?? null);
        break;
      case 'totalDiscountSet':
        nodeResult[nodeKey] = serializeShopMoneySet(nodeSelection, lineItem.totalDiscountSet?.shopMoney ?? null);
        break;
      case 'variant':
        nodeResult[nodeKey] = lineItem.variantId
          ? Object.fromEntries(
              getSelectedChildFields(nodeSelection).map((variantSelection) => {
                const variantKey = getFieldResponseKey(variantSelection);
                switch (variantSelection.name.value) {
                  case 'id':
                    return [variantKey, lineItem.variantId];
                  case 'title':
                    return [variantKey, lineItem.variantTitle];
                  case 'sku':
                    return [variantKey, lineItem.sku === '' ? null : lineItem.sku];
                  default:
                    return [variantKey, null];
                }
              }),
            )
          : null;
        break;
      case 'product':
        nodeResult[nodeKey] =
          lineItem.productId && lineItem.name
            ? Object.fromEntries(
                getSelectedChildFields(nodeSelection).map((productSelection) => {
                  const productKey = getFieldResponseKey(productSelection);
                  switch (productSelection.name.value) {
                    case 'id':
                      return [productKey, lineItem.productId];
                    case 'title':
                      return [productKey, lineItem.name];
                    default:
                      return [productKey, null];
                  }
                }),
              )
            : null;
        break;
      default:
        nodeResult[nodeKey] = null;
        break;
    }
  }
  return nodeResult;
}

function serializeDraftOrderLineItemsConnection(
  field: FieldNode,
  lineItems: DraftOrderLineItemRecord[],
): Record<string, unknown> {
  return serializeConnection(field, {
    items: lineItems,
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: (lineItem) => lineItem.id,
    serializeNode: (lineItem, selection) => serializeDraftOrderLineItemNode(selection, lineItem),
  });
}

function serializeCalculatedDraftOrderLineItem(
  field: FieldNode,
  lineItem: DraftOrderLineItemRecord,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'uuid':
        result[key] = lineItem.id.split('/').at(-1) ?? lineItem.id;
        break;
      case 'title':
        result[key] = lineItem.title;
        break;
      case 'name':
        result[key] = lineItem.name;
        break;
      case 'quantity':
        result[key] = lineItem.quantity;
        break;
      case 'sku':
        result[key] = lineItem.sku;
        break;
      case 'variantTitle':
        result[key] = lineItem.variantTitle === 'Default Title' ? null : lineItem.variantTitle;
        break;
      case 'custom':
        result[key] = lineItem.custom;
        break;
      case 'requiresShipping':
        result[key] = lineItem.requiresShipping;
        break;
      case 'taxable':
        result[key] = lineItem.taxable;
        break;
      case 'customAttributes':
        result[key] = serializeDraftOrderAttributes(selection, lineItem.customAttributes);
        break;
      case 'appliedDiscount':
        result[key] = serializeDraftOrderAppliedDiscount(selection, lineItem.appliedDiscount);
        break;
      case 'originalUnitPrice':
        result[key] = serializeMoneyField(selection, lineItem.originalUnitPriceSet?.shopMoney ?? null);
        break;
      case 'originalTotal':
        result[key] = serializeMoneyField(selection, lineItem.originalTotalSet?.shopMoney ?? null);
        break;
      case 'discountedTotal':
        result[key] = serializeMoneyField(selection, lineItem.discountedTotalSet?.shopMoney ?? null);
        break;
      case 'totalDiscount':
        result[key] = serializeMoneyField(selection, lineItem.totalDiscountSet?.shopMoney ?? null);
        break;
      case 'components':
      case 'customAttributesV2':
        result[key] = [];
        break;
      case 'approximateDiscountedUnitPriceSet':
        result[key] = serializeShopMoneySet(selection, lineItem.discountedTotalSet?.shopMoney ?? null);
        break;
      case 'discountedTotalSet':
        result[key] = serializeShopMoneySet(selection, lineItem.discountedTotalSet?.shopMoney ?? null);
        break;
      case 'originalTotalSet':
        result[key] = serializeShopMoneySet(selection, lineItem.originalTotalSet?.shopMoney ?? null);
        break;
      case 'originalUnitPriceSet':
        result[key] = serializeShopMoneySet(selection, lineItem.originalUnitPriceSet ?? null);
        break;
      case 'totalDiscountSet':
        result[key] = serializeShopMoneySet(selection, lineItem.totalDiscountSet?.shopMoney ?? null);
        break;
      case 'fulfillmentService':
      case 'image':
      case 'priceOverride':
      case 'product':
      case 'variant':
      case 'weight':
        result[key] = serializeDraftOrderLineItemNode(selection, lineItem)[key] ?? null;
        break;
      case 'isGiftCard':
        result[key] = false;
        break;
      case 'vendor':
        result[key] = null;
        break;
      default:
        result[key] = serializeDraftOrderLineItemNode(selection, lineItem)[key];
        break;
    }
  }
  return result;
}

export function serializeCalculatedDraftOrder(field: FieldNode, draftOrder: DraftOrderRecord): Record<string, unknown> {
  const currencyCode = draftOrder.totalPriceSet?.shopMoney.currencyCode ?? 'CAD';
  const zeroMoney = { shopMoney: normalizeMoney('0.0', currencyCode) };
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'acceptAutomaticDiscounts':
        result[key] = false;
        break;
      case 'alerts':
      case 'availableShippingRates':
      case 'platformDiscounts':
      case 'taxLines':
      case 'warnings':
        result[key] = [];
        break;
      case 'allVariantPricesOverridden':
      case 'anyVariantPricesOverridden':
        result[key] = false;
        break;
      case 'appliedDiscount':
        result[key] = serializeDraftOrderAppliedDiscount(selection, draftOrder.appliedDiscount);
        break;
      case 'billingAddressMatchesShippingAddress':
        result[key] = JSON.stringify(draftOrder.billingAddress) === JSON.stringify(draftOrder.shippingAddress);
        break;
      case 'currencyCode':
      case 'presentmentCurrencyCode':
        result[key] = currencyCode;
        break;
      case 'customer':
        result[key] = serializeDraftOrderCustomer(selection, draftOrder.customer);
        break;
      case 'discountCodes':
        result[key] = [];
        break;
      case 'lineItems':
        result[key] = draftOrder.lineItems.map((lineItem) =>
          serializeCalculatedDraftOrderLineItem(selection, lineItem),
        );
        break;
      case 'lineItemsSubtotalPrice':
      case 'subtotalPriceSet':
        result[key] = serializeShopMoneySet(selection, draftOrder.subtotalPriceSet ?? zeroMoney);
        break;
      case 'phone':
      case 'purchasingEntity':
      case 'transformerFingerprint':
        result[key] = null;
        break;
      case 'shippingLine':
        result[key] = serializeDraftOrderShippingLine(selection, draftOrder.shippingLine);
        break;
      case 'taxesIncluded':
        result[key] = draftOrder.taxesIncluded;
        break;
      case 'totalDiscountsSet':
        result[key] = serializeShopMoneySet(selection, draftOrder.totalDiscountsSet ?? zeroMoney);
        break;
      case 'totalLineItemsPriceSet':
        result[key] = serializeShopMoneySet(selection, draftOrder.subtotalPriceSet ?? zeroMoney);
        break;
      case 'totalPriceSet':
        result[key] = serializeShopMoneySet(selection, draftOrder.totalPriceSet ?? zeroMoney);
        break;
      case 'totalQuantityOfLineItems':
        result[key] = draftOrder.lineItems.reduce((sum, lineItem) => sum + lineItem.quantity, 0);
        break;
      case 'totalShippingPriceSet':
        result[key] = serializeShopMoneySet(selection, draftOrder.totalShippingPriceSet ?? zeroMoney);
        break;
      case 'totalTaxSet':
        result[key] = serializeShopMoneySet(selection, zeroMoney);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

export function serializeDraftOrderNode(field: FieldNode, draftOrder: DraftOrderRecord): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = draftOrder.id;
        break;
      case 'name':
        result[key] = draftOrder.name;
        break;
      case 'order': {
        const order = draftOrder.orderId ? store.getOrderById(draftOrder.orderId) : null;
        result[key] = order ? serializeOrderNode(selection, order) : null;
        break;
      }
      case 'invoiceUrl':
        result[key] = draftOrder.invoiceUrl;
        break;
      case 'status':
        result[key] = draftOrder.status;
        break;
      case 'ready':
        result[key] = draftOrder.ready;
        break;
      case 'email':
        result[key] = draftOrder.email;
        break;
      case 'note':
      case 'note2':
        result[key] = draftOrder.note;
        break;
      case 'tags':
        result[key] = structuredClone(draftOrder.tags);
        break;
      case 'customer':
        result[key] = serializeDraftOrderCustomer(selection, draftOrder.customer);
        break;
      case 'taxExempt':
        result[key] = draftOrder.taxExempt;
        break;
      case 'taxesIncluded':
        result[key] = draftOrder.taxesIncluded;
        break;
      case 'reserveInventoryUntil':
        result[key] = draftOrder.reserveInventoryUntil;
        break;
      case 'paymentTerms':
        result[key] = serializeDraftOrderPaymentTerms(selection, draftOrder.paymentTerms);
        break;
      case 'appliedDiscount':
        result[key] = serializeDraftOrderAppliedDiscount(selection, draftOrder.appliedDiscount);
        break;
      case 'customAttributes':
        result[key] = serializeDraftOrderAttributes(selection, draftOrder.customAttributes);
        break;
      case 'billingAddress':
        result[key] = serializeDraftOrderAddress(selection, draftOrder.billingAddress);
        break;
      case 'shippingAddress':
        result[key] = serializeDraftOrderAddress(selection, draftOrder.shippingAddress);
        break;
      case 'shippingLine':
        result[key] = serializeDraftOrderShippingLine(selection, draftOrder.shippingLine);
        break;
      case 'createdAt':
        result[key] = draftOrder.createdAt;
        break;
      case 'updatedAt':
        result[key] = draftOrder.updatedAt;
        break;
      case 'completedAt':
        result[key] = draftOrder.completedAt ?? null;
        break;
      case 'subtotalPriceSet':
        result[key] = serializeShopMoneySet(selection, draftOrder.subtotalPriceSet?.shopMoney ?? null);
        break;
      case 'totalDiscountsSet':
        result[key] = serializeShopMoneySet(selection, draftOrder.totalDiscountsSet?.shopMoney ?? null);
        break;
      case 'totalShippingPriceSet':
        result[key] = serializeShopMoneySet(selection, draftOrder.totalShippingPriceSet?.shopMoney ?? null);
        break;
      case 'totalPriceSet':
        result[key] = serializeShopMoneySet(selection, draftOrder.totalPriceSet?.shopMoney ?? null);
        break;
      case 'totalQuantityOfLineItems':
        result[key] = draftOrder.lineItems.reduce((sum, lineItem) => sum + lineItem.quantity, 0);
        break;
      case 'lineItems':
        result[key] = serializeDraftOrderLineItemsConnection(selection, draftOrder.lineItems);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

export type OrderSearchExtensionEntry = {
  path: string[];
  query: string;
  parsed: {
    field: string;
    match_all: string;
  };
  warnings: Array<{
    field: string;
    message: string;
    code: string;
  }>;
};

export function buildDraftOrderInvalidSearchExtension(
  rawQuery: unknown,
  path: string[],
): OrderSearchExtensionEntry | null {
  if (typeof rawQuery !== 'string') {
    return null;
  }

  const query = rawQuery.trim();
  if (!query) {
    return null;
  }

  const singleFieldMatch = query.match(/^([A-Za-z_]+):(.*)$/u);
  if (!singleFieldMatch) {
    return null;
  }

  const field = singleFieldMatch[1]?.trim().toLowerCase() ?? '';
  const matchAll = singleFieldMatch[2]?.trim() ?? '';
  if (field !== 'email' || !matchAll) {
    return null;
  }

  return {
    path,
    query,
    parsed: {
      field,
      match_all: matchAll,
    },
    warnings: [
      {
        field,
        message: 'Invalid search field for this query.',
        code: 'invalid_field',
      },
    ],
  };
}

export function shouldServeDraftOrderSearchLocally(rawQuery: unknown): boolean {
  return (
    buildDraftOrderInvalidSearchExtension(rawQuery, ['draftOrders']) !== null ||
    isDraftOrderSearchQuerySupported(rawQuery)
  );
}

export function shouldServeDraftOrderCatalogLocally(rawQuery: unknown, rawSavedSearchId: unknown): boolean {
  if (typeof rawSavedSearchId === 'string' && rawSavedSearchId.trim()) {
    return false;
  }

  return typeof rawQuery !== 'string' || shouldServeDraftOrderSearchLocally(rawQuery);
}

function isDraftOrderSearchQuerySupported(rawQuery: unknown): boolean {
  if (typeof rawQuery !== 'string' || !rawQuery.trim()) {
    return true;
  }

  if (buildDraftOrderInvalidSearchExtension(rawQuery, ['draftOrders'])) {
    return true;
  }

  const terms = parseSearchQueryTermList(rawQuery, { quoteCharacters: ['"'] });
  if (terms.length === 0) {
    return true;
  }

  return terms.every((term) => {
    if (term.field === null || term.field === '') {
      return false;
    }

    const field = term.field.toLowerCase();
    return (
      field === 'status' ||
      field === 'tag' ||
      field === 'source' ||
      field === 'customer_id' ||
      field === 'id' ||
      field === 'created_at' ||
      field === 'updated_at'
    );
  });
}

function matchesStringValue(candidate: string, rawValue: string): boolean {
  return matchesSearchQueryString(candidate, rawValue);
}

function readDraftOrderNumericId(draftOrder: DraftOrderRecord): number | null {
  const parsed = Number.parseInt(draftOrder.id.split('/').at(-1) ?? '', 10);
  return Number.isFinite(parsed) ? parsed : null;
}

function matchesNumericTerm(candidate: number | null, rawValue: string): boolean {
  if (candidate === null) {
    return false;
  }

  const match = rawValue.trim().match(/^(<=|>=|<|>|=)?\s*(\d+)$/u);
  if (!match) {
    return false;
  }

  const operator = match[1] ?? '=';
  const threshold = Number.parseInt(match[2] ?? '', 10);
  if (!Number.isFinite(threshold)) {
    return false;
  }

  switch (operator) {
    case '<=':
      return candidate <= threshold;
    case '>=':
      return candidate >= threshold;
    case '<':
      return candidate < threshold;
    case '>':
      return candidate > threshold;
    case '=':
      return candidate === threshold;
    default:
      return false;
  }
}

function matchesTimestampTerm(timestamp: string, rawValue: string): boolean {
  const match = rawValue.trim().match(/^(<=|>=|<|>|=)?\s*(.+)$/u);
  if (!match) {
    return false;
  }

  const operator = match[1] ?? '=';
  const thresholdTime = Date.parse(match[2] ?? '');
  const timestampTime = Date.parse(timestamp);
  if (!Number.isFinite(thresholdTime) || !Number.isFinite(timestampTime)) {
    return false;
  }

  switch (operator) {
    case '<=':
      return timestampTime <= thresholdTime;
    case '>=':
      return timestampTime >= thresholdTime;
    case '<':
      return timestampTime < thresholdTime;
    case '>':
      return timestampTime > thresholdTime;
    case '=':
      return timestampTime === thresholdTime;
    default:
      return false;
  }
}

function normalizeSearchValue(rawValue: string): string {
  return stripSearchQueryValueQuotes(rawValue);
}

function matchesStringValueIncludingContains(candidate: string | null | undefined, rawValue: string): boolean {
  return matchesSearchQueryString(candidate, rawValue, 'includes');
}

function matchesDraftOrderSource(draftOrder: DraftOrderRecord, rawValue: string): boolean {
  return draftOrder.customAttributes.some(
    (attribute) =>
      attribute.key.toLowerCase() === 'source' &&
      typeof attribute.value === 'string' &&
      matchesStringValue(attribute.value, rawValue),
  );
}

function matchesDraftOrderSearchTerm(draftOrder: DraftOrderRecord, term: SearchQueryTerm): boolean {
  if (term.field === null || term.field === '') {
    return false;
  }

  const field = term.field.toLowerCase();
  const value = searchQueryTermValue(term);

  switch (field) {
    case 'status':
      return typeof draftOrder.status === 'string' && matchesStringValue(draftOrder.status, value);
    case 'tag':
      return draftOrder.tags.some((tag) => matchesStringValue(tag, value));
    case 'source':
      return matchesDraftOrderSource(draftOrder, value);
    case 'customer_id':
      return false;
    case 'id':
      return draftOrder.id === value || matchesNumericTerm(readDraftOrderNumericId(draftOrder), value);
    case 'created_at':
      return matchesTimestampTerm(draftOrder.createdAt, value);
    case 'updated_at':
      return matchesTimestampTerm(draftOrder.updatedAt, value);
    default:
      return false;
  }
}

export function applyDraftOrdersQuery(draftOrders: DraftOrderRecord[], rawQuery: unknown): DraftOrderRecord[] {
  if (typeof rawQuery !== 'string' || !rawQuery.trim()) {
    return draftOrders;
  }

  if (buildDraftOrderInvalidSearchExtension(rawQuery, ['draftOrders'])) {
    return draftOrders;
  }

  if (!isDraftOrderSearchQuerySupported(rawQuery)) {
    return [];
  }

  return applySearchQueryTerms(draftOrders, rawQuery, { quoteCharacters: ['"'] }, matchesDraftOrderSearchTerm);
}

function compareDraftOrderIds(leftId: string, rightId: string): number {
  const leftTail = Number.parseInt(leftId.split('/').at(-1) ?? '', 10);
  const rightTail = Number.parseInt(rightId.split('/').at(-1) ?? '', 10);
  if (Number.isFinite(leftTail) && Number.isFinite(rightTail)) {
    return leftTail - rightTail;
  }

  return leftId.localeCompare(rightId);
}

function sortDraftOrdersForConnection(
  draftOrders: DraftOrderRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
): DraftOrderRecord[] {
  const args = getFieldArguments(field, variables);
  const sortKey = typeof args['sortKey'] === 'string' ? args['sortKey'] : null;
  const reverse = args['reverse'] === true;

  if (!sortKey && !reverse) {
    return draftOrders;
  }

  const sorted = [...draftOrders].sort((left, right) => {
    switch (sortKey) {
      case 'CREATED_AT':
        return left.createdAt.localeCompare(right.createdAt) || compareDraftOrderIds(left.id, right.id);
      case 'UPDATED_AT':
        return left.updatedAt.localeCompare(right.updatedAt) || compareDraftOrderIds(left.id, right.id);
      case 'NAME':
        return left.name.localeCompare(right.name) || compareDraftOrderIds(left.id, right.id);
      case 'ID':
      default:
        return compareDraftOrderIds(left.id, right.id);
    }
  });

  return reverse ? sorted.reverse() : sorted;
}

function applySyntheticCursorWindow<T extends { id: string }>(
  records: T[],
  field: FieldNode,
  variables: Record<string, unknown>,
): {
  visibleRecords: T[];
  hasNextPage: boolean;
  hasPreviousPage: boolean;
} {
  const first = readNullableIntArgument(field, 'first', variables);
  const last = readNullableIntArgument(field, 'last', variables);
  const after = readNullableStringArgument(field, 'after', variables);
  const before = readNullableStringArgument(field, 'before', variables);

  let startIndex = 0;
  let endIndex = records.length;

  if (after !== null) {
    const afterIndex = records.findIndex((record) => buildSyntheticCursor(record.id) === after);
    startIndex = afterIndex >= 0 ? afterIndex + 1 : records.length;
  }

  if (before !== null) {
    const beforeIndex = records.findIndex((record) => buildSyntheticCursor(record.id) === before);
    endIndex = beforeIndex >= 0 ? beforeIndex : 0;
  }

  if (endIndex < startIndex) {
    endIndex = startIndex;
  }

  const cursorWindow = records.slice(startIndex, endIndex);
  let visibleRecords = cursorWindow;
  let hasNextPage = endIndex < records.length;
  let hasPreviousPage = startIndex > 0;

  if (first !== null) {
    visibleRecords = cursorWindow.slice(0, Math.max(0, first));
    hasNextPage = hasNextPage || cursorWindow.length > visibleRecords.length;
  } else if (last !== null) {
    visibleRecords = cursorWindow.slice(Math.max(0, cursorWindow.length - Math.max(0, last)));
    hasPreviousPage = hasPreviousPage || cursorWindow.length > visibleRecords.length;
  }

  return {
    visibleRecords,
    hasNextPage,
    hasPreviousPage,
  };
}

export function serializeDraftOrdersConnection(
  field: FieldNode,
  draftOrders: DraftOrderRecord[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const filteredDraftOrders =
    typeof args['savedSearchId'] === 'string' && args['savedSearchId'].trim()
      ? []
      : applyDraftOrdersQuery(draftOrders, args['query']);
  const orderedDraftOrders = sortDraftOrdersForConnection(filteredDraftOrders, field, variables);
  const { visibleRecords, hasNextPage, hasPreviousPage } = applySyntheticCursorWindow(
    orderedDraftOrders,
    field,
    variables,
  );
  return serializeConnection(field, {
    items: visibleRecords,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (draftOrder) => buildSyntheticCursor(draftOrder.id),
    serializeNode: (draftOrder, selection) => serializeDraftOrderNode(selection, draftOrder),
    selectedFieldOptions: { includeInlineFragments: true },
    pageInfoOptions: { prefixCursors: false, includeInlineFragments: true },
  });
}

export function serializeDraftOrdersCount(
  field: FieldNode,
  draftOrders: DraftOrderRecord[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const filteredDraftOrders =
    typeof args['savedSearchId'] === 'string' && args['savedSearchId'].trim()
      ? []
      : applyDraftOrdersQuery(draftOrders, args['query']);
  const rawLimit = args['limit'];
  const limit = typeof rawLimit === 'number' && Number.isFinite(rawLimit) && rawLimit >= 0 ? rawLimit : null;
  const count = limit === null ? filteredDraftOrders.length : Math.min(filteredDraftOrders.length, limit);
  const precision = limit !== null && filteredDraftOrders.length > limit ? 'AT_LEAST' : 'EXACT';
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const selectionKey = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'count':
        result[selectionKey] = count;
        break;
      case 'precision':
        result[selectionKey] = precision;
        break;
      default:
        result[selectionKey] = null;
        break;
    }
  }

  return result;
}

function serializeOrderCustomer(
  field: FieldNode,
  customer: OrderCustomerRecord | null,
): Record<string, unknown> | null {
  if (!customer) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = customer.id;
        break;
      case 'email':
        result[key] = customer.email;
        break;
      case 'displayName':
        result[key] = customer.displayName;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeOrderTaxLines(field: FieldNode, taxLines: OrderTaxLineRecord[]): Array<Record<string, unknown>> {
  return taxLines.map((taxLine) => {
    const result: Record<string, unknown> = {};
    for (const selection of getSelectedChildFields(field)) {
      const key = getFieldResponseKey(selection);
      switch (selection.name.value) {
        case 'title':
          result[key] = taxLine.title;
          break;
        case 'rate':
          result[key] = taxLine.rate;
          break;
        case 'channelLiable':
          result[key] = taxLine.channelLiable;
          break;
        case 'priceSet':
          result[key] = serializeShopMoneySet(selection, taxLine.priceSet ?? null);
          break;
        default:
          result[key] = null;
          break;
      }
    }
    return result;
  });
}

function serializeOrderDiscountApplicationsConnection(
  field: FieldNode,
  discountApplications: OrderDiscountApplicationRecord[],
): Record<string, unknown> {
  return serializeConnection(field, {
    items: discountApplications,
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: (_discountApplication, index) => `discount-application:${index + 1}`,
    serializeNode: (discountApplication, selection) =>
      serializeOrderDiscountApplication(selection, discountApplication),
    pageInfoOptions: {
      includeCursors: false,
    },
  });
}

function serializeOrderDiscountApplication(
  field: FieldNode,
  discountApplication: OrderDiscountApplicationRecord,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'code':
        result[key] = discountApplication.code;
        break;
      case 'value':
        result[key] = serializeOrderDiscountApplicationValue(selection, discountApplication);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeOrderDiscountApplicationValue(
  field: FieldNode,
  discountApplication: OrderDiscountApplicationRecord,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'amount':
        result[key] = discountApplication.value.type === 'money' ? (discountApplication.value.amount ?? null) : null;
        break;
      case 'currencyCode':
        result[key] =
          discountApplication.value.type === 'money' ? (discountApplication.value.currencyCode ?? null) : null;
        break;
      case 'percentage':
        result[key] =
          discountApplication.value.type === 'percentage' ? (discountApplication.value.percentage ?? null) : null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

export function serializeOrderLineItemNode(field: FieldNode, lineItem: OrderLineItemRecord): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = lineItem.id;
        break;
      case 'title':
        result[key] = lineItem.title;
        break;
      case 'quantity':
        result[key] = lineItem.quantity;
        break;
      case 'currentQuantity':
        result[key] = lineItem.currentQuantity ?? lineItem.quantity;
        break;
      case 'refundableQuantity':
      case 'fulfillableQuantity':
      case 'unfulfilledQuantity':
        result[key] = Math.max(0, lineItem.currentQuantity ?? lineItem.quantity);
        break;
      case 'sku':
        result[key] = lineItem.sku;
        break;
      case 'variant':
        result[key] = lineItem.variantId
          ? Object.fromEntries(
              getSelectedChildFields(selection).map((variantSelection) => {
                const variantKey = getFieldResponseKey(variantSelection);
                switch (variantSelection.name.value) {
                  case 'id':
                    return [variantKey, lineItem.variantId];
                  default:
                    return [variantKey, null];
                }
              }),
            )
          : null;
        break;
      case 'variantTitle':
        result[key] = lineItem.variantTitle;
        break;
      case 'originalUnitPriceSet':
        result[key] = serializeShopMoneySet(selection, lineItem.originalUnitPriceSet ?? null);
        break;
      case 'taxLines':
        result[key] = serializeOrderTaxLines(selection, lineItem.taxLines ?? []);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeOrderLineItemsConnection(
  field: FieldNode,
  lineItems: OrderLineItemRecord[],
): Record<string, unknown> {
  return serializeConnection(field, {
    items: lineItems,
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: (lineItem) => lineItem.id,
    serializeNode: (lineItem, selection) => serializeOrderLineItemNode(selection, lineItem),
  });
}

function serializeOrderFulfillmentLineItem(
  field: FieldNode,
  fulfillmentLineItem: OrderFulfillmentLineItemRecord,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = fulfillmentLineItem.id;
        break;
      case 'quantity':
        result[key] = fulfillmentLineItem.quantity;
        break;
      case 'lineItem':
        result[key] = fulfillmentLineItem.lineItemId
          ? Object.fromEntries(
              getSelectedChildFields(selection).map((lineItemSelection) => {
                const lineItemKey = getFieldResponseKey(lineItemSelection);
                switch (lineItemSelection.name.value) {
                  case 'id':
                    return [lineItemKey, fulfillmentLineItem.lineItemId];
                  case 'title':
                    return [lineItemKey, fulfillmentLineItem.title];
                  default:
                    return [lineItemKey, null];
                }
              }),
            )
          : null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeOrderFulfillmentLineItemsConnection(
  field: FieldNode,
  fulfillmentLineItems: OrderFulfillmentLineItemRecord[],
): Record<string, unknown> {
  return serializeConnection(field, {
    items: fulfillmentLineItems,
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: (lineItem) => lineItem.id,
    serializeNode: (lineItem, selection) => serializeOrderFulfillmentLineItem(selection, lineItem),
    pageInfoOptions: {
      includeCursors: false,
    },
  });
}

function serializeOrderFulfillmentLocation(
  field: FieldNode,
  location: OrderFulfillmentLocationRecord | null | undefined,
): Record<string, unknown> | null {
  if (!location) {
    return null;
  }

  return Object.fromEntries(
    getSelectedChildFields(field).map((selection) => {
      const key = getFieldResponseKey(selection);
      switch (selection.name.value) {
        case 'id':
          return [key, location.id ?? null];
        case 'name':
          return [key, location.name];
        default:
          return [key, null];
      }
    }),
  );
}

function serializeOrderFulfillmentService(
  field: FieldNode,
  service: OrderFulfillmentServiceRecord | null | undefined,
): Record<string, unknown> | null {
  if (!service) {
    return null;
  }

  return Object.fromEntries(
    getSelectedChildFields(field).map((selection) => {
      const key = getFieldResponseKey(selection);
      switch (selection.name.value) {
        case 'id':
          return [key, service.id];
        case 'handle':
          return [key, service.handle];
        case 'serviceName':
          return [key, service.serviceName];
        case 'trackingSupport':
          return [key, service.trackingSupport ?? false];
        case 'type':
          return [key, service.type ?? null];
        case 'location':
          return [key, serializeOrderFulfillmentLocation(selection, service.location)];
        default:
          return [key, null];
      }
    }),
  );
}

function serializeOrderFulfillmentOriginAddress(
  field: FieldNode,
  originAddress: OrderFulfillmentOriginAddressRecord | null | undefined,
): Record<string, unknown> | null {
  if (!originAddress) {
    return null;
  }

  return Object.fromEntries(
    getSelectedChildFields(field).map((selection) => {
      const key = getFieldResponseKey(selection);
      switch (selection.name.value) {
        case 'address1':
          return [key, originAddress.address1 ?? null];
        case 'address2':
          return [key, originAddress.address2 ?? null];
        case 'city':
          return [key, originAddress.city ?? null];
        case 'countryCode':
          return [key, originAddress.countryCode];
        case 'provinceCode':
          return [key, originAddress.provinceCode ?? null];
        case 'zip':
          return [key, originAddress.zip ?? null];
        default:
          return [key, null];
      }
    }),
  );
}

export function serializeOrderFulfillmentEvent(
  field: FieldNode,
  event: OrderFulfillmentEventRecord,
): Record<string, unknown> {
  return Object.fromEntries(
    getSelectedChildFields(field).map((selection) => {
      const key = getFieldResponseKey(selection);
      switch (selection.name.value) {
        case 'id':
          return [key, event.id];
        case 'status':
          return [key, event.status];
        case 'message':
          return [key, event.message ?? null];
        case 'happenedAt':
          return [key, event.happenedAt];
        case 'createdAt':
          return [key, event.createdAt ?? event.happenedAt];
        case 'estimatedDeliveryAt':
          return [key, event.estimatedDeliveryAt ?? null];
        case 'city':
          return [key, event.city ?? null];
        case 'province':
          return [key, event.province ?? null];
        case 'country':
          return [key, event.country ?? null];
        case 'zip':
          return [key, event.zip ?? null];
        case 'address1':
          return [key, event.address1 ?? null];
        case 'latitude':
          return [key, event.latitude ?? null];
        case 'longitude':
          return [key, event.longitude ?? null];
        default:
          return [key, null];
      }
    }),
  );
}

function serializeOrderFulfillmentEventsConnection(
  field: FieldNode,
  events: OrderFulfillmentEventRecord[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const orderedEvents = args['reverse'] === true ? [...events].reverse() : events;
  const window = paginateConnectionItems(orderedEvents, field, variables, (event) => event.id);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (event) => event.id,
    serializeNode: (event, selection) => serializeOrderFulfillmentEvent(selection, event),
    pageInfoOptions: {
      includeCursors: true,
    },
  });
}

export function serializeOrderFulfillment(
  field: FieldNode,
  fulfillment: OrderFulfillmentRecord,
  variables: Record<string, unknown> = {},
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = fulfillment.id;
        break;
      case 'status':
        result[key] = fulfillment.status;
        break;
      case 'displayStatus':
        result[key] = fulfillment.displayStatus ?? fulfillment.status;
        break;
      case 'createdAt':
        result[key] = fulfillment.createdAt ?? null;
        break;
      case 'updatedAt':
        result[key] = fulfillment.updatedAt ?? null;
        break;
      case 'deliveredAt':
        result[key] = fulfillment.deliveredAt ?? null;
        break;
      case 'estimatedDeliveryAt':
        result[key] = fulfillment.estimatedDeliveryAt ?? null;
        break;
      case 'inTransitAt':
        result[key] = fulfillment.inTransitAt ?? null;
        break;
      case 'trackingInfo':
        result[key] = (
          readNullableIntArgument(selection, 'first', variables) === null
            ? (fulfillment.trackingInfo ?? [])
            : (fulfillment.trackingInfo ?? []).slice(0, readNullableIntArgument(selection, 'first', variables) ?? 0)
        ).map((trackingInfo) =>
          Object.fromEntries(
            getSelectedChildFields(selection).map((trackingSelection) => {
              const trackingKey = getFieldResponseKey(trackingSelection);
              switch (trackingSelection.name.value) {
                case 'number':
                  return [trackingKey, trackingInfo.number];
                case 'url':
                  return [trackingKey, trackingInfo.url];
                case 'company':
                  return [trackingKey, trackingInfo.company];
                default:
                  return [trackingKey, null];
              }
            }),
          ),
        );
        break;
      case 'events':
        result[key] = serializeOrderFulfillmentEventsConnection(selection, fulfillment.events ?? [], variables);
        break;
      case 'fulfillmentLineItems':
        result[key] = serializeOrderFulfillmentLineItemsConnection(selection, fulfillment.fulfillmentLineItems ?? []);
        break;
      case 'service':
        result[key] = serializeOrderFulfillmentService(selection, fulfillment.service);
        break;
      case 'location':
        result[key] = serializeOrderFulfillmentLocation(selection, fulfillment.location);
        break;
      case 'originAddress':
        result[key] = serializeOrderFulfillmentOriginAddress(selection, fulfillment.originAddress);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeOrderFulfillmentOrderLineItem(
  field: FieldNode,
  lineItem: OrderFulfillmentOrderLineItemRecord,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = lineItem.id;
        break;
      case 'totalQuantity':
        result[key] = lineItem.totalQuantity;
        break;
      case 'remainingQuantity':
        result[key] = lineItem.remainingQuantity;
        break;
      case 'lineItem':
        result[key] = lineItem.lineItemId
          ? Object.fromEntries(
              getSelectedChildFields(selection).map((lineItemSelection) => {
                const lineItemKey = getFieldResponseKey(lineItemSelection);
                switch (lineItemSelection.name.value) {
                  case 'id':
                    return [lineItemKey, lineItem.lineItemId];
                  case 'title':
                    return [lineItemKey, lineItem.title];
                  case 'quantity':
                    return [lineItemKey, lineItem.lineItemQuantity ?? lineItem.totalQuantity];
                  case 'fulfillableQuantity':
                    return [lineItemKey, lineItem.lineItemFulfillableQuantity ?? lineItem.remainingQuantity];
                  default:
                    return [lineItemKey, null];
                }
              }),
            )
          : null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeOrderFulfillmentOrderLineItemsConnection(
  field: FieldNode,
  lineItems: OrderFulfillmentOrderLineItemRecord[],
): Record<string, unknown> {
  return serializeConnection(field, {
    items: lineItems,
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: (lineItem) => lineItem.id,
    serializeNode: (lineItem, selection) => serializeOrderFulfillmentOrderLineItem(selection, lineItem),
    pageInfoOptions: {
      includeCursors: false,
    },
  });
}

function serializeOrderFulfillmentOrderMerchantRequest(
  field: FieldNode,
  fulfillmentOrder: OrderFulfillmentOrderRecord,
  merchantRequest: NonNullable<OrderFulfillmentOrderRecord['merchantRequests']>[number],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = merchantRequest.id;
        break;
      case 'kind':
        result[key] = merchantRequest.kind;
        break;
      case 'message':
        result[key] = merchantRequest.message ?? null;
        break;
      case 'requestOptions':
        result[key] = merchantRequest.requestOptions ?? {};
        break;
      case 'responseData':
        result[key] = merchantRequest.responseData ?? null;
        break;
      case 'sentAt':
        result[key] = merchantRequest.sentAt;
        break;
      case 'fulfillmentOrder':
        result[key] = serializeOrderFulfillmentOrder(selection, fulfillmentOrder);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeOrderFulfillmentOrderMerchantRequestsConnection(
  field: FieldNode,
  fulfillmentOrder: OrderFulfillmentOrderRecord,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const kind = readNullableStringArgument(field, 'kind', variables);
  const merchantRequests = (fulfillmentOrder.merchantRequests ?? []).filter(
    (merchantRequest) => kind === null || merchantRequest.kind === kind,
  );
  const window = paginateConnectionItems(merchantRequests, field, variables, (merchantRequest) => merchantRequest.id);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (merchantRequest) => merchantRequest.id,
    serializeNode: (merchantRequest, selection) =>
      serializeOrderFulfillmentOrderMerchantRequest(selection, fulfillmentOrder, merchantRequest),
    pageInfoOptions: {
      includeCursors: false,
    },
  });
}

function serializeOrderFulfillmentOrderDeliveryMethod(
  field: FieldNode,
  deliveryMethod: OrderFulfillmentOrderRecord['deliveryMethod'],
): Record<string, unknown> | null {
  if (!deliveryMethod) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = deliveryMethod.id;
        break;
      case 'methodType':
        result[key] = deliveryMethod.methodType;
        break;
      case 'presentedName':
        result[key] = deliveryMethod.presentedName ?? null;
        break;
      case 'serviceCode':
        result[key] = deliveryMethod.serviceCode ?? null;
        break;
      case 'minDeliveryDateTime':
        result[key] = deliveryMethod.minDeliveryDateTime ?? null;
        break;
      case 'maxDeliveryDateTime':
        result[key] = deliveryMethod.maxDeliveryDateTime ?? null;
        break;
      case 'sourceReference':
        result[key] = deliveryMethod.sourceReference ?? null;
        break;
      case 'additionalInformation':
      case 'brandedPromise':
        result[key] = null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

export function serializeOrderFulfillmentOrder(
  field: FieldNode,
  fulfillmentOrder: OrderFulfillmentOrderRecord,
  variables: Record<string, unknown> = {},
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = fulfillmentOrder.id;
        break;
      case 'status':
        result[key] = fulfillmentOrder.status;
        break;
      case 'requestStatus':
        result[key] = fulfillmentOrder.requestStatus ?? null;
        break;
      case 'fulfillAt':
        result[key] = fulfillmentOrder.fulfillAt ?? null;
        break;
      case 'fulfillBy':
        result[key] = fulfillmentOrder.fulfillBy ?? null;
        break;
      case 'updatedAt':
        result[key] = fulfillmentOrder.updatedAt ?? null;
        break;
      case 'supportedActions':
        result[key] = (fulfillmentOrder.supportedActions ?? []).map((action) =>
          Object.fromEntries(
            getSelectedChildFields(selection).map((actionSelection) => {
              const actionKey = getFieldResponseKey(actionSelection);
              switch (actionSelection.name.value) {
                case 'action':
                  return [actionKey, action];
                default:
                  return [actionKey, null];
              }
            }),
          ),
        );
        break;
      case 'fulfillmentHolds':
        result[key] = (fulfillmentOrder.fulfillmentHolds ?? []).map((hold) =>
          Object.fromEntries(
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
                case 'heldByRequestingApp':
                  return [holdKey, hold.heldByRequestingApp ?? null];
                case 'heldByApp':
                  return [holdKey, null];
                default:
                  return [holdKey, null];
              }
            }),
          ),
        );
        break;
      case 'assignedLocation':
        result[key] = fulfillmentOrder.assignedLocation
          ? Object.fromEntries(
              getSelectedChildFields(selection).map((locationSelection) => {
                const locationKey = getFieldResponseKey(locationSelection);
                switch (locationSelection.name.value) {
                  case 'name':
                    return [locationKey, fulfillmentOrder.assignedLocation?.name ?? null];
                  case 'location':
                    return fulfillmentOrder.assignedLocation?.locationId
                      ? [
                          locationKey,
                          Object.fromEntries(
                            getSelectedChildFields(locationSelection).map((nestedLocationSelection) => {
                              const nestedLocationKey = getFieldResponseKey(nestedLocationSelection);
                              switch (nestedLocationSelection.name.value) {
                                case 'id':
                                  return [nestedLocationKey, fulfillmentOrder.assignedLocation?.locationId ?? null];
                                case 'name':
                                  return [nestedLocationKey, fulfillmentOrder.assignedLocation?.name ?? null];
                                default:
                                  return [nestedLocationKey, null];
                              }
                            }),
                          ),
                        ]
                      : [locationKey, null];
                  default:
                    return [locationKey, null];
                }
              }),
            )
          : null;
        break;
      case 'deliveryMethod':
        result[key] = serializeOrderFulfillmentOrderDeliveryMethod(selection, fulfillmentOrder.deliveryMethod ?? null);
        break;
      case 'lineItems':
        result[key] = serializeOrderFulfillmentOrderLineItemsConnection(selection, fulfillmentOrder.lineItems ?? []);
        break;
      case 'merchantRequests':
        result[key] = serializeOrderFulfillmentOrderMerchantRequestsConnection(selection, fulfillmentOrder, variables);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

export function serializeOrderFulfillmentOrdersConnection(
  field: FieldNode,
  fulfillmentOrders: OrderFulfillmentOrderRecord[],
  variables: Record<string, unknown> = {},
  options: { includeCursors?: boolean } = {},
): Record<string, unknown> {
  const window = paginateConnectionItems(
    fulfillmentOrders,
    field,
    variables,
    (fulfillmentOrder) => fulfillmentOrder.id,
  );
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (fulfillmentOrder) => fulfillmentOrder.id,
    serializeNode: (fulfillmentOrder, selection) =>
      serializeOrderFulfillmentOrder(selection, fulfillmentOrder, variables),
    pageInfoOptions: {
      includeCursors: options.includeCursors ?? false,
    },
  });
}

export function listOrderFulfillments(orders: OrderRecord[]): OrderFulfillmentRecord[] {
  return orders.flatMap((order) => order.fulfillments ?? []);
}

export function listOrderFulfillmentOrders(orders: OrderRecord[]): OrderFulfillmentOrderRecord[] {
  return orders.flatMap((order) => order.fulfillmentOrders ?? []);
}

export function listOrderReturns(orders: OrderRecord[]): Array<{ order: OrderRecord; orderReturn: OrderReturnRecord }> {
  return orders.flatMap((order) =>
    order.returns.map((orderReturn) => ({
      order,
      orderReturn,
    })),
  );
}

function compareFulfillmentOrderIds(leftId: string, rightId: string): number {
  const leftTail = Number.parseInt(leftId.split('/').at(-1) ?? '', 10);
  const rightTail = Number.parseInt(rightId.split('/').at(-1) ?? '', 10);
  if (Number.isFinite(leftTail) && Number.isFinite(rightTail)) {
    return leftTail - rightTail;
  }

  return leftId.localeCompare(rightId);
}

function matchesFulfillmentOrderStatus(status: string | null | undefined, rawValue: string): boolean {
  const value = normalizeSearchValue(rawValue).toUpperCase();
  return status?.toUpperCase() === value;
}

function readFulfillmentOrderNumericId(fulfillmentOrder: OrderFulfillmentOrderRecord): number | null {
  const parsed = Number.parseInt(fulfillmentOrder.id.split('/').at(-1) ?? '', 10);
  return Number.isFinite(parsed) ? parsed : null;
}

function matchesFulfillmentOrderSearchTerm(
  fulfillmentOrder: OrderFulfillmentOrderRecord,
  term: SearchQueryTerm,
): boolean {
  if (term.field === null || term.field === '') {
    return (
      matchesStringValueIncludingContains(fulfillmentOrder.id, term.value) ||
      matchesStringValueIncludingContains(fulfillmentOrder.status, term.value) ||
      matchesStringValueIncludingContains(fulfillmentOrder.requestStatus, term.value)
    );
  }

  const field = term.field.toLowerCase();
  const value = searchQueryTermValue(term);

  switch (field) {
    case 'id':
      return (
        fulfillmentOrder.id === normalizeSearchValue(value) ||
        matchesNumericTerm(readFulfillmentOrderNumericId(fulfillmentOrder), value)
      );
    case 'status':
      return matchesFulfillmentOrderStatus(fulfillmentOrder.status, value);
    case 'request_status':
    case 'requeststatus':
      return matchesFulfillmentOrderStatus(fulfillmentOrder.requestStatus, value);
    default:
      return false;
  }
}

function applyFulfillmentOrdersQuery(
  fulfillmentOrders: OrderFulfillmentOrderRecord[],
  rawQuery: unknown,
): OrderFulfillmentOrderRecord[] {
  return applySearchQueryTerms(
    fulfillmentOrders,
    rawQuery,
    {
      quoteCharacters: ['"'],
      preserveQuotesInTerms: true,
      ignoredKeywords: ['AND'],
    },
    matchesFulfillmentOrderSearchTerm,
  );
}

export function sortFulfillmentOrdersForConnection(
  fulfillmentOrders: OrderFulfillmentOrderRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
): OrderFulfillmentOrderRecord[] {
  const args = getFieldArguments(field, variables);
  const sortKey = typeof args['sortKey'] === 'string' ? args['sortKey'] : 'ID';
  const reverse = args['reverse'] === true;
  const sorted = [...fulfillmentOrders].sort((left, right) => {
    switch (sortKey) {
      case 'STATUS':
        return (
          compareNullableStrings(left.status, right.status) ||
          compareNullableStrings(left.requestStatus, right.requestStatus) ||
          compareFulfillmentOrderIds(left.id, right.id)
        );
      case 'ID':
      default:
        return compareFulfillmentOrderIds(left.id, right.id);
    }
  });

  return reverse ? sorted.reverse() : sorted;
}

export function prepareTopLevelFulfillmentOrders(
  field: FieldNode,
  fulfillmentOrders: OrderFulfillmentOrderRecord[],
  variables: Record<string, unknown>,
): OrderFulfillmentOrderRecord[] {
  const args = getFieldArguments(field, variables);
  const includeClosed = args['includeClosed'] === true;
  const visibleFulfillmentOrders = includeClosed
    ? fulfillmentOrders
    : fulfillmentOrders.filter((fulfillmentOrder) => fulfillmentOrder.status !== 'CLOSED');
  return sortFulfillmentOrdersForConnection(
    applyFulfillmentOrdersQuery(visibleFulfillmentOrders, args['query']),
    field,
    variables,
  );
}

function serializeOrderTransaction(field: FieldNode, transaction: OrderTransactionRecord): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = transaction.id;
        break;
      case 'kind':
        result[key] = transaction.kind;
        break;
      case 'status':
        result[key] = transaction.status;
        break;
      case 'gateway':
        result[key] = transaction.gateway;
        break;
      case 'amountSet':
        result[key] = serializeShopMoneySet(selection, transaction.amountSet ?? null);
        break;
      case 'parentTransaction': {
        const parentTransaction = transaction.parentTransactionId
          ? findOrderTransactionById(transaction.parentTransactionId)
          : null;
        result[key] = parentTransaction ? serializeOrderTransaction(selection, parentTransaction) : null;
        break;
      }
      case 'paymentId':
        result[key] = transaction.paymentId ?? null;
        break;
      case 'paymentReferenceId':
        result[key] = transaction.paymentReferenceId ?? null;
        break;
      case 'processedAt':
        result[key] = transaction.processedAt ?? null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeOrderTransactionsConnection(
  field: FieldNode,
  transactions: OrderTransactionRecord[],
): Record<string, unknown> {
  return serializeConnection(field, {
    items: transactions,
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: (transaction) => transaction.id,
    serializeNode: (transaction, selection) => serializeOrderTransaction(selection, transaction),
    pageInfoOptions: {
      includeCursors: false,
    },
  });
}

function serializeOrderRefundLineItem(
  field: FieldNode,
  refundLineItem: OrderRefundLineItemRecord,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = refundLineItem.id;
        break;
      case 'quantity':
        result[key] = refundLineItem.quantity;
        break;
      case 'restockType':
        result[key] = refundLineItem.restockType;
        break;
      case 'lineItem':
        result[key] = Object.fromEntries(
          getSelectedChildFields(selection).map((lineItemSelection) => {
            const lineItemKey = getFieldResponseKey(lineItemSelection);
            switch (lineItemSelection.name.value) {
              case 'id':
                return [lineItemKey, refundLineItem.lineItemId];
              case 'title':
                return [lineItemKey, refundLineItem.title];
              default:
                return [lineItemKey, null];
            }
          }),
        );
        break;
      case 'subtotalSet':
        result[key] = serializeShopMoneySet(selection, refundLineItem.subtotalSet?.shopMoney ?? null);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeRefundLineItemsConnection(
  field: FieldNode,
  refundLineItems: OrderRefundLineItemRecord[],
): Record<string, unknown> {
  return serializeConnection(field, {
    items: refundLineItems,
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: (lineItem) => lineItem.id,
    serializeNode: (lineItem, selection) => serializeOrderRefundLineItem(selection, lineItem),
    pageInfoOptions: {
      includeCursors: false,
    },
  });
}

function serializeOrderRefund(field: FieldNode, refund: OrderRefundRecord): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = refund.id;
        break;
      case 'note':
        result[key] = refund.note;
        break;
      case 'createdAt':
        result[key] = refund.createdAt;
        break;
      case 'updatedAt':
        result[key] = refund.updatedAt;
        break;
      case 'totalRefundedSet':
        result[key] = serializeShopMoneySet(selection, refund.totalRefundedSet?.shopMoney ?? null);
        break;
      case 'refundLineItems':
        result[key] = serializeRefundLineItemsConnection(selection, refund.refundLineItems);
        break;
      case 'transactions':
        result[key] = serializeOrderTransactionsConnection(selection, refund.transactions);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeReturnLineItem(field: FieldNode, lineItem: OrderReturnLineItemRecord): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const processedQuantity = lineItem.processedQuantity ?? 0;
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = lineItem.id;
        break;
      case 'quantity':
      case 'refundableQuantity':
      case 'processableQuantity':
        result[key] = lineItem.quantity;
        break;
      case 'processedQuantity':
      case 'refundedQuantity':
        result[key] = processedQuantity;
        break;
      case 'unprocessedQuantity':
        result[key] = Math.max(0, lineItem.quantity - processedQuantity);
        break;
      case 'returnReason':
        result[key] = lineItem.returnReason;
        break;
      case 'returnReasonNote':
        result[key] = lineItem.returnReasonNote;
        break;
      case 'customerNote':
        result[key] = lineItem.customerNote ?? null;
        break;
      case 'fulfillmentLineItem':
        result[key] = serializeOrderFulfillmentLineItem(selection, {
          id: lineItem.fulfillmentLineItemId,
          lineItemId: lineItem.lineItemId,
          title: lineItem.title,
          quantity: lineItem.quantity,
        });
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeReturnLineItemsConnection(
  field: FieldNode,
  lineItems: OrderReturnLineItemRecord[],
): Record<string, unknown> {
  return serializeConnection(field, {
    items: lineItems,
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: (lineItem) => lineItem.id,
    serializeNode: (lineItem, selection) => serializeReturnLineItem(selection, lineItem),
    pageInfoOptions: {
      includeCursors: false,
    },
  });
}

function serializeEmptyConnection(field: FieldNode): Record<string, unknown> {
  return serializeConnection(field, {
    items: [],
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: (_item, index) => `empty:${index}`,
    serializeNode: () => ({}),
    pageInfoOptions: {
      includeCursors: false,
    },
  });
}

export function serializeOrderReturn(
  field: FieldNode,
  orderReturn: OrderReturnRecord,
  variables: Record<string, unknown> = {},
  order: OrderRecord | null = null,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = orderReturn.id;
        break;
      case 'name':
        result[key] = orderReturn.name ?? `#${orderReturn.id.split('/').at(-1) ?? 'RETURN'}`;
        break;
      case 'status':
        result[key] = orderReturn.status;
        break;
      case 'createdAt':
        result[key] = orderReturn.createdAt ?? order?.createdAt ?? makeSyntheticTimestamp();
        break;
      case 'closedAt':
        result[key] = orderReturn.closedAt ?? null;
        break;
      case 'totalQuantity':
        result[key] =
          orderReturn.totalQuantity ??
          (orderReturn.returnLineItems ?? []).reduce((total, lineItem) => total + lineItem.quantity, 0);
        break;
      case 'order':
        result[key] = order ? serializeOrderNode(selection, order, variables) : null;
        break;
      case 'returnLineItems':
        result[key] = serializeReturnLineItemsConnection(selection, orderReturn.returnLineItems ?? []);
        break;
      case 'exchangeLineItems':
      case 'refunds':
      case 'reverseFulfillmentOrders':
        result[key] = serializeEmptyConnection(selection);
        break;
      case 'returnShippingFees':
        result[key] = [];
        break;
      case 'decline':
      case 'requestApprovedAt':
      case 'suggestedFinancialOutcome':
        result[key] = null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeOrderReturnsConnection(
  field: FieldNode,
  returns: OrderReturnRecord[],
  variables: Record<string, unknown> = {},
  order: OrderRecord | null = null,
): Record<string, unknown> {
  return serializeConnection(field, {
    items: returns,
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: (orderReturn) => orderReturn.id,
    serializeNode: (orderReturn, selection) => serializeOrderReturn(selection, orderReturn, variables, order),
    pageInfoOptions: {
      includeCursors: false,
    },
  });
}

function serializeOrderShippingLinesConnection(
  field: FieldNode,
  shippingLines: OrderShippingLineRecord[],
): Record<string, unknown> {
  return serializeConnection(field, {
    items: shippingLines,
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: (_shippingLine, index) => `shipping-line:${index + 1}`,
    serializeNode: (shippingLine, selection) => serializeDraftOrderShippingLine(selection, shippingLine),
  });
}

function deriveOrderTotalShippingPriceSet(order: OrderRecord): { shopMoney: MoneyV2Record } {
  const currencyCode = readOrderCurrencyCode(order);
  const amount = order.shippingLines.reduce(
    (sum, shippingLine) => sum + parseDecimalAmount(shippingLine.originalPriceSet?.shopMoney.amount),
    0,
  );
  return {
    shopMoney: normalizeMoney(formatDecimalAmount(amount), currencyCode),
  };
}

function deriveOrderTotalReceivedSet(order: OrderRecord): { shopMoney: MoneyV2Record } {
  const currencyCode = readOrderCurrencyCode(order);
  const transactionTotal = order.transactions
    .filter((transaction) => transaction.status === 'SUCCESS')
    .reduce((sum, transaction) => sum + parseDecimalAmount(transaction.amountSet?.shopMoney.amount), 0);
  if (transactionTotal > 0) {
    return {
      shopMoney: normalizeMoney(formatDecimalAmount(transactionTotal), currencyCode),
    };
  }

  return {
    shopMoney: normalizeMoney(
      order.displayFinancialStatus === 'PAID' ? (order.currentTotalPriceSet?.shopMoney.amount ?? '0.0') : '0.0',
      currencyCode,
    ),
  };
}

function deriveOrderNetPaymentSet(order: OrderRecord): { shopMoney: MoneyV2Record } {
  return subtractMoney(
    order.totalReceivedSet ?? deriveOrderTotalReceivedSet(order),
    order.totalRefundedSet,
    readOrderCurrencyCode(order),
  );
}

export function serializeOrderNode(
  field: FieldNode,
  order: OrderRecord,
  variables: Record<string, unknown> = {},
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = order.id;
        break;
      case 'name':
        result[key] = order.name;
        break;
      case 'createdAt':
        result[key] = order.createdAt;
        break;
      case 'updatedAt':
        result[key] = order.updatedAt;
        break;
      case 'email':
        result[key] = order.email ?? order.customer?.email ?? null;
        break;
      case 'closed':
        result[key] = order.closed ?? false;
        break;
      case 'closedAt':
        result[key] = order.closedAt ?? null;
        break;
      case 'cancelledAt':
        result[key] = order.cancelledAt ?? null;
        break;
      case 'cancelReason':
        result[key] = order.cancelReason ?? null;
        break;
      case 'sourceName':
        result[key] = order.sourceName ?? null;
        break;
      case 'paymentGatewayNames':
        result[key] = structuredClone(order.paymentGatewayNames ?? []);
        break;
      case 'phone':
        result[key] = order.phone ?? null;
        break;
      case 'poNumber':
        result[key] = order.poNumber ?? null;
        break;
      case 'displayFinancialStatus':
        result[key] = order.displayFinancialStatus;
        break;
      case 'displayFulfillmentStatus':
        result[key] = order.displayFulfillmentStatus;
        break;
      case 'note':
        result[key] = order.note;
        break;
      case 'tags':
        result[key] = structuredClone(order.tags);
        break;
      case 'customAttributes':
        result[key] = serializeDraftOrderAttributes(selection, order.customAttributes);
        break;
      case 'metafield': {
        const args = getFieldArguments(selection, {});
        const namespace = typeof args['namespace'] === 'string' ? args['namespace'] : null;
        const metafieldKey = typeof args['key'] === 'string' ? args['key'] : null;
        const metafield =
          namespace && metafieldKey
            ? (order.metafields ?? []).find(
                (candidate) => candidate.namespace === namespace && candidate.key === metafieldKey,
              )
            : null;
        result[key] = metafield
          ? serializeMetafieldSelection(metafield, selection, { includeInlineFragments: true })
          : null;
        break;
      }
      case 'metafields':
        result[key] = serializeMetafieldsConnection(
          order.metafields ?? [],
          selection,
          {},
          { includeInlineFragments: true },
        );
        break;
      case 'billingAddress':
        result[key] = serializeDraftOrderAddress(selection, order.billingAddress);
        break;
      case 'shippingAddress':
        result[key] = serializeDraftOrderAddress(selection, order.shippingAddress);
        break;
      case 'subtotalPriceSet':
        result[key] = serializeShopMoneySet(selection, order.subtotalPriceSet ?? null);
        break;
      case 'currentSubtotalPriceSet':
        result[key] = serializeShopMoneySet(selection, order.currentSubtotalPriceSet ?? order.subtotalPriceSet ?? null);
        break;
      case 'currentTotalPriceSet':
        result[key] = serializeShopMoneySet(selection, order.currentTotalPriceSet ?? null);
        break;
      case 'currentTotalDiscountsSet':
        result[key] = serializeShopMoneySet(
          selection,
          order.currentTotalDiscountsSet ?? order.totalDiscountsSet ?? null,
        );
        break;
      case 'currentTotalTaxSet':
        result[key] = serializeShopMoneySet(selection, order.currentTotalTaxSet ?? order.totalTaxSet ?? null);
        break;
      case 'totalPriceSet':
        result[key] = serializeShopMoneySet(selection, order.totalPriceSet ?? null);
        break;
      case 'totalOutstandingSet':
        result[key] = serializeShopMoneySet(
          selection,
          (order.totalOutstandingSet ?? order.currentTotalPriceSet)?.shopMoney ?? null,
        );
        break;
      case 'capturable':
        result[key] = order.capturable ?? totalCapturableAmount(order) > 0;
        break;
      case 'totalCapturable':
        result[key] = formatDecimalAmount(
          parseDecimalAmount(order.totalCapturableSet?.shopMoney.amount ?? totalCapturableAmount(order)),
        );
        break;
      case 'totalCapturableSet':
        result[key] = serializeShopMoneySet(
          selection,
          order.totalCapturableSet ?? makeOrderMoneyBag(totalCapturableAmount(order), readOrderCurrencyCode(order)),
        );
        break;
      case 'totalReceivedSet':
        result[key] = serializeShopMoneySet(selection, order.totalReceivedSet ?? deriveOrderTotalReceivedSet(order));
        break;
      case 'netPaymentSet':
        result[key] = serializeShopMoneySet(selection, order.netPaymentSet ?? deriveOrderNetPaymentSet(order));
        break;
      case 'totalRefundedSet':
        result[key] = serializeShopMoneySet(selection, order.totalRefundedSet ?? null);
        break;
      case 'totalRefundedShippingSet':
        result[key] = serializeShopMoneySet(
          selection,
          order.totalRefundedShippingSet ?? normalizeZeroMoneyBag(readOrderCurrencyCode(order)),
        );
        break;
      case 'totalShippingPriceSet':
        result[key] = serializeShopMoneySet(
          selection,
          order.totalShippingPriceSet ?? deriveOrderTotalShippingPriceSet(order),
        );
        break;
      case 'totalTaxSet':
        result[key] = serializeShopMoneySet(selection, order.totalTaxSet ?? null);
        break;
      case 'totalDiscountsSet':
        result[key] = serializeShopMoneySet(selection, order.totalDiscountsSet ?? null);
        break;
      case 'discountCodes':
        result[key] = structuredClone(order.discountCodes ?? []);
        break;
      case 'discountApplications':
        result[key] = serializeOrderDiscountApplicationsConnection(selection, order.discountApplications ?? []);
        break;
      case 'taxLines':
        result[key] = serializeOrderTaxLines(selection, order.taxLines ?? []);
        break;
      case 'taxesIncluded':
        result[key] = order.taxesIncluded ?? false;
        break;
      case 'customer':
        result[key] = serializeOrderCustomer(selection, order.customer);
        break;
      case 'paymentTerms':
        result[key] = serializeDraftOrderPaymentTerms(selection, order.paymentTerms ?? null);
        break;
      case 'shippingLines':
        result[key] = serializeOrderShippingLinesConnection(selection, order.shippingLines);
        break;
      case 'lineItems':
        result[key] = serializeOrderLineItemsConnection(selection, order.lineItems);
        break;
      case 'fulfillments':
        result[key] = (order.fulfillments ?? []).map((fulfillment) =>
          serializeOrderFulfillment(selection, fulfillment, variables),
        );
        break;
      case 'fulfillmentOrders':
        result[key] = serializeOrderFulfillmentOrdersConnection(selection, order.fulfillmentOrders ?? []);
        break;
      case 'transactions':
        result[key] = order.transactions.map((transaction) => serializeOrderTransaction(selection, transaction));
        break;
      case 'refunds':
        result[key] = order.refunds.map((refund) => serializeOrderRefund(selection, refund));
        break;
      case 'returns':
        result[key] = serializeOrderReturnsConnection(selection, order.returns, variables, order);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

export function serializeCalculatedOrder(
  field: FieldNode,
  calculatedOrder: CalculatedOrderRecord,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const originalOrder = store.getOrderById(calculatedOrder.originalOrderId);

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'originalOrder':
        result[key] = originalOrder ? serializeOrderNode(selection, originalOrder) : null;
        break;
      case 'addedLineItems':
        result[key] = serializeOrderLineItemsConnection(
          selection,
          calculatedOrder.lineItems.filter((lineItem) => lineItem.isAdded === true),
        );
        break;
      default:
        result[key] = serializeOrderNode(
          {
            ...field,
            selectionSet: {
              kind: Kind.SELECTION_SET,
              selections: [selection],
            },
          },
          calculatedOrder,
        )[key];
        break;
    }
  }

  return result;
}

function readRawConnectionItems(raw: unknown): Record<string, unknown>[] {
  if (Array.isArray(raw)) {
    return raw.filter((item): item is Record<string, unknown> => typeof item === 'object' && item !== null);
  }

  if (typeof raw !== 'object' || raw === null) {
    return [];
  }

  const source = raw as Record<string, unknown>;
  if (Array.isArray(source['nodes'])) {
    return source['nodes'].filter((item): item is Record<string, unknown> => typeof item === 'object' && item !== null);
  }

  if (Array.isArray(source['edges'])) {
    return source['edges']
      .map((edge) =>
        typeof edge === 'object' && edge !== null ? ((edge as Record<string, unknown>)['node'] ?? null) : null,
      )
      .filter((item): item is Record<string, unknown> => typeof item === 'object' && item !== null);
  }

  return [];
}

function serializeRawRecordConnection(
  field: FieldNode,
  items: Record<string, unknown>[],
  variables: Record<string, unknown>,
  fragments: ReturnType<typeof getDocumentFragments>,
): Record<string, unknown> {
  const {
    items: visibleRecords,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(items, field, variables, (item, index) =>
    typeof item['id'] === 'string' ? item['id'] : String(index),
  );

  return serializeConnection(field, {
    items: visibleRecords,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (item, index) => (typeof item['id'] === 'string' ? item['id'] : String(index)),
    serializeNode: (item, selection) => projectGraphqlObject(item, selection.selectionSet?.selections ?? [], fragments),
    selectedFieldOptions: { includeInlineFragments: true },
    pageInfoOptions: { includeInlineFragments: true },
  });
}

function serializeAbandonedCheckoutNode(
  field: FieldNode,
  checkout: AbandonedCheckoutRecord,
  variables: Record<string, unknown>,
  fragments: ReturnType<typeof getDocumentFragments>,
): Record<string, unknown> {
  return projectGraphqlObject(checkout.data, field.selectionSet?.selections ?? [], fragments, {
    projectFieldValue: ({ source, field: selection, fieldName }) => {
      if (fieldName === 'lineItems') {
        return {
          handled: true,
          value: serializeRawRecordConnection(
            selection,
            readRawConnectionItems(source[fieldName]),
            variables,
            fragments,
          ),
        };
      }
      return { handled: false };
    },
  });
}

export function serializeAbandonedCheckoutsConnection(
  field: FieldNode,
  checkouts: AbandonedCheckoutRecord[],
  variables: Record<string, unknown>,
  fragments: ReturnType<typeof getDocumentFragments>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const filteredCheckouts = typeof args['savedSearchId'] === 'string' && args['savedSearchId'].trim() ? [] : checkouts;
  const orderedCheckouts = args['reverse'] === false ? [...filteredCheckouts].reverse() : filteredCheckouts;
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    orderedCheckouts,
    field,
    variables,
    (checkout) => checkout.cursor ?? checkout.id,
    {
      parseCursor: (cursor) => cursor.replace(/^cursor:/u, ''),
    },
  );

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (checkout) => checkout.cursor ?? checkout.id,
    serializeNode: (checkout, selection) => serializeAbandonedCheckoutNode(selection, checkout, variables, fragments),
    selectedFieldOptions: { includeInlineFragments: true },
    pageInfoOptions: { includeInlineFragments: true, prefixCursors: false },
  });
}

export function serializeAbandonedCheckoutsCount(
  field: FieldNode,
  checkouts: AbandonedCheckoutRecord[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const filteredCheckouts = typeof args['savedSearchId'] === 'string' && args['savedSearchId'].trim() ? [] : checkouts;
  const rawLimit = args['limit'];
  const limit = typeof rawLimit === 'number' && Number.isFinite(rawLimit) && rawLimit >= 0 ? rawLimit : null;
  const count = limit === null ? filteredCheckouts.length : Math.min(filteredCheckouts.length, limit);

  return serializeOrderCount(field, count, limit !== null && filteredCheckouts.length > limit ? 'AT_LEAST' : 'EXACT');
}

export function serializeAbandonmentNode(
  field: FieldNode,
  abandonment: AbandonmentRecord,
  variables: Record<string, unknown>,
  fragments: ReturnType<typeof getDocumentFragments>,
): Record<string, unknown> {
  return projectGraphqlObject(abandonment.data, field.selectionSet?.selections ?? [], fragments, {
    projectFieldValue: ({ source, field: selection, fieldName }) => {
      if (fieldName === 'abandonedCheckoutPayload') {
        const checkoutId =
          typeof source[fieldName] === 'object' && source[fieldName] !== null
            ? ((source[fieldName] as Record<string, unknown>)['id'] as unknown)
            : abandonment.abandonedCheckoutId;
        const checkout = typeof checkoutId === 'string' ? store.getAbandonedCheckoutById(checkoutId) : null;
        return {
          handled: true,
          value: checkout ? serializeAbandonedCheckoutNode(selection, checkout, variables, fragments) : null,
        };
      }
      if (fieldName === 'productsAddedToCart' || fieldName === 'productsViewed') {
        return {
          handled: true,
          value: serializeRawRecordConnection(
            selection,
            readRawConnectionItems(source[fieldName]),
            variables,
            fragments,
          ),
        };
      }
      return { handled: false };
    },
  });
}

function serializeOrderCount(field: FieldNode, count = 0, precision = 'EXACT'): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
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

export function serializeOrdersConnection(
  field: FieldNode,
  orders: OrderRecord[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const filteredOrders =
    typeof args['savedSearchId'] === 'string' && args['savedSearchId'].trim()
      ? []
      : applyOrdersQuery(orders, args['query']);
  const orderedOrders = sortOrdersForConnection(filteredOrders, field, variables);
  const { visibleRecords, hasNextPage, hasPreviousPage } = applySyntheticCursorWindow(orderedOrders, field, variables);
  const result = serializeConnection(field, {
    items: visibleRecords,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (order) => buildSyntheticCursor(order.id),
    serializeNode: (order, selection) => serializeOrderNode(selection, order),
    selectedFieldOptions: { includeInlineFragments: true },
    pageInfoOptions: { prefixCursors: false, includeInlineFragments: true },
  });

  return result;
}

export function serializeOrdersCount(
  field: FieldNode,
  orders: OrderRecord[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const filteredOrders =
    typeof args['savedSearchId'] === 'string' && args['savedSearchId'].trim()
      ? []
      : applyOrdersQuery(orders, args['query']);
  const rawLimit = args['limit'];
  const limit = typeof rawLimit === 'number' && Number.isFinite(rawLimit) && rawLimit >= 0 ? rawLimit : null;
  const count = limit === null ? filteredOrders.length : Math.min(filteredOrders.length, limit);

  return serializeOrderCount(field, count, limit !== null && filteredOrders.length > limit ? 'AT_LEAST' : 'EXACT');
}

function readOrderNumericId(order: OrderRecord): number | null {
  const parsed = Number.parseInt(order.id.split('/').at(-1) ?? '', 10);
  return Number.isFinite(parsed) ? parsed : null;
}

function normalizeOrderStatusValue(rawValue: string): string {
  return normalizeSearchValue(rawValue).replace(/-/gu, '_').toUpperCase();
}

function matchesOrderStatus(candidate: string | null | undefined, rawValue: string): boolean {
  if (!candidate) {
    return false;
  }

  const normalizedCandidate = candidate.replace(/-/gu, '_').toUpperCase();
  const normalizedValue = normalizeOrderStatusValue(rawValue);
  return normalizedCandidate === normalizedValue;
}

function matchesOrderFulfillmentStatus(order: OrderRecord, rawValue: string): boolean {
  const normalizedValue = normalizeOrderStatusValue(rawValue);
  const normalizedCandidate = order.displayFulfillmentStatus?.replace(/-/gu, '_').toUpperCase() ?? null;
  if (normalizedValue === 'UNSHIPPED') {
    return normalizedCandidate === 'UNFULFILLED';
  }
  return normalizedCandidate === normalizedValue;
}

function matchesOrderLifecycleStatus(order: OrderRecord, rawValue: string): boolean {
  const value = normalizeSearchValue(rawValue).toLowerCase();
  switch (value) {
    case 'open':
      return order.cancelledAt === null && order.closedAt === null && order.closed !== true;
    case 'closed':
      return order.closed === true || order.closedAt !== null;
    case 'cancelled':
    case 'canceled':
      return order.cancelledAt !== null;
    case 'not_closed':
      return order.closed !== true && order.closedAt === null;
    default:
      return false;
  }
}

function matchesOrderSearchTerm(order: OrderRecord, term: SearchQueryTerm): boolean {
  if (term.field === null || term.field === '') {
    return (
      matchesStringValueIncludingContains(order.name, term.value) ||
      matchesStringValueIncludingContains(order.email ?? order.customer?.email, term.value) ||
      matchesStringValueIncludingContains(order.note, term.value) ||
      order.tags.some((tag) => matchesStringValueIncludingContains(tag, term.value))
    );
  }

  const field = term.field.toLowerCase();
  const value = searchQueryTermValue(term);

  switch (field) {
    case 'name':
      return matchesStringValueIncludingContains(order.name.replace(/^#/u, ''), value);
    case 'tag':
      return order.tags.some((tag) => matchesStringValue(tag, normalizeSearchValue(value)));
    case 'tag_not':
      return !order.tags.some((tag) => matchesStringValue(tag, normalizeSearchValue(value)));
    case 'email':
      return matchesStringValueIncludingContains(order.email ?? order.customer?.email, value);
    case 'financial_status':
      return matchesOrderStatus(order.displayFinancialStatus, value);
    case 'fulfillment_status':
      return matchesOrderFulfillmentStatus(order, value);
    case 'status':
      return matchesOrderLifecycleStatus(order, value);
    case 'id':
      return order.id === normalizeSearchValue(value) || matchesNumericTerm(readOrderNumericId(order), value);
    case 'created_at':
      return matchesTimestampTerm(order.createdAt, value);
    case 'updated_at':
    case 'processed_at':
      return matchesTimestampTerm(order.updatedAt, value);
    case 'customer_id':
      return (
        order.customer?.id === normalizeSearchValue(value) || matchesNumericTerm(readCustomerNumericId(order), value)
      );
    case 'po_number':
      return matchesStringValueIncludingContains(order.poNumber, value);
    case 'source_name':
    case 'source':
      return matchesStringValue(order.sourceName ?? '', normalizeSearchValue(value));
    case 'gateway':
      return (order.paymentGatewayNames ?? []).some((gateway) => matchesStringValueIncludingContains(gateway, value));
    case 'sku':
      return order.lineItems.some((lineItem) => matchesStringValueIncludingContains(lineItem.sku, value));
    case 'discount_code':
      return (order.discountCodes ?? []).some((discountCode) =>
        matchesStringValueIncludingContains(discountCode, value),
      );
    default:
      return false;
  }
}

function readCustomerNumericId(order: OrderRecord): number | null {
  const parsed = Number.parseInt(order.customer?.id.split('/').at(-1) ?? '', 10);
  return Number.isFinite(parsed) ? parsed : null;
}

function applyOrdersQuery(orders: OrderRecord[], rawQuery: unknown): OrderRecord[] {
  return applySearchQueryTerms(
    orders,
    rawQuery,
    {
      quoteCharacters: ['"'],
      preserveQuotesInTerms: true,
      ignoredKeywords: ['AND'],
    },
    matchesOrderSearchTerm,
  );
}

function compareOrderIds(leftId: string, rightId: string): number {
  const leftTail = Number.parseInt(leftId.split('/').at(-1) ?? '', 10);
  const rightTail = Number.parseInt(rightId.split('/').at(-1) ?? '', 10);
  if (Number.isFinite(leftTail) && Number.isFinite(rightTail)) {
    return leftTail - rightTail;
  }

  return leftId.localeCompare(rightId);
}

function compareNullableStrings(left: string | null | undefined, right: string | null | undefined): number {
  return (left ?? '').localeCompare(right ?? '');
}

function readMoneyAmount(raw: { shopMoney: MoneyV2Record | null } | null | undefined): number {
  return parseDecimalAmount(raw?.shopMoney?.amount);
}

function sortOrdersForConnection(
  orders: OrderRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
): OrderRecord[] {
  const args = getFieldArguments(field, variables);
  const sortKey = typeof args['sortKey'] === 'string' ? args['sortKey'] : null;
  const reverse = args['reverse'] === true;

  if (!sortKey) {
    return reverse ? [...orders].reverse() : orders;
  }

  const sorted = [...orders].sort((left, right) => {
    switch (sortKey) {
      case 'CREATED_AT':
      case 'PROCESSED_AT':
        return left.createdAt.localeCompare(right.createdAt) || compareOrderIds(left.id, right.id);
      case 'UPDATED_AT':
        return left.updatedAt.localeCompare(right.updatedAt) || compareOrderIds(left.id, right.id);
      case 'NAME':
        return left.name.localeCompare(right.name) || compareOrderIds(left.id, right.id);
      case 'TOTAL_PRICE':
        return (
          readMoneyAmount(left.currentTotalPriceSet ?? left.totalPriceSet) -
            readMoneyAmount(right.currentTotalPriceSet ?? right.totalPriceSet) || compareOrderIds(left.id, right.id)
        );
      case 'FINANCIAL_STATUS':
        return (
          compareNullableStrings(left.displayFinancialStatus, right.displayFinancialStatus) ||
          compareOrderIds(left.id, right.id)
        );
      case 'FULFILLMENT_STATUS':
        return (
          compareNullableStrings(left.displayFulfillmentStatus, right.displayFulfillmentStatus) ||
          compareOrderIds(left.id, right.id)
        );
      case 'ID':
      default:
        return compareOrderIds(left.id, right.id);
    }
  });

  return reverse ? sorted.reverse() : sorted;
}
