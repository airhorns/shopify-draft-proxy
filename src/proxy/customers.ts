import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { CustomerCatalogConnectionRecord, CustomerCatalogPageInfoRecord, CustomerRecord } from '../state/types.js';

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function hasOwnField(value: Record<string, unknown>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function getFieldResponseKey(field: FieldNode): string {
  return field.alias?.value ?? field.name.value;
}

function getSelectedChildFields(field: FieldNode): FieldNode[] {
  return (field.selectionSet?.selections ?? []).filter(
    (selection): selection is FieldNode => selection.kind === Kind.FIELD,
  );
}

function normalizeStringField(
  raw: Record<string, unknown>,
  key: string,
  fallback: string | null = null,
): string | null {
  if (!hasOwnField(raw, key)) {
    return fallback;
  }

  const value = raw[key];
  return typeof value === 'string' ? value : null;
}

function normalizeStringLikeField(
  raw: Record<string, unknown>,
  key: string,
  fallback: string | null = null,
): string | null {
  if (!hasOwnField(raw, key)) {
    return fallback;
  }

  const value = raw[key];
  if (typeof value === 'string') {
    return value;
  }

  if (typeof value === 'number' && Number.isFinite(value)) {
    return String(value);
  }

  return null;
}

function normalizeCountField(
  raw: Record<string, unknown>,
  key: string,
  fallback: string | number | null = null,
): string | number | null {
  if (!hasOwnField(raw, key)) {
    return fallback;
  }

  const value = raw[key];
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }

  return typeof value === 'string' ? value : null;
}

function normalizeBooleanField(
  raw: Record<string, unknown>,
  key: string,
  fallback: boolean | null = null,
): boolean | null {
  if (!hasOwnField(raw, key)) {
    return fallback;
  }

  return typeof raw[key] === 'boolean' ? raw[key] : null;
}

function normalizeStringArrayField(raw: Record<string, unknown>, key: string, fallback: string[] = []): string[] {
  if (!hasOwnField(raw, key)) {
    return structuredClone(fallback);
  }

  const value = raw[key];
  if (!Array.isArray(value)) {
    return [];
  }

  return value.filter((entry): entry is string => typeof entry === 'string');
}

function buildCustomerDisplayName(
  firstName: string | null,
  lastName: string | null,
  email: string | null,
): string | null {
  const parts = [firstName?.trim() ?? '', lastName?.trim() ?? ''].filter((value) => value.length > 0);
  if (parts.length > 0) {
    return parts.join(' ');
  }

  return email?.trim() || null;
}

function maskPhoneNumber(phone: string | null): string | null {
  return phone;
}

function normalizeMoney(raw: unknown, fallback: CustomerRecord['amountSpent'] = null): CustomerRecord['amountSpent'] {
  if (raw === undefined) {
    return fallback;
  }

  if (!isObject(raw)) {
    return null;
  }

  return {
    amount: normalizeStringField(raw, 'amount'),
    currencyCode: normalizeStringField(raw, 'currencyCode'),
  };
}

function normalizeDefaultEmailAddress(
  raw: unknown,
  fallback: CustomerRecord['defaultEmailAddress'] = null,
): CustomerRecord['defaultEmailAddress'] {
  if (raw === undefined) {
    return fallback;
  }

  if (!isObject(raw)) {
    return null;
  }

  return {
    emailAddress: normalizeStringField(raw, 'emailAddress'),
  };
}

function normalizeDefaultPhoneNumber(
  raw: unknown,
  fallback: CustomerRecord['defaultPhoneNumber'] = null,
): CustomerRecord['defaultPhoneNumber'] {
  if (raw === undefined) {
    return fallback;
  }

  if (!isObject(raw)) {
    return null;
  }

  return {
    phoneNumber: normalizeStringField(raw, 'phoneNumber'),
  };
}

function normalizeDefaultAddress(
  raw: unknown,
  fallback: CustomerRecord['defaultAddress'] = null,
): CustomerRecord['defaultAddress'] {
  if (raw === undefined) {
    return fallback;
  }

  if (!isObject(raw)) {
    return null;
  }

  return {
    address1: normalizeStringField(raw, 'address1'),
    city: normalizeStringField(raw, 'city'),
    province: normalizeStringField(raw, 'province'),
    country: normalizeStringField(raw, 'country'),
    zip: normalizeStringField(raw, 'zip'),
    formattedArea: normalizeStringField(raw, 'formattedArea'),
  };
}

