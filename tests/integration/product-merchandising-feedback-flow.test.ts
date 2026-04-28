import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function makeBaseProduct(id: string, title: string, handle: string) {
  return {
    id,
    legacyResourceId: id.split('/').at(-1) ?? id,
    title,
    handle,
    status: 'ACTIVE' as const,
    publicationIds: [],
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-02T00:00:00.000Z',
    vendor: null,
    productType: null,
    tags: [],
    totalInventory: null,
    tracksInventory: null,
    descriptionHtml: null,
    onlineStorePreviewUrl: null,
    templateSuffix: null,
    seo: { title: null, description: null },
    category: null,
  };
}

function makeBaseVariant(productId: string, id: string, title = 'Default Title') {
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
    selectedOptions: [{ name: 'Title', value: title }],
    inventoryItem: null,
  };
}

describe('product merchandising and feedback flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages product feed create, sync, delete, and immediate feed reads without upstream writes', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('product feed roots should stay local'));
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation FeedCreate($input: ProductFeedInput) {
            productFeedCreate(input: $input) {
              productFeed { id country language status }
              userErrors { field message code }
            }
          }
        `,
        variables: { input: { country: 'US', language: 'EN' } },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.productFeedCreate.userErrors).toEqual([]);
    expect(createResponse.body.data.productFeedCreate.productFeed).toMatchObject({
      country: 'US',
      language: 'EN',
      status: 'ACTIVE',
    });
    const feedId = createResponse.body.data.productFeedCreate.productFeed.id as string;

    const syncResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: 'mutation Sync($id: ID!) { productFullSync(id: $id) { id userErrors { field message code } } }',
        variables: { id: feedId },
      });
    expect(syncResponse.body.data.productFullSync).toEqual({ id: feedId, userErrors: [] });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query Feeds($id: ID!) {
            productFeed(id: $id) { id status }
            productFeeds(first: 10) { nodes { id country language status } }
          }
        `,
        variables: { id: feedId },
      });
    expect(readResponse.body.data.productFeed).toEqual({ id: feedId, status: 'ACTIVE' });
    expect(readResponse.body.data.productFeeds.nodes).toEqual([
      { id: feedId, country: 'US', language: 'EN', status: 'ACTIVE' },
    ]);

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation Delete($id: ID!) { productFeedDelete(id: $id) { deletedId userErrors { field message code } } }',
        variables: { id: feedId },
      });
    expect(deleteResponse.body.data.productFeedDelete).toEqual({ deletedId: feedId, userErrors: [] });

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.productFeeds).toEqual({});
    expect(stateResponse.body.stagedState.deletedProductFeedIds).toEqual({ [feedId]: true });
    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages product and shop feedback and exposes productResourceFeedback reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('feedback roots should stay local'));
    const productId = 'gid://shopify/Product/7001';
    store.upsertBaseProducts([makeBaseProduct(productId, 'Feedback Product', 'feedback-product')]);
    const app = createApp(config).callback();

    const feedbackVariables = {
      feedbackInput: [
        {
          productId,
          state: 'REQUIRES_ACTION',
          feedbackGeneratedAt: '2024-02-01T00:00:00Z',
          productUpdatedAt: '2024-01-31T00:00:00Z',
          messages: ['Needs a description.'],
        },
      ],
    };
    const feedbackResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation ProductFeedback($feedbackInput: [ProductResourceFeedbackInput!]!) {
            bulkProductResourceFeedbackCreate(feedbackInput: $feedbackInput) {
              feedback { productId state messages feedbackGeneratedAt productUpdatedAt }
              userErrors { field message code }
            }
          }
        `,
        variables: feedbackVariables,
      });
    expect(feedbackResponse.body.data.bulkProductResourceFeedbackCreate.userErrors).toEqual([]);

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query ProductFeedback($id: ID!) {
            productResourceFeedback(id: $id) {
              productId
              state
              messages
              feedbackGeneratedAt
              productUpdatedAt
            }
          }
        `,
        variables: { id: productId },
      });
    expect(readResponse.body.data.productResourceFeedback).toEqual(feedbackVariables.feedbackInput[0]);

    const shopFeedbackResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation ShopFeedback($input: ResourceFeedbackCreateInput!) {
            shopResourceFeedbackCreate(input: $input) {
              feedback { state feedbackGeneratedAt messages { message } }
              userErrors { field message code }
            }
          }
        `,
        variables: {
          input: {
            state: 'ACCEPTED',
            feedbackGeneratedAt: '2024-02-02T00:00:00Z',
            messages: ['Ready.'],
          },
        },
      });
    expect(shopFeedbackResponse.body.data.shopResourceFeedbackCreate).toEqual({
      feedback: {
        state: 'ACCEPTED',
        feedbackGeneratedAt: '2024-02-02T00:00:00Z',
        messages: [{ message: 'Ready.' }],
      },
      userErrors: [],
    });

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.productResourceFeedback[productId]).toMatchObject({
      productId,
      state: 'REQUIRES_ACTION',
    });
    expect(Object.values(stateResponse.body.stagedState.shopResourceFeedback)).toHaveLength(1);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages bundle, variant component, and combined-listing merchandising reads', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('merchandising roots should stay local'));
    const parentProductId = 'gid://shopify/Product/8001';
    const childProductId = 'gid://shopify/Product/8002';
    const componentProductId = 'gid://shopify/Product/8003';
    const parentVariantId = 'gid://shopify/ProductVariant/9001';
    const childVariantId = 'gid://shopify/ProductVariant/9002';
    const componentVariantId = 'gid://shopify/ProductVariant/9003';
    store.upsertBaseProducts([
      makeBaseProduct(parentProductId, 'Parent Helmet', 'parent-helmet'),
      makeBaseProduct(childProductId, 'Child Helmet', 'child-helmet'),
      makeBaseProduct(componentProductId, 'Bundle Strap', 'bundle-strap'),
    ]);
    store.replaceBaseVariantsForProduct(parentProductId, [makeBaseVariant(parentProductId, parentVariantId)]);
    store.replaceBaseVariantsForProduct(childProductId, [makeBaseVariant(childProductId, childVariantId)]);
    store.replaceBaseVariantsForProduct(componentProductId, [makeBaseVariant(componentProductId, componentVariantId)]);

    const app = createApp(config).callback();

    const bundleResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation Bundle($input: ProductBundleCreateInput!) {
            productBundleCreate(input: $input) {
              productBundleOperation {
                id
                status
                product {
                  id
                  title
                  bundleComponents(first: 5) {
                    nodes { componentProduct { id } quantity componentVariantsCount { count precision } }
                  }
                }
              }
              userErrors { field message }
            }
          }
        `,
        variables: {
          input: {
            title: 'Helmet bundle',
            components: [{ productId: componentProductId, quantity: 2, optionSelections: [] }],
          },
        },
      });
    expect(bundleResponse.body.data.productBundleCreate.userErrors).toEqual([]);
    expect(bundleResponse.body.data.productBundleCreate.productBundleOperation.status).toBe('CREATED');
    expect(bundleResponse.body.data.productBundleCreate.productBundleOperation.product.bundleComponents.nodes).toEqual([
      {
        componentProduct: { id: componentProductId },
        quantity: 2,
        componentVariantsCount: { count: 1, precision: 'EXACT' },
      },
    ]);

    const relationshipResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation VariantComponents($input: [ProductVariantRelationshipUpdateInput!]!) {
            productVariantRelationshipBulkUpdate(input: $input) {
              parentProductVariants {
                id
                requiresComponents
                productVariantComponents(first: 5) { nodes { id quantity productVariant { id } } }
              }
              userErrors { field message code }
            }
          }
        `,
        variables: {
          input: [
            {
              parentProductVariantId: parentVariantId,
              productVariantRelationshipsToCreate: [{ id: componentVariantId, quantity: 3 }],
            },
          ],
        },
      });
    expect(relationshipResponse.body.data.productVariantRelationshipBulkUpdate.userErrors).toEqual([]);
    expect(relationshipResponse.body.data.productVariantRelationshipBulkUpdate.parentProductVariants[0]).toMatchObject({
      id: parentVariantId,
      requiresComponents: true,
      productVariantComponents: { nodes: [{ quantity: 3, productVariant: { id: componentVariantId } }] },
    });

    const combinedResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation Combined($parentProductId: ID!, $productsAdded: [ChildProductRelationInput!]) {
            combinedListingUpdate(parentProductId: $parentProductId, productsAdded: $productsAdded) {
              product {
                id
                combinedListingRole
                combinedListing {
                  parentProduct { id }
                  combinedListingChildren(first: 5) { nodes { product { id combinedListingRole } parentVariant { id } } }
                }
              }
              userErrors { field message code }
            }
          }
        `,
        variables: {
          parentProductId,
          productsAdded: [
            {
              childProductId,
              selectedParentOptionValues: [{ name: 'Title', value: 'Default Title' }],
            },
          ],
        },
      });
    expect(combinedResponse.body.data.combinedListingUpdate.userErrors).toEqual([]);
    expect(combinedResponse.body.data.combinedListingUpdate.product).toMatchObject({
      id: parentProductId,
      combinedListingRole: 'PARENT',
      combinedListing: {
        parentProduct: { id: parentProductId },
        combinedListingChildren: {
          nodes: [
            { product: { id: childProductId, combinedListingRole: 'CHILD' }, parentVariant: { id: parentVariantId } },
          ],
        },
      },
    });

    const stateResponse = await request(app).get('/__meta/state');
    expect(Object.values(stateResponse.body.stagedState.productBundleComponents)).toHaveLength(1);
    expect(Object.values(stateResponse.body.stagedState.productVariantComponents)).toHaveLength(1);
    expect(Object.values(stateResponse.body.stagedState.combinedListingChildren)).toHaveLength(1);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
