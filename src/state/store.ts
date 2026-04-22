import type {
  CalculatedOrderRecord,
  CollectionRecord,
  CustomerCatalogConnectionRecord,
  CustomerRecord,
  DraftOrderRecord,
  FileRecord,
  MutationLogEntry,
  NormalizedStateSnapshotFile,
  OrderRecord,
  ProductCatalogConnectionRecord,
  ProductCollectionRecord,
  ProductMediaRecord,
  ProductMetafieldRecord,
  ProductOptionRecord,
  ProductRecord,
  ProductVariantRecord,
  PublicationRecord,
  StateSnapshot,
} from './types.js';

type MetaStateSnapshot = StateSnapshot & {
  orders: Record<string, OrderRecord>;
  draftOrders: Record<string, DraftOrderRecord>;
  calculatedOrders: Record<string, CalculatedOrderRecord>;
};

interface MetaRuntimeState {
  baseState: MetaStateSnapshot;
  stagedState: MetaStateSnapshot;
}

const EMPTY_SNAPSHOT: StateSnapshot = {
  products: {},
  productVariants: {},
  productOptions: {},
  collections: {},
  publications: {},
  customers: {},
  productCollections: {},
  productMedia: {},
  files: {},
  productMetafields: {},
  deletedProductIds: {},
  deletedCollectionIds: {},
  deletedCustomerIds: {},
};

function cloneSnapshot(snapshot: StateSnapshot): StateSnapshot {
  return structuredClone(snapshot);
}

function buildMetaStateSnapshot(
  snapshot: StateSnapshot,
  extraState: {
    orders?: Record<string, OrderRecord>;
    draftOrders?: Record<string, DraftOrderRecord>;
    calculatedOrders?: Record<string, CalculatedOrderRecord>;
  } = {},
): MetaStateSnapshot {
  return {
    ...cloneSnapshot(snapshot),
    orders: structuredClone(extraState.orders ?? {}),
    draftOrders: structuredClone(extraState.draftOrders ?? {}),
    calculatedOrders: structuredClone(extraState.calculatedOrders ?? {}),
  };
}

function compareProductsNewestFirst(left: ProductRecord, right: ProductRecord): number {
  return right.createdAt.localeCompare(left.createdAt) || right.id.localeCompare(left.id);
}

