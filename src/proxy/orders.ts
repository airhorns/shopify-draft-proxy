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
  CustomerRecord,
  OrderFulfillmentLineItemRecord,
  OrderFulfillmentOrderLineItemRecord,
  OrderFulfillmentOrderRecord,
  OrderFulfillmentRecord,
  MoneyV2Record,
  OrderCustomerRecord,
  OrderDiscountApplicationRecord,
  OrderLineItemRecord,
  OrderMetafieldRecord,
  OrderRecord,
  OrderRefundLineItemRecord,
  OrderRefundRecord,
  OrderReturnRecord,
  OrderShippingLineRecord,
  OrderTaxLineRecord,
  OrderTransactionRecord,
} from '../state/types.js';

function getFieldResponseKey(field: FieldNode): string {
  return field.alias?.value ?? field.name.value;
}

function getSelectedChildFields(field: FieldNode): FieldNode[] {
  return (field.selectionSet?.selections ?? []).flatMap((selection) => {
    if (selection.kind === Kind.FIELD) {
      return [selection];
    }

    if (selection.kind === Kind.INLINE_FRAGMENT) {
      return selection.selectionSet.selections.filter(
        (inlineSelection): inlineSelection is FieldNode => inlineSelection.kind === Kind.FIELD,
      );
    }

    return [];
  });
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
  const fixed = value.toFixed(2);
  if (fixed.endsWith('00')) {
    return `${fixed.slice(0, -3)}.0`;
  }
  return fixed.endsWith('0') ? fixed.slice(0, -1) : fixed;
}

function normalizeMoneyBag(
  raw: unknown,
  currencyCode: string,
  fallbackAmount: unknown = 0,
): { shopMoney: MoneyV2Record; presentmentMoney?: MoneyV2Record } {
  const moneyBag = typeof raw === 'object' && raw !== null ? (raw as Record<string, unknown>) : {};
  const shopMoney =
    typeof moneyBag['shopMoney'] === 'object' && moneyBag['shopMoney'] !== null
      ? (moneyBag['shopMoney'] as Record<string, unknown>)
      : {};
  const presentmentMoney =
    typeof moneyBag['presentmentMoney'] === 'object' && moneyBag['presentmentMoney'] !== null
      ? (moneyBag['presentmentMoney'] as Record<string, unknown>)
      : null;
  const amount = formatDecimalAmount(parseDecimalAmount(shopMoney['amount'] ?? moneyBag['amount'] ?? fallbackAmount));
  const normalized: { shopMoney: MoneyV2Record; presentmentMoney?: MoneyV2Record } = {
    shopMoney: normalizeMoney(
      amount,
      typeof shopMoney['currencyCode'] === 'string'
        ? shopMoney['currencyCode']
        : typeof moneyBag['currencyCode'] === 'string'
          ? moneyBag['currencyCode']
          : currencyCode,
    ),
  };

  if (presentmentMoney) {
    normalized.presentmentMoney = normalizeMoney(
      formatDecimalAmount(parseDecimalAmount(presentmentMoney['amount'])),
      typeof presentmentMoney['currencyCode'] === 'string'
        ? presentmentMoney['currencyCode']
        : normalized.shopMoney.currencyCode,
    );
  }

  return normalized;
}

function normalizeZeroMoneyBag(currencyCode: string): { shopMoney: MoneyV2Record } {
  return {
    shopMoney: normalizeMoney('0.0', currencyCode),
  };
}

function readOrderCurrencyFromInput(inputRecord: Record<string, unknown>): string {
  if (typeof inputRecord['currency'] === 'string') {
    return inputRecord['currency'];
  }

  for (const lineItem of Array.isArray(inputRecord['lineItems']) ? inputRecord['lineItems'] : []) {
    if (typeof lineItem !== 'object' || lineItem === null) {
      continue;
    }
    const priceSet = (lineItem as Record<string, unknown>)['priceSet'];
    const shopMoney =
      typeof priceSet === 'object' && priceSet !== null ? (priceSet as Record<string, unknown>)['shopMoney'] : null;
    if (typeof shopMoney === 'object' && shopMoney !== null) {
      const currencyCode = (shopMoney as Record<string, unknown>)['currencyCode'];
      if (typeof currencyCode === 'string') {
        return currencyCode;
      }
    }
  }

  return 'CAD';
}

function normalizeOrderTaxLines(raw: unknown, currencyCode: string): OrderTaxLineRecord[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .filter((taxLine): taxLine is Record<string, unknown> => typeof taxLine === 'object' && taxLine !== null)
    .map((taxLine) => ({
      title: typeof taxLine['title'] === 'string' ? taxLine['title'] : null,
      rate: typeof taxLine['rate'] === 'number' ? taxLine['rate'] : parseDecimalAmount(taxLine['rate']),
      channelLiable: typeof taxLine['channelLiable'] === 'boolean' ? taxLine['channelLiable'] : null,
      priceSet: normalizeMoneyBag(taxLine['priceSet'], currencyCode),
    }));
}

function sumTaxLines(taxLines: OrderTaxLineRecord[]): number {
  return taxLines.reduce((sum, taxLine) => sum + parseDecimalAmount(taxLine.priceSet?.shopMoney.amount), 0);
}

function readString(raw: unknown): string | null {
  return typeof raw === 'string' && raw.length > 0 ? raw : null;
}

function readBoolean(raw: unknown, fallback: boolean): boolean {
  return typeof raw === 'boolean' ? raw : fallback;
}

