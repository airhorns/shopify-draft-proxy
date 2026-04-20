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

export interface InventoryItemRecord {
  id: string;
  tracked: boolean | null;
  requiresShipping: boolean | null;
  measurement: InventoryItemMeasurementRecord | null;
  countryCodeOfOrigin: string | null;
  provinceCodeOfOrigin: string | null;
  harmonizedSystemCode: string | null;
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

export interface ProductCollectionRecord extends CollectionRecord {
  productId: string;
  position?: number;
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
}

export interface FileRecord {
  id: string;
  alt: string | null;
  contentType: string | null;
  createdAt: string;
  fileStatus: string;
  filename: string | null;
  originalSource: string;
  imageUrl: string | null;
  imageWidth: number | null;
  imageHeight: number | null;
}

export interface ProductMetafieldRecord {
  id: string;
  productId: string;
  namespace: string;
  key: string;
  type: string | null;
  value: string | null;
}

export interface StateSnapshot {
  products: Record<string, ProductRecord>;
  productVariants: Record<string, ProductVariantRecord>;
  productOptions: Record<string, ProductOptionRecord>;
  collections: Record<string, CollectionRecord>;
  productCollections: Record<string, ProductCollectionRecord>;
  productMedia: Record<string, ProductMediaRecord>;
  files: Record<string, FileRecord>;
  productMetafields: Record<string, ProductMetafieldRecord>;
  deletedProductIds: Record<string, true>;
  deletedCollectionIds: Record<string, true>;
}

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
  query: string;
  variables: Record<string, unknown>;
  status: 'staged' | 'proxied' | 'committed' | 'failed';
  interpreted: MutationLogInterpretedMetadata;
  notes?: string;
}
