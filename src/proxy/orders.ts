import { Kind, type FieldNode, type ObjectValueNode } from 'graphql';

import type { ReadMode } from '../config.js';
import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { store } from '../state/store.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import type {
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
  OrderCustomerRecord,
  OrderLineItemRecord,
  OrderRecord,
  OrderShippingLineRecord,
} from '../state/types.js';

function getFieldResponseKey(field: FieldNode): string {
  return field.alias?.value ?? field.name.value;
}

function getSelectedChildFields(field: FieldNode): FieldNode[] {
  return (field.selectionSet?.selections ?? []).filter(
    (selection): selection is FieldNode => selection.kind === Kind.FIELD,
  );
}

function normalizeMoney(amount: string | null, currencyCode: string | null): MoneyV2Record {
  return {
    amount,
    currencyCode,
  };
}

function parseDecimalAmount(raw: unknown): number {
  const numeric = typeof raw === 'number' ? raw : Number.parseFloat(typeof raw === 'string' ? raw : '0');
  return Number.isFinite(numeric) ? numeric : 0;
}

function formatDecimalAmount(value: number): string {
  return value.toFixed(1);
}

function readString(raw: unknown): string | null {
  return typeof raw === 'string' && raw.length > 0 ? raw : null;
}

function readBoolean(raw: unknown, fallback: boolean): boolean {
  return typeof raw === 'boolean' ? raw : fallback;
}

function normalizeDraftOrderAddress(raw: unknown): DraftOrderAddressRecord | null {
  if (typeof raw !== 'object' || raw === null) {
    return null;
  }

  const address = raw as Record<string, unknown>;
  const countryCode =
    typeof address['countryCodeV2'] === 'string'
      ? address['countryCodeV2']
      : typeof address['countryCode'] === 'string'
        ? address['countryCode']
        : null;

  return {
    firstName: typeof address['firstName'] === 'string' ? address['firstName'] : null,
    lastName: typeof address['lastName'] === 'string' ? address['lastName'] : null,
    address1: typeof address['address1'] === 'string' ? address['address1'] : null,
    city: typeof address['city'] === 'string' ? address['city'] : null,
    provinceCode: typeof address['provinceCode'] === 'string' ? address['provinceCode'] : null,
    countryCodeV2: countryCode,
    zip: typeof address['zip'] === 'string' ? address['zip'] : null,
    phone: typeof address['phone'] === 'string' ? address['phone'] : null,
  };
}

function normalizeDraftOrderAttributes(raw: unknown): DraftOrderAttributeRecord[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .filter((attribute): attribute is Record<string, unknown> => typeof attribute === 'object' && attribute !== null)
    .map((attribute) => ({
      key: typeof attribute['key'] === 'string' ? attribute['key'] : '',
      value: typeof attribute['value'] === 'string' ? attribute['value'] : null,
    }))
    .filter((attribute) => attribute.key.length > 0);
}

function normalizeDraftOrderAppliedDiscount(
  raw: unknown,
  currencyCode: string,
): DraftOrderAppliedDiscountRecord | null {
  if (typeof raw !== 'object' || raw === null) {
    return null;
  }

  const discount = raw as Record<string, unknown>;
  const value = parseDecimalAmount(discount['value']);
  const amount = formatDecimalAmount(parseDecimalAmount(discount['amount']));

  return {
    title: readString(discount['title']),
    description: readString(discount['description']),
    value,
    valueType: readString(discount['valueType']),
    amountSet: {
      shopMoney: normalizeMoney(amount, currencyCode),
    },
  };
}

function calculateDraftOrderDiscountAmount(
  discount: DraftOrderAppliedDiscountRecord | null,
  basisAmount: number,
): number {
  if (!discount) {
    return 0;
  }

  if (discount.valueType === 'PERCENTAGE') {
    return (basisAmount * (discount.value ?? 0)) / 100;
  }

  return parseDecimalAmount(discount.amountSet?.shopMoney.amount);
}

function normalizeDraftOrderShippingLine(raw: unknown, currencyCode: string): DraftOrderShippingLineRecord | null {
  if (typeof raw !== 'object' || raw === null) {
    return null;
  }

  const shippingLine = raw as Record<string, unknown>;
  const price = shippingLine['price'];
  if (price === undefined || price === null) {
    return null;
  }

  return {
    title: readString(shippingLine['title']),
    code: readString(shippingLine['code']),
    originalPriceSet: {
      shopMoney: normalizeMoney(formatDecimalAmount(parseDecimalAmount(price)), currencyCode),
    },
  };
}

function buildDraftOrderCustomerFromInput(inputRecord: Record<string, unknown>): DraftOrderCustomerRecord | null {
  const email = readString(inputRecord['email']);
  const customerId = readString(inputRecord['customerId']);
  const billingAddress = normalizeDraftOrderAddress(inputRecord['billingAddress']);
  const shippingAddress = normalizeDraftOrderAddress(inputRecord['shippingAddress']);
  const firstName = billingAddress?.firstName ?? shippingAddress?.firstName ?? null;
  const lastName = billingAddress?.lastName ?? shippingAddress?.lastName ?? null;
  const displayName = [firstName, lastName]
    .filter((value): value is string => Boolean(value && value.length > 0))
    .join(' ')
    .trim();

  if (!customerId && !email && !displayName) {
    return null;
  }

  return {
    id: customerId,
    email,
    displayName: displayName.length > 0 ? displayName : email,
  };
}

