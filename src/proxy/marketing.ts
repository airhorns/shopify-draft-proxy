import type { FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import {
  matchesSearchQueryDate,
  matchesSearchQueryNumber,
  matchesSearchQueryText,
  normalizeSearchQueryValue,
  parseSearchQuery,
  type SearchQueryNode,
  type SearchQueryTerm,
} from '../search-query-parser.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { MarketingEngagementRecord, MarketingRecord } from '../state/types.js';
import {
  buildSyntheticCursor,
  getDocumentFragments,
  getFieldResponseKey,
  isPlainObject,
  paginateConnectionItems,
  projectGraphqlValue,
  readGraphqlDataResponsePayload,
  serializeConnection,
  type FragmentMap,
} from './graphql-helpers.js';

type MarketingKind = 'activity' | 'event';

type MarketingConnectionItem = {
  node: Record<string, unknown>;
  paginationCursor: string;
  outputCursor: string;
};

const ACTIVITY_ID_PREFIX = 'gid://shopify/MarketingActivity/';
const EVENT_ID_PREFIX = 'gid://shopify/MarketingEvent/';

export const MARKETING_MUTATION_ROOTS = new Set([
  'marketingActivityCreate',
  'marketingActivityUpdate',
  'marketingActivityCreateExternal',
  'marketingActivityUpdateExternal',
  'marketingActivityUpsertExternal',
  'marketingActivityDeleteExternal',
  'marketingActivitiesDeleteAllExternal',
  'marketingEngagementCreate',
  'marketingEngagementsDelete',
]);

type MarketingMutationResult = {
  response: { data: Record<string, unknown> };
  stagedResourceIds: string[];
  shouldLog: boolean;
  notes: string;
};

type MarketingUserError = {
  field: string[] | null;
  message: string;
  code: string | null;
};

type MarketingEngagementIdentifier =
  | { kind: 'activityId'; value: string; activity: MarketingRecord }
  | { kind: 'remoteId'; value: string; activity: MarketingRecord }
  | { kind: 'channelHandle'; value: string; activity: null };

function collectConnectionCandidates(value: unknown): Array<{ data: unknown; cursor?: string | null }> {
  if (!isPlainObject(value) || !Array.isArray(value['edges'])) {
    return [];
  }

  return value['edges'].flatMap((edge): Array<{ data: unknown; cursor?: string | null }> => {
    if (!isPlainObject(edge)) {
      return [];
    }

    const node = edge['node'];
    if (!isPlainObject(node)) {
      return [];
    }

    const cursor = typeof edge['cursor'] === 'string' && edge['cursor'].length > 0 ? edge['cursor'] : null;
    return [{ data: node, cursor }];
  });
}

function collectMarketingNodes(
  value: unknown,
  result: {
    activities: Array<{ data: unknown; cursor?: string | null }>;
    events: Array<{ data: unknown; cursor?: string | null }>;
  } = {
    activities: [],
    events: [],
  },
): {
  activities: Array<{ data: unknown; cursor?: string | null }>;
  events: Array<{ data: unknown; cursor?: string | null }>;
} {
  if (Array.isArray(value)) {
    for (const item of value) {
      collectMarketingNodes(item, result);
    }
    return result;
  }

  if (!isPlainObject(value)) {
    return result;
  }

  const id = value['id'];
  if (typeof id === 'string') {
    if (id.startsWith(ACTIVITY_ID_PREFIX)) {
      result.activities.push({ data: value });
    } else if (id.startsWith(EVENT_ID_PREFIX)) {
      result.events.push({ data: value });
    }
  }

  for (const child of Object.values(value)) {
    collectMarketingNodes(child, result);
  }

  return result;
}

export function hydrateMarketingFromUpstreamResponse(
  document: string,
  _variables: Record<string, unknown>,
  upstreamPayload: unknown,
): void {
  if (!isPlainObject(upstreamPayload) || !isPlainObject(upstreamPayload['data'])) {
    return;
  }

  for (const field of getRootFields(document)) {
    const rootField = field.name.value;
    const payload = readGraphqlDataResponsePayload(upstreamPayload, getFieldResponseKey(field));

    const collected = collectMarketingNodes(payload);
    if (rootField === 'marketingActivities') {
      collected.activities.unshift(...collectConnectionCandidates(payload));
    }
    if (rootField === 'marketingEvents') {
      collected.events.unshift(...collectConnectionCandidates(payload));
    }

    if (collected.activities.length > 0) {
      store.upsertBaseMarketingActivities(collected.activities);
    }
    if (collected.events.length > 0) {
      store.upsertBaseMarketingEvents(collected.events);
    }
  }
}

function readString(source: Record<string, unknown>, field: string): string | null {
  const value = source[field];
  return typeof value === 'string' ? value : null;
}

function readObject(source: Record<string, unknown>, field: string): Record<string, unknown> | null {
  const value = source[field];
  return isPlainObject(value) ? value : null;
}

function readInput(raw: unknown): Record<string, unknown> {
  return isPlainObject(raw) ? raw : {};
}

function toMarketingData(value: Record<string, unknown>): MarketingRecord['data'] {
  return structuredClone(value) as MarketingRecord['data'];
}

function idNumber(id: string): number | null {
  const value = id.split('/').at(-1);
  if (!value) {
    return null;
  }

  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? parsed : null;
}

function matchesIdTerm(id: string, term: SearchQueryTerm): boolean {
  const expected = normalizeSearchQueryValue(term.value);
  const numericId = idNumber(id);
  if (term.comparator && numericId !== null) {
    return matchesSearchQueryNumber(numericId, term);
  }

  return id.toLowerCase().includes(expected) || String(numericId ?? '').includes(expected);
}

function appName(source: Record<string, unknown>): string | null {
  const app = source['app'];
  return isPlainObject(app) ? (readString(app, 'name') ?? readString(app, 'title')) : null;
}

function marketingRemoteId(source: Record<string, unknown>): string | null {
  const remoteId = readString(source, 'remoteId');
  if (remoteId) {
    return remoteId;
  }

  const event = readObject(source, 'marketingEvent');
  return event ? readString(event, 'remoteId') : null;
}

function readUtm(source: Record<string, unknown>): Record<string, unknown> | null {
  const utm = readObject(source, 'utm') ?? readObject(source, 'utmParameters');
  const candidate = utm ?? source;

  const campaign = readString(candidate, 'campaign');
  const sourceValue = readString(candidate, 'source');
  const medium = readString(candidate, 'medium');
  return campaign && sourceValue && medium ? { campaign, source: sourceValue, medium } : null;
}

function sameUtm(left: Record<string, unknown> | null, right: Record<string, unknown> | null): boolean {
  if (left === null && right === null) {
    return true;
  }

  if (left === null || right === null) {
    return false;
  }

  return (
    left['campaign'] === right['campaign'] && left['source'] === right['source'] && left['medium'] === right['medium']
  );
}

function findMarketingActivityByUtm(utm: Record<string, unknown> | null): MarketingRecord | null {
  if (!utm) {
    return null;
  }

  return (
    store
      .listEffectiveMarketingActivities()
      .find((activity) => sameUtm(readObject(activity.data, 'utmParameters'), utm)) ?? null
  );
}

function statusLabel(status: string | null): string {
  switch (status) {
    case 'ACTIVE':
      return 'Sending';
    case 'DELETED':
      return 'Deleted';
    case 'INACTIVE':
      return 'Sent';
    case 'PAUSED':
      return 'Paused';
    case 'PENDING':
      return 'Pending';
    case 'SCHEDULED':
      return 'Scheduled';
    case 'DRAFT':
      return 'Draft';
    case 'FAILED':
      return 'Failed';
    case 'DISCONNECTED':
      return 'Disconnected';
    case 'DELETED_EXTERNALLY':
      return 'Deleted externally';
    case 'UNDEFINED':
    default:
      return 'Undefined';
  }
}

function sourceAndMedium(marketingChannelType: string | null, tactic: string | null): string {
  if (marketingChannelType === 'EMAIL' && tactic === 'NEWSLETTER') {
    return 'Email newsletter';
  }

  const channel = marketingChannelType ? marketingChannelType.toLowerCase() : 'external';
  const tacticLabel = tactic ? tactic.toLowerCase().replaceAll('_', ' ') : 'marketing';
  return `${channel[0]?.toUpperCase() ?? 'E'}${channel.slice(1)} ${tacticLabel}`;
}

function matchesActivityTerm(source: Record<string, unknown>, term: SearchQueryTerm): boolean {
  const field = term.field ?? 'default';
  switch (field) {
    case 'default':
      return (
        matchesSearchQueryText(readString(source, 'title'), term) ||
        matchesSearchQueryText(readString(source, 'sourceAndMedium'), term) ||
        matchesSearchQueryText(appName(source), term)
      );
    case 'app_name':
      return matchesSearchQueryText(appName(source), term);
    case 'created_at':
      return matchesSearchQueryDate(readString(source, 'createdAt'), term);
    case 'id':
      return matchesIdTerm(String(source['id'] ?? ''), term);
    case 'scheduled_to_end_at':
      return matchesSearchQueryDate(readString(source, 'scheduledToEndAt'), term);
    case 'scheduled_to_start_at':
      return matchesSearchQueryDate(readString(source, 'scheduledToStartAt'), term);
    case 'tactic':
      return normalizeSearchQueryValue(readString(source, 'tactic') ?? '') === normalizeSearchQueryValue(term.value);
    case 'title':
      return matchesSearchQueryText(readString(source, 'title'), term);
    case 'updated_at':
      return matchesSearchQueryDate(readString(source, 'updatedAt'), term);
    default:
      return false;
  }
}

function matchesEventTerm(source: Record<string, unknown>, term: SearchQueryTerm): boolean {
  const field = term.field ?? 'default';
  switch (field) {
    case 'default':
      return (
        matchesSearchQueryText(readString(source, 'description'), term) ||
        matchesSearchQueryText(readString(source, 'sourceAndMedium'), term) ||
        matchesSearchQueryText(readString(source, 'remoteId'), term)
      );
    case 'description':
      return matchesSearchQueryText(readString(source, 'description'), term);
    case 'id':
      return matchesIdTerm(String(source['id'] ?? ''), term);
    case 'started_at':
      return matchesSearchQueryDate(readString(source, 'startedAt'), term);
    case 'type':
      return normalizeSearchQueryValue(readString(source, 'type') ?? '') === normalizeSearchQueryValue(term.value);
    default:
      return false;
  }
}

function matchesSearchNode(
  node: SearchQueryNode | null,
  source: Record<string, unknown>,
  kind: MarketingKind,
): boolean {
  if (node === null) {
    return true;
  }

  switch (node.type) {
    case 'term': {
      const termMatch =
        kind === 'activity' ? matchesActivityTerm(source, node.term) : matchesEventTerm(source, node.term);
      return node.term.negated ? !termMatch : termMatch;
    }
    case 'and':
      return node.children.every((child) => matchesSearchNode(child, source, kind));
    case 'or':
      return node.children.some((child) => matchesSearchNode(child, source, kind));
    case 'not':
      return !matchesSearchNode(node.child, source, kind);
  }
}

function compareNullableString(left: string | null, right: string | null): number {
  return (left ?? '').localeCompare(right ?? '');
}

function sortRecords(records: MarketingRecord[], sortKey: unknown, kind: MarketingKind): MarketingRecord[] {
  const normalizedSortKey = typeof sortKey === 'string' ? sortKey : kind === 'activity' ? 'CREATED_AT' : 'ID';
  const sorted = [...records];

  sorted.sort((left, right) => {
    const leftData = left.data;
    const rightData = right.data;
    switch (normalizedSortKey) {
      case 'CREATED_AT':
        return compareNullableString(readString(leftData, 'createdAt'), readString(rightData, 'createdAt'));
      case 'STARTED_AT':
        return compareNullableString(readString(leftData, 'startedAt'), readString(rightData, 'startedAt'));
      case 'TITLE':
        return compareNullableString(readString(leftData, 'title'), readString(rightData, 'title'));
      case 'ID':
      default:
        return left.id.localeCompare(right.id);
    }
  });

  return sorted;
}

function filterRecords(
  records: MarketingRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
  kind: MarketingKind,
): MarketingRecord[] {
  const args = getFieldArguments(field, variables);
  let filtered = records;

  if (kind === 'activity') {
    const activityIds = Array.isArray(args['marketingActivityIds']) ? args['marketingActivityIds'] : [];
    if (activityIds.length > 0) {
      const ids = new Set(activityIds.filter((id): id is string => typeof id === 'string'));
      filtered = filtered.filter((record) => ids.has(record.id));
    }

    const remoteIds = Array.isArray(args['remoteIds']) ? args['remoteIds'] : [];
    if (remoteIds.length > 0) {
      const ids = new Set(remoteIds.filter((id): id is string => typeof id === 'string'));
      filtered = filtered.filter((record) => {
        const remoteId = marketingRemoteId(record.data);
        return remoteId !== null && ids.has(remoteId);
      });
    }
  }

  const query = typeof args['query'] === 'string' ? args['query'] : null;
  if (query) {
    const search = parseSearchQuery(query);
    filtered = filtered.filter((record) => matchesSearchNode(search, record.data, kind));
  }

  filtered = sortRecords(filtered, args['sortKey'], kind);
  return args['reverse'] === true ? filtered.reverse() : filtered;
}

function connectionItems(records: MarketingRecord[]): MarketingConnectionItem[] {
  return records.map((record) => {
    const id = record.id;
    const capturedCursor = typeof record.cursor === 'string' && record.cursor.length > 0 ? record.cursor : null;
    return {
      node: structuredClone(record.data),
      paginationCursor: capturedCursor ?? id,
      outputCursor: capturedCursor ?? buildSyntheticCursor(id),
    };
  });
}

function buildConnection(
  records: MarketingRecord[],
  field: FieldNode,
  variables: Record<string, unknown>,
  fragments: FragmentMap,
): Record<string, unknown> {
  const items = connectionItems(records);
  const window = paginateConnectionItems(items, field, variables, (item) => item.paginationCursor);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (item) => item.outputCursor,
    serializeNode: (item, selection) =>
      projectGraphqlValue(item.node, selection.selectionSet?.selections ?? [], fragments),
    pageInfoOptions: {
      prefixCursors: false,
    },
  });
}

