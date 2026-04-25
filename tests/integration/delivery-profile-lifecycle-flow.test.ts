import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
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
    handle: title.toLowerCase().replace(/\s+/gu, '-'),
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

function makeDefaultProfile(): DeliveryProfileRecord {
  return {
    id: 'gid://shopify/DeliveryProfile/1',
    cursor: 'profile-cursor-default',
    name: 'General profile',
    default: true,
    merchantOwned: true,
    version: 1,
    activeMethodDefinitionsCount: 0,
    locationsWithoutRatesCount: 0,
    originLocationCount: 0,
    zoneCountryCount: 0,
    productVariantsCount: {
      count: 1,
      precision: 'EXACT',
    },
    profileItems: [
      {
        productId: 'gid://shopify/Product/1',
        variantIds: ['gid://shopify/ProductVariant/1'],
        cursor: 'default-profile-item',
        variantCursors: {
          'gid://shopify/ProductVariant/1': 'default-variant-cursor',
        },
      },
    ],
    profileLocationGroups: [],
    unassignedLocationIds: [],
    sellingPlanGroups: [],
  };
}

const profileSelection = `#graphql
  id
  name
  default
  version
  activeMethodDefinitionsCount
  originLocationCount
  zoneCountryCount
  productVariantsCount {
    count
    precision
  }
  profileItems(first: 5) {
    nodes {
      product {
        id
        title
      }
      variants(first: 5) {
        nodes {
          id
          title
        }
      }
    }
  }
  profileLocationGroups {
    locationGroup {
      id
      locations(first: 5) {
        nodes {
          id
          name
        }
      }
    }
    locationGroupZones(first: 5) {
      nodes {
        zone {
          id
          name
          countries {
            code {
              countryCode
              restOfWorld
            }
            provinces {
              code
            }
          }
        }
        methodDefinitions(first: 5) {
          nodes {
            id
            name
            active
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
                  value
                  unit
                }
                ... on MoneyV2 {
                  amount
                  currencyCode
                }
              }
            }
          }
        }
      }
    }
  }
`;

