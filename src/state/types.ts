export interface ProductSeoRecord {
  title: string | null;
  description: string | null;
}

export interface ProductCategoryRecord {
  id: string;
  fullName: string | null;
}

export interface ProductRecord {
  id: string;
  legacyResourceId: string | null;
  title: string;
  handle: string;
  status: 'ACTIVE' | 'ARCHIVED' | 'DRAFT';
  publicationIds: string[];
  createdAt: string;
  updatedAt: string;
  publishedAt?: string | null;
  vendor: string | null;
  productType: string | null;
  tags: string[];
  totalInventory: number | null;
  tracksInventory: boolean | null;
  descriptionHtml: string | null;
  onlineStorePreviewUrl: string | null;
  templateSuffix: string | null;
  seo: ProductSeoRecord;
  category: ProductCategoryRecord | null;
}

export interface ProductVariantSelectedOptionRecord {
  name: string;
  value: string;
}

export interface InventoryItemMeasurementWeightRecord {
  unit: string | null;
  value: number | null;
}

export interface InventoryItemMeasurementRecord {
  weight: InventoryItemMeasurementWeightRecord | null;
}

export interface InventoryLevelQuantityRecord {
  name: string;
  quantity: number | null;
  updatedAt: string | null;
}

export interface InventoryLevelLocationRecord {
  id: string;
  name: string | null;
}

export interface InventoryLevelRecord {
  id: string;
  cursor: string | null;
  location: InventoryLevelLocationRecord | null;
  quantities: InventoryLevelQuantityRecord[];
}

export interface InventoryItemRecord {
  id: string;
  tracked: boolean | null;
  requiresShipping: boolean | null;
  measurement: InventoryItemMeasurementRecord | null;
  countryCodeOfOrigin: string | null;
  provinceCodeOfOrigin: string | null;
  harmonizedSystemCode: string | null;
  inventoryLevels?: InventoryLevelRecord[] | null;
}

export interface ProductVariantRecord {
  id: string;
  productId: string;
  title: string;
  sku: string | null;
  barcode: string | null;
  price: string | null;
  compareAtPrice: string | null;
  taxable: boolean | null;
  inventoryPolicy: string | null;
  inventoryQuantity: number | null;
  selectedOptions: ProductVariantSelectedOptionRecord[];
  inventoryItem: InventoryItemRecord | null;
}

export interface ProductOptionValueRecord {
  id: string;
  name: string;
  hasVariants: boolean;
}

export interface ProductOptionRecord {
  id: string;
  productId: string;
  name: string;
  position: number;
  optionValues: ProductOptionValueRecord[];
}

export interface CollectionRecord {
  id: string;
  title: string;
  handle: string;
}

export interface PublicationRecord {
  id: string;
  name: string | null;
  cursor?: string | null;
}

export interface ProductCollectionRecord extends CollectionRecord {
  productId: string;
}

export interface ProductMediaRecord {
  key: string;
  productId: string;
  position: number;
  id?: string | null;
  mediaContentType: string | null;
  alt: string | null;
  status?: string | null;
  imageUrl?: string | null;
  previewImageUrl: string | null;
  sourceUrl?: string | null;
}

export interface ProductMetafieldRecord {
  id: string;
  productId: string;
  namespace: string;
  key: string;
  type: string | null;
  value: string | null;
}

export interface MoneyV2Record {
  amount: string | null;
  currencyCode: string | null;
}

export interface CustomerDefaultEmailAddressRecord {
  emailAddress: string | null;
}

export interface CustomerDefaultPhoneNumberRecord {
  phoneNumber: string | null;
}

export interface CustomerDefaultAddressRecord {
  address1: string | null;
  city: string | null;
  province: string | null;
  country: string | null;
  zip: string | null;
  formattedArea: string | null;
}

export interface CustomerRecord {
  id: string;
  firstName: string | null;
  lastName: string | null;
  displayName: string | null;
  email: string | null;
  legacyResourceId: string | null;
  locale: string | null;
  note: string | null;
  canDelete: boolean | null;
  verifiedEmail: boolean | null;
  taxExempt: boolean | null;
  state: string | null;
  tags: string[];
  numberOfOrders: string | number | null;
  amountSpent: MoneyV2Record | null;
  defaultEmailAddress: CustomerDefaultEmailAddressRecord | null;
  defaultPhoneNumber: CustomerDefaultPhoneNumberRecord | null;
  defaultAddress: CustomerDefaultAddressRecord | null;
  createdAt: string | null;
  updatedAt: string | null;
}

export interface ProductCatalogPageInfoRecord {
  hasNextPage: boolean;
  hasPreviousPage: boolean;
  startCursor: string | null;
  endCursor: string | null;
}

