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

export const inventoryTransferLocationSnapshotRecordSchema = z.strictObject({
  id: nullableStringSchema,
  name: z.string(),
  snapshottedAt: z.string(),
});
export type InventoryTransferLocationSnapshotRecord = z.infer<typeof inventoryTransferLocationSnapshotRecordSchema>;

export const inventoryTransferLineItemRecordSchema = z.strictObject({
  id: z.string(),
  inventoryItemId: z.string(),
  title: nullableStringSchema,
  totalQuantity: z.number().int(),
  shippedQuantity: z.number().int().default(0),
  pickedForShipmentQuantity: z.number().int().default(0),
});
export type InventoryTransferLineItemRecord = z.infer<typeof inventoryTransferLineItemRecordSchema>;

export const inventoryTransferRecordSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  referenceName: nullableStringSchema,
  status: z.enum(['DRAFT', 'READY_TO_SHIP', 'IN_PROGRESS', 'TRANSFERRED', 'CANCELED', 'OTHER']),
  note: nullableStringSchema,
  tags: z.array(z.string()),
  dateCreated: z.string(),
  origin: inventoryTransferLocationSnapshotRecordSchema.nullable(),
  destination: inventoryTransferLocationSnapshotRecordSchema.nullable(),
  lineItems: z.array(inventoryTransferLineItemRecordSchema),
});
export type InventoryTransferRecord = z.infer<typeof inventoryTransferRecordSchema>;

export const inventoryShipmentTrackingRecordSchema = z.strictObject({
  trackingNumber: nullableStringSchema,
  company: nullableStringSchema,
  trackingUrl: nullableStringSchema,
  arrivesAt: nullableStringSchema,
});
export type InventoryShipmentTrackingRecord = z.infer<typeof inventoryShipmentTrackingRecordSchema>;

export const inventoryShipmentLineItemRecordSchema = z.strictObject({
  id: z.string(),
  inventoryItemId: z.string(),
  quantity: z.number(),
  acceptedQuantity: z.number().default(0),
  rejectedQuantity: z.number().default(0),
});
export type InventoryShipmentLineItemRecord = z.infer<typeof inventoryShipmentLineItemRecordSchema>;

export const inventoryShipmentRecordSchema = z.strictObject({
  id: z.string(),
  movementId: z.string(),
  name: z.string(),
  status: z.enum(['DRAFT', 'IN_TRANSIT', 'PARTIALLY_RECEIVED', 'RECEIVED', 'OTHER']),
  createdAt: z.string(),
  updatedAt: z.string(),
  tracking: inventoryShipmentTrackingRecordSchema.nullable(),
  lineItems: z.array(inventoryShipmentLineItemRecordSchema),
});
export type InventoryShipmentRecord = z.infer<typeof inventoryShipmentRecordSchema>;

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

export const deliveryLocalPickupSettingsRecordSchema = z.strictObject({
  pickupTime: z.string(),
  instructions: z.string(),
});
export type DeliveryLocalPickupSettingsRecord = z.infer<typeof deliveryLocalPickupSettingsRecordSchema>;

export const shippingPackageWeightRecordSchema = z.strictObject({
  value: nullableNumberSchema,
  unit: nullableStringSchema,
});
export type ShippingPackageWeightRecord = z.infer<typeof shippingPackageWeightRecordSchema>;

export const shippingPackageDimensionsRecordSchema = z.strictObject({
  length: nullableNumberSchema,
  width: nullableNumberSchema,
  height: nullableNumberSchema,
  unit: nullableStringSchema,
});
export type ShippingPackageDimensionsRecord = z.infer<typeof shippingPackageDimensionsRecordSchema>;

export const shippingPackageRecordSchema = z.strictObject({
  id: z.string(),
  name: nullableStringSchema,
  type: nullableStringSchema,
  default: z.boolean(),
  weight: shippingPackageWeightRecordSchema.nullable(),
  dimensions: shippingPackageDimensionsRecordSchema.nullable(),
  createdAt: z.string(),
  updatedAt: z.string(),
});
export type ShippingPackageRecord = z.infer<typeof shippingPackageRecordSchema>;

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
  localPickupSettings: deliveryLocalPickupSettingsRecordSchema.nullable().optional(),
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
  mediaIds: z.array(z.string()).optional(),
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

export const productOperationUserErrorRecordSchema = z.strictObject({
  field: z.array(z.string()).nullable(),
  message: z.string(),
});
export type ProductOperationUserErrorRecord = z.infer<typeof productOperationUserErrorRecordSchema>;

export const productOperationRecordSchema = z.strictObject({
  id: z.string(),
  typeName: z.enum(['ProductSetOperation']),
  productId: nullableStringSchema,
  status: z.string(),
  userErrors: z.array(productOperationUserErrorRecordSchema).default([]),
});
export type ProductOperationRecord = z.infer<typeof productOperationRecordSchema>;

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
  autoPublish: nullableBooleanSchema.optional(),
  supportsFuturePublishing: nullableBooleanSchema.optional(),
  catalogId: nullableStringSchema.optional(),
  channelId: nullableStringSchema.optional(),
  cursor: nullableStringSchema.optional(),
});
export type PublicationRecord = z.infer<typeof publicationRecordSchema>;

export const channelRecordSchema = z.strictObject({
  id: z.string(),
  name: nullableStringSchema,
  handle: nullableStringSchema.optional(),
  publicationId: nullableStringSchema.optional(),
  cursor: nullableStringSchema.optional(),
});
export type ChannelRecord = z.infer<typeof channelRecordSchema>;

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

