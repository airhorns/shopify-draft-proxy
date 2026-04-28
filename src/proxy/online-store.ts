import type { ProxyRuntimeContext } from './runtime-context.js';
import { createHash } from 'node:crypto';

import { type FieldNode } from 'graphql';

import type { JsonValue } from '../json-schemas.js';
import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import {
  applySearchQuery,
  matchesSearchQueryDate,
  matchesSearchQueryNumber,
  matchesSearchQueryString,
  matchesSearchQueryText,
  normalizeSearchQueryValue,
  searchQueryTermValue,
  type SearchQueryTerm,
} from '../search-query-parser.js';
import type {
  OnlineStoreContentKind,
  OnlineStoreContentRecord,
  OnlineStoreIntegrationKind,
  OnlineStoreIntegrationRecord,
} from '../state/types.js';
import {
  getDocumentFragments,
  getFieldResponseKey,
  isPlainObject,
  paginateConnectionItems,
  projectGraphqlValue,
  serializeConnection,
  serializeEmptyConnectionPageInfo,
  type FragmentMap,
} from './graphql-helpers.js';
import {
  readMetafieldInputObjects,
  serializeMetafieldSelection,
  serializeMetafieldsConnection,
  upsertOwnerMetafields,
  type MetafieldRecordCore,
} from './metafields.js';

type OnlineStoreMutationResult = {
  response: Record<string, unknown>;
  stagedResourceIds: string[];
};

type UserError = {
  field: string[];
  message: string;
};

const CONNECTION_ROOTS = new Set(['articles', 'blogs', 'pages', 'comments', 'articleAuthors']);
const INTEGRATION_CONNECTION_ROOTS = new Set(['themes', 'scriptTags', 'mobilePlatformApplications']);

const INTEGRATION_ROOT_KIND: Record<string, OnlineStoreIntegrationKind> = {
  theme: 'theme',
  themes: 'theme',
  scriptTag: 'scriptTag',
  scriptTags: 'scriptTag',
  webPixel: 'webPixel',
  serverPixel: 'serverPixel',
  mobilePlatformApplication: 'mobilePlatformApplication',
  mobilePlatformApplications: 'mobilePlatformApplication',
};

function readOptionalString(input: Record<string, unknown>, field: string): string | null | undefined {
  if (!Object.prototype.hasOwnProperty.call(input, field)) {
    return undefined;
  }
  const value = input[field];
  return typeof value === 'string' || value === null ? value : undefined;
}

function readOptionalBoolean(input: Record<string, unknown>, field: string): boolean | undefined {
  if (!Object.prototype.hasOwnProperty.call(input, field)) {
    return undefined;
  }
  return typeof input[field] === 'boolean' ? input[field] : undefined;
}

function readStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

function slugify(value: string): string {
  const slug = value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/gu, '-')
    .replace(/^-+|-+$/gu, '');
  return slug || 'untitled';
}

function stripHtml(value: string): string {
  return value.replace(/<[^>]*>/gu, '').trim();
}

function numericGidSuffix(id: string): number | null {
  const suffix = id.split('/').at(-1)?.split('?')[0] ?? '';
  const numeric = Number.parseFloat(suffix);
  return Number.isFinite(numeric) ? numeric : null;
}

function countPayload(count: number): Record<string, unknown> {
  return { count, precision: 'EXACT' };
}

