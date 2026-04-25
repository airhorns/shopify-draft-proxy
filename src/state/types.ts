import { z } from 'zod';

const nullableStringSchema = z.string().nullable();
const nullableNumberSchema = z.number().nullable();
const nullableBooleanSchema = z.boolean().nullable();
const moneyV2Schema = z.strictObject({
  amount: nullableStringSchema,
  currencyCode: nullableStringSchema,
});
const moneySetSchema = z.strictObject({
  shopMoney: moneyV2Schema,
  presentmentMoney: moneyV2Schema.optional(),
});

export const productSeoRecordSchema = z.strictObject({
  title: nullableStringSchema,
  description: nullableStringSchema,
});
export type ProductSeoRecord = z.infer<typeof productSeoRecordSchema>;

export const productCategoryRecordSchema = z.strictObject({
  id: z.string(),
  fullName: nullableStringSchema,
});
export type ProductCategoryRecord = z.infer<typeof productCategoryRecordSchema>;

export const productRecordSchema = z.strictObject({
  id: z.string(),
  legacyResourceId: nullableStringSchema,
  title: z.string(),
  handle: z.string(),
  status: z.enum(['ACTIVE', 'ARCHIVED', 'DRAFT']),
  publicationIds: z.array(z.string()),
  createdAt: z.string(),
  updatedAt: z.string(),
  publishedAt: nullableStringSchema.optional(),
  vendor: nullableStringSchema,
  productType: nullableStringSchema,
  tags: z.array(z.string()),
  totalInventory: nullableNumberSchema,
  tracksInventory: nullableBooleanSchema,
  descriptionHtml: nullableStringSchema,
  onlineStorePreviewUrl: nullableStringSchema,
  templateSuffix: nullableStringSchema,
  seo: productSeoRecordSchema,
  category: productCategoryRecordSchema.nullable(),
});
export type ProductRecord = z.infer<typeof productRecordSchema>;

export const collectionImageRecordSchema = z.strictObject({
  id: nullableStringSchema.optional(),
  altText: nullableStringSchema,
  url: nullableStringSchema,
  width: nullableNumberSchema.optional(),
  height: nullableNumberSchema.optional(),
});
export type CollectionImageRecord = z.infer<typeof collectionImageRecordSchema>;

export const collectionRuleRecordSchema = z.strictObject({
  column: z.string(),
  relation: z.string(),
  condition: z.string(),
  conditionObjectId: nullableStringSchema.optional(),
});
export type CollectionRuleRecord = z.infer<typeof collectionRuleRecordSchema>;

export const collectionRuleSetRecordSchema = z.strictObject({
  appliedDisjunctively: z.boolean(),
  rules: z.array(collectionRuleRecordSchema),
});
export type CollectionRuleSetRecord = z.infer<typeof collectionRuleSetRecordSchema>;

export const productVariantSelectedOptionRecordSchema = z.strictObject({
  name: z.string(),
  value: z.string(),
});
export type ProductVariantSelectedOptionRecord = z.infer<typeof productVariantSelectedOptionRecordSchema>;

export const inventoryItemMeasurementWeightRecordSchema = z.strictObject({
  unit: nullableStringSchema,
  value: nullableNumberSchema,
});
export type InventoryItemMeasurementWeightRecord = z.infer<typeof inventoryItemMeasurementWeightRecordSchema>;

export const inventoryItemMeasurementRecordSchema = z.strictObject({
  weight: inventoryItemMeasurementWeightRecordSchema.nullable(),
});
export type InventoryItemMeasurementRecord = z.infer<typeof inventoryItemMeasurementRecordSchema>;

export const inventoryLevelQuantityRecordSchema = z.strictObject({
  name: z.string(),
  quantity: nullableNumberSchema,
  updatedAt: nullableStringSchema,
});
export type InventoryLevelQuantityRecord = z.infer<typeof inventoryLevelQuantityRecordSchema>;

export const inventoryLevelLocationRecordSchema = z.strictObject({
  id: z.string(),
  name: nullableStringSchema,
});
export type InventoryLevelLocationRecord = z.infer<typeof inventoryLevelLocationRecordSchema>;

export const inventoryLevelRecordSchema = z.strictObject({
  id: z.string(),
  cursor: nullableStringSchema,
  location: inventoryLevelLocationRecordSchema.nullable(),
  quantities: z.array(inventoryLevelQuantityRecordSchema),
});
export type InventoryLevelRecord = z.infer<typeof inventoryLevelRecordSchema>;

