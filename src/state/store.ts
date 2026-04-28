import { defaultPaymentTermsTemplateOrder, defaultPaymentTermsTemplateRecordMap } from './types.js';
import type {
  AbandonedCheckoutRecord,
  AbandonmentDeliveryActivityRecord,
  AbandonmentRecord,
  AdminPlatformFlowSignatureRecord,
  AdminPlatformFlowTriggerRecord,
  AppInstallationRecord,
  AppOneTimePurchaseRecord,
  AppRecord,
  AppSubscriptionLineItemRecord,
  AppSubscriptionRecord,
  AppUsageRecord,
  B2BCompanyContactRecord,
  B2BCompanyContactRoleRecord,
  B2BCompanyLocationRecord,
  B2BCompanyRecord,
  BackupRegionRecord,
  BulkOperationRecord,
  BusinessEntityRecord,
  CalculatedOrderRecord,
  CarrierServiceRecord,
  CartTransformRecord,
  ChannelRecord,
  CombinedListingChildRecord,
  CatalogRecord,
  CollectionRecord,
  CustomerAddressRecord,
  CustomerAccountPageRecord,
  CustomerCatalogConnectionRecord,
  CustomerDataErasureRequestRecord,
  CustomerMergeRequestRecord,
  CustomerMetafieldRecord,
  CustomerPaymentMethodRecord,
  CustomerPaymentMethodUpdateUrlRecord,
  StoreCreditAccountRecord,
  StoreCreditAccountTransactionRecord,
  CustomerRecord,
  CustomerSegmentMembersQueryRecord,
  DeliveryProfileRecord,
  DelegatedAccessTokenRecord,
  DiscountRecord,
  DiscountBulkOperationRecord,
  DraftOrderRecord,
  FileRecord,
  FulfillmentServiceRecord,
  InventoryShipmentRecord,
  GiftCardConfigurationRecord,
  GiftCardRecord,
  InventoryTransferRecord,
  LocationRecord,
  LocaleRecord,
  MarketLocalizationRecord,
  MarketRecord,
  MarketingEngagementRecord,
  MarketingRecord,
  MetaobjectDefinitionRecord,
  MetaobjectRecord,
  MetafieldDefinitionRecord,
  MutationLogEntry,
  NormalizedStateSnapshotFile,
  OnlineStoreContentKind,
  OnlineStoreContentRecord,
  OrderMandatePaymentRecord,
  OrderRecord,
  PaymentCustomizationRecord,
  PaymentReminderSendRecord,
  PaymentTermsTemplateRecord,
  ProductCatalogConnectionRecord,
  ProductBundleComponentRecord,
  ProductCollectionRecord,
  ProductFeedRecord,
  ProductMediaRecord,
  ProductMetafieldRecord,
  ProductOptionRecord,
  ProductOperationRecord,
  ProductResourceFeedbackRecord,
  ProductRecord,
  ProductVariantComponentRecord,
  ProductVariantRecord,
  PriceListRecord,
  PublicationRecord,
  SavedSearchRecord,
  SegmentRecord,
  ShippingPackageRecord,
  SellingPlanGroupRecord,
  ShopRecord,
  ShopResourceFeedbackRecord,
  ShopifyFunctionRecord,
  ShopLocaleRecord,
  StateSnapshot,
  TaxAppConfigurationRecord,
  TranslationRecord,
  ValidationRecord,
  WebhookSubscriptionRecord,
  WebPresenceRecord,
} from './types.js';
import { compareShopifyResourceIds } from '../shopify/resource-ids.js';