function normalizeCustomer(raw: unknown): CustomerRecord | null {
  if (!isObject(raw)) {
    return null;
  }

  const id = raw['id'];
  if (typeof id !== 'string' || !id) {
    return null;
  }

  const existing = store.getEffectiveCustomerById(id);

  return {
    id,
    firstName: normalizeStringField(raw, 'firstName', existing?.firstName ?? null),
    lastName: normalizeStringField(raw, 'lastName', existing?.lastName ?? null),
    displayName: normalizeStringField(raw, 'displayName', existing?.displayName ?? null),
    email: normalizeStringField(raw, 'email', existing?.email ?? null),
    legacyResourceId: normalizeStringLikeField(raw, 'legacyResourceId', existing?.legacyResourceId ?? null),
    locale: normalizeStringField(raw, 'locale', existing?.locale ?? null),
    note: normalizeStringField(raw, 'note', existing?.note ?? null),
    canDelete: normalizeBooleanField(raw, 'canDelete', existing?.canDelete ?? null),
    verifiedEmail: normalizeBooleanField(raw, 'verifiedEmail', existing?.verifiedEmail ?? null),
    taxExempt: normalizeBooleanField(raw, 'taxExempt', existing?.taxExempt ?? null),
    state: normalizeStringField(raw, 'state', existing?.state ?? null),
    tags: normalizeStringArrayField(raw, 'tags', existing?.tags ?? []),
    numberOfOrders: normalizeCountField(raw, 'numberOfOrders', existing?.numberOfOrders ?? null),
    amountSpent: normalizeMoney(
      hasOwnField(raw, 'amountSpent') ? raw['amountSpent'] : undefined,
      existing?.amountSpent ?? null,
    ),
    defaultEmailAddress: normalizeDefaultEmailAddress(
      hasOwnField(raw, 'defaultEmailAddress') ? raw['defaultEmailAddress'] : undefined,
      existing?.defaultEmailAddress ?? null,
    ),
    defaultPhoneNumber: normalizeDefaultPhoneNumber(
      hasOwnField(raw, 'defaultPhoneNumber') ? raw['defaultPhoneNumber'] : undefined,
      existing?.defaultPhoneNumber ?? null,
    ),
    defaultAddress: normalizeDefaultAddress(
      hasOwnField(raw, 'defaultAddress') ? raw['defaultAddress'] : undefined,
      existing?.defaultAddress ?? null,
    ),
    createdAt: normalizeStringField(raw, 'createdAt', existing?.createdAt ?? null),
    updatedAt: normalizeStringField(raw, 'updatedAt', existing?.updatedAt ?? null),
  };
}

