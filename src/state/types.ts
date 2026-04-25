import { z } from 'zod';
import { jsonObjectSchema, jsonValueSchema } from '../json-schemas.js';

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

export const locationAddressRecordSchema = z.strictObject({
  address1: nullableStringSchema,
  address2: nullableStringSchema,
  city: nullableStringSchema,
  country: nullableStringSchema,
  countryCode: nullableStringSchema,
  formatted: z.array(z.string()),
  latitude: nullableNumberSchema,
  longitude: nullableNumberSchema,
  phone: nullableStringSchema,
  province: nullableStringSchema,
  provinceCode: nullableStringSchema,
  zip: nullableStringSchema,
});
export type LocationAddressRecord = z.infer<typeof locationAddressRecordSchema>;

export const locationSuggestedAddressRecordSchema = z.strictObject({
  address1: nullableStringSchema,
  countryCode: nullableStringSchema,
  formatted: z.array(z.string()),
});
export type LocationSuggestedAddressRecord = z.infer<typeof locationSuggestedAddressRecordSchema>;

export const locationFulfillmentServiceRecordSchema = z.strictObject({
  id: nullableStringSchema,
  handle: nullableStringSchema,
  serviceName: nullableStringSchema,
  callbackUrl: nullableStringSchema.optional(),
  inventoryManagement: nullableBooleanSchema.optional(),
  locationId: nullableStringSchema.optional(),
  requiresShippingMethod: nullableBooleanSchema.optional(),
  trackingSupport: nullableBooleanSchema.optional(),
  type: nullableStringSchema.optional(),
});
export type LocationFulfillmentServiceRecord = z.infer<typeof locationFulfillmentServiceRecordSchema>;

export const fulfillmentServiceRecordSchema = z.strictObject({
  id: z.string(),
  handle: z.string(),
  serviceName: z.string(),
  callbackUrl: nullableStringSchema,
  inventoryManagement: z.boolean(),
  locationId: nullableStringSchema,
  requiresShippingMethod: z.boolean(),
  trackingSupport: z.boolean(),
  type: z.string(),
});
export type FulfillmentServiceRecord = z.infer<typeof fulfillmentServiceRecordSchema>;

export const carrierServiceRecordSchema = z.strictObject({
  id: z.string(),
  name: nullableStringSchema,
  formattedName: nullableStringSchema,
  callbackUrl: nullableStringSchema,
  active: z.boolean(),
  supportsServiceDiscovery: z.boolean(),
  createdAt: z.string(),
  updatedAt: z.string(),
});
export type CarrierServiceRecord = z.infer<typeof carrierServiceRecordSchema>;

export const locationMetafieldRecordSchema = z.strictObject({
  id: z.string(),
  locationId: z.string(),
  namespace: z.string(),
  key: z.string(),
  type: nullableStringSchema,
  value: nullableStringSchema,
  compareDigest: nullableStringSchema.optional(),
  jsonValue: jsonValueSchema.optional(),
  createdAt: nullableStringSchema.optional(),
  updatedAt: nullableStringSchema.optional(),
  ownerType: nullableStringSchema.optional(),
});
export type LocationMetafieldRecord = z.infer<typeof locationMetafieldRecordSchema>;

export const locationRecordSchema = z.strictObject({
  id: z.string(),
  name: nullableStringSchema,
  legacyResourceId: nullableStringSchema.optional(),
  activatable: nullableBooleanSchema.optional(),
  addressVerified: nullableBooleanSchema.optional(),
  createdAt: nullableStringSchema.optional(),
  deactivatable: nullableBooleanSchema.optional(),
  deactivatedAt: nullableStringSchema.optional(),
  deletable: nullableBooleanSchema.optional(),
  fulfillmentService: locationFulfillmentServiceRecordSchema.nullable().optional(),
  fulfillsOnlineOrders: nullableBooleanSchema.optional(),
  hasActiveInventory: nullableBooleanSchema.optional(),
  hasUnfulfilledOrders: nullableBooleanSchema.optional(),
  isActive: nullableBooleanSchema.optional(),
  isFulfillmentService: nullableBooleanSchema.optional(),
  shipsInventory: nullableBooleanSchema.optional(),
  updatedAt: nullableStringSchema.optional(),
  deleted: z.boolean().optional(),
  address: locationAddressRecordSchema.nullable().optional(),
  suggestedAddresses: z.array(locationSuggestedAddressRecordSchema).optional(),
  metafields: z.array(locationMetafieldRecordSchema).optional(),
});
export type LocationRecord = z.infer<typeof locationRecordSchema>;

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
  productId: z.string().optional(),
  ownerId: z.string().optional(),
  namespace: z.string(),
  key: z.string(),
  type: nullableStringSchema,
  value: nullableStringSchema,
  compareDigest: nullableStringSchema.optional(),
  jsonValue: jsonValueSchema.optional(),
  createdAt: nullableStringSchema.optional(),
  updatedAt: nullableStringSchema.optional(),
  ownerType: nullableStringSchema.optional(),
});
export type ProductMetafieldRecord = z.infer<typeof productMetafieldRecordSchema>;

