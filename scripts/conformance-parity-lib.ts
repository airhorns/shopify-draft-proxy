import { readFileSync } from 'node:fs';
import path from 'node:path';
import { isDeepStrictEqual } from 'node:util';

import { parseOperation, type ParsedOperation } from '../src/graphql/parse-operation.js';
import {
  graphqlVariablesSchema,
  jsonValueSchema,
  parseJsonFileWithSchema,
  type BlockerDetails,
  type ComparisonContract,
  type ComparisonTarget,
  type ExpectedDifference,
  type Matcher,
  type ParitySpec,
  type ProxyRequestSpec,
} from '../src/json-schemas.js';
export type {
  BlockerDetails,
  ComparisonContract,
  ComparisonTarget,
  ExpectedDifference,
  Matcher,
  ParitySpec,
  ProxyRequestSpec,
} from '../src/json-schemas.js';
import { getOperationCapability, type OperationCapability } from '../src/proxy/capabilities.js';
import { handleOrderMutation, handleOrderQuery } from '../src/proxy/orders.js';
import {
  handleProductMutation,
  handleProductQuery,
  hydrateProductsFromUpstreamResponse,
} from '../src/proxy/products.js';
import { makeSyntheticGid, makeSyntheticTimestamp, resetSyntheticIdentity } from '../src/state/synthetic-identity.js';
import { store } from '../src/state/store.js';
import type {
  CollectionRecord,
  DraftOrderLineItemRecord,
  DraftOrderRecord,
  DraftOrderShippingLineRecord,
  InventoryLevelRecord,
  MutationLogInterpretedMetadata,
  OrderLineItemRecord,
  OrderRecord,
  OrderShippingLineRecord,
  ProductCollectionRecord,
  ProductMetafieldRecord,
  ProductMediaRecord,
  ProductOptionRecord,
  ProductRecord,
  ProductVariantRecord,
} from '../src/state/types.js';

function interpretMutationLogEntry(
  parsed: ParsedOperation,
  capability: OperationCapability,
): MutationLogInterpretedMetadata {
  return {
    operationType: parsed.type,
    operationName: parsed.name,
    rootFields: parsed.rootFields,
    primaryRootField: parsed.rootFields[0] ?? null,
    capability: {
      operationName: capability.operationName,
      domain: capability.domain,
      execution: capability.execution,
    },
  };
}

export type ParityScenarioState =
  | 'ready-for-comparison'
  | 'invalid-missing-comparison-contract'
  | 'blocked-with-proxy-request'
  | 'not-yet-implemented';

export interface Scenario {
  id: string;
  status: string;
  operationNames?: string[];
  assertionKinds?: string[];
  captureFiles?: string[];
  paritySpecPath?: string;
}

interface Difference {
  path: string;
  message: string;
  expected: unknown;
  actual: unknown;
}

interface CompiledRule extends ExpectedDifference {
  index: number;
  segments: PathSegment[];
}

type PathSegment = string | number | '*';

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasProxyRequest(paritySpec: ParitySpec | null | undefined): boolean {
  return !!paritySpec?.proxyRequest?.documentPath;
}

function hasComparisonContract(paritySpec: ParitySpec | null | undefined): boolean {
  if (validateComparisonContract(paritySpec?.comparison).length > 0) {
    return false;
  }
  const targets = paritySpec?.comparison?.targets;
  return Array.isArray(targets) && targets.length > 0;
}

function isKnownMatcher(matcher: string): matcher is Matcher {
  return (
    matcher === 'any-string' ||
    matcher === 'non-empty-string' ||
    matcher === 'any-number' ||
    matcher === 'iso-timestamp' ||
    /^shopify-gid:([A-Za-z][A-Za-z0-9]*)$/.test(matcher)
  );
}

export function validateComparisonContract(comparison: unknown): string[] {
  const errors: string[] = [];
  const candidate = isPlainObject(comparison) ? comparison : {};

  if (candidate['mode'] !== 'strict-json') {
    errors.push('Comparison contract mode must be `strict-json`.');
  }

  if ('allowedDifferences' in candidate) {
    errors.push('Comparison contract must use `expectedDifferences`; `allowedDifferences` is no longer supported.');
  }

  if (!Array.isArray(candidate['expectedDifferences'])) {
    errors.push('Comparison contract must declare an `expectedDifferences` array.');
    return errors;
  }

  for (const [index, rawRule] of candidate['expectedDifferences'].entries()) {
    const rule = isPlainObject(rawRule) ? rawRule : {};
    const label = `expectedDifferences[${index}]`;
    if (typeof rule['path'] !== 'string' || rule['path'].length === 0) {
      errors.push(`${label} must declare a non-empty JSON path.`);
    }

    if (typeof rule['reason'] !== 'string' || rule['reason'].length === 0) {
      errors.push(`${label} must document why the expected difference is accepted.`);
    }

    const hasMatcher = typeof rule['matcher'] === 'string';
    const isIgnored = rule['ignore'] === true;
    if (hasMatcher === isIgnored) {
      errors.push(`${label} must declare exactly one of \`matcher\` or \`ignore: true\`.`);
    }

    if (hasMatcher && !isKnownMatcher(rule['matcher'] as string)) {
      errors.push(`${label} declares unknown matcher \`${String(rule['matcher'])}\`.`);
    }

    if ('regrettable' in rule && rule['regrettable'] !== true) {
      errors.push(`${label} \`regrettable\`, when declared, must be true.`);
    }

    if (isIgnored && rule['regrettable'] !== true) {
      errors.push(`${label} with \`ignore: true\` must set \`regrettable: true\` for the parity gap.`);
    }
  }

  const rawTargets = candidate['targets'];
  if (rawTargets !== undefined) {
    if (!Array.isArray(rawTargets) || rawTargets.length === 0) {
      errors.push('Comparison contract `targets`, when declared, must be a non-empty array.');
    } else {
      for (const [index, rawTarget] of rawTargets.entries()) {
        const target = isPlainObject(rawTarget) ? rawTarget : {};
        const label = `targets[${index}]`;
        if (typeof target['name'] !== 'string' || target['name'].length === 0) {
          errors.push(`${label} must declare a non-empty name.`);
        }
        if (typeof target['capturePath'] !== 'string' || target['capturePath'].length === 0) {
          errors.push(`${label} must declare a non-empty capturePath.`);
        }
        if (typeof target['proxyPath'] !== 'string' || target['proxyPath'].length === 0) {
          errors.push(`${label} must declare a non-empty proxyPath.`);
        }
      }
    }
  }

  return errors;
}

export function classifyParityScenarioState(
  scenario: Pick<Scenario, 'status'>,
  paritySpec: ParitySpec | null | undefined,
): ParityScenarioState {
  if (paritySpec?.blocker && hasProxyRequest(paritySpec)) {
    return 'blocked-with-proxy-request';
  }

  if (scenario.status === 'captured') {
    return hasProxyRequest(paritySpec) && hasComparisonContract(paritySpec)
      ? 'ready-for-comparison'
      : 'invalid-missing-comparison-contract';
  }

  return 'not-yet-implemented';
}

export const parityStatusNote =
  'readyForComparison means a captured scenario has a proxy request and an explicit strict-json comparison contract. invalid scenarios are captured recordings that cannot run high-assurance comparison yet. notYetImplemented scenarios are intentionally planned and never partially executable.';

export function summarizeParityResults(results: Array<{ state: ParityScenarioState }>): {
  readyForComparison: number;
  pending: number;
  statusCounts: Record<'readyForComparison' | 'invalidMissingComparisonContract' | 'notYetImplemented', number>;
  statusNote: string;
} {
  const readyForComparison = results.filter((result) => result.state === 'ready-for-comparison').length;
  const invalidMissingComparisonContract = results.filter(
    (result) => result.state === 'invalid-missing-comparison-contract',
  ).length;
  const notYetImplemented = results.filter((result) => result.state === 'not-yet-implemented').length;

  return {
    readyForComparison,
    pending: results.length - readyForComparison,
    statusCounts: {
      readyForComparison,
      invalidMissingComparisonContract,
      notYetImplemented,
    },
    statusNote: parityStatusNote,
  };
}

function appendPath(currentPath: string, segment: string | number): string {
  if (typeof segment === 'number') {
    return `${currentPath}[${segment}]`;
  }

  if (/^[A-Za-z_$][\w$]*$/.test(segment)) {
    return `${currentPath}.${segment}`;
  }

  return `${currentPath}[${JSON.stringify(segment)}]`;
}

