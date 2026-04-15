import { describe, expect, it } from 'vitest';
import { parseOperation } from '../../src/graphql/parse-operation.js';

describe('parseOperation', () => {
  it('classifies queries', () => {
    expect(parseOperation('query Products { products(first: 5) { nodes { id } } }')).toEqual({
      type: 'query',
      name: 'Products',
      rootFields: ['products'],
    });
  });

  it('classifies mutations', () => {
    expect(parseOperation('mutation ProductCreate { productCreate(product: { title: "Hat" }) { product { id } } }')).toEqual({
      type: 'mutation',
      name: 'ProductCreate',
      rootFields: ['productCreate'],
    });
  });
});
