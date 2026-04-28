import { createHash, createHmac } from 'node:crypto';
import { getLocation, Kind, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { BackupRegionRecord, ShopDomainRecord } from '../state/types.js';
import { serializeConnection } from './graphql-helpers.js';
import { serializeProductBulkSelection } from './products.js';

interface GraphQLResponseError {
  message: string;
  locations?: Array<{
    line: number;
    column: number;
  }>;
  path: Array<string | number>;
  extensions: {
    code: string;
    documentation?: string;
    requiredAccess?: string;
  };
}

interface SerializationContext {
  errors: GraphQLResponseError[];
}

export const ADMIN_PLATFORM_QUERY_ROOTS = new Set([
  'backupRegion',
  'domain',
  'job',
  'node',
  'nodes',
  'publicApiVersions',
  'staffMember',
  'staffMembers',
  'taxonomy',
]);

export const FLOW_UTILITY_MUTATION_ROOTS = new Set(['flowGenerateSignature', 'flowTriggerReceive']);
export const ADMIN_PLATFORM_MUTATION_ROOTS = new Set([
  'backupRegionUpdate',
  'flowGenerateSignature',
  'flowTriggerReceive',
]);

const PUBLIC_API_VERSIONS = [
  { handle: '2025-07', displayName: '2025-07', supported: true },
  { handle: '2025-10', displayName: '2025-10', supported: true },
  { handle: '2026-01', displayName: '2026-01', supported: true },
  { handle: '2026-04', displayName: '2026-04 (Latest)', supported: true },
  { handle: '2026-07', displayName: '2026-07 (Release candidate)', supported: false },
  { handle: 'unstable', displayName: 'unstable', supported: false },
] as const;

const CAPTURED_BACKUP_REGION = {
  __typename: 'MarketRegionCountry',
  id: 'gid://shopify/MarketRegionCountry/4062110417202',
  name: 'Canada',
  code: 'CA',
} as const;

const BACKUP_REGION_BY_COUNTRY_CODE: Record<string, BackupRegionRecord> = {
  CA: CAPTURED_BACKUP_REGION,
};

const LOCAL_FLOW_TRIGGER_HANDLE_PREFIXES = ['local-', 'har-374-local'];
const FLOW_TRIGGER_PAYLOAD_LIMIT_BYTES = 50_000;
const FLOW_SIGNATURE_LOCAL_SECRET = 'shopify-draft-proxy-flow-signature-local-secret-v1';

function responseKey(field: FieldNode): string {
  return field.alias?.value ?? field.name.value;
}

function typeConditionApplies(typeCondition: string | undefined, typename: string): boolean {
  return (
    !typeCondition ||
    typeCondition === typename ||
    typeCondition === 'Node' ||
    (typeCondition === 'MarketRegion' && typename === 'MarketRegionCountry')
  );
}

function serializePlainObject(
  source: Record<string, unknown>,
  selections: readonly SelectionNode[],
  typename: string,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (typeConditionApplies(selection.typeCondition?.name.value, typename)) {
        Object.assign(result, serializePlainObject(source, selection.selectionSet.selections, typename));
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = typename;
        break;
      default:
        result[key] = source[selection.name.value] ?? null;
    }
  }

  return result;
}

function serializeUserErrors(
  selections: readonly SelectionNode[],
  errors: Array<{ field: string[]; message: string; code?: string | null }>,
  typename = 'UserError',
): Array<Record<string, unknown>> {
  return errors.map((error) => serializePlainObject(error, selections, typename));
}

function serializePublicApiVersion(
  selections: readonly SelectionNode[],
  version: (typeof PUBLIC_API_VERSIONS)[number],
) {
  return serializePlainObject(version, selections, 'ApiVersion');
}

function serializeQueryRoot(selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (!selection.typeCondition?.name.value || selection.typeCondition.name.value === 'QueryRoot') {
        Object.assign(result, serializeQueryRoot(selection.selectionSet.selections));
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    result[key] = selection.name.value === '__typename' ? 'QueryRoot' : null;
  }

  return result;
}

function serializeJob(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> | null {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' && args['id'].length > 0 ? args['id'] : null;
  if (!id) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (typeConditionApplies(selection.typeCondition?.name.value, 'Job')) {
        Object.assign(result, serializeJobSelection(id, selection.selectionSet.selections));
      }
      continue;
    }

    if (selection.kind === Kind.FIELD) {
      Object.assign(result, serializeJobSelection(id, [selection]));
    }
  }

  return result;
}

