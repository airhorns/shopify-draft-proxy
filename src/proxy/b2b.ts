import { type FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { store } from '../state/store.js';
import type {
  B2BCompanyContactRecord,
  B2BCompanyContactRoleRecord,
  B2BCompanyLocationRecord,
  B2BCompanyRecord,
} from '../state/types.js';
import {
  getFieldResponseKey,
  paginateConnectionItems,
  projectGraphqlObject,
  serializeConnection,
} from './graphql-helpers.js';

type B2BRecord = B2BCompanyRecord | B2BCompanyContactRecord | B2BCompanyContactRoleRecord | B2BCompanyLocationRecord;

function serializeCount(field: FieldNode, count: number): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== 'Field') {
      continue;
    }

    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'count':
        result[key] = count;
        break;
      case 'precision':
        result[key] = 'EXACT';
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function recordSource(record: B2BRecord, typename: string): Record<string, unknown> {
  return {
    __typename: typename,
    ...record.data,
    id: record.id,
  };
}

function readCount(source: Record<string, unknown>, fallback: number): number {
  const count = source['count'];
  if (typeof count === 'number' && Number.isFinite(count) && count >= 0) {
    return Math.floor(count);
  }

  return fallback;
}

function readOptionalCount(source: unknown, fallback: number): number {
  return source && typeof source === 'object' && !Array.isArray(source)
    ? readCount(source as Record<string, unknown>, fallback)
    : fallback;
}

function serializeRecordConnection<T extends B2BRecord>(
  field: FieldNode,
  variables: Record<string, unknown>,
  records: T[],
  serializeNode: (record: T, nodeField: FieldNode, index: number) => unknown,
): Record<string, unknown> {
  const window = paginateConnectionItems(records, field, variables, (record) => record.cursor ?? record.id, {
    parseCursor: (raw) => (raw.startsWith('cursor:') ? raw.slice('cursor:'.length) : raw),
  });

  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (record) => record.cursor ?? record.id,
    serializeNode: (record, nodeField, index) => serializeNode(record, nodeField, index),
  });
}

function serializeEmptyConnection(field: FieldNode): Record<string, unknown> {
  return serializeConnection(field, {
    items: [],
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: () => '',
    serializeNode: () => null,
  });
}

function serializeCompanyRole(role: B2BCompanyContactRoleRecord, field: FieldNode): unknown {
  return projectGraphqlObject(
    recordSource(role, 'CompanyContactRole'),
    field.selectionSet?.selections ?? [],
    new Map(),
  );
}

function serializeCompanyContact(contact: B2BCompanyContactRecord, field: FieldNode): unknown {
  const source = recordSource(contact, 'CompanyContact');
  return projectGraphqlObject(source, field.selectionSet?.selections ?? [], new Map(), {
    projectFieldValue: ({ field: childField, fieldName }) => {
      if (fieldName === 'company') {
        const company = store.getEffectiveB2BCompanyById(contact.companyId);
        return { handled: true, value: company ? serializeCompany(company, childField, {}) : null };
      }
      if (fieldName === 'roleAssignments' || fieldName === 'orders' || fieldName === 'draftOrders') {
        return { handled: true, value: serializeEmptyConnection(childField) };
      }
      if (fieldName === 'customer') {
        return { handled: true, value: null };
      }
      return { handled: false };
    },
  });
}

function serializeCompanyLocation(location: B2BCompanyLocationRecord, field: FieldNode): unknown {
  const source = recordSource(location, 'CompanyLocation');
  return projectGraphqlObject(source, field.selectionSet?.selections ?? [], new Map(), {
    projectFieldValue: ({ field: childField, fieldName }) => {
      if (fieldName === 'company') {
        const company = store.getEffectiveB2BCompanyById(location.companyId);
        return { handled: true, value: company ? serializeCompany(company, childField, {}) : null };
      }
      if (
        fieldName === 'roleAssignments' ||
        fieldName === 'staffMemberAssignments' ||
        fieldName === 'orders' ||
        fieldName === 'draftOrders' ||
        fieldName === 'events' ||
        fieldName === 'catalogs' ||
        fieldName === 'metafields'
      ) {
        return { handled: true, value: serializeEmptyConnection(childField) };
      }
      if (fieldName === 'catalogsCount' || fieldName === 'ordersCount') {
        return { handled: true, value: serializeCount(childField, 0) };
      }
      if (fieldName === 'metafield' || fieldName === 'billingAddress' || fieldName === 'shippingAddress') {
        return { handled: true, value: null };
      }
      return { handled: false };
    },
  });
}