export const inventoryItemRecordSchema = z.strictObject({
  id: z.string(),
  tracked: nullableBooleanSchema,
  requiresShipping: nullableBooleanSchema,
  measurement: inventoryItemMeasurementRecordSchema.nullable(),
  countryCodeOfOrigin: nullableStringSchema,
  provinceCodeOfOrigin: nullableStringSchema,
  harmonizedSystemCode: nullableStringSchema,
  inventoryLevels: z.array(inventoryLevelRecordSchema).nullable().optional(),
});
export type InventoryItemRecord = z.infer<typeof inventoryItemRecordSchema>;

export const productVariantRecordSchema = z.strictObject({
  id: z.string(),
  productId: z.string(),
  title: z.string(),
  sku: nullableStringSchema,
  barcode: nullableStringSchema,
  price: nullableStringSchema,
  compareAtPrice: nullableStringSchema,
  taxable: nullableBooleanSchema,
  inventoryPolicy: nullableStringSchema,
  inventoryQuantity: nullableNumberSchema,
  selectedOptions: z.array(productVariantSelectedOptionRecordSchema),
  inventoryItem: inventoryItemRecordSchema.nullable(),
});
export type ProductVariantRecord = z.infer<typeof productVariantRecordSchema>;

export const productOptionValueRecordSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  hasVariants: z.boolean(),
});
export type ProductOptionValueRecord = z.infer<typeof productOptionValueRecordSchema>;

export const productOptionRecordSchema = z.strictObject({
  id: z.string(),
  productId: z.string(),
  name: z.string(),
  position: z.number(),
  optionValues: z.array(productOptionValueRecordSchema),
});
export type ProductOptionRecord = z.infer<typeof productOptionRecordSchema>;

export const collectionRecordSchema = z.strictObject({
  id: z.string(),
  legacyResourceId: nullableStringSchema.optional(),
  title: z.string(),
  handle: z.string(),
  publicationIds: z.array(z.string()).optional(),
  updatedAt: nullableStringSchema.optional(),
  description: nullableStringSchema.optional(),
  descriptionHtml: nullableStringSchema.optional(),
  image: collectionImageRecordSchema.nullable().optional(),
  sortOrder: nullableStringSchema.optional(),
  templateSuffix: nullableStringSchema.optional(),
  seo: productSeoRecordSchema.optional(),
  ruleSet: collectionRuleSetRecordSchema.nullable().optional(),
  redirectNewHandle: nullableBooleanSchema.optional(),
  isSmart: z.boolean().optional(),
});
export type CollectionRecord = z.infer<typeof collectionRecordSchema>;

export const publicationRecordSchema = z.strictObject({
  id: z.string(),
  name: nullableStringSchema,
  cursor: nullableStringSchema.optional(),
});
export type PublicationRecord = z.infer<typeof publicationRecordSchema>;

export const productCollectionRecordSchema = collectionRecordSchema.extend({
  productId: z.string(),
  position: z.number().optional(),
});
export type ProductCollectionRecord = z.infer<typeof productCollectionRecordSchema>;

export const productMediaRecordSchema = z.strictObject({
  key: z.string(),
  productId: z.string(),
  position: z.number(),
  id: nullableStringSchema.optional(),
  mediaContentType: nullableStringSchema,
  alt: nullableStringSchema,
  status: nullableStringSchema.optional(),
  productImageId: nullableStringSchema.optional(),
  imageUrl: nullableStringSchema.optional(),
  imageWidth: nullableNumberSchema.optional(),
  imageHeight: nullableNumberSchema.optional(),
  previewImageUrl: nullableStringSchema,
  sourceUrl: nullableStringSchema.optional(),
});
export type ProductMediaRecord = z.infer<typeof productMediaRecordSchema>;

export const fileRecordSchema = z.strictObject({
  id: z.string(),
  alt: nullableStringSchema,
  contentType: nullableStringSchema,
  createdAt: z.string(),
  fileStatus: z.string(),
  filename: nullableStringSchema,
  originalSource: z.string(),
  imageUrl: nullableStringSchema,
  imageWidth: nullableNumberSchema,
  imageHeight: nullableNumberSchema,
});
export type FileRecord = z.infer<typeof fileRecordSchema>;

export const productMetafieldRecordSchema = z.strictObject({
  id: z.string(),
  productId: z.string(),
  namespace: z.string(),
  key: z.string(),
  type: nullableStringSchema,
  value: nullableStringSchema,
});
export type ProductMetafieldRecord = z.infer<typeof productMetafieldRecordSchema>;

export const customerMetafieldRecordSchema = z.strictObject({
  id: z.string(),
  customerId: z.string(),
  namespace: z.string(),
  key: z.string(),
  type: nullableStringSchema,
  value: nullableStringSchema,
});
export type CustomerMetafieldRecord = z.infer<typeof customerMetafieldRecordSchema>;