export const metaobjectFieldDefinitionReferenceRecordSchema = z.strictObject({
  key: z.string(),
  name: nullableStringSchema,
  required: nullableBooleanSchema,
  type: metaobjectDefinitionTypeRecordSchema,
});
export type MetaobjectFieldDefinitionReferenceRecord = z.infer<typeof metaobjectFieldDefinitionReferenceRecordSchema>;

export const metaobjectFieldRecordSchema = z.strictObject({
  key: z.string(),
  type: nullableStringSchema,
  value: nullableStringSchema,
  jsonValue: jsonValueSchema.nullable(),
  definition: metaobjectFieldDefinitionReferenceRecordSchema.nullable(),
});
export type MetaobjectFieldRecord = z.infer<typeof metaobjectFieldRecordSchema>;

export const metaobjectPublishableCapabilityRecordSchema = z.strictObject({
  status: nullableStringSchema,
});
export type MetaobjectPublishableCapabilityRecord = z.infer<typeof metaobjectPublishableCapabilityRecordSchema>;

export const metaobjectOnlineStoreCapabilityRecordSchema = z.strictObject({
  templateSuffix: nullableStringSchema,
});
export type MetaobjectOnlineStoreCapabilityRecord = z.infer<typeof metaobjectOnlineStoreCapabilityRecordSchema>;

export const metaobjectCapabilitiesRecordSchema = z.strictObject({
  publishable: metaobjectPublishableCapabilityRecordSchema.optional(),
  onlineStore: metaobjectOnlineStoreCapabilityRecordSchema.nullable().optional(),
});
export type MetaobjectCapabilitiesRecord = z.infer<typeof metaobjectCapabilitiesRecordSchema>;

export const metaobjectRecordSchema = z.strictObject({
  id: z.string(),
  handle: z.string(),
  type: z.string(),
  displayName: nullableStringSchema,
  fields: z.array(metaobjectFieldRecordSchema),
  capabilities: metaobjectCapabilitiesRecordSchema,
  createdAt: nullableStringSchema.optional(),
  updatedAt: nullableStringSchema.optional(),
});
export type MetaobjectRecord = z.infer<typeof metaobjectRecordSchema>;

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

export const giftCardTransactionRecordSchema = z.strictObject({
  id: z.string(),
  kind: z.enum(['CREDIT', 'DEBIT']),
  amount: moneyV2RecordSchema,
  processedAt: z.string(),
  note: nullableStringSchema,
});
export type GiftCardTransactionRecord = z.infer<typeof giftCardTransactionRecordSchema>;

export const giftCardRecordSchema = z.strictObject({
  id: z.string(),
  legacyResourceId: nullableStringSchema,
  lastCharacters: z.string(),
  maskedCode: z.string(),
  enabled: z.boolean(),
  deactivatedAt: nullableStringSchema,
  expiresOn: nullableStringSchema,
  note: nullableStringSchema,
  templateSuffix: nullableStringSchema,
  createdAt: z.string(),
  updatedAt: z.string(),
  initialValue: moneyV2RecordSchema,
  balance: moneyV2RecordSchema,
  customerId: nullableStringSchema,
  recipientId: nullableStringSchema,
  transactions: z.array(giftCardTransactionRecordSchema),
});
export type GiftCardRecord = z.infer<typeof giftCardRecordSchema>;

export const giftCardConfigurationRecordSchema = z.strictObject({
  issueLimit: moneyV2RecordSchema,
  purchaseLimit: moneyV2RecordSchema,
});
export type GiftCardConfigurationRecord = z.infer<typeof giftCardConfigurationRecordSchema>;

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
  dataSaleOptOut: z.boolean().optional(),
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

export const customerPaymentMethodInstrumentRecordSchema = z.strictObject({
  typeName: z.string(),
  data: jsonObjectSchema.default({}),
});
export type CustomerPaymentMethodInstrumentRecord = z.infer<typeof customerPaymentMethodInstrumentRecordSchema>;

export const customerPaymentMethodSubscriptionContractRecordSchema = z.strictObject({
  id: z.string(),
  cursor: nullableStringSchema.optional(),
  data: jsonObjectSchema.default({}),
});
export type CustomerPaymentMethodSubscriptionContractRecord = z.infer<
  typeof customerPaymentMethodSubscriptionContractRecordSchema
>;

export const customerPaymentMethodRecordSchema = z.strictObject({
  id: z.string(),
  customerId: z.string(),
  cursor: nullableStringSchema.optional(),
  instrument: customerPaymentMethodInstrumentRecordSchema.nullable(),
  revokedAt: nullableStringSchema,
  revokedReason: nullableStringSchema.optional(),
  subscriptionContracts: z.array(customerPaymentMethodSubscriptionContractRecordSchema).default([]),
});
export type CustomerPaymentMethodRecord = z.infer<typeof customerPaymentMethodRecordSchema>;

export const storeCreditAccountTransactionRecordSchema = z.strictObject({
  id: z.string(),
  accountId: z.string(),
  amount: moneyV2RecordSchema,
  balanceAfterTransaction: moneyV2RecordSchema,
  createdAt: z.string(),
  event: z.string(),
  origin: jsonObjectSchema.nullable().default(null),
});
export type StoreCreditAccountTransactionRecord = z.infer<typeof storeCreditAccountTransactionRecordSchema>;

export const storeCreditAccountRecordSchema = z.strictObject({
  id: z.string(),
  customerId: z.string(),
  cursor: nullableStringSchema.optional(),
  balance: moneyV2RecordSchema,
});
export type StoreCreditAccountRecord = z.infer<typeof storeCreditAccountRecordSchema>;