function marketingValidationPayload(rootField: string, userErrors: MarketingUserError[]): Record<string, unknown> {
  switch (rootField) {
    case 'marketingEngagementCreate':
      return { marketingEngagement: null, userErrors };
    case 'marketingEngagementsDelete':
      return { result: null, userErrors };
    case 'marketingActivityDeleteExternal':
      return { deletedMarketingActivityId: null, userErrors };
    case 'marketingActivitiesDeleteAllExternal':
      return { job: null, userErrors };
    default:
      return { marketingActivity: null, userErrors };
  }
}

function nonHierarchicalUtmError(): MarketingUserError {
  return {
    field: ['input'],
    message: 'Non-hierarchical marketing activities must have UTM parameters or a URL parameter value.',
    code: 'NON_HIERARCHIAL_REQUIRES_UTM_URL_PARAMETER',
  };
}

function marketingActivityMissingError(): MarketingUserError {
  return {
    field: null,
    message: 'Marketing activity does not exist.',
    code: 'MARKETING_ACTIVITY_DOES_NOT_EXIST',
  };
}

function immutableUtmError(): MarketingUserError {
  return {
    field: ['input'],
    message: 'UTM parameters cannot be modified.',
    code: 'IMMUTABLE_UTM_PARAMETERS',
  };
}