function serializeJobSelection(id: string, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Job';
        break;
      case 'id':
        result[key] = id;
        break;
      case 'done':
        result[key] = true;
        break;
      case 'query':
        result[key] = serializeQueryRoot(selection.selectionSet?.selections ?? []);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeDomain(domain: ShopDomainRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (typeConditionApplies(selection.typeCondition?.name.value, 'Domain')) {
        Object.assign(result, serializeDomain(domain, selection.selectionSet.selections));
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Domain';
        break;
      case 'id':
        result[key] = domain.id;
        break;
      case 'host':
        result[key] = domain.host;
        break;
      case 'url':
        result[key] = domain.url;
        break;
      case 'sslEnabled':
        result[key] = domain.sslEnabled;
        break;
      case 'localization':
      case 'marketWebPresence':
      default:
        result[key] = null;
    }
  }

  return result;
}

function readIdArgument(field: FieldNode, variables: Record<string, unknown>): string | null {
  const args = getFieldArguments(field, variables);
  const id = args['id'];
  return typeof id === 'string' && id.length > 0 ? id : null;
}

function serializeDomainRoot(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> | null {
  const id = readIdArgument(field, variables);
  const primaryDomain = store.getEffectiveShop()?.primaryDomain ?? null;
  if (!id || !primaryDomain || primaryDomain.id !== id) {
    return null;
  }

  return serializeDomain(primaryDomain, field.selectionSet?.selections ?? []);
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

function serializeTaxonomy(selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (!selection.typeCondition?.name.value || selection.typeCondition.name.value === 'Taxonomy') {
        Object.assign(result, serializeTaxonomy(selection.selectionSet.selections));
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Taxonomy';
        break;
      case 'categories':
        result[key] = serializeEmptyConnection(selection);
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeLocalNodeById(
  id: string,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  if (id.startsWith('gid://shopify/Product/')) {
    const product = store.getEffectiveProductById(id);
    return product ? serializeProductBulkSelection(product, selections, variables) : null;
  }

  if (id.startsWith('gid://shopify/Domain/')) {
    const primaryDomain = store.getEffectiveShop()?.primaryDomain ?? null;
    return primaryDomain?.id === id ? serializeDomain(primaryDomain, selections) : null;
  }

  return null;
}

function serializeNode(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> | null {
  const id = readIdArgument(field, variables);
  if (!id) {
    return null;
  }

  return serializeLocalNodeById(id, field.selectionSet?.selections ?? [], variables);
}

function fieldLocation(field: FieldNode): Array<{ line: number; column: number }> | undefined {
  if (!field.loc) {
    return undefined;
  }

  const location = getLocation(field.loc.source, field.loc.start);
  return [{ line: location.line, column: location.column }];
}

function staffMemberAccessDeniedError(field: FieldNode): GraphQLResponseError {
  const path = responseKey(field);
  const locations = fieldLocation(field);
  if (path === 'staffMember') {
    const error: GraphQLResponseError = {
      message:
        'Access denied for staffMember field. Required access: `read_users` access scope. Also: The app must be a finance embedded app or installed on a Shopify Plus or Advanced store. Contact Shopify Support to enable this scope for your app.',
      path: [path],
      extensions: {
        code: 'ACCESS_DENIED',
        documentation: 'https://shopify.dev/api/usage/access-scopes',
        requiredAccess:
          '`read_users` access scope. Also: The app must be a finance embedded app or installed on a Shopify Plus or Advanced store. Contact Shopify Support to enable this scope for your app.',
      },
    };
    if (locations) {
      error.locations = locations;
    }
    return error;
  }

  const error: GraphQLResponseError = {
    message: 'Access denied for staffMembers field.',
    path: [path],
    extensions: {
      code: 'ACCESS_DENIED',
      documentation: 'https://shopify.dev/api/usage/access-scopes',
    },
  };
  if (locations) {
    error.locations = locations;
  }
  return error;
}

function flowGenerateSignatureResourceNotFoundError(field: FieldNode, id: string): GraphQLResponseError {
  const error: GraphQLResponseError = {
    message: `Invalid id: ${id}`,
    path: [responseKey(field)],
    extensions: {
      code: 'RESOURCE_NOT_FOUND',
    },
  };
  const locations = fieldLocation(field);
  if (locations) {
    error.locations = locations;
  }
  return error;
}

function readIdListArgument(field: FieldNode, variables: Record<string, unknown>): string[] {
  const args = getFieldArguments(field, variables);
  const ids = args['ids'];
  return Array.isArray(ids) ? ids.filter((id): id is string => typeof id === 'string') : [];
}

function hashString(value: string): string {
  return createHash('sha256').update(value).digest('hex');
}

function stableJsonStringify(value: unknown): string {
  if (value === null || typeof value !== 'object') {
    return JSON.stringify(value);
  }

  if (Array.isArray(value)) {
    return `[${value.map((item) => stableJsonStringify(item)).join(',')}]`;
  }

  return `{${Object.entries(value)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, item]) => `${JSON.stringify(key)}:${stableJsonStringify(item)}`)
    .join(',')}}`;
}

function payloadBytes(payload: unknown): number {
  return Buffer.byteLength(stableJsonStringify(payload), 'utf8');
}

function isLocalFlowTriggerHandle(handle: string | null): boolean {
  return handle !== null && LOCAL_FLOW_TRIGGER_HANDLE_PREFIXES.some((prefix) => handle.startsWith(prefix));
}

function buildFlowSignature(flowTriggerId: string, payload: string): string {
  return createHmac('sha256', FLOW_SIGNATURE_LOCAL_SECRET)
    .update(flowTriggerId)
    .update('\0')
    .update(payload)
    .digest('hex');
}

function serializeFlowGenerateSignaturePayload(
  field: FieldNode,
  payload: string,
  signature: string,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (
        !selection.typeCondition?.name.value ||
        selection.typeCondition.name.value === 'FlowGenerateSignaturePayload'
      ) {
        Object.assign(
          result,
          serializeFlowGenerateSignaturePayload({ ...field, selectionSet: selection.selectionSet }, payload, signature),
        );
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case 'payload':
        result[key] = payload;
        break;
      case 'signature':
        result[key] = signature;
        break;
      case 'userErrors':
        result[key] = serializeUserErrors(selection.selectionSet?.selections ?? [], []);
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeFlowTriggerReceivePayload(
  field: FieldNode,
  errors: Array<{ field: string[]; message: string }>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    result[key] =
      selection.name.value === 'userErrors'
        ? serializeUserErrors(selection.selectionSet?.selections ?? [], errors)
        : null;
  }
  return result;
}

function serializeBackupRegionUpdatePayload(
  field: FieldNode,
  region: BackupRegionRecord | null,
  errors: Array<{ field: string[]; message: string; code: string }>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case 'backupRegion':
        result[key] = region
          ? serializePlainObject(region, selection.selectionSet?.selections ?? [], region.__typename)
          : null;
        break;
      case 'userErrors':
        result[key] = serializeUserErrors(selection.selectionSet?.selections ?? [], errors, 'MarketUserError');
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

export interface AdminPlatformMutationResult {
  response: Record<string, unknown>;
  stagedResourceIds?: string[];
  staged: boolean;
  notes?: string;
}

export function handleAdminPlatformQuery(
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const fields = getRootFields(document);
  const context: SerializationContext = { errors: [] };
  const data: Record<string, unknown> = {};

  for (const field of fields) {
    const key = responseKey(field);
    switch (field.name.value) {
      case 'publicApiVersions':
        data[key] = PUBLIC_API_VERSIONS.map((version) =>
          serializePublicApiVersion(field.selectionSet?.selections ?? [], version),
        );
        break;
      case 'node':
        data[key] = serializeNode(field, variables);
        break;
      case 'nodes':
        data[key] = readIdListArgument(field, variables).map((id) =>
          serializeLocalNodeById(id, field.selectionSet?.selections ?? [], variables),
        );
        break;
      case 'job':
        data[key] = serializeJob(field, variables);
        break;
      case 'domain':
        data[key] = serializeDomainRoot(field, variables);
        break;
      case 'backupRegion':
        data[key] = serializePlainObject(
          store.getEffectiveBackupRegion() ?? CAPTURED_BACKUP_REGION,
          field.selectionSet?.selections ?? [],
          'MarketRegionCountry',
        );
        break;
      case 'taxonomy':
        data[key] = serializeTaxonomy(field.selectionSet?.selections ?? []);
        break;
      case 'staffMember':
      case 'staffMembers':
        data[key] = null;
        context.errors.push(staffMemberAccessDeniedError(field));
        break;
      default:
        data[key] = null;
    }
  }

  return context.errors.length > 0 ? { data, errors: context.errors } : { data };
}

export function handleAdminPlatformMutation(
  document: string,
  variables: Record<string, unknown>,
): AdminPlatformMutationResult | null {
  const fields = getRootFields(document);
  const data: Record<string, unknown> = {};
  const errors: GraphQLResponseError[] = [];
  const stagedResourceIds: string[] = [];
  const notes: string[] = [];
  let staged = false;

  for (const field of fields) {
    const key = responseKey(field);
    const args = getFieldArguments(field, variables);
    switch (field.name.value) {
      case 'flowGenerateSignature': {
        const flowTriggerId = typeof args['id'] === 'string' ? args['id'] : '';
        const payload = typeof args['payload'] === 'string' ? args['payload'] : '';
        if (!/^gid:\/\/shopify\/FlowTrigger\/[1-9][0-9]*$/u.test(flowTriggerId)) {
          data[key] = null;
          errors.push(flowGenerateSignatureResourceNotFoundError(field, flowTriggerId));
          break;
        }

        const signature = buildFlowSignature(flowTriggerId, payload);
        const recordId = makeSyntheticGid('FlowGenerateSignature');
        store.stageAdminPlatformFlowSignature({
          id: recordId,
          flowTriggerId,
          payloadSha256: hashString(payload),
          signatureSha256: hashString(signature),
          createdAt: makeSyntheticTimestamp(),
        });
        stagedResourceIds.push(recordId);
        staged = true;
        notes.push(
          'Generated a deterministic proxy-local Flow signature without exposing or storing a Shopify secret.',
        );
        data[key] = serializeFlowGenerateSignaturePayload(field, payload, signature);
        break;
      }
      case 'flowTriggerReceive': {
        const handle = typeof args['handle'] === 'string' && args['handle'].length > 0 ? args['handle'] : null;
        const payload = args['payload'] ?? null;
        const size = payloadBytes(payload);
        const userErrors: Array<{ field: string[]; message: string }> = [];
        if (size > FLOW_TRIGGER_PAYLOAD_LIMIT_BYTES) {
          userErrors.push({
            field: ['body'],
            message: `Errors validating schema:\n  Properties size exceeds the limit of ${FLOW_TRIGGER_PAYLOAD_LIMIT_BYTES} bytes.\n`,
          });
        } else if (!isLocalFlowTriggerHandle(handle)) {
          userErrors.push({
            field: ['body'],
            message: `Errors validating schema:\n  Invalid handle '${handle ?? ''}'.\n`,
          });
        }

        if (userErrors.length === 0 && handle) {
          const recordId = makeSyntheticGid('FlowTriggerReceive');
          store.stageAdminPlatformFlowTrigger({
            id: recordId,
            handle,
            payloadBytes: size,
            payloadSha256: hashString(stableJsonStringify(payload)),
            receivedAt: makeSyntheticTimestamp(),
          });
          stagedResourceIds.push(recordId);
          staged = true;
          notes.push('Recorded a local Flow trigger receipt without delivering any external Flow side effects.');
        }

        data[key] = serializeFlowTriggerReceivePayload(field, userErrors);
        break;
      }
      case 'backupRegionUpdate': {
        const rawRegion = args['region'];
        const countryCode =
          rawRegion && typeof rawRegion === 'object' && !Array.isArray(rawRegion)
            ? (rawRegion as Record<string, unknown>)['countryCode']
            : null;
        const region = typeof countryCode === 'string' ? (BACKUP_REGION_BY_COUNTRY_CODE[countryCode] ?? null) : null;
        if (!region) {
          data[key] = serializeBackupRegionUpdatePayload(field, null, [
            { field: ['region'], message: 'Region not found.', code: 'REGION_NOT_FOUND' },
          ]);
          break;
        }

        store.stageBackupRegion(region);
        staged = true;
        stagedResourceIds.push(region.id);
        notes.push('Staged the shop backup region locally; no market or regional setting was changed upstream.');
        data[key] = serializeBackupRegionUpdatePayload(field, region, []);
        break;
      }
      default:
        data[key] = null;
    }
  }

  const response = errors.length > 0 ? { data, errors } : { data };
  return {
    response,
    stagedResourceIds,
    staged,
    ...(notes.length > 0 ? { notes: notes.join(' ') } : {}),
  };
}
