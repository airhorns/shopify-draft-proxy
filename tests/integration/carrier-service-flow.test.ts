import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';
import { resetSyntheticIdentity } from '../support/runtime.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const carrierServiceSelection = `#graphql
    id
    name
    formattedName
    callbackUrl
    active
    supportsServiceDiscovery
`;

function carrierServiceTail(id: string): string {
  return id.split('/').at(-1)?.split('?')[0] ?? id;
}

describe('carrier service local staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves empty and missing carrier-service reads locally in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('carrier service reads should resolve locally in snapshot mode');
    });
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `
          query MissingCarrierService($id: ID!) {
            carrierService(id: $id) {
              ${carrierServiceSelection}
            }
            carrierServices(first: 5, query: "active:true", sortKey: ID) {
              nodes {
                ${carrierServiceSelection}
              }
              edges {
                cursor
                node {
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
        `,
        variables: { id: 'gid://shopify/DeliveryCarrierService/999999999999' },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        carrierService: null,
        carrierServices: {
          nodes: [],
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages create, update, and delete lifecycle locally with catalog filtering, sorting, pagination, and meta state', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('carrier service lifecycle should not hit upstream in snapshot mode');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `
          mutation CreateCarrierService($input: DeliveryCarrierServiceCreateInput!) {
            carrierServiceCreate(input: $input) {
              carrierService {
                ${carrierServiceSelection}
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          input: {
            name: 'Hermes Local Carrier',
            callbackUrl: 'https://mock.shop/carrier-service-rates',
            active: false,
            supportsServiceDiscovery: true,
          },
        },
      });

    expect(createResponse.status).toBe(200);
    const createdService = createResponse.body.data.carrierServiceCreate.carrierService;
    expect(createResponse.body.data.carrierServiceCreate.userErrors).toEqual([]);
    expect(createdService).toMatchObject({
      name: 'Hermes Local Carrier',
      formattedName: 'Hermes Local Carrier (Rates provided by app)',
      callbackUrl: 'https://mock.shop/carrier-service-rates',
      active: false,
      supportsServiceDiscovery: true,
    });

    const secondCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `
          mutation CreateSecondCarrierService($input: DeliveryCarrierServiceCreateInput!) {
            carrierServiceCreate(input: $input) {
              carrierService {
                ${carrierServiceSelection}
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          input: {
            name: 'Hermes Local Carrier Two',
            callbackUrl: 'https://mock.shop/carrier-service-rates-two',
            active: true,
            supportsServiceDiscovery: false,
          },
        },
      });

    const secondService = secondCreateResponse.body.data.carrierServiceCreate.carrierService;
    const firstTail = carrierServiceTail(createdService.id);

    const catalogResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `
          query CarrierServiceCatalog($id: ID!, $idQuery: String!, $after: String) {
            carrierService(id: $id) {
              ${carrierServiceSelection}
            }
            inactive: carrierServices(first: 5, query: "active:false", sortKey: ID) {
              nodes {
                id
                active
              }
            }
            byId: carrierServices(first: 5, query: $idQuery, sortKey: ID) {
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
            firstPage: carrierServices(first: 1, sortKey: ID) {
              edges {
                cursor
                node {
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
            secondPage: carrierServices(first: 1, after: $after, sortKey: ID) {
              nodes {
                id
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            reversed: carrierServices(first: 2, reverse: true, sortKey: ID) {
              nodes {
                id
              }
            }
          }
        `,
        variables: {
          id: createdService.id,
          idQuery: `id:${firstTail}`,
          after: `cursor:${createdService.id}`,
        },
      });

    expect(catalogResponse.status).toBe(200);
    expect(catalogResponse.body.data.carrierService).toEqual(createdService);
    expect(catalogResponse.body.data.inactive.nodes).toEqual([{ id: createdService.id, active: false }]);
    expect(catalogResponse.body.data.byId.edges).toEqual([
      {
        cursor: `cursor:${createdService.id}`,
        node: {
          id: createdService.id,
          name: 'Hermes Local Carrier',
        },
      },
    ]);
    expect(catalogResponse.body.data.firstPage.edges).toEqual([
      {
        cursor: `cursor:${createdService.id}`,
        node: {
          id: createdService.id,
        },
      },
    ]);
    expect(catalogResponse.body.data.firstPage.pageInfo).toEqual({
      hasNextPage: true,
      hasPreviousPage: false,
      startCursor: `cursor:${createdService.id}`,
      endCursor: `cursor:${createdService.id}`,
    });
    expect(catalogResponse.body.data.secondPage.nodes).toEqual([{ id: secondService.id }]);
    expect(catalogResponse.body.data.secondPage.pageInfo).toMatchObject({
      hasNextPage: false,
      hasPreviousPage: true,
    });
    expect(catalogResponse.body.data.reversed.nodes).toEqual([{ id: secondService.id }, { id: createdService.id }]);

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `
          mutation UpdateCarrierService($input: DeliveryCarrierServiceUpdateInput!) {
            carrierServiceUpdate(input: $input) {
              carrierService {
                ${carrierServiceSelection}
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          input: {
            id: createdService.id,
            name: 'Hermes Local Carrier Updated',
            callbackUrl: 'https://mock.shop/carrier-service-rates-updated',
            active: true,
            supportsServiceDiscovery: false,
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.carrierServiceUpdate.userErrors).toEqual([]);
    expect(updateResponse.body.data.carrierServiceUpdate.carrierService).toMatchObject({
      id: createdService.id,
      name: 'Hermes Local Carrier Updated',
      formattedName: 'Hermes Local Carrier Updated (Rates provided by app)',
      callbackUrl: 'https://mock.shop/carrier-service-rates-updated',
      active: true,
      supportsServiceDiscovery: false,
    });

    const logAfterUpdate = await request(app).get('/__meta/log');
    expect(logAfterUpdate.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
    ]);
    expect(
      logAfterUpdate.body.entries.map(
        (entry: { interpreted: { primaryRootField: string }; requestBody: { query: string } }) => ({
          root: entry.interpreted.primaryRootField,
          query: entry.requestBody.query,
        }),
      ),
    ).toEqual([
      { root: 'carrierServiceCreate', query: expect.stringContaining('CreateCarrierService') },
      { root: 'carrierServiceCreate', query: expect.stringContaining('CreateSecondCarrierService') },
      { root: 'carrierServiceUpdate', query: expect.stringContaining('UpdateCarrierService') },
    ]);

    const stateAfterUpdate = await request(app).get('/__meta/state');
    expect(Object.keys(stateAfterUpdate.body.stagedState.carrierServices)).toEqual([
      createdService.id,
      secondService.id,
    ]);

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `
          mutation DeleteCarrierService($id: ID!) {
            carrierServiceDelete(id: $id) {
              deletedId
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { id: secondService.id },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.carrierServiceDelete).toEqual({
      deletedId: secondService.id,
      userErrors: [],
    });

    const afterDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `
          query AfterCarrierDelete($deletedId: ID!) {
            carrierService(id: $deletedId) {
              id
            }
            carrierServices(first: 5, sortKey: ID) {
              nodes {
                id
              }
            }
          }
        `,
        variables: { deletedId: secondService.id },
      });

    expect(afterDeleteResponse.body.data).toEqual({
      carrierService: null,
      carrierServices: {
        nodes: [{ id: createdService.id }],
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns captured validation-style userErrors without staging invalid lifecycle requests', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid carrier service lifecycle should resolve locally in snapshot mode');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `
          mutation InvalidCarrierCreate($input: DeliveryCarrierServiceCreateInput!) {
            carrierServiceCreate(input: $input) {
              carrierService {
                id
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          input: {
            name: '',
            callbackUrl: 'https://mock.shop/carrier-service-rates',
            active: false,
            supportsServiceDiscovery: true,
          },
        },
      });

    expect(createResponse.body.data.carrierServiceCreate).toEqual({
      carrierService: null,
      userErrors: [
        {
          field: null,
          message: "Shipping rate provider name can't be blank",
        },
      ],
    });

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `
          mutation UnknownCarrierUpdate($input: DeliveryCarrierServiceUpdateInput!) {
            carrierServiceUpdate(input: $input) {
              carrierService {
                id
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          input: {
            id: 'gid://shopify/DeliveryCarrierService/999999999999',
            name: 'Nope',
          },
        },
      });

    expect(updateResponse.body.data.carrierServiceUpdate).toEqual({
      carrierService: null,
      userErrors: [
        {
          field: null,
          message: 'The carrier or app could not be found.',
        },
      ],
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `
          mutation UnknownCarrierDelete($id: ID!) {
            carrierServiceDelete(id: $id) {
              deletedId
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { id: 'gid://shopify/DeliveryCarrierService/999999999999' },
      });

    expect(deleteResponse.body.data.carrierServiceDelete).toEqual({
      deletedId: null,
      userErrors: [
        {
          field: ['id'],
          message: 'The carrier or app could not be found.',
        },
      ],
    });
  });
});