function duplicateExternalActivityError(): MarketingUserError {
  return {
    field: ['input'],
    message: 'Validation failed: Remote ID has already been taken, Utm campaign has already been taken',
    code: null,
  };
}

function missingMarketingExtensionError(): MarketingUserError {
  return {
    field: ['input', 'marketingActivityExtensionId'],
    message: 'Could not find the marketing extension',
    code: null,
  };
}

function engagementMissingIdentifierError(): MarketingUserError {
  return {
    field: null,
    message:
      'No identifier found. For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.',
    code: 'INVALID_MARKETING_ENGAGEMENT_ARGUMENT_MISSING',
  };
}

function engagementInvalidIdentifierError(): MarketingUserError {
  return {
    field: null,
    message:
      'For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.',
    code: 'INVALID_MARKETING_ENGAGEMENT_ARGUMENTS',
  };
}

function invalidChannelHandleError(): MarketingUserError {
  return {
    field: ['channelHandle'],
    message: 'The channel handle is not recognized. Please contact your partner manager for more information.',
    code: 'INVALID_CHANNEL_HANDLE',
  };
}

function invalidDeleteEngagementsArgumentsError(): MarketingUserError {
  return {
    field: null,
    message:
      'Either the channel_handle or delete_engagements_for_all_channels must be provided when deleting a marketing engagement.',
    code: 'INVALID_DELETE_ENGAGEMENTS_ARGUMENTS',
  };
}

