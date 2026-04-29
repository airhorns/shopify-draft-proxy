import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { AppConfig } from '../../src/config.js';
import { createApp, resetSyntheticIdentity, store } from '../support/runtime.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

describe('B2B company lifecycle mutations', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages company, contact, location, role, staff, address, tax, and delete lifecycles locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('supported B2B mutations should not proxy to Shopify');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateCompany($input: CompanyCreateInput!) {
          companyCreate(input: $input) {
            company {
              id
              name
              note
              externalId
              contactsCount { count }
              locationsCount { count }
              mainContact {
                id
                title
                isMainContact
                email
                roleAssignments(first: 5) { nodes { id role { id name } companyLocation { id name } } }
              }
              locations(first: 5) { nodes { id name phone billingAddress { id city countryCode } } }
              contactRoles(first: 5) { nodes { id name note } }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            company: { name: 'Acme B2B', note: 'Draft account', externalId: 'acme-1' },
            companyContact: { firstName: 'Ada', lastName: 'Lovelace', email: 'ada@example.com', title: 'Buyer' },
            companyLocation: {
              name: 'Acme HQ',
              phone: '+16135550101',
              billingAddress: { address1: '1 B2B Way', city: 'Ottawa', countryCode: 'CA' },
            },
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.companyCreate.userErrors).toEqual([]);
    const company = createResponse.body.data.companyCreate.company;
    const companyId = company.id as string;
    const contactId = company.mainContact.id as string;
    const locationId = company.locations.nodes[0].id as string;
    const roleId = company.contactRoles.nodes[0].id as string;
    const defaultRole = company.contactRoles.nodes[1] as { id: string; name: string };
    const assignedRole = company.contactRoles.nodes[0] as { id: string; name: string };
    const defaultAssignmentId = company.mainContact.roleAssignments.nodes[0].id as string;
    expect(company.mainContact.roleAssignments.nodes).toEqual([
      {
        id: defaultAssignmentId,
        role: { id: defaultRole.id, name: 'Ordering only' },
        companyLocation: { id: locationId, name: 'Acme HQ' },
      },
    ]);
    expect(company).toMatchObject({
      name: 'Acme B2B',
      note: 'Draft account',
      externalId: 'acme-1',
      contactsCount: { count: 1 },
      locationsCount: { count: 1 },
      mainContact: { title: 'Buyer', isMainContact: true, email: 'ada@example.com' },
    });

    const assignmentLocationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation AssignmentLocation($companyId: ID!) {
          companyLocationCreate(companyId: $companyId, input: { name: "Acme Role Site" }) {
            companyLocation { id name }
            userErrors { field message code }
          }
        }`,
        variables: { companyId },
      });
    const assignmentLocationId = assignmentLocationResponse.body.data.companyLocationCreate.companyLocation
      .id as string;

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateB2B($companyId: ID!, $contactId: ID!, $locationId: ID!) {
          companyUpdate(companyId: $companyId, input: { name: "Acme Wholesale", note: "Updated" }) {
            company { id name note }
            userErrors { field message code }
          }
          companyContactUpdate(companyContactId: $contactId, input: { title: "Approver", phone: "+16135550102" }) {
            companyContact { id title phone }
            userErrors { field message code }
          }
          companyLocationUpdate(companyLocationId: $locationId, input: { name: "Acme Warehouse", note: "Dock" }) {
            companyLocation { id name note }
            userErrors { field message code }
          }
        }`,
        variables: { companyId, contactId, locationId },
      });

    expect(updateResponse.body.data.companyUpdate.company).toEqual({
      id: companyId,
      name: 'Acme Wholesale',
      note: 'Updated',
    });
    expect(updateResponse.body.data.companyContactUpdate.companyContact).toEqual({
      id: contactId,
      title: 'Approver',
      phone: '+16135550102',
    });
    expect(updateResponse.body.data.companyLocationUpdate.companyLocation).toEqual({
      id: locationId,
      name: 'Acme Warehouse',
      note: 'Dock',
    });

    const assignmentResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation AssignRole($contactId: ID!, $roleId: ID!, $locationId: ID!) {
          companyContactAssignRole(
            companyContactId: $contactId
            companyContactRoleId: $roleId
            companyLocationId: $locationId
          ) {
            companyContactRoleAssignment { id role { id name } companyLocation { id name } }
            userErrors { field message code }
          }
        }`,
        variables: { contactId, roleId, locationId: assignmentLocationId },
      });

    const assignmentId = assignmentResponse.body.data.companyContactAssignRole.companyContactRoleAssignment
      .id as string;
    expect(assignmentResponse.body.data.companyContactAssignRole.userErrors).toEqual([]);

    const locationSettingsResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation LocationSettings($locationId: ID!, $address: CompanyAddressInput!) {
          companyLocationAssignStaffMembers(companyLocationId: $locationId, staffMemberIds: ["gid://shopify/StaffMember/1"]) {
            companyLocationStaffMemberAssignments { id staffMember { id } }
            userErrors { field message code }
          }
          companyLocationAssignAddress(locationId: $locationId, address: $address, addressTypes: [BILLING, SHIPPING]) {
            addresses { id city countryCode }
            userErrors { field message code }
          }
          companyLocationTaxSettingsUpdate(
            companyLocationId: $locationId
            taxRegistrationId: "123456789"
            taxExempt: true
            exemptionsToAssign: [CA_STATUS_CARD_EXEMPTION]
          ) {
            companyLocation { id taxRegistrationId taxExempt taxExemptions }
            userErrors { field message code }
          }
        }`,
        variables: {
          locationId,
          address: { address1: '2 B2B Way', city: 'Toronto', countryCode: 'CA' },
        },
      });

    const staffAssignmentId =
      locationSettingsResponse.body.data.companyLocationAssignStaffMembers.companyLocationStaffMemberAssignments[0].id;
    const addressId = locationSettingsResponse.body.data.companyLocationAssignAddress.addresses[0].id;
    expect(locationSettingsResponse.body.data.companyLocationTaxSettingsUpdate.companyLocation).toEqual({
      id: locationId,
      taxRegistrationId: '123456789',
      taxExempt: true,
      taxExemptions: ['CA_STATUS_CARD_EXEMPTION'],
    });

    const postAssignmentUpdateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateAfterAssignments($contactId: ID!, $locationId: ID!) {
          companyContactUpdate(companyContactId: $contactId, input: { title: "Lead buyer" }) {
            companyContact { id title }
            userErrors { field message code }
          }
          companyLocationUpdate(companyLocationId: $locationId, input: { name: "Acme Fulfillment" }) {
            companyLocation { id name }
            userErrors { field message code }
          }
        }`,
        variables: { contactId, locationId },
      });

    expect(postAssignmentUpdateResponse.body.data.companyContactUpdate.companyContact).toEqual({
      id: contactId,
      title: 'Lead buyer',
    });
    expect(postAssignmentUpdateResponse.body.data.companyLocationUpdate.companyLocation).toEqual({
      id: locationId,
      name: 'Acme Fulfillment',
    });

    const readAfterWrite = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadAfterWrite($companyId: ID!, $contactId: ID!, $locationId: ID!) {
          company(id: $companyId) {
            id
            name
            contactsCount { count }
            locationsCount { count }
            contacts(first: 5) {
              nodes {
                id
                title
                roleAssignments(first: 5) {
                  nodes {
                    id
                    role { id name }
                    companyContact { id title }
                    companyLocation { id name }
                  }
                }
              }
            }
            locations(first: 5) {
              nodes {
                id
                name
                billingAddress { id city countryCode }
                shippingAddress { id city countryCode }
                taxRegistrationId
                taxExempt
                taxExemptions
                roleAssignments(first: 5) {
                  nodes {
                    id
                    role { id name }
                    companyContact { id title }
                    companyLocation { id name }
                  }
                }
                staffMemberAssignments(first: 5) {
                  nodes {
                    id
                    staffMember { id }
                    companyLocation { id name }
                  }
                }
              }
            }
          }
          companyContact(id: $contactId) { id title company { id name } }
          companyLocation(id: $locationId) {
            id
            name
            taxRegistrationId
            taxExempt
            taxExemptions
            company { id name }
          }
        }`,
        variables: { companyId, contactId, locationId },
      });

    expect(readAfterWrite.body.data.company).toMatchObject({
      id: companyId,
      name: 'Acme Wholesale',
      contactsCount: { count: 1 },
      locationsCount: { count: 2 },
    });
    const contactRoleAssignments = readAfterWrite.body.data.company.contacts.nodes[0].roleAssignments.nodes;
    expect(contactRoleAssignments.map((item: { id: string }) => item.id)).toEqual([defaultAssignmentId, assignmentId]);
    expect(contactRoleAssignments[1]).toMatchObject({
      companyContact: { id: contactId, title: 'Lead buyer' },
      companyLocation: { id: assignmentLocationId, name: 'Acme Role Site' },
    });
    const mainLocation = readAfterWrite.body.data.company.locations.nodes.find(
      (location: { id: string }) => location.id === locationId,
    );
    const roleLocation = readAfterWrite.body.data.company.locations.nodes.find(
      (location: { id: string }) => location.id === assignmentLocationId,
    );
    expect(mainLocation.billingAddress).toEqual({
      id: addressId,
      city: 'Toronto',
      countryCode: 'CA',
    });
    expect(mainLocation.shippingAddress).toEqual({
      id: addressId,
      city: 'Toronto',
      countryCode: 'CA',
    });
    expect(mainLocation).toMatchObject({
      taxRegistrationId: '123456789',
      taxExempt: true,
      taxExemptions: ['CA_STATUS_CARD_EXEMPTION'],
    });
    expect(mainLocation.roleAssignments.nodes[0]).toMatchObject({
      id: defaultAssignmentId,
      companyContact: { id: contactId, title: 'Lead buyer' },
      companyLocation: { id: locationId, name: 'Acme Fulfillment' },
    });
    expect(roleLocation.roleAssignments.nodes[0]).toMatchObject({
      id: assignmentId,
      companyContact: { id: contactId, title: 'Lead buyer' },
      companyLocation: { id: assignmentLocationId, name: 'Acme Role Site' },
    });
    expect(mainLocation.staffMemberAssignments.nodes[0].id).toBe(staffAssignmentId);
    expect(mainLocation.staffMemberAssignments.nodes[0].companyLocation).toEqual({
      id: locationId,
      name: 'Acme Fulfillment',
    });
    expect(readAfterWrite.body.data.companyContact).toMatchObject({ id: contactId, title: 'Lead buyer' });
    expect(readAfterWrite.body.data.companyLocation).toMatchObject({
      id: locationId,
      name: 'Acme Fulfillment',
      taxRegistrationId: '123456789',
      taxExempt: true,
      taxExemptions: ['CA_STATUS_CARD_EXEMPTION'],
    });

    const nodeReadResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query B2BNestedNodes($ids: [ID!]!) {
          nodes(ids: $ids) {
            ... on CompanyAddress {
              id
              city
              countryCode
            }
            ... on CompanyContactRoleAssignment {
              id
              companyContact { id title }
              role { id name }
              companyLocation { id name }
            }
          }
        }`,
        variables: {
          ids: [addressId, defaultAssignmentId, assignmentId, 'gid://shopify/CompanyAddress/missing'],
        },
      });

    expect(nodeReadResponse.body.data.nodes).toEqual([
      {
        id: addressId,
        city: 'Toronto',
        countryCode: 'CA',
      },
      {
        id: defaultAssignmentId,
        companyContact: { id: contactId, title: 'Lead buyer' },
        role: { id: defaultRole.id, name: defaultRole.name },
        companyLocation: { id: locationId, name: 'Acme Fulfillment' },
      },
      {
        id: assignmentId,
        companyContact: { id: contactId, title: 'Lead buyer' },
        role: { id: assignedRole.id, name: assignedRole.name },
        companyLocation: { id: assignmentLocationId, name: 'Acme Role Site' },
      },
      null,
    ]);

    const secondContactResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation MainContact($companyId: ID!) {
          companyContactCreate(companyId: $companyId, input: { email: "second@example.com", title: "Backup" }) {
            companyContact { id title isMainContact }
            userErrors { field message code }
          }
        }`,
        variables: { companyId },
      });
    const secondContactId = secondContactResponse.body.data.companyContactCreate.companyContact.id as string;

    const mainContactResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation MainContact($companyId: ID!, $contactId: ID!) {
          companyAssignMainContact(companyId: $companyId, companyContactId: $contactId) {
            company { mainContact { id isMainContact } }
            userErrors { field message code }
          }
        }`,
        variables: { companyId, contactId: secondContactId },
      });

    expect(mainContactResponse.body.data.companyAssignMainContact.company.mainContact).toEqual({
      id: secondContactId,
      isMainContact: true,
    });
    const revokeMainContactResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation RevokeMainContact($companyId: ID!) {
          companyRevokeMainContact(companyId: $companyId) {
            company { mainContact { id isMainContact } }
            userErrors { field message code }
          }
        }`,
        variables: { companyId },
      });

    expect(revokeMainContactResponse.body.data.companyRevokeMainContact.company.mainContact).toBeNull();

    const extraLifecycleResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ExtraB2BRoots($companyId: ID!, $contactId: ID!, $roleId: ID!) {
          companyLocationCreate(companyId: $companyId, input: { name: "Acme Annex" }) {
            companyLocation { id name }
            userErrors { field message code }
          }
          companyAssignCustomerAsContact(companyId: $companyId, customerId: "gid://shopify/Customer/900") {
            companyContact { id customer { id } }
            userErrors { field message code }
          }
          companyContactCreate(companyId: $companyId, input: { email: "delete@example.com", title: "Delete me" }) {
            companyContact { id title }
            userErrors { field message code }
          }
        }`,
        variables: { companyId, contactId, roleId },
      });

    const extraLocationId = extraLifecycleResponse.body.data.companyLocationCreate.companyLocation.id as string;
    const assignedCustomerContactId = extraLifecycleResponse.body.data.companyAssignCustomerAsContact.companyContact
      .id as string;
    const deleteContactId = extraLifecycleResponse.body.data.companyContactCreate.companyContact.id as string;
    expect(extraLifecycleResponse.body.data.companyLocationCreate.userErrors).toEqual([]);

    const locationBulkLocationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation LocationBulkLocation($companyId: ID!) {
          companyLocationCreate(companyId: $companyId, input: { name: "Acme Location Bulk" }) {
            companyLocation { id name }
            userErrors { field message code }
          }
        }`,
        variables: { companyId },
      });
    const locationBulkLocationId = locationBulkLocationResponse.body.data.companyLocationCreate.companyLocation
      .id as string;

    const bulkRoleResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation BulkRoleRoots($contactId: ID!, $roleId: ID!, $extraLocationId: ID!, $locationBulkLocationId: ID!) {
          companyContactAssignRoles(
            companyContactId: $contactId
            rolesToAssign: [{ companyContactRoleId: $roleId, companyLocationId: $extraLocationId }]
          ) {
            roleAssignments { id }
            userErrors { field message code }
          }
          companyLocationAssignRoles(
            companyLocationId: $locationBulkLocationId
            rolesToAssign: [{ companyContactId: $contactId, companyContactRoleId: $roleId }]
          ) {
            roleAssignments { id }
            userErrors { field message code }
          }
        }`,
        variables: { contactId, roleId, extraLocationId, locationBulkLocationId },
      });
    const contactBulkAssignmentId = bulkRoleResponse.body.data.companyContactAssignRoles.roleAssignments[0]
      .id as string;
    expect(bulkRoleResponse.body.data.companyLocationAssignRoles.userErrors).toEqual([]);
    const locationBulkAssignmentId = bulkRoleResponse.body.data.companyLocationAssignRoles.roleAssignments[0]
      .id as string;

    const revokeBulkRoleResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation RevokeBulkRoleRoots($contactId: ID!, $locationId: ID!, $contactAssignmentId: ID!, $locationAssignmentId: ID!) {
          companyContactRevokeRoles(companyContactId: $contactId, roleAssignmentIds: [$contactAssignmentId]) {
            revokedRoleAssignmentIds
            userErrors { field message code }
          }
          companyLocationRevokeRoles(companyLocationId: $locationId, rolesToRevoke: [$locationAssignmentId]) {
            revokedRoleAssignmentIds
            userErrors { field message code }
          }
        }`,
        variables: {
          contactId,
          locationId: locationBulkLocationId,
          contactAssignmentId: contactBulkAssignmentId,
          locationAssignmentId: locationBulkAssignmentId,
        },
      });
    expect(revokeBulkRoleResponse.body.data.companyContactRevokeRoles.revokedRoleAssignmentIds).toEqual([
      contactBulkAssignmentId,
    ]);
    expect(revokeBulkRoleResponse.body.data.companyLocationRevokeRoles.revokedRoleAssignmentIds).toEqual([
      locationBulkAssignmentId,
    ]);

    const extraDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ExtraDeleteRoots($assignedCustomerContactId: ID!, $deleteContactId: ID!, $extraLocationId: ID!) {
          companyContactRemoveFromCompany(companyContactId: $assignedCustomerContactId) {
            removedCompanyContactId
            userErrors { field message code }
          }
          companyContactDelete(companyContactId: $deleteContactId) {
            deletedCompanyContactId
            userErrors { field message code }
          }
          companyLocationDelete(companyLocationId: $extraLocationId) {
            deletedCompanyLocationId
            userErrors { field message code }
          }
        }`,
        variables: { assignedCustomerContactId, deleteContactId, extraLocationId },
      });
    expect(extraDeleteResponse.body.data.companyContactRemoveFromCompany.removedCompanyContactId).toBe(
      assignedCustomerContactId,
    );
    expect(extraDeleteResponse.body.data.companyContactDelete.deletedCompanyContactId).toBe(deleteContactId);
    expect(extraDeleteResponse.body.data.companyLocationDelete.deletedCompanyLocationId).toBe(extraLocationId);

    const bulkCompanyResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation BulkCompanyDelete($input: CompanyCreateInput!) {
          companyCreate(input: $input) {
            company { id }
            userErrors { field message code }
          }
        }`,
        variables: { input: { company: { name: 'Bulk delete company' } } },
      });
    const bulkCompanyId = bulkCompanyResponse.body.data.companyCreate.company.id as string;
    const companiesDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CompaniesDelete($companyIds: [ID!]!) {
          companiesDelete(companyIds: $companyIds) {
            deletedCompanyIds
            userErrors { field message code }
          }
        }`,
        variables: { companyIds: [bulkCompanyId] },
      });
    expect(companiesDeleteResponse.body.data.companiesDelete.deletedCompanyIds).toEqual([bulkCompanyId]);

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteB2B($assignmentId: ID!, $contactId: ID!, $locationId: ID!, $staffAssignmentId: ID!, $addressId: ID!, $companyId: ID!) {
          companyContactRevokeRole(companyContactId: $contactId, companyContactRoleAssignmentId: $assignmentId) {
            revokedCompanyContactRoleAssignmentId
            userErrors { field message code }
          }
          companyLocationRemoveStaffMembers(companyLocationStaffMemberAssignmentIds: [$staffAssignmentId]) {
            deletedCompanyLocationStaffMemberAssignmentIds
            userErrors { field message code }
          }
          companyAddressDelete(addressId: $addressId) {
            deletedAddressId
            userErrors { field message code }
          }
          companyContactsDelete(companyContactIds: [$contactId]) {
            deletedCompanyContactIds
            userErrors { field message code }
          }
          companyLocationsDelete(companyLocationIds: [$locationId]) {
            deletedCompanyLocationIds
            userErrors { field message code }
          }
          companyDelete(id: $companyId) {
            deletedCompanyId
            userErrors { field message code }
          }
        }`,
        variables: { assignmentId, contactId, locationId, staffAssignmentId, addressId, companyId },
      });

    expect(deleteResponse.body.data.companyDelete).toEqual({ deletedCompanyId: companyId, userErrors: [] });

    const emptyReads = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query EmptyAfterDelete($companyId: ID!, $contactId: ID!, $locationId: ID!) {
          company(id: $companyId) { id }
          companyContact(id: $contactId) { id }
          companyLocation(id: $locationId) { id }
          companies(first: 5) { nodes { id } }
          companyLocations(first: 5) { nodes { id } }
        }`,
        variables: { companyId, contactId, locationId },
      });

    expect(emptyReads.body.data).toEqual({
      company: null,
      companyContact: null,
      companyLocation: null,
      companies: { nodes: [] },
      companyLocations: { nodes: [] },
    });

    const logResponse = await request(app).get('/__meta/log');
    const logStatuses = logResponse.body.entries.map((entry: { status: string }) => entry.status);
    expect(logStatuses.length).toBeGreaterThanOrEqual(14);
    expect(new Set(logStatuses)).toEqual(new Set(['staged']));
    expect(logResponse.body.entries[0]).toMatchObject({
      operationName: 'companyCreate',
      path: '/admin/api/2026-04/graphql.json',
      status: 'staged',
    });
    expect(logResponse.body.entries[0].requestBody.variables.input.company.name).toBe('Acme B2B');

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.deletedB2BCompanyIds[companyId]).toBe(true);
    expect(stateResponse.body.stagedState.deletedB2BCompanyContactIds[contactId]).toBe(true);
    expect(stateResponse.body.stagedState.deletedB2BCompanyLocationIds[locationId]).toBe(true);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns local validation userErrors for known B2B mutation guardrails without logging commit work', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('B2B validation should not proxy to Shopify');
    });
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation InvalidB2B($companyId: ID!, $contactId: ID!, $locationId: ID!) {
          companyCreate(input: { company: { name: "" } }) {
            company { id }
            userErrors { field message code }
          }
          companyUpdate(companyId: $companyId, input: { name: "Missing" }) {
            company { id }
            userErrors { field message code }
          }
          companyContactUpdate(companyContactId: $contactId, input: { title: "Missing" }) {
            companyContact { id }
            userErrors { field message code }
          }
          companyLocationUpdate(companyLocationId: $locationId, input: { name: "Missing" }) {
            companyLocation { id }
            userErrors { field message code }
          }
        }`,
        variables: {
          companyId: 'gid://shopify/Company/999999999999',
          contactId: 'gid://shopify/CompanyContact/999999999999',
          locationId: 'gid://shopify/CompanyLocation/999999999999',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.companyCreate.userErrors).toEqual([
      { field: ['input', 'company', 'name'], message: "Name can't be blank", code: 'BLANK' },
    ]);
    expect(response.body.data.companyUpdate.userErrors).toEqual([
      { field: ['companyId'], message: 'Resource requested does not exist.', code: 'RESOURCE_NOT_FOUND' },
    ]);
    expect(response.body.data.companyContactUpdate.userErrors).toEqual([
      { field: ['companyContactId'], message: "The company contact doesn't exist.", code: 'RESOURCE_NOT_FOUND' },
    ]);
    expect(response.body.data.companyLocationUpdate.userErrors).toEqual([
      { field: ['input'], message: "The company location doesn't exist", code: 'RESOURCE_NOT_FOUND' },
    ]);
    expect((await request(app).get('/__meta/log')).body.entries).toEqual([]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
