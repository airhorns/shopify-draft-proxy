import type {
  CollectionRecord,
  FileRecord,
  MutationLogEntry,
  ProductCollectionRecord,
  ProductMediaRecord,
  ProductMetafieldRecord,
  ProductOptionRecord,
  ProductRecord,
  ProductVariantRecord,
  StateSnapshot,
} from './types.js';

const EMPTY_SNAPSHOT: StateSnapshot = {
  products: {},
  productVariants: {},
  productOptions: {},
  collections: {},
  productCollections: {},
  productMedia: {},
  files: {},
  productMetafields: {},
  deletedProductIds: {},
  deletedCollectionIds: {},
};

function cloneSnapshot(snapshot: StateSnapshot): StateSnapshot {
  return structuredClone(snapshot);
}

function compareProductsNewestFirst(left: ProductRecord, right: ProductRecord): number {
  return right.createdAt.localeCompare(left.createdAt) || right.id.localeCompare(left.id);
}

function compareResourceIds(leftId: string, rightId: string): number {
  const leftTail = Number.parseInt(leftId.split('/').at(-1) ?? '', 10);
  const rightTail = Number.parseInt(rightId.split('/').at(-1) ?? '', 10);
  if (Number.isFinite(leftTail) && Number.isFinite(rightTail)) {
    return leftTail - rightTail;
  }

  return leftId.localeCompare(rightId);
}

function ensureUpdatedAtAfterBase(baseUpdatedAt: string, stagedUpdatedAt: string): string {
  if (stagedUpdatedAt.localeCompare(baseUpdatedAt) > 0) {
    return stagedUpdatedAt;
  }

  const baseTime = Date.parse(baseUpdatedAt);
  if (Number.isNaN(baseTime)) {
    return stagedUpdatedAt;
  }

  return new Date(baseTime + 1000).toISOString();
}

function buildCollectionStorageKey(collection: ProductCollectionRecord): string {
  return `${collection.productId}::${collection.id}`;
}

function mergeCollectionRecords(base: CollectionRecord | null, staged: CollectionRecord | null): CollectionRecord | null {
  if (!base && !staged) {
    return null;
  }

  if (!base) {
    return staged ? structuredClone(staged) : null;
  }

  if (!staged) {
    return structuredClone(base);
  }

  return structuredClone(staged);
}

function mergeProductRecords(base: ProductRecord | null, staged: ProductRecord | null): ProductRecord | null {
  if (!base && !staged) {
    return null;
  }

  if (!base) {
    return staged ? structuredClone(staged) : null;
  }

  if (!staged) {
    return structuredClone(base);
  }

  return {
    id: staged.id,
    legacyResourceId: staged.legacyResourceId ?? base.legacyResourceId,
    title: staged.title,
    handle: staged.handle || base.handle,
    status: staged.status,
    publicationIds: structuredClone(staged.publicationIds),
    createdAt: base.createdAt,
    updatedAt: ensureUpdatedAtAfterBase(base.updatedAt, staged.updatedAt),
    vendor: staged.vendor ?? base.vendor,
    productType: staged.productType ?? base.productType,
    tags: staged.tags.length > 0 ? structuredClone(staged.tags) : structuredClone(base.tags),
    totalInventory: staged.totalInventory ?? base.totalInventory,
    tracksInventory: staged.tracksInventory ?? base.tracksInventory,
    descriptionHtml: staged.descriptionHtml ?? base.descriptionHtml,
    onlineStorePreviewUrl: staged.onlineStorePreviewUrl ?? base.onlineStorePreviewUrl,
    templateSuffix: staged.templateSuffix ?? base.templateSuffix,
    seo: staged.seo ?? base.seo,
    category: staged.category ?? base.category,
  };
}

export class InMemoryStore {
  private baseState: StateSnapshot = cloneSnapshot(EMPTY_SNAPSHOT);
  private stagedState: StateSnapshot = cloneSnapshot(EMPTY_SNAPSHOT);
  private mutationLog: MutationLogEntry[] = [];
  private stagedCollectionFamilies = new Set<string>();

