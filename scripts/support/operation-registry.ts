import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { operationRegistrySchema, type OperationRegistryEntry } from './json-schemas.js';

export type CapabilityDomain =
  | 'products'
  | 'admin-platform'
  | 'b2b'
  | 'apps'
  | 'media'
  | 'bulk-operations'
  | 'customers'
  | 'orders'
  | 'store-properties'
  | 'discounts'
  | 'events'
  | 'functions'
  | 'payments'
  | 'marketing'
  | 'online-store'
  | 'saved-searches'
  | 'privacy'
  | 'segments'
  | 'shipping-fulfillments'
  | 'gift-cards'
  | 'webhooks'
  | 'localization'
  | 'markets'
  | 'metafields'
  | 'metaobjects'
  | 'unknown';
export type CapabilityExecution = 'overlay-read' | 'stage-locally' | 'passthrough';
export type OperationType = 'query' | 'mutation';

const defaultRepoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const registrySourceRelativePath = 'src/shopify_draft_proxy/proxy/operation_registry_data.gleam';

const typeByVariant = {
  Query: 'query',
  Mutation: 'mutation',
} as const satisfies Record<string, OperationType>;

const domainByVariant = {
  Products: 'products',
  AdminPlatform: 'admin-platform',
  B2b: 'b2b',
  Apps: 'apps',
  Media: 'media',
  BulkOperations: 'bulk-operations',
  Customers: 'customers',
  Orders: 'orders',
  StoreProperties: 'store-properties',
  Discounts: 'discounts',
  Events: 'events',
  Functions: 'functions',
  Payments: 'payments',
  Marketing: 'marketing',
  OnlineStore: 'online-store',
  SavedSearches: 'saved-searches',
  Privacy: 'privacy',
  Segments: 'segments',
  ShippingFulfillments: 'shipping-fulfillments',
  GiftCards: 'gift-cards',
  Webhooks: 'webhooks',
  Localization: 'localization',
  Markets: 'markets',
  Metafields: 'metafields',
  Metaobjects: 'metaobjects',
  Unknown: 'unknown',
} as const satisfies Record<string, CapabilityDomain>;

const executionByVariant = {
  OverlayRead: 'overlay-read',
  StageLocally: 'stage-locally',
  Passthrough: 'passthrough',
} as const satisfies Record<string, CapabilityExecution>;

function decodeGleamString(raw: string): string {
  return JSON.parse(`"${raw}"`) as string;
}

function matchingClose(source: string, openIndex: number): number {
  let depth = 0;
  let inString = false;
  let escaped = false;

  for (let index = openIndex; index < source.length; index += 1) {
    const char = source[index];

    if (inString) {
      if (escaped) {
        escaped = false;
      } else if (char === '\\') {
        escaped = true;
      } else if (char === '"') {
        inString = false;
      }
      continue;
    }

    if (char === '"') {
      inString = true;
      continue;
    }

    if (char === '(') {
      depth += 1;
    } else if (char === ')') {
      depth -= 1;
      if (depth === 0) {
        return index;
      }
    }
  }

  throw new Error(`Unclosed Gleam call starting at offset ${openIndex}`);
}

function listRegistryEntryBlocks(source: string): string[] {
  const blocks: string[] = [];
  let searchIndex = 0;

  while (searchIndex < source.length) {
    const callIndex = source.indexOf('RegistryEntry(', searchIndex);
    if (callIndex === -1) {
      break;
    }

    const openIndex = callIndex + 'RegistryEntry'.length;
    const closeIndex = matchingClose(source, openIndex);
    blocks.push(source.slice(openIndex + 1, closeIndex));
    searchIndex = closeIndex + 1;
  }

  return blocks;
}

function fieldExpression(block: string, fieldName: string): string {
  const marker = `${fieldName}:`;
  const markerIndex = block.indexOf(marker);
  if (markerIndex === -1) {
    throw new Error(`RegistryEntry missing field ${fieldName}`);
  }

  let index = markerIndex + marker.length;
  while (/\s/u.test(block[index] ?? '')) {
    index += 1;
  }

  const start = index;
  let parenDepth = 0;
  let bracketDepth = 0;
  let inString = false;
  let escaped = false;

  for (; index < block.length; index += 1) {
    const char = block[index];

    if (inString) {
      if (escaped) {
        escaped = false;
      } else if (char === '\\') {
        escaped = true;
      } else if (char === '"') {
        inString = false;
      }
      continue;
    }

    if (char === '"') {
      inString = true;
      continue;
    }

    if (char === '(') {
      parenDepth += 1;
      continue;
    }

    if (char === ')') {
      parenDepth -= 1;
      continue;
    }

    if (char === '[') {
      bracketDepth += 1;
      continue;
    }

    if (char === ']') {
      bracketDepth -= 1;
      continue;
    }

    if (char === ',' && parenDepth === 0 && bracketDepth === 0) {
      return block.slice(start, index).trim();
    }
  }

  return block.slice(start).trim();
}

