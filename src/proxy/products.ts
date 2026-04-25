import { getLocation, Kind, parse, type ASTNode, type FieldNode, type SelectionNode } from 'graphql';
import type { ReadMode } from '../config.js';
import { getFieldArguments, getRootField, getRootFieldArguments, getRootFields } from '../graphql/root-field.js';
import { parseSearchQuery, type SearchQueryNode, type SearchQueryTerm } from '../search-query-parser.js';
import { paginateConnectionItems, serializeConnectionPageInfo } from './graphql-helpers.js';
import { makeProxySyntheticGid, makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type {
  CollectionImageRecord,
  CollectionRecord,
  CollectionRuleSetRecord,
  InventoryLevelRecord,
  ProductCatalogConnectionRecord,
  ProductCollectionRecord,
  ProductMediaRecord,
  ProductMetafieldRecord,
  ProductOptionRecord,
  ProductRecord,
  ProductVariantRecord,
  PublicationRecord,
} from '../state/types.js';

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function hasOwnField(value: Record<string, unknown>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

type GraphqlErrorLocation = { line: number; column: number };

function getNodeLocation(node: ASTNode): GraphqlErrorLocation[] {
  const token = node.loc?.startToken;
  return token ? [{ line: token.line, column: token.column }] : [];
}

function getVariableDefinitionLocation(document: string, variableName: string): GraphqlErrorLocation[] {
  const ast = parse(document);
  for (const definition of ast.definitions) {
    if (definition.kind !== Kind.OPERATION_DEFINITION) {
      continue;
    }

    const variableDefinition = definition.variableDefinitions?.find(
      (candidate) => candidate.variable.name.value === variableName,
    );
    if (variableDefinition) {
      return getNodeLocation(variableDefinition);
    }
  }

  return [];
}

function getOperationPathLabel(document: string): string {
  const ast = parse(document);
  const operation = ast.definitions.find((definition) => definition.kind === Kind.OPERATION_DEFINITION);
  if (!operation || operation.kind !== Kind.OPERATION_DEFINITION) {
    return 'mutation';
  }

  const operationType = operation.operation;
  return operation.name ? `${operationType} ${operation.name.value}` : operationType;
}

function normalizeHandleParts(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

function slugifyHandle(title: string): string {
  const normalized = normalizeHandleParts(title);
  return normalized || 'untitled-product';
}

function readLegacyResourceIdFromGid(id: string): string | null {
  const tail = id.split('/').at(-1);
  return tail && /^\d+$/u.test(tail) ? tail : null;
}

function stripHtmlToDescription(value: string): string {
  return value
    .replace(/<[^>]*>/gu, '')
    .replace(/\s+/gu, ' ')
    .trim();
}

function normalizeExplicitProductHandle(handle: string): string {
  const normalized = normalizeHandleParts(handle);
  return normalized || 'product';
}

type ExplicitHandleResolution =
  | { kind: 'normalized-explicit'; handle: string }
  | { kind: 'fallback-explicit'; handle: string };

function readProductInput(raw: unknown): Record<string, unknown> {
  return isObject(raw) ? raw : {};
}

function findEffectiveProductByHandle(handle: string): ProductRecord | null {
  return store.listEffectiveProducts().find((product) => product.handle === handle) ?? null;
}

function readExplicitHandle(input: Record<string, unknown>): ExplicitHandleResolution | null {
  const rawHandle = input['handle'];
  if (typeof rawHandle !== 'string') {
    return null;
  }

  const trimmedHandle = rawHandle.trim();
  if (!trimmedHandle) {
    return null;
  }

  const normalized = normalizeHandleParts(trimmedHandle);
  if (!normalized) {
    return { kind: 'fallback-explicit', handle: 'product' };
  }

  return { kind: 'normalized-explicit', handle: normalized };
}

function productHandleInUse(handle: string, excludedProductId?: string): boolean {
  const existing = findEffectiveProductByHandle(handle);
  return Boolean(existing && existing.id !== excludedProductId);
}

function nextProductHandleCandidate(handle: string): string {
  const numericSuffixMatch = handle.match(/^(.*?)(\d+)$/u);
  if (numericSuffixMatch) {
    const prefix = numericSuffixMatch[1] ?? '';
    const numericSuffix = numericSuffixMatch[2] ?? '';
    return `${prefix}${String(Number.parseInt(numericSuffix, 10) + 1)}`;
  }

  const hyphenatedSuffixMatch = handle.match(/^(.*?)-(\d+)$/u);
  if (hyphenatedSuffixMatch) {
    const prefix = hyphenatedSuffixMatch[1] ?? handle;
    const numericSuffix = hyphenatedSuffixMatch[2] ?? '0';
    return `${prefix}-${String(Number.parseInt(numericSuffix, 10) + 1)}`;
  }

  return `${handle}-1`;
}

function ensureUniqueProductHandle(handle: string, excludedProductId?: string): string {
  let candidate = handle;
  while (productHandleInUse(candidate, excludedProductId)) {
    candidate = nextProductHandleCandidate(candidate);
  }

  return candidate;
}

function productHandleConflictError(handle: string): { field: string[]; message: string } {
  return {
    field: ['input', 'handle'],
    message: `Handle '${handle}' already in use. Please provide a new handle.`,
  };
}

function prepareProductInputWithResolvedHandle(
  input: Record<string, unknown>,
  existing?: ProductRecord,
): { input: Record<string, unknown>; error: { field: string[]; message: string } | null } {
  const explicitHandle = readExplicitHandle(input);
  if (explicitHandle) {
    if (explicitHandle.kind === 'normalized-explicit') {
      if (productHandleInUse(explicitHandle.handle, existing?.id)) {
        return { input, error: productHandleConflictError(explicitHandle.handle) };
      }

      return { input: { ...input, handle: explicitHandle.handle }, error: null };
    }

    return {
      input: {
        ...input,
        handle: ensureUniqueProductHandle(explicitHandle.handle, existing?.id),
      },
      error: null,
    };
  }

  const rawId = input['id'];
  const isSparseUpdate = typeof rawId === 'string' && !existing;
  if (isSparseUpdate) {
    return {
      input: {
        ...input,
        handle: '',
      },
      error: null,
    };
  }

  const rawTitle = input['title'];
  const title =
    typeof rawTitle === 'string' && rawTitle.trim() ? rawTitle.trim() : (existing?.title ?? 'Untitled product');
  const baseHandle = existing?.handle ?? slugifyHandle(title);
  return {
    input: {
      ...input,
      handle: ensureUniqueProductHandle(baseHandle, existing?.id),
    },
    error: null,
  };
}

function findEffectiveCollectionById(collectionId: string): CollectionRecord | null {
  return store.getEffectiveCollectionById(collectionId);
}

function findEffectiveCollectionByHandle(handle: string): CollectionRecord | null {
  return listEffectiveCollections().find((collection) => collection.handle === handle) ?? null;
}

function findEffectiveCollectionByIdentifier(identifier: Record<string, unknown>): CollectionRecord | null {
  const rawId = identifier['id'];
  if (typeof rawId === 'string') {
    return findEffectiveCollectionById(rawId);
  }

  const rawHandle = identifier['handle'];
  if (typeof rawHandle === 'string') {
    return findEffectiveCollectionByHandle(rawHandle);
  }

  return null;
}

function listEffectiveCollections(): CollectionRecord[] {
  return store.listEffectiveCollections();
}

interface LocationRecord {
  id: string;
  name: string | null;
}

function listEffectiveLocations(): LocationRecord[] {
  const locations: LocationRecord[] = [];
  const seenLocationIds = new Set<string>();

  for (const product of store.listEffectiveProducts()) {
    for (const variant of store.getEffectiveVariantsByProductId(product.id)) {
      for (const level of getEffectiveInventoryLevels(variant)) {
        const locationId = level.location?.id;
        if (!locationId || seenLocationIds.has(locationId)) {
          continue;
        }

        seenLocationIds.add(locationId);
        locations.push({
          id: locationId,
          name: level.location?.name ?? null,
        });
      }
    }
  }

  return locations;
}

function listEffectivePublications(): PublicationRecord[] {
  return store.listEffectivePublications();
}

function listEffectiveProductsForCollection(collectionId: string): ProductRecord[] {
  return store
    .listEffectiveProducts()
    .map((product) => ({
      product,
      membership:
        store.getEffectiveCollectionsByProductId(product.id).find((collection) => collection.id === collectionId) ??
        null,
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

function listEffectiveCollectionMembershipEntries(collectionId: string): CollectionMembershipEntry[] {
  return store
    .listEffectiveProducts()
    .map((product) => ({
      product,
      membership:
        store.getEffectiveCollectionsByProductId(product.id).find((collection) => collection.id === collectionId) ??
        null,
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

function reorderCollectionProducts(
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
  const orderedEntries = listEffectiveCollectionMembershipEntries(collection.id);
  const productIdsInCollection = new Set(orderedEntries.map((entry) => entry.product.id));

  for (const [index, move] of moves.entries()) {
    if (!store.getEffectiveProductById(move.id)) {
      userErrors.push({ field: ['moves', `${index}`, 'id'], message: 'Product does not exist' });
    } else if (!productIdsInCollection.has(move.id)) {
      userErrors.push({ field: ['moves', `${index}`, 'id'], message: 'Product is not in the collection' });
    }
  }

  if (userErrors.length > 0) {
    return { job: null, userErrors };
  }

  for (const move of moves) {
    const currentIndex = orderedEntries.findIndex((entry) => entry.product.id === move.id);
    if (currentIndex < 0) {
      continue;
    }

    const [entry] = orderedEntries.splice(currentIndex, 1);
    if (!entry) {
      continue;
    }

    const nextIndex = Math.min(move.newPosition, orderedEntries.length);
    orderedEntries.splice(nextIndex, 0, entry);
  }

  for (const [position, entry] of orderedEntries.entries()) {
    const nextCollections = store.getEffectiveCollectionsByProductId(entry.product.id).map((membership) =>
      membership.id === collection.id
        ? {
            ...membership,
            position,
          }
        : membership,
    );
    store.replaceStagedCollectionsForProduct(entry.product.id, nextCollections);
  }

  return {
    job: { id: makeSyntheticGid('Job'), done: false },
    userErrors: [],
  };
}

function addProductsToCollection(
  collection: CollectionRecord,
  productIds: string[],
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
    store.getEffectiveCollectionsByProductId(productId).some((candidate) => candidate.id === collection.id),
  );
  if (duplicateMembership) {
    return {
      collection: null,
      userErrors: [{ field: ['productIds'], message: 'Product is already in the collection' }],
    };
  }

  const existingProductIds = normalizedProductIds.filter((productId) => store.getEffectiveProductById(productId));
  if (existingProductIds.length === 0) {
    return {
      collection,
      userErrors: [],
    };
  }

  const existingPositions = store
    .listEffectiveProducts()
    .flatMap((product) => store.getEffectiveCollectionsByProductId(product.id))
    .filter((candidate) => candidate.id === collection.id)
    .map((candidate) => candidate.position)
    .filter((position): position is number => typeof position === 'number' && Number.isFinite(position));
  const firstPosition = existingPositions.length > 0 ? Math.max(...existingPositions) + 1 : 0;
  const addedCount = existingProductIds.length;

  for (const [index, productId] of existingProductIds.entries()) {
    const nextCollections = [
      ...store.getEffectiveCollectionsByProductId(productId),
      makeProductCollectionRecord(productId, collection, firstPosition + index),
    ];
    store.replaceStagedCollectionsForProduct(productId, nextCollections);
  }

  return {
    collection,
    userErrors: [],
  };
}

function removeProductsFromCollection(collection: CollectionRecord, productIds: string[]): void {
  const normalizedProductIds = productIds.filter((productId, index) => productIds.indexOf(productId) === index);

  for (const productId of normalizedProductIds) {
    const existingProduct = store.getEffectiveProductById(productId);
    if (!existingProduct) {
      continue;
    }

    const nextCollections = store
      .getEffectiveCollectionsByProductId(productId)
      .filter((candidate) => candidate.id !== collection.id);
    store.replaceStagedCollectionsForProduct(productId, nextCollections);
  }
}

function serializeJobSelectionSet(
  job: { id: string; done: boolean },
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'id':
        result[key] = job.id;
        break;
      case 'done':
        result[key] = job.done;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function readProductSetInventoryQuantity(raw: unknown): number | null {
  if (typeof raw === 'number' && Number.isFinite(raw)) {
    return Math.floor(raw);
  }

  if (!Array.isArray(raw)) {
    return null;
  }

  const quantities = raw
    .filter((value): value is Record<string, unknown> => isObject(value))
    .map((value) => value['quantity'])
    .filter((value): value is number => typeof value === 'number' && Number.isFinite(value));

  if (quantities.length === 0) {
    return null;
  }

  return quantities.reduce((total, quantity) => total + Math.floor(quantity), 0);
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

function getPublishableProductId(rawId: unknown): string | null {
  return typeof rawId === 'string' && rawId.startsWith('gid://shopify/Product/') ? rawId : null;
}

function serializeCountValue(field: FieldNode, count: number): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
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

function makeProductRecord(input: Record<string, unknown>, existing?: ProductRecord): ProductRecord {
  const rawTitle = input['title'];
  const title = typeof rawTitle === 'string' && rawTitle.trim() ? rawTitle : (existing?.title ?? 'Untitled product');
  const now = makeSyntheticTimestamp();
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

  const isSparseUpdate = typeof rawId === 'string' && !existing;
  const existingSeo = existing?.seo ?? { title: null, description: null };

  return {
    id: typeof rawId === 'string' ? rawId : (existing?.id ?? makeProxySyntheticGid('Product')),
    legacyResourceId: existing?.legacyResourceId ?? null,
    title,
    handle:
      typeof rawHandle === 'string' && rawHandle.trim()
        ? rawHandle
        : (existing?.handle ?? (isSparseUpdate ? '' : slugifyHandle(title))),
    status: readStatus(rawStatus, existing?.status ?? 'ACTIVE'),
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

function makeCollectionRecord(input: Record<string, unknown>, existing?: CollectionRecord): CollectionRecord {
  const rawTitle = input['title'];
  const title = typeof rawTitle === 'string' && rawTitle.trim() ? rawTitle : (existing?.title ?? 'Untitled collection');
  const now = makeSyntheticTimestamp();
  const rawId = input['id'];
  const id = typeof rawId === 'string' ? rawId : (existing?.id ?? makeSyntheticGid('Collection'));
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

function makeDefaultInventoryItemRecord(): NonNullable<ProductVariantRecord['inventoryItem']> {
  return {
    id: makeSyntheticGid('InventoryItem'),
    tracked: false,
    requiresShipping: true,
    measurement: null,
    countryCodeOfOrigin: null,
    provinceCodeOfOrigin: null,
    harmonizedSystemCode: null,
    inventoryLevels: null,
  };
}

function makeDefaultVariantRecord(product: ProductRecord): ProductVariantRecord {
  return {
    id: makeSyntheticGid('ProductVariant'),
    productId: product.id,
    title: 'Default Title',
    sku: null,
    barcode: null,
    price: null,
    compareAtPrice: null,
    taxable: null,
    inventoryPolicy: null,
    inventoryQuantity: 0,
    selectedOptions: [],
    inventoryItem: makeDefaultInventoryItemRecord(),
  };
}

function makeDefaultOptionRecord(product: ProductRecord): ProductOptionRecord {
  return {
    id: makeSyntheticGid('ProductOption'),
    productId: product.id,
    name: 'Title',
    position: 1,
    optionValues: [
      {
        id: makeSyntheticGid('ProductOptionValue'),
        name: 'Default Title',
        hasVariants: true,
      },
    ],
  };
}

function makeDuplicatedProductRecord(source: ProductRecord, newTitle?: string): ProductRecord {
  const title = typeof newTitle === 'string' && newTitle.trim() ? newTitle : `Copy of ${source.title}`;
  const now = makeSyntheticTimestamp();

  return {
    id: makeProxySyntheticGid('Product'),
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

function duplicateVariantRecord(variant: ProductVariantRecord, productId: string): ProductVariantRecord {
  return {
    id: makeSyntheticGid('ProductVariant'),
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
          id: makeSyntheticGid('InventoryItem'),
        }
      : null,
  };
}

function duplicateOptionRecord(option: ProductOptionRecord, productId: string): ProductOptionRecord {
  return {
    id: makeSyntheticGid('ProductOption'),
    productId,
    name: option.name,
    position: option.position,
    optionValues: option.optionValues.map((optionValue) => ({
      id: makeSyntheticGid('ProductOptionValue'),
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

function makeSyntheticMediaId(mediaContentType: string | null | undefined): string {
  if (mediaContentType === 'IMAGE') {
    return makeSyntheticGid('MediaImage');
  }

  return makeSyntheticGid('Media');
}

function makeSyntheticProductImageId(mediaContentType: string | null | undefined): string | null {
  if (mediaContentType === 'IMAGE') {
    return makeSyntheticGid('ProductImage');
  }

  return null;
}

function duplicateMetafieldRecord(metafield: ProductMetafieldRecord, productId: string): ProductMetafieldRecord {
  return {
    id: makeSyntheticGid('Metafield'),
    productId,
    namespace: metafield.namespace,
    key: metafield.key,
    type: metafield.type,
    value: metafield.value,
  };
}

function normalizeOptionPositions(options: ProductOptionRecord[]): ProductOptionRecord[] {
  return options.map((option, index) => ({
    ...structuredClone(option),
    position: index + 1,
  }));
}

function makeCreatedMediaRecord(
  productId: string,
  input: Record<string, unknown>,
  position: number,
): ProductMediaRecord {
  const rawMediaContentType = input['mediaContentType'];
  const mediaContentType = typeof rawMediaContentType === 'string' ? rawMediaContentType : 'IMAGE';
  const rawAlt = input['alt'];
  const rawOriginalSource = input['originalSource'];
  const sourceUrl = typeof rawOriginalSource === 'string' && rawOriginalSource.trim() ? rawOriginalSource : null;

  return {
    key: `${productId}:media:${position}`,
    productId,
    position,
    id: makeSyntheticMediaId(mediaContentType),
    mediaContentType,
    alt: typeof rawAlt === 'string' ? rawAlt : null,
    status: 'UPLOADED',
    productImageId: makeSyntheticProductImageId(mediaContentType),
    imageUrl: null,
    imageWidth: null,
    imageHeight: null,
    previewImageUrl: null,
    sourceUrl,
  };
}

function transitionMediaToProcessing(media: ProductMediaRecord): ProductMediaRecord {
  return {
    ...structuredClone(media),
    status: 'PROCESSING',
    imageUrl: null,
    imageWidth: null,
    imageHeight: null,
    previewImageUrl: null,
  };
}

function transitionMediaToReady(media: ProductMediaRecord): ProductMediaRecord {
  const readyUrl = media.sourceUrl ?? media.imageUrl ?? media.previewImageUrl ?? null;
  return {
    ...structuredClone(media),
    status: 'READY',
    imageUrl: readyUrl,
    previewImageUrl: readyUrl,
  };
}

function updateMediaRecord(existing: ProductMediaRecord, input: Record<string, unknown>): ProductMediaRecord {
  const rawAlt = input['alt'];
  const rawPreviewImageSource = input['previewImageSource'];
  const rawOriginalSource = input['originalSource'];
  const nextImageUrl =
    typeof rawPreviewImageSource === 'string' && rawPreviewImageSource.trim()
      ? rawPreviewImageSource
      : typeof rawOriginalSource === 'string' && rawOriginalSource.trim()
        ? rawOriginalSource
        : (existing.imageUrl ?? existing.previewImageUrl ?? existing.sourceUrl ?? null);

  return {
    ...structuredClone(existing),
    alt: typeof rawAlt === 'string' ? rawAlt : existing.alt,
    status: 'READY',
    imageUrl: nextImageUrl,
    imageWidth: existing.imageWidth ?? null,
    imageHeight: existing.imageHeight ?? null,
    previewImageUrl: nextImageUrl,
    sourceUrl: existing.sourceUrl ?? nextImageUrl,
  };
}

function readOptionValueCreateInput(raw: unknown): ProductOptionRecord['optionValues'][number] | null {
  if (!isObject(raw)) {
    return null;
  }

  const rawName = raw['name'];
  if (typeof rawName !== 'string' || !rawName.trim()) {
    return null;
  }

  return {
    id: makeSyntheticGid('ProductOptionValue'),
    name: rawName,
    hasVariants: false,
  };
}

function readOptionValueCreateInputs(raw: unknown): ProductOptionRecord['optionValues'] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .map((value) => readOptionValueCreateInput(value))
    .filter((value): value is ProductOptionRecord['optionValues'][number] => value !== null);
}

function makeCreatedOptionRecord(productId: string, input: Record<string, unknown>): ProductOptionRecord {
  const rawName = input['name'];
  const optionValues = readOptionValueCreateInputs(input['values']);

  return {
    id: makeSyntheticGid('ProductOption'),
    productId,
    name: typeof rawName === 'string' && rawName.trim() ? rawName : 'Option',
    position: 0,
    optionValues,
  };
}

function insertOptionAtPosition(
  options: ProductOptionRecord[],
  option: ProductOptionRecord,
  rawPosition: unknown,
): ProductOptionRecord[] {
  const nextOptions = options.map((existingOption) => structuredClone(existingOption));
  const normalizedPosition =
    typeof rawPosition === 'number' && Number.isInteger(rawPosition) && rawPosition > 0
      ? Math.min(rawPosition, nextOptions.length + 1)
      : nextOptions.length + 1;

  nextOptions.splice(normalizedPosition - 1, 0, structuredClone(option));
  return normalizeOptionPositions(nextOptions);
}

function productUsesOnlyDefaultOptionState(options: ProductOptionRecord[], variants: ProductVariantRecord[]): boolean {
  return (
    options.length === 1 &&
    options[0]?.name === 'Title' &&
    options[0]?.optionValues.length === 1 &&
    options[0]?.optionValues[0]?.name === 'Default Title' &&
    variants.length === 1 &&
    variants[0]?.selectedOptions.length === 0
  );
}

function remapDefaultVariantToCreatedOptions(
  variant: ProductVariantRecord,
  options: ProductOptionRecord[],
): ProductVariantRecord {
  const selectedOptions = options
    .map((option) => {
      const firstValue = option.optionValues[0]?.name;
      if (typeof firstValue !== 'string' || !firstValue.trim()) {
        return null;
      }
      return {
        name: option.name,
        value: firstValue,
      };
    })
    .filter((value): value is ProductVariantRecord['selectedOptions'][number] => value !== null);

  return {
    ...structuredClone(variant),
    title: deriveVariantTitle(null, selectedOptions, 'Default Title'),
    selectedOptions,
  };
}

function restoreDefaultOptionState(
  product: ProductRecord,
  variants: ProductVariantRecord[],
): {
  options: ProductOptionRecord[];
  variants: ProductVariantRecord[];
} {
  const baseVariant = variants[0] ? structuredClone(variants[0]) : makeDefaultVariantRecord(product);
  return {
    options: [makeDefaultOptionRecord(product)],
    variants: [
      {
        ...baseVariant,
        productId: product.id,
        title: 'Default Title',
        selectedOptions: [],
      },
    ],
  };
}

function remapVariantSelectionsForOptionUpdate(
  variants: ProductVariantRecord[],
  previousOptionName: string,
  nextOptionName: string,
  renamedValues: Map<string, string>,
): ProductVariantRecord[] {
  return variants.map((variant) => {
    const selectedOptions = variant.selectedOptions.map((selectedOption) => {
      if (selectedOption.name !== previousOptionName) {
        return selectedOption;
      }
      return {
        name: nextOptionName,
        value: renamedValues.get(selectedOption.value) ?? selectedOption.value,
      };
    });

    return {
      ...structuredClone(variant),
      title: deriveVariantTitle(null, selectedOptions, variant.title),
      selectedOptions,
    };
  });
}

function updateOptionRecords(
  productId: string,
  options: ProductOptionRecord[],
  variants: ProductVariantRecord[],
  optionInput: Record<string, unknown>,
  optionValuesToAddRaw: unknown,
  optionValuesToUpdateRaw: unknown,
  optionValuesToDeleteRaw: unknown,
): { options: ProductOptionRecord[]; variants: ProductVariantRecord[] } | null {
  const rawOptionId = optionInput['id'];
  if (typeof rawOptionId !== 'string') {
    return null;
  }

  const existingIndex = options.findIndex((option) => option.id === rawOptionId && option.productId === productId);
  if (existingIndex < 0) {
    return null;
  }

  const nextOptions = options.map((option) => structuredClone(option));
  const existingTarget = nextOptions[existingIndex];
  if (!existingTarget) {
    return null;
  }

  const target = structuredClone(existingTarget);
  const previousOptionName = existingTarget.name;
  const renamedValues = new Map<string, string>();
  const rawName = optionInput['name'];
  if (typeof rawName === 'string' && rawName.trim()) {
    target.name = rawName;
  }

  const deleteIds = Array.isArray(optionValuesToDeleteRaw)
    ? optionValuesToDeleteRaw.filter((value): value is string => typeof value === 'string')
    : [];
  if (deleteIds.length > 0) {
    target.optionValues = target.optionValues.filter((value) => !deleteIds.includes(value.id));
  }

  if (Array.isArray(optionValuesToUpdateRaw)) {
    for (const rawValue of optionValuesToUpdateRaw) {
      if (!isObject(rawValue)) {
        continue;
      }

      const optionValueId = rawValue['id'];
      const optionValueName = rawValue['name'];
      if (typeof optionValueId !== 'string' || typeof optionValueName !== 'string' || !optionValueName.trim()) {
        continue;
      }

      const existingValue = target.optionValues.find((optionValue) => optionValue.id === optionValueId);
      if (existingValue) {
        renamedValues.set(existingValue.name, optionValueName);
        existingValue.name = optionValueName;
      }
    }
  }

  const optionValuesToAdd = readOptionValueCreateInputs(optionValuesToAddRaw);
  if (optionValuesToAdd.length > 0) {
    target.optionValues = [...target.optionValues, ...optionValuesToAdd];
  }

  nextOptions.splice(existingIndex, 1);
  return {
    options: insertOptionAtPosition(nextOptions, target, optionInput['position']),
    variants: remapVariantSelectionsForOptionUpdate(variants, previousOptionName, target.name, renamedValues),
  };
}

function deleteOptionRecords(
  productId: string,
  options: ProductOptionRecord[],
  rawOptionIds: unknown,
): { options: ProductOptionRecord[]; deletedOptionIds: string[] } {
  const optionIds = Array.isArray(rawOptionIds)
    ? rawOptionIds.filter((value): value is string => typeof value === 'string')
    : [];
  const deletedOptionIds = options
    .filter((option) => option.productId === productId && optionIds.includes(option.id))
    .map((option) => option.id);
  const nextOptions = options.filter((option) => !(option.productId === productId && optionIds.includes(option.id)));

  return {
    options: normalizeOptionPositions(nextOptions),
    deletedOptionIds,
  };
}

function buildProductSetOptionRecords(productId: string, rawOptions: unknown): ProductOptionRecord[] {
  const existingOptions = store.getEffectiveOptionsByProductId(productId);
  const existingOptionsById = new Map(existingOptions.map((option) => [option.id, option]));

  if (!Array.isArray(rawOptions)) {
    return [];
  }

  return normalizeOptionPositions(
    rawOptions
      .filter((value): value is Record<string, unknown> => isObject(value))
      .map((value, index) => {
        const rawId = value['id'];
        const existing = typeof rawId === 'string' ? (existingOptionsById.get(rawId) ?? null) : null;
        const created = makeCreatedOptionRecord(productId, value);
        const optionValuesInput = Array.isArray(value['values']) ? value['values'] : [];
        const existingValuesById = new Map(
          (existing?.optionValues ?? []).map((optionValue) => [optionValue.id, optionValue]),
        );
        const optionValues = optionValuesInput
          .filter((entry): entry is Record<string, unknown> => isObject(entry))
          .map((entry) => {
            const rawValueId = entry['id'];
            const rawValueName = entry['name'];
            const existingValue = typeof rawValueId === 'string' ? (existingValuesById.get(rawValueId) ?? null) : null;
            return {
              id: existingValue?.id ?? makeSyntheticGid('ProductOptionValue'),
              name:
                typeof rawValueName === 'string' && rawValueName.trim()
                  ? rawValueName
                  : (existingValue?.name ?? 'Option value'),
              hasVariants: existingValue?.hasVariants ?? false,
            };
          });

        return {
          id: existing?.id ?? created.id,
          productId,
          name:
            typeof value['name'] === 'string' && value['name'].trim()
              ? value['name']
              : (existing?.name ?? created.name),
          position: typeof value['position'] === 'number' ? value['position'] : index + 1,
          optionValues,
        };
      }),
  );
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

function readVariantOptionsArrayInput(raw: unknown, productId: string): ProductVariantRecord['selectedOptions'] {
  if (!Array.isArray(raw)) {
    return [];
  }

  const optionValues = raw.filter((value): value is string => typeof value === 'string' && value.trim().length > 0);
  const options = store.getEffectiveOptionsByProductId(productId);
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
    return readVariantOptionsArrayInput(input['options'], productId);
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

const DEFAULT_INVENTORY_LEVEL_LOCATION_ID = 'gid://shopify/Location/1';

function buildStableSyntheticInventoryLevelId(inventoryItemId: string, locationId: string): string {
  const inventoryItemTail = inventoryItemId.split('/').at(-1) ?? encodeURIComponent(inventoryItemId);
  const locationTail = locationId.split('/').at(-1) ?? encodeURIComponent(locationId);

  return `gid://shopify/InventoryLevel/${inventoryItemTail}-${locationTail}?inventory_item_id=${encodeURIComponent(
    inventoryItemId,
  )}`;
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
    id: current?.id ?? makeSyntheticGid('InventoryItem'),
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

function deriveVariantTitle(
  rawTitle: unknown,
  selectedOptions: ProductVariantRecord['selectedOptions'],
  fallbackTitle: string,
): string {
  if (typeof rawTitle === 'string' && rawTitle.trim()) {
    return rawTitle;
  }

  const selectedOptionTitle = selectedOptions
    .map((selectedOption) => selectedOption.value)
    .join(' / ')
    .trim();
  return selectedOptionTitle || fallbackTitle;
}

function makeCreatedVariantRecord(
  productId: string,
  input: Record<string, unknown>,
  defaults: ProductVariantRecord | null = null,
): ProductVariantRecord {
  const selectedOptions = readVariantSelectedOptions(input, productId);
  return {
    id: makeSyntheticGid('ProductVariant'),
    productId,
    title: deriveVariantTitle(input['title'], selectedOptions, 'Default Title'),
    sku: readVariantSku(input, null),
    barcode: typeof input['barcode'] === 'string' ? input['barcode'] : null,
    price: typeof input['price'] === 'string' ? input['price'] : null,
    compareAtPrice:
      typeof input['compareAtPrice'] === 'string' ? input['compareAtPrice'] : (defaults?.compareAtPrice ?? null),
    taxable: typeof input['taxable'] === 'boolean' ? input['taxable'] : (defaults?.taxable ?? null),
    inventoryPolicy:
      typeof input['inventoryPolicy'] === 'string' ? input['inventoryPolicy'] : (defaults?.inventoryPolicy ?? null),
    inventoryQuantity: readVariantInventoryQuantity(input, 0),
    selectedOptions,
    inventoryItem: readInventoryItemInput(input['inventoryItem'], null),
  };
}

function makeCreatedProductSetVariantRecord(productId: string, input: Record<string, unknown>): ProductVariantRecord {
  const variant = makeCreatedVariantRecord(productId, input);

  return {
    ...variant,
    taxable: variant.taxable ?? true,
    inventoryPolicy: variant.inventoryPolicy ?? 'DENY',
    inventoryItem: variant.inventoryItem
      ? {
          ...variant.inventoryItem,
          measurement: variant.inventoryItem.measurement ?? makeDefaultInventoryItemMeasurement(),
        }
      : null,
  };
}

function updateVariantRecord(existing: ProductVariantRecord, input: Record<string, unknown>): ProductVariantRecord {
  const selectedOptions = readVariantSelectedOptions(input, existing.productId, existing.selectedOptions);

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
      ? readInventoryItemInput(input['inventoryItem'], existing.inventoryItem)
      : structuredClone(existing.inventoryItem),
  };
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
  productId: string,
  options: ProductOptionRecord[] = store.getEffectiveOptionsByProductId(productId),
  variants: ProductVariantRecord[] = store.getEffectiveVariantsByProductId(productId),
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
          id: makeSyntheticGid('ProductOption'),
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
          id: makeSyntheticGid('ProductOptionValue'),
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

function syncProductInventorySummary(productId: string): ProductRecord | null {
  const existingProduct = store.getEffectiveProductById(productId);
  if (!existingProduct) {
    return null;
  }

  const effectiveVariants = store.getEffectiveVariantsByProductId(productId);
  const nextProduct: ProductRecord = {
    ...structuredClone(existingProduct),
    updatedAt: makeSyntheticTimestamp(),
    totalInventory: sumVariantInventory(effectiveVariants),
    tracksInventory: deriveTracksInventory(effectiveVariants),
  };

  store.stageUpdateProduct(nextProduct);
  return store.getEffectiveProductById(productId);
}

interface InventoryAdjustmentChangeInputRecord {
  inventoryItemId: string | null;
  locationId: string | null;
  ledgerDocumentUri: string | null;
  delta: number | null;
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

const INVENTORY_ADJUSTMENT_STAFF_MEMBER_REQUIRED_ACCESS =
  '`read_users` access scope. Also: The app must be a finance embedded app or installed on a Shopify Plus or Advanced store. Contact Shopify Support to enable this scope for your app.';

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
    };
  });
}

function serializeInventoryAdjustmentGroup(
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
          const variant = store.findEffectiveVariantByInventoryItemId(change.inventoryItemId);
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
                  ? serializeInventoryItemSelectionSet(variant, changeSelection.selectionSet?.selections ?? [])
                  : null;
                break;
              case 'location': {
                const location = change.locationId ? findKnownLocationById(change.locationId) : null;
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

function applyInventoryAdjustQuantities(
  input: Record<string, unknown>,
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

    const variant = store.findEffectiveVariantByInventoryItemId(change.inventoryItemId);
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
      store.getEffectiveVariantsByProductId(variant.productId).map((candidate) => structuredClone(candidate));
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
        : buildSyntheticInventoryLevels({ ...existingVariant, inventoryItem: nextInventoryItem });
    const levelIndex = nextLevels.findIndex((level) => level.location?.id === change.locationId);
    const targetLevel =
      levelIndex >= 0
        ? nextLevels[levelIndex]!
        : (buildSyntheticInventoryLevel(
            { ...existingVariant, inventoryItem: nextInventoryItem },
            {
              locationId: change.locationId,
              availableQuantity:
                change.locationId === DEFAULT_INVENTORY_LEVEL_LOCATION_ID
                  ? (existingVariant.inventoryQuantity ?? 0)
                  : 0,
            },
          ) ?? {
            id: `${makeSyntheticGid('InventoryLevel')}?inventory_item_id=${encodeURIComponent(change.inventoryItemId)}`,
            cursor: null,
            location: change.locationId
              ? { id: change.locationId, name: null }
              : { id: DEFAULT_INVENTORY_LEVEL_LOCATION_ID, name: null },
            quantities: [],
          });
    const nextQuantities = targetLevel.quantities.map((quantity) => structuredClone(quantity));
    const quantityIndex = nextQuantities.findIndex((quantity) => quantity.name === name);
    if (quantityIndex >= 0) {
      nextQuantities[quantityIndex] = {
        ...nextQuantities[quantityIndex]!,
        quantity: (nextQuantities[quantityIndex]!.quantity ?? 0) + change.delta,
        updatedAt: makeSyntheticTimestamp(),
      };
    } else {
      nextQuantities.push({
        name,
        quantity: change.delta,
        updatedAt: makeSyntheticTimestamp(),
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
    store.replaceStagedVariantsForProduct(productId, nextVariants);
  }

  adjustedChanges.push(...mirroredOnHandChanges);

  return {
    group: {
      id: makeSyntheticGid('InventoryAdjustmentGroup'),
      createdAt: makeSyntheticTimestamp(),
      reason,
      referenceDocumentUri: typeof input['referenceDocumentUri'] === 'string' ? input['referenceDocumentUri'] : null,
      app: buildInventoryAdjustmentAppRecord(),
      changes: adjustedChanges,
    },
    userErrors: [],
  };
}

function findInventoryLevelTarget(inventoryLevelId: string): InventoryLevelTargetRecord | null {
  for (const product of store.listEffectiveProducts()) {
    for (const variant of store.getEffectiveVariantsByProductId(product.id)) {
      const level = getEffectiveInventoryLevels(variant).find((candidate) => candidate.id === inventoryLevelId) ?? null;
      if (level) {
        return { variant, level };
      }
    }
  }

  return null;
}

function findKnownLocationById(locationId: string): LocationRecord | null {
  return listEffectiveLocations().find((location) => location.id === locationId) ?? null;
}

function stageVariantInventoryLevels(
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
  const nextVariants = store
    .getEffectiveVariantsByProductId(variant.productId)
    .map((candidate) => (candidate.id === variant.id ? nextVariant : candidate));
  store.replaceStagedVariantsForProduct(variant.productId, nextVariants);
  return store.getEffectiveVariantById(variant.id) ?? nextVariant;
}

function buildActivatedInventoryLevel(
  variant: ProductVariantRecord,
  location: LocationRecord,
): NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']>[number] | null {
  const syntheticLevel = buildSyntheticInventoryLevel(variant, {
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

function findEffectiveVariantById(variantId: string): ProductVariantRecord | null {
  for (const product of store.listEffectiveProducts()) {
    const variant = store.getEffectiveVariantsByProductId(product.id).find((candidate) => candidate.id === variantId);
    if (variant) {
      return variant;
    }
  }

  return null;
}

function serializeVariantPayload(variants: ProductVariantRecord[], field: FieldNode | null): Record<string, unknown>[] {
  if (!field) {
    return variants.map((variant) => ({ id: variant.id }));
  }

  return variants.map((variant) => serializeVariantSelectionSet(variant, field.selectionSet?.selections ?? [], {}));
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

function readMetafieldsSetInput(raw: unknown): Record<string, unknown>[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw.filter((value): value is Record<string, unknown> => isObject(value));
}

type DeletedMetafieldIdentifierRecord = {
  ownerId: string;
  namespace: string;
  key: string;
};

function readMetafieldsDeleteInput(raw: unknown): Record<string, unknown>[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw.filter((value): value is Record<string, unknown> => isObject(value));
}

function serializeDeletedMetafieldIdentifiers(
  identifiers: DeletedMetafieldIdentifierRecord[],
  field: FieldNode | null,
): Record<string, unknown>[] {
  if (!field) {
    return identifiers.map((identifier) => ({
      ownerId: identifier.ownerId,
      namespace: identifier.namespace,
      key: identifier.key,
    }));
  }

  return identifiers.map((identifier) =>
    Object.fromEntries(
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
    ),
  );
}

function upsertMetafieldsForProduct(
  productId: string,
  inputs: Record<string, unknown>[],
): { metafields: ProductMetafieldRecord[]; createdOrUpdated: ProductMetafieldRecord[] } {
  const existingMetafields = store.getEffectiveMetafieldsByProductId(productId);
  const metafieldsByIdentity = new Map(
    existingMetafields.map((metafield) => [`${metafield.namespace}:${metafield.key}`, metafield]),
  );
  const createdOrUpdated: ProductMetafieldRecord[] = [];

  for (const input of inputs) {
    const namespace = typeof input['namespace'] === 'string' ? input['namespace'] : '';
    const key = typeof input['key'] === 'string' ? input['key'] : '';
    const identityKey = `${namespace}:${key}`;
    const existing = metafieldsByIdentity.get(identityKey);
    const nextMetafield: ProductMetafieldRecord = {
      id: existing?.id ?? makeSyntheticGid('Metafield'),
      productId,
      namespace,
      key,
      type: typeof input['type'] === 'string' ? input['type'] : (existing?.type ?? null),
      value: typeof input['value'] === 'string' ? input['value'] : (existing?.value ?? null),
    };
    metafieldsByIdentity.set(identityKey, nextMetafield);
    createdOrUpdated.push(structuredClone(nextMetafield));
  }

  return {
    metafields: Array.from(metafieldsByIdentity.values()).sort(
      (left, right) =>
        left.namespace.localeCompare(right.namespace) ||
        left.key.localeCompare(right.key) ||
        left.id.localeCompare(right.id),
    ),
    createdOrUpdated,
  };
}

function findMetafieldById(metafieldId: string): ProductMetafieldRecord | null {
  for (const product of store.listEffectiveProducts()) {
    const metafield = store
      .getEffectiveMetafieldsByProductId(product.id)
      .find((candidate) => candidate.id === metafieldId);
    if (metafield) {
      return metafield;
    }
  }

  return null;
}

function deleteMetafieldsByIdentifiers(inputs: Record<string, unknown>[]): {
  deletedMetafields: DeletedMetafieldIdentifierRecord[];
  userErrors: Array<{ field: string[]; message: string }>;
} {
  if (inputs.length === 0) {
    return {
      deletedMetafields: [],
      userErrors: [{ field: ['metafields'], message: 'At least one metafield identifier is required' }],
    };
  }

  const effectiveMetafieldsByProductId = new Map<string, ProductMetafieldRecord[]>();
  const deletedMetafields: DeletedMetafieldIdentifierRecord[] = [];
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
      effectiveMetafieldsByProductId.get(ownerId) ?? store.getEffectiveMetafieldsByProductId(ownerId);
    const metafieldExists = effectiveMetafields.some(
      (metafield) => metafield.namespace === namespace && metafield.key === key,
    );
    if (!metafieldExists) {
      userErrors.push({ field: ['metafields', String(index)], message: 'Metafield not found' });
      continue;
    }

    const remainingMetafields = effectiveMetafields.filter(
      (metafield) => metafield.namespace !== namespace || metafield.key !== key,
    );
    effectiveMetafieldsByProductId.set(ownerId, remainingMetafields);
    deletedMetafields.push({ ownerId, namespace, key });
  }

  if (userErrors.length > 0) {
    return {
      deletedMetafields: [],
      userErrors,
    };
  }

  for (const [productId, metafields] of effectiveMetafieldsByProductId.entries()) {
    store.replaceStagedMetafieldsForProduct(productId, metafields);
  }

  return {
    deletedMetafields,
    userErrors: [],
  };
}

function buildProductSetVariantRecords(productId: string, rawVariants: unknown): ProductVariantRecord[] {
  const existingVariants = store.getEffectiveVariantsByProductId(productId);
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
        ? updateVariantRecord(existing, normalized)
        : makeCreatedProductSetVariantRecord(productId, normalized);
    });
}

function buildProductSetMetafieldRecords(productId: string, rawMetafields: unknown): ProductMetafieldRecord[] {
  const inputs = Array.isArray(rawMetafields)
    ? rawMetafields.filter((value): value is Record<string, unknown> => isObject(value))
    : [];

  return inputs.map((input) => {
    const existing = findMetafieldById(typeof input['id'] === 'string' ? input['id'] : '');
    return {
      id: existing?.productId === productId ? existing.id : makeSyntheticGid('Metafield'),
      productId,
      namespace: typeof input['namespace'] === 'string' ? input['namespace'] : (existing?.namespace ?? ''),
      key: typeof input['key'] === 'string' ? input['key'] : (existing?.key ?? ''),
      type: typeof input['type'] === 'string' ? input['type'] : (existing?.type ?? null),
      value: typeof input['value'] === 'string' ? input['value'] : (existing?.value ?? null),
    };
  });
}

function buildProductSetCollectionRecords(productId: string, rawCollections: unknown): ProductCollectionRecord[] {
  const collectionIds = Array.isArray(rawCollections)
    ? rawCollections.filter((value): value is string => typeof value === 'string')
    : [];

  return collectionIds.map((collectionId) => {
    const existing = findEffectiveCollectionById(collectionId);
    return {
      id: collectionId,
      productId,
      title: existing?.title ?? collectionId.split('/').at(-1) ?? collectionId,
      handle: existing?.handle ?? slugifyHandle(existing?.title ?? collectionId.split('/').at(-1) ?? collectionId),
    };
  });
}

function serializeProductSetOperation(field: FieldNode | null): Record<string, unknown> | null {
  if (!field) {
    return null;
  }

  const operationId = makeSyntheticGid('ProductSetOperation');
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'id':
        result[key] = operationId;
        break;
      case 'status':
        result[key] = 'CREATED';
        break;
      case 'userErrors':
        result[key] = [];
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

function normalizeUpstreamMetafield(productId: string, value: unknown): ProductMetafieldRecord | null {
  if (!isObject(value)) {
    return null;
  }

  const rawId = value['id'];
  const rawNamespace = value['namespace'];
  const rawKey = value['key'];
  const rawType = value['type'];
  const rawValue = value['value'];
  if (typeof rawId !== 'string' || typeof rawNamespace !== 'string' || typeof rawKey !== 'string') {
    return null;
  }

  return {
    id: rawId,
    productId,
    namespace: rawNamespace,
    key: rawKey,
    type: typeof rawType === 'string' ? rawType : null,
    value: typeof rawValue === 'string' ? rawValue : null,
  };
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

function readPublicationNodes(value: unknown): unknown[] {
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
    result[getResponseKey(productField)] = serializeProduct(payload.product, productField, variables);
  }

  const userErrorsField = getChildField(field, 'userErrors');
  if (userErrorsField) {
    result[getResponseKey(userErrorsField)] = payload.userErrors;
  }

  return result;
}

function serializePublishableSelectionSet(
  publishable: ProductRecord | CollectionRecord | null,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  if (!publishable) {
    return null;
  }

  if (publishable.id.startsWith('gid://shopify/Product/')) {
    return serializeSelectionSet(publishable as ProductRecord, selections, variables);
  }

  return serializeCollectionObject(publishable as CollectionRecord, selections, variables);
}

function serializeShopSelectionSet(selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'publicationCount':
        result[key] = listEffectivePublications().length;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializePublishableMutationPayload(
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
      payload.publishable,
      publishableField.selectionSet?.selections ?? [],
      variables,
    );
  }

  const shopField = getChildField(field, 'shop');
  if (shopField) {
    result[getResponseKey(shopField)] = serializeShopSelectionSet(shopField.selectionSet?.selections ?? []);
  }

  const userErrorsField = getChildField(field, 'userErrors');
  if (userErrorsField) {
    result[getResponseKey(userErrorsField)] = payload.userErrors;
  }

  return result;
}

function buildSyntheticInventoryLevel(
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
  const availableUpdatedAt = makeSyntheticTimestamp();

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
  variant: ProductVariantRecord,
): NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']> {
  const level = buildSyntheticInventoryLevel(variant);
  return level ? [level] : [];
}

function getEffectiveInventoryLevels(
  variant: ProductVariantRecord,
): NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']> {
  const hydratedLevels = variant.inventoryItem?.inventoryLevels;
  if (!hydratedLevels || hydratedLevels.length === 0) {
    return buildSyntheticInventoryLevels(variant);
  }

  return structuredClone(hydratedLevels);
}

function serializeInventoryLevelQuantities(
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
    level.quantities.length > 0 ? level.quantities : (buildSyntheticInventoryLevels(variant)[0]?.quantities ?? []);
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

function serializeInventoryLevelsConnection(
  variant: ProductVariantRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const getLevelCursor = (
    level: NonNullable<NonNullable<ProductVariantRecord['inventoryItem']>['inventoryLevels']>[number],
  ): string => level.cursor ?? `cursor:${level.id}`;
  const allLevels = getEffectiveInventoryLevels(variant);
  const {
    items: levels,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allLevels, field, variables, getLevelCursor);
  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'nodes':
        result[key] = levels.map((level) => {
          const nodeResult: Record<string, unknown> = {};
          for (const levelSelection of selection.selectionSet?.selections ?? []) {
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
                const locationResult: Record<string, unknown> = {};
                for (const locationSelection of levelSelection.selectionSet?.selections ?? []) {
                  if (locationSelection.kind !== Kind.FIELD) {
                    continue;
                  }
                  const locationKey = locationSelection.alias?.value ?? locationSelection.name.value;
                  switch (locationSelection.name.value) {
                    case 'id':
                      locationResult[locationKey] = level.location.id;
                      break;
                    case 'name':
                      locationResult[locationKey] = level.location.name;
                      break;
                    default:
                      locationResult[locationKey] = null;
                  }
                }
                nodeResult[levelKey] = locationResult;
                break;
              }
              case 'quantities':
                nodeResult[levelKey] = serializeInventoryLevelQuantities(variant, level, levelSelection, variables);
                break;
              default:
                nodeResult[levelKey] = null;
            }
          }
          return nodeResult;
        });
        break;
      case 'edges':
        result[key] = levels.map((level) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of selection.selectionSet?.selections ?? []) {
            if (edgeSelection.kind !== Kind.FIELD) {
              continue;
            }

            const edgeKey = edgeSelection.alias?.value ?? edgeSelection.name.value;
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = getLevelCursor(level);
                break;
              case 'node': {
                const nodeResult: Record<string, unknown> = {};
                for (const levelSelection of edgeSelection.selectionSet?.selections ?? []) {
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
                      const locationResult: Record<string, unknown> = {};
                      for (const locationSelection of levelSelection.selectionSet?.selections ?? []) {
                        if (locationSelection.kind !== Kind.FIELD) {
                          continue;
                        }
                        const locationKey = locationSelection.alias?.value ?? locationSelection.name.value;
                        switch (locationSelection.name.value) {
                          case 'id':
                            locationResult[locationKey] = level.location.id;
                            break;
                          case 'name':
                            locationResult[locationKey] = level.location.name;
                            break;
                          default:
                            locationResult[locationKey] = null;
                        }
                      }
                      nodeResult[levelKey] = locationResult;
                      break;
                    }
                    case 'quantities':
                      nodeResult[levelKey] = serializeInventoryLevelQuantities(
                        variant,
                        level,
                        levelSelection,
                        variables,
                      );
                      break;
                    default:
                      nodeResult[levelKey] = null;
                  }
                }
                edgeResult[edgeKey] = nodeResult;
                break;
              }
              default:
                edgeResult[edgeKey] = null;
            }
          }
          return edgeResult;
        });
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(selection, levels, hasNextPage, hasPreviousPage, getLevelCursor, {
          prefixCursors: false,
        });
        break;
      default:
        result[key] = null;
    }
  }

  return result;
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
        const locationResult: Record<string, unknown> = {};
        for (const locationSelection of selection.selectionSet?.selections ?? []) {
          if (locationSelection.kind !== Kind.FIELD) {
            continue;
          }
          const locationKey = locationSelection.alias?.value ?? locationSelection.name.value;
          switch (locationSelection.name.value) {
            case 'id':
              locationResult[locationKey] = level.location.id;
              break;
            case 'name':
              locationResult[locationKey] = level.location.name;
              break;
            default:
              locationResult[locationKey] = null;
          }
        }
        result[key] = locationResult;
        break;
      }
      case 'quantities':
        result[key] = serializeInventoryLevelQuantities(variant, level, selection, variables);
        break;
      case 'item':
        result[key] = serializeInventoryItemSelectionSet(variant, selection.selectionSet?.selections ?? [], variables);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeInventoryItemSelectionSet(
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
          case 'sku':
            return [inventoryKey, variant.sku ?? null];
          case 'tracked':
            return [inventoryKey, variant.inventoryItem?.tracked ?? null];
          case 'requiresShipping':
            return [inventoryKey, variant.inventoryItem?.requiresShipping ?? null];
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
          case 'inventoryLevels':
            return [inventoryKey, serializeInventoryLevelsConnection(variant, inventorySelection, variables)];
          case 'variant':
            return [
              inventoryKey,
              serializeVariantSelectionSet(variant, inventorySelection.selectionSet?.selections ?? [], variables),
            ];
          default:
            return [inventoryKey, null];
        }
      }),
  );
}

function serializeVariantSelectionSet(
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
        result[key] = serializeInventoryItemSelectionSet(variant, selection.selectionSet?.selections ?? [], variables);
        break;
      case 'product': {
        const product = store.getEffectiveProductById(variant.productId);
        result[key] = serializeProduct(product, selection, {});
        break;
      }
      default:
        result[key] = null;
    }
  }

  return result;
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
        result[key] = option.optionValues.map((optionValue) => {
          const optionValueResult: Record<string, unknown> = {};
          for (const optionValueSelection of selection.selectionSet?.selections ?? []) {
            if (optionValueSelection.kind !== Kind.FIELD) {
              continue;
            }

            const optionValueKey = optionValueSelection.alias?.value ?? optionValueSelection.name.value;
            switch (optionValueSelection.name.value) {
              case 'id':
                optionValueResult[optionValueKey] = optionValue.id;
                break;
              case 'name':
                optionValueResult[optionValueKey] = optionValue.name;
                break;
              case 'hasVariants':
                optionValueResult[optionValueKey] = optionValue.hasVariants;
                break;
              default:
                optionValueResult[optionValueKey] = null;
            }
          }
          return optionValueResult;
        });
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeVariantsConnection(
  productId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const allVariants = store.getEffectiveVariantsByProductId(productId);
  const {
    items: variants,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allVariants, field, variables, (variant) => variant.id);
  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'nodes':
        result[key] = variants.map((variant) =>
          serializeVariantSelectionSet(variant, selection.selectionSet?.selections ?? [], variables),
        );
        break;
      case 'edges':
        result[key] = variants.map((variant) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of selection.selectionSet?.selections ?? []) {
            if (edgeSelection.kind !== Kind.FIELD) {
              continue;
            }

            const edgeKey = edgeSelection.alias?.value ?? edgeSelection.name.value;
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${variant.id}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeVariantSelectionSet(
                  variant,
                  edgeSelection.selectionSet?.selections ?? [],
                  variables,
                );
                break;
              default:
                edgeResult[edgeKey] = null;
            }
          }
          return edgeResult;
        });
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(
          selection,
          variants,
          hasNextPage,
          hasPreviousPage,
          (variant) => variant.id,
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeCollectionSelectionSet(
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
    result[key] = serializeCollectionField(collection, selection, variables);
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
  collection: CollectionRecord | ProductCollectionRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
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
      return serializeCountValue(field, listEffectiveProductsForCollection(collection.id).length);
    case 'hasProduct': {
      const args = getFieldArguments(field, variables);
      const productId = typeof args['id'] === 'string' ? args['id'] : null;
      return productId
        ? listEffectiveProductsForCollection(collection.id).some((product) => product.id === productId)
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
    default:
      return null;
  }
}

function serializeCollectionObject(
  collection: CollectionRecord | ProductCollectionRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeName = selection.typeCondition?.name.value;
      if (typeName && typeName !== 'Collection' && typeName !== 'Publishable' && typeName !== 'Node') {
        continue;
      }

      Object.assign(result, serializeCollectionObject(collection, selection.selectionSet.selections, variables));
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
          listEffectiveProductsForCollection(collection.id),
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
        result[key] = serializeCollectionField(collection, selection, variables);
    }
  }

  return result;
}

function serializeCollectionsConnection(
  productId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const allCollections = sortCollections(
    applyCollectionsQuery(store.getEffectiveCollectionsByProductId(productId), args['query']),
    args['sortKey'],
    args['reverse'],
    args['query'],
  );
  const {
    items: collections,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allCollections, field, variables, (collection) => collection.id);
  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'nodes':
        result[key] = collections.map((collection) =>
          serializeCollectionSelectionSet(collection, selection.selectionSet?.selections ?? [], variables),
        );
        break;
      case 'edges':
        result[key] = collections.map((collection) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of selection.selectionSet?.selections ?? []) {
            if (edgeSelection.kind !== Kind.FIELD) {
              continue;
            }

            const edgeKey = edgeSelection.alias?.value ?? edgeSelection.name.value;
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${collection.id}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeCollectionSelectionSet(
                  collection,
                  edgeSelection.selectionSet?.selections ?? [],
                  variables,
                );
                break;
              default:
                edgeResult[edgeKey] = null;
            }
          }
          return edgeResult;
        });
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(
          selection,
          collections,
          hasNextPage,
          hasPreviousPage,
          (collection) => collection.id,
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
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
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const allCollections = sortCollections(
    applyCollectionsQuery(filterCollectionsByQuery(listEffectiveCollections(), args['query']), args['query']),
    args['sortKey'],
    args['reverse'],
    args['query'],
  );
  const {
    items: collections,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allCollections, field, variables, (collection) => collection.id);
  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'nodes':
        result[key] = collections.map((collection) =>
          serializeCollectionObject(collection, selection.selectionSet?.selections ?? [], variables),
        );
        break;
      case 'edges':
        result[key] = collections.map((collection) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of selection.selectionSet?.selections ?? []) {
            if (edgeSelection.kind !== Kind.FIELD) {
              continue;
            }

            const edgeKey = edgeSelection.alias?.value ?? edgeSelection.name.value;
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${collection.id}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeCollectionObject(
                  collection,
                  edgeSelection.selectionSet?.selections ?? [],
                  variables,
                );
                break;
              default:
                edgeResult[edgeKey] = null;
            }
          }
          return edgeResult;
        });
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(
          selection,
          collections,
          hasNextPage,
          hasPreviousPage,
          (collection) => collection.id,
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeLocationSelectionSet(
  location: LocationRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeName = selection.typeCondition?.name.value;
      if (typeName && typeName !== 'Location') {
        continue;
      }

      Object.assign(result, serializeLocationSelectionSet(location, selection.selectionSet.selections));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'id':
        result[key] = location.id;
        break;
      case 'name':
        result[key] = location.name;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeTopLevelLocationsConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const allLocations = listEffectiveLocations();
  const {
    items: locations,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allLocations, field, variables, (location) => location.id);
  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'nodes':
        result[key] = locations.map((location) =>
          serializeLocationSelectionSet(location, selection.selectionSet?.selections ?? []),
        );
        break;
      case 'edges':
        result[key] = locations.map((location) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of selection.selectionSet?.selections ?? []) {
            if (edgeSelection.kind !== Kind.FIELD) {
              continue;
            }

            const edgeKey = edgeSelection.alias?.value ?? edgeSelection.name.value;
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${location.id}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeLocationSelectionSet(
                  location,
                  edgeSelection.selectionSet?.selections ?? [],
                );
                break;
              default:
                edgeResult[edgeKey] = null;
            }
          }
          return edgeResult;
        });
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(
          selection,
          locations,
          hasNextPage,
          hasPreviousPage,
          (location) => location.id,
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializePublicationSelectionSet(
  publication: PublicationRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'id':
        result[key] = publication.id;
        break;
      case 'name':
        result[key] = publication.name;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function getPublicationCursorValue(publication: PublicationRecord): string {
  return typeof publication.cursor === 'string' && publication.cursor.length > 0 ? publication.cursor : publication.id;
}

function serializePublicationCursor(publication: PublicationRecord): string {
  return typeof publication.cursor === 'string' && publication.cursor.length > 0
    ? publication.cursor
    : `cursor:${publication.id}`;
}

function serializeTopLevelPublicationsConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const allPublications = listEffectivePublications();
  const {
    items: publications,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allPublications, field, variables, (publication) =>
    getPublicationCursorValue(publication),
  );
  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'nodes':
        result[key] = publications.map((publication) =>
          serializePublicationSelectionSet(publication, selection.selectionSet?.selections ?? []),
        );
        break;
      case 'edges':
        result[key] = publications.map((publication) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of selection.selectionSet?.selections ?? []) {
            if (edgeSelection.kind !== Kind.FIELD) {
              continue;
            }

            const edgeKey = edgeSelection.alias?.value ?? edgeSelection.name.value;
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = serializePublicationCursor(publication);
                break;
              case 'node':
                edgeResult[edgeKey] = serializePublicationSelectionSet(
                  publication,
                  edgeSelection.selectionSet?.selections ?? [],
                );
                break;
              default:
                edgeResult[edgeKey] = null;
            }
          }
          return edgeResult;
        });
        break;
      case 'pageInfo':
        result[key] = Object.fromEntries(
          (selection.selectionSet?.selections ?? [])
            .filter((pageInfoSelection): pageInfoSelection is FieldNode => pageInfoSelection.kind === Kind.FIELD)
            .map((pageInfoSelection) => {
              const pageInfoKey = pageInfoSelection.alias?.value ?? pageInfoSelection.name.value;
              switch (pageInfoSelection.name.value) {
                case 'hasNextPage':
                  return [pageInfoKey, hasNextPage];
                case 'hasPreviousPage':
                  return [pageInfoKey, hasPreviousPage];
                case 'startCursor':
                  return [pageInfoKey, publications[0] ? serializePublicationCursor(publications[0]) : null];
                case 'endCursor':
                  return [
                    pageInfoKey,
                    publications.length > 0 ? serializePublicationCursor(publications[publications.length - 1]!) : null,
                  ];
                default:
                  return [pageInfoKey, null];
              }
            }),
        );
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