export const metafieldDefinitionCapabilityRecordSchema = z.strictObject({
  enabled: z.boolean(),
  eligible: z.boolean(),
  status: nullableStringSchema.optional(),
});
export type MetafieldDefinitionCapabilityRecord = z.infer<typeof metafieldDefinitionCapabilityRecordSchema>;

export const metafieldDefinitionCapabilitiesRecordSchema = z.strictObject({
  adminFilterable: metafieldDefinitionCapabilityRecordSchema,
  smartCollectionCondition: metafieldDefinitionCapabilityRecordSchema,
  uniqueValues: metafieldDefinitionCapabilityRecordSchema,
});
export type MetafieldDefinitionCapabilitiesRecord = z.infer<typeof metafieldDefinitionCapabilitiesRecordSchema>;

export const metafieldDefinitionConstraintValueRecordSchema = z.strictObject({
  value: z.string(),
});
export type MetafieldDefinitionConstraintValueRecord = z.infer<typeof metafieldDefinitionConstraintValueRecordSchema>;

export const metafieldDefinitionConstraintsRecordSchema = z.strictObject({
  key: nullableStringSchema,
  values: z.array(metafieldDefinitionConstraintValueRecordSchema),
});
export type MetafieldDefinitionConstraintsRecord = z.infer<typeof metafieldDefinitionConstraintsRecordSchema>;

export const metafieldDefinitionTypeRecordSchema = z.strictObject({
  name: z.string(),
  category: nullableStringSchema.optional(),
});
export type MetafieldDefinitionTypeRecord = z.infer<typeof metafieldDefinitionTypeRecordSchema>;

export const metafieldDefinitionValidationRecordSchema = z.strictObject({
  name: z.string(),
  value: nullableStringSchema,
});
export type MetafieldDefinitionValidationRecord = z.infer<typeof metafieldDefinitionValidationRecordSchema>;

export const metafieldDefinitionRecordSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  namespace: z.string(),
  key: z.string(),
  ownerType: z.string(),
  type: metafieldDefinitionTypeRecordSchema,
  description: nullableStringSchema,
  validations: z.array(metafieldDefinitionValidationRecordSchema),
  access: z.record(z.string(), jsonValueSchema),
  capabilities: metafieldDefinitionCapabilitiesRecordSchema,
  constraints: metafieldDefinitionConstraintsRecordSchema.nullable(),
  pinnedPosition: nullableNumberSchema,
  validationStatus: z.string(),
});
export type MetafieldDefinitionRecord = z.infer<typeof metafieldDefinitionRecordSchema>;

export const metaobjectDefinitionCapabilityRecordSchema = z.strictObject({
  enabled: z.boolean(),
});
export type MetaobjectDefinitionCapabilityRecord = z.infer<typeof metaobjectDefinitionCapabilityRecordSchema>;

export const metaobjectDefinitionCapabilitiesRecordSchema = z.strictObject({
  publishable: metaobjectDefinitionCapabilityRecordSchema.optional(),
  translatable: metaobjectDefinitionCapabilityRecordSchema.optional(),
  renderable: metaobjectDefinitionCapabilityRecordSchema.optional(),
  onlineStore: metaobjectDefinitionCapabilityRecordSchema.optional(),
});
export type MetaobjectDefinitionCapabilitiesRecord = z.infer<typeof metaobjectDefinitionCapabilitiesRecordSchema>;

export const metaobjectDefinitionTypeRecordSchema = z.strictObject({
  name: z.string(),
  category: nullableStringSchema.optional(),
});
export type MetaobjectDefinitionTypeRecord = z.infer<typeof metaobjectDefinitionTypeRecordSchema>;

export const metaobjectFieldDefinitionValidationRecordSchema = z.strictObject({
  name: z.string(),
  value: nullableStringSchema,
});
export type MetaobjectFieldDefinitionValidationRecord = z.infer<typeof metaobjectFieldDefinitionValidationRecordSchema>;

export const metaobjectFieldDefinitionRecordSchema = z.strictObject({
  key: z.string(),
  name: nullableStringSchema,
  description: nullableStringSchema,
  required: nullableBooleanSchema,
  type: metaobjectDefinitionTypeRecordSchema,
  validations: z.array(metaobjectFieldDefinitionValidationRecordSchema),
});
export type MetaobjectFieldDefinitionRecord = z.infer<typeof metaobjectFieldDefinitionRecordSchema>;

export const metaobjectStandardTemplateRecordSchema = z.strictObject({
  type: nullableStringSchema,
  name: nullableStringSchema,
});
export type MetaobjectStandardTemplateRecord = z.infer<typeof metaobjectStandardTemplateRecordSchema>;