type MetaStateSnapshot = StateSnapshot & {
  orders: Record<string, OrderRecord>;
  draftOrders: Record<string, DraftOrderRecord>;
  calculatedOrders: Record<string, CalculatedOrderRecord>;
  orderMandatePayments: Record<string, OrderMandatePaymentRecord>;
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
  productOperations: {},
  productFeeds: {},
  productResourceFeedback: {},
  shopResourceFeedback: {},
  productBundleComponents: {},
  productVariantComponents: {},
  combinedListingChildren: {},
  inventoryTransfers: {},
  inventoryTransferOrder: [],
  locations: {},
  locationOrder: [],
  fulfillmentServices: {},
  fulfillmentServiceOrder: [],
  carrierServices: {},
  carrierServiceOrder: [],
  inventoryShipments: {},
  inventoryShipmentOrder: [],
  shippingPackages: {},
  shippingPackageOrder: [],
  giftCards: {},
  giftCardOrder: [],
  giftCardConfiguration: null,
  collections: {},
  publications: {},
  channels: {},
  customers: {},
  customerAddresses: {},
  customerPaymentMethods: {},
  customerPaymentMethodUpdateUrls: {},
  customerAccountPages: {},
  customerAccountPageOrder: [],
  customerDataErasureRequests: {},
  paymentReminderSends: {},
  storeCreditAccounts: {},
  storeCreditAccountTransactions: {},
  segments: {},
  customerSegmentMembersQueries: {},
  webhookSubscriptions: {},
  webhookSubscriptionOrder: [],
  marketingActivities: {},
  marketingActivityOrder: [],
  marketingEvents: {},
  marketingEventOrder: [],
  marketingEngagements: {},
  marketingEngagementOrder: [],
  deletedMarketingActivityIds: {},
  deletedMarketingEventIds: {},
  deletedMarketingEngagementIds: {},
  onlineStoreArticles: {},
  onlineStoreArticleOrder: [],
  onlineStoreBlogs: {},
  onlineStoreBlogOrder: [],
  onlineStorePages: {},
  onlineStorePageOrder: [],
  onlineStoreComments: {},
  onlineStoreCommentOrder: [],
  savedSearches: {},
  savedSearchOrder: [],
  bulkOperations: {},
  bulkOperationOrder: [],
  bulkOperationResults: {},
  discounts: {},
  discountBulkOperations: {},
  paymentCustomizations: {},
  paymentCustomizationOrder: [],
  paymentTermsTemplates: structuredClone(defaultPaymentTermsTemplateRecordMap),
  paymentTermsTemplateOrder: [...defaultPaymentTermsTemplateOrder],
  shopifyFunctions: {},
  shopifyFunctionOrder: [],
  validations: {},
  validationOrder: [],
  cartTransforms: {},
  cartTransformOrder: [],
  taxAppConfiguration: null,
  businessEntities: {},
  businessEntityOrder: [],
  b2bCompanies: {},
  b2bCompanyOrder: [],
  b2bCompanyContacts: {},
  b2bCompanyContactOrder: [],
  b2bCompanyContactRoles: {},
  b2bCompanyContactRoleOrder: [],
  b2bCompanyLocations: {},
  b2bCompanyLocationOrder: [],
  markets: {},
  marketOrder: [],
  webPresences: {},
  webPresenceOrder: [],
  marketLocalizations: {},
  availableLocales: [],
  shopLocales: {},
  translations: {},
  catalogs: {},
  catalogOrder: [],
  priceLists: {},
  priceListOrder: [],
  deliveryProfiles: {},
  deliveryProfileOrder: [],
  apps: {},
  appInstallations: {},
  currentAppInstallationId: null,
  appSubscriptions: {},
  appSubscriptionLineItems: {},
  appOneTimePurchases: {},
  appUsageRecords: {},
  delegatedAccessTokens: {},
  sellingPlanGroups: {},
  sellingPlanGroupOrder: [],
  abandonedCheckouts: {},
  abandonedCheckoutOrder: [],
  abandonments: {},
  abandonmentOrder: [],
  backupRegion: null,
  adminPlatformFlowSignatures: {},
  adminPlatformFlowSignatureOrder: [],
  adminPlatformFlowTriggers: {},
  adminPlatformFlowTriggerOrder: [],
  productCollections: {},
  productMedia: {},
  files: {},
  productMetafields: {},
  metafieldDefinitions: {},
  metaobjectDefinitions: {},
  metaobjects: {},
  customerMetafields: {},
  deletedProductIds: {},
  deletedProductFeedIds: {},
  deletedInventoryTransferIds: {},
  deletedFileIds: {},
  deletedCollectionIds: {},
  deletedPublicationIds: {},
  deletedLocationIds: {},
  deletedFulfillmentServiceIds: {},
  deletedCarrierServiceIds: {},
  deletedInventoryShipmentIds: {},
  deletedShippingPackageIds: {},
  deletedGiftCardIds: {},
  deletedCustomerIds: {},
  deletedCustomerAddressIds: {},
  deletedCustomerPaymentMethodIds: {},
  deletedSegmentIds: {},
  deletedWebhookSubscriptionIds: {},
  deletedOnlineStoreArticleIds: {},
  deletedOnlineStoreBlogIds: {},
  deletedOnlineStorePageIds: {},
  deletedOnlineStoreCommentIds: {},
  deletedSavedSearchIds: {},
  deletedDiscountIds: {},
  deletedPaymentCustomizationIds: {},
  deletedValidationIds: {},
  deletedCartTransformIds: {},
  deletedB2BCompanyIds: {},
  deletedB2BCompanyContactIds: {},
  deletedB2BCompanyContactRoleIds: {},
  deletedB2BCompanyLocationIds: {},
  deletedMarketIds: {},
  deletedCatalogIds: {},
  deletedPriceListIds: {},
  deletedWebPresenceIds: {},
  deletedShopLocales: {},
  deletedTranslations: {},
  deletedDeliveryProfileIds: {},
  deletedSellingPlanGroupIds: {},
  deletedMetafieldDefinitionIds: {},
  deletedMetaobjectDefinitionIds: {},
  deletedMetaobjectIds: {},
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
    orderMandatePayments?: Record<string, OrderMandatePaymentRecord>;
  } = {},
): MetaStateSnapshot {
  return {
    ...cloneSnapshot(snapshot),
    orders: structuredClone(extraState.orders ?? {}),
    draftOrders: structuredClone(extraState.draftOrders ?? {}),
    calculatedOrders: structuredClone(extraState.calculatedOrders ?? {}),
    orderMandatePayments: structuredClone(extraState.orderMandatePayments ?? {}),
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

function marketLocalizationStorageKey(
  localization: Pick<MarketLocalizationRecord, 'resourceId' | 'marketId' | 'key'>,
): string {
  return `${localization.resourceId}::${localization.marketId}::${localization.key}`;
}

function translationStorageKey(
  translation: Pick<TranslationRecord, 'resourceId' | 'locale' | 'key' | 'marketId'>,
): string {
  return `${translation.resourceId}::${translation.locale}::${translation.marketId ?? ''}::${translation.key}`;
}

function readCollectionPosition(collection: ProductCollectionRecord): number | null {
  return typeof collection.position === 'number' && Number.isFinite(collection.position) ? collection.position : null;
}

function readMarketingNestedObject(source: Record<string, unknown>, field: string): Record<string, unknown> | null {
  const value = source[field];
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readMarketingEventId(source: Record<string, unknown>): string | null {
  const event = readMarketingNestedObject(source, 'marketingEvent');
  const id = event?.['id'];
  return typeof id === 'string' && id.length > 0 ? id : null;
}

function readMarketingRemoteId(source: Record<string, unknown>): string | null {
  const remoteId = source['remoteId'];
  if (typeof remoteId === 'string' && remoteId.length > 0) {
    return remoteId;
  }

  const event = readMarketingNestedObject(source, 'marketingEvent');
  const eventRemoteId = event?.['remoteId'];
  return typeof eventRemoteId === 'string' && eventRemoteId.length > 0 ? eventRemoteId : null;
}

function readMarketingChannelHandle(source: Record<string, unknown>): string | null {
  const channelHandle = source['channelHandle'];
  if (typeof channelHandle === 'string' && channelHandle.length > 0) {
    return channelHandle;
  }

  const event = readMarketingNestedObject(source, 'marketingEvent');
  const eventChannelHandle = event?.['channelHandle'];
  return typeof eventChannelHandle === 'string' && eventChannelHandle.length > 0 ? eventChannelHandle : null;
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

  if (staged?.deleted === true) {
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

function mergeFulfillmentServiceRecords(
  base: FulfillmentServiceRecord | null,
  staged: FulfillmentServiceRecord | null,
): FulfillmentServiceRecord | null {
  if (!base && !staged) {
    return null;
  }

  return structuredClone(staged ?? base);
}

function mergeGiftCardRecords(base: GiftCardRecord | null, staged: GiftCardRecord | null): GiftCardRecord | null {
  if (!base && !staged) {
    return null;
  }

  return structuredClone(staged ?? base);
}

function mergeCarrierServiceRecords(
  base: CarrierServiceRecord | null,
  staged: CarrierServiceRecord | null,
): CarrierServiceRecord | null {
  if (!base && !staged) {
    return null;
  }

  return structuredClone(staged ?? base);
}

function mergeShippingPackageRecords(
  base: ShippingPackageRecord | null,
  staged: ShippingPackageRecord | null,
): ShippingPackageRecord | null {
  if (!base && !staged) {
    return null;
  }

  return structuredClone(staged ?? base);
}

function collectionFromMembership(membership: ProductCollectionRecord): CollectionRecord {
  const { productId: _productId, position: _position, ...collection } = membership;
  return structuredClone(collection);
}

function isInternalPublicationPlaceholder(publicationId: string): boolean {
  return publicationId === '__current_publication__';
}

function readProductMetafieldOwnerId(metafield: ProductMetafieldRecord): string | null {
  return metafield.ownerId ?? metafield.productId ?? null;
}

function mergePublicationRecord(
  base: PublicationRecord | null,
  staged: PublicationRecord | null,
): PublicationRecord | null {
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
  });
}

function publicationChannelId(publication: PublicationRecord): string | null {
  if (publication.channelId) {
    return publication.channelId;
  }

  const legacyResourceId = publication.id.split('/').at(-1);
  return legacyResourceId ? `gid://shopify/Channel/${legacyResourceId}` : null;
}

function channelFromPublication(publication: PublicationRecord): ChannelRecord | null {
  const id = publicationChannelId(publication);
  if (!id) {
    return null;
  }

  return {
    id,
    name: publication.name,
    publicationId: publication.id,
  };
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
    combinedListingRole: staged.combinedListingRole ?? base.combinedListingRole ?? null,
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
    email: staged.email,
    legacyResourceId: staged.legacyResourceId ?? base.legacyResourceId,
    locale: staged.locale,
    note: staged.note,
    canDelete: staged.canDelete,
    verifiedEmail: staged.verifiedEmail,
    dataSaleOptOut: staged.dataSaleOptOut ?? base.dataSaleOptOut ?? false,
    taxExempt: staged.taxExempt,
    taxExemptions: structuredClone(staged.taxExemptions ?? base.taxExemptions ?? []),
    state: staged.state,
    tags: structuredClone(staged.tags),
    numberOfOrders: staged.numberOfOrders,
    amountSpent: staged.amountSpent,
    defaultEmailAddress: staged.defaultEmailAddress,
    defaultPhoneNumber: staged.defaultPhoneNumber,
    emailMarketingConsent: staged.emailMarketingConsent,
    smsMarketingConsent: staged.smsMarketingConsent,
    defaultAddress: staged.defaultAddress,
    createdAt: staged.createdAt ?? base.createdAt,
    updatedAt:
      base.updatedAt && staged.updatedAt
        ? ensureUpdatedAtAfterBase(base.updatedAt, staged.updatedAt)
        : (staged.updatedAt ?? base.updatedAt),
  };
}

function compareCustomerAddresses(left: CustomerAddressRecord, right: CustomerAddressRecord): number {
  return left.position - right.position || left.id.localeCompare(right.id);
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
  private deletedOrderIds = new Set<string>();
  private calculatedOrders: Record<string, CalculatedOrderRecord> = {};
  private stagedDraftOrders: Record<string, DraftOrderRecord> = {};
  private deletedDraftOrderIds = new Set<string>();
  private orderMandatePayments: Record<string, OrderMandatePaymentRecord> = {};
  private stagedUploadContents = new Map<string, string>();

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
    this.deletedOrderIds = new Set<string>();
    this.calculatedOrders = {};
    this.stagedDraftOrders = structuredClone(this.initialDraftOrders);
    this.deletedDraftOrderIds = new Set<string>();
    this.orderMandatePayments = {};
    this.stagedUploadContents = new Map<string, string>();
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
        orderMandatePayments: this.orderMandatePayments,
      }),
    };
  }

  appendLog(entry: MutationLogEntry): void {
    this.mutationLog.push(structuredClone(entry));
  }

  getLog(): MutationLogEntry[] {
    return structuredClone(this.mutationLog);
  }

  getEffectiveBackupRegion(): BackupRegionRecord | null {
    return structuredClone(this.stagedState.backupRegion ?? this.baseState.backupRegion ?? null);
  }

  stageBackupRegion(region: BackupRegionRecord): BackupRegionRecord {
    this.stagedState.backupRegion = structuredClone(region);
    return structuredClone(region);
  }

  stageAdminPlatformFlowSignature(signature: AdminPlatformFlowSignatureRecord): AdminPlatformFlowSignatureRecord {
    this.stagedState.adminPlatformFlowSignatures[signature.id] = structuredClone(signature);
    if (!this.stagedState.adminPlatformFlowSignatureOrder.includes(signature.id)) {
      this.stagedState.adminPlatformFlowSignatureOrder.push(signature.id);
    }
    return structuredClone(signature);
  }

  stageAdminPlatformFlowTrigger(trigger: AdminPlatformFlowTriggerRecord): AdminPlatformFlowTriggerRecord {
    this.stagedState.adminPlatformFlowTriggers[trigger.id] = structuredClone(trigger);
    if (!this.stagedState.adminPlatformFlowTriggerOrder.includes(trigger.id)) {
      this.stagedState.adminPlatformFlowTriggerOrder.push(trigger.id);
    }
    return structuredClone(trigger);
  }

  stageUploadContent(keys: string[], content: string): void {
    for (const key of keys) {
      this.stagedUploadContents.set(key, content);
    }
  }

  getStagedUploadContent(key: string): string | null {
    return this.stagedUploadContents.get(key) ?? null;
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

  upsertBaseCustomerAddresses(addresses: CustomerAddressRecord[]): void {
    for (const address of addresses) {
      delete this.baseState.deletedCustomerAddressIds[address.id];
      delete this.stagedState.deletedCustomerAddressIds[address.id];
      this.baseState.customerAddresses[address.id] = structuredClone(address);
    }
  }

  upsertBaseCustomerPaymentMethods(paymentMethods: CustomerPaymentMethodRecord[]): void {
    for (const paymentMethod of paymentMethods) {
      delete this.baseState.deletedCustomerPaymentMethodIds[paymentMethod.id];
      delete this.stagedState.deletedCustomerPaymentMethodIds[paymentMethod.id];
      this.baseState.customerPaymentMethods[paymentMethod.id] = structuredClone(paymentMethod);
    }
  }

  upsertBaseStoreCreditAccounts(
    accounts: StoreCreditAccountRecord[],
    transactions: StoreCreditAccountTransactionRecord[] = [],
  ): void {
    for (const account of accounts) {
      this.baseState.storeCreditAccounts[account.id] = structuredClone(account);
    }
    for (const transaction of transactions) {
      this.baseState.storeCreditAccountTransactions[transaction.id] = structuredClone(transaction);
    }
  }

  upsertBaseSegments(segments: SegmentRecord[]): void {
    for (const segment of segments) {
      delete this.baseState.deletedSegmentIds[segment.id];
      delete this.stagedState.deletedSegmentIds[segment.id];
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

  upsertBaseWebhookSubscriptions(webhookSubscriptions: WebhookSubscriptionRecord[]): void {
    for (const webhookSubscription of webhookSubscriptions) {
      delete this.baseState.deletedWebhookSubscriptionIds[webhookSubscription.id];
      delete this.stagedState.deletedWebhookSubscriptionIds[webhookSubscription.id];
      this.baseState.webhookSubscriptions[webhookSubscription.id] = structuredClone(webhookSubscription);
      if (!this.baseState.webhookSubscriptionOrder.includes(webhookSubscription.id)) {
        this.baseState.webhookSubscriptionOrder.push(webhookSubscription.id);
      }
    }
  }

  upsertStagedWebhookSubscription(webhookSubscription: WebhookSubscriptionRecord): void {
    delete this.stagedState.deletedWebhookSubscriptionIds[webhookSubscription.id];
    this.stagedState.webhookSubscriptions[webhookSubscription.id] = structuredClone(webhookSubscription);
    if (
      !this.baseState.webhookSubscriptionOrder.includes(webhookSubscription.id) &&
      !this.stagedState.webhookSubscriptionOrder.includes(webhookSubscription.id)
    ) {
      this.stagedState.webhookSubscriptionOrder.push(webhookSubscription.id);
    }
  }

  deleteStagedWebhookSubscription(webhookSubscriptionId: string): void {
    delete this.stagedState.webhookSubscriptions[webhookSubscriptionId];
    this.stagedState.deletedWebhookSubscriptionIds[webhookSubscriptionId] = true;
  }

  getEffectiveWebhookSubscriptionById(webhookSubscriptionId: string): WebhookSubscriptionRecord | null {
    if (this.stagedState.deletedWebhookSubscriptionIds[webhookSubscriptionId]) {
      return null;
    }

    const webhookSubscription =
      this.stagedState.webhookSubscriptions[webhookSubscriptionId] ??
      this.baseState.webhookSubscriptions[webhookSubscriptionId] ??
      null;
    return webhookSubscription ? structuredClone(webhookSubscription) : null;
  }

  listEffectiveWebhookSubscriptions(): WebhookSubscriptionRecord[] {
    const orderedIds = new Set([
      ...this.baseState.webhookSubscriptionOrder,
      ...this.stagedState.webhookSubscriptionOrder,
    ]);
    const orderedWebhookSubscriptions = [...orderedIds]
      .map((webhookSubscriptionId) => this.getEffectiveWebhookSubscriptionById(webhookSubscriptionId))
      .filter((webhookSubscription): webhookSubscription is WebhookSubscriptionRecord => webhookSubscription !== null);
    const unorderedWebhookSubscriptions = Object.values({
      ...this.baseState.webhookSubscriptions,
      ...this.stagedState.webhookSubscriptions,
    })
      .filter((webhookSubscription) => !orderedIds.has(webhookSubscription.id))
      .filter((webhookSubscription) => !this.stagedState.deletedWebhookSubscriptionIds[webhookSubscription.id])
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id))
      .map((webhookSubscription) => structuredClone(webhookSubscription));

    return [...orderedWebhookSubscriptions, ...unorderedWebhookSubscriptions];
  }

  hasWebhookSubscriptions(): boolean {
    return (
      Object.keys(this.baseState.webhookSubscriptions).length > 0 ||
      Object.keys(this.stagedState.webhookSubscriptions).length > 0 ||
      Object.keys(this.stagedState.deletedWebhookSubscriptionIds).length > 0
    );
  }

  hasStagedWebhookSubscriptions(): boolean {
    return (
      Object.keys(this.stagedState.webhookSubscriptions).length > 0 ||
      Object.keys(this.stagedState.deletedWebhookSubscriptionIds).length > 0
    );
  }

  upsertBaseSellingPlanGroups(groups: SellingPlanGroupRecord[]): void {
    for (const group of groups) {
      delete this.baseState.deletedSellingPlanGroupIds[group.id];
      delete this.stagedState.deletedSellingPlanGroupIds[group.id];
      this.baseState.sellingPlanGroups[group.id] = structuredClone(group);
      if (!this.baseState.sellingPlanGroupOrder.includes(group.id)) {
        this.baseState.sellingPlanGroupOrder.push(group.id);
      }
    }
  }

  upsertStagedSellingPlanGroup(group: SellingPlanGroupRecord): SellingPlanGroupRecord {
    delete this.stagedState.deletedSellingPlanGroupIds[group.id];
    this.stagedState.sellingPlanGroups[group.id] = structuredClone(group);
    if (
      !this.baseState.sellingPlanGroupOrder.includes(group.id) &&
      !this.stagedState.sellingPlanGroupOrder.includes(group.id)
    ) {
      this.stagedState.sellingPlanGroupOrder.push(group.id);
    }
    return structuredClone(group);
  }

  deleteStagedSellingPlanGroup(groupId: string): void {
    delete this.stagedState.sellingPlanGroups[groupId];
    this.stagedState.deletedSellingPlanGroupIds[groupId] = true;
  }

  getEffectiveSellingPlanGroupById(groupId: string): SellingPlanGroupRecord | null {
    if (this.stagedState.deletedSellingPlanGroupIds[groupId]) {
      return null;
    }

    const group = this.stagedState.sellingPlanGroups[groupId] ?? this.baseState.sellingPlanGroups[groupId] ?? null;
    return group ? structuredClone(group) : null;
  }

  listEffectiveSellingPlanGroups(): SellingPlanGroupRecord[] {
    const orderedIds = new Set([...this.baseState.sellingPlanGroupOrder, ...this.stagedState.sellingPlanGroupOrder]);
    const orderedGroups = [...orderedIds]
      .map((groupId) => this.getEffectiveSellingPlanGroupById(groupId))
      .filter((group): group is SellingPlanGroupRecord => group !== null);
    const unorderedGroups = Object.values({
      ...this.baseState.sellingPlanGroups,
      ...this.stagedState.sellingPlanGroups,
    })
      .filter((group) => !orderedIds.has(group.id))
      .filter((group) => !this.stagedState.deletedSellingPlanGroupIds[group.id])
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id))
      .map((group) => structuredClone(group));

    return [...orderedGroups, ...unorderedGroups];
  }

  listEffectiveSellingPlanGroupsForProduct(productId: string): SellingPlanGroupRecord[] {
    return this.listEffectiveSellingPlanGroups().filter((group) => group.productIds.includes(productId));
  }

  listEffectiveSellingPlanGroupsVisibleForProduct(productId: string): SellingPlanGroupRecord[] {
    const variantIds = new Set(this.getEffectiveVariantsByProductId(productId).map((variant) => variant.id));
    return this.listEffectiveSellingPlanGroups().filter(
      (group) =>
        group.productIds.includes(productId) || group.productVariantIds.some((variantId) => variantIds.has(variantId)),
    );
  }

  listEffectiveSellingPlanGroupsForProductVariant(variantId: string): SellingPlanGroupRecord[] {
    return this.listEffectiveSellingPlanGroups().filter((group) => group.productVariantIds.includes(variantId));
  }

  listEffectiveSellingPlanGroupsVisibleForProductVariant(variantId: string): SellingPlanGroupRecord[] {
    const variant = this.getEffectiveVariantById(variantId);
    return this.listEffectiveSellingPlanGroups().filter(
      (group) =>
        group.productVariantIds.includes(variantId) || (variant ? group.productIds.includes(variant.productId) : false),
    );
  }

  hasSellingPlanGroups(): boolean {
    return (
      Object.keys(this.baseState.sellingPlanGroups).length > 0 ||
      Object.keys(this.stagedState.sellingPlanGroups).length > 0 ||
      Object.keys(this.stagedState.deletedSellingPlanGroupIds).length > 0
    );
  }

  hasStagedSellingPlanGroups(): boolean {
    return (
      Object.keys(this.stagedState.sellingPlanGroups).length > 0 ||
      Object.keys(this.stagedState.deletedSellingPlanGroupIds).length > 0
    );
  }

  private upsertBaseMarketingRecords(
    bucket: Record<string, MarketingRecord>,
    order: string[],
    records: Array<MarketingRecord | { data: unknown; cursor?: string | null } | unknown>,
  ): void {
    for (const candidate of records) {
      if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
        continue;
      }

      const entry = candidate as Record<string, unknown>;
      const rawData = 'data' in entry ? entry['data'] : candidate;
      if (!rawData || typeof rawData !== 'object' || Array.isArray(rawData)) {
        continue;
      }

      const data = rawData as Record<string, unknown>;
      const id = data['id'];
      if (typeof id !== 'string' || id.length === 0) {
        continue;
      }

      const previous = bucket[id];
      const rawCursor = 'cursor' in entry ? entry['cursor'] : null;
      const cursor = typeof rawCursor === 'string' && rawCursor.length > 0 ? rawCursor : (previous?.cursor ?? null);
      bucket[id] = {
        id,
        cursor,
        data: previous
          ? ({ ...structuredClone(previous.data), ...structuredClone(data) } as MarketingRecord['data'])
          : (structuredClone(data) as MarketingRecord['data']),
      };

      if (!order.includes(id)) {
        order.push(id);
      }
    }
  }

  upsertBaseMarketingActivities(
    records: Array<MarketingRecord | { data: unknown; cursor?: string | null } | unknown>,
  ): void {
    this.upsertBaseMarketingRecords(this.baseState.marketingActivities, this.baseState.marketingActivityOrder, records);
  }

  upsertBaseMarketingEvents(
    records: Array<MarketingRecord | { data: unknown; cursor?: string | null } | unknown>,
  ): void {
    this.upsertBaseMarketingRecords(this.baseState.marketingEvents, this.baseState.marketingEventOrder, records);
  }

  private stageMarketingRecord(
    bucket: Record<string, MarketingRecord>,
    order: string[],
    deletedBucket: Record<string, boolean>,
    record: MarketingRecord,
  ): MarketingRecord {
    delete deletedBucket[record.id];
    bucket[record.id] = structuredClone(record);
    if (!order.includes(record.id)) {
      order.push(record.id);
    }
    return structuredClone(record);
  }

  stageMarketingActivity(record: MarketingRecord): MarketingRecord {
    return this.stageMarketingRecord(
      this.stagedState.marketingActivities,
      this.stagedState.marketingActivityOrder,
      this.stagedState.deletedMarketingActivityIds,
      record,
    );
  }

  stageMarketingEvent(record: MarketingRecord): MarketingRecord {
    return this.stageMarketingRecord(
      this.stagedState.marketingEvents,
      this.stagedState.marketingEventOrder,
      this.stagedState.deletedMarketingEventIds,
      record,
    );
  }

  stageDeleteMarketingActivity(activityId: string): void {
    const activity = this.getEffectiveMarketingActivityRecordById(activityId);
    const eventId = activity ? readMarketingEventId(activity.data) : null;

    delete this.stagedState.marketingActivities[activityId];
    this.stagedState.deletedMarketingActivityIds[activityId] = true;

    if (eventId) {
      delete this.stagedState.marketingEvents[eventId];
      this.stagedState.deletedMarketingEventIds[eventId] = true;
    }
  }

  stageDeleteAllExternalMarketingActivities(): string[] {
    const deletedIds: string[] = [];
    for (const activity of this.listEffectiveMarketingActivities()) {
      if (activity.data['isExternal'] === true) {
        this.stageDeleteMarketingActivity(activity.id);
        deletedIds.push(activity.id);
      }
    }
    return deletedIds;
  }

  getBaseMarketingActivityById(activityId: string): unknown | null {
    const activity = this.baseState.marketingActivities[activityId];
    return activity === undefined ? null : structuredClone(activity.data);
  }

  getBaseMarketingEventById(eventId: string): unknown | null {
    const event = this.baseState.marketingEvents[eventId];
    return event === undefined ? null : structuredClone(event.data);
  }

  getEffectiveMarketingActivityById(activityId: string): unknown | null {
    const activity = this.getEffectiveMarketingActivityRecordById(activityId);
    return activity ? structuredClone(activity.data) : null;
  }

  getEffectiveMarketingEventById(eventId: string): unknown | null {
    const event = this.getEffectiveMarketingEventRecordById(eventId);
    return event ? structuredClone(event.data) : null;
  }

  getEffectiveMarketingActivityRecordById(activityId: string): MarketingRecord | null {
    if (this.stagedState.deletedMarketingActivityIds[activityId]) {
      return null;
    }

    const activity = this.stagedState.marketingActivities[activityId] ?? this.baseState.marketingActivities[activityId];
    return activity ? structuredClone(activity) : null;
  }

  getEffectiveMarketingEventRecordById(eventId: string): MarketingRecord | null {
    if (this.stagedState.deletedMarketingEventIds[eventId]) {
      return null;
    }

    const event = this.stagedState.marketingEvents[eventId] ?? this.baseState.marketingEvents[eventId];
    return event ? structuredClone(event) : null;
  }

  getEffectiveMarketingActivityByRemoteId(remoteId: string): MarketingRecord | null {
    return (
      this.listEffectiveMarketingActivities().find((activity) => {
        return readMarketingRemoteId(activity.data) === remoteId;
      }) ?? null
    );
  }

  listBaseMarketingActivities(): MarketingRecord[] {
    return this.listBaseMarketingRecords(this.baseState.marketingActivities, this.baseState.marketingActivityOrder);
  }

  listBaseMarketingEvents(): MarketingRecord[] {
    return this.listBaseMarketingRecords(this.baseState.marketingEvents, this.baseState.marketingEventOrder);
  }

  listEffectiveMarketingActivities(): MarketingRecord[] {
    return this.listEffectiveMarketingRecords(
      this.baseState.marketingActivities,
      this.baseState.marketingActivityOrder,
      this.stagedState.marketingActivities,
      this.stagedState.marketingActivityOrder,
      this.stagedState.deletedMarketingActivityIds,
    );
  }

  listEffectiveMarketingEvents(): MarketingRecord[] {
    return this.listEffectiveMarketingRecords(
      this.baseState.marketingEvents,
      this.baseState.marketingEventOrder,
      this.stagedState.marketingEvents,
      this.stagedState.marketingEventOrder,
      this.stagedState.deletedMarketingEventIds,
    );
  }

  hasStagedMarketingRecords(): boolean {
    return (
      Object.keys(this.stagedState.marketingActivities).length > 0 ||
      Object.keys(this.stagedState.marketingEvents).length > 0 ||
      Object.keys(this.stagedState.marketingEngagements).length > 0 ||
      Object.keys(this.stagedState.deletedMarketingActivityIds).length > 0 ||
      Object.keys(this.stagedState.deletedMarketingEventIds).length > 0 ||
      Object.keys(this.stagedState.deletedMarketingEngagementIds).length > 0
    );
  }

  stageMarketingEngagement(record: MarketingEngagementRecord): MarketingEngagementRecord {
    delete this.stagedState.deletedMarketingEngagementIds[record.id];
    this.stagedState.marketingEngagements[record.id] = structuredClone(record);
    if (!this.stagedState.marketingEngagementOrder.includes(record.id)) {
      this.stagedState.marketingEngagementOrder.push(record.id);
    }
    return structuredClone(record);
  }

  stageDeleteMarketingEngagement(engagementId: string): void {
    delete this.stagedState.marketingEngagements[engagementId];
    this.stagedState.deletedMarketingEngagementIds[engagementId] = true;
  }

  stageDeleteMarketingEngagementsByChannelHandle(channelHandle: string): string[] {
    const deletedIds: string[] = [];
    for (const engagement of this.listEffectiveMarketingEngagements()) {
      if (engagement.channelHandle === channelHandle) {
        this.stageDeleteMarketingEngagement(engagement.id);
        deletedIds.push(engagement.id);
      }
    }
    return deletedIds;
  }

  stageDeleteAllChannelMarketingEngagements(): string[] {
    const deletedIds: string[] = [];
    for (const engagement of this.listEffectiveMarketingEngagements()) {
      if (engagement.channelHandle !== null && engagement.channelHandle !== undefined) {
        this.stageDeleteMarketingEngagement(engagement.id);
        deletedIds.push(engagement.id);
      }
    }
    return deletedIds;
  }

  listEffectiveMarketingEngagements(): MarketingEngagementRecord[] {
    const merged = new Map<string, MarketingEngagementRecord>();
    const orderedIds = [...this.baseState.marketingEngagementOrder, ...this.stagedState.marketingEngagementOrder];
    const allRecords = {
      ...this.baseState.marketingEngagements,
      ...this.stagedState.marketingEngagements,
    };

    for (const id of orderedIds) {
      const record = allRecords[id];
      if (record && !this.stagedState.deletedMarketingEngagementIds[id]) {
        merged.set(id, record);
      }
    }

    for (const record of Object.values(allRecords)) {
      if (!merged.has(record.id) && !this.stagedState.deletedMarketingEngagementIds[record.id]) {
        merged.set(record.id, record);
      }
    }

    return structuredClone([...merged.values()]);
  }

  hasKnownMarketingChannelHandle(channelHandle: string): boolean {
    return this.listEffectiveMarketingEvents().some(
      (event) => readMarketingChannelHandle(event.data) === channelHandle,
    );
  }

  private listBaseMarketingRecords(bucket: Record<string, MarketingRecord>, order: string[]): MarketingRecord[] {
    const orderedIds = new Set(order);
    const orderedRecords = order
      .map((id) => bucket[id] ?? null)
      .filter((record): record is MarketingRecord => record !== null);
    const unorderedRecords = Object.values(bucket)
      .filter((record) => !orderedIds.has(record.id))
      .sort((left, right) => left.id.localeCompare(right.id));

    return structuredClone([...orderedRecords, ...unorderedRecords]);
  }

  private listEffectiveMarketingRecords(
    baseBucket: Record<string, MarketingRecord>,
    baseOrder: string[],
    stagedBucket: Record<string, MarketingRecord>,
    stagedOrder: string[],
    deletedBucket: Record<string, boolean>,
  ): MarketingRecord[] {
    const merged = new Map<string, MarketingRecord>();
    for (const record of this.listBaseMarketingRecords(baseBucket, baseOrder)) {
      if (!deletedBucket[record.id]) {
        merged.set(record.id, record);
      }
    }
    for (const record of this.listBaseMarketingRecords(stagedBucket, stagedOrder)) {
      if (!deletedBucket[record.id]) {
        merged.set(record.id, record);
      }
    }

    return structuredClone([...merged.values()]);
  }

  private onlineStoreBucket(
    snapshot: StateSnapshot,
    kind: OnlineStoreContentKind,
  ): Record<string, OnlineStoreContentRecord> {
    switch (kind) {
      case 'article':
        return snapshot.onlineStoreArticles;
      case 'blog':
        return snapshot.onlineStoreBlogs;
      case 'page':
        return snapshot.onlineStorePages;
      case 'comment':
        return snapshot.onlineStoreComments;
    }
  }

  private onlineStoreOrder(snapshot: StateSnapshot, kind: OnlineStoreContentKind): string[] {
    switch (kind) {
      case 'article':
        return snapshot.onlineStoreArticleOrder;
      case 'blog':
        return snapshot.onlineStoreBlogOrder;
      case 'page':
        return snapshot.onlineStorePageOrder;
      case 'comment':
        return snapshot.onlineStoreCommentOrder;
    }
  }

  private onlineStoreDeletedIds(snapshot: StateSnapshot, kind: OnlineStoreContentKind): Record<string, true> {
    switch (kind) {
      case 'article':
        return snapshot.deletedOnlineStoreArticleIds;
      case 'blog':
        return snapshot.deletedOnlineStoreBlogIds;
      case 'page':
        return snapshot.deletedOnlineStorePageIds;
      case 'comment':
        return snapshot.deletedOnlineStoreCommentIds;
    }
  }

  upsertBaseOnlineStoreContent(records: OnlineStoreContentRecord[]): void {
    for (const record of records) {
      const bucket = this.onlineStoreBucket(this.baseState, record.kind);
      const order = this.onlineStoreOrder(this.baseState, record.kind);
      const baseDeletedIds = this.onlineStoreDeletedIds(this.baseState, record.kind);
      const stagedDeletedIds = this.onlineStoreDeletedIds(this.stagedState, record.kind);

      delete baseDeletedIds[record.id];
      delete stagedDeletedIds[record.id];
      bucket[record.id] = structuredClone(record);
      if (!order.includes(record.id)) {
        order.push(record.id);
      }
    }
  }

  upsertStagedOnlineStoreContent(record: OnlineStoreContentRecord): void {
    const bucket = this.onlineStoreBucket(this.stagedState, record.kind);
    const baseOrder = this.onlineStoreOrder(this.baseState, record.kind);
    const stagedOrder = this.onlineStoreOrder(this.stagedState, record.kind);
    const stagedDeletedIds = this.onlineStoreDeletedIds(this.stagedState, record.kind);

    delete stagedDeletedIds[record.id];
    bucket[record.id] = structuredClone(record);
    if (!baseOrder.includes(record.id) && !stagedOrder.includes(record.id)) {
      stagedOrder.push(record.id);
    }
  }

  deleteStagedOnlineStoreContent(kind: OnlineStoreContentKind, id: string): void {
    const bucket = this.onlineStoreBucket(this.stagedState, kind);
    const stagedDeletedIds = this.onlineStoreDeletedIds(this.stagedState, kind);
    delete bucket[id];
    stagedDeletedIds[id] = true;
  }

  getEffectiveOnlineStoreContentById(kind: OnlineStoreContentKind, id: string): OnlineStoreContentRecord | null {
    const stagedDeletedIds = this.onlineStoreDeletedIds(this.stagedState, kind);
    if (stagedDeletedIds[id]) {
      return null;
    }

    const record =
      this.onlineStoreBucket(this.stagedState, kind)[id] ?? this.onlineStoreBucket(this.baseState, kind)[id];
    return record ? structuredClone(record) : null;
  }

  listEffectiveOnlineStoreContent(kind: OnlineStoreContentKind): OnlineStoreContentRecord[] {
    const orderedIds = new Set([
      ...this.onlineStoreOrder(this.baseState, kind),
      ...this.onlineStoreOrder(this.stagedState, kind),
    ]);
    const orderedRecords = [...orderedIds]
      .map((id) => this.getEffectiveOnlineStoreContentById(kind, id))
      .filter((record): record is OnlineStoreContentRecord => record !== null);
    const stagedDeletedIds = this.onlineStoreDeletedIds(this.stagedState, kind);
    const unorderedRecords = Object.values({
      ...this.onlineStoreBucket(this.baseState, kind),
      ...this.onlineStoreBucket(this.stagedState, kind),
    })
      .filter((record) => !orderedIds.has(record.id))
      .filter((record) => !stagedDeletedIds[record.id])
      .sort(
        (left, right) =>
          (right.updatedAt ?? '').localeCompare(left.updatedAt ?? '') ||
          (right.createdAt ?? '').localeCompare(left.createdAt ?? '') ||
          compareShopifyResourceIds(left.id, right.id),
      );

    return structuredClone([...orderedRecords, ...unorderedRecords]);
  }

  hasOnlineStoreContent(): boolean {
    return (
      Object.keys(this.baseState.onlineStoreArticles).length > 0 ||
      Object.keys(this.baseState.onlineStoreBlogs).length > 0 ||
      Object.keys(this.baseState.onlineStorePages).length > 0 ||
      Object.keys(this.baseState.onlineStoreComments).length > 0 ||
      this.hasStagedOnlineStoreContent()
    );
  }

  hasStagedOnlineStoreContent(): boolean {
    return (
      Object.keys(this.stagedState.onlineStoreArticles).length > 0 ||
      Object.keys(this.stagedState.onlineStoreBlogs).length > 0 ||
      Object.keys(this.stagedState.onlineStorePages).length > 0 ||
      Object.keys(this.stagedState.onlineStoreComments).length > 0 ||
      Object.keys(this.stagedState.deletedOnlineStoreArticleIds).length > 0 ||
      Object.keys(this.stagedState.deletedOnlineStoreBlogIds).length > 0 ||
      Object.keys(this.stagedState.deletedOnlineStorePageIds).length > 0 ||
      Object.keys(this.stagedState.deletedOnlineStoreCommentIds).length > 0
    );
  }

  upsertBaseSavedSearches(records: SavedSearchRecord[]): void {
    for (const record of records) {
      delete this.baseState.deletedSavedSearchIds[record.id];
      delete this.stagedState.deletedSavedSearchIds[record.id];
      this.baseState.savedSearches[record.id] = structuredClone(record);
      if (!this.baseState.savedSearchOrder.includes(record.id)) {
        this.baseState.savedSearchOrder.push(record.id);
      }
    }
  }

  upsertStagedSavedSearch(record: SavedSearchRecord): SavedSearchRecord {
    delete this.stagedState.deletedSavedSearchIds[record.id];
    this.stagedState.savedSearches[record.id] = structuredClone(record);
    if (
      !this.baseState.savedSearchOrder.includes(record.id) &&
      !this.stagedState.savedSearchOrder.includes(record.id)
    ) {
      this.stagedState.savedSearchOrder.push(record.id);
    }
    return structuredClone(record);
  }

  deleteStagedSavedSearch(savedSearchId: string): void {
    delete this.stagedState.savedSearches[savedSearchId];
    this.stagedState.deletedSavedSearchIds[savedSearchId] = true;
  }

  getEffectiveSavedSearchById(savedSearchId: string): SavedSearchRecord | null {
    if (this.stagedState.deletedSavedSearchIds[savedSearchId] || this.baseState.deletedSavedSearchIds[savedSearchId]) {
      return null;
    }

    const record = this.stagedState.savedSearches[savedSearchId] ?? this.baseState.savedSearches[savedSearchId];
    return record ? structuredClone(record) : null;
  }

  listEffectiveSavedSearches(): SavedSearchRecord[] {
    const orderedIds = new Set([...this.baseState.savedSearchOrder, ...this.stagedState.savedSearchOrder]);
    const orderedRecords = [...orderedIds]
      .map((id) => this.getEffectiveSavedSearchById(id))
      .filter((record): record is SavedSearchRecord => record !== null);
    const unorderedRecords = Object.values({
      ...this.baseState.savedSearches,
      ...this.stagedState.savedSearches,
    })
      .filter((record) => !orderedIds.has(record.id))
      .map((record) => this.getEffectiveSavedSearchById(record.id))
      .filter((record): record is SavedSearchRecord => record !== null)
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedRecords, ...unorderedRecords]);
  }

  hasSavedSearches(): boolean {
    return Object.keys(this.baseState.savedSearches).length > 0 || this.hasStagedSavedSearches();
  }

  hasStagedSavedSearches(): boolean {
    return (
      Object.keys(this.stagedState.savedSearches).length > 0 ||
      Object.keys(this.stagedState.deletedSavedSearchIds).length > 0
    );
  }

  upsertBaseBulkOperations(operations: BulkOperationRecord[]): void {
    for (const operation of operations) {
      this.baseState.bulkOperations[operation.id] = structuredClone(operation);
      if (!this.baseState.bulkOperationOrder.includes(operation.id)) {
        this.baseState.bulkOperationOrder.push(operation.id);
      }
    }
  }

  stageBulkOperation(operation: BulkOperationRecord): BulkOperationRecord {
    this.stagedState.bulkOperations[operation.id] = structuredClone(operation);
    if (
      !this.baseState.bulkOperationOrder.includes(operation.id) &&
      !this.stagedState.bulkOperationOrder.includes(operation.id)
    ) {
      this.stagedState.bulkOperationOrder.push(operation.id);
    }
    return structuredClone(operation);
  }

  stageBulkOperationResult(operation: BulkOperationRecord, jsonl: string): BulkOperationRecord {
    const stagedOperation = this.stageBulkOperation(operation);
    this.stagedState.bulkOperationResults[operation.id] = jsonl;
    return stagedOperation;
  }

  getEffectiveBulkOperationById(operationId: string): BulkOperationRecord | null {
    const operation =
      this.stagedState.bulkOperations[operationId] ?? this.baseState.bulkOperations[operationId] ?? null;
    return operation ? structuredClone(operation) : null;
  }

  getStagedBulkOperationById(operationId: string): BulkOperationRecord | null {
    const operation = this.stagedState.bulkOperations[operationId] ?? null;
    return operation ? structuredClone(operation) : null;
  }

  listEffectiveBulkOperations(): BulkOperationRecord[] {
    const orderedIds = new Set([...this.baseState.bulkOperationOrder, ...this.stagedState.bulkOperationOrder]);
    const orderedOperations = [...orderedIds]
      .map((operationId) => this.getEffectiveBulkOperationById(operationId))
      .filter((operation): operation is BulkOperationRecord => operation !== null);
    const unorderedOperations = Object.values({
      ...this.baseState.bulkOperations,
      ...this.stagedState.bulkOperations,
    })
      .filter((operation) => !orderedIds.has(operation.id))
      .sort(
        (left, right) => right.createdAt.localeCompare(left.createdAt) || compareShopifyResourceIds(left.id, right.id),
      )
      .map((operation) => structuredClone(operation));

    return [...orderedOperations, ...unorderedOperations];
  }

  getEffectiveBulkOperationResultJsonl(operationId: string): string | null {
    return (
      this.stagedState.bulkOperationResults[operationId] ?? this.baseState.bulkOperationResults[operationId] ?? null
    );
  }

  cancelStagedBulkOperation(operationId: string): BulkOperationRecord | null {
    const operation = this.stagedState.bulkOperations[operationId] ?? null;
    if (!operation) {
      return null;
    }

    operation.status = 'CANCELING';
    operation.completedAt = null;
    return structuredClone(operation);
  }

  hasBulkOperations(): boolean {
    return (
      Object.keys(this.baseState.bulkOperations).length > 0 || Object.keys(this.stagedState.bulkOperations).length > 0
    );
  }

  hasStagedBulkOperations(): boolean {
    return Object.keys(this.stagedState.bulkOperations).length > 0;
  }

  stageCreateSegment(segment: SegmentRecord): SegmentRecord {
    delete this.stagedState.deletedSegmentIds[segment.id];
    this.stagedState.segments[segment.id] = structuredClone(segment);
    return structuredClone(segment);
  }

  stageUpdateSegment(segment: SegmentRecord): SegmentRecord {
    delete this.stagedState.deletedSegmentIds[segment.id];
    this.stagedState.segments[segment.id] = structuredClone(segment);
    return structuredClone(segment);
  }

  stageDeleteSegment(segmentId: string): void {
    delete this.stagedState.segments[segmentId];
    this.stagedState.deletedSegmentIds[segmentId] = true;
  }

  stageCustomerSegmentMembersQuery(query: CustomerSegmentMembersQueryRecord): CustomerSegmentMembersQueryRecord {
    this.stagedState.customerSegmentMembersQueries[query.id] = structuredClone(query);
    return structuredClone(query);
  }

  getEffectiveCustomerSegmentMembersQueryById(queryId: string): CustomerSegmentMembersQueryRecord | null {
    const query =
      this.stagedState.customerSegmentMembersQueries[queryId] ??
      this.baseState.customerSegmentMembersQueries[queryId] ??
      null;
    return query ? structuredClone(query) : null;
  }

  hasCustomerSegmentMembersQueries(): boolean {
    return (
      Object.keys(this.baseState.customerSegmentMembersQueries).length > 0 ||
      Object.keys(this.stagedState.customerSegmentMembersQueries).length > 0
    );
  }

  getEffectiveSegmentById(segmentId: string): SegmentRecord | null {
    if (this.stagedState.deletedSegmentIds[segmentId]) {
      return null;
    }

    const segment = this.stagedState.segments[segmentId] ?? this.baseState.segments[segmentId] ?? null;
    return segment ? structuredClone(segment) : null;
  }

  listEffectiveSegments(): SegmentRecord[] {
    const mergedSegments = new Map<string, SegmentRecord>();
    for (const segment of [...Object.values(this.baseState.segments), ...Object.values(this.stagedState.segments)]) {
      if (this.stagedState.deletedSegmentIds[segment.id]) {
        continue;
      }
      mergedSegments.set(segment.id, structuredClone(segment));
    }

    return Array.from(mergedSegments.values()).sort(
      (left, right) =>
        (left.creationDate ?? '').localeCompare(right.creationDate ?? '') || left.id.localeCompare(right.id),
    );
  }

  hasStagedSegments(): boolean {
    return (
      Object.keys(this.stagedState.segments).length > 0 || Object.keys(this.stagedState.deletedSegmentIds).length > 0
    );
  }

  upsertBaseDiscounts(discounts: DiscountRecord[]): void {
    for (const discount of discounts) {
      delete this.baseState.deletedDiscountIds[discount.id];
      delete this.stagedState.deletedDiscountIds[discount.id];
      this.baseState.discounts[discount.id] = structuredClone(discount);
    }
  }

  upsertBasePaymentCustomizations(paymentCustomizations: PaymentCustomizationRecord[]): void {
    for (const customization of paymentCustomizations) {
      delete this.baseState.deletedPaymentCustomizationIds[customization.id];
      delete this.stagedState.deletedPaymentCustomizationIds[customization.id];
      this.baseState.paymentCustomizations[customization.id] = structuredClone(customization);
      if (!this.baseState.paymentCustomizationOrder.includes(customization.id)) {
        this.baseState.paymentCustomizationOrder.push(customization.id);
      }
    }
  }

  upsertBasePaymentTermsTemplates(paymentTermsTemplates: PaymentTermsTemplateRecord[]): void {
    for (const template of paymentTermsTemplates) {
      this.baseState.paymentTermsTemplates[template.id] = structuredClone(template);
      if (!this.baseState.paymentTermsTemplateOrder.includes(template.id)) {
        this.baseState.paymentTermsTemplateOrder.push(template.id);
      }
    }
  }

  upsertStagedPaymentCustomization(paymentCustomization: PaymentCustomizationRecord): void {
    delete this.stagedState.deletedPaymentCustomizationIds[paymentCustomization.id];
    this.stagedState.paymentCustomizations[paymentCustomization.id] = structuredClone(paymentCustomization);
    if (
      !this.baseState.paymentCustomizationOrder.includes(paymentCustomization.id) &&
      !this.stagedState.paymentCustomizationOrder.includes(paymentCustomization.id)
    ) {
      this.stagedState.paymentCustomizationOrder.push(paymentCustomization.id);
    }
  }

  deleteStagedPaymentCustomization(paymentCustomizationId: string): void {
    delete this.stagedState.paymentCustomizations[paymentCustomizationId];
    this.stagedState.deletedPaymentCustomizationIds[paymentCustomizationId] = true;
  }

  upsertStagedShopifyFunction(shopifyFunction: ShopifyFunctionRecord): void {
    this.stagedState.shopifyFunctions[shopifyFunction.id] = structuredClone(shopifyFunction);
    if (
      !this.baseState.shopifyFunctionOrder.includes(shopifyFunction.id) &&
      !this.stagedState.shopifyFunctionOrder.includes(shopifyFunction.id)
    ) {
      this.stagedState.shopifyFunctionOrder.push(shopifyFunction.id);
    }
  }

  getEffectiveShopifyFunctionById(shopifyFunctionId: string): ShopifyFunctionRecord | null {
    const shopifyFunction =
      this.stagedState.shopifyFunctions[shopifyFunctionId] ??
      this.baseState.shopifyFunctions[shopifyFunctionId] ??
      null;
    return shopifyFunction ? structuredClone(shopifyFunction) : null;
  }

  listEffectiveShopifyFunctions(): ShopifyFunctionRecord[] {
    const orderedIds = new Set([...this.baseState.shopifyFunctionOrder, ...this.stagedState.shopifyFunctionOrder]);
    const orderedFunctions = Array.from(orderedIds)
      .map((id) => this.getEffectiveShopifyFunctionById(id))
      .filter((shopifyFunction): shopifyFunction is ShopifyFunctionRecord => shopifyFunction !== null);
    const unorderedFunctions = Object.values({
      ...this.baseState.shopifyFunctions,
      ...this.stagedState.shopifyFunctions,
    })
      .filter((shopifyFunction) => !orderedIds.has(shopifyFunction.id))
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedFunctions, ...unorderedFunctions]);
  }

  upsertStagedValidation(validation: ValidationRecord): void {
    delete this.stagedState.deletedValidationIds[validation.id];
    this.stagedState.validations[validation.id] = structuredClone(validation);
    if (
      !this.baseState.validationOrder.includes(validation.id) &&
      !this.stagedState.validationOrder.includes(validation.id)
    ) {
      this.stagedState.validationOrder.push(validation.id);
    }
  }

  deleteStagedValidation(validationId: string): void {
    delete this.stagedState.validations[validationId];
    this.stagedState.deletedValidationIds[validationId] = true;
  }

  getEffectiveValidationById(validationId: string): ValidationRecord | null {
    if (this.stagedState.deletedValidationIds[validationId]) {
      return null;
    }

    const validation = this.stagedState.validations[validationId] ?? this.baseState.validations[validationId] ?? null;
    return validation ? structuredClone(validation) : null;
  }

  listEffectiveValidations(): ValidationRecord[] {
    const orderedIds = new Set([...this.baseState.validationOrder, ...this.stagedState.validationOrder]);
    const orderedValidations = Array.from(orderedIds)
      .map((id) => this.getEffectiveValidationById(id))
      .filter((validation): validation is ValidationRecord => validation !== null);
    const unorderedValidations = Object.values({
      ...this.baseState.validations,
      ...this.stagedState.validations,
    })
      .filter((validation) => !orderedIds.has(validation.id))
      .filter((validation) => !this.stagedState.deletedValidationIds[validation.id])
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedValidations, ...unorderedValidations]);
  }

  upsertStagedCartTransform(cartTransform: CartTransformRecord): void {
    delete this.stagedState.deletedCartTransformIds[cartTransform.id];
    this.stagedState.cartTransforms[cartTransform.id] = structuredClone(cartTransform);
    if (
      !this.baseState.cartTransformOrder.includes(cartTransform.id) &&
      !this.stagedState.cartTransformOrder.includes(cartTransform.id)
    ) {
      this.stagedState.cartTransformOrder.push(cartTransform.id);
    }
  }

  deleteStagedCartTransform(cartTransformId: string): void {
    delete this.stagedState.cartTransforms[cartTransformId];
    this.stagedState.deletedCartTransformIds[cartTransformId] = true;
  }

  getEffectiveCartTransformById(cartTransformId: string): CartTransformRecord | null {
    if (this.stagedState.deletedCartTransformIds[cartTransformId]) {
      return null;
    }

    const cartTransform =
      this.stagedState.cartTransforms[cartTransformId] ?? this.baseState.cartTransforms[cartTransformId] ?? null;
    return cartTransform ? structuredClone(cartTransform) : null;
  }

  listEffectiveCartTransforms(): CartTransformRecord[] {
    const orderedIds = new Set([...this.baseState.cartTransformOrder, ...this.stagedState.cartTransformOrder]);
    const orderedCartTransforms = Array.from(orderedIds)
      .map((id) => this.getEffectiveCartTransformById(id))
      .filter((cartTransform): cartTransform is CartTransformRecord => cartTransform !== null);
    const unorderedCartTransforms = Object.values({
      ...this.baseState.cartTransforms,
      ...this.stagedState.cartTransforms,
    })
      .filter((cartTransform) => !orderedIds.has(cartTransform.id))
      .filter((cartTransform) => !this.stagedState.deletedCartTransformIds[cartTransform.id])
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedCartTransforms, ...unorderedCartTransforms]);
  }

  setStagedTaxAppConfiguration(configuration: TaxAppConfigurationRecord): void {
    this.stagedState.taxAppConfiguration = structuredClone(configuration);
  }

  getEffectiveTaxAppConfiguration(): TaxAppConfigurationRecord | null {
    const configuration = this.stagedState.taxAppConfiguration ?? this.baseState.taxAppConfiguration ?? null;
    return configuration ? structuredClone(configuration) : null;
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
      delete this.baseState.deletedLocationIds[location.id];
      delete this.stagedState.deletedLocationIds[location.id];
      this.baseState.locations[location.id] = structuredClone(location);
      if (!this.baseState.locationOrder.includes(location.id)) {
        this.baseState.locationOrder.push(location.id);
      }
    }
  }

  upsertBaseFulfillmentServices(services: FulfillmentServiceRecord[]): void {
    for (const service of services) {
      delete this.baseState.deletedFulfillmentServiceIds[service.id];
      delete this.stagedState.deletedFulfillmentServiceIds[service.id];
      this.baseState.fulfillmentServices[service.id] = structuredClone(service);
      if (!this.baseState.fulfillmentServiceOrder.includes(service.id)) {
        this.baseState.fulfillmentServiceOrder.push(service.id);
      }
    }
  }

  upsertBaseCarrierServices(services: CarrierServiceRecord[]): void {
    for (const service of services) {
      delete this.baseState.deletedCarrierServiceIds[service.id];
      delete this.stagedState.deletedCarrierServiceIds[service.id];
      this.baseState.carrierServices[service.id] = structuredClone(service);
      if (!this.baseState.carrierServiceOrder.includes(service.id)) {
        this.baseState.carrierServiceOrder.push(service.id);
      }
    }
  }

  upsertBaseShippingPackages(packages: ShippingPackageRecord[]): void {
    for (const shippingPackage of packages) {
      delete this.baseState.deletedShippingPackageIds[shippingPackage.id];
      delete this.stagedState.deletedShippingPackageIds[shippingPackage.id];
      this.baseState.shippingPackages[shippingPackage.id] = structuredClone(shippingPackage);
      if (!this.baseState.shippingPackageOrder.includes(shippingPackage.id)) {
        this.baseState.shippingPackageOrder.push(shippingPackage.id);
      }
    }
  }

  upsertBaseGiftCards(giftCards: GiftCardRecord[]): void {
    for (const giftCard of giftCards) {
      delete this.baseState.deletedGiftCardIds[giftCard.id];
      delete this.stagedState.deletedGiftCardIds[giftCard.id];
      this.baseState.giftCards[giftCard.id] = structuredClone(giftCard);
      if (!this.baseState.giftCardOrder.includes(giftCard.id)) {
        this.baseState.giftCardOrder.push(giftCard.id);
      }
    }
  }

  upsertBaseGiftCardConfiguration(configuration: GiftCardConfigurationRecord): void {
    this.baseState.giftCardConfiguration = structuredClone(configuration);
  }

  listBaseLocations(): LocationRecord[] {
    const orderedIds = new Set(this.baseState.locationOrder);
    const orderedLocations = this.baseState.locationOrder
      .map((id) => this.baseState.locations[id] ?? null)
      .filter(
        (location): location is LocationRecord => location !== null && !this.baseState.deletedLocationIds[location.id],
      );
    const unorderedLocations = Object.values(this.baseState.locations)
      .filter((location) => !orderedIds.has(location.id) && !this.baseState.deletedLocationIds[location.id])
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedLocations, ...unorderedLocations]);
  }

  getBaseLocationById(locationId: string): LocationRecord | null {
    if (this.baseState.deletedLocationIds[locationId]) {
      return null;
    }

    const location = this.baseState.locations[locationId] ?? null;
    return location ? structuredClone(location) : null;
  }

  stageCreateLocation(location: LocationRecord): LocationRecord {
    delete this.stagedState.deletedLocationIds[location.id];
    this.stagedState.locations[location.id] = structuredClone(location);
    if (!this.stagedState.locationOrder.includes(location.id)) {
      this.stagedState.locationOrder.push(location.id);
    }
    return structuredClone(location);
  }

  stageUpdateLocation(location: LocationRecord): LocationRecord {
    delete this.stagedState.deletedLocationIds[location.id];
    this.stagedState.locations[location.id] = structuredClone(location);
    if (!this.baseState.locationOrder.includes(location.id) && !this.stagedState.locationOrder.includes(location.id)) {
      this.stagedState.locationOrder.push(location.id);
    }
    return structuredClone(location);
  }

  stageDeleteLocation(locationId: string): void {
    delete this.stagedState.locations[locationId];
    this.stagedState.deletedLocationIds[locationId] = true;
  }

  getEffectiveLocationById(locationId: string): LocationRecord | null {
    if (this.stagedState.deletedLocationIds[locationId] || this.baseState.deletedLocationIds[locationId]) {
      return null;
    }

    return mergeLocationRecords(
      this.baseState.locations[locationId] ?? null,
      this.stagedState.locations[locationId] ?? null,
    );
  }

  isLocationDeleted(locationId: string): boolean {
    return this.stagedState.locations[locationId]?.deleted === true;
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

  hasStagedLocations(): boolean {
    return (
      Object.keys(this.stagedState.locations).length > 0 || Object.keys(this.stagedState.deletedLocationIds).length > 0
    );
  }

  stageCreateFulfillmentService(service: FulfillmentServiceRecord): FulfillmentServiceRecord {
    delete this.stagedState.deletedFulfillmentServiceIds[service.id];
    this.stagedState.fulfillmentServices[service.id] = structuredClone(service);
    if (!this.stagedState.fulfillmentServiceOrder.includes(service.id)) {
      this.stagedState.fulfillmentServiceOrder.push(service.id);
    }
    return structuredClone(service);
  }

  stageUpdateFulfillmentService(service: FulfillmentServiceRecord): FulfillmentServiceRecord {
    delete this.stagedState.deletedFulfillmentServiceIds[service.id];
    this.stagedState.fulfillmentServices[service.id] = structuredClone(service);
    if (
      !this.baseState.fulfillmentServiceOrder.includes(service.id) &&
      !this.stagedState.fulfillmentServiceOrder.includes(service.id)
    ) {
      this.stagedState.fulfillmentServiceOrder.push(service.id);
    }
    return structuredClone(service);
  }

  stageDeleteFulfillmentService(serviceId: string): void {
    delete this.stagedState.fulfillmentServices[serviceId];
    this.stagedState.deletedFulfillmentServiceIds[serviceId] = true;
  }

  getEffectiveFulfillmentServiceById(serviceId: string): FulfillmentServiceRecord | null {
    if (
      this.stagedState.deletedFulfillmentServiceIds[serviceId] ||
      this.baseState.deletedFulfillmentServiceIds[serviceId]
    ) {
      return null;
    }

    return mergeFulfillmentServiceRecords(
      this.baseState.fulfillmentServices[serviceId] ?? null,
      this.stagedState.fulfillmentServices[serviceId] ?? null,
    );
  }

  listEffectiveFulfillmentServices(): FulfillmentServiceRecord[] {
    const orderedIds = new Set([
      ...this.baseState.fulfillmentServiceOrder,
      ...this.stagedState.fulfillmentServiceOrder,
    ]);
    const orderedServices = [...orderedIds]
      .map((id) => this.getEffectiveFulfillmentServiceById(id))
      .filter((service): service is FulfillmentServiceRecord => service !== null);
    const unorderedServices = Object.values({
      ...this.baseState.fulfillmentServices,
      ...this.stagedState.fulfillmentServices,
    })
      .filter((service) => !orderedIds.has(service.id))
      .map((service) => this.getEffectiveFulfillmentServiceById(service.id))
      .filter((service): service is FulfillmentServiceRecord => service !== null)
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedServices, ...unorderedServices]);
  }

  hasStagedFulfillmentServices(): boolean {
    return (
      Object.keys(this.stagedState.fulfillmentServices).length > 0 ||
      Object.keys(this.stagedState.deletedFulfillmentServiceIds).length > 0
    );
  }

  stageCreateCarrierService(service: CarrierServiceRecord): CarrierServiceRecord {
    delete this.stagedState.deletedCarrierServiceIds[service.id];
    this.stagedState.carrierServices[service.id] = structuredClone(service);
    if (!this.stagedState.carrierServiceOrder.includes(service.id)) {
      this.stagedState.carrierServiceOrder.push(service.id);
    }
    return structuredClone(service);
  }

  stageUpdateCarrierService(service: CarrierServiceRecord): CarrierServiceRecord {
    delete this.stagedState.deletedCarrierServiceIds[service.id];
    this.stagedState.carrierServices[service.id] = structuredClone(service);
    if (
      !this.baseState.carrierServiceOrder.includes(service.id) &&
      !this.stagedState.carrierServiceOrder.includes(service.id)
    ) {
      this.stagedState.carrierServiceOrder.push(service.id);
    }
    return structuredClone(service);
  }

  stageDeleteCarrierService(serviceId: string): void {
    delete this.stagedState.carrierServices[serviceId];
    this.stagedState.deletedCarrierServiceIds[serviceId] = true;
  }

  getEffectiveCarrierServiceById(serviceId: string): CarrierServiceRecord | null {
    if (this.stagedState.deletedCarrierServiceIds[serviceId] || this.baseState.deletedCarrierServiceIds[serviceId]) {
      return null;
    }

    return mergeCarrierServiceRecords(
      this.baseState.carrierServices[serviceId] ?? null,
      this.stagedState.carrierServices[serviceId] ?? null,
    );
  }

  listEffectiveCarrierServices(): CarrierServiceRecord[] {
    const orderedIds = new Set([...this.baseState.carrierServiceOrder, ...this.stagedState.carrierServiceOrder]);
    const orderedServices = [...orderedIds]
      .map((id) => this.getEffectiveCarrierServiceById(id))
      .filter((service): service is CarrierServiceRecord => service !== null);
    const unorderedServices = Object.values({
      ...this.baseState.carrierServices,
      ...this.stagedState.carrierServices,
    })
      .filter((service) => !orderedIds.has(service.id))
      .map((service) => this.getEffectiveCarrierServiceById(service.id))
      .filter((service): service is CarrierServiceRecord => service !== null)
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedServices, ...unorderedServices]);
  }

  hasStagedCarrierServices(): boolean {
    return (
      Object.keys(this.stagedState.carrierServices).length > 0 ||
      Object.keys(this.stagedState.deletedCarrierServiceIds).length > 0
    );
  }

  upsertBaseInventoryShipments(shipments: InventoryShipmentRecord[]): void {
    for (const shipment of shipments) {
      delete this.baseState.deletedInventoryShipmentIds[shipment.id];
      delete this.stagedState.deletedInventoryShipmentIds[shipment.id];
      this.baseState.inventoryShipments[shipment.id] = structuredClone(shipment);
      if (!this.baseState.inventoryShipmentOrder.includes(shipment.id)) {
        this.baseState.inventoryShipmentOrder.push(shipment.id);
      }
    }
  }

  stageInventoryShipment(shipment: InventoryShipmentRecord): InventoryShipmentRecord {
    delete this.stagedState.deletedInventoryShipmentIds[shipment.id];
    this.stagedState.inventoryShipments[shipment.id] = structuredClone(shipment);
    if (
      !this.baseState.inventoryShipmentOrder.includes(shipment.id) &&
      !this.stagedState.inventoryShipmentOrder.includes(shipment.id)
    ) {
      this.stagedState.inventoryShipmentOrder.push(shipment.id);
    }
    return structuredClone(shipment);
  }

  stageDeleteInventoryShipment(shipmentId: string): void {
    delete this.stagedState.inventoryShipments[shipmentId];
    this.stagedState.deletedInventoryShipmentIds[shipmentId] = true;
  }

  getEffectiveInventoryShipmentById(shipmentId: string): InventoryShipmentRecord | null {
    if (
      this.stagedState.deletedInventoryShipmentIds[shipmentId] ||
      this.baseState.deletedInventoryShipmentIds[shipmentId]
    ) {
      return null;
    }

    const shipment =
      this.stagedState.inventoryShipments[shipmentId] ?? this.baseState.inventoryShipments[shipmentId] ?? null;
    return shipment ? structuredClone(shipment) : null;
  }

  listEffectiveInventoryShipments(): InventoryShipmentRecord[] {
    const orderedIds = new Set([...this.baseState.inventoryShipmentOrder, ...this.stagedState.inventoryShipmentOrder]);
    const orderedShipments = [...orderedIds]
      .map((id) => this.getEffectiveInventoryShipmentById(id))
      .filter((shipment): shipment is InventoryShipmentRecord => shipment !== null);
    const unorderedShipments = Object.values({
      ...this.baseState.inventoryShipments,
      ...this.stagedState.inventoryShipments,
    })
      .filter((shipment) => !orderedIds.has(shipment.id))
      .map((shipment) => this.getEffectiveInventoryShipmentById(shipment.id))
      .filter((shipment): shipment is InventoryShipmentRecord => shipment !== null)
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedShipments, ...unorderedShipments]);
  }

  hasInventoryShipments(): boolean {
    return (
      Object.keys(this.baseState.inventoryShipments).length > 0 ||
      Object.keys(this.stagedState.inventoryShipments).length > 0 ||
      Object.keys(this.stagedState.deletedInventoryShipmentIds).length > 0
    );
  }

  stageUpdateShippingPackage(shippingPackage: ShippingPackageRecord): ShippingPackageRecord {
    delete this.stagedState.deletedShippingPackageIds[shippingPackage.id];
    this.stagedState.shippingPackages[shippingPackage.id] = structuredClone(shippingPackage);
    if (
      !this.baseState.shippingPackageOrder.includes(shippingPackage.id) &&
      !this.stagedState.shippingPackageOrder.includes(shippingPackage.id)
    ) {
      this.stagedState.shippingPackageOrder.push(shippingPackage.id);
    }
    return structuredClone(shippingPackage);
  }

  stageDeleteShippingPackage(shippingPackageId: string): void {
    delete this.stagedState.shippingPackages[shippingPackageId];
    this.stagedState.deletedShippingPackageIds[shippingPackageId] = true;
  }

  getEffectiveShippingPackageById(shippingPackageId: string): ShippingPackageRecord | null {
    if (
      this.stagedState.deletedShippingPackageIds[shippingPackageId] ||
      this.baseState.deletedShippingPackageIds[shippingPackageId]
    ) {
      return null;
    }

    return mergeShippingPackageRecords(
      this.baseState.shippingPackages[shippingPackageId] ?? null,
      this.stagedState.shippingPackages[shippingPackageId] ?? null,
    );
  }

  listEffectiveShippingPackages(): ShippingPackageRecord[] {
    const orderedIds = new Set([...this.baseState.shippingPackageOrder, ...this.stagedState.shippingPackageOrder]);
    const orderedPackages = [...orderedIds]
      .map((id) => this.getEffectiveShippingPackageById(id))
      .filter((shippingPackage): shippingPackage is ShippingPackageRecord => shippingPackage !== null);
    const unorderedPackages = Object.values({
      ...this.baseState.shippingPackages,
      ...this.stagedState.shippingPackages,
    })
      .filter((shippingPackage) => !orderedIds.has(shippingPackage.id))
      .map((shippingPackage) => this.getEffectiveShippingPackageById(shippingPackage.id))
      .filter((shippingPackage): shippingPackage is ShippingPackageRecord => shippingPackage !== null)
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedPackages, ...unorderedPackages]);
  }

  hasStagedShippingPackages(): boolean {
    return (
      Object.keys(this.stagedState.shippingPackages).length > 0 ||
      Object.keys(this.stagedState.deletedShippingPackageIds).length > 0
    );
  }

  stageCreateGiftCard(giftCard: GiftCardRecord): GiftCardRecord {
    delete this.stagedState.deletedGiftCardIds[giftCard.id];
    this.stagedState.giftCards[giftCard.id] = structuredClone(giftCard);
    if (!this.stagedState.giftCardOrder.includes(giftCard.id)) {
      this.stagedState.giftCardOrder.push(giftCard.id);
    }
    return structuredClone(giftCard);
  }

  stageUpdateGiftCard(giftCard: GiftCardRecord): GiftCardRecord {
    delete this.stagedState.deletedGiftCardIds[giftCard.id];
    this.stagedState.giftCards[giftCard.id] = structuredClone(giftCard);
    if (!this.baseState.giftCardOrder.includes(giftCard.id) && !this.stagedState.giftCardOrder.includes(giftCard.id)) {
      this.stagedState.giftCardOrder.push(giftCard.id);
    }
    return structuredClone(giftCard);
  }

  getEffectiveGiftCardById(giftCardId: string): GiftCardRecord | null {
    if (this.stagedState.deletedGiftCardIds[giftCardId] || this.baseState.deletedGiftCardIds[giftCardId]) {
      return null;
    }

    return mergeGiftCardRecords(
      this.baseState.giftCards[giftCardId] ?? null,
      this.stagedState.giftCards[giftCardId] ?? null,
    );
  }

  listEffectiveGiftCards(): GiftCardRecord[] {
    const orderedIds = new Set([...this.baseState.giftCardOrder, ...this.stagedState.giftCardOrder]);
    const orderedGiftCards = [...orderedIds]
      .map((id) => this.getEffectiveGiftCardById(id))
      .filter((giftCard): giftCard is GiftCardRecord => giftCard !== null);
    const unorderedGiftCards = Object.values({ ...this.baseState.giftCards, ...this.stagedState.giftCards })
      .filter((giftCard) => !orderedIds.has(giftCard.id))
      .map((giftCard) => this.getEffectiveGiftCardById(giftCard.id))
      .filter((giftCard): giftCard is GiftCardRecord => giftCard !== null)
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedGiftCards, ...unorderedGiftCards]);
  }

  getEffectiveGiftCardConfiguration(): GiftCardConfigurationRecord {
    return structuredClone(
      this.stagedState.giftCardConfiguration ??
        this.baseState.giftCardConfiguration ?? {
          issueLimit: {
            amount: '0.0',
            currencyCode: (this.stagedState.shop ?? this.baseState.shop)?.currencyCode ?? 'CAD',
          },
          purchaseLimit: {
            amount: '0.0',
            currencyCode: (this.stagedState.shop ?? this.baseState.shop)?.currencyCode ?? 'CAD',
          },
        },
    );
  }

  hasGiftCards(): boolean {
    return Object.keys(this.baseState.giftCards).length > 0;
  }

  hasStagedGiftCards(): boolean {
    return (
      Object.keys(this.stagedState.giftCards).length > 0 || Object.keys(this.stagedState.deletedGiftCardIds).length > 0
    );
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

  upsertBaseB2BCompanies(companies: B2BCompanyRecord[]): void {
    for (const company of companies) {
      this.baseState.b2bCompanies[company.id] = structuredClone(company);
      if (!this.baseState.b2bCompanyOrder.includes(company.id)) {
        this.baseState.b2bCompanyOrder.push(company.id);
      }
    }
  }

  upsertBaseB2BCompanyContacts(contacts: B2BCompanyContactRecord[]): void {
    for (const contact of contacts) {
      this.baseState.b2bCompanyContacts[contact.id] = structuredClone(contact);
      if (!this.baseState.b2bCompanyContactOrder.includes(contact.id)) {
        this.baseState.b2bCompanyContactOrder.push(contact.id);
      }
    }
  }

  upsertBaseB2BCompanyContactRoles(roles: B2BCompanyContactRoleRecord[]): void {
    for (const role of roles) {
      this.baseState.b2bCompanyContactRoles[role.id] = structuredClone(role);
      if (!this.baseState.b2bCompanyContactRoleOrder.includes(role.id)) {
        this.baseState.b2bCompanyContactRoleOrder.push(role.id);
      }
    }
  }

  upsertBaseB2BCompanyLocations(locations: B2BCompanyLocationRecord[]): void {
    for (const location of locations) {
      this.baseState.b2bCompanyLocations[location.id] = structuredClone(location);
      if (!this.baseState.b2bCompanyLocationOrder.includes(location.id)) {
        this.baseState.b2bCompanyLocationOrder.push(location.id);
      }
    }
  }

  upsertStagedB2BCompany(company: B2BCompanyRecord): B2BCompanyRecord {
    delete this.stagedState.deletedB2BCompanyIds[company.id];
    this.stagedState.b2bCompanies[company.id] = structuredClone(company);
    if (
      !this.baseState.b2bCompanyOrder.includes(company.id) &&
      !this.stagedState.b2bCompanyOrder.includes(company.id)
    ) {
      this.stagedState.b2bCompanyOrder.push(company.id);
    }
    return structuredClone(company);
  }

  upsertStagedB2BCompanyContact(contact: B2BCompanyContactRecord): B2BCompanyContactRecord {
    delete this.stagedState.deletedB2BCompanyContactIds[contact.id];
    this.stagedState.b2bCompanyContacts[contact.id] = structuredClone(contact);
    if (
      !this.baseState.b2bCompanyContactOrder.includes(contact.id) &&
      !this.stagedState.b2bCompanyContactOrder.includes(contact.id)
    ) {
      this.stagedState.b2bCompanyContactOrder.push(contact.id);
    }
    return structuredClone(contact);
  }

  upsertStagedB2BCompanyContactRole(role: B2BCompanyContactRoleRecord): B2BCompanyContactRoleRecord {
    delete this.stagedState.deletedB2BCompanyContactRoleIds[role.id];
    this.stagedState.b2bCompanyContactRoles[role.id] = structuredClone(role);
    if (
      !this.baseState.b2bCompanyContactRoleOrder.includes(role.id) &&
      !this.stagedState.b2bCompanyContactRoleOrder.includes(role.id)
    ) {
      this.stagedState.b2bCompanyContactRoleOrder.push(role.id);
    }
    return structuredClone(role);
  }

  upsertStagedB2BCompanyLocation(location: B2BCompanyLocationRecord): B2BCompanyLocationRecord {
    delete this.stagedState.deletedB2BCompanyLocationIds[location.id];
    this.stagedState.b2bCompanyLocations[location.id] = structuredClone(location);
    if (
      !this.baseState.b2bCompanyLocationOrder.includes(location.id) &&
      !this.stagedState.b2bCompanyLocationOrder.includes(location.id)
    ) {
      this.stagedState.b2bCompanyLocationOrder.push(location.id);
    }
    return structuredClone(location);
  }

  deleteStagedB2BCompany(companyId: string): void {
    delete this.stagedState.b2bCompanies[companyId];
    this.stagedState.deletedB2BCompanyIds[companyId] = true;
  }

  deleteStagedB2BCompanyContact(contactId: string): void {
    delete this.stagedState.b2bCompanyContacts[contactId];
    this.stagedState.deletedB2BCompanyContactIds[contactId] = true;
  }

  deleteStagedB2BCompanyContactRole(roleId: string): void {
    delete this.stagedState.b2bCompanyContactRoles[roleId];
    this.stagedState.deletedB2BCompanyContactRoleIds[roleId] = true;
  }

  deleteStagedB2BCompanyLocation(locationId: string): void {
    delete this.stagedState.b2bCompanyLocations[locationId];
    this.stagedState.deletedB2BCompanyLocationIds[locationId] = true;
  }

  listEffectiveB2BCompanies(): B2BCompanyRecord[] {
    return this.listOrderedB2BRecords(
      { ...this.baseState.b2bCompanies, ...this.stagedState.b2bCompanies },
      [...this.baseState.b2bCompanyOrder, ...this.stagedState.b2bCompanyOrder],
      this.stagedState.deletedB2BCompanyIds,
    );
  }

  getEffectiveB2BCompanyById(companyId: string): B2BCompanyRecord | null {
    if (this.stagedState.deletedB2BCompanyIds[companyId]) {
      return null;
    }
    const company = this.stagedState.b2bCompanies[companyId] ?? this.baseState.b2bCompanies[companyId] ?? null;
    return company ? structuredClone(company) : null;
  }

  listEffectiveB2BCompanyContacts(): B2BCompanyContactRecord[] {
    return this.listOrderedB2BRecords(
      { ...this.baseState.b2bCompanyContacts, ...this.stagedState.b2bCompanyContacts },
      [...this.baseState.b2bCompanyContactOrder, ...this.stagedState.b2bCompanyContactOrder],
      this.stagedState.deletedB2BCompanyContactIds,
    );
  }

  getEffectiveB2BCompanyContactById(contactId: string): B2BCompanyContactRecord | null {
    if (this.stagedState.deletedB2BCompanyContactIds[contactId]) {
      return null;
    }
    const contact =
      this.stagedState.b2bCompanyContacts[contactId] ?? this.baseState.b2bCompanyContacts[contactId] ?? null;
    return contact ? structuredClone(contact) : null;
  }

  listEffectiveB2BCompanyContactRoles(): B2BCompanyContactRoleRecord[] {
    return this.listOrderedB2BRecords(
      { ...this.baseState.b2bCompanyContactRoles, ...this.stagedState.b2bCompanyContactRoles },
      [...this.baseState.b2bCompanyContactRoleOrder, ...this.stagedState.b2bCompanyContactRoleOrder],
      this.stagedState.deletedB2BCompanyContactRoleIds,
    );
  }

  getEffectiveB2BCompanyContactRoleById(roleId: string): B2BCompanyContactRoleRecord | null {
    if (this.stagedState.deletedB2BCompanyContactRoleIds[roleId]) {
      return null;
    }
    const role =
      this.stagedState.b2bCompanyContactRoles[roleId] ?? this.baseState.b2bCompanyContactRoles[roleId] ?? null;
    return role ? structuredClone(role) : null;
  }

  listEffectiveB2BCompanyLocations(): B2BCompanyLocationRecord[] {
    return this.listOrderedB2BRecords(
      { ...this.baseState.b2bCompanyLocations, ...this.stagedState.b2bCompanyLocations },
      [...this.baseState.b2bCompanyLocationOrder, ...this.stagedState.b2bCompanyLocationOrder],
      this.stagedState.deletedB2BCompanyLocationIds,
    );
  }

  getEffectiveB2BCompanyLocationById(locationId: string): B2BCompanyLocationRecord | null {
    if (this.stagedState.deletedB2BCompanyLocationIds[locationId]) {
      return null;
    }
    const location =
      this.stagedState.b2bCompanyLocations[locationId] ?? this.baseState.b2bCompanyLocations[locationId] ?? null;
    return location ? structuredClone(location) : null;
  }

  private listOrderedB2BRecords<T extends { id: string }>(
    records: Record<string, T>,
    order: string[],
    deletedIds: Record<string, true> = {},
  ): T[] {
    const orderedIds = new Set(order);
    const orderedRecords = order
      .map((id) => records[id] ?? null)
      .filter((record): record is T => record !== null && !deletedIds[record.id]);
    const unorderedRecords = Object.values(records)
      .filter((record) => !orderedIds.has(record.id) && !deletedIds[record.id])
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedRecords, ...unorderedRecords]);
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

  upsertBaseCatalogs(catalogs: Array<CatalogRecord | { catalog: unknown; cursor?: string | null } | unknown>): void {
    for (const candidate of catalogs) {
      if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
        continue;
      }

      const entry = candidate as Record<string, unknown>;
      const rawCatalog = 'catalog' in entry ? entry['catalog'] : candidate;
      const rawCursor = 'cursor' in entry ? entry['cursor'] : null;
      if (!rawCatalog || typeof rawCatalog !== 'object' || Array.isArray(rawCatalog)) {
        continue;
      }

      const catalog = rawCatalog as Record<string, unknown>;
      const id = catalog['id'];
      if (typeof id === 'string' && id.length > 0) {
        delete this.baseState.deletedCatalogIds[id];
        delete this.stagedState.deletedCatalogIds[id];
        const previous = this.baseState.catalogs[id];
        const cursor = typeof rawCursor === 'string' && rawCursor.length > 0 ? rawCursor : (previous?.cursor ?? null);
        this.baseState.catalogs[id] = {
          id,
          cursor,
          data: previous
            ? ({ ...structuredClone(previous.data), ...structuredClone(catalog) } as CatalogRecord['data'])
            : (structuredClone(catalog) as CatalogRecord['data']),
        };

        if (!this.baseState.catalogOrder.includes(id)) {
          this.baseState.catalogOrder.push(id);
        }
      }
    }
  }

  getBaseCatalogRecordById(catalogId: string): CatalogRecord | null {
    const catalog = this.baseState.catalogs[catalogId] ?? null;
    return catalog ? structuredClone(catalog) : null;
  }

  getBaseCatalogById(catalogId: string): unknown | null {
    return this.getBaseCatalogRecordById(catalogId)?.data ?? null;
  }

  listBaseCatalogs(): CatalogRecord[] {
    const orderedIds = new Set(this.baseState.catalogOrder);
    const orderedCatalogs = this.baseState.catalogOrder
      .map((id) => this.baseState.catalogs[id] ?? null)
      .filter((catalog): catalog is CatalogRecord => catalog !== null);
    const unorderedCatalogs = Object.values(this.baseState.catalogs)
      .filter((catalog) => !orderedIds.has(catalog.id))
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedCatalogs, ...unorderedCatalogs]);
  }

  stageCreateCatalog(catalog: CatalogRecord): CatalogRecord {
    delete this.stagedState.deletedCatalogIds[catalog.id];
    this.stagedState.catalogs[catalog.id] = structuredClone(catalog);
    if (!this.stagedState.catalogOrder.includes(catalog.id)) {
      this.stagedState.catalogOrder.push(catalog.id);
    }
    return structuredClone(catalog);
  }

  stageUpdateCatalog(catalog: CatalogRecord): CatalogRecord {
    delete this.stagedState.deletedCatalogIds[catalog.id];
    this.stagedState.catalogs[catalog.id] = structuredClone(catalog);
    if (!this.baseState.catalogOrder.includes(catalog.id) && !this.stagedState.catalogOrder.includes(catalog.id)) {
      this.stagedState.catalogOrder.push(catalog.id);
    }
    return structuredClone(catalog);
  }

  stageDeleteCatalog(catalogId: string): void {
    delete this.stagedState.catalogs[catalogId];
    this.stagedState.catalogOrder = this.stagedState.catalogOrder.filter((id) => id !== catalogId);
    this.stagedState.deletedCatalogIds[catalogId] = true;
  }

  getEffectiveCatalogRecordById(catalogId: string): CatalogRecord | null {
    if (this.stagedState.deletedCatalogIds[catalogId]) {
      return null;
    }

    const catalog = this.stagedState.catalogs[catalogId] ?? this.baseState.catalogs[catalogId] ?? null;
    return catalog ? structuredClone(catalog) : null;
  }

  getEffectiveCatalogById(catalogId: string): unknown | null {
    return this.getEffectiveCatalogRecordById(catalogId)?.data ?? null;
  }

  listEffectiveCatalogs(): CatalogRecord[] {
    const mergedCatalogs = new Map<string, CatalogRecord>();
    const orderedIds = [...this.baseState.catalogOrder, ...this.stagedState.catalogOrder];

    for (const id of orderedIds) {
      const catalog = this.getEffectiveCatalogRecordById(id);
      if (catalog) {
        mergedCatalogs.set(id, catalog);
      }
    }

    for (const catalog of [...Object.values(this.baseState.catalogs), ...Object.values(this.stagedState.catalogs)]) {
      if (mergedCatalogs.has(catalog.id) || this.stagedState.deletedCatalogIds[catalog.id]) {
        continue;
      }
      mergedCatalogs.set(catalog.id, structuredClone(catalog));
    }

    return Array.from(mergedCatalogs.values());
  }

  upsertBasePriceLists(
    priceLists: Array<PriceListRecord | { priceList: unknown; cursor?: string | null } | unknown>,
  ): void {
    for (const candidate of priceLists) {
      if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
        continue;
      }

      const entry = candidate as Record<string, unknown>;
      const rawPriceList = 'priceList' in entry ? entry['priceList'] : candidate;
      const rawCursor = 'cursor' in entry ? entry['cursor'] : null;
      if (!rawPriceList || typeof rawPriceList !== 'object' || Array.isArray(rawPriceList)) {
        continue;
      }

      const priceList = rawPriceList as Record<string, unknown>;
      const id = priceList['id'];
      if (typeof id === 'string' && id.length > 0) {
        delete this.baseState.deletedPriceListIds[id];
        delete this.stagedState.deletedPriceListIds[id];
        const previous = this.baseState.priceLists[id];
        const cursor = typeof rawCursor === 'string' && rawCursor.length > 0 ? rawCursor : (previous?.cursor ?? null);
        this.baseState.priceLists[id] = {
          id,
          cursor,
          data: previous
            ? ({ ...structuredClone(previous.data), ...structuredClone(priceList) } as PriceListRecord['data'])
            : (structuredClone(priceList) as PriceListRecord['data']),
        };

        if (!this.baseState.priceListOrder.includes(id)) {
          this.baseState.priceListOrder.push(id);
        }
      }
    }
  }

  stageCreatePriceList(priceList: PriceListRecord): PriceListRecord {
    delete this.stagedState.deletedPriceListIds[priceList.id];
    this.stagedState.priceLists[priceList.id] = structuredClone(priceList);
    if (!this.stagedState.priceListOrder.includes(priceList.id)) {
      this.stagedState.priceListOrder.push(priceList.id);
    }
    return structuredClone(priceList);
  }

  stageUpdatePriceList(priceList: PriceListRecord): PriceListRecord {
    delete this.stagedState.deletedPriceListIds[priceList.id];
    this.stagedState.priceLists[priceList.id] = structuredClone(priceList);
    if (
      !this.baseState.priceListOrder.includes(priceList.id) &&
      !this.stagedState.priceListOrder.includes(priceList.id)
    ) {
      this.stagedState.priceListOrder.push(priceList.id);
    }
    return structuredClone(priceList);
  }

  stageDeletePriceList(priceListId: string): void {
    delete this.stagedState.priceLists[priceListId];
    this.stagedState.deletedPriceListIds[priceListId] = true;
  }

  upsertBaseDeliveryProfiles(deliveryProfiles: DeliveryProfileRecord[]): void {
    for (const profile of deliveryProfiles) {
      this.baseState.deliveryProfiles[profile.id] = structuredClone(profile);
      if (!this.baseState.deliveryProfileOrder.includes(profile.id)) {
        this.baseState.deliveryProfileOrder.push(profile.id);
      }
    }
  }

  stageCreateDeliveryProfile(profile: DeliveryProfileRecord): DeliveryProfileRecord {
    delete this.stagedState.deletedDeliveryProfileIds[profile.id];
    this.stagedState.deliveryProfiles[profile.id] = structuredClone(profile);
    if (!this.stagedState.deliveryProfileOrder.includes(profile.id)) {
      this.stagedState.deliveryProfileOrder.push(profile.id);
    }
    return structuredClone(profile);
  }

  stageUpdateDeliveryProfile(profile: DeliveryProfileRecord): DeliveryProfileRecord {
    delete this.stagedState.deletedDeliveryProfileIds[profile.id];
    this.stagedState.deliveryProfiles[profile.id] = structuredClone(profile);
    if (
      !this.baseState.deliveryProfileOrder.includes(profile.id) &&
      !this.stagedState.deliveryProfileOrder.includes(profile.id)
    ) {
      this.stagedState.deliveryProfileOrder.push(profile.id);
    }
    return structuredClone(profile);
  }

  stageDeleteDeliveryProfile(profileId: string): void {
    delete this.stagedState.deliveryProfiles[profileId];
    this.stagedState.deletedDeliveryProfileIds[profileId] = true;
  }

  getBaseDeliveryProfileById(profileId: string): DeliveryProfileRecord | null {
    const profile = this.baseState.deliveryProfiles[profileId] ?? null;
    return profile ? structuredClone(profile) : null;
  }

  getEffectiveDeliveryProfileById(profileId: string): DeliveryProfileRecord | null {
    if (this.stagedState.deletedDeliveryProfileIds[profileId] || this.baseState.deletedDeliveryProfileIds[profileId]) {
      return null;
    }

    const profile = this.stagedState.deliveryProfiles[profileId] ?? this.baseState.deliveryProfiles[profileId] ?? null;
    return profile ? structuredClone(profile) : null;
  }

  listBaseDeliveryProfiles(): DeliveryProfileRecord[] {
    const orderedIds = new Set(this.baseState.deliveryProfileOrder);
    const orderedProfiles = this.baseState.deliveryProfileOrder
      .map((id) => this.baseState.deliveryProfiles[id] ?? null)
      .filter((profile): profile is DeliveryProfileRecord => profile !== null);
    const unorderedProfiles = Object.values(this.baseState.deliveryProfiles)
      .filter((profile) => !orderedIds.has(profile.id))
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedProfiles, ...unorderedProfiles]);
  }

  listEffectiveDeliveryProfiles(): DeliveryProfileRecord[] {
    const orderedIds = new Set([...this.baseState.deliveryProfileOrder, ...this.stagedState.deliveryProfileOrder]);
    const orderedProfiles = [...orderedIds]
      .map((id) => this.getEffectiveDeliveryProfileById(id))
      .filter((profile): profile is DeliveryProfileRecord => profile !== null);
    const unorderedProfiles = Object.values({
      ...this.baseState.deliveryProfiles,
      ...this.stagedState.deliveryProfiles,
    })
      .filter((profile) => !orderedIds.has(profile.id))
      .map((profile) => this.getEffectiveDeliveryProfileById(profile.id))
      .filter((profile): profile is DeliveryProfileRecord => profile !== null)
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedProfiles, ...unorderedProfiles]);
  }

  hasStagedDeliveryProfiles(): boolean {
    return (
      Object.keys(this.stagedState.deliveryProfiles).length > 0 ||
      Object.keys(this.stagedState.deletedDeliveryProfileIds).length > 0
    );
  }

  upsertBaseApp(app: AppRecord): AppRecord {
    this.baseState.apps[app.id] = structuredClone(app);
    return structuredClone(app);
  }

  stageApp(app: AppRecord): AppRecord {
    this.stagedState.apps[app.id] = structuredClone(app);
    return structuredClone(app);
  }

  getEffectiveAppById(appId: string): AppRecord | null {
    const app = this.stagedState.apps[appId] ?? this.baseState.apps[appId] ?? null;
    return app ? structuredClone(app) : null;
  }

  findEffectiveAppByHandle(handle: string): AppRecord | null {
    const app =
      Object.values({ ...this.baseState.apps, ...this.stagedState.apps }).find(
        (candidate) => candidate.handle === handle,
      ) ?? null;
    return app ? structuredClone(app) : null;
  }

  findEffectiveAppByApiKey(apiKey: string): AppRecord | null {
    const app =
      Object.values({ ...this.baseState.apps, ...this.stagedState.apps }).find(
        (candidate) => candidate.apiKey === apiKey,
      ) ?? null;
    return app ? structuredClone(app) : null;
  }

  upsertBaseAppInstallation(installation: AppInstallationRecord, app: AppRecord): AppInstallationRecord {
    this.baseState.apps[app.id] = structuredClone(app);
    this.baseState.appInstallations[installation.id] = structuredClone(installation);
    this.baseState.currentAppInstallationId = installation.id;
    return structuredClone(installation);
  }

  stageAppInstallation(installation: AppInstallationRecord): AppInstallationRecord {
    this.stagedState.appInstallations[installation.id] = structuredClone(installation);
    this.stagedState.currentAppInstallationId = installation.id;
    return structuredClone(installation);
  }

  getCurrentAppInstallation(): AppInstallationRecord | null {
    const installationId = this.stagedState.currentAppInstallationId ?? this.baseState.currentAppInstallationId;
    if (!installationId) {
      return null;
    }

    return this.getEffectiveAppInstallationById(installationId);
  }

  getEffectiveAppInstallationById(installationId: string): AppInstallationRecord | null {
    const installation =
      this.stagedState.appInstallations[installationId] ?? this.baseState.appInstallations[installationId] ?? null;
    if (!installation || installation.uninstalledAt !== null) {
      return null;
    }

    return structuredClone(installation);
  }

  stageAppSubscription(subscription: AppSubscriptionRecord): AppSubscriptionRecord {
    this.stagedState.appSubscriptions[subscription.id] = structuredClone(subscription);
    return structuredClone(subscription);
  }

  getEffectiveAppSubscriptionById(subscriptionId: string): AppSubscriptionRecord | null {
    const subscription =
      this.stagedState.appSubscriptions[subscriptionId] ?? this.baseState.appSubscriptions[subscriptionId] ?? null;
    return subscription ? structuredClone(subscription) : null;
  }

  stageAppSubscriptionLineItem(lineItem: AppSubscriptionLineItemRecord): AppSubscriptionLineItemRecord {
    this.stagedState.appSubscriptionLineItems[lineItem.id] = structuredClone(lineItem);
    return structuredClone(lineItem);
  }

  getEffectiveAppSubscriptionLineItemById(lineItemId: string): AppSubscriptionLineItemRecord | null {
    const lineItem =
      this.stagedState.appSubscriptionLineItems[lineItemId] ??
      this.baseState.appSubscriptionLineItems[lineItemId] ??
      null;
    return lineItem ? structuredClone(lineItem) : null;
  }

  findEffectiveAppSubscriptionByLineItemId(lineItemId: string): AppSubscriptionRecord | null {
    const lineItem = this.getEffectiveAppSubscriptionLineItemById(lineItemId);
    return lineItem ? this.getEffectiveAppSubscriptionById(lineItem.subscriptionId) : null;
  }

  stageAppOneTimePurchase(purchase: AppOneTimePurchaseRecord): AppOneTimePurchaseRecord {
    this.stagedState.appOneTimePurchases[purchase.id] = structuredClone(purchase);
    return structuredClone(purchase);
  }

  getEffectiveAppOneTimePurchaseById(purchaseId: string): AppOneTimePurchaseRecord | null {
    const purchase =
      this.stagedState.appOneTimePurchases[purchaseId] ?? this.baseState.appOneTimePurchases[purchaseId] ?? null;
    return purchase ? structuredClone(purchase) : null;
  }

  stageAppUsageRecord(record: AppUsageRecord): AppUsageRecord {
    this.stagedState.appUsageRecords[record.id] = structuredClone(record);
    return structuredClone(record);
  }

  listEffectiveAppUsageRecordsForLineItem(lineItemId: string): AppUsageRecord[] {
    return Object.values({ ...this.baseState.appUsageRecords, ...this.stagedState.appUsageRecords })
      .filter((record) => record.subscriptionLineItemId === lineItemId)
      .sort((left, right) => left.createdAt.localeCompare(right.createdAt) || left.id.localeCompare(right.id))
      .map((record) => structuredClone(record));
  }

  stageDelegatedAccessToken(record: DelegatedAccessTokenRecord): DelegatedAccessTokenRecord {
    this.stagedState.delegatedAccessTokens[record.id] = structuredClone(record);
    return structuredClone(record);
  }

  findDelegatedAccessTokenByHash(accessTokenSha256: string): DelegatedAccessTokenRecord | null {
    const token =
      Object.values({ ...this.baseState.delegatedAccessTokens, ...this.stagedState.delegatedAccessTokens }).find(
        (candidate) => candidate.accessTokenSha256 === accessTokenSha256 && candidate.destroyedAt === null,
      ) ?? null;
    return token ? structuredClone(token) : null;
  }

  destroyDelegatedAccessToken(id: string, destroyedAt: string): DelegatedAccessTokenRecord | null {
    const token = this.stagedState.delegatedAccessTokens[id] ?? this.baseState.delegatedAccessTokens[id] ?? null;
    if (!token || token.destroyedAt !== null) {
      return null;
    }

    const destroyed = { ...token, destroyedAt };
    this.stagedState.delegatedAccessTokens[id] = structuredClone(destroyed);
    return structuredClone(destroyed);
  }

  hasAppDomainState(): boolean {
    return (
      this.baseState.currentAppInstallationId !== null ||
      this.stagedState.currentAppInstallationId !== null ||
      Object.keys(this.baseState.apps).length > 0 ||
      Object.keys(this.stagedState.apps).length > 0 ||
      Object.keys(this.baseState.appSubscriptions).length > 0 ||
      Object.keys(this.stagedState.appSubscriptions).length > 0 ||
      Object.keys(this.baseState.appOneTimePurchases).length > 0 ||
      Object.keys(this.stagedState.appOneTimePurchases).length > 0 ||
      Object.keys(this.baseState.delegatedAccessTokens).length > 0 ||
      Object.keys(this.stagedState.delegatedAccessTokens).length > 0
    );
  }

  getBasePriceListRecordById(priceListId: string): PriceListRecord | null {
    const priceList = this.baseState.priceLists[priceListId] ?? null;
    return priceList ? structuredClone(priceList) : null;
  }

  getBasePriceListById(priceListId: string): unknown | null {
    return this.getBasePriceListRecordById(priceListId)?.data ?? null;
  }

  getEffectivePriceListRecordById(priceListId: string): PriceListRecord | null {
    if (this.stagedState.deletedPriceListIds[priceListId] || this.baseState.deletedPriceListIds[priceListId]) {
      return null;
    }

    const priceList = this.stagedState.priceLists[priceListId] ?? this.baseState.priceLists[priceListId] ?? null;
    return priceList ? structuredClone(priceList) : null;
  }

  getEffectivePriceListById(priceListId: string): unknown | null {
    return this.getEffectivePriceListRecordById(priceListId)?.data ?? null;
  }

  listBasePriceLists(): PriceListRecord[] {
    const orderedIds = new Set(this.baseState.priceListOrder);
    const orderedPriceLists = this.baseState.priceListOrder
      .map((id) => this.baseState.priceLists[id] ?? null)
      .filter((priceList): priceList is PriceListRecord => priceList !== null);
    const unorderedPriceLists = Object.values(this.baseState.priceLists)
      .filter((priceList) => !orderedIds.has(priceList.id))
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedPriceLists, ...unorderedPriceLists]);
  }

  listEffectivePriceLists(): PriceListRecord[] {
    const orderedIds = new Set([...this.baseState.priceListOrder, ...this.stagedState.priceListOrder]);
    const orderedPriceLists = [...orderedIds]
      .map((id) => this.getEffectivePriceListRecordById(id))
      .filter((priceList): priceList is PriceListRecord => priceList !== null);
    const unorderedPriceLists = Object.values({
      ...this.baseState.priceLists,
      ...this.stagedState.priceLists,
    })
      .filter((priceList) => !orderedIds.has(priceList.id))
      .map((priceList) => this.getEffectivePriceListRecordById(priceList.id))
      .filter((priceList): priceList is PriceListRecord => priceList !== null)
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedPriceLists, ...unorderedPriceLists]);
  }

  hasStagedPriceLists(): boolean {
    return (
      Object.keys(this.stagedState.priceLists).length > 0 ||
      Object.keys(this.stagedState.deletedPriceListIds).length > 0
    );
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

  upsertBaseWebPresences(
    webPresences: Array<WebPresenceRecord | { webPresence: unknown; cursor?: string | null } | unknown>,
  ): void {
    for (const candidate of webPresences) {
      if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
        continue;
      }

      const entry = candidate as Record<string, unknown>;
      const rawWebPresence = 'webPresence' in entry ? entry['webPresence'] : candidate;
      const rawCursor = 'cursor' in entry ? entry['cursor'] : null;
      if (!rawWebPresence || typeof rawWebPresence !== 'object' || Array.isArray(rawWebPresence)) {
        continue;
      }

      const webPresence = rawWebPresence as Record<string, unknown>;
      const id = webPresence['id'];
      if (typeof id === 'string' && id.length > 0) {
        delete this.baseState.deletedWebPresenceIds[id];
        delete this.stagedState.deletedWebPresenceIds[id];
        const previous = this.baseState.webPresences[id];
        const cursor = typeof rawCursor === 'string' && rawCursor.length > 0 ? rawCursor : (previous?.cursor ?? null);
        this.baseState.webPresences[id] = {
          id,
          cursor,
          data: previous
            ? ({ ...structuredClone(previous.data), ...structuredClone(webPresence) } as WebPresenceRecord['data'])
            : (structuredClone(webPresence) as WebPresenceRecord['data']),
        };

        if (!this.baseState.webPresenceOrder.includes(id)) {
          this.baseState.webPresenceOrder.push(id);
        }
      }
    }
  }

  stageCreateWebPresence(webPresence: WebPresenceRecord): WebPresenceRecord {
    delete this.stagedState.deletedWebPresenceIds[webPresence.id];
    this.stagedState.webPresences[webPresence.id] = structuredClone(webPresence);
    if (!this.stagedState.webPresenceOrder.includes(webPresence.id)) {
      this.stagedState.webPresenceOrder.push(webPresence.id);
    }
    return structuredClone(webPresence);
  }

  stageUpdateWebPresence(webPresence: WebPresenceRecord): WebPresenceRecord {
    delete this.stagedState.deletedWebPresenceIds[webPresence.id];
    this.stagedState.webPresences[webPresence.id] = structuredClone(webPresence);
    if (
      !this.baseState.webPresenceOrder.includes(webPresence.id) &&
      !this.stagedState.webPresenceOrder.includes(webPresence.id)
    ) {
      this.stagedState.webPresenceOrder.push(webPresence.id);
    }
    return structuredClone(webPresence);
  }

  stageDeleteWebPresence(webPresenceId: string): void {
    delete this.stagedState.webPresences[webPresenceId];
    this.stagedState.webPresenceOrder = this.stagedState.webPresenceOrder.filter((id) => id !== webPresenceId);
    this.stagedState.deletedWebPresenceIds[webPresenceId] = true;
  }

  isWebPresenceDeleted(webPresenceId: string): boolean {
    return this.stagedState.deletedWebPresenceIds[webPresenceId] === true;
  }

  getEffectiveWebPresenceRecordById(webPresenceId: string): WebPresenceRecord | null {
    if (this.stagedState.deletedWebPresenceIds[webPresenceId]) {
      return null;
    }

    const webPresence =
      this.stagedState.webPresences[webPresenceId] ?? this.baseState.webPresences[webPresenceId] ?? null;
    return webPresence ? structuredClone(webPresence) : null;
  }

  getEffectiveWebPresenceById(webPresenceId: string): unknown | null {
    return this.getEffectiveWebPresenceRecordById(webPresenceId)?.data ?? null;
  }

  listEffectiveWebPresences(): WebPresenceRecord[] {
    const mergedWebPresences = new Map<string, WebPresenceRecord>();
    const orderedIds = [...this.baseState.webPresenceOrder, ...this.stagedState.webPresenceOrder];

    for (const id of orderedIds) {
      const webPresence = this.getEffectiveWebPresenceRecordById(id);
      if (webPresence) {
        mergedWebPresences.set(id, webPresence);
      }
    }

    for (const webPresence of [
      ...Object.values(this.baseState.webPresences),
      ...Object.values(this.stagedState.webPresences),
    ]) {
      if (mergedWebPresences.has(webPresence.id) || this.stagedState.deletedWebPresenceIds[webPresence.id]) {
        continue;
      }
      mergedWebPresences.set(webPresence.id, structuredClone(webPresence));
    }

    return Array.from(mergedWebPresences.values());
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
      Object.keys(this.stagedState.markets).length > 0 ||
      Object.keys(this.stagedState.deletedMarketIds).length > 0 ||
      Object.keys(this.stagedState.catalogs).length > 0 ||
      Object.keys(this.stagedState.deletedCatalogIds).length > 0 ||
      Object.keys(this.stagedState.webPresences).length > 0 ||
      Object.keys(this.stagedState.deletedWebPresenceIds).length > 0 ||
      Object.keys(this.stagedState.marketLocalizations).length > 0
    );
  }

  stageMarketLocalization(localization: MarketLocalizationRecord): MarketLocalizationRecord {
    this.stagedState.marketLocalizations[marketLocalizationStorageKey(localization)] = structuredClone(localization);
    return structuredClone(localization);
  }

  removeMarketLocalization(resourceId: string, marketId: string, key: string): MarketLocalizationRecord | null {
    const storageKey = marketLocalizationStorageKey({ resourceId, marketId, key });
    const existing = this.stagedState.marketLocalizations[storageKey] ?? this.baseState.marketLocalizations[storageKey];
    delete this.stagedState.marketLocalizations[storageKey];
    return existing ? structuredClone(existing) : null;
  }

  listEffectiveMarketLocalizations(resourceId: string, marketId: string): MarketLocalizationRecord[] {
    const merged = new Map<string, MarketLocalizationRecord>();
    for (const localization of Object.values(this.baseState.marketLocalizations)) {
      if (localization.resourceId === resourceId && localization.marketId === marketId) {
        merged.set(marketLocalizationStorageKey(localization), structuredClone(localization));
      }
    }
    for (const localization of Object.values(this.stagedState.marketLocalizations)) {
      if (localization.resourceId === resourceId && localization.marketId === marketId) {
        merged.set(marketLocalizationStorageKey(localization), structuredClone(localization));
      }
    }
    return Array.from(merged.values()).sort(
      (left, right) => left.key.localeCompare(right.key) || left.updatedAt.localeCompare(right.updatedAt),
    );
  }

  replaceBaseAvailableLocales(locales: LocaleRecord[]): void {
    this.baseState.availableLocales = structuredClone(locales);
  }

  listEffectiveAvailableLocales(): LocaleRecord[] {
    return structuredClone(this.baseState.availableLocales);
  }

  upsertBaseShopLocales(locales: ShopLocaleRecord[]): void {
    for (const locale of locales) {
      delete this.baseState.deletedShopLocales[locale.locale];
      delete this.stagedState.deletedShopLocales[locale.locale];
      this.baseState.shopLocales[locale.locale] = structuredClone(locale);
    }
  }

  stageShopLocale(locale: ShopLocaleRecord): ShopLocaleRecord {
    delete this.stagedState.deletedShopLocales[locale.locale];
    this.stagedState.shopLocales[locale.locale] = structuredClone(locale);
    return structuredClone(locale);
  }

  disableShopLocale(locale: string): ShopLocaleRecord | null {
    const existing = this.stagedState.shopLocales[locale] ?? this.baseState.shopLocales[locale] ?? null;
    delete this.stagedState.shopLocales[locale];
    if (existing) {
      this.stagedState.deletedShopLocales[locale] = true;
    }
    return existing ? structuredClone(existing) : null;
  }

  getEffectiveShopLocale(locale: string): ShopLocaleRecord | null {
    if (this.stagedState.deletedShopLocales[locale]) {
      return null;
    }

    const record = this.stagedState.shopLocales[locale] ?? this.baseState.shopLocales[locale] ?? null;
    return record ? structuredClone(record) : null;
  }

  listEffectiveShopLocales(published?: boolean | null): ShopLocaleRecord[] {
    const locales = new Map<string, ShopLocaleRecord>();
    for (const locale of Object.values(this.baseState.shopLocales)) {
      if (!this.stagedState.deletedShopLocales[locale.locale]) {
        locales.set(locale.locale, structuredClone(locale));
      }
    }
    for (const locale of Object.values(this.stagedState.shopLocales)) {
      if (!this.stagedState.deletedShopLocales[locale.locale]) {
        locales.set(locale.locale, structuredClone(locale));
      }
    }

    return Array.from(locales.values())
      .filter((locale) => (typeof published === 'boolean' ? locale.published === published : true))
      .sort((left, right) => Number(right.primary) - Number(left.primary) || left.locale.localeCompare(right.locale));
  }

  stageTranslation(translation: TranslationRecord): TranslationRecord {
    const storageKey = translationStorageKey(translation);
    delete this.stagedState.deletedTranslations[storageKey];
    this.stagedState.translations[storageKey] = structuredClone(translation);
    return structuredClone(translation);
  }

  removeTranslation(
    resourceId: string,
    locale: string,
    key: string,
    marketId: string | null = null,
  ): TranslationRecord | null {
    const storageKey = translationStorageKey({ resourceId, locale, key, marketId });
    const existing = this.stagedState.translations[storageKey] ?? this.baseState.translations[storageKey] ?? null;
    delete this.stagedState.translations[storageKey];
    if (existing) {
      this.stagedState.deletedTranslations[storageKey] = true;
    }
    return existing ? structuredClone(existing) : null;
  }

  listEffectiveTranslations(resourceId: string, locale: string, marketId: string | null = null): TranslationRecord[] {
    const translations = new Map<string, TranslationRecord>();
    for (const translation of Object.values(this.baseState.translations)) {
      const storageKey = translationStorageKey(translation);
      if (
        translation.resourceId === resourceId &&
        translation.locale === locale &&
        (translation.marketId ?? null) === marketId &&
        !this.stagedState.deletedTranslations[storageKey]
      ) {
        translations.set(storageKey, structuredClone(translation));
      }
    }
    for (const translation of Object.values(this.stagedState.translations)) {
      if (
        translation.resourceId === resourceId &&
        translation.locale === locale &&
        (translation.marketId ?? null) === marketId
      ) {
        translations.set(translationStorageKey(translation), structuredClone(translation));
      }
    }

    return Array.from(translations.values()).sort(
      (left, right) => left.key.localeCompare(right.key) || left.updatedAt.localeCompare(right.updatedAt),
    );
  }

  hasLocalizationState(): boolean {
    return (
      this.baseState.availableLocales.length > 0 ||
      Object.keys(this.baseState.shopLocales).length > 0 ||
      Object.keys(this.stagedState.shopLocales).length > 0 ||
      Object.keys(this.stagedState.deletedShopLocales).length > 0 ||
      Object.keys(this.baseState.translations).length > 0 ||
      Object.keys(this.stagedState.translations).length > 0 ||
      Object.keys(this.stagedState.deletedTranslations).length > 0
    );
  }

  hasStagedLocalizationState(): boolean {
    return (
      Object.keys(this.stagedState.shopLocales).length > 0 ||
      Object.keys(this.stagedState.deletedShopLocales).length > 0 ||
      Object.keys(this.stagedState.translations).length > 0 ||
      Object.keys(this.stagedState.deletedTranslations).length > 0
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

  stageUpsertCustomerAddress(address: CustomerAddressRecord): CustomerAddressRecord {
    delete this.stagedState.deletedCustomerAddressIds[address.id];
    this.stagedState.customerAddresses[address.id] = structuredClone(address);
    return structuredClone(address);
  }

  stageUpsertCustomerPaymentMethod(paymentMethod: CustomerPaymentMethodRecord): CustomerPaymentMethodRecord {
    delete this.stagedState.deletedCustomerPaymentMethodIds[paymentMethod.id];
    this.stagedState.customerPaymentMethods[paymentMethod.id] = structuredClone(paymentMethod);
    return structuredClone(paymentMethod);
  }

  stageCustomerPaymentMethodUpdateUrl(
    updateUrl: CustomerPaymentMethodUpdateUrlRecord,
  ): CustomerPaymentMethodUpdateUrlRecord {
    this.stagedState.customerPaymentMethodUpdateUrls[updateUrl.id] = structuredClone(updateUrl);
    return structuredClone(updateUrl);
  }

  stagePaymentReminderSend(reminderSend: PaymentReminderSendRecord): PaymentReminderSendRecord {
    this.stagedState.paymentReminderSends[reminderSend.id] = structuredClone(reminderSend);
    return structuredClone(reminderSend);
  }

  stageStoreCreditAccount(account: StoreCreditAccountRecord): StoreCreditAccountRecord {
    this.stagedState.storeCreditAccounts[account.id] = structuredClone(account);
    return structuredClone(account);
  }

  stageStoreCreditAccountTransaction(
    transaction: StoreCreditAccountTransactionRecord,
  ): StoreCreditAccountTransactionRecord {
    this.stagedState.storeCreditAccountTransactions[transaction.id] = structuredClone(transaction);
    return structuredClone(transaction);
  }

  stageDeleteCustomerAddress(addressId: string): void {
    delete this.stagedState.customerAddresses[addressId];
    this.stagedState.deletedCustomerAddressIds[addressId] = true;
  }

  stageDeleteCustomer(customerId: string): void {
    delete this.stagedState.customers[customerId];
    delete this.stagedState.mergedCustomerIds[customerId];
    for (const address of Object.values(this.stagedState.customerAddresses)) {
      if (address.customerId === customerId) {
        delete this.stagedState.customerAddresses[address.id];
      }
    }
    for (const address of Object.values(this.baseState.customerAddresses)) {
      if (address.customerId === customerId) {
        this.stagedState.deletedCustomerAddressIds[address.id] = true;
      }
    }
    this.stagedState.deletedCustomerIds[customerId] = true;
  }

  upsertBaseCustomerAccountPages(pages: CustomerAccountPageRecord[]): void {
    for (const page of pages) {
      this.baseState.customerAccountPages[page.id] = structuredClone(page);
      if (!this.baseState.customerAccountPageOrder.includes(page.id)) {
        this.baseState.customerAccountPageOrder.push(page.id);
      }
    }
  }

  getEffectiveCustomerAccountPageById(pageId: string): CustomerAccountPageRecord | null {
    return structuredClone(this.baseState.customerAccountPages[pageId] ?? null);
  }

  listEffectiveCustomerAccountPages(): CustomerAccountPageRecord[] {
    const orderedIds = [
      ...this.baseState.customerAccountPageOrder,
      ...Object.keys(this.baseState.customerAccountPages).filter(
        (pageId) => !this.baseState.customerAccountPageOrder.includes(pageId),
      ),
    ];
    return orderedIds.flatMap((pageId) => {
      const page = this.baseState.customerAccountPages[pageId];
      return page ? [structuredClone(page)] : [];
    });
  }

  hasCustomerAccountPages(): boolean {
    return Object.keys(this.baseState.customerAccountPages).length > 0;
  }

  stageCustomerDataErasureRequest(request: CustomerDataErasureRequestRecord): CustomerDataErasureRequestRecord {
    this.stagedState.customerDataErasureRequests[request.customerId] = structuredClone(request);
    return structuredClone(request);
  }

  stageCustomerDataErasureCancellation(customerId: string, canceledAt: string): CustomerDataErasureRequestRecord {
    const existing =
      this.stagedState.customerDataErasureRequests[customerId] ??
      this.baseState.customerDataErasureRequests[customerId];
    const request = existing
      ? { ...existing, canceledAt }
      : {
          customerId,
          requestedAt: canceledAt,
          canceledAt,
        };
    this.stagedState.customerDataErasureRequests[customerId] = structuredClone(request);
    return structuredClone(request);
  }

  stageCreateDiscount(discount: DiscountRecord): DiscountRecord {
    delete this.stagedState.deletedDiscountIds[discount.id];
    this.stagedState.discounts[discount.id] = structuredClone(discount);
    return structuredClone(discount);
  }

  stageDiscountBulkOperation(operation: DiscountBulkOperationRecord): DiscountBulkOperationRecord {
    this.stagedState.discountBulkOperations[operation.id] = structuredClone(operation);
    return structuredClone(operation);
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

  upsertBaseAbandonedCheckouts(checkouts: AbandonedCheckoutRecord[]): void {
    for (const checkout of checkouts) {
      this.baseState.abandonedCheckouts[checkout.id] = structuredClone(checkout);
      if (!this.baseState.abandonedCheckoutOrder.includes(checkout.id)) {
        this.baseState.abandonedCheckoutOrder.push(checkout.id);
      }
    }
  }

  upsertBaseAbandonments(abandonments: AbandonmentRecord[]): void {
    for (const abandonment of abandonments) {
      this.baseState.abandonments[abandonment.id] = structuredClone(abandonment);
      if (!this.baseState.abandonmentOrder.includes(abandonment.id)) {
        this.baseState.abandonmentOrder.push(abandonment.id);
      }
    }
  }

  getAbandonedCheckoutById(checkoutId: string): AbandonedCheckoutRecord | null {
    const checkout = this.stagedState.abandonedCheckouts[checkoutId] ?? this.baseState.abandonedCheckouts[checkoutId];
    return checkout ? structuredClone(checkout) : null;
  }

  getAbandonmentById(abandonmentId: string): AbandonmentRecord | null {
    const abandonment = this.stagedState.abandonments[abandonmentId] ?? this.baseState.abandonments[abandonmentId];
    return abandonment ? structuredClone(abandonment) : null;
  }

  getAbandonmentByAbandonedCheckoutId(checkoutId: string): AbandonmentRecord | null {
    return this.getAbandonments().find((abandonment) => abandonment.abandonedCheckoutId === checkoutId) ?? null;
  }

  getAbandonedCheckouts(): AbandonedCheckoutRecord[] {
    const mergedCheckouts = new Map<string, AbandonedCheckoutRecord>();
    for (const id of [...this.baseState.abandonedCheckoutOrder, ...this.stagedState.abandonedCheckoutOrder]) {
      const checkout = this.getAbandonedCheckoutById(id);
      if (checkout) {
        mergedCheckouts.set(id, checkout);
      }
    }
    for (const checkout of [
      ...Object.values(this.baseState.abandonedCheckouts),
      ...Object.values(this.stagedState.abandonedCheckouts),
    ]) {
      if (!mergedCheckouts.has(checkout.id)) {
        mergedCheckouts.set(checkout.id, structuredClone(checkout));
      }
    }
    return Array.from(mergedCheckouts.values()).sort((left, right) => {
      const leftCreatedAt = typeof left.data['createdAt'] === 'string' ? left.data['createdAt'] : '';
      const rightCreatedAt = typeof right.data['createdAt'] === 'string' ? right.data['createdAt'] : '';
      return rightCreatedAt.localeCompare(leftCreatedAt) || compareShopifyResourceIds(right.id, left.id);
    });
  }

  getAbandonments(): AbandonmentRecord[] {
    const mergedAbandonments = new Map<string, AbandonmentRecord>();
    for (const id of [...this.baseState.abandonmentOrder, ...this.stagedState.abandonmentOrder]) {
      const abandonment = this.getAbandonmentById(id);
      if (abandonment) {
        mergedAbandonments.set(id, abandonment);
      }
    }
    for (const abandonment of [
      ...Object.values(this.baseState.abandonments),
      ...Object.values(this.stagedState.abandonments),
    ]) {
      if (!mergedAbandonments.has(abandonment.id)) {
        mergedAbandonments.set(abandonment.id, structuredClone(abandonment));
      }
    }
    return Array.from(mergedAbandonments.values()).sort((left, right) => {
      const leftCreatedAt = typeof left.data['createdAt'] === 'string' ? left.data['createdAt'] : '';
      const rightCreatedAt = typeof right.data['createdAt'] === 'string' ? right.data['createdAt'] : '';
      return rightCreatedAt.localeCompare(leftCreatedAt) || compareShopifyResourceIds(right.id, left.id);
    });
  }

  stageAbandonmentDeliveryActivity(
    abandonmentId: string,
    activity: AbandonmentDeliveryActivityRecord,
  ): AbandonmentRecord | null {
    const abandonment = this.getAbandonmentById(abandonmentId);
    if (!abandonment) {
      return null;
    }

    const staged: AbandonmentRecord = {
      ...abandonment,
      data: {
        ...structuredClone(abandonment.data),
        emailState: activity.deliveryStatus,
        ...(activity.deliveredAt ? { emailSentAt: activity.deliveredAt } : {}),
      },
      deliveryActivities: {
        ...structuredClone(abandonment.deliveryActivities),
        [activity.marketingActivityId]: structuredClone(activity),
      },
    };
    this.stagedState.abandonments[staged.id] = structuredClone(staged);
    if (!this.stagedState.abandonmentOrder.includes(staged.id)) {
      this.stagedState.abandonmentOrder.push(staged.id);
    }
    return structuredClone(staged);
  }

  upsertBaseOrders(orders: OrderRecord[]): void {
    for (const order of orders) {
      this.baseOrders[order.id] = structuredClone(order);
    }
  }

  stageCreateOrder(order: OrderRecord): OrderRecord {
    this.deletedOrderIds.delete(order.id);
    this.stagedOrders[order.id] = structuredClone(order);
    return structuredClone(order);
  }

  getOrderById(orderId: string): OrderRecord | null {
    if (this.deletedOrderIds.has(orderId)) {
      return null;
    }
    const order = this.stagedOrders[orderId] ?? this.baseOrders[orderId];
    return order ? structuredClone(order) : null;
  }

  hasDeletedOrder(orderId: string): boolean {
    return this.deletedOrderIds.has(orderId);
  }

  hasStagedOrder(orderId: string): boolean {
    return Object.prototype.hasOwnProperty.call(this.stagedOrders, orderId);
  }

  getOrders(): OrderRecord[] {
    const mergedOrders = new Map<string, OrderRecord>();
    for (const order of Object.values(this.baseOrders)) {
      if (this.deletedOrderIds.has(order.id)) {
        continue;
      }
      mergedOrders.set(order.id, structuredClone(order));
    }
    for (const order of Object.values(this.stagedOrders)) {
      if (this.deletedOrderIds.has(order.id)) {
        continue;
      }
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
    this.deletedOrderIds.delete(order.id);
    this.stagedOrders[order.id] = structuredClone(order);
    return structuredClone(order);
  }

  deleteOrder(orderId: string): void {
    delete this.stagedOrders[orderId];
    this.deletedOrderIds.add(orderId);
  }

  stageOrderMandatePayment(record: OrderMandatePaymentRecord): OrderMandatePaymentRecord {
    this.orderMandatePayments[`${record.orderId}::${record.idempotencyKey}`] = structuredClone(record);
    return structuredClone(record);
  }

  getOrderMandatePayment(orderId: string, idempotencyKey: string): OrderMandatePaymentRecord | null {
    const record = this.orderMandatePayments[`${orderId}::${idempotencyKey}`] ?? null;
    return record ? structuredClone(record) : null;
  }

  stageCreateDraftOrder(draftOrder: DraftOrderRecord): DraftOrderRecord {
    this.deletedDraftOrderIds.delete(draftOrder.id);
    this.stagedDraftOrders[draftOrder.id] = structuredClone(draftOrder);
    return structuredClone(draftOrder);
  }

  updateDraftOrder(draftOrder: DraftOrderRecord): DraftOrderRecord {
    this.deletedDraftOrderIds.delete(draftOrder.id);
    this.stagedDraftOrders[draftOrder.id] = structuredClone(draftOrder);
    return structuredClone(draftOrder);
  }

  deleteDraftOrder(draftOrderId: string): void {
    delete this.stagedDraftOrders[draftOrderId];
    this.deletedDraftOrderIds.add(draftOrderId);
  }

  getDraftOrderById(draftOrderId: string): DraftOrderRecord | null {
    if (this.deletedDraftOrderIds.has(draftOrderId)) {
      return null;
    }
    const draftOrder = this.stagedDraftOrders[draftOrderId];
    return draftOrder ? structuredClone(draftOrder) : null;
  }

  hasDeletedDraftOrder(draftOrderId: string): boolean {
    return this.deletedDraftOrderIds.has(draftOrderId);
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
      delete this.baseState.deletedPublicationIds[publication.id];
      delete this.stagedState.deletedPublicationIds[publication.id];
      this.baseState.publications[publication.id] = structuredClone(publication);
    }
  }

  upsertBaseChannels(channels: ChannelRecord[]): void {
    for (const channel of channels) {
      this.baseState.channels[channel.id] = structuredClone(channel);
    }
  }

  stageCreatePublication(publication: PublicationRecord): PublicationRecord {
    delete this.stagedState.deletedPublicationIds[publication.id];
    this.stagedState.publications[publication.id] = structuredClone(publication);
    return structuredClone(publication);
  }

  stageUpdatePublication(publication: PublicationRecord): PublicationRecord {
    delete this.stagedState.deletedPublicationIds[publication.id];
    this.stagedState.publications[publication.id] = structuredClone(publication);
    return structuredClone(publication);
  }

  stageDeletePublication(publicationId: string): void {
    delete this.stagedState.publications[publicationId];
    this.stagedState.deletedPublicationIds[publicationId] = true;
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
    for (const metafield of Object.values(this.stagedState.productMetafields)) {
      if (readProductMetafieldOwnerId(metafield) === collectionId) {
        delete this.stagedState.productMetafields[metafield.id];
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

  stageProductOperation(operation: ProductOperationRecord): ProductOperationRecord {
    this.stagedState.productOperations[operation.id] = structuredClone(operation);
    return structuredClone(operation);
  }

  upsertStagedProductFeed(productFeed: ProductFeedRecord): ProductFeedRecord {
    delete this.stagedState.deletedProductFeedIds[productFeed.id];
    this.stagedState.productFeeds[productFeed.id] = structuredClone(productFeed);
    return structuredClone(productFeed);
  }

  deleteStagedProductFeed(productFeedId: string): void {
    delete this.stagedState.productFeeds[productFeedId];
    this.stagedState.deletedProductFeedIds[productFeedId] = true;
  }

  upsertStagedProductResourceFeedback(feedback: ProductResourceFeedbackRecord): ProductResourceFeedbackRecord {
    this.stagedState.productResourceFeedback[feedback.productId] = structuredClone(feedback);
    return structuredClone(feedback);
  }

  upsertStagedShopResourceFeedback(feedback: ShopResourceFeedbackRecord): ShopResourceFeedbackRecord {
    this.stagedState.shopResourceFeedback[feedback.id] = structuredClone(feedback);
    return structuredClone(feedback);
  }

  replaceStagedBundleComponentsForProduct(productId: string, components: ProductBundleComponentRecord[]): void {
    for (const [componentId, component] of Object.entries(this.stagedState.productBundleComponents)) {
      if (component.bundleProductId === productId) {
        delete this.stagedState.productBundleComponents[componentId];
      }
    }

    for (const component of components) {
      this.stagedState.productBundleComponents[component.id] = structuredClone(component);
    }
  }

  replaceStagedVariantComponentsForParentVariant(
    parentProductVariantId: string,
    components: ProductVariantComponentRecord[],
  ): void {
    for (const [componentId, component] of Object.entries(this.stagedState.productVariantComponents)) {
      if (component.parentProductVariantId === parentProductVariantId) {
        delete this.stagedState.productVariantComponents[componentId];
      }
    }

    for (const component of components) {
      this.stagedState.productVariantComponents[component.id] = structuredClone(component);
    }
  }

  replaceStagedCombinedListingChildren(parentProductId: string, children: CombinedListingChildRecord[]): void {
    for (const [storageKey, child] of Object.entries(this.stagedState.combinedListingChildren)) {
      if (child.parentProductId === parentProductId) {
        delete this.stagedState.combinedListingChildren[storageKey];
      }
    }

    for (const child of children) {
      this.stagedState.combinedListingChildren[`${child.parentProductId}:${child.childProductId}`] =
        structuredClone(child);
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

  listEffectiveFiles(): FileRecord[] {
    const byId = new Map<string, FileRecord>();

    for (const file of Object.values(this.baseState.files)) {
      if (!this.stagedState.deletedFileIds[file.id]) {
        byId.set(file.id, structuredClone(file));
      }
    }

    for (const file of Object.values(this.stagedState.files)) {
      if (!this.stagedState.deletedFileIds[file.id]) {
        byId.set(file.id, structuredClone(file));
      }
    }

    return [...byId.values()];
  }

  replaceBaseMetafieldsForOwner(ownerId: string, metafields: ProductMetafieldRecord[]): void {
    for (const metafield of Object.values(this.baseState.productMetafields)) {
      if (readProductMetafieldOwnerId(metafield) === ownerId) {
        delete this.baseState.productMetafields[metafield.id];
      }
    }

    for (const metafield of metafields) {
      this.baseState.productMetafields[metafield.id] = structuredClone(metafield);
    }
  }

  upsertBaseMetafieldDefinitions(definitions: MetafieldDefinitionRecord[]): void {
    for (const definition of definitions) {
      delete this.baseState.deletedMetafieldDefinitionIds[definition.id];
      delete this.stagedState.deletedMetafieldDefinitionIds[definition.id];
      this.baseState.metafieldDefinitions[definition.id] = structuredClone(definition);
    }
  }

  upsertStagedMetafieldDefinitions(definitions: MetafieldDefinitionRecord[]): void {
    for (const definition of definitions) {
      delete this.stagedState.deletedMetafieldDefinitionIds[definition.id];
      this.stagedState.metafieldDefinitions[definition.id] = structuredClone(definition);
    }
  }

  stageDeleteMetafieldDefinition(definitionId: string): void {
    delete this.stagedState.metafieldDefinitions[definitionId];
    this.stagedState.deletedMetafieldDefinitionIds[definitionId] = true;
  }

  deleteProductMetafieldsForDefinition(definition: { ownerType: string; namespace: string; key: string }): void {
    if (definition.ownerType !== 'PRODUCT') {
      return;
    }

    for (const metafields of [this.baseState.productMetafields, this.stagedState.productMetafields]) {
      for (const [metafieldId, metafield] of Object.entries(metafields)) {
        const ownerType = metafield.ownerType ?? (metafield.productId ? 'PRODUCT' : null);
        if (
          ownerType === 'PRODUCT' &&
          metafield.namespace === definition.namespace &&
          metafield.key === definition.key
        ) {
          delete metafields[metafieldId];
        }
      }
    }
  }

  upsertBaseMetaobjectDefinitions(definitions: MetaobjectDefinitionRecord[]): void {
    for (const definition of definitions) {
      this.baseState.metaobjectDefinitions[definition.id] = structuredClone(definition);
    }
  }

  upsertStagedMetaobjectDefinitions(definitions: MetaobjectDefinitionRecord[]): void {
    for (const definition of definitions) {
      delete this.stagedState.deletedMetaobjectDefinitionIds[definition.id];
      this.stagedState.metaobjectDefinitions[definition.id] = structuredClone(definition);
    }
  }

  upsertBaseMetaobjects(metaobjects: MetaobjectRecord[]): void {
    for (const metaobject of metaobjects) {
      delete this.baseState.deletedMetaobjectIds[metaobject.id];
      delete this.stagedState.deletedMetaobjectIds[metaobject.id];
      this.baseState.metaobjects[metaobject.id] = structuredClone(metaobject);
    }
  }

  upsertStagedMetaobjects(metaobjects: MetaobjectRecord[]): void {
    for (const metaobject of metaobjects) {
      delete this.stagedState.deletedMetaobjectIds[metaobject.id];
      this.stagedState.metaobjects[metaobject.id] = structuredClone(metaobject);
    }
  }

  deleteStagedMetaobjectDefinition(definitionId: string): void {
    delete this.stagedState.metaobjectDefinitions[definitionId];
    this.stagedState.deletedMetaobjectDefinitionIds[definitionId] = true;
  }

  deleteStagedMetaobject(metaobjectId: string): void {
    delete this.stagedState.metaobjects[metaobjectId];
    this.stagedState.deletedMetaobjectIds[metaobjectId] = true;
  }

  replaceBaseMetafieldsForProduct(productId: string, metafields: ProductMetafieldRecord[]): void {
    this.replaceBaseMetafieldsForOwner(productId, metafields);
  }

  replaceStagedMetafieldsForOwner(ownerId: string, metafields: ProductMetafieldRecord[]): void {
    for (const metafield of Object.values(this.stagedState.productMetafields)) {
      if (readProductMetafieldOwnerId(metafield) === ownerId) {
        delete this.stagedState.productMetafields[metafield.id];
      }
    }

    for (const metafield of metafields) {
      this.stagedState.productMetafields[metafield.id] = structuredClone(metafield);
    }
  }

  replaceStagedMetafieldsForProduct(productId: string, metafields: ProductMetafieldRecord[]): void {
    this.replaceStagedMetafieldsForOwner(productId, metafields);
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
    const variantIds = new Set(
      [...Object.values(this.baseState.productVariants), ...Object.values(this.stagedState.productVariants)]
        .filter((variant) => variant.productId === productId)
        .map((variant) => variant.id),
    );
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
      const ownerId = readProductMetafieldOwnerId(metafield);
      if (ownerId === productId || (ownerId ? variantIds.has(ownerId) : false)) {
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

  getEffectiveCustomerAddressById(addressId: string): CustomerAddressRecord | null {
    if (this.stagedState.deletedCustomerAddressIds[addressId]) {
      return null;
    }

    const address =
      this.stagedState.customerAddresses[addressId] ?? this.baseState.customerAddresses[addressId] ?? null;
    return address ? structuredClone(address) : null;
  }

  getEffectiveCustomerPaymentMethodById(
    paymentMethodId: string,
    options: { showRevoked?: boolean } = {},
  ): CustomerPaymentMethodRecord | null {
    if (this.stagedState.deletedCustomerPaymentMethodIds[paymentMethodId]) {
      return null;
    }

    const paymentMethod =
      this.stagedState.customerPaymentMethods[paymentMethodId] ??
      this.baseState.customerPaymentMethods[paymentMethodId] ??
      null;
    if (!paymentMethod || this.stagedState.deletedCustomerIds[paymentMethod.customerId]) {
      return null;
    }

    if (paymentMethod.revokedAt && options.showRevoked !== true) {
      return null;
    }

    return structuredClone(paymentMethod);
  }

  getEffectiveStoreCreditAccountById(accountId: string): StoreCreditAccountRecord | null {
    const account = this.stagedState.storeCreditAccounts[accountId] ?? this.baseState.storeCreditAccounts[accountId];
    if (!account || this.stagedState.deletedCustomerIds[account.customerId]) {
      return null;
    }

    return structuredClone(account);
  }

  listEffectiveCustomerAddresses(customerId: string): CustomerAddressRecord[] {
    if (this.stagedState.deletedCustomerIds[customerId]) {
      return [];
    }

    const addressIds = new Set([
      ...Object.keys(this.baseState.customerAddresses),
      ...Object.keys(this.stagedState.customerAddresses),
    ]);
    const addresses: CustomerAddressRecord[] = [];

    for (const addressId of Array.from(addressIds)) {
      if (this.stagedState.deletedCustomerAddressIds[addressId]) {
        continue;
      }

      const address = this.stagedState.customerAddresses[addressId] ?? this.baseState.customerAddresses[addressId];
      if (address?.customerId === customerId) {
        addresses.push(structuredClone(address));
      }
    }

    return addresses.sort(compareCustomerAddresses);
  }

  listEffectiveCustomerPaymentMethods(
    customerId: string,
    options: { showRevoked?: boolean } = {},
  ): CustomerPaymentMethodRecord[] {
    if (this.stagedState.deletedCustomerIds[customerId]) {
      return [];
    }

    const paymentMethodIds = new Set([
      ...Object.keys(this.baseState.customerPaymentMethods),
      ...Object.keys(this.stagedState.customerPaymentMethods),
    ]);
    const paymentMethods: CustomerPaymentMethodRecord[] = [];

    for (const paymentMethodId of Array.from(paymentMethodIds)) {
      const paymentMethod = this.getEffectiveCustomerPaymentMethodById(paymentMethodId, options);
      if (paymentMethod?.customerId === customerId) {
        paymentMethods.push(paymentMethod);
      }
    }

    return paymentMethods.sort(
      (left, right) =>
        (left.cursor ?? left.id).localeCompare(right.cursor ?? right.id) || left.id.localeCompare(right.id),
    );
  }

  listEffectiveStoreCreditAccountsForCustomer(customerId: string): StoreCreditAccountRecord[] {
    if (this.stagedState.deletedCustomerIds[customerId]) {
      return [];
    }

    const accountIds = new Set([
      ...Object.keys(this.baseState.storeCreditAccounts),
      ...Object.keys(this.stagedState.storeCreditAccounts),
    ]);
    const accounts: StoreCreditAccountRecord[] = [];

    for (const accountId of Array.from(accountIds)) {
      const account = this.getEffectiveStoreCreditAccountById(accountId);
      if (account?.customerId === customerId) {
        accounts.push(account);
      }
    }

    return accounts.sort(
      (left, right) =>
        (left.cursor ?? left.id).localeCompare(right.cursor ?? right.id) || left.id.localeCompare(right.id),
    );
  }

  listEffectiveStoreCreditAccountTransactions(accountId: string): StoreCreditAccountTransactionRecord[] {
    const transactionIds = new Set([
      ...Object.keys(this.baseState.storeCreditAccountTransactions),
      ...Object.keys(this.stagedState.storeCreditAccountTransactions),
    ]);
    const transactions: StoreCreditAccountTransactionRecord[] = [];

    for (const transactionId of Array.from(transactionIds)) {
      const transaction =
        this.stagedState.storeCreditAccountTransactions[transactionId] ??
        this.baseState.storeCreditAccountTransactions[transactionId];
      if (transaction?.accountId === accountId) {
        transactions.push(structuredClone(transaction));
      }
    }

    return transactions.sort(
      (left, right) => right.createdAt.localeCompare(left.createdAt) || right.id.localeCompare(left.id),
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

  getEffectivePaymentCustomizationById(paymentCustomizationId: string): PaymentCustomizationRecord | null {
    if (this.stagedState.deletedPaymentCustomizationIds[paymentCustomizationId]) {
      return null;
    }

    const customization =
      this.stagedState.paymentCustomizations[paymentCustomizationId] ??
      this.baseState.paymentCustomizations[paymentCustomizationId] ??
      null;
    return customization ? structuredClone(customization) : null;
  }

  listEffectivePaymentCustomizations(): PaymentCustomizationRecord[] {
    const orderedIds = new Set([
      ...this.baseState.paymentCustomizationOrder,
      ...this.stagedState.paymentCustomizationOrder,
    ]);
    const orderedCustomizations = Array.from(orderedIds)
      .map((id) => this.getEffectivePaymentCustomizationById(id))
      .filter((customization): customization is PaymentCustomizationRecord => customization !== null);
    const unorderedCustomizations = Object.values({
      ...this.baseState.paymentCustomizations,
      ...this.stagedState.paymentCustomizations,
    })
      .filter((customization) => !orderedIds.has(customization.id))
      .filter((customization) => !this.stagedState.deletedPaymentCustomizationIds[customization.id])
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedCustomizations, ...unorderedCustomizations]);
  }

  getEffectivePaymentTermsTemplateById(paymentTermsTemplateId: string): PaymentTermsTemplateRecord | null {
    const template = this.baseState.paymentTermsTemplates[paymentTermsTemplateId] ?? null;
    return template ? structuredClone(template) : null;
  }

  listEffectivePaymentTermsTemplates(): PaymentTermsTemplateRecord[] {
    const orderedIds = new Set(this.baseState.paymentTermsTemplateOrder);
    const orderedTemplates = Array.from(orderedIds)
      .map((id) => this.baseState.paymentTermsTemplates[id] ?? null)
      .filter((template): template is PaymentTermsTemplateRecord => template !== null);
    const unorderedTemplates = Object.values(this.baseState.paymentTermsTemplates)
      .filter((template) => !orderedIds.has(template.id))
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));

    return structuredClone([...orderedTemplates, ...unorderedTemplates]);
  }

  hasBaseCustomers(): boolean {
    return Object.keys(this.baseState.customers).length > 0;
  }

  hasCustomerPaymentMethods(): boolean {
    return (
      Object.keys(this.baseState.customerPaymentMethods).length > 0 ||
      Object.keys(this.stagedState.customerPaymentMethods).length > 0 ||
      Object.keys(this.stagedState.deletedCustomerPaymentMethodIds).length > 0
    );
  }

  hasStagedCustomers(): boolean {
    return (
      Object.keys(this.stagedState.customers).length > 0 ||
      Object.keys(this.stagedState.customerAddresses).length > 0 ||
      Object.keys(this.stagedState.customerMetafields).length > 0 ||
      Object.keys(this.stagedState.deletedCustomerIds).length > 0 ||
      Object.keys(this.stagedState.deletedCustomerAddressIds).length > 0 ||
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

  hasPaymentCustomizations(): boolean {
    return (
      Object.keys(this.baseState.paymentCustomizations).length > 0 ||
      Object.keys(this.stagedState.paymentCustomizations).length > 0 ||
      Object.keys(this.stagedState.deletedPaymentCustomizationIds).length > 0
    );
  }

  hasPaymentTermsTemplates(): boolean {
    return Object.keys(this.baseState.paymentTermsTemplates).length > 0;
  }

  hasFunctionMetadata(): boolean {
    return (
      Object.keys(this.baseState.shopifyFunctions).length > 0 ||
      Object.keys(this.stagedState.shopifyFunctions).length > 0 ||
      Object.keys(this.baseState.validations).length > 0 ||
      Object.keys(this.stagedState.validations).length > 0 ||
      Object.keys(this.stagedState.deletedValidationIds).length > 0 ||
      Object.keys(this.baseState.cartTransforms).length > 0 ||
      Object.keys(this.stagedState.cartTransforms).length > 0 ||
      Object.keys(this.stagedState.deletedCartTransformIds).length > 0 ||
      this.baseState.taxAppConfiguration !== null ||
      this.stagedState.taxAppConfiguration !== null
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

  getEffectiveProductOperationById(operationId: string): ProductOperationRecord | null {
    const operation = this.stagedState.productOperations[operationId] ?? this.baseState.productOperations[operationId];
    return operation ? structuredClone(operation) : null;
  }

  getEffectiveProductFeedById(productFeedId: string): ProductFeedRecord | null {
    if (this.stagedState.deletedProductFeedIds[productFeedId]) {
      return null;
    }

    const productFeed = this.stagedState.productFeeds[productFeedId] ?? this.baseState.productFeeds[productFeedId];
    return productFeed ? structuredClone(productFeed) : null;
  }

  listEffectiveProductFeeds(): ProductFeedRecord[] {
    const feedIds = new Set([
      ...Object.keys(this.baseState.productFeeds),
      ...Object.keys(this.stagedState.productFeeds),
    ]);
    return Array.from(feedIds)
      .filter((feedId) => !this.stagedState.deletedProductFeedIds[feedId])
      .map((feedId) => this.stagedState.productFeeds[feedId] ?? this.baseState.productFeeds[feedId])
      .filter((productFeed): productFeed is ProductFeedRecord => productFeed !== undefined)
      .map((productFeed) => structuredClone(productFeed))
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));
  }

  getEffectiveProductResourceFeedback(productId: string): ProductResourceFeedbackRecord | null {
    const feedback =
      this.stagedState.productResourceFeedback[productId] ?? this.baseState.productResourceFeedback[productId];
    return feedback ? structuredClone(feedback) : null;
  }

  getEffectiveBundleComponentsByProductId(productId: string): ProductBundleComponentRecord[] {
    const componentIds = new Set([
      ...Object.keys(this.baseState.productBundleComponents),
      ...Object.keys(this.stagedState.productBundleComponents),
    ]);
    return Array.from(componentIds)
      .map(
        (componentId) =>
          this.stagedState.productBundleComponents[componentId] ?? this.baseState.productBundleComponents[componentId],
      )
      .filter(
        (component): component is ProductBundleComponentRecord =>
          component !== undefined && component.bundleProductId === productId,
      )
      .map((component) => structuredClone(component))
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));
  }

  getEffectiveVariantComponentsByParentVariantId(parentProductVariantId: string): ProductVariantComponentRecord[] {
    const componentIds = new Set([
      ...Object.keys(this.baseState.productVariantComponents),
      ...Object.keys(this.stagedState.productVariantComponents),
    ]);
    return Array.from(componentIds)
      .map(
        (componentId) =>
          this.stagedState.productVariantComponents[componentId] ??
          this.baseState.productVariantComponents[componentId],
      )
      .filter(
        (component): component is ProductVariantComponentRecord =>
          component !== undefined && component.parentProductVariantId === parentProductVariantId,
      )
      .map((component) => structuredClone(component))
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));
  }

  getEffectiveCombinedListingChildrenByParentId(parentProductId: string): CombinedListingChildRecord[] {
    const storageKeys = new Set([
      ...Object.keys(this.baseState.combinedListingChildren),
      ...Object.keys(this.stagedState.combinedListingChildren),
    ]);
    return Array.from(storageKeys)
      .map(
        (storageKey) =>
          this.stagedState.combinedListingChildren[storageKey] ?? this.baseState.combinedListingChildren[storageKey],
      )
      .filter(
        (child): child is CombinedListingChildRecord =>
          child !== undefined && child.parentProductId === parentProductId,
      )
      .map((child) => structuredClone(child))
      .sort((left, right) => compareShopifyResourceIds(left.childProductId, right.childProductId));
  }

  getEffectiveCombinedListingParentByChildId(childProductId: string): CombinedListingChildRecord | null {
    return (
      Object.values({
        ...this.baseState.combinedListingChildren,
        ...this.stagedState.combinedListingChildren,
      }).find((child) => child.childProductId === childProductId) ?? null
    );
  }

  getEffectiveInventoryTransferById(transferId: string): InventoryTransferRecord | null {
    if (this.stagedState.deletedInventoryTransferIds[transferId]) {
      return null;
    }

    const transfer = this.stagedState.inventoryTransfers[transferId] ?? this.baseState.inventoryTransfers[transferId];
    return transfer ? structuredClone(transfer) : null;
  }

  listEffectiveInventoryTransfers(): InventoryTransferRecord[] {
    const seenIds = new Set<string>();
    const orderedTransfers: InventoryTransferRecord[] = [];
    for (const id of [...this.baseState.inventoryTransferOrder, ...this.stagedState.inventoryTransferOrder]) {
      if (seenIds.has(id) || this.stagedState.deletedInventoryTransferIds[id]) {
        continue;
      }

      const transfer = this.getEffectiveInventoryTransferById(id);
      if (transfer) {
        orderedTransfers.push(transfer);
        seenIds.add(id);
      }
    }

    const unorderedTransfers = [
      ...Object.values(this.baseState.inventoryTransfers),
      ...Object.values(this.stagedState.inventoryTransfers),
    ]
      .filter((transfer) => !seenIds.has(transfer.id) && !this.stagedState.deletedInventoryTransferIds[transfer.id])
      .map((transfer) => structuredClone(transfer));

    return [...orderedTransfers, ...unorderedTransfers];
  }

  upsertStagedInventoryTransfer(transfer: InventoryTransferRecord): void {
    this.stagedState.inventoryTransfers[transfer.id] = structuredClone(transfer);
    delete this.stagedState.deletedInventoryTransferIds[transfer.id];
    if (
      !this.baseState.inventoryTransferOrder.includes(transfer.id) &&
      !this.stagedState.inventoryTransferOrder.includes(transfer.id)
    ) {
      this.stagedState.inventoryTransferOrder.push(transfer.id);
    }
  }

  deleteStagedInventoryTransfer(transferId: string): void {
    delete this.stagedState.inventoryTransfers[transferId];
    this.stagedState.deletedInventoryTransferIds[transferId] = true;
    this.stagedState.inventoryTransferOrder = this.stagedState.inventoryTransferOrder.filter((id) => id !== transferId);
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
      if (this.stagedState.deletedPublicationIds[publicationId]) {
        continue;
      }

      const publication = mergePublicationRecord(
        this.baseState.publications[publicationId] ?? null,
        this.stagedState.publications[publicationId] ?? null,
      );
      if (publication) {
        publicationsById.set(publication.id, publication);
      }
    }

    for (const publicationId of Object.keys(this.stagedState.publications)) {
      if (this.stagedState.deletedPublicationIds[publicationId] || publicationsById.has(publicationId)) {
        continue;
      }

      const publication = mergePublicationRecord(null, this.stagedState.publications[publicationId] ?? null);
      if (publication) {
        publicationsById.set(publication.id, publication);
      }
    }

    for (const product of this.listEffectiveProducts()) {
      for (const publicationId of product.publicationIds) {
        if (
          !isInternalPublicationPlaceholder(publicationId) &&
          !this.stagedState.deletedPublicationIds[publicationId] &&
          !publicationsById.has(publicationId)
        ) {
          publicationsById.set(publicationId, {
            id: publicationId,
            name: null,
          });
        }
      }
    }

    for (const collection of this.listEffectiveCollections()) {
      for (const publicationId of collection.publicationIds ?? []) {
        if (
          !isInternalPublicationPlaceholder(publicationId) &&
          !this.stagedState.deletedPublicationIds[publicationId] &&
          !publicationsById.has(publicationId)
        ) {
          publicationsById.set(publicationId, {
            id: publicationId,
            name: null,
          });
        }
      }
    }

    return Array.from(publicationsById.values()).sort((left, right) => compareShopifyResourceIds(left.id, right.id));
  }

  getEffectivePublicationById(publicationId: string): PublicationRecord | null {
    if (this.stagedState.deletedPublicationIds[publicationId]) {
      return null;
    }

    return mergePublicationRecord(
      this.baseState.publications[publicationId] ?? null,
      this.stagedState.publications[publicationId] ?? null,
    );
  }

  listEffectiveChannels(): ChannelRecord[] {
    const channelsById = new Map<string, ChannelRecord>();

    for (const channel of Object.values(this.baseState.channels)) {
      channelsById.set(channel.id, structuredClone(channel));
    }

    for (const publication of this.listEffectivePublications()) {
      const channel = channelFromPublication(publication);
      if (channel && !channelsById.has(channel.id)) {
        channelsById.set(channel.id, channel);
      }
    }

    return Array.from(channelsById.values()).sort((left, right) => compareShopifyResourceIds(left.id, right.id));
  }

  getEffectiveChannelById(channelId: string): ChannelRecord | null {
    const direct = this.baseState.channels[channelId] ?? null;
    if (direct) {
      return structuredClone(direct);
    }

    const publication = this.getEffectivePublicationById(channelId);
    const derivedChannel = publication ? channelFromPublication(publication) : null;
    if (derivedChannel) {
      return derivedChannel;
    }

    return this.listEffectiveChannels().find((channel) => channel.id === channelId) ?? null;
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

  getEffectiveMetafieldsByOwnerId(ownerId: string): ProductMetafieldRecord[] {
    if (this.stagedState.deletedProductIds[ownerId] || this.stagedState.deletedCollectionIds[ownerId]) {
      return [];
    }

    const stagedMetafields = Object.values(this.stagedState.productMetafields)
      .filter((metafield) => readProductMetafieldOwnerId(metafield) === ownerId)
      .map((metafield) => structuredClone(metafield));

    const sourceMetafields =
      stagedMetafields.length > 0
        ? stagedMetafields
        : Object.values(this.baseState.productMetafields)
            .filter((metafield) => readProductMetafieldOwnerId(metafield) === ownerId)
            .map((metafield) => structuredClone(metafield));

    return sourceMetafields.sort((left, right) => {
      const leftAppNamespace = left.namespace.startsWith('app--');
      const rightAppNamespace = right.namespace.startsWith('app--');
      if (leftAppNamespace !== rightAppNamespace) {
        return leftAppNamespace ? 1 : -1;
      }

      return (
        left.namespace.localeCompare(right.namespace) ||
        left.key.localeCompare(right.key) ||
        left.id.localeCompare(right.id)
      );
    });
  }

  listEffectiveMetafieldDefinitions(): MetafieldDefinitionRecord[] {
    const definitionsById = new Map<string, MetafieldDefinitionRecord>();

    for (const definition of Object.values(this.baseState.metafieldDefinitions)) {
      if (this.stagedState.deletedMetafieldDefinitionIds[definition.id]) {
        continue;
      }

      definitionsById.set(definition.id, structuredClone(definition));
    }

    for (const definition of Object.values(this.stagedState.metafieldDefinitions)) {
      if (this.stagedState.deletedMetafieldDefinitionIds[definition.id]) {
        continue;
      }

      definitionsById.set(definition.id, structuredClone(definition));
    }

    return [...definitionsById.values()].sort(
      (left, right) =>
        left.ownerType.localeCompare(right.ownerType) ||
        left.namespace.localeCompare(right.namespace) ||
        left.key.localeCompare(right.key) ||
        compareShopifyResourceIds(left.id, right.id),
    );
  }

  getEffectiveMetafieldDefinitionById(definitionId: string): MetafieldDefinitionRecord | null {
    if (this.stagedState.deletedMetafieldDefinitionIds[definitionId]) {
      return null;
    }

    const definition =
      this.stagedState.metafieldDefinitions[definitionId] ?? this.baseState.metafieldDefinitions[definitionId];
    return definition ? structuredClone(definition) : null;
  }

  findEffectiveMetafieldDefinition(identifier: {
    ownerType: string;
    namespace: string;
    key: string;
  }): MetafieldDefinitionRecord | null {
    const definition =
      this.listEffectiveMetafieldDefinitions().find(
        (candidate) =>
          candidate.ownerType === identifier.ownerType &&
          candidate.namespace === identifier.namespace &&
          candidate.key === identifier.key,
      ) ?? null;
    return definition ? structuredClone(definition) : null;
  }

  listEffectiveMetaobjectDefinitions(): MetaobjectDefinitionRecord[] {
    const definitionsById = new Map<string, MetaobjectDefinitionRecord>();

    for (const definition of Object.values(this.baseState.metaobjectDefinitions)) {
      if (this.stagedState.deletedMetaobjectDefinitionIds[definition.id]) {
        continue;
      }
      definitionsById.set(definition.id, structuredClone(definition));
    }

    for (const definition of Object.values(this.stagedState.metaobjectDefinitions)) {
      definitionsById.set(definition.id, structuredClone(definition));
    }

    return [...definitionsById.values()].sort(
      (left, right) => left.type.localeCompare(right.type) || compareShopifyResourceIds(left.id, right.id),
    );
  }

  getEffectiveMetaobjectDefinitionById(definitionId: string): MetaobjectDefinitionRecord | null {
    if (this.stagedState.deletedMetaobjectDefinitionIds[definitionId]) {
      return null;
    }

    const definition =
      this.stagedState.metaobjectDefinitions[definitionId] ?? this.baseState.metaobjectDefinitions[definitionId];
    return definition ? structuredClone(definition) : null;
  }

  findEffectiveMetaobjectDefinitionByType(type: string): MetaobjectDefinitionRecord | null {
    const definition = this.listEffectiveMetaobjectDefinitions().find((candidate) => candidate.type === type) ?? null;
    return definition ? structuredClone(definition) : null;
  }

  listEffectiveMetaobjects(): MetaobjectRecord[] {
    const metaobjectsById = new Map<string, MetaobjectRecord>();

    for (const metaobject of Object.values(this.baseState.metaobjects)) {
      if (this.stagedState.deletedMetaobjectIds[metaobject.id]) {
        continue;
      }
      metaobjectsById.set(metaobject.id, structuredClone(metaobject));
    }

    for (const metaobject of Object.values(this.stagedState.metaobjects)) {
      if (this.stagedState.deletedMetaobjectIds[metaobject.id]) {
        continue;
      }
      metaobjectsById.set(metaobject.id, structuredClone(metaobject));
    }

    return [...metaobjectsById.values()].sort(
      (left, right) =>
        left.type.localeCompare(right.type) ||
        left.handle.localeCompare(right.handle) ||
        compareShopifyResourceIds(left.id, right.id),
    );
  }

  getEffectiveMetaobjectById(metaobjectId: string): MetaobjectRecord | null {
    if (this.stagedState.deletedMetaobjectIds[metaobjectId]) {
      return null;
    }

    const metaobject = this.stagedState.metaobjects[metaobjectId] ?? this.baseState.metaobjects[metaobjectId];
    return metaobject ? structuredClone(metaobject) : null;
  }

  findEffectiveMetaobjectByHandle(identifier: { type: string; handle: string }): MetaobjectRecord | null {
    const metaobject =
      this.listEffectiveMetaobjects().find(
        (candidate) => candidate.type === identifier.type && candidate.handle === identifier.handle,
      ) ?? null;
    return metaobject ? structuredClone(metaobject) : null;
  }

  listEffectiveMetaobjectsByType(type: string): MetaobjectRecord[] {
    return this.listEffectiveMetaobjects().filter((metaobject) => metaobject.type === type);
  }

  hasEffectiveMetaobjectDefinitions(): boolean {
    return (
      Object.keys(this.baseState.metaobjectDefinitions).length > 0 ||
      Object.keys(this.stagedState.metaobjectDefinitions).length > 0 ||
      Object.keys(this.stagedState.deletedMetaobjectDefinitionIds).length > 0
    );
  }

  hasStagedMetaobjectDefinitions(): boolean {
    return Object.keys(this.stagedState.metaobjectDefinitions).length > 0;
  }

  hasEffectiveMetaobjects(): boolean {
    return (
      Object.keys(this.baseState.metaobjects).length > 0 ||
      Object.keys(this.stagedState.metaobjects).length > 0 ||
      Object.keys(this.stagedState.deletedMetaobjectIds).length > 0
    );
  }

  hasStagedMetaobjects(): boolean {
    return (
      Object.keys(this.stagedState.metaobjects).length > 0 ||
      Object.keys(this.stagedState.deletedMetaobjectIds).length > 0
    );
  }

  getEffectiveMetafieldsByProductId(productId: string): ProductMetafieldRecord[] {
    return this.getEffectiveMetafieldsByOwnerId(productId);
  }

  getEffectiveMetafieldsByCustomerId(customerId: string): CustomerMetafieldRecord[] {
    if (this.stagedState.deletedCustomerIds[customerId]) {
      return [];
    }

    const stagedMetafields = Object.values(this.stagedState.customerMetafields)
      .filter((metafield) => metafield.customerId === customerId)
      .map((metafield) => structuredClone(metafield));

    if (stagedMetafields.length > 0) {
      return stagedMetafields;
    }

    return Object.values(this.baseState.customerMetafields)
      .filter((metafield) => metafield.customerId === customerId)
      .map((metafield) => structuredClone(metafield))
      .sort((left, right) => compareShopifyResourceIds(left.id, right.id));
  }

  hasStagedProducts(): boolean {
    return (
      Object.keys(this.stagedState.products).length > 0 ||
      Object.keys(this.stagedState.collections).length > 0 ||
      Object.keys(this.stagedState.productVariants).length > 0 ||
      Object.keys(this.stagedState.productOptions).length > 0 ||
      Object.keys(this.stagedState.productFeeds).length > 0 ||
      Object.keys(this.stagedState.productResourceFeedback).length > 0 ||
      Object.keys(this.stagedState.shopResourceFeedback).length > 0 ||
      Object.keys(this.stagedState.productBundleComponents).length > 0 ||
      Object.keys(this.stagedState.productVariantComponents).length > 0 ||
      Object.keys(this.stagedState.combinedListingChildren).length > 0 ||
      Object.keys(this.stagedState.productCollections).length > 0 ||
      Object.keys(this.stagedState.publications).length > 0 ||
      Object.keys(this.stagedState.channels).length > 0 ||
      this.stagedCollectionFamilies.size > 0 ||
      Object.keys(this.stagedState.productMedia).length > 0 ||
      this.stagedMediaFamilies.size > 0 ||
      Object.keys(this.stagedState.productMetafields).length > 0 ||
      Object.keys(this.stagedState.metafieldDefinitions).length > 0 ||
      Object.keys(this.stagedState.deletedMetafieldDefinitionIds).length > 0 ||
      Object.keys(this.stagedState.deletedProductIds).length > 0 ||
      Object.keys(this.stagedState.deletedProductFeedIds).length > 0 ||
      Object.keys(this.stagedState.deletedCollectionIds).length > 0 ||
      Object.keys(this.stagedState.deletedPublicationIds).length > 0
    );
  }

  hasStagedInventoryTransfers(): boolean {
    return (
      Object.keys(this.stagedState.inventoryTransfers).length > 0 ||
      Object.keys(this.stagedState.deletedInventoryTransferIds).length > 0
    );
  }
}

export const store = new InMemoryStore();
