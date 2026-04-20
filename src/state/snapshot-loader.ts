import { readFileSync } from 'node:fs';

import type { CustomerCatalogConnectionRecord, NormalizedStateSnapshotFile, StateSnapshot } from './types.js';

function normalizeStateSnapshot(value: StateSnapshot): StateSnapshot {
  const rawPublications = (value as unknown as Record<string, unknown>)['publications'];
  return {
    ...value,
    publications: isObject(rawPublications) ? (rawPublications as StateSnapshot['publications']) : {},
  };
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isStateSnapshot(value: unknown): value is StateSnapshot {
  if (!isObject(value)) {
    return false;
  }

  return [
    'products',
    'productVariants',
    'productOptions',
    'collections',
    'customers',
    'productCollections',
    'productMedia',
    'productMetafields',
    'deletedProductIds',
    'deletedCollectionIds',
    'deletedCustomerIds',
  ].every((key) => isObject(value[key]));
}

function isCustomerCatalogConnectionRecord(value: unknown): value is CustomerCatalogConnectionRecord {
  return (
    isObject(value) &&
    Array.isArray(value['orderedCustomerIds']) &&
    isObject(value['cursorByCustomerId']) &&
    isObject(value['pageInfo'])
  );
}

export function loadNormalizedStateSnapshot(snapshotPath: string): NormalizedStateSnapshotFile {
  const parsed = JSON.parse(readFileSync(snapshotPath, 'utf8')) as unknown;

  if (isStateSnapshot(parsed)) {
    return {
      kind: 'normalized-state-snapshot',
      baseState: normalizeStateSnapshot(parsed),
      customerCatalogConnection: null,
      customerSearchConnections: {},
    };
  }

  if (!isObject(parsed) || !isStateSnapshot(parsed['baseState'])) {
    throw new Error(`Invalid normalized snapshot file: ${snapshotPath}`);
  }

  const customerCatalogConnection = isCustomerCatalogConnectionRecord(parsed['customerCatalogConnection'])
    ? parsed['customerCatalogConnection']
    : null;

  const customerSearchConnections = isObject(parsed['customerSearchConnections'])
    ? Object.fromEntries(
        Object.entries(parsed['customerSearchConnections']).filter(
          (entry): entry is [string, CustomerCatalogConnectionRecord] => {
            const [key, value] = entry;
            return typeof key === 'string' && isCustomerCatalogConnectionRecord(value);
          },
        ),
      )
    : {};

  return {
    kind: parsed['kind'] === 'normalized-state-snapshot' ? 'normalized-state-snapshot' : 'normalized-state-snapshot',
    baseState: normalizeStateSnapshot(parsed['baseState']),
    customerCatalogConnection,
    customerSearchConnections,
  };
}
