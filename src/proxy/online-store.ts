import { type FieldNode } from 'graphql';

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
import { makeProxySyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { OnlineStoreContentKind, OnlineStoreContentRecord } from '../state/types.js';
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

type OnlineStoreMutationResult = {
  response: Record<string, unknown>;
  stagedResourceIds: string[];
};

type UserError = {
  field: string[];
  message: string;
};

const CONNECTION_ROOTS = new Set(['articles', 'blogs', 'pages', 'comments', 'articleAuthors']);

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

function readIdArgument(field: FieldNode, variables: Record<string, unknown>): string | null {
  const args = getFieldArguments(field, variables);
  return typeof args['id'] === 'string' ? args['id'] : null;
}

function recordString(record: OnlineStoreContentRecord, field: string): string | null {
  const value = record.data[field];
  return typeof value === 'string' ? value : null;
}

function recordBlog(record: OnlineStoreContentRecord): OnlineStoreContentRecord | null {
  const blogId =
    typeof record.parentId === 'string'
      ? record.parentId
      : typeof record.data['blogId'] === 'string'
        ? record.data['blogId']
        : null;
  return blogId ? store.getEffectiveOnlineStoreContentById('blog', blogId) : null;
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

function matchesOnlineStoreSearchTerm(record: OnlineStoreContentRecord, term: SearchQueryTerm): boolean {
  const field = term.field?.toLowerCase() ?? null;
  const id = String(record.data['id'] ?? record.id);
  const title = recordString(record, 'title');
  const handle = recordString(record, 'handle');
  const body = recordString(record, 'body') ?? recordString(record, 'bodyHtml');
  const status = recordString(record, 'status');
  const author = record.data['author'];
  const authorName = isPlainObject(author) && typeof author['name'] === 'string' ? author['name'] : null;
  const tags = readStringArray(record.data['tags']);
  const blog = record.kind === 'article' ? recordBlog(record) : null;

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

function applyOnlineStoreSearch(records: OnlineStoreContentRecord[], query: unknown): OnlineStoreContentRecord[] {
  return applySearchQuery(records, query, { recognizeNotKeyword: true }, matchesOnlineStoreSearchTerm);
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
  kind: OnlineStoreContentKind,
  field: FieldNode,
  variables: Record<string, unknown>,
): OnlineStoreContentRecord[] {
  const args = getFieldArguments(field, variables);
  return sortRecords(
    applyOnlineStoreSearch(
      store
        .listEffectiveOnlineStoreContent(kind)
        .filter((record) => kind !== 'article' || record.data['isPublished'] !== false),
      args['query'],
    ),
    field,
    variables,
  );
}

function recordsForNestedConnection(
  kind: OnlineStoreContentKind,
  parentId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): OnlineStoreContentRecord[] {
  return sortRecords(
    applyOnlineStoreSearch(
      store.listEffectiveOnlineStoreContent(kind).filter((record) => record.parentId === parentId),
      getFieldArguments(field, variables)['query'],
    ),
    field,
    variables,
  );
}

function projectRecord(
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
            selectedField,
            recordsForNestedConnection('article', record.id, selectedField, variables),
            variables,
            selectedFragments,
          ),
        };
      }

      if (fieldName === 'comments' && record.kind === 'article') {
        return {
          handled: true,
          value: serializeRecordConnection(
            selectedField,
            recordsForNestedConnection('comment', record.id, selectedField, variables),
            variables,
            selectedFragments,
          ),
        };
      }

      if (fieldName === 'articlesCount' && record.kind === 'blog') {
        return {
          handled: true,
          value: countPayload(recordsForNestedConnection('article', record.id, selectedField, variables).length),
        };
      }

      if (fieldName === 'commentsCount' && record.kind === 'article') {
        return {
          handled: true,
          value: countPayload(recordsForNestedConnection('comment', record.id, selectedField, variables).length),
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
        const blog = blogId ? store.getEffectiveOnlineStoreContentById('blog', blogId) : null;
        return { handled: true, value: blog ? projectRecord(blog, selectedField, variables, selectedFragments) : null };
      }

      if (fieldName === 'article' && record.kind === 'comment') {
        const articleId = typeof record.parentId === 'string' ? record.parentId : null;
        const article = articleId ? store.getEffectiveOnlineStoreContentById('article', articleId) : null;
        return {
          handled: true,
          value: article ? projectRecord(article, selectedField, variables, selectedFragments) : null,
        };
      }

      if (fieldName === 'events' || fieldName === 'metafields') {
        return { handled: true, value: emptyConnection(selectedField) };
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
    serializeNode: (record, nodeField) => projectRecord(record, nodeField, variables, fragments),
  });
}

function serializeAuthorConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const names = new Set<string>();
  for (const article of store.listEffectiveOnlineStoreContent('article')) {
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
            ? { handled: true, value: projectRecord(record, selectedField, variables, selectedFragments) }
            : { handled: false };
        },
      })
    : payload;
}

function makeBlog(
  input: Record<string, unknown>,
  existing: OnlineStoreContentRecord | null = null,
): OnlineStoreContentRecord {
  const now = makeSyntheticTimestamp();
  const title = readOptionalString(input, 'title') ?? String(existing?.data['title'] ?? '');
  const handle = readOptionalString(input, 'handle') ?? String(existing?.data['handle'] ?? slugify(title));
  const id = existing?.id ?? makeProxySyntheticGid('Blog');
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
  input: Record<string, unknown>,
  existing: OnlineStoreContentRecord | null = null,
): OnlineStoreContentRecord {
  const now = makeSyntheticTimestamp();
  const title = readOptionalString(input, 'title') ?? String(existing?.data['title'] ?? '');
  const body = readOptionalString(input, 'body') ?? String(existing?.data['body'] ?? '');
  const isPublished = readOptionalBoolean(input, 'isPublished') ?? Boolean(existing?.data['isPublished'] ?? false);
  const publishedAt =
    readOptionalString(input, 'publishDate') ?? (isPublished ? (existing?.data['publishedAt'] ?? now) : null);
  const id = existing?.id ?? makeProxySyntheticGid('Page');
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
  input: Record<string, unknown>,
  blogId: string,
  existing: OnlineStoreContentRecord | null = null,
): OnlineStoreContentRecord {
  const now = makeSyntheticTimestamp();
  const title = readOptionalString(input, 'title') ?? String(existing?.data['title'] ?? '');
  const body = readOptionalString(input, 'body') ?? String(existing?.data['body'] ?? '');
  const isPublished = readOptionalBoolean(input, 'isPublished') ?? Boolean(existing?.data['isPublished'] ?? false);
  const publishedAt =
    readOptionalString(input, 'publishDate') ?? (isPublished ? (existing?.data['publishedAt'] ?? now) : null);
  const authorInput = isPlainObject(input['author']) ? input['author'] : null;
  const existingAuthor = isPlainObject(existing?.data['author']) ? existing?.data['author'] : null;
  const authorName =
    readOptionalString(authorInput ?? {}, 'name') ?? String(existingAuthor?.['name'] ?? 'Shopify Admin');
  const id = existing?.id ?? makeProxySyntheticGid('Article');
  const createdAt = String(existing?.data['createdAt'] ?? now);
  const updatedAt = existing ? now : createdAt;
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
    },
  };
}