  reset(): void {
    this.baseState = cloneSnapshot(EMPTY_SNAPSHOT);
    this.stagedState = cloneSnapshot(EMPTY_SNAPSHOT);
    this.mutationLog = [];
    this.stagedCollectionFamilies = new Set<string>();
  }

  getState(): { baseState: StateSnapshot; stagedState: StateSnapshot } {
    return {
      baseState: cloneSnapshot(this.baseState),
      stagedState: cloneSnapshot(this.stagedState),
    };
  }

  appendLog(entry: MutationLogEntry): void {
    this.mutationLog.push(entry);
  }

  getLog(): MutationLogEntry[] {
    return structuredClone(this.mutationLog);
  }

  upsertBaseProducts(products: ProductRecord[]): void {
    for (const product of products) {
      this.baseState.products[product.id] = structuredClone(product);
    }
  }

  upsertBaseCollections(collections: CollectionRecord[]): void {
    for (const collection of collections) {
      delete this.baseState.deletedCollectionIds?.[collection.id];
      this.baseState.collections[collection.id] = structuredClone(collection);
    }
  }

  stageCreateCollection(collection: CollectionRecord): CollectionRecord {
    delete this.stagedState.deletedCollectionIds[collection.id];
    this.stagedState.collections[collection.id] = structuredClone(collection);
    return structuredClone(collection);
  }

  stageUpdateCollection(collection: CollectionRecord): CollectionRecord {
    delete this.stagedState.deletedCollectionIds[collection.id];
    this.stagedState.collections[collection.id] = structuredClone(collection);
    return structuredClone(collection);
  }

  stageDeleteCollection(collectionId: string): void {
    delete this.stagedState.collections[collectionId];
    for (const [storageKey, collection] of Object.entries(this.stagedState.productCollections)) {
      if (collection.id === collectionId) {
        delete this.stagedState.productCollections[storageKey];
      }
    }
    for (const [storageKey, collection] of Object.entries(this.baseState.productCollections)) {
      if (collection.id === collectionId) {
        delete this.baseState.productCollections[storageKey];
      }
    }
    this.stagedState.deletedCollectionIds[collectionId] = true;
  }

  replaceBaseVariantsForProduct(productId: string, variants: ProductVariantRecord[]): void {
    for (const variant of Object.values(this.baseState.productVariants)) {
      if (variant.productId === productId) {
        delete this.baseState.productVariants[variant.id];
      }
    }

    for (const variant of variants) {
      this.baseState.productVariants[variant.id] = structuredClone(variant);
    }
  }

  replaceStagedVariantsForProduct(productId: string, variants: ProductVariantRecord[]): void {
    for (const variant of Object.values(this.stagedState.productVariants)) {
      if (variant.productId === productId) {
        delete this.stagedState.productVariants[variant.id];
      }
    }

    for (const variant of variants) {
      this.stagedState.productVariants[variant.id] = structuredClone(variant);
    }
  }

  replaceBaseOptionsForProduct(productId: string, options: ProductOptionRecord[]): void {
    for (const option of Object.values(this.baseState.productOptions)) {
      if (option.productId === productId) {
        delete this.baseState.productOptions[option.id];
      }
    }

    for (const option of options) {
      this.baseState.productOptions[option.id] = structuredClone(option);
    }
  }

  replaceStagedOptionsForProduct(productId: string, options: ProductOptionRecord[]): void {
    for (const option of Object.values(this.stagedState.productOptions)) {
      if (option.productId === productId) {
        delete this.stagedState.productOptions[option.id];
      }
    }

    for (const option of options) {
      this.stagedState.productOptions[option.id] = structuredClone(option);
    }
  }

  replaceBaseCollectionsForProduct(productId: string, collections: ProductCollectionRecord[]): void {
    for (const [storageKey, collection] of Object.entries(this.baseState.productCollections)) {
      if (collection.productId === productId) {
        delete this.baseState.productCollections[storageKey];
      }
    }

    for (const collection of collections) {
      this.baseState.productCollections[buildCollectionStorageKey(collection)] = structuredClone(collection);
    }
  }

