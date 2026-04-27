import { Kind, parse, type FieldNode } from 'graphql';
import { describe, expect, it } from 'vitest';

import {
  getFieldResponseKey,
  getSelectedChildFields,
  paginateConnectionItems,
  readNullableIntArgument,
  readNullableStringArgument,
  serializeConnection,
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

  it('paginates backward windows before the selected cursor', () => {
    const products = getRootField(`
      query($before: String) {
        products(last: 2, before: $before) {
          nodes {
            id
          }
        }
      }
    `);
    const items = [{ id: 'a' }, { id: 'b' }, { id: 'c' }, { id: 'd' }];

    expect(paginateConnectionItems(items, products, { before: 'cursor:d' }, (item) => item.id)).toEqual({
      items: [{ id: 'b' }, { id: 'c' }],
      hasNextPage: true,
      hasPreviousPage: true,
    });
  });

  it('can preserve raw upstream cursor values during pagination', () => {
    const customers = getRootField(`
      query($after: String) {
        customers(first: 1, after: $after) {
          nodes {
            id
          }
        }
      }
    `);
    const items = [{ cursor: 'cursor:gid://shopify/Customer/1' }, { cursor: 'cursor:gid://shopify/Customer/2' }];

    expect(
      paginateConnectionItems(items, customers, { after: 'cursor:gid://shopify/Customer/1' }, (item) => item.cursor, {
        parseCursor: (raw) => raw,
      }),
    ).toEqual({
      items: [{ cursor: 'cursor:gid://shopify/Customer/2' }],
      hasNextPage: false,
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

  it('serializes selected connection nodes, edges, and pageInfo consistently', () => {
    const products = getRootField(`
      query {
        products(first: 2) {
          productNodes: nodes {
            productId: id
          }
          productEdges: edges {
            cursor
            productNode: node {
              productId: id
            }
            customEdgeField
          }
          info: pageInfo {
            hasNextPage
            endCursor
          }
          customConnectionField
        }
      }
    `);

    expect(
      serializeConnection(products, {
        items: [{ id: 'gid://shopify/Product/1' }],
        hasNextPage: true,
        hasPreviousPage: false,
        getCursorValue: (item) => item.id,
        serializeNode: (item, selection) =>
          Object.fromEntries(
            getSelectedChildFields(selection).map((childSelection) => [
              getFieldResponseKey(childSelection),
              childSelection.name.value === 'id' ? item.id : null,
            ]),
          ),
      }),
    ).toEqual({
      productNodes: [{ productId: 'gid://shopify/Product/1' }],
      productEdges: [
        {
          cursor: 'cursor:gid://shopify/Product/1',
          productNode: { productId: 'gid://shopify/Product/1' },
          customEdgeField: null,
        },
      ],
      info: {
        hasNextPage: true,
        endCursor: 'cursor:gid://shopify/Product/1',
      },
      customConnectionField: null,
    });
  });

  it('supports index cursors, unknown connection fields, and suppressed PageInfo cursors', () => {
    const shippingLines = getRootField(`
      query {
        shippingLines {
          edges {
            cursor
            node {
              title
            }
          }
          pageInfo {
            startCursor
            endCursor
          }
          totalCount
        }
      }
    `);

    expect(
      serializeConnection(shippingLines, {
        items: [{ title: 'Ground' }, { title: 'Express' }],
        hasNextPage: false,
        hasPreviousPage: false,
        getCursorValue: (_item, index) => `shipping-line:${index + 1}`,
        serializeNode: (item, selection) =>
          Object.fromEntries(
            getSelectedChildFields(selection).map((childSelection) => [
              getFieldResponseKey(childSelection),
              childSelection.name.value === 'title' ? item.title : null,
            ]),
          ),
        pageInfoOptions: {
          includeCursors: false,
        },
        serializeUnknownField: (selection) => (selection.name.value === 'totalCount' ? 2 : null),
      }),
    ).toEqual({
      edges: [
        {
          cursor: 'cursor:shipping-line:1',
          node: { title: 'Ground' },
        },
        {
          cursor: 'cursor:shipping-line:2',
          node: { title: 'Express' },
        },
      ],
      pageInfo: {
        startCursor: null,
        endCursor: null,
      },
      totalCount: 2,
    });
  });
});
