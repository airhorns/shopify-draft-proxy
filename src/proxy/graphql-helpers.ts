import { Kind, type FieldNode } from 'graphql';

import { getFieldArguments } from '../graphql/root-field.js';

export type SelectedFieldOptions = {
  includeInlineFragments?: boolean;
};

export type ConnectionWindow<T> = {
  items: T[];
  hasNextPage: boolean;
  hasPreviousPage: boolean;
};

export type ConnectionPageInfoOptions = SelectedFieldOptions & {
  prefixCursors?: boolean;
  fallbackStartCursor?: string | null;
  fallbackEndCursor?: string | null;
};

export function getFieldResponseKey(field: FieldNode): string {
  return field.alias?.value ?? field.name.value;
}

export function getSelectedChildFields(field: FieldNode, options: SelectedFieldOptions = {}): FieldNode[] {
  return (field.selectionSet?.selections ?? []).flatMap((selection) => {
    if (selection.kind === Kind.FIELD) {
      return [selection];
    }

    if (options.includeInlineFragments === true && selection.kind === Kind.INLINE_FRAGMENT) {
      return selection.selectionSet.selections.filter(
        (inlineSelection): inlineSelection is FieldNode => inlineSelection.kind === Kind.FIELD,
      );
    }

    return [];
  });
}

export function readNullableIntArgument(
  field: FieldNode,
  argumentName: string,
  variables: Record<string, unknown>,
): number | null {
  const argument = field.arguments?.find((candidate) => candidate.name.value === argumentName);
  if (!argument) {
    return null;
  }

  if (argument.value.kind === Kind.INT) {
    const parsed = Number.parseInt(argument.value.value, 10);
    return Number.isFinite(parsed) ? parsed : null;
  }

  if (argument.value.kind === Kind.VARIABLE) {
    const rawValue = variables[argument.value.name.value];
    return typeof rawValue === 'number' && Number.isFinite(rawValue) ? rawValue : null;
  }

  return null;
}

export function readNullableStringArgument(
  field: FieldNode,
  argumentName: string,
  variables: Record<string, unknown>,
): string | null {
  const argument = field.arguments?.find((candidate) => candidate.name.value === argumentName);
  if (!argument) {
    return null;
  }

  if (argument.value.kind === Kind.STRING) {
    return argument.value.value;
  }

  if (argument.value.kind === Kind.VARIABLE) {
    const rawValue = variables[argument.value.name.value];
    return typeof rawValue === 'string' ? rawValue : null;
  }

  return null;
}

export function buildSyntheticCursor(id: string): string {
  return `cursor:${id}`;
}

function readConnectionSizeArgument(raw: unknown): number | null {
  return typeof raw === 'number' && Number.isInteger(raw) && raw >= 0 ? raw : null;
}

function readConnectionCursor(raw: unknown): string | null {
  if (typeof raw !== 'string') {
    return null;
  }

  if (raw.startsWith('cursor:')) {
    const cursorValue = raw.slice('cursor:'.length);
    return cursorValue.length > 0 ? cursorValue : null;
  }

  return raw.length > 0 ? raw : null;
}

export function paginateConnectionItems<T>(
  items: T[],
  field: FieldNode,
  variables: Record<string, unknown>,
  getCursorValue: (item: T) => string,
): ConnectionWindow<T> {
  const args = getFieldArguments(field, variables);
  const first = readConnectionSizeArgument(args['first']);
  const last = readConnectionSizeArgument(args['last']);
  const after = readConnectionCursor(args['after']);
  const before = readConnectionCursor(args['before']);

  const startIndex = after === null ? 0 : items.findIndex((item) => getCursorValue(item) === after) + 1;
  const beforeIndex = before === null ? items.length : items.findIndex((item) => getCursorValue(item) === before);
  const windowStart = Math.max(0, startIndex);
  const windowEnd = Math.max(windowStart, beforeIndex >= 0 ? beforeIndex : items.length);
  const paginatedItems = items.slice(windowStart, windowEnd);

  let limitedItems = paginatedItems;
  let hasNextPage = windowEnd < items.length;
  let hasPreviousPage = windowStart > 0;

  if (first !== null) {
    hasNextPage = hasNextPage || paginatedItems.length > first;
    limitedItems = limitedItems.slice(0, first);
  }

  if (last !== null) {
    hasPreviousPage = hasPreviousPage || limitedItems.length > last;
    limitedItems = limitedItems.slice(Math.max(0, limitedItems.length - last));
  }

  return {
    items: limitedItems,
    hasNextPage,
    hasPreviousPage,
  };
}

export function serializeConnectionPageInfo<T>(
  selection: FieldNode,
  items: T[],
  hasNextPage: boolean,
  hasPreviousPage: boolean,
  getCursorValue: (item: T) => string,
  options: ConnectionPageInfoOptions = {},
): Record<string, unknown> {
  const formatCursor = (item: T): string => {
    const cursor = getCursorValue(item);
    return options.prefixCursors === false ? cursor : buildSyntheticCursor(cursor);
  };

  return Object.fromEntries(
    getSelectedChildFields(selection, options).map((pageInfoSelection) => {
      const pageInfoKey = getFieldResponseKey(pageInfoSelection);
      switch (pageInfoSelection.name.value) {
        case 'hasNextPage':
          return [pageInfoKey, hasNextPage];
        case 'hasPreviousPage':
          return [pageInfoKey, hasPreviousPage];
        case 'startCursor':
          return [pageInfoKey, items[0] ? formatCursor(items[0]) : (options.fallbackStartCursor ?? null)];
        case 'endCursor':
          return [
            pageInfoKey,
            items.length > 0 ? formatCursor(items[items.length - 1]!) : (options.fallbackEndCursor ?? null),
          ];
        default:
          return [pageInfoKey, null];
      }
    }),
  );
}

export function serializeEmptyConnectionPageInfo(
  selection: FieldNode,
  options: SelectedFieldOptions = {},
): Record<string, unknown> {
  return serializeConnectionPageInfo(selection, [], false, false, () => '', options);
}