function stringField(block: string, fieldName: string): string {
  const expression = fieldExpression(block, fieldName);
  const match = /^"((?:\\.|[^"\\])*)"$/su.exec(expression);
  if (!match) {
    throw new Error(`RegistryEntry field ${fieldName} is not a string literal`);
  }
  return decodeGleamString(match[1] ?? '');
}

function variantField<T extends string>(block: string, fieldName: string, variants: Record<string, T>): T {
  const expression = fieldExpression(block, fieldName);
  const value = variants[expression];
  if (!value) {
    throw new Error(`RegistryEntry field ${fieldName} has unknown variant ${expression}`);
  }
  return value;
}

function boolField(block: string, fieldName: string): boolean {
  const expression = fieldExpression(block, fieldName);
  if (expression === 'True') {
    return true;
  }
  if (expression === 'False') {
    return false;
  }
  throw new Error(`RegistryEntry field ${fieldName} is not a Bool literal`);
}

function stringListField(block: string, fieldName: string): string[] {
  const expression = fieldExpression(block, fieldName);
  if (!expression.startsWith('[') || !expression.endsWith(']')) {
    throw new Error(`RegistryEntry field ${fieldName} is not a list literal`);
  }

  const values: string[] = [];
  const body = expression.slice(1, -1);
  const stringPattern = /"((?:\\.|[^"\\])*)"/gsu;
  let match: RegExpExecArray | null;
  while ((match = stringPattern.exec(body)) !== null) {
    values.push(decodeGleamString(match[1] ?? ''));
  }

  const residue = body.replaceAll(stringPattern, '').replaceAll(',', '').trim();
  if (residue.length > 0) {
    throw new Error(`RegistryEntry field ${fieldName} contains unsupported list syntax`);
  }

  return values;
}

function optionalStringField(block: string, fieldName: string): string | undefined {
  const expression = fieldExpression(block, fieldName);
  if (expression === 'None') {
    return undefined;
  }

  const match = /^Some\(\s*"((?:\\.|[^"\\])*)"\s*,?\s*\)$/su.exec(expression);
  if (!match) {
    throw new Error(`RegistryEntry field ${fieldName} is not None or Some(string)`);
  }
  return decodeGleamString(match[1] ?? '');
}

function parseGleamOperationRegistrySource(source: string): OperationRegistryEntry[] {
  const entries = listRegistryEntryBlocks(source).map((block) => {
    const supportNotes = optionalStringField(block, 'support_notes');
    return {
      name: stringField(block, 'name'),
      type: variantField(block, 'type_', typeByVariant),
      domain: variantField(block, 'domain', domainByVariant),
      execution: variantField(block, 'execution', executionByVariant),
      implemented: boolField(block, 'implemented'),
      matchNames: stringListField(block, 'match_names'),
      runtimeTests: stringListField(block, 'runtime_tests'),
      ...(supportNotes ? { supportNotes } : {}),
    };
  });

  return operationRegistrySchema.parse(entries);
}

export function loadOperationRegistryFromSource(repoRoot = defaultRepoRoot): OperationRegistryEntry[] {
  return parseGleamOperationRegistrySource(readFileSync(resolve(repoRoot, registrySourceRelativePath), 'utf8'));
}

const operationRegistry = loadOperationRegistryFromSource();

export function listOperationRegistryEntries(): OperationRegistryEntry[] {
  return operationRegistry.map((entry) => ({
    ...entry,
    matchNames: [...entry.matchNames],
    runtimeTests: [...entry.runtimeTests],
  }));
}

export function listImplementedOperationRegistryEntries(): OperationRegistryEntry[] {
  return operationRegistry.filter((entry) => entry.implemented);
}

export function findOperationRegistryEntry(
  type: OperationType,
  names: Array<string | null | undefined>,
): OperationRegistryEntry | null {
  const candidates = names.filter((name): name is string => typeof name === 'string' && name.length > 0);
  for (const candidate of candidates) {
    const entry = operationRegistry.find(
      (registryEntry) => registryEntry.type === type && registryEntry.matchNames.includes(candidate),
    );
    if (entry) {
      return {
        ...entry,
        matchNames: [...entry.matchNames],
        runtimeTests: [...entry.runtimeTests],
      };
    }
  }

  return null;
}
