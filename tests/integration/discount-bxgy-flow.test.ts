import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';
import type { CollectionRecord, ProductRecord, ProductVariantRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const productId = 'gid://shopify/Product/195001';
const variantId = 'gid://shopify/ProductVariant/195101';
const getsProductId = 'gid://shopify/Product/195002';
const collectionId = 'gid://shopify/Collection/195201';

function product(id: string, title: string): ProductRecord {
  return {
    id,
    legacyResourceId: id.split('/').at(-1) ?? null,
    title,
    handle: title.toLowerCase().replaceAll(' ', '-'),
    status: 'ACTIVE',
    publicationIds: [],
    createdAt: '2026-04-25T00:00:00Z',
    updatedAt: '2026-04-25T00:00:00Z',
    vendor: null,
    productType: null,
    tags: [],
    totalInventory: null,
    tracksInventory: null,
    descriptionHtml: null,
    onlineStorePreviewUrl: null,
    templateSuffix: null,
    seo: {
      title: null,
      description: null,
    },
    category: null,
  };
}

function variant(id: string, ownerProductId: string, title: string): ProductVariantRecord {
  return {
    id,
    productId: ownerProductId,
    title,
    sku: null,
    barcode: null,
    price: null,
    compareAtPrice: null,
    taxable: null,
    inventoryPolicy: null,
    inventoryQuantity: null,
    selectedOptions: [],
    inventoryItem: null,
  };
}

function collection(id: string, title: string): CollectionRecord {
  return {
    id,
    legacyResourceId: id.split('/').at(-1) ?? null,
    title,
    handle: title.toLowerCase().replaceAll(' ', '-'),
    publicationIds: [],
  };
}

function seedLinkedResources(): void {
  store.upsertBaseProducts([product(productId, 'HAR-195 Buy Product'), product(getsProductId, 'HAR-195 Get Product')]);
  store.replaceBaseVariantsForProduct(productId, [variant(variantId, productId, 'HAR-195 Buy Variant')]);
  store.upsertBaseCollections([collection(collectionId, 'HAR-195 Get Collection')]);
}

const linkedItemsSelection = `#graphql
  __typename
  ... on DiscountProducts {
    products(first: 5) {
      nodes {
        id
        title
      }
    }
    productVariants(first: 5) {
      nodes {
        id
        title
      }
    }
  }
  ... on DiscountCollections {
    collections(first: 5) {
      nodes {
        id
        title
      }
    }
  }
  ... on AllDiscountItems {
    allItems
  }
`;

const codeBxgySelection = `#graphql
  id
  codeDiscount {
    __typename
    ... on DiscountCodeBxgy {
      title
      status
      summary
      startsAt
      endsAt
      createdAt
      updatedAt
      asyncUsageCount
      discountClasses
      usageLimit
      usesPerOrderLimit
      combinesWith {
        productDiscounts
        orderDiscounts
        shippingDiscounts
      }
      codes(first: 2) {
        nodes {
          id
          code
          asyncUsageCount
        }
      }
      context {
        __typename
        ... on DiscountBuyerSelectionAll {
          all
        }
      }
      customerBuys {
        value {
          __typename
          ... on DiscountQuantity {
            quantity
          }
        }
        items {
          ${linkedItemsSelection}
        }
      }
      customerGets {
        value {
          __typename
          ... on DiscountOnQuantity {
            quantity {
              quantity
            }
            effect {
              __typename
              ... on DiscountPercentage {
                percentage
              }
              ... on DiscountAmount {
                amount {
                  amount
                  currencyCode
                }
                appliesOnEachItem
              }
            }
          }
        }
        items {
          ${linkedItemsSelection}
        }
        appliesOnOneTimePurchase
        appliesOnSubscription
      }
    }
  }
`;

const automaticBxgySelection = `#graphql
  id
  automaticDiscount {
    __typename
    ... on DiscountAutomaticBxgy {
      title
      status
      summary
      startsAt
      endsAt
      createdAt
      updatedAt
      asyncUsageCount
      discountClasses
      usesPerOrderLimit
      combinesWith {
        productDiscounts
        orderDiscounts
        shippingDiscounts
      }
      context {
        __typename
        ... on DiscountBuyerSelectionAll {
          all
        }
      }
      customerBuys {
        value {
          __typename
          ... on DiscountQuantity {
            quantity
          }
        }
        items {
          ${linkedItemsSelection}
        }
      }
      customerGets {
        value {
          __typename
          ... on DiscountOnQuantity {
            quantity {
              quantity
            }
            effect {
              __typename
              ... on DiscountPercentage {
                percentage
              }
            }
          }
        }
        items {
          ${linkedItemsSelection}
        }
        appliesOnOneTimePurchase
        appliesOnSubscription
      }
    }
  }
`;

function codeBxgyInput(code: string): Record<string, unknown> {
  return {
    title: 'HAR-195 code BXGY',
    code,
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    customerBuys: {
      value: {
        quantity: '2',
      },
      items: {
        products: {
          productsToAdd: [productId],
          productVariantsToAdd: [variantId],
        },
      },
    },
    customerGets: {
      value: {
        discountOnQuantity: {
          quantity: '1',
          effect: {
            percentage: 1,
          },
        },
      },
      items: {
        collections: {
          add: [collectionId],
        },
      },
      appliesOnOneTimePurchase: true,
      appliesOnSubscription: false,
    },
    usesPerOrderLimit: 1,
  };
}

function automaticBxgyInput(): Record<string, unknown> {
  return {
    title: 'HAR-195 automatic BXGY',
    startsAt: '2026-04-25T00:00:00Z',
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    customerBuys: {
      value: {
        quantity: '1',
      },
      items: {
        collections: {
          add: [collectionId],
        },
      },
    },
    customerGets: {
      value: {
        discountOnQuantity: {
          quantity: '1',
          effect: {
            percentage: 0.5,
          },
        },
      },
      items: {
        products: {
          productsToAdd: [getsProductId],
        },
      },
    },
    usesPerOrderLimit: 1,
  };
}

describe('BXGY discount lifecycle staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    seedLinkedResources();
    vi.restoreAllMocks();
  });

  it('stages code BXGY create, update, lifecycle status changes, deletion, and downstream reads locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('code BXGY discount flow should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const create = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation CreateCodeBxgy($input: DiscountCodeBxgyInput!) {
            discountCodeBxgyCreate(bxgyCodeDiscount: $input) {
              codeDiscountNode {
                ${codeBxgySelection}
              }
              userErrors {
                field
                message
                code
                extraInfo
              }
            }
          }
        `,
        variables: {
          input: codeBxgyInput('HAR195BXGY'),
        },
      });

    expect(create.status).toBe(200);
    expect(create.body.data.discountCodeBxgyCreate.userErrors).toEqual([]);
    const discountId = create.body.data.discountCodeBxgyCreate.codeDiscountNode.id as string;
    expect(discountId).toMatch(/^gid:\/\/shopify\/DiscountCodeNode\/[0-9]+\?shopify-draft-proxy=synthetic$/u);
    expect(create.body.data.discountCodeBxgyCreate.codeDiscountNode.codeDiscount).toMatchObject({
      __typename: 'DiscountCodeBxgy',
      title: 'HAR-195 code BXGY',
      status: 'ACTIVE',
      discountClasses: ['PRODUCT'],
      usageLimit: null,
      usesPerOrderLimit: 1,
      codes: {
        nodes: [
          {
            code: 'HAR195BXGY',
            asyncUsageCount: 0,
          },
        ],
      },
      customerBuys: {
        value: {
          __typename: 'DiscountQuantity',
          quantity: '2',
        },
        items: {
          __typename: 'DiscountProducts',
          products: {
            nodes: [
              {
                id: productId,
                title: 'HAR-195 Buy Product',
              },
            ],
          },
          productVariants: {
            nodes: [
              {
                id: variantId,
                title: 'HAR-195 Buy Variant',
              },
            ],
          },
        },
      },
      customerGets: {
        value: {
          __typename: 'DiscountOnQuantity',
          quantity: {
            quantity: '1',
          },
          effect: {
            __typename: 'DiscountPercentage',
            percentage: 1,
          },
        },
        items: {
          __typename: 'DiscountCollections',
          collections: {
            nodes: [
              {
                id: collectionId,
                title: 'HAR-195 Get Collection',
              },
            ],
          },
        },
        appliesOnOneTimePurchase: true,
        appliesOnSubscription: false,
      },
    });

    const readAfterCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query ReadCodeBxgy($id: ID!, $code: String!) {
            discountNode(id: $id) {
              id
              discount {
                __typename
                ... on DiscountCodeBxgy {
                  title
                  status
                  customerGets {
                    items {
                      ${linkedItemsSelection}
                    }
                  }
                }
              }
            }
            codeDiscountNodeByCode(code: $code) {
              id
            }
            discountNodes(first: 5, query: "discount_type:bogo") {
              nodes {
                id
                discount {
                  __typename
                  ... on DiscountCodeBxgy {
                    title
                  }
                }
              }
            }
          }
        `,
        variables: {
          id: discountId,
          code: 'HAR195BXGY',
        },
      });

    expect(readAfterCreate.body.data.discountNode.discount.__typename).toBe('DiscountCodeBxgy');
    expect(readAfterCreate.body.data.discountNode.discount.customerGets.items.collections.nodes).toEqual([
      {
        id: collectionId,
        title: 'HAR-195 Get Collection',
      },
    ]);
    expect(readAfterCreate.body.data.codeDiscountNodeByCode).toEqual({ id: discountId });
    expect(readAfterCreate.body.data.discountNodes.nodes).toEqual([
      {
        id: discountId,
        discount: {
          __typename: 'DiscountCodeBxgy',
          title: 'HAR-195 code BXGY',
        },
      },
    ]);

    const update = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UpdateCodeBxgy($id: ID!, $input: DiscountCodeBxgyInput!) {
            discountCodeBxgyUpdate(id: $id, bxgyCodeDiscount: $input) {
              codeDiscountNode {
                ${codeBxgySelection}
              }
              userErrors {
                field
                message
                code
                extraInfo
              }
            }
          }
        `,
        variables: {
          id: discountId,
          input: {
            ...codeBxgyInput('HAR195BXGY2'),
            title: 'HAR-195 code BXGY updated',
            customerGets: {
              value: {
                discountOnQuantity: {
                  quantity: '2',
                  effect: {
                    percentage: 0.5,
                  },
                },
              },
              items: {
                products: {
                  productsToAdd: [getsProductId],
                },
              },
            },
          },
        },
      });

    expect(update.status).toBe(200);
    expect(update.body.data.discountCodeBxgyUpdate.userErrors).toEqual([]);
    expect(update.body.data.discountCodeBxgyUpdate.codeDiscountNode.id).toBe(discountId);
    expect(update.body.data.discountCodeBxgyUpdate.codeDiscountNode.codeDiscount).toMatchObject({
      title: 'HAR-195 code BXGY updated',
      customerGets: {
        value: {
          quantity: {
            quantity: '2',
          },
          effect: {
            percentage: 0.5,
          },
        },
        items: {
          __typename: 'DiscountProducts',
          products: {
            nodes: [
              {
                id: getsProductId,
                title: 'HAR-195 Get Product',
              },
            ],
          },
        },
      },
    });

    const deactivate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeactivateCodeBxgy($id: ID!) {
            discountCodeDeactivate(id: $id) {
              codeDiscountNode {
                id
                codeDiscount {
                  __typename
                  ... on DiscountCodeBxgy {
                    status
                  }
                }
              }
              userErrors { field message code extraInfo }
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(deactivate.body.data.discountCodeDeactivate.codeDiscountNode.codeDiscount).toEqual({
      __typename: 'DiscountCodeBxgy',
      status: 'EXPIRED',
    });

    const activate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation ActivateCodeBxgy($id: ID!) {
            discountCodeActivate(id: $id) {
              codeDiscountNode {
                id
                codeDiscount {
                  __typename
                  ... on DiscountCodeBxgy {
                    status
                  }
                }
              }
              userErrors { field message code extraInfo }
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(activate.body.data.discountCodeActivate.codeDiscountNode.codeDiscount).toEqual({
      __typename: 'DiscountCodeBxgy',
      status: 'ACTIVE',
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeleteCodeBxgy($id: ID!) {
            discountCodeDelete(id: $id) {
              deletedCodeDiscountId
              userErrors { field message code extraInfo }
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(deleteResponse.body.data.discountCodeDelete).toEqual({
      deletedCodeDiscountId: discountId,
      userErrors: [],
    });
    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages automatic BXGY create, update, lifecycle status changes, deletion, and downstream reads locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('automatic BXGY discount flow should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const create = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation CreateAutomaticBxgy($input: DiscountAutomaticBxgyInput!) {
            discountAutomaticBxgyCreate(automaticBxgyDiscount: $input) {
              automaticDiscountNode {
                ${automaticBxgySelection}
              }
              userErrors { field message code extraInfo }
            }
          }
        `,
        variables: {
          input: automaticBxgyInput(),
        },
      });

    expect(create.status).toBe(200);
    expect(create.body.data.discountAutomaticBxgyCreate.userErrors).toEqual([]);
    const discountId = create.body.data.discountAutomaticBxgyCreate.automaticDiscountNode.id as string;
    expect(discountId).toMatch(/^gid:\/\/shopify\/DiscountAutomaticNode\/[0-9]+\?shopify-draft-proxy=synthetic$/u);
    expect(create.body.data.discountAutomaticBxgyCreate.automaticDiscountNode.automaticDiscount).toMatchObject({
      __typename: 'DiscountAutomaticBxgy',
      title: 'HAR-195 automatic BXGY',
      status: 'ACTIVE',
      discountClasses: ['PRODUCT'],
      customerBuys: {
        items: {
          __typename: 'DiscountCollections',
          collections: {
            nodes: [
              {
                id: collectionId,
                title: 'HAR-195 Get Collection',
              },
            ],
          },
        },
      },
      customerGets: {
        value: {
          __typename: 'DiscountOnQuantity',
          effect: {
            __typename: 'DiscountPercentage',
            percentage: 0.5,
          },
        },
        items: {
          __typename: 'DiscountProducts',
          products: {
            nodes: [
              {
                id: getsProductId,
                title: 'HAR-195 Get Product',
              },
            ],
          },
        },
      },
    });

    const readAfterCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query AutomaticBxgyRead($id: ID!) {
            automaticDiscountNode(id: $id) {
              id
              automaticDiscount {
                __typename
                ... on DiscountAutomaticBxgy {
                  title
                  status
                }
              }
            }
            automaticDiscountNodes(first: 5, query: "discount_type:bogo") {
              nodes {
                id
                automaticDiscount {
                  __typename
                  ... on DiscountAutomaticBxgy {
                    title
                    status
                  }
                }
              }
            }
            discountNodesCount(query: "method:automatic discount_type:bogo") {
              count
              precision
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(readAfterCreate.body.data.automaticDiscountNode.id).toBe(discountId);
    expect(readAfterCreate.body.data.automaticDiscountNodes.nodes).toEqual([
      {
        id: discountId,
        automaticDiscount: {
          __typename: 'DiscountAutomaticBxgy',
          title: 'HAR-195 automatic BXGY',
          status: 'ACTIVE',
        },
      },
    ]);
    expect(readAfterCreate.body.data.discountNodesCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });

    const update = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UpdateAutomaticBxgy($id: ID!, $input: DiscountAutomaticBxgyInput!) {
            discountAutomaticBxgyUpdate(id: $id, automaticBxgyDiscount: $input) {
              automaticDiscountNode {
                ${automaticBxgySelection}
              }
              userErrors { field message code extraInfo }
            }
          }
        `,
        variables: {
          id: discountId,
          input: {
            ...automaticBxgyInput(),
            title: 'HAR-195 automatic BXGY updated',
            customerBuys: {
              value: {
                quantity: '3',
              },
              items: {
                products: {
                  productsToAdd: [productId],
                },
              },
            },
          },
        },
      });

    expect(update.body.data.discountAutomaticBxgyUpdate.userErrors).toEqual([]);
    expect(update.body.data.discountAutomaticBxgyUpdate.automaticDiscountNode.automaticDiscount).toMatchObject({
      title: 'HAR-195 automatic BXGY updated',
      customerBuys: {
        value: {
          quantity: '3',
        },
        items: {
          __typename: 'DiscountProducts',
          products: {
            nodes: [
              {
                id: productId,
                title: 'HAR-195 Buy Product',
              },
            ],
          },
        },
      },
    });

    const deactivate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeactivateAutomaticBxgy($id: ID!) {
            discountAutomaticDeactivate(id: $id) {
              automaticDiscountNode {
                id
                automaticDiscount {
                  __typename
                  ... on DiscountAutomaticBxgy {
                    status
                  }
                }
              }
              userErrors { field message code extraInfo }
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(deactivate.body.data.discountAutomaticDeactivate.automaticDiscountNode.automaticDiscount).toEqual({
      __typename: 'DiscountAutomaticBxgy',
      status: 'EXPIRED',
    });

    const activate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation ActivateAutomaticBxgy($id: ID!) {
            discountAutomaticActivate(id: $id) {
              automaticDiscountNode {
                id
                automaticDiscount {
                  __typename
                  ... on DiscountAutomaticBxgy {
                    status
                  }
                }
              }
              userErrors { field message code extraInfo }
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(activate.body.data.discountAutomaticActivate.automaticDiscountNode.automaticDiscount).toEqual({
      __typename: 'DiscountAutomaticBxgy',
      status: 'ACTIVE',
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeleteAutomaticBxgy($id: ID!) {
            discountAutomaticDelete(id: $id) {
              deletedAutomaticDiscountId
              userErrors { field message code extraInfo }
            }
          }
        `,
        variables: {
          id: discountId,
        },
      });

    expect(deleteResponse.body.data.discountAutomaticDelete).toEqual({
      deletedAutomaticDiscountId: discountId,
      userErrors: [],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns local userErrors for invalid BXGY product, variant, and collection references', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid BXGY references should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation InvalidBxgyRefs($codeInput: DiscountCodeBxgyInput!, $automaticInput: DiscountAutomaticBxgyInput!) {
            code: discountCodeBxgyCreate(bxgyCodeDiscount: $codeInput) {
              codeDiscountNode { id }
              userErrors { field message code extraInfo }
            }
            automatic: discountAutomaticBxgyCreate(automaticBxgyDiscount: $automaticInput) {
              automaticDiscountNode { id }
              userErrors { field message code extraInfo }
            }
          }
        `,
        variables: {
          codeInput: {
            ...codeBxgyInput('HAR195BADREF'),
            customerBuys: {
              value: {
                quantity: '1',
              },
              items: {
                products: {
                  productsToAdd: ['gid://shopify/Product/0'],
                  productVariantsToAdd: ['gid://shopify/ProductVariant/0'],
                },
              },
            },
          },
          automaticInput: {
            ...automaticBxgyInput(),
            customerGets: {
              value: {
                discountOnQuantity: {
                  quantity: '1',
                  effect: {
                    percentage: 1,
                  },
                },
              },
              items: {
                collections: {
                  add: ['gid://shopify/Collection/0'],
                },
              },
            },
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.code).toEqual({
      codeDiscountNode: null,
      userErrors: [
        {
          field: ['bxgyCodeDiscount', 'customerBuys', 'items', 'products', 'productsToAdd'],
          message: 'Product with id: 0 is invalid',
          code: 'INVALID',
          extraInfo: null,
        },
        {
          field: ['bxgyCodeDiscount', 'customerBuys', 'items', 'products', 'productVariantsToAdd'],
          message: 'Product variant with id: 0 is invalid',
          code: 'INVALID',
          extraInfo: null,
        },
      ],
    });
    expect(response.body.data.automatic).toEqual({
      automaticDiscountNode: null,
      userErrors: [
        {
          field: ['automaticBxgyDiscount', 'customerGets', 'items', 'collections', 'add'],
          message: 'Collection with id: 0 is invalid',
          code: 'INVALID',
          extraInfo: null,
        },
      ],
    });
    expect(store.listEffectiveDiscounts()).toEqual([]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
