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

export type ConnectionWindowOptions = {
  parseCursor?: (raw: string) => string | null;
};

export type ConnectionPageInfoOptions = SelectedFieldOptions & {
  prefixCursors?: boolean;
  includeCursors?: boolean;
  fallbackStartCursor?: string | null;
  fallbackEndCursor?: string | null;
};

export type SerializeConnectionOptions<T> = {
  items: T[];
  hasNextPage: boolean;
  hasPreviousPage: boolean;
  getCursorValue: (item: T, index: number) => string;
  serializeNode: (item: T, field: FieldNode, index: number) => unknown;
  serializeUnknownField?: (field: FieldNode) => unknown;
  selectedFieldOptions?: SelectedFieldOptions;
  pageInfoOptions?: ConnectionPageInfoOptions;
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

function formatConnectionCursor<T>(
  item: T,
  index: number,
  getCursorValue: (item: T, index: number) => string,
  options: ConnectionPageInfoOptions,
): string {
  const cursor = getCursorValue(item, index);
  return options.prefixCursors === false ? cursor : buildSyntheticCursor(cursor);
}

export function paginateConnectionItems<T>(
  items: T[],
  field: FieldNode,
  variables: Record<string, unknown>,
  getCursorValue: (item: T) => string,
  options: ConnectionWindowOptions = {},
): ConnectionWindow<T> {
  const args = getFieldArguments(field, variables);
  const first = readConnectionSizeArgument(args['first']);
  const last = readConnectionSizeArgument(args['last']);
  const parseCursor = options.parseCursor ?? readConnectionCursor;
  const after = typeof args['after'] === 'string' ? parseCursor(args['after']) : readConnectionCursor(args['after']);
  const before =
    typeof args['before'] === 'string' ? parseCursor(args['before']) : readConnectionCursor(args['before']);

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
  getCursorValue: (item: T, index: number) => string,
  options: ConnectionPageInfoOptions = {},
): Record<string, unknown> {
  return Object.fromEntries(
    getSelectedChildFields(selection, options).map((pageInfoSelection) => {
      const pageInfoKey = getFieldResponseKey(pageInfoSelection);
      switch (pageInfoSelection.name.value) {
        case 'hasNextPage':
          return [pageInfoKey, hasNextPage];
        case 'hasPreviousPage':
          return [pageInfoKey, hasPreviousPage];
        case 'startCursor':
          return [
            pageInfoKey,
            options.includeCursors === false
              ? null
              : items[0]
                ? formatConnectionCursor(items[0], 0, getCursorValue, options)
                : (options.fallbackStartCursor ?? null),
          ];
        case 'endCursor':
          return [
            pageInfoKey,
            options.includeCursors === false
              ? null
              : items.length > 0
                ? formatConnectionCursor(items[items.length - 1]!, items.length - 1, getCursorValue, options)
                : (options.fallbackEndCursor ?? null),
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

export function serializeConnection<T>(
  field: FieldNode,
  {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue,
    serializeNode,
    serializeUnknownField,
    selectedFieldOptions = {},
    pageInfoOptions = selectedFieldOptions,
  }: SerializeConnectionOptions<T>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field, selectedFieldOptions)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        result[key] = items.map((item, index) => serializeNode(item, selection, index));
        break;
      case 'edges':
        result[key] = items.map((item, index) => {
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection, selectedFieldOptions)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edge[edgeKey] = formatConnectionCursor(item, index, getCursorValue, pageInfoOptions);
                break;
              case 'node':
                edge[edgeKey] = serializeNode(item, edgeSelection, index);
                break;
              default:
                edge[edgeKey] = null;
                break;
            }
          }
          return edge;
        });
        break;
      case 'pageInfo':
        result[key] = serializeConnectionPageInfo(selection, items, hasNextPage, hasPreviousPage, getCursorValue, {
          ...selectedFieldOptions,
          ...pageInfoOptions,
        });
        break;
      default:
        result[key] = serializeUnknownField ? serializeUnknownField(selection) : null;
        break;
    }
  }

  return result;
}
