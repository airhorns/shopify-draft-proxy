import type { ProxyRuntimeContext } from './runtime-context.js';
import { type FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import type { JsonValue } from '../json-schemas.js';
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

function serializeCompanyContact(
  runtime: ProxyRuntimeContext,
  contact: B2BCompanyContactRecord,
  field: FieldNode,
): unknown {
  const source = recordSource(contact, 'CompanyContact');
  return projectGraphqlObject(source, field.selectionSet?.selections ?? [], new Map(), {
    projectFieldValue: ({ field: childField, fieldName }) => {
      if (fieldName === 'company') {
        const company = runtime.store.getEffectiveB2BCompanyById(contact.companyId);
        return { handled: true, value: company ? serializeCompany(runtime, company, childField, {}) : null };
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

function serializeCompanyLocation(
  runtime: ProxyRuntimeContext,
  location: B2BCompanyLocationRecord,
  field: FieldNode,
): unknown {
  const source = recordSource(location, 'CompanyLocation');
  return projectGraphqlObject(source, field.selectionSet?.selections ?? [], new Map(), {
    projectFieldValue: ({ field: childField, fieldName }) => {
      if (fieldName === 'company') {
        const company = runtime.store.getEffectiveB2BCompanyById(location.companyId);
        return { handled: true, value: company ? serializeCompany(runtime, company, childField, {}) : null };
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

function companyContacts(runtime: ProxyRuntimeContext, company: B2BCompanyRecord): B2BCompanyContactRecord[] {
  return company.contactIds
    .map((id) => runtime.store.getEffectiveB2BCompanyContactById(id))
    .filter((contact): contact is B2BCompanyContactRecord => contact !== null);
}

function companyLocations(runtime: ProxyRuntimeContext, company: B2BCompanyRecord): B2BCompanyLocationRecord[] {
  return company.locationIds
    .map((id) => runtime.store.getEffectiveB2BCompanyLocationById(id))
    .filter((location): location is B2BCompanyLocationRecord => location !== null);
}

function companyRoles(runtime: ProxyRuntimeContext, company: B2BCompanyRecord): B2BCompanyContactRoleRecord[] {
  return company.contactRoleIds
    .map((id) => runtime.store.getEffectiveB2BCompanyContactRoleById(id))
    .filter((role): role is B2BCompanyContactRoleRecord => role !== null);
}

function serializeCompany(
  runtime: ProxyRuntimeContext,
  company: B2BCompanyRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): unknown {
  const contacts = companyContacts(runtime, company);
  const locations = companyLocations(runtime, company);
  const roles = companyRoles(runtime, company);
  const source = recordSource(company, 'Company');

  return projectGraphqlObject(source, field.selectionSet?.selections ?? [], new Map(), {
    projectFieldValue: ({ field: childField, fieldName }) => {
      switch (fieldName) {
        case 'contacts':
          return {
            handled: true,
            value: serializeRecordConnection(childField, variables, contacts, (contact, nodeField) =>
              serializeCompanyContact(runtime, contact, nodeField),
            ),
          };
        case 'locations':
          return {
            handled: true,
            value: serializeRecordConnection(childField, variables, locations, (location, nodeField) =>
              serializeCompanyLocation(runtime, location, nodeField),
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
            value: mainContact ? serializeCompanyContact(runtime, mainContact, childField) : null,
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

function serializeCompanies(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  return serializeRecordConnection(field, variables, runtime.store.listEffectiveB2BCompanies(), (company, nodeField) =>
    serializeCompany(runtime, company, nodeField, variables),
  );
}

function serializeCompanyLocations(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  return serializeRecordConnection(
    field,
    variables,
    runtime.store.listEffectiveB2BCompanyLocations(),
    (location, nodeField) => serializeCompanyLocation(runtime, location, nodeField),
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

function addressFromInput(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  existingId: string | null = null,
): Record<string, JsonValue> {
  const id = existingId ?? runtime.syntheticIdentity.makeProxySyntheticGid('CompanyAddress');
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
  runtime: ProxyRuntimeContext,
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
    data['billingAddress'] = addressFromInput(runtime, input['billingAddress']);
  }
  if (isPlainObject(input['shippingAddress'])) {
    data['shippingAddress'] = addressFromInput(runtime, input['shippingAddress']);
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
  runtime: ProxyRuntimeContext,
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
        result[key] = payload.company ? serializeCompany(runtime, payload.company, selection, variables) : null;
        break;
      case 'companyContact':
        result[key] = payload.companyContact
          ? serializeCompanyContact(runtime, payload.companyContact, selection)
          : null;
        break;
      case 'companyLocation':
        result[key] = payload.companyLocation
          ? serializeCompanyLocation(runtime, payload.companyLocation, selection)
          : null;
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
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
  payloadByRoot: Map<string, B2BMutationPayload>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const payload = payloadByRoot.get(field.name.value);
    data[key] = payload ? serializeMutationRoot(runtime, field, variables, payload) : null;
  }
  return { data };
}

function createDefaultRoles(runtime: ProxyRuntimeContext, companyId: string): B2BCompanyContactRoleRecord[] {
  return [
    {
      id: runtime.syntheticIdentity.makeProxySyntheticGid('CompanyContactRole'),
      companyId,
      data: { id: '', name: 'Location admin', note: 'System-defined Location admin role' },
    },
    {
      id: runtime.syntheticIdentity.makeProxySyntheticGid('CompanyContactRole'),
      companyId,
      data: { id: '', name: 'Ordering only', note: 'System-defined Ordering only role' },
    },
  ].map((role) => ({ ...role, data: { ...role.data, id: role.id } }));
}

function createContact(
  runtime: ProxyRuntimeContext,
  companyId: string,
  input: Record<string, unknown>,
  isMainContact = false,
): B2BCompanyContactRecord {
  const id = runtime.syntheticIdentity.makeProxySyntheticGid('CompanyContact');
  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
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
  runtime: ProxyRuntimeContext,
  companyId: string,
  input: Record<string, unknown>,
  fallbackName: string,
): B2BCompanyLocationRecord {
  const id = runtime.syntheticIdentity.makeProxySyntheticGid('CompanyLocation');
  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const data = locationDataFromInput(runtime, { name: fallbackName, ...input }, now, {
    id,
    createdAt: now,
    roleAssignments: [],
    staffMemberAssignments: [],
  });
  return { id, companyId, data: { ...data, id } };
}

function stageCompany(runtime: ProxyRuntimeContext, company: B2BCompanyRecord): B2BCompanyRecord {
  return runtime.store.upsertStagedB2BCompany(refreshCompanyCounts(company));
}

function handleCompanyCreate(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
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

  const companyId = runtime.syntheticIdentity.makeProxySyntheticGid('Company');
  const now = runtime.syntheticIdentity.makeSyntheticTimestamp();
  const roles = createDefaultRoles(runtime, companyId).map((role) =>
    runtime.store.upsertStagedB2BCompanyContactRole(role),
  );
  const location = runtime.store.upsertStagedB2BCompanyLocation(
    createLocation(runtime, companyId, readInputObject(input['companyLocation']), name),
  );
  const contactInput = isPlainObject(input['companyContact']) ? input['companyContact'] : null;
  const contact = contactInput
    ? runtime.store.upsertStagedB2BCompanyContact(createContact(runtime, companyId, contactInput, true))
    : null;
  const company = stageCompany(runtime, {
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

function handleCompanyUpdate(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const companyId = readStringValue(args['companyId']);
  const company = companyId ? runtime.store.getEffectiveB2BCompanyById(companyId) : null;
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

  const updated = stageCompany(runtime, {
    ...company,
    data: companyDataFromInput(input, runtime.syntheticIdentity.makeSyntheticTimestamp(), company.data),
  });
  return { payload: { company: updated, userErrors: [] }, stagedIds: [updated.id] };
}

function deleteCompanyTree(runtime: ProxyRuntimeContext, companyId: string): string[] {
  const company = runtime.store.getEffectiveB2BCompanyById(companyId);
  if (!company) {
    return [];
  }
  for (const contactId of company.contactIds) {
    runtime.store.deleteStagedB2BCompanyContact(contactId);
  }
  for (const locationId of company.locationIds) {
    runtime.store.deleteStagedB2BCompanyLocation(locationId);
  }
  for (const roleId of company.contactRoleIds) {
    runtime.store.deleteStagedB2BCompanyContactRole(roleId);
  }
  runtime.store.deleteStagedB2BCompany(companyId);
  return [companyId, ...company.contactIds, ...company.locationIds, ...company.contactRoleIds];
}

function handleCompanyDelete(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const companyId = readStringValue(args['id']);
  if (!companyId || !runtime.store.getEffectiveB2BCompanyById(companyId)) {
    return {
      payload: { deletedCompanyId: null, userErrors: [resourceNotFound(['id'])] },
      stagedIds: [],
    };
  }
  const stagedIds = deleteCompanyTree(runtime, companyId);
  return { payload: { deletedCompanyId: companyId, userErrors: [] }, stagedIds };
}

function handleCompaniesDelete(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const companyIds = readStringArray(args['companyIds']);
  const deletedCompanyIds: string[] = [];
  const stagedIds: string[] = [];
  const userErrors: B2BUserError[] = [];
  for (const companyId of companyIds) {
    if (!runtime.store.getEffectiveB2BCompanyById(companyId)) {
      userErrors.push(resourceNotFound(['companyIds']));
      continue;
    }
    deletedCompanyIds.push(companyId);
    stagedIds.push(...deleteCompanyTree(runtime, companyId));
  }
  return { payload: { deletedCompanyIds, userErrors }, stagedIds };
}

function handleContactCreate(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const companyId = readStringValue(args['companyId']);
  const company = companyId ? runtime.store.getEffectiveB2BCompanyById(companyId) : null;
  if (!company || !companyId) {
    return {
      payload: { companyContact: null, userErrors: [resourceNotFound(['companyId'])] },
      stagedIds: [],
    };
  }
  const contact = runtime.store.upsertStagedB2BCompanyContact(
    createContact(runtime, companyId, readInputObject(args['input'])),
  );
  stageCompany(runtime, { ...company, contactIds: appendUnique(company.contactIds, contact.id) });
  return { payload: { companyContact: contact, userErrors: [] }, stagedIds: [contact.id, companyId] };
}

function handleContactUpdate(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const contactId = readStringValue(args['companyContactId']);
  const contact = contactId ? runtime.store.getEffectiveB2BCompanyContactById(contactId) : null;
  if (!contact || !contactId) {
    return {
      payload: {
        companyContact: null,
        userErrors: [resourceNotFound(['companyContactId'], "The company contact doesn't exist.")],
      },
      stagedIds: [],
    };
  }
  const updated = runtime.store.upsertStagedB2BCompanyContact({
    ...contact,
    data: contactDataFromInput(
      readInputObject(args['input']),
      runtime.syntheticIdentity.makeSyntheticTimestamp(),
      contact.data,
    ),
  });
  return { payload: { companyContact: updated, userErrors: [] }, stagedIds: [updated.id] };
}

function deleteContact(runtime: ProxyRuntimeContext, contactId: string): string[] {
  const contact = runtime.store.getEffectiveB2BCompanyContactById(contactId);
  if (!contact) {
    return [];
  }
  const company = runtime.store.getEffectiveB2BCompanyById(contact.companyId);
  if (company) {
    stageCompany(runtime, { ...company, contactIds: removeFromList(company.contactIds, contactId) });
  }
  for (const location of runtime.store.listEffectiveB2BCompanyLocations()) {
    const roleAssignments = readPlainObjectArray(location.data['roleAssignments']).filter(
      (assignment) => assignment['companyContactId'] !== contactId,
    );
    if (roleAssignments.length !== readPlainObjectArray(location.data['roleAssignments']).length) {
      runtime.store.upsertStagedB2BCompanyLocation({
        ...location,
        data: jsonRecord({ ...location.data, roleAssignments: jsonValue(roleAssignments) }),
      });
    }
  }
  runtime.store.deleteStagedB2BCompanyContact(contactId);
  return [contactId, contact.companyId];
}

function handleContactDelete(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const contactId = readStringValue(args['companyContactId']);
  if (!contactId || !runtime.store.getEffectiveB2BCompanyContactById(contactId)) {
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
    stagedIds: deleteContact(runtime, contactId),
  };
}

function handleContactsDelete(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const contactIds = readStringArray(args['companyContactIds']);
  const deletedCompanyContactIds: string[] = [];
  const stagedIds: string[] = [];
  const userErrors: B2BUserError[] = [];
  for (const contactId of contactIds) {
    if (!runtime.store.getEffectiveB2BCompanyContactById(contactId)) {
      userErrors.push(resourceNotFound(['companyContactIds'], "The company contact doesn't exist."));
      continue;
    }
    deletedCompanyContactIds.push(contactId);
    stagedIds.push(...deleteContact(runtime, contactId));
  }
  return { payload: { deletedCompanyContactIds, userErrors }, stagedIds };
}

function handleAssignCustomerAsContact(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): {
  payload: B2BMutationPayload;
  stagedIds: string[];
} {
  const companyId = readStringValue(args['companyId']);
  const customerId = readStringValue(args['customerId']);
  const company = companyId ? runtime.store.getEffectiveB2BCompanyById(companyId) : null;
  if (!company || !companyId) {
    return { payload: { companyContact: null, userErrors: [resourceNotFound(['companyId'])] }, stagedIds: [] };
  }
  if (!customerId) {
    return { payload: { companyContact: null, userErrors: [resourceNotFound(['customerId'])] }, stagedIds: [] };
  }
  const contact = createContact(runtime, companyId, {}, false);
  const stagedContact = runtime.store.upsertStagedB2BCompanyContact({
    ...contact,
    data: jsonRecord({
      ...contact.data,
      customerId,
      customer: { __typename: 'Customer', id: customerId },
    }),
  });
  stageCompany(runtime, { ...company, contactIds: appendUnique(company.contactIds, stagedContact.id) });
  return { payload: { companyContact: stagedContact, userErrors: [] }, stagedIds: [stagedContact.id, companyId] };
}

function handleContactRemoveFromCompany(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): {
  payload: B2BMutationPayload;
  stagedIds: string[];
} {
  const contactId = readStringValue(args['companyContactId']);
  if (!contactId || !runtime.store.getEffectiveB2BCompanyContactById(contactId)) {
    return {
      payload: {
        removedCompanyContactId: null,
        userErrors: [resourceNotFound(['companyContactId'], "The company contact doesn't exist.")],
      },
      stagedIds: [],
    };
  }
  return {
    payload: { removedCompanyContactId: contactId, userErrors: [] },
    stagedIds: deleteContact(runtime, contactId),
  };
}

function handleAssignMainContact(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const companyId = readStringValue(args['companyId']);
  const contactId = readStringValue(args['companyContactId']);
  const company = companyId ? runtime.store.getEffectiveB2BCompanyById(companyId) : null;
  const contact = contactId ? runtime.store.getEffectiveB2BCompanyContactById(contactId) : null;
  if (!company || !companyId) {
    return { payload: { company: null, userErrors: [resourceNotFound(['companyId'])] }, stagedIds: [] };
  }
  if (!contact || contact.companyId !== companyId) {
    return { payload: { company: null, userErrors: [resourceNotFound(['companyContactId'])] }, stagedIds: [] };
  }
  const stagedIds: string[] = [companyId];
  for (const candidateId of company.contactIds) {
    const candidate = runtime.store.getEffectiveB2BCompanyContactById(candidateId);
    if (!candidate) {
      continue;
    }
    runtime.store.upsertStagedB2BCompanyContact({
      ...candidate,
      data: jsonRecord({ ...candidate.data, isMainContact: candidate.id === contactId }),
    });
    stagedIds.push(candidate.id);
  }
  return { payload: { company: runtime.store.getEffectiveB2BCompanyById(companyId), userErrors: [] }, stagedIds };
}

function handleRevokeMainContact(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const companyId = readStringValue(args['companyId']);
  const company = companyId ? runtime.store.getEffectiveB2BCompanyById(companyId) : null;
  if (!company || !companyId) {
    return { payload: { company: null, userErrors: [resourceNotFound(['companyId'])] }, stagedIds: [] };
  }
  const stagedIds: string[] = [companyId];
  for (const contactId of company.contactIds) {
    const contact = runtime.store.getEffectiveB2BCompanyContactById(contactId);
    if (contact) {
      runtime.store.upsertStagedB2BCompanyContact({
        ...contact,
        data: jsonRecord({ ...contact.data, isMainContact: false }),
      });
      stagedIds.push(contact.id);
    }
  }
  return { payload: { company: runtime.store.getEffectiveB2BCompanyById(companyId), userErrors: [] }, stagedIds };
}

function handleLocationCreate(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const companyId = readStringValue(args['companyId']);
  const company = companyId ? runtime.store.getEffectiveB2BCompanyById(companyId) : null;
  if (!company || !companyId) {
    return { payload: { companyLocation: null, userErrors: [resourceNotFound(['companyId'])] }, stagedIds: [] };
  }
  const fallbackName = readStringValue(company.data['name']) ?? 'Company location';
  const location = runtime.store.upsertStagedB2BCompanyLocation(
    createLocation(runtime, companyId, readInputObject(args['input']), fallbackName),
  );
  stageCompany(runtime, { ...company, locationIds: appendUnique(company.locationIds, location.id) });
  return { payload: { companyLocation: location, userErrors: [] }, stagedIds: [location.id, companyId] };
}

function handleLocationUpdate(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const locationId = readStringValue(args['companyLocationId']);
  const location = locationId ? runtime.store.getEffectiveB2BCompanyLocationById(locationId) : null;
  if (!location || !locationId) {
    return {
      payload: {
        companyLocation: null,
        userErrors: [resourceNotFound(['input'], "The company location doesn't exist")],
      },
      stagedIds: [],
    };
  }
  const updated = runtime.store.upsertStagedB2BCompanyLocation({
    ...location,
    data: locationDataFromInput(
      runtime,
      readInputObject(args['input']),
      runtime.syntheticIdentity.makeSyntheticTimestamp(),
      location.data,
    ),
  });
  return { payload: { companyLocation: updated, userErrors: [] }, stagedIds: [updated.id] };
}

function deleteLocation(runtime: ProxyRuntimeContext, locationId: string): string[] {
  const location = runtime.store.getEffectiveB2BCompanyLocationById(locationId);
  if (!location) {
    return [];
  }
  const company = runtime.store.getEffectiveB2BCompanyById(location.companyId);
  if (company) {
    stageCompany(runtime, { ...company, locationIds: removeFromList(company.locationIds, locationId) });
  }
  for (const contact of runtime.store.listEffectiveB2BCompanyContacts()) {
    const roleAssignments = readPlainObjectArray(contact.data['roleAssignments']).filter(
      (assignment) => assignment['companyLocationId'] !== locationId,
    );
    if (roleAssignments.length !== readPlainObjectArray(contact.data['roleAssignments']).length) {
      runtime.store.upsertStagedB2BCompanyContact({
        ...contact,
        data: jsonRecord({ ...contact.data, roleAssignments: jsonValue(roleAssignments) }),
      });
    }
  }
  runtime.store.deleteStagedB2BCompanyLocation(locationId);
  return [locationId, location.companyId];
}

function handleLocationDelete(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const locationId = readStringValue(args['companyLocationId']);
  if (!locationId || !runtime.store.getEffectiveB2BCompanyLocationById(locationId)) {
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
    stagedIds: deleteLocation(runtime, locationId),
  };
}

function handleLocationsDelete(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const locationIds = readStringArray(args['companyLocationIds']);
  const deletedCompanyLocationIds: string[] = [];
  const stagedIds: string[] = [];
  const userErrors: B2BUserError[] = [];
  for (const locationId of locationIds) {
    if (!runtime.store.getEffectiveB2BCompanyLocationById(locationId)) {
      userErrors.push(resourceNotFound(['companyLocationIds'], "The company location doesn't exist"));
      continue;
    }
    deletedCompanyLocationIds.push(locationId);
    stagedIds.push(...deleteLocation(runtime, locationId));
  }
  return { payload: { deletedCompanyLocationIds, userErrors }, stagedIds };
}

function buildRoleAssignment(
  runtime: ProxyRuntimeContext,
  contact: B2BCompanyContactRecord,
  role: B2BCompanyContactRoleRecord,
  location: B2BCompanyLocationRecord,
): Record<string, JsonValue> {
  const id = runtime.syntheticIdentity.makeProxySyntheticGid('CompanyContactRoleAssignment');
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

function stageRoleAssignments(runtime: ProxyRuntimeContext, assignments: Record<string, unknown>[]): string[] {
  const stagedIds: string[] = [];
  for (const assignment of assignments) {
    const contactId = readStringValue(assignment['companyContactId']);
    const locationId = readStringValue(assignment['companyLocationId']);
    if (contactId) {
      const contact = runtime.store.getEffectiveB2BCompanyContactById(contactId);
      if (contact) {
        runtime.store.upsertStagedB2BCompanyContact({
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
      const location = runtime.store.getEffectiveB2BCompanyLocationById(locationId);
      if (location) {
        runtime.store.upsertStagedB2BCompanyLocation({
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
  runtime: ProxyRuntimeContext,
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
    const contact = contactId ? runtime.store.getEffectiveB2BCompanyContactById(contactId) : null;
    const role = roleId ? runtime.store.getEffectiveB2BCompanyContactRoleById(roleId) : null;
    const location = locationId ? runtime.store.getEffectiveB2BCompanyLocationById(locationId) : null;
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
    assignments.push(buildRoleAssignment(runtime, contact, role, location));
  }
  return { assignments, userErrors };
}

function revokeRoleAssignments(
  runtime: ProxyRuntimeContext,
  assignmentIds: string[],
  options: { contactId?: string | null; locationId?: string | null; revokeAll?: boolean } = {},
): string[] {
  const removedIds = new Set<string>();
  for (const contact of runtime.store.listEffectiveB2BCompanyContacts()) {
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
      runtime.store.upsertStagedB2BCompanyContact({
        ...contact,
        data: jsonRecord({ ...contact.data, roleAssignments: jsonValue(next) }),
      });
    }
  }
  for (const location of runtime.store.listEffectiveB2BCompanyLocations()) {
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
      runtime.store.upsertStagedB2BCompanyLocation({
        ...location,
        data: jsonRecord({ ...location.data, roleAssignments: jsonValue(next) }),
      });
    }
  }
  return [...removedIds];
}

function handleAssignAddress(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const locationId = readStringValue(args['locationId']);
  const location = locationId ? runtime.store.getEffectiveB2BCompanyLocationById(locationId) : null;
  if (!location || !locationId) {
    return { payload: { addresses: [], userErrors: [resourceNotFound(['locationId'])] }, stagedIds: [] };
  }
  const address = addressFromInput(runtime, readInputObject(args['address']));
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
  const updated = runtime.store.upsertStagedB2BCompanyLocation({ ...location, data: jsonRecord(nextData) });
  return {
    payload: { addresses, userErrors: [] },
    stagedIds: [updated.id, ...addresses.map((item) => String(item['id']))],
  };
}

function handleAddressDelete(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const addressId = readStringValue(args['addressId']);
  if (!addressId) {
    return { payload: { deletedAddressId: null, userErrors: [resourceNotFound(['addressId'])] }, stagedIds: [] };
  }
  for (const location of runtime.store.listEffectiveB2BCompanyLocations()) {
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
      runtime.store.upsertStagedB2BCompanyLocation({ ...location, data: jsonRecord(nextData) });
      return { payload: { deletedAddressId: addressId, userErrors: [] }, stagedIds: [location.id, addressId] };
    }
  }
  return { payload: { deletedAddressId: null, userErrors: [resourceNotFound(['addressId'])] }, stagedIds: [] };
}

function handleAssignStaff(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const locationId = readStringValue(args['companyLocationId']);
  const location = locationId ? runtime.store.getEffectiveB2BCompanyLocationById(locationId) : null;
  if (!location || !locationId) {
    return {
      payload: { companyLocationStaffMemberAssignments: [], userErrors: [resourceNotFound(['companyLocationId'])] },
      stagedIds: [],
    };
  }
  const assignments = readStringArray(args['staffMemberIds']).map((staffMemberId) => {
    const id = runtime.syntheticIdentity.makeProxySyntheticGid('CompanyLocationStaffMemberAssignment');
    return {
      __typename: 'CompanyLocationStaffMemberAssignment',
      id,
      staffMemberId,
      staffMember: { __typename: 'StaffMember', id: staffMemberId },
      companyLocation: recordSource(location, 'CompanyLocation'),
    };
  });
  const updated = runtime.store.upsertStagedB2BCompanyLocation({
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

function handleRemoveStaff(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const assignmentIds = readStringArray(args['companyLocationStaffMemberAssignmentIds']);
  const removed = new Set<string>();
  const stagedIds: string[] = [];
  for (const location of runtime.store.listEffectiveB2BCompanyLocations()) {
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
      runtime.store.upsertStagedB2BCompanyLocation({
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

function handleTaxSettingsUpdate(
  runtime: ProxyRuntimeContext,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } {
  const locationId = readStringValue(args['companyLocationId']);
  const location = locationId ? runtime.store.getEffectiveB2BCompanyLocationById(locationId) : null;
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
    updatedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
  };
  maybeSetString(data, args, 'taxRegistrationId');
  maybeSetBoolean(data, args, 'taxExempt');
  const updated = runtime.store.upsertStagedB2BCompanyLocation({ ...location, data: jsonRecord(data) });
  return { payload: { companyLocation: updated, userErrors: [] }, stagedIds: [updated.id] };
}

function dispatchB2BMutationRoot(
  runtime: ProxyRuntimeContext,
  rootField: string,
  args: Record<string, unknown>,
): { payload: B2BMutationPayload; stagedIds: string[] } | null {
  switch (rootField) {
    case 'companyCreate':
      return handleCompanyCreate(runtime, args);
    case 'companyUpdate':
      return handleCompanyUpdate(runtime, args);
    case 'companyDelete':
      return handleCompanyDelete(runtime, args);
    case 'companiesDelete':
      return handleCompaniesDelete(runtime, args);
    case 'companyContactCreate':
      return handleContactCreate(runtime, args);
    case 'companyContactUpdate':
      return handleContactUpdate(runtime, args);
    case 'companyContactDelete':
      return handleContactDelete(runtime, args);
    case 'companyContactsDelete':
      return handleContactsDelete(runtime, args);
    case 'companyAssignCustomerAsContact':
      return handleAssignCustomerAsContact(runtime, args);
    case 'companyContactRemoveFromCompany':
      return handleContactRemoveFromCompany(runtime, args);
    case 'companyAssignMainContact':
      return handleAssignMainContact(runtime, args);
    case 'companyRevokeMainContact':
      return handleRevokeMainContact(runtime, args);
    case 'companyLocationCreate':
      return handleLocationCreate(runtime, args);
    case 'companyLocationUpdate':
      return handleLocationUpdate(runtime, args);
    case 'companyLocationDelete':
      return handleLocationDelete(runtime, args);
    case 'companyLocationsDelete':
      return handleLocationsDelete(runtime, args);
    case 'companyLocationAssignAddress':
      return handleAssignAddress(runtime, args);
    case 'companyAddressDelete':
      return handleAddressDelete(runtime, args);
    case 'companyLocationAssignStaffMembers':
      return handleAssignStaff(runtime, args);
    case 'companyLocationRemoveStaffMembers':
      return handleRemoveStaff(runtime, args);
    case 'companyLocationTaxSettingsUpdate':
      return handleTaxSettingsUpdate(runtime, args);
    case 'companyContactAssignRole': {
      const { assignments, userErrors } = resolveRoleAssignmentInputs(runtime, [
        {
          companyContactId: args['companyContactId'],
          companyContactRoleId: args['companyContactRoleId'],
          companyLocationId: args['companyLocationId'],
        },
      ]);
      const stagedIds = userErrors.length > 0 ? [] : stageRoleAssignments(runtime, assignments);
      return { payload: { companyContactRoleAssignment: assignments[0] ?? null, userErrors }, stagedIds };
    }
    case 'companyContactAssignRoles': {
      const contactId = readStringValue(args['companyContactId']) ?? undefined;
      const { assignments, userErrors } = resolveRoleAssignmentInputs(
        runtime,
        readPlainObjectArray(args['rolesToAssign']),
        contactId,
      );
      const stagedIds = userErrors.length > 0 ? [] : stageRoleAssignments(runtime, assignments);
      return { payload: { roleAssignments: assignments, userErrors }, stagedIds };
    }
    case 'companyLocationAssignRoles': {
      const locationId = readStringValue(args['companyLocationId']);
      const { assignments, userErrors } = resolveRoleAssignmentInputs(
        runtime,
        readPlainObjectArray(args['rolesToAssign']),
        undefined,
        locationId ?? undefined,
      );
      const stagedIds = userErrors.length > 0 ? [] : stageRoleAssignments(runtime, assignments);
      return { payload: { roleAssignments: assignments, userErrors }, stagedIds };
    }
    case 'companyContactRevokeRole': {
      const assignmentId = readStringValue(args['companyContactRoleAssignmentId']);
      const revoked = assignmentId
        ? revokeRoleAssignments(runtime, [assignmentId], { contactId: readStringValue(args['companyContactId']) })
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
      const revoked = revokeRoleAssignments(runtime, readStringArray(args['roleAssignmentIds']), {
        contactId: readStringValue(args['companyContactId']),
        revokeAll: readBooleanValue(args['revokeAll']) === true,
      });
      return { payload: { revokedRoleAssignmentIds: revoked, userErrors: [] }, stagedIds: revoked };
    }
    case 'companyLocationRevokeRoles': {
      const revoked = revokeRoleAssignments(runtime, readStringArray(args['rolesToRevoke']), {
        locationId: readStringValue(args['companyLocationId']),
      });
      return { payload: { revokedRoleAssignmentIds: revoked, userErrors: [] }, stagedIds: revoked };
    }
    default:
      return null;
  }
}

export function handleB2BMutation(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): B2BMutationResult | null {
  const payloads = new Map<string, B2BMutationPayload>();
  const stagedResourceIds = new Set<string>();
  let handled = false;
  let staged = false;

  for (const field of getRootFields(document)) {
    const result = dispatchB2BMutationRoot(runtime, field.name.value, getFieldArguments(field, variables));
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
    response: mutationResponse(runtime, document, variables, payloads),
    staged,
    stagedResourceIds: [...stagedResourceIds],
    notes: 'Staged locally in the in-memory B2B company draft store.',
  };
}

export function handleB2BQuery(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);
    const id = typeof args['id'] === 'string' && args['id'].length > 0 ? args['id'] : null;

    switch (field.name.value) {
      case 'companies':
        data[key] = serializeCompanies(runtime, field, variables);
        break;
      case 'companiesCount':
        data[key] = serializeCount(field, runtime.store.listEffectiveB2BCompanies().length);
        break;
      case 'company': {
        const company = id ? runtime.store.getEffectiveB2BCompanyById(id) : null;
        data[key] = company ? serializeCompany(runtime, company, field, variables) : null;
        break;
      }
      case 'companyContact': {
        const contact = id ? runtime.store.getEffectiveB2BCompanyContactById(id) : null;
        data[key] = contact ? serializeCompanyContact(runtime, contact, field) : null;
        break;
      }
      case 'companyContactRole': {
        const role = id ? runtime.store.getEffectiveB2BCompanyContactRoleById(id) : null;
        data[key] = role ? serializeCompanyRole(role, field) : null;
        break;
      }
      case 'companyLocation': {
        const location = id ? runtime.store.getEffectiveB2BCompanyLocationById(id) : null;
        data[key] = location ? serializeCompanyLocation(runtime, location, field) : null;
        break;
      }
      case 'companyLocations':
        data[key] = serializeCompanyLocations(runtime, field, variables);
        break;
      default:
        data[key] = null;
        break;
    }
  }

  return { data };
}