export const metaobjectDefinitionRecordSchema = z.strictObject({
  id: z.string(),
  type: z.string(),
  name: nullableStringSchema,
  description: nullableStringSchema,
  displayNameKey: nullableStringSchema,
  access: z.record(z.string(), jsonValueSchema),
  capabilities: metaobjectDefinitionCapabilitiesRecordSchema,
  fieldDefinitions: z.array(metaobjectFieldDefinitionRecordSchema),
  hasThumbnailField: nullableBooleanSchema,
  metaobjectsCount: nullableNumberSchema,
  standardTemplate: metaobjectStandardTemplateRecordSchema.nullable(),
  createdAt: nullableStringSchema.optional(),
  updatedAt: nullableStringSchema.optional(),
});
export type MetaobjectDefinitionRecord = z.infer<typeof metaobjectDefinitionRecordSchema>;

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
  id: nullableStringSchema.optional(),
  firstName: nullableStringSchema.optional(),
  lastName: nullableStringSchema.optional(),
  address2: nullableStringSchema.optional(),
  address1: nullableStringSchema,
  city: nullableStringSchema,
  company: nullableStringSchema.optional(),
  province: nullableStringSchema,
  provinceCode: nullableStringSchema.optional(),
  country: nullableStringSchema,
  countryCodeV2: nullableStringSchema.optional(),
  zip: nullableStringSchema,
  phone: nullableStringSchema.optional(),
  name: nullableStringSchema.optional(),
  formattedArea: nullableStringSchema,
});
export type CustomerDefaultAddressRecord = z.infer<typeof customerDefaultAddressRecordSchema>;

export const customerAddressRecordSchema = z.strictObject({
  id: z.string(),
  customerId: z.string(),
  cursor: nullableStringSchema.optional(),
  position: z.number(),
  firstName: nullableStringSchema,
  lastName: nullableStringSchema,
  address1: nullableStringSchema,
  address2: nullableStringSchema,
  city: nullableStringSchema,
  company: nullableStringSchema,
  province: nullableStringSchema,
  provinceCode: nullableStringSchema,
  country: nullableStringSchema,
  countryCodeV2: nullableStringSchema,
  zip: nullableStringSchema,
  phone: nullableStringSchema,
  name: nullableStringSchema,
  formattedArea: nullableStringSchema,
});
export type CustomerAddressRecord = z.infer<typeof customerAddressRecordSchema>;

export const customerPaymentMethodRecordSchema = z.strictObject({
  id: z.string(),
  customerId: nullableStringSchema,
});
export type CustomerPaymentMethodRecord = z.infer<typeof customerPaymentMethodRecordSchema>;

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

export const segmentRecordSchema = z.strictObject({
  id: z.string(),
  name: nullableStringSchema,
  query: nullableStringSchema,
  creationDate: nullableStringSchema,
  lastEditDate: nullableStringSchema,
});
export type SegmentRecord = z.infer<typeof segmentRecordSchema>;

export const webhookSubscriptionEndpointRecordSchema = z.strictObject({
  __typename: z.enum(['WebhookHttpEndpoint', 'WebhookEventBridgeEndpoint', 'WebhookPubSubEndpoint']),
  callbackUrl: nullableStringSchema.optional(),
  arn: nullableStringSchema.optional(),
  pubSubProject: nullableStringSchema.optional(),
  pubSubTopic: nullableStringSchema.optional(),
});
export type WebhookSubscriptionEndpointRecord = z.infer<typeof webhookSubscriptionEndpointRecordSchema>;

export const webhookSubscriptionRecordSchema = z.strictObject({
  id: z.string(),
  topic: nullableStringSchema,
  format: nullableStringSchema,
  includeFields: z.array(z.string()).default([]),
  metafieldNamespaces: z.array(z.string()).default([]),
  filter: nullableStringSchema,
  createdAt: nullableStringSchema,
  updatedAt: nullableStringSchema,
  endpoint: webhookSubscriptionEndpointRecordSchema.nullable(),
});
export type WebhookSubscriptionRecord = z.infer<typeof webhookSubscriptionRecordSchema>;

export const marketingRecordSchema = z.strictObject({
  id: z.string(),
  cursor: nullableStringSchema.optional(),
  data: z.record(z.string(), jsonValueSchema),
});
export type MarketingRecord = z.infer<typeof marketingRecordSchema>;

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

export const discountCombinesWithRecordSchema = z.strictObject({
  productDiscounts: z.boolean(),
  orderDiscounts: z.boolean(),
  shippingDiscounts: z.boolean(),
});
export type DiscountCombinesWithRecord = z.infer<typeof discountCombinesWithRecordSchema>;

export const discountMoneyRecordSchema = z.strictObject({
  amount: z.string(),
  currencyCode: z.string(),
});
export type DiscountMoneyRecord = z.infer<typeof discountMoneyRecordSchema>;

export const discountRedeemCodeRecordSchema = z.strictObject({
  id: z.string(),
  code: z.string(),
  asyncUsageCount: nullableNumberSchema.default(0),
});
export type DiscountRedeemCodeRecord = z.infer<typeof discountRedeemCodeRecordSchema>;