function serializeMoneySelection(
  field: FieldNode,
  value: CustomerRecord['amountSpent'],
): Record<string, unknown> | null {
  if (!value) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'amount':
        result[key] = value.amount;
        break;
      case 'currencyCode':
        result[key] = value.currencyCode;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDefaultEmailSelection(
  field: FieldNode,
  value: CustomerRecord['defaultEmailAddress'],
): Record<string, unknown> | null {
  if (!value) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'emailAddress':
        result[key] = value.emailAddress;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDefaultPhoneNumberSelection(
  field: FieldNode,
  value: CustomerRecord['defaultPhoneNumber'],
): Record<string, unknown> | null {
  if (!value) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'phoneNumber':
        result[key] = value.phoneNumber;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDefaultAddressSelection(
  field: FieldNode,
  value: CustomerRecord['defaultAddress'],
): Record<string, unknown> | null {
  if (!value) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'address1':
        result[key] = value.address1;
        break;
      case 'city':
        result[key] = value.city;
        break;
      case 'province':
        result[key] = value.province;
        break;
      case 'country':
        result[key] = value.country;
        break;
      case 'zip':
        result[key] = value.zip;
        break;
      case 'formattedArea':
        result[key] = value.formattedArea;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCustomerSelection(customer: CustomerRecord, field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = customer.id;
        break;
      case 'firstName':
        result[key] = customer.firstName;
        break;
      case 'lastName':
        result[key] = customer.lastName;
        break;
      case 'displayName':
        result[key] = customer.displayName;
        break;
      case 'email':
        result[key] = customer.email;
        break;
      case 'legacyResourceId':
        result[key] = customer.legacyResourceId;
        break;
      case 'locale':
        result[key] = customer.locale;
        break;
      case 'note':
        result[key] = customer.note;
        break;
      case 'canDelete':
        result[key] = customer.canDelete;
        break;
      case 'verifiedEmail':
        result[key] = customer.verifiedEmail;
        break;
      case 'taxExempt':
        result[key] = customer.taxExempt;
        break;
      case 'state':
        result[key] = customer.state;
        break;
      case 'tags':
        result[key] = structuredClone(customer.tags);
        break;
      case 'numberOfOrders':
        result[key] = customer.numberOfOrders;
        break;
      case 'amountSpent':
        result[key] = serializeMoneySelection(selection, customer.amountSpent);
        break;
      case 'defaultEmailAddress':
        result[key] = serializeDefaultEmailSelection(selection, customer.defaultEmailAddress);
        break;
      case 'defaultPhoneNumber':
        result[key] = serializeDefaultPhoneNumberSelection(selection, customer.defaultPhoneNumber);
        break;
      case 'defaultAddress':
        result[key] = serializeDefaultAddressSelection(selection, customer.defaultAddress);
        break;
      case 'createdAt':
        result[key] = customer.createdAt;
        break;
      case 'updatedAt':
        result[key] = customer.updatedAt;
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializePageInfo(field: FieldNode): Record<string, boolean | string | null> {
  const pageInfo: Record<string, boolean | string | null> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'hasNextPage':
      case 'hasPreviousPage':
        pageInfo[key] = false;
        break;
      case 'startCursor':
      case 'endCursor':
        pageInfo[key] = null;
        break;
      default:
        pageInfo[key] = null;
        break;
    }
  }

  return pageInfo;
}

function buildSyntheticCustomerCursor(customerId: string): string {
  return `cursor:${customerId}`;
}

function resolveCatalogCustomerCursor(
  customerId: string,
  catalogConnection: CustomerCatalogConnectionRecord | null,
): string {
  return catalogConnection?.cursorByCustomerId[customerId] ?? buildSyntheticCustomerCursor(customerId);
}

function listCustomersForConnection(catalogConnection: CustomerCatalogConnectionRecord | null): CustomerRecord[] {
  if (!catalogConnection) {
    return store.listEffectiveCustomers();
  }

  const orderedCustomers = catalogConnection.orderedCustomerIds
    .map((customerId) => store.getEffectiveCustomerById(customerId))
    .filter((customer): customer is CustomerRecord => customer !== null);
  const seenCustomerIds = new Set(orderedCustomers.map((customer) => customer.id));
  const extraCustomers = store.listEffectiveCustomers().filter((customer) => !seenCustomerIds.has(customer.id));
  return [...orderedCustomers, ...extraCustomers];
}

type CustomerQueryToken =
  | { type: 'term'; value: string }
  | { type: 'or' }
  | { type: 'lparen' }
  | { type: 'rparen' }
  | { type: 'not' };

type CustomerQueryNode =
  | { type: 'term'; value: string }
  | { type: 'and'; children: CustomerQueryNode[] }
  | { type: 'or'; children: CustomerQueryNode[] }
  | { type: 'not'; child: CustomerQueryNode };

function tokenizeCustomerQuery(query: string): CustomerQueryToken[] {
  const tokens: CustomerQueryToken[] = [];
  let current = '';
  let inQuotes = false;

  const flushCurrent = (): void => {
    const value = current.trim();
    if (!value) {
      current = '';
      return;
    }

    if (value.toUpperCase() === 'OR') {
      tokens.push({ type: 'or' });
    } else {
      tokens.push({ type: 'term', value });
    }
    current = '';
  };

  for (let index = 0; index < query.length; index += 1) {
    const character = query[index] ?? '';

    if (character === '"') {
      inQuotes = !inQuotes;
      continue;
    }

    if (!inQuotes && /\s/u.test(character)) {
      flushCurrent();
      continue;
    }

    if (!inQuotes && character === '(') {
      flushCurrent();
      tokens.push({ type: 'lparen' });
      continue;
    }

    if (!inQuotes && character === ')') {
      flushCurrent();
      tokens.push({ type: 'rparen' });
      continue;
    }

    if (!inQuotes && character === '-' && !current) {
      const nextCharacter = query[index + 1] ?? '';
      if (nextCharacter === '(') {
        tokens.push({ type: 'not' });
        continue;
      }
    }

    current += character;
  }

  flushCurrent();
  return tokens;
}

function parseCustomerQuery(query: string): CustomerQueryNode | null {
  const tokens = tokenizeCustomerQuery(query);
  if (tokens.length === 0) {
    return null;
  }

  let index = 0;

  const parseOrExpression = (): CustomerQueryNode | null => {
    const firstChild = parseAndExpression();
    if (!firstChild) {
      return null;
    }

    const children: CustomerQueryNode[] = [firstChild];
    while (tokens[index]?.type === 'or') {
      index += 1;
      const nextChild = parseAndExpression();
      if (!nextChild) {
        break;
      }
      children.push(nextChild);
    }

    return children.length === 1 ? (children[0] ?? null) : { type: 'or', children };
  };

  const parseAndExpression = (): CustomerQueryNode | null => {
    const children: CustomerQueryNode[] = [];

    while (index < tokens.length) {
      const token = tokens[index];
      if (!token || token.type === 'or' || token.type === 'rparen') {
        break;
      }

      const child = parseUnaryExpression();
      if (!child) {
        break;
      }
      children.push(child);
    }

    if (children.length === 0) {
      return null;
    }

    return children.length === 1 ? (children[0] ?? null) : { type: 'and', children };
  };

  const parseUnaryExpression = (): CustomerQueryNode | null => {
    const token = tokens[index];
    if (!token) {
      return null;
    }

    if (token.type === 'not') {
      index += 1;
      const child = parseUnaryExpression();
      return child ? { type: 'not', child } : null;
    }

    if (token.type === 'term') {
      index += 1;
      return { type: 'term', value: token.value };
    }

    if (token.type === 'lparen') {
      index += 1;
      const child = parseOrExpression();
      if (tokens[index]?.type === 'rparen') {
        index += 1;
      }
      return child;
    }

    return null;
  };

  return parseOrExpression();
}

function normalizeCustomerSearchValue(value: string | null | undefined): string {
  return (value ?? '').trim().toLowerCase();
}

function isPrefixPattern(rawValue: string): boolean {
  return rawValue.endsWith('*');
}

function matchesStringValue(
  candidate: string | null | undefined,
  rawValue: string,
  matchMode: 'includes' | 'exact',
): boolean {
  const value = rawValue.trim().toLowerCase();
  if (!value) {
    return true;
  }

  const prefixMode = isPrefixPattern(value);
  const normalizedValue = prefixMode ? value.slice(0, -1) : value;
  if (!normalizedValue) {
    return true;
  }

  const normalizedCandidate = normalizeCustomerSearchValue(candidate);
  if (prefixMode) {
    if (normalizedCandidate.startsWith(normalizedValue)) {
      return true;
    }

    return normalizedCandidate.split(/[^a-z0-9]+/u).some((part) => part.startsWith(normalizedValue));
  }

  return matchMode === 'exact'
    ? normalizedCandidate === normalizedValue
    : normalizedCandidate.includes(normalizedValue);
}

function customerMatchesBareToken(customer: CustomerRecord, token: string): boolean {
  const normalizedToken = normalizeCustomerSearchValue(token);
  if (!normalizedToken) {
    return true;
  }

  const haystacks = [
    customer.displayName,
    customer.email,
    customer.defaultEmailAddress?.emailAddress ?? null,
    customer.firstName,
    customer.lastName,
    ...customer.tags,
  ];

  return haystacks.some((value) => matchesStringValue(value, normalizedToken, 'includes'));
}

function customerMatchesPositiveQueryTerm(customer: CustomerRecord, term: string): boolean {
  const separatorIndex = term.indexOf(':');
  if (separatorIndex <= 0) {
    return customerMatchesBareToken(customer, term);
  }

  const rawField = term.slice(0, separatorIndex);
  const rawValue = term.slice(separatorIndex + 1);
  const field = normalizeCustomerSearchValue(rawField);

  switch (field) {
    case 'email':
      return (
        matchesStringValue(customer.email, rawValue, 'includes') ||
        matchesStringValue(customer.defaultEmailAddress?.emailAddress ?? null, rawValue, 'includes')
      );
    case 'state':
      return matchesStringValue(customer.state, rawValue, 'exact');
    case 'tag':
    case 'tags':
      return customer.tags.some((tag) => matchesStringValue(tag, rawValue, 'exact'));
    case 'first_name':
      return matchesStringValue(customer.firstName, rawValue, 'includes');
    case 'last_name':
      return matchesStringValue(customer.lastName, rawValue, 'includes');
    case 'name':
    case 'display_name':
      return matchesStringValue(customer.displayName, rawValue, 'includes');
    default:
      return customerMatchesBareToken(customer, term);
  }
}

function customerMatchesQueryNode(customer: CustomerRecord, node: CustomerQueryNode): boolean {
  switch (node.type) {
    case 'term': {
      const negated = node.value.startsWith('-') && node.value.length > 1;
      const term = negated ? node.value.slice(1) : node.value;
      const matches = customerMatchesPositiveQueryTerm(customer, term);
      return negated ? !matches : matches;
    }
    case 'and':
      return node.children.every((child) => customerMatchesQueryNode(customer, child));
    case 'or':
      return node.children.some((child) => customerMatchesQueryNode(customer, child));
    case 'not':
      return !customerMatchesQueryNode(customer, node.child);
    default:
      return true;
  }
}

function filterCustomersByQuery(customers: CustomerRecord[], rawQuery: unknown): CustomerRecord[] {
  if (typeof rawQuery !== 'string' || !rawQuery.trim()) {
    return customers;
  }

  const parsedQuery = parseCustomerQuery(rawQuery);
  if (!parsedQuery) {
    return customers;
  }

  return customers.filter((customer) => customerMatchesQueryNode(customer, parsedQuery));
}

function compareNullableStrings(left: string | null, right: string | null): number {
  return (left ?? '').localeCompare(right ?? '');
}

function normalizeSortableString(value: string | null): string {
  return (value ?? '').trim().toLocaleLowerCase();
}

function compareCustomerIds(leftId: string, rightId: string): number {
  const leftTail = Number.parseInt(leftId.split('/').at(-1) ?? '', 10);
  const rightTail = Number.parseInt(rightId.split('/').at(-1) ?? '', 10);

  if (Number.isFinite(leftTail) && Number.isFinite(rightTail)) {
    return leftTail - rightTail;
  }

  return leftId.localeCompare(rightId);
}

function buildCustomerSortName(customer: CustomerRecord): string {
  const lastName = normalizeSortableString(customer.lastName);
  const firstName = normalizeSortableString(customer.firstName);
  const displayName = normalizeSortableString(customer.displayName);

  if (lastName || firstName) {
    return `${lastName}, ${firstName}`;
  }

  return displayName;
}

function compareSortableTuple(left: Array<string | null>, right: Array<string | null>): number {
  for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
    const comparison = normalizeSortableString(left[index] ?? null).localeCompare(
      normalizeSortableString(right[index] ?? null),
    );
    if (comparison !== 0) {
      return comparison;
    }
  }

  return 0;
}

function buildCustomerSearchConnectionKey(rawQuery: unknown, rawSortKey: unknown, rawReverse: unknown): string | null {
  const query = typeof rawQuery === 'string' ? rawQuery.trim() : '';
  const sortKey = typeof rawSortKey === 'string' ? rawSortKey : '';
  if (!query || sortKey !== 'RELEVANCE') {
    return null;
  }

  return JSON.stringify({
    query,
    sortKey,
    reverse: rawReverse === true,
  });
}

function sortCustomers(customers: CustomerRecord[], rawSortKey: unknown, rawReverse: unknown): CustomerRecord[] {
  const reverse = rawReverse === true;
  const sortKey = typeof rawSortKey === 'string' ? rawSortKey : null;
  const sorted = [...customers];

  if (sortKey === 'UPDATED_AT') {
    sorted.sort(
      (left, right) => compareNullableStrings(left.updatedAt, right.updatedAt) || left.id.localeCompare(right.id),
    );
  } else if (sortKey === 'CREATED_AT') {
    sorted.sort(
      (left, right) => compareNullableStrings(left.createdAt, right.createdAt) || left.id.localeCompare(right.id),
    );
  } else if (sortKey === 'NAME') {
    sorted.sort(
      (left, right) =>
        buildCustomerSortName(left).localeCompare(buildCustomerSortName(right)) || left.id.localeCompare(right.id),
    );
  } else if (sortKey === 'ID') {
    sorted.sort((left, right) => compareCustomerIds(left.id, right.id) || left.id.localeCompare(right.id));
  } else if (sortKey === 'LOCATION') {
    sorted.sort(
      (left, right) =>
        compareSortableTuple(
          [
            left.defaultAddress?.country ?? null,
            left.defaultAddress?.province ?? null,
            left.defaultAddress?.city ?? null,
          ],
          [
            right.defaultAddress?.country ?? null,
            right.defaultAddress?.province ?? null,
            right.defaultAddress?.city ?? null,
          ],
        ) || left.id.localeCompare(right.id),
    );
  }

  if (reverse) {
    sorted.reverse();
  }

  return sorted;
}

type CustomerSearchExtensionEntry = {
  path: string[];
  query: string;
  parsed: {
    field: string;
    match_all: string;
  };
  warnings: Array<{
    field: string;
    message: string;
    code: string;
  }>;
};

function buildCustomersCountSearchExtension(rawQuery: unknown, path: string[]): CustomerSearchExtensionEntry | null {
  if (typeof rawQuery !== 'string') {
    return null;
  }

  const query = rawQuery.trim();
  if (!query) {
    return null;
  }

  const singleFieldMatch = query.match(/^([A-Za-z_]+):(.*)$/u);
  if (!singleFieldMatch) {
    return null;
  }

  const field = singleFieldMatch[1]?.trim().toLowerCase() ?? '';
  const matchAll = singleFieldMatch[2]?.trim() ?? '';
  if (!matchAll) {
    return null;
  }

  if (field !== 'email' && field !== 'state') {
    return null;
  }

  return {
    path,
    query,
    parsed: {
      field,
      match_all: matchAll,
    },
    warnings: [
      {
        field,
        message: 'Invalid search field for this query.',
        code: 'invalid_field',
      },
    ],
  };
}

function serializeCustomersCount(_rawQuery: unknown, selections: readonly SelectionNode[]): Record<string, unknown> {
  const allCustomers = store.listEffectiveCustomers();
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'count':
        result[key] = allCustomers.length;
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

function buildCatalogPageInfo(
  visibleCustomers: CustomerRecord[],
  field: FieldNode,
  hasNextPage: boolean,
  hasPreviousPage: boolean,
  catalogConnection: CustomerCatalogConnectionRecord | null,
  options: { preserveBaselinePageInfo: boolean },
): Record<string, boolean | string | null> {
  const pageInfo = serializePageInfo(field);
  for (const pageInfoSelection of getSelectedChildFields(field)) {
    const pageInfoKey = getFieldResponseKey(pageInfoSelection);
    switch (pageInfoSelection.name.value) {
      case 'hasNextPage':
        pageInfo[pageInfoKey] = hasNextPage;
        break;
      case 'hasPreviousPage':
        pageInfo[pageInfoKey] = hasPreviousPage;
        break;
      case 'startCursor':
        pageInfo[pageInfoKey] = visibleCustomers[0]
          ? resolveCatalogCustomerCursor(visibleCustomers[0].id, catalogConnection)
          : options.preserveBaselinePageInfo
            ? (catalogConnection?.pageInfo.startCursor ?? null)
            : null;
        break;
      case 'endCursor':
        pageInfo[pageInfoKey] =
          visibleCustomers.length > 0
            ? resolveCatalogCustomerCursor(visibleCustomers[visibleCustomers.length - 1]!.id, catalogConnection)
            : options.preserveBaselinePageInfo
              ? (catalogConnection?.pageInfo.endCursor ?? null)
              : null;
        break;
    }
  }

  return pageInfo;
}

function serializeCustomersConnection(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const first =
    typeof args['first'] === 'number' && Number.isFinite(args['first']) ? Math.max(0, Math.floor(args['first'])) : null;
  const last =
    typeof args['last'] === 'number' && Number.isFinite(args['last']) ? Math.max(0, Math.floor(args['last'])) : null;
  const after = typeof args['after'] === 'string' ? args['after'] : null;
  const before = typeof args['before'] === 'string' ? args['before'] : null;

  const catalogConnection = store.getBaseCustomerCatalogConnection();
  const searchConnectionKey = buildCustomerSearchConnectionKey(args['query'], args['sortKey'], args['reverse']);
  const searchConnection = searchConnectionKey ? store.getBaseCustomerSearchConnection(searchConnectionKey) : null;
  const activeConnection = searchConnection ?? catalogConnection;
  const preserveBaselinePageInfo =
    ((typeof args['query'] !== 'string' && args['sortKey'] === undefined && args['reverse'] !== true) ||
      searchConnection !== null) &&
    before === null &&
    last === null;
  const allCustomers = sortCustomers(
    filterCustomersByQuery(listCustomersForConnection(activeConnection), args['query']),
    args['sortKey'],
    args['reverse'],
  );
  const afterIndex = after
    ? allCustomers.findIndex((customer) => resolveCatalogCustomerCursor(customer.id, activeConnection) === after)
    : -1;
  const beforeIndex = before
    ? allCustomers.findIndex((customer) => resolveCatalogCustomerCursor(customer.id, activeConnection) === before)
    : -1;
  const startIndex = afterIndex >= 0 ? afterIndex + 1 : 0;
  const endIndex = beforeIndex >= 0 ? beforeIndex : allCustomers.length;
  const cursorWindow = allCustomers.slice(startIndex, endIndex);
  const firstWindow = first === null ? cursorWindow : cursorWindow.slice(0, first);
  const visibleCustomers = last === null ? firstWindow : firstWindow.slice(Math.max(0, firstWindow.length - last));
  const visibleStartIndex =
    last === null ? startIndex : Math.max(startIndex, startIndex + firstWindow.length - visibleCustomers.length);
  const visibleEndIndex = visibleStartIndex + visibleCustomers.length;
  const calculatedHasPreviousPage = visibleStartIndex > 0;
  const calculatedHasNextPage = visibleEndIndex < allCustomers.length;
  const hasPreviousPage =
    calculatedHasPreviousPage || (preserveBaselinePageInfo && (activeConnection?.pageInfo.hasPreviousPage ?? false));
  const hasNextPage =
    calculatedHasNextPage || (preserveBaselinePageInfo && (activeConnection?.pageInfo.hasNextPage ?? false));

  const connection: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        connection[key] = visibleCustomers.map((customer) => serializeCustomerSelection(customer, selection));
        break;
      case 'edges':
        connection[key] = visibleCustomers.map((customer) => {
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edge[edgeKey] = resolveCatalogCustomerCursor(customer.id, activeConnection);
                break;
              case 'node':
                edge[edgeKey] = serializeCustomerSelection(customer, edgeSelection);
                break;
              default:
                edge[edgeKey] = null;
                break;
            }
          }
          return edge;
        });
        break;
      case 'pageInfo':
        connection[key] = buildCatalogPageInfo(
          visibleCustomers,
          selection,
          hasNextPage,
          hasPreviousPage,
          activeConnection,
          {
            preserveBaselinePageInfo,
          },
        );
        break;
      default:
        connection[key] = null;
        break;
    }
  }

  return connection;
}

