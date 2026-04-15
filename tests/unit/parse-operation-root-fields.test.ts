import { describe, expect, it } from 'vitest';
import { parseOperation } from '../../src/graphql/parse-operation.js';

describe('parseOperation root fields', () => {
  it('captures root field names for anonymous mutations', () => {
    expect(
      parseOperation('mutation { productCreate(product: { title: "Hat" }) { product { id } } }'),
    ).toEqual({
      type: 'mutation',
      name: null,
      rootFields: ['productCreate'],
    });
  });

  it('captures root field names for anonymous queries', () => {
    expect(
      parseOperation('query { products(first: 5) { nodes { id } } }'),
    ).toEqual({
      type: 'query',
      name: null,
      rootFields: ['products'],
    });
  });
});