export const discountContextRecordSchema = z.strictObject({
  typeName: z.string(),
  all: nullableStringSchema.optional(),
  customerIds: z.array(z.string()).optional(),
  customerSegmentIds: z.array(z.string()).optional(),
});
export type DiscountContextRecord = z.infer<typeof discountContextRecordSchema>;

export const discountItemsRecordSchema = z.strictObject({
  typeName: z.string(),
  allItems: nullableBooleanSchema.optional(),
  productIds: z.array(z.string()).optional(),
  productVariantIds: z.array(z.string()).optional(),
  collectionIds: z.array(z.string()).optional(),
});
export type DiscountItemsRecord = z.infer<typeof discountItemsRecordSchema>;

export const discountValueRecordSchema = z.strictObject({
  typeName: z.string(),
  percentage: nullableNumberSchema.optional(),
  amount: discountMoneyRecordSchema.nullable().optional(),
  appliesOnEachItem: nullableBooleanSchema.optional(),
});
export type DiscountValueRecord = z.infer<typeof discountValueRecordSchema>;

export const discountCustomerGetsRecordSchema = z.strictObject({
  value: discountValueRecordSchema,
  items: discountItemsRecordSchema,
  appliesOnOneTimePurchase: z.boolean(),
  appliesOnSubscription: z.boolean(),
});
export type DiscountCustomerGetsRecord = z.infer<typeof discountCustomerGetsRecordSchema>;

export const discountMinimumRequirementRecordSchema = z.strictObject({
  typeName: z.string(),
  greaterThanOrEqualToQuantity: nullableStringSchema.optional(),
  greaterThanOrEqualToSubtotal: discountMoneyRecordSchema.nullable().optional(),
});
export type DiscountMinimumRequirementRecord = z.infer<typeof discountMinimumRequirementRecordSchema>;

export const discountDestinationSelectionRecordSchema = z.strictObject({
  typeName: z.string(),
  allCountries: nullableBooleanSchema.optional(),
  countries: z.array(z.string()).optional(),
  includeRestOfWorld: nullableBooleanSchema.optional(),
});
export type DiscountDestinationSelectionRecord = z.infer<typeof discountDestinationSelectionRecordSchema>;

export const discountMetafieldRecordSchema = z.strictObject({
  id: z.string(),
  namespace: z.string(),
  key: z.string(),
  type: z.string(),
  value: z.string(),
  compareDigest: nullableStringSchema.optional(),
  jsonValue: jsonValueSchema.optional(),
  createdAt: nullableStringSchema.optional(),
  updatedAt: nullableStringSchema.optional(),
  ownerType: nullableStringSchema.optional(),
});
export type DiscountMetafieldRecord = z.infer<typeof discountMetafieldRecordSchema>;

export const discountEventRecordSchema = z.strictObject({
  id: z.string(),
  typeName: z.string(),
  action: nullableStringSchema.optional(),
  message: nullableStringSchema.optional(),
  createdAt: nullableStringSchema.optional(),
  subjectId: nullableStringSchema.optional(),
  subjectType: nullableStringSchema.optional(),
});
export type DiscountEventRecord = z.infer<typeof discountEventRecordSchema>;

export const discountBulkOperationRecordSchema = z.strictObject({
  id: z.string(),
  typeName: z.string(),
  operation: z.enum(['discountRedeemCodeBulkAdd', 'discountCodeRedeemCodeBulkDelete']),
  discountId: z.string(),
  status: z.enum(['COMPLETED', 'FAILED', 'IN_PROGRESS']),
  done: z.boolean(),
  createdAt: z.string(),
  completedAt: nullableStringSchema.optional(),
  codesCount: z.number().int().nonnegative().optional(),
  importedCount: z.number().int().nonnegative().optional(),
  failedCount: z.number().int().nonnegative().optional(),
  redeemCodeIds: z.array(z.string()).optional(),
});
export type DiscountBulkOperationRecord = z.infer<typeof discountBulkOperationRecordSchema>;