export const moneyV2RecordSchema = moneyV2Schema;
export type MoneyV2Record = z.infer<typeof moneyV2RecordSchema>;

export const customerDefaultEmailAddressRecordSchema = z.strictObject({
  emailAddress: nullableStringSchema,
  marketingState: nullableStringSchema.optional(),
  marketingOptInLevel: nullableStringSchema.optional(),
  marketingUpdatedAt: nullableStringSchema.optional(),
});
export type CustomerDefaultEmailAddressRecord = z.infer<typeof customerDefaultEmailAddressRecordSchema>;

export const customerDefaultPhoneNumberRecordSchema = z.strictObject({
  phoneNumber: nullableStringSchema,
  marketingState: nullableStringSchema.optional(),
  marketingOptInLevel: nullableStringSchema.optional(),
  marketingUpdatedAt: nullableStringSchema.optional(),
  marketingCollectedFrom: nullableStringSchema.optional(),
});
export type CustomerDefaultPhoneNumberRecord = z.infer<typeof customerDefaultPhoneNumberRecordSchema>;

export const customerEmailMarketingConsentRecordSchema = z.strictObject({
  marketingState: nullableStringSchema,
  marketingOptInLevel: nullableStringSchema,
  consentUpdatedAt: nullableStringSchema,
});
export type CustomerEmailMarketingConsentRecord = z.infer<typeof customerEmailMarketingConsentRecordSchema>;

export const customerSmsMarketingConsentRecordSchema = z.strictObject({
  marketingState: nullableStringSchema,
  marketingOptInLevel: nullableStringSchema,
  consentUpdatedAt: nullableStringSchema,
  consentCollectedFrom: nullableStringSchema,
});
export type CustomerSmsMarketingConsentRecord = z.infer<typeof customerSmsMarketingConsentRecordSchema>;

export const customerDefaultAddressRecordSchema = z.strictObject({
  address1: nullableStringSchema,
  city: nullableStringSchema,
  province: nullableStringSchema,
  country: nullableStringSchema,
  zip: nullableStringSchema,
  formattedArea: nullableStringSchema,
});
export type CustomerDefaultAddressRecord = z.infer<typeof customerDefaultAddressRecordSchema>;

export const customerRecordSchema = z.strictObject({
  id: z.string(),
  firstName: nullableStringSchema,
  lastName: nullableStringSchema,
  displayName: nullableStringSchema,
  email: nullableStringSchema,
  legacyResourceId: nullableStringSchema,
  locale: nullableStringSchema,
  note: nullableStringSchema,
  canDelete: nullableBooleanSchema,
  verifiedEmail: nullableBooleanSchema,
  taxExempt: nullableBooleanSchema,
  taxExemptions: z.array(z.string()).optional(),
  state: nullableStringSchema,
  tags: z.array(z.string()),
  numberOfOrders: z.union([z.string(), z.number()]).nullable(),
  amountSpent: moneyV2RecordSchema.nullable(),
  defaultEmailAddress: customerDefaultEmailAddressRecordSchema.nullable(),
  defaultPhoneNumber: customerDefaultPhoneNumberRecordSchema.nullable(),
  emailMarketingConsent: customerEmailMarketingConsentRecordSchema.nullable().optional(),
  smsMarketingConsent: customerSmsMarketingConsentRecordSchema.nullable().optional(),
  defaultAddress: customerDefaultAddressRecordSchema.nullable(),
  createdAt: nullableStringSchema,
  updatedAt: nullableStringSchema,
});
export type CustomerRecord = z.infer<typeof customerRecordSchema>;

export const businessEntityAddressRecordSchema = z.strictObject({
  address1: nullableStringSchema,
  address2: nullableStringSchema,
  city: nullableStringSchema,
  countryCode: z.string(),
  province: nullableStringSchema,
  zip: nullableStringSchema,
});
export type BusinessEntityAddressRecord = z.infer<typeof businessEntityAddressRecordSchema>;

export const shopifyPaymentsAccountRecordSchema = z.strictObject({
  id: z.string(),
  activated: z.boolean(),
  country: z.string(),
  defaultCurrency: z.string(),
  onboardable: z.boolean(),
});
export type ShopifyPaymentsAccountRecord = z.infer<typeof shopifyPaymentsAccountRecordSchema>;

export const businessEntityRecordSchema = z.strictObject({
  id: z.string(),
  displayName: z.string(),
  companyName: nullableStringSchema,
  primary: z.boolean(),
  archived: z.boolean(),
  address: businessEntityAddressRecordSchema,
  shopifyPaymentsAccount: shopifyPaymentsAccountRecordSchema.nullable(),
});
export type BusinessEntityRecord = z.infer<typeof businessEntityRecordSchema>;