export const segmentRecordSchema = z.strictObject({
  id: z.string(),
  name: nullableStringSchema,
  query: nullableStringSchema,
  creationDate: nullableStringSchema,
  lastEditDate: nullableStringSchema,
});
export type SegmentRecord = z.infer<typeof segmentRecordSchema>;

export const customerSegmentMembersQueryRecordSchema = z.strictObject({
  id: z.string(),
  query: nullableStringSchema,
  segmentId: nullableStringSchema,
  currentCount: z.number(),
  done: z.boolean(),
});
export type CustomerSegmentMembersQueryRecord = z.infer<typeof customerSegmentMembersQueryRecordSchema>;

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
  uri: nullableStringSchema.optional(),
  name: nullableStringSchema.optional(),
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

export const marketingEngagementRecordSchema = z.strictObject({
  id: z.string(),
  marketingActivityId: nullableStringSchema.optional(),
  remoteId: nullableStringSchema.optional(),
  channelHandle: nullableStringSchema.optional(),
  occurredOn: z.string(),
  data: z.record(z.string(), jsonValueSchema),
});
export type MarketingEngagementRecord = z.infer<typeof marketingEngagementRecordSchema>;

export const onlineStoreContentKindSchema = z.enum(['article', 'blog', 'page', 'comment']);
export type OnlineStoreContentKind = z.infer<typeof onlineStoreContentKindSchema>;

export const onlineStoreContentRecordSchema = z.strictObject({
  id: z.string(),
  kind: onlineStoreContentKindSchema,
  cursor: nullableStringSchema.optional(),
  parentId: nullableStringSchema.optional(),
  createdAt: nullableStringSchema.optional(),
  updatedAt: nullableStringSchema.optional(),
  data: z.record(z.string(), jsonValueSchema),
});
export type OnlineStoreContentRecord = z.infer<typeof onlineStoreContentRecordSchema>;

export const onlineStoreIntegrationKindSchema = z.enum([
  'theme',
  'scriptTag',
  'webPixel',
  'serverPixel',
  'storefrontAccessToken',
  'mobilePlatformApplication',
]);
export type OnlineStoreIntegrationKind = z.infer<typeof onlineStoreIntegrationKindSchema>;

export const onlineStoreIntegrationRecordSchema = z.strictObject({
  id: z.string(),
  kind: onlineStoreIntegrationKindSchema,
  cursor: nullableStringSchema.optional(),
  createdAt: nullableStringSchema.optional(),
  updatedAt: nullableStringSchema.optional(),
  data: z.record(z.string(), jsonValueSchema),
});
export type OnlineStoreIntegrationRecord = z.infer<typeof onlineStoreIntegrationRecordSchema>;

export const savedSearchFilterRecordSchema = z.strictObject({
  key: z.string(),
  value: z.string(),
});
export type SavedSearchFilterRecord = z.infer<typeof savedSearchFilterRecordSchema>;

export const savedSearchRecordSchema = z.strictObject({
  id: z.string(),
  cursor: nullableStringSchema.optional(),
  legacyResourceId: z.string(),
  name: z.string(),
  query: z.string(),
  resourceType: z.string(),
  searchTerms: z.string(),
  filters: z.array(savedSearchFilterRecordSchema),
});
export type SavedSearchRecord = z.infer<typeof savedSearchRecordSchema>;

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

export const b2bCompanyContactRoleRecordSchema = z.strictObject({
  id: z.string(),
  companyId: z.string(),
  cursor: nullableStringSchema.optional(),
  data: z.record(z.string(), jsonValueSchema),
});
export type B2BCompanyContactRoleRecord = z.infer<typeof b2bCompanyContactRoleRecordSchema>;

export const b2bCompanyContactRecordSchema = z.strictObject({
  id: z.string(),
  companyId: z.string(),
  cursor: nullableStringSchema.optional(),
  data: z.record(z.string(), jsonValueSchema),
});
export type B2BCompanyContactRecord = z.infer<typeof b2bCompanyContactRecordSchema>;

export const b2bCompanyLocationRecordSchema = z.strictObject({
  id: z.string(),
  companyId: z.string(),
  cursor: nullableStringSchema.optional(),
  data: z.record(z.string(), jsonValueSchema),
});
export type B2BCompanyLocationRecord = z.infer<typeof b2bCompanyLocationRecordSchema>;

export const b2bCompanyRecordSchema = z.strictObject({
  id: z.string(),
  cursor: nullableStringSchema.optional(),
  data: z.record(z.string(), jsonValueSchema),
  contactIds: z.array(z.string()).default([]),
  locationIds: z.array(z.string()).default([]),
  contactRoleIds: z.array(z.string()).default([]),
});
export type B2BCompanyRecord = z.infer<typeof b2bCompanyRecordSchema>;

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

export const customerAccountPageRecordSchema = z.strictObject({
  id: z.string(),
  title: z.string(),
  handle: z.string(),
  defaultCursor: z.string(),
  cursor: nullableStringSchema.optional(),
});
export type CustomerAccountPageRecord = z.infer<typeof customerAccountPageRecordSchema>;

export const customerDataErasureRequestRecordSchema = z.strictObject({
  customerId: z.string(),
  requestedAt: z.string(),
  canceledAt: nullableStringSchema.optional(),
});
export type CustomerDataErasureRequestRecord = z.infer<typeof customerDataErasureRequestRecordSchema>;

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

export const discountEffectRecordSchema = z.strictObject({
  typeName: z.string(),
  percentage: nullableNumberSchema.optional(),
  amount: discountMoneyRecordSchema.nullable().optional(),
  appliesOnEachItem: nullableBooleanSchema.optional(),
});
export type DiscountEffectRecord = z.infer<typeof discountEffectRecordSchema>;