export const discountRecordSchema = z.strictObject({
  id: z.string(),
  typeName: z.string(),
  method: z.enum(['code', 'automatic']),
  title: z.string(),
  status: nullableStringSchema,
  summary: nullableStringSchema,
  startsAt: nullableStringSchema,
  endsAt: nullableStringSchema,
  createdAt: nullableStringSchema,
  updatedAt: nullableStringSchema,
  asyncUsageCount: nullableNumberSchema,
  discountClasses: z.array(z.string()),
  combinesWith: discountCombinesWithRecordSchema,
  codes: z.array(z.string()).default([]),
  redeemCodes: z.array(discountRedeemCodeRecordSchema).optional(),
  context: discountContextRecordSchema.nullable().optional(),
  customerGets: discountCustomerGetsRecordSchema.nullable().optional(),
  minimumRequirement: discountMinimumRequirementRecordSchema.nullable().optional(),
  destinationSelection: discountDestinationSelectionRecordSchema.nullable().optional(),
  maximumShippingPrice: discountMoneyRecordSchema.nullable().optional(),
  appliesOncePerCustomer: nullableBooleanSchema.optional(),
  appliesOnOneTimePurchase: nullableBooleanSchema.optional(),
  appliesOnSubscription: nullableBooleanSchema.optional(),
  recurringCycleLimit: nullableNumberSchema.optional(),
  usageLimit: nullableNumberSchema.optional(),
  metafields: z.array(discountMetafieldRecordSchema).optional(),
  events: z.array(discountEventRecordSchema).optional(),
  discountType: nullableStringSchema.optional(),
  appId: nullableStringSchema.optional(),
  appDiscountType: jsonObjectSchema.optional(),
  discountId: nullableStringSchema.optional(),
  errorHistory: jsonValueSchema.optional(),
  unsupportedAppFieldNames: z.array(z.string()).optional(),
});
export type DiscountRecord = z.infer<typeof discountRecordSchema>;

export const paymentCustomizationMetafieldRecordSchema = z.strictObject({
  id: z.string(),
  paymentCustomizationId: z.string(),
  namespace: z.string(),
  key: z.string(),
  type: nullableStringSchema,
  value: nullableStringSchema,
  compareDigest: nullableStringSchema.optional(),
  jsonValue: jsonValueSchema.optional(),
  createdAt: nullableStringSchema.optional(),
  updatedAt: nullableStringSchema.optional(),
  ownerType: nullableStringSchema.optional(),
});
export type PaymentCustomizationMetafieldRecord = z.infer<typeof paymentCustomizationMetafieldRecordSchema>;

export const paymentCustomizationRecordSchema = z.strictObject({
  id: z.string(),
  title: nullableStringSchema,
  enabled: nullableBooleanSchema,
  functionId: nullableStringSchema,
  shopifyFunction: jsonObjectSchema.optional(),
  errorHistory: jsonValueSchema.optional(),
  metafields: z.array(paymentCustomizationMetafieldRecordSchema).optional(),
});
export type PaymentCustomizationRecord = z.infer<typeof paymentCustomizationRecordSchema>;

export const customerMergeRequestRecordSchema = z.strictObject({
  jobId: z.string(),
  resultingCustomerId: z.string(),
  status: z.enum(['IN_PROGRESS', 'COMPLETED', 'FAILED']),
  customerMergeErrors: z.array(
    z.strictObject({
      errorFields: z.array(z.string()),
      message: z.string(),
    }),
  ),
});
export type CustomerMergeRequestRecord = z.infer<typeof customerMergeRequestRecordSchema>;

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

export const orderFulfillmentEventRecordSchema = z.strictObject({
  id: z.string(),
  status: nullableStringSchema,
  message: nullableStringSchema.optional(),
  happenedAt: nullableStringSchema,
  createdAt: nullableStringSchema.optional(),
  estimatedDeliveryAt: nullableStringSchema.optional(),
  city: nullableStringSchema.optional(),
  province: nullableStringSchema.optional(),
  country: nullableStringSchema.optional(),
  zip: nullableStringSchema.optional(),
  address1: nullableStringSchema.optional(),
  latitude: z.number().nullable().optional(),
  longitude: z.number().nullable().optional(),
});
export type OrderFulfillmentEventRecord = z.infer<typeof orderFulfillmentEventRecordSchema>;

export const orderFulfillmentLocationRecordSchema = z.strictObject({
  id: nullableStringSchema.optional(),
  name: nullableStringSchema,
});
export type OrderFulfillmentLocationRecord = z.infer<typeof orderFulfillmentLocationRecordSchema>;

export const orderFulfillmentOriginAddressRecordSchema = z.strictObject({
  address1: nullableStringSchema.optional(),
  address2: nullableStringSchema.optional(),
  city: nullableStringSchema.optional(),
  countryCode: nullableStringSchema,
  provinceCode: nullableStringSchema.optional(),
  zip: nullableStringSchema.optional(),
});
export type OrderFulfillmentOriginAddressRecord = z.infer<typeof orderFulfillmentOriginAddressRecordSchema>;

export const orderFulfillmentServiceRecordSchema = z.strictObject({
  id: nullableStringSchema,
  handle: nullableStringSchema,
  serviceName: nullableStringSchema,
  trackingSupport: z.boolean().nullable().optional(),
  type: nullableStringSchema.optional(),
  location: orderFulfillmentLocationRecordSchema.nullable().optional(),
});
export type OrderFulfillmentServiceRecord = z.infer<typeof orderFulfillmentServiceRecordSchema>;