  replaceStagedCollectionsForProduct(productId: string, collections: ProductCollectionRecord[]): void {
    for (const [storageKey, collection] of Object.entries(this.stagedState.productCollections)) {
      if (collection.productId === productId) {
        delete this.stagedState.productCollections[storageKey];
      }
    }

    this.stagedCollectionFamilies.add(productId);
    for (const collection of collections) {
      this.stagedState.productCollections[buildCollectionStorageKey(collection)] = structuredClone(collection);
    }
  }

  replaceBaseMediaForProduct(productId: string, media: ProductMediaRecord[]): void {
    for (const mediaRecord of Object.values(this.baseState.productMedia)) {
      if (mediaRecord.productId === productId) {
        delete this.baseState.productMedia[mediaRecord.key];
      }
    }

    for (const mediaRecord of media) {
      this.baseState.productMedia[mediaRecord.key] = structuredClone(mediaRecord);
    }
  }

  replaceStagedMediaForProduct(productId: string, media: ProductMediaRecord[]): void {
    for (const mediaRecord of Object.values(this.stagedState.productMedia)) {
      if (mediaRecord.productId === productId) {
        delete this.stagedState.productMedia[mediaRecord.key];
      }
    }

    for (const mediaRecord of media) {
      this.stagedState.productMedia[mediaRecord.key] = structuredClone(mediaRecord);
    }
  }

  stageCreateFiles(files: FileRecord[]): FileRecord[] {
    for (const file of files) {
      this.stagedState.files[file.id] = structuredClone(file);
    }

    return files.map((file) => structuredClone(file));
  }

  replaceBaseMetafieldsForProduct(productId: string, metafields: ProductMetafieldRecord[]): void {
    for (const metafield of Object.values(this.baseState.productMetafields)) {
      if (metafield.productId === productId) {
        delete this.baseState.productMetafields[metafield.id];
      }
    }

    for (const metafield of metafields) {
      this.baseState.productMetafields[metafield.id] = structuredClone(metafield);
    }
  }

  replaceStagedMetafieldsForProduct(productId: string, metafields: ProductMetafieldRecord[]): void {
    for (const metafield of Object.values(this.stagedState.productMetafields)) {
      if (metafield.productId === productId) {
        delete this.stagedState.productMetafields[metafield.id];
      }
    }

    for (const metafield of metafields) {
      this.stagedState.productMetafields[metafield.id] = structuredClone(metafield);
    }
  }

  stageCreateProduct(product: ProductRecord): ProductRecord {
    delete this.stagedState.deletedProductIds[product.id];
    this.stagedState.products[product.id] = structuredClone(product);
    return structuredClone(product);
  }

  stageUpdateProduct(product: ProductRecord): ProductRecord {
    delete this.stagedState.deletedProductIds[product.id];
    this.stagedState.products[product.id] = structuredClone(product);
    return structuredClone(product);
  }

  stageDeleteProduct(productId: string): void {
    delete this.stagedState.products[productId];
    for (const variant of Object.values(this.stagedState.productVariants)) {
      if (variant.productId === productId) {
        delete this.stagedState.productVariants[variant.id];
      }
    }
    for (const option of Object.values(this.stagedState.productOptions)) {
      if (option.productId === productId) {
        delete this.stagedState.productOptions[option.id];
      }
    }
    for (const [storageKey, collection] of Object.entries(this.stagedState.productCollections)) {
      if (collection.productId === productId) {
        delete this.stagedState.productCollections[storageKey];
      }
    }
    for (const mediaRecord of Object.values(this.stagedState.productMedia)) {
      if (mediaRecord.productId === productId) {
        delete this.stagedState.productMedia[mediaRecord.key];
      }
    }
    for (const metafield of Object.values(this.stagedState.productMetafields)) {
      if (metafield.productId === productId) {
        delete this.stagedState.productMetafields[metafield.id];
      }
    }
    this.stagedState.deletedProductIds[productId] = true;
  }

  getEffectiveProductById(productId: string): ProductRecord | null {
    if (this.stagedState.deletedProductIds[productId]) {
      return null;
    }

    return mergeProductRecords(this.baseState.products[productId] ?? null, this.stagedState.products[productId] ?? null);
  }

