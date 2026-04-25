import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'passthrough',
};

describe('proxy capability classification', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('logs supported product mutations as staged-local intent instead of generic passthrough', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            productCreate: {
              product: { id: 'gid://shopify/Product/999' },
              userErrors: [],
            },
          },
        }),
        {
          status: 200,
          headers: { 'content-type': 'application/json' },
        },
      ),
    );

    const app = createApp(config);

    const response = await request(app.callback())
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query:
          'mutation ProductCreate { productCreate(product: { title: "Hat" }) { product { id } userErrors { field message } } }',
      });

    expect(response.status).toBe(200);

    expect(store.getLog()).toHaveLength(1);
    expect(store.getLog()[0]).toMatchObject({
      operationName: 'ProductCreate',
      status: 'staged',
      notes: 'Staged locally in the in-memory product draft store.',
    });
  });

  it('logs tracked unsupported discounts mutations as discounts passthrough', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            discountCodeBasicCreate: {
              codeDiscountNode: null,
              userErrors: [],
            },
          },
        }),
        {
          status: 200,
          headers: { 'content-type': 'application/json' },
        },
      ),
    );

    const app = createApp(config);

    const query = `#graphql
      mutation CreateDiscount {
        discountCodeBasicCreate(
          basicCodeDiscount: {
            title: "Test"
            code: "TEST"
            startsAt: "2026-04-24T00:00:00Z"
            customerGets: { value: { percentage: 0.1 }, items: { all: true } }
          }
        ) {
          codeDiscountNode {
            id
          }
          userErrors {
            field
            message
          }
        }
      }
    `;

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({ query });

    expect(response.status).toBe(200);
    expect(store.getLog()).toHaveLength(1);
    expect(store.getLog()[0]).toMatchObject({
      operationName: 'discountCodeBasicCreate',
      status: 'proxied',
      interpreted: {
        operationType: 'mutation',
        operationName: 'CreateDiscount',
        rootFields: ['discountCodeBasicCreate'],
        primaryRootField: 'discountCodeBasicCreate',
        capability: {
          operationName: 'discountCodeBasicCreate',
          domain: 'discounts',
          execution: 'passthrough',
        },
      },
      notes: 'Mutation passthrough placeholder until supported local staging is implemented.',
    });
  });

  it('logs generic publishable mutations as tracked Store properties passthrough', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            publishablePublish: {
              publishable: null,
              userErrors: [],
            },
          },
        }),
        {
          status: 200,
          headers: { 'content-type': 'application/json' },
        },
      ),
    );

    const app = createApp(config);

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `#graphql
          mutation PublishGeneric {
            publishablePublish(
              id: "gid://shopify/Product/1"
              input: { publicationId: "gid://shopify/Publication/1" }
            ) {
              publishable {
                ... on Product {
                  id
                }
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
      });

    expect(response.status).toBe(200);
    expect(store.getLog()).toHaveLength(1);
    expect(store.getLog()[0]).toMatchObject({
      operationName: 'publishablePublish',
      status: 'proxied',
      interpreted: {
        operationType: 'mutation',
        operationName: 'PublishGeneric',
        rootFields: ['publishablePublish'],
        primaryRootField: 'publishablePublish',
        capability: {
          operationName: 'publishablePublish',
          domain: 'store-properties',
          execution: 'passthrough',
        },
      },
      notes: 'Mutation passthrough placeholder until supported local staging is implemented.',
    });
  });
});