function hasAttribution(input: Record<string, unknown>): boolean {
  return readUtm(input) !== null || typeof input['urlParameterValue'] === 'string';
}

function eventEndedAtForStatus(status: string | null, timestamp: string): string | null {
  return status === 'INACTIVE' || status === 'DELETED_EXTERNALLY' ? timestamp : null;
}

function buildMarketingRecordsFromCreateInput(input: Record<string, unknown>): {
  activity: MarketingRecord;
  event: MarketingRecord;
} {
  const activityId = makeSyntheticGid('MarketingActivity');
  const eventId = makeSyntheticGid('MarketingEvent');
  const timestamp = makeSyntheticTimestamp();
  const title = readString(input, 'title') ?? '';
  const remoteId = readString(input, 'remoteId');
  const status = readString(input, 'status') ?? 'UNDEFINED';
  const tactic = readString(input, 'tactic') ?? 'NEWSLETTER';
  const marketingChannelType = readString(input, 'marketingChannelType') ?? 'EMAIL';
  const sourceMedium = sourceAndMedium(marketingChannelType, tactic);
  const utm = readUtm(input);
  const remoteUrl = readString(input, 'remoteUrl');
  const remotePreviewImageUrl = readString(input, 'remotePreviewImageUrl');
  const scheduledEnd = readString(input, 'scheduledEnd');
  const startedAt = readString(input, 'start') ?? readString(input, 'scheduledStart') ?? timestamp;
  const endedAt = readString(input, 'end') ?? eventEndedAtForStatus(status, timestamp);

  const eventData: Record<string, unknown> = {
    __typename: 'MarketingEvent',
    id: eventId,
    legacyResourceId: idNumber(eventId) ?? 0,
    type: tactic,
    remoteId,
    startedAt,
    endedAt,
    scheduledToEndAt: scheduledEnd,
    manageUrl: remoteUrl,
    previewUrl: remotePreviewImageUrl,
    utmCampaign: readString(utm ?? {}, 'campaign'),
    utmMedium: readString(utm ?? {}, 'medium'),
    utmSource: readString(utm ?? {}, 'source'),
    description: title,
    marketingChannelType,
    sourceAndMedium: sourceMedium,
    channelHandle: readString(input, 'channelHandle'),
  };

  const activityData: Record<string, unknown> = {
    __typename: 'MarketingActivity',
    id: activityId,
    title,
    createdAt: timestamp,
    updatedAt: timestamp,
    status,
    statusLabel: statusLabel(status),
    tactic,
    marketingChannelType,
    sourceAndMedium: sourceMedium,
    isExternal: true,
    inMainWorkflowVersion: false,
    urlParameterValue: readString(input, 'urlParameterValue'),
    parentActivityId: readString(input, 'parentActivityId'),
    parentRemoteId: readString(input, 'parentRemoteId'),
    hierarchyLevel: readString(input, 'hierarchyLevel'),
    remoteId,
    utmParameters: utm,
    marketingEvent: eventData,
  };

  return {
    activity: { id: activityId, cursor: null, data: toMarketingData(activityData) },
    event: { id: eventId, cursor: null, data: toMarketingData(eventData) },
  };
}

function isKnownLocalMarketingActivityExtension(marketingActivityExtensionId: string | null): boolean {
  if (!marketingActivityExtensionId?.startsWith('gid://shopify/MarketingActivityExtension/')) {
    return false;
  }

  return !marketingActivityExtensionId.endsWith('/00000000-0000-0000-0000-000000000000');
}