function emptyConnection(field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== 'Field') {
      continue;
    }
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
      case 'edges':
        result[key] = [];
        break;
      case 'pageInfo':
        result[key] = serializeEmptyConnectionPageInfo(selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function recordData(record: OnlineStoreContentRecord): Record<string, unknown> {
  return structuredClone(record.data) as Record<string, unknown>;
}

function readArticleMetafields(record: OnlineStoreContentRecord): MetafieldRecordCore[] {
  const metafields = record.data['metafields'];
  if (!Array.isArray(metafields)) {
    return [];
  }

  return metafields.flatMap((metafield): MetafieldRecordCore[] => {
    if (!isPlainObject(metafield)) {
      return [];
    }
    const id = typeof metafield['id'] === 'string' ? metafield['id'] : null;
    const namespace = typeof metafield['namespace'] === 'string' ? metafield['namespace'] : null;
    const key = typeof metafield['key'] === 'string' ? metafield['key'] : null;
    if (!id || !namespace || !key) {
      return [];
    }
    return [
      {
        id,
        namespace,
        key,
        type: typeof metafield['type'] === 'string' ? metafield['type'] : null,
        value: typeof metafield['value'] === 'string' ? metafield['value'] : null,
        compareDigest: typeof metafield['compareDigest'] === 'string' ? metafield['compareDigest'] : null,
        jsonValue: metafield['jsonValue'] as MetafieldRecordCore['jsonValue'],
        createdAt: typeof metafield['createdAt'] === 'string' ? metafield['createdAt'] : null,
        updatedAt: typeof metafield['updatedAt'] === 'string' ? metafield['updatedAt'] : null,
        ownerType: typeof metafield['ownerType'] === 'string' ? metafield['ownerType'] : 'ARTICLE',
      },
    ];
  });
}

function filterArticleMetafields(
  metafields: MetafieldRecordCore[],
  field: FieldNode,
  variables: Record<string, unknown>,
): MetafieldRecordCore[] {
  const args = getFieldArguments(field, variables);
  const namespace = typeof args['namespace'] === 'string' ? args['namespace'] : null;
  const keys = Array.isArray(args['keys']) ? args['keys'].filter((key): key is string => typeof key === 'string') : [];
  return metafields.filter(
    (metafield) =>
      (namespace === null || metafield.namespace === namespace) && (keys.length === 0 || keys.includes(metafield.key)),
  );
}

function readArticleImage(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing: OnlineStoreContentRecord | null,
): Record<string, JsonValue> | null | undefined {
  if (!Object.prototype.hasOwnProperty.call(input, 'image')) {
    const image = existing?.data['image'];
    return isPlainObject(image)
      ? (structuredClone(image) as Record<string, JsonValue>)
      : image === null
        ? null
        : undefined;
  }

  const image = input['image'];
  if (image === null) {
    return null;
  }
  if (!isPlainObject(image)) {
    return undefined;
  }

  return {
    __typename: 'Image',
    id: runtime.syntheticIdentity.makeProxySyntheticGid('ArticleImage'),
    altText: readOptionalString(image, 'altText') ?? null,
    url: readOptionalString(image, 'url') ?? null,
    width: null,
    height: null,
  };
}

function articleMetafieldsForStorage(metafields: Array<MetafieldRecordCore & { articleId: string }>): JsonValue[] {
  return metafields.map((metafield) => ({
    id: metafield.id,
    namespace: metafield.namespace,
    key: metafield.key,
    type: metafield.type,
    value: metafield.value,
    compareDigest: metafield.compareDigest ?? null,
    jsonValue: metafield.jsonValue ?? null,
    createdAt: metafield.createdAt ?? null,
    updatedAt: metafield.updatedAt ?? null,
    ownerType: metafield.ownerType ?? 'ARTICLE',
    articleId: metafield.articleId,
  }));
}

function readIdArgument(field: FieldNode, variables: Record<string, unknown>): string | null {
  const args = getFieldArguments(field, variables);
  return typeof args['id'] === 'string' ? args['id'] : null;
}

function recordString(record: OnlineStoreContentRecord, field: string): string | null {
  const value = record.data[field];
  return typeof value === 'string' ? value : null;
}

function recordBlog(runtime: ProxyRuntimeContext, record: OnlineStoreContentRecord): OnlineStoreContentRecord | null {
  const blogId =
    typeof record.parentId === 'string'
      ? record.parentId
      : typeof record.data['blogId'] === 'string'
        ? record.data['blogId']
        : null;
  return blogId ? runtime.store.getEffectiveOnlineStoreContentById('blog', blogId) : null;
}

function matchesPublishedStatus(record: OnlineStoreContentRecord, term: SearchQueryTerm): boolean {
  const value = normalizeSearchQueryValue(term.value);
  if (value === 'any') {
    return true;
  }

  const isPublished = record.data['isPublished'] === true;
  if (value === 'published' || value === 'visible') {
    return isPublished;
  }

  if (value === 'unpublished' || value === 'hidden') {
    return !isPublished;
  }

  return false;
}

function matchesOnlineStoreSearchTerm(
  runtime: ProxyRuntimeContext,
  record: OnlineStoreContentRecord,
  term: SearchQueryTerm,
): boolean {
  const field = term.field?.toLowerCase() ?? null;
  const id = String(record.data['id'] ?? record.id);
  const title = recordString(record, 'title');
  const handle = recordString(record, 'handle');
  const body = recordString(record, 'body') ?? recordString(record, 'bodyHtml');
  const status = recordString(record, 'status');
  const author = record.data['author'];
  const authorName = isPlainObject(author) && typeof author['name'] === 'string' ? author['name'] : null;
  const tags = readStringArray(record.data['tags']);
  const blog = record.kind === 'article' ? recordBlog(runtime, record) : null;

  switch (field) {
    case null:
      return (
        matchesSearchQueryText(id, term) ||
        matchesSearchQueryText(title, term) ||
        matchesSearchQueryText(handle, term) ||
        matchesSearchQueryText(body, term) ||
        matchesSearchQueryText(recordString(record, 'summary'), term) ||
        matchesSearchQueryText(status, term) ||
        matchesSearchQueryText(authorName, term) ||
        tags.some((tag) => matchesSearchQueryText(tag, term)) ||
        matchesSearchQueryText(recordString(blog ?? record, 'title'), term)
      );
    case 'id':
      return (
        matchesSearchQueryNumber(numericGidSuffix(id), term) || matchesSearchQueryString(id, term.value, 'includes')
      );
    case 'handle':
      return matchesSearchQueryString(handle, term.value, 'includes');
    case 'title':
      return matchesSearchQueryString(title, term.value, 'includes');
    case 'status':
      return matchesSearchQueryString(status, term.value, 'exact');
    case 'author':
      return matchesSearchQueryString(authorName, term.value, 'includes');
    case 'blog_id': {
      const blogId = record.parentId ?? recordString(record, 'blogId');
      return (
        matchesSearchQueryNumber(blogId ? numericGidSuffix(blogId) : null, term) ||
        matchesSearchQueryString(blogId, term.value, 'includes')
      );
    }
    case 'blog_title':
      return matchesSearchQueryString(recordString(blog ?? record, 'title'), term.value, 'includes');
    case 'tag':
      return tags.some((tag) => matchesSearchQueryString(tag, term.value, 'exact'));
    case 'tag_not':
      return tags.every((tag) => !matchesSearchQueryString(tag, term.value, 'exact'));
    case 'published_status':
      return matchesPublishedStatus(record, term);
    case 'created_at':
      return matchesSearchQueryDate(recordString(record, 'createdAt') ?? record.createdAt, term);
    case 'updated_at':
      return matchesSearchQueryDate(recordString(record, 'updatedAt') ?? record.updatedAt, term);
    case 'published_at':
      return matchesSearchQueryDate(recordString(record, 'publishedAt'), term);
    case 'body':
      return matchesSearchQueryString(body, searchQueryTermValue(term), 'includes');
    default:
      return matchesSearchQueryString(recordString(record, field), searchQueryTermValue(term), 'includes');
  }
}

function applyOnlineStoreSearch(
  runtime: ProxyRuntimeContext,
  records: OnlineStoreContentRecord[],
  query: unknown,
): OnlineStoreContentRecord[] {
  return applySearchQuery(records, query, { recognizeNotKeyword: true }, (record, term) =>
    matchesOnlineStoreSearchTerm(runtime, record, term),
  );
}

function sortRecords(
  records: OnlineStoreContentRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
): OnlineStoreContentRecord[] {
  const args = getFieldArguments(field, variables);
  const sortKey = typeof args['sortKey'] === 'string' ? args['sortKey'] : 'ID';
  const reverse = args['reverse'] === true;
  const sorted = [...records].sort((left, right) => {
    const leftData = left.data;
    const rightData = right.data;
    switch (sortKey) {
      case 'TITLE':
        return (
          String(leftData['title'] ?? '').localeCompare(String(rightData['title'] ?? '')) ||
          left.id.localeCompare(right.id)
        );
      case 'HANDLE':
        return (
          String(leftData['handle'] ?? '').localeCompare(String(rightData['handle'] ?? '')) ||
          left.id.localeCompare(right.id)
        );
      case 'CREATED_AT':
        return String(leftData['createdAt'] ?? left.createdAt ?? '').localeCompare(
          String(rightData['createdAt'] ?? right.createdAt ?? ''),
        );
      case 'UPDATED_AT':
        return String(leftData['updatedAt'] ?? left.updatedAt ?? '').localeCompare(
          String(rightData['updatedAt'] ?? right.updatedAt ?? ''),
        );
      default:
        return left.id.localeCompare(right.id);
    }
  });

  return reverse ? sorted.reverse() : sorted;
}

function listRecords(
  runtime: ProxyRuntimeContext,
  kind: OnlineStoreContentKind,
  field: FieldNode,
  variables: Record<string, unknown>,
): OnlineStoreContentRecord[] {
  const args = getFieldArguments(field, variables);
  return sortRecords(
    applyOnlineStoreSearch(
      runtime,
      runtime.store
        .listEffectiveOnlineStoreContent(kind)
        .filter((record) => kind !== 'article' || record.data['isPublished'] !== false),
      args['query'],
    ),
    field,
    variables,
  );
}

function recordsForNestedConnection(
  runtime: ProxyRuntimeContext,
  kind: OnlineStoreContentKind,
  parentId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): OnlineStoreContentRecord[] {
  return sortRecords(
    applyOnlineStoreSearch(
      runtime,
      runtime.store.listEffectiveOnlineStoreContent(kind).filter((record) => record.parentId === parentId),
      getFieldArguments(field, variables)['query'],
    ),
    field,
    variables,
  );
}

function projectRecord(
  runtime: ProxyRuntimeContext,
  record: OnlineStoreContentRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  if (!field.selectionSet) {
    return recordData(record);
  }

  return projectGraphqlValue(recordData(record), field.selectionSet.selections, fragments, {
    projectFieldValue: ({ field: selectedField, fieldName, fragments: selectedFragments }) => {
      if (fieldName === 'articles' && record.kind === 'blog') {
        return {
          handled: true,
          value: serializeRecordConnection(
            runtime,
            selectedField,
            recordsForNestedConnection(runtime, 'article', record.id, selectedField, variables),
            variables,
            selectedFragments,
          ),
        };
      }

      if (fieldName === 'comments' && record.kind === 'article') {
        return {
          handled: true,
          value: serializeRecordConnection(
            runtime,
            selectedField,
            recordsForNestedConnection(runtime, 'comment', record.id, selectedField, variables),
            variables,
            selectedFragments,
          ),
        };
      }

      if (fieldName === 'articlesCount' && record.kind === 'blog') {
        return {
          handled: true,
          value: countPayload(
            recordsForNestedConnection(runtime, 'article', record.id, selectedField, variables).length,
          ),
        };
      }

      if (fieldName === 'commentsCount' && record.kind === 'article') {
        return {
          handled: true,
          value: countPayload(
            recordsForNestedConnection(runtime, 'comment', record.id, selectedField, variables).length,
          ),
        };
      }

      if (fieldName === 'blog' && record.kind === 'article') {
        const blogId =
          typeof record.data['blogId'] === 'string'
            ? record.data['blogId']
            : typeof record.parentId === 'string'
              ? record.parentId
              : isPlainObject(record.data['blog']) && typeof record.data['blog']['id'] === 'string'
                ? record.data['blog']['id']
                : null;
        const blog = blogId ? runtime.store.getEffectiveOnlineStoreContentById('blog', blogId) : null;
        return {
          handled: true,
          value: blog ? projectRecord(runtime, blog, selectedField, variables, selectedFragments) : null,
        };
      }

      if (fieldName === 'article' && record.kind === 'comment') {
        const articleId = typeof record.parentId === 'string' ? record.parentId : null;
        const article = articleId ? runtime.store.getEffectiveOnlineStoreContentById('article', articleId) : null;
        return {
          handled: true,
          value: article ? projectRecord(runtime, article, selectedField, variables, selectedFragments) : null,
        };
      }

      if (fieldName === 'events') {
        return { handled: true, value: emptyConnection(selectedField) };
      }

      if (fieldName === 'metafields' && record.kind === 'article') {
        return {
          handled: true,
          value: serializeMetafieldsConnection(
            filterArticleMetafields(readArticleMetafields(record), selectedField, variables),
            selectedField,
            variables,
          ),
        };
      }

      if (fieldName === 'metafields') {
        return { handled: true, value: emptyConnection(selectedField) };
      }

      if (fieldName === 'metafield' && record.kind === 'article') {
        const args = getFieldArguments(selectedField, variables);
        const key = typeof args['key'] === 'string' ? args['key'] : null;
        const namespace = typeof args['namespace'] === 'string' ? args['namespace'] : null;
        const metafield =
          key === null
            ? null
            : (readArticleMetafields(record).find(
                (candidate) => candidate.key === key && (namespace === null || candidate.namespace === namespace),
              ) ?? null);
        return { handled: true, value: metafield ? serializeMetafieldSelection(metafield, selectedField) : null };
      }

      if (fieldName === 'metafield') {
        return { handled: true, value: null };
      }

      if (fieldName === 'translations') {
        return { handled: true, value: [] };
      }

      return { handled: false };
    },
  });
}

function serializeRecordConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  records: OnlineStoreContentRecord[],
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const window = paginateConnectionItems(records, field, variables, (record) => record.id);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (record) => record.id,
    serializeNode: (record, nodeField) => projectRecord(runtime, record, nodeField, variables, fragments),
  });
}

function serializeAuthorConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const names = new Set<string>();
  for (const article of runtime.store.listEffectiveOnlineStoreContent('article')) {
    const author = article.data['author'];
    if (isPlainObject(author) && typeof author['name'] === 'string' && author['name'].length > 0) {
      names.add(author['name']);
    }
  }
  const records = [...names].sort().map((name) => ({
    id: name,
    kind: 'article' as const,
    data: { name },
  }));
  const window = paginateConnectionItems(records, field, variables, (record) => record.id);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (record) => record.id,
    serializeNode: (record, nodeField) =>
      nodeField.selectionSet
        ? projectGraphqlValue(record.data, nodeField.selectionSet.selections, fragments)
        : structuredClone(record.data),
  });
}

function readInput(args: Record<string, unknown>, key: string): Record<string, unknown> | null {
  const value = args[key];
  return isPlainObject(value) ? value : null;
}

function userError(field: string[], message: string): UserError {
  return { field, message };
}

function projectPayload(
  runtime: ProxyRuntimeContext,
  payload: Record<string, unknown>,
  field: FieldNode,
  fragments: FragmentMap,
  variables: Record<string, unknown>,
  records: Partial<Record<OnlineStoreContentKind, OnlineStoreContentRecord>> = {},
): unknown {
  return field.selectionSet
    ? projectGraphqlValue(payload, field.selectionSet.selections, fragments, {
        projectFieldValue: ({ field: selectedField, fieldName, fragments: selectedFragments }) => {
          const record = records[fieldName as OnlineStoreContentKind];
          return record
            ? { handled: true, value: projectRecord(runtime, record, selectedField, variables, selectedFragments) }
            : { handled: false };
        },
      })
    : payload;
}

function makeBlog(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing: OnlineStoreContentRecord | null = null,
): OnlineStoreContentRecord {
  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const title = readOptionalString(input, 'title') ?? String(existing?.data['title'] ?? '');
  const handle = readOptionalString(input, 'handle') ?? String(existing?.data['handle'] ?? slugify(title));
  const id = existing?.id ?? runtime.syntheticIdentity.makeProxySyntheticGid('Blog');
  const createdAt = String(existing?.data['createdAt'] ?? now);
  const updatedAt = existing ? now : createdAt;
  return {
    id,
    kind: 'blog',
    createdAt,
    updatedAt,
    data: {
      __typename: 'Blog',
      id,
      title,
      handle,
      commentPolicy: readOptionalString(input, 'commentPolicy') ?? existing?.data['commentPolicy'] ?? 'CLOSED',
      tags: existing?.data['tags'] ?? [],
      templateSuffix: readOptionalString(input, 'templateSuffix') ?? existing?.data['templateSuffix'] ?? null,
      createdAt,
      updatedAt,
    },
  };
}

function makePage(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing: OnlineStoreContentRecord | null = null,
): OnlineStoreContentRecord {
  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const title = readOptionalString(input, 'title') ?? String(existing?.data['title'] ?? '');
  const body = readOptionalString(input, 'body') ?? String(existing?.data['body'] ?? '');
  const isPublished = readOptionalBoolean(input, 'isPublished') ?? Boolean(existing?.data['isPublished'] ?? false);
  const publishedAt =
    readOptionalString(input, 'publishDate') ?? (isPublished ? (existing?.data['publishedAt'] ?? now) : null);
  const id = existing?.id ?? runtime.syntheticIdentity.makeProxySyntheticGid('Page');
  const createdAt = String(existing?.data['createdAt'] ?? now);
  const updatedAt = existing ? now : createdAt;
  return {
    id,
    kind: 'page',
    createdAt,
    updatedAt,
    data: {
      __typename: 'Page',
      id,
      title,
      handle: readOptionalString(input, 'handle') ?? existing?.data['handle'] ?? slugify(title),
      body,
      bodySummary: stripHtml(body),
      isPublished,
      publishedAt,
      createdAt,
      updatedAt,
      templateSuffix: readOptionalString(input, 'templateSuffix') ?? existing?.data['templateSuffix'] ?? null,
    },
  };
}

function makeArticle(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  blogId: string,
  existing: OnlineStoreContentRecord | null = null,
): OnlineStoreContentRecord {
  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const title = readOptionalString(input, 'title') ?? String(existing?.data['title'] ?? '');
  const body = readOptionalString(input, 'body') ?? String(existing?.data['body'] ?? '');
  const isPublished = readOptionalBoolean(input, 'isPublished') ?? Boolean(existing?.data['isPublished'] ?? false);
  const publishedAt =
    readOptionalString(input, 'publishDate') ?? (isPublished ? (existing?.data['publishedAt'] ?? now) : null);
  const authorInput = isPlainObject(input['author']) ? input['author'] : null;
  const existingAuthor = isPlainObject(existing?.data['author']) ? existing?.data['author'] : null;
  const authorName =
    readOptionalString(authorInput ?? {}, 'name') ?? String(existingAuthor?.['name'] ?? 'Shopify Admin');
  const id = existing?.id ?? runtime.syntheticIdentity.makeProxySyntheticGid('Article');
  const createdAt = String(existing?.data['createdAt'] ?? now);
  const updatedAt = existing ? now : createdAt;
  const image = readArticleImage(runtime, input, existing);
  const existingMetafields = readArticleMetafields(
    existing ?? {
      id,
      kind: 'article',
      data: {},
    },
  );
  const { metafields } = upsertOwnerMetafields(
    runtime,
    'articleId',
    id,
    readMetafieldInputObjects(input['metafields']),
    existingMetafields.map((metafield) => ({ ...metafield, articleId: id })),
    { allowIdLookup: true, trimIdentity: true, ownerType: 'ARTICLE' },
  );
  return {
    id,
    kind: 'article',
    parentId: blogId,
    createdAt,
    updatedAt,
    data: {
      __typename: 'Article',
      id,
      blogId,
      title,
      handle: readOptionalString(input, 'handle') ?? existing?.data['handle'] ?? slugify(title),
      body,
      summary: readOptionalString(input, 'summary') ?? existing?.data['summary'] ?? null,
      tags: Object.prototype.hasOwnProperty.call(input, 'tags')
        ? readStringArray(input['tags'])
        : (existing?.data['tags'] ?? []),
      author: { name: authorName },
      isPublished,
      publishedAt,
      createdAt,
      updatedAt,
      templateSuffix: readOptionalString(input, 'templateSuffix') ?? existing?.data['templateSuffix'] ?? null,
      image: image ?? null,
      metafields: articleMetafieldsForStorage(metafields),
    },
  };
}