export const productCatalogPageInfoRecordSchema = z.strictObject({
  hasNextPage: z.boolean(),
  hasPreviousPage: z.boolean(),
  startCursor: nullableStringSchema,
  endCursor: nullableStringSchema,
});
export type ProductCatalogPageInfoRecord = z.infer<typeof productCatalogPageInfoRecordSchema>;

export const productCatalogConnectionRecordSchema = z.strictObject({
  orderedProductIds: z.array(z.string()),
  cursorByProductId: z.record(z.string(), z.string()),
  pageInfo: productCatalogPageInfoRecordSchema,
});
export type ProductCatalogConnectionRecord = z.infer<typeof productCatalogConnectionRecordSchema>;

export const customerCatalogPageInfoRecordSchema = z.strictObject({
  hasNextPage: z.boolean(),
  hasPreviousPage: z.boolean(),
  startCursor: nullableStringSchema,
  endCursor: nullableStringSchema,
});
export type CustomerCatalogPageInfoRecord = z.infer<typeof customerCatalogPageInfoRecordSchema>;

export const customerCatalogConnectionRecordSchema = z.strictObject({
  orderedCustomerIds: z.array(z.string()),
  cursorByCustomerId: z.record(z.string(), z.string()),
  pageInfo: customerCatalogPageInfoRecordSchema,
});
export type CustomerCatalogConnectionRecord = z.infer<typeof customerCatalogConnectionRecordSchema>;

export const draftOrderAttributeRecordSchema = z.strictObject({
  key: z.string(),
  value: nullableStringSchema,
});
export type DraftOrderAttributeRecord = z.infer<typeof draftOrderAttributeRecordSchema>;

export const draftOrderAddressRecordSchema = z.strictObject({
  firstName: nullableStringSchema,
  lastName: nullableStringSchema,
  address1: nullableStringSchema,
  address2: nullableStringSchema.optional(),
  company: nullableStringSchema.optional(),
  city: nullableStringSchema,
  province: nullableStringSchema.optional(),
  provinceCode: nullableStringSchema,
  country: nullableStringSchema.optional(),
  countryCodeV2: nullableStringSchema,
  zip: nullableStringSchema,
  phone: nullableStringSchema,
});
export type DraftOrderAddressRecord = z.infer<typeof draftOrderAddressRecordSchema>;

export const draftOrderShippingLineRecordSchema = z.strictObject({
  title: nullableStringSchema,
  code: nullableStringSchema,
  originalPriceSet: moneySetSchema.nullable(),
});
export type DraftOrderShippingLineRecord = z.infer<typeof draftOrderShippingLineRecordSchema>;

export const draftOrderAppliedDiscountRecordSchema = z.strictObject({
  title: nullableStringSchema,
  description: nullableStringSchema,
  value: nullableNumberSchema,
  valueType: nullableStringSchema,
  amountSet: moneySetSchema.nullable(),
});
export type DraftOrderAppliedDiscountRecord = z.infer<typeof draftOrderAppliedDiscountRecordSchema>;

export const draftOrderCustomerRecordSchema = z.strictObject({
  id: nullableStringSchema,
  email: nullableStringSchema,
  displayName: nullableStringSchema,
});
export type DraftOrderCustomerRecord = z.infer<typeof draftOrderCustomerRecordSchema>;

export const draftOrderPaymentTermsRecordSchema = z.strictObject({
  id: z.string(),
  due: z.boolean(),
  overdue: z.boolean(),
  dueInDays: nullableNumberSchema,
  paymentTermsName: z.string(),
  paymentTermsType: z.string(),
  translatedName: z.string(),
});
export type DraftOrderPaymentTermsRecord = z.infer<typeof draftOrderPaymentTermsRecordSchema>;

export const draftOrderLineItemRecordSchema = z.strictObject({
  id: z.string(),
  title: nullableStringSchema,
  name: nullableStringSchema,
  quantity: z.number(),
  sku: nullableStringSchema,
  variantTitle: nullableStringSchema,
  variantId: nullableStringSchema,
  productId: nullableStringSchema,
  custom: z.boolean(),
  requiresShipping: z.boolean(),
  taxable: z.boolean(),
  customAttributes: z.array(draftOrderAttributeRecordSchema),
  appliedDiscount: draftOrderAppliedDiscountRecordSchema.nullable(),
  originalUnitPriceSet: moneySetSchema.nullable(),
  originalTotalSet: moneySetSchema.nullable(),
  discountedTotalSet: moneySetSchema.nullable(),
  totalDiscountSet: moneySetSchema.nullable(),
});
export type DraftOrderLineItemRecord = z.infer<typeof draftOrderLineItemRecordSchema>;

