import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { DiscountRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function buildDiscount(overrides: Partial<DiscountRecord> = {}): DiscountRecord {
  return {
    id: 'gid://shopify/DiscountCodeNode/198001',
    typeName: 'DiscountCodeBasic',
    method: 'code',
    title: 'HAR-198 duplicate seed',
    status: 'ACTIVE',
    summary: '10% off entire order',
    startsAt: '2026-04-25T00:00:00Z',
    endsAt: null,
    createdAt: '2026-04-25T16:00:00Z',
    updatedAt: '2026-04-25T16:00:00Z',
    asyncUsageCount: 0,
    discountClasses: ['ORDER'],
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    codes: ['HAR198DUPLICATE'],
    ...overrides,
  };
}

function validBasicInput(code = 'HAR198NEW'): Record<string, unknown> {
  return {
    title: `HAR-198 ${code}`,
    code,
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    minimumRequirement: {
      subtotal: {
        greaterThanOrEqualToSubtotal: '1.00',
      },
    },
    customerGets: {
      value: {
        percentage: 0.1,
      },
      items: {
        all: true,
      },
    },
  };
}

describe('discount mutation validation', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('returns top-level GraphQL validation errors for missing and inline-null discount inputs', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('discount validation should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const missingVariable = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation MissingDiscountInput($input: DiscountCodeBasicInput!) {
          discountCodeBasicCreate(basicCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: {},
      });

    expect(missingVariable.status).toBe(200);
    expect(missingVariable.body).toEqual({
      errors: [
        {
          message: 'Variable $input of type DiscountCodeBasicInput! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: null,
            problems: [
              {
                path: [],
                explanation: 'Expected value to not be null',
              },
            ],
          },
        },
      ],
    });

    const inlineNull = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation InlineNullDiscountInput {
          discountCodeBasicCreate(basicCodeDiscount: null) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }`,
      });

    expect(inlineNull.status).toBe(200);
    expect(inlineNull.body.errors[0]).toMatchObject({
      message:
        "Argument 'basicCodeDiscount' on Field 'discountCodeBasicCreate' has an invalid value (null). Expected type 'DiscountCodeBasicInput!'.",
      path: ['mutation', 'discountCodeBasicCreate', 'basicCodeDiscount'],
      extensions: {
        code: 'argumentLiteralsIncompatible',
        typeName: 'Field',
        argumentName: 'basicCodeDiscount',
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns captured DiscountUserError branches without staging invalid discount creates', async () => {
    store.upsertBaseDiscounts([buildDiscount()]);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('discount validation should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DiscountValidation(
          $duplicate: DiscountCodeBasicInput!
          $badRefs: DiscountCodeBasicInput!
          $invalidAutomatic: DiscountAutomaticBasicInput!
          $blankBxgy: DiscountCodeBxgyInput!
          $invalidFreeShipping: DiscountCodeFreeShippingInput!
        ) {
          duplicate: discountCodeBasicCreate(basicCodeDiscount: $duplicate) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          badRefs: discountCodeBasicCreate(basicCodeDiscount: $badRefs) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          invalidAutomatic: discountAutomaticBasicCreate(automaticBasicDiscount: $invalidAutomatic) {
            automaticDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          blankBxgy: discountCodeBxgyCreate(bxgyCodeDiscount: $blankBxgy) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          invalidFreeShipping: discountCodeFreeShippingCreate(freeShippingCodeDiscount: $invalidFreeShipping) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: {
          duplicate: validBasicInput('HAR198DUPLICATE'),
          badRefs: {
            ...validBasicInput('HAR198BADREF'),
            customerGets: {
              value: { percentage: 0.1 },
              items: {
                products: {
                  productsToAdd: ['gid://shopify/Product/0'],
                  productVariantsToAdd: ['gid://shopify/ProductVariant/0'],
                },
                collections: {
                  add: ['gid://shopify/Collection/0'],
                },
              },
            },
          },
          invalidAutomatic: {
            title: 'HAR-198 invalid automatic dates',
            startsAt: '2026-04-25T00:00:00Z',
            endsAt: '2026-04-24T00:00:00Z',
            combinesWith: { productDiscounts: false, orderDiscounts: true, shippingDiscounts: false },
            context: { all: 'ALL' },
            minimumRequirement: { quantity: { greaterThanOrEqualToQuantity: '2' } },
            customerGets: { value: { percentage: 0.15 }, items: { all: true } },
          },
          blankBxgy: {
            title: '',
            code: 'HAR198BXGY',
            startsAt: '2026-04-25T00:00:00Z',
            combinesWith: { productDiscounts: true, orderDiscounts: true, shippingDiscounts: true },
            context: { all: 'ALL' },
            customerBuys: { value: { quantity: '1' }, items: { all: true } },
            customerGets: {
              value: { discountOnQuantity: { quantity: '1', effect: { percentage: 1.0 } } },
              items: { all: true },
            },
          },
          invalidFreeShipping: {
            title: '',
            code: 'HAR198FREE',
            startsAt: '2026-04-25T00:00:00Z',
            combinesWith: { productDiscounts: true, orderDiscounts: true, shippingDiscounts: true },
            context: { all: 'ALL' },
            destination: { all: true },
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.duplicate).toEqual({
      codeDiscountNode: null,
      userErrors: [
        {
          field: ['basicCodeDiscount', 'code'],
          message: 'Code must be unique. Please try a different code.',
          code: 'TAKEN',
          extraInfo: null,
        },
      ],
    });
    expect(response.body.data.badRefs.userErrors).toEqual([
      {
        field: ['basicCodeDiscount', 'customerGets', 'items', 'collections', 'add'],
        message: 'Cannot entitle collections in combination with product variants or products',
        code: 'CONFLICT',
        extraInfo: null,
      },
      {
        field: ['basicCodeDiscount', 'customerGets', 'items', 'products', 'productsToAdd'],
        message: 'Product with id: 0 is invalid',
        code: 'INVALID',
        extraInfo: null,
      },
      {
        field: ['basicCodeDiscount', 'customerGets', 'items', 'products', 'productVariantsToAdd'],
        message: 'Product variant with id: 0 is invalid',
        code: 'INVALID',
        extraInfo: null,
      },
    ]);
    expect(response.body.data.invalidAutomatic.userErrors).toEqual([
      {
        field: ['automaticBasicDiscount', 'endsAt'],
        message: 'Ends at needs to be after starts_at',
        code: 'INVALID',
        extraInfo: null,
      },
    ]);
    expect(response.body.data.blankBxgy.userErrors).toEqual([
      {
        field: ['bxgyCodeDiscount', 'customerGets'],
        message: "Items in 'customer get' cannot be set to all",
        code: 'INVALID',
        extraInfo: null,
      },
      {
        field: ['bxgyCodeDiscount', 'title'],
        message: "Title can't be blank",
        code: 'BLANK',
        extraInfo: null,
      },
      {
        field: ['bxgyCodeDiscount', 'customerBuys', 'items'],
        message: "Items in 'customer buys' must be defined",
        code: 'BLANK',
        extraInfo: null,
      },
    ]);
    expect(response.body.data.invalidFreeShipping.userErrors).toEqual([
      {
        field: ['freeShippingCodeDiscount', 'combinesWith'],
        message: 'The combinesWith settings are not valid for the discount class.',
        code: 'INVALID_COMBINES_WITH_FOR_DISCOUNT_CLASS',
        extraInfo: null,
      },
      {
        field: ['freeShippingCodeDiscount', 'title'],
        message: "Title can't be blank",
        code: 'BLANK',
        extraInfo: null,
      },
    ]);
    expect(store.listEffectiveDiscounts()).toHaveLength(1);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns captured invalid-id and bulk-selector userErrors for discount updates and bulk roots', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('discount validation should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DiscountBulkValidation(
          $id: ID!
          $input: DiscountCodeBasicInput!
          $codeIds: [ID!]
          $automaticIds: [ID!]
          $search: String
        ) {
          unknownUpdate: discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
            codeDiscountNode { id }
            userErrors { field message code extraInfo }
          }
          codeBulk: discountCodeBulkDeactivate(ids: $codeIds, search: $search) {
            job { id }
            userErrors { field message code extraInfo }
          }
          automaticBulk: discountAutomaticBulkDelete(ids: $automaticIds, search: $search) {
            job { id }
            userErrors { field message code extraInfo }
          }
        }`,
        variables: {
          id: 'gid://shopify/DiscountCodeNode/0',
          input: validBasicInput('HAR198UNKNOWN'),
          codeIds: ['gid://shopify/DiscountCodeNode/0'],
          automaticIds: ['gid://shopify/DiscountAutomaticNode/0'],
          search: 'status:active',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.unknownUpdate).toEqual({
      codeDiscountNode: null,
      userErrors: [
        {
          field: ['id'],
          message: 'Discount does not exist',
          code: null,
          extraInfo: null,
        },
      ],
    });
    expect(response.body.data.codeBulk).toEqual({
      job: null,
      userErrors: [
        {
          field: null,
          message: "Only one of 'ids', 'search' or 'saved_search_id' is allowed.",
          code: 'TOO_MANY_ARGUMENTS',
          extraInfo: null,
        },
      ],
    });
    expect(response.body.data.automaticBulk).toEqual({
      job: null,
      userErrors: [
        {
          field: null,
          message: 'Only one of IDs, search argument or saved search ID is allowed.',
          code: 'TOO_MANY_ARGUMENTS',
          extraInfo: null,
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
