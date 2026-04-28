import type { ProxyRuntimeContext } from './runtime-context.js';
import { getLocation, Kind, type FieldNode, type SelectionNode } from 'graphql';
import type { ReadMode } from '../config.js';
import { getFieldArguments, getRootField, getRootFieldArguments, getRootFields } from '../graphql/root-field.js';
import { jsonValueSchema, type JsonValue } from '../json-schemas.js';
import {
  applySearchQuery,
  matchesSearchQueryString,
  parseSearchQuery,
  searchQueryTermValue,
  stripSearchQueryValueQuotes,
  type SearchQueryNode,
  type SearchQueryTerm,
} from '../search-query-parser.js';
import { DEFAULT_ADMIN_API_VERSION } from '../shopify/api-version.js';
import {
  buildMissingIdempotencyKeyError,
  getNodeLocation,
  getVariableDefinitionLocation,
  paginateConnectionItems,
  projectGraphqlObject,
  projectGraphqlValue,
  readIdempotencyKey,
  readPlainObjectArray,
  serializeConnection,
  type GraphqlErrorLocation,
} from './graphql-helpers.js';
import {
  getOperationPathLabel,
  hasOwnField,
  isObject,
  readLegacyResourceIdFromGid,
  stripHtmlToDescription,
} from './products/helpers.js';
import {
  ensureUniqueProductHandle,
  findEffectiveProductByHandle,
  prepareProductInputWithResolvedHandle,
  slugifyHandle,
} from './products/handles.js';
import {
  addInventoryQuantityAmount,
  buildStableSyntheticInventoryLevelId,
  DEFAULT_INVENTORY_LEVEL_LOCATION_ID,
  readInventoryQuantityAmount,
  sumAvailableInventoryLevels,
  writeInventoryQuantityAmount,
} from './products/inventory-quantities.js';
import {
  buildInvalidProductMediaContentTypeVariableError,
  buildInvalidProductMediaProductIdVariableError,
  CREATE_MEDIA_CONTENT_TYPES,
  isValidMediaSource,
  makeCreatedMediaRecord,
  mediaValidationProductNotFoundPayload,
  transitionMediaToProcessing,
  transitionMediaToReady,
  updateMediaRecord,
} from './products/media.js';
import { makeMetafieldCompareDigest, parseMetafieldJsonValue } from './products/metafield-values.js';
import {
  buildProductSetOptionRecords,
  deleteOptionRecords,
  deriveVariantTitle,
  insertOptionAtPosition,
  makeCreatedOptionRecord,
  makeDefaultOptionRecord,
  makeDefaultVariantRecord,
  normalizeOptionPositions,
  productUsesOnlyDefaultOptionState,
  remapDefaultVariantToCreatedOptions,
  reorderVariantSelectionsForOptions,
  restoreDefaultOptionState,
  updateOptionRecords,
} from './products/options.js';
import { serializeCountValue, serializeJobSelectionSet } from './products/serializers.js';
import { serializeLocation as serializeStorePropertiesLocation } from './store-properties.js';
import {
  normalizeOwnerMetafield,
  readMetafieldInputObjects,
  serializeMetafieldsConnection as serializeOwnerMetafieldsConnection,
  serializeMetafieldSelectionSet,
  upsertOwnerMetafields,
} from './metafields.js';
import type {
  CollectionImageRecord,
  CollectionRecord,
  CollectionRuleSetRecord,
  ChannelRecord,
  InventoryLevelRecord,
  LocationRecord,
  ProductCatalogConnectionRecord,
  ProductCollectionRecord,
  ProductMediaRecord,
  MetafieldDefinitionRecord,
  ProductMetafieldRecord,
  ProductOptionRecord,
  ProductOptionValueRecord,
  ProductOperationRecord,
  ProductRecord,
  SellingPlanGroupRecord,
  SellingPlanRecord,
  InventoryTransferLineItemRecord,
  InventoryTransferLocationSnapshotRecord,
  InventoryTransferRecord,
  ProductBundleComponentOptionSelectionRecord,
  ProductBundleComponentQuantityOptionRecord,
  ProductBundleComponentRecord,
  ProductVariantRecord,
  ProductVariantComponentRecord,
  PublicationRecord,
  ProductFeedRecord,
  ProductResourceFeedbackRecord,
  CombinedListingChildRecord,
  ShopResourceFeedbackRecord,
} from '../state/types.js';

type ProductIdentifierCustomId = {
  namespace: string;
  key: string;
  value: string;
};

function readProductInput(raw: unknown): Record<string, unknown> {
  return isObject(raw) ? raw : {};
}

function readCapturedJsonValue(raw: unknown): JsonValue | undefined {
  const result = jsonValueSchema.safeParse(raw);
  return result.success ? structuredClone(result.data) : undefined;
}

function readProductIdentifierCustomId(identifier: Record<string, unknown>): ProductIdentifierCustomId | null {
  const customId = readProductInput(identifier['customId']);
  const namespace = customId['namespace'];
  const key = customId['key'];
  const value = customId['value'];
  if (typeof namespace !== 'string' || typeof key !== 'string' || typeof value !== 'string') {
    return null;
  }

  return { namespace, key, value };
}

function getProductCustomIdDefinition(
  runtime: ProxyRuntimeContext,
  customId: ProductIdentifierCustomId,
): MetafieldDefinitionRecord | null {
  const definition = runtime.store.findEffectiveMetafieldDefinition({
    ownerType: 'PRODUCT',
    namespace: customId.namespace,
    key: customId.key,
  });

  return definition?.type.name === 'id' ? definition : null;
}

function findEffectiveProductByCustomId(
  runtime: ProxyRuntimeContext,
  customId: ProductIdentifierCustomId,
): ProductRecord | null {
  return (
    runtime.store
      .listEffectiveProducts()
      .find((product) =>
        runtime.store
          .getEffectiveMetafieldsByProductId(product.id)
          .some(
            (metafield) =>
              metafield.namespace === customId.namespace &&
              metafield.key === customId.key &&
              metafield.type === 'id' &&
              metafield.value === customId.value,
          ),
      ) ?? null
  );
}

function findEffectiveProductByIdentifier(
  runtime: ProxyRuntimeContext,
  identifier: Record<string, unknown>,
): ProductRecord | null {
  const id = typeof identifier['id'] === 'string' ? identifier['id'] : null;
  if (id) {
    return runtime.store.getEffectiveProductById(id);
  }

  const handle = typeof identifier['handle'] === 'string' ? identifier['handle'] : null;
  if (handle) {
    return findEffectiveProductByHandle(runtime, handle);
  }

  const customId = readProductIdentifierCustomId(identifier);
  if (customId && getProductCustomIdDefinition(runtime, customId)) {
    return findEffectiveProductByCustomId(runtime, customId);
  }

  return null;
}

function buildProductCustomIdDefinitionMissingError(field: FieldNode): Record<string, unknown> {
  return {
    message: "Metafield definition of type 'id' is required when using custom ids.",
    locations: getNodeLocation(field),
    extensions: {
      code: 'NOT_FOUND',
    },
    path: [getResponseKey(field)],
  };
}

function findEffectiveVariantByIdentifier(
  runtime: ProxyRuntimeContext,
  identifier: Record<string, unknown>,
): ProductVariantRecord | null {
  const id = typeof identifier['id'] === 'string' ? identifier['id'] : null;
  return id ? runtime.store.getEffectiveVariantById(id) : null;
}

function findEffectiveCollectionById(runtime: ProxyRuntimeContext, collectionId: string): CollectionRecord | null {
  return runtime.store.getEffectiveCollectionById(collectionId);
}

function findEffectiveCollectionByHandle(runtime: ProxyRuntimeContext, handle: string): CollectionRecord | null {
  return listEffectiveCollections(runtime).find((collection) => collection.handle === handle) ?? null;
}

function findEffectiveCollectionByIdentifier(
  runtime: ProxyRuntimeContext,
  identifier: Record<string, unknown>,
): CollectionRecord | null {
  const rawId = identifier['id'];
  if (typeof rawId === 'string') {
    return findEffectiveCollectionById(runtime, rawId);
  }

  const rawHandle = identifier['handle'];
  if (typeof rawHandle === 'string') {
    return findEffectiveCollectionByHandle(runtime, rawHandle);
  }

  return null;
}

function listEffectiveCollections(runtime: ProxyRuntimeContext): CollectionRecord[] {
  return runtime.store.listEffectiveCollections();
}

function listEffectiveLocations(runtime: ProxyRuntimeContext): LocationRecord[] {
  const locations: LocationRecord[] = runtime.store.listEffectiveLocations();
  const seenLocationIds = new Set<string>();

  for (const location of locations) {
    seenLocationIds.add(location.id);
  }

  for (const product of runtime.store.listEffectiveProducts()) {
    for (const variant of runtime.store.getEffectiveVariantsByProductId(product.id)) {
      for (const level of getEffectiveInventoryLevels(runtime, variant)) {
        const locationId = level.location?.id;
        if (!locationId || seenLocationIds.has(locationId)) {
          continue;
        }

        seenLocationIds.add(locationId);
        const effectiveLocation = runtime.store.getEffectiveLocationById(locationId);
        locations.push({
          ...effectiveLocation,
          id: locationId,
          name: effectiveLocation?.name ?? level.location?.name ?? null,
        });
      }
    }
  }

  return locations;
}

function readEffectiveInventoryLevelLocation(
  runtime: ProxyRuntimeContext,
  location: NonNullable<InventoryLevelRecord['location']>,
): NonNullable<InventoryLevelRecord['location']> {
  const effectiveLocation = runtime.store.getEffectiveLocationById(location.id);
  return {
    id: location.id,
    name: effectiveLocation?.name ?? location.name,
  };
}

function listEffectivePublications(runtime: ProxyRuntimeContext): PublicationRecord[] {
  return runtime.store.listEffectivePublications();
}

function listEffectiveChannels(runtime: ProxyRuntimeContext): ChannelRecord[] {
  return runtime.store.listEffectiveChannels();
}

function listEffectiveProductsForCollection(runtime: ProxyRuntimeContext, collectionId: string): ProductRecord[] {
  return runtime.store
    .listEffectiveProducts()
    .map((product) => ({
      product,
      membership:
        runtime.store
          .getEffectiveCollectionsByProductId(product.id)
          .find((collection) => collection.id === collectionId) ?? null,
    }))
    .filter(
      (entry): entry is { product: ProductRecord; membership: ProductCollectionRecord } => entry.membership !== null,
    )
    .sort((left, right) => {
      const leftPosition =
        typeof left.membership.position === 'number' ? left.membership.position : Number.POSITIVE_INFINITY;
      const rightPosition =
        typeof right.membership.position === 'number' ? right.membership.position : Number.POSITIVE_INFINITY;
      return leftPosition - rightPosition || left.product.id.localeCompare(right.product.id);
    })
    .map((entry) => entry.product);
}

function getCollectionPublicationIds(collection: CollectionRecord | ProductCollectionRecord): string[] {
  return structuredClone(collection.publicationIds ?? []);
}

function isPublishedCollection(collection: CollectionRecord | ProductCollectionRecord): boolean {
  return getCollectionPublicationIds(collection).length > 0;
}

function makeProductCollectionRecord(
  productId: string,
  collection: CollectionRecord,
  position?: number,
): ProductCollectionRecord {
  return {
    id: collection.id,
    productId,
    title: collection.title,
    handle: collection.handle,
    ...(typeof position === 'number' ? { position } : {}),
  };
}

interface CollectionMembershipEntry {
  product: ProductRecord;
  membership: ProductCollectionRecord;
}

interface CollectionProductMove {
  id: string;
  newPosition: number;
}

type CollectionReorderUserError = { field: string[]; message: string };
type ProductReorderUserError = { field: string[]; message: string };

interface ProductVariantPosition {
  id: string;
  position: number;
}

function listEffectiveCollectionMembershipEntries(
  runtime: ProxyRuntimeContext,
  collectionId: string,
): CollectionMembershipEntry[] {
  return runtime.store
    .listEffectiveProducts()
    .map((product) => ({
      product,
      membership:
        runtime.store
          .getEffectiveCollectionsByProductId(product.id)
          .find((collection) => collection.id === collectionId) ?? null,
    }))
    .filter((entry): entry is CollectionMembershipEntry => entry.membership !== null)
    .sort((left, right) => {
      const leftPosition =
        typeof left.membership.position === 'number' ? left.membership.position : Number.POSITIVE_INFINITY;
      const rightPosition =
        typeof right.membership.position === 'number' ? right.membership.position : Number.POSITIVE_INFINITY;
      return leftPosition - rightPosition || left.product.id.localeCompare(right.product.id);
    });
}

function readCollectionReorderPosition(rawPosition: unknown): number | null {
  if (typeof rawPosition === 'number' && Number.isFinite(rawPosition)) {
    return Math.max(0, Math.floor(rawPosition));
  }

  if (typeof rawPosition !== 'string' || !/^\d+$/u.test(rawPosition.trim())) {
    return null;
  }

  return Number.parseInt(rawPosition, 10);
}

function readCollectionProductMoves(rawMoves: unknown): {
  moves: CollectionProductMove[];
  userErrors: CollectionReorderUserError[];
} {
  const rawMoveList = Array.isArray(rawMoves) ? rawMoves : isObject(rawMoves) ? [rawMoves] : [];
  const moves: CollectionProductMove[] = [];
  const userErrors: CollectionReorderUserError[] = [];

  if (rawMoveList.length === 0) {
    return {
      moves,
      userErrors: [{ field: ['moves'], message: 'At least one move is required' }],
    };
  }

  if (rawMoveList.length > 250) {
    userErrors.push({ field: ['moves'], message: 'Too many moves were provided' });
  }

  for (const [index, rawMove] of rawMoveList.entries()) {
    if (!isObject(rawMove)) {
      userErrors.push({ field: ['moves', `${index}`], message: 'Move is invalid' });
      continue;
    }

    const productId = typeof rawMove['id'] === 'string' ? rawMove['id'] : null;
    const newPosition = readCollectionReorderPosition(rawMove['newPosition']);

    if (!productId) {
      userErrors.push({ field: ['moves', `${index}`, 'id'], message: 'Product id is required' });
    }
    if (newPosition === null) {
      userErrors.push({ field: ['moves', `${index}`, 'newPosition'], message: 'Position is invalid' });
    }
    if (productId && newPosition !== null) {
      moves.push({ id: productId, newPosition });
    }
  }

  return { moves, userErrors };
}

function readProductVariantPositions(rawPositions: unknown): {
  positions: ProductVariantPosition[];
  userErrors: ProductReorderUserError[];
} {
  const rawPositionList = Array.isArray(rawPositions) ? rawPositions : isObject(rawPositions) ? [rawPositions] : [];
  const positions: ProductVariantPosition[] = [];
  const userErrors: ProductReorderUserError[] = [];

  if (rawPositionList.length === 0) {
    return {
      positions,
      userErrors: [{ field: ['positions'], message: 'At least one position is required' }],
    };
  }

  for (const [index, rawPosition] of rawPositionList.entries()) {
    if (!isObject(rawPosition)) {
      userErrors.push({ field: ['positions', `${index}`], message: 'Position is invalid' });
      continue;
    }

    const variantId = typeof rawPosition['id'] === 'string' ? rawPosition['id'] : null;
    const position = readCollectionReorderPosition(rawPosition['position']);
    if (!variantId) {
      userErrors.push({ field: ['positions', `${index}`, 'id'], message: 'Variant id is required' });
    }
    if (position === null || position < 1) {
      userErrors.push({ field: ['positions', `${index}`, 'position'], message: 'Position is invalid' });
    }
    if (variantId && position !== null && position >= 1) {
      positions.push({ id: variantId, position: position - 1 });
    }
  }

  return { positions, userErrors };
}

function applySequentialReorder<T>(
  items: T[],
  moves: Array<{ id: string; position: number }>,
  getId: (item: T) => string | null | undefined,
): T[] {
  const orderedItems = [...items];
  for (const move of moves) {
    const currentIndex = orderedItems.findIndex((item) => getId(item) === move.id);
    if (currentIndex < 0) {
      continue;
    }

    const [item] = orderedItems.splice(currentIndex, 1);
    if (!item) {
      continue;
    }

    orderedItems.splice(Math.min(move.position, orderedItems.length), 0, item);
  }

  return orderedItems;
}

function reorderCollectionProducts(
  runtime: ProxyRuntimeContext,
  collection: CollectionRecord,
  rawMoves: unknown,
): { job: { id: string; done: boolean } | null; userErrors: CollectionReorderUserError[] } {
  if (
    collection.isSmart ||
    (collection.sortOrder !== undefined && collection.sortOrder !== null && collection.sortOrder !== 'MANUAL')
  ) {
    return {
      job: null,
      userErrors: [{ field: ['id'], message: "Can't reorder products unless collection is manually sorted" }],
    };
  }

  const { moves, userErrors } = readCollectionProductMoves(rawMoves);
  const orderedEntries = listEffectiveCollectionMembershipEntries(runtime, collection.id);
  const productIdsInCollection = new Set(orderedEntries.map((entry) => entry.product.id));

  for (const [index, move] of moves.entries()) {
    if (!runtime.store.getEffectiveProductById(move.id)) {
      userErrors.push({ field: ['moves', `${index}`, 'id'], message: 'Product does not exist' });
    } else if (!productIdsInCollection.has(move.id)) {
      userErrors.push({ field: ['moves', `${index}`, 'id'], message: 'Product is not in the collection' });
    }
  }

  if (userErrors.length > 0) {
    return { job: null, userErrors };
  }

  const reorderedEntries = applySequentialReorder(
    orderedEntries,
    moves.map((move) => ({ id: move.id, position: move.newPosition })),
    (entry) => entry.product.id,
  );
  orderedEntries.splice(0, orderedEntries.length, ...reorderedEntries);

  for (const [position, entry] of orderedEntries.entries()) {
    const nextCollections = runtime.store.getEffectiveCollectionsByProductId(entry.product.id).map((membership) =>
      membership.id === collection.id
        ? {
            ...membership,
            position,
          }
        : membership,
    );
    runtime.store.replaceStagedCollectionsForProduct(entry.product.id, nextCollections);
  }

  return {
    job: { id: runtime.syntheticIdentity.makeSyntheticGid('Job'), done: false },
    userErrors: [],
  };
}

function reorderProductMedia(
  runtime: ProxyRuntimeContext,
  productId: string,
  rawMoves: unknown,
): { job: { id: string; done: boolean } | null; userErrors: ProductReorderUserError[] } {
  const { moves, userErrors } = readCollectionProductMoves(rawMoves);
  const effectiveMedia = runtime.store.getEffectiveMediaByProductId(productId);
  const mediaIds = new Set(
    effectiveMedia
      .map((mediaRecord) => mediaRecord.id)
      .filter((mediaId): mediaId is string => typeof mediaId === 'string'),
  );

  for (const [index, move] of moves.entries()) {
    if (!mediaIds.has(move.id)) {
      userErrors.push({ field: ['moves', `${index}`, 'id'], message: 'Media does not exist' });
    }
  }

  if (userErrors.length > 0) {
    return { job: null, userErrors };
  }

  const nextMedia = applySequentialReorder(
    effectiveMedia,
    moves.map((move) => ({ id: move.id, position: move.newPosition })),
    (mediaRecord) => mediaRecord.id,
  ).map((mediaRecord, position) => ({
    ...mediaRecord,
    position,
  }));
  runtime.store.replaceStagedMediaForProduct(productId, nextMedia);

  return {
    job: { id: runtime.syntheticIdentity.makeSyntheticGid('Job'), done: false },
    userErrors: [],
  };
}

function reorderProductVariants(
  runtime: ProxyRuntimeContext,
  productId: string,
  rawPositions: unknown,
): { product: ProductRecord | null; userErrors: ProductReorderUserError[] } {
  const { positions, userErrors } = readProductVariantPositions(rawPositions);
  const effectiveVariants = runtime.store.getEffectiveVariantsByProductId(productId);
  const variantIds = new Set(effectiveVariants.map((variant) => variant.id));

  for (const [index, position] of positions.entries()) {
    if (!variantIds.has(position.id)) {
      userErrors.push({ field: ['positions', `${index}`, 'id'], message: 'Variant does not exist' });
    }
  }

  if (userErrors.length > 0) {
    return { product: null, userErrors };
  }

  const nextVariants = applySequentialReorder(effectiveVariants, positions, (variant) => variant.id);
  runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);

  return {
    product: syncProductInventorySummary(runtime, productId),
    userErrors: [],
  };
}

function addProductsToCollection(
  runtime: ProxyRuntimeContext,
  collection: CollectionRecord,
  productIds: string[],
  options: { placement?: 'append' | 'prepend-reverse' } = {},
): { collection: CollectionRecord | null; userErrors: Array<{ field: string[]; message: string }> } {
  const normalizedProductIds = productIds.filter((productId, index) => productIds.indexOf(productId) === index);
  if (normalizedProductIds.length === 0) {
    return {
      collection: null,
      userErrors: [{ field: ['productIds'], message: 'At least one product id is required' }],
    };
  }

  if (collection.isSmart) {
    return {
      collection: null,
      userErrors: [{ field: ['id'], message: "Can't manually add products to a smart collection" }],
    };
  }

  const duplicateMembership = normalizedProductIds.find((productId) =>
    runtime.store.getEffectiveCollectionsByProductId(productId).some((candidate) => candidate.id === collection.id),
  );
  if (duplicateMembership) {
    return {
      collection: null,
      userErrors: [{ field: ['productIds'], message: 'Product is already in the collection' }],
    };
  }

  const existingProductIds = normalizedProductIds.filter((productId) =>
    runtime.store.getEffectiveProductById(productId),
  );
  if (existingProductIds.length === 0) {
    return {
      collection,
      userErrors: [],
    };
  }

  const existingPositions = runtime.store
    .listEffectiveProducts()
    .flatMap((product) => runtime.store.getEffectiveCollectionsByProductId(product.id))
    .filter((candidate) => candidate.id === collection.id)
    .map((candidate) => candidate.position)
    .filter((position): position is number => typeof position === 'number' && Number.isFinite(position));
  const placement = options.placement ?? 'append';
  const firstPosition =
    placement === 'prepend-reverse'
      ? existingPositions.length > 0
        ? Math.min(...existingPositions) - existingProductIds.length
        : 0
      : existingPositions.length > 0
        ? Math.max(...existingPositions) + 1
        : 0;
  const positionedProductIds = placement === 'prepend-reverse' ? [...existingProductIds].reverse() : existingProductIds;

  for (const [index, productId] of positionedProductIds.entries()) {
    const nextCollections = [
      ...runtime.store.getEffectiveCollectionsByProductId(productId),
      makeProductCollectionRecord(productId, collection, firstPosition + index),
    ];
    runtime.store.replaceStagedCollectionsForProduct(productId, nextCollections);
  }

  return {
    collection,
    userErrors: [],
  };
}

function readCollectionProductIds(rawProductIds: unknown): string[] {
  return Array.isArray(rawProductIds)
    ? rawProductIds.filter((productId): productId is string => typeof productId === 'string')
    : [];
}

function removeProductsFromCollection(
  runtime: ProxyRuntimeContext,
  collection: CollectionRecord,
  productIds: string[],
): void {
  const normalizedProductIds = productIds.filter((productId, index) => productIds.indexOf(productId) === index);

  for (const productId of normalizedProductIds) {
    const existingProduct = runtime.store.getEffectiveProductById(productId);
    if (!existingProduct) {
      continue;
    }

    const nextCollections = runtime.store
      .getEffectiveCollectionsByProductId(productId)
      .filter((candidate) => candidate.id !== collection.id);
    runtime.store.replaceStagedCollectionsForProduct(productId, nextCollections);
  }
}

interface ProductSetInventoryQuantityInput {
  locationId: string | null;
  name: string;
  quantity: number;
}

function readProductSetInventoryQuantityInputs(raw: unknown): ProductSetInventoryQuantityInput[] {
  if (typeof raw === 'number' && Number.isFinite(raw)) {
    return [
      {
        locationId: null,
        name: 'available',
        quantity: Math.floor(raw),
      },
    ];
  }

  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .filter((value): value is Record<string, unknown> => isObject(value))
    .map((value) => {
      const rawQuantity = value['quantity'];
      if (typeof rawQuantity !== 'number' || !Number.isFinite(rawQuantity)) {
        return null;
      }

      const rawName = value['name'];
      const rawLocationId = value['locationId'];
      return {
        locationId: typeof rawLocationId === 'string' && rawLocationId.trim() ? rawLocationId : null,
        name: typeof rawName === 'string' && rawName.trim() ? rawName : 'available',
        quantity: Math.floor(rawQuantity),
      };
    })
    .filter((value): value is ProductSetInventoryQuantityInput => value !== null);
}

function readProductSetInventoryQuantity(raw: unknown): number | null {
  const quantities = readProductSetInventoryQuantityInputs(raw)
    .filter((value) => value.name === 'available')
    .map((value) => value.quantity);

  if (quantities.length === 0) {
    return null;
  }

  return quantities.reduce((total, quantity) => total + quantity, 0);
}

function readProductSetSelectedOptions(raw: unknown): ProductVariantRecord['selectedOptions'] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .map((value) => {
      if (!isObject(value)) {
        return null;
      }

      const optionName = value['optionName'];
      const optionValue = value['name'];
      if (
        typeof optionName !== 'string' ||
        !optionName.trim() ||
        typeof optionValue !== 'string' ||
        !optionValue.trim()
      ) {
        return null;
      }

      return {
        name: optionName,
        value: optionValue,
      };
    })
    .filter((value): value is ProductVariantRecord['selectedOptions'][number] => value !== null);
}

function normalizeProductSetVariantInput(input: Record<string, unknown>): Record<string, unknown> {
  const selectedOptions = readProductSetSelectedOptions(input['optionValues']);
  const normalized: Record<string, unknown> = {
    ...input,
    selectedOptions,
    inventoryQuantity: readProductSetInventoryQuantity(input['inventoryQuantities']),
  };

  const rawPrice = input['price'];
  if (typeof rawPrice === 'number' && Number.isFinite(rawPrice)) {
    normalized['price'] = rawPrice.toFixed(2);
  }

  const rawCompareAtPrice = input['compareAtPrice'];
  if (typeof rawCompareAtPrice === 'number' && Number.isFinite(rawCompareAtPrice)) {
    normalized['compareAtPrice'] = rawCompareAtPrice.toFixed(2);
  }

  return normalized;
}

function readStatus(raw: unknown, fallback: ProductRecord['status']): ProductRecord['status'] {
  if (raw === 'ACTIVE' || raw === 'ARCHIVED' || raw === 'DRAFT') {
    return raw;
  }
  return fallback;
}

function readPublicationCount(raw: unknown): number {
  if (typeof raw === 'number' && Number.isFinite(raw) && raw >= 0) {
    return Math.floor(raw);
  }

  if (!isObject(raw)) {
    return 0;
  }

  const rawCount = raw['count'];
  return typeof rawCount === 'number' && Number.isFinite(rawCount) && rawCount >= 0 ? Math.floor(rawCount) : 0;
}

function makeUnknownPublicationIds(count: number): string[] {
  return Array.from({ length: Math.max(0, count) }, (_, index) => `__unknown_publication__${index + 1}`);
}

function isUnknownPublicationId(publicationId: string): boolean {
  return publicationId.startsWith('__unknown_publication__');
}

const currentPublicationPlaceholderId = '__current_publication__';

function readPublicationIds(raw: unknown, fallback: string[] = []): string[] {
  if (!Array.isArray(raw)) {
    return structuredClone(fallback);
  }

  return raw.filter((value): value is string => typeof value === 'string' && value.length > 0);
}

function readPublicationTargets(raw: unknown): string[] {
  const entries = Array.isArray(raw) ? raw : isObject(raw) ? [raw] : [];
  if (entries.length === 0) {
    return [];
  }

  const targets: string[] = [];
  for (const entry of entries) {
    if (!isObject(entry)) {
      continue;
    }

    const publicationId = entry['publicationId'];
    if (typeof publicationId === 'string' && publicationId.length > 0) {
      targets.push(publicationId);
      continue;
    }

    const channelId = entry['channelId'];
    if (typeof channelId === 'string' && channelId.length > 0) {
      targets.push(channelId);
      continue;
    }

    const channelHandle = entry['channelHandle'];
    if (typeof channelHandle === 'string' && channelHandle.length > 0) {
      targets.push(`channel-handle:${channelHandle}`);
    }
  }

  return [...new Set(targets)];
}

function mergePublicationTargets(existing: string[], additions: string[]): string[] {
  return [...new Set([...existing, ...additions])];
}

function readTagInputs(raw: unknown, options: { allowCommaSeparatedString: boolean }): string[] {
  const values: string[] = [];

  if (Array.isArray(raw)) {
    for (const value of raw) {
      if (typeof value !== 'string') {
        continue;
      }
      const trimmed = value.trim();
      if (trimmed) {
        values.push(trimmed);
      }
    }
  } else if (options.allowCommaSeparatedString && typeof raw === 'string') {
    for (const value of raw.split(',')) {
      const trimmed = value.trim();
      if (trimmed) {
        values.push(trimmed);
      }
    }
  }

  return [...new Set(values)];
}

function normalizeProductTags(tags: string[]): string[] {
  return [...new Set(tags.map((tag) => tag.trim()).filter((tag) => tag.length > 0))].sort((left, right) => {
    if (left === right) {
      return 0;
    }

    return left < right ? -1 : 1;
  });
}

function removePublicationTargets(existing: string[], removals: string[]): string[] {
  const next = [...existing];

  for (const removal of removals) {
    const exactIndex = next.indexOf(removal);
    if (exactIndex >= 0) {
      next.splice(exactIndex, 1);
      continue;
    }

    const unknownIndex = next.findIndex((value) => isUnknownPublicationId(value));
    if (unknownIndex >= 0) {
      next.splice(unknownIndex, 1);
    }
  }

  return next;
}

function removePublicationFromPublishables(runtime: ProxyRuntimeContext, publicationId: string): void {
  for (const product of runtime.store.listEffectiveProducts()) {
    if (product.publicationIds.includes(publicationId)) {
      runtime.store.stageUpdateProduct(
        makeProductRecord(
          runtime,
          {
            id: product.id,
            publicationIds: removePublicationTargets(product.publicationIds, [publicationId]),
          },
          product,
        ),
      );
    }
  }

  for (const collection of listEffectiveCollections(runtime)) {
    const publicationIds = collection.publicationIds ?? [];
    if (publicationIds.includes(publicationId)) {
      runtime.store.stageUpdateCollection(
        makeCollectionRecord(
          runtime,
          {
            id: collection.id,
            publicationIds: removePublicationTargets(publicationIds, [publicationId]),
          },
          collection,
        ),
      );
    }
  }
}

function getPublishableProductId(rawId: unknown): string | null {
  return typeof rawId === 'string' && rawId.startsWith('gid://shopify/Product/') ? rawId : null;
}

function makeProductRecord(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing?: ProductRecord,
): ProductRecord {
  const rawTitle = input['title'];
  const title = typeof rawTitle === 'string' && rawTitle.trim() ? rawTitle : (existing?.title ?? 'Untitled product');
  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const rawId = input['id'];
  const rawHandle = input['handle'];
  const rawStatus = input['status'];
  const rawVendor = input['vendor'];
  const rawProductType = input['productType'];
  const rawTags = input['tags'];
  const rawDescriptionHtml = input['descriptionHtml'];
  const rawTemplateSuffix = input['templateSuffix'];
  const rawSeo = input['seo'];
  const rawPublishedAt = input['publishedAt'];
  const rawCombinedListingRole = input['combinedListingRole'];

  const isSparseUpdate = typeof rawId === 'string' && !existing;
  const existingSeo = existing?.seo ?? { title: null, description: null };

  return {
    id:
      typeof rawId === 'string' ? rawId : (existing?.id ?? runtime.syntheticIdentity.makeProxySyntheticGid('Product')),
    legacyResourceId: existing?.legacyResourceId ?? null,
    title,
    handle:
      typeof rawHandle === 'string' && rawHandle.trim()
        ? rawHandle
        : (existing?.handle ?? (isSparseUpdate ? '' : slugifyHandle(title))),
    status: readStatus(rawStatus, existing?.status ?? 'ACTIVE'),
    combinedListingRole:
      rawCombinedListingRole === 'PARENT' || rawCombinedListingRole === 'CHILD'
        ? rawCombinedListingRole
        : (existing?.combinedListingRole ?? null),
    publicationIds: readPublicationIds(input['publicationIds'], existing?.publicationIds ?? []),
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
    publishedAt:
      rawPublishedAt === null
        ? null
        : typeof rawPublishedAt === 'string'
          ? rawPublishedAt
          : (existing?.publishedAt ?? null),
    vendor: typeof rawVendor === 'string' ? rawVendor : (existing?.vendor ?? null),
    productType: typeof rawProductType === 'string' ? rawProductType : (existing?.productType ?? null),
    tags: Array.isArray(rawTags)
      ? normalizeProductTags(rawTags.filter((tag): tag is string => typeof tag === 'string'))
      : structuredClone(existing?.tags ?? []),
    totalInventory: existing?.totalInventory ?? null,
    tracksInventory: existing?.tracksInventory ?? null,
    descriptionHtml: typeof rawDescriptionHtml === 'string' ? rawDescriptionHtml : (existing?.descriptionHtml ?? null),
    onlineStorePreviewUrl: existing?.onlineStorePreviewUrl ?? null,
    templateSuffix: typeof rawTemplateSuffix === 'string' ? rawTemplateSuffix : (existing?.templateSuffix ?? null),
    seo: isObject(rawSeo)
      ? {
          title: typeof rawSeo['title'] === 'string' ? rawSeo['title'] : existingSeo.title,
          description: typeof rawSeo['description'] === 'string' ? rawSeo['description'] : existingSeo.description,
        }
      : existingSeo,
    category: existing?.category ?? null,
  };
}

function makeSyntheticOnlineStorePreviewUrl(product: Pick<ProductRecord, 'id' | 'handle'>): string {
  return `https://shopify-draft-proxy.local/products_preview?product_id=${encodeURIComponent(
    product.id,
  )}&handle=${encodeURIComponent(product.handle)}`;
}

function readCollectionSeo(rawSeo: unknown, existingSeo: CollectionRecord['seo']): CollectionRecord['seo'] {
  if (!isObject(rawSeo)) {
    return existingSeo ?? { title: null, description: null };
  }

  return {
    title: typeof rawSeo['title'] === 'string' ? rawSeo['title'] : (existingSeo?.title ?? null),
    description: typeof rawSeo['description'] === 'string' ? rawSeo['description'] : (existingSeo?.description ?? null),
  };
}

function readCollectionImageInput(
  rawImage: unknown,
  existingImage: CollectionImageRecord | null | undefined,
): CollectionImageRecord | null {
  if (rawImage === null) {
    return null;
  }

  if (!isObject(rawImage)) {
    return existingImage ? structuredClone(existingImage) : null;
  }

  const rawId = rawImage['id'];
  const rawSrc = rawImage['src'];
  const rawUrl = rawImage['url'];
  const rawAltText = rawImage['altText'];
  const rawWidth = rawImage['width'];
  const rawHeight = rawImage['height'];

  return {
    id: typeof rawId === 'string' ? rawId : (existingImage?.id ?? null),
    altText: typeof rawAltText === 'string' ? rawAltText : (existingImage?.altText ?? null),
    url: typeof rawUrl === 'string' ? rawUrl : typeof rawSrc === 'string' ? rawSrc : (existingImage?.url ?? null),
    width: typeof rawWidth === 'number' ? rawWidth : (existingImage?.width ?? null),
    height: typeof rawHeight === 'number' ? rawHeight : (existingImage?.height ?? null),
  };
}

function readCollectionRuleSet(
  rawRuleSet: unknown,
  existingRuleSet: CollectionRuleSetRecord | null | undefined,
): CollectionRuleSetRecord | null {
  if (rawRuleSet === null) {
    return null;
  }

  if (!isObject(rawRuleSet)) {
    return existingRuleSet ? structuredClone(existingRuleSet) : null;
  }

  const rawRules = Array.isArray(rawRuleSet['rules']) ? rawRuleSet['rules'] : [];
  return {
    appliedDisjunctively:
      typeof rawRuleSet['appliedDisjunctively'] === 'boolean'
        ? rawRuleSet['appliedDisjunctively']
        : (existingRuleSet?.appliedDisjunctively ?? false),
    rules: rawRules
      .filter(isObject)
      .map((rule) => {
        const column = rule['column'];
        const relation = rule['relation'];
        const condition = rule['condition'];
        const conditionObjectId = rule['conditionObjectId'];
        return {
          column: typeof column === 'string' ? column : '',
          relation: typeof relation === 'string' ? relation : '',
          condition: typeof condition === 'string' ? condition : '',
          conditionObjectId: typeof conditionObjectId === 'string' ? conditionObjectId : null,
        };
      })
      .filter((rule) => rule.column && rule.relation),
  };
}

function makeCollectionRecord(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing?: CollectionRecord,
): CollectionRecord {
  const rawTitle = input['title'];
  const title = typeof rawTitle === 'string' && rawTitle.trim() ? rawTitle : (existing?.title ?? 'Untitled collection');
  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const rawId = input['id'];
  const id =
    typeof rawId === 'string' ? rawId : (existing?.id ?? runtime.syntheticIdentity.makeSyntheticGid('Collection'));
  const rawHandle = input['handle'];
  const rawDescriptionHtml = input['descriptionHtml'];
  const rawImage = input['image'];
  const rawSortOrder = input['sortOrder'];
  const rawTemplateSuffix = input['templateSuffix'];
  const rawRuleSet = input['ruleSet'];
  const descriptionHtml =
    rawDescriptionHtml === null
      ? null
      : typeof rawDescriptionHtml === 'string'
        ? rawDescriptionHtml
        : (existing?.descriptionHtml ?? (existing ? null : ''));

  return {
    id,
    legacyResourceId: existing?.legacyResourceId ?? readLegacyResourceIdFromGid(id),
    title,
    handle:
      typeof rawHandle === 'string' && rawHandle.trim()
        ? rawHandle
        : (existing?.handle ?? slugifyHandle(title).replace(/product$/u, 'collection')),
    publicationIds: readPublicationIds(input['publicationIds'], existing?.publicationIds ?? []),
    updatedAt: now,
    description:
      typeof input['description'] === 'string'
        ? input['description']
        : descriptionHtml !== null
          ? stripHtmlToDescription(descriptionHtml)
          : (existing?.description ?? null),
    descriptionHtml,
    image: hasOwnField(input, 'image')
      ? readCollectionImageInput(rawImage, existing?.image)
      : (existing?.image ?? null),
    sortOrder: typeof rawSortOrder === 'string' ? rawSortOrder : (existing?.sortOrder ?? (existing ? null : 'MANUAL')),
    templateSuffix:
      rawTemplateSuffix === null
        ? null
        : typeof rawTemplateSuffix === 'string'
          ? rawTemplateSuffix
          : (existing?.templateSuffix ?? null),
    seo: readCollectionSeo(input['seo'], existing?.seo),
    ruleSet: hasOwnField(input, 'ruleSet')
      ? readCollectionRuleSet(rawRuleSet, existing?.ruleSet)
      : (existing?.ruleSet ?? null),
    redirectNewHandle:
      typeof input['redirectNewHandle'] === 'boolean'
        ? input['redirectNewHandle']
        : (existing?.redirectNewHandle ?? false),
    ...(existing?.isSmart !== undefined
      ? { isSmart: existing.isSmart }
      : hasOwnField(input, 'ruleSet') && rawRuleSet !== null && rawRuleSet !== undefined
        ? { isSmart: true }
        : {}),
  };
}

function makePublicationRecord(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing?: PublicationRecord,
): PublicationRecord {
  const rawId = input['id'];
  const id =
    typeof rawId === 'string' && rawId.length > 0
      ? rawId
      : (existing?.id ?? runtime.syntheticIdentity.makeSyntheticGid('Publication'));
  const rawName = input['name'] ?? input['title'];
  const rawAutoPublish = input['autoPublish'];
  const rawSupportsFuturePublishing = input['supportsFuturePublishing'];
  const rawCatalogId = input['catalogId'];
  const rawChannelId = input['channelId'];

  return {
    id,
    name: typeof rawName === 'string' ? rawName : (existing?.name ?? null),
    autoPublish: typeof rawAutoPublish === 'boolean' ? rawAutoPublish : existing?.autoPublish,
    supportsFuturePublishing:
      typeof rawSupportsFuturePublishing === 'boolean'
        ? rawSupportsFuturePublishing
        : existing?.supportsFuturePublishing,
    catalogId: typeof rawCatalogId === 'string' ? rawCatalogId : existing?.catalogId,
    channelId: typeof rawChannelId === 'string' ? rawChannelId : existing?.channelId,
    cursor: existing?.cursor,
  };
}

function makeDuplicatedProductRecord(
  runtime: ProxyRuntimeContext,
  source: ProductRecord,
  newTitle?: string,
): ProductRecord {
  const title = typeof newTitle === 'string' && newTitle.trim() ? newTitle : `Copy of ${source.title}`;
  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();

  return {
    id: runtime.syntheticIdentity.makeProxySyntheticGid('Product'),
    legacyResourceId: null,
    title,
    handle: slugifyHandle(title),
    status: 'DRAFT',
    publicationIds: [],
    createdAt: now,
    updatedAt: now,
    publishedAt: null,
    vendor: source.vendor,
    productType: source.productType,
    tags: structuredClone(source.tags),
    totalInventory: source.totalInventory,
    tracksInventory: source.tracksInventory,
    descriptionHtml: source.descriptionHtml,
    onlineStorePreviewUrl: source.onlineStorePreviewUrl,
    templateSuffix: source.templateSuffix,
    seo: structuredClone(source.seo),
    category: source.category ? structuredClone(source.category) : null,
  };
}

function duplicateVariantRecord(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  productId: string,
): ProductVariantRecord {
  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('ProductVariant'),
    productId,
    title: variant.title,
    sku: variant.sku,
    barcode: variant.barcode,
    price: variant.price,
    compareAtPrice: variant.compareAtPrice,
    taxable: variant.taxable,
    inventoryPolicy: variant.inventoryPolicy,
    inventoryQuantity: variant.inventoryQuantity,
    selectedOptions: structuredClone(variant.selectedOptions),
    inventoryItem: variant.inventoryItem
      ? {
          ...structuredClone(variant.inventoryItem),
          id: runtime.syntheticIdentity.makeSyntheticGid('InventoryItem'),
        }
      : null,
  };
}

function duplicateOptionRecord(
  runtime: ProxyRuntimeContext,
  option: ProductOptionRecord,
  productId: string,
): ProductOptionRecord {
  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('ProductOption'),
    productId,
    name: option.name,
    position: option.position,
    optionValues: option.optionValues.map((optionValue) => ({
      id: runtime.syntheticIdentity.makeSyntheticGid('ProductOptionValue'),
      name: optionValue.name,
      hasVariants: optionValue.hasVariants,
    })),
  };
}

function duplicateCollectionRecord(collection: ProductCollectionRecord, productId: string): ProductCollectionRecord {
  return {
    id: collection.id,
    productId,
    title: collection.title,
    handle: collection.handle,
  };
}

function duplicateMetafieldRecord(
  runtime: ProxyRuntimeContext,
  metafield: ProductMetafieldRecord,
  productId: string,
): ProductMetafieldRecord {
  const timestamp = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const duplicated: ProductMetafieldRecord = {
    id: runtime.syntheticIdentity.makeSyntheticGid('Metafield'),
    productId,
    ownerId: productId,
    namespace: metafield.namespace,
    key: metafield.key,
    type: metafield.type,
    value: metafield.value,
    jsonValue: metafield.jsonValue ?? parseMetafieldJsonValue(metafield.type, metafield.value),
    createdAt: timestamp,
    updatedAt: timestamp,
    ownerType: 'PRODUCT',
  };

  return {
    ...duplicated,
    compareDigest: makeMetafieldCompareDigest(duplicated),
  };
}

function readSelectedOptionsInput(raw: unknown): ProductVariantRecord['selectedOptions'] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .map((value) => {
      if (!isObject(value)) {
        return null;
      }

      const rawName = value['name'];
      const rawValue = value['value'];
      if (typeof rawName !== 'string' || !rawName.trim() || typeof rawValue !== 'string' || !rawValue.trim()) {
        return null;
      }

      return {
        name: rawName,
        value: rawValue,
      };
    })
    .filter((value): value is ProductVariantRecord['selectedOptions'][number] => value !== null);
}

function readVariantOptionsArrayInput(
  runtime: ProxyRuntimeContext,
  raw: unknown,
  productId: string,
): ProductVariantRecord['selectedOptions'] {
  if (!Array.isArray(raw)) {
    return [];
  }

  const optionValues = raw.filter((value): value is string => typeof value === 'string' && value.trim().length > 0);
  const options = runtime.store.getEffectiveOptionsByProductId(productId);
  return optionValues
    .map((value, index) => {
      const optionName = options[index]?.name;
      if (typeof optionName !== 'string' || !optionName.trim()) {
        return null;
      }

      return {
        name: optionName,
        value,
      };
    })
    .filter((value): value is ProductVariantRecord['selectedOptions'][number] => value !== null);
}

function readVariantSelectedOptions(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  productId: string,
  fallback: ProductVariantRecord['selectedOptions'] = [],
): ProductVariantRecord['selectedOptions'] {
  if (hasOwnField(input, 'selectedOptions')) {
    return readSelectedOptionsInput(input['selectedOptions']);
  }

  if (hasOwnField(input, 'optionValues')) {
    return readProductSetSelectedOptions(input['optionValues']);
  }

  if (hasOwnField(input, 'options')) {
    return readVariantOptionsArrayInput(runtime, input['options'], productId);
  }

  return structuredClone(fallback);
}

function readVariantInventoryQuantity(input: Record<string, unknown>, fallback: number | null): number | null {
  if (typeof input['inventoryQuantity'] === 'number' && Number.isFinite(input['inventoryQuantity'])) {
    return Math.floor(input['inventoryQuantity']);
  }

  const rawInventoryQuantities = input['inventoryQuantities'];
  if (!Array.isArray(rawInventoryQuantities)) {
    return fallback;
  }

  const quantities = rawInventoryQuantities
    .filter((value): value is Record<string, unknown> => isObject(value))
    .map((value) => value['availableQuantity'])
    .filter((value): value is number => typeof value === 'number' && Number.isFinite(value));

  if (quantities.length === 0) {
    return fallback;
  }

  return quantities.reduce((total, quantity) => total + Math.floor(quantity), 0);
}

function readVariantSku(input: Record<string, unknown>, fallback: string | null): string | null {
  if (typeof input['sku'] === 'string') {
    return input['sku'];
  }

  const rawInventoryItem = input['inventoryItem'];
  if (isObject(rawInventoryItem) && typeof rawInventoryItem['sku'] === 'string') {
    return rawInventoryItem['sku'];
  }

  return fallback;
}

function makeDefaultInventoryItemMeasurement(): NonNullable<
  NonNullable<ProductVariantRecord['inventoryItem']>['measurement']
> {
  return {
    weight: {
      unit: 'KILOGRAMS',
      value: 0,
    },
  };
}

function normalizeInventoryLevelRecords(raw: unknown): InventoryLevelRecord[] | null {
  if (!isObject(raw)) {
    return null;
  }

  const rawEdges = Array.isArray(raw['edges']) ? raw['edges'] : [];
  const levels = rawEdges
    .map((edge) => {
      if (!isObject(edge)) {
        return null;
      }

      const cursor = typeof edge['cursor'] === 'string' && edge['cursor'].trim() ? edge['cursor'] : null;
      const rawNode = edge['node'];
      if (!isObject(rawNode) || typeof rawNode['id'] !== 'string') {
        return null;
      }

      const rawLocation = rawNode['location'];
      const location =
        isObject(rawLocation) && typeof rawLocation['id'] === 'string'
          ? {
              id: rawLocation['id'],
              name: typeof rawLocation['name'] === 'string' ? rawLocation['name'] : null,
            }
          : null;
      const quantities = Array.isArray(rawNode['quantities'])
        ? rawNode['quantities']
            .map((quantity) => {
              if (!isObject(quantity) || typeof quantity['name'] !== 'string') {
                return null;
              }

              return {
                name: quantity['name'],
                quantity: typeof quantity['quantity'] === 'number' ? quantity['quantity'] : null,
                updatedAt: typeof quantity['updatedAt'] === 'string' ? quantity['updatedAt'] : null,
              };
            })
            .filter(
              (
                quantity,
              ): quantity is NonNullable<
                NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']
              >[number]['quantities'][number] => quantity !== null,
            )
        : [];

      return {
        id: rawNode['id'],
        cursor,
        location,
        quantities,
      };
    })
    .filter(
      (level): level is NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']>[number] =>
        level !== null,
    );

  return levels.length > 0 ? levels : null;
}

function readInventoryItemInput(
  runtime: ProxyRuntimeContext,
  raw: unknown,
  existing: ProductVariantRecord['inventoryItem'],
): ProductVariantRecord['inventoryItem'] {
  if (!isObject(raw)) {
    return existing ? structuredClone(existing) : null;
  }

  const current = existing ? structuredClone(existing) : null;
  const rawMeasurement = raw['measurement'];
  const rawWeight = isObject(rawMeasurement) ? rawMeasurement['weight'] : null;

  return {
    id: current?.id ?? runtime.syntheticIdentity.makeSyntheticGid('InventoryItem'),
    tracked: typeof raw['tracked'] === 'boolean' ? raw['tracked'] : (current?.tracked ?? null),
    requiresShipping:
      typeof raw['requiresShipping'] === 'boolean' ? raw['requiresShipping'] : (current?.requiresShipping ?? null),
    measurement: isObject(rawWeight)
      ? {
          weight: {
            unit:
              typeof rawWeight['unit'] === 'string' ? rawWeight['unit'] : (current?.measurement?.weight?.unit ?? null),
            value:
              typeof rawWeight['value'] === 'number'
                ? rawWeight['value']
                : (current?.measurement?.weight?.value ?? null),
          },
        }
      : (current?.measurement ?? null),
    countryCodeOfOrigin:
      typeof raw['countryCodeOfOrigin'] === 'string'
        ? raw['countryCodeOfOrigin']
        : (current?.countryCodeOfOrigin ?? null),
    provinceCodeOfOrigin:
      typeof raw['provinceCodeOfOrigin'] === 'string'
        ? raw['provinceCodeOfOrigin']
        : (current?.provinceCodeOfOrigin ?? null),
    harmonizedSystemCode:
      typeof raw['harmonizedSystemCode'] === 'string'
        ? raw['harmonizedSystemCode']
        : (current?.harmonizedSystemCode ?? null),
    inventoryLevels: normalizeInventoryLevelRecords(raw['inventoryLevels']) ?? current?.inventoryLevels ?? null,
  };
}

function makeInventoryItemForInventoryQuantities(
  runtime: ProxyRuntimeContext,
  existing: ProductVariantRecord['inventoryItem'],
): NonNullable<ProductVariantRecord['inventoryItem']> {
  return existing
    ? structuredClone(existing)
    : {
        id: runtime.syntheticIdentity.makeSyntheticGid('InventoryItem'),
        tracked: null,
        requiresShipping: null,
        measurement: null,
        countryCodeOfOrigin: null,
        provinceCodeOfOrigin: null,
        harmonizedSystemCode: null,
        inventoryLevels: null,
      };
}

function upsertInventoryLevelQuantity(
  runtime: ProxyRuntimeContext,
  quantities: InventoryLevelRecord['quantities'],
  name: string,
  quantity: number,
  updatedAt: string | null = runtime.syntheticIdentity.makeSyntheticTimestamp(),
): InventoryLevelRecord['quantities'] {
  const nextQuantities = quantities.map((candidate) => structuredClone(candidate));
  const existingIndex = nextQuantities.findIndex((candidate) => candidate.name === name);
  const nextQuantity = {
    name,
    quantity,
    updatedAt,
  };

  if (existingIndex >= 0) {
    nextQuantities[existingIndex] = {
      ...nextQuantities[existingIndex]!,
      ...nextQuantity,
    };
  } else {
    nextQuantities.push(nextQuantity);
  }

  return nextQuantities;
}

function ensureInventoryLevelQuantity(
  quantities: InventoryLevelRecord['quantities'],
  name: string,
  quantity: number,
): InventoryLevelRecord['quantities'] {
  if (quantities.some((candidate) => candidate.name === name)) {
    return quantities;
  }

  return [
    ...quantities,
    {
      name,
      quantity,
      updatedAt: null,
    },
  ];
}

function buildProductSetInventoryLevels(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  rawInventoryQuantities: unknown,
  inventoryItem: NonNullable<ProductVariantRecord['inventoryItem']>,
): InventoryLevelRecord[] | null {
  const quantityInputs = readProductSetInventoryQuantityInputs(rawInventoryQuantities);
  if (quantityInputs.length === 0) {
    return inventoryItem.inventoryLevels ?? null;
  }

  const inputsByLocationId = new Map<string, ProductSetInventoryQuantityInput[]>();
  for (const quantityInput of quantityInputs) {
    const locationId = quantityInput.locationId ?? DEFAULT_INVENTORY_LEVEL_LOCATION_ID;
    inputsByLocationId.set(locationId, [...(inputsByLocationId.get(locationId) ?? []), quantityInput]);
  }

  const existingLevelsByLocationId = new Map(
    (inventoryItem.inventoryLevels ?? buildSyntheticInventoryLevels(runtime, { ...variant, inventoryItem })).map(
      (level) => [level.location?.id ?? DEFAULT_INVENTORY_LEVEL_LOCATION_ID, level],
    ),
  );

  return [...inputsByLocationId.entries()].map(([locationId, quantityInputsForLocation]) => {
    const existingLevel = existingLevelsByLocationId.get(locationId) ?? null;
    const effectiveLocation = runtime.store.getEffectiveLocationById(locationId);
    let quantities = existingLevel?.quantities.map((quantity) => structuredClone(quantity)) ?? [];

    for (const quantityInput of quantityInputsForLocation) {
      quantities = upsertInventoryLevelQuantity(runtime, quantities, quantityInput.name, quantityInput.quantity);
      if (quantityInput.name === 'available') {
        quantities = upsertInventoryLevelQuantity(runtime, quantities, 'on_hand', quantityInput.quantity, null);
      }
    }

    quantities = ensureInventoryLevelQuantity(quantities, 'available', 0);
    quantities = ensureInventoryLevelQuantity(quantities, 'on_hand', 0);
    quantities = ensureInventoryLevelQuantity(quantities, 'incoming', 0);

    return {
      id: existingLevel?.id ?? buildStableSyntheticInventoryLevelId(inventoryItem.id, locationId),
      cursor: existingLevel?.cursor ?? null,
      location: {
        id: locationId,
        name: effectiveLocation?.name ?? existingLevel?.location?.name ?? null,
      },
      quantities,
    };
  });
}

function applyProductSetInventoryQuantitiesToVariant(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  input: Record<string, unknown>,
): ProductVariantRecord {
  if (!hasOwnField(input, 'inventoryQuantities')) {
    return variant;
  }

  const quantityInputs = readProductSetInventoryQuantityInputs(input['inventoryQuantities']);
  if (quantityInputs.length === 0) {
    return variant;
  }

  const inventoryItem = makeInventoryItemForInventoryQuantities(runtime, variant.inventoryItem);
  const inventoryLevels = buildProductSetInventoryLevels(runtime, variant, input['inventoryQuantities'], inventoryItem);
  const availableQuantity = readProductSetInventoryQuantity(input['inventoryQuantities']);

  return {
    ...variant,
    inventoryQuantity: availableQuantity ?? variant.inventoryQuantity,
    inventoryItem: {
      ...inventoryItem,
      inventoryLevels,
    },
  };
}

function makeCreatedVariantRecord(
  runtime: ProxyRuntimeContext,
  productId: string,
  input: Record<string, unknown>,
  defaults: ProductVariantRecord | null = null,
): ProductVariantRecord {
  const selectedOptions = readVariantSelectedOptions(runtime, input, productId);
  const defaultInventoryItem = defaults?.inventoryItem
    ? {
        ...structuredClone(defaults.inventoryItem),
        id: runtime.syntheticIdentity.makeSyntheticGid('InventoryItem'),
      }
    : null;
  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('ProductVariant'),
    productId,
    title: deriveVariantTitle(input['title'], selectedOptions, 'Default Title'),
    sku: readVariantSku(input, null),
    barcode: typeof input['barcode'] === 'string' ? input['barcode'] : null,
    price: typeof input['price'] === 'string' ? input['price'] : (defaults?.price ?? null),
    compareAtPrice:
      typeof input['compareAtPrice'] === 'string' ? input['compareAtPrice'] : (defaults?.compareAtPrice ?? null),
    taxable: typeof input['taxable'] === 'boolean' ? input['taxable'] : (defaults?.taxable ?? null),
    inventoryPolicy:
      typeof input['inventoryPolicy'] === 'string' ? input['inventoryPolicy'] : (defaults?.inventoryPolicy ?? null),
    inventoryQuantity: readVariantInventoryQuantity(input, 0),
    selectedOptions,
    inventoryItem: hasOwnField(input, 'inventoryItem')
      ? readInventoryItemInput(runtime, input['inventoryItem'], null)
      : defaultInventoryItem,
  };
}

function selectedOptionsKey(selectedOptions: ProductVariantRecord['selectedOptions']): string {
  return selectedOptions.map((selectedOption) => `${selectedOption.name}\u0000${selectedOption.value}`).join('\u0001');
}

function buildSelectedOptionCombinations(options: ProductOptionRecord[]): ProductVariantRecord['selectedOptions'][] {
  return options.reduce<ProductVariantRecord['selectedOptions'][]>(
    (combinations, option) => {
      const valueNames = option.optionValues
        .map((optionValue) => optionValue.name)
        .filter((valueName) => valueName.trim().length > 0);
      if (valueNames.length === 0) {
        return combinations;
      }

      return combinations.flatMap((combination) =>
        valueNames.map((valueName) => [
          ...combination,
          {
            name: option.name,
            value: valueName,
          },
        ]),
      );
    },
    [[]],
  );
}

function fillMissingVariantOptionSelections(
  variants: ProductVariantRecord[],
  options: ProductOptionRecord[],
): ProductVariantRecord[] {
  return variants.map((variant) => {
    const selectedByName = new Map(
      variant.selectedOptions.map((selectedOption) => [selectedOption.name, selectedOption.value]),
    );
    const selectedOptions = options
      .map((option) => {
        const selectedValue = selectedByName.get(option.name) ?? option.optionValues[0]?.name ?? null;
        return typeof selectedValue === 'string' && selectedValue.trim()
          ? {
              name: option.name,
              value: selectedValue,
            }
          : null;
      })
      .filter(
        (selectedOption): selectedOption is ProductVariantRecord['selectedOptions'][number] => selectedOption !== null,
      );

    return {
      ...structuredClone(variant),
      title: deriveVariantTitle(null, selectedOptions, variant.title),
      selectedOptions,
    };
  });
}

function createVariantsForOptionValueCombinations(
  runtime: ProxyRuntimeContext,
  productId: string,
  options: ProductOptionRecord[],
  existingVariants: ProductVariantRecord[],
): ProductVariantRecord[] {
  const combinations = buildSelectedOptionCombinations(options);
  if (combinations.length === 0) {
    return existingVariants;
  }

  const existingVariantsWithSelections = fillMissingVariantOptionSelections(existingVariants, options);
  const variantsBySelectedOptions = new Map(
    existingVariantsWithSelections.map((variant) => [selectedOptionsKey(variant.selectedOptions), variant]),
  );
  const defaultVariant = existingVariants[0] ?? null;

  const createdVariants = combinations
    .filter((selectedOptions) => !variantsBySelectedOptions.has(selectedOptionsKey(selectedOptions)))
    .map((selectedOptions) => makeCreatedVariantRecord(runtime, productId, { selectedOptions }, defaultVariant));

  return [...existingVariantsWithSelections, ...createdVariants];
}

function productHasStandaloneDefaultVariant(options: ProductOptionRecord[], variants: ProductVariantRecord[]): boolean {
  return productUsesOnlyDefaultOptionState(options, variants) && variants[0]?.title === 'Default Title';
}

function makeCreatedProductSetVariantRecord(
  runtime: ProxyRuntimeContext,
  productId: string,
  input: Record<string, unknown>,
): ProductVariantRecord {
  const variant = makeCreatedVariantRecord(runtime, productId, input);

  return applyProductSetInventoryQuantitiesToVariant(
    runtime,
    {
      ...variant,
      taxable: variant.taxable ?? true,
      inventoryPolicy: variant.inventoryPolicy ?? 'DENY',
      inventoryItem: variant.inventoryItem
        ? {
            ...variant.inventoryItem,
            measurement: variant.inventoryItem.measurement ?? makeDefaultInventoryItemMeasurement(),
          }
        : null,
    },
    input,
  );
}

function updateProductSetVariantRecord(
  runtime: ProxyRuntimeContext,
  existing: ProductVariantRecord,
  input: Record<string, unknown>,
): ProductVariantRecord {
  return applyProductSetInventoryQuantitiesToVariant(runtime, updateVariantRecord(runtime, existing, input), input);
}

function updateVariantRecord(
  runtime: ProxyRuntimeContext,
  existing: ProductVariantRecord,
  input: Record<string, unknown>,
): ProductVariantRecord {
  const selectedOptions = readVariantSelectedOptions(runtime, input, existing.productId, existing.selectedOptions);

  return {
    id: existing.id,
    productId: existing.productId,
    title: deriveVariantTitle(input['title'], selectedOptions, existing.title),
    sku: readVariantSku(input, existing.sku),
    barcode: typeof input['barcode'] === 'string' ? input['barcode'] : existing.barcode,
    price: typeof input['price'] === 'string' ? input['price'] : existing.price,
    compareAtPrice: typeof input['compareAtPrice'] === 'string' ? input['compareAtPrice'] : existing.compareAtPrice,
    taxable: typeof input['taxable'] === 'boolean' ? input['taxable'] : existing.taxable,
    inventoryPolicy: typeof input['inventoryPolicy'] === 'string' ? input['inventoryPolicy'] : existing.inventoryPolicy,
    inventoryQuantity: readVariantInventoryQuantity(input, existing.inventoryQuantity),
    selectedOptions,
    inventoryItem: hasOwnField(input, 'inventoryItem')
      ? readInventoryItemInput(runtime, input['inventoryItem'], existing.inventoryItem)
      : structuredClone(existing.inventoryItem),
  };
}

type BulkVariantUserError = {
  field: string[] | null;
  message: string;
};

function readBulkVariantInputs(raw: unknown): Record<string, unknown>[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw.filter((value): value is Record<string, unknown> => isObject(value));
}

function hasVariantOptionInput(input: Record<string, unknown>): boolean {
  return hasOwnField(input, 'selectedOptions') || hasOwnField(input, 'optionValues') || hasOwnField(input, 'options');
}

function bulkVariantOptionFieldName(input: Record<string, unknown>): string {
  if (hasOwnField(input, 'optionValues')) {
    return 'optionValues';
  }

  if (hasOwnField(input, 'selectedOptions')) {
    return 'selectedOptions';
  }

  return 'options';
}

function validateBulkVariantOptionInput(
  runtime: ProxyRuntimeContext,
  productId: string,
  input: Record<string, unknown>,
  index: number,
  mode: 'create' | 'update',
): {
  selectedOptions: ProductVariantRecord['selectedOptions'];
  userErrors: BulkVariantUserError[];
} {
  const selectedOptions = readVariantSelectedOptions(runtime, input, productId);
  const userErrors: BulkVariantUserError[] = [];
  const productOptions = runtime.store.getEffectiveOptionsByProductId(productId);
  const optionFieldName = bulkVariantOptionFieldName(input);
  const seenOptionNames = new Set<string>();

  for (const [optionIndex, selectedOption] of selectedOptions.entries()) {
    if (seenOptionNames.has(selectedOption.name)) {
      userErrors.push({
        field: ['variants', String(index), optionFieldName],
        message: `Duplicated option name '${selectedOption.name}'`,
      });
      return { selectedOptions, userErrors };
    }
    seenOptionNames.add(selectedOption.name);

    const productOption = productOptions.find((option) => option.name === selectedOption.name);
    if (productOptions.length > 0 && !productOption) {
      userErrors.push({
        field: ['variants', String(index), optionFieldName, String(optionIndex)],
        message: 'Option does not exist',
      });
      return { selectedOptions, userErrors };
    }
  }

  const shouldRequireCompleteOptionSet = mode === 'create' || hasVariantOptionInput(input);
  if (shouldRequireCompleteOptionSet && productOptions.length > 0 && selectedOptions.length > 0) {
    const missingOption = productOptions.find((option) => !seenOptionNames.has(option.name));
    if (missingOption) {
      userErrors.push({
        field: ['variants', String(index)],
        message: `You need to add option values for ${missingOption.name}`,
      });
    }
  }

  return { selectedOptions, userErrors };
}

function validateBulkCreateVariantBatch(
  runtime: ProxyRuntimeContext,
  productId: string,
  inputs: Record<string, unknown>[],
): BulkVariantUserError[] {
  const userErrors: BulkVariantUserError[] = [];

  for (const [index, input] of inputs.entries()) {
    const validation = validateBulkVariantOptionInput(runtime, productId, input, index, 'create');
    if (validation.userErrors.length > 0) {
      userErrors.push(...validation.userErrors);
      continue;
    }

    const rawInventoryQuantities = input['inventoryQuantities'];
    if (!Array.isArray(rawInventoryQuantities)) {
      continue;
    }

    const invalidInventoryLocation = rawInventoryQuantities.some((rawQuantity) => {
      if (!isObject(rawQuantity)) {
        return false;
      }

      const locationId = rawQuantity['locationId'];
      return (
        typeof locationId === 'string' &&
        locationId !== DEFAULT_INVENTORY_LEVEL_LOCATION_ID &&
        !findKnownLocationById(runtime, locationId)
      );
    });
    if (invalidInventoryLocation) {
      userErrors.push({
        field: ['variants', String(index), 'inventoryQuantities'],
        message: `Quantity for ${deriveVariantTitle(input['title'], validation.selectedOptions, 'Default Title')} couldn't be set because the location was deleted.`,
      });
    }
  }

  return userErrors;
}

function validateBulkUpdateVariantBatch(
  runtime: ProxyRuntimeContext,
  productId: string,
  inputs: Record<string, unknown>[],
  variantsById: Map<string, ProductVariantRecord>,
): BulkVariantUserError[] {
  if (inputs.length === 0) {
    return [{ field: null, message: 'Something went wrong, please try again.' }];
  }

  const userErrors: BulkVariantUserError[] = [];
  for (const [index, input] of inputs.entries()) {
    const rawVariantId = input['id'];
    if (typeof rawVariantId !== 'string') {
      userErrors.push({
        field: ['variants', String(index), 'id'],
        message: 'Product variant is missing ID attribute',
      });
      continue;
    }

    if (!variantsById.has(rawVariantId)) {
      userErrors.push({
        field: ['variants', String(index), 'id'],
        message: 'Product variant does not exist',
      });
      continue;
    }

    if (hasOwnField(input, 'inventoryQuantities')) {
      userErrors.push({
        field: ['variants', String(index), 'inventoryQuantities'],
        message:
          'Inventory quantities can only be provided during create. To update inventory for existing variants, use inventoryAdjustQuantities.',
      });
      continue;
    }

    if (hasVariantOptionInput(input)) {
      userErrors.push(...validateBulkVariantOptionInput(runtime, productId, input, index, 'update').userErrors);
    }
  }

  return userErrors;
}

function isKnownMissingShopifyGid(id: string): boolean {
  return /\/9{12,}(?:$|\?)/u.test(id);
}

function sumVariantInventory(variants: ProductVariantRecord[]): number | null {
  const quantities = variants
    .map((variant) => variant.inventoryQuantity)
    .filter((inventoryQuantity): inventoryQuantity is number => typeof inventoryQuantity === 'number');

  if (quantities.length === 0) {
    return null;
  }

  return quantities.reduce((total, quantity) => total + quantity, 0);
}

function deriveTracksInventory(variants: ProductVariantRecord[]): boolean | null {
  const trackedValues = variants
    .map((variant) => variant.inventoryItem?.tracked)
    .filter((tracked): tracked is boolean => typeof tracked === 'boolean');
  if (trackedValues.length > 0) {
    return trackedValues.some((tracked) => tracked);
  }

  return variants.some((variant) => variant.inventoryQuantity !== null) ? true : null;
}

function syncProductOptionsWithVariants(
  runtime: ProxyRuntimeContext,
  productId: string,
  options: ProductOptionRecord[] = runtime.store.getEffectiveOptionsByProductId(productId),
  variants: ProductVariantRecord[] = runtime.store.getEffectiveVariantsByProductId(productId),
): ProductOptionRecord[] {
  const nextOptions = structuredClone(options) as ProductOptionRecord[];
  const optionOrder = new Map(nextOptions.map((option, index) => [option.name, index]));
  const usedOptionValueKeys = new Set<string>();
  let hasDefaultTitleVariant = false;

  for (const variant of variants) {
    if (variant.selectedOptions.length === 0) {
      hasDefaultTitleVariant = true;
      continue;
    }

    for (const selectedOption of variant.selectedOptions) {
      let optionIndex = optionOrder.get(selectedOption.name) ?? -1;
      if (optionIndex < 0) {
        optionIndex = nextOptions.length;
        nextOptions.push({
          id: runtime.syntheticIdentity.makeSyntheticGid('ProductOption'),
          productId,
          name: selectedOption.name,
          position: optionIndex + 1,
          optionValues: [],
        });
        optionOrder.set(selectedOption.name, optionIndex);
      }

      const option = nextOptions[optionIndex]!;
      let optionValue = option.optionValues.find((candidate) => candidate.name === selectedOption.value);
      if (!optionValue) {
        optionValue = {
          id: runtime.syntheticIdentity.makeSyntheticGid('ProductOptionValue'),
          name: selectedOption.value,
          hasVariants: false,
        };
        option.optionValues = [...option.optionValues, optionValue];
      }

      usedOptionValueKeys.add(`${option.name}::${selectedOption.value}`);
    }
  }

  return nextOptions.map((option, index) => ({
    ...option,
    position: index + 1,
    optionValues: option.optionValues.map((optionValue) => ({
      ...optionValue,
      hasVariants:
        usedOptionValueKeys.has(`${option.name}::${optionValue.name}`) ||
        (hasDefaultTitleVariant && option.name === 'Title' && optionValue.name === 'Default Title'),
    })),
  }));
}

function reorderOptionValues(
  option: ProductOptionRecord,
  rawValues: unknown,
): {
  option: ProductOptionRecord;
  requestedValueNames: string[];
  userErrors: Array<{ field: string[]; message: string }>;
} {
  if (!Array.isArray(rawValues)) {
    return { option: structuredClone(option), requestedValueNames: [], userErrors: [] };
  }

  const optionValues = option.optionValues.map((value) => structuredClone(value));
  const requestedValueNames: string[] = [];
  const userErrors: Array<{ field: string[]; message: string }> = [];

  rawValues.forEach((rawValue, index) => {
    if (!isObject(rawValue)) {
      return;
    }

    const rawId = rawValue['id'];
    const rawName = rawValue['name'];
    const value = optionValues.find(
      (value) =>
        (typeof rawId === 'string' && value.id === rawId) || (typeof rawName === 'string' && value.name === rawName),
    );
    if (!value) {
      userErrors.push({ field: ['options', 'values', String(index)], message: 'Option value does not exist' });
      return;
    }

    requestedValueNames.push(value.name);
  });

  return {
    option: structuredClone(option),
    requestedValueNames,
    userErrors,
  };
}

function reorderProductOptionsAndVariants(
  runtime: ProxyRuntimeContext,
  productId: string,
  rawOptions: unknown,
): {
  options: ProductOptionRecord[];
  variants: ProductVariantRecord[];
  userErrors: Array<{ field: string[]; message: string }>;
} {
  const effectiveOptions = runtime.store.getEffectiveOptionsByProductId(productId);
  const remainingOptions = effectiveOptions.map((option) => structuredClone(option));
  const reorderedOptions: ProductOptionRecord[] = [];
  const requestedValueOrderByOptionName = new Map<string, Map<string, number>>();
  const userErrors: Array<{ field: string[]; message: string }> = [];

  if (!Array.isArray(rawOptions)) {
    return {
      options: effectiveOptions,
      variants: runtime.store.getEffectiveVariantsByProductId(productId),
      userErrors: [{ field: ['options'], message: 'Options are required' }],
    };
  }

  rawOptions.forEach((rawOption, index) => {
    if (!isObject(rawOption)) {
      return;
    }

    const rawId = rawOption['id'];
    const rawName = rawOption['name'];
    const optionIndex = remainingOptions.findIndex(
      (option) =>
        (typeof rawId === 'string' && option.id === rawId) || (typeof rawName === 'string' && option.name === rawName),
    );
    if (optionIndex < 0) {
      userErrors.push({ field: ['options', String(index)], message: 'Option does not exist' });
      return;
    }

    const [option] = remainingOptions.splice(optionIndex, 1);
    if (!option) {
      return;
    }

    const valueResult = reorderOptionValues(option, rawOption['values']);
    userErrors.push(...valueResult.userErrors);
    if (valueResult.requestedValueNames.length > 0) {
      requestedValueOrderByOptionName.set(
        option.name,
        new Map(valueResult.requestedValueNames.map((valueName, valueIndex) => [valueName, valueIndex])),
      );
    }
    reorderedOptions.push(valueResult.option);
  });

  const nextOptions = normalizeOptionPositions([...reorderedOptions, ...remainingOptions]);
  const valueOrderByOptionName = new Map(
    nextOptions.map((option) => [
      option.name,
      requestedValueOrderByOptionName.get(option.name) ??
        new Map(option.optionValues.map((optionValue, valueIndex) => [optionValue.name, valueIndex])),
    ]),
  );
  const remappedVariants = reorderVariantSelectionsForOptions(
    runtime.store.getEffectiveVariantsByProductId(productId),
    nextOptions,
  );
  const nextVariants = remappedVariants
    .map((variant) => ({
      ...variant,
      title: deriveVariantTitle(null, variant.selectedOptions, variant.title),
    }))
    .sort((left, right) => {
      for (const option of nextOptions) {
        const valueOrder = valueOrderByOptionName.get(option.name);
        const leftValue = left.selectedOptions.find((selectedOption) => selectedOption.name === option.name)?.value;
        const rightValue = right.selectedOptions.find((selectedOption) => selectedOption.name === option.name)?.value;
        const leftIndex =
          leftValue && valueOrder ? (valueOrder.get(leftValue) ?? Number.POSITIVE_INFINITY) : Number.POSITIVE_INFINITY;
        const rightIndex =
          rightValue && valueOrder
            ? (valueOrder.get(rightValue) ?? Number.POSITIVE_INFINITY)
            : Number.POSITIVE_INFINITY;
        if (leftIndex !== rightIndex) {
          return leftIndex - rightIndex;
        }
      }

      return left.id.localeCompare(right.id);
    });

  return {
    options: nextOptions,
    variants: nextVariants,
    userErrors,
  };
}

function syncProductInventorySummary(runtime: ProxyRuntimeContext, productId: string): ProductRecord | null {
  const existingProduct = runtime.store.getEffectiveProductById(productId);
  if (!existingProduct) {
    return null;
  }

  const effectiveVariants = runtime.store.getEffectiveVariantsByProductId(productId);
  const nextProduct: ProductRecord = {
    ...structuredClone(existingProduct),
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    totalInventory: sumVariantInventory(effectiveVariants),
    tracksInventory: deriveTracksInventory(effectiveVariants),
  };

  runtime.store.stageUpdateProduct(nextProduct);
  return runtime.store.getEffectiveProductById(productId);
}

function sumProductSetCreateInventory(variants: ProductVariantRecord[]): number | null {
  const quantities = variants
    .filter((variant) => variant.inventoryItem?.tracked !== false)
    .map((variant) => variant.inventoryQuantity)
    .filter((inventoryQuantity): inventoryQuantity is number => typeof inventoryQuantity === 'number');

  if (quantities.length === 0) {
    return null;
  }

  return quantities.reduce((total, quantity) => total + quantity, 0);
}

function syncProductSetInventorySummary(
  runtime: ProxyRuntimeContext,
  productId: string,
  previousProduct: ProductRecord | null,
): ProductRecord | null {
  const existingProduct = runtime.store.getEffectiveProductById(productId);
  if (!existingProduct) {
    return null;
  }

  const effectiveVariants = runtime.store.getEffectiveVariantsByProductId(productId);
  const nextProduct: ProductRecord = {
    ...structuredClone(existingProduct),
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    totalInventory: previousProduct ? previousProduct.totalInventory : sumProductSetCreateInventory(effectiveVariants),
    tracksInventory: deriveTracksInventory(effectiveVariants),
  };

  runtime.store.stageUpdateProduct(nextProduct);
  return runtime.store.getEffectiveProductById(productId);
}

interface InventoryAdjustmentChangeInputRecord {
  inventoryItemId: string | null;
  locationId: string | null;
  ledgerDocumentUri: string | null;
  delta: number | null;
  changeFromQuantity: number | null;
}

interface InventoryAdjustmentChangeRecord {
  inventoryItemId: string;
  locationId: string | null;
  ledgerDocumentUri: string | null;
  delta: number;
  name: string;
  quantityAfterChange: number | null;
}

interface InventoryAdjustmentAppRecord {
  id: string | null;
  title: string | null;
  handle: string | null;
  apiKey: string | null;
}

interface InventoryAdjustmentGroupRecord {
  id: string;
  createdAt: string;
  reason: string;
  referenceDocumentUri: string | null;
  app: InventoryAdjustmentAppRecord | null;
  changes: InventoryAdjustmentChangeRecord[];
}

interface InventoryAdjustInputProblemRecord {
  path: Array<string | number>;
  explanation: string;
}

interface InventoryMutationUserError {
  field: string[] | null;
  message: string;
  code?: string | null;
}

interface InventoryLevelTargetRecord {
  variant: ProductVariantRecord;
  level: NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']>[number];
}

interface InventorySetQuantityInputRecord {
  inventoryItemId: string | null;
  locationId: string | null;
  quantity: number | null;
  compareQuantity: number | null;
  changeFromQuantity: number | null;
}

interface InventoryMoveQuantityTerminalInputRecord {
  locationId: string | null;
  name: string | null;
  ledgerDocumentUri: string | null;
}

interface InventoryMoveQuantityChangeInputRecord {
  inventoryItemId: string | null;
  quantity: number | null;
  from: InventoryMoveQuantityTerminalInputRecord;
  to: InventoryMoveQuantityTerminalInputRecord;
}

const INVENTORY_ADJUSTMENT_STAFF_MEMBER_REQUIRED_ACCESS =
  '`read_users` access scope. Also: The app must be a finance embedded app or installed on a Shopify Plus or Advanced store. Contact Shopify Support to enable this scope for your app.';

const INVENTORY_QUANTITY_NAME_DEFINITIONS: Array<{
  name: string;
  displayName: string;
  isInUse: boolean;
  belongsTo: string[];
  comprises: string[];
}> = [
  {
    name: 'available',
    displayName: 'Available',
    isInUse: true,
    belongsTo: ['on_hand'],
    comprises: [],
  },
  {
    name: 'committed',
    displayName: 'Committed',
    isInUse: true,
    belongsTo: ['on_hand'],
    comprises: [],
  },
  {
    name: 'damaged',
    displayName: 'Damaged',
    isInUse: false,
    belongsTo: ['on_hand'],
    comprises: [],
  },
  {
    name: 'incoming',
    displayName: 'Incoming',
    isInUse: false,
    belongsTo: [],
    comprises: [],
  },
  {
    name: 'on_hand',
    displayName: 'On hand',
    isInUse: true,
    belongsTo: [],
    comprises: ['available', 'committed', 'damaged', 'quality_control', 'reserved', 'safety_stock'],
  },
  {
    name: 'quality_control',
    displayName: 'Quality control',
    isInUse: false,
    belongsTo: ['on_hand'],
    comprises: [],
  },
  {
    name: 'reserved',
    displayName: 'Reserved',
    isInUse: true,
    belongsTo: ['on_hand'],
    comprises: [],
  },
  {
    name: 'safety_stock',
    displayName: 'Safety stock',
    isInUse: false,
    belongsTo: ['on_hand'],
    comprises: [],
  },
] as const;

const INVENTORY_STAGED_QUANTITY_NAMES: Set<string> = new Set(
  INVENTORY_QUANTITY_NAME_DEFINITIONS.filter((definition) => definition.name !== 'on_hand').map(
    (definition) => definition.name,
  ),
);

function adminApiVersionAtLeast(apiVersion: string | null | undefined, minimum: string): boolean {
  const versionMatch = apiVersion?.match(/^(\d{4})-(\d{2})$/u);
  const minimumMatch = minimum.match(/^(\d{4})-(\d{2})$/u);
  if (!versionMatch || !minimumMatch) {
    return false;
  }

  const versionYear = Number.parseInt(versionMatch[1]!, 10);
  const versionMonth = Number.parseInt(versionMatch[2]!, 10);
  const minimumYear = Number.parseInt(minimumMatch[1]!, 10);
  const minimumMonth = Number.parseInt(minimumMatch[2]!, 10);

  return versionYear > minimumYear || (versionYear === minimumYear && versionMonth >= minimumMonth);
}

function buildInventoryAdjustInvalidVariableError(
  fieldPath: string,
  value: Record<string, unknown>,
  problemPath: Array<string | number>,
): {
  errors: Array<{
    message: string;
    extensions: {
      code: 'INVALID_VARIABLE';
      value: Record<string, unknown>;
      problems: InventoryAdjustInputProblemRecord[];
    };
  }>;
} {
  return {
    errors: [
      {
        message: `Variable $input of type InventoryAdjustQuantitiesInput! was provided invalid value for ${fieldPath} (Expected value to not be null)`,
        extensions: {
          code: 'INVALID_VARIABLE',
          value: structuredClone(value),
          problems: [{ path: problemPath, explanation: 'Expected value to not be null' }],
        },
      },
    ],
  };
}

function buildInventoryFieldNotDefinedVariableError(
  field: FieldNode,
  inputType: string,
  fieldPath: string,
  value: Record<string, unknown>,
  problemPath: Array<string | number>,
): {
  errors: Array<{
    message: string;
    locations?: GraphqlErrorLocation[];
    extensions: {
      code: 'INVALID_VARIABLE';
      value: Record<string, unknown>;
      problems: InventoryAdjustInputProblemRecord[];
    };
  }>;
} {
  return {
    errors: [
      {
        message: `Variable $input of type ${inputType}! was provided invalid value for ${fieldPath} (Field is not defined on ${inputType})`,
        ...(field.loc ? { locations: [{ line: field.loc.startToken.line, column: field.loc.startToken.column }] } : {}),
        extensions: {
          code: 'INVALID_VARIABLE',
          value: structuredClone(value),
          problems: [{ path: problemPath, explanation: `Field is not defined on ${inputType}` }],
        },
      },
    ],
  };
}

function buildInventoryMissingFieldArgumentError(
  field: FieldNode,
  responseKey: string,
  inputType: 'InventoryChangeInput' | 'InventoryQuantityInput',
): {
  errors: Array<{
    message: string;
    locations?: GraphqlErrorLocation[];
    extensions: { code: 'INVALID_FIELD_ARGUMENTS' };
    path: string[];
  }>;
  data: Record<string, null>;
} {
  return {
    errors: [
      {
        message: `${inputType} must include the following argument: changeFromQuantity.`,
        ...(field.loc ? { locations: [{ line: field.loc.startToken.line, column: field.loc.startToken.column }] } : {}),
        extensions: {
          code: 'INVALID_FIELD_ARGUMENTS',
        },
        path: [responseKey],
      },
    ],
    data: {
      [responseKey]: null,
    },
  };
}

function buildInventoryAdjustmentStaffMemberAccessDeniedError(
  rootField: FieldNode,
  groupField: FieldNode,
  staffMemberField: FieldNode,
): Record<string, unknown> {
  const location = staffMemberField.loc ? getLocation(staffMemberField.loc.source, staffMemberField.loc.start) : null;
  return {
    message: `Access denied for staffMember field. Required access: ${INVENTORY_ADJUSTMENT_STAFF_MEMBER_REQUIRED_ACCESS}`,
    ...(location ? { locations: [{ line: location.line, column: location.column }] } : {}),
    extensions: {
      code: 'ACCESS_DENIED',
      documentation: 'https://shopify.dev/api/usage/access-scopes',
      requiredAccess: INVENTORY_ADJUSTMENT_STAFF_MEMBER_REQUIRED_ACCESS,
    },
    path: [getResponseKey(rootField), getResponseKey(groupField), getResponseKey(staffMemberField)],
  };
}

function buildNullProductChangeStatusArgumentError(
  locations: GraphqlErrorLocation[],
  operationPathLabel: string,
): {
  errors: Array<{
    message: string;
    locations: GraphqlErrorLocation[];
    path: string[];
    extensions: {
      code: 'argumentLiteralsIncompatible';
      typeName: 'Field';
      argumentName: 'productId';
    };
  }>;
} {
  return {
    errors: [
      {
        message:
          "Argument 'productId' on Field 'productChangeStatus' has an invalid value (null). Expected type 'ID!'.",
        locations,
        path: [operationPathLabel, 'productChangeStatus', 'productId'],
        extensions: {
          code: 'argumentLiteralsIncompatible',
          typeName: 'Field',
          argumentName: 'productId',
        },
      },
    ],
  };
}

function buildProductDeleteInvalidVariableError(
  input: Record<string, unknown>,
  locations: GraphqlErrorLocation[],
): {
  errors: Array<{
    message: string;
    locations: GraphqlErrorLocation[];
    extensions: {
      code: 'INVALID_VARIABLE';
      value: Record<string, unknown>;
      problems: Array<{ path: string[]; explanation: 'Expected value to not be null' }>;
    };
  }>;
} {
  return {
    errors: [
      {
        message:
          'Variable $input of type ProductDeleteInput! was provided invalid value for id (Expected value to not be null)',
        locations,
        extensions: {
          code: 'INVALID_VARIABLE',
          value: structuredClone(input),
          problems: [{ path: ['id'], explanation: 'Expected value to not be null' }],
        },
      },
    ],
  };
}

function buildMetafieldsDeleteInvalidVariableError(
  metafields: unknown[],
  fieldPath: string,
  problemPath: Array<string | number>,
  locations: GraphqlErrorLocation[],
): {
  errors: Array<{
    message: string;
    locations: GraphqlErrorLocation[];
    extensions: {
      code: 'INVALID_VARIABLE';
      value: unknown[];
      problems: Array<{ path: Array<string | number>; explanation: 'Expected value to not be null' }>;
    };
  }>;
} {
  return {
    errors: [
      {
        message: `Variable $metafields of type [MetafieldIdentifierInput!]! was provided invalid value for ${fieldPath} (Expected value to not be null)`,
        locations,
        extensions: {
          code: 'INVALID_VARIABLE',
          value: structuredClone(metafields),
          problems: [{ path: problemPath, explanation: 'Expected value to not be null' }],
        },
      },
    ],
  };
}

function buildMissingProductDeleteInputIdArgumentError(locations: GraphqlErrorLocation[]): {
  errors: Array<{
    message: string;
    locations: GraphqlErrorLocation[];
    path: string[];
    extensions: {
      code: 'missingRequiredInputObjectAttribute';
      argumentName: 'id';
      argumentType: 'ID!';
      inputObjectType: 'ProductDeleteInput';
    };
  }>;
} {
  return {
    errors: [
      {
        message: "Argument 'id' on InputObject 'ProductDeleteInput' is required. Expected type ID!",
        locations,
        path: ['mutation', 'productDelete', 'input', 'id'],
        extensions: {
          code: 'missingRequiredInputObjectAttribute',
          argumentName: 'id',
          argumentType: 'ID!',
          inputObjectType: 'ProductDeleteInput',
        },
      },
    ],
  };
}

function buildNullProductDeleteInputIdArgumentError(locations: GraphqlErrorLocation[]): {
  errors: Array<{
    message: string;
    locations: GraphqlErrorLocation[];
    path: string[];
    extensions: {
      code: 'argumentLiteralsIncompatible';
      argumentName: 'id';
      typeName: 'InputObject';
    };
  }>;
} {
  return {
    errors: [
      {
        message: "Argument 'id' on InputObject 'ProductDeleteInput' has an invalid value (null). Expected type 'ID!'.",
        locations,
        path: ['mutation', 'productDelete', 'input', 'id'],
        extensions: {
          code: 'argumentLiteralsIncompatible',
          argumentName: 'id',
          typeName: 'InputObject',
        },
      },
    ],
  };
}

function validateInventoryAdjustRequiredFields(input: Record<string, unknown>): {
  errors: Array<{
    message: string;
    extensions: {
      code: 'INVALID_VARIABLE';
      value: Record<string, unknown>;
      problems: InventoryAdjustInputProblemRecord[];
    };
  }>;
} | null {
  const rawChanges = input['changes'];
  if (!Array.isArray(rawChanges)) {
    return null;
  }

  for (const [index, rawChange] of rawChanges.entries()) {
    if (!isObject(rawChange)) {
      continue;
    }

    if (!hasOwnField(rawChange, 'inventoryItemId')) {
      return buildInventoryAdjustInvalidVariableError(`changes.${index}.inventoryItemId`, input, [
        'changes',
        index,
        'inventoryItemId',
      ]);
    }

    if (!hasOwnField(rawChange, 'delta')) {
      return buildInventoryAdjustInvalidVariableError(`changes.${index}.delta`, input, ['changes', index, 'delta']);
    }

    if (!hasOwnField(rawChange, 'locationId')) {
      return buildInventoryAdjustInvalidVariableError(`changes.${index}.locationId`, input, [
        'changes',
        index,
        'locationId',
      ]);
    }
  }

  return null;
}

function validateInventoryAdjust202604Input(
  input: Record<string, unknown>,
  field: FieldNode,
  responseKey: string,
): ReturnType<typeof buildInventoryMissingFieldArgumentError> | null {
  const rawChanges = input['changes'];
  if (!Array.isArray(rawChanges)) {
    return null;
  }

  for (const rawChange of rawChanges) {
    if (isObject(rawChange) && !hasOwnField(rawChange, 'changeFromQuantity')) {
      return buildInventoryMissingFieldArgumentError(field, responseKey, 'InventoryChangeInput');
    }
  }

  return null;
}

function validateInventorySet202604Input(
  input: Record<string, unknown>,
  field: FieldNode,
  responseKey: string,
):
  | ReturnType<typeof buildInventoryFieldNotDefinedVariableError>
  | ReturnType<typeof buildInventoryMissingFieldArgumentError>
  | null {
  if (hasOwnField(input, 'ignoreCompareQuantity')) {
    return buildInventoryFieldNotDefinedVariableError(
      field,
      'InventorySetQuantitiesInput',
      'ignoreCompareQuantity',
      input,
      ['ignoreCompareQuantity'],
    );
  }

  const rawQuantities = input['quantities'];
  if (!Array.isArray(rawQuantities)) {
    return null;
  }

  for (const [quantityIndex, rawQuantity] of rawQuantities.entries()) {
    if (!isObject(rawQuantity)) {
      continue;
    }

    if (hasOwnField(rawQuantity, 'compareQuantity')) {
      return buildInventoryFieldNotDefinedVariableError(
        field,
        'InventorySetQuantitiesInput',
        `quantities.${quantityIndex}.compareQuantity`,
        input,
        ['quantities', quantityIndex, 'compareQuantity'],
      );
    }

    if (!hasOwnField(rawQuantity, 'changeFromQuantity')) {
      return buildInventoryMissingFieldArgumentError(field, responseKey, 'InventoryQuantityInput');
    }
  }

  return null;
}

function readInventoryAdjustmentChangeInputs(raw: unknown): InventoryAdjustmentChangeInputRecord[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw.map((change) => {
    const value = readProductInput(change);
    return {
      inventoryItemId: typeof value['inventoryItemId'] === 'string' ? value['inventoryItemId'] : null,
      locationId: typeof value['locationId'] === 'string' ? value['locationId'] : null,
      ledgerDocumentUri: typeof value['ledgerDocumentUri'] === 'string' ? value['ledgerDocumentUri'] : null,
      delta: typeof value['delta'] === 'number' ? value['delta'] : null,
      changeFromQuantity: typeof value['changeFromQuantity'] === 'number' ? value['changeFromQuantity'] : null,
    };
  });
}

function readInventorySetQuantityInputs(raw: unknown): InventorySetQuantityInputRecord[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw.map((quantity) => {
    const value = readProductInput(quantity);
    return {
      inventoryItemId: typeof value['inventoryItemId'] === 'string' ? value['inventoryItemId'] : null,
      locationId: typeof value['locationId'] === 'string' ? value['locationId'] : null,
      quantity: typeof value['quantity'] === 'number' ? value['quantity'] : null,
      compareQuantity: typeof value['compareQuantity'] === 'number' ? value['compareQuantity'] : null,
      changeFromQuantity: typeof value['changeFromQuantity'] === 'number' ? value['changeFromQuantity'] : null,
    };
  });
}

function readInventoryMoveTerminalInput(raw: unknown): InventoryMoveQuantityTerminalInputRecord {
  const value = readProductInput(raw);
  return {
    locationId: typeof value['locationId'] === 'string' ? value['locationId'] : null,
    name: typeof value['name'] === 'string' ? value['name'] : null,
    ledgerDocumentUri: typeof value['ledgerDocumentUri'] === 'string' ? value['ledgerDocumentUri'] : null,
  };
}

function readInventoryMoveQuantityChangeInputs(raw: unknown): InventoryMoveQuantityChangeInputRecord[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw.map((change) => {
    const value = readProductInput(change);
    return {
      inventoryItemId: typeof value['inventoryItemId'] === 'string' ? value['inventoryItemId'] : null,
      quantity: typeof value['quantity'] === 'number' ? value['quantity'] : null,
      from: readInventoryMoveTerminalInput(value['from']),
      to: readInventoryMoveTerminalInput(value['to']),
    };
  });
}

function serializeInventoryAdjustmentGroup(
  runtime: ProxyRuntimeContext,
  group: InventoryAdjustmentGroupRecord | null,
  field: FieldNode | null,
): Record<string, unknown> | null {
  if (!group) {
    return null;
  }

  const selections = field?.selectionSet?.selections ?? [];
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'id':
        result[key] = group.id;
        break;
      case 'createdAt':
        result[key] = group.createdAt;
        break;
      case 'reason':
        result[key] = group.reason;
        break;
      case 'referenceDocumentUri':
        result[key] = group.referenceDocumentUri;
        break;
      case 'app':
        result[key] = serializeInventoryAdjustmentApp(group.app, selection);
        break;
      case 'changes':
        result[key] = group.changes.map((change) => {
          const changeResult: Record<string, unknown> = {};
          const variant = runtime.store.findEffectiveVariantByInventoryItemId(change.inventoryItemId);
          for (const changeSelection of selection.selectionSet?.selections ?? []) {
            if (changeSelection.kind !== Kind.FIELD) {
              continue;
            }

            const changeKey = changeSelection.alias?.value ?? changeSelection.name.value;
            switch (changeSelection.name.value) {
              case 'name':
                changeResult[changeKey] = change.name;
                break;
              case 'delta':
                changeResult[changeKey] = change.delta;
                break;
              case 'quantityAfterChange':
                changeResult[changeKey] = change.quantityAfterChange;
                break;
              case 'ledgerDocumentUri':
                changeResult[changeKey] = change.ledgerDocumentUri;
                break;
              case 'item':
                changeResult[changeKey] = variant
                  ? serializeInventoryItemSelectionSet(runtime, variant, changeSelection.selectionSet?.selections ?? [])
                  : null;
                break;
              case 'location': {
                const location = change.locationId ? findKnownLocationById(runtime, change.locationId) : null;
                changeResult[changeKey] = Object.fromEntries(
                  (changeSelection.selectionSet?.selections ?? [])
                    .filter(
                      (locationSelection): locationSelection is FieldNode => locationSelection.kind === Kind.FIELD,
                    )
                    .map((locationSelection) => {
                      const locationKey = locationSelection.alias?.value ?? locationSelection.name.value;
                      switch (locationSelection.name.value) {
                        case 'id':
                          return [locationKey, change.locationId];
                        case 'name':
                          return [locationKey, location?.name ?? null];
                        default:
                          return [locationKey, null];
                      }
                    }),
                );
                break;
              }
              default:
                changeResult[changeKey] = null;
            }
          }
          return changeResult;
        });
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeInventoryAdjustmentApp(
  app: InventoryAdjustmentAppRecord | null,
  field: FieldNode,
): Record<string, unknown> | null {
  if (!app) {
    return null;
  }

  return Object.fromEntries(
    (field.selectionSet?.selections ?? [])
      .filter((selection): selection is FieldNode => selection.kind === Kind.FIELD)
      .map((selection) => {
        const key = selection.alias?.value ?? selection.name.value;
        switch (selection.name.value) {
          case 'id':
            return [key, app.id];
          case 'title':
            return [key, app.title];
          case 'handle':
            return [key, app.handle];
          case 'apiKey':
            return [key, app.apiKey];
          default:
            return [key, null];
        }
      }),
  );
}

function buildInventoryAdjustmentAppRecord(): InventoryAdjustmentAppRecord {
  const handle =
    typeof process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] === 'string'
      ? process.env['SHOPIFY_CONFORMANCE_APP_HANDLE']
      : null;
  const appId =
    typeof process.env['SHOPIFY_CONFORMANCE_APP_ID'] === 'string' ? process.env['SHOPIFY_CONFORMANCE_APP_ID'] : null;
  const apiKey =
    typeof process.env['SHOPIFY_API_KEY'] === 'string'
      ? process.env['SHOPIFY_API_KEY']
      : typeof process.env['SHOPIFY_CONFORMANCE_APP_API_KEY'] === 'string'
        ? process.env['SHOPIFY_CONFORMANCE_APP_API_KEY']
        : null;

  return {
    id: appId,
    title: handle,
    handle,
    apiKey,
  };
}

function isOnHandComponentQuantityName(name: string): boolean {
  return (
    INVENTORY_QUANTITY_NAME_DEFINITIONS.find((definition) => definition.name === name)?.belongsTo.includes('on_hand') ??
    false
  );
}

function getInventoryMutableVariant(
  runtime: ProxyRuntimeContext,
  variantsByProductId: Map<string, ProductVariantRecord[]>,
  inventoryItemId: string,
): { variant: ProductVariantRecord; variants: ProductVariantRecord[]; index: number } | null {
  const baseVariant = runtime.store.findEffectiveVariantByInventoryItemId(inventoryItemId);
  if (!baseVariant) {
    return null;
  }

  const variants =
    variantsByProductId.get(baseVariant.productId) ??
    runtime.store.getEffectiveVariantsByProductId(baseVariant.productId).map((candidate) => structuredClone(candidate));
  const index = variants.findIndex((candidate) => candidate.inventoryItem?.id === inventoryItemId);
  if (index < 0) {
    return null;
  }

  variantsByProductId.set(baseVariant.productId, variants);
  return { variant: variants[index]!, variants, index };
}

function getInventoryMutableLevel(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  locationId: string,
): {
  inventoryItem: NonNullable<ProductVariantRecord['inventoryItem']>;
  levels: InventoryLevelRecord[];
  level: InventoryLevelRecord;
  index: number;
} | null {
  if (!variant.inventoryItem) {
    return null;
  }

  const inventoryItem = structuredClone(variant.inventoryItem);
  const levels =
    inventoryItem.inventoryLevels && inventoryItem.inventoryLevels.length > 0
      ? structuredClone(inventoryItem.inventoryLevels)
      : buildSyntheticInventoryLevels(runtime, { ...variant, inventoryItem });
  const existingIndex = levels.findIndex((level) => level.location?.id === locationId);
  if (existingIndex >= 0) {
    return { inventoryItem, levels, level: levels[existingIndex]!, index: existingIndex };
  }

  const knownLocation = findKnownLocationById(runtime, locationId);
  if (!knownLocation) {
    return null;
  }

  const nextLevel = buildSyntheticInventoryLevel(
    runtime,
    { ...variant, inventoryItem },
    {
      locationId,
      availableQuantity: 0,
    },
  );
  if (!nextLevel) {
    return null;
  }

  levels.push({
    ...nextLevel,
    location: {
      id: knownLocation.id,
      name: knownLocation.name,
    },
  });
  return { inventoryItem, levels, level: levels[levels.length - 1]!, index: levels.length - 1 };
}

function writeInventoryMutableLevel(
  mutable: {
    inventoryItem: NonNullable<ProductVariantRecord['inventoryItem']>;
    levels: InventoryLevelRecord[];
    level: InventoryLevelRecord;
    index: number;
  },
  quantities: InventoryLevelRecord['quantities'],
): InventoryLevelRecord[] {
  mutable.level = {
    ...mutable.level,
    quantities,
  };
  mutable.levels[mutable.index] = mutable.level;
  return mutable.levels;
}

function commitInventoryMutableVariant(
  variantsByProductId: Map<string, ProductVariantRecord[]>,
  variants: ProductVariantRecord[],
  index: number,
  variant: ProductVariantRecord,
  inventoryItem: NonNullable<ProductVariantRecord['inventoryItem']>,
  levels: InventoryLevelRecord[],
): ProductVariantRecord {
  const nextVariant: ProductVariantRecord = {
    ...variant,
    inventoryQuantity: sumAvailableInventoryLevels(levels),
    inventoryItem: {
      ...inventoryItem,
      inventoryLevels: levels,
    },
  };
  variants[index] = nextVariant;
  variantsByProductId.set(nextVariant.productId, variants);
  return nextVariant;
}

function validateInventoryQuantityName(name: string | null, field: string[]): InventoryMutationUserError | null {
  if (!name) {
    return { field, message: 'Inventory quantity name is required' };
  }

  if (!INVENTORY_STAGED_QUANTITY_NAMES.has(name)) {
    return {
      field,
      message:
        'The specified quantity name is invalid. Valid values are: available, damaged, incoming, quality_control, reserved, safety_stock.',
    };
  }

  return null;
}

function applyInventoryAdjustQuantities(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  options: { requireChangeFromQuantity?: boolean } = {},
):
  | { group: InventoryAdjustmentGroupRecord; userErrors: Array<{ field: string[]; message: string }> }
  | { group: null; userErrors: Array<{ field: string[]; message: string }> } {
  const name = typeof input['name'] === 'string' && input['name'].trim() ? input['name'] : null;
  if (!name) {
    return {
      group: null,
      userErrors: [{ field: ['input', 'name'], message: 'Inventory quantity name is required' }],
    };
  }

  const reason = typeof input['reason'] === 'string' && input['reason'].trim() ? input['reason'] : null;
  if (!reason) {
    return {
      group: null,
      userErrors: [{ field: ['input', 'reason'], message: 'Inventory adjustment reason is required' }],
    };
  }

  const changes = readInventoryAdjustmentChangeInputs(input['changes']);
  if (changes.length === 0) {
    return {
      group: null,
      userErrors: [{ field: ['input', 'changes'], message: 'At least one inventory adjustment is required' }],
    };
  }

  const validNames = ['available', 'damaged', 'incoming', 'quality_control', 'reserved', 'safety_stock'];
  const validationErrors: Array<{ field: string[]; message: string }> = [];

  if (!validNames.includes(name)) {
    validationErrors.push({
      field: ['input', 'name'],
      message:
        'The specified quantity name is invalid. Valid values are: available, damaged, incoming, quality_control, reserved, safety_stock.',
    });
  }

  if (name !== 'available') {
    changes.forEach((change, index) => {
      if (!change.ledgerDocumentUri) {
        validationErrors.push({
          field: ['input', 'changes', String(index), 'ledgerDocumentUri'],
          message: 'A ledger document URI is required except when adjusting available.',
        });
      }
    });
  }

  if (validationErrors.length > 0) {
    return {
      group: null,
      userErrors: validationErrors,
    };
  }

  const variantsByProductId = new Map<string, ProductVariantRecord[]>();
  const adjustedChanges: InventoryAdjustmentChangeRecord[] = [];
  const mirroredOnHandChanges: InventoryAdjustmentChangeRecord[] = [];

  for (const [changeIndex, change] of changes.entries()) {
    if (!change.inventoryItemId) {
      return {
        group: null,
        userErrors: [
          {
            field: ['input', 'changes', String(changeIndex), 'inventoryItemId'],
            message: 'Inventory item id is required',
          },
        ],
      };
    }

    if (typeof change.delta !== 'number') {
      return {
        group: null,
        userErrors: [
          { field: ['input', 'changes', String(changeIndex), 'delta'], message: 'Inventory delta is required' },
        ],
      };
    }

    const variant = runtime.store.findEffectiveVariantByInventoryItemId(change.inventoryItemId);
    if (!variant) {
      return {
        group: null,
        userErrors: [
          {
            field: ['input', 'changes', String(changeIndex), 'inventoryItemId'],
            message: 'The specified inventory item could not be found.',
          },
        ],
      };
    }

    const nextVariants =
      variantsByProductId.get(variant.productId) ??
      runtime.store.getEffectiveVariantsByProductId(variant.productId).map((candidate) => structuredClone(candidate));
    const variantIndex = nextVariants.findIndex((candidate) => candidate.id === variant.id);
    if (variantIndex < 0) {
      return {
        group: null,
        userErrors: [
          {
            field: ['input', 'changes', String(changeIndex), 'inventoryItemId'],
            message: 'The specified inventory item could not be found.',
          },
        ],
      };
    }

    const existingVariant = nextVariants[variantIndex]!;
    const explicitLevels = variant.inventoryItem?.inventoryLevels ?? null;
    if (
      change.locationId &&
      explicitLevels &&
      explicitLevels.length > 0 &&
      !explicitLevels.some((level) => level.location?.id === change.locationId)
    ) {
      return {
        group: null,
        userErrors: [
          {
            field: ['input', 'changes', String(changeIndex), 'locationId'],
            message: 'The specified location could not be found.',
          },
        ],
      };
    }

    const nextInventoryItem = existingVariant.inventoryItem
      ? structuredClone(existingVariant.inventoryItem)
      : {
          id: change.inventoryItemId,
          tracked: true,
          requiresShipping: null,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
          inventoryLevels: null,
        };
    const nextLevels =
      nextInventoryItem.inventoryLevels && nextInventoryItem.inventoryLevels.length > 0
        ? structuredClone(nextInventoryItem.inventoryLevels)
        : buildSyntheticInventoryLevels(runtime, { ...existingVariant, inventoryItem: nextInventoryItem });
    const levelIndex = nextLevels.findIndex((level) => level.location?.id === change.locationId);
    const targetLevel =
      levelIndex >= 0
        ? nextLevels[levelIndex]!
        : (buildSyntheticInventoryLevel(
            runtime,
            { ...existingVariant, inventoryItem: nextInventoryItem },
            {
              locationId: change.locationId,
              availableQuantity:
                change.locationId === DEFAULT_INVENTORY_LEVEL_LOCATION_ID
                  ? (existingVariant.inventoryQuantity ?? 0)
                  : 0,
            },
          ) ?? {
            id: `${runtime.syntheticIdentity.makeSyntheticGid('InventoryLevel')}?inventory_item_id=${encodeURIComponent(change.inventoryItemId)}`,
            cursor: null,
            location: change.locationId
              ? { id: change.locationId, name: null }
              : { id: DEFAULT_INVENTORY_LEVEL_LOCATION_ID, name: null },
            quantities: [],
          });
    const nextQuantities = targetLevel.quantities.map((quantity) => structuredClone(quantity));
    const previousQuantity = readInventoryQuantityAmount(nextQuantities, name);
    if (options.requireChangeFromQuantity && change.changeFromQuantity !== previousQuantity) {
      return {
        group: null,
        userErrors: [
          {
            field: ['input', 'changes', String(changeIndex), 'changeFromQuantity'],
            message: 'The specified compare quantity does not match the current quantity.',
          },
        ],
      };
    }

    const quantityIndex = nextQuantities.findIndex((quantity) => quantity.name === name);
    if (quantityIndex >= 0) {
      nextQuantities[quantityIndex] = {
        ...nextQuantities[quantityIndex]!,
        quantity: (nextQuantities[quantityIndex]!.quantity ?? 0) + change.delta,
        updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      };
    } else {
      nextQuantities.push({
        name,
        quantity: change.delta,
        updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      });
    }

    let nextVariant = {
      ...existingVariant,
      inventoryItem: {
        ...nextInventoryItem,
        inventoryLevels: nextLevels.map((level, candidateIndex) =>
          candidateIndex === levelIndex || (levelIndex < 0 && candidateIndex === nextLevels.length) ? level : level,
        ),
      },
    } satisfies ProductVariantRecord;

    const updatedLevel = {
      ...targetLevel,
      quantities: nextQuantities,
    };
    if (levelIndex >= 0) {
      nextLevels[levelIndex] = updatedLevel;
    } else {
      nextLevels.push(updatedLevel);
    }
    nextVariant = {
      ...nextVariant,
      inventoryItem: {
        ...nextInventoryItem,
        inventoryLevels: nextLevels,
      },
    };

    if (name === 'available') {
      const quantityAfterChange = (existingVariant.inventoryQuantity ?? 0) + change.delta;
      nextVariant = {
        ...nextVariant,
        inventoryQuantity: quantityAfterChange,
      };
      const onHandIndex = nextQuantities.findIndex((quantity) => quantity.name === 'on_hand');
      if (onHandIndex >= 0) {
        nextQuantities[onHandIndex] = {
          ...nextQuantities[onHandIndex]!,
          quantity: (nextQuantities[onHandIndex]!.quantity ?? 0) + change.delta,
        };
      }
      adjustedChanges.push({
        inventoryItemId: change.inventoryItemId,
        locationId: change.locationId,
        ledgerDocumentUri: change.ledgerDocumentUri,
        delta: change.delta,
        name,
        quantityAfterChange: null,
      });
      mirroredOnHandChanges.push({
        inventoryItemId: change.inventoryItemId,
        locationId: change.locationId,
        ledgerDocumentUri: null,
        delta: change.delta,
        name: 'on_hand',
        quantityAfterChange: null,
      });
    } else {
      adjustedChanges.push({
        inventoryItemId: change.inventoryItemId,
        locationId: change.locationId,
        ledgerDocumentUri: change.ledgerDocumentUri,
        delta: change.delta,
        name,
        quantityAfterChange: null,
      });
    }

    nextVariants[variantIndex] = nextVariant;
    variantsByProductId.set(variant.productId, nextVariants);
  }

  for (const [productId, nextVariants] of variantsByProductId.entries()) {
    runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);
  }

  adjustedChanges.push(...mirroredOnHandChanges);

  return {
    group: {
      id: runtime.syntheticIdentity.makeSyntheticGid('InventoryAdjustmentGroup'),
      createdAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      reason,
      referenceDocumentUri: typeof input['referenceDocumentUri'] === 'string' ? input['referenceDocumentUri'] : null,
      app: buildInventoryAdjustmentAppRecord(),
      changes: adjustedChanges,
    },
    userErrors: [],
  };
}

function applyInventorySetQuantities(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  options: { useChangeFromQuantity?: boolean } = {},
): {
  group: InventoryAdjustmentGroupRecord | null;
  userErrors: InventoryMutationUserError[];
} {
  const name = typeof input['name'] === 'string' && input['name'].trim() ? input['name'] : null;
  const nameError = validateInventoryQuantityName(name, ['input', 'name']);
  if (nameError || !name) {
    return {
      group: null,
      userErrors: [nameError ?? { field: ['input', 'name'], message: 'Inventory quantity name is required' }],
    };
  }
  const quantityName = name;

  const reason = typeof input['reason'] === 'string' && input['reason'].trim() ? input['reason'] : null;
  if (!reason) {
    return {
      group: null,
      userErrors: [{ field: ['input', 'reason'], message: 'Inventory adjustment reason is required' }],
    };
  }

  const quantities = readInventorySetQuantityInputs(input['quantities']);
  if (quantities.length === 0) {
    return {
      group: null,
      userErrors: [{ field: ['input', 'quantities'], message: 'At least one inventory quantity is required' }],
    };
  }

  const ignoreCompareQuantity = !options.useChangeFromQuantity && input['ignoreCompareQuantity'] === true;
  if (
    !options.useChangeFromQuantity &&
    !ignoreCompareQuantity &&
    quantities.some((quantity) => typeof quantity.compareQuantity !== 'number')
  ) {
    return {
      group: null,
      userErrors: [
        {
          field: ['input', 'ignoreCompareQuantity'],
          message:
            'The compareQuantity argument must be given to each quantity or ignored using ignoreCompareQuantity.',
        },
      ],
    };
  }

  const variantsByProductId = new Map<string, ProductVariantRecord[]>();
  const changes: InventoryAdjustmentChangeRecord[] = [];
  const mirroredOnHandChanges: InventoryAdjustmentChangeRecord[] = [];

  for (const [quantityIndex, quantityInput] of quantities.entries()) {
    if (!quantityInput.inventoryItemId) {
      return {
        group: null,
        userErrors: [
          {
            field: ['input', 'quantities', String(quantityIndex), 'inventoryItemId'],
            message: 'Inventory item id is required',
          },
        ],
      };
    }

    if (!quantityInput.locationId) {
      return {
        group: null,
        userErrors: [
          {
            field: ['input', 'quantities', String(quantityIndex), 'locationId'],
            message: 'Inventory location id is required',
          },
        ],
      };
    }

    if (typeof quantityInput.quantity !== 'number') {
      return {
        group: null,
        userErrors: [
          {
            field: ['input', 'quantities', String(quantityIndex), 'quantity'],
            message: 'Inventory quantity is required',
          },
        ],
      };
    }

    const mutableVariant = getInventoryMutableVariant(runtime, variantsByProductId, quantityInput.inventoryItemId);
    if (!mutableVariant) {
      return {
        group: null,
        userErrors: [
          {
            field: ['input', 'quantities', String(quantityIndex), 'inventoryItemId'],
            message: 'The specified inventory item could not be found.',
          },
        ],
      };
    }

    const mutableLevel = getInventoryMutableLevel(runtime, mutableVariant.variant, quantityInput.locationId);
    if (!mutableLevel) {
      return {
        group: null,
        userErrors: [
          {
            field: ['input', 'quantities', String(quantityIndex), 'locationId'],
            message: 'The specified location could not be found.',
          },
        ],
      };
    }

    const previousQuantity = readInventoryQuantityAmount(mutableLevel.level.quantities, quantityName);
    const expectedPreviousQuantity = options.useChangeFromQuantity
      ? quantityInput.changeFromQuantity
      : quantityInput.compareQuantity;
    if (!ignoreCompareQuantity && expectedPreviousQuantity !== previousQuantity) {
      return {
        group: null,
        userErrors: [
          {
            field: [
              'input',
              'quantities',
              String(quantityIndex),
              options.useChangeFromQuantity ? 'changeFromQuantity' : 'compareQuantity',
            ],
            message: 'The specified compare quantity does not match the current quantity.',
          },
        ],
      };
    }

    const delta = quantityInput.quantity - previousQuantity;
    let nextQuantities = writeInventoryQuantityAmount(
      runtime,
      mutableLevel.level.quantities,
      quantityName,
      quantityInput.quantity,
    );
    if (isOnHandComponentQuantityName(quantityName)) {
      nextQuantities = addInventoryQuantityAmount(runtime, nextQuantities, 'on_hand', delta);
      mirroredOnHandChanges.push({
        inventoryItemId: quantityInput.inventoryItemId,
        locationId: quantityInput.locationId,
        ledgerDocumentUri: null,
        delta,
        name: 'on_hand',
        quantityAfterChange: null,
      });
    }

    const nextLevels = writeInventoryMutableLevel(mutableLevel, nextQuantities);
    commitInventoryMutableVariant(
      variantsByProductId,
      mutableVariant.variants,
      mutableVariant.index,
      mutableVariant.variant,
      mutableLevel.inventoryItem,
      nextLevels,
    );
    changes.push({
      inventoryItemId: quantityInput.inventoryItemId,
      locationId: quantityInput.locationId,
      ledgerDocumentUri: null,
      delta,
      name: quantityName,
      quantityAfterChange: null,
    });
  }

  for (const [productId, nextVariants] of variantsByProductId.entries()) {
    runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);
  }

  return {
    group: {
      id: runtime.syntheticIdentity.makeSyntheticGid('InventoryAdjustmentGroup'),
      createdAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      reason,
      referenceDocumentUri: typeof input['referenceDocumentUri'] === 'string' ? input['referenceDocumentUri'] : null,
      app: buildInventoryAdjustmentAppRecord(),
      changes: [...changes, ...mirroredOnHandChanges],
    },
    userErrors: [],
  };
}

function applyInventoryMoveQuantities(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
): {
  group: InventoryAdjustmentGroupRecord | null;
  userErrors: InventoryMutationUserError[];
} {
  const reason = typeof input['reason'] === 'string' && input['reason'].trim() ? input['reason'] : null;
  if (!reason) {
    return {
      group: null,
      userErrors: [{ field: ['input', 'reason'], message: 'Inventory adjustment reason is required' }],
    };
  }

  const changes = readInventoryMoveQuantityChangeInputs(input['changes']);
  if (changes.length === 0) {
    return {
      group: null,
      userErrors: [{ field: ['input', 'changes'], message: 'At least one inventory quantity move is required' }],
    };
  }

  const validationErrors: InventoryMutationUserError[] = [];
  for (const [changeIndex, change] of changes.entries()) {
    const path = ['input', 'changes', String(changeIndex)];
    const fromNameError = validateInventoryQuantityName(change.from.name, [...path, 'from', 'name']);
    const toNameError = validateInventoryQuantityName(change.to.name, [...path, 'to', 'name']);
    if (fromNameError) {
      validationErrors.push(fromNameError);
    }
    if (toNameError) {
      validationErrors.push(toNameError);
    }
    if (change.from.locationId && change.to.locationId && change.from.locationId !== change.to.locationId) {
      validationErrors.push({
        field: path,
        message: "The quantities can't be moved between different locations.",
      });
    }
    if (change.from.name && change.to.name && change.from.name === change.to.name) {
      validationErrors.push({
        field: path,
        message: "The quantity names for each change can't be the same.",
      });
    }
    if (change.from.name === 'available' && change.from.ledgerDocumentUri) {
      validationErrors.push({
        field: [...path, 'from', 'ledgerDocumentUri'],
        message: 'A ledger document URI is not allowed when adjusting available.',
      });
    }
    if (change.to.name === 'available' && change.to.ledgerDocumentUri) {
      validationErrors.push({
        field: [...path, 'to', 'ledgerDocumentUri'],
        message: 'A ledger document URI is not allowed when adjusting available.',
      });
    }
    if (change.from.name && change.from.name !== 'available' && !change.from.ledgerDocumentUri) {
      validationErrors.push({
        field: [...path, 'from', 'ledgerDocumentUri'],
        message: 'A ledger document URI is required except when adjusting available.',
      });
    }
    if (change.to.name && change.to.name !== 'available' && !change.to.ledgerDocumentUri) {
      validationErrors.push({
        field: [...path, 'to', 'ledgerDocumentUri'],
        message: 'A ledger document URI is required except when adjusting available.',
      });
    }
  }

  if (validationErrors.length > 0) {
    return { group: null, userErrors: validationErrors };
  }

  const variantsByProductId = new Map<string, ProductVariantRecord[]>();
  const adjustmentChanges: InventoryAdjustmentChangeRecord[] = [];

  for (const [changeIndex, change] of changes.entries()) {
    const path = ['input', 'changes', String(changeIndex)];
    if (!change.inventoryItemId) {
      return {
        group: null,
        userErrors: [{ field: [...path, 'inventoryItemId'], message: 'Inventory item id is required' }],
      };
    }
    if (!change.from.locationId || !change.to.locationId || !change.from.name || !change.to.name) {
      return {
        group: null,
        userErrors: [{ field: path, message: 'Inventory move terminals are required' }],
      };
    }
    if (typeof change.quantity !== 'number') {
      return {
        group: null,
        userErrors: [{ field: [...path, 'quantity'], message: 'Inventory move quantity is required' }],
      };
    }

    const mutableVariant = getInventoryMutableVariant(runtime, variantsByProductId, change.inventoryItemId);
    if (!mutableVariant) {
      return {
        group: null,
        userErrors: [
          {
            field: [...path, 'inventoryItemId'],
            message: 'The specified inventory item could not be found.',
          },
        ],
      };
    }

    const mutableLevel = getInventoryMutableLevel(runtime, mutableVariant.variant, change.from.locationId);
    if (!mutableLevel) {
      return {
        group: null,
        userErrors: [
          {
            field: [...path, 'from', 'locationId'],
            message: 'The specified inventory item is not stocked at the location.',
          },
        ],
      };
    }

    let nextQuantities = addInventoryQuantityAmount(
      runtime,
      mutableLevel.level.quantities,
      change.from.name,
      -change.quantity,
    );
    nextQuantities = addInventoryQuantityAmount(runtime, nextQuantities, change.to.name, change.quantity);
    const onHandDelta =
      (isOnHandComponentQuantityName(change.from.name) ? -change.quantity : 0) +
      (isOnHandComponentQuantityName(change.to.name) ? change.quantity : 0);
    if (onHandDelta !== 0) {
      nextQuantities = addInventoryQuantityAmount(runtime, nextQuantities, 'on_hand', onHandDelta);
    }

    const nextLevels = writeInventoryMutableLevel(mutableLevel, nextQuantities);
    commitInventoryMutableVariant(
      variantsByProductId,
      mutableVariant.variants,
      mutableVariant.index,
      mutableVariant.variant,
      mutableLevel.inventoryItem,
      nextLevels,
    );
    adjustmentChanges.push(
      {
        inventoryItemId: change.inventoryItemId,
        locationId: change.from.locationId,
        ledgerDocumentUri: change.from.ledgerDocumentUri,
        delta: -change.quantity,
        name: change.from.name,
        quantityAfterChange: null,
      },
      {
        inventoryItemId: change.inventoryItemId,
        locationId: change.to.locationId,
        ledgerDocumentUri: change.to.ledgerDocumentUri,
        delta: change.quantity,
        name: change.to.name,
        quantityAfterChange: null,
      },
    );
  }

  for (const [productId, nextVariants] of variantsByProductId.entries()) {
    runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);
  }

  return {
    group: {
      id: runtime.syntheticIdentity.makeSyntheticGid('InventoryAdjustmentGroup'),
      createdAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      reason,
      referenceDocumentUri: typeof input['referenceDocumentUri'] === 'string' ? input['referenceDocumentUri'] : null,
      app: buildInventoryAdjustmentAppRecord(),
      changes: adjustmentChanges,
    },
    userErrors: [],
  };
}

function findInventoryLevelTarget(
  runtime: ProxyRuntimeContext,
  inventoryLevelId: string,
): InventoryLevelTargetRecord | null {
  for (const product of runtime.store.listEffectiveProducts()) {
    for (const variant of runtime.store.getEffectiveVariantsByProductId(product.id)) {
      const level =
        getEffectiveInventoryLevels(runtime, variant).find((candidate) => candidate.id === inventoryLevelId) ?? null;
      if (level) {
        return { variant, level };
      }
    }
  }

  return null;
}

function findKnownLocationById(runtime: ProxyRuntimeContext, locationId: string): LocationRecord | null {
  return listEffectiveLocations(runtime).find((location) => location.id === locationId) ?? null;
}

function stageVariantInventoryLevels(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  nextLevels: NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']>,
): ProductVariantRecord {
  const nextVariant: ProductVariantRecord = {
    ...structuredClone(variant),
    inventoryItem: variant.inventoryItem
      ? {
          ...structuredClone(variant.inventoryItem),
          inventoryLevels: nextLevels,
        }
      : null,
  };
  const nextVariants = runtime.store
    .getEffectiveVariantsByProductId(variant.productId)
    .map((candidate) => (candidate.id === variant.id ? nextVariant : candidate));
  runtime.store.replaceStagedVariantsForProduct(variant.productId, nextVariants);
  return runtime.store.getEffectiveVariantById(variant.id) ?? nextVariant;
}

function buildActivatedInventoryLevel(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  location: LocationRecord,
): NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']>[number] | null {
  const syntheticLevel = buildSyntheticInventoryLevel(runtime, variant, {
    locationId: location.id,
    availableQuantity: 0,
  });
  if (!syntheticLevel) {
    return null;
  }

  return {
    ...syntheticLevel,
    location: {
      id: location.id,
      name: location.name,
    },
  };
}

function findEffectiveVariantById(runtime: ProxyRuntimeContext, variantId: string): ProductVariantRecord | null {
  for (const product of runtime.store.listEffectiveProducts()) {
    const variant = runtime.store
      .getEffectiveVariantsByProductId(product.id)
      .find((candidate) => candidate.id === variantId);
    if (variant) {
      return variant;
    }
  }

  return null;
}

function serializeVariantPayload(
  runtime: ProxyRuntimeContext,
  variants: ProductVariantRecord[],
  field: FieldNode | null,
): Record<string, unknown>[] {
  if (!field) {
    return variants.map((variant) => ({ id: variant.id }));
  }

  return variants.map((variant) =>
    serializeVariantSelectionSet(runtime, variant, field.selectionSet?.selections ?? [], {}),
  );
}

function serializeMediaPayload(mediaRecords: ProductMediaRecord[], field: FieldNode | null): Record<string, unknown>[] {
  if (!field) {
    return mediaRecords.map((mediaRecord) => ({ id: mediaRecord.id ?? null }));
  }

  return mediaRecords.map((mediaRecord) =>
    serializeMediaSelectionSet(mediaRecord, field.selectionSet?.selections ?? []),
  );
}

function serializeMetafieldPayload(
  metafields: ProductMetafieldRecord[],
  field: FieldNode | null,
): Record<string, unknown>[] {
  if (!field) {
    return metafields.map((metafield) => ({ id: metafield.id }));
  }

  return metafields.map((metafield) => serializeMetafieldSelectionSet(metafield, field.selectionSet?.selections ?? []));
}

type MetafieldsSetUserError = {
  field: string[];
  message: string;
  code: string | null;
  elementIndex: number | null;
};

type ProductMetafieldOwnerType = 'PRODUCT' | 'PRODUCTVARIANT' | 'COLLECTION';

type ProductMetafieldOwner = {
  id: string;
  ownerType: ProductMetafieldOwnerType;
};

function serializeMetafieldsSetUserErrors(
  field: FieldNode | null,
  errors: MetafieldsSetUserError[],
): Array<Record<string, unknown>> {
  const selections = field?.selectionSet?.selections.filter(
    (selection): selection is FieldNode => selection.kind === Kind.FIELD,
  );
  if (!selections || selections.length === 0) {
    return errors.map((error) => ({
      field: error.field,
      message: error.message,
    }));
  }

  return errors.map((error) =>
    Object.fromEntries(
      selections.map((selection) => {
        const responseKey = selection.alias?.value ?? selection.name.value;
        switch (selection.name.value) {
          case 'field':
            return [responseKey, error.field];
          case 'message':
            return [responseKey, error.message];
          case 'code':
            return [responseKey, error.code];
          case 'elementIndex':
            return [responseKey, error.elementIndex];
          default:
            return [responseKey, null];
        }
      }),
    ),
  );
}

type DeletedMetafieldIdentifierRecord = {
  ownerId: string;
  namespace: string;
  key: string;
};

type DeletedMetafieldIdentifierPayload = DeletedMetafieldIdentifierRecord | null;

function serializeDeletedMetafieldIdentifiers(
  identifiers: DeletedMetafieldIdentifierPayload[],
  field: FieldNode | null,
): Array<Record<string, unknown> | null> {
  if (!field) {
    return identifiers.map((identifier) =>
      identifier === null
        ? null
        : {
            ownerId: identifier.ownerId,
            namespace: identifier.namespace,
            key: identifier.key,
          },
    );
  }

  return identifiers.map((identifier) => {
    if (identifier === null) {
      return null;
    }

    return Object.fromEntries(
      (field.selectionSet?.selections ?? [])
        .filter((selection): selection is FieldNode => selection.kind === Kind.FIELD)
        .map((selection) => {
          const responseKey = selection.alias?.value ?? selection.name.value;
          switch (selection.name.value) {
            case 'ownerId':
              return [responseKey, identifier.ownerId];
            case 'namespace':
              return [responseKey, identifier.namespace];
            case 'key':
              return [responseKey, identifier.key];
            default:
              return [responseKey, null];
          }
        }),
    );
  });
}

function resolveProductMetafieldOwner(runtime: ProxyRuntimeContext, ownerId: string): ProductMetafieldOwner | null {
  if (runtime.store.getEffectiveProductById(ownerId)) {
    return { id: ownerId, ownerType: 'PRODUCT' };
  }

  if (runtime.store.getEffectiveVariantById(ownerId)) {
    return { id: ownerId, ownerType: 'PRODUCTVARIANT' };
  }

  if (runtime.store.getEffectiveCollectionById(ownerId)) {
    return { id: ownerId, ownerType: 'COLLECTION' };
  }

  return null;
}

function getProductMetafieldOwnerId(metafield: ProductMetafieldRecord): string {
  return metafield.ownerId ?? metafield.productId ?? '';
}

function getEffectiveMetafieldsForOwner(
  runtime: ProxyRuntimeContext,
  ownerId: string,
): Array<ProductMetafieldRecord & { ownerId: string }> {
  return runtime.store.getEffectiveMetafieldsByOwnerId(ownerId).map((metafield) => ({
    ...metafield,
    ownerId,
  }));
}

function replaceStagedMetafieldsForOwner(
  runtime: ProxyRuntimeContext,
  ownerId: string,
  metafields: ProductMetafieldRecord[],
): void {
  runtime.store.replaceStagedMetafieldsForOwner(ownerId, metafields);
}

function replaceBaseMetafieldsForHydratedProduct(
  runtime: ProxyRuntimeContext,
  productId: string,
  metafields: ProductMetafieldRecord[],
): void {
  const groupedByOwnerId = new Map<string, ProductMetafieldRecord[]>();
  groupedByOwnerId.set(productId, []);

  for (const metafield of metafields) {
    const ownerId = getProductMetafieldOwnerId(metafield);
    if (!ownerId) {
      continue;
    }

    const ownerMetafields = groupedByOwnerId.get(ownerId) ?? [];
    ownerMetafields.push(metafield);
    groupedByOwnerId.set(ownerId, ownerMetafields);
  }

  for (const [ownerId, ownerMetafields] of groupedByOwnerId.entries()) {
    runtime.store.replaceBaseMetafieldsForOwner(ownerId, ownerMetafields);
  }
}

function upsertMetafieldsForOwner(
  runtime: ProxyRuntimeContext,
  owner: ProductMetafieldOwner,
  inputs: Record<string, unknown>[],
): { metafields: ProductMetafieldRecord[]; createdOrUpdated: ProductMetafieldRecord[] } {
  const result = upsertOwnerMetafields(
    runtime,
    'ownerId',
    owner.id,
    inputs,
    getEffectiveMetafieldsForOwner(runtime, owner.id),
    {
      ownerType: owner.ownerType,
    },
  );

  if (owner.ownerType !== 'PRODUCT') {
    return result;
  }

  return {
    metafields: result.metafields.map((metafield) => ({ ...metafield, productId: owner.id })),
    createdOrUpdated: result.createdOrUpdated.map((metafield) => ({ ...metafield, productId: owner.id })),
  };
}

function readMetafieldsSetIdentity(input: Record<string, unknown>): {
  ownerId: string | null;
  namespace: string | null;
  key: string | null;
} {
  const ownerId = typeof input['ownerId'] === 'string' ? input['ownerId'] : null;
  const namespace =
    typeof input['namespace'] === 'string' && input['namespace'].trim()
      ? input['namespace']
      : getDefaultAppMetafieldNamespace();
  const key = typeof input['key'] === 'string' && input['key'].trim() ? input['key'] : null;
  return { ownerId, namespace, key };
}

function findMetafieldsSetDefinition(
  runtime: ProxyRuntimeContext,
  owner: ProductMetafieldOwner,
  namespace: string | null,
  key: string | null,
): MetafieldDefinitionRecord | null {
  if (!namespace || !key) {
    return null;
  }

  return runtime.store.findEffectiveMetafieldDefinition({
    ownerType: owner.ownerType,
    namespace,
    key,
  });
}

function makeMetafieldsSetUserError(
  index: number | null,
  fieldName: string | null,
  message: string,
  code: string,
): MetafieldsSetUserError {
  return {
    field: index === null ? ['metafields'] : ['metafields', String(index), ...(fieldName ? [fieldName] : [])],
    message,
    code,
    elementIndex: index,
  };
}

function validateMetafieldsSetInputs(
  runtime: ProxyRuntimeContext,
  inputs: Record<string, unknown>[],
): MetafieldsSetUserError[] {
  const errors: MetafieldsSetUserError[] = [];

  if (inputs.length === 0) {
    return [makeMetafieldsSetUserError(null, null, 'At least one metafield input is required.', 'BLANK')];
  }

  if (inputs.length > 25) {
    errors.push(
      makeMetafieldsSetUserError(
        null,
        null,
        'Exceeded the maximum metafields input limit of 25.',
        'LESS_THAN_OR_EQUAL_TO',
      ),
    );
  }

  for (const [index, input] of inputs.entries()) {
    const { ownerId, namespace, key } = readMetafieldsSetIdentity(input);

    if (!ownerId) {
      errors.push(makeMetafieldsSetUserError(index, 'ownerId', 'Owner id is required.', 'BLANK'));
      continue;
    }

    const owner = resolveProductMetafieldOwner(runtime, ownerId);
    if (!owner) {
      errors.push(makeMetafieldsSetUserError(index, 'ownerId', 'Owner does not exist.', 'INVALID'));
      continue;
    }

    if (!key) {
      errors.push(makeMetafieldsSetUserError(index, 'key', 'Key is required.', 'BLANK'));
      continue;
    }

    const effectiveMetafields = getEffectiveMetafieldsForOwner(runtime, owner.id);
    const existing = effectiveMetafields.find(
      (metafield) => metafield.namespace === namespace && metafield.key === key,
    );
    const definition = findMetafieldsSetDefinition(runtime, owner, namespace, key);
    const inputType = typeof input['type'] === 'string' && input['type'].trim() ? input['type'] : null;
    const type = inputType ?? definition?.type.name ?? existing?.type;
    const value = typeof input['value'] === 'string' ? input['value'] : null;

    if (!type) {
      errors.push({
        field: ['metafields', String(index), 'type'],
        message: "Type can't be blank",
        code: 'BLANK',
        elementIndex: null,
      });
    }

    if (value === null) {
      errors.push(makeMetafieldsSetUserError(index, 'value', 'Value is required.', 'BLANK'));
    }

    if (definition && inputType && inputType !== definition.type.name) {
      errors.push(
        makeMetafieldsSetUserError(
          index,
          'type',
          `Type must be ${definition.type.name} for this metafield definition.`,
          'INVALID_TYPE',
        ),
      );
    }

    if (definition && value !== null) {
      for (const validation of definition.validations) {
        if (validation.name === 'max') {
          const max = Number.parseInt(validation.value ?? '', 10);
          if (Number.isFinite(max) && value.length > max) {
            errors.push(
              makeMetafieldsSetUserError(
                index,
                'value',
                `Value must be ${max} characters or fewer for this metafield definition.`,
                'LESS_THAN_OR_EQUAL_TO',
              ),
            );
          }
        }

        if (validation.name === 'regex' && validation.value) {
          try {
            const pattern = new RegExp(validation.value, 'u');
            if (!pattern.test(value)) {
              errors.push(
                makeMetafieldsSetUserError(
                  index,
                  'value',
                  'Value does not match the validation pattern for this metafield definition.',
                  'INVALID',
                ),
              );
            }
          } catch {
            continue;
          }
        }
      }
    }

    if (!hasOwnField(input, 'compareDigest')) {
      continue;
    }

    const compareDigest = input['compareDigest'];
    if (compareDigest !== null && typeof compareDigest !== 'string') {
      errors.push(
        makeMetafieldsSetUserError(index, 'compareDigest', 'Compare digest is invalid.', 'INVALID_COMPARE_DIGEST'),
      );
      continue;
    }

    const currentCompareDigest = existing ? (existing.compareDigest ?? makeMetafieldCompareDigest(existing)) : null;
    if (compareDigest !== currentCompareDigest) {
      errors.push({
        field: ['metafields', String(index)],
        message: 'The resource has been updated since it was loaded. Try again with an updated `compareDigest` value.',
        code: 'STALE_OBJECT',
        elementIndex: null,
      });
    }
  }

  return errors;
}

function getDefaultAppMetafieldNamespace(): string {
  const appId =
    typeof process.env['SHOPIFY_CONFORMANCE_APP_ID'] === 'string' ? process.env['SHOPIFY_CONFORMANCE_APP_ID'] : null;
  const appIdTail = appId?.split('/').at(-1);
  return `app--${appIdTail && /^\d+$/u.test(appIdTail) ? appIdTail : '347082227713'}`;
}

function normalizeMetafieldsSetInput(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  owner: ProductMetafieldOwner,
): Record<string, unknown> {
  const normalizedInput =
    typeof input['namespace'] === 'string' && input['namespace'].trim()
      ? input
      : {
          ...input,
          namespace: getDefaultAppMetafieldNamespace(),
        };

  if (typeof normalizedInput['type'] === 'string' && normalizedInput['type'].trim()) {
    return normalizedInput;
  }

  const { namespace, key } = readMetafieldsSetIdentity(normalizedInput);
  const definition = findMetafieldsSetDefinition(runtime, owner, namespace, key);
  return definition ? { ...normalizedInput, type: definition.type.name } : normalizedInput;
}

function getRootFieldArgumentVariableName(field: FieldNode, argumentName: string): string | null {
  const argument = field.arguments?.find((candidate) => candidate.name.value === argumentName);
  return argument?.value.kind === Kind.VARIABLE ? argument.value.name.value : null;
}

function buildMetafieldsSetInvalidVariableError(
  document: string,
  variableName: string,
  inputs: Record<string, unknown>[],
  index: number,
  fieldName: string,
): Record<string, unknown> {
  return {
    errors: [
      {
        message: `Variable $${variableName} of type [MetafieldsSetInput!]! was provided invalid value for ${index}.${fieldName} (Expected value to not be null)`,
        locations: getVariableDefinitionLocation(document, variableName),
        extensions: {
          code: 'INVALID_VARIABLE',
          value: structuredClone(inputs),
          problems: [{ path: [index, fieldName], explanation: 'Expected value to not be null' }],
        },
      },
    ],
  };
}

function validateMetafieldsSetRequiredVariables(
  document: string,
  field: FieldNode,
  inputs: Record<string, unknown>[],
): Record<string, unknown> | null {
  const variableName = getRootFieldArgumentVariableName(field, 'metafields');
  if (!variableName) {
    return null;
  }

  for (const [index, input] of inputs.entries()) {
    for (const fieldName of ['ownerId', 'key', 'value']) {
      if (!hasOwnField(input, fieldName) || input[fieldName] === null) {
        return buildMetafieldsSetInvalidVariableError(document, variableName, inputs, index, fieldName);
      }
    }
  }

  return null;
}

function findMetafieldById(runtime: ProxyRuntimeContext, metafieldId: string): ProductMetafieldRecord | null {
  for (const product of runtime.store.listEffectiveProducts()) {
    const metafield = getEffectiveMetafieldsForOwner(runtime, product.id).find(
      (candidate) => candidate.id === metafieldId,
    );
    if (metafield) {
      return metafield;
    }

    for (const variant of runtime.store.getEffectiveVariantsByProductId(product.id)) {
      const variantMetafield = getEffectiveMetafieldsForOwner(runtime, variant.id).find(
        (candidate) => candidate.id === metafieldId,
      );
      if (variantMetafield) {
        return variantMetafield;
      }
    }
  }

  for (const collection of runtime.store.listEffectiveCollections()) {
    const metafield = getEffectiveMetafieldsForOwner(runtime, collection.id).find(
      (candidate) => candidate.id === metafieldId,
    );
    if (metafield) {
      return metafield;
    }
  }

  return null;
}

function validateMetafieldsDeleteRequiredFields(rawMetafields: unknown): {
  fieldPath: string;
  problemPath: Array<string | number>;
} | null {
  if (!Array.isArray(rawMetafields)) {
    return null;
  }

  for (const [index, rawMetafield] of rawMetafields.entries()) {
    if (!isObject(rawMetafield)) {
      continue;
    }

    for (const fieldName of ['ownerId', 'namespace', 'key']) {
      if (!hasOwnField(rawMetafield, fieldName) || rawMetafield[fieldName] === null) {
        return {
          fieldPath: `${index}.${fieldName}`,
          problemPath: [index, fieldName],
        };
      }
    }
  }

  return null;
}

function deleteMetafieldsByIdentifiers(
  runtime: ProxyRuntimeContext,
  inputs: Record<string, unknown>[],
): {
  deletedMetafields: DeletedMetafieldIdentifierPayload[];
  userErrors: Array<{ field: string[]; message: string }>;
} {
  if (inputs.length === 0) {
    return {
      deletedMetafields: [],
      userErrors: [],
    };
  }

  const effectiveMetafieldsByOwnerId = new Map<string, ProductMetafieldRecord[]>();
  const deletedMetafields: DeletedMetafieldIdentifierPayload[] = [];
  const userErrors: Array<{ field: string[]; message: string }> = [];

  for (const [index, input] of inputs.entries()) {
    const ownerId = typeof input['ownerId'] === 'string' ? input['ownerId'] : null;
    const namespace = typeof input['namespace'] === 'string' ? input['namespace'] : null;
    const key = typeof input['key'] === 'string' ? input['key'] : null;

    if (!ownerId) {
      userErrors.push({ field: ['metafields', String(index), 'ownerId'], message: 'Owner id is required' });
      continue;
    }

    if (!namespace) {
      userErrors.push({ field: ['metafields', String(index), 'namespace'], message: 'Namespace is required' });
      continue;
    }

    if (!key) {
      userErrors.push({ field: ['metafields', String(index), 'key'], message: 'Key is required' });
      continue;
    }

    const effectiveMetafields =
      effectiveMetafieldsByOwnerId.get(ownerId) ?? getEffectiveMetafieldsForOwner(runtime, ownerId);
    const metafieldExists = effectiveMetafields.some(
      (metafield) => metafield.namespace === namespace && metafield.key === key,
    );
    if (!metafieldExists) {
      deletedMetafields.push(null);
      continue;
    }

    const remainingMetafields = effectiveMetafields.filter(
      (metafield) => metafield.namespace !== namespace || metafield.key !== key,
    );
    effectiveMetafieldsByOwnerId.set(ownerId, remainingMetafields);
    deletedMetafields.push({ ownerId, namespace, key });
  }

  if (userErrors.length > 0) {
    return {
      deletedMetafields: [],
      userErrors,
    };
  }

  for (const [ownerId, metafields] of effectiveMetafieldsByOwnerId.entries()) {
    replaceStagedMetafieldsForOwner(runtime, ownerId, metafields);
  }

  return {
    deletedMetafields,
    userErrors: [],
  };
}

function buildProductSetVariantRecords(
  runtime: ProxyRuntimeContext,
  productId: string,
  rawVariants: unknown,
): ProductVariantRecord[] {
  const existingVariants = runtime.store.getEffectiveVariantsByProductId(productId);
  const existingVariantsById = new Map(existingVariants.map((variant) => [variant.id, variant]));
  if (!Array.isArray(rawVariants)) {
    return [];
  }

  return rawVariants
    .filter((value): value is Record<string, unknown> => isObject(value))
    .map((value) => {
      const normalized = normalizeProductSetVariantInput(value);
      const rawId = normalized['id'];
      const existing = typeof rawId === 'string' ? (existingVariantsById.get(rawId) ?? null) : null;
      return existing
        ? updateProductSetVariantRecord(runtime, existing, normalized)
        : makeCreatedProductSetVariantRecord(runtime, productId, normalized);
    });
}

function buildProductSetMetafieldRecords(
  runtime: ProxyRuntimeContext,
  productId: string,
  rawMetafields: unknown,
): ProductMetafieldRecord[] {
  const inputs = Array.isArray(rawMetafields)
    ? rawMetafields.filter((value): value is Record<string, unknown> => isObject(value))
    : [];

  return inputs.map((input) => {
    const existing = findMetafieldById(runtime, typeof input['id'] === 'string' ? input['id'] : '');
    const type = typeof input['type'] === 'string' ? input['type'] : (existing?.type ?? null);
    const value = typeof input['value'] === 'string' ? input['value'] : (existing?.value ?? null);
    const createdAt = existing?.createdAt ?? runtime.syntheticIdentity.makeSyntheticTimestamp();
    const updatedAt = existing
      ? value === existing.value && type === existing.type
        ? (existing.updatedAt ?? createdAt)
        : runtime.syntheticIdentity.makeSyntheticTimestamp()
      : createdAt;
    const metafield: ProductMetafieldRecord = {
      id:
        existing && getProductMetafieldOwnerId(existing) === productId
          ? existing.id
          : runtime.syntheticIdentity.makeSyntheticGid('Metafield'),
      productId,
      ownerId: productId,
      namespace: typeof input['namespace'] === 'string' ? input['namespace'] : (existing?.namespace ?? ''),
      key: typeof input['key'] === 'string' ? input['key'] : (existing?.key ?? ''),
      type,
      value,
      jsonValue: parseMetafieldJsonValue(type, value),
      createdAt,
      updatedAt,
      ownerType: 'PRODUCT',
    };
    return {
      ...metafield,
      compareDigest: makeMetafieldCompareDigest(metafield),
    };
  });
}

function buildProductSetCollectionRecords(
  runtime: ProxyRuntimeContext,
  productId: string,
  rawCollections: unknown,
): ProductCollectionRecord[] {
  const collectionIds = Array.isArray(rawCollections)
    ? rawCollections.filter((value): value is string => typeof value === 'string')
    : [];

  return collectionIds.map((collectionId) => {
    const existing = findEffectiveCollectionById(runtime, collectionId);
    return {
      id: collectionId,
      productId,
      title: existing?.title ?? collectionId.split('/').at(-1) ?? collectionId,
      handle: existing?.handle ?? slugifyHandle(existing?.title ?? collectionId.split('/').at(-1) ?? collectionId),
    };
  });
}

function serializeProductSetOperation(
  field: FieldNode | null,
  operation: ProductOperationRecord | null,
): Record<string, unknown> | null {
  if (!field || !operation) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = operation.typeName;
        break;
      case 'id':
        result[key] = operation.id;
        break;
      case 'status':
        result[key] = operation.status;
        break;
      case 'userErrors':
        result[key] = serializeProductOperationUserErrors(operation.userErrors, selection);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function normalizeUpstreamVariant(productId: string, value: unknown): ProductVariantRecord | null {
  if (!isObject(value)) {
    return null;
  }

  const rawId = value['id'];
  if (typeof rawId !== 'string') {
    return null;
  }

  const rawInventoryItem = value['inventoryItem'];
  const rawInventoryMeasurement = isObject(rawInventoryItem) ? rawInventoryItem['measurement'] : null;
  const rawInventoryWeight = isObject(rawInventoryMeasurement) ? rawInventoryMeasurement['weight'] : null;
  const inventoryItem =
    isObject(rawInventoryItem) && typeof rawInventoryItem['id'] === 'string'
      ? {
          id: rawInventoryItem['id'],
          tracked: typeof rawInventoryItem['tracked'] === 'boolean' ? rawInventoryItem['tracked'] : null,
          requiresShipping:
            typeof rawInventoryItem['requiresShipping'] === 'boolean' ? rawInventoryItem['requiresShipping'] : null,
          measurement: isObject(rawInventoryWeight)
            ? {
                weight: {
                  unit: typeof rawInventoryWeight['unit'] === 'string' ? rawInventoryWeight['unit'] : null,
                  value: typeof rawInventoryWeight['value'] === 'number' ? rawInventoryWeight['value'] : null,
                },
              }
            : null,
          countryCodeOfOrigin:
            typeof rawInventoryItem['countryCodeOfOrigin'] === 'string'
              ? rawInventoryItem['countryCodeOfOrigin']
              : null,
          provinceCodeOfOrigin:
            typeof rawInventoryItem['provinceCodeOfOrigin'] === 'string'
              ? rawInventoryItem['provinceCodeOfOrigin']
              : null,
          harmonizedSystemCode:
            typeof rawInventoryItem['harmonizedSystemCode'] === 'string'
              ? rawInventoryItem['harmonizedSystemCode']
              : null,
          inventoryLevels: normalizeInventoryLevelRecords(rawInventoryItem['inventoryLevels']),
        }
      : null;

  const selectedOptions = Array.isArray(value['selectedOptions'])
    ? value['selectedOptions']
        .map((selectedOption) => {
          if (!isObject(selectedOption)) {
            return null;
          }

          const rawName = selectedOption['name'];
          const rawValue = selectedOption['value'];
          if (typeof rawName !== 'string' || typeof rawValue !== 'string') {
            return null;
          }

          return { name: rawName, value: rawValue };
        })
        .filter(
          (selectedOption): selectedOption is ProductVariantRecord['selectedOptions'][number] =>
            selectedOption !== null,
        )
    : [];

  const rawTitle = value['title'];
  const rawSku = value['sku'];
  const rawBarcode = value['barcode'];
  const rawPrice = value['price'];
  const rawCompareAtPrice = value['compareAtPrice'];
  const rawTaxable = value['taxable'];
  const rawInventoryPolicy = value['inventoryPolicy'];
  const rawInventoryQuantity = value['inventoryQuantity'];
  const contextualPricing = readCapturedJsonValue(value['contextualPricing']);

  return {
    id: rawId,
    productId,
    title: typeof rawTitle === 'string' && rawTitle.trim() ? rawTitle : 'Default Title',
    sku: typeof rawSku === 'string' ? rawSku : null,
    barcode: typeof rawBarcode === 'string' ? rawBarcode : null,
    price: typeof rawPrice === 'string' ? rawPrice : null,
    compareAtPrice: typeof rawCompareAtPrice === 'string' ? rawCompareAtPrice : null,
    taxable: typeof rawTaxable === 'boolean' ? rawTaxable : null,
    inventoryPolicy: typeof rawInventoryPolicy === 'string' ? rawInventoryPolicy : null,
    inventoryQuantity: typeof rawInventoryQuantity === 'number' ? rawInventoryQuantity : null,
    selectedOptions,
    inventoryItem,
    ...(contextualPricing === undefined ? {} : { contextualPricing }),
  };
}

function normalizeUpstreamOption(productId: string, value: unknown): ProductOptionRecord | null {
  if (!isObject(value)) {
    return null;
  }

  const rawId = value['id'];
  const rawName = value['name'];
  const rawPosition = value['position'];
  if (typeof rawId !== 'string' || typeof rawName !== 'string') {
    return null;
  }

  const optionValues = Array.isArray(value['optionValues'])
    ? value['optionValues']
        .map((optionValue) => {
          if (!isObject(optionValue)) {
            return null;
          }

          const optionValueId = optionValue['id'];
          const optionValueName = optionValue['name'];
          const hasVariants = optionValue['hasVariants'];
          if (typeof optionValueId !== 'string' || typeof optionValueName !== 'string') {
            return null;
          }

          return {
            id: optionValueId,
            name: optionValueName,
            hasVariants: typeof hasVariants === 'boolean' ? hasVariants : false,
          };
        })
        .filter((optionValue): optionValue is ProductOptionRecord['optionValues'][number] => optionValue !== null)
    : [];

  return {
    id: rawId,
    productId,
    name: rawName,
    position: typeof rawPosition === 'number' ? rawPosition : 0,
    optionValues,
  };
}

function normalizeUpstreamCollection(productId: string, value: unknown): ProductCollectionRecord | null {
  if (!isObject(value)) {
    return null;
  }

  const rawId = value['id'];
  const rawTitle = value['title'];
  const rawHandle = value['handle'];
  if (typeof rawId !== 'string' || typeof rawTitle !== 'string' || typeof rawHandle !== 'string') {
    return null;
  }

  return {
    ...normalizeCollectionFields(value, rawId, rawTitle, rawHandle),
    productId,
  };
}

function normalizeUpstreamCollectionRecord(value: unknown): CollectionRecord | null {
  if (!isObject(value)) {
    return null;
  }

  const rawId = value['id'];
  const rawTitle = value['title'];
  const rawHandle = value['handle'];
  if (typeof rawId !== 'string' || typeof rawTitle !== 'string' || typeof rawHandle !== 'string') {
    return null;
  }

  return normalizeCollectionFields(value, rawId, rawTitle, rawHandle);
}

function normalizeCollectionImage(value: unknown): CollectionImageRecord | null {
  if (!isObject(value)) {
    return null;
  }

  const rawId = value['id'];
  const rawAltText = value['altText'];
  const rawUrl = value['url'] ?? value['src'] ?? value['originalSrc'] ?? value['transformedSrc'];
  const rawWidth = value['width'];
  const rawHeight = value['height'];

  return {
    id: typeof rawId === 'string' ? rawId : null,
    altText: typeof rawAltText === 'string' ? rawAltText : null,
    url: typeof rawUrl === 'string' ? rawUrl : null,
    width: typeof rawWidth === 'number' ? rawWidth : null,
    height: typeof rawHeight === 'number' ? rawHeight : null,
  };
}

function normalizeCollectionRuleSet(value: unknown): CollectionRuleSetRecord | null {
  if (!isObject(value)) {
    return null;
  }

  const rawAppliedDisjunctively = value['appliedDisjunctively'];
  const rawRules = Array.isArray(value['rules']) ? value['rules'] : [];

  return {
    appliedDisjunctively: typeof rawAppliedDisjunctively === 'boolean' ? rawAppliedDisjunctively : false,
    rules: rawRules
      .filter(isObject)
      .map((rule) => {
        const column = rule['column'];
        const relation = rule['relation'];
        const condition = rule['condition'];
        const conditionObjectId = rule['conditionObjectId'];
        return {
          column: typeof column === 'string' ? column : '',
          relation: typeof relation === 'string' ? relation : '',
          condition: typeof condition === 'string' ? condition : '',
          conditionObjectId: typeof conditionObjectId === 'string' ? conditionObjectId : null,
        };
      })
      .filter((rule) => rule.column && rule.relation),
  };
}

function normalizeCollectionFields(
  value: Record<string, unknown>,
  id: string,
  title: string,
  handle: string,
): CollectionRecord {
  const rawSeo = value['seo'];
  const rawDescription = value['description'];
  const rawDescriptionHtml = value['descriptionHtml'];
  const descriptionHtml = typeof rawDescriptionHtml === 'string' ? rawDescriptionHtml : null;
  const rawLegacyResourceId = value['legacyResourceId'];
  const rawUpdatedAt = value['updatedAt'];
  const rawSortOrder = value['sortOrder'];
  const rawTemplateSuffix = value['templateSuffix'];

  return {
    id,
    legacyResourceId: typeof rawLegacyResourceId === 'string' ? rawLegacyResourceId : readLegacyResourceIdFromGid(id),
    title,
    handle,
    publicationIds: makeUnknownPublicationIds(
      Math.max(
        readPublicationCount(value['availablePublicationsCount']),
        readPublicationCount(value['resourcePublicationsCount']),
      ),
    ),
    updatedAt: typeof rawUpdatedAt === 'string' ? rawUpdatedAt : null,
    description:
      typeof rawDescription === 'string'
        ? rawDescription
        : descriptionHtml !== null
          ? stripHtmlToDescription(descriptionHtml)
          : null,
    descriptionHtml,
    image: normalizeCollectionImage(value['image']),
    sortOrder: typeof rawSortOrder === 'string' ? rawSortOrder : null,
    templateSuffix:
      rawTemplateSuffix === null ? null : typeof rawTemplateSuffix === 'string' ? rawTemplateSuffix : null,
    seo: isObject(rawSeo)
      ? {
          title: typeof rawSeo['title'] === 'string' ? rawSeo['title'] : null,
          description: typeof rawSeo['description'] === 'string' ? rawSeo['description'] : null,
        }
      : { title: null, description: null },
    ruleSet: normalizeCollectionRuleSet(value['ruleSet']),
    ...(value['ruleSet'] && isObject(value['ruleSet']) ? { isSmart: true } : {}),
  };
}

function normalizeUpstreamMedia(productId: string, value: unknown, position: number): ProductMediaRecord | null {
  if (!isObject(value)) {
    return null;
  }

  const rawId = value['id'];
  const rawMediaContentType = value['mediaContentType'];
  const rawAlt = value['alt'];
  const rawStatus = value['status'];
  const rawPreview = value['preview'];
  const rawPreviewImage = isObject(rawPreview) ? rawPreview['image'] : null;
  const rawPreviewImageUrl = isObject(rawPreviewImage) ? rawPreviewImage['url'] : null;
  const rawImage = value['image'];
  const rawImageUrl = isObject(rawImage) ? rawImage['url'] : null;
  const rawImageWidth = isObject(rawImage) ? rawImage['width'] : null;
  const rawImageHeight = isObject(rawImage) ? rawImage['height'] : null;

  const normalizedImageUrl =
    typeof rawImageUrl === 'string' ? rawImageUrl : typeof rawPreviewImageUrl === 'string' ? rawPreviewImageUrl : null;

  return {
    key: `${productId}:media:${position}`,
    productId,
    position,
    id: typeof rawId === 'string' ? rawId : null,
    mediaContentType: typeof rawMediaContentType === 'string' ? rawMediaContentType : null,
    alt: typeof rawAlt === 'string' ? rawAlt : null,
    status: typeof rawStatus === 'string' ? rawStatus : null,
    productImageId: null,
    imageUrl: normalizedImageUrl,
    imageWidth: typeof rawImageWidth === 'number' ? rawImageWidth : null,
    imageHeight: typeof rawImageHeight === 'number' ? rawImageHeight : null,
    previewImageUrl: typeof rawPreviewImageUrl === 'string' ? rawPreviewImageUrl : null,
    sourceUrl: normalizedImageUrl,
  };
}

function normalizeUpstreamProductImage(productId: string, value: unknown, position: number): ProductMediaRecord | null {
  if (!isObject(value)) {
    return null;
  }

  const rawId = value['id'];
  const rawAltText = value['altText'];
  const rawUrl = value['url'] ?? value['src'] ?? value['originalSrc'] ?? value['transformedSrc'];
  const rawWidth = value['width'];
  const rawHeight = value['height'];
  const imageUrl = typeof rawUrl === 'string' ? rawUrl : null;

  return {
    key: `${productId}:media:${position}`,
    productId,
    position,
    id: null,
    mediaContentType: 'IMAGE',
    alt: typeof rawAltText === 'string' ? rawAltText : null,
    status: imageUrl ? 'READY' : null,
    productImageId: typeof rawId === 'string' ? rawId : null,
    imageUrl,
    imageWidth: typeof rawWidth === 'number' ? rawWidth : null,
    imageHeight: typeof rawHeight === 'number' ? rawHeight : null,
    previewImageUrl: imageUrl,
    sourceUrl: imageUrl,
  };
}

function normalizeUpstreamMetafield(
  ownerId: string,
  value: unknown,
  ownerType: ProductMetafieldOwnerType = 'PRODUCT',
): ProductMetafieldRecord | null {
  const metafield = normalizeOwnerMetafield('ownerId', ownerId, value, { ownerType });
  if (!metafield) {
    return null;
  }

  return ownerType === 'PRODUCT'
    ? {
        ...metafield,
        productId: ownerId,
      }
    : metafield;
}

function normalizeUpstreamMetafieldsForOwner(
  ownerId: string,
  value: Record<string, unknown>,
  ownerType: ProductMetafieldOwnerType,
): ProductMetafieldRecord[] {
  const metafieldsById = new Map<string, ProductMetafieldRecord>();
  const singularMetafield = normalizeUpstreamMetafield(ownerId, value['metafield'], ownerType);
  if (singularMetafield) {
    metafieldsById.set(singularMetafield.id, singularMetafield);
  }

  for (const metafieldNode of readMetafieldNodes(value['metafields'])) {
    const metafield = normalizeUpstreamMetafield(ownerId, metafieldNode, ownerType);
    if (metafield) {
      metafieldsById.set(metafield.id, metafield);
    }
  }

  return Array.from(metafieldsById.values()).sort(
    (left, right) =>
      left.namespace.localeCompare(right.namespace) ||
      left.key.localeCompare(right.key) ||
      left.id.localeCompare(right.id),
  );
}

function readVariantNodes(value: unknown): unknown[] {
  if (!isObject(value)) {
    return [];
  }

  if (Array.isArray(value['nodes'])) {
    return value['nodes'];
  }

  if (Array.isArray(value['edges'])) {
    return value['edges'].map((edge) => (isObject(edge) ? edge['node'] : null)).filter((node) => node !== null);
  }

  return [];
}

function readCollectionNodes(value: unknown): unknown[] {
  if (!isObject(value)) {
    return [];
  }

  if (Array.isArray(value['nodes'])) {
    return value['nodes'];
  }

  if (Array.isArray(value['edges'])) {
    return value['edges'].map((edge) => (isObject(edge) ? edge['node'] : null)).filter((node) => node !== null);
  }

  return [];
}

function readConnectionNodeEntries(value: unknown): Array<{ node: unknown; position: number }> {
  if (!isObject(value)) {
    return [];
  }

  if (Array.isArray(value['nodes'])) {
    return value['nodes'].map((node, position) => ({ node, position }));
  }

  if (Array.isArray(value['edges'])) {
    return value['edges']
      .map((edge, position) => (isObject(edge) ? { node: edge['node'], position } : null))
      .filter((entry): entry is { node: unknown; position: number } => entry !== null && entry.node !== null);
  }

  return [];
}

function readMediaNodes(value: unknown): unknown[] {
  if (!isObject(value)) {
    return [];
  }

  if (Array.isArray(value['nodes'])) {
    return value['nodes'];
  }

  if (Array.isArray(value['edges'])) {
    return value['edges'].map((edge) => (isObject(edge) ? edge['node'] : null)).filter((node) => node !== null);
  }

  return [];
}

function readMetafieldNodes(value: unknown): unknown[] {
  if (!isObject(value)) {
    return [];
  }

  if (Array.isArray(value['nodes'])) {
    return value['nodes'];
  }

  if (Array.isArray(value['edges'])) {
    return value['edges'].map((edge) => (isObject(edge) ? edge['node'] : null)).filter((node) => node !== null);
  }

  return [];
}

function readProductNodes(value: unknown): unknown[] {
  if (!isObject(value)) {
    return [];
  }

  if (Array.isArray(value['nodes'])) {
    return value['nodes'];
  }

  if (Array.isArray(value['edges'])) {
    return value['edges'].map((edge) => (isObject(edge) ? edge['node'] : null)).filter((node) => node !== null);
  }

  return [];
}

function normalizeProductCatalogPageInfo(raw: unknown): ProductCatalogConnectionRecord['pageInfo'] {
  if (!isObject(raw)) {
    return {
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: null,
      endCursor: null,
    };
  }

  return {
    hasNextPage: raw['hasNextPage'] === true,
    hasPreviousPage: raw['hasPreviousPage'] === true,
    startCursor: typeof raw['startCursor'] === 'string' ? raw['startCursor'] : null,
    endCursor: typeof raw['endCursor'] === 'string' ? raw['endCursor'] : null,
  };
}

function collectProductCatalogConnection(
  raw: unknown,
  responseKey = 'products',
): ProductCatalogConnectionRecord | null {
  if (!isObject(raw) || !isObject(raw[responseKey])) {
    return null;
  }

  const productsConnection = raw[responseKey];
  const connectionEntries = readConnectionNodeEntries(productsConnection);

  const orderedProductIds: string[] = [];
  const cursorByProductId: Record<string, string> = {};
  for (const entry of connectionEntries) {
    const normalized = normalizeUpstreamProduct(entry.node);
    const edge = Array.isArray(productsConnection['edges']) ? productsConnection['edges'][entry.position] : null;
    const cursor = isObject(edge) && typeof edge['cursor'] === 'string' ? edge['cursor'] : null;
    if (!normalized) {
      continue;
    }

    orderedProductIds.push(normalized.product.id);
    if (cursor) {
      cursorByProductId[normalized.product.id] = cursor;
    }
  }

  if (orderedProductIds.length === 0) {
    return null;
  }

  return {
    orderedProductIds,
    cursorByProductId,
    pageInfo: normalizeProductCatalogPageInfo(productsConnection['pageInfo']),
  };
}

function collectProductSearchConnections(
  document: string,
  variables: Record<string, unknown>,
  rawData: Record<string, unknown>,
): Record<string, ProductCatalogConnectionRecord> {
  const connections: Record<string, ProductCatalogConnectionRecord> = {};
  for (const field of getRootFields(document)) {
    if (field.name.value !== 'products') {
      continue;
    }

    const args = getFieldArguments(field, variables);
    const key = buildProductSearchConnectionKey(args['query'], args['sortKey'], args['reverse']);
    if (!key) {
      continue;
    }

    const connection = collectProductCatalogConnection(rawData, field.alias?.value ?? field.name.value);
    if (!connection) {
      continue;
    }

    connections[key] = connection;
  }

  return connections;
}

function getChildField(parent: FieldNode, fieldName: string): FieldNode | null {
  const child = parent.selectionSet?.selections.find(
    (selection): selection is FieldNode => selection.kind === Kind.FIELD && selection.name.value === fieldName,
  );

  return child ?? null;
}

function getResponseKey(field: FieldNode): string {
  return field.alias?.value ?? field.name.value;
}

function serializeProductMutationPayload(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  payload: {
    product: ProductRecord | null;
    userErrors: Array<{ field: string[]; message: string }>;
  },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const productField = getChildField(field, 'product');
  if (productField) {
    result[getResponseKey(productField)] = serializeProduct(runtime, payload.product, productField, variables);
  }

  const userErrorsField = getChildField(field, 'userErrors');
  if (userErrorsField) {
    result[getResponseKey(userErrorsField)] = payload.userErrors;
  }

  return result;
}

function serializePublicationMutationPayload(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  payload: {
    publication: PublicationRecord | null;
    deletedId?: string | null;
    userErrors: Array<{ field: string[]; message: string }>;
  },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  const publicationField = getChildField(field, 'publication');
  if (publicationField) {
    result[getResponseKey(publicationField)] = payload.publication
      ? serializePublicationSelectionSet(runtime, payload.publication, publicationField.selectionSet?.selections ?? [])
      : null;
  }

  const deletedIdField = getChildField(field, 'deletedId');
  if (deletedIdField) {
    result[getResponseKey(deletedIdField)] = payload.deletedId ?? null;
  }

  const userErrorsField = getChildField(field, 'userErrors');
  if (userErrorsField) {
    result[getResponseKey(userErrorsField)] = payload.userErrors;
  }

  return result;
}

function serializePublishableSelectionSet(
  runtime: ProxyRuntimeContext,
  publishable: ProductRecord | CollectionRecord | null,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  if (!publishable) {
    return null;
  }

  if (publishable.id.startsWith('gid://shopify/Product/')) {
    return serializeSelectionSet(runtime, publishable as ProductRecord, selections, variables);
  }

  return serializeCollectionObject(runtime, publishable as CollectionRecord, selections, variables);
}

function serializeShopSelectionSet(
  runtime: ProxyRuntimeContext,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'publicationCount':
        result[key] = listEffectivePublications(runtime).length;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializePublishableMutationPayload(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  payload: {
    publishable: ProductRecord | CollectionRecord | null;
    userErrors: Array<{ field: string[]; message: string }>;
  },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  const publishableField = getChildField(field, 'publishable');
  if (publishableField) {
    result[getResponseKey(publishableField)] = serializePublishableSelectionSet(
      runtime,
      payload.publishable,
      publishableField.selectionSet?.selections ?? [],
      variables,
    );
  }

  const shopField = getChildField(field, 'shop');
  if (shopField) {
    result[getResponseKey(shopField)] = serializeShopSelectionSet(runtime, shopField.selectionSet?.selections ?? []);
  }

  const userErrorsField = getChildField(field, 'userErrors');
  if (userErrorsField) {
    result[getResponseKey(userErrorsField)] = payload.userErrors;
  }

  return result;
}

function buildSyntheticInventoryLevel(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  options?: {
    existingLevel?: NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']>[number] | null;
    locationId?: string | null;
    availableQuantity?: number;
  },
): NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']>[number] | null {
  if (!variant.inventoryItem) {
    return null;
  }

  const existingLevel = options?.existingLevel ?? null;
  const locationId = options?.locationId ?? existingLevel?.location?.id ?? DEFAULT_INVENTORY_LEVEL_LOCATION_ID;
  const availableQuantity =
    options?.availableQuantity ??
    (locationId === DEFAULT_INVENTORY_LEVEL_LOCATION_ID ? (variant.inventoryQuantity ?? 0) : 0);
  const availableUpdatedAt = runtime.syntheticIdentity.makeSyntheticTimestamp();

  return {
    id: existingLevel?.id ?? buildStableSyntheticInventoryLevelId(variant.inventoryItem.id, locationId),
    cursor: existingLevel?.cursor ?? null,
    location: existingLevel?.location ?? { id: locationId, name: null },
    quantities: [
      {
        name: 'available',
        quantity: availableQuantity,
        updatedAt: availableUpdatedAt,
      },
      {
        name: 'on_hand',
        quantity: availableQuantity,
        updatedAt: null,
      },
      {
        name: 'incoming',
        quantity: 0,
        updatedAt: null,
      },
      {
        name: 'committed',
        quantity: 0,
        updatedAt: null,
      },
      {
        name: 'reserved',
        quantity: 0,
        updatedAt: null,
      },
    ],
  };
}

function buildSyntheticInventoryLevels(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
): NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']> {
  const level = buildSyntheticInventoryLevel(runtime, variant);
  return level && !runtime.store.isLocationDeleted(level.location?.id ?? '') ? [level] : [];
}

function getEffectiveInventoryLevels(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
): NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']> {
  const hydratedLevels = variant.inventoryItem?.inventoryLevels;
  if (!hydratedLevels || hydratedLevels.length === 0) {
    return buildSyntheticInventoryLevels(runtime, variant);
  }

  return structuredClone(hydratedLevels).filter((level) => !runtime.store.isLocationDeleted(level.location?.id ?? ''));
}

function serializeInventoryLevelQuantities(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  level: NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']>[number],
  field: FieldNode,
  variables: Record<string, unknown>,
): Array<Record<string, unknown>> {
  const args = getFieldArguments(field, variables);
  const requestedNames = Array.isArray(args['names'])
    ? args['names'].filter((value): value is string => typeof value === 'string')
    : [];
  const allQuantities =
    level.quantities.length > 0
      ? level.quantities
      : (buildSyntheticInventoryLevels(runtime, variant)[0]?.quantities ?? []);
  const visibleQuantities =
    requestedNames.length > 0
      ? requestedNames.map(
          (name) => allQuantities.find((quantity) => quantity.name === name) ?? { name, quantity: 0, updatedAt: null },
        )
      : allQuantities;

  return visibleQuantities.map((quantity) => {
    const quantityResult: Record<string, unknown> = {};
    for (const quantitySelection of field.selectionSet?.selections ?? []) {
      if (quantitySelection.kind !== Kind.FIELD) {
        continue;
      }

      const quantityKey = quantitySelection.alias?.value ?? quantitySelection.name.value;
      switch (quantitySelection.name.value) {
        case 'name':
          quantityResult[quantityKey] = quantity.name;
          break;
        case 'quantity':
          quantityResult[quantityKey] = quantity.quantity;
          break;
        case 'updatedAt':
          quantityResult[quantityKey] = quantity.updatedAt;
          break;
        default:
          quantityResult[quantityKey] = null;
      }
    }
    return quantityResult;
  });
}

function serializeInventoryLevelNode(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  level: NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']>[number],
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const nodeResult: Record<string, unknown> = {};
  for (const levelSelection of field.selectionSet?.selections ?? []) {
    if (levelSelection.kind !== Kind.FIELD) {
      continue;
    }

    const levelKey = levelSelection.alias?.value ?? levelSelection.name.value;
    switch (levelSelection.name.value) {
      case 'id':
        nodeResult[levelKey] = level.id;
        break;
      case 'location': {
        if (!level.location) {
          nodeResult[levelKey] = null;
          break;
        }
        const effectiveLocation = readEffectiveInventoryLevelLocation(runtime, level.location);
        const locationResult: Record<string, unknown> = {};
        for (const locationSelection of levelSelection.selectionSet?.selections ?? []) {
          if (locationSelection.kind !== Kind.FIELD) {
            continue;
          }
          const locationKey = locationSelection.alias?.value ?? locationSelection.name.value;
          switch (locationSelection.name.value) {
            case 'id':
              locationResult[locationKey] = effectiveLocation.id;
              break;
            case 'name':
              locationResult[locationKey] = effectiveLocation.name;
              break;
            default:
              locationResult[locationKey] = null;
          }
        }
        nodeResult[levelKey] = locationResult;
        break;
      }
      case 'quantities':
        nodeResult[levelKey] = serializeInventoryLevelQuantities(runtime, variant, level, levelSelection, variables);
        break;
      default:
        nodeResult[levelKey] = null;
    }
  }
  return nodeResult;
}

function serializeInventoryLevelsConnection(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const getLevelCursor = (
    level: NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']>[number],
  ): string => level.cursor ?? `cursor:${level.id}`;
  const allLevels = getEffectiveInventoryLevels(runtime, variant);
  const {
    items: levels,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allLevels, field, variables, getLevelCursor);
  return serializeConnection(field, {
    items: levels,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: getLevelCursor,
    serializeNode: (level, selection) => serializeInventoryLevelNode(runtime, variant, level, selection, variables),
    pageInfoOptions: {
      prefixCursors: false,
    },
  });
}

type InventoryTransferUserError = {
  field: string[] | null;
  message: string;
};

type InventoryTransferLineItemInput = {
  inventoryItemId: string | null;
  quantity: number | null;
};

type InventoryTransferLineItemUpdate = {
  inventoryItemId: string;
  newQuantity: number;
  deltaQuantity: number;
};

function readInventoryTransferLineItemInputs(raw: unknown): InventoryTransferLineItemInput[] {
  return readPlainObjectArray(raw).map((entry) => ({
    inventoryItemId: typeof entry['inventoryItemId'] === 'string' ? entry['inventoryItemId'] : null,
    quantity: typeof entry['quantity'] === 'number' && Number.isInteger(entry['quantity']) ? entry['quantity'] : null,
  }));
}

function makeInventoryTransferLocationSnapshot(
  runtime: ProxyRuntimeContext,
  locationId: string | null,
): InventoryTransferLocationSnapshotRecord | null {
  if (!locationId) {
    return null;
  }

  const location = findKnownLocationById(runtime, locationId);
  return {
    id: locationId,
    name: location?.name ?? locationId,
    snapshottedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
  };
}

function makeInventoryTransferLineItem(
  runtime: ProxyRuntimeContext,
  input: InventoryTransferLineItemInput,
): InventoryTransferLineItemRecord | null {
  if (!input.inventoryItemId || input.quantity === null) {
    return null;
  }

  const variant = runtime.store.findEffectiveVariantByInventoryItemId(input.inventoryItemId);
  const product = variant ? runtime.store.getEffectiveProductById(variant.productId) : null;
  return {
    id: runtime.syntheticIdentity.makeProxySyntheticGid('InventoryTransferLineItem'),
    inventoryItemId: input.inventoryItemId,
    title: product?.title ?? variant?.title ?? null,
    totalQuantity: input.quantity,
    shippedQuantity: 0,
    pickedForShipmentQuantity: 0,
  };
}

function validateInventoryTransferLineItems(
  runtime: ProxyRuntimeContext,
  inputs: InventoryTransferLineItemInput[],
): InventoryTransferUserError[] {
  const errors: InventoryTransferUserError[] = [];
  inputs.forEach((input, index) => {
    if (!input.inventoryItemId || !runtime.store.findEffectiveVariantByInventoryItemId(input.inventoryItemId)) {
      errors.push({
        field: ['input', 'lineItems', `${index}`, 'inventoryItemId'],
        message: "The inventory item can't be found.",
      });
      return;
    }

    const variant = runtime.store.findEffectiveVariantByInventoryItemId(input.inventoryItemId);
    if (variant?.inventoryItem?.tracked !== true) {
      errors.push({
        field: ['input', 'lineItems', `${index}`, 'inventoryItemId'],
        message: 'The inventory item does not track inventory.',
      });
    }

    if (input.quantity === null || input.quantity <= 0) {
      errors.push({
        field: ['input', 'lineItems', `${index}`, 'quantity'],
        message: 'Quantity must be greater than 0.',
      });
    }
  });
  return errors;
}

function makeInventoryTransferRecord(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  status: InventoryTransferRecord['status'],
): { transfer: InventoryTransferRecord | null; userErrors: InventoryTransferUserError[] } {
  const lineItemInputs = readInventoryTransferLineItemInputs(input['lineItems']);
  const userErrors = validateInventoryTransferLineItems(runtime, lineItemInputs);
  if (userErrors.length > 0) {
    return { transfer: null, userErrors };
  }

  const transferIndex = runtime.store.listEffectiveInventoryTransfers().length + 1;
  const id = runtime.syntheticIdentity.makeSyntheticGid('InventoryTransfer');
  const lineItems = lineItemInputs
    .map((lineItemInput) => makeInventoryTransferLineItem(runtime, lineItemInput))
    .filter((lineItem): lineItem is InventoryTransferLineItemRecord => lineItem !== null);
  const dateCreated =
    typeof input['dateCreated'] === 'string'
      ? input['dateCreated']
      : runtime.syntheticIdentity.makeSyntheticTimestamp();

  return {
    transfer: {
      id,
      name: `#T${String(transferIndex).padStart(4, '0')}`,
      referenceName: typeof input['referenceName'] === 'string' ? input['referenceName'] : null,
      status,
      note: typeof input['note'] === 'string' ? input['note'] : null,
      tags: Array.isArray(input['tags']) ? input['tags'].filter((tag): tag is string => typeof tag === 'string') : [],
      dateCreated,
      origin: makeInventoryTransferLocationSnapshot(
        runtime,
        typeof input['originLocationId'] === 'string' ? input['originLocationId'] : null,
      ),
      destination: makeInventoryTransferLocationSnapshot(
        runtime,
        typeof input['destinationLocationId'] === 'string' ? input['destinationLocationId'] : null,
      ),
      lineItems,
    },
    userErrors,
  };
}

function findInventoryTransferOriginLevel(
  runtime: ProxyRuntimeContext,
  transfer: InventoryTransferRecord,
  lineItem: InventoryTransferLineItemRecord,
): { variant: ProductVariantRecord; level: InventoryLevelRecord } | null {
  const variant = runtime.store.findEffectiveVariantByInventoryItemId(lineItem.inventoryItemId);
  if (!variant || !transfer.origin?.id) {
    return null;
  }

  const level = getEffectiveInventoryLevels(runtime, variant).find(
    (candidate) => candidate.location?.id === transfer.origin?.id,
  );
  return level ? { variant, level } : null;
}

function applyInventoryTransferReservation(
  runtime: ProxyRuntimeContext,
  transfer: InventoryTransferRecord,
  direction: 'reserve' | 'release',
): InventoryTransferUserError[] {
  const errors: InventoryTransferUserError[] = [];
  const nextLevelsByVariantId = new Map<string, InventoryLevelRecord[]>();

  for (const lineItem of transfer.lineItems) {
    const target = findInventoryTransferOriginLevel(runtime, transfer, lineItem);
    if (!target) {
      errors.push({
        field: ['id'],
        message:
          'Cannot mark the transfer as ready to ship as the line items contain following errors: The item is not stocked at the origin location.',
      });
      continue;
    }

    const levels = nextLevelsByVariantId.get(target.variant.id) ?? getEffectiveInventoryLevels(runtime, target.variant);
    const levelIndex = levels.findIndex((level) => level.id === target.level.id);
    const level = levels[levelIndex] ?? target.level;
    const available = readInventoryQuantityAmount(level.quantities, 'available', 0);
    const reserved = readInventoryQuantityAmount(level.quantities, 'reserved', 0);
    if (direction === 'reserve' && available < lineItem.totalQuantity) {
      errors.push({
        field: ['id'],
        message:
          'Cannot mark the transfer as ready to ship as the line items contain following errors: The item is not stocked at the origin location.',
      });
      continue;
    }

    const quantity = direction === 'reserve' ? lineItem.totalQuantity : -lineItem.totalQuantity;
    const quantitiesWithAvailable = writeInventoryQuantityAmount(
      runtime,
      level.quantities,
      'available',
      available - quantity,
    );
    const nextQuantities = writeInventoryQuantityAmount(
      runtime,
      quantitiesWithAvailable,
      'reserved',
      reserved + quantity,
    );
    const nextLevel = { ...level, quantities: nextQuantities };
    const nextLevels = levels.map((candidate, index) => (index === levelIndex ? nextLevel : candidate));
    nextLevelsByVariantId.set(target.variant.id, nextLevels);
  }

  if (errors.length > 0) {
    return errors;
  }

  for (const [variantId, nextLevels] of nextLevelsByVariantId.entries()) {
    const variant = runtime.store.getEffectiveVariantById(variantId);
    if (!variant) {
      continue;
    }

    const nextVariant = stageVariantInventoryLevels(runtime, variant, nextLevels);
    const inventoryQuantity = sumAvailableInventoryLevels(getEffectiveInventoryLevels(runtime, nextVariant));
    runtime.store.replaceStagedVariantsForProduct(
      nextVariant.productId,
      runtime.store
        .getEffectiveVariantsByProductId(nextVariant.productId)
        .map((candidate) => (candidate.id === nextVariant.id ? { ...nextVariant, inventoryQuantity } : candidate)),
    );
  }

  return [];
}

function serializeInventoryTransferUserErrors(
  field: FieldNode | null,
  errors: InventoryTransferUserError[],
): Array<Record<string, unknown>> {
  return errors.map((error) => {
    const result: Record<string, unknown> = {};
    for (const selection of field?.selectionSet?.selections ?? []) {
      if (selection.kind !== Kind.FIELD) {
        continue;
      }

      const key = getResponseKey(selection);
      switch (selection.name.value) {
        case 'field':
          result[key] = error.field;
          break;
        case 'message':
          result[key] = error.message;
          break;
        default:
          result[key] = null;
      }
    }
    return result;
  });
}

function serializeInventoryTransferLocationSnapshot(
  runtime: ProxyRuntimeContext,
  snapshot: InventoryTransferLocationSnapshotRecord | null,
  field: FieldNode,
): Record<string, unknown> | null {
  if (!snapshot) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = getResponseKey(selection);
    switch (selection.name.value) {
      case 'name':
        result[key] = snapshot.name;
        break;
      case 'snapshottedAt':
        result[key] = snapshot.snapshottedAt;
        break;
      case 'location': {
        const location = snapshot.id ? findKnownLocationById(runtime, snapshot.id) : null;
        result[key] = location
          ? serializeLocationSelectionSet(runtime, location, selection.selectionSet?.selections ?? [], {})
          : null;
        break;
      }
      case 'address':
        result[key] = {};
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeInventoryTransferLineItem(
  runtime: ProxyRuntimeContext,
  transfer: InventoryTransferRecord,
  lineItem: InventoryTransferLineItemRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const isReady = transfer.status === 'READY_TO_SHIP' || transfer.status === 'IN_PROGRESS';
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = getResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = lineItem.id;
        break;
      case 'title':
        result[key] = lineItem.title;
        break;
      case 'totalQuantity':
        result[key] = lineItem.totalQuantity;
        break;
      case 'shippedQuantity':
        result[key] = lineItem.shippedQuantity;
        break;
      case 'pickedForShipmentQuantity':
        result[key] = lineItem.pickedForShipmentQuantity;
        break;
      case 'processableQuantity':
        result[key] = lineItem.totalQuantity - lineItem.shippedQuantity;
        break;
      case 'shippableQuantity':
        result[key] = isReady ? lineItem.totalQuantity - lineItem.shippedQuantity : 0;
        break;
      case 'inventoryItem': {
        const variant = runtime.store.findEffectiveVariantByInventoryItemId(lineItem.inventoryItemId);
        result[key] = variant
          ? serializeInventoryItemSelectionSet(runtime, variant, selection.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeInventoryTransferLineItemsConnection(
  runtime: ProxyRuntimeContext,
  transfer: InventoryTransferRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const getCursor = (lineItem: InventoryTransferLineItemRecord): string => `cursor:${lineItem.id}`;
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    transfer.lineItems,
    field,
    variables,
    getCursor,
  );
  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: getCursor,
    serializeNode: (lineItem, selection) =>
      serializeInventoryTransferLineItem(runtime, transfer, lineItem, selection, variables),
    pageInfoOptions: { prefixCursors: false },
  });
}

function serializeEmptyInventoryTransferConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems([], field, variables, () => '');
  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: () => '',
    serializeNode: () => null,
    pageInfoOptions: { prefixCursors: false },
  });
}

function serializeInventoryTransfer(
  runtime: ProxyRuntimeContext,
  transfer: InventoryTransferRecord | null,
  field: FieldNode | null,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  if (!transfer || !field) {
    return null;
  }

  const result: Record<string, unknown> = {};
  const totalQuantity = transfer.lineItems.reduce((total, lineItem) => total + lineItem.totalQuantity, 0);
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = getResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = transfer.id;
        break;
      case 'name':
        result[key] = transfer.name;
        break;
      case 'referenceName':
        result[key] = transfer.referenceName;
        break;
      case 'status':
        result[key] = transfer.status;
        break;
      case 'note':
        result[key] = transfer.note;
        break;
      case 'tags':
        result[key] = transfer.tags;
        break;
      case 'dateCreated':
        result[key] = transfer.dateCreated;
        break;
      case 'totalQuantity':
        result[key] = totalQuantity;
        break;
      case 'receivedQuantity':
        result[key] = 0;
        break;
      case 'origin':
        result[key] = serializeInventoryTransferLocationSnapshot(runtime, transfer.origin, selection);
        break;
      case 'destination':
        result[key] = serializeInventoryTransferLocationSnapshot(runtime, transfer.destination, selection);
        break;
      case 'lineItems':
        result[key] = serializeInventoryTransferLineItemsConnection(runtime, transfer, selection, variables);
        break;
      case 'lineItemsCount':
        result[key] = serializeCountObject(totalQuantity, selection.selectionSet?.selections ?? []);
        break;
      case 'events':
      case 'shipments':
      case 'metafields':
        result[key] = serializeEmptyInventoryTransferConnection(selection, variables);
        break;
      case 'metafield':
        result[key] = null;
        break;
      case 'hasTimelineComment':
        result[key] = false;
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeInventoryTransfersConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const transfers = runtime.store.listEffectiveInventoryTransfers();
  const getCursor = (transfer: InventoryTransferRecord): string => `cursor:${transfer.id}`;
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(transfers, field, variables, getCursor);
  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: getCursor,
    serializeNode: (transfer, selection) => serializeInventoryTransfer(runtime, transfer, selection, variables),
    pageInfoOptions: { prefixCursors: false },
  });
}

function serializeInventoryTransferLineItemUpdates(
  field: FieldNode | null,
  updates: InventoryTransferLineItemUpdate[] | null,
): Array<Record<string, unknown>> | null {
  if (updates === null) {
    return null;
  }

  return updates.map((update) => {
    const result: Record<string, unknown> = {};
    for (const selection of field?.selectionSet?.selections ?? []) {
      if (selection.kind !== Kind.FIELD) {
        continue;
      }

      const key = getResponseKey(selection);
      switch (selection.name.value) {
        case 'inventoryItemId':
          result[key] = update.inventoryItemId;
          break;
        case 'newQuantity':
          result[key] = update.newQuantity;
          break;
        case 'deltaQuantity':
          result[key] = update.deltaQuantity;
          break;
        default:
          result[key] = null;
      }
    }
    return result;
  });
}

function inventoryTransferNotFoundPayload(rootName: string, field: FieldNode): Record<string, unknown> {
  return {
    [rootName]: {
      inventoryTransfer: rootName === 'inventoryTransferDelete' ? undefined : null,
      deletedId: rootName === 'inventoryTransferDelete' ? null : undefined,
      userErrors: serializeInventoryTransferUserErrors(getChildField(field, 'userErrors'), [
        { field: ['id'], message: "The inventory transfer can't be found." },
      ]),
    },
  };
}

function serializeCountObject(count: number, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'count':
        result[key] = count;
        break;
      case 'precision':
        result[key] = 'EXACT';
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeEditablePropertyObject(selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'locked':
        result[key] = false;
        break;
      case 'reason':
        result[key] = null;
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeInventoryQuantityName(
  quantityName: (typeof INVENTORY_QUANTITY_NAME_DEFINITIONS)[number],
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'name':
        result[key] = quantityName.name;
        break;
      case 'displayName':
        result[key] = quantityName.displayName;
        break;
      case 'isInUse':
        result[key] = quantityName.isInUse;
        break;
      case 'belongsTo':
        result[key] = [...quantityName.belongsTo];
        break;
      case 'comprises':
        result[key] = [...quantityName.comprises];
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeInventoryPropertiesSelectionSet(selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'quantityNames':
        result[key] = INVENTORY_QUANTITY_NAME_DEFINITIONS.map((quantityName) =>
          serializeInventoryQuantityName(quantityName, selection.selectionSet?.selections ?? []),
        );
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function listEffectiveInventoryItemVariants(runtime: ProxyRuntimeContext): ProductVariantRecord[] {
  return runtime.store
    .listEffectiveProducts()
    .flatMap((product) => runtime.store.getEffectiveVariantsByProductId(product.id))
    .filter((variant) => variant.inventoryItem !== null)
    .sort((left, right) => (left.inventoryItem?.id ?? left.id).localeCompare(right.inventoryItem?.id ?? right.id));
}

function matchesPositiveInventoryItemQueryTerm(variant: ProductVariantRecord, term: SearchQueryTerm): boolean {
  const inventoryItem = variant.inventoryItem;
  if (!inventoryItem) {
    return false;
  }

  const value = searchQueryTermValue(term);
  if (term.field === null) {
    return [inventoryItem.id, variant.sku ?? '', variant.id].some((candidate) =>
      matchesStringValue(candidate, value, 'includes'),
    );
  }

  switch (term.field.toLowerCase()) {
    case 'id':
      return matchesResourceIdValue(inventoryItem.id, value);
    case 'sku':
      return typeof variant.sku === 'string' && matchesStringValue(variant.sku, value, 'exact');
    case 'tracked':
      return String(inventoryItem.tracked === true) === value.toLowerCase();
    default:
      return true;
  }
}

function applyInventoryItemsQuery(variants: ProductVariantRecord[], rawQuery: unknown): ProductVariantRecord[] {
  return applySearchQuery(variants, rawQuery, { recognizeNotKeyword: true }, matchesPositiveInventoryItemQueryTerm);
}

function serializeInventoryItemsConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const variants = applyInventoryItemsQuery(listEffectiveInventoryItemVariants(runtime), args['query']);
  const orderedVariants = args['reverse'] === true ? [...variants].reverse() : variants;
  const getCursorValue = (variant: ProductVariantRecord): string => variant.inventoryItem?.id ?? variant.id;
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    orderedVariants,
    field,
    variables,
    getCursorValue,
  );
  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue,
    serializeNode: (variant, selection) =>
      serializeInventoryItemSelectionSet(runtime, variant, selection.selectionSet?.selections ?? [], variables),
  });
}

function serializeInventoryMutationUserErrors(
  field: FieldNode | null,
  userErrors: InventoryMutationUserError[],
): Array<Record<string, unknown>> {
  if (!field) {
    return userErrors.map((userError) => ({
      field: userError.field,
      message: userError.message,
      code: userError.code ?? null,
    }));
  }

  return userErrors.map((userError) => {
    const result: Record<string, unknown> = {};
    for (const selection of field.selectionSet?.selections ?? []) {
      if (selection.kind !== Kind.FIELD) {
        continue;
      }

      const key = selection.alias?.value ?? selection.name.value;
      switch (selection.name.value) {
        case 'field':
          result[key] = userError.field;
          break;
        case 'message':
          result[key] = userError.message;
          break;
        case 'code':
          result[key] = userError.code ?? null;
          break;
        default:
          result[key] = null;
      }
    }
    return result;
  });
}

function serializeInventoryLevelObject(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  level: NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']>[number],
  selections: readonly SelectionNode[],
  variables: Record<string, unknown> = {},
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'id':
        result[key] = level.id;
        break;
      case 'location': {
        if (!level.location) {
          result[key] = null;
          break;
        }
        const effectiveLocation = readEffectiveInventoryLevelLocation(runtime, level.location);
        const locationResult: Record<string, unknown> = {};
        for (const locationSelection of selection.selectionSet?.selections ?? []) {
          if (locationSelection.kind !== Kind.FIELD) {
            continue;
          }
          const locationKey = locationSelection.alias?.value ?? locationSelection.name.value;
          switch (locationSelection.name.value) {
            case 'id':
              locationResult[locationKey] = effectiveLocation.id;
              break;
            case 'name':
              locationResult[locationKey] = effectiveLocation.name;
              break;
            default:
              locationResult[locationKey] = null;
          }
        }
        result[key] = locationResult;
        break;
      }
      case 'quantities':
        result[key] = serializeInventoryLevelQuantities(runtime, variant, level, selection, variables);
        break;
      case 'item':
        result[key] = serializeInventoryItemSelectionSet(
          runtime,
          variant,
          selection.selectionSet?.selections ?? [],
          variables,
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeInventoryItemSelectionSet(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown> = {},
): Record<string, unknown> | null {
  if (!variant.inventoryItem) {
    return null;
  }

  return Object.fromEntries(
    selections
      .filter((inventorySelection): inventorySelection is FieldNode => inventorySelection.kind === Kind.FIELD)
      .map((inventorySelection) => {
        const inventoryKey = inventorySelection.alias?.value ?? inventorySelection.name.value;
        switch (inventorySelection.name.value) {
          case 'id':
            return [inventoryKey, variant.inventoryItem?.id ?? null];
          case 'createdAt': {
            const product = runtime.store.getEffectiveProductById(variant.productId);
            return [inventoryKey, product?.createdAt ?? runtime.syntheticIdentity.makeSyntheticTimestamp()];
          }
          case 'updatedAt': {
            const product = runtime.store.getEffectiveProductById(variant.productId);
            return [inventoryKey, product?.updatedAt ?? runtime.syntheticIdentity.makeSyntheticTimestamp()];
          }
          case 'legacyResourceId':
            return [inventoryKey, readLegacyResourceIdFromGid(variant.inventoryItem?.id ?? '') ?? '0'];
          case 'duplicateSkuCount':
            return [inventoryKey, 0];
          case 'inventoryHistoryUrl':
            return [inventoryKey, null];
          case 'sku':
            return [inventoryKey, variant.sku ?? null];
          case 'tracked':
            return [inventoryKey, variant.inventoryItem?.tracked ?? null];
          case 'trackedEditable':
            return [inventoryKey, serializeEditablePropertyObject(inventorySelection.selectionSet?.selections ?? [])];
          case 'requiresShipping':
            return [inventoryKey, variant.inventoryItem?.requiresShipping ?? null];
          case 'unitCost':
            return [inventoryKey, null];
          case 'measurement': {
            const measurementSelections = inventorySelection.selectionSet?.selections ?? [];
            if (!variant.inventoryItem?.measurement) {
              return [inventoryKey, null];
            }

            return [
              inventoryKey,
              Object.fromEntries(
                measurementSelections
                  .filter(
                    (measurementSelection): measurementSelection is FieldNode =>
                      measurementSelection.kind === Kind.FIELD,
                  )
                  .map((measurementSelection) => {
                    const measurementKey = measurementSelection.alias?.value ?? measurementSelection.name.value;
                    switch (measurementSelection.name.value) {
                      case 'weight': {
                        const weightSelections = measurementSelection.selectionSet?.selections ?? [];
                        const weight = variant.inventoryItem?.measurement?.weight;
                        if (!weight) {
                          return [measurementKey, null];
                        }

                        return [
                          measurementKey,
                          Object.fromEntries(
                            weightSelections
                              .filter(
                                (weightSelection): weightSelection is FieldNode => weightSelection.kind === Kind.FIELD,
                              )
                              .map((weightSelection) => {
                                const weightKey = weightSelection.alias?.value ?? weightSelection.name.value;
                                switch (weightSelection.name.value) {
                                  case 'unit':
                                    return [weightKey, weight.unit];
                                  case 'value':
                                    return [weightKey, weight.value];
                                  default:
                                    return [weightKey, null];
                                }
                              }),
                          ),
                        ];
                      }
                      default:
                        return [measurementKey, null];
                    }
                  }),
              ),
            ];
          }
          case 'countryCodeOfOrigin':
            return [inventoryKey, variant.inventoryItem?.countryCodeOfOrigin ?? null];
          case 'provinceCodeOfOrigin':
            return [inventoryKey, variant.inventoryItem?.provinceCodeOfOrigin ?? null];
          case 'harmonizedSystemCode':
            return [inventoryKey, variant.inventoryItem?.harmonizedSystemCode ?? null];
          case 'locationsCount':
            return [
              inventoryKey,
              serializeCountObject(
                getEffectiveInventoryLevels(runtime, variant).length,
                inventorySelection.selectionSet?.selections ?? [],
              ),
            ];
          case 'inventoryLevel': {
            const inventoryArgs = getFieldArguments(inventorySelection, variables);
            const locationId = typeof inventoryArgs['locationId'] === 'string' ? inventoryArgs['locationId'] : null;
            const level = locationId
              ? (getEffectiveInventoryLevels(runtime, variant).find(
                  (candidate) => candidate.location?.id === locationId,
                ) ?? null)
              : null;
            return [
              inventoryKey,
              level
                ? serializeInventoryLevelObject(
                    runtime,
                    variant,
                    level,
                    inventorySelection.selectionSet?.selections ?? [],
                    variables,
                  )
                : null,
            ];
          }
          case 'inventoryLevels':
            return [inventoryKey, serializeInventoryLevelsConnection(runtime, variant, inventorySelection, variables)];
          case 'variant':
            return [
              inventoryKey,
              serializeVariantSelectionSet(
                runtime,
                variant,
                inventorySelection.selectionSet?.selections ?? [],
                variables,
              ),
            ];
          default:
            return [inventoryKey, null];
        }
      }),
  );
}

function readStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

function uniqueStrings(values: string[]): string[] {
  return [...new Set(values)];
}

function readVariantMediaInputs(value: unknown): Array<{ variantId: string; mediaIds: string[] }> {
  return readPlainObjectArray(value)
    .map((input) => {
      const variantId = input['variantId'];
      if (typeof variantId !== 'string') {
        return null;
      }

      return {
        variantId,
        mediaIds: readStringArray(input['mediaIds']),
      };
    })
    .filter((input): input is { variantId: string; mediaIds: string[] } => input !== null);
}

type SellingPlanGroupUserError = {
  field: string[];
  message: string;
  code?: string | null;
};

function sellingPlanGroupDoesNotExistError(): SellingPlanGroupUserError {
  return {
    field: ['id'],
    message: 'Selling plan group does not exist.',
    code: 'GROUP_DOES_NOT_EXIST',
  };
}

function serializeSellingPlanGroupUserErrors(
  errors: SellingPlanGroupUserError[],
  field: FieldNode | null,
): Array<Record<string, unknown>> {
  const selections = field?.selectionSet?.selections ?? [];
  return errors.map((error) => {
    const result: Record<string, unknown> = {};
    for (const selection of selections) {
      if (selection.kind !== Kind.FIELD) {
        continue;
      }

      const key = selection.alias?.value ?? selection.name.value;
      switch (selection.name.value) {
        case 'field':
          result[key] = error.field;
          break;
        case 'message':
          result[key] = error.message;
          break;
        case 'code':
          result[key] = error.code ?? null;
          break;
        default:
          result[key] = null;
      }
    }
    return result;
  });
}

function serializeSellingPlanRecord(plan: SellingPlanRecord, field: FieldNode | null): unknown {
  if (!field) {
    return null;
  }

  return projectGraphqlObject(plan.data, field.selectionSet?.selections ?? [], new Map());
}

function serializeSellingPlanConnection(
  plans: SellingPlanRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(plans, field, variables, (plan) => plan.id);

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (plan) => plan.id,
    serializeNode: (plan, selection) => serializeSellingPlanRecord(plan, selection),
  });
}

function serializeSellingPlanGroupProductsConnection(
  runtime: ProxyRuntimeContext,
  group: SellingPlanGroupRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const products = group.productIds
    .map((productId) => runtime.store.getEffectiveProductById(productId))
    .filter((product): product is ProductRecord => product !== null);
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    products,
    field,
    variables,
    (product) => product.id,
  );

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (product) => product.id,
    serializeNode: (product, selection) => serializeProduct(runtime, product, selection, variables),
  });
}

function serializeSellingPlanGroupProductVariantsConnection(
  runtime: ProxyRuntimeContext,
  group: SellingPlanGroupRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const productId = typeof args['productId'] === 'string' ? args['productId'] : null;
  const variants = group.productVariantIds
    .map((variantId) => runtime.store.getEffectiveVariantById(variantId))
    .filter((variant): variant is ProductVariantRecord => variant !== null)
    .filter((variant) => productId === null || variant.productId === productId);
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    variants,
    field,
    variables,
    (variant) => variant.id,
  );

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (variant) => variant.id,
    serializeNode: (variant, selection) =>
      serializeVariantSelectionSet(runtime, variant, selection.selectionSet?.selections ?? [], variables),
  });
}

function serializeSellingPlanGroup(
  runtime: ProxyRuntimeContext,
  group: SellingPlanGroupRecord | null,
  field: FieldNode | null,
  variables: Record<string, unknown>,
): unknown {
  if (!group || !field) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'SellingPlanGroup';
        break;
      case 'id':
        result[key] = group.id;
        break;
      case 'appId':
        result[key] = group.appId;
        break;
      case 'name':
        result[key] = group.name;
        break;
      case 'merchantCode':
        result[key] = group.merchantCode;
        break;
      case 'description':
        result[key] = group.description;
        break;
      case 'options':
        result[key] = [...group.options];
        break;
      case 'position':
        result[key] = group.position;
        break;
      case 'summary':
        result[key] = group.summary;
        break;
      case 'createdAt':
        result[key] = group.createdAt;
        break;
      case 'appliesToProduct': {
        const args = getFieldArguments(selection, variables);
        result[key] = typeof args['productId'] === 'string' && group.productIds.includes(args['productId']);
        break;
      }
      case 'appliesToProductVariant': {
        const args = getFieldArguments(selection, variables);
        result[key] =
          typeof args['productVariantId'] === 'string' && group.productVariantIds.includes(args['productVariantId']);
        break;
      }
      case 'appliesToProductVariants': {
        const args = getFieldArguments(selection, variables);
        const productId = typeof args['productId'] === 'string' ? args['productId'] : null;
        result[key] =
          productId !== null &&
          group.productVariantIds.some(
            (variantId) => runtime.store.getEffectiveVariantById(variantId)?.productId === productId,
          );
        break;
      }
      case 'products':
        result[key] = serializeSellingPlanGroupProductsConnection(runtime, group, selection, variables);
        break;
      case 'productsCount':
        result[key] = serializeCountValue(selection, group.productIds.length);
        break;
      case 'productVariants':
        result[key] = serializeSellingPlanGroupProductVariantsConnection(runtime, group, selection, variables);
        break;
      case 'productVariantsCount': {
        const args = getFieldArguments(selection, variables);
        const productId = typeof args['productId'] === 'string' ? args['productId'] : null;
        const count =
          productId === null
            ? group.productVariantIds.length
            : group.productVariantIds.filter(
                (variantId) => runtime.store.getEffectiveVariantById(variantId)?.productId === productId,
              ).length;
        result[key] = serializeCountValue(selection, count);
        break;
      }
      case 'sellingPlans':
        result[key] = serializeSellingPlanConnection(group.sellingPlans, selection, variables);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeSellingPlanGroupConnection(
  runtime: ProxyRuntimeContext,
  groups: SellingPlanGroupRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    groups,
    field,
    variables,
    (group) => group.id,
  );

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (group) => group.cursor ?? group.id,
    serializeNode: (group, selection) => serializeSellingPlanGroup(runtime, group, selection, variables),
  });
}

function serializeVariantSelectionSet(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown> = {},
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ProductVariant';
        break;
      case 'id':
        result[key] = variant.id;
        break;
      case 'title':
        result[key] = variant.title;
        break;
      case 'sku':
        result[key] = variant.sku;
        break;
      case 'barcode':
        result[key] = variant.barcode;
        break;
      case 'price':
        result[key] = variant.price;
        break;
      case 'compareAtPrice':
        result[key] = variant.compareAtPrice;
        break;
      case 'taxable':
        result[key] = variant.taxable;
        break;
      case 'inventoryPolicy':
        result[key] = variant.inventoryPolicy;
        break;
      case 'inventoryQuantity':
        result[key] = variant.inventoryQuantity;
        break;
      case 'selectedOptions':
        result[key] = variant.selectedOptions.map((selectedOption) => {
          const selectedOptionResult: Record<string, unknown> = {};
          for (const selectedOptionSelection of selection.selectionSet?.selections ?? []) {
            if (selectedOptionSelection.kind !== Kind.FIELD) {
              continue;
            }

            const selectedOptionKey = selectedOptionSelection.alias?.value ?? selectedOptionSelection.name.value;
            switch (selectedOptionSelection.name.value) {
              case 'name':
                selectedOptionResult[selectedOptionKey] = selectedOption.name;
                break;
              case 'value':
                selectedOptionResult[selectedOptionKey] = selectedOption.value;
                break;
              default:
                selectedOptionResult[selectedOptionKey] = null;
            }
          }

          return selectedOptionResult;
        });
        break;
      case 'inventoryItem':
        result[key] = serializeInventoryItemSelectionSet(
          runtime,
          variant,
          selection.selectionSet?.selections ?? [],
          variables,
        );
        break;
      case 'contextualPricing':
        result[key] = projectGraphqlValue(
          variant.contextualPricing,
          selection.selectionSet?.selections ?? [],
          new Map(),
        );
        break;
      case 'product': {
        const product = runtime.store.getEffectiveProductById(variant.productId);
        result[key] = serializeProduct(runtime, product, selection, {});
        break;
      }
      case 'metafield': {
        const args = getFieldArguments(selection, variables);
        const namespace = typeof args['namespace'] === 'string' ? args['namespace'] : null;
        const metafieldKey = typeof args['key'] === 'string' ? args['key'] : null;
        if (!namespace || !metafieldKey) {
          result[key] = null;
          break;
        }

        const metafield = getEffectiveMetafieldsForOwner(runtime, variant.id).find(
          (candidate) => candidate.namespace === namespace && candidate.key === metafieldKey,
        );
        result[key] = metafield
          ? serializeMetafieldSelectionSet(metafield, selection.selectionSet?.selections ?? [])
          : null;
        break;
      }
      case 'metafields':
        result[key] = serializeOwnerMetafieldsConnection(
          getEffectiveMetafieldsForOwner(runtime, variant.id),
          selection,
          variables,
        );
        break;
      case 'media':
        result[key] = serializeVariantMediaConnection(runtime, variant, selection, variables);
        break;
      case 'requiresComponents':
        result[key] = runtime.store.getEffectiveVariantComponentsByParentVariantId(variant.id).length > 0;
        break;
      case 'productVariantComponents':
        result[key] = serializeProductVariantComponentConnection(runtime, variant.id, selection, variables);
        break;
      case 'sellingPlanGroups':
        result[key] = serializeSellingPlanGroupConnection(
          runtime,
          runtime.store.listEffectiveSellingPlanGroupsVisibleForProductVariant(variant.id),
          selection,
          variables,
        );
        break;
      case 'sellingPlanGroupsCount':
        result[key] = serializeCountValue(
          selection,
          runtime.store.listEffectiveSellingPlanGroupsForProductVariant(variant.id).length,
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeProductVariantComponentSelectionSet(
  runtime: ProxyRuntimeContext,
  component: ProductVariantComponentRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ProductVariantComponent';
        break;
      case 'id':
        result[key] = component.id;
        break;
      case 'quantity':
        result[key] = component.quantity;
        break;
      case 'productVariant':
        result[key] = serializeVariantSelectionSet(
          runtime,
          runtime.store.getEffectiveVariantById(component.componentProductVariantId) ?? {
            id: component.componentProductVariantId,
            productId: '',
            title: component.componentProductVariantId.split('/').at(-1) ?? component.componentProductVariantId,
            sku: null,
            barcode: null,
            price: null,
            compareAtPrice: null,
            taxable: null,
            inventoryPolicy: null,
            inventoryQuantity: null,
            selectedOptions: [],
            inventoryItem: null,
          },
          selection.selectionSet?.selections ?? [],
          variables,
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeProductVariantComponentConnection(
  runtime: ProxyRuntimeContext,
  parentProductVariantId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const components = runtime.store.getEffectiveVariantComponentsByParentVariantId(parentProductVariantId);
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    components,
    field,
    variables,
    (component) => component.id,
  );
  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (component) => component.id,
    serializeNode: (component, selection) =>
      serializeProductVariantComponentSelectionSet(
        runtime,
        component,
        selection.selectionSet?.selections ?? [],
        variables,
      ),
  });
}

function serializeOptionSelectionSet(
  option: ProductOptionRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ProductOption';
        break;
      case 'id':
        result[key] = option.id;
        break;
      case 'name':
        result[key] = option.name;
        break;
      case 'position':
        result[key] = option.position;
        break;
      case 'values':
        result[key] = option.optionValues
          .filter((optionValue) => optionValue.hasVariants)
          .map((optionValue) => optionValue.name);
        break;
      case 'optionValues':
        result[key] = option.optionValues.map((optionValue) =>
          serializeOptionValueSelectionSet(optionValue, selection.selectionSet?.selections ?? []),
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeOptionValueSelectionSet(
  optionValue: ProductOptionValueRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ProductOptionValue';
        break;
      case 'id':
        result[key] = optionValue.id;
        break;
      case 'name':
        result[key] = optionValue.name;
        break;
      case 'hasVariants':
        result[key] = optionValue.hasVariants;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

export function serializeProductOptionNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  selectedFields: readonly FieldNode[],
): Record<string, unknown> | null {
  for (const product of runtime.store.listEffectiveProducts()) {
    const option = runtime.store.getEffectiveOptionsByProductId(product.id).find((candidate) => candidate.id === id);
    if (option) {
      return serializeOptionSelectionSet(option, selectedFields);
    }
  }

  return null;
}

export function serializeProductOptionValueNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  selectedFields: readonly FieldNode[],
): Record<string, unknown> | null {
  for (const product of runtime.store.listEffectiveProducts()) {
    for (const option of runtime.store.getEffectiveOptionsByProductId(product.id)) {
      const optionValue = option.optionValues.find((candidate) => candidate.id === id);
      if (optionValue) {
        return serializeOptionValueSelectionSet(optionValue, selectedFields);
      }
    }
  }

  return null;
}

function serializeVariantsConnection(
  runtime: ProxyRuntimeContext,
  productId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const allVariants = runtime.store.getEffectiveVariantsByProductId(productId);
  const {
    items: variants,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allVariants, field, variables, (variant) => variant.id);
  return serializeConnection(field, {
    items: variants,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (variant) => variant.id,
    serializeNode: (variant, selection) =>
      serializeVariantSelectionSet(runtime, variant, selection.selectionSet?.selections ?? [], variables),
  });
}

function serializeCollectionSelectionSet(
  runtime: ProxyRuntimeContext,
  collection: CollectionRecord | ProductCollectionRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    result[key] = serializeCollectionField(runtime, collection, selection, variables);
  }

  return result;
}

function serializeCollectionImage(
  image: CollectionImageRecord | null | undefined,
  selections: readonly SelectionNode[],
): Record<string, unknown> | null {
  if (!image) {
    return null;
  }

  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'id':
        result[key] = image.id ?? null;
        break;
      case 'altText':
        result[key] = image.altText;
        break;
      case 'url':
      case 'src':
      case 'originalSrc':
      case 'transformedSrc':
        result[key] = image.url;
        break;
      case 'width':
        result[key] = image.width ?? null;
        break;
      case 'height':
        result[key] = image.height ?? null;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeCollectionSeo(
  seo: CollectionRecord['seo'],
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const effectiveSeo = seo ?? { title: null, description: null };

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'title':
        result[key] = effectiveSeo.title;
        break;
      case 'description':
        result[key] = effectiveSeo.description;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeCollectionRuleSet(
  ruleSet: CollectionRuleSetRecord | null | undefined,
  selections: readonly SelectionNode[],
): Record<string, unknown> | null {
  if (!ruleSet) {
    return null;
  }

  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'appliedDisjunctively':
        result[key] = ruleSet.appliedDisjunctively;
        break;
      case 'rules':
        result[key] = ruleSet.rules.map((rule) =>
          Object.fromEntries(
            (selection.selectionSet?.selections ?? [])
              .filter((ruleSelection): ruleSelection is FieldNode => ruleSelection.kind === Kind.FIELD)
              .map((ruleSelection) => {
                const ruleKey = ruleSelection.alias?.value ?? ruleSelection.name.value;
                switch (ruleSelection.name.value) {
                  case 'column':
                    return [ruleKey, rule.column];
                  case 'relation':
                    return [ruleKey, rule.relation];
                  case 'condition':
                    return [ruleKey, rule.condition];
                  case 'conditionObject':
                    return [ruleKey, null];
                  default:
                    return [ruleKey, null];
                }
              }),
          ),
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeCollectionField(
  runtime: ProxyRuntimeContext,
  collection: CollectionRecord | ProductCollectionRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
  options: { productsCountOverride?: number } = {},
): unknown {
  const publicationIds = getCollectionPublicationIds(collection);

  switch (field.name.value) {
    case '__typename':
      return 'Collection';
    case 'id':
      return collection.id;
    case 'legacyResourceId':
      return collection.legacyResourceId ?? readLegacyResourceIdFromGid(collection.id);
    case 'title':
      return collection.title;
    case 'handle':
      return collection.handle;
    case 'publishedOnCurrentPublication':
    case 'publishedOnCurrentChannel':
      return false;
    case 'publishedOnPublication': {
      const args = getFieldArguments(field, variables);
      const publicationId = typeof args['publicationId'] === 'string' ? args['publicationId'] : null;
      return publicationId ? publicationIds.includes(publicationId) : false;
    }
    case 'publishedOnChannel': {
      const args = getFieldArguments(field, variables);
      const channelId = typeof args['channelId'] === 'string' ? args['channelId'] : null;
      return channelId ? publicationIds.includes(channelId) : false;
    }
    case 'availablePublicationsCount':
    case 'resourcePublicationsCount':
    case 'publicationCount':
      return serializeCountValue(field, publicationIds.length);
    case 'updatedAt':
      return collection.updatedAt ?? null;
    case 'description': {
      const description =
        collection.description ??
        (collection.descriptionHtml ? stripHtmlToDescription(collection.descriptionHtml) : null);
      const args = getFieldArguments(field, variables);
      const rawTruncateAt = args['truncateAt'];
      if (description && typeof rawTruncateAt === 'number' && rawTruncateAt >= 0) {
        return description.slice(0, rawTruncateAt);
      }
      return description;
    }
    case 'descriptionHtml':
      return collection.descriptionHtml ?? null;
    case 'image':
      return serializeCollectionImage(collection.image, field.selectionSet?.selections ?? []);
    case 'productsCount':
      return serializeCountValue(
        field,
        options.productsCountOverride ?? listEffectiveProductsForCollection(runtime, collection.id).length,
      );
    case 'hasProduct': {
      const args = getFieldArguments(field, variables);
      const productId = typeof args['id'] === 'string' ? args['id'] : null;
      return productId
        ? listEffectiveProductsForCollection(runtime, collection.id).some((product) => product.id === productId)
        : false;
    }
    case 'sortOrder':
      return collection.sortOrder ?? null;
    case 'templateSuffix':
      return collection.templateSuffix ?? null;
    case 'seo':
      return serializeCollectionSeo(collection.seo, field.selectionSet?.selections ?? []);
    case 'ruleSet':
      return serializeCollectionRuleSet(collection.ruleSet, field.selectionSet?.selections ?? []);
    case 'metafield': {
      const args = getFieldArguments(field, variables);
      const namespace = typeof args['namespace'] === 'string' ? args['namespace'] : null;
      const key = typeof args['key'] === 'string' ? args['key'] : null;
      if (!namespace || !key) {
        return null;
      }

      const metafield = getEffectiveMetafieldsForOwner(runtime, collection.id).find(
        (candidate) => candidate.namespace === namespace && candidate.key === key,
      );
      return metafield ? serializeMetafieldSelectionSet(metafield, field.selectionSet?.selections ?? []) : null;
    }
    case 'metafields':
      return serializeOwnerMetafieldsConnection(
        getEffectiveMetafieldsForOwner(runtime, collection.id),
        field,
        variables,
      );
    default:
      return null;
  }
}

function serializeCollectionObject(
  runtime: ProxyRuntimeContext,
  collection: CollectionRecord | ProductCollectionRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
  options: { productsCountOverride?: number } = {},
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeName = selection.typeCondition?.name.value;
      if (typeName && typeName !== 'Collection' && typeName !== 'Publishable' && typeName !== 'Node') {
        continue;
      }

      Object.assign(
        result,
        serializeCollectionObject(runtime, collection, selection.selectionSet.selections, variables, options),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'products': {
        const args = getFieldArguments(selection, variables);
        const rawFirst = args['first'];
        const rawLast = args['last'];
        const first = typeof rawFirst === 'number' ? rawFirst : null;
        const last = typeof rawLast === 'number' ? rawLast : null;
        result[key] = serializeProductsConnection(
          runtime,
          listEffectiveProductsForCollection(runtime, collection.id),
          selection,
          first,
          last,
          args['after'],
          args['before'],
          args['query'],
          args['sortKey'],
          args['reverse'],
          variables,
          { preserveDefaultOrder: true },
        );
        break;
      }
      default:
        result[key] = serializeCollectionField(runtime, collection, selection, variables, options);
    }
  }

  return result;
}

function serializeCollectionsConnection(
  runtime: ProxyRuntimeContext,
  productId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const allCollections = sortCollections(
    applyCollectionsQuery(runtime, runtime.store.getEffectiveCollectionsByProductId(productId), args['query']),
    args['sortKey'],
    args['reverse'],
    args['query'],
  );
  const {
    items: collections,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allCollections, field, variables, (collection) => collection.id);
  return serializeConnection(field, {
    items: collections,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (collection) => collection.id,
    serializeNode: (collection, selection) =>
      serializeCollectionSelectionSet(runtime, collection, selection.selectionSet?.selections ?? [], variables),
  });
}

function readCollectionPublishedStatus(rawQuery: unknown): 'published' | 'unpublished' | 'any' | null {
  if (typeof rawQuery !== 'string') {
    return null;
  }

  const match = rawQuery.match(/(?:^|\s)published_status:\s*(?:"([^"]+)"|'([^']+)'|(\S+))/iu);
  const value = (match?.[1] ?? match?.[2] ?? match?.[3] ?? '').toLowerCase();
  if (value === 'published' || value === 'visible') {
    return 'published';
  }
  if (value === 'unpublished' || value === 'hidden') {
    return 'unpublished';
  }
  if (value === 'any') {
    return 'any';
  }

  return null;
}

function filterCollectionsByQuery(collections: CollectionRecord[], rawQuery: unknown): CollectionRecord[] {
  const publishedStatus = readCollectionPublishedStatus(rawQuery);
  if (!publishedStatus || publishedStatus === 'any') {
    return collections;
  }

  return collections.filter((collection) =>
    publishedStatus === 'published' ? isPublishedCollection(collection) : !isPublishedCollection(collection),
  );
}

function serializeTopLevelCollectionsConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const allCollections = sortCollections(
    applyCollectionsQuery(
      runtime,
      filterCollectionsByQuery(listEffectiveCollections(runtime), args['query']),
      args['query'],
    ),
    args['sortKey'],
    args['reverse'],
    args['query'],
  );
  const {
    items: collections,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allCollections, field, variables, (collection) => collection.id);
  return serializeConnection(field, {
    items: collections,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (collection) => collection.id,
    serializeNode: (collection, selection) =>
      serializeCollectionObject(runtime, collection, selection.selectionSet?.selections ?? [], variables),
  });
}

function serializeLocationSelectionSet(
  runtime: ProxyRuntimeContext,
  location: LocationRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  return serializeStorePropertiesLocation(runtime, location, selections, variables);
}

function serializeTopLevelLocationsConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const allLocations = listEffectiveLocations(runtime);
  const {
    items: locations,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allLocations, field, variables, (location) => location.id);
  return serializeConnection(field, {
    items: locations,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (location) => location.id,
    serializeNode: (location, selection) =>
      serializeLocationSelectionSet(runtime, location, selection.selectionSet?.selections ?? [], variables),
  });
}

function listProductsPublishedToPublication(runtime: ProxyRuntimeContext, publicationId: string): ProductRecord[] {
  return runtime.store
    .listEffectiveProducts()
    .filter((product) => product.status === 'ACTIVE' && product.publicationIds.includes(publicationId));
}

function countCollectionsPublishedToPublication(runtime: ProxyRuntimeContext, publicationId: string): number {
  return listEffectiveCollections(runtime).filter((collection) =>
    (collection.publicationIds ?? []).includes(publicationId),
  ).length;
}

function serializeChannelSelectionSet(
  runtime: ProxyRuntimeContext,
  channel: ChannelRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown> = {},
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const publicationId = channel.publicationId ?? null;

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeName = selection.typeCondition?.name.value;
      if (typeName && typeName !== 'Channel' && typeName !== 'Node') {
        continue;
      }

      Object.assign(
        result,
        serializeChannelSelectionSet(runtime, channel, selection.selectionSet.selections, variables),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Channel';
        break;
      case 'id':
        result[key] = channel.id;
        break;
      case 'name':
        result[key] = channel.name;
        break;
      case 'handle':
        result[key] = channel.handle ?? null;
        break;
      case 'publication':
        result[key] = publicationId
          ? serializePublicationSelectionSet(
              runtime,
              runtime.store.getEffectivePublicationById(publicationId) ?? { id: publicationId, name: channel.name },
              selection.selectionSet?.selections ?? [],
              variables,
            )
          : null;
        break;
      case 'productsCount':
        result[key] = serializeCountValue(
          selection,
          publicationId ? listProductsPublishedToPublication(runtime, publicationId).length : 0,
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeTopLevelChannelsConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const allChannels = listEffectiveChannels(runtime);
  const {
    items: channels,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allChannels, field, variables, (channel) => channel.cursor ?? channel.id, {
    parseCursor: (raw) => raw,
  });
  return serializeConnection(field, {
    items: channels,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (channel) => channel.cursor ?? `cursor:${channel.id}`,
    serializeNode: (channel, selection) =>
      serializeChannelSelectionSet(runtime, channel, selection.selectionSet?.selections ?? [], variables),
    pageInfoOptions: {
      prefixCursors: false,
    },
  });
}

function serializePublicationSelectionSet(
  runtime: ProxyRuntimeContext,
  publication: PublicationRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown> = {},
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeName = selection.typeCondition?.name.value;
      if (typeName && typeName !== 'Publication' && typeName !== 'Node') {
        continue;
      }

      Object.assign(
        result,
        serializePublicationSelectionSet(runtime, publication, selection.selectionSet.selections, variables),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Publication';
        break;
      case 'id':
        result[key] = publication.id;
        break;
      case 'name':
        result[key] = publication.name;
        break;
      case 'autoPublish':
        result[key] = publication.autoPublish ?? false;
        break;
      case 'supportsFuturePublishing':
        result[key] = publication.supportsFuturePublishing ?? false;
        break;
      case 'catalog':
        result[key] = publication.catalogId
          ? Object.fromEntries(
              (selection.selectionSet?.selections ?? [])
                .filter((catalogSelection): catalogSelection is FieldNode => catalogSelection.kind === Kind.FIELD)
                .map((catalogSelection) => {
                  const catalogKey = catalogSelection.alias?.value ?? catalogSelection.name.value;
                  switch (catalogSelection.name.value) {
                    case '__typename':
                      return [catalogKey, 'MarketCatalog'];
                    case 'id':
                      return [catalogKey, publication.catalogId];
                    default:
                      return [catalogKey, null];
                  }
                }),
            )
          : null;
        break;
      case 'channel': {
        const channel =
          runtime.store.listEffectiveChannels().find((candidate) => candidate.publicationId === publication.id) ?? null;
        result[key] = channel
          ? serializeChannelSelectionSet(runtime, channel, selection.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'products': {
        const args = getFieldArguments(selection, variables);
        const rawFirst = args['first'];
        const rawLast = args['last'];
        result[key] = serializeProductsConnection(
          runtime,
          listProductsPublishedToPublication(runtime, publication.id),
          selection,
          typeof rawFirst === 'number' ? rawFirst : null,
          typeof rawLast === 'number' ? rawLast : null,
          args['after'],
          args['before'],
          args['query'],
          args['sortKey'],
          args['reverse'],
          variables,
        );
        break;
      }
      case 'productsCount':
      case 'publishedProductsCount':
        result[key] = serializeCountValue(
          selection,
          listProductsPublishedToPublication(runtime, publication.id).length,
        );
        break;
      case 'collectionsCount':
        result[key] = serializeCountValue(selection, countCollectionsPublishedToPublication(runtime, publication.id));
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializePublicationCursor(publication: PublicationRecord): string {
  return typeof publication.cursor === 'string' && publication.cursor.length > 0
    ? publication.cursor
    : `cursor:${publication.id}`;
}

function serializeTopLevelPublicationsConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const allPublications = listEffectivePublications(runtime);
  const {
    items: publications,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allPublications, field, variables, serializePublicationCursor, {
    parseCursor: (raw) => raw,
  });
  return serializeConnection(field, {
    items: publications,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: serializePublicationCursor,
    serializeNode: (publication, selection) =>
      serializePublicationSelectionSet(runtime, publication, selection.selectionSet?.selections ?? []),
    pageInfoOptions: {
      prefixCursors: false,
    },
  });
}

function serializeProductFeedSelectionSet(
  productFeed: ProductFeedRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeName = selection.typeCondition?.name.value;
      if (typeName && typeName !== 'ProductFeed' && typeName !== 'Node') {
        continue;
      }

      Object.assign(result, serializeProductFeedSelectionSet(productFeed, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ProductFeed';
        break;
      case 'id':
        result[key] = productFeed.id;
        break;
      case 'country':
        result[key] = productFeed.country;
        break;
      case 'language':
        result[key] = productFeed.language;
        break;
      case 'status':
        result[key] = productFeed.status;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeTopLevelProductFeedsConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const productFeeds = runtime.store.listEffectiveProductFeeds();
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    productFeeds,
    field,
    variables,
    (productFeed) => productFeed.id,
  );

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (productFeed) => productFeed.id,
    serializeNode: (productFeed, selection) =>
      serializeProductFeedSelectionSet(productFeed, selection.selectionSet?.selections ?? []),
  });
}

function serializeProductResourceFeedbackSelectionSet(
  feedback: ProductResourceFeedbackRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ProductResourceFeedback';
        break;
      case 'productId':
        result[key] = feedback.productId;
        break;
      case 'state':
        result[key] = feedback.state;
        break;
      case 'messages':
        result[key] = structuredClone(feedback.messages);
        break;
      case 'feedbackGeneratedAt':
        result[key] = feedback.feedbackGeneratedAt;
        break;
      case 'productUpdatedAt':
        result[key] = feedback.productUpdatedAt;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeUserErrorLikeSelections(
  error: { field?: string[] | null; message: string; code?: string | null },
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'field':
        result[key] = error.field ?? null;
        break;
      case 'message':
        result[key] = error.message;
        break;
      case 'code':
        result[key] = error.code ?? null;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeUserErrorLikeList(
  errors: Array<{ field?: string[] | null; message: string; code?: string | null }>,
  field: FieldNode | null,
): Array<Record<string, unknown>> {
  return errors.map((error) => serializeUserErrorLikeSelections(error, field?.selectionSet?.selections ?? []));
}

function serializeAppFeedbackSelectionSet(
  feedback: ShopResourceFeedbackRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'AppFeedback';
        break;
      case 'state':
        result[key] = feedback.state;
        break;
      case 'feedbackGeneratedAt':
        result[key] = feedback.feedbackGeneratedAt;
        break;
      case 'messages':
        result[key] = feedback.messages.map((message) =>
          serializeUserErrorLikeSelections({ message }, selection.selectionSet?.selections ?? []),
        );
        break;
      case 'app':
      case 'link':
        result[key] = null;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeMediaImageSelectionSet(
  imageUrl: string | null,
  selections: readonly SelectionNode[],
): Record<string, unknown> | null {
  if (!imageUrl) {
    return null;
  }

  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'url':
        result[key] = imageUrl;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function getProductMediaTypename(media: ProductMediaRecord): string {
  switch (media.mediaContentType) {
    case 'IMAGE':
      return 'MediaImage';
    case 'VIDEO':
      return 'Video';
    case 'EXTERNAL_VIDEO':
      return 'ExternalVideo';
    case 'MODEL_3D':
      return 'Model3d';
    default:
      return 'Media';
  }
}

function mediaInlineFragmentApplies(media: ProductMediaRecord, typeName: string): boolean {
  return typeName === 'Media' || typeName === getProductMediaTypename(media);
}

function serializeMediaSelectionSet(
  media: ProductMediaRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeName = selection.typeCondition?.name.value;
      if (!typeName || !mediaInlineFragmentApplies(media, typeName)) {
        continue;
      }

      Object.assign(result, serializeMediaSelectionSet(media, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = getProductMediaTypename(media);
        break;
      case 'id':
        result[key] = media.id ?? null;
        break;
      case 'mediaContentType':
        result[key] = media.mediaContentType;
        break;
      case 'alt':
        result[key] = media.alt;
        break;
      case 'status':
        result[key] = media.status ?? null;
        break;
      case 'preview':
        result[key] = Object.fromEntries(
          (selection.selectionSet?.selections ?? [])
            .filter((previewSelection): previewSelection is FieldNode => previewSelection.kind === Kind.FIELD)
            .map((previewSelection) => {
              const previewKey = previewSelection.alias?.value ?? previewSelection.name.value;
              switch (previewSelection.name.value) {
                case 'image':
                  return [
                    previewKey,
                    serializeMediaImageSelectionSet(
                      media.previewImageUrl,
                      previewSelection.selectionSet?.selections ?? [],
                    ),
                  ];
                default:
                  return [previewKey, null];
              }
            }),
        );
        break;
      case 'image':
        result[key] = serializeMediaImageSelectionSet(
          media.imageUrl ?? media.previewImageUrl,
          selection.selectionSet?.selections ?? [],
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function promoteProcessingMediaAfterRead(
  runtime: ProxyRuntimeContext,
  productId: string,
  mediaRecords: ProductMediaRecord[],
): void {
  const needsPromotion = mediaRecords.some((mediaRecord) => mediaRecord.status === 'PROCESSING');
  if (!needsPromotion) {
    return;
  }

  const nextMedia = runtime.store
    .getEffectiveMediaByProductId(productId)
    .map((mediaRecord) => (mediaRecord.status === 'PROCESSING' ? transitionMediaToReady(mediaRecord) : mediaRecord));
  runtime.store.replaceStagedMediaForProduct(productId, nextMedia);
}

function serializeMediaConnection(
  runtime: ProxyRuntimeContext,
  productId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const allMediaRecords = runtime.store.getEffectiveMediaByProductId(productId);
  const {
    items: mediaRecords,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allMediaRecords, field, variables, (mediaRecord) => mediaRecord.key);
  const result = serializeConnection(field, {
    items: mediaRecords,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (mediaRecord) => mediaRecord.key,
    serializeNode: (mediaRecord, selection) =>
      serializeMediaSelectionSet(mediaRecord, selection.selectionSet?.selections ?? []),
  });

  promoteProcessingMediaAfterRead(runtime, productId, allMediaRecords);
  return result;
}

function serializeVariantMediaConnection(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const productMediaById = new Map(
    runtime.store
      .getEffectiveMediaByProductId(variant.productId)
      .filter((mediaRecord) => typeof mediaRecord.id === 'string')
      .map((mediaRecord) => [mediaRecord.id as string, mediaRecord]),
  );
  const allMediaRecords = (variant.mediaIds ?? [])
    .map((mediaId) => productMediaById.get(mediaId) ?? null)
    .filter((mediaRecord): mediaRecord is ProductMediaRecord => mediaRecord !== null);
  const {
    items: mediaRecords,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allMediaRecords, field, variables, (mediaRecord) => mediaRecord.id ?? mediaRecord.key);

  return serializeConnection(field, {
    items: mediaRecords,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (mediaRecord) => mediaRecord.id ?? mediaRecord.key,
    serializeNode: (mediaRecord, selection) =>
      serializeMediaSelectionSet(mediaRecord, selection.selectionSet?.selections ?? []),
  });
}

function productImageInlineFragmentApplies(typeName: string): boolean {
  return typeName === 'Image';
}

function getProductImageUrl(media: ProductMediaRecord): string | null {
  return media.imageUrl ?? media.previewImageUrl ?? media.sourceUrl ?? null;
}

function serializeProductImageSelectionSet(
  media: ProductMediaRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const imageUrl = getProductImageUrl(media);

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeName = selection.typeCondition?.name.value;
      if (!typeName || !productImageInlineFragmentApplies(typeName)) {
        continue;
      }

      Object.assign(result, serializeProductImageSelectionSet(media, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Image';
        break;
      case 'id':
        result[key] = media.productImageId ?? media.id ?? null;
        break;
      case 'altText':
        result[key] = media.alt;
        break;
      case 'url':
      case 'src':
      case 'originalSrc':
      case 'transformedSrc':
        result[key] = imageUrl;
        break;
      case 'width':
        result[key] = media.imageWidth ?? null;
        break;
      case 'height':
        result[key] = media.imageHeight ?? null;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeProductImagesConnection(
  runtime: ProxyRuntimeContext,
  productId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const allMediaRecords = runtime.store.getEffectiveMediaByProductId(productId);
  const allImageRecords = allMediaRecords.filter(
    (mediaRecord) =>
      mediaRecord.mediaContentType === 'IMAGE' &&
      (mediaRecord.productImageId !== null || getProductImageUrl(mediaRecord) !== null),
  );
  const {
    items: imageRecords,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allImageRecords, field, variables, (mediaRecord) => mediaRecord.key);
  const result = serializeConnection(field, {
    items: imageRecords,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (mediaRecord) => mediaRecord.key,
    serializeNode: (mediaRecord, selection) =>
      serializeProductImageSelectionSet(mediaRecord, selection.selectionSet?.selections ?? []),
  });

  promoteProcessingMediaAfterRead(runtime, productId, allMediaRecords);
  return result;
}

function serializeBundleOptionSelectionValue(
  value: string,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }
    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'value':
      case 'name':
        result[key] = value;
        break;
      case 'selectionStatus':
        result[key] = 'SELECTED';
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeBundleComponentOptionSelection(
  runtime: ProxyRuntimeContext,
  component: ProductBundleComponentRecord,
  optionSelection: ProductBundleComponentOptionSelectionRecord,
  field: FieldNode,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const componentOption =
    runtime.store
      .getEffectiveOptionsByProductId(component.componentProductId)
      .find((option) => option.id === optionSelection.componentOptionId || option.name === optionSelection.name) ??
    null;
  const parentOption =
    runtime.store
      .getEffectiveOptionsByProductId(component.bundleProductId)
      .find((option) => option.name === optionSelection.name) ?? null;

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ProductBundleComponentOptionSelection';
        break;
      case 'componentOption':
        result[key] = componentOption
          ? serializeOptionSelectionSet(componentOption, selection.selectionSet?.selections ?? [])
          : null;
        break;
      case 'parentOption':
        result[key] = parentOption
          ? serializeOptionSelectionSet(parentOption, selection.selectionSet?.selections ?? [])
          : null;
        break;
      case 'values':
        result[key] = optionSelection.values.map((value) =>
          serializeBundleOptionSelectionValue(value, selection.selectionSet?.selections ?? []),
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeBundleQuantityOptionValue(
  value: { name: string; quantity: number },
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }
    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'name':
        result[key] = value.name;
        break;
      case 'quantity':
        result[key] = value.quantity;
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeBundleQuantityOption(
  quantityOption: ProductBundleComponentQuantityOptionRecord,
  field: FieldNode,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }
    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ProductBundleComponentQuantityOption';
        break;
      case 'name':
        result[key] = quantityOption.name;
        break;
      case 'values':
        result[key] = quantityOption.values.map((value) =>
          serializeBundleQuantityOptionValue(value, selection.selectionSet?.selections ?? []),
        );
        break;
      case 'parentOption':
        result[key] = null;
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeProductBundleComponentSelectionSet(
  runtime: ProxyRuntimeContext,
  component: ProductBundleComponentRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ProductBundleComponent';
        break;
      case 'componentProduct':
        result[key] = serializeProduct(
          runtime,
          runtime.store.getEffectiveProductById(component.componentProductId),
          selection,
          variables,
        );
        break;
      case 'componentVariants':
        result[key] = serializeVariantsConnection(runtime, component.componentProductId, selection, variables);
        break;
      case 'componentVariantsCount':
        result[key] = serializeCountValue(
          selection,
          runtime.store.getEffectiveVariantsByProductId(component.componentProductId).length,
        );
        break;
      case 'optionSelections':
        result[key] = component.optionSelections.map((optionSelection) =>
          serializeBundleComponentOptionSelection(runtime, component, optionSelection, selection),
        );
        break;
      case 'quantity':
        result[key] = component.quantity;
        break;
      case 'quantityOption':
        result[key] = component.quantityOption
          ? serializeBundleQuantityOption(component.quantityOption, selection)
          : null;
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeProductBundleComponentsConnection(
  runtime: ProxyRuntimeContext,
  productId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const components = runtime.store.getEffectiveBundleComponentsByProductId(productId);
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    components,
    field,
    variables,
    (component) => component.id,
  );
  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (component) => component.id,
    serializeNode: (component, selection) =>
      serializeProductBundleComponentSelectionSet(
        runtime,
        component,
        selection.selectionSet?.selections ?? [],
        variables,
      ),
  });
}

function serializeCombinedListingChildSelectionSet(
  runtime: ProxyRuntimeContext,
  child: CombinedListingChildRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }
    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'CombinedListingChild';
        break;
      case 'product':
        result[key] = serializeProduct(
          runtime,
          runtime.store.getEffectiveProductById(child.childProductId),
          selection,
          variables,
        );
        break;
      case 'parentVariant': {
        const parentVariant =
          runtime.store
            .getEffectiveVariantsByProductId(child.parentProductId)
            .find((variant) =>
              child.selectedParentOptionValues.every((optionValue) =>
                variant.selectedOptions.some(
                  (selectedOption) =>
                    selectedOption.name === optionValue.name && selectedOption.value === optionValue.value,
                ),
              ),
            ) ??
          runtime.store.getEffectiveVariantsByProductId(child.parentProductId)[0] ??
          null;
        result[key] = parentVariant
          ? serializeVariantSelectionSet(runtime, parentVariant, selection.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeCombinedListingChildrenConnection(
  runtime: ProxyRuntimeContext,
  parentProductId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const children = runtime.store.getEffectiveCombinedListingChildrenByParentId(parentProductId);
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    children,
    field,
    variables,
    (child) => child.childProductId,
  );
  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (child) => child.childProductId,
    serializeNode: (child, selection) =>
      serializeCombinedListingChildSelectionSet(runtime, child, selection.selectionSet?.selections ?? [], variables),
  });
}

function serializeCombinedListing(
  runtime: ProxyRuntimeContext,
  product: ProductRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): unknown {
  if ((product.combinedListingRole ?? null) !== 'PARENT') {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }
    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'CombinedListing';
        break;
      case 'parentProduct':
        result[key] = serializeProduct(runtime, product, selection, variables);
        break;
      case 'combinedListingChildren':
        result[key] = serializeCombinedListingChildrenConnection(runtime, product.id, selection, variables);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeProductField(
  runtime: ProxyRuntimeContext,
  product: ProductRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): unknown {
  const visiblePublicationCount = product.status === 'ACTIVE' ? product.publicationIds.length : 0;

  switch (field.name.value) {
    case '__typename':
      return 'Product';
    case 'id':
      return product.id;
    case 'legacyResourceId':
      return product.legacyResourceId;
    case 'title':
      return product.title;
    case 'handle':
      return product.handle;
    case 'status':
      return product.status;
    case 'publishedOnCurrentPublication':
    case 'publishedOnCurrentChannel':
      return visiblePublicationCount > 0;
    case 'publishedOnPublication': {
      const args = getFieldArguments(field, variables);
      const publicationId = typeof args['publicationId'] === 'string' ? args['publicationId'] : null;
      if (!publicationId || product.status !== 'ACTIVE') {
        return false;
      }

      return product.publicationIds.includes(publicationId);
    }
    case 'publishedOnChannel': {
      const args = getFieldArguments(field, variables);
      const channelId = typeof args['channelId'] === 'string' ? args['channelId'] : null;
      if (!channelId || product.status !== 'ACTIVE') {
        return false;
      }

      return product.publicationIds.includes(channelId);
    }
    case 'availablePublicationsCount':
      return serializeCountValue(field, visiblePublicationCount);
    case 'resourcePublicationsCount':
      return serializeCountValue(field, visiblePublicationCount);
    case 'vendor':
      return product.vendor;
    case 'productType':
      return product.productType;
    case 'tags':
      return structuredClone(product.tags);
    case 'totalInventory':
      return product.totalInventory;
    case 'tracksInventory':
      return product.tracksInventory;
    case 'requiresSellingPlan':
      return false;
    case 'combinedListingRole': {
      const parentLink = runtime.store.getEffectiveCombinedListingParentByChildId(product.id);
      return product.combinedListingRole ?? (parentLink ? 'CHILD' : null);
    }
    case 'combinedListing':
      return serializeCombinedListing(runtime, product, field, variables);
    case 'bundleComponents':
      return serializeProductBundleComponentsConnection(runtime, product.id, field, variables);
    case 'createdAt':
      return product.createdAt;
    case 'updatedAt':
      return product.updatedAt;
    case 'publishedAt':
      return product.publishedAt ?? null;
    case 'descriptionHtml':
      return product.descriptionHtml;
    case 'onlineStorePreviewUrl':
      return product.onlineStorePreviewUrl;
    case 'templateSuffix':
      return product.templateSuffix;
    case 'seo':
      return Object.fromEntries(
        (field.selectionSet?.selections ?? [])
          .filter((seoSelection): seoSelection is FieldNode => seoSelection.kind === Kind.FIELD)
          .map((seoSelection) => {
            const seoKey = seoSelection.alias?.value ?? seoSelection.name.value;
            switch (seoSelection.name.value) {
              case 'title':
                return [seoKey, product.seo.title];
              case 'description':
                return [seoKey, product.seo.description];
              default:
                return [seoKey, null];
            }
          }),
      );
    case 'category':
      if (!product.category) {
        return null;
      }
      return Object.fromEntries(
        (field.selectionSet?.selections ?? [])
          .filter((categorySelection): categorySelection is FieldNode => categorySelection.kind === Kind.FIELD)
          .map((categorySelection) => {
            const categoryKey = categorySelection.alias?.value ?? categorySelection.name.value;
            switch (categorySelection.name.value) {
              case 'id':
                return [categoryKey, product.category?.id ?? null];
              case 'fullName':
                return [categoryKey, product.category?.fullName ?? null];
              default:
                return [categoryKey, null];
            }
          }),
      );
    case 'options':
      return runtime.store
        .getEffectiveOptionsByProductId(product.id)
        .map((option) => serializeOptionSelectionSet(option, field.selectionSet?.selections ?? []));
    case 'variants':
      return serializeVariantsConnection(runtime, product.id, field, variables);
    case 'collections':
      return serializeCollectionsConnection(runtime, product.id, field, variables);
    case 'media':
      return serializeMediaConnection(runtime, product.id, field, variables);
    case 'images':
      return serializeProductImagesConnection(runtime, product.id, field, variables);
    case 'metafield': {
      const args = getFieldArguments(field, variables);
      const namespace = typeof args['namespace'] === 'string' ? args['namespace'] : null;
      const key = typeof args['key'] === 'string' ? args['key'] : null;
      if (!namespace || !key) {
        return null;
      }

      const metafield = runtime.store
        .getEffectiveMetafieldsByOwnerId(product.id)
        .find((candidate) => candidate.namespace === namespace && candidate.key === key);
      return metafield ? serializeMetafieldSelectionSet(metafield, field.selectionSet?.selections ?? []) : null;
    }
    case 'metafields':
      return serializeOwnerMetafieldsConnection(getEffectiveMetafieldsForOwner(runtime, product.id), field, variables);
    case 'contextualPricing':
      return projectGraphqlValue(product.contextualPricing, field.selectionSet?.selections ?? [], new Map());
    case 'sellingPlanGroups':
      return serializeSellingPlanGroupConnection(
        runtime,
        runtime.store.listEffectiveSellingPlanGroupsVisibleForProduct(product.id),
        field,
        variables,
      );
    case 'sellingPlanGroupsCount':
      return serializeCountValue(field, runtime.store.listEffectiveSellingPlanGroupsForProduct(product.id).length);
    default:
      return null;
  }
}

function serializeSelectionSet(
  runtime: ProxyRuntimeContext,
  product: ProductRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeName = selection.typeCondition?.name.value;
      if (typeName && typeName !== 'Product' && typeName !== 'Node') {
        continue;
      }

      Object.assign(result, serializeSelectionSet(runtime, product, selection.selectionSet.selections, variables));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    result[key] = serializeProductField(runtime, product, selection, variables);
  }

  return result;
}

function serializeProduct(
  runtime: ProxyRuntimeContext,
  product: ProductRecord | null,
  field: FieldNode | null,
  variables: Record<string, unknown>,
): unknown {
  if (!product) {
    return null;
  }

  const selections = field?.selectionSet?.selections ?? [];
  return serializeSelectionSet(runtime, product, selections, variables);
}

export function serializeProductBulkSelection(
  runtime: ProxyRuntimeContext,
  product: ProductRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  return serializeSelectionSet(runtime, product, selections, variables);
}

function serializeProductsCount(
  runtime: ProxyRuntimeContext,
  rawQuery: unknown,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const filteredProducts = applyProductsQuery(runtime, runtime.store.listEffectiveProducts(), rawQuery);
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'count':
        result[key] = filteredProducts.length;
        break;
      case 'precision':
        result[key] = 'EXACT';
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function matchesProductTimestampTerm(productValue: string, rawValue: string): boolean {
  const match = rawValue.match(/^(<=|>=|<|>|=)?\s*(.+)$/);
  if (!match) {
    return true;
  }

  const operator = match[1] ?? '=';
  const thresholdValue = stripSearchQueryValueQuotes(match[2]?.trim() ?? '');
  if (!thresholdValue) {
    return true;
  }

  const productTime = Date.parse(productValue);
  const thresholdTime = Date.parse(thresholdValue);
  if (Number.isNaN(productTime) || Number.isNaN(thresholdTime)) {
    return true;
  }

  switch (operator) {
    case '<=':
      return productTime <= thresholdTime;
    case '>=':
      return productTime >= thresholdTime;
    case '<':
      return productTime < thresholdTime;
    case '>':
      return productTime > thresholdTime;
    case '=':
      return productTime === thresholdTime;
    default:
      return true;
  }
}

function matchesNullableProductTimestampTerm(productValue: string | null, rawValue: string): boolean {
  const normalizedValue = stripSearchQueryValueQuotes(rawValue);
  if (normalizedValue === '*') {
    return productValue !== null;
  }

  return productValue === null ? false : matchesProductTimestampTerm(productValue, normalizedValue);
}

function isProductPublished(product: Pick<ProductRecord, 'publicationIds' | 'status'>): boolean {
  return product.status === 'ACTIVE' && product.publicationIds.length > 0;
}

function matchesProductPublicationStatus(product: ProductRecord, rawValue: string): boolean {
  const normalizedValue = stripSearchQueryValueQuotes(rawValue).trim().toLowerCase();
  if (normalizedValue === 'published' || normalizedValue === 'visible') {
    return isProductPublished(product);
  }
  if (normalizedValue === 'unpublished' || normalizedValue === 'hidden') {
    return !isProductPublished(product);
  }
  if (normalizedValue === 'any') {
    return true;
  }

  return true;
}
function matchesStringValue(candidate: string, rawValue: string, matchMode: 'includes' | 'exact'): boolean {
  return matchesSearchQueryString(candidate, rawValue, matchMode, { wordPrefix: true });
}

function getSearchableProductTags(runtime: ProxyRuntimeContext, product: ProductRecord): string[] {
  if (!runtime.store.isTagSearchLagged(product.id)) {
    return product.tags;
  }

  const baseProduct = runtime.store.getBaseProductById(product.id);
  if (!baseProduct) {
    return product.tags;
  }

  return product.tags.filter((tag) => baseProduct.tags.includes(tag));
}

function getSearchableProductVariants(runtime: ProxyRuntimeContext, product: ProductRecord): ProductVariantRecord[] {
  if (!runtime.store.isVariantSearchLagged(product.id)) {
    return runtime.store.getEffectiveVariantsByProductId(product.id);
  }

  const baseProduct = runtime.store.getBaseProductById(product.id);
  if (!baseProduct) {
    return [];
  }

  return runtime.store.getBaseVariantsByProductId(baseProduct.id);
}

function matchesProductSearchText(runtime: ProxyRuntimeContext, product: ProductRecord, rawValue: string): boolean {
  const searchableValues = [
    product.title,
    product.handle,
    product.vendor ?? '',
    product.productType ?? '',
    ...getSearchableProductTags(runtime, product),
  ];

  return searchableValues.some((candidate) => matchesStringValue(candidate, rawValue, 'includes'));
}

function matchesPositiveProductQueryTerm(
  runtime: ProxyRuntimeContext,
  product: ProductRecord,
  term: SearchQueryTerm,
): boolean {
  if (term.field === null) {
    return matchesProductSearchText(runtime, product, term.value);
  }

  const field = term.field.toLowerCase();
  const value = searchQueryTermValue(term);

  switch (field) {
    case 'title':
      return matchesStringValue(product.title, value, 'includes');
    case 'handle':
      return matchesStringValue(product.handle, value, 'exact');
    case 'tag':
      return getSearchableProductTags(runtime, product).some((tag) => matchesStringValue(tag, value, 'exact'));
    case 'product_type':
      return typeof product.productType === 'string' && matchesStringValue(product.productType, value, 'exact');
    case 'vendor':
      return typeof product.vendor === 'string' && matchesStringValue(product.vendor, value, 'exact');
    case 'status':
      return matchesStringValue(product.status, value, 'exact');
    case 'created_at':
      return matchesProductTimestampTerm(product.createdAt, value);
    case 'published_at':
      return matchesNullableProductTimestampTerm(product.publishedAt ?? null, value);
    case 'published_status':
    case 'product_publication_status':
    case 'publishable_status':
      return matchesProductPublicationStatus(product, value);
    case 'updated_at':
      return matchesProductTimestampTerm(product.updatedAt, value);
    case 'tag_not':
      return !getSearchableProductTags(runtime, product).some((tag) => matchesStringValue(tag, value, 'exact'));
    case 'sku':
      return getSearchableProductVariants(runtime, product).some(
        (variant) => typeof variant.sku === 'string' && matchesStringValue(variant.sku, value, 'exact'),
      );
    case 'barcode':
      return getSearchableProductVariants(runtime, product).some(
        (variant) => typeof variant.barcode === 'string' && matchesStringValue(variant.barcode, value, 'exact'),
      );
    case 'inventory_total': {
      if (product.totalInventory === null) {
        return false;
      }

      const match = value.match(/^(<=|>=|<|>|=)?\s*(-?\d+)$/);
      if (!match) {
        return true;
      }

      const operator = match[1] ?? '=';
      const threshold = Number.parseInt(match[2] ?? '0', 10);
      switch (operator) {
        case '<=':
          return product.totalInventory <= threshold;
        case '>=':
          return product.totalInventory >= threshold;
        case '<':
          return product.totalInventory < threshold;
        case '>':
          return product.totalInventory > threshold;
        case '=':
          return product.totalInventory === threshold;
        default:
          return true;
      }
    }
    default:
      return true;
  }
}

function applyProductsQuery(
  runtime: ProxyRuntimeContext,
  products: ProductRecord[],
  rawQuery: unknown,
): ProductRecord[] {
  return applySearchQuery(products, rawQuery, { recognizeNotKeyword: true }, (product, term) =>
    matchesPositiveProductQueryTerm(runtime, product, term),
  );
}

function collectionIsSmart(collection: CollectionRecord | ProductCollectionRecord): boolean {
  return collection.isSmart === true || Boolean(collection.ruleSet);
}

function matchesResourceIdValue(resourceId: string, rawValue: string): boolean {
  const normalizedValue = stripSearchQueryValueQuotes(rawValue).trim();
  if (!normalizedValue) {
    return true;
  }

  if (normalizedValue.startsWith('gid://')) {
    return resourceId === normalizedValue;
  }

  return readLegacyResourceIdFromGid(resourceId) === normalizedValue;
}

function matchesResourceIdRange(resourceId: string, rawValue: string): boolean {
  const match = rawValue.match(/^(<=|>=|<|>|=)?\s*(.+)$/);
  if (!match) {
    return matchesResourceIdValue(resourceId, rawValue);
  }

  const operator = match[1] ?? '=';
  const thresholdValue = stripSearchQueryValueQuotes(match[2]?.trim() ?? '');
  if (!thresholdValue) {
    return true;
  }

  if (operator === '=') {
    return matchesResourceIdValue(resourceId, thresholdValue);
  }

  const resourceNumericId = Number.parseInt(resourceId.split('/').at(-1) ?? '', 10);
  const thresholdNumericId = Number.parseInt(thresholdValue.split('/').at(-1) ?? thresholdValue, 10);
  if (!Number.isFinite(resourceNumericId) || !Number.isFinite(thresholdNumericId)) {
    return true;
  }

  switch (operator) {
    case '<=':
      return resourceNumericId <= thresholdNumericId;
    case '>=':
      return resourceNumericId >= thresholdNumericId;
    case '<':
      return resourceNumericId < thresholdNumericId;
    case '>':
      return resourceNumericId > thresholdNumericId;
    default:
      return true;
  }
}

function collectionHasProduct(
  runtime: ProxyRuntimeContext,
  collection: CollectionRecord | ProductCollectionRecord,
  rawValue: string,
): boolean {
  return listEffectiveProductsForCollection(runtime, collection.id).some((product) =>
    matchesResourceIdValue(product.id, rawValue),
  );
}

function matchesCollectionSearchText(
  collection: CollectionRecord | ProductCollectionRecord,
  rawValue: string,
): boolean {
  const searchableValues = [
    collection.title,
    collection.handle,
    collection.description ?? '',
    collection.descriptionHtml ? stripHtmlToDescription(collection.descriptionHtml) : '',
  ];

  return searchableValues.some((candidate) => matchesStringValue(candidate, rawValue, 'includes'));
}

function matchesPositiveCollectionQueryTerm(
  runtime: ProxyRuntimeContext,
  collection: CollectionRecord | ProductCollectionRecord,
  term: SearchQueryTerm,
): boolean {
  if (term.field === null) {
    return matchesCollectionSearchText(collection, term.value);
  }

  const field = term.field.toLowerCase();
  const value = searchQueryTermValue(term);

  switch (field) {
    case 'title':
      return matchesStringValue(collection.title, value, 'includes');
    case 'handle':
      return matchesStringValue(collection.handle, value, 'exact');
    case 'collection_type': {
      const normalizedValue = stripSearchQueryValueQuotes(value).trim().toLowerCase();
      if (normalizedValue === 'smart') {
        return collectionIsSmart(collection);
      }
      if (normalizedValue === 'custom') {
        return !collectionIsSmart(collection);
      }
      return true;
    }
    case 'id':
      return matchesResourceIdRange(collection.id, value);
    case 'product_id':
      return collectionHasProduct(runtime, collection, value);
    case 'updated_at':
      return matchesNullableProductTimestampTerm(collection.updatedAt ?? null, value);
    case 'product_publication_status':
    case 'publishable_status':
    case 'published_at':
    case 'published_status':
      return true;
    default:
      return true;
  }
}

function applyCollectionsQuery<T extends CollectionRecord | ProductCollectionRecord>(
  runtime: ProxyRuntimeContext,
  collections: T[],
  rawQuery: unknown,
): T[] {
  return applySearchQuery(collections, rawQuery, { recognizeNotKeyword: true }, (collection, term) =>
    matchesPositiveCollectionQueryTerm(runtime, collection, term),
  );
}

function compareCollectionIds(leftId: string, rightId: string): number {
  const leftTail = Number.parseInt(leftId.split('/').at(-1) ?? '', 10);
  const rightTail = Number.parseInt(rightId.split('/').at(-1) ?? '', 10);

  if (Number.isFinite(leftTail) && Number.isFinite(rightTail)) {
    return leftTail - rightTail;
  }

  return leftId.localeCompare(rightId);
}

function compareCollectionsBySortKey<T extends CollectionRecord | ProductCollectionRecord>(
  left: T,
  right: T,
  rawSortKey: unknown,
): number {
  switch (rawSortKey) {
    case 'TITLE':
      return (
        left.title.localeCompare(right.title) ||
        compareCollectionIds(left.id, right.id) ||
        left.id.localeCompare(right.id)
      );
    case 'UPDATED_AT':
      return (
        (left.updatedAt ?? '').localeCompare(right.updatedAt ?? '') ||
        compareCollectionIds(left.id, right.id) ||
        left.id.localeCompare(right.id)
      );
    case 'ID':
    default:
      return compareCollectionIds(left.id, right.id) || left.id.localeCompare(right.id);
  }
}

function sortCollections<T extends CollectionRecord | ProductCollectionRecord>(
  collections: T[],
  rawSortKey: unknown,
  rawReverse: unknown,
  rawQuery: unknown,
): T[] {
  const hasQuery = typeof rawQuery === 'string' && rawQuery.trim().length > 0;
  const effectiveSortKey = rawSortKey === 'RELEVANCE' && hasQuery ? null : rawSortKey;
  const sortedCollections =
    effectiveSortKey === null
      ? [...collections]
      : [...collections].sort((left, right) => compareCollectionsBySortKey(left, right, effectiveSortKey));

  return rawReverse === true ? sortedCollections.reverse() : sortedCollections;
}

function buildProductSearchConnectionKey(rawQuery: unknown, rawSortKey: unknown, rawReverse: unknown): string | null {
  const query = typeof rawQuery === 'string' ? rawQuery.trim() : '';
  const sortKey = typeof rawSortKey === 'string' ? rawSortKey : '';
  if (!query || sortKey !== 'RELEVANCE') {
    return null;
  }

  return JSON.stringify({
    query,
    sortKey,
    reverse: rawReverse === true,
  });
}

function buildSyntheticProductCursor(productId: string): string {
  return `cursor:${productId}`;
}

function resolveCatalogProductCursor(
  productId: string,
  catalogConnection: ProductCatalogConnectionRecord | null,
): string {
  return catalogConnection?.cursorByProductId[productId] ?? buildSyntheticProductCursor(productId);
}

function listProductsForConnection(
  runtime: ProxyRuntimeContext,
  catalogConnection: ProductCatalogConnectionRecord | null,
): ProductRecord[] {
  if (!catalogConnection) {
    return runtime.store.listEffectiveProducts();
  }

  const orderedProducts = catalogConnection.orderedProductIds
    .map((productId) => runtime.store.getEffectiveProductById(productId))
    .filter((product): product is ProductRecord => product !== null);
  const seenProductIds = new Set(orderedProducts.map((product) => product.id));
  const extraProducts = runtime.store.listEffectiveProducts().filter((product) => !seenProductIds.has(product.id));
  return [...orderedProducts, ...extraProducts];
}

function compareProductIds(leftId: string, rightId: string): number {
  const leftTail = Number.parseInt(leftId.split('/').at(-1) ?? '', 10);
  const rightTail = Number.parseInt(rightId.split('/').at(-1) ?? '', 10);

  if (Number.isFinite(leftTail) && Number.isFinite(rightTail)) {
    return leftTail - rightTail;
  }

  return leftId.localeCompare(rightId);
}

function compareProductsBySortKey(left: ProductRecord, right: ProductRecord, rawSortKey: unknown): number {
  switch (rawSortKey) {
    case 'TITLE':
      return left.title.localeCompare(right.title) || left.id.localeCompare(right.id);
    case 'UPDATED_AT':
      return left.updatedAt.localeCompare(right.updatedAt) || left.id.localeCompare(right.id);
    case 'INVENTORY_TOTAL': {
      const leftInventory = left.totalInventory ?? Number.POSITIVE_INFINITY;
      const rightInventory = right.totalInventory ?? Number.POSITIVE_INFINITY;
      return leftInventory - rightInventory || left.id.localeCompare(right.id);
    }
    case 'CREATED_AT':
      return left.createdAt.localeCompare(right.createdAt) || left.id.localeCompare(right.id);
    case 'PUBLISHED_AT':
      return (left.publishedAt ?? '').localeCompare(right.publishedAt ?? '') || left.id.localeCompare(right.id);
    case 'ID':
      return compareProductIds(left.id, right.id) || left.id.localeCompare(right.id);
    case 'HANDLE':
      return left.handle.localeCompare(right.handle) || left.id.localeCompare(right.id);
    case 'STATUS':
      return (
        left.status.localeCompare(right.status) ||
        left.title.localeCompare(right.title) ||
        left.id.localeCompare(right.id)
      );
    case 'VENDOR':
      return (
        (left.vendor ?? '').localeCompare(right.vendor ?? '') ||
        left.title.localeCompare(right.title) ||
        left.id.localeCompare(right.id)
      );
    case 'PRODUCT_TYPE':
      return (
        (left.productType ?? '').localeCompare(right.productType ?? '') ||
        left.title.localeCompare(right.title) ||
        left.id.localeCompare(right.id)
      );
    default:
      return 0;
  }
}

function compareProductsDefaultOrder(left: ProductRecord, right: ProductRecord): number {
  return right.createdAt.localeCompare(left.createdAt) || right.id.localeCompare(left.id);
}

function sortProducts(
  products: ProductRecord[],
  rawSortKey: unknown,
  rawReverse: unknown,
  options: { preserveDefaultOrder?: boolean } = {},
): ProductRecord[] {
  const sortedProducts =
    rawSortKey === undefined || rawSortKey === null
      ? options.preserveDefaultOrder === true
        ? [...products]
        : [...products].sort(compareProductsDefaultOrder)
      : [...products].sort((left, right) => compareProductsBySortKey(left, right, rawSortKey));

  return rawReverse === true ? sortedProducts.reverse() : sortedProducts;
}

function listEffectiveProductVariants(runtime: ProxyRuntimeContext): ProductVariantRecord[] {
  return runtime.store
    .listEffectiveProducts()
    .flatMap((product) => runtime.store.getEffectiveVariantsByProductId(product.id));
}

export function serializeProductVariantBulkSelection(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  return serializeVariantSelectionSet(runtime, variant, selections, variables);
}

function compareVariantIds(leftId: string, rightId: string): number {
  const leftTail = Number.parseInt(leftId.split('/').at(-1) ?? '', 10);
  const rightTail = Number.parseInt(rightId.split('/').at(-1) ?? '', 10);

  if (Number.isFinite(leftTail) && Number.isFinite(rightTail)) {
    return leftTail - rightTail;
  }

  return leftId.localeCompare(rightId);
}

function matchesProductVariantSearchText(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  rawValue: string,
): boolean {
  const product = runtime.store.getEffectiveProductById(variant.productId);
  const searchableValues = [variant.title, variant.sku ?? '', variant.barcode ?? '', product?.title ?? ''];
  return searchableValues.some((candidate) => matchesStringValue(candidate, rawValue, 'includes'));
}

function matchesPositiveProductVariantQueryTerm(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  term: SearchQueryTerm,
): boolean {
  if (term.field === null) {
    return matchesProductVariantSearchText(runtime, variant, term.value);
  }

  const field = term.field.toLowerCase();
  const value = searchQueryTermValue(term);
  const product = runtime.store.getEffectiveProductById(variant.productId);

  switch (field) {
    case 'id':
      return matchesResourceIdValue(variant.id, value);
    case 'product_id':
      return matchesResourceIdValue(variant.productId, value);
    case 'title':
      return matchesStringValue(variant.title, value, 'includes');
    case 'sku':
      return typeof variant.sku === 'string' && matchesStringValue(variant.sku, value, 'exact');
    case 'barcode':
      return typeof variant.barcode === 'string' && matchesStringValue(variant.barcode, value, 'exact');
    case 'vendor':
      return typeof product?.vendor === 'string' && matchesStringValue(product.vendor, value, 'exact');
    case 'product_type':
      return typeof product?.productType === 'string' && matchesStringValue(product.productType, value, 'exact');
    case 'tag':
      return product
        ? getSearchableProductTags(runtime, product).some((tag) => matchesStringValue(tag, value, 'exact'))
        : false;
    default:
      return true;
  }
}

function matchesProductVariantQueryTerm(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  term: SearchQueryTerm,
): boolean {
  if (!term.raw) {
    return true;
  }

  const matches = matchesPositiveProductVariantQueryTerm(runtime, variant, term);
  return term.negated ? !matches : matches;
}

function matchesProductVariantQueryNode(
  runtime: ProxyRuntimeContext,
  variant: ProductVariantRecord,
  node: SearchQueryNode,
): boolean {
  switch (node.type) {
    case 'term':
      return matchesProductVariantQueryTerm(runtime, variant, node.term);
    case 'and':
      return node.children.every((child) => matchesProductVariantQueryNode(runtime, variant, child));
    case 'or':
      return node.children.some((child) => matchesProductVariantQueryNode(runtime, variant, child));
    case 'not':
      return !matchesProductVariantQueryNode(runtime, variant, node.child);
    default:
      return true;
  }
}

function applyProductVariantsQuery(
  runtime: ProxyRuntimeContext,
  variants: ProductVariantRecord[],
  rawQuery: unknown,
): ProductVariantRecord[] {
  if (typeof rawQuery !== 'string' || !rawQuery.trim()) {
    return variants;
  }

  const parsedQuery = parseSearchQuery(rawQuery, { recognizeNotKeyword: true });
  if (!parsedQuery) {
    return variants;
  }

  return variants.filter((variant) => matchesProductVariantQueryNode(runtime, variant, parsedQuery));
}

function compareProductVariantsBySortKey(
  runtime: ProxyRuntimeContext,
  left: ProductVariantRecord,
  right: ProductVariantRecord,
  rawSortKey: unknown,
): number {
  switch (rawSortKey) {
    case 'TITLE':
      return (
        left.title.localeCompare(right.title) || compareVariantIds(left.id, right.id) || left.id.localeCompare(right.id)
      );
    case 'SKU':
      return (left.sku ?? '').localeCompare(right.sku ?? '') || compareVariantIds(left.id, right.id);
    case 'POSITION':
      return (
        left.productId.localeCompare(right.productId) ||
        runtime.store.getEffectiveVariantsByProductId(left.productId).findIndex((variant) => variant.id === left.id) -
          runtime.store
            .getEffectiveVariantsByProductId(right.productId)
            .findIndex((variant) => variant.id === right.id) ||
        compareVariantIds(left.id, right.id)
      );
    case 'INVENTORY_QUANTITY':
      return (left.inventoryQuantity ?? 0) - (right.inventoryQuantity ?? 0) || compareVariantIds(left.id, right.id);
    case 'ID':
    default:
      return compareVariantIds(left.id, right.id) || left.id.localeCompare(right.id);
  }
}

function sortProductVariants(
  runtime: ProxyRuntimeContext,
  variants: ProductVariantRecord[],
  rawSortKey: unknown,
  rawReverse: unknown,
): ProductVariantRecord[] {
  const sortedVariants = [...variants].sort((left, right) =>
    compareProductVariantsBySortKey(runtime, left, right, rawSortKey),
  );
  return rawReverse === true ? sortedVariants.reverse() : sortedVariants;
}

export function listProductVariantsForBulkExport(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): ProductVariantRecord[] {
  const args = getFieldArguments(field, variables);
  return sortProductVariants(
    runtime,
    applyProductVariantsQuery(runtime, listEffectiveProductVariants(runtime), args['query']),
    args['sortKey'],
    args['reverse'],
  );
}

export function listProductVariantsForProductBulkExport(
  runtime: ProxyRuntimeContext,
  productId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): ProductVariantRecord[] {
  const args = getFieldArguments(field, variables);
  return sortProductVariants(
    runtime,
    applyProductVariantsQuery(runtime, runtime.store.getEffectiveVariantsByProductId(productId), args['query']),
    args['sortKey'],
    args['reverse'],
  );
}

function parseProductsCursor(rawCursor: unknown): string | null {
  if (typeof rawCursor !== 'string' || !rawCursor.startsWith('cursor:')) {
    return null;
  }

  const productId = rawCursor.slice('cursor:'.length);
  return productId.length > 0 ? productId : null;
}

function serializeProductsConnection(
  runtime: ProxyRuntimeContext,
  products: ProductRecord[],
  field: FieldNode,
  first: number | null,
  last: number | null,
  rawAfter: unknown,
  rawBefore: unknown,
  rawQuery: unknown,
  rawSortKey: unknown,
  rawReverse: unknown,
  variables: Record<string, unknown>,
  options: { preserveDefaultOrder?: boolean } = {},
): Record<string, unknown> {
  const searchConnectionKey = buildProductSearchConnectionKey(rawQuery, rawSortKey, rawReverse);
  const searchConnection = searchConnectionKey
    ? runtime.store.getBaseProductSearchConnection(searchConnectionKey)
    : null;
  const candidateProducts = searchConnection ? listProductsForConnection(runtime, searchConnection) : products;
  const filteredProducts = applyProductsQuery(runtime, candidateProducts, rawQuery);
  const sortedProducts = searchConnection
    ? filteredProducts
    : sortProducts(filteredProducts, rawSortKey, rawReverse, options);
  const afterCursor = typeof rawAfter === 'string' ? rawAfter : null;
  const beforeCursor = typeof rawBefore === 'string' ? rawBefore : null;
  const afterProductId = searchConnection ? null : parseProductsCursor(rawAfter);
  const beforeProductId = searchConnection ? null : parseProductsCursor(rawBefore);
  const startIndex = searchConnection
    ? afterCursor
      ? sortedProducts.findIndex(
          (product) => resolveCatalogProductCursor(product.id, searchConnection) === afterCursor,
        ) + 1
      : 0
    : afterProductId === null
      ? 0
      : sortedProducts.findIndex((product) => product.id === afterProductId) + 1;
  const beforeIndex = searchConnection
    ? beforeCursor
      ? sortedProducts.findIndex(
          (product) => resolveCatalogProductCursor(product.id, searchConnection) === beforeCursor,
        )
      : sortedProducts.length
    : beforeProductId === null
      ? sortedProducts.length
      : sortedProducts.findIndex((product) => product.id === beforeProductId);
  const endIndex = beforeIndex >= 0 ? beforeIndex : sortedProducts.length;
  const windowStart = Math.max(0, startIndex);
  const windowEnd = Math.max(windowStart, endIndex);
  const paginatedProducts = sortedProducts.slice(windowStart, windowEnd);

  let limitedProducts = paginatedProducts;
  const preserveBaselinePageInfo = searchConnection !== null && beforeCursor === null && last === null;
  const calculatedHasNextPage =
    windowEnd < sortedProducts.length || (first !== null && paginatedProducts.length > first);

  if (first !== null) {
    limitedProducts = limitedProducts.slice(0, first);
  }

  let visibleStartIndex = windowStart;
  if (last !== null) {
    visibleStartIndex = Math.max(windowStart, windowStart + limitedProducts.length - last);
    limitedProducts = limitedProducts.slice(Math.max(0, limitedProducts.length - last));
  }

  const hasNextPage =
    calculatedHasNextPage || (preserveBaselinePageInfo && (searchConnection?.pageInfo.hasNextPage ?? false));
  const hasPreviousPage =
    visibleStartIndex > 0 || (preserveBaselinePageInfo && (searchConnection?.pageInfo.hasPreviousPage ?? false));

  return serializeConnection(field, {
    items: limitedProducts,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (product) => resolveCatalogProductCursor(product.id, searchConnection),
    serializeNode: (product, selection) =>
      serializeSelectionSet(runtime, product, selection.selectionSet?.selections ?? [], variables),
    pageInfoOptions: {
      prefixCursors: false,
      fallbackStartCursor: preserveBaselinePageInfo ? (searchConnection?.pageInfo.startCursor ?? null) : null,
      fallbackEndCursor: preserveBaselinePageInfo ? (searchConnection?.pageInfo.endCursor ?? null) : null,
    },
  });
}

export function listProductsForBulkExport(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): ProductRecord[] {
  const args = getFieldArguments(field, variables);
  const searchConnectionKey = buildProductSearchConnectionKey(args['query'], args['sortKey'], args['reverse']);
  const searchConnection = searchConnectionKey
    ? runtime.store.getBaseProductSearchConnection(searchConnectionKey)
    : null;
  const candidateProducts = searchConnection
    ? listProductsForConnection(runtime, searchConnection)
    : runtime.store.listEffectiveProducts();
  const filteredProducts = applyProductsQuery(runtime, candidateProducts, args['query']);
  return searchConnection ? filteredProducts : sortProducts(filteredProducts, args['sortKey'], args['reverse']);
}

function serializeProductVariantsConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const variants = sortProductVariants(
    runtime,
    applyProductVariantsQuery(runtime, listEffectiveProductVariants(runtime), args['query']),
    args['sortKey'],
    args['reverse'],
  );
  const {
    items: paginatedVariants,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(variants, field, variables, (variant) => variant.id);

  return serializeConnection(field, {
    items: paginatedVariants,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (variant) => variant.id,
    serializeNode: (variant, selection) =>
      serializeVariantSelectionSet(runtime, variant, selection.selectionSet?.selections ?? [], variables),
  });
}

function serializeProductVariantsCount(
  runtime: ProxyRuntimeContext,
  rawQuery: unknown,
  field: FieldNode,
): Record<string, unknown> {
  const variants = applyProductVariantsQuery(runtime, listEffectiveProductVariants(runtime), rawQuery);
  return serializeCountValue(field, variants.length);
}

function serializeStringConnection(
  values: string[],
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const sortedValues = [...new Set(values.filter((value) => value.trim().length > 0))].sort((left, right) =>
    left.localeCompare(right),
  );
  const orderedValues = args['reverse'] === true ? sortedValues.reverse() : sortedValues;
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    orderedValues,
    field,
    variables,
    (value) => value,
  );

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (value) => value,
    serializeNode: (value) => value,
  });
}

function serializeEmptySavedSearchConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems([], field, variables, () => '');
  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: () => '',
    serializeNode: () => null,
  });
}

function serializeProductOperationUserErrors(
  userErrors: ProductOperationRecord['userErrors'],
  field: FieldNode,
): Array<Record<string, unknown>> {
  return userErrors.map((userError) => {
    const result: Record<string, unknown> = {};
    for (const selection of field.selectionSet?.selections ?? []) {
      if (selection.kind !== Kind.FIELD) {
        continue;
      }

      const key = selection.alias?.value ?? selection.name.value;
      switch (selection.name.value) {
        case 'field':
          result[key] = userError.field;
          break;
        case 'message':
          result[key] = userError.message;
          break;
        default:
          result[key] = null;
      }
    }

    return result;
  });
}

function serializeProductOperationField(
  runtime: ProxyRuntimeContext,
  operation: ProductOperationRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): unknown {
  switch (field.name.value) {
    case '__typename':
      return operation.typeName;
    case 'id':
      return operation.id;
    case 'status':
      return operation.status;
    case 'product':
      return serializeProduct(
        runtime,
        operation.productId ? runtime.store.getEffectiveProductById(operation.productId) : null,
        field,
        variables,
      );
    case 'newProduct':
      return serializeProduct(
        runtime,
        operation.newProductId ? runtime.store.getEffectiveProductById(operation.newProductId) : null,
        field,
        variables,
      );
    case 'userErrors':
      return serializeProductOperationUserErrors(operation.userErrors, field);
    default:
      return null;
  }
}

function serializeProductOperation(
  runtime: ProxyRuntimeContext,
  operation: ProductOperationRecord | null,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  if (!operation) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeName = selection.typeCondition?.name.value;
      if (typeName && typeName !== operation.typeName && typeName !== 'ProductOperation' && typeName !== 'Node') {
        continue;
      }

      for (const fragmentSelection of selection.selectionSet.selections) {
        if (fragmentSelection.kind !== Kind.FIELD) {
          continue;
        }
        const key = fragmentSelection.alias?.value ?? fragmentSelection.name.value;
        result[key] = serializeProductOperationField(runtime, operation, fragmentSelection, variables);
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    result[key] = serializeProductOperationField(runtime, operation, selection, variables);
  }

  return result;
}

function serializeProductDuplicateOperation(
  runtime: ProxyRuntimeContext,
  field: FieldNode | null,
  operation: ProductOperationRecord | null,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  if (!field || !operation) {
    return null;
  }

  return serializeProductOperation(runtime, operation, field, variables);
}

function serializeProductDuplicateMutationPayload(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  payload: {
    newProduct: ProductRecord | null;
    productDuplicateOperation: ProductOperationRecord | null;
    userErrors: ProductOperationRecord['userErrors'];
  },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  const newProductField = getChildField(field, 'newProduct');
  if (newProductField) {
    result[getResponseKey(newProductField)] = serializeProduct(runtime, payload.newProduct, newProductField, variables);
  }

  const operationField = getChildField(field, 'productDuplicateOperation');
  if (operationField) {
    result[getResponseKey(operationField)] = serializeProductDuplicateOperation(
      runtime,
      operationField,
      payload.productDuplicateOperation,
      variables,
    );
  }

  const userErrorsField = getChildField(field, 'userErrors');
  if (userErrorsField) {
    result[getResponseKey(userErrorsField)] = payload.userErrors;
  }

  return result;
}

function readPlainObjectInputs(raw: unknown): Record<string, unknown>[] {
  return Array.isArray(raw) ? raw.filter((item): item is Record<string, unknown> => isObject(item)) : [];
}

function makeProductFeedRecord(runtime: ProxyRuntimeContext, input: Record<string, unknown>): ProductFeedRecord {
  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('ProductFeed'),
    country: typeof input['country'] === 'string' ? input['country'] : null,
    language: typeof input['language'] === 'string' ? input['language'] : null,
    status: 'ACTIVE',
  };
}

function makeProductResourceFeedbackRecord(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
): ProductResourceFeedbackRecord | null {
  const productId = typeof input['productId'] === 'string' ? input['productId'] : null;
  const state = input['state'];
  const feedbackGeneratedAt =
    typeof input['feedbackGeneratedAt'] === 'string'
      ? input['feedbackGeneratedAt']
      : runtime.syntheticIdentity.makeSyntheticTimestamp();
  const productUpdatedAt =
    typeof input['productUpdatedAt'] === 'string' ? input['productUpdatedAt'] : feedbackGeneratedAt;
  if (!productId || (state !== 'ACCEPTED' && state !== 'REQUIRES_ACTION')) {
    return null;
  }

  return {
    productId,
    state,
    feedbackGeneratedAt,
    productUpdatedAt,
    messages: readStringArray(input['messages']),
  };
}

function makeShopResourceFeedbackRecord(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
): ShopResourceFeedbackRecord | null {
  const state = input['state'];
  if (state !== 'ACCEPTED' && state !== 'REQUIRES_ACTION') {
    return null;
  }

  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('AppFeedback'),
    state,
    feedbackGeneratedAt:
      typeof input['feedbackGeneratedAt'] === 'string'
        ? input['feedbackGeneratedAt']
        : runtime.syntheticIdentity.makeSyntheticTimestamp(),
    messages: readStringArray(input['messages']),
  };
}

function readBundleComponentQuantityOption(raw: unknown): ProductBundleComponentQuantityOptionRecord | null {
  if (!isObject(raw) || typeof raw['name'] !== 'string') {
    return null;
  }

  return {
    name: raw['name'],
    values: readPlainObjectInputs(raw['values'])
      .map((value) => ({
        name: typeof value['name'] === 'string' ? value['name'] : null,
        quantity: typeof value['quantity'] === 'number' ? value['quantity'] : null,
      }))
      .filter((value): value is { name: string; quantity: number } => value.name !== null && value.quantity !== null),
  };
}

function readBundleComponentOptionSelections(raw: unknown): ProductBundleComponentOptionSelectionRecord[] {
  return readPlainObjectInputs(raw)
    .map((selection) => ({
      componentOptionId: typeof selection['componentOptionId'] === 'string' ? selection['componentOptionId'] : null,
      name: typeof selection['name'] === 'string' ? selection['name'] : null,
      values: readStringArray(selection['values']),
    }))
    .filter(
      (selection): selection is { componentOptionId: string; name: string; values: string[] } =>
        selection.componentOptionId !== null && selection.name !== null,
    );
}

function makeBundleComponentRecords(
  runtime: ProxyRuntimeContext,
  bundleProductId: string,
  rawComponents: unknown,
): ProductBundleComponentRecord[] {
  return readPlainObjectInputs(rawComponents).flatMap((component) => {
    const componentProductId = typeof component['productId'] === 'string' ? component['productId'] : null;
    if (!componentProductId) {
      return [];
    }

    return [
      {
        id: runtime.syntheticIdentity.makeSyntheticGid('ProductBundleComponent'),
        bundleProductId,
        componentProductId,
        quantity: typeof component['quantity'] === 'number' ? component['quantity'] : null,
        optionSelections: readBundleComponentOptionSelections(component['optionSelections']),
        quantityOption: readBundleComponentQuantityOption(component['quantityOption']),
      },
    ];
  });
}

function validateBundleComponents(
  runtime: ProxyRuntimeContext,
  rawComponents: unknown,
): Array<{ field: string[] | null; message: string }> {
  const components = readPlainObjectInputs(rawComponents);
  if (components.length === 0) {
    return [{ field: null, message: 'At least one component is required.' }];
  }

  const errors: Array<{ field: string[] | null; message: string }> = [];
  components.forEach((component, index) => {
    const productId = typeof component['productId'] === 'string' ? component['productId'] : null;
    if (!productId || !runtime.store.getEffectiveProductById(productId)) {
      errors.push({ field: ['input', 'components', String(index), 'productId'], message: 'Product does not exist' });
    }
  });
  return errors;
}

function readSelectedParentOptionValues(raw: unknown): CombinedListingChildRecord['selectedParentOptionValues'] {
  return readPlainObjectInputs(raw)
    .map((optionValue) => ({
      name: typeof optionValue['name'] === 'string' ? optionValue['name'] : null,
      value: typeof optionValue['value'] === 'string' ? optionValue['value'] : null,
      linkedMetafieldValue:
        typeof optionValue['linkedMetafieldValue'] === 'string' ? optionValue['linkedMetafieldValue'] : null,
    }))
    .filter(
      (optionValue): optionValue is { name: string; value: string; linkedMetafieldValue: string | null } =>
        optionValue.name !== null && optionValue.value !== null,
    );
}

function readCombinedListingChildInputs(raw: unknown): CombinedListingChildRecord[] {
  return readPlainObjectInputs(raw)
    .map((input) => ({
      childProductId: typeof input['childProductId'] === 'string' ? input['childProductId'] : null,
      selectedParentOptionValues: readSelectedParentOptionValues(input['selectedParentOptionValues']),
    }))
    .filter(
      (
        input,
      ): input is {
        childProductId: string;
        selectedParentOptionValues: CombinedListingChildRecord['selectedParentOptionValues'];
      } => input.childProductId !== null,
    )
    .map((input) => ({
      parentProductId: '',
      childProductId: input.childProductId,
      selectedParentOptionValues: input.selectedParentOptionValues,
    }));
}

function makeCombinedListingOptionRecords(
  runtime: ProxyRuntimeContext,
  productId: string,
  rawOptions: unknown,
): ProductOptionRecord[] {
  return readPlainObjectInputs(rawOptions).map((option, index) => ({
    id:
      typeof option['optionId'] === 'string'
        ? option['optionId']
        : runtime.syntheticIdentity.makeSyntheticGid('ProductOption'),
    productId,
    name: typeof option['name'] === 'string' ? option['name'] : `Option ${index + 1}`,
    position: index + 1,
    optionValues: readStringArray(option['values']).map((value) => ({
      id: runtime.syntheticIdentity.makeSyntheticGid('ProductOptionValue'),
      name: value,
      hasVariants: true,
    })),
  }));
}

function serializeProductDuplicateJob(rawId: unknown, field: FieldNode): Record<string, unknown> | null {
  const id = typeof rawId === 'string' ? rawId : null;
  if (!id) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'ProductDuplicateJob';
        break;
      case 'id':
        result[key] = id;
        break;
      case 'done':
        result[key] = true;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function normalizeUpstreamPublication(value: unknown, cursor?: string | null): PublicationRecord | null {
  if (!isObject(value)) {
    return null;
  }

  const rawId = value['id'];
  if (typeof rawId !== 'string') {
    return null;
  }

  return {
    id: rawId,
    name: typeof value['name'] === 'string' ? value['name'] : null,
    autoPublish: typeof value['autoPublish'] === 'boolean' ? value['autoPublish'] : undefined,
    supportsFuturePublishing:
      typeof value['supportsFuturePublishing'] === 'boolean' ? value['supportsFuturePublishing'] : undefined,
    catalogId:
      isObject(value['catalog']) && typeof value['catalog']['id'] === 'string' ? value['catalog']['id'] : undefined,
    channelId:
      isObject(value['channel']) && typeof value['channel']['id'] === 'string' ? value['channel']['id'] : undefined,
    cursor: typeof cursor === 'string' && cursor.length > 0 ? cursor : null,
  };
}

function normalizeUpstreamChannel(value: unknown, cursor?: string | null): ChannelRecord | null {
  if (!isObject(value)) {
    return null;
  }

  const rawId = value['id'];
  if (typeof rawId !== 'string') {
    return null;
  }

  return {
    id: rawId,
    name: typeof value['name'] === 'string' ? value['name'] : null,
    handle: typeof value['handle'] === 'string' ? value['handle'] : null,
    publicationId:
      isObject(value['publication']) && typeof value['publication']['id'] === 'string'
        ? value['publication']['id']
        : null,
    cursor: typeof cursor === 'string' && cursor.length > 0 ? cursor : null,
  };
}

function readPublicationRecords(value: unknown): PublicationRecord[] {
  if (!isObject(value)) {
    return [];
  }

  if (Array.isArray(value['edges'])) {
    return value['edges']
      .map((edge) => {
        if (!isObject(edge)) {
          return null;
        }

        const cursor = typeof edge['cursor'] === 'string' ? edge['cursor'] : null;
        return normalizeUpstreamPublication(edge['node'], cursor);
      })
      .filter((publication): publication is PublicationRecord => publication !== null);
  }

  if (Array.isArray(value['nodes'])) {
    return value['nodes']
      .map((publication) => normalizeUpstreamPublication(publication))
      .filter((publication): publication is PublicationRecord => publication !== null);
  }

  return [];
}

function readChannelRecords(value: unknown): ChannelRecord[] {
  if (!isObject(value)) {
    return [];
  }

  if (Array.isArray(value['edges'])) {
    return value['edges']
      .map((edge) => {
        if (!isObject(edge)) {
          return null;
        }

        const cursor = typeof edge['cursor'] === 'string' ? edge['cursor'] : null;
        return normalizeUpstreamChannel(edge['node'], cursor);
      })
      .filter((channel): channel is ChannelRecord => channel !== null);
  }

  if (Array.isArray(value['nodes'])) {
    return value['nodes']
      .map((channel) => normalizeUpstreamChannel(channel))
      .filter((channel): channel is ChannelRecord => channel !== null);
  }

  return [];
}

function normalizeUpstreamProduct(value: unknown): {
  product: ProductRecord;
  options: ProductOptionRecord[];
  hasOptions: boolean;
  variants: ProductVariantRecord[];
  hasVariants: boolean;
  collections: ProductCollectionRecord[];
  hasCollections: boolean;
  media: ProductMediaRecord[];
  hasMedia: boolean;
  hasImages: boolean;
  metafields: ProductMetafieldRecord[];
  hasMetafields: boolean;
} | null {
  if (!isObject(value)) {
    return null;
  }

  const rawId = value['id'];
  if (typeof rawId !== 'string') {
    return null;
  }

  const rawTitle = value['title'];
  const title = typeof rawTitle === 'string' ? rawTitle : 'Untitled product';
  const rawHandle = value['handle'];
  const rawStatus = value['status'];
  const rawCreatedAt = value['createdAt'];
  const rawUpdatedAt = value['updatedAt'];
  const rawPublishedAt = value['publishedAt'];
  const rawLegacyResourceId = value['legacyResourceId'];
  const rawVendor = value['vendor'];
  const rawProductType = value['productType'];
  const rawTags = value['tags'];
  const rawTotalInventory = value['totalInventory'];
  const rawTracksInventory = value['tracksInventory'];
  const rawDescriptionHtml = value['descriptionHtml'];
  const rawOnlineStorePreviewUrl = value['onlineStorePreviewUrl'];
  const rawTemplateSuffix = value['templateSuffix'];
  const rawSeo = value['seo'];
  const rawCategory = value['category'];
  const rawPublishedOnCurrentPublication = value['publishedOnCurrentPublication'];
  const rawAvailablePublicationsCount = value['availablePublicationsCount'];
  const rawResourcePublicationsCount = value['resourcePublicationsCount'];
  const contextualPricing = readCapturedJsonValue(value['contextualPricing']);
  const hasOptions = hasOwnField(value, 'options');
  const hasVariants = hasOwnField(value, 'variants');
  const hasCollections = hasOwnField(value, 'collections');
  const hasMedia = hasOwnField(value, 'media');
  const hasImages = hasOwnField(value, 'images');
  const hasMetafields =
    hasOwnField(value, 'metafields') ||
    hasOwnField(value, 'metafield') ||
    readVariantNodes(value['variants']).some(
      (variantNode) =>
        isObject(variantNode) && (hasOwnField(variantNode, 'metafields') || hasOwnField(variantNode, 'metafield')),
    );
  const options = Array.isArray(value['options'])
    ? value['options']
        .map((option) => normalizeUpstreamOption(rawId, option))
        .filter((option): option is ProductOptionRecord => option !== null)
    : [];
  const rawVariantNodes = readVariantNodes(value['variants']);
  const variants = rawVariantNodes
    .map((variant) => normalizeUpstreamVariant(rawId, variant))
    .filter((variant): variant is ProductVariantRecord => variant !== null);
  const collections = readCollectionNodes(value['collections'])
    .map((collection) => normalizeUpstreamCollection(rawId, collection))
    .filter((collection): collection is ProductCollectionRecord => collection !== null);
  const media = readMediaNodes(value['media'])
    .map((mediaNode, index) => normalizeUpstreamMedia(rawId, mediaNode, index))
    .filter((mediaRecord): mediaRecord is ProductMediaRecord => mediaRecord !== null);
  const imageMedia = readConnectionNodeEntries(value['images'])
    .map((entry) => normalizeUpstreamProductImage(rawId, entry.node, entry.position))
    .filter((mediaRecord): mediaRecord is ProductMediaRecord => mediaRecord !== null);
  const metafields = normalizeUpstreamMetafieldsForOwner(rawId, value, 'PRODUCT');
  for (const variantNode of rawVariantNodes) {
    if (!isObject(variantNode) || typeof variantNode['id'] !== 'string') {
      continue;
    }

    metafields.push(...normalizeUpstreamMetafieldsForOwner(variantNode['id'], variantNode, 'PRODUCTVARIANT'));
  }
  const publicationCount = Math.max(
    readPublicationCount(rawAvailablePublicationsCount),
    readPublicationCount(rawResourcePublicationsCount),
    rawPublishedOnCurrentPublication === true ? 1 : 0,
  );

  return {
    product: {
      id: rawId,
      legacyResourceId: typeof rawLegacyResourceId === 'string' ? rawLegacyResourceId : null,
      title,
      handle: typeof rawHandle === 'string' ? rawHandle : slugifyHandle(title),
      status: readStatus(rawStatus, 'ACTIVE'),
      publicationIds: makeUnknownPublicationIds(publicationCount),
      createdAt: typeof rawCreatedAt === 'string' ? rawCreatedAt : '1970-01-01T00:00:00.000Z',
      updatedAt: typeof rawUpdatedAt === 'string' ? rawUpdatedAt : '1970-01-01T00:00:00.000Z',
      publishedAt: rawPublishedAt === null ? null : typeof rawPublishedAt === 'string' ? rawPublishedAt : null,
      vendor: typeof rawVendor === 'string' ? rawVendor : null,
      productType: typeof rawProductType === 'string' ? rawProductType : null,
      tags: Array.isArray(rawTags) ? rawTags.filter((tag): tag is string => typeof tag === 'string') : [],
      totalInventory: typeof rawTotalInventory === 'number' ? rawTotalInventory : null,
      tracksInventory: typeof rawTracksInventory === 'boolean' ? rawTracksInventory : null,
      descriptionHtml: typeof rawDescriptionHtml === 'string' ? rawDescriptionHtml : null,
      onlineStorePreviewUrl: typeof rawOnlineStorePreviewUrl === 'string' ? rawOnlineStorePreviewUrl : null,
      templateSuffix: typeof rawTemplateSuffix === 'string' ? rawTemplateSuffix : null,
      seo: isObject(rawSeo)
        ? {
            title: typeof rawSeo['title'] === 'string' ? rawSeo['title'] : null,
            description: typeof rawSeo['description'] === 'string' ? rawSeo['description'] : null,
          }
        : { title: null, description: null },
      category:
        isObject(rawCategory) && typeof rawCategory['id'] === 'string'
          ? {
              id: rawCategory['id'],
              fullName: typeof rawCategory['fullName'] === 'string' ? rawCategory['fullName'] : null,
            }
          : null,
      ...(contextualPricing === undefined ? {} : { contextualPricing }),
    },
    options,
    hasOptions,
    variants,
    hasVariants,
    collections,
    hasCollections,
    media: hasMedia ? media : imageMedia,
    hasMedia,
    hasImages,
    metafields,
    hasMetafields,
  };
}

export function hydrateProductsFromUpstreamResponse(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
  responseBody: unknown,
): void {
  if (!isObject(responseBody)) {
    return;
  }

  const rawData = responseBody['data'];
  if (!isObject(rawData)) {
    return;
  }

  const productSearchConnections = collectProductSearchConnections(document, variables, rawData);
  for (const [key, connection] of Object.entries(productSearchConnections)) {
    runtime.store.setBaseProductSearchConnection(key, connection);
  }

  const publications = readPublicationRecords(rawData['publications']);
  if (publications.length > 0) {
    runtime.store.upsertBasePublications(publications);
  }

  const maybePublication = normalizeUpstreamPublication(rawData['publication']);
  if (maybePublication) {
    runtime.store.upsertBasePublications([maybePublication]);
  }

  const channels = readChannelRecords(rawData['channels']);
  if (channels.length > 0) {
    runtime.store.upsertBaseChannels(channels);
  }

  const maybeChannel = normalizeUpstreamChannel(rawData['channel']);
  if (maybeChannel) {
    runtime.store.upsertBaseChannels([maybeChannel]);
  }

  const maybeProduct = normalizeUpstreamProduct(rawData['product']);
  if (maybeProduct) {
    runtime.store.upsertBaseProducts([maybeProduct.product]);
    if (maybeProduct.hasOptions) {
      runtime.store.replaceBaseOptionsForProduct(maybeProduct.product.id, maybeProduct.options);
    }
    if (maybeProduct.hasVariants) {
      runtime.store.replaceBaseVariantsForProduct(maybeProduct.product.id, maybeProduct.variants);
    }
    if (maybeProduct.hasCollections) {
      runtime.store.replaceBaseCollectionsForProduct(maybeProduct.product.id, maybeProduct.collections);
    }
    if (maybeProduct.hasMedia || maybeProduct.hasImages) {
      runtime.store.replaceBaseMediaForProduct(maybeProduct.product.id, maybeProduct.media);
    }
    if (maybeProduct.hasMetafields) {
      replaceBaseMetafieldsForHydratedProduct(runtime, maybeProduct.product.id, maybeProduct.metafields);
    }
  }

  const hydrateProductValue = (value: unknown): void => {
    const normalized = normalizeUpstreamProduct(value);
    if (!normalized) {
      return;
    }

    runtime.store.upsertBaseProducts([normalized.product]);
    if (normalized.hasOptions) {
      runtime.store.replaceBaseOptionsForProduct(normalized.product.id, normalized.options);
    }
    if (normalized.hasVariants) {
      runtime.store.replaceBaseVariantsForProduct(normalized.product.id, normalized.variants);
    }
    if (normalized.hasCollections) {
      runtime.store.replaceBaseCollectionsForProduct(normalized.product.id, normalized.collections);
    }
    if (normalized.hasMedia || normalized.hasImages) {
      runtime.store.replaceBaseMediaForProduct(normalized.product.id, normalized.media);
    }
    if (normalized.hasMetafields) {
      replaceBaseMetafieldsForHydratedProduct(runtime, normalized.product.id, normalized.metafields);
    }
  };

  const hydratedTopLevelVariantsByProductId = new Map<string, ProductVariantRecord[]>();
  const hydrateProductVariantValue = (value: unknown): void => {
    if (!isObject(value)) {
      return;
    }

    const rawProduct = value['product'];
    const productId = isObject(rawProduct) && typeof rawProduct['id'] === 'string' ? rawProduct['id'] : null;
    if (!productId) {
      return;
    }

    hydrateProductValue(rawProduct);
    const variant = normalizeUpstreamVariant(productId, value);
    if (!variant) {
      return;
    }

    const variants =
      hydratedTopLevelVariantsByProductId.get(productId) ?? runtime.store.getBaseVariantsByProductId(productId);
    hydratedTopLevelVariantsByProductId.set(productId, [
      ...variants.filter((candidate) => candidate.id !== variant.id),
      variant,
    ]);
  };

  for (const field of getRootFields(document)) {
    const responseKey = field.alias?.value ?? field.name.value;
    if (field.name.value === 'productByIdentifier') {
      hydrateProductValue(rawData[responseKey]);
    }
    if (field.name.value === 'productVariantByIdentifier') {
      hydrateProductVariantValue(rawData[responseKey]);
    }
    if (field.name.value === 'productVariants') {
      for (const variant of readVariantNodes(rawData[responseKey])) {
        hydrateProductVariantValue(variant);
      }
    }
  }

  for (const [productId, variants] of hydratedTopLevelVariantsByProductId) {
    runtime.store.replaceBaseVariantsForProduct(productId, variants);
  }

  const hydrateCollection = (value: unknown): void => {
    const collection = normalizeUpstreamCollectionRecord(value);
    if (!collection || !isObject(value)) {
      return;
    }

    runtime.store.upsertBaseCollections([collection]);
    if (hasOwnField(value, 'metafields') || hasOwnField(value, 'metafield')) {
      runtime.store.replaceBaseMetafieldsForOwner(
        collection.id,
        normalizeUpstreamMetafieldsForOwner(collection.id, value, 'COLLECTION'),
      );
    }

    const productEntries = readConnectionNodeEntries(value['products']);
    for (const productEntry of productEntries) {
      const normalizedProduct = normalizeUpstreamProduct(productEntry.node);
      if (!normalizedProduct) {
        continue;
      }

      runtime.store.upsertBaseProducts([normalizedProduct.product]);
      if (normalizedProduct.hasOptions) {
        runtime.store.replaceBaseOptionsForProduct(normalizedProduct.product.id, normalizedProduct.options);
      }
      if (normalizedProduct.hasVariants) {
        runtime.store.replaceBaseVariantsForProduct(normalizedProduct.product.id, normalizedProduct.variants);
      }
      if (normalizedProduct.hasMedia || normalizedProduct.hasImages) {
        runtime.store.replaceBaseMediaForProduct(normalizedProduct.product.id, normalizedProduct.media);
      }
      if (normalizedProduct.hasMetafields) {
        replaceBaseMetafieldsForHydratedProduct(runtime, normalizedProduct.product.id, normalizedProduct.metafields);
      }

      const nextCollections = [
        ...runtime.store
          .getEffectiveCollectionsByProductId(normalizedProduct.product.id)
          .filter((candidate) => candidate.id !== collection.id),
        {
          ...collection,
          productId: normalizedProduct.product.id,
          position: productEntry.position,
        },
      ];
      runtime.store.replaceBaseCollectionsForProduct(normalizedProduct.product.id, nextCollections);
    }
  };

  hydrateCollection(rawData['collection']);
  hydrateCollection(rawData['collectionByIdentifier']);
  hydrateCollection(rawData['collectionByHandle']);
  for (const collection of readCollectionNodes(rawData['collections'])) {
    hydrateCollection(collection);
  }

  for (const field of getRootFields(document)) {
    if (
      field.name.value !== 'collection' &&
      field.name.value !== 'collectionByIdentifier' &&
      field.name.value !== 'collectionByHandle'
    ) {
      continue;
    }

    const responseKey = field.alias?.value ?? field.name.value;
    hydrateCollection(rawData[responseKey]);
  }

  const rawProducts = rawData['products'];
  const rawProductNodes = readProductNodes(rawProducts);
  if (rawProductNodes.length > 0) {
    const products = rawProductNodes
      .map((product) => normalizeUpstreamProduct(product))
      .filter(
        (
          product,
        ): product is {
          product: ProductRecord;
          options: ProductOptionRecord[];
          hasOptions: boolean;
          variants: ProductVariantRecord[];
          hasVariants: boolean;
          collections: ProductCollectionRecord[];
          hasCollections: boolean;
          media: ProductMediaRecord[];
          hasMedia: boolean;
          hasImages: boolean;
          metafields: ProductMetafieldRecord[];
          hasMetafields: boolean;
        } => product !== null,
      );

    runtime.store.upsertBaseProducts(products.map((entry) => entry.product));
    for (const entry of products) {
      if (entry.hasOptions) {
        runtime.store.replaceBaseOptionsForProduct(entry.product.id, entry.options);
      }
      if (entry.hasVariants) {
        runtime.store.replaceBaseVariantsForProduct(entry.product.id, entry.variants);
      }
      if (entry.hasCollections) {
        runtime.store.replaceBaseCollectionsForProduct(entry.product.id, entry.collections);
      }
      if (entry.hasMedia || entry.hasImages) {
        runtime.store.replaceBaseMediaForProduct(entry.product.id, entry.media);
      }
      if (entry.hasMetafields) {
        replaceBaseMetafieldsForHydratedProduct(runtime, entry.product.id, entry.metafields);
      }
    }
  }

  for (const field of getRootFields(document)) {
    if (field.name.value !== 'products') {
      continue;
    }

    const responseKey = field.alias?.value ?? field.name.value;
    const productNodes = readProductNodes(rawData[responseKey]);
    if (productNodes.length === 0) {
      continue;
    }

    const products = productNodes
      .map((product) => normalizeUpstreamProduct(product))
      .filter(
        (
          product,
        ): product is {
          product: ProductRecord;
          options: ProductOptionRecord[];
          hasOptions: boolean;
          variants: ProductVariantRecord[];
          hasVariants: boolean;
          collections: ProductCollectionRecord[];
          hasCollections: boolean;
          media: ProductMediaRecord[];
          hasMedia: boolean;
          hasImages: boolean;
          metafields: ProductMetafieldRecord[];
          hasMetafields: boolean;
        } => product !== null,
      );

    runtime.store.upsertBaseProducts(products.map((entry) => entry.product));
    for (const entry of products) {
      if (entry.hasOptions) {
        runtime.store.replaceBaseOptionsForProduct(entry.product.id, entry.options);
      }
      if (entry.hasVariants) {
        runtime.store.replaceBaseVariantsForProduct(entry.product.id, entry.variants);
      }
      if (entry.hasCollections) {
        runtime.store.replaceBaseCollectionsForProduct(entry.product.id, entry.collections);
      }
      if (entry.hasMedia || entry.hasImages) {
        runtime.store.replaceBaseMediaForProduct(entry.product.id, entry.media);
      }
      if (entry.hasMetafields) {
        replaceBaseMetafieldsForHydratedProduct(runtime, entry.product.id, entry.metafields);
      }
    }
  }
}

function readStringInput(input: Record<string, unknown>, key: string, fallback: string | null = null): string | null {
  const value = input[key];
  return typeof value === 'string' ? value : fallback;
}

function readNumberInput(input: Record<string, unknown>, key: string, fallback: number | null = null): number | null {
  const value = input[key];
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

function sellingPlanPolicyValue(input: Record<string, unknown>): Record<string, unknown> {
  const fixedValue = input['fixedValue'];
  if (typeof fixedValue === 'string' || typeof fixedValue === 'number') {
    return {
      __typename: 'SellingPlanPricingPolicyFixedValue',
      fixedValue,
    };
  }

  return {
    __typename: 'SellingPlanPricingPolicyPercentageValue',
    percentage: typeof input['percentage'] === 'number' ? input['percentage'] : null,
  };
}

function sellingPlanBillingPolicy(input: Record<string, unknown>, existing: unknown): Record<string, unknown> {
  const recurring = readProductInput(input['recurring']);
  if (Object.keys(recurring).length > 0) {
    return {
      __typename: 'SellingPlanRecurringBillingPolicy',
      interval: readStringInput(recurring, 'interval'),
      intervalCount: readNumberInput(recurring, 'intervalCount'),
      minCycles: readNumberInput(recurring, 'minCycles'),
      maxCycles: readNumberInput(recurring, 'maxCycles'),
    };
  }

  const fixed = readProductInput(input['fixed']);
  if (Object.keys(fixed).length > 0) {
    return {
      __typename: 'SellingPlanFixedBillingPolicy',
      checkoutCharge: readProductInput(fixed['checkoutCharge']),
      remainingBalanceChargeTrigger: readStringInput(fixed, 'remainingBalanceChargeTrigger'),
      remainingBalanceChargeExactTime: readStringInput(fixed, 'remainingBalanceChargeExactTime'),
      remainingBalanceChargeTimeAfterCheckout: readStringInput(fixed, 'remainingBalanceChargeTimeAfterCheckout'),
    };
  }

  return isObject(existing) ? structuredClone(existing) : { __typename: 'SellingPlanRecurringBillingPolicy' };
}

function sellingPlanDeliveryPolicy(input: Record<string, unknown>, existing: unknown): Record<string, unknown> {
  const recurring = readProductInput(input['recurring']);
  if (Object.keys(recurring).length > 0) {
    return {
      __typename: 'SellingPlanRecurringDeliveryPolicy',
      interval: readStringInput(recurring, 'interval'),
      intervalCount: readNumberInput(recurring, 'intervalCount'),
      cutoff: readNumberInput(recurring, 'cutoff'),
      intent: readStringInput(recurring, 'intent', 'FULFILLMENT_BEGIN'),
      preAnchorBehavior: readStringInput(recurring, 'preAnchorBehavior', 'ASAP'),
    };
  }

  const fixed = readProductInput(input['fixed']);
  if (Object.keys(fixed).length > 0) {
    return {
      __typename: 'SellingPlanFixedDeliveryPolicy',
      cutoff: readNumberInput(fixed, 'cutoff'),
      fulfillmentTrigger: readStringInput(fixed, 'fulfillmentTrigger'),
      fulfillmentExactTime: readStringInput(fixed, 'fulfillmentExactTime'),
      intent: readStringInput(fixed, 'intent'),
      preAnchorBehavior: readStringInput(fixed, 'preAnchorBehavior'),
    };
  }

  return isObject(existing) ? structuredClone(existing) : { __typename: 'SellingPlanRecurringDeliveryPolicy' };
}

function sellingPlanPricingPolicies(input: unknown): Array<Record<string, unknown>> {
  return readPlainObjectArray(input).map((policyInput) => {
    const fixed = readProductInput(policyInput['fixed']);
    if (Object.keys(fixed).length > 0) {
      return {
        __typename: 'SellingPlanFixedPricingPolicy',
        adjustmentType: readStringInput(fixed, 'adjustmentType'),
        adjustmentValue: sellingPlanPolicyValue(readProductInput(fixed['adjustmentValue'])),
      };
    }

    const recurring = readProductInput(policyInput['recurring']);
    return {
      __typename: 'SellingPlanRecurringPricingPolicy',
      adjustmentType: readStringInput(recurring, 'adjustmentType'),
      adjustmentValue: sellingPlanPolicyValue(readProductInput(recurring['adjustmentValue'])),
      afterCycle: readNumberInput(recurring, 'afterCycle'),
    };
  });
}

function makeSellingPlanRecord(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing?: SellingPlanRecord,
): SellingPlanRecord {
  const previous = existing?.data ?? {};
  const id = readStringInput(
    input,
    'id',
    existing?.id ?? runtime.syntheticIdentity.makeProxySyntheticGid('SellingPlan'),
  )!;
  const data: Record<string, unknown> = {
    ...structuredClone(previous),
    __typename: 'SellingPlan',
    id,
    name: readStringInput(input, 'name', typeof previous['name'] === 'string' ? previous['name'] : 'Selling plan'),
    description: readStringInput(
      input,
      'description',
      typeof previous['description'] === 'string' ? previous['description'] : null,
    ),
    options: readStringArray(input['options'] ?? previous['options']),
    position: readNumberInput(
      input,
      'position',
      typeof previous['position'] === 'number' ? previous['position'] : null,
    ),
    category: readStringInput(
      input,
      'category',
      typeof previous['category'] === 'string' ? previous['category'] : null,
    ),
    createdAt:
      typeof previous['createdAt'] === 'string'
        ? previous['createdAt']
        : runtime.syntheticIdentity.makeSyntheticTimestamp(),
    billingPolicy: sellingPlanBillingPolicy(readProductInput(input['billingPolicy']), previous['billingPolicy']),
    deliveryPolicy: sellingPlanDeliveryPolicy(readProductInput(input['deliveryPolicy']), previous['deliveryPolicy']),
    inventoryPolicy: {
      reserve:
        readStringInput(readProductInput(input['inventoryPolicy']), 'reserve') ??
        (isObject(previous['inventoryPolicy']) && typeof previous['inventoryPolicy']['reserve'] === 'string'
          ? previous['inventoryPolicy']['reserve']
          : null),
    },
    pricingPolicies: Object.prototype.hasOwnProperty.call(input, 'pricingPolicies')
      ? sellingPlanPricingPolicies(input['pricingPolicies'])
      : existing
        ? []
        : [],
  };

  return {
    id,
    data: data as SellingPlanRecord['data'],
  };
}

function summarizeSellingPlanGroup(plans: SellingPlanRecord[]): string | null {
  const policies: unknown[] = plans.flatMap((plan) =>
    Array.isArray(plan.data['pricingPolicies']) ? plan.data['pricingPolicies'] : [],
  );
  const firstPolicy = policies.find((policy): policy is Record<string, unknown> => isObject(policy));
  const adjustmentValue = isObject(firstPolicy?.['adjustmentValue']) ? firstPolicy['adjustmentValue'] : null;
  const percentage = typeof adjustmentValue?.['percentage'] === 'number' ? `${adjustmentValue['percentage']}%` : '';
  return `${plans.length} delivery frequency, ${percentage} discount`;
}

function applySellingPlanGroupInput(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing?: SellingPlanGroupRecord,
  resources: Record<string, unknown> = {},
): SellingPlanGroupRecord {
  const currentPlans = existing?.sellingPlans ?? [];
  const plansById = new Map(currentPlans.map((plan) => [plan.id, plan]));
  const nextPlans = [...currentPlans];

  for (const createInput of readPlainObjectArray(input['sellingPlansToCreate'])) {
    nextPlans.push(makeSellingPlanRecord(runtime, createInput));
  }

  for (const updateInput of readPlainObjectArray(input['sellingPlansToUpdate'])) {
    const planId = readStringInput(updateInput, 'id');
    const existingPlan = planId ? plansById.get(planId) : undefined;
    if (!planId || !existingPlan) {
      continue;
    }

    const index = nextPlans.findIndex((plan) => plan.id === planId);
    nextPlans[index] = makeSellingPlanRecord(runtime, updateInput, existingPlan);
  }

  const deletedPlanIds = new Set(readStringArray(input['sellingPlansToDelete']));
  const filteredPlans = nextPlans.filter((plan) => !deletedPlanIds.has(plan.id));
  const createdAt = existing?.createdAt ?? runtime.syntheticIdentity.makeSyntheticTimestamp();
  const productIds = uniqueStrings([...(existing?.productIds ?? []), ...readStringArray(resources['productIds'])]);
  const productVariantIds = uniqueStrings([
    ...(existing?.productVariantIds ?? []),
    ...readStringArray(resources['productVariantIds']),
  ]);

  const group: SellingPlanGroupRecord = {
    id: existing?.id ?? runtime.syntheticIdentity.makeProxySyntheticGid('SellingPlanGroup'),
    appId: readStringInput(input, 'appId', existing?.appId ?? null),
    name: readStringInput(input, 'name', existing?.name ?? 'Selling plan group')!,
    merchantCode: readStringInput(input, 'merchantCode', existing?.merchantCode ?? 'selling-plan-group')!,
    description: readStringInput(input, 'description', existing?.description ?? null),
    options: readStringArray(input['options'] ?? existing?.options ?? []),
    position: readNumberInput(input, 'position', existing?.position ?? null),
    summary: null,
    createdAt,
    productIds,
    productVariantIds,
    sellingPlans: filteredPlans,
  };
  group.summary = summarizeSellingPlanGroup(group.sellingPlans);
  return group;
}

function serializeSellingPlanGroupMutationPayload(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  payload: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'sellingPlanGroup':
        result[key] = serializeSellingPlanGroup(
          runtime,
          (payload['sellingPlanGroup'] as SellingPlanGroupRecord | null | undefined) ?? null,
          selection,
          variables,
        );
        break;
      case 'userErrors':
        result[key] = serializeSellingPlanGroupUserErrors(
          (payload['userErrors'] as SellingPlanGroupUserError[] | undefined) ?? [],
          selection,
        );
        break;
      case 'deletedSellingPlanGroupId':
      case 'deletedSellingPlanIds':
      case 'removedProductIds':
      case 'removedProductVariantIds':
        result[key] = payload[selection.name.value] ?? null;
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeProductSellingPlanGroupMutationPayload(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  payload: {
    product?: ProductRecord | null;
    productVariant?: ProductVariantRecord | null;
    userErrors: SellingPlanGroupUserError[];
  },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'product':
        result[key] = serializeProduct(runtime, payload.product ?? null, selection, variables);
        break;
      case 'productVariant':
        result[key] = payload.productVariant
          ? serializeVariantSelectionSet(
              runtime,
              payload.productVariant,
              selection.selectionSet?.selections ?? [],
              variables,
            )
          : null;
        break;
      case 'userErrors':
        result[key] = serializeSellingPlanGroupUserErrors(payload.userErrors, selection);
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeVariantMediaMutationPayload(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  payload: {
    product: ProductRecord | null;
    productVariants: ProductVariantRecord[];
    userErrors: Array<{ field: string[]; message: string }>;
  },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'product':
        result[key] = serializeProduct(runtime, payload.product, selection, variables);
        break;
      case 'productVariants':
        result[key] = serializeVariantPayload(runtime, payload.productVariants, selection);
        break;
      case 'userErrors':
        result[key] = payload.userErrors.map((error) => {
          const errorResult: Record<string, unknown> = {};
          for (const errorSelection of selection.selectionSet?.selections ?? []) {
            if (errorSelection.kind !== Kind.FIELD) {
              continue;
            }

            const errorKey = errorSelection.alias?.value ?? errorSelection.name.value;
            switch (errorSelection.name.value) {
              case 'field':
                errorResult[errorKey] = error.field;
                break;
              case 'message':
                errorResult[errorKey] = error.message;
                break;
              default:
                errorResult[errorKey] = null;
            }
          }
          return errorResult;
        });
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

export function handleProductMutation(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
  readMode: ReadMode,
  apiVersion: string | null = DEFAULT_ADMIN_API_VERSION,
): Record<string, unknown> {
  const field = getRootField(document);
  const args = getRootFieldArguments(document, variables);
  const responseKey = field.alias?.value ?? field.name.value;
  const usesInventoryQuantity202604Contract = adminApiVersionAtLeast(apiVersion, '2026-04');

  switch (field.name.value) {
    case 'productFeedCreate': {
      const input = readProductInput(args['input']);
      const productFeed = runtime.store.upsertStagedProductFeed(makeProductFeedRecord(runtime, input));
      return {
        data: {
          [responseKey]: {
            productFeed: serializeProductFeedSelectionSet(
              productFeed,
              getChildField(field, 'productFeed')?.selectionSet?.selections ?? [],
            ),
            userErrors: serializeUserErrorLikeList([], getChildField(field, 'userErrors')),
          },
        },
      };
    }
    case 'productFeedDelete': {
      const productFeedId = typeof args['id'] === 'string' ? args['id'] : null;
      const existing = productFeedId ? runtime.store.getEffectiveProductFeedById(productFeedId) : null;
      if (!productFeedId || !existing) {
        return {
          data: {
            [responseKey]: {
              deletedId: null,
              userErrors: serializeUserErrorLikeList(
                [{ field: ['id'], message: 'ProductFeed does not exist', code: null }],
                getChildField(field, 'userErrors'),
              ),
            },
          },
        };
      }

      runtime.store.deleteStagedProductFeed(productFeedId);
      return {
        data: {
          [responseKey]: {
            deletedId: productFeedId,
            userErrors: serializeUserErrorLikeList([], getChildField(field, 'userErrors')),
          },
        },
      };
    }
    case 'productFullSync': {
      const productFeedId = typeof args['id'] === 'string' ? args['id'] : null;
      const existing = productFeedId ? runtime.store.getEffectiveProductFeedById(productFeedId) : null;
      return {
        data: {
          [responseKey]: {
            id: existing ? productFeedId : null,
            userErrors: serializeUserErrorLikeList(
              existing ? [] : [{ field: ['id'], message: 'ProductFeed does not exist', code: null }],
              getChildField(field, 'userErrors'),
            ),
          },
        },
      };
    }
    case 'bulkProductResourceFeedbackCreate': {
      const feedbackInput = readPlainObjectInputs(args['feedbackInput']);
      const feedback: ProductResourceFeedbackRecord[] = [];
      const userErrors: Array<{ field: string[] | null; message: string; code?: string | null }> = [];
      feedbackInput.forEach((input, index) => {
        const record = makeProductResourceFeedbackRecord(runtime, input);
        if (!record || !runtime.store.getEffectiveProductById(record.productId)) {
          userErrors.push({
            field: ['feedbackInput', String(index), 'productId'],
            message: 'Product does not exist',
            code: null,
          });
          return;
        }

        feedback.push(runtime.store.upsertStagedProductResourceFeedback(record));
      });

      return {
        data: {
          [responseKey]: {
            feedback: feedback.map((record) =>
              serializeProductResourceFeedbackSelectionSet(
                record,
                getChildField(field, 'feedback')?.selectionSet?.selections ?? [],
              ),
            ),
            userErrors: serializeUserErrorLikeList(userErrors, getChildField(field, 'userErrors')),
          },
        },
      };
    }
    case 'shopResourceFeedbackCreate': {
      const feedback = makeShopResourceFeedbackRecord(runtime, readProductInput(args['input']));
      if (!feedback) {
        return {
          data: {
            [responseKey]: {
              feedback: null,
              userErrors: serializeUserErrorLikeList(
                [{ field: ['input', 'state'], message: 'State is invalid', code: null }],
                getChildField(field, 'userErrors'),
              ),
            },
          },
        };
      }

      const stagedFeedback = runtime.store.upsertStagedShopResourceFeedback(feedback);
      return {
        data: {
          [responseKey]: {
            feedback: serializeAppFeedbackSelectionSet(
              stagedFeedback,
              getChildField(field, 'feedback')?.selectionSet?.selections ?? [],
            ),
            userErrors: serializeUserErrorLikeList([], getChildField(field, 'userErrors')),
          },
        },
      };
    }
    case 'productBundleCreate': {
      const input = readProductInput(args['input']);
      const userErrors = validateBundleComponents(runtime, input['components']);
      if (userErrors.length > 0) {
        return {
          data: {
            [responseKey]: {
              productBundleOperation: null,
              userErrors: serializeUserErrorLikeList(userErrors, getChildField(field, 'userErrors')),
            },
          },
        };
      }

      const product = runtime.store.stageCreateProduct(
        makeProductRecord(runtime, { title: input['title'], status: 'ACTIVE' }),
      );
      runtime.store.replaceStagedOptionsForProduct(product.id, [makeDefaultOptionRecord(runtime, product)]);
      runtime.store.replaceStagedVariantsForProduct(product.id, [makeDefaultVariantRecord(runtime, product)]);
      runtime.store.replaceStagedBundleComponentsForProduct(
        product.id,
        makeBundleComponentRecords(runtime, product.id, input['components']),
      );
      const operation = runtime.store.stageProductOperation({
        id: runtime.syntheticIdentity.makeSyntheticGid('ProductBundleOperation'),
        typeName: 'ProductBundleOperation',
        productId: product.id,
        status: 'CREATED',
        userErrors: [],
      });
      return {
        data: {
          [responseKey]: {
            productBundleOperation: serializeProductOperation(
              runtime,
              operation,
              getChildField(field, 'productBundleOperation') ?? field,
              variables,
            ),
            userErrors: serializeUserErrorLikeList([], getChildField(field, 'userErrors')),
          },
        },
      };
    }
    case 'productBundleUpdate': {
      const input = readProductInput(args['input']);
      const productId = typeof input['productId'] === 'string' ? input['productId'] : null;
      const existing = productId ? runtime.store.getEffectiveProductById(productId) : null;
      if (!productId || !existing) {
        return {
          data: {
            [responseKey]: {
              productBundleOperation: null,
              userErrors: serializeUserErrorLikeList(
                [{ field: null, message: 'Product does not exist' }],
                getChildField(field, 'userErrors'),
              ),
            },
          },
        };
      }

      const userErrors = hasOwnField(input, 'components') ? validateBundleComponents(runtime, input['components']) : [];
      if (userErrors.length > 0) {
        return {
          data: {
            [responseKey]: {
              productBundleOperation: null,
              userErrors: serializeUserErrorLikeList(userErrors, getChildField(field, 'userErrors')),
            },
          },
        };
      }

      if (typeof input['title'] === 'string') {
        runtime.store.stageUpdateProduct(
          makeProductRecord(runtime, { id: productId, title: input['title'] }, existing),
        );
      }
      if (hasOwnField(input, 'components')) {
        runtime.store.replaceStagedBundleComponentsForProduct(
          productId,
          makeBundleComponentRecords(runtime, productId, input['components']),
        );
      }
      const operation = runtime.store.stageProductOperation({
        id: runtime.syntheticIdentity.makeSyntheticGid('ProductBundleOperation'),
        typeName: 'ProductBundleOperation',
        productId,
        status: 'CREATED',
        userErrors: [],
      });
      return {
        data: {
          [responseKey]: {
            productBundleOperation: serializeProductOperation(
              runtime,
              operation,
              getChildField(field, 'productBundleOperation') ?? field,
              variables,
            ),
            userErrors: serializeUserErrorLikeList([], getChildField(field, 'userErrors')),
          },
        },
      };
    }
    case 'productVariantRelationshipBulkUpdate': {
      const inputs = readPlainObjectInputs(args['input']);
      const parentVariants: ProductVariantRecord[] = [];
      const userErrors: Array<{ field: string[] | null; message: string; code?: string | null }> = [];
      inputs.forEach((input, index) => {
        const parentVariantId =
          typeof input['parentProductVariantId'] === 'string'
            ? input['parentProductVariantId']
            : typeof input['parentProductId'] === 'string'
              ? (runtime.store.getEffectiveVariantsByProductId(input['parentProductId'])[0]?.id ?? null)
              : null;
        const parentVariant = parentVariantId ? runtime.store.getEffectiveVariantById(parentVariantId) : null;
        if (!parentVariant) {
          const missingIds = [
            parentVariantId,
            ...[
              ...readPlainObjectInputs(input['productVariantRelationshipsToCreate']),
              ...readPlainObjectInputs(input['productVariantRelationshipsToUpdate']),
            ].flatMap((relationship) => (typeof relationship['id'] === 'string' ? [relationship['id']] : [])),
          ].filter((id): id is string => typeof id === 'string');
          userErrors.push({
            field: ['input'],
            message:
              missingIds.length > 0
                ? `The product variants with ID(s) ${JSON.stringify(missingIds)} could not be found.`
                : 'Parent product variant does not exist',
            code: missingIds.length > 0 ? 'PRODUCT_VARIANTS_NOT_FOUND' : null,
          });
          return;
        }

        let nextComponents =
          input['removeAllProductVariantRelationships'] === true
            ? []
            : runtime.store.getEffectiveVariantComponentsByParentVariantId(parentVariant.id);
        const removeIds = new Set(readStringArray(input['productVariantRelationshipsToRemove']));
        nextComponents = nextComponents.filter((component) => !removeIds.has(component.componentProductVariantId));
        const upsertInputs = [
          ...readPlainObjectInputs(input['productVariantRelationshipsToCreate']),
          ...readPlainObjectInputs(input['productVariantRelationshipsToUpdate']),
        ];
        for (const relationship of upsertInputs) {
          const componentVariantId = typeof relationship['id'] === 'string' ? relationship['id'] : null;
          const quantity = typeof relationship['quantity'] === 'number' ? relationship['quantity'] : 1;
          if (!componentVariantId || !runtime.store.getEffectiveVariantById(componentVariantId)) {
            userErrors.push({
              field: ['input', String(index), 'productVariantRelationshipsToCreate', 'id'],
              message: 'Product variant does not exist',
              code: null,
            });
            continue;
          }
          nextComponents = nextComponents.filter(
            (component) => component.componentProductVariantId !== componentVariantId,
          );
          nextComponents.push({
            id: runtime.syntheticIdentity.makeSyntheticGid('ProductVariantComponent'),
            parentProductVariantId: parentVariant.id,
            componentProductVariantId: componentVariantId,
            quantity,
          });
        }

        if (userErrors.length === 0) {
          runtime.store.replaceStagedVariantComponentsForParentVariant(parentVariant.id, nextComponents);
          parentVariants.push(parentVariant);
        }
      });

      return {
        data: {
          [responseKey]: {
            parentProductVariants:
              userErrors.length === 0
                ? parentVariants.map((variant) =>
                    serializeVariantSelectionSet(
                      runtime,
                      variant,
                      getChildField(field, 'parentProductVariants')?.selectionSet?.selections ?? [],
                      variables,
                    ),
                  )
                : null,
            userErrors: serializeUserErrorLikeList(userErrors, getChildField(field, 'userErrors')),
          },
        },
      };
    }
    case 'combinedListingUpdate': {
      const parentProductId = typeof args['parentProductId'] === 'string' ? args['parentProductId'] : null;
      const parentProduct = parentProductId ? runtime.store.getEffectiveProductById(parentProductId) : null;
      if (!parentProductId || !parentProduct) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: serializeUserErrorLikeList(
                [{ field: ['parentProductId'], message: 'Product does not exist', code: 'PARENT_PRODUCT_NOT_FOUND' }],
                getChildField(field, 'userErrors'),
              ),
            },
          },
        };
      }

      let children = runtime.store.getEffectiveCombinedListingChildrenByParentId(parentProductId);
      const removedIds = new Set(readStringArray(args['productsRemovedIds']));
      children = children.filter((child) => !removedIds.has(child.childProductId));
      for (const child of [
        ...readCombinedListingChildInputs(args['productsAdded']),
        ...readCombinedListingChildInputs(args['productsEdited']),
      ]) {
        if (!runtime.store.getEffectiveProductById(child.childProductId)) {
          continue;
        }
        children = children.filter((existingChild) => existingChild.childProductId !== child.childProductId);
        children.push({ ...child, parentProductId });
        const childProduct = runtime.store.getEffectiveProductById(child.childProductId);
        if (childProduct) {
          runtime.store.stageUpdateProduct(
            makeProductRecord(runtime, { id: childProduct.id, combinedListingRole: 'CHILD' }, childProduct),
          );
        }
      }
      runtime.store.replaceStagedCombinedListingChildren(parentProductId, children);
      const title = typeof args['title'] === 'string' ? args['title'] : parentProduct.title;
      runtime.store.stageUpdateProduct(
        makeProductRecord(runtime, { id: parentProductId, title, combinedListingRole: 'PARENT' }, parentProduct),
      );
      const optionRecords = makeCombinedListingOptionRecords(runtime, parentProductId, args['optionsAndValues']);
      if (optionRecords.length > 0) {
        runtime.store.replaceStagedOptionsForProduct(parentProductId, optionRecords);
      }

      return {
        data: {
          [responseKey]: {
            product: serializeProduct(
              runtime,
              runtime.store.getEffectiveProductById(parentProductId),
              getChildField(field, 'product'),
              variables,
            ),
            userErrors: serializeUserErrorLikeList([], getChildField(field, 'userErrors')),
          },
        },
      };
    }
    case 'sellingPlanGroupCreate': {
      const group = runtime.store.upsertStagedSellingPlanGroup(
        applySellingPlanGroupInput(
          runtime,
          readProductInput(args['input']),
          undefined,
          readProductInput(args['resources']),
        ),
      );
      return {
        data: {
          [responseKey]: serializeSellingPlanGroupMutationPayload(runtime, field, variables, {
            sellingPlanGroup: group,
            userErrors: [],
          }),
        },
      };
    }
    case 'sellingPlanGroupUpdate': {
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      const existing = id ? runtime.store.getEffectiveSellingPlanGroupById(id) : null;
      if (!id || !existing) {
        return {
          data: {
            [responseKey]: serializeSellingPlanGroupMutationPayload(runtime, field, variables, {
              deletedSellingPlanIds: null,
              sellingPlanGroup: null,
              userErrors: [sellingPlanGroupDoesNotExistError()],
            }),
          },
        };
      }

      const input = readProductInput(args['input']);
      const deletedSellingPlanIds = readStringArray(input['sellingPlansToDelete']).filter((planId) =>
        existing.sellingPlans.some((plan) => plan.id === planId),
      );
      const group = runtime.store.upsertStagedSellingPlanGroup(applySellingPlanGroupInput(runtime, input, existing));
      return {
        data: {
          [responseKey]: serializeSellingPlanGroupMutationPayload(runtime, field, variables, {
            deletedSellingPlanIds,
            sellingPlanGroup: group,
            userErrors: [],
          }),
        },
      };
    }
    case 'sellingPlanGroupDelete': {
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      const existing = id ? runtime.store.getEffectiveSellingPlanGroupById(id) : null;
      if (!id || !existing) {
        return {
          data: {
            [responseKey]: serializeSellingPlanGroupMutationPayload(runtime, field, variables, {
              deletedSellingPlanGroupId: null,
              userErrors: [sellingPlanGroupDoesNotExistError()],
            }),
          },
        };
      }

      runtime.store.deleteStagedSellingPlanGroup(id);
      return {
        data: {
          [responseKey]: serializeSellingPlanGroupMutationPayload(runtime, field, variables, {
            deletedSellingPlanGroupId: id,
            userErrors: [],
          }),
        },
      };
    }
    case 'sellingPlanGroupAddProducts': {
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      const existing = id ? runtime.store.getEffectiveSellingPlanGroupById(id) : null;
      if (!id || !existing) {
        return {
          data: {
            [responseKey]: serializeSellingPlanGroupMutationPayload(runtime, field, variables, {
              sellingPlanGroup: null,
              userErrors: [sellingPlanGroupDoesNotExistError()],
            }),
          },
        };
      }

      const group = runtime.store.upsertStagedSellingPlanGroup({
        ...existing,
        productIds: uniqueStrings([...existing.productIds, ...readStringArray(args['productIds'])]),
      });
      return {
        data: {
          [responseKey]: serializeSellingPlanGroupMutationPayload(runtime, field, variables, {
            sellingPlanGroup: group,
            userErrors: [],
          }),
        },
      };
    }
    case 'sellingPlanGroupRemoveProducts': {
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      const existing = id ? runtime.store.getEffectiveSellingPlanGroupById(id) : null;
      if (!id || !existing) {
        return {
          data: {
            [responseKey]: serializeSellingPlanGroupMutationPayload(runtime, field, variables, {
              removedProductIds: null,
              userErrors: [sellingPlanGroupDoesNotExistError()],
            }),
          },
        };
      }

      const requestedIds = readStringArray(args['productIds']);
      const requested = new Set(requestedIds);
      const removedProductIds = existing.productIds.filter((productId) => requested.has(productId));
      runtime.store.upsertStagedSellingPlanGroup({
        ...existing,
        productIds: existing.productIds.filter((productId) => !requested.has(productId)),
      });
      return {
        data: {
          [responseKey]: serializeSellingPlanGroupMutationPayload(runtime, field, variables, {
            removedProductIds,
            userErrors: [],
          }),
        },
      };
    }
    case 'sellingPlanGroupAddProductVariants': {
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      const existing = id ? runtime.store.getEffectiveSellingPlanGroupById(id) : null;
      if (!id || !existing) {
        return {
          data: {
            [responseKey]: serializeSellingPlanGroupMutationPayload(runtime, field, variables, {
              sellingPlanGroup: null,
              userErrors: [sellingPlanGroupDoesNotExistError()],
            }),
          },
        };
      }

      const group = runtime.store.upsertStagedSellingPlanGroup({
        ...existing,
        productVariantIds: uniqueStrings([
          ...existing.productVariantIds,
          ...readStringArray(args['productVariantIds']),
        ]),
      });
      return {
        data: {
          [responseKey]: serializeSellingPlanGroupMutationPayload(runtime, field, variables, {
            sellingPlanGroup: group,
            userErrors: [],
          }),
        },
      };
    }
    case 'sellingPlanGroupRemoveProductVariants': {
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      const existing = id ? runtime.store.getEffectiveSellingPlanGroupById(id) : null;
      if (!id || !existing) {
        return {
          data: {
            [responseKey]: serializeSellingPlanGroupMutationPayload(runtime, field, variables, {
              removedProductVariantIds: null,
              userErrors: [sellingPlanGroupDoesNotExistError()],
            }),
          },
        };
      }

      const requested = new Set(readStringArray(args['productVariantIds']));
      const removedProductVariantIds = existing.productVariantIds.filter((variantId) => requested.has(variantId));
      runtime.store.upsertStagedSellingPlanGroup({
        ...existing,
        productVariantIds: existing.productVariantIds.filter((variantId) => !requested.has(variantId)),
      });
      return {
        data: {
          [responseKey]: serializeSellingPlanGroupMutationPayload(runtime, field, variables, {
            removedProductVariantIds,
            userErrors: [],
          }),
        },
      };
    }
    case 'productJoinSellingPlanGroups':
    case 'productLeaveSellingPlanGroups': {
      const productId = typeof args['id'] === 'string' ? args['id'] : null;
      const product = productId ? runtime.store.getEffectiveProductById(productId) : null;
      if (!productId || !product) {
        return {
          data: {
            [responseKey]: serializeProductSellingPlanGroupMutationPayload(runtime, field, variables, {
              product: null,
              userErrors: [{ field: ['id'], message: 'Product does not exist.', code: 'PRODUCT_DOES_NOT_EXIST' }],
            }),
          },
        };
      }

      const isJoin = field.name.value === 'productJoinSellingPlanGroups';
      const userErrors: SellingPlanGroupUserError[] = [];
      for (const groupId of readStringArray(args['sellingPlanGroupIds'])) {
        const group = runtime.store.getEffectiveSellingPlanGroupById(groupId);
        if (!group) {
          userErrors.push(sellingPlanGroupDoesNotExistError());
          continue;
        }

        runtime.store.upsertStagedSellingPlanGroup({
          ...group,
          productIds: isJoin
            ? uniqueStrings([...group.productIds, productId])
            : group.productIds.filter((existingProductId) => existingProductId !== productId),
        });
      }

      return {
        data: {
          [responseKey]: serializeProductSellingPlanGroupMutationPayload(runtime, field, variables, {
            product: runtime.store.getEffectiveProductById(productId),
            userErrors,
          }),
        },
      };
    }
    case 'productVariantJoinSellingPlanGroups':
    case 'productVariantLeaveSellingPlanGroups': {
      const variantId = typeof args['id'] === 'string' ? args['id'] : null;
      const variant = variantId ? runtime.store.getEffectiveVariantById(variantId) : null;
      if (!variantId || !variant) {
        return {
          data: {
            [responseKey]: serializeProductSellingPlanGroupMutationPayload(runtime, field, variables, {
              productVariant: null,
              userErrors: [
                {
                  field: ['id'],
                  message: 'Product variant does not exist.',
                  code: 'PRODUCT_VARIANT_DOES_NOT_EXIST',
                },
              ],
            }),
          },
        };
      }

      const isJoin = field.name.value === 'productVariantJoinSellingPlanGroups';
      const userErrors: SellingPlanGroupUserError[] = [];
      for (const groupId of readStringArray(args['sellingPlanGroupIds'])) {
        const group = runtime.store.getEffectiveSellingPlanGroupById(groupId);
        if (!group) {
          userErrors.push(sellingPlanGroupDoesNotExistError());
          continue;
        }

        runtime.store.upsertStagedSellingPlanGroup({
          ...group,
          productVariantIds: isJoin
            ? uniqueStrings([...group.productVariantIds, variantId])
            : group.productVariantIds.filter((existingVariantId) => existingVariantId !== variantId),
        });
      }

      return {
        data: {
          [responseKey]: serializeProductSellingPlanGroupMutationPayload(runtime, field, variables, {
            productVariant: runtime.store.getEffectiveVariantById(variantId),
            userErrors,
          }),
        },
      };
    }
    case 'tagsAdd': {
      const rawId = args['id'];
      const productId = typeof rawId === 'string' ? rawId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              node: null,
              userErrors: [{ field: ['id'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              node: null,
              userErrors: [{ field: ['id'], message: 'Product not found' }],
            },
          },
        };
      }

      const tags = readTagInputs(args['tags'], { allowCommaSeparatedString: true });
      if (tags.length === 0) {
        return {
          data: {
            [responseKey]: {
              node: null,
              userErrors: [{ field: ['tags'], message: 'At least one tag is required' }],
            },
          },
        };
      }

      const nextTags = [...existingProduct.tags];
      for (const tag of tags) {
        if (!nextTags.includes(tag)) {
          nextTags.push(tag);
        }
      }

      runtime.store.stageUpdateProduct(
        makeProductRecord(runtime, { id: productId, tags: normalizeProductTags(nextTags) }, existingProduct),
      );
      if (runtime.store.getBaseProductById(productId)) {
        runtime.store.markTagSearchLagged(productId);
      }
      const product = runtime.store.getEffectiveProductById(productId);
      return {
        data: {
          [responseKey]: {
            node: serializeProduct(runtime, product, getChildField(field, 'node'), variables),
            userErrors: [],
          },
        },
      };
    }
    case 'tagsRemove': {
      const rawId = args['id'];
      const productId = typeof rawId === 'string' ? rawId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              node: null,
              userErrors: [{ field: ['id'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              node: null,
              userErrors: [{ field: ['id'], message: 'Product not found' }],
            },
          },
        };
      }

      const tags = readTagInputs(args['tags'], { allowCommaSeparatedString: false });
      if (tags.length === 0) {
        return {
          data: {
            [responseKey]: {
              node: null,
              userErrors: [{ field: ['tags'], message: 'At least one tag is required' }],
            },
          },
        };
      }

      const nextTags = normalizeProductTags(existingProduct.tags.filter((tag) => !tags.includes(tag)));
      runtime.store.stageUpdateProduct(makeProductRecord(runtime, { id: productId, tags: nextTags }, existingProduct));
      if (runtime.store.getBaseProductById(productId)) {
        runtime.store.markTagSearchLagged(productId);
      }
      const product = runtime.store.getEffectiveProductById(productId);
      return {
        data: {
          [responseKey]: {
            node: serializeProduct(runtime, product, getChildField(field, 'node'), variables),
            userErrors: [],
          },
        },
      };
    }
    case 'productCreate': {
      const input = readProductInput(args['product']);
      const rawTitle = input['title'];
      const title = typeof rawTitle === 'string' ? rawTitle.trim() : '';
      if (!title) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['title'], message: "Title can't be blank" }],
            },
          },
        };
      }

      const preparedCreateInput = prepareProductInputWithResolvedHandle(runtime, input);
      if (preparedCreateInput.error) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [preparedCreateInput.error],
            },
          },
        };
      }

      const product = runtime.store.stageCreateProduct(makeProductRecord(runtime, preparedCreateInput.input));
      runtime.store.replaceStagedOptionsForProduct(product.id, [makeDefaultOptionRecord(runtime, product)]);
      runtime.store.replaceStagedVariantsForProduct(product.id, [makeDefaultVariantRecord(runtime, product)]);
      const syncedProduct = syncProductInventorySummary(runtime, product.id);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(runtime, syncedProduct ?? product, getChildField(field, 'product'), variables),
            userErrors: [],
          },
        },
      };
    }
    case 'productUpdate': {
      const input = readProductInput(args['product']);
      const rawId = input['id'];
      const id = typeof rawId === 'string' ? rawId : null;
      if (!id) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['id'], message: 'Product does not exist' }],
            },
          },
        };
      }

      const existing = runtime.store.getEffectiveProductById(id) ?? undefined;
      if (!existing && readMode === 'snapshot') {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['id'], message: 'Product does not exist' }],
            },
          },
        };
      }

      const rawTitle = input['title'];
      if (existing && typeof rawTitle === 'string' && !rawTitle.trim()) {
        return {
          data: {
            [responseKey]: {
              product: serializeProduct(runtime, existing, getChildField(field, 'product'), variables),
              userErrors: [{ field: ['title'], message: "Title can't be blank" }],
            },
          },
        };
      }

      const preparedUpdateInput = prepareProductInputWithResolvedHandle(runtime, { ...input, id }, existing);
      if (preparedUpdateInput.error) {
        return {
          data: {
            [responseKey]: {
              product: serializeProduct(
                runtime,
                existing ?? runtime.store.getEffectiveProductById(id),
                getChildField(field, 'product'),
                variables,
              ),
              userErrors: [preparedUpdateInput.error],
            },
          },
        };
      }

      runtime.store.stageUpdateProduct(makeProductRecord(runtime, preparedUpdateInput.input, existing));
      const product = runtime.store.getEffectiveProductById(id);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(runtime, product, getChildField(field, 'product'), variables),
            userErrors: [],
          },
        },
      };
    }
    case 'productDelete': {
      const inputArg = field.arguments?.find((argument) => argument.name.value === 'input') ?? null;
      if (inputArg?.value.kind === Kind.VARIABLE) {
        const rawVariableInput = readProductInput(variables[inputArg.value.name.value]);
        if (!hasOwnField(rawVariableInput, 'id') || rawVariableInput['id'] === null) {
          return buildProductDeleteInvalidVariableError(
            rawVariableInput,
            getVariableDefinitionLocation(document, inputArg.value.name.value),
          );
        }
      }

      if (inputArg?.value.kind === Kind.OBJECT) {
        const idField = inputArg.value.fields.find((objectField) => objectField.name.value === 'id') ?? null;
        if (!idField) {
          return buildMissingProductDeleteInputIdArgumentError(getNodeLocation(inputArg.value));
        }

        if (idField.value.kind === Kind.NULL) {
          return buildNullProductDeleteInputIdArgumentError(getNodeLocation(inputArg.value));
        }
      }

      const input = readProductInput(args['input']);
      const inputId = input['id'];
      const argId = args['id'];
      const id = typeof inputId === 'string' ? inputId : typeof argId === 'string' ? argId : null;
      if (!id) {
        return buildProductDeleteInvalidVariableError(input, []);
      }

      const existing = runtime.store.getEffectiveProductById(id);
      if (!existing && readMode === 'snapshot') {
        return {
          data: {
            [responseKey]: {
              deletedProductId: null,
              userErrors: [{ field: ['id'], message: 'Product does not exist' }],
            },
          },
        };
      }

      runtime.store.stageDeleteProduct(id);
      return {
        data: {
          [responseKey]: {
            deletedProductId: id,
            userErrors: [],
          },
        },
      };
    }
    case 'productDuplicate': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: serializeProductDuplicateMutationPayload(runtime, field, variables, {
              newProduct: null,
              productDuplicateOperation: null,
              userErrors: [{ field: ['productId'], message: 'Product id is required' }],
            }),
          },
        };
      }

      const synchronous = args['synchronous'] !== false;
      const sourceProduct = runtime.store.getEffectiveProductById(productId);
      if (!sourceProduct) {
        if (!synchronous) {
          const operation = runtime.store.stageProductOperation({
            id: runtime.syntheticIdentity.makeSyntheticGid('ProductDuplicateOperation'),
            typeName: 'ProductDuplicateOperation',
            productId: null,
            newProductId: null,
            status: 'COMPLETE',
            userErrors: [{ field: ['productId'], message: 'Product does not exist' }],
          });
          const initialOperation: ProductOperationRecord = {
            ...operation,
            status: 'CREATED',
            userErrors: [],
          };

          return {
            data: {
              [responseKey]: serializeProductDuplicateMutationPayload(runtime, field, variables, {
                newProduct: null,
                productDuplicateOperation: initialOperation,
                userErrors: [],
              }),
            },
          };
        }

        return {
          data: {
            [responseKey]: serializeProductDuplicateMutationPayload(runtime, field, variables, {
              newProduct: null,
              productDuplicateOperation: null,
              userErrors: [{ field: ['productId'], message: 'Product not found' }],
            }),
          },
        };
      }

      const duplicatedRecord = makeDuplicatedProductRecord(
        runtime,
        sourceProduct,
        typeof args['newTitle'] === 'string' ? args['newTitle'] : undefined,
      );
      const duplicatedProduct = runtime.store.stageCreateProduct(
        makeProductRecord(
          runtime,
          {
            ...duplicatedRecord,
            handle: ensureUniqueProductHandle(runtime, duplicatedRecord.handle),
          },
          duplicatedRecord,
        ),
      );
      runtime.store.replaceStagedOptionsForProduct(
        duplicatedProduct.id,
        runtime.store
          .getEffectiveOptionsByProductId(productId)
          .map((option) => duplicateOptionRecord(runtime, option, duplicatedProduct.id)),
      );
      runtime.store.replaceStagedVariantsForProduct(
        duplicatedProduct.id,
        runtime.store
          .getEffectiveVariantsByProductId(productId)
          .map((variant) => duplicateVariantRecord(runtime, variant, duplicatedProduct.id)),
      );
      runtime.store.replaceStagedCollectionsForProduct(
        duplicatedProduct.id,
        runtime.store
          .getEffectiveCollectionsByProductId(productId)
          .map((collection) => duplicateCollectionRecord(collection, duplicatedProduct.id)),
      );
      // Captured Shopify duplicate responses keep immediate duplicate media empty even when the source has ready media.
      runtime.store.replaceStagedMediaForProduct(duplicatedProduct.id, []);
      runtime.store.replaceStagedMetafieldsForProduct(
        duplicatedProduct.id,
        runtime.store
          .getEffectiveMetafieldsByProductId(productId)
          .map((metafield) => duplicateMetafieldRecord(runtime, metafield, duplicatedProduct.id)),
      );
      const product =
        syncProductInventorySummary(runtime, duplicatedProduct.id) ??
        runtime.store.getEffectiveProductById(duplicatedProduct.id);
      const productDuplicateOperation = synchronous
        ? null
        : runtime.store.stageProductOperation({
            id: runtime.syntheticIdentity.makeSyntheticGid('ProductDuplicateOperation'),
            typeName: 'ProductDuplicateOperation',
            productId,
            newProductId: duplicatedProduct.id,
            status: 'COMPLETE',
            userErrors: [],
          });

      return {
        data: {
          [responseKey]: synchronous
            ? serializeProductDuplicateMutationPayload(runtime, field, variables, {
                newProduct: product,
                productDuplicateOperation: null,
                userErrors: [],
              })
            : serializeProductDuplicateMutationPayload(runtime, field, variables, {
                newProduct: null,
                productDuplicateOperation: productDuplicateOperation
                  ? {
                      ...productDuplicateOperation,
                      newProductId: null,
                      status: 'CREATED',
                    }
                  : null,
                userErrors: [],
              }),
        },
      };
    }
    case 'productSet': {
      const identifier = readProductInput(args['identifier']);
      const input = readProductInput(args['input']);
      const identifierId = typeof identifier['id'] === 'string' ? identifier['id'] : null;
      const identifierHandle = typeof identifier['handle'] === 'string' ? identifier['handle'] : null;
      const inputId = typeof input['id'] === 'string' ? input['id'] : null;
      const synchronous = args['synchronous'] !== false;
      const existing =
        (identifierId ? runtime.store.getEffectiveProductById(identifierId) : null) ??
        (inputId ? runtime.store.getEffectiveProductById(inputId) : null) ??
        (identifierHandle ? findEffectiveProductByHandle(runtime, identifierHandle) : null);
      const productInput =
        !existing && !hasOwnField(input, 'descriptionHtml') ? { ...input, descriptionHtml: '' } : input;
      const preparedInput = prepareProductInputWithResolvedHandle(
        runtime,
        existing ? { ...productInput, id: existing.id } : productInput,
        existing ?? undefined,
      );
      if (preparedInput.error) {
        return {
          data: {
            [responseKey]: {
              product: synchronous
                ? serializeProduct(runtime, existing, getChildField(field, 'product'), variables)
                : null,
              productSetOperation: null,
              userErrors: [preparedInput.error],
            },
          },
        };
      }

      const productRecord = makeProductRecord(runtime, preparedInput.input, existing ?? undefined);
      const stagedProduct = existing
        ? runtime.store.stageUpdateProduct(productRecord)
        : runtime.store.stageCreateProduct({
            ...productRecord,
            onlineStorePreviewUrl:
              productRecord.onlineStorePreviewUrl ?? makeSyntheticOnlineStorePreviewUrl(productRecord),
          });
      const productId = stagedProduct.id;

      if (hasOwnField(input, 'productOptions')) {
        runtime.store.replaceStagedOptionsForProduct(
          productId,
          buildProductSetOptionRecords(runtime, productId, input['productOptions']),
        );
      } else if (!existing && runtime.store.getEffectiveOptionsByProductId(productId).length === 0) {
        runtime.store.replaceStagedOptionsForProduct(productId, [makeDefaultOptionRecord(runtime, stagedProduct)]);
      }

      if (hasOwnField(input, 'variants')) {
        const nextVariants = buildProductSetVariantRecords(runtime, productId, input['variants']);
        runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);
      } else if (!existing && runtime.store.getEffectiveVariantsByProductId(productId).length === 0) {
        runtime.store.replaceStagedVariantsForProduct(productId, [makeDefaultVariantRecord(runtime, stagedProduct)]);
      }

      if (hasOwnField(input, 'productOptions') || hasOwnField(input, 'variants')) {
        runtime.store.replaceStagedOptionsForProduct(
          productId,
          syncProductOptionsWithVariants(
            runtime,
            productId,
            runtime.store.getEffectiveOptionsByProductId(productId),
            runtime.store.getEffectiveVariantsByProductId(productId),
          ),
        );
      }

      if (hasOwnField(input, 'collections')) {
        runtime.store.replaceStagedCollectionsForProduct(
          productId,
          buildProductSetCollectionRecords(runtime, productId, input['collections']),
        );
      }

      if (hasOwnField(input, 'metafields')) {
        runtime.store.replaceStagedMetafieldsForProduct(
          productId,
          buildProductSetMetafieldRecords(runtime, productId, input['metafields']),
        );
      }

      const product =
        syncProductSetInventorySummary(runtime, productId, existing) ??
        runtime.store.getEffectiveProductById(productId);
      const productSetOperation = synchronous
        ? null
        : runtime.store.stageProductOperation({
            id: runtime.syntheticIdentity.makeSyntheticGid('ProductSetOperation'),
            typeName: 'ProductSetOperation',
            productId,
            status: 'CREATED',
            userErrors: [],
          });
      return {
        data: {
          [responseKey]: {
            product: synchronous
              ? serializeProduct(runtime, product, getChildField(field, 'product'), variables)
              : null,
            productSetOperation: serializeProductSetOperation(
              getChildField(field, 'productSetOperation'),
              productSetOperation,
            ),
            userErrors: [],
          },
        },
      };
    }
    case 'productChangeStatus': {
      const rawProductId = args['productId'];
      if (rawProductId === null) {
        return buildNullProductChangeStatusArgumentError(getNodeLocation(field), getOperationPathLabel(document));
      }

      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['productId'], message: 'Product id is required' }],
            },
          },
        };
      }

      const rawStatus = args['status'];
      if (rawStatus !== 'ACTIVE' && rawStatus !== 'ARCHIVED' && rawStatus !== 'DRAFT') {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['status'], message: 'Product status is required' }],
            },
          },
        };
      }

      const existing = runtime.store.getEffectiveProductById(productId);
      if (!existing) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['productId'], message: 'Product does not exist' }],
            },
          },
        };
      }

      runtime.store.stageUpdateProduct(
        makeProductRecord(
          runtime,
          {
            id: productId,
            status: rawStatus,
          },
          existing,
        ),
      );
      const product = runtime.store.getEffectiveProductById(productId);

      return {
        data: {
          [responseKey]: {
            product: serializeProduct(runtime, product, getChildField(field, 'product'), variables),
            userErrors: [],
          },
        },
      };
    }
    case 'productPublish': {
      const input = isObject(args['input']) ? args['input'] : null;
      const productId = input && typeof input['id'] === 'string' ? input['id'] : null;
      const productField = getChildField(field, 'product');
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              ...(productField ? { product: null } : {}),
              userErrors: [{ field: ['input', 'id'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existing = runtime.store.getEffectiveProductById(productId);
      if (!existing) {
        return {
          data: {
            [responseKey]: {
              ...(productField ? { product: null } : {}),
              userErrors: [{ field: ['input', 'id'], message: 'Product not found' }],
            },
          },
        };
      }

      const nextPublicationIds = mergePublicationTargets(
        existing.publicationIds,
        readPublicationTargets(input?.['productPublications']),
      );
      runtime.store.stageUpdateProduct(
        makeProductRecord(runtime, { id: productId, publicationIds: nextPublicationIds }, existing),
      );
      const product = runtime.store.getEffectiveProductById(productId);

      return {
        data: {
          [responseKey]: {
            ...(productField ? { product: serializeProduct(runtime, product, productField, variables) } : {}),
            userErrors: [],
          },
        },
      };
    }
    case 'productUnpublish': {
      const input = isObject(args['input']) ? args['input'] : null;
      const productId = input && typeof input['id'] === 'string' ? input['id'] : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: serializeProductMutationPayload(runtime, field, variables, {
              product: null,
              userErrors: [{ field: ['input', 'id'], message: 'Product id is required' }],
            }),
          },
        };
      }

      const existing = runtime.store.getEffectiveProductById(productId);
      if (!existing) {
        return {
          data: {
            [responseKey]: serializeProductMutationPayload(runtime, field, variables, {
              product: null,
              userErrors: [{ field: ['input', 'id'], message: 'Product not found' }],
            }),
          },
        };
      }

      const nextPublicationIds = removePublicationTargets(
        existing.publicationIds,
        readPublicationTargets(input?.['productPublications']),
      );
      runtime.store.stageUpdateProduct(
        makeProductRecord(runtime, { id: productId, publicationIds: nextPublicationIds }, existing),
      );
      const product = runtime.store.getEffectiveProductById(productId);

      return {
        data: {
          [responseKey]: serializeProductMutationPayload(runtime, field, variables, {
            product,
            userErrors: [],
          }),
        },
      };
    }
    case 'publicationCreate': {
      const input = readProductInput(args['input'] ?? args['publication']);
      const publication = runtime.store.stageCreatePublication(makePublicationRecord(runtime, input));

      return {
        data: {
          [responseKey]: serializePublicationMutationPayload(runtime, field, {
            publication,
            userErrors: [],
          }),
        },
      };
    }
    case 'publicationUpdate': {
      const input = readProductInput(args['input'] ?? args['publication']);
      const rawId = args['id'] ?? input['id'];
      const publicationId = typeof rawId === 'string' ? rawId : null;
      if (!publicationId) {
        return {
          data: {
            [responseKey]: serializePublicationMutationPayload(runtime, field, {
              publication: null,
              userErrors: [{ field: ['id'], message: 'Publication id is required' }],
            }),
          },
        };
      }

      const existing = runtime.store.getEffectivePublicationById(publicationId);
      if (!existing) {
        return {
          data: {
            [responseKey]: serializePublicationMutationPayload(runtime, field, {
              publication: null,
              userErrors: [{ field: ['id'], message: 'Publication not found' }],
            }),
          },
        };
      }

      const publication = runtime.store.stageUpdatePublication(
        makePublicationRecord(runtime, { ...input, id: publicationId }, existing),
      );
      return {
        data: {
          [responseKey]: serializePublicationMutationPayload(runtime, field, {
            publication,
            userErrors: [],
          }),
        },
      };
    }
    case 'publicationDelete': {
      const rawId = args['id'];
      const publicationId = typeof rawId === 'string' ? rawId : null;
      if (!publicationId) {
        return {
          data: {
            [responseKey]: serializePublicationMutationPayload(runtime, field, {
              publication: null,
              deletedId: null,
              userErrors: [{ field: ['id'], message: 'Publication id is required' }],
            }),
          },
        };
      }

      const existing = runtime.store.getEffectivePublicationById(publicationId);
      if (!existing) {
        return {
          data: {
            [responseKey]: serializePublicationMutationPayload(runtime, field, {
              publication: null,
              deletedId: null,
              userErrors: [{ field: ['id'], message: 'Publication not found' }],
            }),
          },
        };
      }

      removePublicationFromPublishables(runtime, publicationId);
      runtime.store.stageDeletePublication(publicationId);
      return {
        data: {
          [responseKey]: serializePublicationMutationPayload(runtime, field, {
            publication: existing,
            deletedId: publicationId,
            userErrors: [],
          }),
        },
      };
    }
    case 'publishablePublish':
    case 'publishableUnpublish': {
      const publishableId = typeof args['id'] === 'string' ? args['id'] : null;
      if (!publishableId) {
        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
              publishable: null,
              userErrors: [{ field: ['id'], message: 'Publishable id is required' }],
            }),
          },
        };
      }

      const isPublish = field.name.value === 'publishablePublish';
      const publicationTargets = readPublicationTargets(args['input']);
      const existingProduct = runtime.store.getEffectiveProductById(publishableId);
      if (existingProduct) {
        if (publicationTargets.length === 0) {
          return {
            data: {
              [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
                publishable: existingProduct,
                userErrors: [{ field: ['input'], message: 'Publication target is required' }],
              }),
            },
          };
        }

        const nextPublicationIds = isPublish
          ? mergePublicationTargets(existingProduct.publicationIds, publicationTargets)
          : removePublicationTargets(existingProduct.publicationIds, publicationTargets);
        runtime.store.stageUpdateProduct(
          makeProductRecord(runtime, { id: publishableId, publicationIds: nextPublicationIds }, existingProduct),
        );

        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
              publishable: runtime.store.getEffectiveProductById(publishableId),
              userErrors: [],
            }),
          },
        };
      }

      if (publishableId.startsWith('gid://shopify/Product/')) {
        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
              publishable: null,
              userErrors: [{ field: ['id'], message: 'Product not found' }],
            }),
          },
        };
      }

      const existingCollection = findEffectiveCollectionById(runtime, publishableId);
      if (existingCollection) {
        if (publicationTargets.length === 0) {
          return {
            data: {
              [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
                publishable: existingCollection,
                userErrors: [{ field: ['input'], message: 'Publication target is required' }],
              }),
            },
          };
        }

        const nextPublicationIds = isPublish
          ? mergePublicationTargets(existingCollection.publicationIds ?? [], publicationTargets)
          : removePublicationTargets(existingCollection.publicationIds ?? [], publicationTargets);
        runtime.store.stageUpdateCollection(
          makeCollectionRecord(runtime, { id: publishableId, publicationIds: nextPublicationIds }, existingCollection),
        );

        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
              publishable: findEffectiveCollectionById(runtime, publishableId),
              userErrors: [],
            }),
          },
        };
      }

      if (publishableId.startsWith('gid://shopify/Collection/')) {
        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
              publishable: null,
              userErrors: [{ field: ['id'], message: 'Collection not found' }],
            }),
          },
        };
      }

      return {
        data: {
          [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
            publishable: null,
            userErrors: [
              { field: ['id'], message: 'Only Product and Collection publishable IDs are supported locally' },
            ],
          }),
        },
      };
    }
    case 'publishablePublishToCurrentChannel': {
      const rawPublishableId = args['id'];
      const productId = getPublishableProductId(rawPublishableId);
      if (!productId) {
        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
              publishable: null,
              userErrors: [{ field: ['id'], message: 'Only Product publishable IDs are supported locally' }],
            }),
          },
        };
      }

      const existing = runtime.store.getEffectiveProductById(productId);
      if (!existing) {
        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
              publishable: null,
              userErrors: [{ field: ['id'], message: 'Product not found' }],
            }),
          },
        };
      }

      const publicationTargets =
        field.name.value === 'publishablePublishToCurrentChannel'
          ? [currentPublicationPlaceholderId]
          : readPublicationTargets(args['input']);
      if (publicationTargets.length === 0) {
        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
              publishable: existing,
              userErrors: [{ field: ['input'], message: 'Publication target is required' }],
            }),
          },
        };
      }

      const nextPublicationIds = mergePublicationTargets(existing.publicationIds, publicationTargets);
      runtime.store.stageUpdateProduct(
        makeProductRecord(runtime, { id: productId, publicationIds: nextPublicationIds }, existing),
      );
      const product = runtime.store.getEffectiveProductById(productId);

      return {
        data: {
          [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
            publishable: product,
            userErrors: [],
          }),
        },
      };
    }
    case 'publishableUnpublishToCurrentChannel': {
      const rawPublishableId = args['id'];
      const productId = getPublishableProductId(rawPublishableId);
      if (!productId) {
        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
              publishable: null,
              userErrors: [{ field: ['id'], message: 'Only Product publishable IDs are supported locally' }],
            }),
          },
        };
      }

      const existing = runtime.store.getEffectiveProductById(productId);
      if (!existing) {
        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
              publishable: null,
              userErrors: [{ field: ['id'], message: 'Product not found' }],
            }),
          },
        };
      }

      const publicationTargets =
        field.name.value === 'publishableUnpublishToCurrentChannel'
          ? [currentPublicationPlaceholderId]
          : readPublicationTargets(args['input']);
      if (publicationTargets.length === 0) {
        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
              publishable: existing,
              userErrors: [{ field: ['input'], message: 'Publication target is required' }],
            }),
          },
        };
      }

      const nextPublicationIds = removePublicationTargets(existing.publicationIds, publicationTargets);
      runtime.store.stageUpdateProduct(
        makeProductRecord(runtime, { id: productId, publicationIds: nextPublicationIds }, existing),
      );
      const product = runtime.store.getEffectiveProductById(productId);

      return {
        data: {
          [responseKey]: serializePublishableMutationPayload(runtime, field, variables, {
            publishable: product,
            userErrors: [],
          }),
        },
      };
    }
    case 'productOptionsCreate': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['productId'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['productId'], message: 'Product does not exist' }],
            },
          },
        };
      }

      const existingOptions = runtime.store.getEffectiveOptionsByProductId(productId);
      const existingVariants = runtime.store.getEffectiveVariantsByProductId(productId);
      let nextOptions = existingOptions;
      let nextVariants = existingVariants;
      const optionInputs = Array.isArray(args['options']) ? args['options'] : [];
      const shouldCreateOptionVariants = args['variantStrategy'] === 'CREATE';
      const shouldReplaceDefaultOptionState = productUsesOnlyDefaultOptionState(existingOptions, existingVariants);
      if (shouldReplaceDefaultOptionState) {
        nextOptions = [];
      }
      for (const optionInput of optionInputs) {
        if (!isObject(optionInput)) {
          continue;
        }

        nextOptions = insertOptionAtPosition(
          nextOptions,
          makeCreatedOptionRecord(runtime, productId, optionInput),
          optionInput['position'],
        );
      }

      if (shouldReplaceDefaultOptionState && existingVariants[0]) {
        nextVariants = [remapDefaultVariantToCreatedOptions(existingVariants[0], nextOptions)];
        runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);
      }

      if (shouldCreateOptionVariants) {
        nextVariants = createVariantsForOptionValueCombinations(runtime, productId, nextOptions, nextVariants);
        runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);
      }

      nextOptions = syncProductOptionsWithVariants(runtime, productId, nextOptions, nextVariants);
      runtime.store.replaceStagedOptionsForProduct(productId, nextOptions);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(
              runtime,
              runtime.store.getEffectiveProductById(productId),
              getChildField(field, 'product'),
              variables,
            ),
            userErrors: [],
          },
        },
      };
    }
    case 'productOptionUpdate': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['productId'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['productId'], message: 'Product not found' }],
            },
          },
        };
      }

      const optionInput = readProductInput(args['option']);
      const rawOptionId = optionInput['id'];
      if (typeof rawOptionId === 'string') {
        const optionExists = runtime.store
          .getEffectiveOptionsByProductId(productId)
          .some((option) => option.id === rawOptionId && option.productId === productId);
        if (!optionExists) {
          return {
            data: {
              [responseKey]: {
                product: serializeProduct(runtime, existingProduct, getChildField(field, 'product'), variables),
                userErrors: [{ field: ['option'], message: 'Option does not exist' }],
              },
            },
          };
        }
      }

      const updateResult = updateOptionRecords(
        runtime,
        productId,
        runtime.store.getEffectiveOptionsByProductId(productId),
        runtime.store.getEffectiveVariantsByProductId(productId),
        optionInput,
        args['optionValuesToAdd'],
        args['optionValuesToUpdate'],
        args['optionValuesToDelete'],
      );
      if (!updateResult) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['option', 'id'], message: 'Option id is required' }],
            },
          },
        };
      }

      runtime.store.replaceStagedVariantsForProduct(productId, updateResult.variants);
      const syncedOptions = syncProductOptionsWithVariants(
        runtime,
        productId,
        updateResult.options,
        updateResult.variants,
      );
      runtime.store.replaceStagedOptionsForProduct(productId, syncedOptions);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(
              runtime,
              runtime.store.getEffectiveProductById(productId),
              getChildField(field, 'product'),
              variables,
            ),
            userErrors: [],
          },
        },
      };
    }
    case 'productOptionsDelete': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              deletedOptionsIds: [],
              product: null,
              userErrors: [{ field: ['productId'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              deletedOptionsIds: [],
              product: null,
              userErrors: [{ field: ['productId'], message: 'Product not found' }],
            },
          },
        };
      }

      const effectiveOptions = runtime.store.getEffectiveOptionsByProductId(productId);
      const effectiveVariants = runtime.store.getEffectiveVariantsByProductId(productId);
      const optionIds = Array.isArray(args['options'])
        ? args['options'].filter((value): value is string => typeof value === 'string')
        : [];
      const existingOptionIds = new Set(effectiveOptions.map((option) => option.id));
      const unknownOptionErrors = optionIds
        .map((optionId, index) =>
          existingOptionIds.has(optionId)
            ? null
            : {
                field: ['options', String(index)],
                message: 'Option does not exist',
              },
        )
        .filter((userError): userError is { field: string[]; message: string } => userError !== null);
      if (unknownOptionErrors.length > 0) {
        return {
          data: {
            [responseKey]: {
              deletedOptionsIds: [],
              product: serializeProduct(runtime, existingProduct, getChildField(field, 'product'), variables),
              userErrors: unknownOptionErrors,
            },
          },
        };
      }

      const deleteResult = deleteOptionRecords(productId, effectiveOptions, args['options']);
      let nextOptions = deleteResult.options;
      let nextVariants = effectiveVariants;
      if (nextOptions.length === 0) {
        const restoredDefaultState = restoreDefaultOptionState(runtime, existingProduct, effectiveVariants);
        nextOptions = restoredDefaultState.options;
        nextVariants = restoredDefaultState.variants;
        runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);
      }
      runtime.store.replaceStagedOptionsForProduct(
        productId,
        syncProductOptionsWithVariants(runtime, productId, nextOptions, nextVariants),
      );
      return {
        data: {
          [responseKey]: {
            deletedOptionsIds: deleteResult.deletedOptionIds,
            product: serializeProduct(
              runtime,
              runtime.store.getEffectiveProductById(productId),
              getChildField(field, 'product'),
              variables,
            ),
            userErrors: [],
          },
        },
      };
    }
    case 'productOptionsReorder': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['productId'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['productId'], message: 'Product not found' }],
            },
          },
        };
      }

      const reorderResult = reorderProductOptionsAndVariants(runtime, productId, args['options']);
      if (reorderResult.userErrors.length > 0) {
        return {
          data: {
            [responseKey]: {
              product: serializeProduct(runtime, existingProduct, getChildField(field, 'product'), variables),
              userErrors: reorderResult.userErrors,
            },
          },
        };
      }

      runtime.store.replaceStagedOptionsForProduct(productId, reorderResult.options);
      runtime.store.replaceStagedVariantsForProduct(productId, reorderResult.variants);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(
              runtime,
              runtime.store.getEffectiveProductById(productId),
              getChildField(field, 'product'),
              variables,
            ),
            userErrors: [],
          },
        },
      };
    }
    case 'collectionCreate': {
      const input = readProductInput(args['input']);
      const rawTitle = input['title'];
      const title = typeof rawTitle === 'string' && rawTitle.trim() ? rawTitle : null;
      if (!title) {
        return {
          data: {
            [responseKey]: {
              collection: null,
              userErrors: [{ field: ['input', 'title'], message: 'Collection title is required' }],
            },
          },
        };
      }

      const collection = makeCollectionRecord(runtime, input);
      const productIds = readCollectionProductIds(input['products']);
      if (productIds.length > 0) {
        const result = addProductsToCollection(runtime, collection, productIds);
        if (result.userErrors.length > 0) {
          return {
            data: {
              [responseKey]: {
                collection: null,
                userErrors: result.userErrors,
              },
            },
          };
        }
      }

      const stagedCollection = runtime.store.stageCreateCollection(collection);
      return {
        data: {
          [responseKey]: {
            collection: serializeCollectionObject(
              runtime,
              stagedCollection,
              getChildField(field, 'collection')?.selectionSet?.selections ?? [],
              variables,
              { productsCountOverride: 0 },
            ),
            userErrors: [],
          },
        },
      };
    }
    case 'collectionUpdate': {
      const input = readProductInput(args['input']);
      const rawId = input['id'];
      const collectionId = typeof rawId === 'string' ? rawId : null;
      if (!collectionId) {
        return {
          data: {
            [responseKey]: {
              collection: null,
              userErrors: [{ field: ['input', 'id'], message: 'Collection id is required' }],
            },
          },
        };
      }

      const existing = findEffectiveCollectionById(runtime, collectionId);
      if (!existing) {
        return {
          data: {
            [responseKey]: {
              collection: null,
              userErrors: [{ field: ['input', 'id'], message: 'Collection not found' }],
            },
          },
        };
      }

      const collection = runtime.store.stageUpdateCollection(makeCollectionRecord(runtime, input, existing));
      return {
        data: {
          [responseKey]: {
            collection: serializeCollectionObject(
              runtime,
              collection,
              getChildField(field, 'collection')?.selectionSet?.selections ?? [],
              variables,
            ),
            userErrors: [],
          },
        },
      };
    }
    case 'collectionDelete': {
      const input = readProductInput(args['input']);
      const rawId = input['id'];
      const collectionId = typeof rawId === 'string' ? rawId : null;
      if (!collectionId) {
        return {
          data: {
            [responseKey]: {
              deletedCollectionId: null,
              userErrors: [{ field: ['input', 'id'], message: 'Collection id is required' }],
            },
          },
        };
      }

      const existing = findEffectiveCollectionById(runtime, collectionId);
      if (!existing) {
        return {
          data: {
            [responseKey]: {
              deletedCollectionId: null,
              userErrors: [{ field: ['input', 'id'], message: 'Collection not found' }],
            },
          },
        };
      }

      runtime.store.stageDeleteCollection(collectionId);
      return {
        data: {
          [responseKey]: {
            deletedCollectionId: collectionId,
            userErrors: [],
          },
        },
      };
    }
    case 'collectionAddProducts': {
      const rawCollectionId = args['id'];
      const collectionId = typeof rawCollectionId === 'string' ? rawCollectionId : null;
      if (!collectionId) {
        return {
          data: {
            [responseKey]: {
              collection: null,
              userErrors: [{ field: ['id'], message: 'Collection id is required' }],
            },
          },
        };
      }

      const existing = findEffectiveCollectionById(runtime, collectionId);
      if (!existing) {
        return {
          data: {
            [responseKey]: {
              collection: null,
              userErrors: [{ field: ['id'], message: 'Collection does not exist' }],
            },
          },
        };
      }

      const productIds = Array.isArray(args['productIds'])
        ? args['productIds'].filter((productId): productId is string => typeof productId === 'string')
        : [];
      const result = addProductsToCollection(runtime, existing, productIds);
      return {
        data: {
          [responseKey]: {
            collection: result.collection
              ? serializeCollectionObject(
                  runtime,
                  result.collection,
                  getChildField(field, 'collection')?.selectionSet?.selections ?? [],
                  variables,
                )
              : null,
            userErrors: result.userErrors,
          },
        },
      };
    }
    case 'collectionAddProductsV2': {
      const rawCollectionId = args['id'];
      const collectionId = typeof rawCollectionId === 'string' ? rawCollectionId : null;
      if (!collectionId) {
        return {
          data: {
            [responseKey]: {
              job: null,
              userErrors: [{ field: ['id'], message: 'Collection id is required' }],
            },
          },
        };
      }

      const existing = findEffectiveCollectionById(runtime, collectionId);
      if (!existing) {
        return {
          data: {
            [responseKey]: {
              job: null,
              userErrors: [{ field: ['id'], message: 'Collection does not exist' }],
            },
          },
        };
      }

      const productIds = Array.isArray(args['productIds'])
        ? args['productIds'].filter((productId): productId is string => typeof productId === 'string')
        : [];
      const result = addProductsToCollection(runtime, existing, productIds, {
        placement: existing.sortOrder === 'MANUAL' ? 'append' : 'prepend-reverse',
      });
      const job = result.collection ? { id: runtime.syntheticIdentity.makeSyntheticGid('Job'), done: false } : null;
      return {
        data: {
          [responseKey]: {
            job: job
              ? serializeJobSelectionSet(job, getChildField(field, 'job')?.selectionSet?.selections ?? [])
              : null,
            userErrors: result.userErrors,
          },
        },
      };
    }
    case 'collectionRemoveProducts': {
      const rawCollectionId = args['id'];
      const collectionId = typeof rawCollectionId === 'string' ? rawCollectionId : null;
      if (!collectionId) {
        return {
          data: {
            [responseKey]: {
              job: null,
              userErrors: [{ field: ['id'], message: 'Collection id is required' }],
            },
          },
        };
      }

      const existing = findEffectiveCollectionById(runtime, collectionId);
      if (!existing) {
        return {
          data: {
            [responseKey]: {
              job: null,
              userErrors: [{ field: ['id'], message: 'Collection not found' }],
            },
          },
        };
      }

      const productIds = Array.isArray(args['productIds'])
        ? args['productIds'].filter((productId): productId is string => typeof productId === 'string')
        : [];
      removeProductsFromCollection(runtime, existing, productIds);
      const job = { id: runtime.syntheticIdentity.makeSyntheticGid('Job'), done: false };
      return {
        data: {
          [responseKey]: {
            job: serializeJobSelectionSet(job, getChildField(field, 'job')?.selectionSet?.selections ?? []),
            userErrors: [],
          },
        },
      };
    }
    case 'collectionReorderProducts': {
      const rawCollectionId = args['id'];
      const collectionId = typeof rawCollectionId === 'string' ? rawCollectionId : null;
      if (!collectionId) {
        return {
          data: {
            [responseKey]: {
              job: null,
              userErrors: [{ field: ['id'], message: 'Collection id is required' }],
            },
          },
        };
      }

      const existing = findEffectiveCollectionById(runtime, collectionId);
      if (!existing) {
        return {
          data: {
            [responseKey]: {
              job: null,
              userErrors: [{ field: ['id'], message: 'Collection not found' }],
            },
          },
        };
      }

      const result = reorderCollectionProducts(runtime, existing, args['moves']);
      return {
        data: {
          [responseKey]: {
            job: result.job
              ? serializeJobSelectionSet(result.job, getChildField(field, 'job')?.selectionSet?.selections ?? [])
              : null,
            userErrors: result.userErrors,
          },
        },
      };
    }
    case 'productReorderMedia': {
      const rawProductId = args['id'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              job: null,
              mediaUserErrors: [{ field: ['id'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              job: null,
              mediaUserErrors: [{ field: ['id'], message: 'Product not found' }],
            },
          },
        };
      }

      const result = reorderProductMedia(runtime, productId, args['moves']);
      return {
        data: {
          [responseKey]: {
            job: result.job
              ? serializeJobSelectionSet(result.job, getChildField(field, 'job')?.selectionSet?.selections ?? [])
              : null,
            mediaUserErrors: result.userErrors,
          },
        },
      };
    }
    case 'productVariantAppendMedia':
    case 'productVariantDetachMedia': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: serializeVariantMediaMutationPayload(runtime, field, variables, {
              product: null,
              productVariants: [],
              userErrors: [{ field: ['productId'], message: 'Product id is required' }],
            }),
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: serializeVariantMediaMutationPayload(runtime, field, variables, {
              product: null,
              productVariants: [],
              userErrors: [{ field: ['productId'], message: 'Product does not exist' }],
            }),
          },
        };
      }

      const isAppend = field.name.value === 'productVariantAppendMedia';
      const effectiveVariants = runtime.store.getEffectiveVariantsByProductId(productId);
      const variantsById = new Map(effectiveVariants.map((variant) => [variant.id, structuredClone(variant)]));
      const productMediaIds = new Set(
        runtime.store
          .getEffectiveMediaByProductId(productId)
          .map((mediaRecord) => mediaRecord.id)
          .filter((mediaId): mediaId is string => typeof mediaId === 'string'),
      );
      const updatedVariantIds: string[] = [];
      const userErrors: Array<{ field: string[]; message: string }> = [];

      readVariantMediaInputs(args['variantMedia']).forEach((input, index) => {
        const variant = variantsById.get(input.variantId);
        if (!variant) {
          userErrors.push({ field: ['variantMedia', String(index), 'variantId'], message: 'Variant does not exist' });
          return;
        }

        const unknownMediaIndex = input.mediaIds.findIndex((mediaId) => !productMediaIds.has(mediaId));
        if (unknownMediaIndex >= 0) {
          userErrors.push({
            field: ['variantMedia', String(index), 'mediaIds', String(unknownMediaIndex)],
            message: 'Media does not exist',
          });
          return;
        }

        const currentMediaIds = variant.mediaIds ?? [];
        variant.mediaIds = isAppend
          ? uniqueStrings([...currentMediaIds, ...input.mediaIds])
          : currentMediaIds.filter((mediaId) => !input.mediaIds.includes(mediaId));
        variantsById.set(variant.id, variant);
        updatedVariantIds.push(variant.id);
      });

      if (userErrors.length === 0) {
        runtime.store.replaceStagedVariantsForProduct(
          productId,
          effectiveVariants.map((variant) => variantsById.get(variant.id) ?? variant),
        );
      }

      const updatedVariants = uniqueStrings(updatedVariantIds)
        .map((variantId) => runtime.store.getEffectiveVariantById(variantId))
        .filter((variant): variant is ProductVariantRecord => variant !== null);
      return {
        data: {
          [responseKey]: serializeVariantMediaMutationPayload(runtime, field, variables, {
            product: existingProduct,
            productVariants: updatedVariants,
            userErrors,
          }),
        },
      };
    }
    case 'productCreateMedia': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (productId === '') {
        return buildInvalidProductMediaProductIdVariableError(productId, document);
      }

      const mediaInput = Array.isArray(args['media']) ? args['media'] : [];
      for (const [mediaIndex, media] of mediaInput.entries()) {
        if (!isObject(media)) {
          continue;
        }

        const rawMediaContentType = media['mediaContentType'];
        if (typeof rawMediaContentType === 'string' && !CREATE_MEDIA_CONTENT_TYPES.has(rawMediaContentType)) {
          return buildInvalidProductMediaContentTypeVariableError(
            mediaInput,
            mediaIndex,
            rawMediaContentType,
            document,
          );
        }
      }

      if (!productId) {
        return {
          data: {
            [responseKey]: {
              media: [],
              mediaUserErrors: [{ field: ['productId'], message: 'Product id is required' }],
              product: null,
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: mediaValidationProductNotFoundPayload('create'),
          },
        };
      }

      const existingMedia = runtime.store.getEffectiveMediaByProductId(productId);
      const createdMedia: ProductMediaRecord[] = [];
      const mediaUserErrors: Array<{ field: string[]; message: string }> = [];
      for (const [mediaIndex, media] of mediaInput.entries()) {
        if (!isObject(media)) {
          continue;
        }

        const mediaContentType = typeof media['mediaContentType'] === 'string' ? media['mediaContentType'] : 'IMAGE';
        if (mediaContentType === 'IMAGE' && !isValidMediaSource(media['originalSource'])) {
          mediaUserErrors.push({
            field: ['media', `${mediaIndex}`, 'originalSource'],
            message: 'Image URL is invalid',
          });
          continue;
        }

        createdMedia.push(
          makeCreatedMediaRecord(runtime, productId, media, existingMedia.length + createdMedia.length),
        );
      }
      const nextMedia = [...existingMedia, ...createdMedia];
      if (createdMedia.length > 0) {
        runtime.store.replaceStagedMediaForProduct(productId, nextMedia);
      }

      const response = {
        data: {
          [responseKey]: {
            media: serializeMediaPayload(createdMedia, getChildField(field, 'media')),
            mediaUserErrors,
            product: serializeProduct(
              runtime,
              runtime.store.getEffectiveProductById(productId),
              getChildField(field, 'product'),
              variables,
            ),
          },
        },
      };

      if (createdMedia.length > 0) {
        runtime.store.replaceStagedMediaForProduct(productId, [
          ...existingMedia,
          ...createdMedia.map((mediaRecord) => transitionMediaToProcessing(mediaRecord)),
        ]);
      }

      return response;
    }
    case 'productUpdateMedia': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (productId === '') {
        return buildInvalidProductMediaProductIdVariableError(productId, document);
      }

      if (!productId) {
        return {
          data: {
            [responseKey]: {
              media: [],
              mediaUserErrors: [{ field: ['productId'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: mediaValidationProductNotFoundPayload('update'),
          },
        };
      }

      const effectiveMedia = runtime.store.getEffectiveMediaByProductId(productId);
      const updates = (Array.isArray(args['media']) ? args['media'] : []).filter(
        (media): media is Record<string, unknown> => isObject(media),
      );
      const missingMediaId = updates.find(
        (media) => typeof media['id'] !== 'string' || !effectiveMedia.some((candidate) => candidate.id === media['id']),
      );
      if (missingMediaId) {
        const rawMediaId = missingMediaId['id'];
        const mediaUserError =
          typeof rawMediaId === 'string'
            ? { field: ['media'], message: `Media id ${rawMediaId} does not exist` }
            : { field: ['media', 'id'], message: 'Media id is required' };
        return {
          data: {
            [responseKey]: {
              media: typeof rawMediaId === 'string' ? null : [],
              mediaUserErrors: [mediaUserError],
            },
          },
        };
      }

      const nonReadyMedia = updates.find((media) => {
        const mediaId = typeof media['id'] === 'string' ? media['id'] : null;
        const existingMediaRecord = mediaId
          ? (effectiveMedia.find((candidate) => candidate.id === mediaId) ?? null)
          : null;
        return existingMediaRecord?.status !== 'READY';
      });
      if (nonReadyMedia) {
        const updateIndex = updates.indexOf(nonReadyMedia);
        return {
          data: {
            [responseKey]: {
              media: [],
              mediaUserErrors: [
                { field: ['media', `${updateIndex}`, 'id'], message: 'Non-ready media cannot be updated.' },
              ],
            },
          },
        };
      }

      const updatedMedia: ProductMediaRecord[] = [];
      const nextMedia = effectiveMedia.map((mediaRecord) => {
        const update = updates.find((candidate) => candidate['id'] === mediaRecord.id);
        if (!update) {
          return mediaRecord;
        }

        const nextRecord = updateMediaRecord(mediaRecord, update);
        updatedMedia.push(nextRecord);
        return nextRecord;
      });

      runtime.store.replaceStagedMediaForProduct(productId, nextMedia);
      return {
        data: {
          [responseKey]: {
            media: serializeMediaPayload(updatedMedia, getChildField(field, 'media')),
            mediaUserErrors: [],
          },
        },
      };
    }
    case 'productDeleteMedia': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (productId === '') {
        return buildInvalidProductMediaProductIdVariableError(productId, document);
      }

      if (!productId) {
        return {
          data: {
            [responseKey]: {
              deletedMediaIds: [],
              deletedProductImageIds: [],
              mediaUserErrors: [{ field: ['productId'], message: 'Product id is required' }],
              product: null,
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: mediaValidationProductNotFoundPayload('delete'),
          },
        };
      }

      const mediaIds = Array.isArray(args['mediaIds'])
        ? args['mediaIds'].filter((mediaId): mediaId is string => typeof mediaId === 'string')
        : [];
      const effectiveMedia = runtime.store.getEffectiveMediaByProductId(productId);
      const unknownMediaId = mediaIds.find(
        (mediaId) => !effectiveMedia.some((mediaRecord) => mediaRecord.id === mediaId),
      );
      if (unknownMediaId) {
        return {
          data: {
            [responseKey]: {
              deletedMediaIds: null,
              deletedProductImageIds: null,
              mediaUserErrors: [{ field: ['mediaIds'], message: `Media id ${unknownMediaId} does not exist` }],
              product: serializeProduct(
                runtime,
                runtime.store.getEffectiveProductById(productId),
                getChildField(field, 'product'),
                variables,
              ),
            },
          },
        };
      }

      const deletedMedia = effectiveMedia.filter(
        (mediaRecord) => typeof mediaRecord.id === 'string' && mediaIds.includes(mediaRecord.id),
      );
      const nextMedia = effectiveMedia.filter(
        (mediaRecord) => typeof mediaRecord.id !== 'string' || !mediaIds.includes(mediaRecord.id),
      );
      runtime.store.replaceStagedMediaForProduct(productId, nextMedia);

      const deletedMediaIds = deletedMedia
        .map((mediaRecord) => mediaRecord.id)
        .filter((mediaId): mediaId is string => typeof mediaId === 'string');
      const deletedProductImageIds = deletedMedia
        .map((mediaRecord) => mediaRecord.productImageId)
        .filter((productImageId): productImageId is string => typeof productImageId === 'string');

      return {
        data: {
          [responseKey]: {
            deletedMediaIds,
            deletedProductImageIds,
            mediaUserErrors: [],
            product: serializeProduct(
              runtime,
              runtime.store.getEffectiveProductById(productId),
              getChildField(field, 'product'),
              variables,
            ),
          },
        },
      };
    }
    case 'inventoryTransferCreate':
    case 'inventoryTransferCreateAsReadyToShip': {
      const input = readProductInput(args['input']);
      const status = field.name.value === 'inventoryTransferCreateAsReadyToShip' ? 'READY_TO_SHIP' : 'DRAFT';
      const result = makeInventoryTransferRecord(runtime, input, status);
      let transfer = result.transfer;
      let userErrors = result.userErrors;
      if (transfer && status === 'READY_TO_SHIP') {
        userErrors = applyInventoryTransferReservation(runtime, transfer, 'reserve');
        if (userErrors.length > 0) {
          transfer = null;
        }
      }

      if (transfer) {
        runtime.store.upsertStagedInventoryTransfer(transfer);
      }

      return {
        data: {
          [responseKey]: {
            inventoryTransfer: serializeInventoryTransfer(
              runtime,
              transfer,
              getChildField(field, 'inventoryTransfer'),
              variables,
            ),
            userErrors: serializeInventoryTransferUserErrors(getChildField(field, 'userErrors'), userErrors),
          },
        },
      };
    }
    case 'inventoryTransferEdit': {
      const rawId = args['id'];
      const id = typeof rawId === 'string' ? rawId : null;
      const transfer = id ? runtime.store.getEffectiveInventoryTransferById(id) : null;
      if (!transfer) {
        return { data: inventoryTransferNotFoundPayload(responseKey, field) };
      }

      const input = readProductInput(args['input']);
      const nextTransfer: InventoryTransferRecord = {
        ...transfer,
        referenceName: typeof input['referenceName'] === 'string' ? input['referenceName'] : transfer.referenceName,
        note: typeof input['note'] === 'string' ? input['note'] : transfer.note,
        tags: Array.isArray(input['tags'])
          ? input['tags'].filter((tag): tag is string => typeof tag === 'string')
          : transfer.tags,
        dateCreated: typeof input['dateCreated'] === 'string' ? input['dateCreated'] : transfer.dateCreated,
        origin:
          typeof input['originId'] === 'string'
            ? makeInventoryTransferLocationSnapshot(runtime, input['originId'])
            : transfer.origin,
        destination:
          typeof input['destinationId'] === 'string'
            ? makeInventoryTransferLocationSnapshot(runtime, input['destinationId'])
            : transfer.destination,
      };
      runtime.store.upsertStagedInventoryTransfer(nextTransfer);
      return {
        data: {
          [responseKey]: {
            inventoryTransfer: serializeInventoryTransfer(
              runtime,
              nextTransfer,
              getChildField(field, 'inventoryTransfer'),
              variables,
            ),
            userErrors: serializeInventoryTransferUserErrors(getChildField(field, 'userErrors'), []),
          },
        },
      };
    }
    case 'inventoryTransferSetItems': {
      const input = readProductInput(args['input']);
      const id = typeof input['id'] === 'string' ? input['id'] : null;
      const transfer = id ? runtime.store.getEffectiveInventoryTransferById(id) : null;
      if (!transfer) {
        return { data: inventoryTransferNotFoundPayload(responseKey, field) };
      }

      const lineItemInputs = readInventoryTransferLineItemInputs(input['lineItems']);
      const userErrors = validateInventoryTransferLineItems(runtime, lineItemInputs);
      const priorByItemId = new Map(transfer.lineItems.map((lineItem) => [lineItem.inventoryItemId, lineItem]));
      const updatedLineItems = lineItemInputs
        .map((lineItemInput) => {
          const lineItem = makeInventoryTransferLineItem(runtime, lineItemInput);
          const prior = lineItemInput.inventoryItemId ? priorByItemId.get(lineItemInput.inventoryItemId) : null;
          return lineItem && prior ? { ...lineItem, id: prior.id } : lineItem;
        })
        .filter((lineItem): lineItem is InventoryTransferLineItemRecord => lineItem !== null);
      const updates: InventoryTransferLineItemUpdate[] = updatedLineItems.map((lineItem) => {
        const priorQuantity = priorByItemId.get(lineItem.inventoryItemId)?.totalQuantity ?? 0;
        return {
          inventoryItemId: lineItem.inventoryItemId,
          newQuantity: lineItem.totalQuantity,
          deltaQuantity: lineItem.totalQuantity - priorQuantity,
        };
      });
      const nextTransfer = { ...transfer, lineItems: updatedLineItems };
      if (userErrors.length === 0) {
        runtime.store.upsertStagedInventoryTransfer(nextTransfer);
      }

      return {
        data: {
          [responseKey]: {
            inventoryTransfer: serializeInventoryTransfer(
              runtime,
              userErrors.length === 0 ? nextTransfer : transfer,
              getChildField(field, 'inventoryTransfer'),
              variables,
            ),
            updatedLineItems: serializeInventoryTransferLineItemUpdates(
              getChildField(field, 'updatedLineItems'),
              userErrors.length === 0 ? updates : null,
            ),
            userErrors: serializeInventoryTransferUserErrors(getChildField(field, 'userErrors'), userErrors),
          },
        },
      };
    }
    case 'inventoryTransferRemoveItems': {
      const input = readProductInput(args['input']);
      const id = typeof input['id'] === 'string' ? input['id'] : null;
      const transfer = id ? runtime.store.getEffectiveInventoryTransferById(id) : null;
      if (!transfer) {
        return { data: inventoryTransferNotFoundPayload(responseKey, field) };
      }

      const removeIds = Array.isArray(input['transferLineItemIds'])
        ? input['transferLineItemIds'].filter((value): value is string => typeof value === 'string')
        : [];
      const removedItems = transfer.lineItems.filter((lineItem) => removeIds.includes(lineItem.id));
      const nextTransfer = {
        ...transfer,
        lineItems: transfer.lineItems.filter((lineItem) => !removeIds.includes(lineItem.id)),
      };
      const updates = removedItems.map((lineItem) => ({
        inventoryItemId: lineItem.inventoryItemId,
        newQuantity: 0,
        deltaQuantity: -lineItem.totalQuantity,
      }));
      runtime.store.upsertStagedInventoryTransfer(nextTransfer);
      return {
        data: {
          [responseKey]: {
            inventoryTransfer: serializeInventoryTransfer(
              runtime,
              nextTransfer,
              getChildField(field, 'inventoryTransfer'),
              variables,
            ),
            removedQuantities: serializeInventoryTransferLineItemUpdates(
              getChildField(field, 'removedQuantities'),
              updates,
            ),
            userErrors: serializeInventoryTransferUserErrors(getChildField(field, 'userErrors'), []),
          },
        },
      };
    }
    case 'inventoryTransferMarkAsReadyToShip': {
      const rawId = args['id'];
      const id = typeof rawId === 'string' ? rawId : null;
      const transfer = id ? runtime.store.getEffectiveInventoryTransferById(id) : null;
      if (!transfer) {
        return { data: inventoryTransferNotFoundPayload(responseKey, field) };
      }

      const userErrors =
        transfer.status === 'DRAFT' ? applyInventoryTransferReservation(runtime, transfer, 'reserve') : [];
      const nextTransfer = userErrors.length === 0 ? { ...transfer, status: 'READY_TO_SHIP' as const } : null;
      if (nextTransfer) {
        runtime.store.upsertStagedInventoryTransfer(nextTransfer);
      }
      return {
        data: {
          [responseKey]: {
            inventoryTransfer: serializeInventoryTransfer(
              runtime,
              nextTransfer,
              getChildField(field, 'inventoryTransfer'),
              variables,
            ),
            userErrors: serializeInventoryTransferUserErrors(getChildField(field, 'userErrors'), userErrors),
          },
        },
      };
    }
    case 'inventoryTransferCancel': {
      const rawId = args['id'];
      const id = typeof rawId === 'string' ? rawId : null;
      const transfer = id ? runtime.store.getEffectiveInventoryTransferById(id) : null;
      if (!transfer) {
        return { data: inventoryTransferNotFoundPayload(responseKey, field) };
      }

      if (transfer.status === 'READY_TO_SHIP') {
        applyInventoryTransferReservation(runtime, transfer, 'release');
      }
      const nextTransfer: InventoryTransferRecord = { ...transfer, status: 'CANCELED' };
      runtime.store.upsertStagedInventoryTransfer(nextTransfer);
      return {
        data: {
          [responseKey]: {
            inventoryTransfer: serializeInventoryTransfer(
              runtime,
              nextTransfer,
              getChildField(field, 'inventoryTransfer'),
              variables,
            ),
            userErrors: serializeInventoryTransferUserErrors(getChildField(field, 'userErrors'), []),
          },
        },
      };
    }
    case 'inventoryTransferDuplicate': {
      const rawId = args['id'];
      const id = typeof rawId === 'string' ? rawId : null;
      const transfer = id ? runtime.store.getEffectiveInventoryTransferById(id) : null;
      if (!transfer) {
        return { data: inventoryTransferNotFoundPayload(responseKey, field) };
      }

      const duplicated: InventoryTransferRecord = {
        ...structuredClone(transfer),
        id: runtime.syntheticIdentity.makeSyntheticGid('InventoryTransfer'),
        name: `#T${String(runtime.store.listEffectiveInventoryTransfers().length + 1).padStart(4, '0')}`,
        status: 'DRAFT',
        lineItems: transfer.lineItems.map((lineItem) => ({
          ...lineItem,
          id: runtime.syntheticIdentity.makeProxySyntheticGid('InventoryTransferLineItem'),
        })),
      };
      runtime.store.upsertStagedInventoryTransfer(duplicated);
      return {
        data: {
          [responseKey]: {
            inventoryTransfer: serializeInventoryTransfer(
              runtime,
              duplicated,
              getChildField(field, 'inventoryTransfer'),
              variables,
            ),
            userErrors: serializeInventoryTransferUserErrors(getChildField(field, 'userErrors'), []),
          },
        },
      };
    }
    case 'inventoryTransferDelete': {
      const rawId = args['id'];
      const id = typeof rawId === 'string' ? rawId : null;
      const transfer = id ? runtime.store.getEffectiveInventoryTransferById(id) : null;
      if (!transfer) {
        return { data: inventoryTransferNotFoundPayload(responseKey, field) };
      }

      if (transfer.status !== 'DRAFT') {
        return {
          data: {
            [responseKey]: {
              deletedId: null,
              userErrors: serializeInventoryTransferUserErrors(getChildField(field, 'userErrors'), [
                { field: ['id'], message: "Can't delete the transfer if it's not in the draft status." },
              ]),
            },
          },
        };
      }

      runtime.store.deleteStagedInventoryTransfer(transfer.id);
      return {
        data: {
          [responseKey]: {
            deletedId: transfer.id,
            userErrors: serializeInventoryTransferUserErrors(getChildField(field, 'userErrors'), []),
          },
        },
      };
    }
    case 'inventoryItemUpdate': {
      const rawId = args['id'];
      const inventoryItemId = typeof rawId === 'string' ? rawId : null;
      const existingVariant = inventoryItemId
        ? runtime.store.findEffectiveVariantByInventoryItemId(inventoryItemId)
        : null;
      if (!existingVariant || !existingVariant.inventoryItem) {
        return {
          data: {
            [responseKey]: {
              inventoryItem: null,
              userErrors: [{ field: ['id'], message: "The product couldn't be updated because it does not exist." }],
            },
          },
        };
      }

      const nextVariant: ProductVariantRecord = {
        ...structuredClone(existingVariant),
        inventoryItem: readInventoryItemInput(runtime, args['input'], existingVariant.inventoryItem),
      };
      const productId = existingVariant.productId;
      const nextVariants = runtime.store
        .getEffectiveVariantsByProductId(productId)
        .map((variant) => (variant.id === existingVariant.id ? nextVariant : variant));
      runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);
      syncProductInventorySummary(runtime, productId);
      const updatedVariant = runtime.store.getEffectiveVariantById(existingVariant.id) ?? nextVariant;

      return {
        data: {
          [responseKey]: {
            inventoryItem: serializeInventoryItemSelectionSet(
              runtime,
              updatedVariant,
              getChildField(field, 'inventoryItem')?.selectionSet?.selections ?? [],
              variables,
            ),
            userErrors: [],
          },
        },
      };
    }
    case 'inventoryAdjustQuantities': {
      const input = readProductInput(args['input']);
      if (usesInventoryQuantity202604Contract) {
        const invalid202604Input = validateInventoryAdjust202604Input(input, field, responseKey);
        if (invalid202604Input) {
          return invalid202604Input;
        }

        if (!readIdempotencyKey(field, variables)) {
          return { errors: [buildMissingIdempotencyKeyError(field)], data: { [responseKey]: null } };
        }
      }

      const invalidVariableError = validateInventoryAdjustRequiredFields(input);
      if (invalidVariableError) {
        return invalidVariableError;
      }

      const result = applyInventoryAdjustQuantities(runtime, input, {
        requireChangeFromQuantity: usesInventoryQuantity202604Contract,
      });
      const inventoryAdjustmentGroupField = getChildField(field, 'inventoryAdjustmentGroup');
      const staffMemberField = inventoryAdjustmentGroupField
        ? getChildField(inventoryAdjustmentGroupField, 'staffMember')
        : null;
      const response: Record<string, unknown> = {
        data: {
          [responseKey]: {
            inventoryAdjustmentGroup: serializeInventoryAdjustmentGroup(
              runtime,
              result.group,
              inventoryAdjustmentGroupField,
            ),
            userErrors: result.userErrors,
          },
        },
      };
      if (result.group && inventoryAdjustmentGroupField && staffMemberField) {
        response['errors'] = [
          buildInventoryAdjustmentStaffMemberAccessDeniedError(field, inventoryAdjustmentGroupField, staffMemberField),
        ];
      }

      return response;
    }
    case 'inventorySetQuantities': {
      const input = readProductInput(args['input']);
      if (usesInventoryQuantity202604Contract) {
        const invalid202604Input = validateInventorySet202604Input(input, field, responseKey);
        if (invalid202604Input) {
          return invalid202604Input;
        }

        if (!readIdempotencyKey(field, variables)) {
          return { errors: [buildMissingIdempotencyKeyError(field)], data: { [responseKey]: null } };
        }
      }

      const result = applyInventorySetQuantities(runtime, input, {
        useChangeFromQuantity: usesInventoryQuantity202604Contract,
      });
      return {
        data: {
          [responseKey]: {
            inventoryAdjustmentGroup: serializeInventoryAdjustmentGroup(
              runtime,
              result.group,
              getChildField(field, 'inventoryAdjustmentGroup'),
            ),
            userErrors: serializeInventoryMutationUserErrors(getChildField(field, 'userErrors'), result.userErrors),
          },
        },
      };
    }
    case 'inventoryMoveQuantities': {
      const result = applyInventoryMoveQuantities(runtime, readProductInput(args['input']));
      return {
        data: {
          [responseKey]: {
            inventoryAdjustmentGroup: serializeInventoryAdjustmentGroup(
              runtime,
              result.group,
              getChildField(field, 'inventoryAdjustmentGroup'),
            ),
            userErrors: serializeInventoryMutationUserErrors(getChildField(field, 'userErrors'), result.userErrors),
          },
        },
      };
    }
    case 'inventoryActivate': {
      const rawInventoryItemId = args['inventoryItemId'];
      const rawLocationId = args['locationId'];
      const inventoryItemId = typeof rawInventoryItemId === 'string' ? rawInventoryItemId : null;
      const locationId = typeof rawLocationId === 'string' ? rawLocationId : null;
      const variant = inventoryItemId ? runtime.store.findEffectiveVariantByInventoryItemId(inventoryItemId) : null;
      const knownLocation = locationId ? findKnownLocationById(runtime, locationId) : null;
      const level =
        variant && locationId
          ? (getEffectiveInventoryLevels(runtime, variant).find((candidate) => candidate.location?.id === locationId) ??
            null)
          : null;
      const hasAvailableArg = hasOwnField(args, 'available');
      const userErrors: InventoryMutationUserError[] = [];

      if (variant && locationId && !level && !knownLocation) {
        userErrors.push({
          field: ['locationId'],
          message: "The product couldn't be stocked because the location wasn't found.",
        });
      }

      if (hasAvailableArg && level) {
        userErrors.push({
          field: ['available'],
          message: 'Not allowed to set available quantity when the item is already active at the location.',
        });
      }

      let resolvedVariant = variant;
      let resolvedLevel = level;
      if (variant && knownLocation && !level) {
        const nextLevel = buildActivatedInventoryLevel(runtime, variant, knownLocation);
        if (nextLevel) {
          resolvedVariant = stageVariantInventoryLevels(runtime, variant, [
            ...getEffectiveInventoryLevels(runtime, variant),
            nextLevel,
          ]);
          resolvedLevel =
            getEffectiveInventoryLevels(runtime, resolvedVariant).find(
              (candidate) => candidate.location?.id === knownLocation.id,
            ) ?? nextLevel;
        }
      }

      return {
        data: {
          [responseKey]: {
            inventoryLevel:
              resolvedVariant && resolvedLevel && userErrors.length === 0
                ? serializeInventoryLevelObject(
                    runtime,
                    resolvedVariant,
                    resolvedLevel,
                    getChildField(field, 'inventoryLevel')?.selectionSet?.selections ?? [],
                    variables,
                  )
                : null,
            userErrors: serializeInventoryMutationUserErrors(getChildField(field, 'userErrors'), userErrors),
          },
        },
      };
    }
    case 'inventoryDeactivate': {
      const rawInventoryLevelId = args['inventoryLevelId'];
      const inventoryLevelId = typeof rawInventoryLevelId === 'string' ? rawInventoryLevelId : null;
      const target = inventoryLevelId ? findInventoryLevelTarget(runtime, inventoryLevelId) : null;
      const allLevels = target ? getEffectiveInventoryLevels(runtime, target.variant) : [];
      const userErrors: InventoryMutationUserError[] = [];

      if (target && allLevels.length <= 1) {
        userErrors.push({
          field: null,
          message: `The product couldn't be unstocked from ${target.level.location?.name ?? 'this location'} because products need to be stocked at a minimum of 1 location.`,
        });
      }

      if (target && userErrors.length === 0) {
        stageVariantInventoryLevels(
          runtime,
          target.variant,
          allLevels.filter((candidate) => candidate.id !== target.level.id),
        );
      }

      return {
        data: {
          [responseKey]: {
            userErrors: serializeInventoryMutationUserErrors(getChildField(field, 'userErrors'), userErrors),
          },
        },
      };
    }
    case 'inventoryBulkToggleActivation': {
      const rawInventoryItemId = args['inventoryItemId'];
      const inventoryItemId = typeof rawInventoryItemId === 'string' ? rawInventoryItemId : null;
      const variant = inventoryItemId ? runtime.store.findEffectiveVariantByInventoryItemId(inventoryItemId) : null;
      const updates = Array.isArray(args['inventoryItemUpdates'])
        ? args['inventoryItemUpdates'].filter((value): value is Record<string, unknown> => isObject(value))
        : [];
      const firstUpdate = updates[0] ?? null;
      const locationId = typeof firstUpdate?.['locationId'] === 'string' ? firstUpdate['locationId'] : null;
      const activate = typeof firstUpdate?.['activate'] === 'boolean' ? firstUpdate['activate'] : null;
      const knownLocation = locationId ? findKnownLocationById(runtime, locationId) : null;
      const level =
        variant && locationId
          ? (getEffectiveInventoryLevels(runtime, variant).find((candidate) => candidate.location?.id === locationId) ??
            null)
          : null;
      const userErrors: InventoryMutationUserError[] = [];

      if (variant && locationId && !level && !knownLocation) {
        userErrors.push({
          field: ['inventoryItemUpdates', '0', 'locationId'],
          message: "The quantity couldn't be updated because the location was not found.",
          code: 'LOCATION_NOT_FOUND',
        });
      }

      if (variant && activate === false && level && getEffectiveInventoryLevels(runtime, variant).length <= 1) {
        userErrors.push({
          field: ['inventoryItemUpdates', '0', 'locationId'],
          message: `The variant couldn't be unstocked from ${level.location?.name ?? 'this location'} because products need to be stocked at a minimum of 1 location.`,
          code: 'CANNOT_DEACTIVATE_FROM_ONLY_LOCATION',
        });
      }

      let resolvedVariant = variant;
      let responseLevels: Record<string, unknown>[] | null = null;
      if (variant && userErrors.length === 0 && locationId) {
        if (activate === true && !level && knownLocation) {
          const nextLevel = buildActivatedInventoryLevel(runtime, variant, knownLocation);
          if (nextLevel) {
            resolvedVariant = stageVariantInventoryLevels(runtime, variant, [
              ...getEffectiveInventoryLevels(runtime, variant),
              nextLevel,
            ]);
            const resolvedLevel =
              getEffectiveInventoryLevels(runtime, resolvedVariant).find(
                (candidate) => candidate.location?.id === knownLocation.id,
              ) ?? nextLevel;
            responseLevels = [
              serializeInventoryLevelObject(
                runtime,
                resolvedVariant,
                resolvedLevel,
                getChildField(field, 'inventoryLevels')?.selectionSet?.selections ?? [],
                variables,
              ),
            ];
          }
        } else if (activate === false && level) {
          resolvedVariant = stageVariantInventoryLevels(
            runtime,
            variant,
            getEffectiveInventoryLevels(runtime, variant).filter((candidate) => candidate.id !== level.id),
          );
          responseLevels = [];
        } else if (level) {
          responseLevels = [
            serializeInventoryLevelObject(
              runtime,
              variant,
              level,
              getChildField(field, 'inventoryLevels')?.selectionSet?.selections ?? [],
              variables,
            ),
          ];
        }
      }

      return {
        data: {
          [responseKey]: {
            inventoryItem:
              resolvedVariant && userErrors.length === 0
                ? serializeInventoryItemSelectionSet(
                    runtime,
                    resolvedVariant,
                    getChildField(field, 'inventoryItem')?.selectionSet?.selections ?? [],
                    variables,
                  )
                : null,
            inventoryLevels: userErrors.length === 0 ? responseLevels : null,
            userErrors: serializeInventoryMutationUserErrors(getChildField(field, 'userErrors'), userErrors),
          },
        },
      };
    }
    case 'metafieldsSet': {
      const inputs = readMetafieldInputObjects(args['metafields']);
      const invalidVariableResponse = validateMetafieldsSetRequiredVariables(document, field, inputs);
      if (invalidVariableResponse) {
        return invalidVariableResponse;
      }

      const userErrors = validateMetafieldsSetInputs(runtime, inputs);

      if (userErrors.length > 0) {
        return {
          data: {
            [responseKey]: {
              metafields: userErrors.some((error) => error.code === 'LESS_THAN_OR_EQUAL_TO') ? null : [],
              userErrors: serializeMetafieldsSetUserErrors(getChildField(field, 'userErrors'), userErrors),
            },
          },
        };
      }

      const inputsByOwnerId = new Map<string, Record<string, unknown>[]>();
      for (const input of inputs) {
        const ownerId = input['ownerId'] as string;
        const owner = resolveProductMetafieldOwner(runtime, ownerId);
        if (!owner) {
          continue;
        }
        const ownerInputs = inputsByOwnerId.get(ownerId) ?? [];
        ownerInputs.push(normalizeMetafieldsSetInput(runtime, input, owner));
        inputsByOwnerId.set(ownerId, ownerInputs);
      }

      const createdOrUpdated: ProductMetafieldRecord[] = [];
      for (const [ownerId, ownerInputs] of inputsByOwnerId.entries()) {
        const owner = resolveProductMetafieldOwner(runtime, ownerId);
        if (!owner) {
          continue;
        }

        const updateResult = upsertMetafieldsForOwner(runtime, owner, ownerInputs);
        replaceStagedMetafieldsForOwner(runtime, owner.id, updateResult.metafields);
        createdOrUpdated.push(...updateResult.createdOrUpdated);
      }

      return {
        data: {
          [responseKey]: {
            metafields: serializeMetafieldPayload(createdOrUpdated, getChildField(field, 'metafields')),
            userErrors: [],
          },
        },
      };
    }
    case 'metafieldsDelete': {
      const metafieldsArg = field.arguments?.find((argument) => argument.name.value === 'metafields') ?? null;
      if (metafieldsArg?.value.kind === Kind.VARIABLE) {
        const variableName = metafieldsArg.value.name.value;
        const rawMetafields = variables[variableName];
        const validationError = validateMetafieldsDeleteRequiredFields(rawMetafields);
        if (validationError) {
          return buildMetafieldsDeleteInvalidVariableError(
            Array.isArray(rawMetafields) ? rawMetafields : [],
            validationError.fieldPath,
            validationError.problemPath,
            getVariableDefinitionLocation(document, variableName),
          );
        }
      }

      const deleteResult = deleteMetafieldsByIdentifiers(runtime, readMetafieldInputObjects(args['metafields']));
      return {
        data: {
          [responseKey]: {
            deletedMetafields: serializeDeletedMetafieldIdentifiers(
              deleteResult.deletedMetafields,
              getChildField(field, 'deletedMetafields'),
            ),
            userErrors: deleteResult.userErrors,
          },
        },
      };
    }
    case 'metafieldDelete': {
      const input = readProductInput(args['input']);
      const rawId = input['id'];
      const metafieldId = typeof rawId === 'string' ? rawId : null;
      if (!metafieldId) {
        return {
          data: {
            [responseKey]: {
              deletedId: null,
              userErrors: [{ field: ['input', 'id'], message: 'Metafield id is required' }],
            },
          },
        };
      }

      const existingMetafield = findMetafieldById(runtime, metafieldId);
      if (!existingMetafield) {
        return {
          data: {
            [responseKey]: {
              deletedId: null,
              userErrors: [{ field: ['input', 'id'], message: 'Metafield not found' }],
            },
          },
        };
      }

      const deleteResult = deleteMetafieldsByIdentifiers(runtime, [
        {
          ownerId: getProductMetafieldOwnerId(existingMetafield),
          namespace: existingMetafield.namespace,
          key: existingMetafield.key,
        },
      ]);
      return {
        data: {
          [responseKey]: {
            deletedId: deleteResult.userErrors.length === 0 ? metafieldId : null,
            userErrors: deleteResult.userErrors,
          },
        },
      };
    }
    case 'productVariantCreate': {
      const input = readProductInput(args['input']);
      const rawProductId = input['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              product: null,
              productVariant: null,
              userErrors: [{ field: ['input', 'productId'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              product: null,
              productVariant: null,
              userErrors: [{ field: ['input', 'productId'], message: 'Product not found' }],
            },
          },
        };
      }

      const effectiveVariants = runtime.store.getEffectiveVariantsByProductId(productId);
      const createdVariant = makeCreatedVariantRecord(runtime, productId, input, effectiveVariants[0] ?? null);
      const nextVariants = [...effectiveVariants, createdVariant];
      runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);
      runtime.store.replaceStagedOptionsForProduct(
        productId,
        syncProductOptionsWithVariants(runtime, productId, undefined, nextVariants),
      );
      const product = syncProductInventorySummary(runtime, productId);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(runtime, product, getChildField(field, 'product'), variables),
            productVariant: serializeVariantSelectionSet(
              runtime,
              createdVariant,
              getChildField(field, 'productVariant')?.selectionSet?.selections ?? [],
              variables,
            ),
            userErrors: [],
          },
        },
      };
    }
    case 'productVariantUpdate': {
      const input = readProductInput(args['input']);
      const rawVariantId = input['id'];
      const variantId = typeof rawVariantId === 'string' ? rawVariantId : null;
      if (!variantId) {
        return {
          data: {
            [responseKey]: {
              product: null,
              productVariant: null,
              userErrors: [{ field: ['input', 'id'], message: 'Variant id is required' }],
            },
          },
        };
      }

      const existingVariant = findEffectiveVariantById(runtime, variantId);
      if (!existingVariant) {
        return {
          data: {
            [responseKey]: {
              product: null,
              productVariant: null,
              userErrors: [{ field: ['input', 'id'], message: 'Variant not found' }],
            },
          },
        };
      }

      const productId = existingVariant.productId;
      const updatedVariants: ProductVariantRecord[] = [];
      const nextVariants = runtime.store.getEffectiveVariantsByProductId(productId).map((variant) => {
        if (variant.id !== variantId) {
          return variant;
        }

        const updatedVariant = updateVariantRecord(runtime, variant, input);
        updatedVariants.push(updatedVariant);
        return updatedVariant;
      });
      runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);
      runtime.store.replaceStagedOptionsForProduct(
        productId,
        syncProductOptionsWithVariants(runtime, productId, undefined, nextVariants),
      );
      runtime.store.markVariantSearchLagged(productId);
      const product = syncProductInventorySummary(runtime, productId);
      const updatedVariant = updatedVariants[0] ?? null;
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(runtime, product, getChildField(field, 'product'), variables),
            productVariant: updatedVariant
              ? serializeVariantSelectionSet(
                  runtime,
                  updatedVariant,
                  getChildField(field, 'productVariant')?.selectionSet?.selections ?? [],
                  variables,
                )
              : null,
            userErrors: [],
          },
        },
      };
    }
    case 'productVariantDelete': {
      const rawVariantId = args['id'];
      const variantId = typeof rawVariantId === 'string' ? rawVariantId : null;
      if (!variantId) {
        return {
          data: {
            [responseKey]: {
              deletedProductVariantId: null,
              userErrors: [{ field: ['id'], message: 'Variant id is required' }],
            },
          },
        };
      }

      const existingVariant = findEffectiveVariantById(runtime, variantId);
      if (!existingVariant) {
        return {
          data: {
            [responseKey]: {
              deletedProductVariantId: null,
              userErrors: [{ field: ['id'], message: 'Variant not found' }],
            },
          },
        };
      }

      const productId = existingVariant.productId;
      const nextVariants = runtime.store
        .getEffectiveVariantsByProductId(productId)
        .filter((variant) => variant.id !== variantId);
      runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);
      runtime.store.replaceStagedOptionsForProduct(
        productId,
        syncProductOptionsWithVariants(runtime, productId, undefined, nextVariants),
      );
      syncProductInventorySummary(runtime, productId);
      return {
        data: {
          [responseKey]: {
            deletedProductVariantId: variantId,
            userErrors: [],
          },
        },
      };
    }
    case 'productVariantsBulkCreate': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              product: null,
              productVariants: [],
              userErrors: [{ field: ['productId'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              product: null,
              productVariants: [],
              userErrors: [{ field: ['productId'], message: 'Product does not exist' }],
            },
          },
        };
      }

      const effectiveVariants = runtime.store.getEffectiveVariantsByProductId(productId);
      const defaultVariant = effectiveVariants[0] ?? null;
      const variantInputs = readBulkVariantInputs(args['variants']);
      const userErrors = validateBulkCreateVariantBatch(runtime, productId, variantInputs);
      if (userErrors.length > 0) {
        return {
          data: {
            [responseKey]: {
              product: null,
              productVariants: [],
              userErrors,
            },
          },
        };
      }

      const createdVariants = variantInputs.map((variant) =>
        makeCreatedVariantRecord(runtime, productId, variant, defaultVariant),
      );
      const shouldRemoveStandaloneVariant =
        variantInputs.length > 0 &&
        effectiveVariants.length === 1 &&
        (args['strategy'] === 'REMOVE_STANDALONE_VARIANT' ||
          productHasStandaloneDefaultVariant(
            runtime.store.getEffectiveOptionsByProductId(productId),
            effectiveVariants,
          ));
      const nextVariants = [...(shouldRemoveStandaloneVariant ? [] : effectiveVariants), ...createdVariants];
      runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);
      runtime.store.replaceStagedOptionsForProduct(
        productId,
        syncProductOptionsWithVariants(
          runtime,
          productId,
          shouldRemoveStandaloneVariant ? [] : undefined,
          nextVariants,
        ),
      );
      runtime.store.markVariantSearchLagged(productId);
      const product = syncProductInventorySummary(runtime, productId);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(runtime, product, getChildField(field, 'product'), variables),
            productVariants: serializeVariantPayload(runtime, createdVariants, getChildField(field, 'productVariants')),
            userErrors: [],
          },
        },
      };
    }
    case 'productVariantsBulkUpdate': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              product: null,
              productVariants: [],
              userErrors: [{ field: ['productId'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              product: null,
              productVariants: null,
              userErrors: [{ field: ['productId'], message: 'Product does not exist' }],
            },
          },
        };
      }

      const effectiveVariants = runtime.store.getEffectiveVariantsByProductId(productId);
      const variantsById = new Map(effectiveVariants.map((variant) => [variant.id, variant]));
      const updates = readBulkVariantInputs(args['variants']);
      const userErrors = validateBulkUpdateVariantBatch(runtime, productId, updates, variantsById);
      if (userErrors.length > 0) {
        return {
          data: {
            [responseKey]: {
              product:
                userErrors[0]?.field === null
                  ? null
                  : serializeProduct(runtime, existingProduct, getChildField(field, 'product'), variables),
              productVariants: null,
              userErrors,
            },
          },
        };
      }

      const updatedVariants: ProductVariantRecord[] = [];
      const nextVariants = effectiveVariants.map((variant) => {
        const update = updates.find((candidate) => candidate['id'] === variant.id);
        if (!update) {
          return variant;
        }

        const updatedVariant = updateVariantRecord(runtime, variant, update);
        updatedVariants.push(updatedVariant);
        return updatedVariant;
      });

      runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);
      runtime.store.replaceStagedOptionsForProduct(
        productId,
        syncProductOptionsWithVariants(runtime, productId, undefined, nextVariants),
      );
      runtime.store.markVariantSearchLagged(productId);
      const product = syncProductInventorySummary(runtime, productId);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(runtime, product, getChildField(field, 'product'), variables),
            productVariants: serializeVariantPayload(runtime, updatedVariants, getChildField(field, 'productVariants')),
            userErrors: [],
          },
        },
      };
    }
    case 'productVariantsBulkDelete': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['productId'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['productId'], message: 'Product does not exist' }],
            },
          },
        };
      }

      const variantIds = Array.isArray(args['variantsIds'])
        ? args['variantsIds'].filter((variantId): variantId is string => typeof variantId === 'string')
        : [];
      const effectiveVariants = runtime.store.getEffectiveVariantsByProductId(productId);
      const variantsById = new Map(effectiveVariants.map((variant) => [variant.id, variant]));
      const missingVariantIndex =
        effectiveVariants.length > 0
          ? variantIds.findIndex((variantId) => !variantsById.has(variantId) && isKnownMissingShopifyGid(variantId))
          : -1;
      if (missingVariantIndex >= 0) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [
                {
                  field: ['variantsIds', String(missingVariantIndex)],
                  message: 'At least one variant does not belong to the product',
                },
              ],
            },
          },
        };
      }

      const nextVariants = effectiveVariants.filter((variant) => !variantIds.includes(variant.id));
      runtime.store.replaceStagedVariantsForProduct(productId, nextVariants);
      runtime.store.replaceStagedOptionsForProduct(
        productId,
        syncProductOptionsWithVariants(runtime, productId, undefined, nextVariants),
      );
      runtime.store.markVariantSearchLagged(productId);
      const product = syncProductInventorySummary(runtime, productId);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(runtime, product, getChildField(field, 'product'), variables),
            userErrors: [],
          },
        },
      };
    }
    case 'productVariantsBulkReorder': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['productId'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existingProduct = runtime.store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              product: null,
              userErrors: [{ field: ['productId'], message: 'Product not found' }],
            },
          },
        };
      }

      const result = reorderProductVariants(runtime, productId, args['positions']);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(runtime, result.product, getChildField(field, 'product'), variables),
            userErrors: result.userErrors,
          },
        },
      };
    }
    default:
      throw new Error(`Unsupported product mutation field: ${field.name.value}`);
  }
}

export function handleProductQuery(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
  readMode: ReadMode,
): Record<string, unknown> {
  const fields = getRootFields(document);
  const data: Record<string, unknown> = {};
  const errors: Record<string, unknown>[] = [];

  for (const field of fields) {
    const args = getFieldArguments(field, variables);
    const responseKey = field.alias?.value ?? field.name.value;

    switch (field.name.value) {
      case 'product': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        const product = id ? runtime.store.getEffectiveProductById(id) : null;
        data[responseKey] = serializeProduct(runtime, product, field, variables);
        break;
      }
      case 'productByIdentifier': {
        const identifier = readProductInput(args['identifier']);
        const customId = readProductIdentifierCustomId(identifier);
        if (customId && !getProductCustomIdDefinition(runtime, customId)) {
          data[responseKey] = null;
          errors.push(buildProductCustomIdDefinitionMissingError(field));
          break;
        }

        data[responseKey] = serializeProduct(
          runtime,
          findEffectiveProductByIdentifier(runtime, identifier),
          field,
          variables,
        );
        break;
      }
      case 'products': {
        const rawFirst = args['first'];
        const rawLast = args['last'];
        const first = typeof rawFirst === 'number' ? rawFirst : null;
        const last = typeof rawLast === 'number' ? rawLast : null;
        data[responseKey] = serializeProductsConnection(
          runtime,
          runtime.store.listEffectiveProducts(),
          field,
          first,
          last,
          args['after'],
          args['before'],
          args['query'],
          args['sortKey'],
          args['reverse'],
          variables,
        );
        break;
      }
      case 'productsCount': {
        data[responseKey] = serializeProductsCount(runtime, args['query'], field.selectionSet?.selections ?? []);
        break;
      }
      case 'productFeed': {
        const id = typeof args['id'] === 'string' ? args['id'] : null;
        const productFeed = id ? runtime.store.getEffectiveProductFeedById(id) : null;
        data[responseKey] = productFeed
          ? serializeProductFeedSelectionSet(productFeed, field.selectionSet?.selections ?? [])
          : null;
        break;
      }
      case 'productFeeds': {
        data[responseKey] = serializeTopLevelProductFeedsConnection(runtime, field, variables);
        break;
      }
      case 'productVariant': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        const variant = id ? runtime.store.getEffectiveVariantById(id) : null;
        data[responseKey] = variant
          ? serializeVariantSelectionSet(runtime, variant, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'productVariantByIdentifier': {
        const identifier = readProductInput(args['identifier']);
        const variant = findEffectiveVariantByIdentifier(runtime, identifier);
        data[responseKey] = variant
          ? serializeVariantSelectionSet(runtime, variant, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'productVariants': {
        data[responseKey] = serializeProductVariantsConnection(runtime, field, variables);
        break;
      }
      case 'productVariantsCount': {
        data[responseKey] = serializeProductVariantsCount(runtime, args['query'], field);
        break;
      }
      case 'productTags': {
        data[responseKey] = serializeStringConnection(
          runtime.store.listEffectiveProducts().flatMap((product) => product.tags),
          field,
          variables,
        );
        break;
      }
      case 'productTypes': {
        data[responseKey] = serializeStringConnection(
          runtime.store
            .listEffectiveProducts()
            .map((product) => product.productType)
            .filter((productType): productType is string => typeof productType === 'string'),
          field,
          variables,
        );
        break;
      }
      case 'productVendors': {
        data[responseKey] = serializeStringConnection(
          runtime.store
            .listEffectiveProducts()
            .map((product) => product.vendor)
            .filter((vendor): vendor is string => typeof vendor === 'string'),
          field,
          variables,
        );
        break;
      }
      case 'productSavedSearches': {
        data[responseKey] = serializeEmptySavedSearchConnection(field, variables);
        break;
      }
      case 'productOperation': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        data[responseKey] = serializeProductOperation(
          runtime,
          id ? runtime.store.getEffectiveProductOperationById(id) : null,
          field,
          variables,
        );
        break;
      }
      case 'productDuplicateJob': {
        data[responseKey] = serializeProductDuplicateJob(args['id'], field);
        break;
      }
      case 'productResourceFeedback': {
        const id = typeof args['id'] === 'string' ? args['id'] : null;
        const feedback = id ? runtime.store.getEffectiveProductResourceFeedback(id) : null;
        data[responseKey] = feedback
          ? serializeProductResourceFeedbackSelectionSet(feedback, field.selectionSet?.selections ?? [])
          : null;
        break;
      }
      case 'sellingPlanGroup': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        data[responseKey] = serializeSellingPlanGroup(
          runtime,
          id ? runtime.store.getEffectiveSellingPlanGroupById(id) : null,
          field,
          variables,
        );
        break;
      }
      case 'sellingPlanGroups': {
        data[responseKey] = serializeSellingPlanGroupConnection(
          runtime,
          runtime.store.listEffectiveSellingPlanGroups(),
          field,
          variables,
        );
        break;
      }
      case 'inventoryItem': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        const variant = id ? runtime.store.findEffectiveVariantByInventoryItemId(id) : null;
        data[responseKey] = variant
          ? serializeInventoryItemSelectionSet(runtime, variant, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'inventoryItems': {
        data[responseKey] = serializeInventoryItemsConnection(runtime, field, variables);
        break;
      }
      case 'inventoryLevel': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        const target = id ? findInventoryLevelTarget(runtime, id) : null;
        data[responseKey] = target
          ? serializeInventoryLevelObject(
              runtime,
              target.variant,
              target.level,
              field.selectionSet?.selections ?? [],
              variables,
            )
          : null;
        break;
      }
      case 'inventoryProperties': {
        data[responseKey] = serializeInventoryPropertiesSelectionSet(field.selectionSet?.selections ?? []);
        break;
      }
      case 'inventoryTransfer': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        data[responseKey] = serializeInventoryTransfer(
          runtime,
          id ? runtime.store.getEffectiveInventoryTransferById(id) : null,
          field,
          variables,
        );
        break;
      }
      case 'inventoryTransfers': {
        data[responseKey] = serializeInventoryTransfersConnection(runtime, field, variables);
        break;
      }
      case 'collection': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        const collection = id ? findEffectiveCollectionById(runtime, id) : null;
        data[responseKey] = collection
          ? serializeCollectionObject(runtime, collection, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'collectionByIdentifier': {
        const identifier = readProductInput(args['identifier']);
        const collection = findEffectiveCollectionByIdentifier(runtime, identifier);
        data[responseKey] = collection
          ? serializeCollectionObject(runtime, collection, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'collectionByHandle': {
        const rawHandle = args['handle'];
        const handle = typeof rawHandle === 'string' ? rawHandle : null;
        const collection = handle ? findEffectiveCollectionByHandle(runtime, handle) : null;
        data[responseKey] = collection
          ? serializeCollectionObject(runtime, collection, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'collections': {
        data[responseKey] = serializeTopLevelCollectionsConnection(runtime, field, variables);
        break;
      }
      case 'locations': {
        data[responseKey] = serializeTopLevelLocationsConnection(runtime, field, variables);
        break;
      }
      case 'channel': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        const channel = id ? runtime.store.getEffectiveChannelById(id) : null;
        data[responseKey] = channel
          ? serializeChannelSelectionSet(runtime, channel, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'channels': {
        data[responseKey] = serializeTopLevelChannelsConnection(runtime, field, variables);
        break;
      }
      case 'publication': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        const publication = id ? runtime.store.getEffectivePublicationById(id) : null;
        data[responseKey] = publication
          ? serializePublicationSelectionSet(runtime, publication, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'publications': {
        data[responseKey] = serializeTopLevelPublicationsConnection(runtime, field, variables);
        break;
      }
      case 'publicationsCount': {
        data[responseKey] = serializeCountValue(field, listEffectivePublications(runtime).length);
        break;
      }
      case 'publishedProductsCount': {
        const rawPublicationId = args['publicationId'];
        const publicationId = typeof rawPublicationId === 'string' ? rawPublicationId : null;
        const count = publicationId
          ? listProductsPublishedToPublication(runtime, publicationId).length
          : runtime.store
              .listEffectiveProducts()
              .filter((product) => product.status === 'ACTIVE' && product.publicationIds.length > 0).length;
        data[responseKey] = serializeCountValue(field, count);
        break;
      }
      default:
        if (readMode === 'snapshot') {
          data[responseKey] = null;
          break;
        }
        throw new Error(`Unsupported product query field: ${field.name.value}`);
    }
  }

  return errors.length > 0 ? { errors, data } : { data };
}
