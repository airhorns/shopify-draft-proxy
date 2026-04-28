import type { ProxyRuntimeContext } from '../runtime-context.js';
import { type FieldNode } from 'graphql';
import type {
  CalculatedOrderRecord,
  CustomerRecord,
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
  OrderLineItemRecord,
  OrderMandatePaymentRecord,
  OrderMetafieldRecord,
  OrderRecord,
  OrderRefundLineItemRecord,
  OrderRefundRecord,
  OrderShippingLineRecord,
  OrderTaxLineRecord,
  OrderTransactionRecord,
} from '../../state/types.js';
import { getFieldResponseKey, getSelectedChildFields as getGraphQLSelectedChildFields } from '../graphql-helpers.js';
import { readMetafieldInputObjects, upsertOwnerMetafields } from '../metafields.js';

export type MutationUserError = { field: string[] | null; message: string; code?: string | null };

export function getSelectedChildFields(field: FieldNode): FieldNode[] {
  return getGraphQLSelectedChildFields(field, { includeInlineFragments: true });
}

export function normalizeMoney(amount: string | null, currencyCode: string | null): MoneyV2Record {
  return {
    amount,
    currencyCode,
  };
}

export function parseDecimalAmount(raw: unknown): number {
  const numeric = typeof raw === 'number' ? raw : Number.parseFloat(typeof raw === 'string' ? raw : '0');
  return Number.isFinite(numeric) ? numeric : 0;
}

export function formatDecimalAmount(value: number): string {
  const fixed = value.toFixed(2);
  if (fixed.endsWith('00')) {
    return `${fixed.slice(0, -3)}.0`;
  }
  return fixed.endsWith('0') ? fixed.slice(0, -1) : fixed;
}

export type DraftOrderSavedSearchRecord = {
  id: string;
  legacyResourceId: string;
  name: string;
  query: string;
  resourceType: 'DRAFT_ORDER';
  searchTerms: string;
};

export const DRAFT_ORDER_SAVED_SEARCHES: DraftOrderSavedSearchRecord[] = [
  {
    id: 'gid://shopify/SavedSearch/3634390597938',
    legacyResourceId: '3634390597938',
    name: 'Open and invoice sent',
    query: 'status:open_and_invoice_sent',
    resourceType: 'DRAFT_ORDER',
    searchTerms: '',
  },
  {
    id: 'gid://shopify/SavedSearch/3634390630706',
    legacyResourceId: '3634390630706',
    name: 'Open',
    query: 'status:open',
    resourceType: 'DRAFT_ORDER',
    searchTerms: '',
  },
  {
    id: 'gid://shopify/SavedSearch/3634390663474',
    legacyResourceId: '3634390663474',
    name: 'Invoice sent',
    query: 'status:invoice_sent',
    resourceType: 'DRAFT_ORDER',
    searchTerms: '',
  },
  {
    id: 'gid://shopify/SavedSearch/3634390696242',
    legacyResourceId: '3634390696242',
    name: 'Completed',
    query: 'status:completed',
    resourceType: 'DRAFT_ORDER',
    searchTerms: '',
  },
  {
    id: 'gid://shopify/SavedSearch/3634390729010',
    legacyResourceId: '3634390729010',
    name: 'Submitted for review',
    query: 'status:open source:online_store',
    resourceType: 'DRAFT_ORDER',
    searchTerms: '',
  },
];

export function normalizeDraftOrderTagHandle(tag: string): string {
  return tag.trim().toLowerCase().replace(/\s+/g, '-');
}