export interface ProductCatalogConnectionRecord {
  orderedProductIds: string[];
  cursorByProductId: Record<string, string>;
  pageInfo: ProductCatalogPageInfoRecord;
}

export interface CustomerCatalogPageInfoRecord {
  hasNextPage: boolean;
  hasPreviousPage: boolean;
  startCursor: string | null;
  endCursor: string | null;
}

export interface CustomerCatalogConnectionRecord {
  orderedCustomerIds: string[];
  cursorByCustomerId: Record<string, string>;
  pageInfo: CustomerCatalogPageInfoRecord;
}

export interface DraftOrderAttributeRecord {
  key: string;
  value: string | null;
}

export interface DraftOrderAddressRecord {
  firstName: string | null;
  lastName: string | null;
  address1: string | null;
  city: string | null;
  provinceCode: string | null;
  countryCodeV2: string | null;
  zip: string | null;
  phone: string | null;
}

export interface DraftOrderShippingLineRecord {
  title: string | null;
  code: string | null;
  originalPriceSet: {
    shopMoney: MoneyV2Record;
  } | null;
}

export interface DraftOrderLineItemRecord {
  id: string;
  title: string | null;
  quantity: number;
  sku: string | null;
  variantTitle: string | null;
  originalUnitPriceSet: {
    shopMoney: MoneyV2Record;
  } | null;
}

export interface DraftOrderRecord {
  id: string;
  name: string;
  invoiceUrl: string | null;
  status: string | null;
  ready: boolean | null;
  email: string | null;
  note: string | null;
  tags: string[];
  customAttributes: DraftOrderAttributeRecord[];
  billingAddress: DraftOrderAddressRecord | null;
  shippingAddress: DraftOrderAddressRecord | null;
  shippingLine: DraftOrderShippingLineRecord | null;
  createdAt: string;
  updatedAt: string;
  subtotalPriceSet: {
    shopMoney: MoneyV2Record;
  } | null;
  totalPriceSet: {
    shopMoney: MoneyV2Record;
  } | null;
  lineItems: DraftOrderLineItemRecord[];
}

export interface OrderCustomerRecord {
  id: string;
  email: string | null;
  displayName: string | null;
}

export interface OrderShippingLineRecord {
  title: string | null;
  code: string | null;
  originalPriceSet: {
    shopMoney: MoneyV2Record;
  } | null;
}

export interface OrderLineItemRecord {
  id: string;
  title: string | null;
  quantity: number;
  sku: string | null;
  variantTitle: string | null;
  originalUnitPriceSet: {
    shopMoney: MoneyV2Record;
  } | null;
}

export interface OrderRecord {
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  displayFinancialStatus: string | null;
  displayFulfillmentStatus: string | null;
  note: string | null;
  tags: string[];
  customAttributes: DraftOrderAttributeRecord[];
  billingAddress: DraftOrderAddressRecord | null;
  shippingAddress: DraftOrderAddressRecord | null;
  subtotalPriceSet: {
    shopMoney: MoneyV2Record;
  } | null;
  currentTotalPriceSet: {
    shopMoney: MoneyV2Record;
  } | null;
  totalPriceSet: {
    shopMoney: MoneyV2Record;
  } | null;
  customer: OrderCustomerRecord | null;
  shippingLines: OrderShippingLineRecord[];
  lineItems: OrderLineItemRecord[];
}

export interface CalculatedOrderRecord extends OrderRecord {
  originalOrderId: string;
}

export interface StateSnapshot {
  products: Record<string, ProductRecord>;
  productVariants: Record<string, ProductVariantRecord>;
  productOptions: Record<string, ProductOptionRecord>;
  collections: Record<string, CollectionRecord>;
  publications: Record<string, PublicationRecord>;
  customers: Record<string, CustomerRecord>;
  productCollections: Record<string, ProductCollectionRecord>;
  productMedia: Record<string, ProductMediaRecord>;
  productMetafields: Record<string, ProductMetafieldRecord>;
  deletedProductIds: Record<string, true>;
  deletedCollectionIds: Record<string, true>;
  deletedCustomerIds: Record<string, true>;
}

export interface NormalizedStateSnapshotFile {
  kind?: 'normalized-state-snapshot';
  baseState: StateSnapshot;
  customerCatalogConnection?: CustomerCatalogConnectionRecord | null;
  customerSearchConnections?: Record<string, CustomerCatalogConnectionRecord>;
}

export interface MutationLogEntry {
  id: string;
  receivedAt: string;
  operationName: string | null;
  path: string;
  query: string;
  variables: Record<string, unknown>;
  status: 'staged' | 'proxied' | 'committed' | 'failed';
  notes?: string;
}