function parsePath(pathValue: string): PathSegment[] {
  if (!pathValue.startsWith('$')) {
    throw new Error(`Invalid comparison path: ${pathValue}`);
  }

  const segments: PathSegment[] = [];
  let index = 1;
  while (index < pathValue.length) {
    if (pathValue[index] === '.') {
      index += 1;
      const match = /^[A-Za-z_$][\w$]*/.exec(pathValue.slice(index));
      if (!match?.[0]) {
        throw new Error(`Invalid comparison path segment in: ${pathValue}`);
      }
      segments.push(match[0]);
      index += match[0].length;
      continue;
    }

    if (pathValue[index] === '[') {
      const closeIndex = pathValue.indexOf(']', index);
      if (closeIndex === -1) {
        throw new Error(`Invalid comparison path segment in: ${pathValue}`);
      }
      const raw = pathValue.slice(index + 1, closeIndex);
      if (raw === '*') {
        segments.push('*');
      } else if (/^\d+$/.test(raw)) {
        segments.push(Number.parseInt(raw, 10));
      } else {
        segments.push(JSON.parse(raw) as string);
      }
      index = closeIndex + 1;
      continue;
    }

    throw new Error(`Invalid comparison path segment in: ${pathValue}`);
  }

  return segments;
}

function pathMatches(ruleSegments: PathSegment[], pathSegments: PathSegment[]): boolean {
  if (ruleSegments.length !== pathSegments.length) {
    return false;
  }

  return ruleSegments.every((segment, index) => segment === '*' || segment === pathSegments[index]);
}

function makeRule(rawRule: ExpectedDifference, index: number): CompiledRule {
  return {
    ...rawRule,
    index,
    segments: parsePath(rawRule.path),
  };
}

function findRule(rules: CompiledRule[], pathSegments: PathSegment[]): CompiledRule | null {
  return rules.find((rule) => pathMatches(rule.segments, pathSegments)) ?? null;
}

function isIsoTimestamp(value: unknown): boolean {
  if (typeof value !== 'string') {
    return false;
  }

  const parsed = Date.parse(value);
  return Number.isFinite(parsed);
}

function isShopifyGid(value: unknown, resourceType: string): boolean {
  return (
    typeof value === 'string' &&
    value.startsWith(`gid://shopify/${resourceType}/`) &&
    value.length > `gid://shopify/${resourceType}/`.length
  );
}

function matcherAccepts(matcher: Matcher, expected: unknown, actual: unknown): boolean {
  if (matcher === 'any-string') {
    return typeof expected === 'string' && typeof actual === 'string';
  }

  if (matcher === 'non-empty-string') {
    return typeof expected === 'string' && expected.length > 0 && typeof actual === 'string' && actual.length > 0;
  }

  if (matcher === 'any-number') {
    return typeof expected === 'number' && typeof actual === 'number';
  }

  if (matcher === 'iso-timestamp') {
    return isIsoTimestamp(expected) && isIsoTimestamp(actual);
  }

  const gidMatch = /^shopify-gid:([A-Za-z][A-Za-z0-9]*)$/.exec(matcher);
  if (gidMatch?.[1]) {
    return isShopifyGid(expected, gidMatch[1]) && isShopifyGid(actual, gidMatch[1]);
  }

  throw new Error(`Unknown comparison matcher: ${matcher}`);
}

function diffValues(
  expected: unknown,
  actual: unknown,
  currentPath: string,
  pathSegments: PathSegment[],
  rules: CompiledRule[],
  differences: Difference[],
  observedRuleIndexes: Set<number>,
  applicableRuleIndexes: Set<number>,
): void {
  const rule = findRule(rules, pathSegments);
  if (rule) {
    applicableRuleIndexes.add(rule.index);
  }
  if (rule && !isDeepStrictEqual(expected, actual)) {
    observedRuleIndexes.add(rule.index);
  }

  if (rule?.ignore === true) {
    return;
  }

  if (Object.is(expected, actual)) {
    return;
  }

  if (rule?.matcher && matcherAccepts(rule.matcher, expected, actual)) {
    return;
  }

  if (Array.isArray(expected) || Array.isArray(actual)) {
    if (!Array.isArray(expected) || !Array.isArray(actual)) {
      differences.push({ path: currentPath, message: 'Expected both values to be arrays.', expected, actual });
      return;
    }

    if (expected.length !== actual.length) {
      differences.push({
        path: currentPath,
        message: `Array length differs: expected ${expected.length}, received ${actual.length}.`,
        expected,
        actual,
      });
      return;
    }

    for (let index = 0; index < expected.length; index += 1) {
      diffValues(
        expected[index],
        actual[index],
        appendPath(currentPath, index),
        [...pathSegments, index],
        rules,
        differences,
        observedRuleIndexes,
        applicableRuleIndexes,
      );
    }
    return;
  }

  if (isPlainObject(expected) || isPlainObject(actual)) {
    if (!isPlainObject(expected) || !isPlainObject(actual)) {
      differences.push({ path: currentPath, message: 'Expected both values to be objects.', expected, actual });
      return;
    }

    const keys = new Set([...Object.keys(expected), ...Object.keys(actual)]);
    for (const key of [...keys].sort()) {
      const childPath = appendPath(currentPath, key);
      const childSegments = [...pathSegments, key];
      const childRule = findRule(rules, childSegments);

      if (childRule) {
        applicableRuleIndexes.add(childRule.index);
      }

      if (childRule?.ignore === true) {
        if (!isDeepStrictEqual(expected[key], actual[key])) {
          observedRuleIndexes.add(childRule.index);
        }
        continue;
      }

      if (!Object.prototype.hasOwnProperty.call(expected, key)) {
        if (childRule) {
          observedRuleIndexes.add(childRule.index);
        }
        differences.push({
          path: childPath,
          message: 'Unexpected field in actual payload.',
          expected: undefined,
          actual: actual[key],
        });
        continue;
      }

      if (!Object.prototype.hasOwnProperty.call(actual, key)) {
        if (childRule) {
          observedRuleIndexes.add(childRule.index);
        }
        differences.push({
          path: childPath,
          message: 'Missing field in actual payload.',
          expected: expected[key],
          actual: undefined,
        });
        continue;
      }

      diffValues(
        expected[key],
        actual[key],
        childPath,
        childSegments,
        rules,
        differences,
        observedRuleIndexes,
        applicableRuleIndexes,
      );
    }
    return;
  }

  differences.push({ path: currentPath, message: 'Value differs.', expected, actual });
}

export function compareJsonPayloads(
  expected: unknown,
  actual: unknown,
  comparison: Pick<ComparisonContract, 'expectedDifferences'> = {},
): { ok: boolean; differences: Difference[] } {
  const expectedDifferences = Array.isArray(comparison.expectedDifferences) ? comparison.expectedDifferences : [];
  const rules = expectedDifferences.map(makeRule);
  const differences: Difference[] = [];
  const observedRuleIndexes = new Set<number>();
  const applicableRuleIndexes = new Set<number>();

  diffValues(expected, actual, '$', [], rules, differences, observedRuleIndexes, applicableRuleIndexes);

  for (const rule of rules) {
    if (applicableRuleIndexes.has(rule.index) && !observedRuleIndexes.has(rule.index)) {
      differences.push({
        path: rule.path,
        message: 'Expected difference was not observed.',
        expected: undefined,
        actual: undefined,
      });
    }
  }

  return {
    ok: differences.length === 0,
    differences,
  };
}

function readJsonFile(repoRoot: string, relativePath: string): unknown {
  return parseJsonFileWithSchema(path.join(repoRoot, relativePath), jsonValueSchema);
}

