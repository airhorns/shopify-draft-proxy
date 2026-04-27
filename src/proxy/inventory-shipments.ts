import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootField, getRootFields } from '../graphql/root-field.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type {
  InventoryLevelRecord,
  InventoryShipmentLineItemRecord,
  InventoryShipmentRecord,
  InventoryShipmentTrackingRecord,
  ProductVariantRecord,
} from '../state/types.js';
import { paginateConnectionItems, serializeConnection } from './graphql-helpers.js';

type InventoryShipmentUserError = {
  field: string[] | null;
  message: string;
  code?: string | null;
};

type ShipmentMutationResult = {
  response: Record<string, unknown>;
  staged: boolean;
  stagedResourceIds: string[];
  notes: string;
};

const DEFAULT_INVENTORY_LEVEL_LOCATION_ID = 'gid://shopify/Location/1';
const SHIPMENT_STATUS_TERMINAL = new Set(['RECEIVED']);

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readObjectArray(value: unknown): Record<string, unknown>[] {
  return Array.isArray(value) ? value.filter(isObject) : [];
}

function responseKey(field: FieldNode): string {
  return field.alias?.value ?? field.name.value;
}

function selectedFields(selections: readonly SelectionNode[] | undefined): FieldNode[] {
  return (selections ?? []).flatMap((selection) => {
    if (selection.kind === Kind.FIELD) {
      return [selection];
    }
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      return selectedFields(selection.selectionSet.selections);
    }
    return [];
  });
}

function inventoryItemLegacyId(id: string): string | null {
  return id.split('/').at(-1)?.split('?')[0] ?? null;
}

function findVariantByInventoryItemId(inventoryItemId: string): ProductVariantRecord | null {
  return store.findEffectiveVariantByInventoryItemId(inventoryItemId);
}

function getShipmentLineItemUnreceivedQuantity(lineItem: InventoryShipmentLineItemRecord): number {
  return Math.max(0, lineItem.quantity - lineItem.acceptedQuantity - lineItem.rejectedQuantity);
}

function shipmentTotals(shipment: InventoryShipmentRecord): {
  lineItemTotalQuantity: number;
  totalAcceptedQuantity: number;
  totalReceivedQuantity: number;
  totalRejectedQuantity: number;
} {
  const totalAcceptedQuantity = shipment.lineItems.reduce((total, lineItem) => total + lineItem.acceptedQuantity, 0);
  const totalRejectedQuantity = shipment.lineItems.reduce((total, lineItem) => total + lineItem.rejectedQuantity, 0);
  return {
    lineItemTotalQuantity: shipment.lineItems.reduce((total, lineItem) => total + lineItem.quantity, 0),
    totalAcceptedQuantity,
    totalReceivedQuantity: totalAcceptedQuantity + totalRejectedQuantity,
    totalRejectedQuantity,
  };
}

function statusAfterReceive(lineItems: InventoryShipmentLineItemRecord[]): InventoryShipmentRecord['status'] {
  const total = lineItems.reduce((sum, lineItem) => sum + lineItem.quantity, 0);
  const received = lineItems.reduce((sum, lineItem) => sum + lineItem.acceptedQuantity + lineItem.rejectedQuantity, 0);
  if (received <= 0) {
    return 'IN_TRANSIT';
  }
  return received >= total ? 'RECEIVED' : 'PARTIALLY_RECEIVED';
}

function trackingFromInput(input: unknown): InventoryShipmentTrackingRecord | null {
  if (!isObject(input)) {
    return null;
  }
  return {
    trackingNumber: typeof input['trackingNumber'] === 'string' ? input['trackingNumber'] : null,
    company: typeof input['company'] === 'string' ? input['company'] : null,
    trackingUrl: typeof input['trackingUrl'] === 'string' ? input['trackingUrl'] : null,
    arrivesAt: typeof input['arrivesAt'] === 'string' ? input['arrivesAt'] : null,
  };
}