function buildNativeMarketingActivityFromCreateInput(input: Record<string, unknown>): MarketingRecord {
  const activityId = makeSyntheticGid('MarketingActivity');
  const timestamp = makeSyntheticTimestamp();
  const status = readString(input, 'status') ?? 'UNDEFINED';
  const title = readString(input, 'marketingActivityTitle') ?? readString(input, 'title') ?? 'Marketing activity';
  const tactic = readString(input, 'tactic') ?? 'NEWSLETTER';
  const marketingChannelType = readString(input, 'marketingChannelType') ?? 'EMAIL';
  const sourceMedium = sourceAndMedium(marketingChannelType, tactic);

  const activityData: Record<string, unknown> = {
    __typename: 'MarketingActivity',
    id: activityId,
    title,
    createdAt: timestamp,
    updatedAt: timestamp,
    status,
    statusLabel: statusLabel(status),
    tactic,
    marketingChannelType,
    sourceAndMedium: sourceMedium,
    isExternal: false,
    inMainWorkflowVersion: true,
    urlParameterValue: readString(input, 'urlParameterValue'),
    parentActivityId: readString(input, 'parentActivityId'),
    parentRemoteId: readString(input, 'parentRemoteId'),
    hierarchyLevel: readString(input, 'hierarchyLevel'),
    marketingActivityExtensionId: readString(input, 'marketingActivityExtensionId'),
    context: readString(input, 'context'),
    formData: readString(input, 'formData'),
    utmParameters: readUtm(input),
    marketingEvent: null,
  };

  return { id: activityId, cursor: null, data: toMarketingData(activityData) };
}

function applyNativeMarketingActivityUpdate(record: MarketingRecord, input: Record<string, unknown>): MarketingRecord {
  const timestamp = makeSyntheticTimestamp();
  const existingActivity = structuredClone(record.data);
  const status = readString(input, 'status') ?? readString(existingActivity, 'status') ?? 'UNDEFINED';
  const tactic = readString(input, 'tactic') ?? readString(existingActivity, 'tactic') ?? 'NEWSLETTER';
  const marketingChannelType =
    readString(input, 'marketingChannelType') ?? readString(existingActivity, 'marketingChannelType') ?? 'EMAIL';
  const title =
    readString(input, 'marketingActivityTitle') ??
    readString(input, 'title') ??
    readString(existingActivity, 'title') ??
    'Marketing activity';
  const sourceMedium = sourceAndMedium(marketingChannelType, tactic);
  const nextUtm = readUtm(input) ?? readObject(existingActivity, 'utmParameters');

  return {
    id: record.id,
    cursor: record.cursor ?? null,
    data: toMarketingData({
      ...existingActivity,
      title,
      updatedAt: timestamp,
      status,
      statusLabel: statusLabel(status),
      tactic,
      marketingChannelType,
      sourceAndMedium: sourceMedium,
      urlParameterValue: readString(input, 'urlParameterValue') ?? readString(existingActivity, 'urlParameterValue'),
      context: readString(input, 'context') ?? readString(existingActivity, 'context'),
      formData: readString(input, 'formData') ?? readString(existingActivity, 'formData'),
      utmParameters: nextUtm,
      marketingEvent: readObject(existingActivity, 'marketingEvent'),
    }),
  };
}

function applyExternalActivityUpdate(
  record: MarketingRecord,
  input: Record<string, unknown>,
): {
  activity: MarketingRecord;
  event: MarketingRecord;
} {
  const timestamp = makeSyntheticTimestamp();
  const existingActivity = structuredClone(record.data);
  const existingEvent = readObject(existingActivity, 'marketingEvent') ?? {};
  const status = readString(input, 'status') ?? readString(existingActivity, 'status') ?? 'UNDEFINED';
  const tactic = readString(input, 'tactic') ?? readString(existingActivity, 'tactic') ?? 'NEWSLETTER';
  const marketingChannelType =
    readString(input, 'marketingChannelType') ?? readString(existingActivity, 'marketingChannelType') ?? 'EMAIL';
  const title = readString(input, 'title') ?? readString(existingActivity, 'title') ?? '';
  const sourceMedium = sourceAndMedium(marketingChannelType, tactic);
  const eventId = readString(existingEvent, 'id') ?? makeSyntheticGid('MarketingEvent');
  const remoteId = marketingRemoteId(existingActivity);
  const existingUtm = readObject(existingActivity, 'utmParameters');
  const endedAt =
    readString(input, 'end') ??
    (status === readString(existingActivity, 'status')
      ? readString(existingEvent, 'endedAt')
      : eventEndedAtForStatus(status, timestamp));

  const eventData: Record<string, unknown> = {
    ...existingEvent,
    __typename: 'MarketingEvent',
    id: eventId,
    legacyResourceId: idNumber(eventId) ?? readString(existingEvent, 'legacyResourceId') ?? 0,
    type: tactic,
    remoteId,
    startedAt:
      readString(input, 'start') ??
      readString(input, 'scheduledStart') ??
      readString(existingEvent, 'startedAt') ??
      timestamp,
    endedAt,
    scheduledToEndAt: readString(input, 'scheduledEnd') ?? readString(existingEvent, 'scheduledToEndAt'),
    manageUrl: readString(input, 'remoteUrl') ?? readString(existingEvent, 'manageUrl'),
    previewUrl: readString(input, 'remotePreviewImageUrl') ?? readString(existingEvent, 'previewUrl'),
    utmCampaign: readString(existingUtm ?? {}, 'campaign'),
    utmMedium: readString(existingUtm ?? {}, 'medium'),
    utmSource: readString(existingUtm ?? {}, 'source'),
    description: title,
    marketingChannelType,
    sourceAndMedium: sourceMedium,
  };

  const activityData: Record<string, unknown> = {
    ...existingActivity,
    title,
    updatedAt: timestamp,
    status,
    statusLabel: statusLabel(status),
    tactic,
    marketingChannelType,
    sourceAndMedium: sourceMedium,
    marketingEvent: eventData,
  };

  return {
    activity: { id: record.id, cursor: record.cursor ?? null, data: toMarketingData(activityData) },
    event: { id: eventId, cursor: null, data: toMarketingData(eventData) },
  };
}

