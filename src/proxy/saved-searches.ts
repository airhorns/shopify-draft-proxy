import type { ProxyRuntimeContext } from './runtime-context.js';
import { type FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import type { SavedSearchRecord } from '../state/types.js';
import {
  getDocumentFragments,
  getFieldResponseKey,
  isPlainObject,
  paginateConnectionItems,
  projectGraphqlValue,
  readPlainObjectArray,
  serializeConnection,
  type FragmentMap,
} from './graphql-helpers.js';
import { DRAFT_ORDER_SAVED_SEARCHES } from './orders.js';

type SavedSearchMutationResult = {
  response: Record<string, unknown>;
  stagedResourceIds: string[];
};

type UserError = {
  field: string[] | null;
  message: string;
};

const SAVED_SEARCH_ROOT_RESOURCE_TYPES: Record<string, string> = {
  automaticDiscountSavedSearches: 'PRICE_RULE',
  codeDiscountSavedSearches: 'PRICE_RULE',
  collectionSavedSearches: 'COLLECTION',
  customerSavedSearches: 'CUSTOMER',
  discountRedeemCodeSavedSearches: 'DISCOUNT_REDEEM_CODE',
  draftOrderSavedSearches: 'DRAFT_ORDER',
  fileSavedSearches: 'FILE',
  orderSavedSearches: 'ORDER',
  productSavedSearches: 'PRODUCT',
};

const SUPPORTED_SAVED_SEARCH_RESOURCE_TYPES = new Set(Object.values(SAVED_SEARCH_ROOT_RESOURCE_TYPES));

const ORDER_SAVED_SEARCHES: Array<
  Pick<SavedSearchRecord, 'id' | 'legacyResourceId' | 'name' | 'query' | 'resourceType'>
> = [
  {
    id: 'gid://shopify/SavedSearch/3634391515442',
    legacyResourceId: '3634391515442',
    name: 'Unfulfilled',
    query: 'status:open fulfillment_status:unshipped,partial',
    resourceType: 'ORDER',
  },
  {
    id: 'gid://shopify/SavedSearch/3634391548210',
    legacyResourceId: '3634391548210',
    name: 'Unpaid',
    query: 'status:open financial_status:unpaid',
    resourceType: 'ORDER',
  },
  {
    id: 'gid://shopify/SavedSearch/3634391580978',
    legacyResourceId: '3634391580978',
    name: 'Open',
    query: 'status:open',
    resourceType: 'ORDER',
  },
  {
    id: 'gid://shopify/SavedSearch/3634391613746',
    legacyResourceId: '3634391613746',
    name: 'Archived',
    query: 'status:closed',
    resourceType: 'ORDER',
  },
];

function readInput(args: Record<string, unknown>): Record<string, unknown> | null {
  return isPlainObject(args['input']) ? args['input'] : null;
}

function readOptionalString(input: Record<string, unknown>, field: string): string | undefined {
  if (!Object.prototype.hasOwnProperty.call(input, field)) {
    return undefined;
  }

  return typeof input[field] === 'string' ? input[field] : undefined;
}

function userError(field: string[] | null, message: string): UserError {
  return { field, message };
}

function readLegacyResourceId(id: string): string {
  const [gidWithoutQuery] = id.split('?');
  return gidWithoutQuery?.split('/').at(-1) ?? id;
}

function escapeSavedSearchTermForStoredQuery(term: string): string {
  return term.replace(/-/gu, '\\-');
}

function parseSavedSearchQuery(rawQuery: string): Pick<SavedSearchRecord, 'filters' | 'query' | 'searchTerms'> {
  const filters: SavedSearchRecord['filters'] = [];
  const searchTerms: string[] = [];

  for (const token of rawQuery.trim().split(/\s+/u).filter(Boolean)) {
    const separatorIndex = token.indexOf(':');
    if (separatorIndex > 0 && separatorIndex < token.length - 1) {
      filters.push({
        key: token.slice(0, separatorIndex),
        value: token.slice(separatorIndex + 1),
      });
    } else {
      searchTerms.push(token);
    }
  }

  const searchTermsText = searchTerms.join(' ');
  const storedSearchTermsText = searchTerms.map(escapeSavedSearchTermForStoredQuery).join(' ');
  return {
    filters,
    searchTerms: searchTermsText,
    query: [
      ...(storedSearchTermsText ? [storedSearchTermsText] : []),
      ...filters.map((filter) => `${filter.key}:${filter.value}`),
    ].join(' '),
  };
}

function makeDefaultSavedSearch(
  savedSearch: Pick<SavedSearchRecord, 'id' | 'legacyResourceId' | 'name' | 'query' | 'resourceType'>,
): SavedSearchRecord {
  return {
    ...savedSearch,
    ...parseSavedSearchQuery(savedSearch.query),
    cursor: null,
  };
}

function defaultSavedSearchesForResourceType(resourceType: string): SavedSearchRecord[] {
  if (resourceType === 'DRAFT_ORDER') {
    return DRAFT_ORDER_SAVED_SEARCHES.map(makeDefaultSavedSearch);
  }

  if (resourceType === 'ORDER') {
    return ORDER_SAVED_SEARCHES.map(makeDefaultSavedSearch);
  }

  return [];
}

function makeSavedSearch(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existing: SavedSearchRecord | null = null,
): SavedSearchRecord {
  const id = existing?.id ?? runtime.syntheticIdentity.makeProxySyntheticGid('SavedSearch');
  const queryInput = readOptionalString(input, 'query') ?? existing?.query ?? '';
  const queryParts = parseSavedSearchQuery(queryInput);

  return {
    id,
    cursor: existing?.cursor ?? null,
    legacyResourceId: existing?.legacyResourceId ?? readLegacyResourceId(id),
    name: readOptionalString(input, 'name') ?? existing?.name ?? '',
    resourceType: existing?.resourceType ?? readOptionalString(input, 'resourceType') ?? '',
    ...queryParts,
  };
}

function validateSavedSearchInput(
  input: Record<string, unknown> | null,
  options: { requireResourceType: boolean },
): UserError[] {
  const errors: UserError[] = [];
  if (!input) {
    return [userError(['input'], 'Input is required')];
  }

  const name = readOptionalString(input, 'name');
  if (Object.prototype.hasOwnProperty.call(input, 'name')) {
    if (!name || name.trim().length === 0) {
      errors.push(userError(['input', 'name'], "Name can't be blank"));
    } else if (name.length > 40) {
      errors.push(userError(['input', 'name'], 'Name is too long (maximum is 40 characters)'));
    }
  }

  const query = readOptionalString(input, 'query');
  if (Object.prototype.hasOwnProperty.call(input, 'query') && (!query || query.trim().length === 0)) {
    errors.push(userError(['input', 'query'], "Query can't be blank"));
  }

  const resourceType = readOptionalString(input, 'resourceType');
  if (options.requireResourceType) {
    if (!resourceType) {
      errors.push(userError(['input', 'resourceType'], "Resource type can't be blank"));
    } else if (resourceType === 'CUSTOMER') {
      errors.push(userError(null, 'Customer saved searches have been deprecated. Use Segmentation API instead.'));
    } else if (!SUPPORTED_SAVED_SEARCH_RESOURCE_TYPES.has(resourceType)) {
      errors.push(
        userError(
          ['input', 'resourceType'],
          resourceType === 'URL_REDIRECT'
            ? 'URL redirect saved searches require online-store navigation conformance before local support'
            : 'Resource type is not supported by the local saved search model',
        ),
      );
    }
  }

  return errors;
}

function recordData(record: SavedSearchRecord): Record<string, unknown> {
  return {
    id: record.id,
    legacyResourceId: record.legacyResourceId,
    name: record.name,
    query: record.query,
    resourceType: record.resourceType,
    searchTerms: record.searchTerms,
    filters: record.filters.map((filter) => ({ ...filter })),
  };
}

function mutationRecordData(record: SavedSearchRecord, input: Record<string, unknown> | null): Record<string, unknown> {
  const data = recordData(record);
  const query = input ? readOptionalString(input, 'query') : undefined;
  if (query !== undefined) {
    data['query'] = query;
  }

  return data;
}

function projectSavedSearch(
  data: Record<string, unknown>,
  field: FieldNode,
  fragments: FragmentMap,
): Record<string, unknown> {
  if (!field.selectionSet) {
    return { ...data };
  }

  return projectGraphqlValue(data, field.selectionSet.selections, fragments) as Record<string, unknown>;
}

function projectPayload(payload: Record<string, unknown>, field: FieldNode, fragments: FragmentMap): unknown {
  const savedSearchPayload = isPlainObject(payload['savedSearch']) ? payload['savedSearch'] : null;
  return field.selectionSet
    ? projectGraphqlValue(payload, field.selectionSet.selections, fragments, {
        projectFieldValue: ({ field: selectedField, fieldName, fragments: selectedFragments }) =>
          fieldName === 'savedSearch' && savedSearchPayload
            ? { handled: true, value: projectSavedSearch(savedSearchPayload, selectedField, selectedFragments) }
            : { handled: false },
      })
    : payload;
}

export function serializeSavedSearchNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  field: FieldNode,
  fragments: FragmentMap,
): Record<string, unknown> | null {
  const record = runtime.store.getEffectiveSavedSearchById(id);
  return record ? projectSavedSearch({ __typename: 'SavedSearch', ...recordData(record) }, field, fragments) : null;
}