function promoteProcessingMediaAfterRead(productId: string, mediaRecords: ProductMediaRecord[]): void {
  const needsPromotion = mediaRecords.some((mediaRecord) => mediaRecord.status === 'PROCESSING');
  if (!needsPromotion) {
    return;
  }

  const nextMedia = store
    .getEffectiveMediaByProductId(productId)
    .map((mediaRecord) => (mediaRecord.status === 'PROCESSING' ? transitionMediaToReady(mediaRecord) : mediaRecord));
  store.replaceStagedMediaForProduct(productId, nextMedia);
}

function serializeMediaConnection(
  productId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const allMediaRecords = store.getEffectiveMediaByProductId(productId);
  const {
    items: mediaRecords,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allMediaRecords, field, variables, (mediaRecord) => mediaRecord.key);
  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'nodes':
        result[key] = mediaRecords.map((mediaRecord) =>
          serializeMediaSelectionSet(mediaRecord, selection.selectionSet?.selections ?? []),
        );
        break;
      case 'edges':
        result[key] = mediaRecords.map((mediaRecord) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of selection.selectionSet?.selections ?? []) {
            if (edgeSelection.kind !== Kind.FIELD) {
              continue;
            }

            const edgeKey = edgeSelection.alias?.value ?? edgeSelection.name.value;
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${mediaRecord.key}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeMediaSelectionSet(
                  mediaRecord,
                  edgeSelection.selectionSet?.selections ?? [],
                );
                break;
              default:
                edgeResult[edgeKey] = null;
            }
          }
          return edgeResult;
        });
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(
          selection,
          mediaRecords,
          hasNextPage,
          hasPreviousPage,
          (mediaRecord) => mediaRecord.key,
        );
        break;
      default:
        result[key] = null;
    }
  }

  promoteProcessingMediaAfterRead(productId, allMediaRecords);
  return result;
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
  productId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const allMediaRecords = store.getEffectiveMediaByProductId(productId);
  const allImageRecords = allMediaRecords.filter((mediaRecord) => mediaRecord.mediaContentType === 'IMAGE');
  const {
    items: imageRecords,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allImageRecords, field, variables, (mediaRecord) => mediaRecord.key);
  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'nodes':
        result[key] = imageRecords.map((mediaRecord) =>
          serializeProductImageSelectionSet(mediaRecord, selection.selectionSet?.selections ?? []),
        );
        break;
      case 'edges':
        result[key] = imageRecords.map((mediaRecord) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of selection.selectionSet?.selections ?? []) {
            if (edgeSelection.kind !== Kind.FIELD) {
              continue;
            }

            const edgeKey = edgeSelection.alias?.value ?? edgeSelection.name.value;
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${mediaRecord.key}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeProductImageSelectionSet(
                  mediaRecord,
                  edgeSelection.selectionSet?.selections ?? [],
                );
                break;
              default:
                edgeResult[edgeKey] = null;
            }
          }
          return edgeResult;
        });
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(
          selection,
          imageRecords,
          hasNextPage,
          hasPreviousPage,
          (mediaRecord) => mediaRecord.key,
        );
        break;
      default:
        result[key] = null;
    }
  }

  promoteProcessingMediaAfterRead(productId, allMediaRecords);
  return result;
}

