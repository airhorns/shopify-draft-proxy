import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { store } from '../state/store.js';
import type { ShopDomainRecord } from '../state/types.js';
import { serializeConnection } from './graphql-helpers.js';

interface GraphQLResponseError {
  message: string;
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
      if (!selection.typeCondition?.name.value || selection.typeCondition.name.value === 'Job') {
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
      if (!selection.typeCondition?.name.value || selection.typeCondition.name.value === 'Domain') {
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

function serializeDomainRoot(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> | null {
  const args = getFieldArguments(field, variables);
  const id = typeof args['id'] === 'string' && args['id'].length > 0 ? args['id'] : null;
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

function staffMemberAccessDeniedError(path: string): GraphQLResponseError {
  if (path === 'staffMember') {
    return {
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
  }

  return {
    message: 'Access denied for staffMembers field.',
    path: [path],
    extensions: {
      code: 'ACCESS_DENIED',
      documentation: 'https://shopify.dev/api/usage/access-scopes',
    },
  };
}

function readIdListArgument(field: FieldNode, variables: Record<string, unknown>): string[] {
  const args = getFieldArguments(field, variables);
  const ids = args['ids'];
  return Array.isArray(ids) ? ids.filter((id): id is string => typeof id === 'string') : [];
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
        data[key] = null;
        break;
      case 'nodes':
        data[key] = readIdListArgument(field, variables).map(() => null);
        break;
      case 'job':
        data[key] = serializeJob(field, variables);
        break;
      case 'domain':
        data[key] = serializeDomainRoot(field, variables);
        break;
      case 'backupRegion':
        data[key] = serializePlainObject(
          CAPTURED_BACKUP_REGION,
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
        context.errors.push(staffMemberAccessDeniedError(field.name.value));
        break;
      default:
        data[key] = null;
    }
  }

  return context.errors.length > 0 ? { data, errors: context.errors } : { data };
}