function stageMarketingRecords(records: { activity: MarketingRecord; event: MarketingRecord }): void {
  store.stageMarketingEvent(records.event);
  store.stageMarketingActivity(records.activity);
}

function readMoneyInput(input: Record<string, unknown>, field: string): Record<string, unknown> | null {
  const money = readObject(input, field);
  if (!money) {
    return null;
  }

  const amount = money['amount'];
  const currencyCode = readString(money, 'currencyCode');
  if ((typeof amount !== 'string' && typeof amount !== 'number') || !currencyCode) {
    return null;
  }

  return {
    amount: String(amount),
    currencyCode,
  };
}

function readDecimalInput(input: Record<string, unknown>, field: string): string | null {
  const value = input[field];
  if (typeof value === 'string' || typeof value === 'number') {
    return String(value);
  }

  return null;
}

function engagementRecordId(identifier: MarketingEngagementIdentifier, occurredOn: string): string {
  const target =
    identifier.kind === 'channelHandle' ? `channel:${identifier.value}` : `activity:${identifier.activity.id}`;
  return `gid://shopify/MarketingEngagement/${encodeURIComponent(`${target}:${occurredOn}`)}`;
}

function resolveMarketingEngagementIdentifier(
  args: Record<string, unknown>,
): { ok: true; identifier: MarketingEngagementIdentifier } | { ok: false; error: MarketingUserError } {
  const marketingActivityId = typeof args['marketingActivityId'] === 'string' ? args['marketingActivityId'] : null;
  const remoteId = typeof args['remoteId'] === 'string' ? args['remoteId'] : null;
  const channelHandle = typeof args['channelHandle'] === 'string' ? args['channelHandle'] : null;
  const identifierCount = [marketingActivityId, remoteId, channelHandle].filter((value) => value !== null).length;

  if (identifierCount === 0) {
    return { ok: false, error: engagementMissingIdentifierError() };
  }

  if (identifierCount > 1) {
    return { ok: false, error: engagementInvalidIdentifierError() };
  }

  if (marketingActivityId) {
    const activity = store.getEffectiveMarketingActivityRecordById(marketingActivityId);
    return activity
      ? { ok: true, identifier: { kind: 'activityId', value: marketingActivityId, activity } }
      : { ok: false, error: marketingActivityMissingError() };
  }

  if (remoteId) {
    const activity = store.getEffectiveMarketingActivityByRemoteId(remoteId);
    return activity
      ? { ok: true, identifier: { kind: 'remoteId', value: remoteId, activity } }
      : { ok: false, error: marketingActivityMissingError() };
  }

  if (channelHandle && store.hasKnownMarketingChannelHandle(channelHandle)) {
    return { ok: true, identifier: { kind: 'channelHandle', value: channelHandle, activity: null } };
  }

  return { ok: false, error: invalidChannelHandleError() };
}

function buildMarketingEngagementRecord(
  identifier: MarketingEngagementIdentifier,
  input: Record<string, unknown>,
): MarketingEngagementRecord {
  const occurredOn = readString(input, 'occurredOn') ?? '';
  const data: Record<string, unknown> = {
    __typename: 'MarketingEngagement',
    occurredOn,
    utcOffset: readString(input, 'utcOffset') ?? '+00:00',
    isCumulative: input['isCumulative'] === true,
    channelHandle: identifier.kind === 'channelHandle' ? identifier.value : null,
    marketingActivity: identifier.activity ? structuredClone(identifier.activity.data) : null,
  };
  const integerFields = [
    'impressionsCount',
    'viewsCount',
    'clicksCount',
    'sharesCount',
    'favoritesCount',
    'commentsCount',
    'unsubscribesCount',
    'complaintsCount',
    'failsCount',
    'sendsCount',
    'uniqueViewsCount',
    'uniqueClicksCount',
    'sessionsCount',
  ];
  for (const field of integerFields) {
    const value = input[field];
    if (typeof value === 'number' && Number.isInteger(value)) {
      data[field] = value;
    }
  }

  for (const field of ['adSpend', 'sales']) {
    const money = readMoneyInput(input, field);
    if (money) {
      data[field] = money;
    }
  }

  for (const field of ['orders', 'primaryConversions', 'allConversions', 'firstTimeCustomers', 'returningCustomers']) {
    const decimal = readDecimalInput(input, field);
    if (decimal !== null) {
      data[field] = decimal;
    }
  }

  return {
    id: engagementRecordId(identifier, occurredOn),
    marketingActivityId: identifier.activity?.id ?? null,
    remoteId: identifier.kind === 'remoteId' ? identifier.value : marketingRemoteId(identifier.activity?.data ?? {}),
    channelHandle: identifier.kind === 'channelHandle' ? identifier.value : null,
    occurredOn,
    data: toMarketingData(data),
  };
}