export const discountValueRecordSchema = z.strictObject({
  typeName: z.string(),
  percentage: nullableNumberSchema.optional(),
  amount: discountMoneyRecordSchema.nullable().optional(),
  appliesOnEachItem: nullableBooleanSchema.optional(),
  quantity: nullableStringSchema.optional(),
  effect: discountEffectRecordSchema.nullable().optional(),
});
export type DiscountValueRecord = z.infer<typeof discountValueRecordSchema>;

export const discountCustomerBuysRecordSchema = z.strictObject({
  value: discountValueRecordSchema,
  items: discountItemsRecordSchema,
});
export type DiscountCustomerBuysRecord = z.infer<typeof discountCustomerBuysRecordSchema>;

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

export const bulkOperationRecordSchema = z.strictObject({
  id: z.string(),
  status: z.enum(['CANCELED', 'CANCELING', 'COMPLETED', 'CREATED', 'EXPIRED', 'FAILED', 'RUNNING']),
  type: z.enum(['MUTATION', 'QUERY']),
  errorCode: z.enum(['ACCESS_DENIED', 'INTERNAL_SERVER_ERROR', 'TIMEOUT']).nullable(),
  createdAt: z.string(),
  completedAt: nullableStringSchema,
  objectCount: z.string(),
  rootObjectCount: z.string(),
  fileSize: nullableStringSchema,
  url: nullableStringSchema,
  partialDataUrl: nullableStringSchema,
  query: nullableStringSchema,
  cursor: nullableStringSchema.optional(),
  resultJsonl: z.string().optional(),
});
export type BulkOperationRecord = z.infer<typeof bulkOperationRecordSchema>;

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
  usageLimit: nullableNumberSchema.optional(),
  usesPerOrderLimit: nullableNumberSchema.optional(),
  discountClasses: z.array(z.string()),
  combinesWith: discountCombinesWithRecordSchema,
  codes: z.array(z.string()).default([]),
  redeemCodes: z.array(discountRedeemCodeRecordSchema).optional(),
  context: discountContextRecordSchema.nullable().optional(),
  customerBuys: discountCustomerBuysRecordSchema.nullable().optional(),
  customerGets: discountCustomerGetsRecordSchema.nullable().optional(),
  minimumRequirement: discountMinimumRequirementRecordSchema.nullable().optional(),
  destinationSelection: discountDestinationSelectionRecordSchema.nullable().optional(),
  maximumShippingPrice: discountMoneyRecordSchema.nullable().optional(),
  appliesOncePerCustomer: nullableBooleanSchema.optional(),
  appliesOnOneTimePurchase: nullableBooleanSchema.optional(),
  appliesOnSubscription: nullableBooleanSchema.optional(),
  recurringCycleLimit: nullableNumberSchema.optional(),
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
  functionHandle: nullableStringSchema.optional(),
  shopifyFunction: jsonObjectSchema.optional(),
  errorHistory: jsonValueSchema.optional(),
  metafields: z.array(paymentCustomizationMetafieldRecordSchema).optional(),
});
export type PaymentCustomizationRecord = z.infer<typeof paymentCustomizationRecordSchema>;

export const paymentTermsTemplateRecordSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  description: z.string(),
  dueInDays: nullableNumberSchema,
  paymentTermsType: z.string(),
  translatedName: z.string(),
});
export type PaymentTermsTemplateRecord = z.infer<typeof paymentTermsTemplateRecordSchema>;

export const defaultPaymentTermsTemplates: PaymentTermsTemplateRecord[] = [
  {
    id: 'gid://shopify/PaymentTermsTemplate/1',
    name: 'Due on receipt',
    description: 'Due on receipt',
    dueInDays: null,
    paymentTermsType: 'RECEIPT',
    translatedName: 'Due on receipt',
  },
  {
    id: 'gid://shopify/PaymentTermsTemplate/9',
    name: 'Due on fulfillment',
    description: 'Due on fulfillment',
    dueInDays: null,
    paymentTermsType: 'FULFILLMENT',
    translatedName: 'Due on fulfillment',
  },
  {
    id: 'gid://shopify/PaymentTermsTemplate/2',
    name: 'Net 7',
    description: 'Within 7 days',
    dueInDays: 7,
    paymentTermsType: 'NET',
    translatedName: 'Net 7',
  },
  {
    id: 'gid://shopify/PaymentTermsTemplate/3',
    name: 'Net 15',
    description: 'Within 15 days',
    dueInDays: 15,
    paymentTermsType: 'NET',
    translatedName: 'Net 15',
  },
  {
    id: 'gid://shopify/PaymentTermsTemplate/4',
    name: 'Net 30',
    description: 'Within 30 days',
    dueInDays: 30,
    paymentTermsType: 'NET',
    translatedName: 'Net 30',
  },
  {
    id: 'gid://shopify/PaymentTermsTemplate/8',
    name: 'Net 45',
    description: 'Within 45 days',
    dueInDays: 45,
    paymentTermsType: 'NET',
    translatedName: 'Net 45',
  },
  {
    id: 'gid://shopify/PaymentTermsTemplate/5',
    name: 'Net 60',
    description: 'Within 60 days',
    dueInDays: 60,
    paymentTermsType: 'NET',
    translatedName: 'Net 60',
  },
  {
    id: 'gid://shopify/PaymentTermsTemplate/6',
    name: 'Net 90',
    description: 'Within 90 days',
    dueInDays: 90,
    paymentTermsType: 'NET',
    translatedName: 'Net 90',
  },
  {
    id: 'gid://shopify/PaymentTermsTemplate/7',
    name: 'Fixed',
    description: 'Fixed date',
    dueInDays: null,
    paymentTermsType: 'FIXED',
    translatedName: 'Fixed',
  },
];