function companyContacts(company: B2BCompanyRecord): B2BCompanyContactRecord[] {
  return company.contactIds
    .map((id) => store.getEffectiveB2BCompanyContactById(id))
    .filter((contact): contact is B2BCompanyContactRecord => contact !== null);
}

function companyLocations(company: B2BCompanyRecord): B2BCompanyLocationRecord[] {
  return company.locationIds
    .map((id) => store.getEffectiveB2BCompanyLocationById(id))
    .filter((location): location is B2BCompanyLocationRecord => location !== null);
}

function companyRoles(company: B2BCompanyRecord): B2BCompanyContactRoleRecord[] {
  return company.contactRoleIds
    .map((id) => store.getEffectiveB2BCompanyContactRoleById(id))
    .filter((role): role is B2BCompanyContactRoleRecord => role !== null);
}

function serializeCompany(company: B2BCompanyRecord, field: FieldNode, variables: Record<string, unknown>): unknown {
  const contacts = companyContacts(company);
  const locations = companyLocations(company);
  const roles = companyRoles(company);
  const source = recordSource(company, 'Company');

  return projectGraphqlObject(source, field.selectionSet?.selections ?? [], new Map(), {
    projectFieldValue: ({ field: childField, fieldName }) => {
      switch (fieldName) {
        case 'contacts':
          return {
            handled: true,
            value: serializeRecordConnection(childField, variables, contacts, (contact, nodeField) =>
              serializeCompanyContact(contact, nodeField),
            ),
          };
        case 'locations':
          return {
            handled: true,
            value: serializeRecordConnection(childField, variables, locations, (location, nodeField) =>
              serializeCompanyLocation(location, nodeField),
            ),
          };
        case 'contactRoles':
          return {
            handled: true,
            value: serializeRecordConnection(childField, variables, roles, (role, nodeField) =>
              serializeCompanyRole(role, nodeField),
            ),
          };
        case 'contactsCount':
          return {
            handled: true,
            value: serializeCount(childField, readOptionalCount(source['contactsCount'], contacts.length)),
          };
        case 'locationsCount':
          return {
            handled: true,
            value: serializeCount(childField, readOptionalCount(source['locationsCount'], locations.length)),
          };
        case 'mainContact': {
          const mainContact = contacts.find((contact) => contact.data['isMainContact'] === true) ?? contacts[0] ?? null;
          return {
            handled: true,
            value: mainContact ? serializeCompanyContact(mainContact, childField) : null,
          };
        }
        case 'defaultRole':
          return { handled: true, value: roles[0] ? serializeCompanyRole(roles[0], childField) : null };
        case 'orders':
        case 'draftOrders':
        case 'events':
        case 'metafields':
          return { handled: true, value: serializeEmptyConnection(childField) };
        case 'ordersCount':
          return { handled: true, value: serializeCount(childField, 0) };
        case 'metafield':
          return { handled: true, value: null };
        default:
          return { handled: false };
      }
    },
  });
}

function serializeCompanies(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  return serializeRecordConnection(field, variables, store.listEffectiveB2BCompanies(), (company, nodeField) =>
    serializeCompany(company, nodeField, variables),
  );
}

function serializeCompanyLocations(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  return serializeRecordConnection(field, variables, store.listEffectiveB2BCompanyLocations(), (location, nodeField) =>
    serializeCompanyLocation(location, nodeField),
  );
}

export function handleB2BQuery(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);
    const id = typeof args['id'] === 'string' && args['id'].length > 0 ? args['id'] : null;

    switch (field.name.value) {
      case 'companies':
        data[key] = serializeCompanies(field, variables);
        break;
      case 'companiesCount':
        data[key] = serializeCount(field, store.listEffectiveB2BCompanies().length);
        break;
      case 'company': {
        const company = id ? store.getEffectiveB2BCompanyById(id) : null;
        data[key] = company ? serializeCompany(company, field, variables) : null;
        break;
      }
      case 'companyContact': {
        const contact = id ? store.getEffectiveB2BCompanyContactById(id) : null;
        data[key] = contact ? serializeCompanyContact(contact, field) : null;
        break;
      }
      case 'companyContactRole': {
        const role = id ? store.getEffectiveB2BCompanyContactRoleById(id) : null;
        data[key] = role ? serializeCompanyRole(role, field) : null;
        break;
      }
      case 'companyLocation': {
        const location = id ? store.getEffectiveB2BCompanyLocationById(id) : null;
        data[key] = location ? serializeCompanyLocation(location, field) : null;
        break;
      }
      case 'companyLocations':
        data[key] = serializeCompanyLocations(field, variables);
        break;
      default:
        data[key] = null;
        break;
    }
  }

  return { data };
}