export const orderFulfillmentRecordSchema = z.strictObject({
  id: z.string(),
  status: nullableStringSchema,
  displayStatus: nullableStringSchema.optional(),
  createdAt: nullableStringSchema.optional(),
  updatedAt: nullableStringSchema.optional(),
  deliveredAt: nullableStringSchema.optional(),
  estimatedDeliveryAt: nullableStringSchema.optional(),
  inTransitAt: nullableStringSchema.optional(),
  trackingInfo: z.array(orderFulfillmentTrackingInfoRecordSchema).optional(),
  events: z.array(orderFulfillmentEventRecordSchema).optional(),
  fulfillmentLineItems: z.array(orderFulfillmentLineItemRecordSchema).optional(),
  service: orderFulfillmentServiceRecordSchema.nullable().optional(),
  location: orderFulfillmentLocationRecordSchema.nullable().optional(),
  originAddress: orderFulfillmentOriginAddressRecordSchema.nullable().optional(),
});
export type OrderFulfillmentRecord = z.infer<typeof orderFulfillmentRecordSchema>;

export const orderFulfillmentOrderAssignedLocationRecordSchema = z.strictObject({
  name: nullableStringSchema,
  locationId: nullableStringSchema.optional(),
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

export const orderFulfillmentOrderDeliveryMethodRecordSchema = z.strictObject({
  id: z.string(),
  methodType: z.string(),
  presentedName: nullableStringSchema.optional(),
  serviceCode: nullableStringSchema.optional(),
  minDeliveryDateTime: nullableStringSchema.optional(),
  maxDeliveryDateTime: nullableStringSchema.optional(),
  sourceReference: nullableStringSchema.optional(),
});
export type OrderFulfillmentOrderDeliveryMethodRecord = z.infer<typeof orderFulfillmentOrderDeliveryMethodRecordSchema>;

export const orderFulfillmentOrderRecordSchema = z.strictObject({
  id: z.string(),
  status: nullableStringSchema,
  requestStatus: nullableStringSchema.optional(),
  fulfillAt: nullableStringSchema.optional(),
  fulfillBy: nullableStringSchema.optional(),
  updatedAt: nullableStringSchema.optional(),
  supportedActions: z.array(z.string()).optional(),
  fulfillmentHolds: z
    .array(
      z.strictObject({
        id: z.string(),
        handle: nullableStringSchema.optional(),
        reason: nullableStringSchema.optional(),
        reasonNotes: nullableStringSchema.optional(),
        displayReason: nullableStringSchema.optional(),
        heldByRequestingApp: z.boolean().optional(),
      }),
    )
    .optional(),
  assignedLocation: orderFulfillmentOrderAssignedLocationRecordSchema.nullable().optional(),
  deliveryMethod: orderFulfillmentOrderDeliveryMethodRecordSchema.nullable().optional(),
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
  parentTransactionId: nullableStringSchema.optional(),
  paymentId: nullableStringSchema.optional(),
  paymentReferenceId: nullableStringSchema.optional(),
  processedAt: nullableStringSchema.optional(),
});
export type OrderTransactionRecord = z.infer<typeof orderTransactionRecordSchema>;

export const orderMandatePaymentRecordSchema = z.strictObject({
  idempotencyKey: z.string(),
  orderId: z.string(),
  jobId: z.string(),
  paymentReferenceId: z.string(),
  transactionId: z.string(),
  createdAt: z.string(),
});
export type OrderMandatePaymentRecord = z.infer<typeof orderMandatePaymentRecordSchema>;

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
  totalRefundedShippingSet: moneySetSchema.nullable().optional(),
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
  totalCapturableSet: moneySetSchema.nullable().optional(),
  capturable: z.boolean().optional(),
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

export const marketRecordSchema = z.strictObject({
  id: z.string(),
  cursor: nullableStringSchema.optional(),
  data: z.record(z.string(), jsonValueSchema),
});
export type MarketRecord = z.infer<typeof marketRecordSchema>;

export const webPresenceRecordSchema = z.strictObject({
  id: z.string(),
  cursor: nullableStringSchema.optional(),
  data: z.record(z.string(), jsonValueSchema),
});
export type WebPresenceRecord = z.infer<typeof webPresenceRecordSchema>;

export const marketLocalizationRecordSchema = z.strictObject({
  resourceId: z.string(),
  marketId: z.string(),
  key: z.string(),
  value: z.string(),
  updatedAt: z.string(),
  outdated: z.boolean(),
});
export type MarketLocalizationRecord = z.infer<typeof marketLocalizationRecordSchema>;

export const catalogRecordSchema = z.strictObject({
  id: z.string(),
  cursor: nullableStringSchema.optional(),
  data: z.record(z.string(), jsonValueSchema),
});
export type CatalogRecord = z.infer<typeof catalogRecordSchema>;

export const priceListRecordSchema = z.strictObject({
  id: z.string(),
  cursor: nullableStringSchema.optional(),
  data: z.record(z.string(), jsonValueSchema),
});
export type PriceListRecord = z.infer<typeof priceListRecordSchema>;

export const deliveryProfileCountRecordSchema = z.strictObject({
  count: z.number(),
  precision: nullableStringSchema.optional(),
});
export type DeliveryProfileCountRecord = z.infer<typeof deliveryProfileCountRecordSchema>;

export const deliveryProfileItemRecordSchema = z.strictObject({
  productId: z.string(),
  variantIds: z.array(z.string()),
  cursor: nullableStringSchema.optional(),
  variantCursors: z.record(z.string(), z.string()).optional(),
});
export type DeliveryProfileItemRecord = z.infer<typeof deliveryProfileItemRecordSchema>;

export const deliveryProfileProvinceRecordSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  code: z.string(),
});
export type DeliveryProfileProvinceRecord = z.infer<typeof deliveryProfileProvinceRecordSchema>;