function normalizeDraftOrderPaymentTerms(raw: unknown): DraftOrderPaymentTermsRecord | null {
  if (typeof raw !== 'object' || raw === null) {
    return null;
  }

  const paymentTerms = raw as Record<string, unknown>;
  const schedules = Array.isArray(paymentTerms['paymentSchedules']) ? paymentTerms['paymentSchedules'] : [];
  const firstSchedule = schedules.find(
    (schedule): schedule is Record<string, unknown> => typeof schedule === 'object' && schedule !== null,
  );
  const hasDueAt = typeof firstSchedule?.['dueAt'] === 'string';
  const hasIssuedAt = typeof firstSchedule?.['issuedAt'] === 'string';
  const name = hasDueAt ? 'Due on date' : hasIssuedAt ? 'Net terms' : 'Custom payment terms';

  return {
    id: makeSyntheticGid('PaymentTerms'),
    due: false,
    overdue: false,
    dueInDays: null,
    paymentTermsName: name,
    paymentTermsType: hasDueAt ? 'FIXED' : hasIssuedAt ? 'NET' : 'UNKNOWN',
    translatedName: name,
  };
}

function normalizeDraftOrderLineItems(raw: unknown, currencyCode: string): DraftOrderLineItemRecord[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .filter((lineItem): lineItem is Record<string, unknown> => typeof lineItem === 'object' && lineItem !== null)
    .map((lineItem) => {
      const variantId = readString(lineItem['variantId']);
      const variant = variantId ? store.getEffectiveVariantById(variantId) : null;
      const product = variant ? store.getEffectiveProductById(variant.productId) : null;
      const isVariantLine = Boolean(variant);
      const quantity = typeof lineItem['quantity'] === 'number' ? lineItem['quantity'] : 0;
      const rawUnitPrice = isVariantLine ? variant?.price : lineItem['originalUnitPrice'];
      const unitPrice = parseDecimalAmount(rawUnitPrice);
      const grossTotal = unitPrice * quantity;
      const appliedDiscount = normalizeDraftOrderAppliedDiscount(lineItem['appliedDiscount'], currencyCode);
      const discountTotal = calculateDraftOrderDiscountAmount(appliedDiscount, grossTotal);
      const discountedTotal = Math.max(0, grossTotal - discountTotal);
      const price = formatDecimalAmount(unitPrice);
      return {
        id: makeSyntheticGid('DraftOrderLineItem'),
        title: isVariantLine ? (product?.title ?? variant?.title ?? null) : readString(lineItem['title']),
        name: isVariantLine ? (product?.title ?? variant?.title ?? null) : readString(lineItem['title']),
        quantity,
        sku: isVariantLine ? (variant?.sku ?? null) : readString(lineItem['sku']),
        variantTitle: isVariantLine ? (variant?.title ?? null) : null,
        variantId: variant?.id ?? null,
        productId: product?.id ?? variant?.productId ?? null,
        custom: !isVariantLine,
        requiresShipping: readBoolean(lineItem['requiresShipping'], variant?.inventoryItem?.requiresShipping ?? true),
        taxable: readBoolean(lineItem['taxable'], variant?.taxable ?? true),
        customAttributes: normalizeDraftOrderAttributes(lineItem['customAttributes']),
        appliedDiscount,
        originalUnitPriceSet: {
          shopMoney: normalizeMoney(price, currencyCode),
        },
        originalTotalSet: {
          shopMoney: normalizeMoney(formatDecimalAmount(grossTotal), currencyCode),
        },
        discountedTotalSet: {
          shopMoney: normalizeMoney(formatDecimalAmount(discountedTotal), currencyCode),
        },
        totalDiscountSet: {
          shopMoney: normalizeMoney(formatDecimalAmount(discountTotal), currencyCode),
        },
      };
    });
}

function normalizeOrderLineItems(raw: unknown, currencyCode: string): OrderLineItemRecord[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .filter((lineItem): lineItem is Record<string, unknown> => typeof lineItem === 'object' && lineItem !== null)
    .map((lineItem) => {
      const originalUnitPriceSet =
        typeof lineItem['originalUnitPriceSet'] === 'object' && lineItem['originalUnitPriceSet'] !== null
          ? (lineItem['originalUnitPriceSet'] as Record<string, unknown>)
          : {};
      const shopMoney =
        typeof originalUnitPriceSet['shopMoney'] === 'object' && originalUnitPriceSet['shopMoney'] !== null
          ? (originalUnitPriceSet['shopMoney'] as Record<string, unknown>)
          : {};
      const price = formatDecimalAmount(parseDecimalAmount(shopMoney['amount']));

      return {
        id: makeSyntheticGid('LineItem'),
        title: typeof lineItem['title'] === 'string' ? lineItem['title'] : null,
        quantity: typeof lineItem['quantity'] === 'number' ? lineItem['quantity'] : 0,
        sku: typeof lineItem['sku'] === 'string' ? lineItem['sku'] : null,
        variantTitle: null,
        originalUnitPriceSet: {
          shopMoney: normalizeMoney(
            price,
            typeof shopMoney['currencyCode'] === 'string' ? shopMoney['currencyCode'] : currencyCode,
          ),
        },
      };
    });
}