function handleCreate(
  runtime: ProxyRuntimeContext,
  root: string,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { key: string; payload: unknown; stagedResourceIds: string[] } {
  const args = getFieldArguments(field, variables);
  const inputKey = root.replace(/Create$/u, '');
  const input = readInput(args, inputKey);
  const errors: UserError[] = [];

  if (!input || typeof input['title'] !== 'string' || input['title'].trim().length === 0) {
    errors.push(userError([inputKey, 'title'], "Title can't be blank"));
  }

  let record: OnlineStoreContentRecord | null = null;
  const stagedResourceIds: string[] = [];
  if (errors.length === 0 && input) {
    if (root === 'blogCreate') {
      record = makeBlog(runtime, input);
    } else if (root === 'pageCreate') {
      record = makePage(runtime, input);
    } else {
      let blogId = readOptionalString(input, 'blogId') ?? null;
      const blogInput = readInput(args, 'blog');
      if (!blogId && blogInput && typeof blogInput['title'] === 'string') {
        const blog = makeBlog(runtime, blogInput);
        runtime.store.upsertStagedOnlineStoreContent(blog);
        blogId = blog.id;
        stagedResourceIds.push(blog.id);
      }
      if (!blogId) {
        errors.push(userError(['article', 'blogId'], 'Blog must exist'));
      } else {
        record = makeArticle(runtime, input, blogId);
      }
    }
  }

  if (record) {
    runtime.store.upsertStagedOnlineStoreContent(record);
  }

  const payloadKey = inputKey;
  return {
    key: root,
    payload: projectPayload(
      runtime,
      { [payloadKey]: record ? recordData(record) : null, userErrors: errors },
      field,
      fragments,
      variables,
      record ? { [record.kind]: record } : {},
    ),
    stagedResourceIds: record ? [...stagedResourceIds, record.id] : stagedResourceIds,
  };
}

function handleUpdate(
  runtime: ProxyRuntimeContext,
  root: string,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { key: string; payload: unknown; stagedResourceIds: string[] } {
  const args = getFieldArguments(field, variables);
  const inputKey = root.replace(/Update$/u, '');
  const kind = inputKey as OnlineStoreContentKind;
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const input = readInput(args, inputKey);
  const existing = id ? runtime.store.getEffectiveOnlineStoreContentById(kind, id) : null;
  const errors: UserError[] = [];

  if (!id || !existing) {
    errors.push(userError(['id'], `${inputKey[0]?.toUpperCase() ?? ''}${inputKey.slice(1)} does not exist`));
  }

  let record: OnlineStoreContentRecord | null = null;
  if (errors.length === 0 && input && existing) {
    if (kind === 'blog') {
      record = makeBlog(runtime, input, existing);
    } else if (kind === 'page') {
      record = makePage(runtime, input, existing);
    } else {
      const blogId = readOptionalString(input, 'blogId') ?? existing.parentId ?? String(existing.data['blogId'] ?? '');
      record = makeArticle(runtime, input, blogId, existing);
    }
    runtime.store.upsertStagedOnlineStoreContent(record);
  }

  return {
    key: root,
    payload: projectPayload(
      runtime,
      { [inputKey]: record ? recordData(record) : null, userErrors: errors },
      field,
      fragments,
      variables,
      record ? { [record.kind]: record } : {},
    ),
    stagedResourceIds: record ? [record.id] : [],
  };
}

function handleDelete(
  runtime: ProxyRuntimeContext,
  root: string,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { key: string; payload: unknown; stagedResourceIds: string[] } {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const kind = root.replace(/Delete$/u, '') as OnlineStoreContentKind;
  const existing = id ? runtime.store.getEffectiveOnlineStoreContentById(kind, id) : null;
  const errors = existing ? [] : [userError(['id'], `${kind[0]?.toUpperCase() ?? ''}${kind.slice(1)} does not exist`)];
  if (id && existing) {
    runtime.store.deleteStagedOnlineStoreContent(kind, id);
  }

  const deletedKey = `deleted${kind[0]?.toUpperCase() ?? ''}${kind.slice(1)}Id`;
  return {
    key: root,
    payload: projectPayload(
      runtime,
      { [deletedKey]: errors.length === 0 ? id : null, userErrors: errors },
      field,
      fragments,
      variables,
    ),
    stagedResourceIds: [],
  };
}

function handleCommentModeration(
  runtime: ProxyRuntimeContext,
  root: string,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { key: string; payload: unknown; stagedResourceIds: string[] } {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const existing = id ? runtime.store.getEffectiveOnlineStoreContentById('comment', id) : null;
  const errors = existing ? [] : [userError(['id'], 'Comment does not exist')];
  const statusByRoot: Record<string, string> = {
    commentApprove: 'PUBLISHED',
    commentSpam: 'SPAM',
    commentNotSpam: 'PENDING',
  };
  const updatedAt = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const comment =
    existing && errors.length === 0
      ? {
          ...existing,
          updatedAt,
          data: {
            ...existing.data,
            status: statusByRoot[root] ?? String(existing.data['status'] ?? 'PENDING'),
            isPublished: root === 'commentApprove',
            publishedAt: root === 'commentApprove' ? (existing.data['publishedAt'] ?? updatedAt) : null,
            updatedAt,
          },
        }
      : null;
  if (comment) {
    runtime.store.upsertStagedOnlineStoreContent(comment);
  }

  return {
    key: root,
    payload: projectPayload(
      runtime,
      { comment: comment ? recordData(comment) : null, userErrors: errors },
      field,
      fragments,
      variables,
      comment ? { comment } : {},
    ),
    stagedResourceIds: comment ? [comment.id] : [],
  };
}

function integrationData(record: OnlineStoreIntegrationRecord): Record<string, unknown> {
  return structuredClone(record.data) as Record<string, unknown>;
}

function readInputArray(value: unknown): Array<Record<string, unknown>> {
  return Array.isArray(value) ? value.filter((item): item is Record<string, unknown> => isPlainObject(item)) : [];
}

function readFiles(record: OnlineStoreIntegrationRecord): Array<Record<string, unknown>> {
  const files = record.data['files'];
  return Array.isArray(files)
    ? (files as unknown[]).filter((file): file is Record<string, unknown> => isPlainObject(file))
    : [];
}

function writeFiles(
  record: OnlineStoreIntegrationRecord,
  files: Array<Record<string, unknown>>,
): OnlineStoreIntegrationRecord {
  return {
    ...record,
    data: {
      ...record.data,
      files: structuredClone(files) as unknown as OnlineStoreIntegrationRecord['data'][string],
    },
  };
}

function redactSensitiveIntegrationData(record: OnlineStoreIntegrationRecord): OnlineStoreIntegrationRecord {
  if (record.kind !== 'storefrontAccessToken') {
    return record;
  }

  return {
    ...record,
    data: {
      ...record.data,
      accessToken: 'shpat_redacted',
    },
  };
}

function projectIntegrationRecord(
  record: OnlineStoreIntegrationRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): unknown {
  const data = integrationData(redactSensitiveIntegrationData(record));
  if (!field.selectionSet) {
    return data;
  }

  return projectGraphqlValue(data, field.selectionSet.selections, fragments, {
    projectFieldValue: ({ field: selectedField, fieldName, fragments: selectedFragments }) => {
      if (fieldName === 'files' && record.kind === 'theme') {
        const files = readFiles(record);
        const window = paginateConnectionItems(files, selectedField, variables, (file) =>
          String(file['filename'] ?? ''),
        );
        return {
          handled: true,
          value: serializeConnection(selectedField, {
            items: window.items,
            hasNextPage: window.hasNextPage,
            hasPreviousPage: window.hasPreviousPage,
            getCursorValue: (file) => String(file['filename'] ?? ''),
            serializeNode: (file, nodeField) =>
              nodeField.selectionSet
                ? projectGraphqlValue(file, nodeField.selectionSet.selections, selectedFragments)
                : structuredClone(file),
            serializeUnknownField: (connectionField) => (connectionField.name.value === 'userErrors' ? [] : null),
          }),
        };
      }

      if (fieldName === 'accessScopes' && record.kind === 'storefrontAccessToken') {
        return { handled: true, value: data['accessScopes'] ?? [] };
      }

      return { handled: false };
    },
  });
}

function serializeIntegrationConnection(
  field: FieldNode,
  records: OnlineStoreIntegrationRecord[],
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const filtered = records.filter((record) => {
    if (record.kind === 'theme') {
      const roles = Array.isArray(args['roles'])
        ? args['roles'].filter((role): role is string => typeof role === 'string')
        : [];
      const names = Array.isArray(args['names'])
        ? args['names'].filter((name): name is string => typeof name === 'string')
        : [];
      return (
        (roles.length === 0 || roles.includes(String(record.data['role'] ?? ''))) &&
        (names.length === 0 || names.includes(String(record.data['name'] ?? '')))
      );
    }

    if (record.kind === 'scriptTag') {
      const src = typeof args['src'] === 'string' ? args['src'] : null;
      const query = typeof args['query'] === 'string' ? args['query'].trim().toLowerCase() : '';
      return (
        (!src || record.data['src'] === src) &&
        (query.length === 0 ||
          String(record.data['src'] ?? '')
            .toLowerCase()
            .includes(query) ||
          String(record.id).toLowerCase().includes(query))
      );
    }

    return true;
  });
  const sorted = args['reverse'] === true ? [...filtered].reverse() : filtered;
  const window = paginateConnectionItems(sorted, field, variables, (record) => record.id);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (record) => record.id,
    serializeNode: (record, nodeField) => projectIntegrationRecord(record, nodeField, variables, fragments),
  });
}

function projectIntegrationPayload(
  payload: Record<string, unknown>,
  field: FieldNode,
  fragments: FragmentMap,
  variables: Record<string, unknown>,
  records: Partial<Record<OnlineStoreIntegrationKind, OnlineStoreIntegrationRecord>> = {},
): unknown {
  return field.selectionSet
    ? projectGraphqlValue(payload, field.selectionSet.selections, fragments, {
        projectFieldValue: ({ field: selectedField, fieldName, fragments: selectedFragments }) => {
          const record =
            records[fieldName as OnlineStoreIntegrationKind] ?? records[`${fieldName}` as OnlineStoreIntegrationKind];
          if (record) {
            return {
              handled: true,
              value: projectIntegrationRecord(record, selectedField, variables, selectedFragments),
            };
          }
          return { handled: false };
        },
      })
    : payload;
}

function makeIntegrationRecord(
  runtime: ProxyRuntimeContext,
  kind: OnlineStoreIntegrationKind,
  data: Record<string, unknown>,
  existing: OnlineStoreIntegrationRecord | null = null,
): OnlineStoreIntegrationRecord {
  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const id = existing?.id ?? runtime.syntheticIdentity.makeProxySyntheticGid(integrationGidType(kind));
  const createdAt = String(existing?.data['createdAt'] ?? now);
  const updatedAt = existing ? now : createdAt;
  return {
    id,
    kind,
    createdAt,
    updatedAt,
    data: {
      ...structuredClone(existing?.data ?? {}),
      ...structuredClone(data),
      id,
      createdAt,
      updatedAt,
    },
  };
}

function integrationGidType(kind: OnlineStoreIntegrationKind): string {
  switch (kind) {
    case 'theme':
      return 'OnlineStoreTheme';
    case 'scriptTag':
      return 'ScriptTag';
    case 'webPixel':
      return 'WebPixel';
    case 'serverPixel':
      return 'ServerPixel';
    case 'storefrontAccessToken':
      return 'StorefrontAccessToken';
    case 'mobilePlatformApplication':
      return 'MobilePlatformApplication';
  }
}

function makeThemeRecord(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
  existing: OnlineStoreIntegrationRecord | null = null,
) {
  const name = readOptionalString(args, 'name') ?? String(existing?.data['name'] ?? 'Draft proxy theme');
  const role = typeof args['role'] === 'string' ? args['role'] : String(existing?.data['role'] ?? 'UNPUBLISHED');
  const record = makeIntegrationRecord(
    runtime,
    'theme',
    {
      __typename: 'OnlineStoreTheme',
      name,
      role,
      prefix: existing?.data['prefix'] ?? `themes/${slugify(name)}`,
      processing: false,
      processingFailed: false,
      themeStoreId: existing?.data['themeStoreId'] ?? null,
      translations: existing?.data['translations'] ?? [],
      files: existing?.data['files'] ?? [],
    },
    existing,
  );
  return record;
}

function makeScriptTagRecord(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing: OnlineStoreIntegrationRecord | null = null,
): OnlineStoreIntegrationRecord {
  const src = readOptionalString(input, 'src') ?? String(existing?.data['src'] ?? '');
  const legacyResourceId = existing?.data['legacyResourceId'] ?? numericGidSuffix(existing?.id ?? '') ?? 1;
  return makeIntegrationRecord(
    runtime,
    'scriptTag',
    {
      __typename: 'ScriptTag',
      src,
      displayScope: readOptionalString(input, 'displayScope') ?? existing?.data['displayScope'] ?? 'ALL',
      cache: readOptionalBoolean(input, 'cache') ?? Boolean(existing?.data['cache'] ?? false),
      legacyResourceId,
    },
    existing,
  );
}

function makeWebPixelRecord(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing: OnlineStoreIntegrationRecord | null = null,
): OnlineStoreIntegrationRecord {
  return makeIntegrationRecord(
    runtime,
    'webPixel',
    {
      __typename: 'WebPixel',
      settings: structuredClone(input['settings'] ?? existing?.data['settings'] ?? {}),
    },
    existing,
  );
}

function makeServerPixelRecord(
  runtime: ProxyRuntimeContext,
  data: Record<string, unknown> = {},
  existing: OnlineStoreIntegrationRecord | null = null,
): OnlineStoreIntegrationRecord {
  return makeIntegrationRecord(
    runtime,
    'serverPixel',
    {
      __typename: 'ServerPixel',
      status: data['status'] ?? existing?.data['status'] ?? 'CONNECTED',
      webhookEndpointAddress:
        readOptionalString(data, 'webhookEndpointAddress') ?? existing?.data['webhookEndpointAddress'] ?? null,
    },
    existing,
  );
}

function makeStorefrontAccessTokenRecord(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing: OnlineStoreIntegrationRecord | null = null,
): OnlineStoreIntegrationRecord {
  const title = readOptionalString(input, 'title') ?? String(existing?.data['title'] ?? '');
  return makeIntegrationRecord(
    runtime,
    'storefrontAccessToken',
    {
      __typename: 'StorefrontAccessToken',
      title,
      accessToken: 'shpat_redacted',
      accessScopes: existing?.data['accessScopes'] ?? [],
    },
    existing,
  );
}

function makeMobilePlatformApplicationRecord(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing: OnlineStoreIntegrationRecord | null = null,
): OnlineStoreIntegrationRecord {
  const android = isPlainObject(input['android']) ? input['android'] : null;
  const apple = isPlainObject(input['apple']) ? input['apple'] : null;
  if (android) {
    return makeIntegrationRecord(
      runtime,
      'mobilePlatformApplication',
      {
        __typename: 'AndroidApplication',
        applicationId: readOptionalString(android, 'applicationId') ?? existing?.data['applicationId'] ?? null,
        sha256CertFingerprints: Object.prototype.hasOwnProperty.call(android, 'sha256CertFingerprints')
          ? readStringArray(android['sha256CertFingerprints'])
          : (existing?.data['sha256CertFingerprints'] ?? []),
        appLinksEnabled:
          readOptionalBoolean(android, 'appLinksEnabled') ?? Boolean(existing?.data['appLinksEnabled'] ?? false),
      },
      existing,
    );
  }

  return makeIntegrationRecord(
    runtime,
    'mobilePlatformApplication',
    {
      __typename: 'AppleApplication',
      appId: readOptionalString(apple ?? {}, 'appId') ?? existing?.data['appId'] ?? null,
      appClipApplicationId:
        readOptionalString(apple ?? {}, 'appClipApplicationId') ?? existing?.data['appClipApplicationId'] ?? null,
      sharedWebCredentialsEnabled:
        readOptionalBoolean(apple ?? {}, 'sharedWebCredentialsEnabled') ??
        Boolean(existing?.data['sharedWebCredentialsEnabled'] ?? false),
      universalLinksEnabled:
        readOptionalBoolean(apple ?? {}, 'universalLinksEnabled') ??
        Boolean(existing?.data['universalLinksEnabled'] ?? false),
      appClipsEnabled:
        readOptionalBoolean(apple ?? {}, 'appClipsEnabled') ?? Boolean(existing?.data['appClipsEnabled'] ?? false),
    },
    existing,
  );
}

function themeFileBodyValue(file: Record<string, unknown> | null): string {
  const body = isPlainObject(file?.['body']) ? file['body'] : null;
  if (typeof body?.['content'] === 'string') {
    return body['content'];
  }
  if (typeof body?.['contentBase64'] === 'string') {
    return body['contentBase64'];
  }
  return '';
}

function themeFileFromInput(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing: Record<string, unknown> | null = null,
) {
  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const filename = String(input['filename'] ?? input['dstFilename'] ?? existing?.['filename'] ?? '');
  const bodyInput = isPlainObject(input['body']) ? input['body'] : null;
  const bodyValue = typeof bodyInput?.['value'] === 'string' ? bodyInput['value'] : themeFileBodyValue(existing);
  const isBase64 =
    bodyInput?.['type'] === 'BASE64' ||
    (bodyInput === null &&
      isPlainObject(existing?.['body']) &&
      existing['body']['__typename'] === 'OnlineStoreThemeFileBodyBase64');
  const body = isBase64
    ? { __typename: 'OnlineStoreThemeFileBodyBase64', contentBase64: bodyValue }
    : { __typename: 'OnlineStoreThemeFileBodyText', content: bodyValue };
  const createdAt = String(existing?.['createdAt'] ?? now);
  const updatedAt = existing ? now : createdAt;
  return {
    __typename: 'OnlineStoreThemeFile',
    filename,
    body,
    contentType: String(existing?.['contentType'] ?? 'text/plain'),
    checksumMd5: createHash('md5').update(bodyValue).digest('hex'),
    size: bodyValue.length,
    createdAt,
    updatedAt,
  };
}

function fileOperationResult(runtime: ProxyRuntimeContext, file: Record<string, unknown>): Record<string, unknown> {
  return {
    __typename: 'OnlineStoreThemeFileOperationResult',
    filename: file['filename'],
    checksumMd5: file['checksumMd5'] ?? null,
    size: file['size'] ?? 0,
    createdAt: file['createdAt'] ?? runtime.syntheticIdentity.makeSyntheticTimestamp(),
    updatedAt: file['updatedAt'] ?? runtime.syntheticIdentity.makeSyntheticTimestamp(),
  };
}

function handleThemeMutation(
  runtime: ProxyRuntimeContext,
  root: string,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { payload: unknown; stagedResourceIds: string[] } {
  const args = getFieldArguments(field, variables);
  const errors: UserError[] = [];
  const id = typeof args['id'] === 'string' ? args['id'] : typeof args['themeId'] === 'string' ? args['themeId'] : null;
  const existing = id ? runtime.store.getEffectiveOnlineStoreIntegrationById('theme', id) : null;

  if ((root !== 'themeCreate' && !existing) || (root === 'themeCreate' && typeof args['source'] !== 'string')) {
    errors.push(
      userError(
        [root === 'themeCreate' ? 'source' : 'id'],
        root === 'themeCreate' ? "Source can't be blank" : 'Theme does not exist',
      ),
    );
  }

  if (errors.length > 0) {
    const emptyPayload =
      root === 'themeDelete'
        ? { deletedThemeId: null, userErrors: errors }
        : root === 'themeFilesUpsert'
          ? { job: null, upsertedThemeFiles: [], userErrors: errors }
          : root === 'themeFilesCopy'
            ? { copiedThemeFiles: [], userErrors: errors }
            : root === 'themeFilesDelete'
              ? { deletedThemeFiles: [], userErrors: errors }
              : { theme: null, userErrors: errors };
    return { payload: projectIntegrationPayload(emptyPayload, field, fragments, variables), stagedResourceIds: [] };
  }

  if (root === 'themeDelete' && id) {
    runtime.store.deleteStagedOnlineStoreIntegration('theme', id);
    return {
      payload: projectIntegrationPayload({ deletedThemeId: id, userErrors: [] }, field, fragments, variables),
      stagedResourceIds: [],
    };
  }

  if (root === 'themeFilesUpsert' && existing) {
    const files = readFiles(existing);
    const byName = new Map(files.map((file) => [String(file['filename'] ?? ''), file]));
    const upserted = readInputArray(args['files']).map((input) => {
      const file = themeFileFromInput(runtime, input, byName.get(String(input['filename'] ?? '')) ?? null);
      byName.set(String(file['filename']), file);
      return file;
    });
    const next = writeFiles(existing, [...byName.values()]);
    runtime.store.upsertStagedOnlineStoreIntegration(next);
    return {
      payload: projectIntegrationPayload(
        {
          job: { __typename: 'Job', id: runtime.syntheticIdentity.makeProxySyntheticGid('Job'), done: true },
          upsertedThemeFiles: upserted.map((file) => fileOperationResult(runtime, file)),
          userErrors: [],
        },
        field,
        fragments,
        variables,
      ),
      stagedResourceIds: [existing.id],
    };
  }

  if (root === 'themeFilesCopy' && existing) {
    const files = readFiles(existing);
    const byName = new Map(files.map((file) => [String(file['filename'] ?? ''), file]));
    const copied = readInputArray(args['files']).flatMap((input): Array<Record<string, unknown>> => {
      const source = byName.get(String(input['srcFilename'] ?? ''));
      if (!source) {
        return [];
      }
      const file = themeFileFromInput(runtime, { ...source, filename: input['dstFilename'] }, source);
      byName.set(String(file['filename']), file);
      return [file];
    });
    const next = writeFiles(existing, [...byName.values()]);
    runtime.store.upsertStagedOnlineStoreIntegration(next);
    return {
      payload: projectIntegrationPayload(
        { copiedThemeFiles: copied.map((file) => fileOperationResult(runtime, file)), userErrors: [] },
        field,
        fragments,
        variables,
      ),
      stagedResourceIds: [existing.id],
    };
  }

  if (root === 'themeFilesDelete' && existing) {
    const deleteNames = new Set(readStringArray(args['files']));
    const deleted = readFiles(existing).filter((file) => deleteNames.has(String(file['filename'] ?? '')));
    const next = writeFiles(
      existing,
      readFiles(existing).filter((file) => !deleteNames.has(String(file['filename'] ?? ''))),
    );
    runtime.store.upsertStagedOnlineStoreIntegration(next);
    return {
      payload: projectIntegrationPayload(
        { deletedThemeFiles: deleted.map((file) => fileOperationResult(runtime, file)), userErrors: [] },
        field,
        fragments,
        variables,
      ),
      stagedResourceIds: [existing.id],
    };
  }

  const input = isPlainObject(args['input']) ? args['input'] : args;
  const theme = root === 'themeCreate' ? makeThemeRecord(runtime, args) : makeThemeRecord(runtime, input, existing);
  if (root === 'themePublish') {
    for (const current of runtime.store.listEffectiveOnlineStoreIntegrations('theme')) {
      if (current.id !== theme.id && current.data['role'] === 'MAIN') {
        runtime.store.upsertStagedOnlineStoreIntegration(makeThemeRecord(runtime, { role: 'UNPUBLISHED' }, current));
      }
    }
    theme.data['role'] = 'MAIN';
  }
  runtime.store.upsertStagedOnlineStoreIntegration(theme);
  return {
    payload: projectIntegrationPayload({ theme: integrationData(theme), userErrors: [] }, field, fragments, variables, {
      theme,
    }),
    stagedResourceIds: [theme.id],
  };
}

function handleScriptTagMutation(
  runtime: ProxyRuntimeContext,
  root: string,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { payload: unknown; stagedResourceIds: string[] } {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const input = readInput(args, 'input');
  const existing = id ? runtime.store.getEffectiveOnlineStoreIntegrationById('scriptTag', id) : null;
  const errors: UserError[] = [];

  if (root === 'scriptTagCreate' && (!input || typeof input['src'] !== 'string' || input['src'].length === 0)) {
    errors.push(userError(['input', 'src'], "Src can't be blank"));
  } else if (root !== 'scriptTagCreate' && (!id || !existing)) {
    errors.push(userError(['id'], 'Script tag does not exist'));
  }

  if (errors.length > 0) {
    const payload =
      root === 'scriptTagDelete'
        ? { deletedScriptTagId: null, userErrors: errors }
        : { scriptTag: null, userErrors: errors };
    return { payload: projectIntegrationPayload(payload, field, fragments, variables), stagedResourceIds: [] };
  }

  if (root === 'scriptTagDelete' && id) {
    runtime.store.deleteStagedOnlineStoreIntegration('scriptTag', id);
    return {
      payload: projectIntegrationPayload({ deletedScriptTagId: id, userErrors: [] }, field, fragments, variables),
      stagedResourceIds: [],
    };
  }

  const scriptTag = makeScriptTagRecord(runtime, input ?? {}, existing);
  runtime.store.upsertStagedOnlineStoreIntegration(scriptTag);
  return {
    payload: projectIntegrationPayload(
      { scriptTag: integrationData(scriptTag), userErrors: [] },
      field,
      fragments,
      variables,
      { scriptTag },
    ),
    stagedResourceIds: [scriptTag.id],
  };
}

function handlePixelMutation(
  runtime: ProxyRuntimeContext,
  root: string,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { payload: unknown; stagedResourceIds: string[] } {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const isServerPixel =
    root.startsWith('serverPixel') || root === 'eventBridgeServerPixelUpdate' || root === 'pubSubServerPixelUpdate';
  const kind: OnlineStoreIntegrationKind = isServerPixel ? 'serverPixel' : 'webPixel';
  const existing = id
    ? runtime.store.getEffectiveOnlineStoreIntegrationById(kind, id)
    : (runtime.store.listEffectiveOnlineStoreIntegrations(kind)[0] ?? null);
  const errors: UserError[] = [];

  if ((root.endsWith('Update') || root.endsWith('Delete')) && !existing) {
    errors.push(
      userError(id ? ['id'] : [], isServerPixel ? 'Server pixel does not exist' : 'Web pixel does not exist'),
    );
  }

  if (errors.length > 0) {
    const deletedKey = isServerPixel ? 'deletedServerPixelId' : 'deletedWebPixelId';
    const recordKey = isServerPixel ? 'serverPixel' : 'webPixel';
    const payload = root.endsWith('Delete')
      ? { [deletedKey]: null, userErrors: errors }
      : { [recordKey]: null, userErrors: errors };
    return { payload: projectIntegrationPayload(payload, field, fragments, variables), stagedResourceIds: [] };
  }

  if (root.endsWith('Delete') && existing) {
    runtime.store.deleteStagedOnlineStoreIntegration(kind, existing.id);
    const deletedKey = isServerPixel ? 'deletedServerPixelId' : 'deletedWebPixelId';
    return {
      payload: projectIntegrationPayload({ [deletedKey]: existing.id, userErrors: [] }, field, fragments, variables),
      stagedResourceIds: [],
    };
  }

  const record = isServerPixel
    ? makeServerPixelRecord(
        runtime,
        root === 'eventBridgeServerPixelUpdate'
          ? { webhookEndpointAddress: args['arn'] }
          : root === 'pubSubServerPixelUpdate'
            ? { webhookEndpointAddress: `${args['pubSubProject'] ?? ''}/${args['pubSubTopic'] ?? ''}` }
            : {},
        existing,
      )
    : makeWebPixelRecord(runtime, readInput(args, 'webPixel') ?? {}, existing);
  runtime.store.upsertStagedOnlineStoreIntegration(record);
  const recordKey = isServerPixel ? 'serverPixel' : 'webPixel';
  return {
    payload: projectIntegrationPayload(
      { [recordKey]: integrationData(record), userErrors: [] },
      field,
      fragments,
      variables,
      {
        [kind]: record,
      },
    ),
    stagedResourceIds: [record.id],
  };
}

function handleStorefrontAccessTokenMutation(
  runtime: ProxyRuntimeContext,
  root: string,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { payload: unknown; stagedResourceIds: string[] } {
  const args = getFieldArguments(field, variables);
  const input = readInput(args, 'input');
  const id = input && typeof input['id'] === 'string' ? input['id'] : null;
  const existing = id ? runtime.store.getEffectiveOnlineStoreIntegrationById('storefrontAccessToken', id) : null;
  const errors: UserError[] = [];

  if (
    root === 'storefrontAccessTokenCreate' &&
    (!input || typeof input['title'] !== 'string' || input['title'].trim().length === 0)
  ) {
    errors.push(userError(['input', 'title'], "Title can't be blank"));
  } else if (root === 'storefrontAccessTokenDelete' && (!id || !existing)) {
    errors.push(userError(['input', 'id'], 'Storefront access token does not exist'));
  }

  if (errors.length > 0) {
    const payload =
      root === 'storefrontAccessTokenDelete'
        ? { deletedStorefrontAccessTokenId: null, userErrors: errors }
        : { shop: null, storefrontAccessToken: null, userErrors: errors };
    return { payload: projectIntegrationPayload(payload, field, fragments, variables), stagedResourceIds: [] };
  }

  if (root === 'storefrontAccessTokenDelete' && id) {
    runtime.store.deleteStagedOnlineStoreIntegration('storefrontAccessToken', id);
    return {
      payload: projectIntegrationPayload(
        { deletedStorefrontAccessTokenId: id, userErrors: [] },
        field,
        fragments,
        variables,
      ),
      stagedResourceIds: [],
    };
  }

  const token = makeStorefrontAccessTokenRecord(runtime, input ?? {});
  runtime.store.upsertStagedOnlineStoreIntegration(token);
  return {
    payload: projectIntegrationPayload(
      {
        shop: { __typename: 'Shop', id: 'gid://shopify/Shop/local', name: 'Local draft shop' },
        storefrontAccessToken: integrationData(redactSensitiveIntegrationData(token)),
        userErrors: [],
      },
      field,
      fragments,
      variables,
      { storefrontAccessToken: token },
    ),
    stagedResourceIds: [token.id],
  };
}

function handleMobilePlatformApplicationMutation(
  runtime: ProxyRuntimeContext,
  root: string,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { payload: unknown; stagedResourceIds: string[] } {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const input = readInput(args, 'input');
  const existing = id ? runtime.store.getEffectiveOnlineStoreIntegrationById('mobilePlatformApplication', id) : null;
  const errors: UserError[] = [];

  if (root === 'mobilePlatformApplicationCreate' && !input) {
    errors.push(userError(['input'], 'Input must be present'));
  } else if (root !== 'mobilePlatformApplicationCreate' && (!id || !existing)) {
    errors.push(userError(['id'], 'Mobile platform application does not exist'));
  }

  if (errors.length > 0) {
    const payload =
      root === 'mobilePlatformApplicationDelete'
        ? { deletedMobilePlatformApplicationId: null, userErrors: errors }
        : { mobilePlatformApplication: null, userErrors: errors };
    return { payload: projectIntegrationPayload(payload, field, fragments, variables), stagedResourceIds: [] };
  }

  if (root === 'mobilePlatformApplicationDelete' && id) {
    runtime.store.deleteStagedOnlineStoreIntegration('mobilePlatformApplication', id);
    return {
      payload: projectIntegrationPayload(
        { deletedMobilePlatformApplicationId: id, userErrors: [] },
        field,
        fragments,
        variables,
      ),
      stagedResourceIds: [],
    };
  }

  const app = makeMobilePlatformApplicationRecord(runtime, input ?? {}, existing);
  runtime.store.upsertStagedOnlineStoreIntegration(app);
  return {
    payload: projectIntegrationPayload(
      { mobilePlatformApplication: integrationData(app), userErrors: [] },
      field,
      fragments,
      variables,
      { mobilePlatformApplication: app },
    ),
    stagedResourceIds: [app.id],
  };
}

function handleIntegrationMutation(
  runtime: ProxyRuntimeContext,
  root: string,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { payload: unknown; stagedResourceIds: string[] } | null {
  if (root.startsWith('theme')) {
    return handleThemeMutation(runtime, root, field, variables, fragments);
  }
  if (root.startsWith('scriptTag')) {
    return handleScriptTagMutation(runtime, root, field, variables, fragments);
  }
  if (
    root.startsWith('webPixel') ||
    root.startsWith('serverPixel') ||
    root === 'eventBridgeServerPixelUpdate' ||
    root === 'pubSubServerPixelUpdate'
  ) {
    return handlePixelMutation(runtime, root, field, variables, fragments);
  }
  if (root.startsWith('storefrontAccessToken')) {
    return handleStorefrontAccessTokenMutation(runtime, root, field, variables, fragments);
  }
  if (root.startsWith('mobilePlatformApplication')) {
    return handleMobilePlatformApplicationMutation(runtime, root, field, variables, fragments);
  }
  return null;
}

export function handleOnlineStoreMutation(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): OnlineStoreMutationResult | null {
  const fragments = getDocumentFragments(document);
  const data: Record<string, unknown> = {};
  const stagedResourceIds: string[] = [];
  let handled = false;

  for (const field of getRootFields(document)) {
    const root = field.name.value;
    const result =
      root === 'blogCreate' || root === 'pageCreate' || root === 'articleCreate'
        ? handleCreate(runtime, root, field, variables, fragments)
        : root === 'blogUpdate' || root === 'pageUpdate' || root === 'articleUpdate'
          ? handleUpdate(runtime, root, field, variables, fragments)
          : root === 'blogDelete' || root === 'pageDelete' || root === 'articleDelete'
            ? handleDelete(runtime, root, field, variables, fragments)
            : root === 'commentDelete'
              ? handleDelete(runtime, root, field, variables, fragments)
              : root === 'commentApprove' || root === 'commentSpam' || root === 'commentNotSpam'
                ? handleCommentModeration(runtime, root, field, variables, fragments)
                : handleIntegrationMutation(runtime, root, field, variables, fragments);
    if (!result) {
      continue;
    }
    handled = true;
    data[getFieldResponseKey(field)] = result.payload;
    stagedResourceIds.push(...result.stagedResourceIds);
  }

  return handled ? { response: { data }, stagedResourceIds } : null;
}

export function handleOnlineStoreQuery(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const fragments = getDocumentFragments(document);
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const root = field.name.value;
    const key = getFieldResponseKey(field);
    if (root === 'article' || root === 'blog' || root === 'page' || root === 'comment') {
      const id = readIdArgument(field, variables);
      const record = id ? runtime.store.getEffectiveOnlineStoreContentById(root, id) : null;
      data[key] = record ? projectRecord(runtime, record, field, variables, fragments) : null;
      continue;
    }

    if (root === 'articles' || root === 'blogs' || root === 'pages' || root === 'comments') {
      const kind = root.slice(0, -1) as OnlineStoreContentKind;
      data[key] = serializeRecordConnection(
        runtime,
        field,
        listRecords(runtime, kind, field, variables),
        variables,
        fragments,
      );
      continue;
    }

    if (root === 'articleAuthors') {
      data[key] = serializeAuthorConnection(runtime, field, variables, fragments);
      continue;
    }

    if (root === 'articleTags') {
      const tags = new Set<string>();
      for (const article of runtime.store.listEffectiveOnlineStoreContent('article')) {
        for (const tag of readStringArray(article.data['tags'])) {
          tags.add(tag);
        }
      }
      data[key] = [...tags].sort().slice(0, Number(getFieldArguments(field, variables)['limit'] ?? tags.size));
      continue;
    }

    if (root === 'blogsCount' || root === 'pagesCount') {
      const kind = root === 'blogsCount' ? 'blog' : 'page';
      const count = listRecords(runtime, kind, field, variables).length;
      data[key] = field.selectionSet
        ? projectGraphqlValue(countPayload(count), field.selectionSet.selections, fragments)
        : countPayload(count);
      continue;
    }

    if (root === 'theme' || root === 'scriptTag' || root === 'mobilePlatformApplication') {
      const kind = INTEGRATION_ROOT_KIND[root];
      const id = readIdArgument(field, variables);
      const record = id && kind ? runtime.store.getEffectiveOnlineStoreIntegrationById(kind, id) : null;
      data[key] = record ? projectIntegrationRecord(record, field, variables, fragments) : null;
      continue;
    }

    if (root === 'themes' || root === 'scriptTags' || root === 'mobilePlatformApplications') {
      const kind = INTEGRATION_ROOT_KIND[root];
      data[key] = kind
        ? serializeIntegrationConnection(
            field,
            runtime.store.listEffectiveOnlineStoreIntegrations(kind),
            variables,
            fragments,
          )
        : emptyConnection(field);
      continue;
    }

    if (root === 'webPixel' || root === 'serverPixel') {
      const kind = INTEGRATION_ROOT_KIND[root];
      const args = getFieldArguments(field, variables);
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      const record = kind
        ? id
          ? runtime.store.getEffectiveOnlineStoreIntegrationById(kind, id)
          : (runtime.store.listEffectiveOnlineStoreIntegrations(kind)[0] ?? null)
        : null;
      data[key] = record ? projectIntegrationRecord(record, field, variables, fragments) : null;
    }
  }

  return { data };
}

function collectConnectionRecords(kind: OnlineStoreContentKind, payload: unknown): OnlineStoreContentRecord[] {
  if (!isPlainObject(payload) || !Array.isArray(payload['edges'])) {
    return [];
  }

  return payload['edges'].flatMap((edge): OnlineStoreContentRecord[] => {
    if (!isPlainObject(edge) || !isPlainObject(edge['node'])) {
      return [];
    }
    const id = edge['node']['id'];
    if (typeof id !== 'string') {
      return [];
    }
    const cursor = typeof edge['cursor'] === 'string' ? edge['cursor'] : null;
    return [
      {
        id,
        kind,
        cursor,
        parentId:
          kind === 'article' && isPlainObject(edge['node']['blog'])
            ? String(edge['node']['blog']['id'] ?? '')
            : undefined,
        createdAt: typeof edge['node']['createdAt'] === 'string' ? edge['node']['createdAt'] : undefined,
        updatedAt: typeof edge['node']['updatedAt'] === 'string' ? edge['node']['updatedAt'] : undefined,
        data: structuredClone(edge['node']) as OnlineStoreContentRecord['data'],
      },
    ];
  });
}

function collectIntegrationConnectionRecords(
  kind: OnlineStoreIntegrationKind,
  payload: unknown,
): OnlineStoreIntegrationRecord[] {
  if (!isPlainObject(payload) || !Array.isArray(payload['edges'])) {
    return [];
  }

  return payload['edges'].flatMap((edge): OnlineStoreIntegrationRecord[] => {
    if (!isPlainObject(edge) || !isPlainObject(edge['node'])) {
      return [];
    }
    const id = edge['node']['id'];
    if (typeof id !== 'string') {
      return [];
    }
    const cursor = typeof edge['cursor'] === 'string' ? edge['cursor'] : null;
    return [
      {
        id,
        kind,
        cursor,
        createdAt: typeof edge['node']['createdAt'] === 'string' ? edge['node']['createdAt'] : undefined,
        updatedAt: typeof edge['node']['updatedAt'] === 'string' ? edge['node']['updatedAt'] : undefined,
        data: structuredClone(edge['node']) as OnlineStoreIntegrationRecord['data'],
      },
    ];
  });
}

export function hydrateOnlineStoreFromUpstreamResponse(
  runtime: ProxyRuntimeContext,
  document: string,
  upstreamPayload: unknown,
): void {
  if (!isPlainObject(upstreamPayload) || !isPlainObject(upstreamPayload['data'])) {
    return;
  }

  const records: OnlineStoreContentRecord[] = [];
  const integrations: OnlineStoreIntegrationRecord[] = [];
  for (const field of getRootFields(document)) {
    const root = field.name.value;
    const payload = upstreamPayload['data'][getFieldResponseKey(field)];
    if (root === 'articles' || root === 'blogs' || root === 'pages' || root === 'comments') {
      records.push(...collectConnectionRecords(root.slice(0, -1) as OnlineStoreContentKind, payload));
    } else if (root === 'themes' || root === 'scriptTags' || root === 'mobilePlatformApplications') {
      const kind = INTEGRATION_ROOT_KIND[root];
      if (kind) {
        integrations.push(...collectIntegrationConnectionRecords(kind, payload));
      }
    } else if (
      (root === 'article' || root === 'blog' || root === 'page' || root === 'comment') &&
      isPlainObject(payload)
    ) {
      const id = payload['id'];
      if (typeof id === 'string') {
        records.push({
          id,
          kind: root,
          createdAt: typeof payload['createdAt'] === 'string' ? payload['createdAt'] : undefined,
          updatedAt: typeof payload['updatedAt'] === 'string' ? payload['updatedAt'] : undefined,
          data: structuredClone(payload) as OnlineStoreContentRecord['data'],
        });
      }
    } else if (
      (root === 'theme' || root === 'scriptTag' || root === 'webPixel' || root === 'serverPixel') &&
      isPlainObject(payload)
    ) {
      const id = payload['id'];
      const kind = INTEGRATION_ROOT_KIND[root];
      if (kind && typeof id === 'string') {
        integrations.push({
          id,
          kind,
          createdAt: typeof payload['createdAt'] === 'string' ? payload['createdAt'] : undefined,
          updatedAt: typeof payload['updatedAt'] === 'string' ? payload['updatedAt'] : undefined,
          data: structuredClone(payload) as OnlineStoreIntegrationRecord['data'],
        });
      }
    }
  }

  if (records.length > 0) {
    runtime.store.upsertBaseOnlineStoreContent(records);
  }
  if (integrations.length > 0) {
    runtime.store.upsertBaseOnlineStoreIntegrations(integrations);
  }
}

export function isOnlineStoreContentQueryRoot(root: string | null | undefined): boolean {
  return (
    root === 'article' ||
    root === 'blog' ||
    root === 'page' ||
    root === 'comment' ||
    root === 'articleTags' ||
    root === 'blogsCount' ||
    root === 'pagesCount' ||
    CONNECTION_ROOTS.has(root ?? '') ||
    root === 'theme' ||
    root === 'scriptTag' ||
    root === 'webPixel' ||
    root === 'serverPixel' ||
    root === 'mobilePlatformApplication' ||
    INTEGRATION_CONNECTION_ROOTS.has(root ?? '')
  );
}