  listEffectiveProducts(): ProductRecord[] {
    const productIds = new Set([...Object.keys(this.baseState.products), ...Object.keys(this.stagedState.products)]);
    const merged: ProductRecord[] = [];

    for (const productId of Array.from(productIds)) {
      if (this.stagedState.deletedProductIds[productId]) {
        continue;
      }

      const product = mergeProductRecords(this.baseState.products[productId] ?? null, this.stagedState.products[productId] ?? null);
      if (product) {
        merged.push(product);
      }
    }

    return merged.sort(compareProductsNewestFirst);
  }

  getEffectiveVariantsByProductId(productId: string): ProductVariantRecord[] {
    if (this.stagedState.deletedProductIds[productId]) {
      return [];
    }

    const stagedVariants = Object.values(this.stagedState.productVariants)
      .filter((variant) => variant.productId === productId)
      .map((variant) => structuredClone(variant));

    const sourceVariants =
      stagedVariants.length > 0
        ? stagedVariants
        : Object.values(this.baseState.productVariants)
            .filter((variant) => variant.productId === productId)
            .map((variant) => structuredClone(variant));

    return sourceVariants.sort((left, right) => compareResourceIds(left.id, right.id));
  }

  getEffectiveVariantById(variantId: string): ProductVariantRecord | null {
    const stagedVariant = this.stagedState.productVariants[variantId];
    if (stagedVariant) {
      if (this.stagedState.deletedProductIds[stagedVariant.productId]) {
        return null;
      }

      return structuredClone(stagedVariant);
    }

    const baseVariant = this.baseState.productVariants[variantId];
    if (!baseVariant || this.stagedState.deletedProductIds[baseVariant.productId]) {
      return null;
    }

    const hasStagedVariantFamily = Object.values(this.stagedState.productVariants).some(
      (variant) => variant.productId === baseVariant.productId,
    );
    if (hasStagedVariantFamily) {
      return null;
    }

    return structuredClone(baseVariant);
  }

  findEffectiveVariantByInventoryItemId(inventoryItemId: string): ProductVariantRecord | null {
    const productIds = new Set<string>();
    for (const variant of Object.values(this.baseState.productVariants)) {
      productIds.add(variant.productId);
    }
    for (const variant of Object.values(this.stagedState.productVariants)) {
      productIds.add(variant.productId);
    }

    for (const productId of productIds) {
      const variant = this.getEffectiveVariantsByProductId(productId).find(
        (candidate) => candidate.inventoryItem?.id === inventoryItemId,
      );
      if (variant) {
        return variant;
      }
    }

    return null;
  }

  getEffectiveOptionsByProductId(productId: string): ProductOptionRecord[] {
    if (this.stagedState.deletedProductIds[productId]) {
      return [];
    }

    const stagedOptions = Object.values(this.stagedState.productOptions)
      .filter((option) => option.productId === productId)
      .map((option) => structuredClone(option));

    const sourceOptions =
      stagedOptions.length > 0
        ? stagedOptions
        : Object.values(this.baseState.productOptions)
            .filter((option) => option.productId === productId)
            .map((option) => structuredClone(option));

    return sourceOptions.sort((left, right) => left.position - right.position || compareResourceIds(left.id, right.id));
  }

  getEffectiveCollectionById(collectionId: string): CollectionRecord | null {
    if (this.stagedState.deletedCollectionIds[collectionId]) {
      return null;
    }

    const standalone = mergeCollectionRecords(
      this.baseState.collections[collectionId] ?? null,
      this.stagedState.collections[collectionId] ?? null,
    );
    if (standalone) {
      return standalone;
    }

    for (const product of this.listEffectiveProducts()) {
      const membership = this.getEffectiveCollectionsByProductId(product.id).find((collection) => collection.id === collectionId);
      if (membership) {
        return {
          id: membership.id,
          title: membership.title,
          handle: membership.handle,
        };
      }
    }

    return null;
  }