function compareCustomersNewestFirst(left: CustomerRecord, right: CustomerRecord): number {
  return (right.updatedAt ?? '').localeCompare(left.updatedAt ?? '') || right.id.localeCompare(left.id);
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

function readCollectionPosition(collection: ProductCollectionRecord): number | null {
  return typeof collection.position === 'number' && Number.isFinite(collection.position) ? collection.position : null;
}

function mergeCollectionRecords(
  base: CollectionRecord | null,
  staged: CollectionRecord | null,
): CollectionRecord | null {
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

function mergePublicationRecord(base: PublicationRecord | null): PublicationRecord | null {
  return base ? structuredClone(base) : null;
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

function mergeCustomerRecords(base: CustomerRecord | null, staged: CustomerRecord | null): CustomerRecord | null {
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
    firstName: staged.firstName,
    lastName: staged.lastName,
    displayName: staged.displayName,
    email: staged.email ?? base.email,
    legacyResourceId: staged.legacyResourceId ?? base.legacyResourceId,
    locale: staged.locale ?? base.locale,
    note: staged.note ?? base.note,
    canDelete: staged.canDelete ?? base.canDelete,
    verifiedEmail: staged.verifiedEmail ?? base.verifiedEmail,
    taxExempt: staged.taxExempt ?? base.taxExempt,
    state: staged.state ?? base.state,
    tags: structuredClone(staged.tags),
    numberOfOrders: staged.numberOfOrders ?? base.numberOfOrders,
    amountSpent: staged.amountSpent ?? base.amountSpent,
    defaultEmailAddress: staged.defaultEmailAddress ?? base.defaultEmailAddress,
    defaultPhoneNumber: staged.defaultPhoneNumber ?? base.defaultPhoneNumber,
    defaultAddress: staged.defaultAddress ?? base.defaultAddress,
    createdAt: base.createdAt,
    updatedAt:
      base.updatedAt && staged.updatedAt
        ? ensureUpdatedAtAfterBase(base.updatedAt, staged.updatedAt)
        : (staged.updatedAt ?? base.updatedAt),
  };
}

export class InMemoryStore {
  private initialBaseState: StateSnapshot = cloneSnapshot(EMPTY_SNAPSHOT);
  private initialProductSearchConnections: Record<string, ProductCatalogConnectionRecord> = {};
  private initialCustomerCatalogConnection: CustomerCatalogConnectionRecord | null = null;
  private initialCustomerSearchConnections: Record<string, CustomerCatalogConnectionRecord> = {};
  private initialBaseOrders: Record<string, OrderRecord> = {};
  private initialDraftOrders: Record<string, DraftOrderRecord> = {};
  private baseState: StateSnapshot = cloneSnapshot(EMPTY_SNAPSHOT);
  private stagedState: StateSnapshot = cloneSnapshot(EMPTY_SNAPSHOT);
  private mutationLog: MutationLogEntry[] = [];
  private stagedCollectionFamilies = new Set<string>();
  private stagedMediaFamilies = new Set<string>();
  private laggedTagSearchProductIds = new Map<string, number>();
  private laggedVariantSearchProductIds = new Set<string>();
  private baseProductSearchConnections: Record<string, ProductCatalogConnectionRecord> = {};
  private baseCustomerCatalogConnection: CustomerCatalogConnectionRecord | null = null;
  private baseCustomerSearchConnections: Record<string, CustomerCatalogConnectionRecord> = {};
  private baseOrders: Record<string, OrderRecord> = {};
  private stagedOrders: Record<string, OrderRecord> = {};
  private calculatedOrders: Record<string, CalculatedOrderRecord> = {};
  private stagedDraftOrders: Record<string, DraftOrderRecord> = {};

  installSnapshot(snapshotFile: NormalizedStateSnapshotFile): void {
    this.initialBaseState = cloneSnapshot(snapshotFile.baseState);
    this.initialProductSearchConnections = structuredClone(snapshotFile.productSearchConnections ?? {});
    this.initialCustomerCatalogConnection = snapshotFile.customerCatalogConnection
      ? structuredClone(snapshotFile.customerCatalogConnection)
      : null;
    this.initialCustomerSearchConnections = structuredClone(snapshotFile.customerSearchConnections ?? {});
    this.initialBaseOrders = {};
    this.restoreInitialState();
  }

  restoreInitialState(): void {
    this.baseState = cloneSnapshot(this.initialBaseState);
    this.stagedState = cloneSnapshot(EMPTY_SNAPSHOT);
    this.mutationLog = [];
    this.stagedCollectionFamilies = new Set<string>();
    this.stagedMediaFamilies = new Set<string>();
    this.laggedTagSearchProductIds = new Map<string, number>();
    this.laggedVariantSearchProductIds = new Set<string>();
    this.baseProductSearchConnections = structuredClone(this.initialProductSearchConnections);
    this.baseCustomerCatalogConnection = this.initialCustomerCatalogConnection
      ? structuredClone(this.initialCustomerCatalogConnection)
      : null;
    this.baseCustomerSearchConnections = structuredClone(this.initialCustomerSearchConnections);
    this.baseOrders = structuredClone(this.initialBaseOrders);
    this.stagedOrders = {};
    this.calculatedOrders = {};
    this.stagedDraftOrders = structuredClone(this.initialDraftOrders);
  }

  reset(): void {
    this.initialBaseState = cloneSnapshot(EMPTY_SNAPSHOT);
    this.initialProductSearchConnections = {};
    this.initialCustomerCatalogConnection = null;
    this.initialCustomerSearchConnections = {};
    this.initialBaseOrders = {};
    this.initialDraftOrders = {};
    this.restoreInitialState();
  }

  getState(): MetaRuntimeState {
    return {
      baseState: buildMetaStateSnapshot(this.baseState, {
        orders: this.baseOrders,
      }),
      stagedState: buildMetaStateSnapshot(this.stagedState, {
        orders: this.stagedOrders,
        draftOrders: this.stagedDraftOrders,
        calculatedOrders: this.calculatedOrders,
      }),
    };
  }

  appendLog(entry: MutationLogEntry): void {
    this.mutationLog.push(structuredClone(entry));
  }

  getLog(): MutationLogEntry[] {
    return structuredClone(this.mutationLog);
  }

  updateLogEntry(
    entryId: string,
    updates: Partial<Pick<MutationLogEntry, 'status' | 'notes'>>,
  ): MutationLogEntry | null {
    const entry = this.mutationLog.find((candidate) => candidate.id === entryId);
    if (!entry) {
      return null;
    }

    if (updates.status) {
      entry.status = updates.status;
    }

    if (updates.notes !== undefined) {
      entry.notes = updates.notes;
    }

    return structuredClone(entry);
  }

  upsertBaseProducts(products: ProductRecord[]): void {
    for (const product of products) {
      this.baseState.products[product.id] = structuredClone(product);
    }
  }

  upsertBaseCustomers(customers: CustomerRecord[]): void {
    for (const customer of customers) {
      delete this.baseState.deletedCustomerIds[customer.id];
      delete this.stagedState.deletedCustomerIds[customer.id];
      this.baseState.customers[customer.id] = structuredClone(customer);
    }
  }

  stageCreateCustomer(customer: CustomerRecord): CustomerRecord {
    delete this.stagedState.deletedCustomerIds[customer.id];
    this.stagedState.customers[customer.id] = structuredClone(customer);
    return structuredClone(customer);
  }

  stageUpdateCustomer(customer: CustomerRecord): CustomerRecord {
    delete this.stagedState.deletedCustomerIds[customer.id];
    this.stagedState.customers[customer.id] = structuredClone(customer);
    return structuredClone(customer);
  }

  stageDeleteCustomer(customerId: string): void {
    delete this.stagedState.customers[customerId];
    this.stagedState.deletedCustomerIds[customerId] = true;
  }

  upsertBaseOrders(orders: OrderRecord[]): void {
    for (const order of orders) {
      this.baseOrders[order.id] = structuredClone(order);
    }
  }

  stageCreateOrder(order: OrderRecord): OrderRecord {
    this.stagedOrders[order.id] = structuredClone(order);
    return structuredClone(order);
  }

  getOrderById(orderId: string): OrderRecord | null {
    const order = this.stagedOrders[orderId] ?? this.baseOrders[orderId];
    return order ? structuredClone(order) : null;
  }

  getOrders(): OrderRecord[] {
    const mergedOrders = new Map<string, OrderRecord>();
    for (const order of Object.values(this.baseOrders)) {
      mergedOrders.set(order.id, structuredClone(order));
    }
    for (const order of Object.values(this.stagedOrders)) {
      mergedOrders.set(order.id, structuredClone(order));
    }

    return Array.from(mergedOrders.values()).sort(
      (left, right) => right.createdAt.localeCompare(left.createdAt) || compareResourceIds(right.id, left.id),
    );
  }

  stageCalculatedOrder(calculatedOrder: CalculatedOrderRecord): CalculatedOrderRecord {
    this.calculatedOrders[calculatedOrder.id] = structuredClone(calculatedOrder);
    return structuredClone(calculatedOrder);
  }

  getCalculatedOrderById(calculatedOrderId: string): CalculatedOrderRecord | null {
    const calculatedOrder = this.calculatedOrders[calculatedOrderId];
    return calculatedOrder ? structuredClone(calculatedOrder) : null;
  }

  updateCalculatedOrder(calculatedOrder: CalculatedOrderRecord): CalculatedOrderRecord {
    this.calculatedOrders[calculatedOrder.id] = structuredClone(calculatedOrder);
    return structuredClone(calculatedOrder);
  }

  discardCalculatedOrder(calculatedOrderId: string): void {
    delete this.calculatedOrders[calculatedOrderId];
  }

  updateOrder(order: OrderRecord): OrderRecord {
    this.stagedOrders[order.id] = structuredClone(order);
    return structuredClone(order);
  }

  stageCreateDraftOrder(draftOrder: DraftOrderRecord): DraftOrderRecord {
    this.stagedDraftOrders[draftOrder.id] = structuredClone(draftOrder);
    return structuredClone(draftOrder);
  }

  updateDraftOrder(draftOrder: DraftOrderRecord): DraftOrderRecord {
    this.stagedDraftOrders[draftOrder.id] = structuredClone(draftOrder);
    return structuredClone(draftOrder);
  }

  deleteDraftOrder(draftOrderId: string): void {
    delete this.stagedDraftOrders[draftOrderId];
  }

  getDraftOrderById(draftOrderId: string): DraftOrderRecord | null {
    const draftOrder = this.stagedDraftOrders[draftOrderId];
    return draftOrder ? structuredClone(draftOrder) : null;
  }

  getDraftOrders(): DraftOrderRecord[] {
    return Object.values(this.stagedDraftOrders)
      .map((draftOrder) => structuredClone(draftOrder))
      .sort((left, right) => right.createdAt.localeCompare(left.createdAt) || compareResourceIds(right.id, left.id));
  }

  hasDraftOrders(): boolean {
    return Object.keys(this.stagedDraftOrders).length > 0;
  }

  setBaseProductSearchConnection(key: string, connection: ProductCatalogConnectionRecord | null): void {
    if (!connection) {
      delete this.baseProductSearchConnections[key];
      return;
    }

    this.baseProductSearchConnections[key] = structuredClone(connection);
  }

  getBaseProductSearchConnection(key: string): ProductCatalogConnectionRecord | null {
    const connection = this.baseProductSearchConnections[key];
    return connection ? structuredClone(connection) : null;
  }

  setBaseCustomerCatalogConnection(connection: CustomerCatalogConnectionRecord | null): void {
    this.baseCustomerCatalogConnection = connection ? structuredClone(connection) : null;
  }

  getBaseCustomerCatalogConnection(): CustomerCatalogConnectionRecord | null {
    return this.baseCustomerCatalogConnection ? structuredClone(this.baseCustomerCatalogConnection) : null;
  }

  setBaseCustomerSearchConnection(key: string, connection: CustomerCatalogConnectionRecord | null): void {
    if (!connection) {
      delete this.baseCustomerSearchConnections[key];
      return;
    }

    this.baseCustomerSearchConnections[key] = structuredClone(connection);
  }

  getBaseCustomerSearchConnection(key: string): CustomerCatalogConnectionRecord | null {
    const connection = this.baseCustomerSearchConnections[key];
    return connection ? structuredClone(connection) : null;
  }

  upsertBaseCollections(collections: CollectionRecord[]): void {
    for (const collection of collections) {
      delete this.baseState.deletedCollectionIds?.[collection.id];
      this.baseState.collections[collection.id] = structuredClone(collection);
    }
  }

  private nextCollectionPosition(collectionId: string): number {
    const positions = [
      ...Object.values(this.baseState.productCollections),
      ...Object.values(this.stagedState.productCollections),
    ]
      .filter((collection) => collection.id === collectionId)
      .map(readCollectionPosition)
      .filter((position): position is number => position !== null);

    return positions.length > 0 ? Math.max(...positions) + 1 : 0;
  }

  private withCollectionPositions(
    collections: ProductCollectionRecord[],
    previousCollections: ProductCollectionRecord[],
  ): ProductCollectionRecord[] {
    const previousById = new Map(previousCollections.map((collection) => [collection.id, collection]));

    return collections.map((collection) => {
      const requestedPosition = readCollectionPosition(collection);
      const previousPosition = readCollectionPosition(previousById.get(collection.id) ?? collection);
      const position = requestedPosition ?? previousPosition ?? this.nextCollectionPosition(collection.id);

      return {
        ...structuredClone(collection),
        position,
      };
    });
  }

  upsertBasePublications(publications: PublicationRecord[]): void {
    for (const publication of publications) {
      this.baseState.publications[publication.id] = structuredClone(publication);
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

    this.laggedVariantSearchProductIds.delete(productId);
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
    const previousCollections = Object.values(this.baseState.productCollections)
      .filter((collection) => collection.productId === productId)
      .map((collection) => structuredClone(collection));

    for (const [storageKey, collection] of Object.entries(this.baseState.productCollections)) {
      if (collection.productId === productId) {
        delete this.baseState.productCollections[storageKey];
      }
    }

    for (const collection of this.withCollectionPositions(collections, previousCollections)) {
      this.baseState.productCollections[buildCollectionStorageKey(collection)] = structuredClone(collection);
    }
  }

  replaceStagedCollectionsForProduct(productId: string, collections: ProductCollectionRecord[]): void {
    const previousCollections = Object.values(this.stagedState.productCollections)
      .filter((collection) => collection.productId === productId)
      .map((collection) => structuredClone(collection));

    for (const [storageKey, collection] of Object.entries(this.stagedState.productCollections)) {
      if (collection.productId === productId) {
        delete this.stagedState.productCollections[storageKey];
      }
    }

    this.stagedCollectionFamilies.add(productId);
    for (const collection of this.withCollectionPositions(collections, previousCollections)) {
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

    this.stagedMediaFamilies.add(productId);
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

  getBaseProductById(productId: string): ProductRecord | null {
    if (this.stagedState.deletedProductIds[productId]) {
      return null;
    }

    const baseProduct = this.baseState.products[productId];
    return baseProduct ? structuredClone(baseProduct) : null;
  }

  markTagSearchLagged(productId: string, lagMs = 10_000): void {
    this.laggedTagSearchProductIds.set(productId, Date.now() + lagMs);
  }

  isTagSearchLagged(productId: string): boolean {
    const lagExpiresAt = this.laggedTagSearchProductIds.get(productId);
    if (lagExpiresAt === undefined) {
      return false;
    }
    if (Date.now() < lagExpiresAt) {
      return true;
    }
    this.laggedTagSearchProductIds.delete(productId);
    return false;
  }

  markVariantSearchLagged(productId: string): void {
    this.laggedVariantSearchProductIds.add(productId);
  }

  isVariantSearchLagged(productId: string): boolean {
    return this.laggedVariantSearchProductIds.has(productId);
  }

  getEffectiveProductById(productId: string): ProductRecord | null {
    if (this.stagedState.deletedProductIds[productId]) {
      return null;
    }

    return mergeProductRecords(
      this.baseState.products[productId] ?? null,
      this.stagedState.products[productId] ?? null,
    );
  }

  listEffectiveProducts(): ProductRecord[] {
    const productIds = new Set([...Object.keys(this.baseState.products), ...Object.keys(this.stagedState.products)]);
    const merged: ProductRecord[] = [];

    for (const productId of Array.from(productIds)) {
      if (this.stagedState.deletedProductIds[productId]) {
        continue;
      }

      const product = mergeProductRecords(
        this.baseState.products[productId] ?? null,
        this.stagedState.products[productId] ?? null,
      );
      if (product) {
        merged.push(product);
      }
    }

    return merged.sort(compareProductsNewestFirst);
  }

  getEffectiveCustomerById(customerId: string): CustomerRecord | null {
    if (this.stagedState.deletedCustomerIds[customerId]) {
      return null;
    }

    return mergeCustomerRecords(
      this.baseState.customers[customerId] ?? null,
      this.stagedState.customers[customerId] ?? null,
    );
  }

  listEffectiveCustomers(): CustomerRecord[] {
    const customerIds = new Set([...Object.keys(this.baseState.customers), ...Object.keys(this.stagedState.customers)]);
    const merged: CustomerRecord[] = [];

    for (const customerId of Array.from(customerIds)) {
      if (this.stagedState.deletedCustomerIds[customerId]) {
        continue;
      }

      const customer = mergeCustomerRecords(
        this.baseState.customers[customerId] ?? null,
        this.stagedState.customers[customerId] ?? null,
      );
      if (customer) {
        merged.push(customer);
      }
    }

    return merged.sort(compareCustomersNewestFirst);
  }

  hasBaseCustomers(): boolean {
    return Object.keys(this.baseState.customers).length > 0;
  }

  hasStagedCustomers(): boolean {
    return (
      Object.keys(this.stagedState.customers).length > 0 || Object.keys(this.stagedState.deletedCustomerIds).length > 0
    );
  }

  getBaseVariantsByProductId(productId: string): ProductVariantRecord[] {
    if (this.stagedState.deletedProductIds[productId]) {
      return [];
    }

    return Object.values(this.baseState.productVariants)
      .filter((variant) => variant.productId === productId)
      .map((variant) => structuredClone(variant));
  }

  getEffectiveVariantsByProductId(productId: string): ProductVariantRecord[] {
    if (this.stagedState.deletedProductIds[productId]) {
      return [];
    }

    const stagedVariants = Object.values(this.stagedState.productVariants)
      .filter((variant) => variant.productId === productId)
      .map((variant) => structuredClone(variant));

    const sourceVariants = stagedVariants.length > 0 ? stagedVariants : this.getBaseVariantsByProductId(productId);

    return sourceVariants;
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
      const membership = this.getEffectiveCollectionsByProductId(product.id).find(
        (collection) => collection.id === collectionId,
      );
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

    for (const collectionId of new Set([
      ...Object.keys(this.baseState.collections),
      ...Object.keys(this.stagedState.collections),
    ])) {
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

    return Array.from(collectionsById.values()).sort(
      (left, right) => left.title.localeCompare(right.title) || compareResourceIds(left.id, right.id),
    );
  }

  listEffectivePublications(): PublicationRecord[] {
    const publicationsById = new Map<string, PublicationRecord>();

    for (const publicationId of Object.keys(this.baseState.publications)) {
      const publication = mergePublicationRecord(this.baseState.publications[publicationId] ?? null);
      if (publication) {
        publicationsById.set(publication.id, publication);
      }
    }

    for (const product of this.listEffectiveProducts()) {
      for (const publicationId of product.publicationIds) {
        if (!publicationsById.has(publicationId)) {
          publicationsById.set(publicationId, {
            id: publicationId,
            name: null,
          });
        }
      }
    }

    return Array.from(publicationsById.values()).sort((left, right) => compareResourceIds(left.id, right.id));
  }

  getEffectiveCollectionsByProductId(productId: string): ProductCollectionRecord[] {
    if (this.stagedState.deletedProductIds[productId]) {
      return [];
    }

    const stagedCollections = Object.values(this.stagedState.productCollections)
      .filter((collection) => collection.productId === productId)
      .map((collection) => structuredClone(collection));

    const sourceCollections = this.stagedCollectionFamilies.has(productId)
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

    return visibleCollections.sort(
      (left, right) => left.title.localeCompare(right.title) || compareResourceIds(left.id, right.id),
    );
  }

  getEffectiveMediaByProductId(productId: string): ProductMediaRecord[] {
    if (this.stagedState.deletedProductIds[productId]) {
      return [];
    }

    const stagedMedia = Object.values(this.stagedState.productMedia)
      .filter((mediaRecord) => mediaRecord.productId === productId)
      .map((mediaRecord) => structuredClone(mediaRecord));

    const sourceMedia = this.stagedMediaFamilies.has(productId)
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
        left.namespace.localeCompare(right.namespace) ||
        left.key.localeCompare(right.key) ||
        left.id.localeCompare(right.id),
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
      this.stagedMediaFamilies.size > 0 ||
      Object.keys(this.stagedState.productMetafields).length > 0 ||
      Object.keys(this.stagedState.deletedProductIds).length > 0 ||
      Object.keys(this.stagedState.deletedCollectionIds).length > 0
    );
  }
}

export const store = new InMemoryStore();