function buildMutationRootPayload(
  rootField: string,
  args: Record<string, unknown>,
): {
  payload: Record<string, unknown>;
  stagedResourceIds: string[];
  shouldLog: boolean;
} {
  if (rootField === 'marketingActivityCreate') {
    const input = readInput(args['input']);
    if (!isKnownLocalMarketingActivityExtension(readString(input, 'marketingActivityExtensionId'))) {
      return {
        payload: { userErrors: [missingMarketingExtensionError()] },
        stagedResourceIds: [],
        shouldLog: false,
      };
    }

    const activity = buildNativeMarketingActivityFromCreateInput(input);
    store.stageMarketingActivity(activity);
    return {
      payload: { userErrors: [] },
      stagedResourceIds: [activity.id],
      shouldLog: true,
    };
  }

  if (rootField === 'marketingActivityUpdate') {
    const input = readInput(args['input']);
    const activityId = readString(input, 'id');
    const activity = activityId ? store.getEffectiveMarketingActivityRecordById(activityId) : null;

    if (!activity) {
      return {
        payload: { marketingActivity: null, redirectPath: null, userErrors: [marketingActivityMissingError()] },
        stagedResourceIds: [],
        shouldLog: false,
      };
    }

    const updated = applyNativeMarketingActivityUpdate(activity, input);
    store.stageMarketingActivity(updated);
    return {
      payload: { marketingActivity: updated.data, redirectPath: null, userErrors: [] },
      stagedResourceIds: [updated.id],
      shouldLog: true,
    };
  }

  if (rootField === 'marketingActivityCreateExternal') {
    const input = readInput(args['input']);
    if (!hasAttribution(input)) {
      return {
        payload: marketingValidationPayload(rootField, [nonHierarchicalUtmError()]),
        stagedResourceIds: [],
        shouldLog: false,
      };
    }

    const remoteId = readString(input, 'remoteId');
    if (remoteId && store.getEffectiveMarketingActivityByRemoteId(remoteId)) {
      return {
        payload: marketingValidationPayload(rootField, [duplicateExternalActivityError()]),
        stagedResourceIds: [],
        shouldLog: false,
      };
    }

    const records = buildMarketingRecordsFromCreateInput(input);
    stageMarketingRecords(records);
    return {
      payload: { marketingActivity: records.activity.data, userErrors: [] },
      stagedResourceIds: [records.activity.id, records.event.id],
      shouldLog: true,
    };
  }

  if (rootField === 'marketingActivityUpdateExternal') {
    const input = readInput(args['input']);
    const remoteId = typeof args['remoteId'] === 'string' ? args['remoteId'] : null;
    const activityId = typeof args['marketingActivityId'] === 'string' ? args['marketingActivityId'] : null;
    const selectorUtm = readUtm(readInput(args['utm']));
    const activity = remoteId
      ? store.getEffectiveMarketingActivityByRemoteId(remoteId)
      : activityId
        ? store.getEffectiveMarketingActivityRecordById(activityId)
        : findMarketingActivityByUtm(selectorUtm);

    if (!activity) {
      return {
        payload: marketingValidationPayload(rootField, [marketingActivityMissingError()]),
        stagedResourceIds: [],
        shouldLog: false,
      };
    }

    const requestedUtm = readInput(args['utm']);
    if (
      Object.keys(requestedUtm).length > 0 &&
      !sameUtm(readObject(activity.data, 'utmParameters'), readUtm(requestedUtm))
    ) {
      return {
        payload: marketingValidationPayload(rootField, [immutableUtmError()]),
        stagedResourceIds: [],
        shouldLog: false,
      };
    }

    const records = applyExternalActivityUpdate(activity, input);
    stageMarketingRecords(records);
    return {
      payload: { marketingActivity: records.activity.data, userErrors: [] },
      stagedResourceIds: [records.activity.id, records.event.id],
      shouldLog: true,
    };
  }

  if (rootField === 'marketingActivityUpsertExternal') {
    const input = readInput(args['input']);
    const remoteId = readString(input, 'remoteId');
    const existing = remoteId ? store.getEffectiveMarketingActivityByRemoteId(remoteId) : null;

    if (!existing) {
      if (!hasAttribution(input)) {
        return {
          payload: marketingValidationPayload(rootField, [nonHierarchicalUtmError()]),
          stagedResourceIds: [],
          shouldLog: false,
        };
      }

      const records = buildMarketingRecordsFromCreateInput(input);
      stageMarketingRecords(records);
      return {
        payload: { marketingActivity: records.activity.data, userErrors: [] },
        stagedResourceIds: [records.activity.id, records.event.id],
        shouldLog: true,
      };
    }

    if (!sameUtm(readObject(existing.data, 'utmParameters'), readUtm(input))) {
      return {
        payload: marketingValidationPayload(rootField, [immutableUtmError()]),
        stagedResourceIds: [],
        shouldLog: false,
      };
    }

    const records = applyExternalActivityUpdate(existing, input);
    stageMarketingRecords(records);
    return {
      payload: { marketingActivity: records.activity.data, userErrors: [] },
      stagedResourceIds: [records.activity.id, records.event.id],
      shouldLog: true,
    };
  }

  if (rootField === 'marketingActivityDeleteExternal') {
    const remoteId = typeof args['remoteId'] === 'string' ? args['remoteId'] : null;
    const activityId = typeof args['marketingActivityId'] === 'string' ? args['marketingActivityId'] : null;
    const activity = remoteId
      ? store.getEffectiveMarketingActivityByRemoteId(remoteId)
      : activityId
        ? store.getEffectiveMarketingActivityRecordById(activityId)
        : null;

    if (!activity) {
      return {
        payload: marketingValidationPayload(rootField, [marketingActivityMissingError()]),
        stagedResourceIds: [],
        shouldLog: false,
      };
    }

    store.stageDeleteMarketingActivity(activity.id);
    return {
      payload: { deletedMarketingActivityId: activity.id, userErrors: [] },
      stagedResourceIds: [activity.id],
      shouldLog: true,
    };
  }

  if (rootField === 'marketingActivitiesDeleteAllExternal') {
    const deletedIds = store.stageDeleteAllExternalMarketingActivities();
    const jobId = makeSyntheticGid('Job');
    return {
      payload: {
        job: {
          __typename: 'Job',
          id: jobId,
          done: false,
        },
        userErrors: [],
      },
      stagedResourceIds: [jobId, ...deletedIds],
      shouldLog: true,
    };
  }

  if (rootField === 'marketingEngagementCreate') {
    const input = readInput(args['marketingEngagement']);
    const resolved = resolveMarketingEngagementIdentifier(args);
    if (!resolved.ok) {
      return {
        payload: marketingValidationPayload(rootField, [resolved.error]),
        stagedResourceIds: [],
        shouldLog: false,
      };
    }

    const engagement = store.stageMarketingEngagement(buildMarketingEngagementRecord(resolved.identifier, input));
    return {
      payload: { marketingEngagement: engagement.data, userErrors: [] },
      stagedResourceIds: [engagement.id],
      shouldLog: true,
    };
  }

  if (rootField === 'marketingEngagementsDelete') {
    const channelHandle = typeof args['channelHandle'] === 'string' ? args['channelHandle'] : null;
    const deleteAll = args['deleteEngagementsForAllChannels'] === true;
    if ((channelHandle && deleteAll) || (!channelHandle && !deleteAll)) {
      return {
        payload: marketingValidationPayload(rootField, [invalidDeleteEngagementsArgumentsError()]),
        stagedResourceIds: [],
        shouldLog: false,
      };
    }

    if (channelHandle) {
      if (!store.hasKnownMarketingChannelHandle(channelHandle)) {
        return {
          payload: marketingValidationPayload(rootField, [invalidChannelHandleError()]),
          stagedResourceIds: [],
          shouldLog: false,
        };
      }

      const deletedIds = store.stageDeleteMarketingEngagementsByChannelHandle(channelHandle);
      return {
        payload: { result: 'Engagement data marked for deletion for 1 channel(s)', userErrors: [] },
        stagedResourceIds: deletedIds,
        shouldLog: true,
      };
    }

    const channelHandles = new Set(
      store
        .listEffectiveMarketingEngagements()
        .map((engagement) => engagement.channelHandle)
        .filter((value): value is string => typeof value === 'string' && value.length > 0),
    );
    const deletedIds = store.stageDeleteAllChannelMarketingEngagements();
    return {
      payload: { result: `Engagement data marked for deletion for ${channelHandles.size} channel(s)`, userErrors: [] },
      stagedResourceIds: deletedIds,
      shouldLog: true,
    };
  }

  return { payload: {}, stagedResourceIds: [], shouldLog: false };
}