function sanitizedUpdateInput(input: Record<string, unknown>, errors: UserError[]): Record<string, unknown> {
  const sanitized = { ...input };
  for (const error of errors) {
    const invalidField = error.field?.at(-1);
    if (invalidField === 'name' || invalidField === 'query') {
      delete sanitized[invalidField];
    }
  }

  return sanitized;
}

function handleCreate(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { key: string; payload: unknown; stagedResourceIds: string[] } {
  const input = readInput(getFieldArguments(field, variables));
  const errors = validateSavedSearchInput(input, { requireResourceType: true });
  const record =
    input && errors.length === 0 ? runtime.store.upsertStagedSavedSearch(makeSavedSearch(runtime, input)) : null;

  return {
    key: getFieldResponseKey(field),
    payload: projectPayload(
      { savedSearch: record ? mutationRecordData(record, input) : null, userErrors: errors },
      field,
      fragments,
    ),
    stagedResourceIds: record ? [record.id] : [],
  };
}

function handleUpdate(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { key: string; payload: unknown; stagedResourceIds: string[] } {
  const input = readInput(getFieldArguments(field, variables));
  const id = input && typeof input['id'] === 'string' ? input['id'] : null;
  const existing = id ? runtime.store.getEffectiveSavedSearchById(id) : null;
  const errors = existing
    ? validateSavedSearchInput(input, { requireResourceType: false })
    : [userError(['input', 'id'], 'Saved Search does not exist')];
  const sanitizedInput = input && existing ? sanitizedUpdateInput(input, errors) : null;
  const record =
    sanitizedInput && existing
      ? runtime.store.upsertStagedSavedSearch(makeSavedSearch(runtime, sanitizedInput, existing))
      : null;
  const payloadRecord = record ?? existing;

  return {
    key: getFieldResponseKey(field),
    payload: projectPayload(
      {
        savedSearch: payloadRecord ? mutationRecordData(payloadRecord, record ? sanitizedInput : null) : null,
        userErrors: errors,
      },
      field,
      fragments,
    ),
    stagedResourceIds: record ? [record.id] : [],
  };
}

function handleDelete(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): { key: string; payload: unknown; stagedResourceIds: string[] } {
  const input = readInput(getFieldArguments(field, variables));
  const id = input && typeof input['id'] === 'string' ? input['id'] : null;
  const existing = id ? runtime.store.getEffectiveSavedSearchById(id) : null;
  const errors = existing ? [] : [userError(['input', 'id'], 'Saved Search does not exist')];

  if (id && existing) {
    runtime.store.deleteStagedSavedSearch(id);
  }

  return {
    key: getFieldResponseKey(field),
    payload: projectPayload(
      { deletedSavedSearchId: errors.length === 0 ? id : null, userErrors: errors },
      field,
      fragments,
    ),
    stagedResourceIds: [],
  };
}

export function handleSavedSearchMutation(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): SavedSearchMutationResult | null {
  const fragments = getDocumentFragments(document);
  const data: Record<string, unknown> = {};
  const stagedResourceIds: string[] = [];
  let handled = false;

  for (const field of getRootFields(document)) {
    const root = field.name.value;
    const result =
      root === 'savedSearchCreate'
        ? handleCreate(runtime, field, variables, fragments)
        : root === 'savedSearchUpdate'
          ? handleUpdate(runtime, field, variables, fragments)
          : root === 'savedSearchDelete'
            ? handleDelete(runtime, field, variables, fragments)
            : null;
    if (!result) {
      continue;
    }

    handled = true;
    data[result.key] = result.payload;
    stagedResourceIds.push(...result.stagedResourceIds);
  }

  return handled ? { response: { data }, stagedResourceIds } : null;
}

function matchesQuery(record: SavedSearchRecord, query: unknown): boolean {
  if (typeof query !== 'string' || query.trim().length === 0) {
    return true;
  }

  const normalized = query.trim().toLowerCase();
  return [record.id, record.name, record.query, record.searchTerms, record.resourceType]
    .map((value) => value.toLowerCase())
    .some((value) => value.includes(normalized));
}

function listSavedSearches(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): SavedSearchRecord[] {
  const resourceType = SAVED_SEARCH_ROOT_RESOURCE_TYPES[field.name.value] ?? '';
  const args = getFieldArguments(field, variables);
  const localRecords = runtime.store.listEffectiveSavedSearches();
  const records = [
    ...defaultSavedSearchesForResourceType(resourceType).filter(
      (defaultRecord) => !localRecords.some((record) => record.id === defaultRecord.id),
    ),
    ...localRecords,
  ]
    .filter((record) => record.resourceType === resourceType)
    .filter((record) => matchesQuery(record, args['query']));

  return args['reverse'] === true ? records.reverse() : records;
}

function serializeSavedSearchConnection(
  field: FieldNode,
  records: SavedSearchRecord[],
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const window = paginateConnectionItems(records, field, variables, (record) => record.id);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (record) => record.id,
    serializeNode: (record, nodeField) => projectSavedSearch(record, nodeField, fragments),
  });
}

