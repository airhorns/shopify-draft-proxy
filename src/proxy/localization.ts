import type { ProxyRuntimeContext } from './runtime-context.js';
import { createHash } from 'node:crypto';

import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import type {
  LocaleRecord,
  ProductMetafieldRecord,
  ProductRecord,
  ShopLocaleRecord,
  TranslationRecord,
} from '../state/types.js';
import {
  getDocumentFragments,
  getFieldResponseKey,
  getSelectedChildFields,
  isPlainObject,
  paginateConnectionItems,
  projectGraphqlValue,
  readGraphqlDataResponsePayload,
  serializeConnection,
  type FragmentMap,
} from './graphql-helpers.js';

type TranslatableContentRecord = {
  key: string;
  value: string | null;
  digest: string | null;
  locale: string;
  type: string;
};

type TranslatableResourceRecord = {
  resourceId: string;
  resourceType: 'PRODUCT' | 'METAFIELD';
  content: TranslatableContentRecord[];
};

type TranslationUserError = {
  field: string[];
  message: string;
  code: string;
};

type UserError = {
  field: string[];
  message: string;
};

const DEFAULT_AVAILABLE_LOCALES: LocaleRecord[] = [
  { isoCode: 'en', name: 'English' },
  { isoCode: 'fr', name: 'French' },
  { isoCode: 'de', name: 'German' },
  { isoCode: 'es', name: 'Spanish' },
  { isoCode: 'it', name: 'Italian' },
  { isoCode: 'pt-BR', name: 'Portuguese (Brazil)' },
  { isoCode: 'ja', name: 'Japanese' },
  { isoCode: 'zh-CN', name: 'Chinese (Simplified)' },
];

function digestValue(value: string | null): string | null {
  return value === null ? null : createHash('sha256').update(value).digest('hex');
}

function readInput(raw: unknown): Record<string, unknown> {
  return isPlainObject(raw) ? raw : {};
}

function availableLocales(runtime: ProxyRuntimeContext): LocaleRecord[] {
  const stored = runtime.store.listEffectiveAvailableLocales();
  return stored.length > 0 ? stored : DEFAULT_AVAILABLE_LOCALES;
}

function localeName(runtime: ProxyRuntimeContext, locale: string): string | null {
  return availableLocales(runtime).find((candidate) => candidate.isoCode === locale)?.name ?? null;
}

function defaultShopLocales(runtime: ProxyRuntimeContext): ShopLocaleRecord[] {
  return [
    {
      locale: 'en',
      name: localeName(runtime, 'en') ?? 'English',
      primary: true,
      published: true,
      marketWebPresenceIds: [],
    },
  ];
}

function listShopLocales(runtime: ProxyRuntimeContext, published?: boolean | null): ShopLocaleRecord[] {
  const stored = runtime.store.listEffectiveShopLocales(published);
  if (stored.length > 0) {
    return stored;
  }

  return defaultShopLocales(runtime).filter((locale) =>
    typeof published === 'boolean' ? locale.published === published : true,
  );
}

function getShopLocale(runtime: ProxyRuntimeContext, locale: string): ShopLocaleRecord | null {
  return (
    runtime.store.getEffectiveShopLocale(locale) ??
    defaultShopLocales(runtime).find((candidate) => candidate.locale === locale) ??
    null
  );
}

function serializeUserErrors(errors: UserError[], selection: FieldNode): Array<Record<string, unknown>> {
  return errors.map((error) => {
    const result: Record<string, unknown> = {};
    for (const field of getSelectedChildFields(selection)) {
      const key = getFieldResponseKey(field);
      switch (field.name.value) {
        case 'field':
          result[key] = error.field;
          break;
        case 'message':
          result[key] = error.message;
          break;
        default:
          result[key] = null;
          break;
      }
    }
    return result;
  });
}

function serializeTranslationUserErrors(
  errors: TranslationUserError[],
  selection: FieldNode,
): Array<Record<string, unknown>> {
  return errors.map((error) => {
    const result: Record<string, unknown> = {};
    for (const field of getSelectedChildFields(selection)) {
      const key = getFieldResponseKey(field);
      switch (field.name.value) {
        case 'field':
          result[key] = error.field;
          break;
        case 'message':
          result[key] = error.message;
          break;
        case 'code':
          result[key] = error.code;
          break;
        default:
          result[key] = null;
          break;
      }
    }
    return result;
  });
}

