import { Kind, parse, type FieldNode } from 'graphql';
import { describe, expect, it } from 'vitest';

import {
  getFieldResponseKey,
  getSelectedChildFields,
  paginateConnectionItems,
  readNullableIntArgument,
  readNullableStringArgument,
  serializeConnectionPageInfo,
  serializeEmptyConnectionPageInfo,
} from '../../src/proxy/graphql-helpers.js';

function getRootField(document: string): FieldNode {
  const parsed = parse(document);
  const operation = parsed.definitions.find((definition) => definition.kind === Kind.OPERATION_DEFINITION);
  if (!operation) {
    throw new Error('Missing operation definition');
  }

  const selection = operation.selectionSet.selections.find((candidate) => candidate.kind === Kind.FIELD);
  if (!selection || selection.kind !== Kind.FIELD) {
    throw new Error('Missing root field');
  }

  return selection;
}

function getChildField(field: FieldNode, name: string): FieldNode {
  const child = getSelectedChildFields(field, { includeInlineFragments: true }).find(
    (selection) => selection.name.value === name,
  );
  if (!child) {
    throw new Error(`Missing child field ${name}`);
  }

  return child;
}

describe('proxy GraphQL helpers', () => {
  it('reads aliases and selected child fields with optional inline fragments', () => {
    const product = getRootField(`
      query {
        product(id: "gid://shopify/Product/1") {
          productId: id
          ... on Product {
            title
          }
        }
      }
    `);

    expect(getFieldResponseKey(getSelectedChildFields(product)[0]!)).toBe('productId');
    expect(getSelectedChildFields(product).map((field) => field.name.value)).toEqual(['id']);
    expect(getSelectedChildFields(product, { includeInlineFragments: true }).map((field) => field.name.value)).toEqual([
      'id',
      'title',
    ]);
  });

  it('reads nullable literal and variable arguments with GraphQL semantics', () => {
    const products = getRootField(`
      query($first: Int, $after: String) {
        products(first: $first, after: $after, last: 2, before: "cursor:gid://shopify/Product/2") {
          nodes {
            id
          }
        }
      }
    `);

    expect(readNullableIntArgument(products, 'first', { first: 3 })).toBe(3);
    expect(readNullableIntArgument(products, 'last', {})).toBe(2);
    expect(readNullableStringArgument(products, 'after', { after: 'cursor:gid://shopify/Product/1' })).toBe(
      'cursor:gid://shopify/Product/1',
    );
    expect(readNullableStringArgument(products, 'before', {})).toBe('cursor:gid://shopify/Product/2');
    expect(readNullableIntArgument(products, 'missing', {})).toBeNull();
  });

  it('paginates connection items using Shopify-style cursor windows', () => {
    const products = getRootField(`
      query($after: String) {
        products(first: 1, after: $after) {
          nodes {
            id
          }
        }
      }
    `);
    const items = [{ id: 'a' }, { id: 'b' }, { id: 'c' }, { id: 'd' }];

    expect(paginateConnectionItems(items, products, { after: 'cursor:b' }, (item) => item.id)).toEqual({
      items: [{ id: 'c' }],
      hasNextPage: true,
      hasPreviousPage: true,
    });
  });

  it('serializes PageInfo aliases, fallback cursors, and cursor prefixing', () => {
    const products = getRootField(`
      query {
        products(first: 1) {
          pageInfo {
            next: hasNextPage
            previous: hasPreviousPage
            startCursor
            endCursor
            custom
          }
        }
      }
    `);
    const pageInfo = getChildField(products, 'pageInfo');

    expect(serializeConnectionPageInfo(pageInfo, [{ id: '1' }], true, false, (item) => item.id)).toEqual({
      next: true,
      previous: false,
      startCursor: 'cursor:1',
      endCursor: 'cursor:1',
      custom: null,
    });

    expect(
      serializeConnectionPageInfo(pageInfo, [], false, true, (item: { cursor: string }) => item.cursor, {
        prefixCursors: false,
        fallbackStartCursor: 'baseline-start',
        fallbackEndCursor: 'baseline-end',
      }),
    ).toEqual({
      next: false,
      previous: true,
      startCursor: 'baseline-start',
      endCursor: 'baseline-end',
      custom: null,
    });

    expect(serializeEmptyConnectionPageInfo(pageInfo)).toEqual({
      next: false,
      previous: false,
      startCursor: null,
      endCursor: null,
      custom: null,
    });
  });
});
