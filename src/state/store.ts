import type {
  BusinessEntityRecord,
  CalculatedOrderRecord,
  CollectionRecord,
  CustomerCatalogConnectionRecord,
  CustomerMergeRequestRecord,
  CustomerMetafieldRecord,
  CustomerRecord,
  DiscountRecord,
  DraftOrderRecord,
  FileRecord,
  LocationRecord,
  MarketRecord,
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
  SegmentRecord,
  ShopRecord,
  StateSnapshot,
} from './types.js';
import { compareShopifyResourceIds } from '../shopify/resource-ids.js';

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
  shop: null,
  products: {},
  productVariants: {},
  productOptions: {},
  locations: {},
  locationOrder: [],
  collections: {},
  publications: {},
  customers: {},
  segments: {},
  discounts: {},
  businessEntities: {},
  businessEntityOrder: [],
  markets: {},
  marketOrder: [],
  productCollections: {},
  productMedia: {},
  files: {},
  productMetafields: {},
  customerMetafields: {},
  deletedProductIds: {},
  deletedFileIds: {},
  deletedCollectionIds: {},
  deletedCustomerIds: {},
  deletedDiscountIds: {},
  deletedMarketIds: {},
  mergedCustomerIds: {},
  customerMergeRequests: {},
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

function mergeLocationRecords(base: LocationRecord | null, staged: LocationRecord | null): LocationRecord | null {
  if (!base && !staged) {
    return null;
  }

  if (!base) {
    return staged ? structuredClone(staged) : null;
  }

  if (!staged) {
    return structuredClone(base);
  }

  return structuredClone({
    ...base,
    ...staged,
    address:
      staged.address === undefined
        ? base.address
        : staged.address === null
          ? null
          : {
              ...(base.address ?? {
                address1: null,
                address2: null,
                city: null,
                country: null,
                countryCode: null,
                formatted: [],
                latitude: null,
                longitude: null,
                phone: null,
                province: null,
                provinceCode: null,
                zip: null,
              }),
              ...staged.address,
            },
    suggestedAddresses: staged.suggestedAddresses ?? base.suggestedAddresses,
    metafields: staged.metafields ?? base.metafields,
  });
}

function collectionFromMembership(membership: ProductCollectionRecord): CollectionRecord {
  const { productId: _productId, position: _position, ...collection } = membership;
  return structuredClone(collection);
}

function mergePublicationRecord(base: PublicationRecord | null): PublicationRecord | null {
  return base ? structuredClone(base) : null;
}

