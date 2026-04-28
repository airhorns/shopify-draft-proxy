import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import type {
  B2BCompanyContactRecord,
  B2BCompanyContactRoleRecord,
  B2BCompanyLocationRecord,
  B2BCompanyRecord,
} from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function makeCompany(id: string, overrides: Partial<B2BCompanyRecord> = {}): B2BCompanyRecord {
  const data = {
    id,
    name: 'Snowdevil',
    note: 'B2B account',
    externalId: 'external-1',
    createdAt: '2025-03-26T19:51:37Z',
    updatedAt: '2025-03-26T19:51:38Z',
    contactsCount: { count: 1 },
    locationsCount: { count: 1 },
    ...overrides.data,
  };
  return {
    id,
    contactIds: ['gid://shopify/CompanyContact/300'],
    locationIds: ['gid://shopify/CompanyLocation/400'],
    contactRoleIds: ['gid://shopify/CompanyContactRole/500'],
    ...overrides,
    data,
  };
}

function makeContact(
  id: string,
  companyId: string,
  overrides: Partial<B2BCompanyContactRecord> = {},
): B2BCompanyContactRecord {
  const data = {
    id,
    title: 'Buyer',
    locale: 'en',
    isMainContact: true,
    createdAt: '2025-03-26T19:51:38Z',
    updatedAt: '2025-03-26T19:51:38Z',
    ...overrides.data,
  };
  return {
    id,
    companyId,
    ...overrides,
    data,
  };
}

function makeLocation(
  id: string,
  companyId: string,
  overrides: Partial<B2BCompanyLocationRecord> = {},
): B2BCompanyLocationRecord {
  const data = {
    id,
    name: 'Snowdevil Calgary',
    externalId: null,
    locale: 'en',
    phone: '+16135550114',
    note: 'Warehouse',
    createdAt: '2025-03-26T19:51:37Z',
    updatedAt: '2025-03-26T19:51:38Z',
    ...overrides.data,
  };
  return {
    id,
    companyId,
    ...overrides,
    data,
  };
}

function makeRole(
  id: string,
  companyId: string,
  overrides: Partial<B2BCompanyContactRoleRecord> = {},
): B2BCompanyContactRoleRecord {
  const data = {
    id,
    name: 'Ordering only',
    note: 'System-defined Ordering only role',
    ...overrides.data,
  };
  return {
    id,
    companyId,
    ...overrides,
    data,
  };
}