function normalizeCustomerCatalogPageInfo(raw: unknown): CustomerCatalogPageInfoRecord {
  if (!isObject(raw)) {
    return {
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: null,
      endCursor: null,
    };
  }

  return {
    hasNextPage: raw['hasNextPage'] === true,
    hasPreviousPage: raw['hasPreviousPage'] === true,
    startCursor: typeof raw['startCursor'] === 'string' ? raw['startCursor'] : null,
    endCursor: typeof raw['endCursor'] === 'string' ? raw['endCursor'] : null,
  };
}

function collectHydratableCustomers(raw: unknown): CustomerRecord[] {
  if (!isObject(raw)) {
    return [];
  }

  const directCustomer = normalizeCustomer(raw['customer']);
  const customersConnection = raw['customers'];
  const connectionCustomers = isObject(customersConnection)
    ? [
        ...(Array.isArray(customersConnection['nodes']) ? customersConnection['nodes'] : []),
        ...(Array.isArray(customersConnection['edges'])
          ? customersConnection['edges']
              .filter((edge): edge is Record<string, unknown> => isObject(edge))
              .map((edge) => edge['node'])
          : []),
      ]
        .map((candidate) => normalizeCustomer(candidate))
        .filter((candidate): candidate is CustomerRecord => candidate !== null)
    : [];

  return [directCustomer, ...connectionCustomers].filter(
    (candidate): candidate is CustomerRecord => candidate !== null,
  );
}