export const deliveryProfileCountryCodeRecordSchema = z.strictObject({
  countryCode: nullableStringSchema,
  restOfWorld: z.boolean(),
});
export type DeliveryProfileCountryCodeRecord = z.infer<typeof deliveryProfileCountryCodeRecordSchema>;

export const deliveryProfileCountryRecordSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  translatedName: nullableStringSchema.optional(),
  code: deliveryProfileCountryCodeRecordSchema,
  provinces: z.array(deliveryProfileProvinceRecordSchema),
});
export type DeliveryProfileCountryRecord = z.infer<typeof deliveryProfileCountryRecordSchema>;

export const deliveryProfileCountryAndZoneRecordSchema = z.strictObject({
  zone: z.string(),
  country: deliveryProfileCountryRecordSchema,
});
export type DeliveryProfileCountryAndZoneRecord = z.infer<typeof deliveryProfileCountryAndZoneRecordSchema>;

export const deliveryProfileZoneRecordSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  countries: z.array(deliveryProfileCountryRecordSchema),
});
export type DeliveryProfileZoneRecord = z.infer<typeof deliveryProfileZoneRecordSchema>;

export const deliveryProfileMethodConditionRecordSchema = z.strictObject({
  id: z.string(),
  field: z.string(),
  operator: z.string(),
  conditionCriteria: z.record(z.string(), jsonValueSchema),
});
export type DeliveryProfileMethodConditionRecord = z.infer<typeof deliveryProfileMethodConditionRecordSchema>;

export const deliveryProfileMethodDefinitionRecordSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  active: z.boolean(),
  description: nullableStringSchema,
  rateProvider: z.record(z.string(), jsonValueSchema),
  methodConditions: z.array(deliveryProfileMethodConditionRecordSchema),
  cursor: nullableStringSchema.optional(),
});
export type DeliveryProfileMethodDefinitionRecord = z.infer<typeof deliveryProfileMethodDefinitionRecordSchema>;

export const deliveryProfileLocationGroupZoneRecordSchema = z.strictObject({
  zone: deliveryProfileZoneRecordSchema,
  methodDefinitions: z.array(deliveryProfileMethodDefinitionRecordSchema),
  cursor: nullableStringSchema.optional(),
});
export type DeliveryProfileLocationGroupZoneRecord = z.infer<typeof deliveryProfileLocationGroupZoneRecordSchema>;

export const deliveryProfileLocationGroupRecordSchema = z.strictObject({
  id: z.string(),
  locationIds: z.array(z.string()),
  locationCursors: z.record(z.string(), z.string()).optional(),
  countriesInAnyZone: z.array(deliveryProfileCountryAndZoneRecordSchema).default([]),
  locationGroupZones: z.array(deliveryProfileLocationGroupZoneRecordSchema),
});
export type DeliveryProfileLocationGroupRecord = z.infer<typeof deliveryProfileLocationGroupRecordSchema>;

export const deliveryProfileRecordSchema = z.strictObject({
  id: z.string(),
  cursor: nullableStringSchema.optional(),
  name: z.string(),
  default: z.boolean(),
  merchantOwned: z.boolean(),
  version: z.number(),
  activeMethodDefinitionsCount: z.number(),
  locationsWithoutRatesCount: z.number(),
  originLocationCount: z.number(),
  zoneCountryCount: z.number(),
  productVariantsCount: deliveryProfileCountRecordSchema.nullable(),
  profileItems: z.array(deliveryProfileItemRecordSchema),
  profileLocationGroups: z.array(deliveryProfileLocationGroupRecordSchema),
  unassignedLocationIds: z.array(z.string()).default([]),
  unassignedLocationCursors: z.record(z.string(), z.string()).optional(),
  sellingPlanGroups: z.array(z.record(z.string(), jsonValueSchema)).default([]),
});
export type DeliveryProfileRecord = z.infer<typeof deliveryProfileRecordSchema>;

export const calculatedOrderRecordSchema = orderRecordSchema.extend({
  originalOrderId: z.string(),
});
export type CalculatedOrderRecord = z.infer<typeof calculatedOrderRecordSchema>;

