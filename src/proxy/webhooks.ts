import { Kind, type FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import {
  matchesSearchQueryText,
  normalizeSearchQueryValue,
  parseSearchQueryTerms,
  type SearchQueryTerm,
} from '../search-query-parser.js';
import { compareShopifyResourceIds } from '../shopify/resource-ids.js';
import { store } from '../state/store.js';
import type { WebhookSubscriptionRecord } from '../state/types.js';
import {
  defaultGraphqlTypeConditionApplies,
  getDocumentFragments,
  getFieldResponseKey,
  isPlainObject,
  paginateConnectionItems,
  projectGraphqlValue,
  readGraphqlDataResponsePayload,
  serializeConnection,
  type FragmentMap,
} from './graphql-helpers.js';

const webhookProjectionOptions = {
  shouldApplyTypeCondition: (source: Record<string, unknown>, typeCondition: string | undefined): boolean =>
    defaultGraphqlTypeConditionApplies(source, typeCondition) || typeCondition === 'WebhookSubscription',
};

function normalizeStringArray(raw: unknown): string[] {
  return Array.isArray(raw) ? raw.filter((value): value is string => typeof value === 'string') : [];
}

function normalizeEndpoint(raw: unknown): WebhookSubscriptionRecord['endpoint'] {
  if (!isPlainObject(raw)) {
    return null;
  }

  const typename = raw['__typename'];
  if (
    typename !== 'WebhookHttpEndpoint' &&
    typename !== 'WebhookEventBridgeEndpoint' &&
    typename !== 'WebhookPubSubEndpoint'
  ) {
    return null;
  }

  return {
    __typename: typename,
    ...(typeof raw['callbackUrl'] === 'string' || raw['callbackUrl'] === null
      ? { callbackUrl: raw['callbackUrl'] }
      : {}),
    ...(typeof raw['arn'] === 'string' || raw['arn'] === null ? { arn: raw['arn'] } : {}),
    ...(typeof raw['pubSubProject'] === 'string' || raw['pubSubProject'] === null
      ? { pubSubProject: raw['pubSubProject'] }
      : {}),
    ...(typeof raw['pubSubTopic'] === 'string' || raw['pubSubTopic'] === null
      ? { pubSubTopic: raw['pubSubTopic'] }
      : {}),
  };
}

function normalizeWebhookSubscription(raw: unknown): WebhookSubscriptionRecord | null {
  if (!isPlainObject(raw)) {
    return null;
  }

  const id = raw['id'];
  if (typeof id !== 'string' || !id.startsWith('gid://shopify/WebhookSubscription/')) {
    return null;
  }

  return {
    id,
    topic: typeof raw['topic'] === 'string' ? raw['topic'] : null,
    format: typeof raw['format'] === 'string' ? raw['format'] : null,
    includeFields: normalizeStringArray(raw['includeFields']),
    metafieldNamespaces: normalizeStringArray(raw['metafieldNamespaces']),
    filter: typeof raw['filter'] === 'string' || raw['filter'] === null ? raw['filter'] : null,
    createdAt: typeof raw['createdAt'] === 'string' || raw['createdAt'] === null ? raw['createdAt'] : null,
    updatedAt: typeof raw['updatedAt'] === 'string' || raw['updatedAt'] === null ? raw['updatedAt'] : null,
    endpoint: normalizeEndpoint(raw['endpoint']),
  };
}

function collectWebhookSubscriptions(
  value: unknown,
  records: WebhookSubscriptionRecord[] = [],
): WebhookSubscriptionRecord[] {
  if (Array.isArray(value)) {
    for (const item of value) {
      collectWebhookSubscriptions(item, records);
    }
    return records;
  }

  const webhookSubscription = normalizeWebhookSubscription(value);
  if (webhookSubscription) {
    records.push(webhookSubscription);
  }

  if (!isPlainObject(value)) {
    return records;
  }

  for (const child of Object.values(value)) {
    collectWebhookSubscriptions(child, records);
  }

  return records;
}

export function hydrateWebhookSubscriptionsFromUpstreamResponse(
  document: string,
  _variables: Record<string, unknown>,
  upstreamPayload: unknown,
): void {
  if (!isPlainObject(upstreamPayload) || !isPlainObject(upstreamPayload['data'])) {
    return;
  }

  const records: WebhookSubscriptionRecord[] = [];
  for (const field of getRootFields(document)) {
    collectWebhookSubscriptions(readGraphqlDataResponsePayload(upstreamPayload, getFieldResponseKey(field)), records);
  }

  if (records.length > 0) {
    store.upsertBaseWebhookSubscriptions(records);
  }
}

function webhookSubscriptionLegacyId(webhookSubscription: WebhookSubscriptionRecord): string {
  return webhookSubscription.id.split('/').at(-1) ?? webhookSubscription.id;
}

function matchesWebhookTerm(webhookSubscription: WebhookSubscriptionRecord, term: SearchQueryTerm): boolean {
  const field = term.field?.toLowerCase() ?? null;
  let matches = false;

  switch (field) {
    case null:
      matches =
        matchesSearchQueryText(webhookSubscription.id, term) ||
        matchesSearchQueryText(webhookSubscription.topic, term) ||
        matchesSearchQueryText(webhookSubscription.format, term);
      break;
    case 'id': {
      const expected = normalizeSearchQueryValue(term.value);
      matches =
        normalizeSearchQueryValue(webhookSubscription.id) === expected ||
        normalizeSearchQueryValue(webhookSubscriptionLegacyId(webhookSubscription)) === expected;
      break;
    }
    case 'topic':
      matches = matchesSearchQueryText(webhookSubscription.topic, term);
      break;
    case 'format':
      matches = matchesSearchQueryText(webhookSubscription.format, term);
      break;
    case 'created_at':
    case 'createdat':
      matches = matchesSearchQueryText(webhookSubscription.createdAt, term);
      break;
    case 'updated_at':
    case 'updatedat':
      matches = matchesSearchQueryText(webhookSubscription.updatedAt, term);
      break;
    default:
      matches = false;
      break;
  }

  return term.negated ? !matches : matches;
}

function filterWebhookSubscriptionsByQuery(
  webhookSubscriptions: WebhookSubscriptionRecord[],
  rawQuery: unknown,
): WebhookSubscriptionRecord[] {
  if (typeof rawQuery !== 'string' || rawQuery.trim().length === 0) {
    return webhookSubscriptions;
  }

  const terms = parseSearchQueryTerms(rawQuery.trim(), { ignoredKeywords: ['AND'] });
  if (terms.length === 0) {
    return webhookSubscriptions;
  }

  return webhookSubscriptions.filter((webhookSubscription) =>
    terms.every((term) => matchesWebhookTerm(webhookSubscription, term)),
  );
}

function sortWebhookSubscriptionsForConnection(
  webhookSubscriptions: WebhookSubscriptionRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
): WebhookSubscriptionRecord[] {
  const args = getFieldArguments(field, variables);
  const sortKey = typeof args['sortKey'] === 'string' ? args['sortKey'] : 'ID';
  const reverse = args['reverse'] === true;

  const sorted = [...webhookSubscriptions].sort((left, right) => {
    switch (sortKey) {
      case 'CREATED_AT':
        return (
          (left.createdAt ?? '').localeCompare(right.createdAt ?? '') || compareShopifyResourceIds(left.id, right.id)
        );
      case 'UPDATED_AT':
        return (
          (left.updatedAt ?? '').localeCompare(right.updatedAt ?? '') || compareShopifyResourceIds(left.id, right.id)
        );
      case 'TOPIC':
        return (left.topic ?? '').localeCompare(right.topic ?? '') || compareShopifyResourceIds(left.id, right.id);
      case 'ID':
      default:
        return compareShopifyResourceIds(left.id, right.id);
    }
  });

  return reverse ? sorted.reverse() : sorted;
}

function serializeWebhookSubscriptionNode(
  selection: FieldNode,
  webhookSubscription: WebhookSubscriptionRecord,
  fragments: FragmentMap,
): unknown {
  return selection.selectionSet
    ? projectGraphqlValue(webhookSubscription, selection.selectionSet.selections, fragments, webhookProjectionOptions)
    : webhookSubscription.id;
}

function serializeWebhookSubscriptionsConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const filteredWebhookSubscriptions = filterWebhookSubscriptionsByQuery(
    store.listEffectiveWebhookSubscriptions(),
    args['query'],
  );
  const sortedWebhookSubscriptions = sortWebhookSubscriptionsForConnection(
    filteredWebhookSubscriptions,
    field,
    variables,
  );
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    sortedWebhookSubscriptions,
    field,
    variables,
    (webhookSubscription) => webhookSubscription.id,
  );

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (webhookSubscription) => webhookSubscription.id,
    serializeNode: (webhookSubscription, selection) =>
      serializeWebhookSubscriptionNode(selection, webhookSubscription, fragments),
    selectedFieldOptions: { includeInlineFragments: true },
    pageInfoOptions: { includeInlineFragments: true },
  });
}