export const defaultPaymentTermsTemplateRecordMap: Record<string, PaymentTermsTemplateRecord> = Object.fromEntries(
  defaultPaymentTermsTemplates.map((template) => [template.id, template]),
);

export const defaultPaymentTermsTemplateOrder = defaultPaymentTermsTemplates.map((template) => template.id);

export const shopifyFunctionRecordSchema = z.strictObject({
  id: z.string(),
  title: nullableStringSchema,
  handle: nullableStringSchema,
  apiType: nullableStringSchema,
  description: nullableStringSchema.optional(),
  appKey: nullableStringSchema.optional(),
  app: jsonObjectSchema.optional(),
});
export type ShopifyFunctionRecord = z.infer<typeof shopifyFunctionRecordSchema>;

export const validationRecordSchema = z.strictObject({
  id: z.string(),
  title: nullableStringSchema,
  enable: nullableBooleanSchema,
  blockOnFailure: nullableBooleanSchema,
  functionId: nullableStringSchema,
  functionHandle: nullableStringSchema.optional(),
  shopifyFunctionId: nullableStringSchema,
  createdAt: nullableStringSchema.optional(),
  updatedAt: nullableStringSchema.optional(),
});
export type ValidationRecord = z.infer<typeof validationRecordSchema>;

export const cartTransformRecordSchema = z.strictObject({
  id: z.string(),
  title: nullableStringSchema,
  blockOnFailure: nullableBooleanSchema,
  functionId: nullableStringSchema,
  functionHandle: nullableStringSchema.optional(),
  shopifyFunctionId: nullableStringSchema,
  createdAt: nullableStringSchema.optional(),
  updatedAt: nullableStringSchema.optional(),
});
export type CartTransformRecord = z.infer<typeof cartTransformRecordSchema>;

export const taxAppConfigurationRecordSchema = z.strictObject({
  id: z.string(),
  ready: z.boolean(),
  state: z.string(),
  updatedAt: nullableStringSchema.optional(),
});
export type TaxAppConfigurationRecord = z.infer<typeof taxAppConfigurationRecordSchema>;

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

export const paymentScheduleRecordSchema = z.strictObject({
  id: z.string(),
  dueAt: nullableStringSchema,
  issuedAt: nullableStringSchema,
  completedAt: nullableStringSchema,
  completed: z.boolean().optional(),
  due: nullableBooleanSchema.optional(),
  amount: moneyV2Schema.nullable().optional(),
  balanceDue: moneyV2Schema.nullable().optional(),
  totalBalance: moneyV2Schema.nullable().optional(),
});
export type PaymentScheduleRecord = z.infer<typeof paymentScheduleRecordSchema>;

export const draftOrderPaymentTermsRecordSchema = z.strictObject({
  id: z.string(),
  due: z.boolean(),
  overdue: z.boolean(),
  dueInDays: nullableNumberSchema,
  paymentTermsName: z.string(),
  paymentTermsType: z.string(),
  translatedName: z.string(),
  paymentSchedules: z.array(paymentScheduleRecordSchema).optional(),
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
  lineItemQuantity: z.number().nullable().optional(),
  lineItemFulfillableQuantity: z.number().nullable().optional(),
  totalQuantity: z.number(),
  remainingQuantity: z.number(),
});
export type OrderFulfillmentOrderLineItemRecord = z.infer<typeof orderFulfillmentOrderLineItemRecordSchema>;

export const orderFulfillmentOrderMerchantRequestRecordSchema = z.strictObject({
  id: z.string(),
  kind: z.string(),
  message: nullableStringSchema.optional(),
  requestOptions: z.record(z.string(), z.unknown()).optional(),
  responseData: z.record(z.string(), z.unknown()).nullable().optional(),
  sentAt: z.string(),
});
export type OrderFulfillmentOrderMerchantRequestRecord = z.infer<
  typeof orderFulfillmentOrderMerchantRequestRecordSchema
>;

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
  merchantRequests: z.array(orderFulfillmentOrderMerchantRequestRecordSchema).optional(),
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

export const orderReturnLineItemRecordSchema = z.strictObject({
  id: z.string(),
  fulfillmentLineItemId: z.string(),
  lineItemId: nullableStringSchema,
  title: nullableStringSchema,
  quantity: z.number(),
  processedQuantity: z.number().optional(),
  returnReason: z.string(),
  returnReasonNote: z.string(),
  customerNote: nullableStringSchema.optional(),
});
export type OrderReturnLineItemRecord = z.infer<typeof orderReturnLineItemRecordSchema>;

