import { Kind, type FieldNode, type SelectionNode } from 'graphql';
import type { ReadMode } from '../config.js';
import { getFieldArguments, getRootField, getRootFieldArguments, getRootFields } from '../graphql/root-field.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type {
  CollectionRecord,
  ProductCollectionRecord,
  ProductMediaRecord,
  ProductMetafieldRecord,
  ProductOptionRecord,
  ProductRecord,
  ProductVariantRecord,
} from '../state/types.js';

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function hasOwnField(value: Record<string, unknown>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function slugifyHandle(title: string): string {
  const normalized = title
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');

  return normalized || 'untitled-product';
}

function readProductInput(raw: unknown): Record<string, unknown> {
  return isObject(raw) ? raw : {};
}

function findEffectiveProductByHandle(handle: string): ProductRecord | null {
  return store.listEffectiveProducts().find((product) => product.handle === handle) ?? null;
}

function findEffectiveCollectionById(collectionId: string): CollectionRecord | null {
  return store.getEffectiveCollectionById(collectionId);
}

function listEffectiveCollections(): CollectionRecord[] {
  return store.listEffectiveCollections();
}

function listEffectiveProductsForCollection(collectionId: string): ProductRecord[] {
  return store
    .listEffectiveProducts()
    .filter((product) =>
      store.getEffectiveCollectionsByProductId(product.id).some((collection) => collection.id === collectionId),
    );
}

function makeProductCollectionRecord(productId: string, collection: CollectionRecord): ProductCollectionRecord {
  return {
    id: collection.id,
    productId,
    title: collection.title,
    handle: collection.handle,
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

  const duplicateMembership = normalizedProductIds.find((productId) =>
    store.getEffectiveCollectionsByProductId(productId).some((candidate) => candidate.id === collection.id),
  );
  if (duplicateMembership) {
    return {
      collection: null,
      userErrors: [{ field: ['productIds'], message: 'Product is already in the collection' }],
    };
  }

  const missingProductId = normalizedProductIds.find((productId) => !store.getEffectiveProductById(productId));
  if (missingProductId) {
    return {
      collection: null,
      userErrors: [{ field: ['productIds'], message: 'Product not found' }],
    };
  }

  for (const productId of normalizedProductIds) {
    const nextCollections = [
      ...store.getEffectiveCollectionsByProductId(productId),
      makeProductCollectionRecord(productId, collection),
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

function readPublicationIds(raw: unknown, fallback: string[] = []): string[] {
  if (!Array.isArray(raw)) {
    return structuredClone(fallback);
  }

  return raw.filter((value): value is string => typeof value === 'string' && value.length > 0);
}

function readPublicationTargets(raw: unknown): string[] {
  if (!Array.isArray(raw)) {
    return [];
  }

  const targets: string[] = [];
  for (const entry of raw) {
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

  const isSparseUpdate = typeof rawId === 'string' && !existing;
  const existingSeo = existing?.seo ?? { title: null, description: null };

  return {
    id: typeof rawId === 'string' ? rawId : (existing?.id ?? makeSyntheticGid('Product')),
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
    vendor: typeof rawVendor === 'string' ? rawVendor : (existing?.vendor ?? null),
    productType: typeof rawProductType === 'string' ? rawProductType : (existing?.productType ?? null),
    tags: Array.isArray(rawTags)
      ? rawTags.filter((tag): tag is string => typeof tag === 'string')
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

function makeCollectionRecord(input: Record<string, unknown>, existing?: CollectionRecord): CollectionRecord {
  const rawTitle = input['title'];
  const title = typeof rawTitle === 'string' && rawTitle.trim() ? rawTitle : (existing?.title ?? 'Untitled collection');
  const rawId = input['id'];
  const rawHandle = input['handle'];

  return {
    id: typeof rawId === 'string' ? rawId : (existing?.id ?? makeSyntheticGid('Collection')),
    title,
    handle:
      typeof rawHandle === 'string' && rawHandle.trim()
        ? rawHandle
        : (existing?.handle ?? slugifyHandle(title).replace(/product$/u, 'collection')),
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
    inventoryQuantity: null,
    selectedOptions: [],
    inventoryItem: null,
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
    id: makeSyntheticGid('Product'),
    legacyResourceId: null,
    title,
    handle: slugifyHandle(title),
    status: 'DRAFT',
    publicationIds: [],
    createdAt: now,
    updatedAt: now,
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

function duplicateMediaRecord(media: ProductMediaRecord, productId: string): ProductMediaRecord {
  const duplicatedId = makeSyntheticMediaId(media.mediaContentType);

  return {
    key: `${productId}:media:${media.position}`,
    productId,
    position: media.position,
    id: duplicatedId,
    mediaContentType: media.mediaContentType,
    alt: media.alt,
    status: media.status ?? null,
    imageUrl: media.imageUrl ?? media.previewImageUrl,
    previewImageUrl: media.previewImageUrl,
  };
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
  const imageUrl = typeof rawOriginalSource === 'string' && rawOriginalSource.trim() ? rawOriginalSource : null;

  return {
    key: `${productId}:media:${position}`,
    productId,
    position,
    id: makeSyntheticMediaId(mediaContentType),
    mediaContentType,
    alt: typeof rawAlt === 'string' ? rawAlt : null,
    status: 'UPLOADED',
    imageUrl,
    previewImageUrl: imageUrl,
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
        : (existing.imageUrl ?? existing.previewImageUrl);

  return {
    ...structuredClone(existing),
    alt: typeof rawAlt === 'string' ? rawAlt : existing.alt,
    imageUrl: nextImageUrl,
    previewImageUrl: nextImageUrl,
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

function makeCreatedVariantRecord(productId: string, input: Record<string, unknown>): ProductVariantRecord {
  const selectedOptions = readSelectedOptionsInput(input['selectedOptions']);
  return {
    id: makeSyntheticGid('ProductVariant'),
    productId,
    title: deriveVariantTitle(input['title'], selectedOptions, 'Default Title'),
    sku: typeof input['sku'] === 'string' ? input['sku'] : null,
    barcode: typeof input['barcode'] === 'string' ? input['barcode'] : null,
    price: typeof input['price'] === 'string' ? input['price'] : null,
    compareAtPrice: typeof input['compareAtPrice'] === 'string' ? input['compareAtPrice'] : null,
    taxable: typeof input['taxable'] === 'boolean' ? input['taxable'] : null,
    inventoryPolicy: typeof input['inventoryPolicy'] === 'string' ? input['inventoryPolicy'] : null,
    inventoryQuantity: typeof input['inventoryQuantity'] === 'number' ? input['inventoryQuantity'] : null,
    selectedOptions,
    inventoryItem: readInventoryItemInput(input['inventoryItem'], null),
  };
}

function updateVariantRecord(existing: ProductVariantRecord, input: Record<string, unknown>): ProductVariantRecord {
  const selectedOptions = hasOwnField(input, 'selectedOptions')
    ? readSelectedOptionsInput(input['selectedOptions'])
    : existing.selectedOptions;

  return {
    id: existing.id,
    productId: existing.productId,
    title: deriveVariantTitle(input['title'], selectedOptions, existing.title),
    sku: typeof input['sku'] === 'string' ? input['sku'] : existing.sku,
    barcode: typeof input['barcode'] === 'string' ? input['barcode'] : existing.barcode,
    price: typeof input['price'] === 'string' ? input['price'] : existing.price,
    compareAtPrice: typeof input['compareAtPrice'] === 'string' ? input['compareAtPrice'] : existing.compareAtPrice,
    taxable: typeof input['taxable'] === 'boolean' ? input['taxable'] : existing.taxable,
    inventoryPolicy: typeof input['inventoryPolicy'] === 'string' ? input['inventoryPolicy'] : existing.inventoryPolicy,
    inventoryQuantity:
      typeof input['inventoryQuantity'] === 'number' ? input['inventoryQuantity'] : existing.inventoryQuantity,
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
  delta: number | null;
}

interface InventoryAdjustmentChangeRecord {
  inventoryItemId: string;
  locationId: string | null;
  delta: number;
  name: string;
  quantityAfterChange: number;
}

interface InventoryAdjustmentGroupRecord {
  id: string;
  createdAt: string;
  reason: string;
  referenceDocumentUri: string | null;
  changes: InventoryAdjustmentChangeRecord[];
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
                changeResult[changeKey] = group.referenceDocumentUri;
                break;
              case 'item':
                changeResult[changeKey] = variant
                  ? serializeInventoryItemSelectionSet(variant, changeSelection.selectionSet?.selections ?? [])
                  : null;
                break;
              case 'location':
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
                        default:
                          return [locationKey, null];
                      }
                    }),
                );
                break;
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

  if (name !== 'available') {
    return {
      group: null,
      userErrors: [{ field: ['input', 'name'], message: 'Only available inventory adjustments are supported' }],
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

  const variantsByProductId = new Map<string, ProductVariantRecord[]>();
  const adjustedChanges: InventoryAdjustmentChangeRecord[] = [];

  for (const change of changes) {
    if (!change.inventoryItemId) {
      return {
        group: null,
        userErrors: [{ field: ['input', 'changes', 'inventoryItemId'], message: 'Inventory item id is required' }],
      };
    }

    if (typeof change.delta !== 'number') {
      return {
        group: null,
        userErrors: [{ field: ['input', 'changes', 'delta'], message: 'Inventory delta is required' }],
      };
    }

    const variant = store.findEffectiveVariantByInventoryItemId(change.inventoryItemId);
    if (!variant) {
      return {
        group: null,
        userErrors: [{ field: ['input', 'changes', 'inventoryItemId'], message: 'Inventory item not found' }],
      };
    }

    const nextVariants =
      variantsByProductId.get(variant.productId) ??
      store.getEffectiveVariantsByProductId(variant.productId).map((candidate) => structuredClone(candidate));
    const variantIndex = nextVariants.findIndex((candidate) => candidate.id === variant.id);
    if (variantIndex < 0) {
      return {
        group: null,
        userErrors: [{ field: ['input', 'changes', 'inventoryItemId'], message: 'Inventory item not found' }],
      };
    }

    const existingVariant = nextVariants[variantIndex]!;
    const quantityAfterChange = (existingVariant.inventoryQuantity ?? 0) + change.delta;
    nextVariants[variantIndex] = {
      ...existingVariant,
      inventoryQuantity: quantityAfterChange,
    };
    variantsByProductId.set(variant.productId, nextVariants);
    adjustedChanges.push({
      inventoryItemId: change.inventoryItemId,
      locationId: change.locationId,
      delta: change.delta,
      name,
      quantityAfterChange,
    });
  }

  for (const [productId, nextVariants] of variantsByProductId.entries()) {
    store.replaceStagedVariantsForProduct(productId, nextVariants);
    syncProductInventorySummary(productId);
  }

  return {
    group: {
      id: makeSyntheticGid('InventoryAdjustmentGroup'),
      createdAt: makeSyntheticTimestamp(),
      reason,
      referenceDocumentUri: typeof input['referenceDocumentUri'] === 'string' ? input['referenceDocumentUri'] : null,
      changes: adjustedChanges,
    },
    userErrors: [],
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

  return variants.map((variant) => serializeVariantSelectionSet(variant, field.selectionSet?.selections ?? []));
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
      return existing ? updateVariantRecord(existing, normalized) : makeCreatedVariantRecord(productId, normalized);
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
    id: rawId,
    productId,
    title: rawTitle,
    handle: rawHandle,
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

  return {
    key: `${productId}:media:${position}`,
    productId,
    position,
    id: typeof rawId === 'string' ? rawId : null,
    mediaContentType: typeof rawMediaContentType === 'string' ? rawMediaContentType : null,
    alt: typeof rawAlt === 'string' ? rawAlt : null,
    status: typeof rawStatus === 'string' ? rawStatus : null,
    imageUrl:
      typeof rawImageUrl === 'string'
        ? rawImageUrl
        : typeof rawPreviewImageUrl === 'string'
          ? rawPreviewImageUrl
          : null,
    previewImageUrl: typeof rawPreviewImageUrl === 'string' ? rawPreviewImageUrl : null,
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

function getChildField(parent: FieldNode, fieldName: string): FieldNode | null {
  const child = parent.selectionSet?.selections.find(
    (selection): selection is FieldNode => selection.kind === Kind.FIELD && selection.name.value === fieldName,
  );

  return child ?? null;
}

function serializeInventoryItemSelectionSet(
  variant: ProductVariantRecord,
  selections: readonly SelectionNode[],
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
          case 'variant':
            return [
              inventoryKey,
              serializeVariantSelectionSet(variant, inventorySelection.selectionSet?.selections ?? []),
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
        result[key] = serializeInventoryItemSelectionSet(variant, selection.selectionSet?.selections ?? []);
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

function readConnectionSizeArgument(raw: unknown): number | null {
  return typeof raw === 'number' && Number.isInteger(raw) && raw >= 0 ? raw : null;
}

function readConnectionCursor(raw: unknown): string | null {
  if (typeof raw !== 'string' || !raw.startsWith('cursor:')) {
    return null;
  }

  const cursorValue = raw.slice('cursor:'.length);
  return cursorValue.length > 0 ? cursorValue : null;
}

function paginateConnectionItems<T>(
  items: T[],
  field: FieldNode,
  variables: Record<string, unknown>,
  getCursorValue: (item: T) => string,
): { items: T[]; hasNextPage: boolean; hasPreviousPage: boolean } {
  const args = getFieldArguments(field, variables);
  const first = readConnectionSizeArgument(args['first']);
  const last = readConnectionSizeArgument(args['last']);
  const after = readConnectionCursor(args['after']);
  const before = readConnectionCursor(args['before']);

  const startIndex = after === null ? 0 : items.findIndex((item) => getCursorValue(item) === after) + 1;
  const beforeIndex = before === null ? items.length : items.findIndex((item) => getCursorValue(item) === before);
  const windowStart = Math.max(0, startIndex);
  const windowEnd = Math.max(windowStart, beforeIndex >= 0 ? beforeIndex : items.length);
  const paginatedItems = items.slice(windowStart, windowEnd);

  let limitedItems = paginatedItems;
  let hasNextPage = windowEnd < items.length;
  let hasPreviousPage = windowStart > 0;

  if (first !== null) {
    hasNextPage = hasNextPage || paginatedItems.length > first;
    limitedItems = limitedItems.slice(0, first);
  }

  if (last !== null) {
    hasPreviousPage = hasPreviousPage || limitedItems.length > last;
    limitedItems = limitedItems.slice(Math.max(0, limitedItems.length - last));
  }

  return {
    items: limitedItems,
    hasNextPage,
    hasPreviousPage,
  };
}

function serializeConnectionPageInfo<T>(
  selection: FieldNode,
  items: T[],
  hasNextPage: boolean,
  hasPreviousPage: boolean,
  getCursorValue: (item: T) => string,
): Record<string, unknown> {
  return Object.fromEntries(
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
            return [pageInfoKey, items[0] ? `cursor:${getCursorValue(items[0])}` : null];
          case 'endCursor':
            return [pageInfoKey, items.length > 0 ? `cursor:${getCursorValue(items[items.length - 1]!)}` : null];
          default:
            return [pageInfoKey, null];
        }
      }),
  );
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
          serializeVariantSelectionSet(variant, selection.selectionSet?.selections ?? []),
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
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'id':
        result[key] = collection.id;
        break;
      case 'title':
        result[key] = collection.title;
        break;
      case 'handle':
        result[key] = collection.handle;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeCollectionObject(
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
    switch (selection.name.value) {
      case 'id':
        result[key] = collection.id;
        break;
      case 'title':
        result[key] = collection.title;
        break;
      case 'handle':
        result[key] = collection.handle;
        break;
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
        );
        break;
      }
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeCollectionsConnection(
  productId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const allCollections = store.getEffectiveCollectionsByProductId(productId);
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
          serializeCollectionSelectionSet(collection, selection.selectionSet?.selections ?? []),
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

function serializeTopLevelCollectionsConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const allCollections = listEffectiveCollections();
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

function serializeMediaImageSelectionSet(
  imageUrl: string | null,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
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

function serializeMediaSelectionSet(
  media: ProductMediaRecord,
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value !== 'MediaImage') {
        continue;
      }

      for (const fragmentSelection of selection.selectionSet.selections) {
        if (fragmentSelection.kind !== Kind.FIELD) {
          continue;
        }

        const fragmentKey = fragmentSelection.alias?.value ?? fragmentSelection.name.value;
        switch (fragmentSelection.name.value) {
          case 'image':
            result[fragmentKey] = serializeMediaImageSelectionSet(
              media.imageUrl ?? media.previewImageUrl,
              fragmentSelection.selectionSet?.selections ?? [],
            );
            break;
          default:
            result[fragmentKey] = null;
        }
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
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
      default:
        result[key] = null;
    }
  }

  return result;
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
  switch (field.name.value) {
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
      return product.publicationIds.length > 0;
    case 'publishedOnChannel': {
      const args = getFieldArguments(field, variables);
      const channelId = typeof args['channelId'] === 'string' ? args['channelId'] : null;
      if (!channelId) {
        return false;
      }

      return product.publicationIds.includes(channelId);
    }
    case 'availablePublicationsCount':
      return serializeCountValue(field, product.publicationIds.length);
    case 'resourcePublicationsCount':
      return serializeCountValue(field, product.publicationIds.length);
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
  const thresholdValue = match[2]?.trim() ?? '';
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

type ProductsQueryToken =
  | { type: 'term'; value: string }
  | { type: 'or' }
  | { type: 'lparen' }
  | { type: 'rparen' }
  | { type: 'not' };

type ProductsQueryNode =
  | { type: 'term'; value: string }
  | { type: 'and'; children: ProductsQueryNode[] }
  | { type: 'or'; children: ProductsQueryNode[] }
  | { type: 'not'; child: ProductsQueryNode };

function tokenizeProductsQuery(query: string): ProductsQueryToken[] {
  const tokens: ProductsQueryToken[] = [];
  let current = '';
  let inQuotes = false;

  const flushCurrent = (): void => {
    const value = current.trim();
    if (!value) {
      current = '';
      return;
    }

    if (value.toUpperCase() === 'OR') {
      tokens.push({ type: 'or' });
    } else {
      tokens.push({ type: 'term', value });
    }
    current = '';
  };

  for (let index = 0; index < query.length; index += 1) {
    const character = query[index] ?? '';

    if (character === '"') {
      inQuotes = !inQuotes;
      continue;
    }

    if (!inQuotes && /\s/.test(character)) {
      flushCurrent();
      continue;
    }

    if (!inQuotes && character === '(') {
      flushCurrent();
      tokens.push({ type: 'lparen' });
      continue;
    }

    if (!inQuotes && character === ')') {
      flushCurrent();
      tokens.push({ type: 'rparen' });
      continue;
    }

    if (!inQuotes && character === '-' && !current) {
      const nextCharacter = query[index + 1] ?? '';
      if (nextCharacter === '(') {
        tokens.push({ type: 'not' });
        continue;
      }
    }

    current += character;
  }

  flushCurrent();
  return tokens;
}

function parseProductsQuery(query: string): ProductsQueryNode | null {
  const tokens = tokenizeProductsQuery(query);
  if (tokens.length === 0) {
    return null;
  }

  let index = 0;

  const parseOrExpression = (): ProductsQueryNode | null => {
    const firstChild = parseAndExpression();
    if (!firstChild) {
      return null;
    }

    const children: ProductsQueryNode[] = [firstChild];
    while (tokens[index]?.type === 'or') {
      index += 1;
      const nextChild = parseAndExpression();
      if (!nextChild) {
        break;
      }
      children.push(nextChild);
    }

    return children.length === 1 ? (children[0] ?? null) : { type: 'or', children };
  };

  const parseAndExpression = (): ProductsQueryNode | null => {
    const children: ProductsQueryNode[] = [];

    while (index < tokens.length) {
      const token = tokens[index];
      if (!token || token.type === 'or' || token.type === 'rparen') {
        break;
      }

      const child = parseUnaryExpression();
      if (!child) {
        break;
      }
      children.push(child);
    }

    if (children.length === 0) {
      return null;
    }

    return children.length === 1 ? (children[0] ?? null) : { type: 'and', children };
  };

  const parseUnaryExpression = (): ProductsQueryNode | null => {
    const token = tokens[index];
    if (!token) {
      return null;
    }

    if (token.type === 'not') {
      index += 1;
      const child = parseUnaryExpression();
      return child ? { type: 'not', child } : null;
    }

    if (token.type === 'term') {
      index += 1;
      return { type: 'term', value: token.value };
    }

    if (token.type === 'lparen') {
      index += 1;
      const child = parseOrExpression();
      if (tokens[index]?.type === 'rparen') {
        index += 1;
      }
      return child;
    }

    return null;
  };

  return parseOrExpression();
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

function matchesProductSearchText(product: ProductRecord, rawValue: string): boolean {
  const searchableValues = [
    product.title,
    product.handle,
    product.vendor ?? '',
    product.productType ?? '',
    ...product.tags,
  ];

  return searchableValues.some((candidate) => matchesStringValue(candidate, rawValue, 'includes'));
}

function matchesPositiveProductQueryTerm(product: ProductRecord, term: string): boolean {
  const separatorIndex = term.indexOf(':');
  if (separatorIndex === -1) {
    return matchesProductSearchText(product, term);
  }

  const field = term.slice(0, separatorIndex).toLowerCase();
  const value = term.slice(separatorIndex + 1);

  switch (field) {
    case 'title':
      return matchesStringValue(product.title, value, 'includes');
    case 'handle':
      return matchesStringValue(product.handle, value, 'exact');
    case 'tag':
      return product.tags.some((tag) => matchesStringValue(tag, value, 'exact'));
    case 'product_type':
      return typeof product.productType === 'string' && matchesStringValue(product.productType, value, 'exact');
    case 'vendor':
      return typeof product.vendor === 'string' && matchesStringValue(product.vendor, value, 'exact');
    case 'status':
      return matchesStringValue(product.status, value, 'exact');
    case 'created_at':
      return matchesProductTimestampTerm(product.createdAt, value);
    case 'updated_at':
      return matchesProductTimestampTerm(product.updatedAt, value);
    case 'sku':
      return store
        .getEffectiveVariantsByProductId(product.id)
        .some((variant) => typeof variant.sku === 'string' && matchesStringValue(variant.sku, value, 'exact'));
    case 'barcode':
      return store
        .getEffectiveVariantsByProductId(product.id)
        .some((variant) => typeof variant.barcode === 'string' && matchesStringValue(variant.barcode, value, 'exact'));
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

function matchesProductQueryTerm(product: ProductRecord, rawTerm: string): boolean {
  const term = rawTerm.trim();
  if (!term) {
    return true;
  }

  const isNegated = term.startsWith('-');
  const normalizedTerm = isNegated ? term.slice(1).trim() : term;
  if (!normalizedTerm) {
    return true;
  }

  const matches = matchesPositiveProductQueryTerm(product, normalizedTerm);
  return isNegated ? !matches : matches;
}

function matchesProductsQueryNode(product: ProductRecord, node: ProductsQueryNode): boolean {
  switch (node.type) {
    case 'term':
      return matchesProductQueryTerm(product, node.value);
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

  const parsedQuery = parseProductsQuery(rawQuery);
  if (!parsedQuery) {
    return products;
  }

  return products.filter((product) => matchesProductsQueryNode(product, parsedQuery));
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

function sortProducts(products: ProductRecord[], rawSortKey: unknown, rawReverse: unknown): ProductRecord[] {
  const direction = rawReverse === true ? -1 : 1;
  return [...products].sort((left, right) => {
    const comparison = compareProductsBySortKey(left, right, rawSortKey);
    return (
      (rawSortKey === undefined || rawSortKey === null ? compareProductsDefaultOrder(left, right) : comparison) *
      direction
    );
  });
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
): Record<string, unknown> {
  const filteredProducts = applyProductsQuery(products, rawQuery);
  const sortedProducts = sortProducts(filteredProducts, rawSortKey, rawReverse);
  const afterProductId = parseProductsCursor(rawAfter);
  const beforeProductId = parseProductsCursor(rawBefore);
  const startIndex =
    afterProductId === null ? 0 : sortedProducts.findIndex((product) => product.id === afterProductId) + 1;
  const beforeIndex =
    beforeProductId === null
      ? sortedProducts.length
      : sortedProducts.findIndex((product) => product.id === beforeProductId);
  const endIndex = beforeIndex >= 0 ? beforeIndex : sortedProducts.length;
  const windowStart = Math.max(0, startIndex);
  const windowEnd = Math.max(windowStart, endIndex);
  const paginatedProducts = sortedProducts.slice(windowStart, windowEnd);

  let limitedProducts = paginatedProducts;
  let hasNextPage = windowEnd < sortedProducts.length;
  let hasPreviousPage = windowStart > 0;

  if (first !== null) {
    hasNextPage = hasNextPage || paginatedProducts.length > first;
    limitedProducts = limitedProducts.slice(0, first);
  }

  if (last !== null) {
    hasPreviousPage = hasPreviousPage || limitedProducts.length > last;
    limitedProducts = limitedProducts.slice(Math.max(0, limitedProducts.length - last));
  }

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
                edgeResult[edgeKey] = `cursor:${product.id}`;
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
                  return [pageInfoKey, limitedProducts[0] ? `cursor:${limitedProducts[0].id}` : null];
                case 'endCursor':
                  return [
                    pageInfoKey,
                    limitedProducts.length > 0 ? `cursor:${limitedProducts[limitedProducts.length - 1]?.id}` : null,
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
    media,
    hasMedia,
    metafields,
    hasMetafields,
  };
}

export function hydrateProductsFromUpstreamResponse(responseBody: unknown): void {
  if (!isObject(responseBody)) {
    return;
  }

  const rawData = responseBody['data'];
  if (!isObject(rawData)) {
    return;
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
    if (maybeProduct.hasMedia) {
      store.replaceBaseMediaForProduct(maybeProduct.product.id, maybeProduct.media);
    }
    if (maybeProduct.hasMetafields) {
      store.replaceBaseMetafieldsForProduct(maybeProduct.product.id, maybeProduct.metafields);
    }
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
      if (entry.hasMedia) {
        store.replaceBaseMediaForProduct(entry.product.id, entry.media);
      }
      if (entry.hasMetafields) {
        store.replaceBaseMetafieldsForProduct(entry.product.id, entry.metafields);
      }
    }
  }
}

export function handleProductMutation(document: string, variables: Record<string, unknown>): Record<string, unknown> {
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

      store.stageUpdateProduct(makeProductRecord({ id: productId, tags: nextTags }, existingProduct));
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

      const nextTags = existingProduct.tags.filter((tag) => !tags.includes(tag));
      store.stageUpdateProduct(makeProductRecord({ id: productId, tags: nextTags }, existingProduct));
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
      const product = store.stageCreateProduct(makeProductRecord(input));
      store.replaceStagedOptionsForProduct(product.id, [makeDefaultOptionRecord(product)]);
      store.replaceStagedVariantsForProduct(product.id, [makeDefaultVariantRecord(product)]);
      return {
        data: {
          [responseKey]: {
            product: serializeProduct(product, getChildField(field, 'product'), variables),
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
              userErrors: [{ field: ['product', 'id'], message: 'Product id is required' }],
            },
          },
        };
      }

      const existing = store.getEffectiveProductById(id) ?? undefined;
      store.stageUpdateProduct(makeProductRecord({ ...input, id }, existing));
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
      const input = readProductInput(args['input']);
      const inputId = input['id'];
      const argId = args['id'];
      const id = typeof inputId === 'string' ? inputId : typeof argId === 'string' ? argId : null;
      if (!id) {
        return {
          data: {
            [responseKey]: {
              deletedProductId: null,
              userErrors: [{ field: ['input', 'id'], message: 'Product id is required' }],
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

      const duplicatedProduct = store.stageCreateProduct(
        makeDuplicatedProductRecord(sourceProduct, typeof args['newTitle'] === 'string' ? args['newTitle'] : undefined),
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
      store.replaceStagedMediaForProduct(
        duplicatedProduct.id,
        store.getEffectiveMediaByProductId(productId).map((media) => duplicateMediaRecord(media, duplicatedProduct.id)),
      );
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
      const existing =
        (identifierId ? store.getEffectiveProductById(identifierId) : null) ??
        (inputId ? store.getEffectiveProductById(inputId) : null) ??
        (identifierHandle ? findEffectiveProductByHandle(identifierHandle) : null);

      const stagedProduct = existing
        ? store.stageUpdateProduct(makeProductRecord({ ...input, id: existing.id }, existing))
        : store.stageCreateProduct(makeProductRecord(input));
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
      const synchronous = args['synchronous'] !== false;
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

      const existing = store.getEffectiveProductById(productId) ?? undefined;
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
      if (!productId) {
        return {
          data: {
            [responseKey]: {
              product: null,
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
              product: null,
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
            product: serializeProduct(product, getChildField(field, 'product'), variables),
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
            [responseKey]: {
              product: null,
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
              product: null,
              userErrors: [{ field: ['input', 'id'], message: 'Product not found' }],
            },
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
          [responseKey]: {
            product: serializeProduct(product, getChildField(field, 'product'), variables),
            userErrors: [],
          },
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
              userErrors: [{ field: ['id'], message: 'Collection not found' }],
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

      return {
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
        .filter((mediaRecord) => mediaRecord.mediaContentType === 'IMAGE')
        .map((mediaRecord) => mediaRecord.id)
        .filter((mediaId): mediaId is string => typeof mediaId === 'string');

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
    case 'inventoryAdjustQuantities': {
      const input = readProductInput(args['input']);
      const result = applyInventoryAdjustQuantities(input);
      return {
        data: {
          [responseKey]: {
            inventoryAdjustmentGroup: serializeInventoryAdjustmentGroup(
              result.group,
              getChildField(field, 'inventoryAdjustmentGroup'),
            ),
            userErrors: result.userErrors,
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

      const remainingMetafields = store
        .getEffectiveMetafieldsByProductId(existingMetafield.productId)
        .filter((metafield) => metafield.id !== metafieldId);
      store.replaceStagedMetafieldsForProduct(existingMetafield.productId, remainingMetafields);
      return {
        data: {
          [responseKey]: {
            deletedId: metafieldId,
            userErrors: [],
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

      const createdVariant = makeCreatedVariantRecord(productId, input);
      const nextVariants = [...store.getEffectiveVariantsByProductId(productId), createdVariant];
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

      const createdVariants = (Array.isArray(args['variants']) ? args['variants'] : [])
        .filter((variant): variant is Record<string, unknown> => isObject(variant))
        .map((variant) => makeCreatedVariantRecord(productId, variant));
      const nextVariants = [...store.getEffectiveVariantsByProductId(productId), ...createdVariants];
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
          ? serializeVariantSelectionSet(variant, field.selectionSet?.selections ?? [])
          : null;
        break;
      }
      case 'inventoryItem': {
        const rawId = args['id'];
        const id = typeof rawId === 'string' ? rawId : null;
        const variant = id ? store.findEffectiveVariantByInventoryItemId(id) : null;
        data[responseKey] = variant
          ? serializeInventoryItemSelectionSet(variant, field.selectionSet?.selections ?? [])
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
      case 'collections': {
        data[responseKey] = serializeTopLevelCollectionsConnection(field, variables);
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