export function buildDraftOrderTagId(tag: string): string {
  return `gid://shopify/DraftOrderTag/${encodeURIComponent(normalizeDraftOrderTagHandle(tag))}`;
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

export function normalizeZeroMoneyBag(currencyCode: string): { shopMoney: MoneyV2Record } {
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

export function readString(raw: unknown): string | null {
  return typeof raw === 'string' && raw.length > 0 ? raw : null;
}

function readBoolean(raw: unknown, fallback: boolean): boolean {
  return typeof raw === 'boolean' ? raw : fallback;
}

export function subtractMoney(
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

export function normalizeDraftOrderAddress(raw: unknown): DraftOrderAddressRecord | null {
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

export function normalizeDraftOrderAttributes(raw: unknown): DraftOrderAttributeRecord[] {
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

function buildDraftOrderCustomerFromInput(
  runtime: ProxyRuntimeContext,
  inputRecord: Record<string, unknown>,
): DraftOrderCustomerRecord | null {
  const email = readString(inputRecord['email']);
  const purchasingEntity =
    typeof inputRecord['purchasingEntity'] === 'object' && inputRecord['purchasingEntity'] !== null
      ? (inputRecord['purchasingEntity'] as Record<string, unknown>)
      : {};
  const customerId = readString(inputRecord['customerId']) ?? readString(purchasingEntity['customerId']);
  const customer = customerId ? runtime.store.getEffectiveCustomerById(customerId) : null;
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

function normalizePaymentScheduleAmount(
  amountSet: { shopMoney: MoneyV2Record } | null | undefined,
): MoneyV2Record | null {
  return amountSet?.shopMoney ? structuredClone(amountSet.shopMoney) : null;
}

function normalizeDraftOrderPaymentTerms(
  runtime: ProxyRuntimeContext,
  raw: unknown,
  amountSet?: { shopMoney: MoneyV2Record } | null,
): DraftOrderPaymentTermsRecord | null {
  if (typeof raw !== 'object' || raw === null) {
    return null;
  }

  const paymentTerms = raw as Record<string, unknown>;
  const schedules = Array.isArray(paymentTerms['paymentSchedules']) ? paymentTerms['paymentSchedules'] : [];
  const normalizedSchedules: PaymentScheduleRecord[] = schedules
    .filter((schedule): schedule is Record<string, unknown> => typeof schedule === 'object' && schedule !== null)
    .map((schedule) => ({
      id: runtime.syntheticIdentity.makeSyntheticGid('PaymentSchedule'),
      dueAt: readString(schedule['dueAt']),
      issuedAt: readString(schedule['issuedAt']),
      completedAt: readString(schedule['completedAt']),
      completed: readBoolean(schedule['completed'], false),
      due: typeof schedule['due'] === 'boolean' ? schedule['due'] : false,
      amount: normalizePaymentScheduleAmount(amountSet),
      balanceDue: normalizePaymentScheduleAmount(amountSet),
      totalBalance: normalizePaymentScheduleAmount(amountSet),
    }));
  const firstSchedule = normalizedSchedules[0] ?? null;
  const hasDueAt = typeof firstSchedule?.['dueAt'] === 'string';
  const hasIssuedAt = typeof firstSchedule?.['issuedAt'] === 'string';
  const templateId = readString(paymentTerms['paymentTermsTemplateId']);
  const template = templateId ? runtime.store.getEffectivePaymentTermsTemplateById(templateId) : null;
  const name = template?.name ?? (hasDueAt ? 'Fixed' : hasIssuedAt ? 'Net terms' : 'Custom payment terms');
  const paymentTermsType = template?.paymentTermsType ?? (hasDueAt ? 'FIXED' : hasIssuedAt ? 'NET' : 'UNKNOWN');

  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('PaymentTerms'),
    due: false,
    overdue: false,
    dueInDays: template?.dueInDays ?? null,
    paymentTermsName: name,
    paymentTermsType,
    translatedName: template?.translatedName ?? name,
    paymentSchedules: normalizedSchedules,
  };
}

export function normalizeOrderMetafields(
  runtime: ProxyRuntimeContext,
  orderId: string,
  raw: unknown,
  existing: OrderMetafieldRecord[] = [],
): OrderMetafieldRecord[] {
  return Array.isArray(raw)
    ? upsertOwnerMetafields(runtime, 'orderId', orderId, readMetafieldInputObjects(raw), existing).metafields
    : existing.map((metafield) => structuredClone(metafield));
}

function normalizeDraftOrderLineItems(
  runtime: ProxyRuntimeContext,
  raw: unknown,
  currencyCode: string,
): DraftOrderLineItemRecord[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .filter((lineItem): lineItem is Record<string, unknown> => typeof lineItem === 'object' && lineItem !== null)
    .map((lineItem) => {
      const variantId = readString(lineItem['variantId']);
      const variant = variantId ? runtime.store.getEffectiveVariantById(variantId) : null;
      const product = variant ? runtime.store.getEffectiveProductById(variant.productId) : null;
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
        id: runtime.syntheticIdentity.makeSyntheticGid('DraftOrderLineItem'),
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

function normalizeOrderLineItems(
  runtime: ProxyRuntimeContext,
  raw: unknown,
  currencyCode: string,
): OrderLineItemRecord[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .filter((lineItem): lineItem is Record<string, unknown> => typeof lineItem === 'object' && lineItem !== null)
    .map((lineItem) => {
      const variantId = typeof lineItem['variantId'] === 'string' ? lineItem['variantId'] : null;
      const variant = variantId ? runtime.store.getEffectiveVariantById(variantId) : null;
      const product = variant ? runtime.store.getEffectiveProductById(variant.productId) : null;
      const rawPriceSet =
        typeof lineItem['originalUnitPriceSet'] === 'object' && lineItem['originalUnitPriceSet'] !== null
          ? lineItem['originalUnitPriceSet']
          : lineItem['priceSet'];
      const fallbackPrice = variant?.price ?? 0;

      return {
        id: runtime.syntheticIdentity.makeSyntheticGid('LineItem'),
        title:
          typeof lineItem['title'] === 'string'
            ? lineItem['title']
            : product?.title
              ? product.title
              : (variant?.title ?? null),
        quantity: typeof lineItem['quantity'] === 'number' ? lineItem['quantity'] : 0,
        currentQuantity: typeof lineItem['currentQuantity'] === 'number' ? lineItem['currentQuantity'] : undefined,
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

function normalizeOrderTransactions(
  runtime: ProxyRuntimeContext,
  raw: unknown,
  currencyCode: string,
): OrderTransactionRecord[] {
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
        id:
          typeof transaction['id'] === 'string'
            ? transaction['id']
            : runtime.syntheticIdentity.makeSyntheticGid('OrderTransaction'),
        kind: typeof transaction['kind'] === 'string' ? transaction['kind'] : null,
        status: typeof transaction['status'] === 'string' ? transaction['status'] : 'SUCCESS',
        gateway: typeof transaction['gateway'] === 'string' ? transaction['gateway'] : null,
        amountSet: normalizeMoneyBag(amountSet, currencyCode, amount),
        parentTransactionId:
          typeof transaction['parentTransactionId'] === 'string' ? transaction['parentTransactionId'] : null,
        paymentId: typeof transaction['paymentId'] === 'string' ? transaction['paymentId'] : null,
        paymentReferenceId:
          typeof transaction['paymentReferenceId'] === 'string' ? transaction['paymentReferenceId'] : null,
        processedAt: typeof transaction['processedAt'] === 'string' ? transaction['processedAt'] : null,
      };
    });
}

export function readDiscountCodeInput(inputRecord: Record<string, unknown>): Record<string, unknown> | null {
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

export function readDiscountCodeAttributes(
  discountCode: Record<string, unknown>,
  key: string,
): Record<string, unknown> | null {
  const attributes = discountCode[key];
  return typeof attributes === 'object' && attributes !== null ? (attributes as Record<string, unknown>) : null;
}

function buildOrderCustomerFromInput(
  runtime: ProxyRuntimeContext,
  inputRecord: Record<string, unknown>,
): OrderCustomerRecord | null {
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
    id: runtime.syntheticIdentity.makeSyntheticGid('Customer'),
    email,
    displayName: displayName.length > 0 ? displayName : email,
  };
}

function buildOrderCustomerFromDraftOrder(
  runtime: ProxyRuntimeContext,
  draftOrder: DraftOrderRecord,
): OrderCustomerRecord | null {
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
    id: runtime.syntheticIdentity.makeSyntheticGid('Customer'),
    email,
    displayName: displayName.length > 0 ? displayName : email,
  };
}

export function buildOrderFromInput(runtime: ProxyRuntimeContext, input: unknown): OrderRecord {
  const inputRecord = typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : {};
  const currencyCode = readOrderCurrencyFromInput(inputRecord);
  const orderId = runtime.syntheticIdentity.makeSyntheticGid('Order');
  const createdAt = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const lineItems = normalizeOrderLineItems(runtime, inputRecord['lineItems'], currencyCode);
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
  const hasSuccessfulPaidTransaction = transactions.some((transaction) => {
    if (typeof transaction !== 'object' || transaction === null) {
      return false;
    }

    const transactionRecord = transaction as Record<string, unknown>;
    const kind = typeof transactionRecord['kind'] === 'string' ? transactionRecord['kind'].toUpperCase() : null;
    return transactionRecord['status'] === 'SUCCESS' && (kind === 'SALE' || kind === 'CAPTURE');
  });
  const hasSuccessfulAuthorization = transactions.some((transaction) => {
    if (typeof transaction !== 'object' || transaction === null) {
      return false;
    }

    const transactionRecord = transaction as Record<string, unknown>;
    const kind = typeof transactionRecord['kind'] === 'string' ? transactionRecord['kind'].toUpperCase() : null;
    return transactionRecord['status'] === 'SUCCESS' && kind === 'AUTHORIZATION';
  });
  const customer = buildOrderCustomerFromInput(runtime, inputRecord);
  const normalizedTransactions = normalizeOrderTransactions(runtime, transactions, currencyCode);

  return {
    id: orderId,
    name: `#${runtime.store.getOrders().length + 1}`,
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
      return gatewayNames.length > 0 ? gatewayNames : hasSuccessfulPaidTransaction ? ['manual'] : [];
    })(),
    displayFinancialStatus:
      typeof inputRecord['financialStatus'] === 'string'
        ? inputRecord['financialStatus'].toUpperCase()
        : hasSuccessfulPaidTransaction
          ? 'PAID'
          : hasSuccessfulAuthorization
            ? 'AUTHORIZED'
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
    metafields: normalizeOrderMetafields(runtime, orderId, inputRecord['metafields']),
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
      shopMoney: normalizeMoney(hasSuccessfulPaidTransaction ? '0.0' : currentTotal, currencyCode),
    },
    totalCapturableSet: {
      shopMoney: normalizeMoney(hasSuccessfulAuthorization ? currentTotal : '0.0', currencyCode),
    },
    capturable: hasSuccessfulAuthorization,
    totalRefundedSet: {
      shopMoney: normalizeMoney('0.0', currencyCode),
    },
    totalRefundedShippingSet: normalizeZeroMoneyBag(currencyCode),
    totalReceivedSet: {
      shopMoney: normalizeMoney(hasSuccessfulPaidTransaction ? currentTotal : '0.0', currencyCode),
    },
    netPaymentSet: {
      shopMoney: normalizeMoney(hasSuccessfulPaidTransaction ? currentTotal : '0.0', currencyCode),
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
    paymentTerms: null,
    fulfillments: [],
    fulfillmentOrders: [],
    transactions: normalizedTransactions,
    refunds: [],
    returns: [],
  };
}

export function buildDraftOrderFromInput(
  runtime: ProxyRuntimeContext,
  input: unknown,
  shopifyAdminOrigin: string,
): DraftOrderRecord {
  const inputRecord = typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : {};
  const currencyCode = 'CAD';
  const draftOrderId = runtime.syntheticIdentity.makeSyntheticGid('DraftOrder');
  const createdAt = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const lineItems = normalizeDraftOrderLineItems(runtime, inputRecord['lineItems'], currencyCode);
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
  const name = `#D${runtime.store.getDraftOrders().length + 1}`;
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
    customer: buildDraftOrderCustomerFromInput(runtime, inputRecord),
    taxExempt: readBoolean(inputRecord['taxExempt'], false),
    taxesIncluded: readBoolean(inputRecord['taxesIncluded'], false),
    reserveInventoryUntil: readString(inputRecord['reserveInventoryUntil']),
    paymentTerms: normalizeDraftOrderPaymentTerms(runtime, inputRecord['paymentTerms'], {
      shopMoney: normalizeMoney(total, currencyCode),
    }),
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

export function buildUpdatedDraftOrder(
  runtime: ProxyRuntimeContext,
  draftOrder: DraftOrderRecord,
  input: unknown,
  shopifyAdminOrigin: string,
): DraftOrderRecord {
  const inputRecord = typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : {};
  const currencyCode = draftOrder.totalPriceSet?.shopMoney.currencyCode ?? 'CAD';
  const updatedAt = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const lineItems = Object.hasOwn(inputRecord, 'lineItems')
    ? normalizeDraftOrderLineItems(runtime, inputRecord['lineItems'], currencyCode)
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
    paymentTerms: Object.hasOwn(inputRecord, 'paymentTerms')
      ? normalizeDraftOrderPaymentTerms(runtime, inputRecord['paymentTerms'], draftOrder.totalPriceSet)
      : structuredClone(draftOrder.paymentTerms),
    updatedAt,
    lineItems,
  });
}

export function duplicateDraftOrder(
  runtime: ProxyRuntimeContext,
  draftOrder: DraftOrderRecord,
  shopifyAdminOrigin: string,
): DraftOrderRecord {
  const draftOrderId = runtime.syntheticIdentity.makeSyntheticGid('DraftOrder');
  const createdAt = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const invoiceId = draftOrderId.split('/').at(-1) ?? 'draft-order';

  return recalculateDraftOrderTotals({
    ...structuredClone(draftOrder),
    id: draftOrderId,
    name: `#D${runtime.store.getDraftOrders().length + 1}`,
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
      id: runtime.syntheticIdentity.makeSyntheticGid('DraftOrderLineItem'),
      appliedDiscount: null,
      discountedTotalSet: structuredClone(lineItem.originalTotalSet),
      totalDiscountSet: {
        shopMoney: normalizeMoney('0.0', lineItem.originalUnitPriceSet?.shopMoney.currencyCode ?? 'CAD'),
      },
    })),
  });
}

export function buildDraftOrderFromOrder(
  runtime: ProxyRuntimeContext,
  order: OrderRecord,
  shopifyAdminOrigin: string,
): DraftOrderRecord {
  const draftOrderId = runtime.syntheticIdentity.makeSyntheticGid('DraftOrder');
  const createdAt = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const invoiceId = draftOrderId.split('/').at(-1) ?? 'draft-order';
  const currencyCode =
    order.totalPriceSet?.shopMoney.currencyCode ?? order.subtotalPriceSet?.shopMoney.currencyCode ?? 'CAD';

  return {
    id: draftOrderId,
    name: `#D${runtime.store.getDraftOrders().length + 1}`,
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
      id: runtime.syntheticIdentity.makeSyntheticGid('DraftOrderLineItem'),
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

function cloneOrderLineItemsForCalculatedOrder(
  runtime: ProxyRuntimeContext,
  order: OrderRecord,
): OrderLineItemRecord[] {
  return order.lineItems.map((lineItem) => ({
    ...structuredClone(lineItem),
    id: runtime.syntheticIdentity.makeSyntheticGid('CalculatedLineItem'),
    originalLineItemId: lineItem.id,
    isAdded: false,
  }));
}

export function recalculateOrderTotals(order: OrderRecord): OrderRecord {
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

export function buildCalculatedOrderFromOrder(runtime: ProxyRuntimeContext, order: OrderRecord): CalculatedOrderRecord {
  return recalculateOrderTotals({
    ...structuredClone(order),
    id: runtime.syntheticIdentity.makeSyntheticGid('CalculatedOrder'),
    originalOrderId: order.id,
    lineItems: cloneOrderLineItemsForCalculatedOrder(runtime, order),
  } as CalculatedOrderRecord) as CalculatedOrderRecord;
}

export function buildCompletedDraftOrder(runtime: ProxyRuntimeContext, draftOrder: DraftOrderRecord): DraftOrderRecord {
  const completedAt = runtime.syntheticIdentity.makeSyntheticTimestamp();
  return {
    ...structuredClone(draftOrder),
    status: 'COMPLETED',
    ready: true,
    completedAt,
    updatedAt: completedAt,
  };
}

function buildOrderLineItemsFromDraftOrder(
  runtime: ProxyRuntimeContext,
  draftOrder: DraftOrderRecord,
): OrderLineItemRecord[] {
  return draftOrder.lineItems.map((lineItem) => ({
    id: runtime.syntheticIdentity.makeSyntheticGid('LineItem'),
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

export function buildOrderFromCompletedDraftOrder(
  runtime: ProxyRuntimeContext,
  draftOrder: DraftOrderRecord,
  completion: {
    sourceName: string | null;
    paymentPending: boolean;
  },
): OrderRecord {
  const createdAt = draftOrder.completedAt ?? runtime.syntheticIdentity.makeSyntheticTimestamp();
  const currencyCode = draftOrder.totalPriceSet?.shopMoney.currencyCode ?? 'CAD';
  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('Order'),
    name: `#${runtime.store.getOrders().length + 1}`,
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
    customer: buildOrderCustomerFromDraftOrder(runtime, draftOrder),
    shippingLines: buildOrderShippingLinesFromDraftOrder(draftOrder),
    lineItems: buildOrderLineItemsFromDraftOrder(runtime, draftOrder),
    paymentTerms: structuredClone(draftOrder.paymentTerms),
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

export function buildCalculatedLineItemFromVariant(
  runtime: ProxyRuntimeContext,
  variantId: string,
  quantity: number,
): OrderLineItemRecord | null {
  const variant = runtime.store.getEffectiveVariantById(variantId);
  if (!variant) {
    return null;
  }

  const product = runtime.store.getEffectiveProductById(variant.productId);
  const currencyCode = 'CAD';
  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('CalculatedLineItem'),
    originalLineItemId: null,
    isAdded: true,
    title: product?.title ?? variant.title,
    quantity,
    currentQuantity: quantity,
    sku: variant.sku,
    variantId,
    variantTitle: variant.title,
    originalUnitPriceSet: {
      shopMoney: normalizeMoney(formatDecimalAmount(parseDecimalAmount(variant.price)), currencyCode),
    },
    taxLines: [],
  };
}

export function readRefundCreateInput(variables: Record<string, unknown>): Record<string, unknown> | null {
  const input = variables['input'];
  return typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : null;
}

export function readOrderCaptureInput(variables: Record<string, unknown>): Record<string, unknown> | null {
  const input = variables['input'];
  return typeof input === 'object' && input !== null ? (input as Record<string, unknown>) : null;
}

export function readMandatePaymentInput(variables: Record<string, unknown>): Record<string, unknown> {
  const input = variables['input'];
  if (typeof input === 'object' && input !== null) {
    return input as Record<string, unknown>;
  }

  return variables;
}

function readPaymentInputAmount(input: Record<string, unknown>, fallbackAmount: string | number): string | number {
  const amount = input['amount'];
  if (typeof amount === 'string' || typeof amount === 'number') {
    return amount;
  }

  if (typeof amount === 'object' && amount !== null) {
    const amountRecord = amount as Record<string, unknown>;
    if (typeof amountRecord['amount'] === 'string' || typeof amountRecord['amount'] === 'number') {
      return amountRecord['amount'];
    }
  }

  const amountSet = input['amountSet'];
  if (typeof amountSet === 'object' && amountSet !== null) {
    const shopMoney = (amountSet as Record<string, unknown>)['shopMoney'];
    if (typeof shopMoney === 'object' && shopMoney !== null) {
      const rawAmount = (shopMoney as Record<string, unknown>)['amount'];
      if (typeof rawAmount === 'string' || typeof rawAmount === 'number') {
        return rawAmount;
      }
    }
  }

  return fallbackAmount;
}

function readPaymentInputCurrency(input: Record<string, unknown>, fallbackCurrency: string): string {
  if (typeof input['currency'] === 'string') {
    return input['currency'];
  }

  const amount = input['amount'];
  if (
    typeof amount === 'object' &&
    amount !== null &&
    typeof (amount as Record<string, unknown>)['currencyCode'] === 'string'
  ) {
    return (amount as Record<string, unknown>)['currencyCode'] as string;
  }

  const amountSet = input['amountSet'];
  if (typeof amountSet === 'object' && amountSet !== null) {
    const shopMoney = (amountSet as Record<string, unknown>)['shopMoney'];
    if (typeof shopMoney === 'object' && shopMoney !== null) {
      const currencyCode = (shopMoney as Record<string, unknown>)['currencyCode'];
      if (typeof currencyCode === 'string') {
        return currencyCode;
      }
    }
  }

  return fallbackCurrency;
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

function buildRefundLineItems(
  runtime: ProxyRuntimeContext,
  raw: unknown,
  order: OrderRecord,
  currencyCode: string,
): OrderRefundLineItemRecord[] {
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
        id: runtime.syntheticIdentity.makeSyntheticGid('RefundLineItem'),
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

export function sumRefundedAmount(order: OrderRecord): number {
  return order.refunds.reduce((sum, refund) => sum + parseDecimalAmount(refund.totalRefundedSet?.shopMoney.amount), 0);
}

function sumRefundedShippingAmount(order: OrderRecord): number {
  return order.refunds.reduce(
    (sum, refund) => sum + parseDecimalAmount(refund.totalRefundedShippingSet?.shopMoney.amount),
    0,
  );
}

export function makeOrderMoneyBag(amount: number | string, currencyCode: string): { shopMoney: MoneyV2Record } {
  return {
    shopMoney: normalizeMoney(formatDecimalAmount(parseDecimalAmount(amount)), currencyCode),
  };
}

export function readOrderCurrencyCode(order: OrderRecord): string {
  return (
    order.totalPriceSet?.shopMoney.currencyCode ??
    order.currentTotalPriceSet?.shopMoney.currencyCode ??
    order.subtotalPriceSet?.shopMoney.currencyCode ??
    'CAD'
  );
}

export function findOrderTransactionById(
  runtime: ProxyRuntimeContext,
  transactionId: string,
): OrderTransactionRecord | null {
  for (const order of runtime.store.getOrders()) {
    const transaction = order.transactions.find((candidate) => candidate.id === transactionId) ?? null;
    if (transaction) {
      return transaction;
    }
  }

  return null;
}

export function findOrderWithTransaction(
  runtime: ProxyRuntimeContext,
  transactionId: string,
): { order: OrderRecord; transaction: OrderTransactionRecord } | null {
  for (const order of runtime.store.getOrders()) {
    const transaction = order.transactions.find((candidate) => candidate.id === transactionId) ?? null;
    if (transaction) {
      return { order, transaction };
    }
  }

  return null;
}

function isSuccessfulAuthorization(transaction: OrderTransactionRecord): boolean {
  return transaction.kind === 'AUTHORIZATION' && transaction.status === 'SUCCESS';
}

function isSuccessfulPaymentCapture(transaction: OrderTransactionRecord): boolean {
  return (
    transaction.status === 'SUCCESS' &&
    (transaction.kind === 'SALE' || transaction.kind === 'CAPTURE' || transaction.kind === 'MANDATE_PAYMENT')
  );
}

function transactionHasVoidingChild(order: OrderRecord, parentTransactionId: string): boolean {
  return order.transactions.some(
    (transaction) =>
      transaction.kind === 'VOID' &&
      transaction.status === 'SUCCESS' &&
      transaction.parentTransactionId === parentTransactionId,
  );
}

function capturedAmountForAuthorization(order: OrderRecord, parentTransactionId: string): number {
  return order.transactions
    .filter(
      (transaction) =>
        transaction.kind === 'CAPTURE' &&
        transaction.status === 'SUCCESS' &&
        transaction.parentTransactionId === parentTransactionId,
    )
    .reduce((sum, transaction) => sum + parseDecimalAmount(transaction.amountSet?.shopMoney.amount), 0);
}

function capturableAmountForAuthorization(order: OrderRecord, authorization: OrderTransactionRecord): number {
  if (!isSuccessfulAuthorization(authorization) || transactionHasVoidingChild(order, authorization.id)) {
    return 0;
  }

  return Math.max(
    0,
    parseDecimalAmount(authorization.amountSet?.shopMoney.amount) -
      capturedAmountForAuthorization(order, authorization.id),
  );
}

export function totalCapturableAmount(order: OrderRecord): number {
  return order.transactions
    .filter(isSuccessfulAuthorization)
    .reduce((sum, transaction) => sum + capturableAmountForAuthorization(order, transaction), 0);
}

function totalReceivedAmount(order: OrderRecord): number {
  return order.transactions.filter(isSuccessfulPaymentCapture).reduce((sum, transaction) => {
    return sum + parseDecimalAmount(transaction.amountSet?.shopMoney.amount);
  }, 0);
}

function applyPaymentDerivedFields(order: OrderRecord): OrderRecord {
  const currencyCode = readOrderCurrencyCode(order);
  const received = totalReceivedAmount(order);
  const total = parseDecimalAmount(
    order.currentTotalPriceSet?.shopMoney.amount ?? order.totalPriceSet?.shopMoney.amount,
  );
  const outstanding = Math.max(0, total - received);
  const capturableAmount = totalCapturableAmount(order);
  const hasVoidedAuthorization = order.transactions.some(
    (transaction) => isSuccessfulAuthorization(transaction) && transactionHasVoidingChild(order, transaction.id),
  );
  const displayFinancialStatus =
    received >= total && total > 0
      ? 'PAID'
      : received > 0
        ? 'PARTIALLY_PAID'
        : capturableAmount > 0
          ? 'AUTHORIZED'
          : hasVoidedAuthorization
            ? 'VOIDED'
            : (order.displayFinancialStatus ?? 'PENDING');

  return {
    ...order,
    displayFinancialStatus,
    capturable: capturableAmount > 0,
    totalCapturableSet: makeOrderMoneyBag(capturableAmount, currencyCode),
    totalOutstandingSet: makeOrderMoneyBag(outstanding, currencyCode),
    totalReceivedSet: makeOrderMoneyBag(received, currencyCode),
    netPaymentSet: subtractMoney(makeOrderMoneyBag(received, currencyCode), order.totalRefundedSet, currencyCode),
  };
}

export function buildRefundFromInput(
  runtime: ProxyRuntimeContext,
  order: OrderRecord,
  input: Record<string, unknown>,
): OrderRefundRecord {
  const currencyCode = readOrderCurrencyCode(order);
  const refundId = runtime.syntheticIdentity.makeSyntheticGid('Refund');
  const createdAt = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const refundLineItems = buildRefundLineItems(runtime, input['refundLineItems'], order, currencyCode);
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
  const transactions = normalizeOrderTransactions(runtime, rawTransactions, currencyCode);
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
    totalRefundedShippingSet: {
      shopMoney: normalizeMoney(formatDecimalAmount(shippingRefundAmount), currencyCode),
    },
    refundLineItems,
    transactions,
  };
}

export function applyRefundToOrder(order: OrderRecord, refund: OrderRefundRecord): OrderRecord {
  const total = parseDecimalAmount(order.totalPriceSet?.shopMoney.amount);
  const totalRefunded = sumRefundedAmount({ ...order, refunds: [...order.refunds, refund] });
  const displayFinancialStatus = totalRefunded >= total && total > 0 ? 'REFUNDED' : 'PARTIALLY_REFUNDED';
  const currencyCode = readOrderCurrencyCode(order);
  const totalRefundedSet = {
    shopMoney: normalizeMoney(formatDecimalAmount(totalRefunded), currencyCode),
  };
  const totalRefundedShippingSet = {
    shopMoney: normalizeMoney(
      formatDecimalAmount(sumRefundedShippingAmount({ ...order, refunds: [...order.refunds, refund] })),
      currencyCode,
    ),
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
    totalRefundedShippingSet,
    netPaymentSet,
    transactions: [...structuredClone(order.transactions), ...structuredClone(refund.transactions)],
    refunds: [...structuredClone(order.refunds), structuredClone(refund)],
  };
}

function buildOrderTransaction(
  runtime: ProxyRuntimeContext,
  amountSet: OrderTransactionRecord['amountSet'],
  gateway: string,
): OrderTransactionRecord {
  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('OrderTransaction'),
    kind: 'SALE',
    status: 'SUCCESS',
    gateway,
    amountSet: structuredClone(amountSet),
    processedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
  };
}

function buildPaymentTransaction(
  runtime: ProxyRuntimeContext,
  kind: string,
  amountSet: OrderTransactionRecord['amountSet'],
  gateway: string | null,
  parentTransactionId: string | null,
  paymentReferenceId: string | null = null,
  processedAt: string | null = null,
): OrderTransactionRecord {
  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('OrderTransaction'),
    kind,
    status: 'SUCCESS',
    gateway,
    amountSet: structuredClone(amountSet),
    parentTransactionId,
    paymentId: runtime.syntheticIdentity.makeSyntheticGid('Payment'),
    paymentReferenceId,
    processedAt: processedAt ?? runtime.syntheticIdentity.makeSyntheticTimestamp(),
  };
}

export function markOrderAsPaid(runtime: ProxyRuntimeContext, order: OrderRecord, gateway = 'manual'): OrderRecord {
  const currencyCode = readOrderCurrencyCode(order);
  const amountSet = structuredClone(order.totalOutstandingSet ?? order.currentTotalPriceSet ?? order.totalPriceSet);
  const transaction = buildOrderTransaction(runtime, amountSet, gateway);

  return {
    ...structuredClone(order),
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
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

export function createManualPayment(
  runtime: ProxyRuntimeContext,
  order: OrderRecord,
  input: Record<string, unknown>,
):
  | { order: OrderRecord; transaction: OrderTransactionRecord }
  | { userErrors: Array<{ field: string[] | null; message: string }> } {
  const currencyCode = readPaymentInputCurrency(input, readOrderCurrencyCode(order));
  const outstanding = parseDecimalAmount(
    order.totalOutstandingSet?.shopMoney.amount ?? order.currentTotalPriceSet?.shopMoney.amount ?? '0.0',
  );
  const amount = parseDecimalAmount(readPaymentInputAmount(input, outstanding));

  if (outstanding <= 0 || order.displayFinancialStatus === 'PAID') {
    return {
      userErrors: [{ field: ['id'], message: 'Order is already paid' }],
    };
  }

  if (amount <= 0) {
    return {
      userErrors: [{ field: ['amount'], message: 'Amount must be greater than zero' }],
    };
  }

  if (amount > outstanding) {
    return {
      userErrors: [{ field: ['amount'], message: 'Amount exceeds outstanding amount' }],
    };
  }

  const paymentMethodName =
    typeof input['paymentMethodName'] === 'string' && input['paymentMethodName'].trim().length > 0
      ? input['paymentMethodName'].trim()
      : 'manual';
  const processedAt = typeof input['processedAt'] === 'string' ? input['processedAt'] : null;
  const transaction = buildPaymentTransaction(
    runtime,
    'SALE',
    makeOrderMoneyBag(amount, currencyCode),
    paymentMethodName,
    null,
    runtime.syntheticIdentity.makeSyntheticGid('PaymentReference'),
    processedAt,
  );
  const updatedOrder = applyPaymentDerivedFields({
    ...structuredClone(order),
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    paymentGatewayNames: Array.from(new Set([...(order.paymentGatewayNames ?? []), paymentMethodName])),
    transactions: [...structuredClone(order.transactions), transaction],
  });

  return { order: updatedOrder, transaction };
}

export function captureOrderPayment(
  runtime: ProxyRuntimeContext,
  order: OrderRecord,
  authorization: OrderTransactionRecord,
  input: Record<string, unknown>,
):
  | { order: OrderRecord; transaction: OrderTransactionRecord }
  | { userErrors: Array<{ field: string[] | null; message: string }> } {
  const currencyCode = readPaymentInputCurrency(input, readOrderCurrencyCode(order));
  const remainingCapturable = capturableAmountForAuthorization(order, authorization);
  const amount = parseDecimalAmount(readPaymentInputAmount(input, remainingCapturable));

  if (remainingCapturable <= 0) {
    return {
      userErrors: [{ field: ['input', 'parentTransactionId'], message: 'Transaction is not capturable' }],
    };
  }

  if (amount <= 0) {
    return {
      userErrors: [{ field: ['input', 'amount'], message: 'Amount must be greater than zero' }],
    };
  }

  if (amount > remainingCapturable) {
    return {
      userErrors: [{ field: ['input', 'amount'], message: 'Amount exceeds capturable amount' }],
    };
  }

  const transaction = buildPaymentTransaction(
    runtime,
    'CAPTURE',
    makeOrderMoneyBag(amount, currencyCode),
    authorization.gateway,
    authorization.id,
    runtime.syntheticIdentity.makeSyntheticGid('PaymentReference'),
  );
  const finalCapture = input['finalCapture'] === true;
  const remainingAfterCapture = Math.max(0, remainingCapturable - amount);
  const finalVoidTransaction =
    finalCapture && remainingAfterCapture > 0
      ? buildPaymentTransaction(
          runtime,
          'VOID',
          makeOrderMoneyBag(remainingAfterCapture, currencyCode),
          authorization.gateway,
          authorization.id,
        )
      : null;
  const updatedOrder = applyPaymentDerivedFields({
    ...structuredClone(order),
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    paymentGatewayNames: Array.from(
      new Set([...(order.paymentGatewayNames ?? []), ...(authorization.gateway ? [authorization.gateway] : [])]),
    ),
    transactions: [
      ...structuredClone(order.transactions),
      transaction,
      ...(finalVoidTransaction ? [finalVoidTransaction] : []),
    ],
  });

  return { order: updatedOrder, transaction };
}

export function voidOrderTransaction(
  runtime: ProxyRuntimeContext,
  order: OrderRecord,
  authorization: OrderTransactionRecord,
):
  | { order: OrderRecord; transaction: OrderTransactionRecord }
  | { userErrors: Array<{ field: string[] | null; message: string }> } {
  if (!isSuccessfulAuthorization(authorization)) {
    return {
      userErrors: [{ field: ['id'], message: 'Transaction is not voidable' }],
    };
  }

  if (transactionHasVoidingChild(order, authorization.id)) {
    return {
      userErrors: [{ field: ['id'], message: 'Transaction has already been voided' }],
    };
  }

  if (capturedAmountForAuthorization(order, authorization.id) > 0) {
    return {
      userErrors: [{ field: ['id'], message: 'Transaction has already been captured' }],
    };
  }

  const transaction = buildPaymentTransaction(
    runtime,
    'VOID',
    structuredClone(authorization.amountSet),
    authorization.gateway,
    authorization.id,
  );
  const updatedOrder = applyPaymentDerivedFields({
    ...structuredClone(order),
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    transactions: [...structuredClone(order.transactions), transaction],
  });

  return { order: updatedOrder, transaction };
}

export function createMandatePayment(
  runtime: ProxyRuntimeContext,
  order: OrderRecord,
  input: Record<string, unknown>,
):
  | { order: OrderRecord; mandatePayment: OrderMandatePaymentRecord }
  | { userErrors: Array<{ field: string[] | null; message: string }> } {
  const idempotencyKey = typeof input['idempotencyKey'] === 'string' ? input['idempotencyKey'] : null;
  if (!idempotencyKey) {
    return {
      userErrors: [{ field: ['idempotencyKey'], message: 'Idempotency key is required' }],
    };
  }

  const existing = runtime.store.getOrderMandatePayment(order.id, idempotencyKey);
  if (existing) {
    return { order, mandatePayment: existing };
  }

  const currencyCode = readPaymentInputCurrency(input, readOrderCurrencyCode(order));
  const amount = parseDecimalAmount(
    readPaymentInputAmount(
      input,
      order.totalOutstandingSet?.shopMoney.amount ?? order.currentTotalPriceSet?.shopMoney.amount ?? '0.0',
    ),
  );
  if (amount <= 0) {
    return {
      userErrors: [{ field: ['amount'], message: 'Amount must be greater than zero' }],
    };
  }

  const paymentReferenceId = runtime.syntheticIdentity.makeSyntheticGid('PaymentReference');
  const transaction = buildPaymentTransaction(
    runtime,
    'MANDATE_PAYMENT',
    makeOrderMoneyBag(amount, currencyCode),
    'mandate',
    null,
    paymentReferenceId,
  );
  const updatedOrder = applyPaymentDerivedFields({
    ...structuredClone(order),
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    paymentGatewayNames: Array.from(new Set([...(order.paymentGatewayNames ?? []), 'mandate'])),
    transactions: [...structuredClone(order.transactions), transaction],
  });
  const mandatePayment = runtime.store.stageOrderMandatePayment({
    idempotencyKey,
    orderId: order.id,
    jobId: runtime.syntheticIdentity.makeSyntheticGid('Job'),
    paymentReferenceId,
    transactionId: transaction.id,
    createdAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
  });

  return { order: updatedOrder, mandatePayment };
}

export function orderCustomerFromCustomer(customer: CustomerRecord): OrderCustomerRecord {
  return {
    id: customer.id,
    email: customer.email,
    displayName: customer.displayName,
  };
}

export function serializeSelectedUserErrors(
  field: FieldNode,
  userErrors: MutationUserError[],
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
        case 'code':
          result[key] = userError.code ?? null;
          break;
        default:
          result[key] = null;
          break;
      }
    }
    return result;
  });
}