function collectCustomerCatalogConnection(
  raw: unknown,
  responseKey = 'customers',
): CustomerCatalogConnectionRecord | null {
  if (!isObject(raw) || !isObject(raw[responseKey])) {
    return null;
  }

  const customersConnection = raw[responseKey];
  const connectionEdges = Array.isArray(customersConnection['edges'])
    ? customersConnection['edges'].filter((edge): edge is Record<string, unknown> => isObject(edge))
    : [];

  const orderedCustomerIds: string[] = [];
  const cursorByCustomerId: Record<string, string> = {};
  for (const edge of connectionEdges) {
    const customer = normalizeCustomer(edge['node']);
    const cursor = typeof edge['cursor'] === 'string' ? edge['cursor'] : null;
    if (!customer) {
      continue;
    }

    orderedCustomerIds.push(customer.id);
    if (cursor) {
      cursorByCustomerId[customer.id] = cursor;
    }
  }

  if (orderedCustomerIds.length === 0) {
    return null;
  }

  return {
    orderedCustomerIds,
    cursorByCustomerId,
    pageInfo: normalizeCustomerCatalogPageInfo(customersConnection['pageInfo']),
  };
}

function collectCustomerSearchConnections(
  document: string,
  variables: Record<string, unknown>,
  rawData: Record<string, unknown>,
): Record<string, CustomerCatalogConnectionRecord> {
  const connections: Record<string, CustomerCatalogConnectionRecord> = {};
  for (const field of getRootFields(document)) {
    if (field.name.value !== 'customers') {
      continue;
    }

    const args = getFieldArguments(field, variables);
    const key = buildCustomerSearchConnectionKey(args['query'], args['sortKey'], args['reverse']);
    if (!key) {
      continue;
    }

    const connection = collectCustomerCatalogConnection(rawData, getFieldResponseKey(field));
    if (!connection) {
      continue;
    }

    connections[key] = connection;
  }

  return connections;
}