function handleCreate(
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
      record = makeBlog(input);
    } else if (root === 'pageCreate') {
      record = makePage(input);
    } else {
      let blogId = readOptionalString(input, 'blogId') ?? null;
      const blogInput = readInput(args, 'blog');
      if (!blogId && blogInput && typeof blogInput['title'] === 'string') {
        const blog = makeBlog(blogInput);
        store.upsertStagedOnlineStoreContent(blog);
        blogId = blog.id;
        stagedResourceIds.push(blog.id);
      }
      if (!blogId) {
        errors.push(userError(['article', 'blogId'], 'Blog must exist'));
      } else {
        record = makeArticle(input, blogId);
      }
    }
  }

  if (record) {
    store.upsertStagedOnlineStoreContent(record);
  }

  const payloadKey = inputKey;
  return {
    key: root,
    payload: projectPayload(
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
  const existing = id ? store.getEffectiveOnlineStoreContentById(kind, id) : null;
  const errors: UserError[] = [];

  if (!id || !existing) {
    errors.push(userError(['id'], `${inputKey[0]?.toUpperCase() ?? ''}${inputKey.slice(1)} does not exist`));
  }

  let record: OnlineStoreContentRecord | null = null;
  if (errors.length === 0 && input && existing) {
    if (kind === 'blog') {
      record = makeBlog(input, existing);
    } else if (kind === 'page') {
      record = makePage(input, existing);
    } else {
      const blogId = readOptionalString(input, 'blogId') ?? existing.parentId ?? String(existing.data['blogId'] ?? '');
      record = makeArticle(input, blogId, existing);
    }
    store.upsertStagedOnlineStoreContent(record);
  }

  return {
    key: root,
    payload: projectPayload(
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
  root: string,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { key: string; payload: unknown; stagedResourceIds: string[] } {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const kind = root.replace(/Delete$/u, '') as OnlineStoreContentKind;
  const existing = id ? store.getEffectiveOnlineStoreContentById(kind, id) : null;
  const errors = existing ? [] : [userError(['id'], `${kind[0]?.toUpperCase() ?? ''}${kind.slice(1)} does not exist`)];
  if (id && existing) {
    store.deleteStagedOnlineStoreContent(kind, id);
  }

  const deletedKey = `deleted${kind[0]?.toUpperCase() ?? ''}${kind.slice(1)}Id`;
  return {
    key: root,
    payload: projectPayload(
      { [deletedKey]: errors.length === 0 ? id : null, userErrors: errors },
      field,
      fragments,
      variables,
    ),
    stagedResourceIds: [],
  };
}

function handleCommentModeration(
  root: string,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { key: string; payload: unknown; stagedResourceIds: string[] } {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' ? args['id'] : null;
  const existing = id ? store.getEffectiveOnlineStoreContentById('comment', id) : null;
  const errors = existing ? [] : [userError(['id'], 'Comment does not exist')];
  const statusByRoot: Record<string, string> = {
    commentApprove: 'PUBLISHED',
    commentSpam: 'SPAM',
    commentNotSpam: 'PENDING',
  };
  const updatedAt = makeSyntheticTimestamp();
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
    store.upsertStagedOnlineStoreContent(comment);
  }

  return {
    key: root,
    payload: projectPayload(
      { comment: comment ? recordData(comment) : null, userErrors: errors },
      field,
      fragments,
      variables,
      comment ? { comment } : {},
    ),
    stagedResourceIds: comment ? [comment.id] : [],
  };
}

export function handleOnlineStoreMutation(
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
        ? handleCreate(root, field, variables, fragments)
        : root === 'blogUpdate' || root === 'pageUpdate' || root === 'articleUpdate'
          ? handleUpdate(root, field, variables, fragments)
          : root === 'blogDelete' || root === 'pageDelete' || root === 'articleDelete'
            ? handleDelete(root, field, variables, fragments)
            : root === 'commentDelete'
              ? handleDelete(root, field, variables, fragments)
              : root === 'commentApprove' || root === 'commentSpam' || root === 'commentNotSpam'
                ? handleCommentModeration(root, field, variables, fragments)
                : null;
    if (!result) {
      continue;
    }
    handled = true;
    data[getFieldResponseKey(field)] = result.payload;
    stagedResourceIds.push(...result.stagedResourceIds);
  }

  return handled ? { response: { data }, stagedResourceIds } : null;
}

export function handleOnlineStoreQuery(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const fragments = getDocumentFragments(document);
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const root = field.name.value;
    const key = getFieldResponseKey(field);
    if (root === 'article' || root === 'blog' || root === 'page' || root === 'comment') {
      const id = readIdArgument(field, variables);
      const record = id ? store.getEffectiveOnlineStoreContentById(root, id) : null;
      data[key] = record ? projectRecord(record, field, variables, fragments) : null;
      continue;
    }

    if (root === 'articles' || root === 'blogs' || root === 'pages' || root === 'comments') {
      const kind = root.slice(0, -1) as OnlineStoreContentKind;
      data[key] = serializeRecordConnection(field, listRecords(kind, field, variables), variables, fragments);
      continue;
    }

    if (root === 'articleAuthors') {
      data[key] = serializeAuthorConnection(field, variables, fragments);
      continue;
    }

    if (root === 'articleTags') {
      const tags = new Set<string>();
      for (const article of store.listEffectiveOnlineStoreContent('article')) {
        for (const tag of readStringArray(article.data['tags'])) {
          tags.add(tag);
        }
      }
      data[key] = [...tags].sort().slice(0, Number(getFieldArguments(field, variables)['limit'] ?? tags.size));
      continue;
    }

    if (root === 'blogsCount' || root === 'pagesCount') {
      const kind = root === 'blogsCount' ? 'blog' : 'page';
      const count = listRecords(kind, field, variables).length;
      data[key] = field.selectionSet
        ? projectGraphqlValue(countPayload(count), field.selectionSet.selections, fragments)
        : countPayload(count);
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

export function hydrateOnlineStoreFromUpstreamResponse(document: string, upstreamPayload: unknown): void {
  if (!isPlainObject(upstreamPayload) || !isPlainObject(upstreamPayload['data'])) {
    return;
  }

  const records: OnlineStoreContentRecord[] = [];
  for (const field of getRootFields(document)) {
    const root = field.name.value;
    const payload = upstreamPayload['data'][getFieldResponseKey(field)];
    if (root === 'articles' || root === 'blogs' || root === 'pages' || root === 'comments') {
      records.push(...collectConnectionRecords(root.slice(0, -1) as OnlineStoreContentKind, payload));
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
    }
  }

  if (records.length > 0) {
    store.upsertBaseOnlineStoreContent(records);
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
    CONNECTION_ROOTS.has(root ?? '')
  );
}