function serializeMetafieldSelectionSet(
  metafield: ProductMetafieldRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'id':
        result[key] = metafield.id;
        break;
      case 'namespace':
        result[key] = metafield.namespace;
        break;
      case 'key':
        result[key] = metafield.key;
        break;
      case 'type':
        result[key] = metafield.type;
        break;
      case 'value':
        result[key] = metafield.value;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeMetafieldsConnection(
  productId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const allMetafields = store.getEffectiveMetafieldsByProductId(productId);
  const {
    items: metafields,
    hasNextPage,
    hasPreviousPage,
  } = paginateConnectionItems(allMetafields, field, variables, (metafield) => metafield.id);
  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'nodes':
        result[key] = metafields.map((metafield) =>
          serializeMetafieldSelectionSet(metafield, selection.selectionSet?.selections ?? []),
        );
        break;
      case 'edges':
        result[key] = metafields.map((metafield) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of selection.selectionSet?.selections ?? []) {
            if (edgeSelection.kind !== Kind.FIELD) {
              continue;
            }

            const edgeKey = edgeSelection.alias?.value ?? edgeSelection.name.value;
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = `cursor:${metafield.id}`;
                break;
              case 'node':
                edgeResult[edgeKey] = serializeMetafieldSelectionSet(
                  metafield,
                  edgeSelection.selectionSet?.selections ?? [],
                );
                break;
              default:
                edgeResult[edgeKey] = null;
            }
          }
          return edgeResult;
        });
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(
          selection,
          metafields,
          hasNextPage,
          hasPreviousPage,
          (metafield) => metafield.id,
        );
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeProductField(product: ProductRecord, field: FieldNode, variables: Record<string, unknown>): unknown {
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
      return store
        .getEffectiveOptionsByProductId(product.id)
        .map((option) => serializeOptionSelectionSet(option, field.selectionSet?.selections ?? []));
    case 'variants':
      return serializeVariantsConnection(product.id, field, variables);
    case 'collections':
      return serializeCollectionsConnection(product.id, field, variables);
    case 'media':
      return serializeMediaConnection(product.id, field, variables);
    case 'images':
      return serializeProductImagesConnection(product.id, field, variables);
    case 'metafield': {
      const args = getFieldArguments(field, variables);
      const namespace = typeof args['namespace'] === 'string' ? args['namespace'] : null;
      const key = typeof args['key'] === 'string' ? args['key'] : null;
      if (!namespace || !key) {
        return null;
      }

      const metafield = store
        .getEffectiveMetafieldsByProductId(product.id)
        .find((candidate) => candidate.namespace === namespace && candidate.key === key);
      return metafield ? serializeMetafieldSelectionSet(metafield, field.selectionSet?.selections ?? []) : null;
    }
    case 'metafields':
      return serializeMetafieldsConnection(product.id, field, variables);
    default:
      return null;
  }
}