function readTextFile(repoRoot: string, relativePath: string): string {
  return readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

export function readJsonPath(value: unknown, pathValue: string): unknown {
  let current = value;
  for (const segment of parsePath(pathValue)) {
    if (segment === '*') {
      throw new Error(`Wildcard is not supported when reading a single JSON path: ${pathValue}`);
    }
    if (current === null || typeof current !== 'object') {
      return undefined;
    }
    current = (current as Record<string | number, unknown>)[segment];
  }
  return current;
}

function materializeValue(rawValue: unknown, primaryProxyResponse: unknown): unknown {
  if (Array.isArray(rawValue)) {
    return rawValue.map((item) => materializeValue(item, primaryProxyResponse));
  }

  if (!isPlainObject(rawValue)) {
    return rawValue;
  }

  if (typeof rawValue['fromPrimaryProxyPath'] === 'string') {
    return readJsonPath(primaryProxyResponse, rawValue['fromPrimaryProxyPath']);
  }

  return Object.fromEntries(
    Object.entries(rawValue).map(([key, value]) => [key, materializeValue(value, primaryProxyResponse)]),
  );
}

function materializeVariables(rawVariables: unknown, primaryProxyResponse: unknown): Record<string, unknown> {
  const materialized = materializeValue(rawVariables ?? {}, primaryProxyResponse);
  return isPlainObject(materialized) ? materialized : {};
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function executeGraphQLAgainstLocalProxy(
  document: string,
  variables: Record<string, unknown>,
  upstreamPayload?: unknown,
): Promise<{ status: number; body: Record<string, unknown> }> {
  const parsed = parseOperation(document);
  const capability = getOperationCapability(parsed);

  if (capability.execution === 'stage-locally' && capability.domain === 'products') {
    store.appendLog({
      id: makeSyntheticGid('MutationLogEntry'),
      receivedAt: makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: '/admin/api/2025-01/graphql.json',
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleProductMutation(document, variables, 'snapshot'),
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'orders') {
    const body = handleOrderMutation(document, variables, 'snapshot');
    if (!body) {
      throw new Error(`Order-domain parity request was not handled locally: ${capability.operationName}`);
    }

    store.appendLog({
      id: makeSyntheticGid('MutationLogEntry'),
      receivedAt: makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: '/admin/api/2025-01/graphql.json',
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body,
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'products') {
    if (upstreamPayload !== undefined) {
      hydrateProductsFromUpstreamResponse(document, variables, upstreamPayload);
      if (!hasStagedState()) {
        return {
          status: 200,
          body: isPlainObject(upstreamPayload) ? upstreamPayload : {},
        };
      }
    }

    return {
      status: 200,
      body: handleProductQuery(document, variables, upstreamPayload === undefined ? 'snapshot' : 'live-hybrid'),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'orders') {
    const upstreamPayloadIsResponseEnvelope =
      isPlainObject(upstreamPayload) && ('data' in upstreamPayload || 'errors' in upstreamPayload);

    if (upstreamPayload !== undefined && upstreamPayloadIsResponseEnvelope && !hasOrderState()) {
      return {
        status: 200,
        body: upstreamPayload,
      };
    }

    if (upstreamPayload !== undefined) {
      hydrateOrdersFromUpstreamResponse(upstreamPayload);
    }

    return {
      status: 200,
      body: handleOrderQuery(document, variables),
    };
  }

  throw new Error(
    `Parity execution does not allow live Shopify requests or unsupported operations: ${capability.operationName}`,
  );
}

function hasStagedState(): boolean {
  const { stagedState } = store.getState();
  return (
    Object.keys(stagedState.products).length > 0 ||
    Object.keys(stagedState.productVariants).length > 0 ||
    Object.keys(stagedState.productOptions).length > 0 ||
    Object.keys(stagedState.collections).length > 0 ||
    Object.keys(stagedState.productCollections).length > 0 ||
    Object.keys(stagedState.productMedia).length > 0 ||
    Object.keys(stagedState.productMetafields).length > 0 ||
    Object.keys(stagedState.deletedProductIds).length > 0 ||
    Object.keys(stagedState.deletedCollectionIds).length > 0 ||
    Object.keys(stagedState.orders).length > 0 ||
    Object.keys(stagedState.draftOrders).length > 0 ||
    Object.keys(stagedState.calculatedOrders).length > 0
  );
}

function hasOrderState(): boolean {
  const { baseState, stagedState } = store.getState();
  return (
    Object.keys(baseState.orders).length > 0 ||
    Object.keys(stagedState.orders).length > 0 ||
    Object.keys(stagedState.draftOrders).length > 0 ||
    Object.keys(stagedState.calculatedOrders).length > 0
  );
}

function firstObjectValue(value: unknown): Record<string, unknown> | null {
  if (!isPlainObject(value)) {
    return null;
  }
  const firstValue = Object.values(value)[0];
  return isPlainObject(firstValue) ? firstValue : null;
}

function mutationPayloadFromCapture(capture: unknown): Record<string, unknown> | null {
  return firstObjectValue(readJsonPath(capture, '$.mutation.response.data'));
}

function mutationNameFromCapture(capture: unknown): string | null {
  const data = readJsonPath(capture, '$.mutation.response.data');
  if (!isPlainObject(data)) {
    return null;
  }
  return Object.keys(data)[0] ?? null;
}

function readRecordField(
  value: Record<string, unknown> | null | undefined,
  key: string,
): Record<string, unknown> | null {
  const fieldValue = value?.[key];
  return isPlainObject(fieldValue) ? fieldValue : null;
}

function readStringField(value: Record<string, unknown> | null | undefined, key: string): string | null {
  const fieldValue = value?.[key];
  return typeof fieldValue === 'string' && fieldValue.length > 0 ? fieldValue : null;
}

function readNumberField(value: Record<string, unknown> | null | undefined, key: string): number | null {
  const fieldValue = value?.[key];
  return typeof fieldValue === 'number' ? fieldValue : null;
}

function readBooleanField(value: Record<string, unknown> | null | undefined, key: string): boolean | null {
  const fieldValue = value?.[key];
  return typeof fieldValue === 'boolean' ? fieldValue : null;
}

function readArrayField(value: Record<string, unknown> | null | undefined, key: string): unknown[] {
  const fieldValue = value?.[key];
  return Array.isArray(fieldValue) ? fieldValue : [];
}

function readMoneySetField(
  value: Record<string, unknown> | null | undefined,
  key: string,
): OrderRecord['currentTotalPriceSet'] {
  const rawSet = readRecordField(value, key);
  const shopMoney = readRecordField(rawSet, 'shopMoney');
  const amount = readStringField(shopMoney, 'amount');
  const currencyCode = readStringField(shopMoney, 'currencyCode');
  return amount || currencyCode
    ? {
        shopMoney: {
          amount,
          currencyCode,
        },
      }
    : null;
}

function readCapturedOrderLineItems(order: Record<string, unknown> | null): OrderLineItemRecord[] {
  return readArrayField(readRecordField(order, 'lineItems'), 'nodes')
    .filter(isPlainObject)
    .map((lineItem, index) => ({
      id: readStringField(lineItem, 'id') ?? `gid://shopify/LineItem/conformance-${index}`,
      title: readStringField(lineItem, 'title'),
      quantity: readNumberField(lineItem, 'quantity') ?? 0,
      sku: readStringField(lineItem, 'sku'),
      variantTitle: readStringField(lineItem, 'variantTitle'),
      originalUnitPriceSet: readMoneySetField(lineItem, 'originalUnitPriceSet'),
    }));
}

function readCapturedOrderShippingLines(order: Record<string, unknown> | null): OrderShippingLineRecord[] {
  return readArrayField(readRecordField(order, 'shippingLines'), 'nodes')
    .filter(isPlainObject)
    .map((shippingLine) => ({
      title: readStringField(shippingLine, 'title'),
      code: readStringField(shippingLine, 'code'),
      originalPriceSet: readMoneySetField(shippingLine, 'originalPriceSet'),
    }));
}

function makeSeedOrder(orderId: string, source: Record<string, unknown> | null = null): OrderRecord {
  const now = '2026-04-19T00:00:00.000Z';
  const totalPriceSet = readMoneySetField(source, 'totalPriceSet');
  const currentTotalPriceSet = readMoneySetField(source, 'currentTotalPriceSet');
  const subtotalPriceSet = readMoneySetField(source, 'subtotalPriceSet');
  const currencyCode = totalPriceSet?.shopMoney.currencyCode ?? currentTotalPriceSet?.shopMoney.currencyCode ?? 'CAD';

  return {
    id: orderId,
    name: readStringField(source, 'name') ?? '#1',
    createdAt: readStringField(source, 'createdAt') ?? readStringField(source, 'updatedAt') ?? now,
    updatedAt: readStringField(source, 'updatedAt') ?? now,
    displayFinancialStatus: readStringField(source, 'displayFinancialStatus'),
    displayFulfillmentStatus: readStringField(source, 'displayFulfillmentStatus'),
    note: readStringField(source, 'note'),
    tags: readArrayField(source, 'tags').filter((tag): tag is string => typeof tag === 'string'),
    customAttributes: readArrayField(source, 'customAttributes')
      .filter(isPlainObject)
      .map((attribute) => ({
        key: readStringField(attribute, 'key') ?? '',
        value: readStringField(attribute, 'value'),
      }))
      .filter((attribute) => attribute.key.length > 0),
    billingAddress: readCapturedAddress(source, 'billingAddress'),
    shippingAddress: readCapturedAddress(source, 'shippingAddress'),
    subtotalPriceSet,
    currentTotalPriceSet,
    totalPriceSet,
    totalRefundedSet: {
      shopMoney: {
        amount: '0.0',
        currencyCode,
      },
    },
    customer: null,
    shippingLines: readCapturedOrderShippingLines(source),
    lineItems: readCapturedOrderLineItems(source),
    transactions: [],
    refunds: [],
    returns: [],
  };
}

function readCapturedDraftOrderLineItems(draftOrder: Record<string, unknown> | null): DraftOrderLineItemRecord[] {
  return readArrayField(readRecordField(draftOrder, 'lineItems'), 'nodes')
    .filter(isPlainObject)
    .map((lineItem, index) => ({
      id: readStringField(lineItem, 'id') ?? `gid://shopify/DraftOrderLineItem/conformance-${index}`,
      title: readStringField(lineItem, 'title'),
      quantity: readNumberField(lineItem, 'quantity') ?? 0,
      sku: readStringField(lineItem, 'sku'),
      variantTitle: readStringField(lineItem, 'variantTitle'),
      originalUnitPriceSet: readMoneySetField(lineItem, 'originalUnitPriceSet'),
    }));
}

function readCapturedDraftOrderShippingLine(
  draftOrder: Record<string, unknown> | null,
): DraftOrderShippingLineRecord | null {
  const shippingLine = readRecordField(draftOrder, 'shippingLine');
  if (!shippingLine) {
    return null;
  }

  return {
    title: readStringField(shippingLine, 'title'),
    code: readStringField(shippingLine, 'code'),
    originalPriceSet: readMoneySetField(shippingLine, 'originalPriceSet'),
  };
}

function makeSeedDraftOrder(draftOrderId: string, source: Record<string, unknown> | null = null): DraftOrderRecord {
  const now = '2026-04-19T00:00:00.000Z';
  return {
    id: draftOrderId,
    name: readStringField(source, 'name') ?? '#D1',
    invoiceUrl: readStringField(source, 'invoiceUrl'),
    status: readStringField(source, 'status'),
    ready: readBooleanField(source, 'ready'),
    email: readStringField(source, 'email'),
    note: readStringField(source, 'note'),
    tags: readArrayField(source, 'tags').filter((tag): tag is string => typeof tag === 'string'),
    customAttributes: readArrayField(source, 'customAttributes')
      .filter(isPlainObject)
      .map((attribute) => ({
        key: readStringField(attribute, 'key') ?? '',
        value: readStringField(attribute, 'value'),
      }))
      .filter((attribute) => attribute.key.length > 0),
    billingAddress: readCapturedAddress(source, 'billingAddress'),
    shippingAddress: readCapturedAddress(source, 'shippingAddress'),
    shippingLine: readCapturedDraftOrderShippingLine(source),
    createdAt: readStringField(source, 'createdAt') ?? readStringField(source, 'updatedAt') ?? now,
    updatedAt: readStringField(source, 'updatedAt') ?? now,
    subtotalPriceSet: readMoneySetField(source, 'subtotalPriceSet'),
    totalPriceSet: readMoneySetField(source, 'totalPriceSet'),
    lineItems: readCapturedDraftOrderLineItems(source),
  };
}

function readCapturedAddress(
  source: Record<string, unknown> | null | undefined,
  key: string,
): OrderRecord['billingAddress'] {
  const address = readRecordField(source, key);
  if (!address) {
    return null;
  }

  return {
    firstName: readStringField(address, 'firstName'),
    lastName: readStringField(address, 'lastName'),
    address1: readStringField(address, 'address1'),
    city: readStringField(address, 'city'),
    provinceCode: readStringField(address, 'provinceCode'),
    countryCodeV2: readStringField(address, 'countryCodeV2'),
    zip: readStringField(address, 'zip'),
    phone: readStringField(address, 'phone'),
  };
}

function hydrateOrdersFromUpstreamResponse(upstreamPayload: unknown): void {
  const payload = isPlainObject(upstreamPayload) ? upstreamPayload : {};
  const data = readRecordField(payload, 'data') ?? payload;

  const order = readRecordField(data, 'order');
  const orderId = readStringField(order, 'id');
  if (orderId) {
    store.upsertBaseOrders([makeSeedOrder(orderId, order)]);
  }

  for (const edge of readArrayField(readRecordField(data, 'orders'), 'edges').filter(isPlainObject)) {
    const node = readRecordField(edge, 'node');
    const nodeId = readStringField(node, 'id');
    if (nodeId) {
      store.upsertBaseOrders([makeSeedOrder(nodeId, node)]);
    }
  }

  const draftOrder = readRecordField(data, 'draftOrder');
  const draftOrderId = readStringField(draftOrder, 'id');
  if (draftOrderId) {
    store.stageCreateDraftOrder(makeSeedDraftOrder(draftOrderId, draftOrder));
  }

  for (const edge of readArrayField(readRecordField(data, 'draftOrders'), 'edges').filter(isPlainObject)) {
    const node = readRecordField(edge, 'node');
    const nodeId = readStringField(node, 'id');
    if (nodeId) {
      store.stageCreateDraftOrder(makeSeedDraftOrder(nodeId, node));
    }
  }
}

function makeSeedProduct(
  productId: string,
  source: Record<string, unknown> | null = null,
  fallbackTitle = 'Conformance seed product',
): ProductRecord {
  const rawSeo = readRecordField(source, 'seo');
  const rawTags = readArrayField(source, 'tags').filter((tag): tag is string => typeof tag === 'string');
  const now = '2026-04-19T00:00:00.000Z';

  return {
    id: productId,
    legacyResourceId: readStringField(source, 'legacyResourceId'),
    title: readStringField(source, 'title') ?? fallbackTitle,
    handle: readStringField(source, 'handle') ?? `conformance-seed-${productId.split('/').at(-1) ?? 'product'}`,
    status:
      source?.['status'] === 'ACTIVE' || source?.['status'] === 'ARCHIVED' || source?.['status'] === 'DRAFT'
        ? source['status']
        : 'ACTIVE',
    publicationIds: readArrayField(source, 'publicationIds').filter(
      (publicationId): publicationId is string => typeof publicationId === 'string',
    ),
    createdAt: readStringField(source, 'createdAt') ?? now,
    updatedAt: readStringField(source, 'updatedAt') ?? now,
    vendor: readStringField(source, 'vendor'),
    productType: readStringField(source, 'productType'),
    tags: rawTags,
    totalInventory: typeof source?.['totalInventory'] === 'number' ? source['totalInventory'] : null,
    tracksInventory: typeof source?.['tracksInventory'] === 'boolean' ? source['tracksInventory'] : null,
    descriptionHtml: readStringField(source, 'descriptionHtml'),
    onlineStorePreviewUrl: readStringField(source, 'onlineStorePreviewUrl'),
    templateSuffix: readStringField(source, 'templateSuffix'),
    seo: {
      title: readStringField(rawSeo, 'title'),
      description: readStringField(rawSeo, 'description'),
    },
    category: null,
  };
}

function makeSeedVariant(
  productId: string,
  selectedOptions: ProductVariantRecord['selectedOptions'] = [],
): ProductVariantRecord {
  return {
    id: `gid://shopify/ProductVariant/${productId.split('/').at(-1) ?? '1'}0`,
    productId,
    title: selectedOptions.length > 0 ? selectedOptions.map((option) => option.value).join(' / ') : 'Default Title',
    sku: null,
    barcode: null,
    price: null,
    compareAtPrice: null,
    taxable: null,
    inventoryPolicy: null,
    inventoryQuantity: null,
    selectedOptions,
    inventoryItem: null,
  };
}

function makeCapturedVariant(productId: string, source: Record<string, unknown>): ProductVariantRecord | null {
  const id = readStringField(source, 'id');
  if (!id) {
    return null;
  }

  const selectedOptions = readArrayField(source, 'selectedOptions')
    .filter(isPlainObject)
    .map((selectedOption) => {
      const name = readStringField(selectedOption, 'name');
      const value = readStringField(selectedOption, 'value');
      return name && value ? { name, value } : null;
    })
    .filter(
      (selectedOption): selectedOption is ProductVariantRecord['selectedOptions'][number] => selectedOption !== null,
    );
  const inventoryItem = readRecordField(source, 'inventoryItem');
  const inventoryItemId = readStringField(inventoryItem, 'id');

  return {
    id,
    productId,
    title: readStringField(source, 'title') ?? 'Default Title',
    sku: readStringField(source, 'sku'),
    barcode: readStringField(source, 'barcode'),
    price: readStringField(source, 'price'),
    compareAtPrice: readStringField(source, 'compareAtPrice'),
    taxable: readBooleanField(source, 'taxable'),
    inventoryPolicy: readStringField(source, 'inventoryPolicy'),
    inventoryQuantity: readNumberField(source, 'inventoryQuantity'),
    selectedOptions,
    inventoryItem: inventoryItemId
      ? {
          id: inventoryItemId,
          tracked: readBooleanField(inventoryItem, 'tracked'),
          requiresShipping: readBooleanField(inventoryItem, 'requiresShipping'),
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
          inventoryLevels: [],
        }
      : null,
  };
}

function readCapturedProductVariants(
  productId: string,
  product: Record<string, unknown> | null,
): ProductVariantRecord[] {
  const variantNodes = readArrayField(readRecordField(product, 'variants'), 'nodes');
  return variantNodes
    .filter(isPlainObject)
    .map((variant) => makeCapturedVariant(productId, variant))
    .filter((variant): variant is ProductVariantRecord => variant !== null);
}

function readBulkUpdateSeedVariants(
  productId: string,
  product: Record<string, unknown> | null,
): ProductVariantRecord[] {
  return readCapturedProductVariants(productId, product).map((variant) => ({
    ...variant,
    // Seed the pre-update searchable variant state; the mutation under test must stage the captured values.
    sku: null,
    barcode: null,
    price: null,
    compareAtPrice: null,
    taxable: null,
    inventoryPolicy: null,
    inventoryItem: variant.inventoryItem
      ? {
          ...variant.inventoryItem,
          tracked: null,
          requiresShipping: null,
        }
      : null,
  }));
}

function readCapturedCreatedVariantIds(payload: Record<string, unknown> | null): Set<string> {
  return new Set(
    readArrayField(payload, 'productVariants')
      .filter(isPlainObject)
      .map((variant) => readStringField(variant, 'id'))
      .filter((id): id is string => id !== null),
  );
}

function makeDefaultOption(productId: string): ProductOptionRecord {
  return {
    id: `gid://shopify/ProductOption/${productId.split('/').at(-1) ?? '1'}0`,
    productId,
    name: 'Title',
    position: 1,
    optionValues: [
      {
        id: `gid://shopify/ProductOptionValue/${productId.split('/').at(-1) ?? '1'}0`,
        name: 'Default Title',
        hasVariants: true,
      },
    ],
  };
}

function makeSeedCollection(collectionId: string, source: Record<string, unknown> | null = null): CollectionRecord {
  return {
    id: collectionId,
    title: readStringField(source, 'title') ?? 'Conformance seed collection',
    handle: readStringField(source, 'handle') ?? `conformance-seed-${collectionId.split('/').at(-1) ?? 'collection'}`,
  };
}

function seedProductOptionState(productId: string, variables: Record<string, unknown>): void {
  const optionInput = readRecordField(variables, 'option');
  const optionId =
    readStringField(optionInput, 'id') ??
    readArrayField(variables, 'options').find((option): option is string => typeof option === 'string') ??
    null;
  if (!optionId) {
    store.replaceBaseOptionsForProduct(productId, [makeDefaultOption(productId)]);
    store.replaceBaseVariantsForProduct(productId, [makeSeedVariant(productId)]);
    return;
  }

  const valueToUpdate = readArrayField(variables, 'optionValuesToUpdate').find(isPlainObject) ?? null;
  const optionValueId =
    readStringField(valueToUpdate, 'id') ?? `gid://shopify/ProductOptionValue/${productId.split('/').at(-1) ?? '1'}0`;
  store.replaceBaseOptionsForProduct(productId, [
    {
      id: optionId,
      productId,
      name: readStringField(optionInput, 'name') ?? 'Color',
      position: 1,
      optionValues: [
        {
          id: optionValueId,
          name: 'Red',
          hasVariants: true,
        },
      ],
    },
  ]);
  store.replaceBaseVariantsForProduct(productId, [
    makeSeedVariant(productId, [
      {
        name: readStringField(optionInput, 'name') ?? 'Color',
        value: 'Red',
      },
    ]),
  ]);
}

function seedCollectionProducts(collection: CollectionRecord, productNodes: unknown[]): void {
  const collectionMemberships: ProductCollectionRecord[] = [];
  for (const node of productNodes.filter(isPlainObject)) {
    const productId = readStringField(node, 'id');
    if (!productId) {
      continue;
    }
    store.upsertBaseProducts([makeSeedProduct(productId, node)]);
    collectionMemberships.push({
      id: collection.id,
      productId,
      title: collection.title,
      handle: collection.handle,
    });
  }
  for (const membership of collectionMemberships) {
    store.replaceBaseCollectionsForProduct(membership.productId, [membership]);
  }
}

function inventoryAdjustmentPayload(capture: unknown): Record<string, unknown> | null {
  const mutationData = readJsonPath(capture, '$.mutation.response.data');
  return readRecordField(
    readRecordField(isPlainObject(mutationData) ? mutationData : null, 'inventoryAdjustQuantities'),
    'inventoryAdjustmentGroup',
  );
}

function inventoryAdjustmentLocation(capture: unknown): { id: string; name: string | null } | null {
  const changes = readArrayField(inventoryAdjustmentPayload(capture), 'changes');
  for (const change of changes.filter(isPlainObject)) {
    const location = readRecordField(change, 'location');
    const id = readStringField(location, 'id');
    if (id) {
      return { id, name: readStringField(location, 'name') };
    }
  }

  return null;
}

function seededAvailableQuantity(capture: unknown, inventoryItemId: string): number | null {
  const seedAdjustment = readJsonPath(capture, '$.setup.seedAdjustment.data.inventoryAdjustQuantities');
  const changes = readArrayField(
    readRecordField(isPlainObject(seedAdjustment) ? seedAdjustment : null, 'inventoryAdjustmentGroup'),
    'changes',
  );
  let quantity = 0;
  let found = false;

  for (const change of changes.filter(isPlainObject)) {
    const item = readRecordField(change, 'item');
    if (readStringField(change, 'name') !== 'available' || readStringField(item, 'id') !== inventoryItemId) {
      continue;
    }
    const delta = readNumberField(change, 'delta');
    if (delta !== null) {
      quantity += delta;
      found = true;
    }
  }

  return found ? quantity : null;
}

function makeInventoryAdjustmentSeedLevel(
  inventoryItemId: string,
  location: { id: string; name: string | null },
  availableQuantity: number,
): InventoryLevelRecord {
  return {
    id: `gid://shopify/InventoryLevel/${location.id.split('/').at(-1) ?? '1'}?inventory_item_id=${encodeURIComponent(
      inventoryItemId,
    )}`,
    cursor: `cursor:${inventoryItemId}:${location.id}`,
    location,
    quantities: [
      { name: 'available', quantity: availableQuantity, updatedAt: '2026-04-18T22:21:57Z' },
      { name: 'on_hand', quantity: availableQuantity, updatedAt: null },
      { name: 'incoming', quantity: 0, updatedAt: null },
    ],
  };
}

function makeInventoryAdjustmentSeedVariant(
  productId: string,
  source: Record<string, unknown>,
  location: { id: string; name: string | null },
  capture: unknown,
): ProductVariantRecord | null {
  const id = readStringField(source, 'id');
  const inventoryItem = readRecordField(source, 'inventoryItem');
  const inventoryItemId = readStringField(inventoryItem, 'id');
  if (!id || !inventoryItemId) {
    return null;
  }

  const inventoryQuantity =
    seededAvailableQuantity(capture, inventoryItemId) ?? readNumberField(source, 'inventoryQuantity');

  return {
    id,
    productId,
    title: readStringField(source, 'title') ?? 'Default Title',
    sku: readStringField(source, 'sku'),
    barcode: readStringField(source, 'barcode'),
    price: readStringField(source, 'price'),
    compareAtPrice: readStringField(source, 'compareAtPrice'),
    taxable: readBooleanField(source, 'taxable'),
    inventoryPolicy: readStringField(source, 'inventoryPolicy'),
    inventoryQuantity,
    selectedOptions: [],
    inventoryItem: {
      id: inventoryItemId,
      tracked: readBooleanField(inventoryItem, 'tracked'),
      requiresShipping: readBooleanField(inventoryItem, 'requiresShipping'),
      measurement: null,
      countryCodeOfOrigin: null,
      provinceCodeOfOrigin: null,
      harmonizedSystemCode: null,
      inventoryLevels: [makeInventoryAdjustmentSeedLevel(inventoryItemId, location, inventoryQuantity ?? 0)],
    },
  };
}

function makeProductVariantUpdateCompatibilitySeedVariant(
  productId: string,
  variantId: string,
  source: Record<string, unknown> | null,
): ProductVariantRecord {
  const inventoryItem = readRecordField(source, 'inventoryItem');
  const inventoryItemId = readStringField(inventoryItem, 'id');

  return {
    id: variantId,
    productId,
    title: 'Default Title',
    sku: null,
    barcode: null,
    price: null,
    compareAtPrice: null,
    taxable: null,
    inventoryPolicy: null,
    inventoryQuantity: null,
    selectedOptions: [],
    inventoryItem: inventoryItemId
      ? {
          id: inventoryItemId,
          tracked: null,
          requiresShipping: null,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
          inventoryLevels: null,
        }
      : null,
  };
}

function seedProductVariantUpdateCompatibilityPreconditions(
  capture: unknown,
  variables: Record<string, unknown>,
): boolean {
  if (mutationNameFromCapture(capture) !== 'productVariantsBulkUpdate') {
    return false;
  }

  const input = readRecordField(variables, 'input');
  const variantId = readStringField(input, 'id');
  if (!variantId) {
    return false;
  }

  const payload = mutationPayloadFromCapture(capture);
  const productPayload = readRecordField(payload, 'product');
  const productId = readStringField(productPayload, 'id');
  if (!productId) {
    return false;
  }

  const capturedVariant =
    readArrayField(payload, 'productVariants')
      .filter(isPlainObject)
      .find((variant) => readStringField(variant, 'id') === variantId) ??
    readArrayField(readRecordField(productPayload, 'variants'), 'nodes')
      .filter(isPlainObject)
      .find((variant) => readStringField(variant, 'id') === variantId) ??
    null;

  store.upsertBaseProducts([makeSeedProduct(productId, productPayload, 'Product variant update conformance seed')]);
  store.replaceBaseVariantsForProduct(productId, [
    makeProductVariantUpdateCompatibilitySeedVariant(productId, variantId, capturedVariant),
  ]);
  return true;
}

function seedProductVariantDeleteCompatibilityPreconditions(
  capture: unknown,
  variables: Record<string, unknown>,
): boolean {
  if (mutationNameFromCapture(capture) !== 'productVariantsBulkDelete') {
    return false;
  }

  const variantId = readStringField(variables, 'id');
  if (!variantId) {
    return false;
  }

  const productId = readStringField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'mutation'), 'variables'),
    'productId',
  );
  if (!productId) {
    return false;
  }

  const payload = mutationPayloadFromCapture(capture);
  const productPayload = readRecordField(payload, 'product');
  const downstreamProduct = readRecordField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'downstreamRead'), 'data'),
    'product',
  );
  const variantsSource = readStringField(downstreamProduct, 'id') === productId ? downstreamProduct : productPayload;
  const retainedVariants = readCapturedProductVariants(productId, variantsSource);

  store.upsertBaseProducts([makeSeedProduct(productId, productPayload, 'Product variant delete conformance seed')]);
  store.replaceBaseVariantsForProduct(productId, [
    makeProductVariantUpdateCompatibilitySeedVariant(productId, variantId, null),
    ...retainedVariants.filter((variant) => variant.id !== variantId),
  ]);
  return true;
}

function readTagQueryValue(query: string | null): string | null {
  if (!query) {
    return null;
  }

  const match = query.match(/\btag:("[^"]+"|'[^']+'|[^\s)]+)/i);
  if (!match) {
    return null;
  }

  return match[1]?.replace(/^["']|["']$/g, '') ?? null;
}

function readTagsRemoveSearchLaggedTags(capture: unknown): Set<string> {
  const downstreamVariables = readRecordField(capture as Record<string, unknown>, 'downstreamReadVariables');
  return new Set(
    ['remainingQuery', 'removedQuery']
      .map((key) => readTagQueryValue(readStringField(downstreamVariables, key)))
      .filter((tag): tag is string => typeof tag === 'string' && tag.length > 0),
  );
}

function seedTagsRemovePreconditions(
  productId: string,
  productPayload: Record<string, unknown> | null,
  capture: unknown,
  variables: Record<string, unknown>,
): boolean {
  if (!productPayload) {
    return false;
  }

  const postMutationTags = readArrayField(productPayload, 'tags').filter(
    (tag): tag is string => typeof tag === 'string',
  );
  const removedTags = readArrayField(variables, 'tags').filter((tag): tag is string => typeof tag === 'string');
  const searchLaggedTags = readTagsRemoveSearchLaggedTags(capture);
  const baseTags = postMutationTags.filter((tag) => !searchLaggedTags.has(tag));
  const preMutationTags = [...new Set([...postMutationTags, ...removedTags])];

  store.upsertBaseProducts([makeSeedProduct(productId, { ...productPayload, tags: baseTags })]);
  store.stageUpdateProduct(makeSeedProduct(productId, { ...productPayload, tags: preMutationTags }));
  return true;
}

function seedInventoryAdjustmentPreconditions(capture: unknown): void {
  const location = inventoryAdjustmentLocation(capture);
  if (!location) {
    return;
  }

  const trackedInventory = readRecordField(
    readRecordField(capture as Record<string, unknown>, 'setup'),
    'trackedInventory',
  );
  for (const setupKey of ['first', 'second']) {
    const productPayload = readRecordField(
      readRecordField(
        readRecordField(readRecordField(trackedInventory, setupKey), 'data'),
        'productVariantsBulkUpdate',
      ),
      'product',
    );
    const productId = readStringField(productPayload, 'id');
    if (!productId) {
      continue;
    }

    const variants = readArrayField(
      readRecordField(
        readRecordField(readRecordField(trackedInventory, setupKey), 'data'),
        'productVariantsBulkUpdate',
      ),
      'productVariants',
    )
      .filter(isPlainObject)
      .map((variant) => makeInventoryAdjustmentSeedVariant(productId, variant, location, capture))
      .filter((variant): variant is ProductVariantRecord => variant !== null);

    store.upsertBaseProducts([makeSeedProduct(productId, productPayload, 'Inventory adjustment conformance seed')]);
    if (variants.length > 0) {
      store.replaceBaseVariantsForProduct(productId, variants);
    }
  }
}

function readCapturedInventoryLevel(source: Record<string, unknown>): InventoryLevelRecord | null {
  const id = readStringField(source, 'id');
  if (!id) {
    return null;
  }

  const location = readRecordField(source, 'location');
  const locationId = readStringField(location, 'id');

  return {
    id,
    cursor: readStringField(source, 'cursor'),
    location: locationId ? { id: locationId, name: readStringField(location, 'name') } : null,
    quantities: readArrayField(source, 'quantities')
      .filter(isPlainObject)
      .map((quantity) => ({
        name: readStringField(quantity, 'name') ?? '',
        quantity: readNumberField(quantity, 'quantity'),
        updatedAt: readStringField(quantity, 'updatedAt'),
      }))
      .filter((quantity) => quantity.name.length > 0),
  };
}

function makeInventoryLinkageSeedVariant(
  productId: string,
  source: Record<string, unknown>,
): ProductVariantRecord | null {
  const id = readStringField(source, 'id');
  const inventoryItem = readRecordField(source, 'inventoryItem');
  const inventoryItemId = readStringField(inventoryItem, 'id');
  if (!id || !inventoryItemId) {
    return null;
  }

  const levels = readArrayField(readRecordField(inventoryItem, 'inventoryLevels'), 'nodes')
    .filter(isPlainObject)
    .map(readCapturedInventoryLevel)
    .filter((level): level is InventoryLevelRecord => level !== null);

  return {
    id,
    productId,
    title: readStringField(source, 'title') ?? 'Default Title',
    sku: readStringField(source, 'sku'),
    barcode: readStringField(source, 'barcode'),
    price: readStringField(source, 'price'),
    compareAtPrice: readStringField(source, 'compareAtPrice'),
    taxable: readBooleanField(source, 'taxable'),
    inventoryPolicy: readStringField(source, 'inventoryPolicy'),
    inventoryQuantity: readNumberField(source, 'inventoryQuantity'),
    selectedOptions: [],
    inventoryItem: {
      id: inventoryItemId,
      tracked: readBooleanField(inventoryItem, 'tracked'),
      requiresShipping: readBooleanField(inventoryItem, 'requiresShipping'),
      measurement: null,
      countryCodeOfOrigin: null,
      provinceCodeOfOrigin: null,
      harmonizedSystemCode: null,
      inventoryLevels: levels,
    },
  };
}

function seedInventoryLinkagePreconditions(capture: unknown): boolean {
  const captureObject = isPlainObject(capture) ? capture : {};
  if (
    !(
      'inventoryActivateNoOp' in captureObject ||
      'inventoryDeactivateOnlyLocationError' in captureObject ||
      'inventoryBulkToggleActivateNoOp' in captureObject
    )
  ) {
    return false;
  }

  const product = readRecordField(capture as Record<string, unknown>, 'createdProduct');
  const productId = readStringField(product, 'id');
  if (!productId) {
    return false;
  }

  const variants = readArrayField(readRecordField(product, 'variants'), 'nodes')
    .filter(isPlainObject)
    .map((variant) => makeInventoryLinkageSeedVariant(productId, variant))
    .filter((variant): variant is ProductVariantRecord => variant !== null);
  const firstVariant = variants[0] ?? null;

  store.upsertBaseProducts([
    makeSeedProduct(productId, {
      ...product,
      totalInventory: firstVariant?.inventoryQuantity ?? null,
      tracksInventory: firstVariant?.inventoryItem?.tracked ?? null,
    }),
  ]);
  if (variants.length > 0) {
    store.replaceBaseVariantsForProduct(productId, variants);
  }

  return true;
}

function seedInventoryItemUpdatePreconditions(capture: unknown): boolean {
  if (mutationNameFromCapture(capture) !== 'inventoryItemUpdate') {
    return false;
  }

  const productPayload = readRecordField(
    readRecordField(
      readRecordField(
        readRecordField(readRecordField(capture as Record<string, unknown>, 'mutation'), 'create'),
        'response',
      ),
      'data',
    ),
    'productCreate',
  )?.['product'];
  const product = isPlainObject(productPayload) ? productPayload : null;
  const productId = readStringField(product, 'id');
  if (!productId) {
    return false;
  }

  store.upsertBaseProducts([makeSeedProduct(productId, product)]);
  const variants = readCapturedProductVariants(productId, product);
  if (variants.length > 0) {
    store.replaceBaseVariantsForProduct(productId, variants);
  }

  return true;
}

function seedMetafieldsSetOwnerProducts(capture: unknown, variables: Record<string, unknown>): void {
  const downstreamProduct = readRecordField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'downstreamRead'), 'data'),
    'product',
  );
  for (const input of readArrayField(variables, 'metafields').filter(isPlainObject)) {
    const ownerId = readStringField(input, 'ownerId');
    if (!ownerId?.startsWith('gid://shopify/Product/') || store.getEffectiveProductById(ownerId)) {
      continue;
    }

    const source = readStringField(downstreamProduct, 'id') === ownerId ? downstreamProduct : null;
    store.upsertBaseProducts([makeSeedProduct(ownerId, source)]);
    if (source) {
      store.replaceBaseMetafieldsForProduct(ownerId, readCapturedProductMetafields(ownerId, source));
    }
  }
}

function seedMetafieldsDeleteOwnerProducts(capture: unknown, variables: Record<string, unknown>): boolean {
  if (mutationNameFromCapture(capture) !== 'metafieldsDelete') {
    return false;
  }

  const downstreamProduct = readRecordField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'downstreamRead'), 'data'),
    'product',
  );
  const deletedIdentifiers = readArrayField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'mutation'), 'variables'),
    'metafields',
  ).filter(isPlainObject);
  const retainedOwnerId = readStringField(downstreamProduct, 'id');
  const fallbackOwnerId = readStringField(deletedIdentifiers[0] ?? null, 'ownerId');
  const ownerId = retainedOwnerId ?? fallbackOwnerId;
  if (!ownerId?.startsWith('gid://shopify/Product/')) {
    return false;
  }

  const retainedMetafields = downstreamProduct ? readCapturedProductMetafields(ownerId, downstreamProduct) : [];
  const existingKeys = new Set(retainedMetafields.map((metafield) => `${metafield.namespace}:${metafield.key}`));
  const primaryInput = readRecordField(variables, 'input');
  const singularDeleteId = readStringField(primaryInput, 'id');
  const deletedMetafields = deletedIdentifiers
    .map((identifier, index): ProductMetafieldRecord | null => {
      const namespace = readStringField(identifier, 'namespace');
      const key = readStringField(identifier, 'key');
      const productId = readStringField(identifier, 'ownerId') ?? ownerId;
      if (!namespace || !key || !productId.startsWith('gid://shopify/Product/')) {
        return null;
      }
      const storageKey = `${namespace}:${key}`;
      if (existingKeys.has(storageKey)) {
        return null;
      }
      return {
        id: singularDeleteId ?? `gid://shopify/Metafield/conformance-deleted-${index}`,
        productId,
        namespace,
        key,
        type: 'single_line_text_field',
        value: null,
      };
    })
    .filter((metafield): metafield is ProductMetafieldRecord => metafield !== null);

  store.upsertBaseProducts([makeSeedProduct(ownerId, downstreamProduct)]);
  store.replaceBaseMetafieldsForProduct(ownerId, [...retainedMetafields, ...deletedMetafields]);
  return true;
}