function serializeLocale(locale: LocaleRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'isoCode':
        result[key] = locale.isoCode;
        break;
      case 'name':
        result[key] = locale.name;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeShopLocale(
  runtime: ProxyRuntimeContext,
  locale: ShopLocaleRecord | null,
  selections: readonly SelectionNode[],
): Record<string, unknown> | null {
  if (!locale) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'locale':
        result[key] = locale.locale;
        break;
      case 'name':
        result[key] = locale.name;
        break;
      case 'primary':
        result[key] = locale.primary;
        break;
      case 'published':
        result[key] = locale.published;
        break;
      case 'marketWebPresences':
        result[key] = locale.marketWebPresenceIds
          .map((id) => runtime.store.getEffectiveWebPresenceById(id))
          .filter((presence): presence is Record<string, unknown> => isPlainObject(presence))
          .map((presence) => projectGraphqlValue(presence, selection.selectionSet?.selections ?? [], new Map()));
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function productContent(product: ProductRecord): TranslatableContentRecord[] {
  const content: TranslatableContentRecord[] = [
    {
      key: 'title',
      value: product.title,
      digest: digestValue(product.title),
      locale: 'en',
      type: 'SINGLE_LINE_TEXT_FIELD',
    },
    {
      key: 'handle',
      value: product.handle,
      digest: digestValue(product.handle),
      locale: 'en',
      type: 'URI',
    },
  ];

  if (product.descriptionHtml !== null) {
    content.push({
      key: 'body_html',
      value: product.descriptionHtml,
      digest: digestValue(product.descriptionHtml),
      locale: 'en',
      type: 'HTML',
    });
  }

  if (product.productType !== null) {
    content.push({
      key: 'product_type',
      value: product.productType,
      digest: digestValue(product.productType),
      locale: 'en',
      type: 'SINGLE_LINE_TEXT_FIELD',
    });
  }

  if (product.seo.title !== null) {
    content.push({
      key: 'meta_title',
      value: product.seo.title,
      digest: digestValue(product.seo.title),
      locale: 'en',
      type: 'SINGLE_LINE_TEXT_FIELD',
    });
  }

  if (product.seo.description !== null) {
    content.push({
      key: 'meta_description',
      value: product.seo.description,
      digest: digestValue(product.seo.description),
      locale: 'en',
      type: 'MULTI_LINE_TEXT_FIELD',
    });
  }

  return content;
}

function metafieldContent(metafield: ProductMetafieldRecord): TranslatableContentRecord[] {
  return [
    {
      key: 'value',
      value: metafield.value,
      digest: metafield.compareDigest ?? digestValue(metafield.value),
      locale: 'en',
      type: localizableContentTypeForMetafield(metafield.type),
    },
  ];
}

function localizableContentTypeForMetafield(type: string | null): string {
  switch (type) {
    case 'multi_line_text_field':
      return 'MULTI_LINE_TEXT_FIELD';
    case 'rich_text_field':
      return 'RICH_TEXT_FIELD';
    case 'url':
      return 'URL';
    case 'json':
      return 'JSON';
    case 'single_line_text_field':
    default:
      return 'SINGLE_LINE_TEXT_FIELD';
  }
}

function listProductMetafields(runtime: ProxyRuntimeContext): ProductMetafieldRecord[] {
  const metafieldsById = new Map<string, ProductMetafieldRecord>();
  for (const product of runtime.store.listEffectiveProducts()) {
    for (const metafield of runtime.store.getEffectiveMetafieldsByOwnerId(product.id)) {
      metafieldsById.set(metafield.id, metafield);
    }
  }
  return Array.from(metafieldsById.values()).sort((left, right) => left.id.localeCompare(right.id));
}

function listResources(runtime: ProxyRuntimeContext, resourceType: unknown): TranslatableResourceRecord[] {
  switch (resourceType) {
    case 'PRODUCT':
      return runtime.store
        .listEffectiveProducts()
        .map((product) => ({ resourceId: product.id, resourceType: 'PRODUCT', content: productContent(product) }));
    case 'METAFIELD':
      return listProductMetafields(runtime).map((metafield) => ({
        resourceId: metafield.id,
        resourceType: 'METAFIELD',
        content: metafieldContent(metafield),
      }));
    default:
      return [];
  }
}

function findResource(runtime: ProxyRuntimeContext, resourceId: string): TranslatableResourceRecord | null {
  const product = runtime.store.getEffectiveProductById(resourceId);
  if (product) {
    return { resourceId: product.id, resourceType: 'PRODUCT', content: productContent(product) };
  }

  const metafield = listProductMetafields(runtime).find((candidate) => candidate.id === resourceId) ?? null;
  return metafield
    ? { resourceId: metafield.id, resourceType: 'METAFIELD', content: metafieldContent(metafield) }
    : null;
}

function serializeContent(
  content: TranslatableContentRecord[],
  selections: readonly SelectionNode[],
): Array<Record<string, unknown>> {
  return content.map((entry) => {
    const result: Record<string, unknown> = {};
    for (const selection of selections) {
      if (selection.kind !== Kind.FIELD) {
        continue;
      }
      const key = getFieldResponseKey(selection);
      switch (selection.name.value) {
        case 'key':
          result[key] = entry.key;
          break;
        case 'value':
          result[key] = entry.value;
          break;
        case 'digest':
          result[key] = entry.digest;
          break;
        case 'locale':
          result[key] = entry.locale;
          break;
        case 'type':
          result[key] = entry.type;
          break;
        default:
          result[key] = null;
          break;
      }
    }
    return result;
  });
}

function serializeTranslation(
  runtime: ProxyRuntimeContext,
  translation: TranslationRecord,
  selections: readonly SelectionNode[],
  fragments: FragmentMap,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'key':
        result[key] = translation.key;
        break;
      case 'value':
        result[key] = translation.value;
        break;
      case 'locale':
        result[key] = translation.locale;
        break;
      case 'outdated':
        result[key] = translation.outdated;
        break;
      case 'updatedAt':
        result[key] = translation.updatedAt;
        break;
      case 'market': {
        const market = translation.marketId ? runtime.store.getEffectiveMarketById(translation.marketId) : null;
        result[key] =
          market && selection.selectionSet
            ? projectGraphqlValue(market, selection.selectionSet.selections, fragments)
            : null;
        break;
      }
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeEmptyConnection(field: FieldNode): Record<string, unknown> {
  return serializeConnection(field, {
    items: [],
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: () => '',
    serializeNode: () => null,
  });
}

function serializeResource(
  runtime: ProxyRuntimeContext,
  resource: TranslatableResourceRecord | null,
  selections: readonly SelectionNode[],
  fragments: FragmentMap,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  if (!resource) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'resourceId':
        result[key] = resource.resourceId;
        break;
      case 'translatableContent':
        result[key] = serializeContent(resource.content, selection.selectionSet?.selections ?? []);
        break;
      case 'translations': {
        const args = getFieldArguments(selection, variables);
        const locale = typeof args['locale'] === 'string' ? args['locale'] : null;
        const marketId = typeof args['marketId'] === 'string' ? args['marketId'] : null;
        const outdated = typeof args['outdated'] === 'boolean' ? args['outdated'] : null;
        const translations = locale
          ? runtime.store.listEffectiveTranslations(resource.resourceId, locale, marketId)
          : [];
        result[key] = translations
          .filter((translation) => (outdated === null ? true : translation.outdated === outdated))
          .map((translation) =>
            serializeTranslation(runtime, translation, selection.selectionSet?.selections ?? [], fragments),
          );
        break;
      }
      case 'nestedTranslatableResources':
        result[key] = serializeEmptyConnection(selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function resourceCursor(resource: TranslatableResourceRecord): string {
  return resource.resourceId;
}

function serializeResourceConnection(
  runtime: ProxyRuntimeContext,
  resources: TranslatableResourceRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const window = paginateConnectionItems(resources, field, variables, resourceCursor);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: resourceCursor,
    serializeNode: (resource, selection) =>
      serializeResource(runtime, resource, selection.selectionSet?.selections ?? [], fragments, variables),
  });
}

function translationError(field: string[], message: string, code: string): TranslationUserError {
  return { field, message, code };
}

function validateResource(
  runtime: ProxyRuntimeContext,
  resourceId: unknown,
): {
  resource: TranslatableResourceRecord | null;
  errors: TranslationUserError[];
} {
  if (typeof resourceId !== 'string' || resourceId.length === 0) {
    return {
      resource: null,
      errors: [translationError(['resourceId'], 'Resource does not exist', 'RESOURCE_NOT_FOUND')],
    };
  }

  const resource = findResource(runtime, resourceId);
  if (!resource) {
    return {
      resource: null,
      errors: [translationError(['resourceId'], `Resource ${resourceId} does not exist`, 'RESOURCE_NOT_FOUND')],
    };
  }

  return { resource, errors: [] };
}

function validateLocale(
  runtime: ProxyRuntimeContext,
  locale: unknown,
  field: string[],
): { locale: string | null; errors: TranslationUserError[] } {
  if (typeof locale !== 'string' || !getShopLocale(runtime, locale)) {
    return {
      locale: typeof locale === 'string' ? locale : null,
      errors: [translationError(field, 'Locale is not enabled for this shop', 'INVALID_LOCALE_FOR_SHOP')],
    };
  }

  return { locale, errors: [] };
}

function projectTranslationsPayload(
  runtime: ProxyRuntimeContext,
  payload: { translations: TranslationRecord[] | null; userErrors: TranslationUserError[] },
  field: FieldNode,
  fragments: FragmentMap,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'translations':
        result[key] = payload.translations
          ? payload.translations.map((translation) =>
              serializeTranslation(runtime, translation, selection.selectionSet?.selections ?? [], fragments),
            )
          : null;
        break;
      case 'userErrors':
        result[key] = serializeTranslationUserErrors(payload.userErrors, selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function handleTranslationsRegister(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const resourceValidation = validateResource(runtime, args['resourceId']);
  const errors = [...resourceValidation.errors];
  const inputs = Array.isArray(args['translations'])
    ? args['translations'].filter((input): input is Record<string, unknown> => isPlainObject(input))
    : [];

  if (inputs.length === 0) {
    errors.push(translationError(['translations'], 'At least one translation is required', 'BLANK'));
  }

  const translations: TranslationRecord[] = [];
  const resource = resourceValidation.resource;
  if (resource) {
    inputs.forEach((input, index) => {
      const prefix = ['translations', String(index)];
      const localeValidation = validateLocale(runtime, input['locale'], [...prefix, 'locale']);
      errors.push(...localeValidation.errors);
      const key = typeof input['key'] === 'string' ? input['key'] : '';
      const content = resource.content.find((entry) => entry.key === key) ?? null;
      if (!content) {
        errors.push(
          translationError(
            [...prefix, 'key'],
            `Key ${key || String(input['key'])} is not translatable for this resource`,
            'INVALID_KEY_FOR_MODEL',
          ),
        );
      }
      if (typeof input['value'] !== 'string' || input['value'] === '') {
        errors.push(translationError([...prefix, 'value'], "Value can't be blank", 'BLANK'));
      }
      if (content && content.digest !== null && input['translatableContentDigest'] !== content.digest) {
        errors.push(
          translationError(
            [...prefix, 'translatableContentDigest'],
            'Translatable content digest does not match the resource content',
            'INVALID_TRANSLATABLE_CONTENT',
          ),
        );
      }
      if (typeof input['marketId'] === 'string') {
        errors.push(
          translationError(
            [...prefix, 'marketId'],
            'Market-specific translations are not supported for this local resource branch',
            'MARKET_CUSTOM_CONTENT_NOT_ALLOWED',
          ),
        );
      }

      if (
        errors.length === 0 &&
        resource &&
        content?.digest &&
        localeValidation.locale &&
        typeof input['value'] === 'string'
      ) {
        translations.push({
          resourceId: resource.resourceId,
          key,
          locale: localeValidation.locale,
          value: input['value'],
          translatableContentDigest: content.digest,
          marketId: null,
          updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
          outdated: false,
        });
      }
    });
  }

  if (errors.length === 0) {
    for (const translation of translations) {
      runtime.store.stageTranslation(translation);
    }
  }

  return projectTranslationsPayload(
    runtime,
    { translations: errors.length === 0 ? translations : null, userErrors: errors },
    field,
    fragments,
  );
}

function handleTranslationsRemove(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const resourceValidation = validateResource(runtime, args['resourceId']);
  const errors = [...resourceValidation.errors];
  const keys = Array.isArray(args['translationKeys'])
    ? args['translationKeys'].filter((key): key is string => typeof key === 'string')
    : [];
  const locales = Array.isArray(args['locales'])
    ? args['locales'].filter((locale): locale is string => typeof locale === 'string')
    : [];

  if (keys.length === 0) {
    errors.push(translationError(['translationKeys'], 'At least one translation key is required', 'BLANK'));
  }
  if (locales.length === 0) {
    errors.push(translationError(['locales'], 'At least one locale is required', 'BLANK'));
  }
  if (Array.isArray(args['marketIds']) && args['marketIds'].length > 0) {
    errors.push(
      translationError(
        ['marketIds'],
        'Market-specific translations are not supported for this local resource branch',
        'MARKET_CUSTOM_CONTENT_NOT_ALLOWED',
      ),
    );
  }

  const resource = resourceValidation.resource;
  if (resource) {
    for (const key of keys) {
      if (!resource.content.some((entry) => entry.key === key)) {
        errors.push(
          translationError(
            ['translationKeys'],
            `Key ${key} is not translatable for this resource`,
            'INVALID_KEY_FOR_MODEL',
          ),
        );
      }
    }
    for (const locale of locales) {
      errors.push(...validateLocale(runtime, locale, ['locales']).errors);
    }
  }

  const removed: TranslationRecord[] = [];
  if (errors.length === 0 && resource) {
    for (const locale of locales) {
      for (const key of keys) {
        const translation = runtime.store.removeTranslation(resource.resourceId, locale, key);
        if (translation) {
          removed.push(translation);
        }
      }
    }
  }

  return projectTranslationsPayload(
    runtime,
    { translations: errors.length === 0 ? removed : null, userErrors: errors },
    field,
    fragments,
  );
}

function invalidLocaleError(): UserError {
  return { field: ['locale'], message: 'Locale is invalid' };
}

function projectShopLocalePayload(
  runtime: ProxyRuntimeContext,
  payload: { shopLocale?: ShopLocaleRecord | null; locale?: string | null; userErrors: UserError[] },
  field: FieldNode,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'shopLocale':
        result[key] = serializeShopLocale(
          runtime,
          payload.shopLocale ?? null,
          selection.selectionSet?.selections ?? [],
        );
        break;
      case 'locale':
        result[key] = payload.locale ?? null;
        break;
      case 'userErrors':
        result[key] = serializeUserErrors(payload.userErrors, selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function handleShopLocaleEnable(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const locale = typeof args['locale'] === 'string' ? args['locale'] : '';
  const name = localeName(runtime, locale);
  if (!name) {
    return projectShopLocalePayload(runtime, { shopLocale: null, userErrors: [invalidLocaleError()] }, field);
  }

  const marketWebPresenceIds = Array.isArray(args['marketWebPresenceIds'])
    ? args['marketWebPresenceIds'].filter((id): id is string => typeof id === 'string')
    : [];
  const existing = getShopLocale(runtime, locale);
  const record = runtime.store.stageShopLocale({
    locale,
    name,
    primary: existing?.primary ?? false,
    published: existing?.published ?? false,
    marketWebPresenceIds,
  });

  return projectShopLocalePayload(runtime, { shopLocale: record, userErrors: [] }, field);
}

function handleShopLocaleUpdate(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const locale = typeof args['locale'] === 'string' ? args['locale'] : '';
  const existing = getShopLocale(runtime, locale);
  if (!existing || !localeName(runtime, locale)) {
    return projectShopLocalePayload(runtime, { shopLocale: null, userErrors: [invalidLocaleError()] }, field);
  }

  const input = readInput(args['shopLocale']);
  const marketWebPresenceIds = Array.isArray(input['marketWebPresenceIds'])
    ? input['marketWebPresenceIds'].filter((id): id is string => typeof id === 'string')
    : existing.marketWebPresenceIds;
  const record = runtime.store.stageShopLocale({
    ...existing,
    published: typeof input['published'] === 'boolean' ? input['published'] : existing.published,
    marketWebPresenceIds,
  });

  return projectShopLocalePayload(runtime, { shopLocale: record, userErrors: [] }, field);
}

function handleShopLocaleDisable(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const locale = typeof args['locale'] === 'string' ? args['locale'] : '';
  const existing = getShopLocale(runtime, locale);
  if (!existing || !localeName(runtime, locale) || existing.primary) {
    return projectShopLocalePayload(runtime, { locale, userErrors: [invalidLocaleError()] }, field);
  }

  runtime.store.disableShopLocale(locale);
  return projectShopLocalePayload(runtime, { locale, userErrors: [] }, field);
}

function rootPayloadForField(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  switch (field.name.value) {
    case 'availableLocales':
      return availableLocales(runtime).map((locale) => serializeLocale(locale, field.selectionSet?.selections ?? []));
    case 'shopLocales': {
      const args = getFieldArguments(field, variables);
      const published = typeof args['published'] === 'boolean' ? args['published'] : null;
      return listShopLocales(runtime, published).map((locale) =>
        serializeShopLocale(runtime, locale, field.selectionSet?.selections ?? []),
      );
    }
    case 'translatableResource': {
      const args = getFieldArguments(field, variables);
      const resourceId = typeof args['resourceId'] === 'string' ? args['resourceId'] : null;
      return resourceId
        ? serializeResource(
            runtime,
            findResource(runtime, resourceId),
            field.selectionSet?.selections ?? [],
            fragments,
            variables,
          )
        : null;
    }
    case 'translatableResources': {
      const args = getFieldArguments(field, variables);
      const resources = listResources(runtime, args['resourceType']);
      return serializeResourceConnection(
        runtime,
        args['reverse'] === true ? resources.reverse() : resources,
        field,
        variables,
        fragments,
      );
    }
    case 'translatableResourcesByIds': {
      const args = getFieldArguments(field, variables);
      const ids = Array.isArray(args['resourceIds'])
        ? args['resourceIds'].filter((id): id is string => typeof id === 'string')
        : [];
      const resources = ids.flatMap((id) => {
        const resource = findResource(runtime, id);
        return resource ? [resource] : [];
      });
      return serializeResourceConnection(
        runtime,
        args['reverse'] === true ? resources.reverse() : resources,
        field,
        variables,
        fragments,
      );
    }
    default:
      return null;
  }
}

export function handleLocalizationQuery(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const fragments = getDocumentFragments(document);
  const data: Record<string, unknown> = {};
  for (const field of getRootFields(document)) {
    data[getFieldResponseKey(field)] = rootPayloadForField(runtime, field, variables, fragments);
  }
  return { data };
}

export function handleLocalizationMutation(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const fragments = getDocumentFragments(document);
  const data: Record<string, unknown> = {};
  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'translationsRegister':
        data[key] = handleTranslationsRegister(runtime, field, variables, fragments);
        break;
      case 'translationsRemove':
        data[key] = handleTranslationsRemove(runtime, field, variables, fragments);
        break;
      case 'shopLocaleEnable':
        data[key] = handleShopLocaleEnable(runtime, field, variables);
        break;
      case 'shopLocaleUpdate':
        data[key] = handleShopLocaleUpdate(runtime, field, variables);
        break;
      case 'shopLocaleDisable':
        data[key] = handleShopLocaleDisable(runtime, field, variables);
        break;
      default:
        data[key] = null;
        break;
    }
  }
  return { data };
}

function normalizeLocale(value: unknown): LocaleRecord | null {
  if (!isPlainObject(value) || typeof value['isoCode'] !== 'string' || typeof value['name'] !== 'string') {
    return null;
  }
  return { isoCode: value['isoCode'], name: value['name'] };
}

function normalizeShopLocale(value: unknown): ShopLocaleRecord | null {
  if (
    !isPlainObject(value) ||
    typeof value['locale'] !== 'string' ||
    typeof value['name'] !== 'string' ||
    typeof value['primary'] !== 'boolean' ||
    typeof value['published'] !== 'boolean'
  ) {
    return null;
  }

  const marketWebPresenceIds = Array.isArray(value['marketWebPresences'])
    ? value['marketWebPresences'].flatMap((presence) =>
        isPlainObject(presence) && typeof presence['id'] === 'string' ? [presence['id']] : [],
      )
    : [];

  return {
    locale: value['locale'],
    name: value['name'],
    primary: value['primary'],
    published: value['published'],
    marketWebPresenceIds,
  };
}

export function hydrateLocalizationFromUpstreamResponse(runtime: ProxyRuntimeContext, upstreamPayload: unknown): void {
  const availableLocalesPayload = readGraphqlDataResponsePayload(upstreamPayload, 'availableLocales');
  if (Array.isArray(availableLocalesPayload)) {
    runtime.store.replaceBaseAvailableLocales(
      availableLocalesPayload.flatMap((locale) => {
        const normalized = normalizeLocale(locale);
        return normalized ? [normalized] : [];
      }),
    );
  }

  const shopLocalesPayload = readGraphqlDataResponsePayload(upstreamPayload, 'shopLocales');
  if (Array.isArray(shopLocalesPayload)) {
    runtime.store.upsertBaseShopLocales(
      shopLocalesPayload.flatMap((locale) => {
        const normalized = normalizeShopLocale(locale);
        return normalized ? [normalized] : [];
      }),
    );
  }
}
