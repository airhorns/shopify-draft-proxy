import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { parseSearchQueryTerms, normalizeSearchQueryValue, type SearchQueryTerm } from '../search-query-parser.js';
import { compareShopifyResourceIds } from '../shopify/resource-ids.js';
import { makeProxySyntheticGid, makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { GiftCardRecord, GiftCardTransactionRecord, MoneyV2Record } from '../state/types.js';
import { paginateConnectionItems, serializeConnection } from './graphql-helpers.js';

type GiftCardUserErrorRecord = {
  field: string[] | null;
  message: string;
};

type GiftCardPayload = {
  giftCard: GiftCardRecord | null;
  userErrors: GiftCardUserErrorRecord[];
  giftCardCode?: string | null;
  giftCardTransaction?: GiftCardTransactionRecord | null;
};

function responseKey(field: FieldNode): string {
  return field.alias?.value ?? field.name.value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readInput(args: Record<string, unknown>): Record<string, unknown> {
  return isRecord(args['input']) ? args['input'] : {};
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function parseDecimalAmount(raw: unknown): number {
  const numeric = typeof raw === 'number' ? raw : Number.parseFloat(typeof raw === 'string' ? raw : '0');
  return Number.isFinite(numeric) ? numeric : 0;
}

function formatDecimalAmount(value: number): string {
  const fixed = value.toFixed(2);
  if (fixed.endsWith('00')) {
    return `${fixed.slice(0, -3)}.0`;
  }
  return fixed.endsWith('0') ? fixed.slice(0, -1) : fixed;
}

function normalizeMoney(raw: unknown, fallbackCurrencyCode = 'CAD'): MoneyV2Record {
  if (typeof raw === 'string' || typeof raw === 'number') {
    return {
      amount: formatDecimalAmount(parseDecimalAmount(raw)),
      currencyCode: fallbackCurrencyCode,
    };
  }

  const money = isRecord(raw) ? raw : {};
  const amount = formatDecimalAmount(parseDecimalAmount(money['amount']));
  const currencyCode = typeof money['currencyCode'] === 'string' ? money['currencyCode'] : fallbackCurrencyCode;
  return { amount, currencyCode };
}

function readGiftCardId(args: Record<string, unknown>): string | null {
  const input = readInput(args);
  return (
    readString(args['id']) ??
    readString(args['giftCardId']) ??
    readString(input['id']) ??
    readString(input['giftCardId'])
  );
}

function readMutationMoney(args: Record<string, unknown>, preferredKey: 'creditAmount' | 'debitAmount'): MoneyV2Record {
  const input = readInput(args);
  const nestedInput =
    preferredKey === 'creditAmount'
      ? isRecord(args['creditInput'])
        ? args['creditInput']
        : input
      : isRecord(args['debitInput'])
        ? args['debitInput']
        : input;
  const rawMoney = args[preferredKey] ?? args['amount'] ?? nestedInput[preferredKey] ?? nestedInput['amount'];
  return normalizeMoney(rawMoney);
}

function readMutationNote(
  args: Record<string, unknown>,
  preferredInputKey: 'creditInput' | 'debitInput',
): string | null {
  const input = readInput(args);
  const nestedInput = isRecord(args[preferredInputKey]) ? args[preferredInputKey] : input;
  return readString(args['note']) ?? readString(nestedInput['note']);
}

function giftCardTail(id: string): string {
  return id.split('/').at(-1)?.split('?')[0] ?? id;
}

function normalizeGiftCardCode(raw: unknown, fallbackId: string): string {
  const explicitCode = typeof raw === 'string' ? raw.replace(/\s+/gu, '').trim() : '';
  return explicitCode.length > 0 ? explicitCode : `PROXY${giftCardTail(fallbackId).padStart(8, '0')}`;
}

function lastCharactersFromCode(code: string): string {
  return code.slice(-4).toUpperCase().padStart(4, '0');
}

function maskedCode(lastCharacters: string): string {
  return `**** **** **** ${lastCharacters}`;
}

function serializeMoney(money: MoneyV2Record, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'MoneyV2';
        break;
      case 'amount':
        result[key] = money.amount;
        break;
      case 'currencyCode':
        result[key] = money.currencyCode;
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeGiftCardTransaction(
  transaction: GiftCardTransactionRecord,
  selections: readonly SelectionNode[],
  giftCard: GiftCardRecord | null = null,
  variables: Record<string, unknown> = {},
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'GiftCardTransaction') {
        continue;
      }
      Object.assign(
        result,
        serializeGiftCardTransaction(transaction, selection.selectionSet.selections, giftCard, variables),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'GiftCardTransaction';
        break;
      case 'id':
        result[key] = transaction.id;
        break;
      case 'kind':
        result[key] = transaction.kind;
        break;
      case 'note':
        result[key] = transaction.note;
        break;
      case 'processedAt':
        result[key] = transaction.processedAt;
        break;
      case 'amount':
        result[key] = serializeMoney(transaction.amount, selection.selectionSet?.selections ?? []);
        break;
      case 'giftCard':
        result[key] = giftCard
          ? serializeGiftCard(giftCard, selection.selectionSet?.selections ?? [], variables)
          : null;
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeGiftCardTransactionsConnection(
  giftCard: GiftCardRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const getCursor = (transaction: GiftCardTransactionRecord): string => transaction.id;
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(
    giftCard.transactions,
    field,
    variables,
    getCursor,
  );

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: getCursor,
    serializeNode: (transaction, selection) =>
      serializeGiftCardTransaction(transaction, selection.selectionSet?.selections ?? [], giftCard, variables),
  });
}

function serializeGiftCard(
  giftCard: GiftCardRecord,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== 'GiftCard') {
        continue;
      }
      Object.assign(result, serializeGiftCard(giftCard, selection.selectionSet.selections, variables));
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'GiftCard';
        break;
      case 'id':
        result[key] = giftCard.id;
        break;
      case 'legacyResourceId':
        result[key] = giftCard.legacyResourceId;
        break;
      case 'lastCharacters':
        result[key] = giftCard.lastCharacters;
        break;
      case 'maskedCode':
        result[key] = giftCard.maskedCode;
        break;
      case 'enabled':
        result[key] = giftCard.enabled;
        break;
      case 'disabledAt':
      case 'deactivatedAt':
        result[key] = giftCard.deactivatedAt;
        break;
      case 'expiresOn':
        result[key] = giftCard.expiresOn;
        break;
      case 'note':
        result[key] = giftCard.note;
        break;
      case 'templateSuffix':
        result[key] = giftCard.templateSuffix;
        break;
      case 'createdAt':
        result[key] = giftCard.createdAt;
        break;
      case 'updatedAt':
        result[key] = giftCard.updatedAt;
        break;
      case 'initialValue':
        result[key] = serializeMoney(giftCard.initialValue, selection.selectionSet?.selections ?? []);
        break;
      case 'balance':
        result[key] = serializeMoney(giftCard.balance, selection.selectionSet?.selections ?? []);
        break;
      case 'transactions':
        result[key] = serializeGiftCardTransactionsConnection(giftCard, selection, variables);
        break;
      case 'customer':
        result[key] = giftCard.customerId ? { id: giftCard.customerId } : null;
        break;
      case 'recipient':
        result[key] = giftCard.recipientId ? { id: giftCard.recipientId } : null;
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function giftCardMatchesTerm(giftCard: GiftCardRecord, term: SearchQueryTerm): boolean {
  const normalizedValue = normalizeSearchQueryValue(term.value);
  let matches = true;

  switch (term.field) {
    case 'enabled':
    case 'active':
      matches = normalizedValue === String(giftCard.enabled);
      break;
    case 'id':
      matches =
        normalizedValue === normalizeSearchQueryValue(giftCard.id) || normalizedValue === giftCardTail(giftCard.id);
      break;
    case 'last_characters':
    case 'lastCharacters':
      matches = normalizedValue === normalizeSearchQueryValue(giftCard.lastCharacters);
      break;
    default:
      matches = true;
      break;
  }

  return term.negated ? !matches : matches;
}

function filterGiftCardsByQuery(giftCards: GiftCardRecord[], rawQuery: unknown): GiftCardRecord[] {
  if (typeof rawQuery !== 'string' || rawQuery.trim().length === 0) {
    return giftCards;
  }

  const terms = parseSearchQueryTerms(rawQuery.trim(), { ignoredKeywords: ['AND'] }).filter(
    (term) =>
      term.field === 'enabled' ||
      term.field === 'active' ||
      term.field === 'id' ||
      term.field === 'last_characters' ||
      term.field === 'lastCharacters',
  );
  return terms.length === 0
    ? giftCards
    : giftCards.filter((giftCard) => terms.every((term) => giftCardMatchesTerm(giftCard, term)));
}

function compareGiftCards(left: GiftCardRecord, right: GiftCardRecord, sortKey: unknown): number {
  switch (sortKey) {
    case 'CREATED_AT':
      return Date.parse(left.createdAt) - Date.parse(right.createdAt) || compareShopifyResourceIds(left.id, right.id);
    case 'UPDATED_AT':
      return Date.parse(left.updatedAt) - Date.parse(right.updatedAt) || compareShopifyResourceIds(left.id, right.id);
    case 'ID':
    default:
      return compareShopifyResourceIds(left.id, right.id);
  }
}

function listGiftCardsForConnection(field: FieldNode, variables: Record<string, unknown>): GiftCardRecord[] {
  const args = getFieldArguments(field, variables);
  const reverse = args['reverse'] === true;
  const sortedGiftCards = filterGiftCardsByQuery(store.listEffectiveGiftCards(), args['query']).sort((left, right) =>
    compareGiftCards(left, right, args['sortKey']),
  );

  return reverse ? sortedGiftCards.reverse() : sortedGiftCards;
}

function serializeGiftCardsConnection(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const giftCards = listGiftCardsForConnection(field, variables);
  const getCursor = (giftCard: GiftCardRecord): string => giftCard.id;
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(giftCards, field, variables, getCursor);

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: getCursor,
    serializeNode: (giftCard, selection) =>
      serializeGiftCard(giftCard, selection.selectionSet?.selections ?? [], variables),
  });
}

function serializeGiftCardsCount(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const count = filterGiftCardsByQuery(store.listEffectiveGiftCards(), args['query']).length;
  const limit =
    typeof args['limit'] === 'number' && Number.isInteger(args['limit']) && args['limit'] >= 0 ? args['limit'] : null;
  const visibleCount = limit === null ? count : Math.min(count, limit);
  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'Count';
        break;
      case 'count':
        result[key] = visibleCount;
        break;
      case 'precision':
        result[key] = limit !== null && count > limit ? 'AT_LEAST' : 'EXACT';
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

function serializeGiftCardConfiguration(selections: readonly SelectionNode[]): Record<string, unknown> {
  const configuration = store.getEffectiveGiftCardConfiguration();
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = 'GiftCardConfiguration';
        break;
      case 'issueLimit':
        result[key] = serializeMoney(configuration.issueLimit, selection.selectionSet?.selections ?? []);
        break;
      case 'purchaseLimit':
        result[key] = serializeMoney(configuration.purchaseLimit, selection.selectionSet?.selections ?? []);
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function serializeGiftCardUserErrors(
  userErrors: GiftCardUserErrorRecord[],
  selections: readonly SelectionNode[],
): Array<Record<string, unknown>> {
  return userErrors.map((userError) => {
    const result: Record<string, unknown> = {};
    for (const selection of selections) {
      if (selection.kind !== Kind.FIELD) {
        continue;
      }

      const key = responseKey(selection);
      switch (selection.name.value) {
        case '__typename':
          result[key] = 'UserError';
          break;
        case 'field':
          result[key] = userError.field ? structuredClone(userError.field) : null;
          break;
        case 'message':
          result[key] = userError.message;
          break;
        default:
          result[key] = null;
      }
    }
    return result;
  });
}

function serializeGiftCardPayload(
  payload: GiftCardPayload,
  payloadTypename: string,
  selections: readonly SelectionNode[],
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      if (selection.typeCondition?.name.value && selection.typeCondition.name.value !== payloadTypename) {
        continue;
      }
      Object.assign(
        result,
        serializeGiftCardPayload(payload, payloadTypename, selection.selectionSet.selections, variables),
      );
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = responseKey(selection);
    switch (selection.name.value) {
      case '__typename':
        result[key] = payloadTypename;
        break;
      case 'giftCard':
        result[key] = payload.giftCard
          ? serializeGiftCard(payload.giftCard, selection.selectionSet?.selections ?? [], variables)
          : null;
        break;
      case 'giftCardCode':
        result[key] = payload.giftCardCode ?? null;
        break;
      case 'giftCardTransaction':
      case 'transaction':
      case 'giftCardCreditTransaction':
      case 'giftCardDebitTransaction':
        result[key] = payload.giftCardTransaction
          ? serializeGiftCardTransaction(
              payload.giftCardTransaction,
              selection.selectionSet?.selections ?? [],
              payload.giftCard,
              variables,
            )
          : null;
        break;
      case 'userErrors':
        result[key] = serializeGiftCardUserErrors(payload.userErrors, selection.selectionSet?.selections ?? []);
        break;
      default:
        result[key] = null;
    }
  }
  return result;
}

function notFoundUserError(): GiftCardUserErrorRecord {
  return { field: ['id'], message: 'Gift card does not exist' };
}

function stageGiftCardCreate(args: Record<string, unknown>): GiftCardPayload {
  const input = readInput(args);
  const id = makeProxySyntheticGid('GiftCard');
  const code = normalizeGiftCardCode(input['code'], id);
  const initialValue = normalizeMoney(input['initialValue']);
  if (parseDecimalAmount(initialValue.amount) <= 0) {
    return {
      giftCard: null,
      giftCardCode: null,
      userErrors: [{ field: ['input', 'initialValue'], message: 'Initial value must be greater than zero' }],
    };
  }

  const now = makeSyntheticTimestamp();
  const lastCharacters = lastCharactersFromCode(code);
  const giftCard: GiftCardRecord = {
    id,
    legacyResourceId: giftCardTail(id),
    lastCharacters,
    maskedCode: maskedCode(lastCharacters),
    enabled: true,
    deactivatedAt: null,
    expiresOn: readString(input['expiresOn']),
    note: readString(input['note']),
    templateSuffix: readString(input['templateSuffix']),
    createdAt: now,
    updatedAt: now,
    initialValue,
    balance: { ...initialValue },
    customerId: readString(input['customerId']),
    recipientId: readString(input['recipientId']),
    transactions: [],
  };

  return {
    giftCard: store.stageCreateGiftCard(giftCard),
    giftCardCode: code,
    userErrors: [],
  };
}

function stageGiftCardUpdate(args: Record<string, unknown>): GiftCardPayload {
  const input = readInput(args);
  const id = readString(input['id']) ?? readString(args['id']);
  const existing = id ? store.getEffectiveGiftCardById(id) : null;
  if (!id || !existing) {
    return { giftCard: null, userErrors: [notFoundUserError()] };
  }

  const giftCard: GiftCardRecord = {
    ...existing,
    note: Object.prototype.hasOwnProperty.call(input, 'note') ? readString(input['note']) : existing.note,
    templateSuffix: Object.prototype.hasOwnProperty.call(input, 'templateSuffix')
      ? readString(input['templateSuffix'])
      : existing.templateSuffix,
    expiresOn: Object.prototype.hasOwnProperty.call(input, 'expiresOn')
      ? readString(input['expiresOn'])
      : existing.expiresOn,
    customerId: Object.prototype.hasOwnProperty.call(input, 'customerId')
      ? readString(input['customerId'])
      : existing.customerId,
    recipientId: Object.prototype.hasOwnProperty.call(input, 'recipientId')
      ? readString(input['recipientId'])
      : existing.recipientId,
    updatedAt: makeSyntheticTimestamp(),
  };

  return { giftCard: store.stageUpdateGiftCard(giftCard), userErrors: [] };
}

function stageGiftCardTransaction(
  args: Record<string, unknown>,
  kind: 'CREDIT' | 'DEBIT',
  preferredAmountKey: 'creditAmount' | 'debitAmount',
): GiftCardPayload {
  const id = readGiftCardId(args);
  const existing = id ? store.getEffectiveGiftCardById(id) : null;
  if (!id || !existing) {
    return { giftCard: null, giftCardTransaction: null, userErrors: [notFoundUserError()] };
  }

  const rawMoney = readMutationMoney(args, preferredAmountKey);
  const magnitude = parseDecimalAmount(rawMoney.amount);
  if (magnitude <= 0) {
    return {
      giftCard: null,
      giftCardTransaction: null,
      userErrors: [{ field: [preferredAmountKey], message: 'Amount must be greater than zero' }],
    };
  }

  const currentBalance = parseDecimalAmount(existing.balance.amount);
  if (kind === 'DEBIT' && magnitude > currentBalance) {
    return {
      giftCard: null,
      giftCardTransaction: null,
      userErrors: [{ field: [preferredAmountKey], message: 'Insufficient balance' }],
    };
  }

  const signedAmount = kind === 'CREDIT' ? magnitude : -magnitude;
  const currencyCode = rawMoney.currencyCode ?? existing.balance.currencyCode;
  const transaction: GiftCardTransactionRecord = {
    id: makeSyntheticGid('GiftCardTransaction'),
    kind,
    amount: {
      amount: formatDecimalAmount(signedAmount),
      currencyCode,
    },
    processedAt: makeSyntheticTimestamp(),
    note: readMutationNote(args, kind === 'CREDIT' ? 'creditInput' : 'debitInput'),
  };
  const giftCard: GiftCardRecord = {
    ...existing,
    balance: {
      amount: formatDecimalAmount(currentBalance + signedAmount),
      currencyCode,
    },
    updatedAt: makeSyntheticTimestamp(),
    transactions: [...existing.transactions, transaction],
  };

  return {
    giftCard: store.stageUpdateGiftCard(giftCard),
    giftCardTransaction: transaction,
    userErrors: [],
  };
}

function stageGiftCardDeactivate(args: Record<string, unknown>): GiftCardPayload {
  const id = readGiftCardId(args);
  const existing = id ? store.getEffectiveGiftCardById(id) : null;
  if (!id || !existing) {
    return { giftCard: null, userErrors: [notFoundUserError()] };
  }

  const now = makeSyntheticTimestamp();
  const giftCard: GiftCardRecord = {
    ...existing,
    enabled: false,
    deactivatedAt: existing.deactivatedAt ?? now,
    updatedAt: now,
  };
  return { giftCard: store.stageUpdateGiftCard(giftCard), userErrors: [] };
}

function stageGiftCardNotification(args: Record<string, unknown>): GiftCardPayload {
  const id = readGiftCardId(args);
  const existing = id ? store.getEffectiveGiftCardById(id) : null;
  if (!id || !existing) {
    return { giftCard: null, userErrors: [notFoundUserError()] };
  }

  return { giftCard: existing, userErrors: [] };
}

export function handleGiftCardQuery(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  for (const field of getRootFields(document)) {
    const key = responseKey(field);
    const args = getFieldArguments(field, variables);
    switch (field.name.value) {
      case 'giftCard': {
        const id = readString(args['id']);
        const giftCard = id ? store.getEffectiveGiftCardById(id) : null;
        data[key] = giftCard ? serializeGiftCard(giftCard, field.selectionSet?.selections ?? [], variables) : null;
        break;
      }
      case 'giftCards':
        data[key] = serializeGiftCardsConnection(field, variables);
        break;
      case 'giftCardsCount':
        data[key] = serializeGiftCardsCount(field, variables);
        break;
      case 'giftCardConfiguration':
        data[key] = serializeGiftCardConfiguration(field.selectionSet?.selections ?? []);
        break;
      default:
        data[key] = null;
    }
  }

  return { data };
}

export function handleGiftCardMutation(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  for (const field of getRootFields(document)) {
    const key = responseKey(field);
    const args = getFieldArguments(field, variables);
    switch (field.name.value) {
      case 'giftCardCreate':
        data[key] = serializeGiftCardPayload(
          stageGiftCardCreate(args),
          'GiftCardCreatePayload',
          field.selectionSet?.selections ?? [],
          variables,
        );
        break;
      case 'giftCardUpdate':
        data[key] = serializeGiftCardPayload(
          stageGiftCardUpdate(args),
          'GiftCardUpdatePayload',
          field.selectionSet?.selections ?? [],
          variables,
        );
        break;
      case 'giftCardCredit':
        data[key] = serializeGiftCardPayload(
          stageGiftCardTransaction(args, 'CREDIT', 'creditAmount'),
          'GiftCardCreditPayload',
          field.selectionSet?.selections ?? [],
          variables,
        );
        break;
      case 'giftCardDebit':
        data[key] = serializeGiftCardPayload(
          stageGiftCardTransaction(args, 'DEBIT', 'debitAmount'),
          'GiftCardDebitPayload',
          field.selectionSet?.selections ?? [],
          variables,
        );
        break;
      case 'giftCardDeactivate':
        data[key] = serializeGiftCardPayload(
          stageGiftCardDeactivate(args),
          'GiftCardDeactivatePayload',
          field.selectionSet?.selections ?? [],
          variables,
        );
        break;
      case 'giftCardSendNotificationToCustomer':
        data[key] = serializeGiftCardPayload(
          stageGiftCardNotification(args),
          'GiftCardSendNotificationToCustomerPayload',
          field.selectionSet?.selections ?? [],
          variables,
        );
        break;
      case 'giftCardSendNotificationToRecipient':
        data[key] = serializeGiftCardPayload(
          stageGiftCardNotification(args),
          'GiftCardSendNotificationToRecipientPayload',
          field.selectionSet?.selections ?? [],
          variables,
        );
        break;
      default:
        data[key] = null;
    }
  }

  return { data };
}
