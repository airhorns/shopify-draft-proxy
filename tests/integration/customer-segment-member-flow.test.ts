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

describe('customer segment member query jobs and reads', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages member query jobs and exposes staged customer members through queryId pagination', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('segment member flow must not proxy'));
    const app = createApp(config).callback();
    const tag = 'codex-har217-members';

    for (const index of [1, 2, 3]) {
      const response = await request(app)
        .post('/admin/api/2025-01/graphql.json')
        .send({
          query: `mutation CustomerCreate($input: CustomerInput!) {
            customerCreate(input: $input) {
              customer {
                id
              }
              userErrors {
                field
                message
              }
            }
          }`,
          variables: {
            input: {
              email: `codex-har217-${index}@example.com`,
              firstName: `Codex${index}`,
              lastName: 'Har217',
              tags: [tag],
            },
          },
        });

      expect(response.status).toBe(200);
      expect(response.body.data.customerCreate.userErrors).toEqual([]);
    }

    const createQueryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CreateMembersQuery($input: CustomerSegmentMembersQueryInput!) {
          customerSegmentMembersQueryCreate(input: $input) {
            customerSegmentMembersQuery {
              id
              currentCount
              done
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            query: `customer_tags CONTAINS '${tag}'`,
          },
        },
      });

    expect(createQueryResponse.status).toBe(200);
    expect(createQueryResponse.body.data.customerSegmentMembersQueryCreate).toEqual({
      customerSegmentMembersQuery: {
        id: 'gid://shopify/CustomerSegmentMembersQuery/7',
        currentCount: 0,
        done: false,
      },
      userErrors: [],
    });

    const queryId = createQueryResponse.body.data.customerSegmentMembersQueryCreate.customerSegmentMembersQuery.id;
    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query ReadMembers($id: ID!, $first: Int!, $after: String) {
          job: customerSegmentMembersQuery(id: $id) {
            id
            currentCount
            done
          }
          members: customerSegmentMembers(queryId: $id, first: $first, after: $after) {
            totalCount
            statistics {
              amountSpent: attributeStatistics(attributeName: "amount_spent") {
                average
                sum
              }
            }
            edges {
              cursor
              node {
                id
                displayName
                firstName
                lastName
                defaultEmailAddress {
                  emailAddress
                }
                numberOfOrders
                amountSpent {
                  amount
                  currencyCode
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
        }`,
        variables: {
          id: queryId,
          first: 2,
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.job).toEqual({
      id: queryId,
      currentCount: 3,
      done: true,
    });
    expect(readResponse.body.data.members).toMatchObject({
      totalCount: 3,
      statistics: {
        amountSpent: {
          average: 0,
          sum: 0,
        },
      },
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: false,
      },
    });
    expect(readResponse.body.data.members.edges).toHaveLength(2);
    expect(readResponse.body.data.members.edges[0].node).toMatchObject({
      id: expect.stringMatching(/^gid:\/\/shopify\/CustomerSegmentMember\//),
      displayName: 'Codex3 Har217',
      firstName: 'Codex3',
      lastName: 'Har217',
      defaultEmailAddress: {
        emailAddress: 'codex-har217-3@example.com',
      },
      numberOfOrders: '0',
      amountSpent: {
        amount: '0.0',
        currencyCode: 'USD',
      },
    });

    const secondPageResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query ReadMembers($id: ID!, $first: Int!, $after: String) {
          customerSegmentMembers(queryId: $id, first: $first, after: $after) {
            edges {
              cursor
              node {
                firstName
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
        }`,
        variables: {
          id: queryId,
          first: 2,
          after: readResponse.body.data.members.pageInfo.endCursor,
        },
      });

    expect(secondPageResponse.status).toBe(200);
    expect(secondPageResponse.body.data.customerSegmentMembers.edges).toEqual([
      {
        cursor: expect.any(String),
        node: {
          firstName: 'Codex1',
        },
      },
    ]);
    expect(secondPageResponse.body.data.customerSegmentMembers.pageInfo).toMatchObject({
      hasNextPage: false,
      hasPreviousPage: true,
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.at(-1)).toMatchObject({
      operationName: 'customerSegmentMembersQueryCreate',
      status: 'staged',
      requestBody: {
        variables: {
          input: {
            query: `customer_tags CONTAINS '${tag}'`,
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('evaluates staged segment definitions for member reads and membership lookup', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('segment membership flow must not proxy'));
    const app = createApp(config).callback();

    const customerResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            email: 'numbered-segment-member@example.com',
            firstName: 'Numbered',
            tags: [],
          },
        },
      });
    const customerId = customerResponse.body.data.customerCreate.customer.id as string;

    const segmentResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation SegmentCreate($name: String!, $query: String!) {
          segmentCreate(name: $name, query: $query) {
            segment {
              id
              query
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          name: 'Zero-order customers',
          query: 'number_of_orders = 0',
        },
      });
    const segmentId = segmentResponse.body.data.segmentCreate.segment.id as string;

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query SegmentMembers($segmentId: ID!, $customerId: ID!, $missingSegmentId: ID!) {
          members: customerSegmentMembers(segmentId: $segmentId, first: 5) {
            totalCount
            edges {
              node {
                firstName
              }
            }
          }
          membership: customerSegmentMembership(customerId: $customerId, segmentIds: [$segmentId, $missingSegmentId]) {
            memberships {
              segmentId
              isMember
            }
          }
          missingCustomer: customerSegmentMembership(customerId: "gid://shopify/Customer/999999999999", segmentIds: [$segmentId]) {
            memberships {
              segmentId
              isMember
            }
          }
        }`,
        variables: {
          segmentId,
          customerId,
          missingSegmentId: 'gid://shopify/Segment/999999999999',
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.members).toEqual({
      totalCount: 1,
      edges: [
        {
          node: {
            firstName: 'Numbered',
          },
        },
      ],
    });
    expect(readResponse.body.data.membership.memberships).toEqual([
      {
        segmentId,
        isMember: true,
      },
    ]);
    expect(readResponse.body.data.missingCustomer.memberships).toEqual([
      {
        segmentId,
        isMember: false,
      },
    ]);
  });

  it('returns captured empty/no-data and validation branches for member reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('segment member validation must not proxy'));
    const app = createApp(config).callback();

    const invalidQueryCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InvalidMemberQuery($input: CustomerSegmentMembersQueryInput!) {
          customerSegmentMembersQueryCreate(input: $input) {
            customerSegmentMembersQuery {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            query: 'not a valid segment query ???',
          },
        },
      });

    expect(invalidQueryCreateResponse.status).toBe(200);
    expect(invalidQueryCreateResponse.body.data.customerSegmentMembersQueryCreate).toEqual({
      customerSegmentMembersQuery: null,
      userErrors: [
        {
          field: null,
          message: "Line 1 Column 6: 'valid' is unexpected.",
        },
      ],
    });

    const emptyMembersResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query EmptyMembers($query: String!) {
          customerSegmentMembers(query: $query, first: 2) {
            totalCount
            statistics {
              amountSpent: attributeStatistics(attributeName: "amount_spent") {
                average
                sum
              }
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
        }`,
        variables: {
          query: 'number_of_orders > 999999',
        },
      });

    expect(emptyMembersResponse.status).toBe(200);
    expect(emptyMembersResponse.body).toEqual({
      data: {
        customerSegmentMembers: {
          totalCount: 0,
          statistics: {
            amountSpent: {
              average: 0,
              sum: 0,
            },
          },
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

    const missingQueryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query MissingQuery($id: ID!, $queryId: ID!) {
          customerSegmentMembersQuery(id: $id) {
            id
          }
          customerSegmentMembers(queryId: $queryId, first: 2) {
            totalCount
          }
        }`,
        variables: {
          id: 'gid://shopify/CustomerSegmentMembersQuery/999999999999',
          queryId: 'gid://shopify/CustomerSegmentMembersQuery/999999999999',
        },
      });

    expect(missingQueryResponse.status).toBe(200);
    expect(missingQueryResponse.body.data).toBeNull();
    expect(missingQueryResponse.body.errors).toEqual([
      {
        message: 'Something went wrong',
        locations: [
          {
            line: 2,
            column: 11,
          },
        ],
        extensions: {
          code: 'INTERNAL_SERVER_ERROR',
        },
        path: ['customerSegmentMembersQuery'],
      },
      {
        message: 'this async query cannot be found in segmentMembers',
        locations: [
          {
            line: 5,
            column: 11,
          },
        ],
        path: ['customerSegmentMembers'],
      },
    ]);
  });
});