function readQuantity(level: InventoryLevelRecord, name: string): number {
  return level.quantities.find((quantity) => quantity.name === name)?.quantity ?? 0;
}

function writeQuantity(level: InventoryLevelRecord, name: string, delta: number): InventoryLevelRecord {
  const nextQuantity = Math.max(0, readQuantity(level, name) + delta);
  const existingIndex = level.quantities.findIndex((quantity) => quantity.name === name);
  const nextQuantities = [...level.quantities];
  if (existingIndex >= 0) {
    nextQuantities[existingIndex] = {
      ...nextQuantities[existingIndex]!,
      quantity: nextQuantity,
      updatedAt: makeSyntheticTimestamp(),
    };
  } else {
    nextQuantities.push({ name, quantity: nextQuantity, updatedAt: makeSyntheticTimestamp() });
  }
  return { ...level, quantities: nextQuantities };
}

function defaultInventoryLevel(variant: ProductVariantRecord): InventoryLevelRecord {
  const inventoryItemId = variant.inventoryItem?.id ?? makeSyntheticGid('InventoryItem');
  const location = store.listEffectiveLocations()[0] ?? null;
  const locationId = location?.id ?? DEFAULT_INVENTORY_LEVEL_LOCATION_ID;
  const available = variant.inventoryQuantity ?? 0;
  return {
    id: `gid://shopify/InventoryLevel/${inventoryItemLegacyId(inventoryItemId) ?? '0'}?inventory_item_id=${encodeURIComponent(inventoryItemId)}`,
    cursor: null,
    location: { id: locationId, name: location?.name ?? null },
    quantities: [
      { name: 'available', quantity: available, updatedAt: null },
      { name: 'on_hand', quantity: available, updatedAt: null },
      { name: 'incoming', quantity: 0, updatedAt: null },
    ],
  };
}

function adjustInventoryQuantities(inventoryItemId: string, deltas: { incoming?: number; available?: number }): void {
  const baseVariant = findVariantByInventoryItemId(inventoryItemId);
  if (!baseVariant?.inventoryItem) {
    return;
  }

  const variants = store
    .getEffectiveVariantsByProductId(baseVariant.productId)
    .map((candidate) => structuredClone(candidate));
  const variantIndex = variants.findIndex((candidate) => candidate.inventoryItem?.id === inventoryItemId);
  const variant = variants[variantIndex];
  if (variantIndex < 0 || !variant?.inventoryItem) {
    return;
  }

  const levels = variant.inventoryItem.inventoryLevels?.length
    ? structuredClone(variant.inventoryItem.inventoryLevels)
    : [defaultInventoryLevel(variant)];
  let targetLevel = levels[0] ?? defaultInventoryLevel(variant);
  if (typeof deltas.incoming === 'number') {
    targetLevel = writeQuantity(targetLevel, 'incoming', deltas.incoming);
  }
  if (typeof deltas.available === 'number') {
    targetLevel = writeQuantity(writeQuantity(targetLevel, 'available', deltas.available), 'on_hand', deltas.available);
  }
  levels[0] = targetLevel;

  const availableTotal = levels.reduce((total, level) => total + readQuantity(level, 'available'), 0);
  variants[variantIndex] = {
    ...variant,
    inventoryQuantity: availableTotal,
    inventoryItem: {
      ...variant.inventoryItem,
      inventoryLevels: levels,
    },
  };
  store.replaceStagedVariantsForProduct(baseVariant.productId, variants);
}

function serializeCount(count: number, selections: readonly SelectionNode[] | undefined): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    switch (field.name.value) {
      case 'count':
        result[responseKey(field)] = count;
        break;
      case 'precision':
        result[responseKey(field)] = 'EXACT';
        break;
      default:
        result[responseKey(field)] = null;
    }
  }
  return result;
}