export const draftOrderRecordSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  orderId: nullableStringSchema.optional(),
  completedAt: nullableStringSchema.optional(),
  invoiceUrl: nullableStringSchema,
  status: nullableStringSchema,
  ready: nullableBooleanSchema,
  email: nullableStringSchema,
  note: nullableStringSchema,
  tags: z.array(z.string()),
  customer: draftOrderCustomerRecordSchema.nullable(),
  taxExempt: z.boolean(),
  taxesIncluded: z.boolean(),
  reserveInventoryUntil: nullableStringSchema,
  paymentTerms: draftOrderPaymentTermsRecordSchema.nullable(),
  appliedDiscount: draftOrderAppliedDiscountRecordSchema.nullable(),
  customAttributes: z.array(draftOrderAttributeRecordSchema),
  billingAddress: draftOrderAddressRecordSchema.nullable(),
  shippingAddress: draftOrderAddressRecordSchema.nullable(),
  shippingLine: draftOrderShippingLineRecordSchema.nullable(),
  createdAt: z.string(),
  updatedAt: z.string(),
  subtotalPriceSet: moneySetSchema.nullable(),
  totalDiscountsSet: moneySetSchema.nullable(),
  totalShippingPriceSet: moneySetSchema.nullable(),
  totalPriceSet: moneySetSchema.nullable(),
  lineItems: z.array(draftOrderLineItemRecordSchema),
});
export type DraftOrderRecord = z.infer<typeof draftOrderRecordSchema>;

export const orderCustomerRecordSchema = z.strictObject({
  id: z.string(),
  email: nullableStringSchema,
  displayName: nullableStringSchema,
});
export type OrderCustomerRecord = z.infer<typeof orderCustomerRecordSchema>;

export const orderTaxLineRecordSchema = z.strictObject({
  title: nullableStringSchema,
  rate: nullableNumberSchema,
  channelLiable: nullableBooleanSchema,
  priceSet: moneySetSchema.nullable(),
});
export type OrderTaxLineRecord = z.infer<typeof orderTaxLineRecordSchema>;

export const orderShippingLineRecordSchema = z.strictObject({
  title: nullableStringSchema,
  code: nullableStringSchema,
  source: nullableStringSchema.optional(),
  originalPriceSet: moneySetSchema.nullable(),
  taxLines: z.array(orderTaxLineRecordSchema).optional(),
});
export type OrderShippingLineRecord = z.infer<typeof orderShippingLineRecordSchema>;

export const orderMetafieldRecordSchema = z.strictObject({
  id: z.string(),
  orderId: z.string(),
  namespace: z.string(),
  key: z.string(),
  type: nullableStringSchema,
  value: nullableStringSchema,
});
export type OrderMetafieldRecord = z.infer<typeof orderMetafieldRecordSchema>;

export const orderLineItemRecordSchema = z.strictObject({
  id: z.string(),
  originalLineItemId: nullableStringSchema.optional(),
  title: nullableStringSchema,
  quantity: z.number(),
  currentQuantity: z.number().optional(),
  sku: nullableStringSchema,
  variantId: nullableStringSchema.optional(),
  variantTitle: nullableStringSchema,
  originalUnitPriceSet: moneySetSchema.nullable(),
  taxLines: z.array(orderTaxLineRecordSchema).optional(),
  isAdded: z.boolean().optional(),
});
export type OrderLineItemRecord = z.infer<typeof orderLineItemRecordSchema>;

export const orderFulfillmentLineItemRecordSchema = z.strictObject({
  id: z.string(),
  lineItemId: nullableStringSchema,
  title: nullableStringSchema,
  quantity: z.number(),
});
export type OrderFulfillmentLineItemRecord = z.infer<typeof orderFulfillmentLineItemRecordSchema>;

export const orderFulfillmentTrackingInfoRecordSchema = z.strictObject({
  number: nullableStringSchema,
  url: nullableStringSchema,
  company: nullableStringSchema,
});
export type OrderFulfillmentTrackingInfoRecord = z.infer<typeof orderFulfillmentTrackingInfoRecordSchema>;

export const orderFulfillmentRecordSchema = z.strictObject({
  id: z.string(),
  status: nullableStringSchema,
  displayStatus: nullableStringSchema.optional(),
  createdAt: nullableStringSchema.optional(),
  updatedAt: nullableStringSchema.optional(),
  trackingInfo: z.array(orderFulfillmentTrackingInfoRecordSchema).optional(),
  fulfillmentLineItems: z.array(orderFulfillmentLineItemRecordSchema).optional(),
});
export type OrderFulfillmentRecord = z.infer<typeof orderFulfillmentRecordSchema>;