function seedProductDuplicateSource(capture: unknown): boolean {
  if (mutationNameFromCapture(capture) !== 'productDuplicate') {
    return false;
  }

  const sourceRead = readRecordField(
    readRecordField(capture as Record<string, unknown>, 'setup'),
    'sourceReadBeforeDuplicate',
  );
  if (!sourceRead) {
    return false;
  }

  hydrateProductsFromUpstreamResponse('query ProductDuplicateSourceSeed { product { id } }', {}, sourceRead);
  return true;
}

function readCapturedProductMetafields(productId: string, product: Record<string, unknown>): ProductMetafieldRecord[] {
  const byIdentity = new Map<string, ProductMetafieldRecord>();
  const addMetafield = (candidate: unknown): void => {
    if (!isPlainObject(candidate)) {
      return;
    }
    const id = readStringField(candidate, 'id');
    const namespace = readStringField(candidate, 'namespace');
    const key = readStringField(candidate, 'key');
    if (!id?.startsWith('gid://shopify/Metafield/') || !namespace || !key) {
      return;
    }
    byIdentity.set(`${namespace}:${key}`, {
      id,
      productId,
      namespace,
      key,
      type: readStringField(candidate, 'type'),
      value: readStringField(candidate, 'value'),
    });
  };

  for (const value of Object.values(product)) {
    addMetafield(value);
  }

  const metafieldsConnection = readRecordField(product, 'metafields');
  for (const node of readArrayField(metafieldsConnection, 'nodes')) {
    addMetafield(node);
  }
  for (const edge of readArrayField(metafieldsConnection, 'edges').filter(isPlainObject)) {
    addMetafield(readRecordField(edge, 'node'));
  }

  return Array.from(byIdentity.values()).sort(
    (left, right) =>
      left.namespace.localeCompare(right.namespace) ||
      left.key.localeCompare(right.key) ||
      left.id.localeCompare(right.id),
  );
}