export const orderReturnRecordSchema = z.strictObject({
  id: z.string(),
  orderId: z.string().optional(),
  name: z.string().optional(),
  status: nullableStringSchema,
  createdAt: z.string().optional(),
  closedAt: nullableStringSchema.optional(),
  totalQuantity: z.number().optional(),
  returnLineItems: z.array(orderReturnLineItemRecordSchema).optional(),
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
  paymentTerms: draftOrderPaymentTermsRecordSchema.nullable().optional(),
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

export const localeRecordSchema = z.strictObject({
  isoCode: z.string(),
  name: z.string(),
});
export type LocaleRecord = z.infer<typeof localeRecordSchema>;

export const shopLocaleRecordSchema = z.strictObject({
  locale: z.string(),
  name: z.string(),
  primary: z.boolean(),
  published: z.boolean(),
  marketWebPresenceIds: z.array(z.string()).default([]),
});
export type ShopLocaleRecord = z.infer<typeof shopLocaleRecordSchema>;

export const translationRecordSchema = z.strictObject({
  resourceId: z.string(),
  key: z.string(),
  locale: z.string(),
  value: z.string(),
  translatableContentDigest: z.string(),
  marketId: nullableStringSchema,
  updatedAt: z.string(),
  outdated: z.boolean(),
});
export type TranslationRecord = z.infer<typeof translationRecordSchema>;

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

export const sellingPlanRecordSchema = z.strictObject({
  id: z.string(),
  data: jsonObjectSchema.default({}),
});
export type SellingPlanRecord = z.infer<typeof sellingPlanRecordSchema>;

export const sellingPlanGroupRecordSchema = z.strictObject({
  id: z.string(),
  cursor: nullableStringSchema.optional(),
  appId: nullableStringSchema,
  name: z.string(),
  merchantCode: z.string(),
  description: nullableStringSchema,
  options: z.array(z.string()),
  position: nullableNumberSchema,
  summary: nullableStringSchema,
  createdAt: z.string(),
  productIds: z.array(z.string()).default([]),
  productVariantIds: z.array(z.string()).default([]),
  sellingPlans: z.array(sellingPlanRecordSchema).default([]),
});
export type SellingPlanGroupRecord = z.infer<typeof sellingPlanGroupRecordSchema>;

export const calculatedOrderRecordSchema = orderRecordSchema.extend({
  originalOrderId: z.string(),
});
export type CalculatedOrderRecord = z.infer<typeof calculatedOrderRecordSchema>;

export const abandonedCheckoutRecordSchema = z.strictObject({
  id: z.string(),
  cursor: nullableStringSchema.optional(),
  data: z.record(z.string(), jsonValueSchema),
});
export type AbandonedCheckoutRecord = z.infer<typeof abandonedCheckoutRecordSchema>;

export const abandonmentDeliveryActivityRecordSchema = z.strictObject({
  marketingActivityId: z.string(),
  deliveryStatus: z.string(),
  deliveredAt: nullableStringSchema.optional(),
  deliveryStatusChangeReason: nullableStringSchema.optional(),
});
export type AbandonmentDeliveryActivityRecord = z.infer<typeof abandonmentDeliveryActivityRecordSchema>;

export const abandonmentRecordSchema = z.strictObject({
  id: z.string(),
  abandonedCheckoutId: nullableStringSchema.optional(),
  cursor: nullableStringSchema.optional(),
  data: z.record(z.string(), jsonValueSchema),
  deliveryActivities: z.record(z.string(), abandonmentDeliveryActivityRecordSchema).default({}),
});
export type AbandonmentRecord = z.infer<typeof abandonmentRecordSchema>;

export const stateSnapshotSchema = z.strictObject({
  shop: shopRecordSchema.nullable().default(null),
  products: z.record(z.string(), productRecordSchema),
  productVariants: z.record(z.string(), productVariantRecordSchema),
  productOptions: z.record(z.string(), productOptionRecordSchema),
  productOperations: z.record(z.string(), productOperationRecordSchema).default({}),
  inventoryTransfers: z.record(z.string(), inventoryTransferRecordSchema).default({}),
  inventoryTransferOrder: z.array(z.string()).default([]),
  locations: z.record(z.string(), locationRecordSchema).default({}),
  locationOrder: z.array(z.string()).default([]),
  fulfillmentServices: z.record(z.string(), fulfillmentServiceRecordSchema).default({}),
  fulfillmentServiceOrder: z.array(z.string()).default([]),
  carrierServices: z.record(z.string(), carrierServiceRecordSchema).default({}),
  carrierServiceOrder: z.array(z.string()).default([]),
  inventoryShipments: z.record(z.string(), inventoryShipmentRecordSchema).default({}),
  inventoryShipmentOrder: z.array(z.string()).default([]),
  shippingPackages: z.record(z.string(), shippingPackageRecordSchema).default({}),
  shippingPackageOrder: z.array(z.string()).default([]),
  giftCards: z.record(z.string(), giftCardRecordSchema).default({}),
  giftCardOrder: z.array(z.string()).default([]),
  giftCardConfiguration: giftCardConfigurationRecordSchema.nullable().default(null),
  collections: z.record(z.string(), collectionRecordSchema),
  publications: z.record(z.string(), publicationRecordSchema).default({}),
  channels: z.record(z.string(), channelRecordSchema).default({}),
  customers: z.record(z.string(), customerRecordSchema),
  customerAddresses: z.record(z.string(), customerAddressRecordSchema).default({}),
  customerPaymentMethods: z.record(z.string(), customerPaymentMethodRecordSchema).default({}),
  customerAccountPages: z.record(z.string(), customerAccountPageRecordSchema).default({}),
  customerAccountPageOrder: z.array(z.string()).default([]),
  customerDataErasureRequests: z.record(z.string(), customerDataErasureRequestRecordSchema).default({}),
  storeCreditAccounts: z.record(z.string(), storeCreditAccountRecordSchema).default({}),
  storeCreditAccountTransactions: z.record(z.string(), storeCreditAccountTransactionRecordSchema).default({}),
  segments: z.record(z.string(), segmentRecordSchema).default({}),
  customerSegmentMembersQueries: z.record(z.string(), customerSegmentMembersQueryRecordSchema).default({}),
  webhookSubscriptions: z.record(z.string(), webhookSubscriptionRecordSchema).default({}),
  webhookSubscriptionOrder: z.array(z.string()).default([]),
  marketingActivities: z.record(z.string(), marketingRecordSchema).default({}),
  marketingActivityOrder: z.array(z.string()).default([]),
  marketingEvents: z.record(z.string(), marketingRecordSchema).default({}),
  marketingEventOrder: z.array(z.string()).default([]),
  marketingEngagements: z.record(z.string(), marketingEngagementRecordSchema).default({}),
  marketingEngagementOrder: z.array(z.string()).default([]),
  deletedMarketingActivityIds: z.record(z.string(), z.boolean()).default({}),
  deletedMarketingEventIds: z.record(z.string(), z.boolean()).default({}),
  deletedMarketingEngagementIds: z.record(z.string(), z.boolean()).default({}),
  onlineStoreArticles: z.record(z.string(), onlineStoreContentRecordSchema).default({}),
  onlineStoreArticleOrder: z.array(z.string()).default([]),
  onlineStoreBlogs: z.record(z.string(), onlineStoreContentRecordSchema).default({}),
  onlineStoreBlogOrder: z.array(z.string()).default([]),
  onlineStorePages: z.record(z.string(), onlineStoreContentRecordSchema).default({}),
  onlineStorePageOrder: z.array(z.string()).default([]),
  onlineStoreComments: z.record(z.string(), onlineStoreContentRecordSchema).default({}),
  onlineStoreCommentOrder: z.array(z.string()).default([]),
  onlineStoreThemes: z.record(z.string(), onlineStoreIntegrationRecordSchema).default({}),
  onlineStoreThemeOrder: z.array(z.string()).default([]),
  onlineStoreScriptTags: z.record(z.string(), onlineStoreIntegrationRecordSchema).default({}),
  onlineStoreScriptTagOrder: z.array(z.string()).default([]),
  onlineStoreWebPixels: z.record(z.string(), onlineStoreIntegrationRecordSchema).default({}),
  onlineStoreWebPixelOrder: z.array(z.string()).default([]),
  onlineStoreServerPixels: z.record(z.string(), onlineStoreIntegrationRecordSchema).default({}),
  onlineStoreServerPixelOrder: z.array(z.string()).default([]),
  onlineStoreStorefrontAccessTokens: z.record(z.string(), onlineStoreIntegrationRecordSchema).default({}),
  onlineStoreStorefrontAccessTokenOrder: z.array(z.string()).default([]),
  onlineStoreMobilePlatformApplications: z.record(z.string(), onlineStoreIntegrationRecordSchema).default({}),
  onlineStoreMobilePlatformApplicationOrder: z.array(z.string()).default([]),
  savedSearches: z.record(z.string(), savedSearchRecordSchema).default({}),
  savedSearchOrder: z.array(z.string()).default([]),
  bulkOperations: z.record(z.string(), bulkOperationRecordSchema).default({}),
  bulkOperationOrder: z.array(z.string()).default([]),
  bulkOperationResults: z.record(z.string(), z.string()).default({}),
  discounts: z.record(z.string(), discountRecordSchema).default({}),
  discountBulkOperations: z.record(z.string(), discountBulkOperationRecordSchema).default({}),
  paymentCustomizations: z.record(z.string(), paymentCustomizationRecordSchema).default({}),
  paymentCustomizationOrder: z.array(z.string()).default([]),
  paymentTermsTemplates: z
    .record(z.string(), paymentTermsTemplateRecordSchema)
    .default(defaultPaymentTermsTemplateRecordMap),
  paymentTermsTemplateOrder: z.array(z.string()).default(defaultPaymentTermsTemplateOrder),
  shopifyFunctions: z.record(z.string(), shopifyFunctionRecordSchema).default({}),
  shopifyFunctionOrder: z.array(z.string()).default([]),
  validations: z.record(z.string(), validationRecordSchema).default({}),
  validationOrder: z.array(z.string()).default([]),
  cartTransforms: z.record(z.string(), cartTransformRecordSchema).default({}),
  cartTransformOrder: z.array(z.string()).default([]),
  taxAppConfiguration: taxAppConfigurationRecordSchema.nullable().default(null),
  businessEntities: z.record(z.string(), businessEntityRecordSchema).default({}),
  businessEntityOrder: z.array(z.string()).default([]),
  b2bCompanies: z.record(z.string(), b2bCompanyRecordSchema).default({}),
  b2bCompanyOrder: z.array(z.string()).default([]),
  b2bCompanyContacts: z.record(z.string(), b2bCompanyContactRecordSchema).default({}),
  b2bCompanyContactOrder: z.array(z.string()).default([]),
  b2bCompanyContactRoles: z.record(z.string(), b2bCompanyContactRoleRecordSchema).default({}),
  b2bCompanyContactRoleOrder: z.array(z.string()).default([]),
  b2bCompanyLocations: z.record(z.string(), b2bCompanyLocationRecordSchema).default({}),
  b2bCompanyLocationOrder: z.array(z.string()).default([]),
  markets: z.record(z.string(), marketRecordSchema).default({}),
  marketOrder: z.array(z.string()).default([]),
  webPresences: z.record(z.string(), webPresenceRecordSchema).default({}),
  webPresenceOrder: z.array(z.string()).default([]),
  marketLocalizations: z.record(z.string(), marketLocalizationRecordSchema).default({}),
  availableLocales: z.array(localeRecordSchema).default([]),
  shopLocales: z.record(z.string(), shopLocaleRecordSchema).default({}),
  translations: z.record(z.string(), translationRecordSchema).default({}),
  catalogs: z.record(z.string(), catalogRecordSchema).default({}),
  catalogOrder: z.array(z.string()).default([]),
  priceLists: z.record(z.string(), priceListRecordSchema).default({}),
  priceListOrder: z.array(z.string()).default([]),
  deliveryProfiles: z.record(z.string(), deliveryProfileRecordSchema).default({}),
  deliveryProfileOrder: z.array(z.string()).default([]),
  sellingPlanGroups: z.record(z.string(), sellingPlanGroupRecordSchema).default({}),
  sellingPlanGroupOrder: z.array(z.string()).default([]),
  abandonedCheckouts: z.record(z.string(), abandonedCheckoutRecordSchema).default({}),
  abandonedCheckoutOrder: z.array(z.string()).default([]),
  abandonments: z.record(z.string(), abandonmentRecordSchema).default({}),
  abandonmentOrder: z.array(z.string()).default([]),
  productCollections: z.record(z.string(), productCollectionRecordSchema),
  productMedia: z.record(z.string(), productMediaRecordSchema),
  files: z.record(z.string(), fileRecordSchema).default({}),
  productMetafields: z.record(z.string(), productMetafieldRecordSchema),
  metafieldDefinitions: z.record(z.string(), metafieldDefinitionRecordSchema).default({}),
  metaobjectDefinitions: z.record(z.string(), metaobjectDefinitionRecordSchema).default({}),
  metaobjects: z.record(z.string(), metaobjectRecordSchema).default({}),
  customerMetafields: z.record(z.string(), customerMetafieldRecordSchema).default({}),
  deletedProductIds: z.record(z.string(), z.literal(true)),
  deletedInventoryTransferIds: z.record(z.string(), z.literal(true)).default({}),
  deletedFileIds: z.record(z.string(), z.literal(true)).default({}),
  deletedCollectionIds: z.record(z.string(), z.literal(true)),
  deletedPublicationIds: z.record(z.string(), z.literal(true)).default({}),
  deletedLocationIds: z.record(z.string(), z.literal(true)).default({}),
  deletedFulfillmentServiceIds: z.record(z.string(), z.literal(true)).default({}),
  deletedCarrierServiceIds: z.record(z.string(), z.literal(true)).default({}),
  deletedInventoryShipmentIds: z.record(z.string(), z.literal(true)).default({}),
  deletedShippingPackageIds: z.record(z.string(), z.literal(true)).default({}),
  deletedGiftCardIds: z.record(z.string(), z.literal(true)).default({}),
  deletedCustomerIds: z.record(z.string(), z.literal(true)),
  deletedCustomerAddressIds: z.record(z.string(), z.literal(true)).default({}),
  deletedCustomerPaymentMethodIds: z.record(z.string(), z.literal(true)).default({}),
  deletedSegmentIds: z.record(z.string(), z.literal(true)).default({}),
  deletedWebhookSubscriptionIds: z.record(z.string(), z.literal(true)).default({}),
  deletedOnlineStoreArticleIds: z.record(z.string(), z.literal(true)).default({}),
  deletedOnlineStoreBlogIds: z.record(z.string(), z.literal(true)).default({}),
  deletedOnlineStorePageIds: z.record(z.string(), z.literal(true)).default({}),
  deletedOnlineStoreCommentIds: z.record(z.string(), z.literal(true)).default({}),
  deletedOnlineStoreThemeIds: z.record(z.string(), z.literal(true)).default({}),
  deletedOnlineStoreScriptTagIds: z.record(z.string(), z.literal(true)).default({}),
  deletedOnlineStoreWebPixelIds: z.record(z.string(), z.literal(true)).default({}),
  deletedOnlineStoreServerPixelIds: z.record(z.string(), z.literal(true)).default({}),
  deletedOnlineStoreStorefrontAccessTokenIds: z.record(z.string(), z.literal(true)).default({}),
  deletedOnlineStoreMobilePlatformApplicationIds: z.record(z.string(), z.literal(true)).default({}),
  deletedSavedSearchIds: z.record(z.string(), z.literal(true)).default({}),
  deletedDiscountIds: z.record(z.string(), z.literal(true)).default({}),
  deletedPaymentCustomizationIds: z.record(z.string(), z.literal(true)).default({}),
  deletedValidationIds: z.record(z.string(), z.literal(true)).default({}),
  deletedCartTransformIds: z.record(z.string(), z.literal(true)).default({}),
  deletedMarketIds: z.record(z.string(), z.literal(true)).default({}),
  deletedCatalogIds: z.record(z.string(), z.literal(true)).default({}),
  deletedPriceListIds: z.record(z.string(), z.literal(true)).default({}),
  deletedWebPresenceIds: z.record(z.string(), z.literal(true)).default({}),
  deletedShopLocales: z.record(z.string(), z.literal(true)).default({}),
  deletedTranslations: z.record(z.string(), z.literal(true)).default({}),
  deletedDeliveryProfileIds: z.record(z.string(), z.literal(true)).default({}),
  deletedSellingPlanGroupIds: z.record(z.string(), z.literal(true)).default({}),
  deletedMetafieldDefinitionIds: z.record(z.string(), z.literal(true)).default({}),
  deletedMetaobjectDefinitionIds: z.record(z.string(), z.literal(true)).default({}),
  deletedMetaobjectIds: z.record(z.string(), z.literal(true)).default({}),
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
  bulkOperationImport?: {
    bulkOperationId: string;
    lineNumber: number | null;
    stagedUploadPath: string | null;
    outerRequestBody: Record<string, unknown>;
    innerMutation: string | null;
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
