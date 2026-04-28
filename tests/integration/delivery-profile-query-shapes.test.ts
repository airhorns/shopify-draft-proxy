import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';
import type {
  DeliveryProfileRecord,
  LocationRecord,
  ProductRecord,
  ProductVariantRecord,
} from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function makeProduct(id: string, title: string): ProductRecord {
  return {
    id,
    legacyResourceId: id.split('/').at(-1) ?? null,
    title,
    handle: title.toLowerCase().replace(/\s+/g, '-'),
    status: 'ACTIVE',
    publicationIds: [],
    createdAt: '2026-04-01T00:00:00.000Z',
    updatedAt: '2026-04-01T00:00:00.000Z',
    vendor: null,
    productType: null,
    tags: [],
    totalInventory: 0,
    tracksInventory: true,
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

function makeVariant(productId: string, id: string, title: string): ProductVariantRecord {
  return {
    id,
    productId,
    title,
    sku: null,
    barcode: null,
    price: null,
    compareAtPrice: null,
    taxable: null,
    inventoryPolicy: null,
    inventoryQuantity: 0,
    selectedOptions: [],
    inventoryItem: null,
  };
}

function makeLocation(id: string, name: string): LocationRecord {
  return {
    id,
    name,
    isActive: true,
    shipsInventory: true,
  };
}

function makeDeliveryProfileFixture(): DeliveryProfileRecord[] {
  return [
    {
      id: 'gid://shopify/DeliveryProfile/1',
      cursor: 'profile-cursor-1',
      name: 'General profile',
      default: true,
      merchantOwned: true,
      version: 3,
      activeMethodDefinitionsCount: 1,
      locationsWithoutRatesCount: 0,
      originLocationCount: 1,
      zoneCountryCount: 1,
      productVariantsCount: {
        count: 2,
        precision: 'EXACT',
      },
      sellingPlanGroups: [],
      profileItems: [
        {
          productId: 'gid://shopify/Product/1',
          variantIds: ['gid://shopify/ProductVariant/1', 'gid://shopify/ProductVariant/2'],
          cursor: 'profile-item-cursor-1',
          variantCursors: {
            'gid://shopify/ProductVariant/1': 'variant-cursor-1',
            'gid://shopify/ProductVariant/2': 'variant-cursor-2',
          },
        },
      ],
      profileLocationGroups: [
        {
          id: 'gid://shopify/DeliveryLocationGroup/1',
          locationIds: ['gid://shopify/Location/1'],
          countriesInAnyZone: [
            {
              zone: 'Domestic',
              country: {
                id: 'gid://shopify/DeliveryCountry/1',
                name: 'United States',
                translatedName: 'United States',
                code: {
                  countryCode: 'US',
                  restOfWorld: false,
                },
                provinces: [
                  {
                    id: 'gid://shopify/DeliveryProvince/1',
                    name: 'California',
                    code: 'CA',
                  },
                ],
              },
            },
          ],
          locationGroupZones: [
            {
              cursor: 'zone-cursor-domestic',
              zone: {
                id: 'gid://shopify/DeliveryZone/1',
                name: 'Domestic',
                countries: [
                  {
                    id: 'gid://shopify/DeliveryCountry/1',
                    name: 'United States',
                    translatedName: 'United States',
                    code: {
                      countryCode: 'US',
                      restOfWorld: false,
                    },
                    provinces: [
                      {
                        id: 'gid://shopify/DeliveryProvince/1',
                        name: 'California',
                        code: 'CA',
                      },
                    ],
                  },
                ],
              },
              methodDefinitions: [
                {
                  id: 'gid://shopify/DeliveryMethodDefinition/1',
                  cursor: 'method-cursor-standard',
                  name: 'Standard',
                  active: true,
                  description: null,
                  rateProvider: {
                    __typename: 'DeliveryRateDefinition',
                    id: 'gid://shopify/DeliveryRateDefinition/1',
                    price: {
                      amount: '5.00',
                      currencyCode: 'USD',
                    },
                  },
                  methodConditions: [
                    {
                      id: 'gid://shopify/DeliveryCondition/1?operator=greater_than_or_equal_to',
                      field: 'TOTAL_WEIGHT',
                      operator: 'GREATER_THAN_OR_EQUAL_TO',
                      conditionCriteria: {
                        __typename: 'Weight',
                        unit: 'KILOGRAMS',
                        value: 1,
                      },
                    },
                  ],
                },
              ],
            },
          ],
        },
      ],
      unassignedLocationIds: ['gid://shopify/Location/2'],
    },
    {
      id: 'gid://shopify/DeliveryProfile/2',
      cursor: 'profile-cursor-2',
      name: 'App-managed profile',
      default: false,
      merchantOwned: false,
      version: 1,
      activeMethodDefinitionsCount: 0,
      locationsWithoutRatesCount: 0,
      originLocationCount: 0,
      zoneCountryCount: 0,
      productVariantsCount: {
        count: 0,
        precision: 'EXACT',
      },
      sellingPlanGroups: [],
      profileItems: [],
      profileLocationGroups: [],
      unassignedLocationIds: [],
    },
  ];
}

describe('delivery profile query shapes', () => {
  beforeEach(() => {
    store.reset();
    vi.restoreAllMocks();
  });

  it('serves empty delivery profile reads in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('delivery profile snapshot reads must stay local');
    });

    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query DeliveryProfileEmptyRead($missingId: ID!) {
            deliveryProfiles(first: 2) {
              edges {
                cursor
                node {
                  id
                  name
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            deliveryProfile(id: $missingId) {
              id
              name
            }
          }
        `,
        variables: {
          missingId: 'gid://shopify/DeliveryProfile/999999999',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        deliveryProfiles: {
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        deliveryProfile: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('serves delivery profile catalog pagination and nested detail from normalized snapshot state', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('delivery profile snapshot reads must stay local');
    });

    const product = makeProduct('gid://shopify/Product/1', 'Snowboard');
    store.upsertBaseProducts([product]);
    store.replaceBaseVariantsForProduct(product.id, [
      makeVariant(product.id, 'gid://shopify/ProductVariant/1', 'Default Title'),
      makeVariant(product.id, 'gid://shopify/ProductVariant/2', 'Blue'),
    ]);
    store.upsertBaseLocations([
      makeLocation('gid://shopify/Location/1', 'Main Warehouse'),
      makeLocation('gid://shopify/Location/2', 'Overflow Warehouse'),
    ]);
    store.upsertBaseDeliveryProfiles(makeDeliveryProfileFixture());

    const app = createApp(config).callback();

    const firstPage = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query DeliveryProfileCatalog($first: Int!) {
            deliveryProfiles(first: $first) {
              edges {
                cursor
                node {
                  id
                  name
                  default
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
          }
        `,
        variables: { first: 1 },
      });

    expect(firstPage.status).toBe(200);
    expect(firstPage.body).toEqual({
      data: {
        deliveryProfiles: {
          edges: [
            {
              cursor: 'profile-cursor-1',
              node: {
                id: 'gid://shopify/DeliveryProfile/1',
                name: 'General profile',
                default: true,
              },
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: false,
            startCursor: 'profile-cursor-1',
            endCursor: 'profile-cursor-1',
          },
        },
      },
    });

    const secondPage = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query DeliveryProfileCatalog($after: String!) {
            deliveryProfiles(first: 2, after: $after) {
              nodes {
                id
                name
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
          }
        `,
        variables: { after: 'profile-cursor-1' },
      });

    expect(secondPage.body).toEqual({
      data: {
        deliveryProfiles: {
          nodes: [
            {
              id: 'gid://shopify/DeliveryProfile/2',
              name: 'App-managed profile',
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: true,
            startCursor: 'profile-cursor-2',
            endCursor: 'profile-cursor-2',
          },
        },
      },
    });

    const merchantOwnedReverse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query MerchantOwnedProfiles {
            deliveryProfiles(last: 2, reverse: true, merchantOwnedOnly: true) {
              nodes {
                id
                name
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
          }
        `,
      });

    expect(merchantOwnedReverse.body).toEqual({
      data: {
        deliveryProfiles: {
          nodes: [
            {
              id: 'gid://shopify/DeliveryProfile/1',
              name: 'General profile',
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'profile-cursor-1',
            endCursor: 'profile-cursor-1',
          },
        },
      },
    });

    const detail = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query DeliveryProfileDetail($id: ID!) {
            deliveryProfile(id: $id) {
              id
              name
              default
              version
              activeMethodDefinitionsCount
              locationsWithoutRatesCount
              originLocationCount
              zoneCountryCount
              productVariantsCount {
                count
                precision
              }
              profileItems(first: 1) {
                nodes {
                  product {
                    id
                    title
                  }
                  variants(first: 1) {
                    nodes {
                      id
                      title
                      product {
                        id
                      }
                    }
                    pageInfo {
                      hasNextPage
                      hasPreviousPage
                      startCursor
                      endCursor
                    }
                  }
                }
                pageInfo {
                  hasNextPage
                  hasPreviousPage
                  startCursor
                  endCursor
                }
              }
              profileLocationGroups {
                countriesInAnyZone {
                  zone
                  country {
                    id
                    name
                    code {
                      countryCode
                      restOfWorld
                    }
                    provinces {
                      id
                      code
                    }
                  }
                }
                locationGroup {
                  id
                  locations(first: 1) {
                    nodes {
                      id
                      name
                    }
                    pageInfo {
                      hasNextPage
                      hasPreviousPage
                      startCursor
                      endCursor
                    }
                  }
                  locationsCount {
                    count
                    precision
                  }
                }
                locationGroupZones(first: 1) {
                  nodes {
                    zone {
                      id
                      name
                      countries {
                        id
                        code {
                          countryCode
                          restOfWorld
                        }
                      }
                    }
                    methodDefinitions(first: 1) {
                      nodes {
                        id
                        name
                        active
                        description
                        rateProvider {
                          ... on DeliveryRateDefinition {
                            id
                            price {
                              amount
                              currencyCode
                            }
                          }
                        }
                        methodConditions {
                          id
                          field
                          operator
                          conditionCriteria {
                            __typename
                            ... on Weight {
                              unit
                              value
                            }
                          }
                        }
                      }
                      pageInfo {
                        hasNextPage
                        hasPreviousPage
                        startCursor
                        endCursor
                      }
                    }
                  }
                  pageInfo {
                    hasNextPage
                    hasPreviousPage
                    startCursor
                    endCursor
                  }
                }
              }
              unassignedLocationsPaginated(first: 1) {
                nodes {
                  id
                  name
                }
                pageInfo {
                  hasNextPage
                  hasPreviousPage
                  startCursor
                  endCursor
                }
              }
            }
          }
        `,
        variables: { id: 'gid://shopify/DeliveryProfile/1' },
      });

    expect(detail.status).toBe(200);
    expect(detail.body).toEqual({
      data: {
        deliveryProfile: {
          id: 'gid://shopify/DeliveryProfile/1',
          name: 'General profile',
          default: true,
          version: 3,
          activeMethodDefinitionsCount: 1,
          locationsWithoutRatesCount: 0,
          originLocationCount: 1,
          zoneCountryCount: 1,
          productVariantsCount: {
            count: 2,
            precision: 'EXACT',
          },
          profileItems: {
            nodes: [
              {
                product: {
                  id: 'gid://shopify/Product/1',
                  title: 'Snowboard',
                },
                variants: {
                  nodes: [
                    {
                      id: 'gid://shopify/ProductVariant/1',
                      title: 'Default Title',
                      product: {
                        id: 'gid://shopify/Product/1',
                      },
                    },
                  ],
                  pageInfo: {
                    hasNextPage: true,
                    hasPreviousPage: false,
                    startCursor: 'variant-cursor-1',
                    endCursor: 'variant-cursor-1',
                  },
                },
              },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: 'profile-item-cursor-1',
              endCursor: 'profile-item-cursor-1',
            },
          },
          profileLocationGroups: [
            {
              countriesInAnyZone: [
                {
                  zone: 'Domestic',
                  country: {
                    id: 'gid://shopify/DeliveryCountry/1',
                    name: 'United States',
                    code: {
                      countryCode: 'US',
                      restOfWorld: false,
                    },
                    provinces: [
                      {
                        id: 'gid://shopify/DeliveryProvince/1',
                        code: 'CA',
                      },
                    ],
                  },
                },
              ],
              locationGroup: {
                id: 'gid://shopify/DeliveryLocationGroup/1',
                locations: {
                  nodes: [
                    {
                      id: 'gid://shopify/Location/1',
                      name: 'Main Warehouse',
                    },
                  ],
                  pageInfo: {
                    hasNextPage: false,
                    hasPreviousPage: false,
                    startCursor: 'gid://shopify/Location/1',
                    endCursor: 'gid://shopify/Location/1',
                  },
                },
                locationsCount: {
                  count: 1,
                  precision: 'EXACT',
                },
              },
              locationGroupZones: {
                nodes: [
                  {
                    zone: {
                      id: 'gid://shopify/DeliveryZone/1',
                      name: 'Domestic',
                      countries: [
                        {
                          id: 'gid://shopify/DeliveryCountry/1',
                          code: {
                            countryCode: 'US',
                            restOfWorld: false,
                          },
                        },
                      ],
                    },
                    methodDefinitions: {
                      nodes: [
                        {
                          id: 'gid://shopify/DeliveryMethodDefinition/1',
                          name: 'Standard',
                          active: true,
                          description: null,
                          rateProvider: {
                            id: 'gid://shopify/DeliveryRateDefinition/1',
                            price: {
                              amount: '5.00',
                              currencyCode: 'USD',
                            },
                          },
                          methodConditions: [
                            {
                              id: 'gid://shopify/DeliveryCondition/1?operator=greater_than_or_equal_to',
                              field: 'TOTAL_WEIGHT',
                              operator: 'GREATER_THAN_OR_EQUAL_TO',
                              conditionCriteria: {
                                __typename: 'Weight',
                                unit: 'KILOGRAMS',
                                value: 1,
                              },
                            },
                          ],
                        },
                      ],
                      pageInfo: {
                        hasNextPage: false,
                        hasPreviousPage: false,
                        startCursor: 'method-cursor-standard',
                        endCursor: 'method-cursor-standard',
                      },
                    },
                  },
                ],
                pageInfo: {
                  hasNextPage: false,
                  hasPreviousPage: false,
                  startCursor: 'zone-cursor-domestic',
                  endCursor: 'zone-cursor-domestic',
                },
              },
            },
          ],
          unassignedLocationsPaginated: {
            nodes: [
              {
                id: 'gid://shopify/Location/2',
                name: 'Overflow Warehouse',
              },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: 'gid://shopify/Location/2',
              endCursor: 'gid://shopify/Location/2',
            },
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