export const stateSnapshotSchema = z.strictObject({
  shop: shopRecordSchema.nullable().default(null),
  products: z.record(z.string(), productRecordSchema),
  productVariants: z.record(z.string(), productVariantRecordSchema),
  productOptions: z.record(z.string(), productOptionRecordSchema),
  locations: z.record(z.string(), locationRecordSchema).default({}),
  locationOrder: z.array(z.string()).default([]),
  fulfillmentServices: z.record(z.string(), fulfillmentServiceRecordSchema).default({}),
  fulfillmentServiceOrder: z.array(z.string()).default([]),
  carrierServices: z.record(z.string(), carrierServiceRecordSchema).default({}),
  carrierServiceOrder: z.array(z.string()).default([]),
  collections: z.record(z.string(), collectionRecordSchema),
  publications: z.record(z.string(), publicationRecordSchema).default({}),
  customers: z.record(z.string(), customerRecordSchema),
  customerAddresses: z.record(z.string(), customerAddressRecordSchema).default({}),
  customerPaymentMethods: z.record(z.string(), customerPaymentMethodRecordSchema).default({}),
  segments: z.record(z.string(), segmentRecordSchema).default({}),
  webhookSubscriptions: z.record(z.string(), webhookSubscriptionRecordSchema).default({}),
  webhookSubscriptionOrder: z.array(z.string()).default([]),
  marketingActivities: z.record(z.string(), marketingRecordSchema).default({}),
  marketingActivityOrder: z.array(z.string()).default([]),
  marketingEvents: z.record(z.string(), marketingRecordSchema).default({}),
  marketingEventOrder: z.array(z.string()).default([]),
  discounts: z.record(z.string(), discountRecordSchema).default({}),
  discountBulkOperations: z.record(z.string(), discountBulkOperationRecordSchema).default({}),
  paymentCustomizations: z.record(z.string(), paymentCustomizationRecordSchema).default({}),
  paymentCustomizationOrder: z.array(z.string()).default([]),
  businessEntities: z.record(z.string(), businessEntityRecordSchema).default({}),
  businessEntityOrder: z.array(z.string()).default([]),
  markets: z.record(z.string(), marketRecordSchema).default({}),
  marketOrder: z.array(z.string()).default([]),
  webPresences: z.record(z.string(), webPresenceRecordSchema).default({}),
  webPresenceOrder: z.array(z.string()).default([]),
  marketLocalizations: z.record(z.string(), marketLocalizationRecordSchema).default({}),
  catalogs: z.record(z.string(), catalogRecordSchema).default({}),
  catalogOrder: z.array(z.string()).default([]),
  priceLists: z.record(z.string(), priceListRecordSchema).default({}),
  priceListOrder: z.array(z.string()).default([]),
  deliveryProfiles: z.record(z.string(), deliveryProfileRecordSchema).default({}),
  deliveryProfileOrder: z.array(z.string()).default([]),
  productCollections: z.record(z.string(), productCollectionRecordSchema),
  productMedia: z.record(z.string(), productMediaRecordSchema),
  files: z.record(z.string(), fileRecordSchema).default({}),
  productMetafields: z.record(z.string(), productMetafieldRecordSchema),
  metafieldDefinitions: z.record(z.string(), metafieldDefinitionRecordSchema).default({}),
  metaobjectDefinitions: z.record(z.string(), metaobjectDefinitionRecordSchema).default({}),
  customerMetafields: z.record(z.string(), customerMetafieldRecordSchema).default({}),
  deletedProductIds: z.record(z.string(), z.literal(true)),
  deletedFileIds: z.record(z.string(), z.literal(true)).default({}),
  deletedCollectionIds: z.record(z.string(), z.literal(true)),
  deletedLocationIds: z.record(z.string(), z.literal(true)).default({}),
  deletedFulfillmentServiceIds: z.record(z.string(), z.literal(true)).default({}),
  deletedCarrierServiceIds: z.record(z.string(), z.literal(true)).default({}),
  deletedCustomerIds: z.record(z.string(), z.literal(true)),
  deletedCustomerAddressIds: z.record(z.string(), z.literal(true)).default({}),
  deletedSegmentIds: z.record(z.string(), z.literal(true)).default({}),
  deletedWebhookSubscriptionIds: z.record(z.string(), z.literal(true)).default({}),
  deletedDiscountIds: z.record(z.string(), z.literal(true)).default({}),
  deletedPaymentCustomizationIds: z.record(z.string(), z.literal(true)).default({}),
  deletedMarketIds: z.record(z.string(), z.literal(true)).default({}),
  deletedCatalogIds: z.record(z.string(), z.literal(true)).default({}),
  deletedWebPresenceIds: z.record(z.string(), z.literal(true)).default({}),
  deletedDeliveryProfileIds: z.record(z.string(), z.literal(true)).default({}),
  mergedCustomerIds: z.record(z.string(), z.string()).default({}),
  customerMergeRequests: z.record(z.string(), customerMergeRequestRecordSchema).default({}),
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
  registeredOperation?: {
    name: string;
    domain: string;
    execution: string;
    implemented: boolean;
    supportNotes?: string;
  };
  safety?: {
    classification: string;
    wouldProxyToShopify: boolean;
    reason: string;
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