function normalizeOrderShippingLines(raw: unknown, currencyCode: string): OrderShippingLineRecord[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .filter(
      (shippingLine): shippingLine is Record<string, unknown> =>
        typeof shippingLine === 'object' && shippingLine !== null,
    )
    .map((shippingLine) => {
      const priceSet =
        typeof shippingLine['priceSet'] === 'object' && shippingLine['priceSet'] !== null
          ? (shippingLine['priceSet'] as Record<string, unknown>)
          : {};
      const shopMoney =
        typeof priceSet['shopMoney'] === 'object' && priceSet['shopMoney'] !== null
          ? (priceSet['shopMoney'] as Record<string, unknown>)
          : {};
      const amount = formatDecimalAmount(parseDecimalAmount(shopMoney['amount']));

      return {
        title: typeof shippingLine['title'] === 'string' ? shippingLine['title'] : null,
        code: typeof shippingLine['code'] === 'string' ? shippingLine['code'] : null,
        originalPriceSet: {
          shopMoney: normalizeMoney(
            amount,
            typeof shopMoney['currencyCode'] === 'string' ? shopMoney['currencyCode'] : currencyCode,
          ),
        },
      };
    });
}

function buildOrderCustomerFromInput(inputRecord: Record<string, unknown>): OrderCustomerRecord | null {
  const email = typeof inputRecord['email'] === 'string' ? inputRecord['email'] : null;
  const billingAddress = normalizeDraftOrderAddress(inputRecord['billingAddress']);
  const firstName = billingAddress?.firstName ?? null;
  const lastName = billingAddress?.lastName ?? null;
  const displayName = [firstName, lastName]
    .filter((value): value is string => Boolean(value && value.length > 0))
    .join(' ')
    .trim();

  if (!email && !displayName) {
    return null;
  }

  return {
    id: makeSyntheticGid('Customer'),
    email,
    displayName: displayName.length > 0 ? displayName : email,
  };
}

function buildOrderFromInput(input: unknown): OrderRecord {
  const inputRecord = typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : {};
  const currencyCode = 'CAD';
  const orderId = makeSyntheticGid('Order');
  const createdAt = makeSyntheticTimestamp();
  const lineItems = normalizeOrderLineItems(inputRecord['lineItems'], currencyCode);
  const shippingLines = normalizeOrderShippingLines(inputRecord['shippingLines'], currencyCode);
  const subtotal = formatDecimalAmount(
    lineItems.reduce(
      (sum, lineItem) => sum + parseDecimalAmount(lineItem.originalUnitPriceSet?.shopMoney.amount) * lineItem.quantity,
      0,
    ),
  );
  const shippingTotal = formatDecimalAmount(
    shippingLines.reduce(
      (sum, shippingLine) => sum + parseDecimalAmount(shippingLine.originalPriceSet?.shopMoney.amount),
      0,
    ),
  );
  const total = formatDecimalAmount(parseDecimalAmount(subtotal) + parseDecimalAmount(shippingTotal));
  const transactions = Array.isArray(inputRecord['transactions']) ? inputRecord['transactions'] : [];
  const hasSuccessfulTransaction = transactions.some((transaction) => {
    if (typeof transaction !== 'object' || transaction === null) {
      return false;
    }

    return (transaction as Record<string, unknown>)['status'] === 'SUCCESS';
  });

  return {
    id: orderId,
    name: `#${store.getOrders().length + 1}`,
    createdAt,
    updatedAt: createdAt,
    displayFinancialStatus: hasSuccessfulTransaction ? 'PAID' : 'PENDING',
    displayFulfillmentStatus: 'UNFULFILLED',
    note: typeof inputRecord['note'] === 'string' ? inputRecord['note'] : null,
    tags: Array.isArray(inputRecord['tags'])
      ? inputRecord['tags']
          .filter((tag): tag is string => typeof tag['toString'] === 'function' && typeof tag === 'string')
          .sort((left, right) => left.localeCompare(right))
      : [],
    customAttributes: normalizeDraftOrderAttributes(inputRecord['customAttributes']),
    billingAddress: normalizeDraftOrderAddress(inputRecord['billingAddress']),
    shippingAddress: normalizeDraftOrderAddress(inputRecord['shippingAddress']),
    subtotalPriceSet: {
      shopMoney: normalizeMoney(subtotal, currencyCode),
    },
    currentTotalPriceSet: {
      shopMoney: normalizeMoney(total, currencyCode),
    },
    totalPriceSet: {
      shopMoney: normalizeMoney(total, currencyCode),
    },
    customer: buildOrderCustomerFromInput(inputRecord),
    shippingLines,
    lineItems,
  };
}