export const orderFulfillmentOrderAssignedLocationRecordSchema = z.strictObject({
  name: nullableStringSchema,
});
export type OrderFulfillmentOrderAssignedLocationRecord = z.infer<
  typeof orderFulfillmentOrderAssignedLocationRecordSchema
>;

export const orderFulfillmentOrderLineItemRecordSchema = z.strictObject({
  id: z.string(),
  lineItemId: nullableStringSchema,
  title: nullableStringSchema,
  totalQuantity: z.number(),
  remainingQuantity: z.number(),
});
export type OrderFulfillmentOrderLineItemRecord = z.infer<typeof orderFulfillmentOrderLineItemRecordSchema>;

export const orderFulfillmentOrderRecordSchema = z.strictObject({
  id: z.string(),
  status: nullableStringSchema,
  requestStatus: nullableStringSchema.optional(),
  assignedLocation: orderFulfillmentOrderAssignedLocationRecordSchema.nullable().optional(),
  lineItems: z.array(orderFulfillmentOrderLineItemRecordSchema).optional(),
});
export type OrderFulfillmentOrderRecord = z.infer<typeof orderFulfillmentOrderRecordSchema>;

export const orderDiscountApplicationRecordSchema = z.strictObject({
  code: nullableStringSchema,
  value: z.strictObject({
    type: z.enum(['money', 'percentage']),
    amount: nullableStringSchema.optional(),
    currencyCode: nullableStringSchema.optional(),
    percentage: nullableNumberSchema.optional(),
  }),
});
export type OrderDiscountApplicationRecord = z.infer<typeof orderDiscountApplicationRecordSchema>;

export const orderTransactionRecordSchema = z.strictObject({
  id: z.string(),
  kind: nullableStringSchema,
  status: nullableStringSchema,
  gateway: nullableStringSchema,
  amountSet: moneySetSchema.nullable(),
});
export type OrderTransactionRecord = z.infer<typeof orderTransactionRecordSchema>;

export const orderRefundLineItemRecordSchema = z.strictObject({
  id: z.string(),
  lineItemId: z.string(),
  title: nullableStringSchema,
  quantity: z.number(),
  restockType: nullableStringSchema,
  subtotalSet: moneySetSchema.nullable(),
});
export type OrderRefundLineItemRecord = z.infer<typeof orderRefundLineItemRecordSchema>;

export const orderRefundRecordSchema = z.strictObject({
  id: z.string(),
  note: nullableStringSchema,
  createdAt: z.string(),
  updatedAt: z.string(),
  totalRefundedSet: moneySetSchema.nullable(),
  refundLineItems: z.array(orderRefundLineItemRecordSchema),
  transactions: z.array(orderTransactionRecordSchema),
});
export type OrderRefundRecord = z.infer<typeof orderRefundRecordSchema>;

export const orderReturnRecordSchema = z.strictObject({
  id: z.string(),
  status: nullableStringSchema,
});
export type OrderReturnRecord = z.infer<typeof orderReturnRecordSchema>;

export const orderRecordSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  createdAt: z.string(),
  updatedAt: z.string(),
  closed: z.boolean().optional(),
  closedAt: nullableStringSchema.optional(),
  cancelledAt: nullableStringSchema.optional(),
  cancelReason: nullableStringSchema.optional(),
  sourceName: nullableStringSchema.optional(),
  paymentGatewayNames: z.array(z.string()).optional(),
  email: nullableStringSchema.optional(),
  phone: nullableStringSchema.optional(),
  poNumber: nullableStringSchema.optional(),
  displayFinancialStatus: nullableStringSchema,
  displayFulfillmentStatus: nullableStringSchema,
  note: nullableStringSchema,
  tags: z.array(z.string()),
  customAttributes: z.array(draftOrderAttributeRecordSchema),
  metafields: z.array(orderMetafieldRecordSchema).optional(),
  billingAddress: draftOrderAddressRecordSchema.nullable(),
  shippingAddress: draftOrderAddressRecordSchema.nullable(),
  subtotalPriceSet: moneySetSchema.nullable(),
  currentSubtotalPriceSet: moneySetSchema.nullable().optional(),
  currentTotalPriceSet: moneySetSchema.nullable(),
  currentTotalDiscountsSet: moneySetSchema.nullable().optional(),
  currentTotalTaxSet: moneySetSchema.nullable().optional(),
  totalPriceSet: moneySetSchema.nullable(),
  totalOutstandingSet: moneySetSchema.nullable().optional(),
  totalReceivedSet: moneySetSchema.nullable().optional(),
  netPaymentSet: moneySetSchema.nullable().optional(),
  totalRefundedSet: moneySetSchema.nullable(),
  totalRefundedShippingSet: moneySetSchema.nullable().optional(),
  totalShippingPriceSet: moneySetSchema.nullable().optional(),
  totalTaxSet: moneySetSchema.nullable().optional(),
  totalDiscountsSet: moneySetSchema.nullable().optional(),
  discountCodes: z.array(z.string()).optional(),
  discountApplications: z.array(orderDiscountApplicationRecordSchema).optional(),
  taxLines: z.array(orderTaxLineRecordSchema).optional(),
  taxesIncluded: nullableBooleanSchema.optional(),
  customer: orderCustomerRecordSchema.nullable(),
  shippingLines: z.array(orderShippingLineRecordSchema),
  lineItems: z.array(orderLineItemRecordSchema),
  fulfillments: z.array(orderFulfillmentRecordSchema).optional(),
  fulfillmentOrders: z.array(orderFulfillmentOrderRecordSchema).optional(),
  transactions: z.array(orderTransactionRecordSchema),
  refunds: z.array(orderRefundRecordSchema),
  returns: z.array(orderReturnRecordSchema),
});
export type OrderRecord = z.infer<typeof orderRecordSchema>;

