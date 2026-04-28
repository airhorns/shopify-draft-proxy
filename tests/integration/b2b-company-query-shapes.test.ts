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
  return {
    id,
    data: {
      id,
      name: 'Snowdevil',
      note: 'B2B account',
      externalId: 'external-1',
      createdAt: '2025-03-26T19:51:37Z',
      updatedAt: '2025-03-26T19:51:38Z',
      contactsCount: { count: 1 },
      locationsCount: { count: 1 },
    },
    contactIds: ['gid://shopify/CompanyContact/300'],
    locationIds: ['gid://shopify/CompanyLocation/400'],
    contactRoleIds: ['gid://shopify/CompanyContactRole/500'],
    ...overrides,
  };
}

function makeContact(id: string, companyId: string): B2BCompanyContactRecord {
  return {
    id,
    companyId,
    data: {
      id,
      title: 'Buyer',
      locale: 'en',
      isMainContact: true,
      createdAt: '2025-03-26T19:51:38Z',
      updatedAt: '2025-03-26T19:51:38Z',
    },
  };
}

function makeLocation(id: string, companyId: string): B2BCompanyLocationRecord {
  return {
    id,
    companyId,
    data: {
      id,
      name: 'Snowdevil Calgary',
      externalId: null,
      locale: 'en',
      phone: '+16135550114',
      note: 'Warehouse',
      createdAt: '2025-03-26T19:51:37Z',
      updatedAt: '2025-03-26T19:51:38Z',
    },
  };
}

function makeRole(id: string, companyId: string): B2BCompanyContactRoleRecord {
  return {
    id,
    companyId,
    data: {
      id,
      name: 'Ordering only',
      note: 'System-defined Ordering only role',
    },
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
});