function serializeWebhookSubscriptionsCount(
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const filteredWebhookSubscriptions = filterWebhookSubscriptionsByQuery(
    store.listEffectiveWebhookSubscriptions(),
    args['query'],
  );
  const rawLimit = args['limit'];
  const limit =
    typeof rawLimit === 'number' && Number.isFinite(rawLimit) && rawLimit >= 0 ? Math.floor(rawLimit) : null;
  const count =
    limit === null ? filteredWebhookSubscriptions.length : Math.min(filteredWebhookSubscriptions.length, limit);
  const precision = limit !== null && filteredWebhookSubscriptions.length > limit ? 'AT_LEAST' : 'EXACT';
  const result: Record<string, unknown> = {};

  for (const selection of (field.selectionSet?.selections ?? []).filter(
    (candidate): candidate is FieldNode => candidate.kind === Kind.FIELD,
  )) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'count':
        result[key] = count;
        break;
      case 'precision':
        result[key] = precision;
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function rootPayloadForField(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);
  switch (field.name.value) {
    case 'webhookSubscription': {
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      const webhookSubscription = id ? store.getEffectiveWebhookSubscriptionById(id) : null;
      return webhookSubscription && field.selectionSet
        ? projectGraphqlValue(webhookSubscription, field.selectionSet.selections, fragments, webhookProjectionOptions)
        : webhookSubscription;
    }
    case 'webhookSubscriptions':
      return serializeWebhookSubscriptionsConnection(field, variables, fragments);
    case 'webhookSubscriptionsCount':
      return serializeWebhookSubscriptionsCount(field, variables);
    default:
      return null;
  }
}

export function handleWebhookSubscriptionQuery(
  document: string,
  variables: Record<string, unknown>,
): { data: Record<string, unknown> } {
  const fragments = getDocumentFragments(document);
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    data[getFieldResponseKey(field)] = rootPayloadForField(field, variables, fragments);
  }

  return { data };
}