  listEffectiveCollections(): CollectionRecord[] {
    const collectionsById = new Map<string, CollectionRecord>();

    for (const collectionId of new Set([...Object.keys(this.baseState.collections), ...Object.keys(this.stagedState.collections)])) {
      const collection = this.getEffectiveCollectionById(collectionId);
      if (collection) {
        collectionsById.set(collection.id, collection);
      }
    }

    for (const product of this.listEffectiveProducts()) {
      for (const collection of this.getEffectiveCollectionsByProductId(product.id)) {
        if (!collectionsById.has(collection.id)) {
          collectionsById.set(collection.id, {
            id: collection.id,
            title: collection.title,
            handle: collection.handle,
          });
        }
      }
    }

    return Array.from(collectionsById.values()).sort((left, right) => left.title.localeCompare(right.title) || compareResourceIds(left.id, right.id));
  }

  getEffectiveCollectionsByProductId(productId: string): ProductCollectionRecord[] {
    if (this.stagedState.deletedProductIds[productId]) {
      return [];
    }

    const stagedCollections = Object.values(this.stagedState.productCollections)
      .filter((collection) => collection.productId === productId)
      .map((collection) => structuredClone(collection));

    const sourceCollections =
      this.stagedCollectionFamilies.has(productId)
        ? stagedCollections
        : Object.values(this.baseState.productCollections)
            .filter((collection) => collection.productId === productId)
            .map((collection) => structuredClone(collection));

    const visibleCollections = sourceCollections
      .filter((collection) => !this.stagedState.deletedCollectionIds[collection.id])
      .map((collection) => {
        const standalone = mergeCollectionRecords(
          this.baseState.collections[collection.id] ?? null,
          this.stagedState.collections[collection.id] ?? null,
        );
        if (!standalone) {
          return collection;
        }

        return {
          ...collection,
          title: standalone.title,
          handle: standalone.handle,
        };
      });

    return visibleCollections.sort((left, right) => left.title.localeCompare(right.title) || compareResourceIds(left.id, right.id));
  }

  getEffectiveMediaByProductId(productId: string): ProductMediaRecord[] {
    if (this.stagedState.deletedProductIds[productId]) {
      return [];
    }

    const stagedMedia = Object.values(this.stagedState.productMedia)
      .filter((mediaRecord) => mediaRecord.productId === productId)
      .map((mediaRecord) => structuredClone(mediaRecord));

    const sourceMedia =
      stagedMedia.length > 0
        ? stagedMedia
        : Object.values(this.baseState.productMedia)
            .filter((mediaRecord) => mediaRecord.productId === productId)
            .map((mediaRecord) => structuredClone(mediaRecord));

    return sourceMedia.sort((left, right) => left.position - right.position || left.key.localeCompare(right.key));
  }

  getEffectiveMetafieldsByProductId(productId: string): ProductMetafieldRecord[] {
    if (this.stagedState.deletedProductIds[productId]) {
      return [];
    }

    const stagedMetafields = Object.values(this.stagedState.productMetafields)
      .filter((metafield) => metafield.productId === productId)
      .map((metafield) => structuredClone(metafield));

    const sourceMetafields =
      stagedMetafields.length > 0
        ? stagedMetafields
        : Object.values(this.baseState.productMetafields)
            .filter((metafield) => metafield.productId === productId)
            .map((metafield) => structuredClone(metafield));

    return sourceMetafields.sort(
      (left, right) =>
        left.namespace.localeCompare(right.namespace) || left.key.localeCompare(right.key) || left.id.localeCompare(right.id),
    );
  }

  hasStagedProducts(): boolean {
    return (
      Object.keys(this.stagedState.products).length > 0 ||
      Object.keys(this.stagedState.collections).length > 0 ||
      Object.keys(this.stagedState.productVariants).length > 0 ||
      Object.keys(this.stagedState.productOptions).length > 0 ||
      Object.keys(this.stagedState.productCollections).length > 0 ||
      this.stagedCollectionFamilies.size > 0 ||
      Object.keys(this.stagedState.productMedia).length > 0 ||
      Object.keys(this.stagedState.productMetafields).length > 0 ||
      Object.keys(this.stagedState.deletedProductIds).length > 0 ||
      Object.keys(this.stagedState.deletedCollectionIds).length > 0
    );
  }
}

export const store = new InMemoryStore();
