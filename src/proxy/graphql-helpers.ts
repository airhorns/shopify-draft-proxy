import {
  Kind,
  parse,
  type ASTNode,
  type FieldNode,
  type FragmentDefinitionNode,
  type SelectionNode,
  type ValueNode,
} from 'graphql';

import { getFieldArguments } from '../graphql/root-field.js';

export type SelectedFieldOptions = {
  includeInlineFragments?: boolean;
};

export type FragmentMap = Map<string, FragmentDefinitionNode>;

export type GraphqlErrorLocation = { line: number; column: number };

export type ProjectGraphqlFieldProjection = { handled: true; value: unknown } | { handled: false };

export type ProjectGraphqlValueOptions = {
  shouldApplyTypeCondition?: (source: Record<string, unknown>, typeCondition: string | undefined) => boolean;
  projectFieldValue?: (context: {
    source: Record<string, unknown>;
    field: FieldNode;
    fieldName: string;
    responseKey: string;
    fragments: FragmentMap;
  }) => ProjectGraphqlFieldProjection;
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
  serializeNode: (item: T, field: FieldNode, index: number, context: { path: string[] }) => unknown;
  serializePageInfo?: (field: FieldNode) => Record<string, unknown> | undefined;
  serializeUnknownField?: (field: FieldNode) => unknown;
  selectedFieldOptions?: SelectedFieldOptions;
  pageInfoOptions?: ConnectionPageInfoOptions;
};

export function getFieldResponseKey(field: FieldNode): string {
  return field.alias?.value ?? field.name.value;
}

export function resolveGraphQLValueNode(node: ValueNode, variables: Record<string, unknown>): unknown {
  switch (node.kind) {
    case Kind.NULL:
      return null;
    case Kind.STRING:
    case Kind.ENUM:
    case Kind.BOOLEAN:
      return node.value;
    case Kind.INT:
      return Number.parseInt(node.value, 10);
    case Kind.FLOAT:
      return Number.parseFloat(node.value);
    case Kind.LIST:
      return node.values.map((value) => resolveGraphQLValueNode(value, variables));
    case Kind.OBJECT:
      return Object.fromEntries(
        node.fields.map((field) => [field.name.value, resolveGraphQLValueNode(field.value, variables)]),
      );
    case Kind.VARIABLE:
      return variables[node.name.value] ?? null;
  }
}

export function readIdempotencyKey(field: FieldNode, variables: Record<string, unknown>): string | null {
  const directive = field.directives?.find((candidate) => candidate.name.value === 'idempotent') ?? null;
  const keyArgument =
    directive?.arguments?.find((argument) => argument.name.value === 'key') ??
    directive?.arguments?.find((argument) => argument.name.value === 'idempotencyKey') ??
    null;
  if (!keyArgument) {
    return null;
  }

  const key = resolveGraphQLValueNode(keyArgument.value, variables);
  return typeof key === 'string' && key.trim().length > 0 ? key : null;
}

export function buildMissingIdempotencyKeyError(field: FieldNode): Record<string, unknown> {
  return {
    message: 'The @idempotent directive is required for this mutation but was not provided.',
    ...(field.loc
      ? {
          locations: [
            {
              line: field.loc.startToken.line,
              column: field.loc.startToken.column,
            },
          ],
        }
      : {}),
    path: [getFieldResponseKey(field)],
    extensions: {
      code: 'BAD_REQUEST',
    },
  };
}

export function getNodeLocation(node: ASTNode): GraphqlErrorLocation[] {
  const token = node.loc?.startToken;
  return token ? [{ line: token.line, column: token.column }] : [];
}

