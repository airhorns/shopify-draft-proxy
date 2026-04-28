import { type FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import type { JsonValue } from '../json-schemas.js';
import { store } from '../state/store.js';
import { makeProxySyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import type {
  B2BCompanyContactRecord,
  B2BCompanyContactRoleRecord,
  B2BCompanyLocationRecord,
  B2BCompanyRecord,
} from '../state/types.js';
import {
  getFieldResponseKey,
  isPlainObject,
  paginateConnectionItems,
  projectGraphqlObject,
  projectGraphqlValue,
  readBooleanValue,
  readPlainObjectArray,
  readStringValue,
  serializeConnection,
} from './graphql-helpers.js';

type B2BRecord = B2BCompanyRecord | B2BCompanyContactRecord | B2BCompanyContactRoleRecord | B2BCompanyLocationRecord;
type B2BUserError = { field: string[]; message: string; code: string };
type B2BMutationResult = {
  response: Record<string, unknown>;
  staged: boolean;
  stagedResourceIds: string[];
  notes: string;
};
type B2BMutationPayload = {
  company?: B2BCompanyRecord | null;
  companyContact?: B2BCompanyContactRecord | null;
  companyLocation?: B2BCompanyLocationRecord | null;
  companyContactRoleAssignment?: Record<string, unknown> | null;
  roleAssignments?: Record<string, unknown>[];
  addresses?: Record<string, unknown>[];
  companyLocationStaffMemberAssignments?: Record<string, unknown>[];
  deletedCompanyId?: string | null;
  deletedCompanyIds?: string[];
  deletedCompanyContactId?: string | null;
  deletedCompanyContactIds?: string[];
  deletedCompanyLocationId?: string | null;
  deletedCompanyLocationIds?: string[];
  deletedAddressId?: string | null;
  revokedCompanyContactRoleAssignmentId?: string | null;
  revokedRoleAssignmentIds?: string[];
  deletedCompanyLocationStaffMemberAssignmentIds?: string[];
  removedCompanyContactId?: string | null;
  userErrors: B2BUserError[];
};

function jsonRecord(value: Record<string, unknown>): Record<string, JsonValue> {
  return value as Record<string, JsonValue>;
}

function jsonValue(value: unknown): JsonValue {
  return value as JsonValue;
}

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

function serializePlainConnection(
  field: FieldNode,
  variables: Record<string, unknown>,
  items: Record<string, unknown>[],
): Record<string, unknown> {
  const window = paginateConnectionItems(items, field, variables, (item) => readStringValue(item['id']) ?? '', {
    parseCursor: (raw) => (raw.startsWith('cursor:') ? raw.slice('cursor:'.length) : raw),
  });

  return serializeConnection(field, {
    items: window.items,
    hasNextPage: window.hasNextPage,
    hasPreviousPage: window.hasPreviousPage,
    getCursorValue: (item) => readStringValue(item['id']) ?? '',
    serializeNode: (item, nodeField) => projectGraphqlObject(item, nodeField.selectionSet?.selections ?? [], new Map()),
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
      if (fieldName === 'roleAssignments') {
        return {
          handled: true,
          value: serializePlainConnection(childField, {}, readPlainObjectArray(contact.data['roleAssignments'])),
        };
      }
      if (fieldName === 'orders' || fieldName === 'draftOrders') {
        return { handled: true, value: serializeEmptyConnection(childField) };
      }
      if (fieldName === 'customer') {
        const customer = isPlainObject(contact.data['customer']) ? contact.data['customer'] : null;
        return {
          handled: true,
          value: customer ? projectGraphqlObject(customer, childField.selectionSet?.selections ?? [], new Map()) : null,
        };
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
      if (fieldName === 'roleAssignments' || fieldName === 'staffMemberAssignments') {
        const items =
          fieldName === 'roleAssignments'
            ? readPlainObjectArray(location.data['roleAssignments'])
            : readPlainObjectArray(location.data['staffMemberAssignments']);
        return { handled: true, value: serializePlainConnection(childField, {}, items) };
      }
      if (
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
      if (fieldName === 'billingAddress' || fieldName === 'shippingAddress') {
        const address = isPlainObject(location.data[fieldName]) ? location.data[fieldName] : null;
        return {
          handled: true,
          value: address ? projectGraphqlObject(address, childField.selectionSet?.selections ?? [], new Map()) : null,
        };
      }
      if (fieldName === 'metafield') {
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

function userError(field: string[], message: string, code: string): B2BUserError {
  return { field, message, code };
}

function resourceNotFound(field: string[], message = 'Resource requested does not exist.'): B2BUserError {
  return userError(field, message, 'RESOURCE_NOT_FOUND');
}

function readInputObject(value: unknown): Record<string, unknown> {
  return isPlainObject(value) ? value : {};
}

function readStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

function maybeSetString(target: Record<string, unknown>, source: Record<string, unknown>, key: string): void {
  if (key in source) {
    target[key] = readStringValue(source[key]);
  }
}

function maybeSetBoolean(target: Record<string, unknown>, source: Record<string, unknown>, key: string): void {
  if (key in source) {
    target[key] = readBooleanValue(source[key]);
  }
}

function appendUnique(list: string[], value: string): string[] {
  return list.includes(value) ? list : [...list, value];
}

function removeFromList(list: string[], value: string): string[] {
  return list.filter((item) => item !== value);
}

function addressFromInput(input: Record<string, unknown>, existingId: string | null = null): Record<string, JsonValue> {
  const id = existingId ?? makeProxySyntheticGid('CompanyAddress');
  return {
    __typename: 'CompanyAddress',
    id,
    address1: readStringValue(input['address1']),
    address2: readStringValue(input['address2']),
    city: readStringValue(input['city']),
    zip: readStringValue(input['zip']),
    recipient: readStringValue(input['recipient']),
    firstName: readStringValue(input['firstName']),
    lastName: readStringValue(input['lastName']),
    phone: readStringValue(input['phone']),
    zoneCode: readStringValue(input['zoneCode']),
    countryCode: readStringValue(input['countryCode']),
  };
}

function companyDataFromInput(
  input: Record<string, unknown>,
  now: string,
  existing: Record<string, unknown> = {},
): Record<string, JsonValue> {
  const data = { ...existing };
  maybeSetString(data, input, 'name');
  maybeSetString(data, input, 'note');
  maybeSetString(data, input, 'externalId');
  maybeSetString(data, input, 'customerSince');
  data['updatedAt'] = now;
  return jsonRecord(data);
}

function contactDataFromInput(
  input: Record<string, unknown>,
  now: string,
  existing: Record<string, unknown> = {},
): Record<string, JsonValue> {
  const data = { ...existing };
  for (const key of ['firstName', 'lastName', 'email', 'title', 'locale', 'phone']) {
    maybeSetString(data, input, key);
  }
  data['updatedAt'] = now;
  return jsonRecord(data);
}

function locationDataFromInput(
  input: Record<string, unknown>,
  now: string,
  existing: Record<string, unknown> = {},
): Record<string, JsonValue> {
  const data = { ...existing };
  for (const key of ['name', 'phone', 'locale', 'externalId', 'note', 'taxRegistrationId']) {
    maybeSetString(data, input, key);
  }
  maybeSetBoolean(data, input, 'billingSameAsShipping');
  maybeSetBoolean(data, input, 'taxExempt');
  if (Array.isArray(input['taxExemptions'])) {
    data['taxExemptions'] = readStringArray(input['taxExemptions']);
  }
  if (isPlainObject(input['billingAddress'])) {
    data['billingAddress'] = addressFromInput(input['billingAddress']);
  }
  if (isPlainObject(input['shippingAddress'])) {
    data['shippingAddress'] = addressFromInput(input['shippingAddress']);
  }
  if (data['billingSameAsShipping'] === true && isPlainObject(data['shippingAddress'])) {
    data['billingAddress'] = data['shippingAddress'];
  }
  data['updatedAt'] = now;
  return jsonRecord(data);
}

function refreshCompanyCounts(company: B2BCompanyRecord): B2BCompanyRecord {
  return {
    ...company,
    data: {
      ...company.data,
      contactsCount: { count: company.contactIds.length },
      locationsCount: { count: company.locationIds.length },
    },
  };
}

function serializeMutationRoot(
  field: FieldNode,
  variables: Record<string, unknown>,
  payload: B2BMutationPayload,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== 'Field') {
      continue;
    }

    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'company':
        result[key] = payload.company ? serializeCompany(payload.company, selection, variables) : null;
        break;
      case 'companyContact':
        result[key] = payload.companyContact ? serializeCompanyContact(payload.companyContact, selection) : null;
        break;
      case 'companyLocation':
        result[key] = payload.companyLocation ? serializeCompanyLocation(payload.companyLocation, selection) : null;
        break;
      case 'companyContactRoleAssignment':
        result[key] = payload.companyContactRoleAssignment
          ? projectGraphqlObject(
              payload.companyContactRoleAssignment,
              selection.selectionSet?.selections ?? [],
              new Map(),
            )
          : null;
        break;
      case 'roleAssignments':
        result[key] = projectGraphqlValue(
          payload.roleAssignments ?? [],
          selection.selectionSet?.selections ?? [],
          new Map(),
        );
        break;
      case 'addresses':
        result[key] = projectGraphqlValue(payload.addresses ?? [], selection.selectionSet?.selections ?? [], new Map());
        break;
      case 'companyLocationStaffMemberAssignments':
        result[key] = projectGraphqlValue(
          payload.companyLocationStaffMemberAssignments ?? [],
          selection.selectionSet?.selections ?? [],
          new Map(),
        );
        break;
      case 'userErrors':
        result[key] = projectGraphqlValue(payload.userErrors, selection.selectionSet?.selections ?? [], new Map());
        break;
      default:
        result[key] = (payload as Record<string, unknown>)[selection.name.value] ?? null;
        break;
    }
  }
  return result;
}

function mutationResponse(
  document: string,
  variables: Record<string, unknown>,
  payloadByRoot: Map<string, B2BMutationPayload>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const payload = payloadByRoot.get(field.name.value);
    data[key] = payload ? serializeMutationRoot(field, variables, payload) : null;
  }
  return { data };
}

function createDefaultRoles(companyId: string): B2BCompanyContactRoleRecord[] {
  return [
    {
      id: makeProxySyntheticGid('CompanyContactRole'),
      companyId,
      data: { id: '', name: 'Location admin', note: 'System-defined Location admin role' },
    },
    {
      id: makeProxySyntheticGid('CompanyContactRole'),
      companyId,
      data: { id: '', name: 'Ordering only', note: 'System-defined Ordering only role' },
    },
  ].map((role) => ({ ...role, data: { ...role.data, id: role.id } }));
}

function createContact(
  companyId: string,
  input: Record<string, unknown>,
  isMainContact = false,
): B2BCompanyContactRecord {
  const id = makeProxySyntheticGid('CompanyContact');
  const now = makeSyntheticTimestamp();
  return {
    id,
    companyId,
    data: {
      id,
      createdAt: now,
      isMainContact,
      roleAssignments: [],
      ...contactDataFromInput(input, now),
    },
  };
}

function createLocation(
  companyId: string,
  input: Record<string, unknown>,
  fallbackName: string,
): B2BCompanyLocationRecord {
  const id = makeProxySyntheticGid('CompanyLocation');
  const now = makeSyntheticTimestamp();
  const data = locationDataFromInput({ name: fallbackName, ...input }, now, {
    id,
    createdAt: now,
    roleAssignments: [],
    staffMemberAssignments: [],
  });
  return { id, companyId, data: { ...data, id } };
}

function stageCompany(company: B2BCompanyRecord): B2BCompanyRecord {
  return store.upsertStagedB2BCompany(refreshCompanyCounts(company));
}

function handleCompanyCreate(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const input = readInputObject(args['input']);
  const companyInput = readInputObject(input['company']);
  const name = readStringValue(companyInput['name']);
  if (!name || name.trim().length === 0) {
    return {
      payload: {
        company: null,
        userErrors: [userError(['input', 'company', 'name'], "Name can't be blank", 'BLANK')],
      },
      stagedIds: [],
    };
  }

  const companyId = makeProxySyntheticGid('Company');
  const now = makeSyntheticTimestamp();
  const roles = createDefaultRoles(companyId).map((role) => store.upsertStagedB2BCompanyContactRole(role));
  const location = store.upsertStagedB2BCompanyLocation(
    createLocation(companyId, readInputObject(input['companyLocation']), name),
  );
  const contactInput = isPlainObject(input['companyContact']) ? input['companyContact'] : null;
  const contact = contactInput
    ? store.upsertStagedB2BCompanyContact(createContact(companyId, contactInput, true))
    : null;
  const company = stageCompany({
    id: companyId,
    data: {
      id: companyId,
      createdAt: now,
      ...companyDataFromInput(companyInput, now),
    },
    contactIds: contact ? [contact.id] : [],
    locationIds: [location.id],
    contactRoleIds: roles.map((role) => role.id),
  });

  return {
    payload: { company, userErrors: [] },
    stagedIds: [company.id, location.id, ...roles.map((role) => role.id), ...(contact ? [contact.id] : [])],
  };
}

function handleCompanyUpdate(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const companyId = readStringValue(args['companyId']);
  const company = companyId ? store.getEffectiveB2BCompanyById(companyId) : null;
  if (!company || !companyId) {
    return {
      payload: { company: null, userErrors: [resourceNotFound(['companyId'])] },
      stagedIds: [],
    };
  }

  const input = readInputObject(args['input']);
  const name = 'name' in input ? readStringValue(input['name']) : company.data['name'];
  if (typeof name === 'string' && name.trim().length === 0) {
    return {
      payload: { company: null, userErrors: [userError(['input', 'name'], "Name can't be blank", 'BLANK')] },
      stagedIds: [],
    };
  }

  const updated = stageCompany({
    ...company,
    data: companyDataFromInput(input, makeSyntheticTimestamp(), company.data),
  });
  return { payload: { company: updated, userErrors: [] }, stagedIds: [updated.id] };
}

function deleteCompanyTree(companyId: string): string[] {
  const company = store.getEffectiveB2BCompanyById(companyId);
  if (!company) {
    return [];
  }
  for (const contactId of company.contactIds) {
    store.deleteStagedB2BCompanyContact(contactId);
  }
  for (const locationId of company.locationIds) {
    store.deleteStagedB2BCompanyLocation(locationId);
  }
  for (const roleId of company.contactRoleIds) {
    store.deleteStagedB2BCompanyContactRole(roleId);
  }
  store.deleteStagedB2BCompany(companyId);
  return [companyId, ...company.contactIds, ...company.locationIds, ...company.contactRoleIds];
}

function handleCompanyDelete(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const companyId = readStringValue(args['id']);
  if (!companyId || !store.getEffectiveB2BCompanyById(companyId)) {
    return {
      payload: { deletedCompanyId: null, userErrors: [resourceNotFound(['id'])] },
      stagedIds: [],
    };
  }
  const stagedIds = deleteCompanyTree(companyId);
  return { payload: { deletedCompanyId: companyId, userErrors: [] }, stagedIds };
}

function handleCompaniesDelete(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const companyIds = readStringArray(args['companyIds']);
  const deletedCompanyIds: string[] = [];
  const stagedIds: string[] = [];
  const userErrors: B2BUserError[] = [];
  for (const companyId of companyIds) {
    if (!store.getEffectiveB2BCompanyById(companyId)) {
      userErrors.push(resourceNotFound(['companyIds']));
      continue;
    }
    deletedCompanyIds.push(companyId);
    stagedIds.push(...deleteCompanyTree(companyId));
  }
  return { payload: { deletedCompanyIds, userErrors }, stagedIds };
}

function handleContactCreate(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const companyId = readStringValue(args['companyId']);
  const company = companyId ? store.getEffectiveB2BCompanyById(companyId) : null;
  if (!company || !companyId) {
    return {
      payload: { companyContact: null, userErrors: [resourceNotFound(['companyId'])] },
      stagedIds: [],
    };
  }
  const contact = store.upsertStagedB2BCompanyContact(createContact(companyId, readInputObject(args['input'])));
  stageCompany({ ...company, contactIds: appendUnique(company.contactIds, contact.id) });
  return { payload: { companyContact: contact, userErrors: [] }, stagedIds: [contact.id, companyId] };
}

function handleContactUpdate(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const contactId = readStringValue(args['companyContactId']);
  const contact = contactId ? store.getEffectiveB2BCompanyContactById(contactId) : null;
  if (!contact || !contactId) {
    return {
      payload: {
        companyContact: null,
        userErrors: [resourceNotFound(['companyContactId'], "The company contact doesn't exist.")],
      },
      stagedIds: [],
    };
  }
  const updated = store.upsertStagedB2BCompanyContact({
    ...contact,
    data: contactDataFromInput(readInputObject(args['input']), makeSyntheticTimestamp(), contact.data),
  });
  return { payload: { companyContact: updated, userErrors: [] }, stagedIds: [updated.id] };
}

function deleteContact(contactId: string): string[] {
  const contact = store.getEffectiveB2BCompanyContactById(contactId);
  if (!contact) {
    return [];
  }
  const company = store.getEffectiveB2BCompanyById(contact.companyId);
  if (company) {
    stageCompany({ ...company, contactIds: removeFromList(company.contactIds, contactId) });
  }
  for (const location of store.listEffectiveB2BCompanyLocations()) {
    const roleAssignments = readPlainObjectArray(location.data['roleAssignments']).filter(
      (assignment) => assignment['companyContactId'] !== contactId,
    );
    if (roleAssignments.length !== readPlainObjectArray(location.data['roleAssignments']).length) {
      store.upsertStagedB2BCompanyLocation({
        ...location,
        data: jsonRecord({ ...location.data, roleAssignments: jsonValue(roleAssignments) }),
      });
    }
  }
  store.deleteStagedB2BCompanyContact(contactId);
  return [contactId, contact.companyId];
}

function handleContactDelete(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const contactId = readStringValue(args['companyContactId']);
  if (!contactId || !store.getEffectiveB2BCompanyContactById(contactId)) {
    return {
      payload: {
        deletedCompanyContactId: null,
        userErrors: [resourceNotFound(['companyContactId'], "The company contact doesn't exist.")],
      },
      stagedIds: [],
    };
  }
  return {
    payload: { deletedCompanyContactId: contactId, userErrors: [] },
    stagedIds: deleteContact(contactId),
  };
}

function handleContactsDelete(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const contactIds = readStringArray(args['companyContactIds']);
  const deletedCompanyContactIds: string[] = [];
  const stagedIds: string[] = [];
  const userErrors: B2BUserError[] = [];
  for (const contactId of contactIds) {
    if (!store.getEffectiveB2BCompanyContactById(contactId)) {
      userErrors.push(resourceNotFound(['companyContactIds'], "The company contact doesn't exist."));
      continue;
    }
    deletedCompanyContactIds.push(contactId);
    stagedIds.push(...deleteContact(contactId));
  }
  return { payload: { deletedCompanyContactIds, userErrors }, stagedIds };
}

function handleAssignCustomerAsContact(args: Record<string, unknown>): {
  payload: B2BMutationPayload;
  stagedIds: string[];
} {
  const companyId = readStringValue(args['companyId']);
  const customerId = readStringValue(args['customerId']);
  const company = companyId ? store.getEffectiveB2BCompanyById(companyId) : null;
  if (!company || !companyId) {
    return { payload: { companyContact: null, userErrors: [resourceNotFound(['companyId'])] }, stagedIds: [] };
  }
  if (!customerId) {
    return { payload: { companyContact: null, userErrors: [resourceNotFound(['customerId'])] }, stagedIds: [] };
  }
  const contact = createContact(companyId, {}, false);
  const stagedContact = store.upsertStagedB2BCompanyContact({
    ...contact,
    data: jsonRecord({
      ...contact.data,
      customerId,
      customer: { __typename: 'Customer', id: customerId },
    }),
  });
  stageCompany({ ...company, contactIds: appendUnique(company.contactIds, stagedContact.id) });
  return { payload: { companyContact: stagedContact, userErrors: [] }, stagedIds: [stagedContact.id, companyId] };
}

function handleContactRemoveFromCompany(args: Record<string, unknown>): {
  payload: B2BMutationPayload;
  stagedIds: string[];
} {
  const contactId = readStringValue(args['companyContactId']);
  if (!contactId || !store.getEffectiveB2BCompanyContactById(contactId)) {
    return {
      payload: {
        removedCompanyContactId: null,
        userErrors: [resourceNotFound(['companyContactId'], "The company contact doesn't exist.")],
      },
      stagedIds: [],
    };
  }
  return { payload: { removedCompanyContactId: contactId, userErrors: [] }, stagedIds: deleteContact(contactId) };
}

function handleAssignMainContact(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const companyId = readStringValue(args['companyId']);
  const contactId = readStringValue(args['companyContactId']);
  const company = companyId ? store.getEffectiveB2BCompanyById(companyId) : null;
  const contact = contactId ? store.getEffectiveB2BCompanyContactById(contactId) : null;
  if (!company || !companyId) {
    return { payload: { company: null, userErrors: [resourceNotFound(['companyId'])] }, stagedIds: [] };
  }
  if (!contact || contact.companyId !== companyId) {
    return { payload: { company: null, userErrors: [resourceNotFound(['companyContactId'])] }, stagedIds: [] };
  }
  const stagedIds: string[] = [companyId];
  for (const candidateId of company.contactIds) {
    const candidate = store.getEffectiveB2BCompanyContactById(candidateId);
    if (!candidate) {
      continue;
    }
    store.upsertStagedB2BCompanyContact({
      ...candidate,
      data: jsonRecord({ ...candidate.data, isMainContact: candidate.id === contactId }),
    });
    stagedIds.push(candidate.id);
  }
  return { payload: { company: store.getEffectiveB2BCompanyById(companyId), userErrors: [] }, stagedIds };
}

function handleRevokeMainContact(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const companyId = readStringValue(args['companyId']);
  const company = companyId ? store.getEffectiveB2BCompanyById(companyId) : null;
  if (!company || !companyId) {
    return { payload: { company: null, userErrors: [resourceNotFound(['companyId'])] }, stagedIds: [] };
  }
  const stagedIds: string[] = [companyId];
  for (const contactId of company.contactIds) {
    const contact = store.getEffectiveB2BCompanyContactById(contactId);
    if (contact) {
      store.upsertStagedB2BCompanyContact({
        ...contact,
        data: jsonRecord({ ...contact.data, isMainContact: false }),
      });
      stagedIds.push(contact.id);
    }
  }
  return { payload: { company: store.getEffectiveB2BCompanyById(companyId), userErrors: [] }, stagedIds };
}

function handleLocationCreate(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const companyId = readStringValue(args['companyId']);
  const company = companyId ? store.getEffectiveB2BCompanyById(companyId) : null;
  if (!company || !companyId) {
    return { payload: { companyLocation: null, userErrors: [resourceNotFound(['companyId'])] }, stagedIds: [] };
  }
  const fallbackName = readStringValue(company.data['name']) ?? 'Company location';
  const location = store.upsertStagedB2BCompanyLocation(
    createLocation(companyId, readInputObject(args['input']), fallbackName),
  );
  stageCompany({ ...company, locationIds: appendUnique(company.locationIds, location.id) });
  return { payload: { companyLocation: location, userErrors: [] }, stagedIds: [location.id, companyId] };
}

function handleLocationUpdate(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const locationId = readStringValue(args['companyLocationId']);
  const location = locationId ? store.getEffectiveB2BCompanyLocationById(locationId) : null;
  if (!location || !locationId) {
    return {
      payload: {
        companyLocation: null,
        userErrors: [resourceNotFound(['input'], "The company location doesn't exist")],
      },
      stagedIds: [],
    };
  }
  const updated = store.upsertStagedB2BCompanyLocation({
    ...location,
    data: locationDataFromInput(readInputObject(args['input']), makeSyntheticTimestamp(), location.data),
  });
  return { payload: { companyLocation: updated, userErrors: [] }, stagedIds: [updated.id] };
}

function deleteLocation(locationId: string): string[] {
  const location = store.getEffectiveB2BCompanyLocationById(locationId);
  if (!location) {
    return [];
  }
  const company = store.getEffectiveB2BCompanyById(location.companyId);
  if (company) {
    stageCompany({ ...company, locationIds: removeFromList(company.locationIds, locationId) });
  }
  for (const contact of store.listEffectiveB2BCompanyContacts()) {
    const roleAssignments = readPlainObjectArray(contact.data['roleAssignments']).filter(
      (assignment) => assignment['companyLocationId'] !== locationId,
    );
    if (roleAssignments.length !== readPlainObjectArray(contact.data['roleAssignments']).length) {
      store.upsertStagedB2BCompanyContact({
        ...contact,
        data: jsonRecord({ ...contact.data, roleAssignments: jsonValue(roleAssignments) }),
      });
    }
  }
  store.deleteStagedB2BCompanyLocation(locationId);
  return [locationId, location.companyId];
}

function handleLocationDelete(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const locationId = readStringValue(args['companyLocationId']);
  if (!locationId || !store.getEffectiveB2BCompanyLocationById(locationId)) {
    return {
      payload: {
        deletedCompanyLocationId: null,
        userErrors: [resourceNotFound(['companyLocationId'], "The company location doesn't exist")],
      },
      stagedIds: [],
    };
  }
  return {
    payload: { deletedCompanyLocationId: locationId, userErrors: [] },
    stagedIds: deleteLocation(locationId),
  };
}

function handleLocationsDelete(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const locationIds = readStringArray(args['companyLocationIds']);
  const deletedCompanyLocationIds: string[] = [];
  const stagedIds: string[] = [];
  const userErrors: B2BUserError[] = [];
  for (const locationId of locationIds) {
    if (!store.getEffectiveB2BCompanyLocationById(locationId)) {
      userErrors.push(resourceNotFound(['companyLocationIds'], "The company location doesn't exist"));
      continue;
    }
    deletedCompanyLocationIds.push(locationId);
    stagedIds.push(...deleteLocation(locationId));
  }
  return { payload: { deletedCompanyLocationIds, userErrors }, stagedIds };
}

function buildRoleAssignment(
  contact: B2BCompanyContactRecord,
  role: B2BCompanyContactRoleRecord,
  location: B2BCompanyLocationRecord,
): Record<string, JsonValue> {
  const id = makeProxySyntheticGid('CompanyContactRoleAssignment');
  return jsonRecord({
    __typename: 'CompanyContactRoleAssignment',
    id,
    companyContactId: contact.id,
    companyContactRoleId: role.id,
    companyLocationId: location.id,
    companyContact: recordSource(contact, 'CompanyContact'),
    role: recordSource(role, 'CompanyContactRole'),
    companyLocation: recordSource(location, 'CompanyLocation'),
  });
}

function stageRoleAssignments(assignments: Record<string, unknown>[]): string[] {
  const stagedIds: string[] = [];
  for (const assignment of assignments) {
    const contactId = readStringValue(assignment['companyContactId']);
    const locationId = readStringValue(assignment['companyLocationId']);
    if (contactId) {
      const contact = store.getEffectiveB2BCompanyContactById(contactId);
      if (contact) {
        store.upsertStagedB2BCompanyContact({
          ...contact,
          data: jsonRecord({
            ...contact.data,
            roleAssignments: jsonValue([...readPlainObjectArray(contact.data['roleAssignments']), assignment]),
          }),
        });
        stagedIds.push(contact.id);
      }
    }
    if (locationId) {
      const location = store.getEffectiveB2BCompanyLocationById(locationId);
      if (location) {
        store.upsertStagedB2BCompanyLocation({
          ...location,
          data: jsonRecord({
            ...location.data,
            roleAssignments: jsonValue([...readPlainObjectArray(location.data['roleAssignments']), assignment]),
          }),
        });
        stagedIds.push(location.id);
      }
    }
  }
  return stagedIds;
}

function resolveRoleAssignmentInputs(
  rawInputs: Record<string, unknown>[],
  contactIdFallback?: string,
  locationIdFallback?: string,
): { assignments: Record<string, unknown>[]; userErrors: B2BUserError[] } {
  const assignments: Record<string, unknown>[] = [];
  const userErrors: B2BUserError[] = [];
  for (const input of rawInputs) {
    const contactId = readStringValue(input['companyContactId']) ?? contactIdFallback ?? null;
    const roleId = readStringValue(input['companyContactRoleId']);
    const locationId = readStringValue(input['companyLocationId']) ?? locationIdFallback ?? null;
    const contact = contactId ? store.getEffectiveB2BCompanyContactById(contactId) : null;
    const role = roleId ? store.getEffectiveB2BCompanyContactRoleById(roleId) : null;
    const location = locationId ? store.getEffectiveB2BCompanyLocationById(locationId) : null;
    if (
      !contact ||
      !role ||
      !location ||
      contact.companyId !== role.companyId ||
      contact.companyId !== location.companyId
    ) {
      userErrors.push(resourceNotFound(['rolesToAssign']));
      continue;
    }
    assignments.push(buildRoleAssignment(contact, role, location));
  }
  return { assignments, userErrors };
}

function revokeRoleAssignments(
  assignmentIds: string[],
  options: { contactId?: string | null; locationId?: string | null; revokeAll?: boolean } = {},
): string[] {
  const removedIds = new Set<string>();
  for (const contact of store.listEffectiveB2BCompanyContacts()) {
    if (options.contactId && contact.id !== options.contactId) {
      continue;
    }
    const current = readPlainObjectArray(contact.data['roleAssignments']);
    const next = current.filter((assignment) => {
      const id = readStringValue(assignment['id']);
      const shouldRemove = options.revokeAll === true || (id !== null && assignmentIds.includes(id));
      if (shouldRemove && id) {
        removedIds.add(id);
      }
      return !shouldRemove;
    });
    if (next.length !== current.length) {
      store.upsertStagedB2BCompanyContact({
        ...contact,
        data: jsonRecord({ ...contact.data, roleAssignments: jsonValue(next) }),
      });
    }
  }
  for (const location of store.listEffectiveB2BCompanyLocations()) {
    if (options.locationId && location.id !== options.locationId) {
      continue;
    }
    const current = readPlainObjectArray(location.data['roleAssignments']);
    const next = current.filter((assignment) => {
      const id = readStringValue(assignment['id']);
      const shouldRemove = options.revokeAll === true || (id !== null && assignmentIds.includes(id));
      if (shouldRemove && id) {
        removedIds.add(id);
      }
      return !shouldRemove;
    });
    if (next.length !== current.length) {
      store.upsertStagedB2BCompanyLocation({
        ...location,
        data: jsonRecord({ ...location.data, roleAssignments: jsonValue(next) }),
      });
    }
  }
  return [...removedIds];
}

function handleAssignAddress(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const locationId = readStringValue(args['locationId']);
  const location = locationId ? store.getEffectiveB2BCompanyLocationById(locationId) : null;
  if (!location || !locationId) {
    return { payload: { addresses: [], userErrors: [resourceNotFound(['locationId'])] }, stagedIds: [] };
  }
  const address = addressFromInput(readInputObject(args['address']));
  const addressTypes = readStringArray(args['addressTypes']);
  const nextData: Record<string, unknown> = { ...location.data };
  const addresses: Record<string, unknown>[] = [];
  if (addressTypes.includes('BILLING')) {
    nextData['billingAddress'] = address;
    addresses.push(address);
  }
  if (addressTypes.includes('SHIPPING')) {
    nextData['shippingAddress'] = address;
    addresses.push(address);
  }
  if (addresses.length === 0) {
    return {
      payload: { addresses: [], userErrors: [userError(['addressTypes'], 'Address type is invalid', 'INVALID')] },
      stagedIds: [],
    };
  }
  const updated = store.upsertStagedB2BCompanyLocation({ ...location, data: jsonRecord(nextData) });
  return {
    payload: { addresses, userErrors: [] },
    stagedIds: [updated.id, ...addresses.map((item) => String(item['id']))],
  };
}

function handleAddressDelete(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const addressId = readStringValue(args['addressId']);
  if (!addressId) {
    return { payload: { deletedAddressId: null, userErrors: [resourceNotFound(['addressId'])] }, stagedIds: [] };
  }
  for (const location of store.listEffectiveB2BCompanyLocations()) {
    const nextData: Record<string, unknown> = { ...location.data };
    let found = false;
    for (const key of ['billingAddress', 'shippingAddress']) {
      const address = isPlainObject(nextData[key]) ? nextData[key] : null;
      if (address?.['id'] === addressId) {
        nextData[key] = null;
        found = true;
      }
    }
    if (found) {
      store.upsertStagedB2BCompanyLocation({ ...location, data: jsonRecord(nextData) });
      return { payload: { deletedAddressId: addressId, userErrors: [] }, stagedIds: [location.id, addressId] };
    }
  }
  return { payload: { deletedAddressId: null, userErrors: [resourceNotFound(['addressId'])] }, stagedIds: [] };
}

function handleAssignStaff(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const locationId = readStringValue(args['companyLocationId']);
  const location = locationId ? store.getEffectiveB2BCompanyLocationById(locationId) : null;
  if (!location || !locationId) {
    return {
      payload: { companyLocationStaffMemberAssignments: [], userErrors: [resourceNotFound(['companyLocationId'])] },
      stagedIds: [],
    };
  }
  const assignments = readStringArray(args['staffMemberIds']).map((staffMemberId) => {
    const id = makeProxySyntheticGid('CompanyLocationStaffMemberAssignment');
    return {
      __typename: 'CompanyLocationStaffMemberAssignment',
      id,
      staffMemberId,
      staffMember: { __typename: 'StaffMember', id: staffMemberId },
      companyLocation: recordSource(location, 'CompanyLocation'),
    };
  });
  const updated = store.upsertStagedB2BCompanyLocation({
    ...location,
    data: {
      ...location.data,
      staffMemberAssignments: jsonValue([
        ...readPlainObjectArray(location.data['staffMemberAssignments']),
        ...assignments,
      ]),
    },
  });
  return {
    payload: { companyLocationStaffMemberAssignments: assignments, userErrors: [] },
    stagedIds: [updated.id, ...assignments.map((item) => String(item['id']))],
  };
}

function handleRemoveStaff(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const assignmentIds = readStringArray(args['companyLocationStaffMemberAssignmentIds']);
  const removed = new Set<string>();
  const stagedIds: string[] = [];
  for (const location of store.listEffectiveB2BCompanyLocations()) {
    const current = readPlainObjectArray(location.data['staffMemberAssignments']);
    const next = current.filter((assignment) => {
      const id = readStringValue(assignment['id']);
      const shouldRemove = id !== null && assignmentIds.includes(id);
      if (shouldRemove && id) {
        removed.add(id);
      }
      return !shouldRemove;
    });
    if (next.length !== current.length) {
      store.upsertStagedB2BCompanyLocation({
        ...location,
        data: jsonRecord({ ...location.data, staffMemberAssignments: jsonValue(next) }),
      });
      stagedIds.push(location.id);
    }
  }
  return {
    payload: { deletedCompanyLocationStaffMemberAssignmentIds: [...removed], userErrors: [] },
    stagedIds: [...stagedIds, ...removed],
  };
}

function handleTaxSettingsUpdate(args: Record<string, unknown>): { payload: B2BMutationPayload; stagedIds: string[] } {
  const locationId = readStringValue(args['companyLocationId']);
  const location = locationId ? store.getEffectiveB2BCompanyLocationById(locationId) : null;
  if (!location || !locationId) {
    return {
      payload: {
        companyLocation: null,
        userErrors: [resourceNotFound(['companyLocationId'], "The company location doesn't exist")],
      },
      stagedIds: [],
    };
  }
  const exemptions = new Set(readStringArray(location.data['taxExemptions']));
  for (const exemption of readStringArray(args['exemptionsToAssign'])) {
    exemptions.add(exemption);
  }
  for (const exemption of readStringArray(args['exemptionsToRemove'])) {
    exemptions.delete(exemption);
  }
  const data: Record<string, unknown> = {
    ...location.data,
    taxExemptions: [...exemptions],
    updatedAt: makeSyntheticTimestamp(),
  };
  maybeSetString(data, args, 'taxRegistrationId');
  maybeSetBoolean(data, args, 'taxExempt');
  const updated = store.upsertStagedB2BCompanyLocation({ ...location, data: jsonRecord(data) });
  return { payload: { companyLocation: updated, userErrors: [] }, stagedIds: [updated.id] };
}

function dispatchB2BMutationRoot(
  rootField: string,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } | null {
  switch (rootField) {
    case 'companyCreate':
      return handleCompanyCreate(args);
    case 'companyUpdate':
      return handleCompanyUpdate(args);
    case 'companyDelete':
      return handleCompanyDelete(args);
    case 'companiesDelete':
      return handleCompaniesDelete(args);
    case 'companyContactCreate':
      return handleContactCreate(args);
    case 'companyContactUpdate':
      return handleContactUpdate(args);
    case 'companyContactDelete':
      return handleContactDelete(args);
    case 'companyContactsDelete':
      return handleContactsDelete(args);
    case 'companyAssignCustomerAsContact':
      return handleAssignCustomerAsContact(args);
    case 'companyContactRemoveFromCompany':
      return handleContactRemoveFromCompany(args);
    case 'companyAssignMainContact':
      return handleAssignMainContact(args);
    case 'companyRevokeMainContact':
      return handleRevokeMainContact(args);
    case 'companyLocationCreate':
      return handleLocationCreate(args);
    case 'companyLocationUpdate':
      return handleLocationUpdate(args);
    case 'companyLocationDelete':
      return handleLocationDelete(args);
    case 'companyLocationsDelete':
      return handleLocationsDelete(args);
    case 'companyLocationAssignAddress':
      return handleAssignAddress(args);
    case 'companyAddressDelete':
      return handleAddressDelete(args);
    case 'companyLocationAssignStaffMembers':
      return handleAssignStaff(args);
    case 'companyLocationRemoveStaffMembers':
      return handleRemoveStaff(args);
    case 'companyLocationTaxSettingsUpdate':
      return handleTaxSettingsUpdate(args);
    case 'companyContactAssignRole': {
      const { assignments, userErrors } = resolveRoleAssignmentInputs([
        {
          companyContactId: args['companyContactId'],
          companyContactRoleId: args['companyContactRoleId'],
          companyLocationId: args['companyLocationId'],
        },
      ]);
      const stagedIds = userErrors.length > 0 ? [] : stageRoleAssignments(assignments);
      return { payload: { companyContactRoleAssignment: assignments[0] ?? null, userErrors }, stagedIds };
    }
    case 'companyContactAssignRoles': {
      const contactId = readStringValue(args['companyContactId']) ?? undefined;
      const { assignments, userErrors } = resolveRoleAssignmentInputs(
        readPlainObjectArray(args['rolesToAssign']),
        contactId,
      );
      const stagedIds = userErrors.length > 0 ? [] : stageRoleAssignments(assignments);
      return { payload: { roleAssignments: assignments, userErrors }, stagedIds };
    }
    case 'companyLocationAssignRoles': {
      const locationId = readStringValue(args['companyLocationId']);
      const { assignments, userErrors } = resolveRoleAssignmentInputs(
        readPlainObjectArray(args['rolesToAssign']),
        undefined,
        locationId ?? undefined,
      );
      const stagedIds = userErrors.length > 0 ? [] : stageRoleAssignments(assignments);
      return { payload: { roleAssignments: assignments, userErrors }, stagedIds };
    }
    case 'companyContactRevokeRole': {
      const assignmentId = readStringValue(args['companyContactRoleAssignmentId']);
      const revoked = assignmentId
        ? revokeRoleAssignments([assignmentId], { contactId: readStringValue(args['companyContactId']) })
        : [];
      return {
        payload: {
          revokedCompanyContactRoleAssignmentId: revoked[0] ?? null,
          userErrors: revoked.length > 0 ? [] : [resourceNotFound(['companyContactRoleAssignmentId'])],
        },
        stagedIds: revoked,
      };
    }
    case 'companyContactRevokeRoles': {
      const revoked = revokeRoleAssignments(readStringArray(args['roleAssignmentIds']), {
        contactId: readStringValue(args['companyContactId']),
        revokeAll: readBooleanValue(args['revokeAll']) === true,
      });
      return { payload: { revokedRoleAssignmentIds: revoked, userErrors: [] }, stagedIds: revoked };
    }
    case 'companyLocationRevokeRoles': {
      const revoked = revokeRoleAssignments(readStringArray(args['rolesToRevoke']), {
        locationId: readStringValue(args['companyLocationId']),
      });
      return { payload: { revokedRoleAssignmentIds: revoked, userErrors: [] }, stagedIds: revoked };
    }
    default:
      return null;
  }
}

export function handleB2BMutation(document: string, variables: Record<string, unknown>): B2BMutationResult | null {
  const payloads = new Map<string, B2BMutationPayload>();
  const stagedResourceIds = new Set<string>();
  let handled = false;
  let staged = false;

  for (const field of getRootFields(document)) {
    const result = dispatchB2BMutationRoot(field.name.value, getFieldArguments(field, variables));
    if (!result) {
      continue;
    }
    handled = true;
    payloads.set(field.name.value, result.payload);
    for (const id of result.stagedIds) {
      stagedResourceIds.add(id);
    }
    if (result.payload.userErrors.length === 0 && result.stagedIds.length > 0) {
      staged = true;
    }
  }

  if (!handled) {
    return null;
  }

  return {
    response: mutationResponse(document, variables, payloads),
    staged,
    stagedResourceIds: [...stagedResourceIds],
    notes: 'Staged locally in the in-memory B2B company draft store.',
  };
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