export const shopDomainRecordSchema = z.strictObject({
  id: z.string(),
  host: z.string(),
  url: z.string(),
  sslEnabled: z.boolean(),
});
export type ShopDomainRecord = z.infer<typeof shopDomainRecordSchema>;

export const shopAddressRecordSchema = z.strictObject({
  id: z.string(),
  address1: nullableStringSchema,
  address2: nullableStringSchema,
  city: nullableStringSchema,
  company: nullableStringSchema,
  coordinatesValidated: z.boolean(),
  country: nullableStringSchema,
  countryCodeV2: nullableStringSchema,
  formatted: z.array(z.string()),
  formattedArea: nullableStringSchema,
  latitude: nullableNumberSchema,
  longitude: nullableNumberSchema,
  phone: nullableStringSchema,
  province: nullableStringSchema,
  provinceCode: nullableStringSchema,
  zip: nullableStringSchema,
});
export type ShopAddressRecord = z.infer<typeof shopAddressRecordSchema>;

export const shopPlanRecordSchema = z.strictObject({
  partnerDevelopment: z.boolean(),
  publicDisplayName: z.string(),
  shopifyPlus: z.boolean(),
});
export type ShopPlanRecord = z.infer<typeof shopPlanRecordSchema>;

export const shopResourceLimitsRecordSchema = z.strictObject({
  locationLimit: z.number(),
  maxProductOptions: z.number(),
  maxProductVariants: z.number(),
  redirectLimitReached: z.boolean(),
});
export type ShopResourceLimitsRecord = z.infer<typeof shopResourceLimitsRecordSchema>;

export const shopBundlesFeatureRecordSchema = z.strictObject({
  eligibleForBundles: z.boolean(),
  ineligibilityReason: nullableStringSchema,
  sellsBundles: z.boolean(),
});
export type ShopBundlesFeatureRecord = z.infer<typeof shopBundlesFeatureRecordSchema>;

export const shopCartTransformEligibleOperationsRecordSchema = z.strictObject({
  expandOperation: z.boolean(),
  mergeOperation: z.boolean(),
  updateOperation: z.boolean(),
});
export type ShopCartTransformEligibleOperationsRecord = z.infer<typeof shopCartTransformEligibleOperationsRecordSchema>;

export const shopCartTransformFeatureRecordSchema = z.strictObject({
  eligibleOperations: shopCartTransformEligibleOperationsRecordSchema,
});
export type ShopCartTransformFeatureRecord = z.infer<typeof shopCartTransformFeatureRecordSchema>;

export const shopFeaturesRecordSchema = z.strictObject({
  avalaraAvatax: z.boolean(),
  branding: z.string(),
  bundles: shopBundlesFeatureRecordSchema,
  captcha: z.boolean(),
  cartTransform: shopCartTransformFeatureRecordSchema,
  dynamicRemarketing: z.boolean(),
  eligibleForSubscriptionMigration: z.boolean(),
  eligibleForSubscriptions: z.boolean(),
  giftCards: z.boolean(),
  harmonizedSystemCode: z.boolean(),
  legacySubscriptionGatewayEnabled: z.boolean(),
  liveView: z.boolean(),
  paypalExpressSubscriptionGatewayStatus: z.string(),
  reports: z.boolean(),
  sellsSubscriptions: z.boolean(),
  showMetrics: z.boolean(),
  storefront: z.boolean(),
  unifiedMarkets: z.boolean(),
});
export type ShopFeaturesRecord = z.infer<typeof shopFeaturesRecordSchema>;