export function handleSavedSearchQuery(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const fragments = getDocumentFragments(document);
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    if (!(field.name.value in SAVED_SEARCH_ROOT_RESOURCE_TYPES)) {
      continue;
    }

    data[getFieldResponseKey(field)] = serializeSavedSearchConnection(
      field,
      listSavedSearches(runtime, field, variables),
      variables,
      fragments,
    );
  }

  return { data };
}

function readSavedSearchNode(raw: Record<string, unknown>, cursor: string | null): SavedSearchRecord | null {
  const id = raw['id'];
  const name = raw['name'];
  const query = raw['query'];
  const resourceType = raw['resourceType'];
  if (
    typeof id !== 'string' ||
    typeof name !== 'string' ||
    typeof query !== 'string' ||
    typeof resourceType !== 'string'
  ) {
    return null;
  }

  const queryParts = parseSavedSearchQuery(query);
  const rawFilters = readPlainObjectArray(raw['filters']);
  return {
    id,
    cursor,
    legacyResourceId: typeof raw['legacyResourceId'] === 'string' ? raw['legacyResourceId'] : readLegacyResourceId(id),
    name,
    resourceType,
    query,
    searchTerms: typeof raw['searchTerms'] === 'string' ? raw['searchTerms'] : queryParts.searchTerms,
    filters:
      rawFilters.length > 0
        ? rawFilters.flatMap((filter) =>
            typeof filter['key'] === 'string' && typeof filter['value'] === 'string'
              ? [{ key: filter['key'], value: filter['value'] }]
              : [],
          )
        : queryParts.filters,
  };
}

