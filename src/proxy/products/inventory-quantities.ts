import { makeSyntheticTimestamp } from '../../state/synthetic-identity.js';
import type { InventoryLevelRecord } from '../../state/types.js';

export const DEFAULT_INVENTORY_LEVEL_LOCATION_ID = 'gid://shopify/Location/1';

export function buildStableSyntheticInventoryLevelId(inventoryItemId: string, locationId: string): string {
  const inventoryItemTail = inventoryItemId.split('/').at(-1) ?? encodeURIComponent(inventoryItemId);
  const locationTail = locationId.split('/').at(-1) ?? encodeURIComponent(locationId);

  return `gid://shopify/InventoryLevel/${inventoryItemTail}-${locationTail}?inventory_item_id=${encodeURIComponent(
    inventoryItemId,
  )}`;
}

export function readInventoryQuantityAmount(
  quantities: InventoryLevelRecord['quantities'],
  name: string,
  fallback = 0,
): number {
  return quantities.find((quantity) => quantity.name === name)?.quantity ?? fallback;
}

export function writeInventoryQuantityAmount(
  quantities: InventoryLevelRecord['quantities'],
  name: string,
  quantity: number,
): InventoryLevelRecord['quantities'] {
  const existingIndex = quantities.findIndex((candidate) => candidate.name === name);
  if (existingIndex >= 0) {
    return quantities.map((candidate, index) =>
      index === existingIndex ? { ...candidate, quantity, updatedAt: makeSyntheticTimestamp() } : candidate,
    );
  }

  return [...quantities, { name, quantity, updatedAt: makeSyntheticTimestamp() }];
}

export function addInventoryQuantityAmount(
  quantities: InventoryLevelRecord['quantities'],
  name: string,
  delta: number,
): InventoryLevelRecord['quantities'] {
  return writeInventoryQuantityAmount(quantities, name, readInventoryQuantityAmount(quantities, name) + delta);
}

export function sumAvailableInventoryLevels(levels: InventoryLevelRecord[]): number {
  return levels.reduce((total, level) => total + readInventoryQuantityAmount(level.quantities, 'available'), 0);
}