export const paymentSettingsRecordSchema = z.strictObject({
  supportedDigitalWallets: z.array(z.string()),
});
export type PaymentSettingsRecord = z.infer<typeof paymentSettingsRecordSchema>;

export const shopPolicyRecordSchema = z.strictObject({
  id: z.string(),
  title: z.string(),
  body: z.string(),
  type: z.string(),
  url: z.string(),
  createdAt: z.string(),
  updatedAt: z.string(),
});
export type ShopPolicyRecord = z.infer<typeof shopPolicyRecordSchema>;

export const shopRecordSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  myshopifyDomain: z.string(),
  url: z.string(),
  primaryDomain: shopDomainRecordSchema,
  contactEmail: z.string(),
  email: z.string(),
  currencyCode: z.string(),
  enabledPresentmentCurrencies: z.array(z.string()),
  ianaTimezone: z.string(),
  timezoneAbbreviation: z.string(),
  timezoneOffset: z.string(),
  timezoneOffsetMinutes: z.number(),
  taxesIncluded: z.boolean(),
  taxShipping: z.boolean(),
  unitSystem: z.string(),
  weightUnit: z.string(),
  shopAddress: shopAddressRecordSchema,
  plan: shopPlanRecordSchema,
  resourceLimits: shopResourceLimitsRecordSchema,
  features: shopFeaturesRecordSchema,
  paymentSettings: paymentSettingsRecordSchema,
  shopPolicies: z.array(shopPolicyRecordSchema),
});
export type ShopRecord = z.infer<typeof shopRecordSchema>;

export const calculatedOrderRecordSchema = orderRecordSchema.extend({
  originalOrderId: z.string(),
});
export type CalculatedOrderRecord = z.infer<typeof calculatedOrderRecordSchema>;

export const stateSnapshotSchema = z.strictObject({
  shop: shopRecordSchema.nullable().default(null),
  products: z.record(z.string(), productRecordSchema),
  productVariants: z.record(z.string(), productVariantRecordSchema),
  productOptions: z.record(z.string(), productOptionRecordSchema),
  collections: z.record(z.string(), collectionRecordSchema),
  publications: z.record(z.string(), publicationRecordSchema).default({}),
  customers: z.record(z.string(), customerRecordSchema),
  businessEntities: z.record(z.string(), businessEntityRecordSchema).default({}),
  businessEntityOrder: z.array(z.string()).default([]),
  productCollections: z.record(z.string(), productCollectionRecordSchema),
  productMedia: z.record(z.string(), productMediaRecordSchema),
  files: z.record(z.string(), fileRecordSchema).default({}),
  productMetafields: z.record(z.string(), productMetafieldRecordSchema),
  customerMetafields: z.record(z.string(), customerMetafieldRecordSchema).default({}),
  deletedProductIds: z.record(z.string(), z.literal(true)),
  deletedFileIds: z.record(z.string(), z.literal(true)).default({}),
  deletedCollectionIds: z.record(z.string(), z.literal(true)),
  deletedCustomerIds: z.record(z.string(), z.literal(true)),
});
export type StateSnapshot = z.infer<typeof stateSnapshotSchema>;

export const normalizedStateSnapshotFileSchema = z.strictObject({
  kind: z.literal('normalized-state-snapshot').optional(),
  baseState: stateSnapshotSchema,
  productSearchConnections: z.record(z.string(), productCatalogConnectionRecordSchema).optional(),
  customerCatalogConnection: customerCatalogConnectionRecordSchema.nullable().optional(),
  customerSearchConnections: z.record(z.string(), customerCatalogConnectionRecordSchema).optional(),
});
export type NormalizedStateSnapshotFile = z.infer<typeof normalizedStateSnapshotFileSchema>;

export interface MutationLogInterpretedMetadata {
  operationType: 'query' | 'mutation';
  operationName: string | null;
  rootFields: string[];
  primaryRootField: string | null;
  capability: {
    operationName: string | null;
    domain: string;
    execution: string;
  };
}

export interface MutationLogEntry {
  id: string;
  receivedAt: string;
  operationName: string | null;
  path: string;
  query: string;
  variables: Record<string, unknown>;
  requestBody?: Record<string, unknown>;
  stagedResourceIds?: string[];
  status: 'staged' | 'proxied' | 'committed' | 'failed';
  interpreted: MutationLogInterpretedMetadata;
  notes?: string;
}
