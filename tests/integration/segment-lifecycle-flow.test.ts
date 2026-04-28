import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const SEGMENT_FIELDS = `#graphql
  fragment LifecycleSegmentFields on Segment {
    id
    name
    query
    creationDate
    lastEditDate
  }
`;

describe('segment lifecycle staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages segmentCreate, segmentUpdate, and segmentDelete locally with read-after-write and meta visibility', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('segment lifecycle must not proxy'));
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `${SEGMENT_FIELDS}
          mutation CreateSegment($name: String!, $query: String!) {
            segmentCreate(name: $name, query: $query) {
              segment {
                ...LifecycleSegmentFields
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          name: 'Codex Subscribers',
          query: "email_subscription_status = 'SUBSCRIBED'",
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.segmentCreate).toEqual({
      segment: {
        id: 'gid://shopify/Segment/1',
        name: 'Codex Subscribers',
        query: "email_subscription_status = 'SUBSCRIBED'",
        creationDate: '2024-01-01T00:00:00.000Z',
        lastEditDate: '2024-01-01T00:00:00.000Z',
      },
      userErrors: [],
    });

    const duplicateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${SEGMENT_FIELDS}
          mutation DuplicateSegment($name: String!, $query: String!) {
            segmentCreate(name: $name, query: $query) {
              segment {
                ...LifecycleSegmentFields
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          name: 'Codex Subscribers',
          query: "email_subscription_status = 'SUBSCRIBED'",
        },
      });

    expect(duplicateResponse.status).toBe(200);
    expect(duplicateResponse.body.data.segmentCreate.segment).toMatchObject({
      id: 'gid://shopify/Segment/3',
      name: 'Codex Subscribers (2)',
      query: "email_subscription_status = 'SUBSCRIBED'",
    });
    expect(duplicateResponse.body.data.segmentCreate.userErrors).toEqual([]);

    const segmentId = createResponse.body.data.segmentCreate.segment.id as string;
    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${SEGMENT_FIELDS}
          mutation UpdateSegment($id: ID!, $name: String, $query: String) {
            segmentUpdate(id: $id, name: $name, query: $query) {
              segment {
                ...LifecycleSegmentFields
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          id: segmentId,
          name: 'Codex Buyers',
          query: 'number_of_orders >= 1',
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.segmentUpdate).toEqual({
      segment: {
        id: segmentId,
        name: 'Codex Buyers',
        query: 'number_of_orders >= 1',
        creationDate: '2024-01-01T00:00:00.000Z',
        lastEditDate: '2024-01-01T00:00:04.000Z',
      },
      userErrors: [],
    });

    const readAfterUpdateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query ReadSegments($id: ID!) {
            segment(id: $id) {
              id
              name
              query
              creationDate
              lastEditDate
            }
            segments(first: 10) {
              nodes {
                id
                name
                query
              }
            }
            segmentsCount {
              count
              precision
            }
          }
        `,
        variables: { id: segmentId },
      });

    expect(readAfterUpdateResponse.status).toBe(200);
    expect(readAfterUpdateResponse.body.data.segment).toEqual(updateResponse.body.data.segmentUpdate.segment);
    expect(readAfterUpdateResponse.body.data.segments.nodes).toEqual([
      {
        id: segmentId,
        name: 'Codex Buyers',
        query: 'number_of_orders >= 1',
      },
      {
        id: duplicateResponse.body.data.segmentCreate.segment.id,
        name: 'Codex Subscribers (2)',
        query: "email_subscription_status = 'SUBSCRIBED'",
      },
    ]);
    expect(readAfterUpdateResponse.body.data.segmentsCount).toEqual({
      count: 2,
      precision: 'EXACT',
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeleteSegment($id: ID!) {
            segmentDelete(id: $id) {
              deletedSegmentId
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { id: segmentId },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.segmentDelete).toEqual({
      deletedSegmentId: segmentId,
      userErrors: [],
    });

    const readAfterDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query ReadDeletedSegment($id: ID!) {
            deleted: segment(id: $id) {
              id
            }
            segments(first: 10) {
              nodes {
                id
              }
            }
            segmentsCount {
              count
              precision
            }
          }
        `,
        variables: { id: segmentId },
      });

    expect(readAfterDeleteResponse.status).toBe(200);
    expect(readAfterDeleteResponse.body.data.deleted).toBeNull();
    expect(readAfterDeleteResponse.body.data.segments.nodes).toEqual([
      { id: duplicateResponse.body.data.segmentCreate.segment.id },
    ]);
    expect(readAfterDeleteResponse.body.data.segmentsCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });
    expect(readAfterDeleteResponse.body.errors).toEqual([
      {
        message: 'Segment does not exist',
        locations: [
          {
            line: 3,
            column: 13,
          },
        ],
        path: ['deleted'],
        extensions: {
          code: 'NOT_FOUND',
        },
      },
    ]);

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'segmentCreate',
      'segmentCreate',
      'segmentUpdate',
      'segmentDelete',
    ]);
    expect(logResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
      'staged',
    ]);
    expect(logResponse.body.entries[0].requestBody.variables.name).toBe('Codex Subscribers');

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.segments).toMatchObject({
      [duplicateResponse.body.data.segmentCreate.segment.id]: {
        name: 'Codex Subscribers (2)',
      },
    });
    expect(stateResponse.body.stagedState.deletedSegmentIds).toEqual({ [segmentId]: true });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns captured Segment userErrors and GraphQL missing-argument errors without staging records', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('segment validation must not proxy'));
    const app = createApp(config).callback();

    const invalidCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation InvalidSegment($name: String!, $query: String!) {
            segmentCreate(name: $name, query: $query) {
              segment {
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
          name: '',
          query: '',
        },
      });

    expect(invalidCreateResponse.status).toBe(200);
    expect(invalidCreateResponse.body.data.segmentCreate).toEqual({
      segment: null,
      userErrors: [
        { field: ['name'], message: "Name can't be blank" },
        { field: ['query'], message: "Query can't be blank" },
      ],
    });

    const invalidQueryResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation InvalidQuerySegment($name: String!, $query: String!) {
            segmentCreate(name: $name, query: $query) {
              segment {
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
          name: 'Invalid query',
          query: 'not a valid segment query ???',
        },
      });

    expect(invalidQueryResponse.status).toBe(200);
    expect(invalidQueryResponse.body.data.segmentCreate).toEqual({
      segment: null,
      userErrors: [
        { field: ['query'], message: "Query Line 1 Column 6: 'valid' is unexpected." },
        { field: ['query'], message: "Query Line 1 Column 4: 'a' filter cannot be found." },
      ],
    });

    const unknownUpdateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UnknownUpdate($id: ID!, $name: String) {
            segmentUpdate(id: $id, name: $name) {
              segment {
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
          id: 'gid://shopify/Segment/999999999999',
          name: 'Nope',
        },
      });

    expect(unknownUpdateResponse.status).toBe(200);
    expect(unknownUpdateResponse.body.data.segmentUpdate).toEqual({
      segment: null,
      userErrors: [{ field: ['id'], message: 'Segment does not exist' }],
    });

    const unknownDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UnknownDelete($id: ID!) {
            segmentDelete(id: $id) {
              deletedSegmentId
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          id: 'gid://shopify/Segment/999999999999',
        },
      });

    expect(unknownDeleteResponse.status).toBe(200);
    expect(unknownDeleteResponse.body.data.segmentDelete).toEqual({
      deletedSegmentId: null,
      userErrors: [{ field: ['id'], message: 'Segment does not exist' }],
    });

    const missingCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation MissingCreate {
            segmentCreate {
              segment {
                id
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
      });

    expect(missingCreateResponse.status).toBe(200);
    expect(missingCreateResponse.body).toEqual({
      errors: [
        {
          message: "Field 'segmentCreate' is missing required arguments: name, query",
          locations: [
            {
              line: 3,
              column: 13,
            },
          ],
          path: ['mutation MissingCreate', 'segmentCreate'],
          extensions: {
            code: 'missingRequiredArguments',
            className: 'Field',
            name: 'segmentCreate',
            arguments: 'name, query',
          },
        },
      ],
    });

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.segments).toEqual({});
    expect(stateResponse.body.stagedState.deletedSegmentIds).toEqual({});
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });
});