function serializeSelectionSet(
  product: ProductRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeName = selection.typeCondition?.name.value;
      if (typeName && typeName !== 'Product') {
        continue;
      }

      Object.assign(result, serializeSelectionSet(product, selection.selectionSet.selections, variables));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    result[key] = serializeProductField(product, selection, variables);
  }

  return result;
}

function serializeProduct(
  product: ProductRecord | null,
  field: FieldNode | null,
  variables: Record<string, unknown>,
): unknown {
  if (!product) {
    return null;
  }

  const selections = field?.selectionSet?.selections ?? [];
  return serializeSelectionSet(product, selections, variables);
}

function serializeProductsCount(rawQuery: unknown, selections: readonly SelectionNode[]): Record<string, unknown> {
  const filteredProducts = applyProductsQuery(store.listEffectiveProducts(), rawQuery);
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

function matchesProductVariantTerm(product: ProductRecord, field: 'sku' | 'barcode', value: string): boolean {
  const normalizedValue = value.toLowerCase();
  return store
    .getEffectiveVariantsByProductId(product.id)
    .some((variant) => typeof variant[field] === 'string' && variant[field]!.toLowerCase() === normalizedValue);
}

function matchesProductTimestampTerm(productValue: string, rawValue: string): boolean {
  const match = rawValue.match(/^(<=|>=|<|>|=)?\s*(.+)$/);
  if (!match) {
    return true;
  }

  const operator = match[1] ?? '=';
  const thresholdValue = stripSearchValueQuotes(match[2]?.trim() ?? '');
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

function stripSearchValueQuotes(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length >= 2) {
    const firstCharacter = trimmed[0];
    const lastCharacter = trimmed[trimmed.length - 1];
    if ((firstCharacter === '"' || firstCharacter === "'") && firstCharacter === lastCharacter) {
      return trimmed.slice(1, -1);
    }
  }

  return trimmed;
}

function matchesNullableProductTimestampTerm(productValue: string | null, rawValue: string): boolean {
  const normalizedValue = stripSearchValueQuotes(rawValue);
  if (normalizedValue === '*') {
    return productValue !== null;
  }

  return productValue === null ? false : matchesProductTimestampTerm(productValue, normalizedValue);
}

function isPrefixPattern(rawValue: string): boolean {
  return rawValue.endsWith('*');
}

function matchesStringValue(candidate: string, rawValue: string, matchMode: 'includes' | 'exact'): boolean {
  const value = rawValue.trim().toLowerCase();
  if (!value) {
    return true;
  }

  const prefixMode = isPrefixPattern(value);
  const normalizedValue = prefixMode ? value.slice(0, -1) : value;
  if (!normalizedValue) {
    return true;
  }

  const normalizedCandidate = candidate.toLowerCase();
  if (prefixMode) {
    if (normalizedCandidate.startsWith(normalizedValue)) {
      return true;
    }

    return normalizedCandidate.split(/[^a-z0-9]+/).some((part) => part.startsWith(normalizedValue));
  }

  return matchMode === 'exact'
    ? normalizedCandidate === normalizedValue
    : normalizedCandidate.includes(normalizedValue);
}

function getSearchableProductTags(product: ProductRecord): string[] {
  if (!store.isTagSearchLagged(product.id)) {
    return product.tags;
  }

  const baseProduct = store.getBaseProductById(product.id);
  if (!baseProduct) {
    return product.tags;
  }

  return product.tags.filter((tag) => baseProduct.tags.includes(tag));
}

function getSearchableProductVariants(product: ProductRecord): ProductVariantRecord[] {
  if (!store.isVariantSearchLagged(product.id)) {
    return store.getEffectiveVariantsByProductId(product.id);
  }

  const baseProduct = store.getBaseProductById(product.id);
  if (!baseProduct) {
    return [];
  }

  return store.getBaseVariantsByProductId(baseProduct.id);
}

function matchesProductSearchText(product: ProductRecord, rawValue: string): boolean {
  const searchableValues = [
    product.title,
    product.handle,
    product.vendor ?? '',
    product.productType ?? '',
    ...getSearchableProductTags(product),
  ];

  return searchableValues.some((candidate) => matchesStringValue(candidate, rawValue, 'includes'));
}

function searchTermValue(term: SearchQueryTerm): string {
  return term.comparator === null ? term.value : `${term.comparator}${term.value}`;
}

function matchesPositiveProductQueryTerm(product: ProductRecord, term: SearchQueryTerm): boolean {
  if (term.field === null) {
    return matchesProductSearchText(product, term.value);
  }

  const field = term.field.toLowerCase();
  const value = searchTermValue(term);

  switch (field) {
    case 'title':
      return matchesStringValue(product.title, value, 'includes');
    case 'handle':
      return matchesStringValue(product.handle, value, 'exact');
    case 'tag':
      return getSearchableProductTags(product).some((tag) => matchesStringValue(tag, value, 'exact'));
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
    case 'updated_at':
      return matchesProductTimestampTerm(product.updatedAt, value);
    case 'tag_not':
      return !getSearchableProductTags(product).some((tag) => matchesStringValue(tag, value, 'exact'));
    case 'sku':
      return getSearchableProductVariants(product).some(
        (variant) => typeof variant.sku === 'string' && matchesStringValue(variant.sku, value, 'exact'),
      );
    case 'barcode':
      return getSearchableProductVariants(product).some(
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

function matchesProductQueryTerm(product: ProductRecord, term: SearchQueryTerm): boolean {
  if (!term.raw) {
    return true;
  }

  if (term.negated && !term.value && term.field === null) {
    return true;
  }

  const matches = matchesPositiveProductQueryTerm(product, term);
  return term.negated ? !matches : matches;
}

function matchesProductsQueryNode(product: ProductRecord, node: SearchQueryNode): boolean {
  switch (node.type) {
    case 'term':
      return matchesProductQueryTerm(product, node.term);
    case 'and':
      return node.children.every((child) => matchesProductsQueryNode(product, child));
    case 'or':
      return node.children.some((child) => matchesProductsQueryNode(product, child));
    case 'not':
      return !matchesProductsQueryNode(product, node.child);
    default:
      return true;
  }
}

function applyProductsQuery(products: ProductRecord[], rawQuery: unknown): ProductRecord[] {
  if (typeof rawQuery !== 'string' || !rawQuery.trim()) {
    return products;
  }

  const parsedQuery = parseSearchQuery(rawQuery, { recognizeNotKeyword: true });
  if (!parsedQuery) {
    return products;
  }

  return products.filter((product) => matchesProductsQueryNode(product, parsedQuery));
}

function collectionIsSmart(collection: CollectionRecord | ProductCollectionRecord): boolean {
  return collection.isSmart === true || Boolean(collection.ruleSet);
}

function matchesResourceIdValue(resourceId: string, rawValue: string): boolean {
  const normalizedValue = stripSearchValueQuotes(rawValue).trim();
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
  const thresholdValue = stripSearchValueQuotes(match[2]?.trim() ?? '');
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

function collectionHasProduct(collection: CollectionRecord | ProductCollectionRecord, rawValue: string): boolean {
  return listEffectiveProductsForCollection(collection.id).some((product) =>
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
  collection: CollectionRecord | ProductCollectionRecord,
  term: SearchQueryTerm,
): boolean {
  if (term.field === null) {
    return matchesCollectionSearchText(collection, term.value);
  }

  const field = term.field.toLowerCase();
  const value = searchTermValue(term);

  switch (field) {
    case 'title':
      return matchesStringValue(collection.title, value, 'includes');
    case 'handle':
      return matchesStringValue(collection.handle, value, 'exact');
    case 'collection_type': {
      const normalizedValue = stripSearchValueQuotes(value).trim().toLowerCase();
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
      return collectionHasProduct(collection, value);
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

function matchesCollectionQueryTerm(
  collection: CollectionRecord | ProductCollectionRecord,
  term: SearchQueryTerm,
): boolean {
  if (!term.raw) {
    return true;
  }

  if (term.negated && !term.value && term.field === null) {
    return true;
  }

  const matches = matchesPositiveCollectionQueryTerm(collection, term);
  return term.negated ? !matches : matches;
}

function matchesCollectionsQueryNode(
  collection: CollectionRecord | ProductCollectionRecord,
  node: SearchQueryNode,
): boolean {
  switch (node.type) {
    case 'term':
      return matchesCollectionQueryTerm(collection, node.term);
    case 'and':
      return node.children.every((child) => matchesCollectionsQueryNode(collection, child));
    case 'or':
      return node.children.some((child) => matchesCollectionsQueryNode(collection, child));
    case 'not':
      return !matchesCollectionsQueryNode(collection, node.child);
    default:
      return true;
  }
}

function applyCollectionsQuery<T extends CollectionRecord | ProductCollectionRecord>(
  collections: T[],
  rawQuery: unknown,
): T[] {
  if (typeof rawQuery !== 'string' || !rawQuery.trim()) {
    return collections;
  }

  const parsedQuery = parseSearchQuery(rawQuery, { recognizeNotKeyword: true });
  if (!parsedQuery) {
    return collections;
  }

  return collections.filter((collection) => matchesCollectionsQueryNode(collection, parsedQuery));
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

function listProductsForConnection(catalogConnection: ProductCatalogConnectionRecord | null): ProductRecord[] {
  if (!catalogConnection) {
    return store.listEffectiveProducts();
  }

  const orderedProducts = catalogConnection.orderedProductIds
    .map((productId) => store.getEffectiveProductById(productId))
    .filter((product): product is ProductRecord => product !== null);
  const seenProductIds = new Set(orderedProducts.map((product) => product.id));
  const extraProducts = store.listEffectiveProducts().filter((product) => !seenProductIds.has(product.id));
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

function parseProductsCursor(rawCursor: unknown): string | null {
  if (typeof rawCursor !== 'string' || !rawCursor.startsWith('cursor:')) {
    return null;
  }

  const productId = rawCursor.slice('cursor:'.length);
  return productId.length > 0 ? productId : null;
}

function serializeProductsConnection(
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
  const searchConnection = searchConnectionKey ? store.getBaseProductSearchConnection(searchConnectionKey) : null;
  const candidateProducts = searchConnection ? listProductsForConnection(searchConnection) : products;
  const filteredProducts = applyProductsQuery(candidateProducts, rawQuery);
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
  const calculatedHasPreviousPage = windowStart > 0;

  if (first !== null) {
    limitedProducts = limitedProducts.slice(0, first);
  }

  let visibleStartIndex = windowStart;
  if (last !== null) {
    visibleStartIndex = Math.max(windowStart, windowStart + limitedProducts.length - last);
    limitedProducts = limitedProducts.slice(Math.max(0, limitedProducts.length - last));
  }

  const visibleEndIndex = visibleStartIndex + limitedProducts.length;
  const hasNextPage =
    calculatedHasNextPage || (preserveBaselinePageInfo && (searchConnection?.pageInfo.hasNextPage ?? false));
  const hasPreviousPage =
    visibleStartIndex > 0 || (preserveBaselinePageInfo && (searchConnection?.pageInfo.hasPreviousPage ?? false));

  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;

    switch (selection.name.value) {
      case 'nodes':
        result[key] = limitedProducts.map((product) =>
          serializeSelectionSet(product, selection.selectionSet?.selections ?? [], variables),
        );
        break;
      case 'edges':
        result[key] = limitedProducts.map((product) => {
          const edgeResult: Record<string, unknown> = {};
          for (const edgeSelection of selection.selectionSet?.selections ?? []) {
            if (edgeSelection.kind !== Kind.FIELD) {
              continue;
            }

            const edgeKey = edgeSelection.alias?.value ?? edgeSelection.name.value;
            switch (edgeSelection.name.value) {
              case 'cursor':
                edgeResult[edgeKey] = resolveCatalogProductCursor(product.id, searchConnection);
                break;
              case 'node':
                edgeResult[edgeKey] = serializeSelectionSet(
                  product,
                  edgeSelection.selectionSet?.selections ?? [],
                  variables,
                );
                break;
              default:
                edgeResult[edgeKey] = null;
            }
          }
          return edgeResult;
        });
        break;
      case 'pageInfo':
        result[key] = Object.fromEntries(
          (selection.selectionSet?.selections ?? [])
            .filter((pageInfoSelection): pageInfoSelection is FieldNode => pageInfoSelection.kind === Kind.FIELD)
            .map((pageInfoSelection) => {
              const pageInfoKey = pageInfoSelection.alias?.value ?? pageInfoSelection.name.value;
              switch (pageInfoSelection.name.value) {
                case 'hasNextPage':
                  return [pageInfoKey, hasNextPage];
                case 'hasPreviousPage':
                  return [pageInfoKey, hasPreviousPage];
                case 'startCursor':
                  return [
                    pageInfoKey,
                    limitedProducts[0]
                      ? resolveCatalogProductCursor(limitedProducts[0].id, searchConnection)
                      : preserveBaselinePageInfo
                        ? (searchConnection?.pageInfo.startCursor ?? null)
                        : null,
                  ];
                case 'endCursor':
                  return [
                    pageInfoKey,
                    limitedProducts.length > 0
                      ? resolveCatalogProductCursor(limitedProducts[limitedProducts.length - 1]!.id, searchConnection)
                      : preserveBaselinePageInfo
                        ? (searchConnection?.pageInfo.endCursor ?? null)
                        : null,
                  ];
                default:
                  return [pageInfoKey, null];
              }
            }),
        );
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
  const hasOptions = hasOwnField(value, 'options');
  const hasVariants = hasOwnField(value, 'variants');
  const hasCollections = hasOwnField(value, 'collections');
  const hasMedia = hasOwnField(value, 'media');
  const hasImages = hasOwnField(value, 'images');
  const hasMetafields = hasOwnField(value, 'metafields') || hasOwnField(value, 'metafield');
  const options = Array.isArray(value['options'])
    ? value['options']
        .map((option) => normalizeUpstreamOption(rawId, option))
        .filter((option): option is ProductOptionRecord => option !== null)
    : [];
  const variants = readVariantNodes(value['variants'])
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
  const metafieldsById = new Map<string, ProductMetafieldRecord>();
  const singularMetafield = normalizeUpstreamMetafield(rawId, value['metafield']);
  if (singularMetafield) {
    metafieldsById.set(singularMetafield.id, singularMetafield);
  }
  for (const metafieldNode of readMetafieldNodes(value['metafields'])) {
    const metafield = normalizeUpstreamMetafield(rawId, metafieldNode);
    if (metafield) {
      metafieldsById.set(metafield.id, metafield);
    }
  }
  const metafields = Array.from(metafieldsById.values()).sort(
    (left, right) =>
      left.namespace.localeCompare(right.namespace) ||
      left.key.localeCompare(right.key) ||
      left.id.localeCompare(right.id),
  );
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
    store.setBaseProductSearchConnection(key, connection);
  }

  const publications = readPublicationRecords(rawData['publications']);
  if (publications.length > 0) {
    store.upsertBasePublications(publications);
  }

  const maybeProduct = normalizeUpstreamProduct(rawData['product']);
  if (maybeProduct) {
    store.upsertBaseProducts([maybeProduct.product]);
    if (maybeProduct.hasOptions) {
      store.replaceBaseOptionsForProduct(maybeProduct.product.id, maybeProduct.options);
    }
    if (maybeProduct.hasVariants) {
      store.replaceBaseVariantsForProduct(maybeProduct.product.id, maybeProduct.variants);
    }
    if (maybeProduct.hasCollections) {
      store.replaceBaseCollectionsForProduct(maybeProduct.product.id, maybeProduct.collections);
    }
    if (maybeProduct.hasMedia || maybeProduct.hasImages) {
      store.replaceBaseMediaForProduct(maybeProduct.product.id, maybeProduct.media);
    }
    if (maybeProduct.hasMetafields) {
      store.replaceBaseMetafieldsForProduct(maybeProduct.product.id, maybeProduct.metafields);
    }
  }

  const hydrateCollection = (value: unknown): void => {
    const collection = normalizeUpstreamCollectionRecord(value);
    if (!collection || !isObject(value)) {
      return;
    }

    store.upsertBaseCollections([collection]);

    const productEntries = readConnectionNodeEntries(value['products']);
    for (const productEntry of productEntries) {
      const normalizedProduct = normalizeUpstreamProduct(productEntry.node);
      if (!normalizedProduct) {
        continue;
      }

      store.upsertBaseProducts([normalizedProduct.product]);
      if (normalizedProduct.hasOptions) {
        store.replaceBaseOptionsForProduct(normalizedProduct.product.id, normalizedProduct.options);
      }
      if (normalizedProduct.hasVariants) {
        store.replaceBaseVariantsForProduct(normalizedProduct.product.id, normalizedProduct.variants);
      }
      if (normalizedProduct.hasMedia || normalizedProduct.hasImages) {
        store.replaceBaseMediaForProduct(normalizedProduct.product.id, normalizedProduct.media);
      }
      if (normalizedProduct.hasMetafields) {
        store.replaceBaseMetafieldsForProduct(normalizedProduct.product.id, normalizedProduct.metafields);
      }

      const nextCollections = [
        ...store
          .getEffectiveCollectionsByProductId(normalizedProduct.product.id)
          .filter((candidate) => candidate.id !== collection.id),
        {
          ...collection,
          productId: normalizedProduct.product.id,
          position: productEntry.position,
        },
      ];
      store.replaceBaseCollectionsForProduct(normalizedProduct.product.id, nextCollections);
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

    store.upsertBaseProducts(products.map((entry) => entry.product));
    for (const entry of products) {
      if (entry.hasOptions) {
        store.replaceBaseOptionsForProduct(entry.product.id, entry.options);
      }
      if (entry.hasVariants) {
        store.replaceBaseVariantsForProduct(entry.product.id, entry.variants);
      }
      if (entry.hasCollections) {
        store.replaceBaseCollectionsForProduct(entry.product.id, entry.collections);
      }
      if (entry.hasMedia || entry.hasImages) {
        store.replaceBaseMediaForProduct(entry.product.id, entry.media);
      }
      if (entry.hasMetafields) {
        store.replaceBaseMetafieldsForProduct(entry.product.id, entry.metafields);
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

    store.upsertBaseProducts(products.map((entry) => entry.product));
    for (const entry of products) {
      if (entry.hasOptions) {
        store.replaceBaseOptionsForProduct(entry.product.id, entry.options);
      }
      if (entry.hasVariants) {
        store.replaceBaseVariantsForProduct(entry.product.id, entry.variants);
      }
      if (entry.hasCollections) {
        store.replaceBaseCollectionsForProduct(entry.product.id, entry.collections);
      }
      if (entry.hasMedia || entry.hasImages) {
        store.replaceBaseMediaForProduct(entry.product.id, entry.media);
      }
      if (entry.hasMetafields) {
        store.replaceBaseMetafieldsForProduct(entry.product.id, entry.metafields);
      }
    }
  }
}

export function handleProductMutation(
  document: string,
  variables: Record<string, unknown>,
  readMode: ReadMode,
): Record<string, unknown> {
  const field = getRootField(document);
  const args = getRootFieldArguments(document, variables);
  const responseKey = field.alias?.value ?? field.name.value;

  switch (field.name.value) {
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

      const existingProduct = store.getEffectiveProductById(productId);
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

      store.stageUpdateProduct(
        makeProductRecord({ id: productId, tags: normalizeProductTags(nextTags) }, existingProduct),
      );
      if (store.getBaseProductById(productId)) {
        store.markTagSearchLagged(productId);
      }
      const product = store.getEffectiveProductById(productId);
      return {
        data: {
          [responseKey]: {
            node: serializeProduct(product, getChildField(field, 'node'), variables),
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

      const existingProduct = store.getEffectiveProductById(productId);
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
      store.stageUpdateProduct(makeProductRecord({ id: productId, tags: nextTags }, existingProduct));
      if (store.getBaseProductById(productId)) {
        store.markTagSearchLagged(productId);
      }
      const product = store.getEffectiveProductById(productId);
      return {
        data: {
          [responseKey]: {
            node: serializeProduct(product, getChildField(field, 'node'), variables),
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

      const preparedCreateInput = prepareProductInputWithResolvedHandle(input);
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

      const product = store.stageCreateProduct(makeProductRecord(preparedCreateInput.input));
      store.replaceStagedOptionsForProduct(product.id, [makeDefaultOptionRecord(product)]);
      store.replaceStagedVariantsForProduct(product.id, [makeDefaultVariantRecord(product)]);
      const syncedProduct = syncProductInventorySummary(product.id);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(syncedProduct ?? product, getChildField(field, 'product'), variables),
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

      const existing = store.getEffectiveProductById(id) ?? undefined;
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
              product: serializeProduct(existing, getChildField(field, 'product'), variables),
              userErrors: [{ field: ['title'], message: "Title can't be blank" }],
            },
          },
        };
      }

      const preparedUpdateInput = prepareProductInputWithResolvedHandle({ ...input, id }, existing);
      if (preparedUpdateInput.error) {
        return {
          data: {
            [responseKey]: {
              product: serializeProduct(
                existing ?? store.getEffectiveProductById(id),
                getChildField(field, 'product'),
                variables,
              ),
              userErrors: [preparedUpdateInput.error],
            },
          },
        };
      }

      store.stageUpdateProduct(makeProductRecord(preparedUpdateInput.input, existing));
      const product = store.getEffectiveProductById(id);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(product, getChildField(field, 'product'), variables),
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

      const existing = store.getEffectiveProductById(id);
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

      store.stageDeleteProduct(id);
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
            [responseKey]: {
              newProduct: null,
              userErrors: [{ field: ['productId'], message: 'Product id is required' }],
            },
          },
        };
      }

      const sourceProduct = store.getEffectiveProductById(productId);
      if (!sourceProduct) {
        return {
          data: {
            [responseKey]: {
              newProduct: null,
              userErrors: [{ field: ['productId'], message: 'Product not found' }],
            },
          },
        };
      }

      const duplicatedRecord = makeDuplicatedProductRecord(
        sourceProduct,
        typeof args['newTitle'] === 'string' ? args['newTitle'] : undefined,
      );
      const duplicatedProduct = store.stageCreateProduct(
        makeProductRecord(
          {
            ...duplicatedRecord,
            handle: ensureUniqueProductHandle(duplicatedRecord.handle),
          },
          duplicatedRecord,
        ),
      );
      store.replaceStagedOptionsForProduct(
        duplicatedProduct.id,
        store
          .getEffectiveOptionsByProductId(productId)
          .map((option) => duplicateOptionRecord(option, duplicatedProduct.id)),
      );
      store.replaceStagedVariantsForProduct(
        duplicatedProduct.id,
        store
          .getEffectiveVariantsByProductId(productId)
          .map((variant) => duplicateVariantRecord(variant, duplicatedProduct.id)),
      );
      store.replaceStagedCollectionsForProduct(
        duplicatedProduct.id,
        store
          .getEffectiveCollectionsByProductId(productId)
          .map((collection) => duplicateCollectionRecord(collection, duplicatedProduct.id)),
      );
      // Captured Shopify duplicate responses keep immediate duplicate media empty even when the source has ready media.
      store.replaceStagedMediaForProduct(duplicatedProduct.id, []);
      store.replaceStagedMetafieldsForProduct(
        duplicatedProduct.id,
        store
          .getEffectiveMetafieldsByProductId(productId)
          .map((metafield) => duplicateMetafieldRecord(metafield, duplicatedProduct.id)),
      );
      const product =
        syncProductInventorySummary(duplicatedProduct.id) ?? store.getEffectiveProductById(duplicatedProduct.id);

      return {
        data: {
          [responseKey]: {
            newProduct: serializeProduct(product, getChildField(field, 'newProduct'), variables),
            userErrors: [],
          },
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
        (identifierId ? store.getEffectiveProductById(identifierId) : null) ??
        (inputId ? store.getEffectiveProductById(inputId) : null) ??
        (identifierHandle ? findEffectiveProductByHandle(identifierHandle) : null);
      const productInput =
        !existing && !hasOwnField(input, 'descriptionHtml') ? { ...input, descriptionHtml: '' } : input;
      const preparedInput = prepareProductInputWithResolvedHandle(
        existing ? { ...productInput, id: existing.id } : productInput,
        existing ?? undefined,
      );
      if (preparedInput.error) {
        return {
          data: {
            [responseKey]: {
              product: synchronous ? serializeProduct(existing, getChildField(field, 'product'), variables) : null,
              productSetOperation: null,
              userErrors: [preparedInput.error],
            },
          },
        };
      }

      const productRecord = makeProductRecord(preparedInput.input, existing ?? undefined);
      const stagedProduct = existing
        ? store.stageUpdateProduct(productRecord)
        : store.stageCreateProduct({
            ...productRecord,
            onlineStorePreviewUrl:
              productRecord.onlineStorePreviewUrl ?? makeSyntheticOnlineStorePreviewUrl(productRecord),
          });
      const productId = stagedProduct.id;

      if (hasOwnField(input, 'productOptions')) {
        store.replaceStagedOptionsForProduct(
          productId,
          buildProductSetOptionRecords(productId, input['productOptions']),
        );
      } else if (!existing && store.getEffectiveOptionsByProductId(productId).length === 0) {
        store.replaceStagedOptionsForProduct(productId, [makeDefaultOptionRecord(stagedProduct)]);
      }

      if (hasOwnField(input, 'variants')) {
        const nextVariants = buildProductSetVariantRecords(productId, input['variants']);
        store.replaceStagedVariantsForProduct(productId, nextVariants);
      } else if (!existing && store.getEffectiveVariantsByProductId(productId).length === 0) {
        store.replaceStagedVariantsForProduct(productId, [makeDefaultVariantRecord(stagedProduct)]);
      }

      if (hasOwnField(input, 'productOptions') || hasOwnField(input, 'variants')) {
        store.replaceStagedOptionsForProduct(
          productId,
          syncProductOptionsWithVariants(
            productId,
            store.getEffectiveOptionsByProductId(productId),
            store.getEffectiveVariantsByProductId(productId),
          ),
        );
      }

      if (hasOwnField(input, 'collections')) {
        store.replaceStagedCollectionsForProduct(
          productId,
          buildProductSetCollectionRecords(productId, input['collections']),
        );
      }

      if (hasOwnField(input, 'metafields')) {
        store.replaceStagedMetafieldsForProduct(
          productId,
          buildProductSetMetafieldRecords(productId, input['metafields']),
        );
      }

      const product = syncProductInventorySummary(productId) ?? store.getEffectiveProductById(productId);
      return {
        data: {
          [responseKey]: {
            product: synchronous ? serializeProduct(product, getChildField(field, 'product'), variables) : null,
            productSetOperation: synchronous
              ? null
              : serializeProductSetOperation(getChildField(field, 'productSetOperation')),
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

      const existing = store.getEffectiveProductById(productId);
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

      store.stageUpdateProduct(
        makeProductRecord(
          {
            id: productId,
            status: rawStatus,
          },
          existing,
        ),
      );
      const product = store.getEffectiveProductById(productId);

      return {
        data: {
          [responseKey]: {
            product: serializeProduct(product, getChildField(field, 'product'), variables),
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

      const existing = store.getEffectiveProductById(productId);
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
      store.stageUpdateProduct(makeProductRecord({ id: productId, publicationIds: nextPublicationIds }, existing));
      const product = store.getEffectiveProductById(productId);

      return {
        data: {
          [responseKey]: {
            ...(productField ? { product: serializeProduct(product, productField, variables) } : {}),
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
            [responseKey]: serializeProductMutationPayload(field, variables, {
              product: null,
              userErrors: [{ field: ['input', 'id'], message: 'Product id is required' }],
            }),
          },
        };
      }

      const existing = store.getEffectiveProductById(productId);
      if (!existing) {
        return {
          data: {
            [responseKey]: serializeProductMutationPayload(field, variables, {
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
      store.stageUpdateProduct(makeProductRecord({ id: productId, publicationIds: nextPublicationIds }, existing));
      const product = store.getEffectiveProductById(productId);

      return {
        data: {
          [responseKey]: serializeProductMutationPayload(field, variables, {
            product,
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
            [responseKey]: serializePublishableMutationPayload(field, variables, {
              publishable: null,
              userErrors: [{ field: ['id'], message: 'Publishable id is required' }],
            }),
          },
        };
      }

      const isPublish = field.name.value === 'publishablePublish';
      const publicationTargets = readPublicationTargets(args['input']);
      const existingProduct = store.getEffectiveProductById(publishableId);
      if (existingProduct) {
        if (publicationTargets.length === 0) {
          return {
            data: {
              [responseKey]: serializePublishableMutationPayload(field, variables, {
                publishable: existingProduct,
                userErrors: [{ field: ['input'], message: 'Publication target is required' }],
              }),
            },
          };
        }

        const nextPublicationIds = isPublish
          ? mergePublicationTargets(existingProduct.publicationIds, publicationTargets)
          : removePublicationTargets(existingProduct.publicationIds, publicationTargets);
        store.stageUpdateProduct(
          makeProductRecord({ id: publishableId, publicationIds: nextPublicationIds }, existingProduct),
        );

        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(field, variables, {
              publishable: store.getEffectiveProductById(publishableId),
              userErrors: [],
            }),
          },
        };
      }

      if (publishableId.startsWith('gid://shopify/Product/')) {
        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(field, variables, {
              publishable: null,
              userErrors: [{ field: ['id'], message: 'Product not found' }],
            }),
          },
        };
      }

      const existingCollection = findEffectiveCollectionById(publishableId);
      if (existingCollection) {
        if (publicationTargets.length === 0) {
          return {
            data: {
              [responseKey]: serializePublishableMutationPayload(field, variables, {
                publishable: existingCollection,
                userErrors: [{ field: ['input'], message: 'Publication target is required' }],
              }),
            },
          };
        }

        const nextPublicationIds = isPublish
          ? mergePublicationTargets(existingCollection.publicationIds ?? [], publicationTargets)
          : removePublicationTargets(existingCollection.publicationIds ?? [], publicationTargets);
        store.stageUpdateCollection(
          makeCollectionRecord({ id: publishableId, publicationIds: nextPublicationIds }, existingCollection),
        );

        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(field, variables, {
              publishable: findEffectiveCollectionById(publishableId),
              userErrors: [],
            }),
          },
        };
      }

      if (publishableId.startsWith('gid://shopify/Collection/')) {
        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(field, variables, {
              publishable: null,
              userErrors: [{ field: ['id'], message: 'Collection not found' }],
            }),
          },
        };
      }

      return {
        data: {
          [responseKey]: serializePublishableMutationPayload(field, variables, {
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
            [responseKey]: serializePublishableMutationPayload(field, variables, {
              publishable: null,
              userErrors: [{ field: ['id'], message: 'Only Product publishable IDs are supported locally' }],
            }),
          },
        };
      }

      const existing = store.getEffectiveProductById(productId);
      if (!existing) {
        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(field, variables, {
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
            [responseKey]: serializePublishableMutationPayload(field, variables, {
              publishable: existing,
              userErrors: [{ field: ['input'], message: 'Publication target is required' }],
            }),
          },
        };
      }

      const nextPublicationIds = mergePublicationTargets(existing.publicationIds, publicationTargets);
      store.stageUpdateProduct(makeProductRecord({ id: productId, publicationIds: nextPublicationIds }, existing));
      const product = store.getEffectiveProductById(productId);

      return {
        data: {
          [responseKey]: serializePublishableMutationPayload(field, variables, {
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
            [responseKey]: serializePublishableMutationPayload(field, variables, {
              publishable: null,
              userErrors: [{ field: ['id'], message: 'Only Product publishable IDs are supported locally' }],
            }),
          },
        };
      }

      const existing = store.getEffectiveProductById(productId);
      if (!existing) {
        return {
          data: {
            [responseKey]: serializePublishableMutationPayload(field, variables, {
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
            [responseKey]: serializePublishableMutationPayload(field, variables, {
              publishable: existing,
              userErrors: [{ field: ['input'], message: 'Publication target is required' }],
            }),
          },
        };
      }

      const nextPublicationIds = removePublicationTargets(existing.publicationIds, publicationTargets);
      store.stageUpdateProduct(makeProductRecord({ id: productId, publicationIds: nextPublicationIds }, existing));
      const product = store.getEffectiveProductById(productId);

      return {
        data: {
          [responseKey]: serializePublishableMutationPayload(field, variables, {
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

      const existingProduct = store.getEffectiveProductById(productId);
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

      const existingOptions = store.getEffectiveOptionsByProductId(productId);
      const existingVariants = store.getEffectiveVariantsByProductId(productId);
      let nextOptions = existingOptions;
      const optionInputs = Array.isArray(args['options']) ? args['options'] : [];
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
          makeCreatedOptionRecord(productId, optionInput),
          optionInput['position'],
        );
      }

      let nextVariants = existingVariants;
      if (shouldReplaceDefaultOptionState && existingVariants[0]) {
        nextVariants = [remapDefaultVariantToCreatedOptions(existingVariants[0], nextOptions)];
        store.replaceStagedVariantsForProduct(productId, nextVariants);
      }

      nextOptions = syncProductOptionsWithVariants(productId, nextOptions, nextVariants);
      store.replaceStagedOptionsForProduct(productId, nextOptions);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(
              store.getEffectiveProductById(productId),
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

      const existingProduct = store.getEffectiveProductById(productId);
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
      const updateResult = updateOptionRecords(
        productId,
        store.getEffectiveOptionsByProductId(productId),
        store.getEffectiveVariantsByProductId(productId),
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

      store.replaceStagedVariantsForProduct(productId, updateResult.variants);
      const syncedOptions = syncProductOptionsWithVariants(productId, updateResult.options, updateResult.variants);
      store.replaceStagedOptionsForProduct(productId, syncedOptions);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(
              store.getEffectiveProductById(productId),
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

      const existingProduct = store.getEffectiveProductById(productId);
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

      const effectiveOptions = store.getEffectiveOptionsByProductId(productId);
      const effectiveVariants = store.getEffectiveVariantsByProductId(productId);
      const deleteResult = deleteOptionRecords(productId, effectiveOptions, args['options']);
      let nextOptions = deleteResult.options;
      let nextVariants = effectiveVariants;
      if (nextOptions.length === 0) {
        const restoredDefaultState = restoreDefaultOptionState(existingProduct, effectiveVariants);
        nextOptions = restoredDefaultState.options;
        nextVariants = restoredDefaultState.variants;
        store.replaceStagedVariantsForProduct(productId, nextVariants);
      }
      store.replaceStagedOptionsForProduct(
        productId,
        syncProductOptionsWithVariants(productId, nextOptions, nextVariants),
      );
      return {
        data: {
          [responseKey]: {
            deletedOptionsIds: deleteResult.deletedOptionIds,
            product: serializeProduct(
              store.getEffectiveProductById(productId),
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

      const collection = store.stageCreateCollection(makeCollectionRecord(input));
      return {
        data: {
          [responseKey]: {
            collection: serializeCollectionObject(
              collection,
              getChildField(field, 'collection')?.selectionSet?.selections ?? [],
              variables,
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

      const existing = findEffectiveCollectionById(collectionId);
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

      const collection = store.stageUpdateCollection(makeCollectionRecord(input, existing));
      return {
        data: {
          [responseKey]: {
            collection: serializeCollectionObject(
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

      const existing = findEffectiveCollectionById(collectionId);
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

      store.stageDeleteCollection(collectionId);
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

      const existing = findEffectiveCollectionById(collectionId);
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
      const result = addProductsToCollection(existing, productIds);
      return {
        data: {
          [responseKey]: {
            collection: result.collection
              ? serializeCollectionObject(
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

      const existing = findEffectiveCollectionById(collectionId);
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
      removeProductsFromCollection(existing, productIds);
      const job = { id: makeSyntheticGid('Job'), done: false };
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

      const existing = findEffectiveCollectionById(collectionId);
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

      const result = reorderCollectionProducts(existing, args['moves']);
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
    case 'productCreateMedia': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
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

      const existingProduct = store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              media: [],
              mediaUserErrors: [{ field: ['productId'], message: 'Product not found' }],
              product: null,
            },
          },
        };
      }

      const existingMedia = store.getEffectiveMediaByProductId(productId);
      const createdMedia = (Array.isArray(args['media']) ? args['media'] : [])
        .filter((media): media is Record<string, unknown> => isObject(media))
        .map((media, index) => makeCreatedMediaRecord(productId, media, existingMedia.length + index));
      const nextMedia = [...existingMedia, ...createdMedia];
      store.replaceStagedMediaForProduct(productId, nextMedia);

      const response = {
        data: {
          [responseKey]: {
            media: serializeMediaPayload(createdMedia, getChildField(field, 'media')),
            mediaUserErrors: [],
            product: serializeProduct(
              store.getEffectiveProductById(productId),
              getChildField(field, 'product'),
              variables,
            ),
          },
        },
      };

      store.replaceStagedMediaForProduct(productId, [
        ...existingMedia,
        ...createdMedia.map((mediaRecord) => transitionMediaToProcessing(mediaRecord)),
      ]);

      return response;
    }
    case 'productUpdateMedia': {
      const rawProductId = args['productId'];
      const productId = typeof rawProductId === 'string' ? rawProductId : null;
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

      const existingProduct = store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              media: [],
              mediaUserErrors: [{ field: ['productId'], message: 'Product not found' }],
            },
          },
        };
      }

      const effectiveMedia = store.getEffectiveMediaByProductId(productId);
      const updates = (Array.isArray(args['media']) ? args['media'] : []).filter(
        (media): media is Record<string, unknown> => isObject(media),
      );
      const missingMediaId = updates.find(
        (media) => typeof media['id'] !== 'string' || !effectiveMedia.some((candidate) => candidate.id === media['id']),
      );
      if (missingMediaId) {
        return {
          data: {
            [responseKey]: {
              media: [],
              mediaUserErrors: [{ field: ['media', 'id'], message: 'Media id is required' }],
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

      store.replaceStagedMediaForProduct(productId, nextMedia);
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

      const existingProduct = store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              deletedMediaIds: [],
              deletedProductImageIds: [],
              mediaUserErrors: [{ field: ['productId'], message: 'Product not found' }],
              product: null,
            },
          },
        };
      }

      const mediaIds = Array.isArray(args['mediaIds'])
        ? args['mediaIds'].filter((mediaId): mediaId is string => typeof mediaId === 'string')
        : [];
      const effectiveMedia = store.getEffectiveMediaByProductId(productId);
      const deletedMedia = effectiveMedia.filter(
        (mediaRecord) => typeof mediaRecord.id === 'string' && mediaIds.includes(mediaRecord.id),
      );
      const nextMedia = effectiveMedia.filter(
        (mediaRecord) => typeof mediaRecord.id !== 'string' || !mediaIds.includes(mediaRecord.id),
      );
      store.replaceStagedMediaForProduct(productId, nextMedia);

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
              store.getEffectiveProductById(productId),
              getChildField(field, 'product'),
              variables,
            ),
          },
        },
      };
    }
    case 'inventoryItemUpdate': {
      const rawId = args['id'];
      const inventoryItemId = typeof rawId === 'string' ? rawId : null;
      const existingVariant = inventoryItemId ? store.findEffectiveVariantByInventoryItemId(inventoryItemId) : null;
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
        inventoryItem: readInventoryItemInput(args['input'], existingVariant.inventoryItem),
      };
      const productId = existingVariant.productId;
      const nextVariants = store
        .getEffectiveVariantsByProductId(productId)
        .map((variant) => (variant.id === existingVariant.id ? nextVariant : variant));
      store.replaceStagedVariantsForProduct(productId, nextVariants);
      syncProductInventorySummary(productId);
      const updatedVariant = store.getEffectiveVariantById(existingVariant.id) ?? nextVariant;

      return {
        data: {
          [responseKey]: {
            inventoryItem: serializeInventoryItemSelectionSet(
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
      const invalidVariableError = validateInventoryAdjustRequiredFields(input);
      if (invalidVariableError) {
        return invalidVariableError;
      }

      const result = applyInventoryAdjustQuantities(input);
      const inventoryAdjustmentGroupField = getChildField(field, 'inventoryAdjustmentGroup');
      const staffMemberField = inventoryAdjustmentGroupField
        ? getChildField(inventoryAdjustmentGroupField, 'staffMember')
        : null;
      const response: Record<string, unknown> = {
        data: {
          [responseKey]: {
            inventoryAdjustmentGroup: serializeInventoryAdjustmentGroup(result.group, inventoryAdjustmentGroupField),
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
    case 'inventoryActivate': {
      const rawInventoryItemId = args['inventoryItemId'];
      const rawLocationId = args['locationId'];
      const inventoryItemId = typeof rawInventoryItemId === 'string' ? rawInventoryItemId : null;
      const locationId = typeof rawLocationId === 'string' ? rawLocationId : null;
      const variant = inventoryItemId ? store.findEffectiveVariantByInventoryItemId(inventoryItemId) : null;
      const knownLocation = locationId ? findKnownLocationById(locationId) : null;
      const level =
        variant && locationId
          ? (getEffectiveInventoryLevels(variant).find((candidate) => candidate.location?.id === locationId) ?? null)
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
        const nextLevel = buildActivatedInventoryLevel(variant, knownLocation);
        if (nextLevel) {
          resolvedVariant = stageVariantInventoryLevels(variant, [...getEffectiveInventoryLevels(variant), nextLevel]);
          resolvedLevel =
            getEffectiveInventoryLevels(resolvedVariant).find(
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
      const target = inventoryLevelId ? findInventoryLevelTarget(inventoryLevelId) : null;
      const allLevels = target ? getEffectiveInventoryLevels(target.variant) : [];
      const userErrors: InventoryMutationUserError[] = [];

      if (target && allLevels.length <= 1) {
        userErrors.push({
          field: null,
          message: `The product couldn't be unstocked from ${target.level.location?.name ?? 'this location'} because products need to be stocked at a minimum of 1 location.`,
        });
      }

      if (target && userErrors.length === 0) {
        stageVariantInventoryLevels(
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
      const variant = inventoryItemId ? store.findEffectiveVariantByInventoryItemId(inventoryItemId) : null;
      const updates = Array.isArray(args['inventoryItemUpdates'])
        ? args['inventoryItemUpdates'].filter((value): value is Record<string, unknown> => isObject(value))
        : [];
      const firstUpdate = updates[0] ?? null;
      const locationId = typeof firstUpdate?.['locationId'] === 'string' ? firstUpdate['locationId'] : null;
      const activate = typeof firstUpdate?.['activate'] === 'boolean' ? firstUpdate['activate'] : null;
      const knownLocation = locationId ? findKnownLocationById(locationId) : null;
      const level =
        variant && locationId
          ? (getEffectiveInventoryLevels(variant).find((candidate) => candidate.location?.id === locationId) ?? null)
          : null;
      const userErrors: InventoryMutationUserError[] = [];

      if (variant && locationId && !level && !knownLocation) {
        userErrors.push({
          field: ['inventoryItemUpdates', '0', 'locationId'],
          message: "The quantity couldn't be updated because the location was not found.",
          code: 'LOCATION_NOT_FOUND',
        });
      }

      if (variant && activate === false && level && getEffectiveInventoryLevels(variant).length <= 1) {
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
          const nextLevel = buildActivatedInventoryLevel(variant, knownLocation);
          if (nextLevel) {
            resolvedVariant = stageVariantInventoryLevels(variant, [
              ...getEffectiveInventoryLevels(variant),
              nextLevel,
            ]);
            const resolvedLevel =
              getEffectiveInventoryLevels(resolvedVariant).find(
                (candidate) => candidate.location?.id === knownLocation.id,
              ) ?? nextLevel;
            responseLevels = [
              serializeInventoryLevelObject(
                resolvedVariant,
                resolvedLevel,
                getChildField(field, 'inventoryLevels')?.selectionSet?.selections ?? [],
                variables,
              ),
            ];
          }
        } else if (activate === false && level) {
          resolvedVariant = stageVariantInventoryLevels(
            variant,
            getEffectiveInventoryLevels(variant).filter((candidate) => candidate.id !== level.id),
          );
          responseLevels = [];
        } else if (level) {
          responseLevels = [
            serializeInventoryLevelObject(
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
      const inputs = readMetafieldsSetInput(args['metafields']);
      if (inputs.length === 0) {
        return {
          data: {
            [responseKey]: {
              metafields: [],
              userErrors: [{ field: ['metafields'], message: 'At least one metafield input is required' }],
            },
          },
        };
      }

      const firstInvalidInput = inputs.find((input) => {
        const ownerId = input['ownerId'];
        const namespace = input['namespace'];
        const key = input['key'];
        return (
          typeof ownerId !== 'string' ||
          !store.getEffectiveProductById(ownerId) ||
          typeof namespace !== 'string' ||
          !namespace.trim() ||
          typeof key !== 'string' ||
          !key.trim()
        );
      });
      if (firstInvalidInput) {
        const ownerId = firstInvalidInput['ownerId'];
        const namespace = firstInvalidInput['namespace'];
        const key = firstInvalidInput['key'];
        const fieldName =
          typeof ownerId !== 'string'
            ? 'ownerId'
            : !store.getEffectiveProductById(ownerId)
              ? 'ownerId'
              : typeof namespace !== 'string' || !namespace.trim()
                ? 'namespace'
                : 'key';
        const message =
          typeof ownerId !== 'string'
            ? 'Product ownerId is required'
            : !store.getEffectiveProductById(ownerId)
              ? 'Product not found'
              : typeof namespace !== 'string' || !namespace.trim()
                ? 'Metafield namespace is required'
                : 'Metafield key is required';

        return {
          data: {
            [responseKey]: {
              metafields: [],
              userErrors: [{ field: ['metafields', fieldName], message }],
            },
          },
        };
      }

      const inputsByProductId = new Map<string, Record<string, unknown>[]>();
      for (const input of inputs) {
        const ownerId = input['ownerId'] as string;
        const productInputs = inputsByProductId.get(ownerId) ?? [];
        productInputs.push(input);
        inputsByProductId.set(ownerId, productInputs);
      }

      const createdOrUpdated: ProductMetafieldRecord[] = [];
      for (const [productId, productInputs] of inputsByProductId.entries()) {
        const updateResult = upsertMetafieldsForProduct(productId, productInputs);
        store.replaceStagedMetafieldsForProduct(productId, updateResult.metafields);
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
      const deleteResult = deleteMetafieldsByIdentifiers(readMetafieldsDeleteInput(args['metafields']));
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

      const existingMetafield = findMetafieldById(metafieldId);
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

      const deleteResult = deleteMetafieldsByIdentifiers([
        {
          ownerId: existingMetafield.productId,
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

      const existingProduct = store.getEffectiveProductById(productId);
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

      const effectiveVariants = store.getEffectiveVariantsByProductId(productId);
      const createdVariant = makeCreatedVariantRecord(productId, input, effectiveVariants[0] ?? null);
      const nextVariants = [...effectiveVariants, createdVariant];
      store.replaceStagedVariantsForProduct(productId, nextVariants);
      store.replaceStagedOptionsForProduct(
        productId,
        syncProductOptionsWithVariants(productId, undefined, nextVariants),
      );
      const product = syncProductInventorySummary(productId);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(product, getChildField(field, 'product'), variables),
            productVariant: serializeVariantSelectionSet(
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

      const existingVariant = findEffectiveVariantById(variantId);
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
      const nextVariants = store.getEffectiveVariantsByProductId(productId).map((variant) => {
        if (variant.id !== variantId) {
          return variant;
        }

        const updatedVariant = updateVariantRecord(variant, input);
        updatedVariants.push(updatedVariant);
        return updatedVariant;
      });
      store.replaceStagedVariantsForProduct(productId, nextVariants);
      store.replaceStagedOptionsForProduct(
        productId,
        syncProductOptionsWithVariants(productId, undefined, nextVariants),
      );
      store.markVariantSearchLagged(productId);
      const product = syncProductInventorySummary(productId);
      const updatedVariant = updatedVariants[0] ?? null;
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(product, getChildField(field, 'product'), variables),
            productVariant: updatedVariant
              ? serializeVariantSelectionSet(
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

      const existingVariant = findEffectiveVariantById(variantId);
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
      const nextVariants = store
        .getEffectiveVariantsByProductId(productId)
        .filter((variant) => variant.id !== variantId);
      store.replaceStagedVariantsForProduct(productId, nextVariants);
      store.replaceStagedOptionsForProduct(
        productId,
        syncProductOptionsWithVariants(productId, undefined, nextVariants),
      );
      syncProductInventorySummary(productId);
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

      const existingProduct = store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              product: null,
              productVariants: [],
              userErrors: [{ field: ['productId'], message: 'Product not found' }],
            },
          },
        };
      }

      const effectiveVariants = store.getEffectiveVariantsByProductId(productId);
      const defaultVariant = effectiveVariants[0] ?? null;
      const createdVariants = (Array.isArray(args['variants']) ? args['variants'] : [])
        .filter((variant): variant is Record<string, unknown> => isObject(variant))
        .map((variant) => makeCreatedVariantRecord(productId, variant, defaultVariant));
      const nextVariants = [...effectiveVariants, ...createdVariants];
      store.replaceStagedVariantsForProduct(productId, nextVariants);
      store.replaceStagedOptionsForProduct(
        productId,
        syncProductOptionsWithVariants(productId, undefined, nextVariants),
      );
      store.markVariantSearchLagged(productId);
      const product = syncProductInventorySummary(productId);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(product, getChildField(field, 'product'), variables),
            productVariants: serializeVariantPayload(createdVariants, getChildField(field, 'productVariants')),
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

      const existingProduct = store.getEffectiveProductById(productId);
      if (!existingProduct) {
        return {
          data: {
            [responseKey]: {
              product: null,
              productVariants: [],
              userErrors: [{ field: ['productId'], message: 'Product not found' }],
            },
          },
        };
      }

      const effectiveVariants = store.getEffectiveVariantsByProductId(productId);
      const variantsById = new Map(effectiveVariants.map((variant) => [variant.id, variant]));
      const updates = (Array.isArray(args['variants']) ? args['variants'] : []).filter(
        (variant): variant is Record<string, unknown> => isObject(variant),
      );
      const inventoryQuantityUpdateIndex = updates.findIndex((variant) => hasOwnField(variant, 'inventoryQuantities'));
      if (inventoryQuantityUpdateIndex >= 0) {
        return {
          data: {
            [responseKey]: {
              product: null,
              productVariants: [],
              userErrors: [
                {
                  field: ['variants', String(inventoryQuantityUpdateIndex), 'inventoryQuantities'],
                  message:
                    'Inventory quantities can only be provided during create. To update inventory for existing variants, use inventoryAdjustQuantities.',
                },
              ],
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

        const updatedVariant = updateVariantRecord(variant, update);
        updatedVariants.push(updatedVariant);
        return updatedVariant;
      });

      const missingVariantId = updates.find(
        (variant) => typeof variant['id'] !== 'string' || !variantsById.has(variant['id']),
      );
      if (missingVariantId) {
        return {
          data: {
            [responseKey]: {
              product: null,
              productVariants: [],
              userErrors: [{ field: ['variants', 'id'], message: 'Variant id is required' }],
            },
          },
        };
      }

      store.replaceStagedVariantsForProduct(productId, nextVariants);
      store.replaceStagedOptionsForProduct(
        productId,
        syncProductOptionsWithVariants(productId, undefined, nextVariants),
      );
      store.markVariantSearchLagged(productId);
      const product = syncProductInventorySummary(productId);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(product, getChildField(field, 'product'), variables),
            productVariants: serializeVariantPayload(updatedVariants, getChildField(field, 'productVariants')),
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

      const existingProduct = store.getEffectiveProductById(productId);
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

      const variantIds = Array.isArray(args['variantsIds'])
        ? args['variantsIds'].filter((variantId): variantId is string => typeof variantId === 'string')
        : [];
      const nextVariants = store
        .getEffectiveVariantsByProductId(productId)
        .filter((variant) => !variantIds.includes(variant.id));
      store.replaceStagedVariantsForProduct(productId, nextVariants);
      store.replaceStagedOptionsForProduct(
        productId,
        syncProductOptionsWithVariants(productId, undefined, nextVariants),
      );
      store.markVariantSearchLagged(productId);
      const product = syncProductInventorySummary(productId);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(product, getChildField(field, 'product'), variables),
            userErrors: [],
          },
        },
      };
    }
    default:
      throw new Error(`Unsupported product mutation field: ${field.name.value}`);
  }
}

export function handleProductQuery(
  document: string,
  variables: Record<string, unknown>,
  readMode: ReadMode,
): Record<string, unknown> {
  const fields = getRootFields(document);
  const data: Record<string, unknown> = {};

  for (const field of fields) {
    const args = getFieldArguments(field, variables);
    const responseKey = field.alias?.value ?? field.name.value;

    switch (field.name.value) {
      case 'product': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        const product = id ? store.getEffectiveProductById(id) : null;
        data[responseKey] = serializeProduct(product, field, variables);
        break;
      }
      case 'products': {
        const rawFirst = args['first'];
        const rawLast = args['last'];
        const first = typeof rawFirst === 'number' ? rawFirst : null;
        const last = typeof rawLast === 'number' ? rawLast : null;
        data[responseKey] = serializeProductsConnection(
          store.listEffectiveProducts(),
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
        data[responseKey] = serializeProductsCount(args['query'], field.selectionSet?.selections ?? []);
        break;
      }
      case 'productVariant': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        const variant = id ? store.getEffectiveVariantById(id) : null;
        data[responseKey] = variant
          ? serializeVariantSelectionSet(variant, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'inventoryItem': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        const variant = id ? store.findEffectiveVariantByInventoryItemId(id) : null;
        data[responseKey] = variant
          ? serializeInventoryItemSelectionSet(variant, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'inventoryLevel': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        const target = id ? findInventoryLevelTarget(id) : null;
        data[responseKey] = target
          ? serializeInventoryLevelObject(target.variant, target.level, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'collection': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        const collection = id ? findEffectiveCollectionById(id) : null;
        data[responseKey] = collection
          ? serializeCollectionObject(collection, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'collectionByIdentifier': {
        const identifier = readProductInput(args['identifier']);
        const collection = findEffectiveCollectionByIdentifier(identifier);
        data[responseKey] = collection
          ? serializeCollectionObject(collection, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'collectionByHandle': {
        const rawHandle = args['handle'];
        const handle = typeof rawHandle === 'string' ? rawHandle : null;
        const collection = handle ? findEffectiveCollectionByHandle(handle) : null;
        data[responseKey] = collection
          ? serializeCollectionObject(collection, field.selectionSet?.selections ?? [], variables)
          : null;
        break;
      }
      case 'collections': {
        data[responseKey] = serializeTopLevelCollectionsConnection(field, variables);
        break;
      }
      case 'locations': {
        data[responseKey] = serializeTopLevelLocationsConnection(field, variables);
        break;
      }
      case 'publications': {
        data[responseKey] = serializeTopLevelPublicationsConnection(field, variables);
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

  return { data };
}