function serializeInventoryItem(
  inventoryItemId: string,
  selections: readonly SelectionNode[] | undefined,
): Record<string, unknown> | null {
  const variant = findVariantByInventoryItemId(inventoryItemId);
  if (!variant?.inventoryItem) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    switch (field.name.value) {
      case 'id':
        result[responseKey(field)] = variant.inventoryItem.id;
        break;
      case 'legacyResourceId':
        result[responseKey(field)] = inventoryItemLegacyId(variant.inventoryItem.id);
        break;
      case 'sku':
        result[responseKey(field)] = variant.sku;
        break;
      case 'tracked':
        result[responseKey(field)] = variant.inventoryItem.tracked;
        break;
      case 'requiresShipping':
        result[responseKey(field)] = variant.inventoryItem.requiresShipping;
        break;
      default:
        result[responseKey(field)] = null;
    }
  }
  return result;
}

function serializeShipmentTracking(
  tracking: InventoryShipmentTrackingRecord | null,
  selections: readonly SelectionNode[] | undefined,
): Record<string, unknown> | null {
  if (!tracking) {
    return null;
  }
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    result[responseKey(field)] =
      field.name.value === 'trackingNumber' ||
      field.name.value === 'company' ||
      field.name.value === 'trackingUrl' ||
      field.name.value === 'arrivesAt'
        ? tracking[field.name.value]
        : null;
  }
  return result;
}

function serializeShipmentLineItem(
  lineItem: InventoryShipmentLineItemRecord,
  field: FieldNode,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const child of selectedFields(field.selectionSet?.selections)) {
    switch (child.name.value) {
      case 'id':
        result[responseKey(child)] = lineItem.id;
        break;
      case 'quantity':
        result[responseKey(child)] = lineItem.quantity;
        break;
      case 'acceptedQuantity':
        result[responseKey(child)] = lineItem.acceptedQuantity;
        break;
      case 'rejectedQuantity':
        result[responseKey(child)] = lineItem.rejectedQuantity;
        break;
      case 'unreceivedQuantity':
        result[responseKey(child)] = getShipmentLineItemUnreceivedQuantity(lineItem);
        break;
      case 'inventoryItem':
        result[responseKey(child)] = serializeInventoryItem(lineItem.inventoryItemId, child.selectionSet?.selections);
        break;
      default:
        result[responseKey(child)] = null;
    }
  }
  return result;
}

function serializeShipmentLineItemsConnection(
  shipment: InventoryShipmentRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const items = [...shipment.lineItems];
  const window = paginateConnectionItems(items, field, variables, (lineItem) => `cursor:${lineItem.id}`);
  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (lineItem) => lineItem.id,
    serializeNode: (lineItem, selection) => serializeShipmentLineItem(lineItem, selection),
  });
}

function serializeShipment(
  shipment: InventoryShipmentRecord | null,
  selections: readonly SelectionNode[] | undefined,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  if (!shipment) {
    return null;
  }
  const totals = shipmentTotals(shipment);
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(selections)) {
    switch (field.name.value) {
      case 'id':
      case 'name':
      case 'status':
        result[responseKey(field)] = shipment[field.name.value];
        break;
      case 'lineItemTotalQuantity':
      case 'totalAcceptedQuantity':
      case 'totalReceivedQuantity':
      case 'totalRejectedQuantity':
        result[responseKey(field)] = totals[field.name.value];
        break;
      case 'lineItems':
        result[responseKey(field)] = serializeShipmentLineItemsConnection(shipment, field, variables);
        break;
      case 'lineItemsCount':
        result[responseKey(field)] = serializeCount(shipment.lineItems.length, field.selectionSet?.selections);
        break;
      case 'tracking':
        result[responseKey(field)] = serializeShipmentTracking(shipment.tracking, field.selectionSet?.selections);
        break;
      default:
        result[responseKey(field)] = null;
    }
  }
  return result;
}