describe('delivery profile local staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages create, update, and remove locally with downstream reads and meta visibility', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('delivery profile lifecycle should not hit upstream in snapshot mode');
    });
    const product = makeProduct('gid://shopify/Product/1', 'Shipping Snowboard');
    store.upsertBaseProducts([product]);
    store.replaceBaseVariantsForProduct(product.id, [
      makeVariant(product.id, 'gid://shopify/ProductVariant/1', 'Default Title'),
    ]);
    store.upsertBaseLocations([
      makeLocation('gid://shopify/Location/1', 'Shop location'),
      makeLocation('gid://shopify/Location/2', 'Warehouse'),
    ]);
    store.upsertBaseDeliveryProfiles([makeDefaultProfile()]);
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation CreateDeliveryProfile($profile: DeliveryProfileInput!) {
            deliveryProfileCreate(profile: $profile) {
              profile {
                ${profileSelection}
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          profile: {
            name: 'Local custom shipping',
            variantsToAssociate: ['gid://shopify/ProductVariant/1'],
            locationGroupsToCreate: [
              {
                locations: ['gid://shopify/Location/1'],
                zonesToCreate: [
                  {
                    name: 'Domestic',
                    countries: [{ code: 'US', provinces: [{ code: 'CA' }] }],
                    methodDefinitionsToCreate: [
                      {
                        name: 'Standard',
                        active: true,
                        rateDefinition: { price: { amount: '7.25', currencyCode: 'USD' } },
                        weightConditionsToCreate: [
                          {
                            operator: 'GREATER_THAN_OR_EQUAL_TO',
                            criteria: { value: 1, unit: 'KILOGRAMS' },
                          },
                        ],
                      },
                    ],
                  },
                ],
              },
            ],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.deliveryProfileCreate.userErrors).toEqual([]);
    const createdProfile = createResponse.body.data.deliveryProfileCreate.profile;
    expect(createdProfile).toMatchObject({
      name: 'Local custom shipping',
      default: false,
      version: 1,
      activeMethodDefinitionsCount: 1,
      originLocationCount: 1,
      zoneCountryCount: 1,
      productVariantsCount: { count: 1, precision: 'EXACT' },
    });
    expect(createdProfile.profileItems.nodes).toEqual([
      {
        product: { id: product.id, title: 'Shipping Snowboard' },
        variants: { nodes: [{ id: 'gid://shopify/ProductVariant/1', title: 'Default Title' }] },
      },
    ]);
    expect(createdProfile.profileLocationGroups[0].locationGroup.locations.nodes).toEqual([
      { id: 'gid://shopify/Location/1', name: 'Shop location' },
    ]);

    const groupId = createdProfile.profileLocationGroups[0].locationGroup.id;
    const zone = createdProfile.profileLocationGroups[0].locationGroupZones.nodes[0].zone;
    const standardMethod =
      createdProfile.profileLocationGroups[0].locationGroupZones.nodes[0].methodDefinitions.nodes[0];
    const weightCondition = standardMethod.methodConditions[0];

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UpdateDeliveryProfile($id: ID!, $profile: DeliveryProfileInput!) {
            deliveryProfileUpdate(id: $id, profile: $profile) {
              profile {
                ${profileSelection}
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          id: createdProfile.id,
          profile: {
            name: 'Local custom shipping updated',
            variantsToDissociate: ['gid://shopify/ProductVariant/1'],
            conditionsToDelete: [weightCondition.id],
            locationGroupsToUpdate: [
              {
                id: groupId,
                locationsToAdd: ['gid://shopify/Location/2'],
                zonesToUpdate: [
                  {
                    id: zone.id,
                    name: 'Domestic updated',
                    methodDefinitionsToUpdate: [
                      {
                        id: standardMethod.id,
                        name: 'Standard updated',
                        active: false,
                        rateDefinition: {
                          id: standardMethod.rateProvider.id,
                          price: { amount: '8.50', currencyCode: 'USD' },
                        },
                      },
                    ],
                    methodDefinitionsToCreate: [
                      {
                        name: 'Express',
                        active: true,
                        rateDefinition: { price: { amount: '12.00', currencyCode: 'USD' } },
                        priceConditionsToCreate: [
                          {
                            operator: 'LESS_THAN_OR_EQUAL_TO',
                            criteria: { amount: '100.00', currencyCode: 'USD' },
                          },
                        ],
                      },
                    ],
                  },
                ],
              },
            ],
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.deliveryProfileUpdate.userErrors).toEqual([]);
    const updatedProfile = updateResponse.body.data.deliveryProfileUpdate.profile;
    expect(updatedProfile).toMatchObject({
      id: createdProfile.id,
      name: 'Local custom shipping updated',
      version: 2,
      activeMethodDefinitionsCount: 1,
      originLocationCount: 2,
      productVariantsCount: { count: 0, precision: 'EXACT' },
    });
    expect(updatedProfile.profileItems.nodes).toEqual([]);
    expect(updatedProfile.profileLocationGroups[0].locationGroup.locations.nodes).toEqual([
      { id: 'gid://shopify/Location/1', name: 'Shop location' },
      { id: 'gid://shopify/Location/2', name: 'Warehouse' },
    ]);
    expect(updatedProfile.profileLocationGroups[0].locationGroupZones.nodes[0].zone.name).toBe('Domestic updated');
    expect(updatedProfile.profileLocationGroups[0].locationGroupZones.nodes[0].methodDefinitions.nodes).toMatchObject([
      {
        id: standardMethod.id,
        name: 'Standard updated',
        active: false,
        methodConditions: [],
      },
      {
        name: 'Express',
        active: true,
        methodConditions: [
          {
            field: 'TOTAL_PRICE',
            operator: 'LESS_THAN_OR_EQUAL_TO',
            conditionCriteria: {
              __typename: 'MoneyV2',
              amount: '100',
              currencyCode: 'USD',
            },
          },
        ],
      },
    ]);

    const readAfterUpdate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query ReadAfterUpdate($id: ID!) {
            deliveryProfile(id: $id) {
              id
              name
              productVariantsCount {
                count
                precision
              }
              profileItems(first: 5) {
                nodes {
                  product {
                    id
                  }
                }
              }
            }
            deliveryProfiles(first: 5) {
              nodes {
                id
                name
              }
            }
          }
        `,
        variables: { id: createdProfile.id },
      });

    expect(readAfterUpdate.body.data.deliveryProfile).toEqual({
      id: createdProfile.id,
      name: 'Local custom shipping updated',
      productVariantsCount: { count: 0, precision: 'EXACT' },
      profileItems: { nodes: [] },
    });
    expect(readAfterUpdate.body.data.deliveryProfiles.nodes).toEqual([
      { id: 'gid://shopify/DeliveryProfile/1', name: 'General profile' },
      { id: createdProfile.id, name: 'Local custom shipping updated' },
    ]);

    const logAfterUpdate = await request(app).get('/__meta/log');
    expect(logAfterUpdate.body.entries.map((entry: { status: string }) => entry.status)).toEqual(['staged', 'staged']);
    expect(
      logAfterUpdate.body.entries.map(
        (entry: { requestBody: { operationName?: string }; interpreted: { primaryRootField: string } }) => ({
          root: entry.interpreted.primaryRootField,
          operationName: entry.requestBody.operationName ?? null,
        }),
      ),
    ).toEqual([
      { root: 'deliveryProfileCreate', operationName: null },
      { root: 'deliveryProfileUpdate', operationName: null },
    ]);

    const stateAfterUpdate = await request(app).get('/__meta/state');
    expect(Object.keys(stateAfterUpdate.body.stagedState.deliveryProfiles)).toEqual([
      'gid://shopify/DeliveryProfile/1',
      createdProfile.id,
    ]);

    const removeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation RemoveDeliveryProfile($id: ID!) {
            deliveryProfileRemove(id: $id) {
              job {
                id
                done
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { id: createdProfile.id },
      });

    expect(removeResponse.status).toBe(200);
    expect(removeResponse.body.data.deliveryProfileRemove).toMatchObject({
      job: {
        done: false,
      },
      userErrors: [],
    });
    expect(removeResponse.body.data.deliveryProfileRemove.job.id).toMatch(/^gid:\/\/shopify\/Job\//u);

    const readAfterRemove = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query ReadAfterRemove($id: ID!) {
            deliveryProfile(id: $id) {
              id
            }
            deliveryProfiles(first: 5) {
              nodes {
                id
              }
            }
          }
        `,
        variables: { id: createdProfile.id },
      });

    expect(readAfterRemove.body.data).toEqual({
      deliveryProfile: null,
      deliveryProfiles: {
        nodes: [{ id: 'gid://shopify/DeliveryProfile/1' }],
      },
    });

    const finalLog = await request(app).get('/__meta/log');
    expect(
      finalLog.body.entries.map(
        (entry: { interpreted: { primaryRootField: string } }) => entry.interpreted.primaryRootField,
      ),
    ).toEqual(['deliveryProfileCreate', 'deliveryProfileUpdate', 'deliveryProfileRemove']);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns captured validation-style userErrors without staging invalid profile writes', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid delivery profile lifecycle should resolve locally in snapshot mode');
    });
    store.upsertBaseDeliveryProfiles([makeDefaultProfile()]);
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation BlankCreate($profile: DeliveryProfileInput!) {
            deliveryProfileCreate(profile: $profile) {
              profile {
                id
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { profile: { name: '' } },
      });

    expect(createResponse.body.data.deliveryProfileCreate).toEqual({
      profile: null,
      userErrors: [{ field: ['profile', 'name'], message: 'Add a profile name' }],
    });

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation MissingUpdate($id: ID!, $profile: DeliveryProfileInput!) {
            deliveryProfileUpdate(id: $id, profile: $profile) {
              profile {
                id
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { id: 'gid://shopify/DeliveryProfile/999999999999', profile: { name: 'Nope' } },
      });

    expect(updateResponse.body.data.deliveryProfileUpdate).toEqual({
      profile: null,
      userErrors: [{ field: null, message: 'Profile could not be updated.' }],
    });

    const missingRemoveResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation MissingRemove($id: ID!) {
            deliveryProfileRemove(id: $id) {
              job {
                id
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { id: 'gid://shopify/DeliveryProfile/999999999999' },
      });

    expect(missingRemoveResponse.body.data.deliveryProfileRemove).toEqual({
      job: null,
      userErrors: [{ field: null, message: 'The Delivery Profile cannot be found for the shop.' }],
    });

    const defaultRemoveResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation RemoveDefault($id: ID!) {
            deliveryProfileRemove(id: $id) {
              job {
                id
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { id: 'gid://shopify/DeliveryProfile/1' },
      });

    expect(defaultRemoveResponse.body.data.deliveryProfileRemove).toEqual({
      job: null,
      userErrors: [{ field: null, message: 'Cannot delete the default profile.' }],
    });

    const log = await request(app).get('/__meta/log');
    expect(log.body.entries).toEqual([]);
  });
});