export function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function readStringValue(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

export function readNumberValue(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

export function readBooleanValue(value: unknown): boolean | null {
  return typeof value === 'boolean' ? value : null;
}

export function readPlainObjectArray(value: unknown): Record<string, unknown>[] {
  return Array.isArray(value) ? value.filter(isPlainObject) : [];
}

export function defaultGraphqlTypeConditionApplies(
  source: Record<string, unknown>,
  typeCondition: string | undefined,
): boolean {
  if (!typeCondition) {
    return true;
  }

  const sourceTypename = typeof source['__typename'] === 'string' ? source['__typename'] : null;
  return !sourceTypename || sourceTypename === typeCondition;
}

export function projectGraphqlValue(
  value: unknown,
  selections: readonly SelectionNode[],
  fragments: FragmentMap,
  options: ProjectGraphqlValueOptions = {},
): unknown {
  if (Array.isArray(value)) {
    return value.map((item) => projectGraphqlValue(item, selections, fragments, options));
  }

  if (!isPlainObject(value)) {
    return value ?? null;
  }

  return projectGraphqlObject(value, selections, fragments, options);
}

export function projectGraphqlObject(
  source: Record<string, unknown>,
  selections: readonly SelectionNode[],
  fragments: FragmentMap,
  options: ProjectGraphqlValueOptions = {},
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const shouldApplyTypeCondition = options.shouldApplyTypeCondition ?? defaultGraphqlTypeConditionApplies;

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeCondition = selection.typeCondition?.name.value;
      if (shouldApplyTypeCondition(source, typeCondition)) {
        Object.assign(result, projectGraphqlObject(source, selection.selectionSet.selections, fragments, options));
      }
      continue;
    }

    if (selection.kind === Kind.FRAGMENT_SPREAD) {
      const fragment = fragments.get(selection.name.value);
      if (fragment && shouldApplyTypeCondition(source, fragment.typeCondition.name.value)) {
        Object.assign(result, projectGraphqlObject(source, fragment.selectionSet.selections, fragments, options));
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const fieldName = selection.name.value;
    const key = getFieldResponseKey(selection);
    if (fieldName === '__typename') {
      result[key] = source['__typename'] ?? null;
      continue;
    }

    const projectedField = options.projectFieldValue?.({
      source,
      field: selection,
      fieldName,
      responseKey: key,
      fragments,
    });
    if (projectedField?.handled === true) {
      result[key] = projectedField.value;
      continue;
    }

    let value = source[fieldName];
    if (fieldName === 'nodes' && value === undefined && Array.isArray(source['edges'])) {
      value = source['edges']
        .filter((edge): edge is Record<string, unknown> => isPlainObject(edge))
        .map((edge) => edge['node'] ?? null);
    }

    result[key] = selection.selectionSet
      ? projectGraphqlValue(value, selection.selectionSet.selections, fragments, options)
      : (value ?? null);
  }

  return result;
}

export function getDocumentFragments(document: string): FragmentMap {
  const ast = parse(document);
  return new Map(
    ast.definitions
      .filter((definition): definition is FragmentDefinitionNode => definition.kind === Kind.FRAGMENT_DEFINITION)
      .map((definition) => [definition.name.value, definition]),
  );
}

export function getVariableDefinitionLocation(document: string, variableName: string): GraphqlErrorLocation[] {
  const ast = parse(document);
  for (const definition of ast.definitions) {
    if (definition.kind !== Kind.OPERATION_DEFINITION) {
      continue;
    }

    const variableDefinition = definition.variableDefinitions?.find(
      (candidate) => candidate.variable.name.value === variableName,
    );
    if (variableDefinition) {
      return getNodeLocation(variableDefinition);
    }
  }

  return [];
}

export function readGraphqlDataResponsePayload(upstreamPayload: unknown, responseKey: string): unknown {
  if (!isPlainObject(upstreamPayload) || !isPlainObject(upstreamPayload['data'])) {
    return null;
  }

  return upstreamPayload['data'][responseKey] ?? null;
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
  getCursorValue: (item: T, index: number) => string,
  options: ConnectionWindowOptions = {},
): ConnectionWindow<T> {
  const args = getFieldArguments(field, variables);
  const first = readConnectionSizeArgument(args['first']);
  const last = readConnectionSizeArgument(args['last']);
  const parseCursor = options.parseCursor ?? readConnectionCursor;
  const after = typeof args['after'] === 'string' ? parseCursor(args['after']) : readConnectionCursor(args['after']);
  const before =
    typeof args['before'] === 'string' ? parseCursor(args['before']) : readConnectionCursor(args['before']);

  const startIndex = after === null ? 0 : items.findIndex((item, index) => getCursorValue(item, index) === after) + 1;
  const beforeIndex =
    before === null ? items.length : items.findIndex((item, index) => getCursorValue(item, index) === before);
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
    serializePageInfo,
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
        result[key] = items.map((item, index) => serializeNode(item, selection, index, { path: [key, String(index)] }));
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
                edge[edgeKey] = serializeNode(item, edgeSelection, index, { path: [key, String(index), edgeKey] });
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
        result[key] =
          serializePageInfo?.(selection) ??
          serializeConnectionPageInfo(selection, items, hasNextPage, hasPreviousPage, getCursorValue, {
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
