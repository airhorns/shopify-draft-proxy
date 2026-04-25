import { describe, expect, it } from 'vitest';

import { compareNullableStrings, compareShopifyResourceIds } from '../../src/shopify/resource-ids.js';

describe('Shopify resource id helpers', () => {
  it('sorts numeric GID tails numerically with lexical fallback', () => {
    expect(
      ['gid://shopify/DiscountNode/10', 'gid://shopify/DiscountNode/2', 'gid://shopify/DiscountNode/custom'].sort(
        compareShopifyResourceIds,
      ),
    ).toEqual(['gid://shopify/DiscountNode/2', 'gid://shopify/DiscountNode/10', 'gid://shopify/DiscountNode/custom']);
  });

  it('orders nullable strings with populated values first', () => {
    expect(['2026-01-02', null, '2026-01-01'].sort(compareNullableStrings)).toEqual(['2026-01-01', '2026-01-02', null]);
  });
});