function serializeUserErrors(
  field: FieldNode | undefined,
  userErrors: InventoryShipmentUserError[],
): Array<Record<string, unknown>> {
  const selections = selectedFields(field?.selectionSet?.selections);
  if (selections.length === 0) {
    return userErrors.map((error) => ({ field: error.field, message: error.message }));
  }

  return userErrors.map((error) => {
    const result: Record<string, unknown> = {};
    for (const selection of selections) {
      switch (selection.name.value) {
        case 'field':
          result[responseKey(selection)] = error.field;
          break;
        case 'message':
          result[responseKey(selection)] = error.message;
          break;
        case 'code':
          result[responseKey(selection)] = error.code ?? null;
          break;
        default:
          result[responseKey(selection)] = null;
      }
    }
    return result;
  });
}

function childField(root: FieldNode, name: string): FieldNode | undefined {
  return selectedFields(root.selectionSet?.selections).find((field) => field.name.value === name);
}

function validateLineItemInputs(
  lineItems: Record<string, unknown>[],
  fieldPrefix: string[],
): InventoryShipmentUserError[] {
  const userErrors: InventoryShipmentUserError[] = [];
  if (lineItems.length === 0) {
    userErrors.push({ field: fieldPrefix, message: 'At least one line item is required.', code: 'BLANK' });
  }

  lineItems.forEach((lineItem, index) => {
    const inventoryItemId = typeof lineItem['inventoryItemId'] === 'string' ? lineItem['inventoryItemId'] : null;
    const quantity = typeof lineItem['quantity'] === 'number' ? lineItem['quantity'] : null;
    if (!inventoryItemId || !findVariantByInventoryItemId(inventoryItemId)) {
      userErrors.push({
        field: [...fieldPrefix, String(index), 'inventoryItemId'],
        message: 'The specified inventory item could not be found.',
        code: 'NOT_FOUND',
      });
    }
    if (quantity === null || quantity <= 0) {
      userErrors.push({
        field: [...fieldPrefix, String(index), 'quantity'],
        message: 'Quantity must be greater than 0.',
        code: 'INVALID',
      });
    }
  });
  return userErrors;
}

function buildLineItems(lineItems: Record<string, unknown>[]): InventoryShipmentLineItemRecord[] {
  return lineItems.map((lineItem) => ({
    id: makeSyntheticGid('InventoryShipmentLineItem'),
    inventoryItemId: lineItem['inventoryItemId'] as string,
    quantity: lineItem['quantity'] as number,
    acceptedQuantity: 0,
    rejectedQuantity: 0,
  }));
}

function stageShipmentWithIncoming(shipment: InventoryShipmentRecord): InventoryShipmentRecord {
  const previous = store.getEffectiveInventoryShipmentById(shipment.id);
  if (previous?.status !== 'IN_TRANSIT' && shipment.status === 'IN_TRANSIT') {
    for (const lineItem of shipment.lineItems) {
      adjustInventoryQuantities(lineItem.inventoryItemId, {
        incoming: getShipmentLineItemUnreceivedQuantity(lineItem),
      });
    }
  }
  return store.stageInventoryShipment(shipment);
}

function mutationPayload(
  rootField: FieldNode,
  payload: Record<string, unknown>,
  userErrors: InventoryShipmentUserError[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const field of selectedFields(rootField.selectionSet?.selections)) {
    if (field.name.value === 'userErrors') {
      result[responseKey(field)] = serializeUserErrors(field, userErrors);
    } else {
      result[responseKey(field)] = payload[field.name.value] ?? null;
    }
  }
  return { data: { [responseKey(rootField)]: result } };
}

function shipmentPayloadValue(
  shipment: InventoryShipmentRecord | null,
  field: FieldNode | undefined,
  variables: Record<string, unknown>,
): Record<string, unknown> | null {
  return serializeShipment(shipment, field?.selectionSet?.selections, variables);
}

export function handleInventoryShipmentQuery(
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  for (const rootField of getRootFields(document)) {
    if (rootField.name.value !== 'inventoryShipment') {
      continue;
    }
    const args = getFieldArguments(rootField, variables);
    const shipmentId = typeof args['id'] === 'string' ? args['id'] : null;
    data[responseKey(rootField)] = shipmentId
      ? serializeShipment(
          store.getEffectiveInventoryShipmentById(shipmentId),
          rootField.selectionSet?.selections,
          variables,
        )
      : null;
  }

  return {
    data,
  };
}