function buildDraftOrderFromInput(input: unknown, shopifyAdminOrigin: string): DraftOrderRecord {
  const inputRecord = typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : {};
  const currencyCode = 'CAD';
  const draftOrderId = makeSyntheticGid('DraftOrder');
  const createdAt = makeSyntheticTimestamp();
  const lineItems = normalizeDraftOrderLineItems(inputRecord['lineItems'], currencyCode);
  const shippingLine = normalizeDraftOrderShippingLine(inputRecord['shippingLine'], currencyCode);
  const appliedDiscount = normalizeDraftOrderAppliedDiscount(inputRecord['appliedDiscount'], currencyCode);
  const lineDiscountTotal = lineItems.reduce(
    (sum, lineItem) => sum + parseDecimalAmount(lineItem.totalDiscountSet?.shopMoney.amount),
    0,
  );
  const subtotal = formatDecimalAmount(
    lineItems.reduce((sum, lineItem) => sum + parseDecimalAmount(lineItem.discountedTotalSet?.shopMoney.amount), 0),
  );
  const orderDiscountTotal = calculateDraftOrderDiscountAmount(appliedDiscount, parseDecimalAmount(subtotal));
  const shippingTotal = parseDecimalAmount(shippingLine?.originalPriceSet?.shopMoney.amount);
  const totalDiscount = formatDecimalAmount(lineDiscountTotal + orderDiscountTotal);
  const totalShipping = formatDecimalAmount(shippingTotal);
  const total = formatDecimalAmount(Math.max(0, parseDecimalAmount(subtotal) - orderDiscountTotal) + shippingTotal);
  const name = `#D${store.getDraftOrders().length + 1}`;
  const invoiceId = draftOrderId.split('/').at(-1) ?? 'draft-order';

  return {
    id: draftOrderId,
    name,
    invoiceUrl: `${shopifyAdminOrigin.replace(/\/$/, '')}/draft_orders/${invoiceId}/invoice`,
    status: 'OPEN',
    ready: false,
    email: typeof inputRecord['email'] === 'string' ? inputRecord['email'] : null,
    note: typeof inputRecord['note'] === 'string' ? inputRecord['note'] : null,
    tags: Array.isArray(inputRecord['tags'])
      ? inputRecord['tags']
          .filter((tag): tag is string => typeof tag === 'string')
          .sort((left, right) => left.localeCompare(right))
      : [],
    customer: buildDraftOrderCustomerFromInput(inputRecord),
    taxExempt: readBoolean(inputRecord['taxExempt'], false),
    taxesIncluded: readBoolean(inputRecord['taxesIncluded'], false),
    reserveInventoryUntil: readString(inputRecord['reserveInventoryUntil']),
    paymentTerms: normalizeDraftOrderPaymentTerms(inputRecord['paymentTerms']),
    appliedDiscount,
    customAttributes: normalizeDraftOrderAttributes(inputRecord['customAttributes']),
    billingAddress: normalizeDraftOrderAddress(inputRecord['billingAddress']),
    shippingAddress: normalizeDraftOrderAddress(inputRecord['shippingAddress']),
    shippingLine,
    createdAt,
    updatedAt: createdAt,
    subtotalPriceSet: {
      shopMoney: normalizeMoney(subtotal, currencyCode),
    },
    totalDiscountsSet: {
      shopMoney: normalizeMoney(totalDiscount, currencyCode),
    },
    totalShippingPriceSet: {
      shopMoney: normalizeMoney(totalShipping, currencyCode),
    },
    totalPriceSet: {
      shopMoney: normalizeMoney(total, currencyCode),
    },
    lineItems,
  };
}

function cloneOrderLineItemsForCalculatedOrder(order: OrderRecord): OrderLineItemRecord[] {
  return order.lineItems.map((lineItem) => ({
    ...structuredClone(lineItem),
    id: makeSyntheticGid('CalculatedLineItem'),
  }));
}

function recalculateOrderTotals(order: OrderRecord): OrderRecord {
  const currencyCode =
    order.currentTotalPriceSet?.shopMoney.currencyCode ??
    order.subtotalPriceSet?.shopMoney.currencyCode ??
    order.totalPriceSet?.shopMoney.currencyCode ??
    'CAD';
  const subtotal = formatDecimalAmount(
    order.lineItems.reduce(
      (sum, lineItem) => sum + parseDecimalAmount(lineItem.originalUnitPriceSet?.shopMoney.amount) * lineItem.quantity,
      0,
    ),
  );
  const shippingTotal = formatDecimalAmount(
    order.shippingLines.reduce(
      (sum, shippingLine) => sum + parseDecimalAmount(shippingLine.originalPriceSet?.shopMoney.amount),
      0,
    ),
  );
  const total = formatDecimalAmount(parseDecimalAmount(subtotal) + parseDecimalAmount(shippingTotal));

  return {
    ...order,
    subtotalPriceSet: {
      shopMoney: normalizeMoney(subtotal, currencyCode),
    },
    currentTotalPriceSet: {
      shopMoney: normalizeMoney(total, currencyCode),
    },
    totalPriceSet: {
      shopMoney: normalizeMoney(total, currencyCode),
    },
  };
}

