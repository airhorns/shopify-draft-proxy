import type { FieldNode } from 'graphql';

import { getRootFields } from '../graphql/root-field.js';
import { getFieldResponseKey, getSelectedChildFields, serializeConnection } from './graphql-helpers.js';

function serializeEmptyEventsConnection(field: FieldNode): Record<string, unknown> {
  return serializeConnection<never>(field, {
    items: [],
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: () => '',
    serializeNode: () => null,
  });
}

function serializeExactZeroCount(field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'count':
        result[key] = 0;
        break;
      case 'precision':
        result[key] = 'EXACT';
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

export function handleEventsQuery(document: string): { data: Record<string, unknown> } {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'event':
        data[key] = null;
        break;
      case 'events':
        data[key] = serializeEmptyEventsConnection(field);
        break;
      case 'eventsCount':
        data[key] = serializeExactZeroCount(field);
        break;
      default:
        data[key] = null;
        break;
    }
  }

  return { data };
}