export function hydrateSavedSearchesFromUpstreamResponse(
  runtime: ProxyRuntimeContext,
  document: string,
  upstreamPayload: unknown,
): void {
  if (!isPlainObject(upstreamPayload) || !isPlainObject(upstreamPayload['data'])) {
    return;
  }

  const records: SavedSearchRecord[] = [];
  for (const field of getRootFields(document)) {
    if (!(field.name.value in SAVED_SEARCH_ROOT_RESOURCE_TYPES)) {
      continue;
    }

    const payload = upstreamPayload['data'][getFieldResponseKey(field)];
    if (!isPlainObject(payload) || !Array.isArray(payload['edges'])) {
      continue;
    }

    for (const edge of payload['edges']) {
      if (!isPlainObject(edge) || !isPlainObject(edge['node'])) {
        continue;
      }

      const record = readSavedSearchNode(edge['node'], typeof edge['cursor'] === 'string' ? edge['cursor'] : null);
      if (record) {
        records.push(record);
      }
    }
  }

  if (records.length > 0) {
    runtime.store.upsertBaseSavedSearches(records);
  }
}

export function isSavedSearchQueryRoot(root: string | null | undefined): boolean {
  return typeof root === 'string' && root in SAVED_SEARCH_ROOT_RESOURCE_TYPES;
}