function subtractMoney(
  left: { shopMoney: MoneyV2Record | null } | null | undefined,
  right: { shopMoney: MoneyV2Record | null } | null | undefined,
  currencyCode: string,
): { shopMoney: MoneyV2Record } {
  return {
    shopMoney: normalizeMoney(
      formatDecimalAmount(parseDecimalAmount(left?.shopMoney?.amount) - parseDecimalAmount(right?.shopMoney?.amount)),
      left?.shopMoney?.currencyCode ?? right?.shopMoney?.currencyCode ?? currencyCode,
    ),
  };
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
    address2: typeof address['address2'] === 'string' ? address['address2'] : null,
    company: typeof address['company'] === 'string' ? address['company'] : null,
    city: typeof address['city'] === 'string' ? address['city'] : null,
    province: typeof address['province'] === 'string' ? address['province'] : null,
    provinceCode: typeof address['provinceCode'] === 'string' ? address['provinceCode'] : null,
    country: typeof address['country'] === 'string' ? address['country'] : null,
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

function buildDraftOrderCustomerFromInput(inputRecord: Record<string, unknown>): DraftOrderCustomerRecord | null {
  const email = readString(inputRecord['email']);
  const purchasingEntity =
    typeof inputRecord['purchasingEntity'] === 'object' && inputRecord['purchasingEntity'] !== null
      ? (inputRecord['purchasingEntity'] as Record<string, unknown>)
      : {};
  const customerId = readString(inputRecord['customerId']) ?? readString(purchasingEntity['customerId']);
  const customer = customerId ? store.getEffectiveCustomerById(customerId) : null;
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
    email: customer?.email ?? email,
    displayName: customer?.displayName ?? (displayName.length > 0 ? displayName : email),
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

function normalizeOrderMetafields(
  orderId: string,
  raw: unknown,
  existing: OrderMetafieldRecord[] = [],
): OrderMetafieldRecord[] {
  if (!Array.isArray(raw)) {
    return existing.map((metafield) => structuredClone(metafield));
  }

  const metafieldsByIdentity = new Map(
    existing.map((metafield) => [`${metafield.namespace}:${metafield.key}`, structuredClone(metafield)]),
  );

  for (const value of raw) {
    if (typeof value !== 'object' || value === null) {
      continue;
    }

    const input = value as Record<string, unknown>;
    const namespace = typeof input['namespace'] === 'string' ? input['namespace'] : '';
    const key = typeof input['key'] === 'string' ? input['key'] : '';
    if (!namespace || !key) {
      continue;
    }

    const identityKey = `${namespace}:${key}`;
    const existingMetafield = metafieldsByIdentity.get(identityKey);
    metafieldsByIdentity.set(identityKey, {
      id: existingMetafield?.id ?? makeSyntheticGid('Metafield'),
      orderId,
      namespace,
      key,
      type: typeof input['type'] === 'string' ? input['type'] : (existingMetafield?.type ?? null),
      value: typeof input['value'] === 'string' ? input['value'] : (existingMetafield?.value ?? null),
    });
  }

  return Array.from(metafieldsByIdentity.values()).sort(
    (left, right) =>
      left.namespace.localeCompare(right.namespace) ||
      left.key.localeCompare(right.key) ||
      left.id.localeCompare(right.id),
  );
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
        sku: isVariantLine ? (variant?.sku ?? '') : readString(lineItem['sku']),
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

function normalizeDraftOrderShippingLine(raw: unknown, currencyCode: string): DraftOrderShippingLineRecord | null {
  if (typeof raw !== 'object' || raw === null) {
    return null;
  }

  const shippingLine = raw as Record<string, unknown>;
  const priceWithCurrency =
    typeof shippingLine['priceWithCurrency'] === 'object' && shippingLine['priceWithCurrency'] !== null
      ? (shippingLine['priceWithCurrency'] as Record<string, unknown>)
      : {};
  const price =
    typeof priceWithCurrency['amount'] === 'string' || typeof priceWithCurrency['amount'] === 'number'
      ? priceWithCurrency['amount']
      : (shippingLine['price'] ?? shippingLine['originalPrice']);
  const lineCurrency =
    typeof priceWithCurrency['currencyCode'] === 'string' ? priceWithCurrency['currencyCode'] : currencyCode;

  return {
    title: typeof shippingLine['title'] === 'string' ? shippingLine['title'] : null,
    code:
      typeof shippingLine['code'] === 'string'
        ? shippingLine['code']
        : typeof shippingLine['shippingRateHandle'] === 'string'
          ? shippingLine['shippingRateHandle']
          : 'custom',
    originalPriceSet: {
      shopMoney: normalizeMoney(formatDecimalAmount(parseDecimalAmount(price)), lineCurrency),
    },
  };
}

function normalizeOrderLineItems(raw: unknown, currencyCode: string): OrderLineItemRecord[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .filter((lineItem): lineItem is Record<string, unknown> => typeof lineItem === 'object' && lineItem !== null)
    .map((lineItem) => {
      const variantId = typeof lineItem['variantId'] === 'string' ? lineItem['variantId'] : null;
      const variant = variantId ? store.getEffectiveVariantById(variantId) : null;
      const product = variant ? store.getEffectiveProductById(variant.productId) : null;
      const rawPriceSet =
        typeof lineItem['originalUnitPriceSet'] === 'object' && lineItem['originalUnitPriceSet'] !== null
          ? lineItem['originalUnitPriceSet']
          : lineItem['priceSet'];
      const fallbackPrice = variant?.price ?? 0;

      return {
        id: makeSyntheticGid('LineItem'),
        title:
          typeof lineItem['title'] === 'string'
            ? lineItem['title']
            : product?.title
              ? product.title
              : (variant?.title ?? null),
        quantity: typeof lineItem['quantity'] === 'number' ? lineItem['quantity'] : 0,
        sku: typeof lineItem['sku'] === 'string' ? lineItem['sku'] : (variant?.sku ?? null),
        variantId,
        variantTitle:
          typeof lineItem['variantTitle'] === 'string' ? lineItem['variantTitle'] : (variant?.title ?? null),
        originalUnitPriceSet: normalizeMoneyBag(rawPriceSet, currencyCode, fallbackPrice),
        taxLines: normalizeOrderTaxLines(lineItem['taxLines'], currencyCode),
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

      return {
        title: typeof shippingLine['title'] === 'string' ? shippingLine['title'] : null,
        code: typeof shippingLine['code'] === 'string' ? shippingLine['code'] : null,
        source: typeof shippingLine['source'] === 'string' ? shippingLine['source'] : null,
        originalPriceSet: normalizeMoneyBag(priceSet, currencyCode),
        taxLines: normalizeOrderTaxLines(shippingLine['taxLines'], currencyCode),
      };
    });
}

function normalizeOrderTransactions(raw: unknown, currencyCode: string): OrderTransactionRecord[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .filter(
      (transaction): transaction is Record<string, unknown> => typeof transaction === 'object' && transaction !== null,
    )
    .map((transaction) => {
      const amountSet =
        typeof transaction['amountSet'] === 'object' && transaction['amountSet'] !== null
          ? (transaction['amountSet'] as Record<string, unknown>)
          : {};
      const shopMoney =
        typeof amountSet['shopMoney'] === 'object' && amountSet['shopMoney'] !== null
          ? (amountSet['shopMoney'] as Record<string, unknown>)
          : {};
      const directAmount =
        typeof transaction['amount'] === 'string' || typeof transaction['amount'] === 'number'
          ? transaction['amount']
          : null;
      const amount = formatDecimalAmount(parseDecimalAmount(shopMoney['amount'] ?? directAmount));

      return {
        id: makeSyntheticGid('OrderTransaction'),
        kind: typeof transaction['kind'] === 'string' ? transaction['kind'] : null,
        status: typeof transaction['status'] === 'string' ? transaction['status'] : 'SUCCESS',
        gateway: typeof transaction['gateway'] === 'string' ? transaction['gateway'] : null,
        amountSet: normalizeMoneyBag(amountSet, currencyCode, amount),
      };
    });
}

function readDiscountCodeInput(inputRecord: Record<string, unknown>): Record<string, unknown> | null {
  const discountCode = inputRecord['discountCode'];
  return typeof discountCode === 'object' && discountCode !== null ? (discountCode as Record<string, unknown>) : null;
}

function normalizeOrderDiscountApplications(
  inputRecord: Record<string, unknown>,
  currencyCode: string,
  discountableSubtotal: number,
  shippingTotal: number,
): {
  discountCodes: string[];
  discountApplications: OrderDiscountApplicationRecord[];
  totalDiscountsSet: { shopMoney: MoneyV2Record } | null;
} {
  const discountCode = readDiscountCodeInput(inputRecord);
  if (!discountCode) {
    return {
      discountCodes: [],
      discountApplications: [],
      totalDiscountsSet: null,
    };
  }

  const fixedDiscount = readDiscountCodeAttributes(discountCode, 'itemFixedDiscountCode');
  if (fixedDiscount) {
    const code = typeof fixedDiscount['code'] === 'string' ? fixedDiscount['code'] : null;
    const amountSet = normalizeMoneyBag(fixedDiscount['amountSet'], currencyCode);
    return {
      discountCodes: code ? [code] : [],
      discountApplications: [
        {
          code,
          value: {
            type: 'money',
            amount: amountSet.shopMoney.amount,
            currencyCode: amountSet.shopMoney.currencyCode,
          },
        },
      ],
      totalDiscountsSet: amountSet,
    };
  }

  const percentageDiscount = readDiscountCodeAttributes(discountCode, 'itemPercentageDiscountCode');
  if (percentageDiscount) {
    const code = typeof percentageDiscount['code'] === 'string' ? percentageDiscount['code'] : null;
    const percentage = parseDecimalAmount(percentageDiscount['percentage']);
    const amount = formatDecimalAmount((discountableSubtotal * percentage) / 100);
    return {
      discountCodes: code ? [code] : [],
      discountApplications: [
        {
          code,
          value: {
            type: 'percentage',
            percentage,
          },
        },
      ],
      totalDiscountsSet: {
        shopMoney: normalizeMoney(amount, currencyCode),
      },
    };
  }

  const freeShippingDiscount = readDiscountCodeAttributes(discountCode, 'freeShippingDiscountCode');
  if (freeShippingDiscount) {
    const code = typeof freeShippingDiscount['code'] === 'string' ? freeShippingDiscount['code'] : null;
    return {
      discountCodes: code ? [code] : [],
      discountApplications: [
        {
          code,
          value: {
            type: 'money',
            amount: formatDecimalAmount(shippingTotal),
            currencyCode,
          },
        },
      ],
      totalDiscountsSet: {
        shopMoney: normalizeMoney(formatDecimalAmount(shippingTotal), currencyCode),
      },
    };
  }

  return {
    discountCodes: [],
    discountApplications: [],
    totalDiscountsSet: null,
  };
}

function readDiscountCodeAttributes(
  discountCode: Record<string, unknown>,
  key: string,
): Record<string, unknown> | null {
  const attributes = discountCode[key];
  return typeof attributes === 'object' && attributes !== null ? (attributes as Record<string, unknown>) : null;
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

function buildOrderCustomerFromDraftOrder(draftOrder: DraftOrderRecord): OrderCustomerRecord | null {
  const email = draftOrder.email;
  const firstName = draftOrder.billingAddress?.firstName ?? null;
  const lastName = draftOrder.billingAddress?.lastName ?? null;
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
  const currencyCode = readOrderCurrencyFromInput(inputRecord);
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
  const orderTaxLines = normalizeOrderTaxLines(inputRecord['taxLines'], currencyCode);
  const taxTotal = formatDecimalAmount(
    sumTaxLines(orderTaxLines) +
      lineItems.reduce((sum, lineItem) => sum + sumTaxLines(lineItem.taxLines ?? []), 0) +
      shippingLines.reduce((sum, shippingLine) => sum + sumTaxLines(shippingLine.taxLines ?? []), 0),
  );
  const discounts = normalizeOrderDiscountApplications(
    inputRecord,
    currencyCode,
    parseDecimalAmount(subtotal),
    parseDecimalAmount(shippingTotal),
  );
  const discountTotal = parseDecimalAmount(discounts.totalDiscountsSet?.shopMoney.amount);
  const total = formatDecimalAmount(parseDecimalAmount(subtotal) + parseDecimalAmount(shippingTotal));
  const currentTotal = formatDecimalAmount(
    Math.max(0, parseDecimalAmount(total) + parseDecimalAmount(taxTotal) - discountTotal),
  );
  const transactions = Array.isArray(inputRecord['transactions']) ? inputRecord['transactions'] : [];
  const hasSuccessfulTransaction = transactions.some((transaction) => {
    if (typeof transaction !== 'object' || transaction === null) {
      return false;
    }

    return (transaction as Record<string, unknown>)['status'] === 'SUCCESS';
  });
  const customer = buildOrderCustomerFromInput(inputRecord);
  const normalizedTransactions = normalizeOrderTransactions(transactions, currencyCode);

  return {
    id: orderId,
    name: `#${store.getOrders().length + 1}`,
    createdAt,
    updatedAt: createdAt,
    email: typeof inputRecord['email'] === 'string' ? inputRecord['email'] : null,
    phone: typeof inputRecord['phone'] === 'string' ? inputRecord['phone'] : null,
    poNumber: typeof inputRecord['poNumber'] === 'string' ? inputRecord['poNumber'] : null,
    closed: false,
    closedAt: null,
    cancelledAt: null,
    cancelReason: null,
    sourceName: typeof inputRecord['sourceName'] === 'string' ? inputRecord['sourceName'] : null,
    paymentGatewayNames: (() => {
      const gatewayNames = normalizedTransactions
        .map((transaction) => transaction.gateway)
        .filter((gateway): gateway is string => typeof gateway === 'string' && gateway.length > 0);
      return gatewayNames.length > 0 ? gatewayNames : hasSuccessfulTransaction ? ['manual'] : [];
    })(),
    displayFinancialStatus:
      typeof inputRecord['financialStatus'] === 'string'
        ? inputRecord['financialStatus'].toUpperCase()
        : hasSuccessfulTransaction
          ? 'PAID'
          : 'PENDING',
    displayFulfillmentStatus:
      typeof inputRecord['fulfillmentStatus'] === 'string'
        ? inputRecord['fulfillmentStatus'].toUpperCase()
        : 'UNFULFILLED',
    note: typeof inputRecord['note'] === 'string' ? inputRecord['note'] : null,
    tags: Array.isArray(inputRecord['tags'])
      ? inputRecord['tags']
          .filter((tag): tag is string => typeof tag['toString'] === 'function' && typeof tag === 'string')
          .sort((left, right) => left.localeCompare(right))
      : [],
    customAttributes: normalizeDraftOrderAttributes(inputRecord['customAttributes']),
    metafields: normalizeOrderMetafields(orderId, inputRecord['metafields']),
    billingAddress: normalizeDraftOrderAddress(inputRecord['billingAddress']),
    shippingAddress: normalizeDraftOrderAddress(inputRecord['shippingAddress']),
    subtotalPriceSet: {
      shopMoney: normalizeMoney(subtotal, currencyCode),
    },
    currentSubtotalPriceSet: {
      shopMoney: normalizeMoney(subtotal, currencyCode),
    },
    currentTotalPriceSet: {
      shopMoney: normalizeMoney(currentTotal, currencyCode),
    },
    currentTotalDiscountsSet: discounts.totalDiscountsSet ?? normalizeZeroMoneyBag(currencyCode),
    currentTotalTaxSet:
      parseDecimalAmount(taxTotal) > 0
        ? {
            shopMoney: normalizeMoney(taxTotal, currencyCode),
          }
        : normalizeZeroMoneyBag(currencyCode),
    totalPriceSet: {
      shopMoney: normalizeMoney(currentTotal, currencyCode),
    },
    totalOutstandingSet: {
      shopMoney: normalizeMoney(hasSuccessfulTransaction ? '0.0' : currentTotal, currencyCode),
    },
    totalRefundedSet: {
      shopMoney: normalizeMoney('0.0', currencyCode),
    },
    totalRefundedShippingSet: normalizeZeroMoneyBag(currencyCode),
    totalReceivedSet: {
      shopMoney: normalizeMoney(hasSuccessfulTransaction ? currentTotal : '0.0', currencyCode),
    },
    netPaymentSet: {
      shopMoney: normalizeMoney(hasSuccessfulTransaction ? currentTotal : '0.0', currencyCode),
    },
    totalShippingPriceSet: {
      shopMoney: normalizeMoney(shippingTotal, currencyCode),
    },
    totalTaxSet:
      parseDecimalAmount(taxTotal) > 0
        ? {
            shopMoney: normalizeMoney(taxTotal, currencyCode),
          }
        : normalizeZeroMoneyBag(currencyCode),
    totalDiscountsSet: discounts.totalDiscountsSet ?? normalizeZeroMoneyBag(currencyCode),
    discountCodes: discounts.discountCodes,
    discountApplications: discounts.discountApplications,
    taxLines: orderTaxLines,
    taxesIncluded: typeof inputRecord['taxesIncluded'] === 'boolean' ? inputRecord['taxesIncluded'] : false,
    customer,
    shippingLines,
    lineItems,
    fulfillments: [],
    fulfillmentOrders: [],
    transactions: normalizedTransactions,
    refunds: [],
    returns: [],
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
  const discountedLineSubtotal = formatDecimalAmount(
    lineItems.reduce((sum, lineItem) => sum + parseDecimalAmount(lineItem.discountedTotalSet?.shopMoney.amount), 0),
  );
  const orderDiscountTotal = calculateDraftOrderDiscountAmount(
    appliedDiscount,
    parseDecimalAmount(discountedLineSubtotal),
  );
  const subtotal = formatDecimalAmount(Math.max(0, parseDecimalAmount(discountedLineSubtotal) - orderDiscountTotal));
  const shippingTotal = parseDecimalAmount(shippingLine?.originalPriceSet?.shopMoney.amount);
  const totalDiscount = formatDecimalAmount(lineDiscountTotal + orderDiscountTotal);
  const totalShipping = formatDecimalAmount(shippingTotal);
  const total = formatDecimalAmount(parseDecimalAmount(subtotal) + shippingTotal);
  const name = `#D${store.getDraftOrders().length + 1}`;
  const invoiceId = draftOrderId.split('/').at(-1) ?? 'draft-order';

  return {
    id: draftOrderId,
    name,
    invoiceUrl: `${shopifyAdminOrigin.replace(/\/$/, '')}/draft_orders/${invoiceId}/invoice`,
    status: 'OPEN',
    ready: true,
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

function recalculateDraftOrderTotals(draftOrder: DraftOrderRecord): DraftOrderRecord {
  const currencyCode =
    draftOrder.totalPriceSet?.shopMoney.currencyCode ??
    draftOrder.subtotalPriceSet?.shopMoney.currencyCode ??
    draftOrder.shippingLine?.originalPriceSet?.shopMoney.currencyCode ??
    'CAD';
  const lineDiscountTotal = draftOrder.lineItems.reduce(
    (sum, lineItem) => sum + parseDecimalAmount(lineItem.totalDiscountSet?.shopMoney.amount),
    0,
  );
  const discountedLineSubtotal = formatDecimalAmount(
    draftOrder.lineItems.reduce((sum, lineItem) => {
      const discountedTotal = lineItem.discountedTotalSet?.shopMoney.amount;
      const fallbackTotal = formatDecimalAmount(
        parseDecimalAmount(lineItem.originalUnitPriceSet?.shopMoney.amount) * lineItem.quantity,
      );
      return sum + parseDecimalAmount(discountedTotal ?? fallbackTotal);
    }, 0),
  );
  const orderDiscountTotal = calculateDraftOrderDiscountAmount(
    draftOrder.appliedDiscount,
    parseDecimalAmount(discountedLineSubtotal),
  );
  const subtotal = formatDecimalAmount(Math.max(0, parseDecimalAmount(discountedLineSubtotal) - orderDiscountTotal));
  const shippingTotal = formatDecimalAmount(
    parseDecimalAmount(draftOrder.shippingLine?.originalPriceSet?.shopMoney.amount),
  );
  const totalDiscount = formatDecimalAmount(lineDiscountTotal + orderDiscountTotal);
  const total = formatDecimalAmount(parseDecimalAmount(subtotal) + parseDecimalAmount(shippingTotal));

  return {
    ...draftOrder,
    subtotalPriceSet: {
      shopMoney: normalizeMoney(subtotal, currencyCode),
    },
    totalPriceSet: {
      shopMoney: normalizeMoney(total, currencyCode),
    },
    totalDiscountsSet: {
      shopMoney: normalizeMoney(totalDiscount, currencyCode),
    },
    totalShippingPriceSet: {
      shopMoney: normalizeMoney(shippingTotal, currencyCode),
    },
  };
}

function buildUpdatedDraftOrder(
  draftOrder: DraftOrderRecord,
  input: unknown,
  shopifyAdminOrigin: string,
): DraftOrderRecord {
  const inputRecord = typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : {};
  const currencyCode = draftOrder.totalPriceSet?.shopMoney.currencyCode ?? 'CAD';
  const updatedAt = makeSyntheticTimestamp();
  const lineItems = Object.hasOwn(inputRecord, 'lineItems')
    ? normalizeDraftOrderLineItems(inputRecord['lineItems'], currencyCode)
    : structuredClone(draftOrder.lineItems);

  return recalculateDraftOrderTotals({
    ...structuredClone(draftOrder),
    invoiceUrl:
      draftOrder.invoiceUrl || `${shopifyAdminOrigin.replace(/\/$/, '')}/draft_orders/${draftOrder.id}/invoice`,
    email: typeof inputRecord['email'] === 'string' ? inputRecord['email'] : draftOrder.email,
    note: typeof inputRecord['note'] === 'string' ? inputRecord['note'] : draftOrder.note,
    tags: Array.isArray(inputRecord['tags'])
      ? inputRecord['tags']
          .filter((tag): tag is string => typeof tag === 'string')
          .sort((left, right) => left.localeCompare(right))
      : structuredClone(draftOrder.tags),
    customAttributes: Object.hasOwn(inputRecord, 'customAttributes')
      ? normalizeDraftOrderAttributes(inputRecord['customAttributes'])
      : structuredClone(draftOrder.customAttributes),
    billingAddress: Object.hasOwn(inputRecord, 'billingAddress')
      ? normalizeDraftOrderAddress(inputRecord['billingAddress'])
      : structuredClone(draftOrder.billingAddress),
    shippingAddress: Object.hasOwn(inputRecord, 'shippingAddress')
      ? normalizeDraftOrderAddress(inputRecord['shippingAddress'])
      : structuredClone(draftOrder.shippingAddress),
    shippingLine: Object.hasOwn(inputRecord, 'shippingLine')
      ? normalizeDraftOrderShippingLine(inputRecord['shippingLine'], currencyCode)
      : structuredClone(draftOrder.shippingLine),
    updatedAt,
    lineItems,
  });
}

function duplicateDraftOrder(draftOrder: DraftOrderRecord, shopifyAdminOrigin: string): DraftOrderRecord {
  const draftOrderId = makeSyntheticGid('DraftOrder');
  const createdAt = makeSyntheticTimestamp();
  const invoiceId = draftOrderId.split('/').at(-1) ?? 'draft-order';

  return recalculateDraftOrderTotals({
    ...structuredClone(draftOrder),
    id: draftOrderId,
    name: `#D${store.getDraftOrders().length + 1}`,
    orderId: null,
    completedAt: null,
    invoiceUrl: `${shopifyAdminOrigin.replace(/\/$/, '')}/draft_orders/${invoiceId}/invoice`,
    status: 'OPEN',
    ready: true,
    taxExempt: false,
    reserveInventoryUntil: null,
    appliedDiscount: null,
    shippingLine: null,
    createdAt,
    updatedAt: createdAt,
    lineItems: draftOrder.lineItems.map((lineItem) => ({
      ...structuredClone(lineItem),
      id: makeSyntheticGid('DraftOrderLineItem'),
      appliedDiscount: null,
      discountedTotalSet: structuredClone(lineItem.originalTotalSet),
      totalDiscountSet: {
        shopMoney: normalizeMoney('0.0', lineItem.originalUnitPriceSet?.shopMoney.currencyCode ?? 'CAD'),
      },
    })),
  });
}

function buildDraftOrderFromOrder(order: OrderRecord, shopifyAdminOrigin: string): DraftOrderRecord {
  const draftOrderId = makeSyntheticGid('DraftOrder');
  const createdAt = makeSyntheticTimestamp();
  const invoiceId = draftOrderId.split('/').at(-1) ?? 'draft-order';
  const currencyCode =
    order.totalPriceSet?.shopMoney.currencyCode ?? order.subtotalPriceSet?.shopMoney.currencyCode ?? 'CAD';

  return {
    id: draftOrderId,
    name: `#D${store.getDraftOrders().length + 1}`,
    invoiceUrl: `${shopifyAdminOrigin.replace(/\/$/, '')}/draft_orders/${invoiceId}/invoice`,
    status: 'OPEN',
    ready: true,
    email: order.email ?? order.customer?.email ?? null,
    note: order.note,
    tags: structuredClone(order.tags),
    customer: order.customer
      ? {
          id: order.customer.id,
          email: order.customer.email,
          displayName: order.customer.displayName,
        }
      : null,
    taxExempt: false,
    taxesIncluded: false,
    reserveInventoryUntil: null,
    paymentTerms: null,
    appliedDiscount: null,
    customAttributes: structuredClone(order.customAttributes),
    billingAddress: structuredClone(order.billingAddress),
    shippingAddress: structuredClone(order.shippingAddress),
    shippingLine: null,
    createdAt,
    updatedAt: createdAt,
    subtotalPriceSet: {
      shopMoney: normalizeMoney(
        formatDecimalAmount(
          order.lineItems.reduce(
            (sum, lineItem) =>
              sum + parseDecimalAmount(lineItem.originalUnitPriceSet?.shopMoney.amount) * lineItem.quantity,
            0,
          ),
        ),
        currencyCode,
      ),
    },
    totalDiscountsSet: {
      shopMoney: normalizeMoney('0.0', currencyCode),
    },
    totalShippingPriceSet: {
      shopMoney: normalizeMoney('0.0', currencyCode),
    },
    totalPriceSet: {
      shopMoney: normalizeMoney(
        formatDecimalAmount(
          order.lineItems.reduce(
            (sum, lineItem) =>
              sum + parseDecimalAmount(lineItem.originalUnitPriceSet?.shopMoney.amount) * lineItem.quantity,
            0,
          ),
        ),
        currencyCode,
      ),
    },
    lineItems: order.lineItems.map((lineItem) => ({
      id: makeSyntheticGid('DraftOrderLineItem'),
      title: lineItem.title,
      name: lineItem.title,
      quantity: lineItem.quantity,
      sku: lineItem.sku,
      variantTitle: lineItem.variantTitle,
      variantId: lineItem.variantId ?? null,
      productId: null,
      custom: !lineItem.variantId,
      requiresShipping: true,
      taxable: true,
      customAttributes: [],
      appliedDiscount: null,
      originalUnitPriceSet: structuredClone(lineItem.originalUnitPriceSet),
      originalTotalSet: lineItem.originalUnitPriceSet
        ? {
            shopMoney: normalizeMoney(
              formatDecimalAmount(
                parseDecimalAmount(lineItem.originalUnitPriceSet.shopMoney.amount) * lineItem.quantity,
              ),
              lineItem.originalUnitPriceSet.shopMoney.currencyCode,
            ),
          }
        : null,
      discountedTotalSet: lineItem.originalUnitPriceSet
        ? {
            shopMoney: normalizeMoney(
              formatDecimalAmount(
                parseDecimalAmount(lineItem.originalUnitPriceSet.shopMoney.amount) * lineItem.quantity,
              ),
              lineItem.originalUnitPriceSet.shopMoney.currencyCode,
            ),
          }
        : null,
      totalDiscountSet: {
        shopMoney: normalizeMoney('0.0', currencyCode),
      },
    })),
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
  const taxTotal = formatDecimalAmount(
    sumTaxLines(order.taxLines ?? []) +
      order.lineItems.reduce((sum, lineItem) => sum + sumTaxLines(lineItem.taxLines ?? []), 0) +
      order.shippingLines.reduce((sum, shippingLine) => sum + sumTaxLines(shippingLine.taxLines ?? []), 0),
  );
  const discountTotal = parseDecimalAmount(order.totalDiscountsSet?.shopMoney.amount);
  const total = formatDecimalAmount(
    Math.max(
      0,
      parseDecimalAmount(subtotal) + parseDecimalAmount(shippingTotal) + parseDecimalAmount(taxTotal) - discountTotal,
    ),
  );

  return {
    ...order,
    subtotalPriceSet: {
      shopMoney: normalizeMoney(subtotal, currencyCode),
    },
    currentSubtotalPriceSet: {
      shopMoney: normalizeMoney(subtotal, currencyCode),
    },
    currentTotalPriceSet: {
      shopMoney: normalizeMoney(total, currencyCode),
    },
    currentTotalTaxSet:
      parseDecimalAmount(taxTotal) > 0
        ? { shopMoney: normalizeMoney(taxTotal, currencyCode) }
        : order.currentTotalTaxSet,
    totalPriceSet: {
      shopMoney: normalizeMoney(total, currencyCode),
    },
    totalShippingPriceSet: {
      shopMoney: normalizeMoney(shippingTotal, currencyCode),
    },
    totalTaxSet:
      parseDecimalAmount(taxTotal) > 0 ? { shopMoney: normalizeMoney(taxTotal, currencyCode) } : order.totalTaxSet,
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
  const completedAt = makeSyntheticTimestamp();
  return {
    ...structuredClone(draftOrder),
    status: 'COMPLETED',
    ready: true,
    completedAt,
    updatedAt: completedAt,
  };
}

function buildOrderLineItemsFromDraftOrder(draftOrder: DraftOrderRecord): OrderLineItemRecord[] {
  return draftOrder.lineItems.map((lineItem) => ({
    id: makeSyntheticGid('LineItem'),
    title: lineItem.title,
    quantity: lineItem.quantity,
    sku: lineItem.sku === '' ? null : lineItem.sku,
    variantId: null,
    variantTitle: lineItem.variantTitle === 'Default Title' ? null : lineItem.variantTitle,
    originalUnitPriceSet: structuredClone(lineItem.originalUnitPriceSet),
    taxLines: [],
  }));
}

function buildOrderShippingLinesFromDraftOrder(draftOrder: DraftOrderRecord): OrderShippingLineRecord[] {
  return draftOrder.shippingLine
    ? [
        {
          ...structuredClone(draftOrder.shippingLine),
          source: null,
          taxLines: [],
        },
      ]
    : [];
}

function buildOrderFromCompletedDraftOrder(
  draftOrder: DraftOrderRecord,
  completion: {
    sourceName: string | null;
    paymentPending: boolean;
  },
): OrderRecord {
  const createdAt = draftOrder.completedAt ?? makeSyntheticTimestamp();
  const currencyCode = draftOrder.totalPriceSet?.shopMoney.currencyCode ?? 'CAD';
  return {
    id: makeSyntheticGid('Order'),
    name: `#${store.getOrders().length + 1}`,
    createdAt,
    updatedAt: createdAt,
    email: draftOrder.email,
    phone: draftOrder.billingAddress?.phone ?? draftOrder.shippingAddress?.phone ?? null,
    poNumber: null,
    closed: false,
    closedAt: null,
    cancelledAt: null,
    cancelReason: null,
    sourceName: normalizeDraftOrderCompleteOrderSourceName(completion.sourceName),
    paymentGatewayNames: completion.paymentPending ? [] : ['manual'],
    displayFinancialStatus: completion.paymentPending ? 'PENDING' : 'PAID',
    displayFulfillmentStatus: 'UNFULFILLED',
    note: draftOrder.note,
    tags: structuredClone(draftOrder.tags),
    customAttributes: structuredClone(draftOrder.customAttributes),
    metafields: [],
    billingAddress: structuredClone(draftOrder.billingAddress),
    shippingAddress: structuredClone(draftOrder.shippingAddress),
    subtotalPriceSet: structuredClone(draftOrder.subtotalPriceSet),
    currentTotalPriceSet: structuredClone(draftOrder.totalPriceSet),
    totalPriceSet: structuredClone(draftOrder.totalPriceSet),
    totalOutstandingSet: {
      shopMoney: normalizeMoney(
        completion.paymentPending ? (draftOrder.totalPriceSet?.shopMoney.amount ?? '0.0') : '0.0',
        currencyCode,
      ),
    },
    totalRefundedSet: {
      shopMoney: normalizeMoney('0.0', currencyCode),
    },
    totalTaxSet: normalizeZeroMoneyBag(currencyCode),
    totalDiscountsSet: normalizeZeroMoneyBag(currencyCode),
    discountCodes: [],
    discountApplications: [],
    taxLines: [],
    taxesIncluded: false,
    customer: buildOrderCustomerFromDraftOrder(draftOrder),
    shippingLines: buildOrderShippingLinesFromDraftOrder(draftOrder),
    lineItems: buildOrderLineItemsFromDraftOrder(draftOrder),
    transactions: [],
    refunds: [],
    returns: [],
  };
}

function normalizeDraftOrderCompleteOrderSourceName(sourceName: string | null): string | null {
  if (sourceName === null) {
    return null;
  }

  return '347082227713';
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
    variantId,
    variantTitle: variant.title,
    originalUnitPriceSet: {
      shopMoney: normalizeMoney(formatDecimalAmount(parseDecimalAmount(variant.price)), currencyCode),
    },
    taxLines: [],
  };
}

function readRefundCreateInput(variables: Record<string, unknown>): Record<string, unknown> | null {
  const input = variables['input'];
  return typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : null;
}

function readRefundShippingAmount(input: Record<string, unknown>): unknown {
  const shipping = input['shipping'] ?? input['refundShipping'];
  if (typeof shipping !== 'object' || shipping === null) {
    return null;
  }

  const shippingRecord = shipping as Record<string, unknown>;
  if (shippingRecord['fullRefund'] === true) {
    return null;
  }

  if (typeof shippingRecord['amount'] === 'string' || typeof shippingRecord['amount'] === 'number') {
    return shippingRecord['amount'];
  }

  const shippingRefundAmount = shippingRecord['shippingRefundAmount'];
  if (typeof shippingRefundAmount === 'object' && shippingRefundAmount !== null) {
    return (shippingRefundAmount as Record<string, unknown>)['amount'];
  }

  return null;
}

function buildRefundLineItems(raw: unknown, order: OrderRecord, currencyCode: string): OrderRefundLineItemRecord[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .filter((lineItem): lineItem is Record<string, unknown> => typeof lineItem === 'object' && lineItem !== null)
    .map((lineItem) => {
      const lineItemId = typeof lineItem['lineItemId'] === 'string' ? lineItem['lineItemId'] : '';
      const orderLineItem = order.lineItems.find((candidate) => candidate.id === lineItemId) ?? null;
      const quantity = typeof lineItem['quantity'] === 'number' ? lineItem['quantity'] : 0;
      const subtotal = formatDecimalAmount(
        parseDecimalAmount(orderLineItem?.originalUnitPriceSet?.shopMoney.amount) * quantity,
      );

      return {
        id: makeSyntheticGid('RefundLineItem'),
        lineItemId,
        title: orderLineItem?.title ?? null,
        quantity,
        restockType: typeof lineItem['restockType'] === 'string' ? lineItem['restockType'] : null,
        subtotalSet: {
          shopMoney: normalizeMoney(subtotal, currencyCode),
        },
      };
    });
}

function sumMoney(records: Array<{ amountSet?: { shopMoney: MoneyV2Record | null } | null }>): number {
  return records.reduce((sum, record) => sum + parseDecimalAmount(record.amountSet?.shopMoney?.amount), 0);
}

function sumRefundedAmount(order: OrderRecord): number {
  return order.refunds.reduce((sum, refund) => sum + parseDecimalAmount(refund.totalRefundedSet?.shopMoney.amount), 0);
}

function readOrderCurrencyCode(order: OrderRecord): string {
  return (
    order.totalPriceSet?.shopMoney.currencyCode ??
    order.currentTotalPriceSet?.shopMoney.currencyCode ??
    order.subtotalPriceSet?.shopMoney.currencyCode ??
    'CAD'
  );
}

function buildRefundFromInput(order: OrderRecord, input: Record<string, unknown>): OrderRefundRecord {
  const currencyCode = readOrderCurrencyCode(order);
  const refundId = makeSyntheticGid('Refund');
  const createdAt = makeSyntheticTimestamp();
  const refundLineItems = buildRefundLineItems(input['refundLineItems'], order, currencyCode);
  const shippingAmount = readRefundShippingAmount(input);
  const shippingRefundAmount = shippingAmount === null ? 0 : parseDecimalAmount(shippingAmount);
  const lineItemSubtotal = refundLineItems.reduce(
    (sum, lineItem) => sum + parseDecimalAmount(lineItem.subtotalSet?.shopMoney.amount),
    0,
  );
  const fallbackRefundAmount = formatDecimalAmount(lineItemSubtotal + shippingRefundAmount);
  const rawTransactions = Array.isArray(input['transactions'])
    ? input['transactions']
    : [
        {
          kind: 'REFUND',
          status: 'SUCCESS',
          amountSet: {
            shopMoney: {
              amount: fallbackRefundAmount,
              currencyCode,
            },
          },
        },
      ];
  const transactions = normalizeOrderTransactions(rawTransactions, currencyCode);
  const transactionTotal = sumMoney(transactions);
  const totalRefunded = transactionTotal > 0 ? transactionTotal : lineItemSubtotal + shippingRefundAmount;

  return {
    id: refundId,
    note: typeof input['note'] === 'string' ? input['note'] : null,
    createdAt,
    updatedAt: createdAt,
    totalRefundedSet: {
      shopMoney: normalizeMoney(formatDecimalAmount(totalRefunded), currencyCode),
    },
    refundLineItems,
    transactions,
  };
}

function applyRefundToOrder(order: OrderRecord, refund: OrderRefundRecord): OrderRecord {
  const total = parseDecimalAmount(order.totalPriceSet?.shopMoney.amount);
  const totalRefunded = sumRefundedAmount({ ...order, refunds: [...order.refunds, refund] });
  const displayFinancialStatus = totalRefunded >= total && total > 0 ? 'REFUNDED' : 'PARTIALLY_REFUNDED';
  const currencyCode = readOrderCurrencyCode(order);
  const totalRefundedSet = {
    shopMoney: normalizeMoney(formatDecimalAmount(totalRefunded), currencyCode),
  };
  const netPaymentSet = subtractMoney(
    order.totalReceivedSet ?? order.currentTotalPriceSet,
    totalRefundedSet,
    currencyCode,
  );

  return {
    ...order,
    updatedAt: refund.createdAt,
    displayFinancialStatus,
    totalRefundedSet,
    netPaymentSet,
    transactions: [...structuredClone(order.transactions), ...structuredClone(refund.transactions)],
    refunds: [...structuredClone(order.refunds), structuredClone(refund)],
  };
}

function buildOrderTransaction(
  amountSet: OrderTransactionRecord['amountSet'],
  gateway: string,
): OrderTransactionRecord {
  return {
    id: makeSyntheticGid('OrderTransaction'),
    kind: 'SALE',
    status: 'SUCCESS',
    gateway,
    amountSet: structuredClone(amountSet),
  };
}

function markOrderAsPaid(order: OrderRecord, gateway = 'manual'): OrderRecord {
  const currencyCode = readOrderCurrencyCode(order);
  const amountSet = structuredClone(order.totalOutstandingSet ?? order.currentTotalPriceSet ?? order.totalPriceSet);
  const transaction = buildOrderTransaction(amountSet, gateway);

  return {
    ...structuredClone(order),
    updatedAt: makeSyntheticTimestamp(),
    displayFinancialStatus: 'PAID',
    paymentGatewayNames: Array.from(new Set([...(order.paymentGatewayNames ?? []), gateway])),
    totalOutstandingSet: {
      shopMoney: normalizeMoney('0.0', currencyCode),
    },
    totalReceivedSet: structuredClone(amountSet),
    netPaymentSet: subtractMoney(amountSet, order.totalRefundedSet, currencyCode),
    transactions: [...structuredClone(order.transactions), transaction],
  };
}

function orderCustomerFromCustomer(customer: CustomerRecord): OrderCustomerRecord {
  return {
    id: customer.id,
    email: customer.email,
    displayName: customer.displayName,
  };
}

function serializeOrderManagementPayload(
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

function serializeOrderCancelPayload(
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

function buildAccessDeniedError(operationName: string, requiredAccess: string): Record<string, unknown> {
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

function serializeRefundCreatePayload(
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

function tokenizeDraftOrderSearchQuery(query: string): string[] {
  const terms: string[] = [];
  let current = '';
  let inQuotes = false;

  const flushCurrent = (): void => {
    const value = current.trim();
    if (value) {
      terms.push(value);
    }
    current = '';
  };

  for (const character of query) {
    if (character === '"') {
      inQuotes = !inQuotes;
      continue;
    }

    if (!inQuotes && /\s/u.test(character)) {
      flushCurrent();
      continue;
    }

    current += character;
  }

  flushCurrent();
  return terms;
}

function isDraftOrderSearchQuerySupported(rawQuery: unknown): boolean {
  if (typeof rawQuery !== 'string' || !rawQuery.trim()) {
    return true;
  }

  if (buildDraftOrderInvalidSearchExtension(rawQuery, ['draftOrders'])) {
    return true;
  }

  const terms = tokenizeDraftOrderSearchQuery(rawQuery.trim());
  if (terms.length === 0) {
    return true;
  }

  return terms.every((term) => {
    const separatorIndex = term.indexOf(':');
    if (separatorIndex <= 0) {
      return false;
    }

    const field = term.slice(0, separatorIndex).toLowerCase();
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
  const value = rawValue.trim().toLowerCase();
  return value.length === 0 || candidate.toLowerCase() === value;
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

function matchesDraftOrderSource(draftOrder: DraftOrderRecord, rawValue: string): boolean {
  return draftOrder.customAttributes.some(
    (attribute) =>
      attribute.key.toLowerCase() === 'source' &&
      typeof attribute.value === 'string' &&
      matchesStringValue(attribute.value, rawValue),
  );
}

function matchesDraftOrderSearchTerm(draftOrder: DraftOrderRecord, term: string): boolean {
  const separatorIndex = term.indexOf(':');
  if (separatorIndex <= 0) {
    return false;
  }

  const field = term.slice(0, separatorIndex).toLowerCase();
  const value = term.slice(separatorIndex + 1);

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

function applyDraftOrdersQuery(draftOrders: DraftOrderRecord[], rawQuery: unknown): DraftOrderRecord[] {
  if (typeof rawQuery !== 'string' || !rawQuery.trim()) {
    return draftOrders;
  }

  if (buildDraftOrderInvalidSearchExtension(rawQuery, ['draftOrders'])) {
    return draftOrders;
  }

  if (!isDraftOrderSearchQuerySupported(rawQuery)) {
    return [];
  }

  const terms = tokenizeDraftOrderSearchQuery(rawQuery.trim());
  return draftOrders.filter((draftOrder) => terms.every((term) => matchesDraftOrderSearchTerm(draftOrder, term)));
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

function serializeDraftOrdersConnection(
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

function serializeDraftOrdersCount(
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
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = discountApplications.map((discountApplication) =>
          serializeOrderDiscountApplication(selection, discountApplication),
        );
        break;
      case 'edges':
        result[key] = discountApplications.map((discountApplication, index) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:discount-application:${index + 1}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeOrderDiscountApplication(edgeSelection, discountApplication);
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
        result[key] = serializePageInfo(selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
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
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = fulfillmentLineItems.map((lineItem) => serializeOrderFulfillmentLineItem(selection, lineItem));
        break;
      case 'edges':
        result[key] = fulfillmentLineItems.map((lineItem) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${lineItem.id}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeOrderFulfillmentLineItem(edgeSelection, lineItem);
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
        result[key] = serializePageInfo(selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeOrderFulfillment(field: FieldNode, fulfillment: OrderFulfillmentRecord): Record<string, unknown> {
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
      case 'trackingInfo':
        result[key] = (fulfillment.trackingInfo ?? []).map((trackingInfo) =>
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
      case 'fulfillmentLineItems':
        result[key] = serializeOrderFulfillmentLineItemsConnection(selection, fulfillment.fulfillmentLineItems ?? []);
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
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = lineItems.map((lineItem) => serializeOrderFulfillmentOrderLineItem(selection, lineItem));
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
                edgeResult[edgeKey] = serializeOrderFulfillmentOrderLineItem(edgeSelection, lineItem);
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
        result[key] = serializePageInfo(selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeOrderFulfillmentOrder(
  field: FieldNode,
  fulfillmentOrder: OrderFulfillmentOrderRecord,
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
      case 'assignedLocation':
        result[key] = fulfillmentOrder.assignedLocation
          ? Object.fromEntries(
              getSelectedChildFields(selection).map((locationSelection) => {
                const locationKey = getFieldResponseKey(locationSelection);
                switch (locationSelection.name.value) {
                  case 'name':
                    return [locationKey, fulfillmentOrder.assignedLocation?.name ?? null];
                  default:
                    return [locationKey, null];
                }
              }),
            )
          : null;
        break;
      case 'lineItems':
        result[key] = serializeOrderFulfillmentOrderLineItemsConnection(selection, fulfillmentOrder.lineItems ?? []);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeOrderFulfillmentOrdersConnection(
  field: FieldNode,
  fulfillmentOrders: OrderFulfillmentOrderRecord[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = fulfillmentOrders.map((fulfillmentOrder) =>
          serializeOrderFulfillmentOrder(selection, fulfillmentOrder),
        );
        break;
      case 'edges':
        result[key] = fulfillmentOrders.map((fulfillmentOrder) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${fulfillmentOrder.id}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeOrderFulfillmentOrder(edgeSelection, fulfillmentOrder);
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
        result[key] = serializePageInfo(selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
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
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = transactions.map((transaction) => serializeOrderTransaction(selection, transaction));
        break;
      case 'edges':
        result[key] = transactions.map((transaction) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${transaction.id}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeOrderTransaction(edgeSelection, transaction);
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
        result[key] = serializePageInfo(selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
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
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = refundLineItems.map((lineItem) => serializeOrderRefundLineItem(selection, lineItem));
        break;
      case 'edges':
        result[key] = refundLineItems.map((lineItem) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${lineItem.id}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeOrderRefundLineItem(edgeSelection, lineItem);
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
        result[key] = serializePageInfo(selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
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

function serializeOrderReturn(field: FieldNode, orderReturn: OrderReturnRecord): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = orderReturn.id;
        break;
      case 'status':
        result[key] = orderReturn.status;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeOrderReturnsConnection(field: FieldNode, returns: OrderReturnRecord[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = returns.map((orderReturn) => serializeOrderReturn(selection, orderReturn));
        break;
      case 'edges':
        result[key] = returns.map((orderReturn) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${orderReturn.id}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeOrderReturn(edgeSelection, orderReturn);
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
        result[key] = serializePageInfo(selection);
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

function serializeOrderMetafieldSelectionSet(
  metafield: OrderMetafieldRecord,
  selections: readonly FieldNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
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
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeOrderMetafieldsConnection(
  field: FieldNode,
  metafields: OrderMetafieldRecord[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = metafields.map((metafield) =>
          serializeOrderMetafieldSelectionSet(metafield, getSelectedChildFields(selection)),
        );
        break;
      case 'edges':
        result[key] = metafields.map((metafield) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${metafield.id}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeOrderMetafieldSelectionSet(
                  metafield,
                  getSelectedChildFields(edgeSelection),
                );
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
                return [pageInfoKey, metafields[0] ? `cursor:${metafields[0].id}` : null];
              case 'endCursor':
                return [pageInfoKey, metafields.length > 0 ? `cursor:${metafields[metafields.length - 1]!.id}` : null];
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
          ? serializeOrderMetafieldSelectionSet(metafield, getSelectedChildFields(selection))
          : null;
        break;
      }
      case 'metafields':
        result[key] = serializeOrderMetafieldsConnection(selection, order.metafields ?? []);
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
      case 'shippingLines':
        result[key] = serializeOrderShippingLinesConnection(selection, order.shippingLines);
        break;
      case 'lineItems':
        result[key] = serializeOrderLineItemsConnection(selection, order.lineItems);
        break;
      case 'fulfillments':
        result[key] = (order.fulfillments ?? []).map((fulfillment) =>
          serializeOrderFulfillment(selection, fulfillment),
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
        result[key] = serializeOrderReturnsConnection(selection, order.returns);
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

function readOrderCreateOptions(variables: Record<string, unknown>): Record<string, unknown> {
  const options = variables['options'];
  return typeof options === 'object' && options !== null ? (options as Record<string, unknown>) : {};
}

function readDraftOrderCreateInput(variables: Record<string, unknown>): unknown {
  return variables['input'] ?? null;
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

function readDraftOrderInvoiceSendId(variables: Record<string, unknown>): string | null {
  return typeof variables['id'] === 'string' ? variables['id'] : null;
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

function getInlineArgument(field: FieldNode, argumentName: string) {
  return field.arguments?.find((argument) => argument.name.value === argumentName) ?? null;
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
    return [{ field: ['order', 'lineItems'], message: 'Line items must include at least one line item.' }];
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
        data[key] = serializeDraftOrdersCount(field, store.getDraftOrders(), variables);
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
      data[key] = serializeDraftOrderMutationPayload(field, draftOrder, [
        {
          field: ['id'],
          message: 'draftOrderInvoiceSend is intentionally not executed by the local proxy because it sends email.',
        },
      ]);
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