export function handleInventoryShipmentMutation(
  document: string,
  variables: Record<string, unknown>,
): ShipmentMutationResult | null {
  const rootField = getRootField(document);
  const args = getFieldArguments(rootField, variables);
  const rootName = rootField.name.value;
  const now = makeSyntheticTimestamp();

  if (rootName === 'inventoryShipmentCreate' || rootName === 'inventoryShipmentCreateInTransit') {
    const input = isObject(args['input']) ? args['input'] : {};
    const movementId = typeof input['movementId'] === 'string' ? input['movementId'] : null;
    const lineItemInputs = readObjectArray(input['lineItems']);
    const userErrors = validateLineItemInputs(lineItemInputs, ['input', 'lineItems']);
    if (!movementId) {
      userErrors.push({ field: ['input', 'movementId'], message: 'Movement id is required.', code: 'BLANK' });
    }
    const shipment =
      userErrors.length === 0 && movementId
        ? stageShipmentWithIncoming({
            id: makeSyntheticGid('InventoryShipment'),
            movementId,
            name: `#S${store.listEffectiveInventoryShipments().length + 1}`,
            status: rootName === 'inventoryShipmentCreateInTransit' ? 'IN_TRANSIT' : 'DRAFT',
            createdAt: now,
            updatedAt: now,
            tracking: trackingFromInput(input['trackingInput']),
            lineItems: buildLineItems(lineItemInputs),
          })
        : null;
    const inventoryShipmentField = childField(rootField, 'inventoryShipment');
    return {
      response: mutationPayload(
        rootField,
        { inventoryShipment: shipmentPayloadValue(shipment, inventoryShipmentField, variables) },
        userErrors,
      ),
      staged: userErrors.length === 0,
      stagedResourceIds: shipment ? [shipment.id, ...shipment.lineItems.map((lineItem) => lineItem.id)] : [],
      notes: 'Staged locally in the in-memory inventory shipment draft store.',
    };
  }

  const shipmentId = typeof args['id'] === 'string' ? args['id'] : null;
  const existing = shipmentId ? store.getEffectiveInventoryShipmentById(shipmentId) : null;
  const notFound = !existing
    ? [{ field: ['id'], message: 'The specified inventory shipment could not be found.', code: 'NOT_FOUND' }]
    : [];
  if (!existing) {
    return {
      response: mutationPayload(rootField, {}, notFound),
      staged: false,
      stagedResourceIds: [],
      notes: 'Returned a local inventory shipment validation error without proxying upstream.',
    };
  }

  if (rootName === 'inventoryShipmentSetTracking') {
    const userErrors = SHIPMENT_STATUS_TERMINAL.has(existing.status)
      ? [{ field: ['id'], message: 'Received shipments cannot be updated.', code: 'INVALID_STATUS' }]
      : [];
    const shipment =
      userErrors.length === 0
        ? store.stageInventoryShipment({ ...existing, tracking: trackingFromInput(args['tracking']), updatedAt: now })
        : existing;
    const inventoryShipmentField = childField(rootField, 'inventoryShipment');
    return {
      response: mutationPayload(
        rootField,
        {
          inventoryShipment: shipmentPayloadValue(
            userErrors.length === 0 ? shipment : null,
            inventoryShipmentField,
            variables,
          ),
        },
        userErrors,
      ),
      staged: userErrors.length === 0,
      stagedResourceIds: userErrors.length === 0 ? [existing.id] : [],
      notes: 'Staged local inventory shipment tracking update.',
    };
  }

  if (rootName === 'inventoryShipmentMarkInTransit') {
    const userErrors =
      existing.status === 'DRAFT'
        ? []
        : [{ field: ['id'], message: 'Only draft shipments can be marked in transit.', code: 'INVALID_STATUS' }];
    const shipment =
      userErrors.length === 0
        ? stageShipmentWithIncoming({ ...existing, status: 'IN_TRANSIT', updatedAt: now })
        : existing;
    const inventoryShipmentField = childField(rootField, 'inventoryShipment');
    return {
      response: mutationPayload(
        rootField,
        {
          inventoryShipment: shipmentPayloadValue(
            userErrors.length === 0 ? shipment : null,
            inventoryShipmentField,
            variables,
          ),
        },
        userErrors,
      ),
      staged: userErrors.length === 0,
      stagedResourceIds: userErrors.length === 0 ? [existing.id] : [],
      notes: 'Staged local inventory shipment transition to in transit.',
    };
  }

  if (rootName === 'inventoryShipmentAddItems') {
    const lineItemInputs = readObjectArray(args['lineItems']);
    const userErrors = SHIPMENT_STATUS_TERMINAL.has(existing.status)
      ? [{ field: ['id'], message: 'Received shipments cannot be updated.', code: 'INVALID_STATUS' }]
      : validateLineItemInputs(lineItemInputs, ['lineItems']);
    const addedItems = userErrors.length === 0 ? buildLineItems(lineItemInputs) : [];
    const shipment =
      userErrors.length === 0
        ? store.stageInventoryShipment({
            ...existing,
            updatedAt: now,
            lineItems: [...existing.lineItems, ...addedItems],
          })
        : existing;
    if (userErrors.length === 0 && existing.status === 'IN_TRANSIT') {
      for (const lineItem of addedItems) {
        adjustInventoryQuantities(lineItem.inventoryItemId, { incoming: lineItem.quantity });
      }
    }
    return {
      response: mutationPayload(
        rootField,
        {
          addedItems: childField(rootField, 'addedItems')
            ? addedItems.map((lineItem) => serializeShipmentLineItem(lineItem, childField(rootField, 'addedItems')!))
            : addedItems.map((lineItem) => ({ id: lineItem.id })),
          inventoryShipment: shipmentPayloadValue(
            userErrors.length === 0 ? shipment : null,
            childField(rootField, 'inventoryShipment'),
            variables,
          ),
        },
        userErrors,
      ),
      staged: userErrors.length === 0,
      stagedResourceIds: userErrors.length === 0 ? [shipment.id, ...addedItems.map((lineItem) => lineItem.id)] : [],
      notes: 'Staged local inventory shipment line-item additions.',
    };
  }

  if (rootName === 'inventoryShipmentRemoveItems') {
    const ids = Array.isArray(args['lineItems'])
      ? args['lineItems'].filter((value): value is string => typeof value === 'string')
      : [];
    const knownIds = new Set(existing.lineItems.map((lineItem) => lineItem.id));
    const userErrors = ids.some((id) => !knownIds.has(id))
      ? [{ field: ['lineItems'], message: 'One or more shipment line items could not be found.', code: 'NOT_FOUND' }]
      : SHIPMENT_STATUS_TERMINAL.has(existing.status)
        ? [{ field: ['id'], message: 'Received shipments cannot be updated.', code: 'INVALID_STATUS' }]
        : [];
    const removed = existing.lineItems.filter((lineItem) => ids.includes(lineItem.id));
    if (userErrors.length === 0 && existing.status === 'IN_TRANSIT') {
      for (const lineItem of removed) {
        adjustInventoryQuantities(lineItem.inventoryItemId, {
          incoming: -getShipmentLineItemUnreceivedQuantity(lineItem),
        });
      }
    }
    const shipment =
      userErrors.length === 0
        ? store.stageInventoryShipment({
            ...existing,
            updatedAt: now,
            lineItems: existing.lineItems.filter((lineItem) => !ids.includes(lineItem.id)),
          })
        : existing;
    return {
      response: mutationPayload(
        rootField,
        {
          inventoryShipment: shipmentPayloadValue(
            userErrors.length === 0 ? shipment : null,
            childField(rootField, 'inventoryShipment'),
            variables,
          ),
        },
        userErrors,
      ),
      staged: userErrors.length === 0,
      stagedResourceIds: userErrors.length === 0 ? [existing.id] : [],
      notes: 'Staged local inventory shipment line-item removals.',
    };
  }

  if (rootName === 'inventoryShipmentUpdateItemQuantities') {
    const updates = readObjectArray(args['items']);
    const lineItemById = new Map(existing.lineItems.map((lineItem) => [lineItem.id, lineItem]));
    const userErrors: InventoryShipmentUserError[] = [];
    const nextLineItems = existing.lineItems.map((lineItem) => ({ ...lineItem }));
    const inventoryDeltas: Array<{ inventoryItemId: string; incoming: number }> = [];
    for (const [index, update] of updates.entries()) {
      const lineItemId = typeof update['shipmentLineItemId'] === 'string' ? update['shipmentLineItemId'] : null;
      const quantity = typeof update['quantity'] === 'number' ? update['quantity'] : null;
      const lineItem = lineItemId ? lineItemById.get(lineItemId) : null;
      if (!lineItem) {
        userErrors.push({
          field: ['items', String(index), 'shipmentLineItemId'],
          message: 'Shipment line item could not be found.',
          code: 'NOT_FOUND',
        });
        continue;
      }
      if (quantity === null || quantity < lineItem.acceptedQuantity + lineItem.rejectedQuantity) {
        userErrors.push({
          field: ['items', String(index), 'quantity'],
          message: 'Quantity cannot be less than received quantity.',
          code: 'INVALID',
        });
        continue;
      }
      const next = nextLineItems.find((candidate) => candidate.id === lineItem.id);
      if (next) {
        if (existing.status === 'IN_TRANSIT') {
          inventoryDeltas.push({ inventoryItemId: next.inventoryItemId, incoming: quantity - next.quantity });
        }
        next.quantity = quantity;
      }
    }
    if (userErrors.length === 0) {
      for (const delta of inventoryDeltas) {
        adjustInventoryQuantities(delta.inventoryItemId, { incoming: delta.incoming });
      }
    }
    const shipment =
      userErrors.length === 0
        ? store.stageInventoryShipment({ ...existing, updatedAt: now, lineItems: nextLineItems })
        : existing;
    const updatedLineItems = updates
      .map((update) =>
        typeof update['shipmentLineItemId'] === 'string'
          ? nextLineItems.find((lineItem) => lineItem.id === update['shipmentLineItemId'])
          : null,
      )
      .filter((lineItem): lineItem is InventoryShipmentLineItemRecord => lineItem != null);
    return {
      response: mutationPayload(
        rootField,
        {
          shipment: shipmentPayloadValue(
            userErrors.length === 0 ? shipment : null,
            childField(rootField, 'shipment'),
            variables,
          ),
          updatedLineItems: childField(rootField, 'updatedLineItems')
            ? userErrors.length === 0
              ? updatedLineItems.map((lineItem) =>
                  serializeShipmentLineItem(lineItem, childField(rootField, 'updatedLineItems')!),
                )
              : []
            : userErrors.length === 0
              ? updatedLineItems.map((lineItem) => ({ id: lineItem.id }))
              : [],
        },
        userErrors,
      ),
      staged: userErrors.length === 0,
      stagedResourceIds: userErrors.length === 0 ? [existing.id] : [],
      notes: 'Staged local inventory shipment line-item quantity updates.',
    };
  }

  if (rootName === 'inventoryShipmentReceive') {
    const receiveInputs = readObjectArray(args['lineItems']);
    const lineItemById = new Map(existing.lineItems.map((lineItem) => [lineItem.id, lineItem]));
    const userErrors =
      existing.status === 'DRAFT'
        ? [{ field: ['id'], message: 'Only in-transit shipments can be received.', code: 'INVALID_STATUS' }]
        : [];
    const nextLineItems = existing.lineItems.map((lineItem) => ({ ...lineItem }));
    const inventoryDeltas: Array<{ inventoryItemId: string; incoming: number; available?: number }> = [];
    const inputs =
      receiveInputs.length > 0
        ? receiveInputs
        : existing.lineItems.map((lineItem) => ({
            shipmentLineItemId: lineItem.id,
            quantity: getShipmentLineItemUnreceivedQuantity(lineItem),
            reason: args['bulkReceiveAction'] === 'REJECTED' ? 'REJECTED' : 'ACCEPTED',
          }));
    for (const [index, input] of inputs.entries()) {
      const lineItemId = typeof input['shipmentLineItemId'] === 'string' ? input['shipmentLineItemId'] : null;
      const quantity = typeof input['quantity'] === 'number' ? input['quantity'] : null;
      const reason = input['reason'] === 'REJECTED' ? 'REJECTED' : input['reason'] === 'ACCEPTED' ? 'ACCEPTED' : null;
      const current = lineItemId ? lineItemById.get(lineItemId) : null;
      const next = lineItemId ? nextLineItems.find((lineItem) => lineItem.id === lineItemId) : null;
      if (!current || !next) {
        userErrors.push({
          field: ['lineItems', String(index), 'shipmentLineItemId'],
          message: 'Shipment line item could not be found.',
          code: 'NOT_FOUND',
        });
        continue;
      }
      if (quantity === null || quantity <= 0 || quantity > getShipmentLineItemUnreceivedQuantity(current)) {
        userErrors.push({
          field: ['lineItems', String(index), 'quantity'],
          message: 'Quantity must be greater than 0 and no more than the unreceived quantity.',
          code: 'INVALID',
        });
        continue;
      }
      if (!reason) {
        userErrors.push({
          field: ['lineItems', String(index), 'reason'],
          message: 'Receive reason is required.',
          code: 'BLANK',
        });
        continue;
      }
      if (reason === 'ACCEPTED') {
        next.acceptedQuantity += quantity;
        inventoryDeltas.push({ inventoryItemId: next.inventoryItemId, incoming: -quantity, available: quantity });
      } else {
        next.rejectedQuantity += quantity;
        inventoryDeltas.push({ inventoryItemId: next.inventoryItemId, incoming: -quantity });
      }
    }
    if (userErrors.length === 0) {
      for (const delta of inventoryDeltas) {
        adjustInventoryQuantities(delta.inventoryItemId, {
          incoming: delta.incoming,
          ...(typeof delta.available === 'number' ? { available: delta.available } : {}),
        });
      }
    }
    const shipment =
      userErrors.length === 0
        ? store.stageInventoryShipment({
            ...existing,
            status: statusAfterReceive(nextLineItems),
            updatedAt: now,
            lineItems: nextLineItems,
          })
        : existing;
    return {
      response: mutationPayload(
        rootField,
        {
          inventoryShipment: shipmentPayloadValue(
            userErrors.length === 0 ? shipment : null,
            childField(rootField, 'inventoryShipment'),
            variables,
          ),
        },
        userErrors,
      ),
      staged: userErrors.length === 0,
      stagedResourceIds: userErrors.length === 0 ? [existing.id] : [],
      notes: 'Staged local inventory shipment receiving effects over product-backed inventory levels.',
    };
  }

  if (rootName === 'inventoryShipmentDelete') {
    const userErrors =
      existing.status === 'RECEIVED'
        ? [{ field: ['id'], message: 'Received shipments cannot be deleted.', code: 'INVALID_STATUS' }]
        : [];
    if (userErrors.length === 0 && existing.status === 'IN_TRANSIT') {
      for (const lineItem of existing.lineItems) {
        adjustInventoryQuantities(lineItem.inventoryItemId, {
          incoming: -getShipmentLineItemUnreceivedQuantity(lineItem),
        });
      }
    }
    if (userErrors.length === 0) {
      store.stageDeleteInventoryShipment(existing.id);
    }
    return {
      response: mutationPayload(rootField, { id: userErrors.length === 0 ? existing.id : null }, userErrors),
      staged: userErrors.length === 0,
      stagedResourceIds: userErrors.length === 0 ? [existing.id] : [],
      notes: 'Staged local inventory shipment deletion.',
    };
  }

  return null;
}