describe('B2B company query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves company catalog, count, detail, contact, role, and location reads from snapshot state', async () => {
    const companyId = 'gid://shopify/Company/200';
    const contactId = 'gid://shopify/CompanyContact/300';
    const locationId = 'gid://shopify/CompanyLocation/400';
    const roleId = 'gid://shopify/CompanyContactRole/500';
    store.upsertBaseB2BCompanies([makeCompany(companyId)]);
    store.upsertBaseB2BCompanyContacts([makeContact(contactId, companyId)]);
    store.upsertBaseB2BCompanyLocations([makeLocation(locationId, companyId)]);
    store.upsertBaseB2BCompanyContactRoles([makeRole(roleId, companyId)]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('B2B company reads should resolve locally in snapshot mode');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query B2BCompanyReads($companyId: ID!, $contactId: ID!, $roleId: ID!, $locationId: ID!) {
          companies(first: 5) {
            nodes {
              id
              name
              contactsCount { count }
              locationsCount { count }
              mainContact { id title isMainContact }
              defaultRole { id name note }
              contacts(first: 5) { nodes { id title locale isMainContact } pageInfo { hasNextPage hasPreviousPage } }
              locations(first: 5) { nodes { id name phone note } pageInfo { hasNextPage hasPreviousPage } }
              contactRoles(first: 5) { nodes { id name note } pageInfo { hasNextPage hasPreviousPage } }
            }
            pageInfo { hasNextPage hasPreviousPage }
          }
          companiesCount { count precision }
          company(id: $companyId) { id name note externalId }
          companyContact(id: $contactId) { id title company { id name } }
          companyContactRole(id: $roleId) { id name note }
          companyLocation(id: $locationId) { id name phone company { id name } }
          companyLocations(first: 5) {
            nodes { id name phone company { id name } }
            pageInfo { hasNextPage hasPreviousPage }
          }
        }`,
        variables: { companyId, contactId, roleId, locationId },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        companies: {
          nodes: [
            {
              id: companyId,
              name: 'Snowdevil',
              contactsCount: { count: 1 },
              locationsCount: { count: 1 },
              mainContact: { id: contactId, title: 'Buyer', isMainContact: true },
              defaultRole: { id: roleId, name: 'Ordering only', note: 'System-defined Ordering only role' },
              contacts: {
                nodes: [{ id: contactId, title: 'Buyer', locale: 'en', isMainContact: true }],
                pageInfo: { hasNextPage: false, hasPreviousPage: false },
              },
              locations: {
                nodes: [{ id: locationId, name: 'Snowdevil Calgary', phone: '+16135550114', note: 'Warehouse' }],
                pageInfo: { hasNextPage: false, hasPreviousPage: false },
              },
              contactRoles: {
                nodes: [{ id: roleId, name: 'Ordering only', note: 'System-defined Ordering only role' }],
                pageInfo: { hasNextPage: false, hasPreviousPage: false },
              },
            },
          ],
          pageInfo: { hasNextPage: false, hasPreviousPage: false },
        },
        companiesCount: { count: 1, precision: 'EXACT' },
        company: { id: companyId, name: 'Snowdevil', note: 'B2B account', externalId: 'external-1' },
        companyContact: { id: contactId, title: 'Buyer', company: { id: companyId, name: 'Snowdevil' } },
        companyContactRole: { id: roleId, name: 'Ordering only', note: 'System-defined Ordering only role' },
        companyLocation: {
          id: locationId,
          name: 'Snowdevil Calgary',
          phone: '+16135550114',
          company: { id: companyId, name: 'Snowdevil' },
        },
        companyLocations: {
          nodes: [
            {
              id: locationId,
              name: 'Snowdevil Calgary',
              phone: '+16135550114',
              company: { id: companyId, name: 'Snowdevil' },
            },
          ],
          pageInfo: { hasNextPage: false, hasPreviousPage: false },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('serializes B2B connection edges, cursors, and nested resource windows from snapshot state', async () => {
    const firstCompanyId = 'gid://shopify/Company/100';
    const secondCompanyId = 'gid://shopify/Company/200';
    const firstLocationId = 'gid://shopify/CompanyLocation/101';
    const secondLocationId = 'gid://shopify/CompanyLocation/201';
    const firstRoleId = 'gid://shopify/CompanyContactRole/102';
    const secondRoleId = 'gid://shopify/CompanyContactRole/202';
    const secondContactId = 'gid://shopify/CompanyContact/203';

    store.upsertBaseB2BCompanies([
      makeCompany(firstCompanyId, {
        data: {
          name: 'Powderbound',
          contactsCount: { count: 0 },
          locationsCount: { count: 1 },
        },
        contactIds: [],
        locationIds: [firstLocationId],
        contactRoleIds: [firstRoleId],
      }),
      makeCompany(secondCompanyId, {
        contactIds: [secondContactId],
        locationIds: [secondLocationId],
        contactRoleIds: [secondRoleId],
      }),
    ]);
    store.upsertBaseB2BCompanyContacts([makeContact(secondContactId, secondCompanyId)]);
    store.upsertBaseB2BCompanyLocations([
      makeLocation(firstLocationId, firstCompanyId, { data: { name: 'Powderbound HQ', phone: null } }),
      makeLocation(secondLocationId, secondCompanyId),
    ]);
    store.upsertBaseB2BCompanyContactRoles([
      makeRole(firstRoleId, firstCompanyId, { data: { name: 'Location admin' } }),
      makeRole(secondRoleId, secondCompanyId),
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('B2B connection reads should resolve locally in snapshot mode');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query B2BConnectionWindows($afterCompany: String!, $afterLocation: String!) {
          firstCompanyPage: companies(first: 1) {
            edges {
              cursor
              node {
                id
                name
                contactRoles(first: 1) {
                  edges { cursor node { id name } }
                  pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                }
                locations(first: 1) {
                  edges { cursor node { id name phone } }
                  pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                }
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          nextCompanyPage: companies(first: 1, after: $afterCompany) {
            edges { cursor node { id name mainContact { id title } } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          nextLocationPage: companyLocations(first: 1, after: $afterLocation) {
            edges { cursor node { id name company { id name } } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: {
          afterCompany: `cursor:${firstCompanyId}`,
          afterLocation: `cursor:${firstLocationId}`,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        firstCompanyPage: {
          edges: [
            {
              cursor: `cursor:${firstCompanyId}`,
              node: {
                id: firstCompanyId,
                name: 'Powderbound',
                contactRoles: {
                  edges: [{ cursor: `cursor:${firstRoleId}`, node: { id: firstRoleId, name: 'Location admin' } }],
                  pageInfo: {
                    hasNextPage: false,
                    hasPreviousPage: false,
                    startCursor: `cursor:${firstRoleId}`,
                    endCursor: `cursor:${firstRoleId}`,
                  },
                },
                locations: {
                  edges: [
                    {
                      cursor: `cursor:${firstLocationId}`,
                      node: { id: firstLocationId, name: 'Powderbound HQ', phone: null },
                    },
                  ],
                  pageInfo: {
                    hasNextPage: false,
                    hasPreviousPage: false,
                    startCursor: `cursor:${firstLocationId}`,
                    endCursor: `cursor:${firstLocationId}`,
                  },
                },
              },
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: false,
            startCursor: `cursor:${firstCompanyId}`,
            endCursor: `cursor:${firstCompanyId}`,
          },
        },
        nextCompanyPage: {
          edges: [
            {
              cursor: `cursor:${secondCompanyId}`,
              node: {
                id: secondCompanyId,
                name: 'Snowdevil',
                mainContact: { id: secondContactId, title: 'Buyer' },
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: true,
            startCursor: `cursor:${secondCompanyId}`,
            endCursor: `cursor:${secondCompanyId}`,
          },
        },
        nextLocationPage: {
          edges: [
            {
              cursor: `cursor:${secondLocationId}`,
              node: {
                id: secondLocationId,
                name: 'Snowdevil Calgary',
                company: { id: secondCompanyId, name: 'Snowdevil' },
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: true,
            startCursor: `cursor:${secondLocationId}`,
            endCursor: `cursor:${secondLocationId}`,
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns Shopify-like empty B2B company reads when snapshot state has no companies', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('empty B2B company reads should resolve locally in snapshot mode');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query EmptyB2BCompanyReads($id: ID!) {
          companies(first: 5) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          companiesCount { count precision }
          company(id: $id) { id }
          companyContact(id: $id) { id }
          companyContactRole(id: $id) { id }
          companyLocation(id: $id) { id }
          companyLocations(first: 5) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
        }`,
        variables: { id: 'gid://shopify/Company/999999999' },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        companies: {
          nodes: [],
          pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
        },
        companiesCount: { count: 0, precision: 'EXACT' },
        company: null,
        companyContact: null,
        companyContactRole: null,
        companyLocation: null,
        companyLocations: {
          nodes: [],
          pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('preserves upstream B2B read access errors in live-hybrid mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async (_input, init) => {
      expect(init?.headers).toMatchObject({
        'x-shopify-access-token': 'shpat_b2b_scope_probe',
      });

      return new Response(
        JSON.stringify({
          data: {
            companies: null,
            companiesCount: null,
          },
          errors: [
            {
              message: 'Access denied for companies field. Required access: `read_companies` access scope.',
              path: ['companies'],
              extensions: { code: 'ACCESS_DENIED' },
            },
            {
              message: 'Access denied for companiesCount field. Required access: `read_companies` access scope.',
              path: ['companiesCount'],
              extensions: { code: 'ACCESS_DENIED' },
            },
          ],
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('X-Shopify-Access-Token', 'shpat_b2b_scope_probe')
      .send({
        query: `query B2BAccessProbe {
          companies(first: 1) { nodes { id } }
          companiesCount { count }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        companies: null,
        companiesCount: null,
      },
      errors: [
        {
          message: 'Access denied for companies field. Required access: `read_companies` access scope.',
          path: ['companies'],
          extensions: { code: 'ACCESS_DENIED' },
        },
        {
          message: 'Access denied for companiesCount field. Required access: `read_companies` access scope.',
          path: ['companiesCount'],
          extensions: { code: 'ACCESS_DENIED' },
        },
      ],
    });
    expect(fetchSpy).toHaveBeenCalledOnce();
  });
});