function readCapturedProductMedia(
  productId: string,
  product: Record<string, unknown> | null | undefined,
): ProductMediaRecord[] {
  const mediaConnection = readRecordField(product, 'media');
  return readArrayField(mediaConnection, 'nodes')
    .filter(isPlainObject)
    .map((node, index): ProductMediaRecord | null => {
      const id = readStringField(node, 'id');
      if (!id) {
        return null;
      }

      const previewImage = readRecordField(readRecordField(node, 'preview'), 'image');
      const image = readRecordField(node, 'image');
      const previewImageUrl = readStringField(previewImage, 'url');
      const imageUrl = readStringField(image, 'url') ?? previewImageUrl;

      return {
        key: `${productId}:media:${index}`,
        productId,
        position: index,
        id,
        mediaContentType: readStringField(node, 'mediaContentType'),
        alt: readStringField(node, 'alt'),
        status: readStringField(node, 'status'),
        productImageId: null,
        imageUrl,
        previewImageUrl,
        sourceUrl: imageUrl ?? previewImageUrl,
      };
    })
    .filter((mediaRecord): mediaRecord is ProductMediaRecord => mediaRecord !== null);
}

function seedPreconditionsFromCapture(capture: unknown, variables: Record<string, unknown>): void {
  if (seedInventoryLinkagePreconditions(capture)) {
    return;
  }

  if (seedMetafieldsDeleteOwnerProducts(capture, variables)) {
    return;
  }

  if (seedProductVariantUpdateCompatibilityPreconditions(capture, variables)) {
    return;
  }

  if (seedProductVariantDeleteCompatibilityPreconditions(capture, variables)) {
    return;
  }

  const payload = mutationPayloadFromCapture(capture);
  const mutationName = mutationNameFromCapture(capture);
  if (mutationName === 'orderUpdate') {
    const input = readRecordField(variables, 'input');
    const orderId = readStringField(input, 'id');
    if (orderId) {
      const orderPayload = readRecordField(payload, 'order');
      const downstreamOrder = readRecordField(
        readRecordField(readRecordField(capture as Record<string, unknown>, 'downstreamRead'), 'response'),
        'data',
      );
      const downstreamSource = readRecordField(downstreamOrder, 'order');
      const seedSource = orderPayload ?? downstreamSource;
      if (seedSource) {
        store.upsertBaseOrders([makeSeedOrder(orderId, seedSource)]);
      }
    }
    return;
  }

  if (mutationName === 'refundCreate') {
    const input = readRecordField(variables, 'input');
    const orderId = readStringField(input, 'orderId');
    if (orderId) {
      const setupOrder = readRecordField(
        readRecordField(
          readRecordField(
            readRecordField(readRecordField(capture as Record<string, unknown>, 'setup'), 'orderCreate'),
            'response',
          ),
          'data',
        ),
        'orderCreate',
      );
      const orderCreateSource = readRecordField(setupOrder, 'order');
      const downstreamOrder = readRecordField(
        readRecordField(readRecordField(capture as Record<string, unknown>, 'downstreamRead'), 'response'),
        'data',
      );
      const downstreamSource = readRecordField(downstreamOrder, 'order');
      const seedSource = orderCreateSource ?? downstreamSource;
      if (seedSource) {
        store.upsertBaseOrders([makeSeedOrder(orderId, seedSource)]);
      }
    }
    return;
  }

  if (mutationName === 'inventoryAdjustQuantities') {
    seedInventoryAdjustmentPreconditions(capture);
    return;
  }

  if (seedInventoryItemUpdatePreconditions(capture)) {
    return;
  }

  const productInput = readRecordField(variables, 'product');
  const input = readRecordField(variables, 'input');
  const identifier = readRecordField(variables, 'identifier');
  const productPayload =
    readRecordField(payload, 'product') ??
    (readStringField(readRecordField(payload, 'node'), 'id')?.startsWith('gid://shopify/Product/')
      ? readRecordField(payload, 'node')
      : null);
  const rawProductId =
    readStringField(productInput, 'id') ??
    readStringField(variables, 'productId') ??
    readStringField(variables, 'id') ??
    readStringField(input, 'id') ??
    readStringField(productPayload, 'id') ??
    readStringField(payload, 'deletedProductId');
  const productId = rawProductId?.startsWith('gid://shopify/Product/') ? rawProductId : null;
  const isProductSetCreate =
    mutationName === 'productSet' &&
    !readStringField(identifier, 'id') &&
    !readStringField(identifier, 'handle') &&
    !readStringField(input, 'id');

  const shouldSeedProduct =
    productId !== null &&
    !(mutationName === 'productCreate' && readStringField(productInput, 'id') === null) &&
    !isProductSetCreate;

  if (seedProductDuplicateSource(capture)) {
    return;
  }

  if (shouldSeedProduct) {
    if (mutationName === 'tagsRemove' && seedTagsRemovePreconditions(productId, productPayload, capture, variables)) {
      return;
    }

    const seedSource = mutationName === 'tagsAdd' ? null : (productPayload ?? productInput);
    store.upsertBaseProducts([makeSeedProduct(productId, seedSource)]);
    if (
      mutationName === 'productVariantsBulkCreate' ||
      mutationName === 'productVariantsBulkUpdate' ||
      mutationName === 'productVariantsBulkDelete'
    ) {
      const downstreamProduct = readRecordField(
        readRecordField(readRecordField(capture as Record<string, unknown>, 'downstreamRead'), 'data'),
        'product',
      );
      const variantsSource =
        readStringField(downstreamProduct, 'id') === productId ? downstreamProduct : productPayload;
      const variants =
        mutationName === 'productVariantsBulkCreate'
          ? readCapturedProductVariants(productId, variantsSource).filter(
              (variant) => !readCapturedCreatedVariantIds(payload).has(variant.id),
            )
          : mutationName === 'productVariantsBulkUpdate'
            ? readBulkUpdateSeedVariants(productId, variantsSource)
            : readCapturedProductVariants(productId, variantsSource);
      if (variants.length > 0) {
        store.replaceBaseVariantsForProduct(productId, variants);
      }
    }
    if (readArrayField(variables, 'options').length > 0 || readRecordField(variables, 'option')) {
      seedProductOptionState(productId, variables);
    }

    if (mutationName === 'productUpdateMedia') {
      const downstreamProduct = readRecordField(
        readRecordField(readRecordField(capture as Record<string, unknown>, 'downstreamRead'), 'data'),
        'product',
      );
      const mediaSource = readStringField(downstreamProduct, 'id') === productId ? downstreamProduct : null;
      const capturedMedia = readCapturedProductMedia(productId, mediaSource);
      if (capturedMedia.length > 0) {
        store.replaceBaseMediaForProduct(productId, capturedMedia);
      }
    }
    if (mutationName === 'productDeleteMedia') {
      const mediaIds = readArrayField(variables, 'mediaIds').filter(
        (mediaId): mediaId is string => typeof mediaId === 'string',
      );
      if (mediaIds.length > 0) {
        const deletedProductImageIds = readArrayField(payload, 'deletedProductImageIds').filter(
          (productImageId): productImageId is string => typeof productImageId === 'string',
        );
        store.replaceBaseMediaForProduct(
          productId,
          mediaIds.map((mediaId, index) => ({
            key: `${productId}:media:${index}`,
            productId,
            position: index,
            id: mediaId,
            mediaContentType: 'IMAGE',
            alt: null,
            status: 'READY',
            productImageId: deletedProductImageIds[index] ?? null,
            imageUrl: null,
            previewImageUrl: null,
            sourceUrl: null,
          })),
        );
      }
    }
  }

  if (mutationName === 'metafieldsSet') {
    seedMetafieldsSetOwnerProducts(capture, variables);
  }

  const collectionPayload = readRecordField(payload, 'collection');
  const rawCollectionId =
    readStringField(variables, 'id') ?? readStringField(input, 'id') ?? readStringField(collectionPayload, 'id');
  const collectionId = rawCollectionId?.startsWith('gid://shopify/Collection/') ? rawCollectionId : null;
  if (collectionId) {
    const collection = makeSeedCollection(collectionId, collectionPayload);
    store.upsertBaseCollections([collection]);
    const rawProductNodes = readRecordField(collectionPayload, 'products')?.['nodes'];
    const productNodes = Array.isArray(rawProductNodes) ? rawProductNodes : [];
    if (mutationName === 'collectionUpdate') {
      seedCollectionProducts(collection, productNodes);
    } else {
      for (const node of productNodes.filter(isPlainObject)) {
        const productId = readStringField(node, 'id');
        if (productId) {
          store.upsertBaseProducts([makeSeedProduct(productId, node)]);
        }
      }
    }
    for (const productIdValue of readArrayField(variables, 'productIds')) {
      if (typeof productIdValue !== 'string') {
        continue;
      }
      store.upsertBaseProducts([makeSeedProduct(productIdValue)]);
    }
  }
}

