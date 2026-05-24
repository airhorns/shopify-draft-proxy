import { execFileSync } from 'node:child_process';
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
const registryCache = new Map<string, OperationRegistryEntry[]>();

export function loadOperationRegistryFromSource(repoRoot = defaultRepoRoot): OperationRegistryEntry[] {
  const cacheKey = resolve(repoRoot);
  const cached = registryCache.get(cacheKey);
  if (cached) {
    return cloneRegistryEntries(cached);
  }

  let output: string;
  try {
    output = execFileSync('cargo', ['run', '--quiet', '--bin', 'operation-registry-json'], {
      cwd: cacheKey,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  } catch (error) {
    const stderr = stderrFromExecError(error);
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Failed to export operation registry from Rust: ${stderr ?? message}`);
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(output) as unknown;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Rust operation registry export produced invalid JSON: ${message}`);
  }

  const registry = operationRegistrySchema.parse(parsed);
  registryCache.set(cacheKey, registry);
  return cloneRegistryEntries(registry);
}

const operationRegistry = loadOperationRegistryFromSource();

export function listOperationRegistryEntries(): OperationRegistryEntry[] {
  return cloneRegistryEntries(operationRegistry);
}

function cloneRegistryEntries(registry: OperationRegistryEntry[]): OperationRegistryEntry[] {
  return registry.map((entry) => ({
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

function stderrFromExecError(error: unknown): string | null {
  if (typeof error !== 'object' || error === null || !('stderr' in error)) {
    return null;
  }

  const stderr = (error as { stderr?: unknown }).stderr;
  if (Buffer.isBuffer(stderr)) {
    const text = stderr.toString('utf8').trim();
    return text.length > 0 ? text : null;
  }

  if (typeof stderr === 'string') {
    const text = stderr.trim();
    return text.length > 0 ? text : null;
  }

  return null;
}