export function handleMarketingMutation(
  document: string,
  variables: Record<string, unknown> = {},
): MarketingMutationResult | null {
  const data: Record<string, unknown> = {};
  const fragments = getDocumentFragments(document);
  const stagedResourceIds: string[] = [];
  let handled = false;
  let shouldLog = false;

  for (const field of getRootFields(document)) {
    const rootField = field.name.value;
    if (!MARKETING_MUTATION_ROOTS.has(rootField)) {
      continue;
    }

    handled = true;
    const args = getFieldArguments(field, variables);
    const {
      payload,
      stagedResourceIds: rootStagedResourceIds,
      shouldLog: rootShouldLog,
    } = buildMutationRootPayload(rootField, args);
    stagedResourceIds.push(...rootStagedResourceIds);
    shouldLog = shouldLog || rootShouldLog;
    data[getFieldResponseKey(field)] = field.selectionSet
      ? projectGraphqlValue(payload, field.selectionSet.selections, fragments)
      : payload;
  }

  return handled
    ? {
        response: { data },
        stagedResourceIds: [...new Set(stagedResourceIds)],
        shouldLog,
        notes: 'Staged locally in the in-memory marketing draft store.',
      }
    : null;
}

function rootPayloadForField(field: FieldNode, variables: Record<string, unknown>, fragments: FragmentMap): unknown {
  const args = getFieldArguments(field, variables);

  switch (field.name.value) {
    case 'marketingActivity': {
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      return id ? store.getEffectiveMarketingActivityById(id) : null;
    }
    case 'marketingActivities':
      return buildConnection(
        filterRecords(store.listEffectiveMarketingActivities(), field, variables, 'activity'),
        field,
        variables,
        fragments,
      );
    case 'marketingEvent': {
      const id = typeof args['id'] === 'string' ? args['id'] : null;
      return id ? store.getEffectiveMarketingEventById(id) : null;
    }
    case 'marketingEvents':
      return buildConnection(
        filterRecords(store.listEffectiveMarketingEvents(), field, variables, 'event'),
        field,
        variables,
        fragments,
      );
    default:
      return null;
  }
}

export function handleMarketingQuery(
  document: string,
  variables: Record<string, unknown> = {},
): {
  data: Record<string, unknown>;
} {
  const data: Record<string, unknown> = {};
  const fragments = getDocumentFragments(document);

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const rootPayload = rootPayloadForField(field, variables, fragments);
    data[key] = field.selectionSet
      ? projectGraphqlValue(rootPayload, field.selectionSet.selections, fragments)
      : rootPayload;
  }

  return { data };
}