function readComparisonTargets(comparison: ComparisonContract): ComparisonTarget[] {
  return Array.isArray(comparison.targets) ? comparison.targets : [];
}

function readRequestVariables(
  repoRoot: string,
  request: ProxyRequestSpec,
  capture: unknown,
  primaryProxyResponse: unknown,
): Record<string, unknown> {
  if (request.variablesCapturePath) {
    return materializeVariables(readJsonPath(capture, request.variablesCapturePath), primaryProxyResponse);
  }

  const rawVariables = request.variablesPath
    ? parseJsonFileWithSchema(path.join(repoRoot, request.variablesPath), graphqlVariablesSchema)
    : request.variables;
  return materializeVariables(rawVariables, primaryProxyResponse);
}

function readPrimaryUpstreamPayload(capture: unknown, comparison: ComparisonContract, document: string): unknown {
  const parsed = parseOperation(document);
  const capability = getOperationCapability(parsed);
  if (capability.execution !== 'overlay-read') {
    return undefined;
  }

  const target = readComparisonTargets(comparison)[0];
  if (!target) {
    return undefined;
  }

  if (target.upstreamCapturePath === null) {
    return undefined;
  }

  if (typeof target.upstreamCapturePath === 'string') {
    return readJsonPath(capture, target.upstreamCapturePath);
  }

  if (target.capturePath.startsWith('$.data')) {
    return capture;
  }

  return readJsonPath(capture, target.capturePath);
}