function mergeShopRecords(base: ShopRecord | null, staged: ShopRecord | null): ShopRecord | null {
  if (!base && !staged) {
    return null;
  }

  return structuredClone(staged ?? base);
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
    taxExemptions: structuredClone(staged.taxExemptions ?? base.taxExemptions ?? []),
    state: staged.state ?? base.state,
    tags: structuredClone(staged.tags),
    numberOfOrders: staged.numberOfOrders ?? base.numberOfOrders,
    amountSpent: staged.amountSpent ?? base.amountSpent,
    defaultEmailAddress: staged.defaultEmailAddress ?? base.defaultEmailAddress,
    defaultPhoneNumber: staged.defaultPhoneNumber ?? base.defaultPhoneNumber,
    emailMarketingConsent: staged.emailMarketingConsent ?? base.emailMarketingConsent,
    smsMarketingConsent: staged.smsMarketingConsent ?? base.smsMarketingConsent,
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
  private baseMarketsRootPayloads: Record<string, unknown> = {};
  private baseSegmentsRootPayloads: Record<string, unknown> = {};
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
    this.baseMarketsRootPayloads = {};
    this.baseSegmentsRootPayloads = {};
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
      delete this.baseState.mergedCustomerIds[customer.id];
      delete this.stagedState.mergedCustomerIds[customer.id];
      this.baseState.customers[customer.id] = structuredClone(customer);
    }
  }

  upsertBaseSegments(segments: SegmentRecord[]): void {
    for (const segment of segments) {
      this.baseState.segments[segment.id] = structuredClone(segment);
    }
  }

  getBaseSegmentById(segmentId: string): SegmentRecord | null {
    const segment = this.baseState.segments[segmentId] ?? null;
    return segment ? structuredClone(segment) : null;
  }

  listBaseSegments(): SegmentRecord[] {
    return Object.values(this.baseState.segments)
      .map((segment) => structuredClone(segment))
      .sort(
        (left, right) =>
          (left.creationDate ?? '').localeCompare(right.creationDate ?? '') || left.id.localeCompare(right.id),
      );
  }

  upsertBaseDiscounts(discounts: DiscountRecord[]): void {
    for (const discount of discounts) {
      delete this.baseState.deletedDiscountIds[discount.id];
      delete this.stagedState.deletedDiscountIds[discount.id];
      this.baseState.discounts[discount.id] = structuredClone(discount);
    }
  }

  upsertBaseBusinessEntities(businessEntities: BusinessEntityRecord[]): void {
    for (const businessEntity of businessEntities) {
      this.baseState.businessEntities[businessEntity.id] = structuredClone(businessEntity);
      if (!this.baseState.businessEntityOrder.includes(businessEntity.id)) {
        this.baseState.businessEntityOrder.push(businessEntity.id);
      }
    }
  }

  upsertBaseShop(shop: ShopRecord): void {
    this.baseState.shop = structuredClone(shop);
  }

  upsertBaseLocations(locations: LocationRecord[]): void {
    for (const location of locations) {
      this.baseState.locations[location.id] = structuredClone(location);
      if (!this.baseState.locationOrder.includes(location.id)) {
        this.baseState.locationOrder.push(location.id);
      }
    }
  }

  listBaseLocations(): LocationRecord[] {
    const orderedIds = new Set(this.baseState.locationOrder);
    const orderedLocations = this.baseState.locationOrder
      .map((id) => this.baseState.locations[id] ?? null)
      .filter((location): location is LocationRecord => location !== null);
    const unorderedLocations = Object.values(this.baseState.locations)
      .filter((location) => !orderedIds.has(location.id))
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedLocations, ...unorderedLocations]);
  }

  getBaseLocationById(locationId: string): LocationRecord | null {
    const location = this.baseState.locations[locationId] ?? null;
    return location ? structuredClone(location) : null;
  }

  stageCreateLocation(location: LocationRecord): LocationRecord {
    this.stagedState.locations[location.id] = structuredClone(location);
    if (!this.stagedState.locationOrder.includes(location.id)) {
      this.stagedState.locationOrder.push(location.id);
    }
    return structuredClone(location);
  }

  stageUpdateLocation(location: LocationRecord): LocationRecord {
    this.stagedState.locations[location.id] = structuredClone(location);
    if (!this.baseState.locationOrder.includes(location.id) && !this.stagedState.locationOrder.includes(location.id)) {
      this.stagedState.locationOrder.push(location.id);
    }
    return structuredClone(location);
  }

  getEffectiveLocationById(locationId: string): LocationRecord | null {
    return mergeLocationRecords(
      this.baseState.locations[locationId] ?? null,
      this.stagedState.locations[locationId] ?? null,
    );
  }

  listEffectiveLocations(): LocationRecord[] {
    const orderedIds = new Set([...this.baseState.locationOrder, ...this.stagedState.locationOrder]);
    const orderedLocations = [...orderedIds]
      .map((id) => this.getEffectiveLocationById(id))
      .filter((location): location is LocationRecord => location !== null);
    const unorderedLocations = Object.values({ ...this.baseState.locations, ...this.stagedState.locations })
      .filter((location) => !orderedIds.has(location.id))
      .map((location) => this.getEffectiveLocationById(location.id))
      .filter((location): location is LocationRecord => location !== null)
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedLocations, ...unorderedLocations]);
  }

  stageShop(shop: ShopRecord): ShopRecord {
    this.stagedState.shop = structuredClone(shop);
    return structuredClone(shop);
  }

  getEffectiveShop(): ShopRecord | null {
    return mergeShopRecords(this.baseState.shop, this.stagedState.shop);
  }

  listEffectiveBusinessEntities(): BusinessEntityRecord[] {
    const orderedIds = new Set(this.baseState.businessEntityOrder);
    const orderedEntities = this.baseState.businessEntityOrder
      .map((id) => this.baseState.businessEntities[id] ?? null)
      .filter((entity): entity is BusinessEntityRecord => entity !== null);
    const unorderedEntities = Object.values(this.baseState.businessEntities)
      .filter((entity) => !orderedIds.has(entity.id))
      .sort((left, right) => Number(right.primary) - Number(left.primary) || left.id.localeCompare(right.id));

    return structuredClone([...orderedEntities, ...unorderedEntities]);
  }

  getBusinessEntityById(businessEntityId: string): BusinessEntityRecord | null {
    const businessEntity = this.baseState.businessEntities[businessEntityId] ?? null;
    return businessEntity ? structuredClone(businessEntity) : null;
  }

  getPrimaryBusinessEntity(): BusinessEntityRecord | null {
    const businessEntity = this.listEffectiveBusinessEntities().find((candidate) => candidate.primary) ?? null;
    return businessEntity ? structuredClone(businessEntity) : null;
  }

  upsertBaseMarkets(markets: Array<MarketRecord | { market: unknown; cursor?: string | null } | unknown>): void {
    for (const candidate of markets) {
      if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
        continue;
      }

      const entry = candidate as Record<string, unknown>;
      const rawMarket = 'market' in entry ? entry['market'] : candidate;
      const rawCursor = 'cursor' in entry ? entry['cursor'] : null;
      if (!rawMarket || typeof rawMarket !== 'object' || Array.isArray(rawMarket)) {
        continue;
      }

      const market = rawMarket as Record<string, unknown>;
      const id = market['id'];
      if (typeof id === 'string' && id.length > 0) {
        delete this.baseState.deletedMarketIds[id];
        delete this.stagedState.deletedMarketIds[id];
        const previous = this.baseState.markets[id];
        const cursor = typeof rawCursor === 'string' && rawCursor.length > 0 ? rawCursor : (previous?.cursor ?? null);
        this.baseState.markets[id] = {
          id,
          cursor,
          data: previous
            ? ({ ...structuredClone(previous.data), ...structuredClone(market) } as MarketRecord['data'])
            : (structuredClone(market) as MarketRecord['data']),
        };

        if (!this.baseState.marketOrder.includes(id)) {
          this.baseState.marketOrder.push(id);
        }
      }
    }
  }

  setBaseMarketsRootPayload(rootField: string, payload: unknown): void {
    this.baseMarketsRootPayloads[rootField] = structuredClone(payload);
  }

  getBaseMarketById(marketId: string): unknown | null {
    const market = this.baseState.markets[marketId];
    return market === undefined ? null : structuredClone(market.data);
  }

  listBaseMarkets(): MarketRecord[] {
    const orderedIds = new Set(this.baseState.marketOrder);
    const orderedMarkets = this.baseState.marketOrder
      .map((id) => this.baseState.markets[id] ?? null)
      .filter((market): market is MarketRecord => market !== null);
    const unorderedMarkets = Object.values(this.baseState.markets)
      .filter((market) => !orderedIds.has(market.id))
      .sort((left, right) => left.id.localeCompare(right.id));

    return structuredClone([...orderedMarkets, ...unorderedMarkets]);
  }

  getBaseMarketsRootPayload(rootField: string): unknown | null {
    const payload = this.baseMarketsRootPayloads[rootField];
    return payload === undefined ? null : structuredClone(payload);
  }

  stageCreateMarket(market: MarketRecord): MarketRecord {
    delete this.stagedState.deletedMarketIds[market.id];
    this.stagedState.markets[market.id] = structuredClone(market);
    if (!this.stagedState.marketOrder.includes(market.id)) {
      this.stagedState.marketOrder.push(market.id);
    }
    return structuredClone(market);
  }

  stageUpdateMarket(market: MarketRecord): MarketRecord {
    delete this.stagedState.deletedMarketIds[market.id];
    this.stagedState.markets[market.id] = structuredClone(market);
    if (!this.baseState.marketOrder.includes(market.id) && !this.stagedState.marketOrder.includes(market.id)) {
      this.stagedState.marketOrder.push(market.id);
    }
    return structuredClone(market);
  }

  stageDeleteMarket(marketId: string): void {
    delete this.stagedState.markets[marketId];
    this.stagedState.marketOrder = this.stagedState.marketOrder.filter((id) => id !== marketId);
    this.stagedState.deletedMarketIds[marketId] = true;
  }

  getEffectiveMarketRecordById(marketId: string): MarketRecord | null {
    if (this.stagedState.deletedMarketIds[marketId]) {
      return null;
    }

    const market = this.stagedState.markets[marketId] ?? this.baseState.markets[marketId] ?? null;
    return market ? structuredClone(market) : null;
  }

  getEffectiveMarketById(marketId: string): unknown | null {
    return this.getEffectiveMarketRecordById(marketId)?.data ?? null;
  }

  listEffectiveMarkets(): MarketRecord[] {
    const mergedMarkets = new Map<string, MarketRecord>();
    const orderedIds = [...this.baseState.marketOrder, ...this.stagedState.marketOrder];

    for (const id of orderedIds) {
      const market = this.getEffectiveMarketRecordById(id);
      if (market) {
        mergedMarkets.set(id, market);
      }
    }

    for (const market of [...Object.values(this.baseState.markets), ...Object.values(this.stagedState.markets)]) {
      if (mergedMarkets.has(market.id) || this.stagedState.deletedMarketIds[market.id]) {
        continue;
      }
      mergedMarkets.set(market.id, structuredClone(market));
    }

    return Array.from(mergedMarkets.values());
  }

  hasStagedMarkets(): boolean {
    return (
      Object.keys(this.stagedState.markets).length > 0 || Object.keys(this.stagedState.deletedMarketIds).length > 0
    );
  }

  setBaseSegmentsRootPayload(rootField: string, payload: unknown): void {
    this.baseSegmentsRootPayloads[rootField] = structuredClone(payload);
  }

  getBaseSegmentsRootPayload(rootField: string): unknown | null {
    const payload = this.baseSegmentsRootPayloads[rootField];
    return payload === undefined ? null : structuredClone(payload);
  }

  stageCreateCustomer(customer: CustomerRecord): CustomerRecord {
    delete this.stagedState.deletedCustomerIds[customer.id];
    delete this.stagedState.mergedCustomerIds[customer.id];
    this.stagedState.customers[customer.id] = structuredClone(customer);
    return structuredClone(customer);
  }

  stageUpdateCustomer(customer: CustomerRecord): CustomerRecord {
    delete this.stagedState.deletedCustomerIds[customer.id];
    delete this.stagedState.mergedCustomerIds[customer.id];
    this.stagedState.customers[customer.id] = structuredClone(customer);
    return structuredClone(customer);
  }

  stageDeleteCustomer(customerId: string): void {
    delete this.stagedState.customers[customerId];
    delete this.stagedState.mergedCustomerIds[customerId];
    this.stagedState.deletedCustomerIds[customerId] = true;
  }

  stageCreateDiscount(discount: DiscountRecord): DiscountRecord {
    delete this.stagedState.deletedDiscountIds[discount.id];
    this.stagedState.discounts[discount.id] = structuredClone(discount);
    return structuredClone(discount);
  }

  stageDeleteDiscount(discountId: string): void {
    delete this.stagedState.discounts[discountId];
    this.stagedState.deletedDiscountIds[discountId] = true;
  }

  stageMergeCustomers(
    sourceCustomerId: string,
    resultingCustomer: CustomerRecord,
    mergeRequest: CustomerMergeRequestRecord,
  ): CustomerRecord {
    this.stageDeleteCustomer(sourceCustomerId);
    this.stagedState.mergedCustomerIds[sourceCustomerId] = resultingCustomer.id;
    this.stagedState.customerMergeRequests[mergeRequest.jobId] = structuredClone(mergeRequest);
    return this.stageUpdateCustomer(resultingCustomer);
  }

  getCustomerMergeRequest(jobId: string): CustomerMergeRequestRecord | null {
    const request = this.stagedState.customerMergeRequests[jobId] ?? this.baseState.customerMergeRequests[jobId];
    return request ? structuredClone(request) : null;
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
      (left, right) => right.createdAt.localeCompare(left.createdAt) || compareShopifyResourceIds(right.id, left.id),
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
      .sort(
        (left, right) => right.createdAt.localeCompare(left.createdAt) || compareShopifyResourceIds(right.id, left.id),
      );
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
      delete this.stagedState.deletedFileIds[file.id];
      this.stagedState.files[file.id] = structuredClone(file);
    }

    return files.map((file) => structuredClone(file));
  }

  stageDeleteFiles(fileIds: string[]): void {
    const deletedFileIds = new Set(fileIds);

    for (const fileId of deletedFileIds) {
      delete this.stagedState.files[fileId];
      this.stagedState.deletedFileIds[fileId] = true;
    }

    const productIdsWithDeletedMedia = new Set(
      [...Object.values(this.baseState.productMedia), ...Object.values(this.stagedState.productMedia)]
        .filter((mediaRecord) => mediaRecord.id && deletedFileIds.has(mediaRecord.id))
        .map((mediaRecord) => mediaRecord.productId),
    );

    for (const productId of productIdsWithDeletedMedia) {
      const nextMedia = this.getEffectiveMediaByProductId(productId).filter(
        (mediaRecord) => !mediaRecord.id || !deletedFileIds.has(mediaRecord.id),
      );
      this.replaceStagedMediaForProduct(productId, nextMedia);
    }
  }

  hasEffectiveFileById(fileId: string): boolean {
    if (this.stagedState.deletedFileIds[fileId]) {
      return false;
    }

    if (this.stagedState.files[fileId] || this.baseState.files[fileId]) {
      return true;
    }

    return [...Object.values(this.baseState.productMedia), ...Object.values(this.stagedState.productMedia)].some(
      (mediaRecord) =>
        mediaRecord.id === fileId &&
        this.getEffectiveMediaByProductId(mediaRecord.productId).some((candidate) => candidate.id === fileId),
    );
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

  replaceBaseMetafieldsForCustomer(customerId: string, metafields: CustomerMetafieldRecord[]): void {
    for (const metafield of Object.values(this.baseState.customerMetafields)) {
      if (metafield.customerId === customerId) {
        delete this.baseState.customerMetafields[metafield.id];
      }
    }

    for (const metafield of metafields) {
      this.baseState.customerMetafields[metafield.id] = structuredClone(metafield);
    }
  }

  replaceStagedMetafieldsForCustomer(customerId: string, metafields: CustomerMetafieldRecord[]): void {
    for (const metafield of Object.values(this.stagedState.customerMetafields)) {
      if (metafield.customerId === customerId) {
        delete this.stagedState.customerMetafields[metafield.id];
      }
    }

    for (const metafield of metafields) {
      this.stagedState.customerMetafields[metafield.id] = structuredClone(metafield);
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

  listEffectiveDiscounts(): DiscountRecord[] {
    const discountIds = new Set([...Object.keys(this.baseState.discounts), ...Object.keys(this.stagedState.discounts)]);
    const merged: DiscountRecord[] = [];

    for (const discountId of Array.from(discountIds)) {
      if (this.stagedState.deletedDiscountIds[discountId]) {
        continue;
      }

      const discount = this.stagedState.discounts[discountId] ?? this.baseState.discounts[discountId];
      if (discount) {
        merged.push(structuredClone(discount));
      }
    }

    return merged;
  }

  hasBaseCustomers(): boolean {
    return Object.keys(this.baseState.customers).length > 0;
  }

  hasStagedCustomers(): boolean {
    return (
      Object.keys(this.stagedState.customers).length > 0 ||
      Object.keys(this.stagedState.customerMetafields).length > 0 ||
      Object.keys(this.stagedState.deletedCustomerIds).length > 0 ||
      Object.keys(this.stagedState.mergedCustomerIds).length > 0 ||
      Object.keys(this.stagedState.customerMergeRequests).length > 0
    );
  }

  hasDiscounts(): boolean {
    return (
      Object.keys(this.baseState.discounts).length > 0 ||
      Object.keys(this.stagedState.discounts).length > 0 ||
      Object.keys(this.stagedState.deletedDiscountIds).length > 0
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

    return sourceOptions.sort(
      (left, right) => left.position - right.position || compareShopifyResourceIds(left.id, right.id),
    );
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
        return collectionFromMembership(membership);
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
          collectionsById.set(collection.id, collectionFromMembership(collection));
        }
      }
    }

    return Array.from(collectionsById.values()).sort(
      (left, right) => left.title.localeCompare(right.title) || compareShopifyResourceIds(left.id, right.id),
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

    for (const collection of this.listEffectiveCollections()) {
      for (const publicationId of collection.publicationIds ?? []) {
        if (!publicationsById.has(publicationId)) {
          publicationsById.set(publicationId, {
            id: publicationId,
            name: null,
          });
        }
      }
    }

    return Array.from(publicationsById.values()).sort((left, right) => compareShopifyResourceIds(left.id, right.id));
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
          ...standalone,
          productId: collection.productId,
          position: collection.position,
        };
      });

    return visibleCollections.sort(
      (left, right) => left.title.localeCompare(right.title) || compareShopifyResourceIds(left.id, right.id),
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

  getEffectiveMetafieldsByCustomerId(customerId: string): CustomerMetafieldRecord[] {
    if (this.stagedState.deletedCustomerIds[customerId]) {
      return [];
    }

    const stagedMetafields = Object.values(this.stagedState.customerMetafields)
      .filter((metafield) => metafield.customerId === customerId)
      .map((metafield) => structuredClone(metafield));

    const sourceMetafields =
      stagedMetafields.length > 0
        ? stagedMetafields
        : Object.values(this.baseState.customerMetafields)
            .filter((metafield) => metafield.customerId === customerId)
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
