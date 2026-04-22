import { z } from 'zod';

const nullableStringSchema = z.string().nullable();
const nullableNumberSchema = z.number().nullable();
const nullableBooleanSchema = z.boolean().nullable();
const moneySetSchema = z.strictObject({
  shopMoney: z.strictObject({
    amount: nullableStringSchema,
    currencyCode: nullableStringSchema,
  }),
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
  title: z.string(),
  handle: z.string(),
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

export const moneyV2RecordSchema = moneySetSchema.shape.shopMoney;
export type MoneyV2Record = z.infer<typeof moneyV2RecordSchema>;

export const customerDefaultEmailAddressRecordSchema = z.strictObject({
  emailAddress: nullableStringSchema,
});
export type CustomerDefaultEmailAddressRecord = z.infer<typeof customerDefaultEmailAddressRecordSchema>;

export const customerDefaultPhoneNumberRecordSchema = z.strictObject({
  phoneNumber: nullableStringSchema,
});
export type CustomerDefaultPhoneNumberRecord = z.infer<typeof customerDefaultPhoneNumberRecordSchema>;

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
  state: nullableStringSchema,
  tags: z.array(z.string()),
  numberOfOrders: z.union([z.string(), z.number()]).nullable(),
  amountSpent: moneyV2RecordSchema.nullable(),
  defaultEmailAddress: customerDefaultEmailAddressRecordSchema.nullable(),
  defaultPhoneNumber: customerDefaultPhoneNumberRecordSchema.nullable(),
  defaultAddress: customerDefaultAddressRecordSchema.nullable(),
  createdAt: nullableStringSchema,
  updatedAt: nullableStringSchema,
});
export type CustomerRecord = z.infer<typeof customerRecordSchema>;

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
  city: nullableStringSchema,
  provinceCode: nullableStringSchema,
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

export const draftOrderLineItemRecordSchema = z.strictObject({
  id: z.string(),
  title: nullableStringSchema,
  quantity: z.number(),
  sku: nullableStringSchema,
  variantTitle: nullableStringSchema,
  originalUnitPriceSet: moneySetSchema.nullable(),
});
export type DraftOrderLineItemRecord = z.infer<typeof draftOrderLineItemRecordSchema>;

export const draftOrderRecordSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  invoiceUrl: nullableStringSchema,
  status: nullableStringSchema,
  ready: nullableBooleanSchema,
  email: nullableStringSchema,
  note: nullableStringSchema,
  tags: z.array(z.string()),
  customAttributes: z.array(draftOrderAttributeRecordSchema),
  billingAddress: draftOrderAddressRecordSchema.nullable(),
  shippingAddress: draftOrderAddressRecordSchema.nullable(),
  shippingLine: draftOrderShippingLineRecordSchema.nullable(),
  createdAt: z.string(),
  updatedAt: z.string(),
  subtotalPriceSet: moneySetSchema.nullable(),
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

export const orderShippingLineRecordSchema = z.strictObject({
  title: nullableStringSchema,
  code: nullableStringSchema,
  originalPriceSet: moneySetSchema.nullable(),
});
export type OrderShippingLineRecord = z.infer<typeof orderShippingLineRecordSchema>;

export const orderLineItemRecordSchema = z.strictObject({
  id: z.string(),
  title: nullableStringSchema,
  quantity: z.number(),
  sku: nullableStringSchema,
  variantTitle: nullableStringSchema,
  originalUnitPriceSet: moneySetSchema.nullable(),
});
export type OrderLineItemRecord = z.infer<typeof orderLineItemRecordSchema>;

export const orderRecordSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  createdAt: z.string(),
  updatedAt: z.string(),
  displayFinancialStatus: nullableStringSchema,
  displayFulfillmentStatus: nullableStringSchema,
  note: nullableStringSchema,
  tags: z.array(z.string()),
  customAttributes: z.array(draftOrderAttributeRecordSchema),
  billingAddress: draftOrderAddressRecordSchema.nullable(),
  shippingAddress: draftOrderAddressRecordSchema.nullable(),
  subtotalPriceSet: moneySetSchema.nullable(),
  currentTotalPriceSet: moneySetSchema.nullable(),
  totalPriceSet: moneySetSchema.nullable(),
  customer: orderCustomerRecordSchema.nullable(),
  shippingLines: z.array(orderShippingLineRecordSchema),
  lineItems: z.array(orderLineItemRecordSchema),
});
export type OrderRecord = z.infer<typeof orderRecordSchema>;

export const calculatedOrderRecordSchema = orderRecordSchema.extend({
  originalOrderId: z.string(),
});
export type CalculatedOrderRecord = z.infer<typeof calculatedOrderRecordSchema>;

export const stateSnapshotSchema = z.strictObject({
  products: z.record(z.string(), productRecordSchema),
  productVariants: z.record(z.string(), productVariantRecordSchema),
  productOptions: z.record(z.string(), productOptionRecordSchema),
  collections: z.record(z.string(), collectionRecordSchema),
  publications: z.record(z.string(), publicationRecordSchema).default({}),
  customers: z.record(z.string(), customerRecordSchema),
  productCollections: z.record(z.string(), productCollectionRecordSchema),
  productMedia: z.record(z.string(), productMediaRecordSchema),
  files: z.record(z.string(), fileRecordSchema).default({}),
  productMetafields: z.record(z.string(), productMetafieldRecordSchema),
  deletedProductIds: z.record(z.string(), z.literal(true)),
  deletedCollectionIds: z.record(z.string(), z.literal(true)),
  deletedCustomerIds: z.record(z.string(), z.literal(true)),
});
export type StateSnapshot = z.infer<typeof stateSnapshotSchema>;

export const normalizedStateSnapshotFileSchema = z.strictObject({
  kind: z.literal('normalized-state-snapshot').optional(),
  baseState: stateSnapshotSchema,
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
  status: 'staged' | 'proxied' | 'committed' | 'failed';
  interpreted: MutationLogInterpretedMetadata;
  notes?: string;
}