function buildCalculatedOrderFromOrder(order: OrderRecord): CalculatedOrderRecord {
  return recalculateOrderTotals({
    ...structuredClone(order),
    id: makeSyntheticGid('CalculatedOrder'),
    originalOrderId: order.id,
    lineItems: cloneOrderLineItemsForCalculatedOrder(order),
  } as CalculatedOrderRecord) as CalculatedOrderRecord;
}

function buildCompletedDraftOrder(draftOrder: DraftOrderRecord): DraftOrderRecord {
  return {
    ...structuredClone(draftOrder),
    status: 'COMPLETED',
    ready: true,
    updatedAt: makeSyntheticTimestamp(),
  };
}

function buildCalculatedLineItemFromVariant(variantId: string, quantity: number): OrderLineItemRecord | null {
  const variant = store.getEffectiveVariantById(variantId);
  if (!variant) {
    return null;
  }

  const product = store.getEffectiveProductById(variant.productId);
  const currencyCode = 'CAD';
  return {
    id: makeSyntheticGid('CalculatedLineItem'),
    title: product?.title ?? variant.title,
    quantity,
    sku: variant.sku,
    variantTitle: variant.title,
    originalUnitPriceSet: {
      shopMoney: normalizeMoney(formatDecimalAmount(parseDecimalAmount(variant.price)), currencyCode),
    },
  };
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

function serializeShopMoneySet(field: FieldNode, money: MoneyV2Record | null): Record<string, unknown> | null {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'shopMoney':
        result[key] = serializeMoneyField(selection, money);
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
      case 'city':
        result[key] = address.city;
        break;
      case 'provinceCode':
        result[key] = address.provinceCode;
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
      case 'originalPriceSet':
        result[key] = serializeShopMoneySet(selection, shippingLine.originalPriceSet?.shopMoney ?? null);
        break;
      case 'discountedPriceSet':
      case 'currentDiscountedPriceSet':
        result[key] = serializeShopMoneySet(selection, shippingLine.originalPriceSet?.shopMoney ?? null);
        break;
      case 'custom':
        result[key] = true;
        break;
      case 'source':
      case 'carrierIdentifier':
      case 'deliveryCategory':
      case 'phone':
      case 'shippingRateHandle':
        result[key] = null;
        break;
      case 'isRemoved':
        result[key] = false;
        break;
      case 'taxLines':
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
        result[key] = serializeDraftOrderLineItemsConnection(selection, []);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
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
        nodeResult[nodeKey] = lineItem.variantTitle;
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
        nodeResult[nodeKey] = serializeShopMoneySet(nodeSelection, lineItem.originalUnitPriceSet?.shopMoney ?? null);
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
                    return [variantKey, lineItem.sku];
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
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = lineItems.map((lineItem) => serializeDraftOrderLineItemNode(selection, lineItem));
        break;
      case 'edges':
        result[key] = lineItems.map((lineItem) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${lineItem.id}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeDraftOrderLineItemNode(edgeSelection, lineItem);
                break;
              default:
                edgeResult[edgeKey] = null;
                break;
            }
          }
          return edgeResult;
        });
        break;
      case 'pageInfo':
        result[key] = Object.fromEntries(
          getSelectedChildFields(selection).map((pageInfoSelection) => {
            const pageInfoKey = getFieldResponseKey(pageInfoSelection);
            switch (pageInfoSelection.name.value) {
              case 'hasNextPage':
              case 'hasPreviousPage':
                return [pageInfoKey, false];
              case 'startCursor':
                return [pageInfoKey, lineItems[0] ? `cursor:${lineItems[0].id}` : null];
              case 'endCursor':
                return [pageInfoKey, lineItems.length > 0 ? `cursor:${lineItems[lineItems.length - 1]!.id}` : null];
              default:
                return [pageInfoKey, null];
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

function serializeDraftOrderNode(field: FieldNode, draftOrder: DraftOrderRecord): Record<string, unknown> {
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

function readNullableIntArgument(
  field: FieldNode,
  argumentName: string,
  variables: Record<string, unknown>,
): number | null {
  const argument = field.arguments?.find((candidate) => candidate.name.value === argumentName);
  if (!argument) {
    return null;
  }

  if (argument.value.kind === Kind.INT) {
    const parsed = Number.parseInt(argument.value.value, 10);
    return Number.isFinite(parsed) ? parsed : null;
  }

  if (argument.value.kind === Kind.VARIABLE) {
    const rawValue = variables[argument.value.name.value];
    return typeof rawValue === 'number' && Number.isFinite(rawValue) ? rawValue : null;
  }

  return null;
}

function readNullableStringArgument(
  field: FieldNode,
  argumentName: string,
  variables: Record<string, unknown>,
): string | null {
  const argument = field.arguments?.find((candidate) => candidate.name.value === argumentName);
  if (!argument) {
    return null;
  }

  if (argument.value.kind === Kind.STRING) {
    return argument.value.value;
  }

  if (argument.value.kind === Kind.VARIABLE) {
    const rawValue = variables[argument.value.name.value];
    return typeof rawValue === 'string' ? rawValue : null;
  }

  return null;
}

type OrderSearchExtensionEntry = {
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

function buildSyntheticCursor(id: string): string {
  return `cursor:${id}`;
}

function buildDraftOrderInvalidSearchExtension(rawQuery: unknown, path: string[]): OrderSearchExtensionEntry | null {
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
  return buildDraftOrderInvalidSearchExtension(rawQuery, ['draftOrders']) !== null;
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

function serializeDraftOrdersConnection(
  field: FieldNode,
  draftOrders: DraftOrderRecord[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const { visibleRecords, hasNextPage, hasPreviousPage } = applySyntheticCursorWindow(draftOrders, field, variables);
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'edges':
        result[key] = visibleRecords.map((draftOrder) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = buildSyntheticCursor(draftOrder.id);
                break;
              case 'node':
                edgeResult[edgeKey] = serializeDraftOrderNode(edgeSelection, draftOrder);
                break;
              default:
                edgeResult[edgeKey] = null;
                break;
            }
          }
          return edgeResult;
        });
        break;
      case 'nodes':
        result[key] = visibleRecords.map((draftOrder) => serializeDraftOrderNode(selection, draftOrder));
        break;
      case 'pageInfo':
        result[key] = Object.fromEntries(
          getSelectedChildFields(selection).map((pageInfoSelection) => {
            const pageInfoKey = getFieldResponseKey(pageInfoSelection);
            switch (pageInfoSelection.name.value) {
              case 'hasNextPage':
                return [pageInfoKey, hasNextPage];
              case 'hasPreviousPage':
                return [pageInfoKey, hasPreviousPage];
              case 'startCursor':
                return [pageInfoKey, visibleRecords[0] ? buildSyntheticCursor(visibleRecords[0].id) : null];
              case 'endCursor':
                return [
                  pageInfoKey,
                  visibleRecords.length > 0
                    ? buildSyntheticCursor(visibleRecords[visibleRecords.length - 1]!.id)
                    : null,
                ];
              default:
                return [pageInfoKey, null];
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

function serializeOrderLineItemNode(field: FieldNode, lineItem: OrderLineItemRecord): Record<string, unknown> {
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
      case 'sku':
        result[key] = lineItem.sku;
        break;
      case 'variantTitle':
        result[key] = lineItem.variantTitle;
        break;
      case 'originalUnitPriceSet':
        result[key] = serializeShopMoneySet(selection, lineItem.originalUnitPriceSet?.shopMoney ?? null);
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
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = lineItems.map((lineItem) => serializeOrderLineItemNode(selection, lineItem));
        break;
      case 'edges':
        result[key] = lineItems.map((lineItem) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${lineItem.id}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeOrderLineItemNode(edgeSelection, lineItem);
                break;
              default:
                edgeResult[edgeKey] = null;
                break;
            }
          }
          return edgeResult;
        });
        break;
      case 'pageInfo':
        result[key] = Object.fromEntries(
          getSelectedChildFields(selection).map((pageInfoSelection) => {
            const pageInfoKey = getFieldResponseKey(pageInfoSelection);
            switch (pageInfoSelection.name.value) {
              case 'hasNextPage':
              case 'hasPreviousPage':
                return [pageInfoKey, false];
              case 'startCursor':
                return [pageInfoKey, lineItems[0] ? `cursor:${lineItems[0].id}` : null];
              case 'endCursor':
                return [pageInfoKey, lineItems.length > 0 ? `cursor:${lineItems[lineItems.length - 1]!.id}` : null];
              default:
                return [pageInfoKey, null];
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

function serializeOrderShippingLinesConnection(
  field: FieldNode,
  shippingLines: OrderShippingLineRecord[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = shippingLines.map((shippingLine) => serializeDraftOrderShippingLine(selection, shippingLine));
        break;
      case 'edges':
        result[key] = shippingLines.map((shippingLine, index) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:shipping-line:${index + 1}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeDraftOrderShippingLine(edgeSelection, shippingLine);
                break;
              default:
                edgeResult[edgeKey] = null;
                break;
            }
          }
          return edgeResult;
        });
        break;
      case 'pageInfo':
        result[key] = Object.fromEntries(
          getSelectedChildFields(selection).map((pageInfoSelection) => {
            const pageInfoKey = getFieldResponseKey(pageInfoSelection);
            switch (pageInfoSelection.name.value) {
              case 'hasNextPage':
              case 'hasPreviousPage':
                return [pageInfoKey, false];
              case 'startCursor':
                return [pageInfoKey, shippingLines.length > 0 ? 'cursor:shipping-line:1' : null];
              case 'endCursor':
                return [pageInfoKey, shippingLines.length > 0 ? `cursor:shipping-line:${shippingLines.length}` : null];
              default:
                return [pageInfoKey, null];
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

function serializeOrderNode(field: FieldNode, order: OrderRecord): Record<string, unknown> {
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
      case 'billingAddress':
        result[key] = serializeDraftOrderAddress(selection, order.billingAddress);
        break;
      case 'shippingAddress':
        result[key] = serializeDraftOrderAddress(selection, order.shippingAddress);
        break;
      case 'subtotalPriceSet':
        result[key] = serializeShopMoneySet(selection, order.subtotalPriceSet?.shopMoney ?? null);
        break;
      case 'currentTotalPriceSet':
        result[key] = serializeShopMoneySet(selection, order.currentTotalPriceSet?.shopMoney ?? null);
        break;
      case 'totalPriceSet':
        result[key] = serializeShopMoneySet(selection, order.totalPriceSet?.shopMoney ?? null);
        break;
      case 'customer':
        result[key] = serializeOrderCustomer(selection, order.customer);
        break;
      case 'shippingLines':
        result[key] = serializeOrderShippingLinesConnection(selection, order.shippingLines);
        break;
      case 'lineItems':
        result[key] = serializeOrderLineItemsConnection(selection, order.lineItems);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCalculatedOrder(field: FieldNode, calculatedOrder: CalculatedOrderRecord): Record<string, unknown> {
  return serializeOrderNode(field, calculatedOrder);
}

function serializeOrderCount(field: FieldNode, count = 0): Record<string, unknown> {
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

function serializePageInfo(field: FieldNode): Record<string, unknown> {
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

function serializeOrdersConnection(
  field: FieldNode,
  orders: OrderRecord[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const first = readNullableIntArgument(field, 'first', variables);
  const visibleOrders = first === null ? orders : orders.slice(0, Math.max(0, first));
  const hasNextPage = first !== null && orders.length > visibleOrders.length;
  const hasPreviousPage = false;
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'edges':
        result[key] = visibleOrders.map((order) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${order.id}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeOrderNode(edgeSelection, order);
                break;
              default:
                edgeResult[edgeKey] = null;
                break;
            }
          }
          return edgeResult;
        });
        break;
      case 'nodes':
        result[key] = visibleOrders.map((order) => serializeOrderNode(selection, order));
        break;
      case 'pageInfo':
        result[key] = Object.fromEntries(
          getSelectedChildFields(selection).map((pageInfoSelection) => {
            const pageInfoKey = getFieldResponseKey(pageInfoSelection);
            switch (pageInfoSelection.name.value) {
              case 'hasNextPage':
                return [pageInfoKey, hasNextPage];
              case 'hasPreviousPage':
                return [pageInfoKey, hasPreviousPage];
              case 'startCursor':
                return [pageInfoKey, visibleOrders[0] ? `cursor:${visibleOrders[0].id}` : null];
              case 'endCursor':
                return [
                  pageInfoKey,
                  visibleOrders.length > 0 ? `cursor:${visibleOrders[visibleOrders.length - 1]!.id}` : null,
                ];
              default:
                return [pageInfoKey, null];
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

function readOrderUpdateInput(variables: Record<string, unknown>): Record<string, unknown> {
  const input = variables['input'];
  return typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : {};
}

function readOrderCreateInput(variables: Record<string, unknown>): unknown {
  return variables['order'] ?? null;
}

function readDraftOrderCreateInput(variables: Record<string, unknown>): unknown {
  return variables['input'] ?? null;
}

function readDraftOrderCompleteId(variables: Record<string, unknown>): string | null {
  return typeof variables['id'] === 'string' ? variables['id'] : null;
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

function serializeSelectedUserErrors(
  field: FieldNode,
  userErrors: Array<{ field: string[] | null; message: string }>,
): Array<Record<string, unknown>> {
  return userErrors.map((userError) => {
    const result: Record<string, unknown> = {};
    for (const selection of getSelectedChildFields(field)) {
      const key = getFieldResponseKey(selection);
      switch (selection.name.value) {
        case 'field':
          result[key] = userError.field;
          break;
        case 'message':
          result[key] = userError.message;
          break;
        default:
          result[key] = null;
          break;
      }
    }
    return result;
  });
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

function validateDraftOrderCreateInput(input: unknown): Array<{ field: string[] | null; message: string }> {
  const inputRecord = typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : {};
  const lineItems = inputRecord['lineItems'];
  if (!Array.isArray(lineItems) || lineItems.length === 0) {
    return [{ field: null, message: 'Add at least 1 product' }];
  }

  const userErrors: Array<{ field: string[] | null; message: string }> = [];
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

    if (variantId) {
      if (hasCustomTitle || hasCustomPrice) {
        userErrors.push({
          field: ['input', 'lineItems', String(index)],
          message: 'Variant line items cannot include custom title or originalUnitPrice fields',
        });
        return;
      }

      if (!store.getEffectiveVariantById(variantId)) {
        userErrors.push({
          field: ['input', 'lineItems', String(index), 'variantId'],
          message: 'Product variant does not exist',
        });
      }
      return;
    }

    if (!hasCustomTitle) {
      userErrors.push({
        field: ['input', 'lineItems', String(index), 'title'],
        message: "Title can't be blank",
      });
    }

    if (!hasCustomPrice) {
      userErrors.push({
        field: ['input', 'lineItems', String(index), 'originalUnitPrice'],
        message: "Original unit price can't be blank",
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

function getDraftOrderCompleteInlineIdArgument(field: FieldNode) {
  return field.arguments?.find((argument) => argument.name.value === 'id') ?? null;
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

function readFulfillmentCancelId(variables: Record<string, unknown>): string | null {
  return typeof variables['id'] === 'string' ? variables['id'] : null;
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

export function handleOrderQuery(
  document: string,
  variables: Record<string, unknown> = {},
): { data: Record<string, unknown>; extensions?: { search: OrderSearchExtensionEntry[] } } {
  const data: Record<string, unknown> = {};
  const orders = store.getOrders();
  const searchExtensions: OrderSearchExtensionEntry[] = [];

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);

    switch (field.name.value) {
      case 'order': {
        const id = typeof variables['id'] === 'string' ? variables['id'] : null;
        const order = id ? store.getOrderById(id) : null;
        data[key] = order ? serializeOrderNode(field, order) : null;
        break;
      }
      case 'orders':
        data[key] = serializeOrdersConnection(field, orders, variables);
        break;
      case 'ordersCount':
        data[key] = serializeOrderCount(field, orders.length);
        break;
      case 'draftOrder': {
        const id = typeof variables['id'] === 'string' ? variables['id'] : null;
        const draftOrder = id ? store.getDraftOrderById(id) : null;
        data[key] = draftOrder ? serializeDraftOrderNode(field, draftOrder) : null;
        break;
      }
      case 'draftOrders': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeDraftOrdersConnection(field, store.getDraftOrders(), variables);
        const searchExtension = buildDraftOrderInvalidSearchExtension(args['query'], [key]);
        if (searchExtension) {
          searchExtensions.push(searchExtension);
        }
        break;
      }
      case 'draftOrdersCount': {
        const args = getFieldArguments(field, variables);
        const countResult: Record<string, unknown> = {};
        for (const selection of getSelectedChildFields(field)) {
          const selectionKey = getFieldResponseKey(selection);
          switch (selection.name.value) {
            case 'count':
              countResult[selectionKey] = store.getDraftOrders().length;
              break;
            case 'precision':
              countResult[selectionKey] = 'EXACT';
              break;
            default:
              countResult[selectionKey] = null;
              break;
          }
        }
        data[key] = countResult;
        const searchExtension = buildDraftOrderInvalidSearchExtension(args['query'], [key]);
        if (searchExtension) {
          searchExtensions.push(searchExtension);
        }
        break;
      }
      default:
        break;
    }
  }

  if (searchExtensions.length > 0) {
    return {
      data,
      extensions: {
        search: searchExtensions,
      },
    };
  }

  return { data };
}

export function handleOrderMutation(
  document: string,
  variables: Record<string, unknown> = {},
  readMode: ReadMode,
  shopifyAdminOrigin = 'https://example.myshopify.com',
): { data?: Record<string, unknown>; errors?: Array<Record<string, unknown>> } | null {
  const data: Record<string, unknown> = {};
  const errors: Array<Record<string, unknown>> = [];
  let handled = false;

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);

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
        const updatedOrder = store.updateOrder(
          recalculateOrderTotals({
            ...existingOrder,
            updatedAt: makeSyntheticTimestamp(),
            note: typeof input['note'] === 'string' ? input['note'] : existingOrder.note,
            tags: Array.isArray(input['tags'])
              ? input['tags']
                  .filter((tag): tag is string => typeof tag === 'string')
                  .sort((left, right) => left.localeCompare(right))
              : existingOrder.tags,
          }),
        );

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

      const order = readOrderCreateInput(variables);

      if (order === null) {
        handled = true;
        errors.push(buildOrderCreateMissingOrderError());
        continue;
      }

      if (readMode === 'snapshot' || readMode === 'live-hybrid') {
        handled = true;
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

      const input = readDraftOrderCreateInput(variables);

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
        const completedDraftOrder = store.updateDraftOrder(buildCompletedDraftOrder(draftOrder));
        const payload: Record<string, unknown> = {};
        for (const selection of getSelectedChildFields(field)) {
          const selectionKey = getFieldResponseKey(selection);
          switch (selection.name.value) {
            case 'draftOrder':
              payload[selectionKey] = serializeDraftOrderNode(selection, completedDraftOrder);
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

      if (readMode === 'snapshot') {
        handled = true;
      }

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
      }
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
        continue;
      }

      handled = true;
      const updatedCalculatedOrder = store.updateCalculatedOrder(
        recalculateOrderTotals({
          ...calculatedOrder,
          lineItems: [...calculatedOrder.lineItems, calculatedLineItem],
        }) as CalculatedOrderRecord,
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
        recalculateOrderTotals({
          ...calculatedOrder,
          lineItems: calculatedOrder.lineItems.map((lineItem) =>
            lineItem.id === lineItemId ? { ...lineItem, quantity } : lineItem,
          ),
        }) as CalculatedOrderRecord,
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
      const staffNote = typeof variables['staffNote'] === 'string' ? variables['staffNote'] : calculatedOrder.note;
      const committedOrder = store.updateOrder(
        recalculateOrderTotals({
          ...store.getOrderById(calculatedOrder.originalOrderId)!,
          updatedAt: makeSyntheticTimestamp(),
          note: staffNote,
          lineItems: structuredClone(calculatedOrder.lineItems),
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
      const fulfillmentOrderId = Array.isArray(lineItemsByFulfillmentOrder)
        ? (lineItemsByFulfillmentOrder[0] as Record<string, unknown> | undefined)?.['fulfillmentOrderId']
        : undefined;

      data[key] = null;

      if (typeof fulfillmentOrderId === 'string' && fulfillmentOrderId.length > 0) {
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
