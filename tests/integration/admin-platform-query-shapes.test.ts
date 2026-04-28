import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const passthroughConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'passthrough',
};

describe('admin platform utility query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves safe utility read roots in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('admin platform utility reads should resolve locally in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query AdminPlatformUtilityReads($ids: [ID!]!, $domainId: ID!, $jobId: ID!) {
          publicApiVersions {
            __typename
            handle
            displayName
            supported
          }
          node(id: "gid://shopify/Product/0") {
            __typename
            id
          }
          nodes(ids: $ids) {
            __typename
            id
          }
          job(id: $jobId) {
            __typename
            id
            done
            query {
              __typename
            }
          }
          domain(id: $domainId) {
            id
            host
            url
            sslEnabled
          }
          backupRegion {
            __typename
            id
            name
            ... on MarketRegionCountry {
              code
            }
          }
          taxonomy {
            categories(first: 2, search: "zzzzzz-no-match-har-315") {
              nodes {
                id
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
        }`,
        variables: {
          ids: ['gid://shopify/Product/0', 'gid://shopify/Job/0', 'gid://shopify/Domain/0'],
          domainId: 'gid://shopify/Domain/0',
          jobId: 'gid://shopify/Job/0',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        publicApiVersions: [
          { __typename: 'ApiVersion', handle: '2025-07', displayName: '2025-07', supported: true },
          { __typename: 'ApiVersion', handle: '2025-10', displayName: '2025-10', supported: true },
          { __typename: 'ApiVersion', handle: '2026-01', displayName: '2026-01', supported: true },
          { __typename: 'ApiVersion', handle: '2026-04', displayName: '2026-04 (Latest)', supported: true },
          {
            __typename: 'ApiVersion',
            handle: '2026-07',
            displayName: '2026-07 (Release candidate)',
            supported: false,
          },
          { __typename: 'ApiVersion', handle: 'unstable', displayName: 'unstable', supported: false },
        ],
        node: null,
        nodes: [null, null, null],
        job: {
          __typename: 'Job',
          id: 'gid://shopify/Job/0',
          done: true,
          query: {
            __typename: 'QueryRoot',
          },
        },
        domain: null,
        backupRegion: {
          __typename: 'MarketRegionCountry',
          id: 'gid://shopify/MarketRegionCountry/4062110417202',
          name: 'Canada',
          code: 'CA',
        },
        taxonomy: {
          categories: {
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
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors captured staff utility access blockers locally in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('staff utility blockers should resolve locally in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query StaffUtilityRead {
          staffMember {
            id
            exists
            active
          }
          staffMembers(first: 1) {
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
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      staffMember: null,
      staffMembers: null,
    });
    expect(response.body.errors).toEqual([
      expect.objectContaining({
        message: expect.stringContaining('Access denied for staffMember field.'),
        path: ['staffMember'],
        extensions: expect.objectContaining({ code: 'ACCESS_DENIED' }),
      }),
      expect.objectContaining({
        message: 'Access denied for staffMembers field.',
        path: ['staffMembers'],
        extensions: expect.objectContaining({ code: 'ACCESS_DENIED' }),
      }),
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages Flow utility mutations locally without external trigger delivery or signature leakage', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('Flow utilities must not proxy'));

    const app = createApp(passthroughConfig).callback();
    const signatureResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation FlowGenerateSignature($payload: String!) {
          flowGenerateSignature(id: "gid://shopify/FlowTrigger/374", payload: $payload) {
            payload
            signature
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          payload: '{"customer_id":374}',
        },
      });
    const triggerResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation FlowTriggerReceive($payload: JSON) {
          flowTriggerReceive(handle: "har-374-local-trigger", payload: $payload) {
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          payload: { customer_id: 374, action: 'created' },
        },
      });

    expect(signatureResponse.status).toBe(200);
    expect(signatureResponse.body.data.flowGenerateSignature).toEqual({
      payload: '{"customer_id":374}',
      signature: expect.any(String),
      userErrors: [],
    });
    expect(signatureResponse.body.data.flowGenerateSignature.signature).toHaveLength(64);
    expect(triggerResponse.status).toBe(200);
    expect(triggerResponse.body.data.flowTriggerReceive.userErrors).toEqual([]);
    expect(fetchSpy).not.toHaveBeenCalled();

    const log = store.getLog();
    expect(log).toHaveLength(2);
    expect(log.map((entry) => ({ operationName: entry.operationName, status: entry.status }))).toEqual([
      { operationName: 'FlowGenerateSignature', status: 'staged' },
      { operationName: 'FlowTriggerReceive', status: 'staged' },
    ]);
    expect(log[0]?.variables).toEqual({ payload: '{"customer_id":374}' });
    expect(log[1]?.variables).toEqual({ payload: { customer_id: 374, action: 'created' } });
    expect(JSON.stringify(log)).not.toContain(signatureResponse.body.data.flowGenerateSignature.signature);

    const state = store.getState().stagedState;
    expect(Object.values(state.adminPlatformFlowSignatures)).toEqual([
      expect.objectContaining({
        flowTriggerId: 'gid://shopify/FlowTrigger/374',
        payloadSha256: expect.any(String),
        signatureSha256: expect.any(String),
      }),
    ]);
    expect(Object.values(state.adminPlatformFlowTriggers)).toEqual([
      expect.objectContaining({
        handle: 'har-374-local-trigger',
        payloadBytes: expect.any(Number),
        payloadSha256: expect.any(String),
      }),
    ]);
    expect(JSON.stringify(state)).not.toContain(signatureResponse.body.data.flowGenerateSignature.signature);
    expect(JSON.stringify(state)).not.toContain('customer_id');
    expect(JSON.stringify(state)).not.toContain('"action"');
  });

  it('mirrors captured Flow validation branches locally without staging', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('Flow validation must not proxy'));

    const app = createApp(passthroughConfig).callback();
    const invalidHandleResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation FlowTriggerReceive {
          flowTriggerReceive(handle: "har-374-missing", payload: { test: "value" }) {
            userErrors {
              field
              message
            }
          }
        }`,
      });
    const oversizeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation FlowTriggerReceive($payload: JSON) {
          flowTriggerReceive(handle: "har-374-missing", payload: $payload) {
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          payload: { value: 'x'.repeat(50_001) },
        },
      });
    const unknownSignatureResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation { flowGenerateSignature(id: "gid://shopify/FlowTrigger/0", payload: "{}") { signature userErrors { field message } } }`,
      });

    expect(invalidHandleResponse.status).toBe(200);
    expect(invalidHandleResponse.body.data.flowTriggerReceive.userErrors).toEqual([
      {
        field: ['body'],
        message: "Errors validating schema:\n  Invalid handle 'har-374-missing'.\n",
      },
    ]);
    expect(oversizeResponse.status).toBe(200);
    expect(oversizeResponse.body.data.flowTriggerReceive.userErrors).toEqual([
      {
        field: ['body'],
        message: 'Errors validating schema:\n  Properties size exceeds the limit of 50000 bytes.\n',
      },
    ]);
    expect(unknownSignatureResponse.status).toBe(200);
    expect(unknownSignatureResponse.body).toMatchObject({
      data: { flowGenerateSignature: null },
      errors: [
        {
          message: 'Invalid id: gid://shopify/FlowTrigger/0',
          extensions: { code: 'RESOURCE_NOT_FOUND' },
          path: ['flowGenerateSignature'],
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getLog()).toEqual([]);
    expect(store.getState().stagedState.adminPlatformFlowSignatureOrder).toEqual([]);
    expect(store.getState().stagedState.adminPlatformFlowTriggerOrder).toEqual([]);
  });

  it('stages backupRegionUpdate locally and preserves backupRegion read-after-write', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('backupRegionUpdate must not proxy'));

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation BackupRegionUpdate {
          backupRegionUpdate(region: { countryCode: CA }) {
            backupRegion {
              __typename
              id
              name
              ... on MarketRegionCountry {
                code
              }
            }
            userErrors {
              field
              message
              code
            }
          }
        }`,
      });
    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `query BackupRegionRead {
          backupRegion {
            __typename
            id
            name
            ... on MarketRegionCountry {
              code
            }
          }
        }`,
      });

    const expectedRegion = {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/4062110417202',
      name: 'Canada',
      code: 'CA',
    };
    expect(response.status).toBe(200);
    expect(response.body.data.backupRegionUpdate).toEqual({
      backupRegion: expectedRegion,
      userErrors: [],
    });
    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.backupRegion).toEqual(expectedRegion);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getLog()).toHaveLength(1);
    expect(store.getLog()[0]).toMatchObject({
      operationName: 'BackupRegionUpdate',
      status: 'staged',
      stagedResourceIds: ['gid://shopify/MarketRegionCountry/4062110417202'],
      interpreted: {
        capability: {
          operationName: 'BackupRegionUpdate',
          domain: 'admin-platform',
          execution: 'stage-locally',
        },
      },
    });
    expect(store.getState().stagedState.backupRegion).toEqual(expectedRegion);
  });

  it('mirrors captured backupRegionUpdate REGION_NOT_FOUND validation without staging', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('backupRegion validation must not proxy'));

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation BackupRegionUpdate {
          backupRegionUpdate(region: { countryCode: ZZ }) {
            backupRegion {
              __typename
              id
              name
            }
            userErrors {
              field
              message
              code
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body.data.backupRegionUpdate).toEqual({
      backupRegion: null,
      userErrors: [{ field: ['region'], message: 'Region not found.', code: 'REGION_NOT_FOUND' }],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getLog()).toEqual([]);
  });
});