export async function executeParityScenario({
  repoRoot,
  scenario,
  paritySpec,
}: {
  repoRoot: string;
  scenario: Scenario;
  paritySpec: ParitySpec;
}): Promise<{
  ok: boolean;
  primaryProxyStatus: number;
  comparisons: Array<{ name: string; ok: boolean; differences: Difference[] }>;
}> {
  if (!paritySpec.proxyRequest?.documentPath) {
    throw new Error(`Scenario ${scenario.id} does not define a proxy request.`);
  }
  if (validateComparisonContract(paritySpec.comparison).length > 0 || !paritySpec.comparison) {
    throw new Error(`Scenario ${scenario.id} does not define a valid comparison contract.`);
  }
  if (readComparisonTargets(paritySpec.comparison).length === 0) {
    throw new Error(
      `Scenario ${scenario.id} must declare at least one comparison target or a blocker; no implicit fallback target is used.`,
    );
  }

  store.reset();
  resetSyntheticIdentity();

  const capturePath = scenario.captureFiles?.[0] ?? paritySpec.liveCaptureFiles?.[0];
  if (typeof capturePath !== 'string') {
    throw new Error(`Scenario ${scenario.id} does not reference a capture fixture.`);
  }

  const capture = readJsonFile(repoRoot, capturePath);
  const primaryDocument = readTextFile(repoRoot, paritySpec.proxyRequest.documentPath);
  const primaryVariables = readRequestVariables(repoRoot, paritySpec.proxyRequest, capture, {});
  seedPreconditionsFromCapture(capture, primaryVariables);
  const primaryProxyResponse = await executeGraphQLAgainstLocalProxy(
    primaryDocument,
    primaryVariables,
    readPrimaryUpstreamPayload(capture, paritySpec.comparison, primaryDocument),
  );

  const comparisons = [];
  for (const target of readComparisonTargets(paritySpec.comparison)) {
    const expected = readJsonPath(capture, target.capturePath);
    let proxyResponseBody: unknown = primaryProxyResponse.body;

    if (target.proxyRequest?.documentPath) {
      if (typeof target.proxyRequest.waitBeforeMs === 'number' && target.proxyRequest.waitBeforeMs > 0) {
        await sleep(target.proxyRequest.waitBeforeMs);
      }
      const document = readTextFile(repoRoot, target.proxyRequest.documentPath);
      const variables = readRequestVariables(repoRoot, target.proxyRequest, capture, primaryProxyResponse.body);
      const upstreamPayload =
        target.upstreamCapturePath === null
          ? undefined
          : typeof target.upstreamCapturePath === 'string'
            ? readJsonPath(capture, target.upstreamCapturePath)
            : undefined;
      const proxyResponse = await executeGraphQLAgainstLocalProxy(document, variables, upstreamPayload);
      proxyResponseBody = proxyResponse.body;
    }

    const actual = readJsonPath(proxyResponseBody, target.proxyPath);
    const comparison = compareJsonPayloads(expected, actual, paritySpec.comparison);
    comparisons.push({
      name: target.name,
      ok: comparison.ok,
      differences: comparison.differences,
    });
  }

  return {
    ok: comparisons.every((comparison) => comparison.ok),
    primaryProxyStatus: primaryProxyResponse.status,
    comparisons,
  };
}
