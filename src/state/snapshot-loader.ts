import { readFileSync } from 'node:fs';

import type { NormalizedStateSnapshotFile } from './types.js';
import { normalizedStateSnapshotFileSchema, stateSnapshotSchema } from './types.js';

export function loadNormalizedStateSnapshot(snapshotPath: string): NormalizedStateSnapshotFile {
  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(snapshotPath, 'utf8')) as unknown;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Invalid JSON in normalized snapshot file ${snapshotPath}: ${message}`);
  }

  const rawStateSnapshot = stateSnapshotSchema.safeParse(parsed);
  if (rawStateSnapshot.success) {
    return {
      kind: 'normalized-state-snapshot',
      baseState: rawStateSnapshot.data,
      productSearchConnections: {},
      customerCatalogConnection: null,
      customerSearchConnections: {},
    };
  }

  const snapshotFile = normalizedStateSnapshotFileSchema.safeParse(parsed);
  if (!snapshotFile.success) {
    throw new Error(`Invalid normalized snapshot file ${snapshotPath}: ${snapshotFile.error.message}`);
  }

  return {
    kind: 'normalized-state-snapshot',
    baseState: snapshotFile.data.baseState,
    productSearchConnections: snapshotFile.data.productSearchConnections ?? {},
    customerCatalogConnection: snapshotFile.data.customerCatalogConnection ?? null,
    customerSearchConnections: snapshotFile.data.customerSearchConnections ?? {},
  };
}