type CustomerMutationUserError = {
  field: string[] | null;
  message: string;
  code?: string | null;
};

function serializeCustomerMutationUserErrors(
  field: FieldNode,
  userErrors: CustomerMutationUserError[],
): Array<Record<string, unknown>> {
  return userErrors.map((userError) => {
    const result: Record<string, unknown> = {};
    for (const selection of getSelectedChildFields(field)) {
      const key = getFieldResponseKey(selection);
      switch (selection.name.value) {
        case 'field':
          result[key] = userError.field;
          break;
        case 'message':
          result[key] = userError.message;
          break;
        case 'code':
          result[key] = userError.code ?? null;
          break;
        default:
          result[key] = null;
          break;
      }
    }
    return result;
  });
}

function serializeShopSelection(field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = 'gid://shopify/Shop/1';
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function readCustomerInput(raw: unknown): Record<string, unknown> {
  return isObject(raw) ? raw : {};
}

function readRequiredIdArgument(args: Record<string, unknown>, argumentName: string): string | null {
  const value = args[argumentName];
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function normalizeCustomerTags(raw: unknown, fallback: string[]): string[] {
  if (!Array.isArray(raw)) {
    return structuredClone(fallback);
  }

  return raw
    .filter((value): value is string => typeof value === 'string' && value.trim().length > 0)
    .sort((left, right) => left.localeCompare(right));
}

function buildCreatedCustomer(input: Record<string, unknown>): CustomerRecord {
  const id = makeSyntheticGid('Customer');
  const timestamp = makeSyntheticTimestamp();
  const email = typeof input['email'] === 'string' && input['email'].trim().length > 0 ? input['email'].trim() : null;
  const firstName =
    typeof input['firstName'] === 'string' && input['firstName'].trim().length > 0 ? input['firstName'].trim() : null;
  const lastName =
    typeof input['lastName'] === 'string' && input['lastName'].trim().length > 0 ? input['lastName'].trim() : null;
  const locale =
    typeof input['locale'] === 'string' && input['locale'].trim().length > 0 ? input['locale'].trim() : null;
  const note = typeof input['note'] === 'string' && input['note'].trim().length > 0 ? input['note'] : null;
  const phone = typeof input['phone'] === 'string' && input['phone'].trim().length > 0 ? input['phone'].trim() : null;
  const taxExempt = input['taxExempt'] === true;
  const tags = normalizeCustomerTags(input['tags'], []);

  return {
    id,
    firstName,
    lastName,
    displayName: buildCustomerDisplayName(firstName, lastName, email),
    email,
    legacyResourceId: id.split('/').at(-1) ?? null,
    locale,
    note,
    canDelete: true,
    verifiedEmail: email ? true : null,
    taxExempt,
    state: 'DISABLED',
    tags,
    numberOfOrders: 0,
    amountSpent: null,
    defaultEmailAddress: email ? { emailAddress: email } : null,
    defaultPhoneNumber: phone ? { phoneNumber: maskPhoneNumber(phone) } : null,
    defaultAddress: null,
    createdAt: timestamp,
    updatedAt: timestamp,
  };
}

function buildUpdatedCustomer(existing: CustomerRecord, input: Record<string, unknown>): CustomerRecord {
  const email = typeof input['email'] === 'string' ? input['email'].trim() || null : existing.email;
  const firstName = typeof input['firstName'] === 'string' ? input['firstName'].trim() || null : existing.firstName;
  const lastName = typeof input['lastName'] === 'string' ? input['lastName'].trim() || null : existing.lastName;
  const locale = typeof input['locale'] === 'string' ? input['locale'].trim() || null : existing.locale;
  const note = typeof input['note'] === 'string' ? input['note'] || null : existing.note;
  const phone =
    typeof input['phone'] === 'string'
      ? input['phone'].trim() || null
      : (existing.defaultPhoneNumber?.phoneNumber ?? null);

  return {
    ...existing,
    firstName,
    lastName,
    displayName: buildCustomerDisplayName(firstName, lastName, email),
    email,
    locale,
    note,
    verifiedEmail: email ? true : existing.verifiedEmail,
    taxExempt: typeof input['taxExempt'] === 'boolean' ? input['taxExempt'] : existing.taxExempt,
    tags: normalizeCustomerTags(input['tags'], existing.tags),
    defaultEmailAddress: email ? { emailAddress: email } : null,
    defaultPhoneNumber: phone ? { phoneNumber: maskPhoneNumber(phone) } : null,
    updatedAt: makeSyntheticTimestamp(),
  };
}

function validateCustomerCreateInput(input: Record<string, unknown>): CustomerMutationUserError[] {
  const hasEmail = typeof input['email'] === 'string' && input['email'].trim().length > 0;
  const hasPhone = typeof input['phone'] === 'string' && input['phone'].trim().length > 0;
  const hasFirstName = typeof input['firstName'] === 'string' && input['firstName'].trim().length > 0;
  const hasLastName = typeof input['lastName'] === 'string' && input['lastName'].trim().length > 0;
  if (hasEmail || hasPhone || hasFirstName || hasLastName) {
    return [];
  }

  return [{ field: null, message: 'A name, phone number, or email address must be present' }];
}

function serializeCustomerMutationPayload(
  field: FieldNode,
  payload: {
    customer?: CustomerRecord | null;
    deletedCustomerId?: string | null;
    shop?: boolean;
    userErrors: CustomerMutationUserError[];
  },
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'customer':
        result[key] = payload.customer ? serializeCustomerSelection(payload.customer, selection) : null;
        break;
      case 'deletedCustomerId':
        result[key] = payload.deletedCustomerId ?? null;
        break;
      case 'shop':
        result[key] = payload.shop ? serializeShopSelection(selection) : null;
        break;
      case 'userErrors':
        result[key] = serializeCustomerMutationUserErrors(selection, payload.userErrors);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

export function handleCustomerMutation(
  document: string,
  variables: Record<string, unknown> = {},
): { data: Record<string, unknown> } {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);

    if (field.name.value === 'customerCreate') {
      const input = readCustomerInput(args['input']);
      const userErrors = validateCustomerCreateInput(input);
      if (userErrors.length > 0) {
        data[key] = serializeCustomerMutationPayload(field, { customer: null, userErrors });
        continue;
      }

      const customer = store.stageCreateCustomer(buildCreatedCustomer(input));
      data[key] = serializeCustomerMutationPayload(field, { customer, userErrors: [] });
      continue;
    }

    if (field.name.value === 'customerUpdate') {
      const input = readCustomerInput(args['input']);
      const customerId = typeof input['id'] === 'string' ? input['id'] : null;
      const existingCustomer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      if (!existingCustomer) {
        data[key] = serializeCustomerMutationPayload(field, {
          customer: null,
          userErrors: [{ field: ['id'], message: 'Customer does not exist' }],
        });
        continue;
      }

      const customer = store.stageUpdateCustomer(buildUpdatedCustomer(existingCustomer, input));
      data[key] = serializeCustomerMutationPayload(field, { customer, userErrors: [] });
      continue;
    }

    if (field.name.value === 'customerDelete') {
      const input = readCustomerInput(args['input']);
      const customerId = typeof input['id'] === 'string' ? input['id'] : null;
      const existingCustomer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      if (!existingCustomer || !customerId) {
        data[key] = serializeCustomerMutationPayload(field, {
          deletedCustomerId: null,
          shop: true,
          userErrors: [{ field: ['id'], message: "Customer can't be found" }],
        });
        continue;
      }

      store.stageDeleteCustomer(customerId);
      data[key] = serializeCustomerMutationPayload(field, {
        deletedCustomerId: customerId,
        shop: true,
        userErrors: [],
      });
      continue;
    }

    if (field.name.value === 'customerSendAccountInviteEmail') {
      const customerId = readRequiredIdArgument(args, 'customerId');
      const customer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      data[key] = serializeCustomerMutationPayload(field, {
        customer,
        userErrors: [
          {
            field: ['customerId'],
            message:
              'customerSendAccountInviteEmail is intentionally suppressed by the local proxy because it sends email.',
          },
        ],
      });
      continue;
    }

    if (field.name.value === 'customerPaymentMethodSendUpdateEmail') {
      data[key] = serializeCustomerMutationPayload(field, {
        customer: null,
        userErrors: [
          {
            field: ['customerPaymentMethodId'],
            message:
              'customerPaymentMethodSendUpdateEmail is intentionally suppressed by the local proxy because it sends email.',
          },
        ],
      });
    }
  }

  return { data };
}

export function hydrateCustomersFromUpstreamResponse(
  document: string,
  variables: Record<string, unknown>,
  upstreamBody: unknown,
): void {
  if (!isObject(upstreamBody) || !isObject(upstreamBody['data'])) {
    return;
  }

  const customers = collectHydratableCustomers(upstreamBody['data']);
  if (customers.length > 0) {
    store.upsertBaseCustomers(customers);
  }

  const customerCatalogConnection = collectCustomerCatalogConnection(upstreamBody['data']);
  if (customerCatalogConnection) {
    store.setBaseCustomerCatalogConnection(customerCatalogConnection);
  }

  const customerSearchConnections = collectCustomerSearchConnections(document, variables, upstreamBody['data']);
  for (const [key, connection] of Object.entries(customerSearchConnections)) {
    store.setBaseCustomerSearchConnection(key, connection);
  }
}

export function handleCustomerQuery(
  document: string,
  variables: Record<string, unknown> = {},
): { data: Record<string, unknown>; extensions?: { search: CustomerSearchExtensionEntry[] } } {
  const rootFields = getRootFields(document);
  const data: Record<string, unknown> = {};
  const searchExtensions: CustomerSearchExtensionEntry[] = [];

  for (const field of rootFields) {
    const key = getFieldResponseKey(field);
    if (field.name.value === 'customer') {
      const args = getFieldArguments(field, variables);
      const customerId = typeof args['id'] === 'string' ? args['id'] : null;
      const customer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      data[key] = customer ? serializeCustomerSelection(customer, field) : null;
      continue;
    }

    if (field.name.value === 'customers') {
      data[key] = serializeCustomersConnection(field, variables);
      continue;
    }

    if (field.name.value === 'customersCount') {
      const args = getFieldArguments(field, variables);
      data[key] = serializeCustomersCount(args['query'], field.selectionSet?.selections ?? []);
      const searchExtension = buildCustomersCountSearchExtension(args['query'], [key]);
      if (searchExtension) {
        searchExtensions.push(searchExtension);
      }
    }
  }

  if (searchExtensions.length > 0) {
    return {
      data,
      extensions: {
        search: searchExtensions,
      },
    };
  }

  return { data };
}
