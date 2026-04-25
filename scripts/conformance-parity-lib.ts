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
import {
  handleCustomerMutation,
  handleCustomerQuery,
  hydrateCustomersFromUpstreamResponse,
} from '../src/proxy/customers.js';
import { getOperationCapability, type OperationCapability } from '../src/proxy/capabilities.js';
import { handleMediaMutation } from '../src/proxy/media.js';
import { handleOrderMutation, handleOrderQuery } from '../src/proxy/orders.js';
import {
  handleProductMutation,
  handleProductQuery,
  hydrateProductsFromUpstreamResponse,
} from '../src/proxy/products.js';
import { handleStorePropertiesQuery } from '../src/proxy/store-properties.js';
import { makeSyntheticGid, makeSyntheticTimestamp, resetSyntheticIdentity } from '../src/state/synthetic-identity.js';
import { store } from '../src/state/store.js';
import type {
  BusinessEntityRecord,
  CollectionRecord,
  CustomerRecord,
  DraftOrderLineItemRecord,
  DraftOrderRecord,
  DraftOrderShippingLineRecord,
  InventoryLevelRecord,
  MutationLogInterpretedMetadata,
  OrderCustomerRecord,
  OrderFulfillmentLineItemRecord,
  OrderFulfillmentOrderLineItemRecord,
  OrderFulfillmentOrderRecord,
  OrderFulfillmentRecord,
  OrderLineItemRecord,
  OrderMetafieldRecord,
  OrderRecord,
  OrderShippingLineRecord,
  ProductCollectionRecord,
  ProductMetafieldRecord,
  ProductMediaRecord,
  ProductOptionRecord,
  ProductRecord,
  ProductVariantRecord,
  ShopifyPaymentsAccountRecord,
  ShopRecord,
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
  'readyForComparison means a captured scenario has a proxy request and an explicit strict-json comparison contract. invalid scenarios are captured recordings that cannot run high-assurance comparison yet. notYetImplemented scenarios are legacy non-executable entries; do not add new planned-only or blocked-only parity specs.';

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

  if (
    capability.execution === 'stage-locally' &&
    (capability.domain === 'products' ||
      (capability.domain === 'store-properties' && capability.operationName?.startsWith('publishable') === true))
  ) {
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

  if (
    capability.execution === 'stage-locally' &&
    capability.domain === 'store-properties' &&
    (capability.operationName === 'publishablePublish' ||
      capability.operationName === 'PublishablePublish' ||
      capability.operationName === 'publishableUnpublish' ||
      capability.operationName === 'PublishableUnpublish')
  ) {
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

  if (capability.execution === 'stage-locally' && capability.domain === 'media') {
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
      body: handleMediaMutation(document, variables),
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

  if (capability.execution === 'stage-locally' && capability.domain === 'customers') {
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
      body: handleCustomerMutation(document, variables),
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

  if (capability.execution === 'overlay-read' && capability.domain === 'customers') {
    if (upstreamPayload !== undefined) {
      hydrateCustomersFromUpstreamResponse(document, variables, upstreamPayload);
      if (!hasStagedState()) {
        return {
          status: 200,
          body: isPlainObject(upstreamPayload) ? upstreamPayload : {},
        };
      }
    }

    return {
      status: 200,
      body: handleCustomerQuery(document, variables),
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

  if (capability.execution === 'overlay-read' && capability.domain === 'store-properties') {
    return {
      status: 200,
      body: handleStorePropertiesQuery(document, variables),
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
    Object.keys(stagedState.files).length > 0 ||
    Object.keys(stagedState.productMetafields).length > 0 ||
    Object.keys(stagedState.deletedProductIds).length > 0 ||
    Object.keys(stagedState.deletedFileIds).length > 0 ||
    Object.keys(stagedState.deletedCollectionIds).length > 0 ||
    Object.keys(stagedState.customers).length > 0 ||
    Object.keys(stagedState.deletedCustomerIds).length > 0 ||
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

function readNullableStringField(value: Record<string, unknown> | null | undefined, key: string): string | null {
  const fieldValue = value?.[key];
  return typeof fieldValue === 'string' ? fieldValue : null;
}

function readStringArrayField(value: Record<string, unknown> | null | undefined, key: string): string[] {
  return readArrayField(value, key).filter((entry): entry is string => typeof entry === 'string');
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
      currentQuantity: readNumberField(lineItem, 'currentQuantity') ?? undefined,
      sku: typeof lineItem['sku'] === 'string' ? lineItem['sku'] : null,
      variantId: readStringField(readRecordField(lineItem, 'variant'), 'id'),
      variantTitle: readStringField(lineItem, 'variantTitle'),
      originalUnitPriceSet: readMoneySetField(lineItem, 'originalUnitPriceSet'),
      taxLines: readCapturedOrderTaxLines(lineItem),
    }));
}

function readCapturedOrderTaxLines(source: Record<string, unknown> | null): OrderRecord['taxLines'] {
  return readArrayField(source, 'taxLines')
    .filter(isPlainObject)
    .map((taxLine) => ({
      title: readStringField(taxLine, 'title'),
      rate: readNumberField(taxLine, 'rate'),
      channelLiable: readBooleanField(taxLine, 'channelLiable'),
      priceSet: readMoneySetField(taxLine, 'priceSet'),
    }));
}

function readCapturedOrderShippingLines(order: Record<string, unknown> | null): OrderShippingLineRecord[] {
  return readArrayField(readRecordField(order, 'shippingLines'), 'nodes')
    .filter(isPlainObject)
    .map((shippingLine) => ({
      title: readStringField(shippingLine, 'title'),
      code: readStringField(shippingLine, 'code'),
      source: readStringField(shippingLine, 'source'),
      originalPriceSet: readMoneySetField(shippingLine, 'originalPriceSet'),
      taxLines: readCapturedOrderTaxLines(shippingLine),
    }));
}

function readCapturedOrderCustomer(order: Record<string, unknown> | null): OrderCustomerRecord | null {
  const customer = readRecordField(order, 'customer');
  const id = readStringField(customer, 'id');
  if (!id) {
    return null;
  }

  return {
    id,
    email: readStringField(customer, 'email'),
    displayName: readStringField(customer, 'displayName'),
  };
}

function readCustomerMoneyField(
  value: Record<string, unknown> | null | undefined,
  key: string,
): CustomerRecord['amountSpent'] {
  const rawMoney = readRecordField(value, key);
  const amount = readStringField(rawMoney, 'amount');
  const currencyCode = readStringField(rawMoney, 'currencyCode');
  return amount || currencyCode
    ? {
        amount,
        currencyCode,
      }
    : null;
}

function readCustomerDefaultAddress(
  customer: Record<string, unknown> | null | undefined,
): CustomerRecord['defaultAddress'] {
  const address = readRecordField(customer, 'defaultAddress');
  if (!address) {
    return null;
  }

  return {
    address1: readStringField(address, 'address1'),
    city: readStringField(address, 'city'),
    province: readStringField(address, 'province'),
    country: readStringField(address, 'country'),
    zip: readStringField(address, 'zip'),
    formattedArea: readStringField(address, 'formattedArea'),
  };
}

function readCustomerDefaultEmailAddress(
  customer: Record<string, unknown> | null | undefined,
): CustomerRecord['defaultEmailAddress'] {
  const email = readStringField(customer, 'email');
  const defaultEmailAddress = readRecordField(customer, 'defaultEmailAddress');
  if (!defaultEmailAddress && !email) {
    return null;
  }

  return {
    emailAddress: readStringField(defaultEmailAddress, 'emailAddress') ?? email,
    marketingState: readStringField(defaultEmailAddress, 'marketingState'),
    marketingOptInLevel: readStringField(defaultEmailAddress, 'marketingOptInLevel'),
    marketingUpdatedAt: readStringField(defaultEmailAddress, 'marketingUpdatedAt'),
  };
}

function readCustomerDefaultPhoneNumber(
  customer: Record<string, unknown> | null | undefined,
): CustomerRecord['defaultPhoneNumber'] {
  const defaultPhoneNumber = readRecordField(customer, 'defaultPhoneNumber');
  if (!defaultPhoneNumber) {
    return null;
  }

  return {
    phoneNumber: readStringField(defaultPhoneNumber, 'phoneNumber'),
    marketingState: readStringField(defaultPhoneNumber, 'marketingState'),
    marketingOptInLevel: readStringField(defaultPhoneNumber, 'marketingOptInLevel'),
    marketingUpdatedAt: readStringField(defaultPhoneNumber, 'marketingUpdatedAt'),
    marketingCollectedFrom: readStringField(defaultPhoneNumber, 'marketingCollectedFrom'),
  };
}

function makeSeedCustomer(customerId: string, source: Record<string, unknown> | null = null): CustomerRecord {
  const email = readStringField(source, 'email');
  const firstName = readStringField(source, 'firstName');
  const lastName = readStringField(source, 'lastName');
  const nameFromParts = [firstName, lastName]
    .filter((part): part is string => typeof part === 'string' && part.length > 0)
    .join(' ');
  const defaultEmailAddress = readCustomerDefaultEmailAddress(source);
  const defaultPhoneNumber = readCustomerDefaultPhoneNumber(source);

  return {
    id: customerId,
    firstName,
    lastName,
    displayName: readStringField(source, 'displayName') ?? (nameFromParts || email),
    email,
    legacyResourceId: readStringField(source, 'legacyResourceId') ?? customerId.split('/').at(-1) ?? null,
    locale: readStringField(source, 'locale'),
    note: readStringField(source, 'note'),
    canDelete: readBooleanField(source, 'canDelete') ?? true,
    verifiedEmail: readBooleanField(source, 'verifiedEmail') ?? (email ? true : null),
    taxExempt: readBooleanField(source, 'taxExempt') ?? false,
    state: readStringField(source, 'state') ?? 'DISABLED',
    tags: readArrayField(source, 'tags').filter((tag): tag is string => typeof tag === 'string'),
    numberOfOrders: readNumberField(source, 'numberOfOrders') ?? readStringField(source, 'numberOfOrders') ?? 0,
    amountSpent: readCustomerMoneyField(source, 'amountSpent'),
    defaultEmailAddress,
    defaultPhoneNumber,
    emailMarketingConsent: defaultEmailAddress?.marketingState
      ? {
          marketingState: defaultEmailAddress.marketingState,
          marketingOptInLevel: defaultEmailAddress.marketingOptInLevel ?? null,
          consentUpdatedAt: defaultEmailAddress.marketingUpdatedAt ?? null,
        }
      : null,
    smsMarketingConsent: defaultPhoneNumber?.marketingState
      ? {
          marketingState: defaultPhoneNumber.marketingState,
          marketingOptInLevel: defaultPhoneNumber.marketingOptInLevel ?? null,
          consentUpdatedAt: defaultPhoneNumber.marketingUpdatedAt ?? null,
          consentCollectedFrom: defaultPhoneNumber.marketingCollectedFrom ?? null,
        }
      : null,
    defaultAddress: readCustomerDefaultAddress(source),
    createdAt: readStringField(source, 'createdAt') ?? '2024-01-01T00:00:00.000Z',
    updatedAt: readStringField(source, 'updatedAt') ?? '2024-01-01T00:00:00.000Z',
  };
}

function makePlaceholderCustomer(index: number): CustomerRecord {
  const id = `gid://shopify/Customer/conformance-baseline-${index}`;
  return {
    id,
    firstName: 'Conformance',
    lastName: `Baseline ${index}`,
    displayName: `Conformance Baseline ${index}`,
    email: `customer-baseline-${index}@example.invalid`,
    legacyResourceId: `conformance-baseline-${index}`,
    locale: 'en',
    note: null,
    canDelete: true,
    verifiedEmail: true,
    taxExempt: false,
    state: 'DISABLED',
    tags: ['baseline'],
    numberOfOrders: 0,
    amountSpent: null,
    defaultEmailAddress: { emailAddress: `customer-baseline-${index}@example.invalid` },
    defaultPhoneNumber: null,
    emailMarketingConsent: null,
    smsMarketingConsent: null,
    defaultAddress: null,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
  };
}

function seedCustomerMutationPreconditions(
  capture: unknown,
  variables: Record<string, unknown>,
  mutationName: string | null,
  payload: Record<string, unknown> | null,
): boolean {
  if (
    mutationName !== 'customerCreate' &&
    mutationName !== 'customerUpdate' &&
    mutationName !== 'customerDelete' &&
    mutationName !== 'customerEmailMarketingConsentUpdate' &&
    mutationName !== 'customerSmsMarketingConsentUpdate'
  ) {
    return false;
  }

  const input = readRecordField(variables, 'input');
  const customerPayload = readRecordField(payload, 'customer');
  const preconditionPayload = firstObjectValue(readJsonPath(capture, '$.precondition.response.data'));
  const preconditionCustomerPayload = readRecordField(preconditionPayload, 'customer');
  const downstreamData = readRecordField(readRecordField(capture as Record<string, unknown>, 'downstreamRead'), 'data');
  const downstreamCount = readNumberField(readRecordField(downstreamData, 'customersCount'), 'count');
  const targetCustomerId =
    readStringField(input, 'id') ??
    readStringField(input, 'customerId') ??
    readStringField(customerPayload, 'id') ??
    readStringField(preconditionCustomerPayload, 'id') ??
    readStringField(payload, 'deletedCustomerId');
  const seedCustomers: CustomerRecord[] = [];

  if (targetCustomerId && mutationName !== 'customerCreate') {
    seedCustomers.push(makeSeedCustomer(targetCustomerId, preconditionCustomerPayload ?? customerPayload));
  }

  if (downstreamCount !== null) {
    const targetContributesToDownstreamCount =
      mutationName === 'customerCreate' || mutationName === 'customerUpdate' ? 1 : 0;
    const placeholderCount = Math.max(0, downstreamCount - targetContributesToDownstreamCount);
    for (let index = 0; index < placeholderCount; index += 1) {
      seedCustomers.push(makePlaceholderCustomer(index));
    }
  }

  if (seedCustomers.length > 0) {
    store.upsertBaseCustomers(seedCustomers);
  }

  return true;
}

function seedCustomerByIdentifierPreconditions(capture: unknown): boolean {
  const positiveAndMissingData = readRecordField(
    readRecordField(capture as Record<string, unknown>, 'positiveAndMissing'),
    'data',
  );
  const customers = ['byId', 'byEmail', 'byPhone']
    .map((key) => readRecordField(positiveAndMissingData, key))
    .filter((customer): customer is Record<string, unknown> => customer !== null);
  const seedCustomers = new Map<string, CustomerRecord>();

  for (const customer of customers) {
    const customerId = readStringField(customer, 'id');
    if (customerId && !seedCustomers.has(customerId)) {
      seedCustomers.set(customerId, makeSeedCustomer(customerId, customer));
    }
  }

  if (seedCustomers.size === 0) {
    return false;
  }

  store.upsertBaseCustomers([...seedCustomers.values()]);
  return true;
}

function readShopifyPaymentsAccountRecord(source: Record<string, unknown> | null): ShopifyPaymentsAccountRecord | null {
  if (!source) {
    return null;
  }

  const id = readStringField(source, 'id');
  const activated = readBooleanField(source, 'activated');
  const country = readStringField(source, 'country');
  const defaultCurrency = readStringField(source, 'defaultCurrency');
  const onboardable = readBooleanField(source, 'onboardable');

  if (!id || activated === null || !country || !defaultCurrency || onboardable === null) {
    return null;
  }

  return {
    id,
    activated,
    country,
    defaultCurrency,
    onboardable,
  };
}

function readBusinessEntityRecord(source: Record<string, unknown> | null): BusinessEntityRecord | null {
  if (!source) {
    return null;
  }

  const id = readStringField(source, 'id');
  const displayName = readStringField(source, 'displayName');
  const primary = readBooleanField(source, 'primary');
  const archived = readBooleanField(source, 'archived');
  const address = readRecordField(source, 'address');
  const countryCode = readStringField(address, 'countryCode');

  if (!id || !displayName || primary === null || archived === null || !address || !countryCode) {
    return null;
  }

  return {
    id,
    displayName,
    companyName: readStringField(source, 'companyName'),
    primary,
    archived,
    address: {
      address1: readStringField(address, 'address1'),
      address2: readStringField(address, 'address2'),
      city: readStringField(address, 'city'),
      countryCode,
      province: readStringField(address, 'province'),
      zip: readStringField(address, 'zip'),
    },
    shopifyPaymentsAccount: readShopifyPaymentsAccountRecord(readRecordField(source, 'shopifyPaymentsAccount')),
  };
}

function readShopRecord(source: Record<string, unknown> | null): ShopRecord | null {
  if (!source) {
    return null;
  }

  const primaryDomain = readRecordField(source, 'primaryDomain');
  const shopAddress = readRecordField(source, 'shopAddress');
  const plan = readRecordField(source, 'plan');
  const resourceLimits = readRecordField(source, 'resourceLimits');
  const features = readRecordField(source, 'features');
  const bundles = readRecordField(features, 'bundles');
  const cartTransform = readRecordField(features, 'cartTransform');
  const eligibleOperations = readRecordField(cartTransform, 'eligibleOperations');
  const paymentSettings = readRecordField(source, 'paymentSettings');
  const policies = readArrayField(source, 'shopPolicies')
    .filter(isPlainObject)
    .map((policy) => {
      const id = readStringField(policy, 'id');
      const title = readNullableStringField(policy, 'title');
      const body = readNullableStringField(policy, 'body');
      const type = readStringField(policy, 'type');
      const url = readStringField(policy, 'url');
      const createdAt = readStringField(policy, 'createdAt');
      const updatedAt = readStringField(policy, 'updatedAt');

      return id && title !== null && body !== null && type && url && createdAt && updatedAt
        ? {
            id,
            title,
            body,
            type,
            url,
            createdAt,
            updatedAt,
          }
        : null;
    })
    .filter((policy): policy is ShopRecord['shopPolicies'][number] => policy !== null);

  const id = readStringField(source, 'id');
  const name = readStringField(source, 'name');
  const myshopifyDomain = readStringField(source, 'myshopifyDomain');
  const url = readStringField(source, 'url');
  const primaryDomainId = readStringField(primaryDomain, 'id');
  const primaryDomainHost = readStringField(primaryDomain, 'host');
  const primaryDomainUrl = readStringField(primaryDomain, 'url');
  const primaryDomainSslEnabled = readBooleanField(primaryDomain, 'sslEnabled');
  const contactEmail = readStringField(source, 'contactEmail');
  const email = readStringField(source, 'email');
  const currencyCode = readStringField(source, 'currencyCode');
  const ianaTimezone = readStringField(source, 'ianaTimezone');
  const timezoneAbbreviation = readStringField(source, 'timezoneAbbreviation');
  const timezoneOffset = readStringField(source, 'timezoneOffset');
  const timezoneOffsetMinutes = readNumberField(source, 'timezoneOffsetMinutes');
  const taxesIncluded = readBooleanField(source, 'taxesIncluded');
  const taxShipping = readBooleanField(source, 'taxShipping');
  const unitSystem = readStringField(source, 'unitSystem');
  const weightUnit = readStringField(source, 'weightUnit');
  const shopAddressId = readStringField(shopAddress, 'id');
  const coordinatesValidated = readBooleanField(shopAddress, 'coordinatesValidated');
  const planPartnerDevelopment = readBooleanField(plan, 'partnerDevelopment');
  const planPublicDisplayName = readStringField(plan, 'publicDisplayName');
  const planShopifyPlus = readBooleanField(plan, 'shopifyPlus');
  const locationLimit = readNumberField(resourceLimits, 'locationLimit');
  const maxProductOptions = readNumberField(resourceLimits, 'maxProductOptions');
  const maxProductVariants = readNumberField(resourceLimits, 'maxProductVariants');
  const redirectLimitReached = readBooleanField(resourceLimits, 'redirectLimitReached');
  const avalaraAvatax = readBooleanField(features, 'avalaraAvatax');
  const branding = readStringField(features, 'branding');
  const eligibleForBundles = readBooleanField(bundles, 'eligibleForBundles');
  const sellsBundles = readBooleanField(bundles, 'sellsBundles');
  const captcha = readBooleanField(features, 'captcha');
  const expandOperation = readBooleanField(eligibleOperations, 'expandOperation');
  const mergeOperation = readBooleanField(eligibleOperations, 'mergeOperation');
  const updateOperation = readBooleanField(eligibleOperations, 'updateOperation');
  const dynamicRemarketing = readBooleanField(features, 'dynamicRemarketing');
  const eligibleForSubscriptionMigration = readBooleanField(features, 'eligibleForSubscriptionMigration');
  const eligibleForSubscriptions = readBooleanField(features, 'eligibleForSubscriptions');
  const giftCards = readBooleanField(features, 'giftCards');
  const harmonizedSystemCode = readBooleanField(features, 'harmonizedSystemCode');
  const legacySubscriptionGatewayEnabled = readBooleanField(features, 'legacySubscriptionGatewayEnabled');
  const liveView = readBooleanField(features, 'liveView');
  const paypalExpressSubscriptionGatewayStatus = readStringField(features, 'paypalExpressSubscriptionGatewayStatus');
  const reports = readBooleanField(features, 'reports');
  const sellsSubscriptions = readBooleanField(features, 'sellsSubscriptions');
  const showMetrics = readBooleanField(features, 'showMetrics');
  const storefront = readBooleanField(features, 'storefront');
  const unifiedMarkets = readBooleanField(features, 'unifiedMarkets');

  if (
    !id ||
    !name ||
    !myshopifyDomain ||
    !url ||
    !primaryDomainId ||
    !primaryDomainHost ||
    !primaryDomainUrl ||
    primaryDomainSslEnabled === null ||
    !contactEmail ||
    !email ||
    !currencyCode ||
    !ianaTimezone ||
    !timezoneAbbreviation ||
    !timezoneOffset ||
    timezoneOffsetMinutes === null ||
    taxesIncluded === null ||
    taxShipping === null ||
    !unitSystem ||
    !weightUnit ||
    !shopAddressId ||
    coordinatesValidated === null ||
    planPartnerDevelopment === null ||
    !planPublicDisplayName ||
    planShopifyPlus === null ||
    locationLimit === null ||
    maxProductOptions === null ||
    maxProductVariants === null ||
    redirectLimitReached === null ||
    avalaraAvatax === null ||
    !branding ||
    eligibleForBundles === null ||
    sellsBundles === null ||
    captcha === null ||
    expandOperation === null ||
    mergeOperation === null ||
    updateOperation === null ||
    dynamicRemarketing === null ||
    eligibleForSubscriptionMigration === null ||
    eligibleForSubscriptions === null ||
    giftCards === null ||
    harmonizedSystemCode === null ||
    legacySubscriptionGatewayEnabled === null ||
    liveView === null ||
    !paypalExpressSubscriptionGatewayStatus ||
    reports === null ||
    sellsSubscriptions === null ||
    showMetrics === null ||
    storefront === null ||
    unifiedMarkets === null
  ) {
    return null;
  }

  return {
    id,
    name,
    myshopifyDomain,
    url,
    primaryDomain: {
      id: primaryDomainId,
      host: primaryDomainHost,
      url: primaryDomainUrl,
      sslEnabled: primaryDomainSslEnabled,
    },
    contactEmail,
    email,
    currencyCode,
    enabledPresentmentCurrencies: readStringArrayField(source, 'enabledPresentmentCurrencies'),
    ianaTimezone,
    timezoneAbbreviation,
    timezoneOffset,
    timezoneOffsetMinutes,
    taxesIncluded,
    taxShipping,
    unitSystem,
    weightUnit,
    shopAddress: {
      id: shopAddressId,
      address1: readNullableStringField(shopAddress, 'address1'),
      address2: readNullableStringField(shopAddress, 'address2'),
      city: readNullableStringField(shopAddress, 'city'),
      company: readNullableStringField(shopAddress, 'company'),
      coordinatesValidated,
      country: readNullableStringField(shopAddress, 'country'),
      countryCodeV2: readNullableStringField(shopAddress, 'countryCodeV2'),
      formatted: readStringArrayField(shopAddress, 'formatted'),
      formattedArea: readNullableStringField(shopAddress, 'formattedArea'),
      latitude: readNumberField(shopAddress, 'latitude'),
      longitude: readNumberField(shopAddress, 'longitude'),
      phone: readNullableStringField(shopAddress, 'phone'),
      province: readNullableStringField(shopAddress, 'province'),
      provinceCode: readNullableStringField(shopAddress, 'provinceCode'),
      zip: readNullableStringField(shopAddress, 'zip'),
    },
    plan: {
      partnerDevelopment: planPartnerDevelopment,
      publicDisplayName: planPublicDisplayName,
      shopifyPlus: planShopifyPlus,
    },
    resourceLimits: {
      locationLimit,
      maxProductOptions,
      maxProductVariants,
      redirectLimitReached,
    },
    features: {
      avalaraAvatax,
      branding,
      bundles: {
        eligibleForBundles,
        ineligibilityReason: readNullableStringField(bundles, 'ineligibilityReason'),
        sellsBundles,
      },
      captcha,
      cartTransform: {
        eligibleOperations: {
          expandOperation,
          mergeOperation,
          updateOperation,
        },
      },
      dynamicRemarketing,
      eligibleForSubscriptionMigration,
      eligibleForSubscriptions,
      giftCards,
      harmonizedSystemCode,
      legacySubscriptionGatewayEnabled,
      liveView,
      paypalExpressSubscriptionGatewayStatus,
      reports,
      sellsSubscriptions,
      showMetrics,
      storefront,
      unifiedMarkets,
    },
    paymentSettings: {
      supportedDigitalWallets: readStringArrayField(paymentSettings, 'supportedDigitalWallets'),
    },
    shopPolicies: policies,
  };
}

function seedShopPreconditions(capture: unknown): boolean {
  const captureRoot = isPlainObject(capture) ? capture : {};
  const directData = readRecordField(captureRoot, 'data');
  const shopBaseline = readRecordField(readRecordField(captureRoot, 'readOnlyBaselines'), 'shop');
  const baselineData = readRecordField(shopBaseline, 'data');
  const shop = readShopRecord(readRecordField(directData ?? baselineData, 'shop'));

  if (!shop) {
    return false;
  }

  store.upsertBaseShop(shop);
  return true;
}

function seedBusinessEntityPreconditions(capture: unknown): boolean {
  const data = readRecordField(capture as Record<string, unknown>, 'data');
  const catalogEntities = readArrayField(data, 'businessEntities');
  const fallbackEntities = [readRecordField(data, 'primary'), readRecordField(data, 'known')];
  const rawEntities =
    catalogEntities.length > 0
      ? catalogEntities
      : fallbackEntities.filter((entity): entity is Record<string, unknown> => entity !== null);
  const businessEntities = rawEntities
    .filter(isPlainObject)
    .map(readBusinessEntityRecord)
    .filter((entity): entity is BusinessEntityRecord => entity !== null);

  if (businessEntities.length === 0) {
    return false;
  }

  store.upsertBaseBusinessEntities(businessEntities);
  return true;
}

function readCapturedOrderMetafields(orderId: string, order: Record<string, unknown> | null): OrderMetafieldRecord[] {
  const byIdentity = new Map<string, OrderMetafieldRecord>();
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
      orderId,
      namespace,
      key,
      type: readStringField(candidate, 'type'),
      value: readStringField(candidate, 'value'),
    });
  };

  for (const value of Object.values(order ?? {})) {
    addMetafield(value);
  }

  const metafieldsConnection = readRecordField(order, 'metafields');
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

function readCapturedOrderTransactions(order: Record<string, unknown> | null): OrderRecord['transactions'] {
  return readArrayField(order, 'transactions')
    .filter(isPlainObject)
    .map((transaction, index) => ({
      id: readStringField(transaction, 'id') ?? `gid://shopify/OrderTransaction/conformance-${index}`,
      kind: readStringField(transaction, 'kind'),
      status: readStringField(transaction, 'status'),
      gateway: readStringField(transaction, 'gateway'),
      amountSet: readMoneySetField(transaction, 'amountSet'),
    }));
}

function readCapturedFulfillmentLineItems(source: Record<string, unknown> | null): OrderFulfillmentLineItemRecord[] {
  return readArrayField(readRecordField(source, 'fulfillmentLineItems'), 'nodes')
    .filter(isPlainObject)
    .map((fulfillmentLineItem, index) => {
      const lineItem = readRecordField(fulfillmentLineItem, 'lineItem');
      return {
        id: readStringField(fulfillmentLineItem, 'id') ?? `gid://shopify/FulfillmentLineItem/conformance-${index}`,
        lineItemId: readStringField(lineItem, 'id'),
        title: readStringField(lineItem, 'title'),
        quantity: readNumberField(fulfillmentLineItem, 'quantity') ?? 0,
      };
    });
}

function readCapturedOrderFulfillments(order: Record<string, unknown> | null): OrderFulfillmentRecord[] {
  return readArrayField(order, 'fulfillments')
    .filter(isPlainObject)
    .map((fulfillment, index) => ({
      id: readStringField(fulfillment, 'id') ?? `gid://shopify/Fulfillment/conformance-${index}`,
      status: readStringField(fulfillment, 'status'),
      displayStatus: readStringField(fulfillment, 'displayStatus'),
      createdAt: readStringField(fulfillment, 'createdAt'),
      updatedAt: readStringField(fulfillment, 'updatedAt'),
      trackingInfo: readArrayField(fulfillment, 'trackingInfo')
        .filter(isPlainObject)
        .map((trackingInfo) => ({
          number: readStringField(trackingInfo, 'number'),
          url: readStringField(trackingInfo, 'url'),
          company: readStringField(trackingInfo, 'company'),
        })),
      fulfillmentLineItems: readCapturedFulfillmentLineItems(fulfillment),
    }));
}

function readFulfillmentPayloadFromSetup(capture: unknown, pathName: string): Record<string, unknown> | null {
  return readRecordField(
    readRecordField(
      readRecordField(
        readRecordField(
          readRecordField(readRecordField(capture as Record<string, unknown>, 'setup'), pathName),
          'response',
        ),
        'data',
      ),
      pathName,
    ),
    'fulfillment',
  );
}

function seedFulfillmentLifecyclePreconditions(capture: unknown, mutationName: string | null): boolean {
  if (mutationName !== 'fulfillmentTrackingInfoUpdate' && mutationName !== 'fulfillmentCancel') {
    return false;
  }

  const setup = readRecordField(capture as Record<string, unknown>, 'setup');
  const candidate = readRecordField(setup, 'candidate');
  const orderSource = readRecordField(candidate, 'order');
  const orderId = readStringField(orderSource, 'id');
  if (!orderId) {
    return false;
  }

  const createFulfillment = readFulfillmentPayloadFromSetup(capture, 'fulfillmentCreate');
  const updateFulfillment = readFulfillmentPayloadFromSetup(capture, 'fulfillmentTrackingInfoUpdate');
  const seedFulfillmentSource =
    mutationName === 'fulfillmentCancel' && updateFulfillment
      ? {
          ...createFulfillment,
          ...updateFulfillment,
          fulfillmentLineItems:
            readRecordField(updateFulfillment, 'fulfillmentLineItems') ??
            readRecordField(createFulfillment, 'fulfillmentLineItems'),
        }
      : createFulfillment;
  const seedFulfillment = readCapturedOrderFulfillments({ fulfillments: [seedFulfillmentSource] })[0];
  if (!seedFulfillment) {
    return false;
  }

  const order = makeSeedOrder(orderId, orderSource);
  order.fulfillments = [
    seedFulfillment,
    ...(order.fulfillments ?? []).filter((fulfillment) => fulfillment.id !== seedFulfillment.id),
  ];
  store.upsertBaseOrders([order]);
  return true;
}

function readCapturedFulfillmentOrderLineItems(
  source: Record<string, unknown> | null,
): OrderFulfillmentOrderLineItemRecord[] {
  return readArrayField(readRecordField(source, 'lineItems'), 'nodes')
    .filter(isPlainObject)
    .map((fulfillmentOrderLineItem, index) => {
      const lineItem = readRecordField(fulfillmentOrderLineItem, 'lineItem');
      return {
        id:
          readStringField(fulfillmentOrderLineItem, 'id') ??
          `gid://shopify/FulfillmentOrderLineItem/conformance-${index}`,
        lineItemId: readStringField(lineItem, 'id'),
        title: readStringField(lineItem, 'title'),
        totalQuantity: readNumberField(fulfillmentOrderLineItem, 'totalQuantity') ?? 0,
        remainingQuantity: readNumberField(fulfillmentOrderLineItem, 'remainingQuantity') ?? 0,
      };
    });
}

function readCapturedOrderFulfillmentOrders(order: Record<string, unknown> | null): OrderFulfillmentOrderRecord[] {
  return readArrayField(readRecordField(order, 'fulfillmentOrders'), 'nodes')
    .filter(isPlainObject)
    .map((fulfillmentOrder, index) => ({
      id: readStringField(fulfillmentOrder, 'id') ?? `gid://shopify/FulfillmentOrder/conformance-${index}`,
      status: readStringField(fulfillmentOrder, 'status'),
      requestStatus: readStringField(fulfillmentOrder, 'requestStatus'),
      assignedLocation: readRecordField(fulfillmentOrder, 'assignedLocation')
        ? {
            name: readStringField(readRecordField(fulfillmentOrder, 'assignedLocation'), 'name'),
          }
        : null,
      lineItems: readCapturedFulfillmentOrderLineItems(fulfillmentOrder),
    }));
}

function readCapturedOrderRefundLineItems(
  source: Record<string, unknown> | null,
): OrderRecord['refunds'][number]['refundLineItems'] {
  return readArrayField(readRecordField(source, 'refundLineItems'), 'nodes')
    .filter(isPlainObject)
    .map((refundLineItem, index) => {
      const lineItem = readRecordField(refundLineItem, 'lineItem');
      return {
        id: readStringField(refundLineItem, 'id') ?? `gid://shopify/RefundLineItem/conformance-${index}`,
        lineItemId: readStringField(lineItem, 'id') ?? `gid://shopify/LineItem/conformance-${index}`,
        title: readStringField(lineItem, 'title'),
        quantity: readNumberField(refundLineItem, 'quantity') ?? 0,
        restockType: readStringField(refundLineItem, 'restockType'),
        subtotalSet: readMoneySetField(refundLineItem, 'subtotalSet'),
      };
    });
}

function readCapturedOrderRefunds(order: Record<string, unknown> | null): OrderRecord['refunds'] {
  return readArrayField(order, 'refunds')
    .filter(isPlainObject)
    .map((refund, index) => ({
      id: readStringField(refund, 'id') ?? `gid://shopify/Refund/conformance-${index}`,
      note: readStringField(refund, 'note'),
      createdAt: readStringField(refund, 'createdAt') ?? '2026-04-19T00:00:00.000Z',
      updatedAt:
        readStringField(refund, 'updatedAt') ?? readStringField(refund, 'createdAt') ?? '2026-04-19T00:00:00.000Z',
      totalRefundedSet: readMoneySetField(refund, 'totalRefundedSet'),
      refundLineItems: readCapturedOrderRefundLineItems(refund),
      transactions: readCapturedOrderTransactions(refund),
    }));
}

function readCapturedOrderReturns(order: Record<string, unknown> | null): OrderRecord['returns'] {
  return readArrayField(readRecordField(order, 'returns'), 'nodes')
    .filter(isPlainObject)
    .map((orderReturn, index) => ({
      id: readStringField(orderReturn, 'id') ?? `gid://shopify/Return/conformance-${index}`,
      status: readStringField(orderReturn, 'status'),
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
    email: readStringField(source, 'email'),
    phone: readStringField(source, 'phone'),
    poNumber: readStringField(source, 'poNumber'),
    closed: readBooleanField(source, 'closed') ?? false,
    closedAt: readStringField(source, 'closedAt'),
    cancelledAt: readStringField(source, 'cancelledAt'),
    cancelReason: readStringField(source, 'cancelReason'),
    displayFinancialStatus: readStringField(source, 'displayFinancialStatus'),
    displayFulfillmentStatus: readStringField(source, 'displayFulfillmentStatus'),
    paymentGatewayNames: readArrayField(source, 'paymentGatewayNames').filter(
      (gateway): gateway is string => typeof gateway === 'string',
    ),
    note: readStringField(source, 'note'),
    tags: readArrayField(source, 'tags').filter((tag): tag is string => typeof tag === 'string'),
    customAttributes: readArrayField(source, 'customAttributes')
      .filter(isPlainObject)
      .map((attribute) => ({
        key: readStringField(attribute, 'key') ?? '',
        value: readStringField(attribute, 'value'),
      }))
      .filter((attribute) => attribute.key.length > 0),
    metafields: readCapturedOrderMetafields(orderId, source),
    billingAddress: readCapturedAddress(source, 'billingAddress'),
    shippingAddress: readCapturedAddress(source, 'shippingAddress'),
    subtotalPriceSet,
    currentSubtotalPriceSet: readMoneySetField(source, 'currentSubtotalPriceSet'),
    currentTotalPriceSet,
    currentTotalDiscountsSet: readMoneySetField(source, 'currentTotalDiscountsSet'),
    currentTotalTaxSet: readMoneySetField(source, 'currentTotalTaxSet'),
    totalPriceSet,
    totalOutstandingSet: readMoneySetField(source, 'totalOutstandingSet'),
    totalReceivedSet: readMoneySetField(source, 'totalReceivedSet'),
    netPaymentSet: readMoneySetField(source, 'netPaymentSet'),
    totalRefundedSet: readMoneySetField(source, 'totalRefundedSet') ?? {
      shopMoney: {
        amount: '0.0',
        currencyCode,
      },
    },
    totalRefundedShippingSet: readMoneySetField(source, 'totalRefundedShippingSet'),
    totalShippingPriceSet: readMoneySetField(source, 'totalShippingPriceSet'),
    totalTaxSet: readMoneySetField(source, 'totalTaxSet'),
    totalDiscountsSet: readMoneySetField(source, 'totalDiscountsSet'),
    discountCodes: readArrayField(source, 'discountCodes').filter(
      (discountCode): discountCode is string => typeof discountCode === 'string',
    ),
    taxLines: readCapturedOrderTaxLines(source),
    taxesIncluded: readBooleanField(source, 'taxesIncluded'),
    customer: readCapturedOrderCustomer(source),
    shippingLines: readCapturedOrderShippingLines(source),
    lineItems: readCapturedOrderLineItems(source),
    fulfillments: readCapturedOrderFulfillments(source),
    fulfillmentOrders: readCapturedOrderFulfillmentOrders(source),
    transactions: readCapturedOrderTransactions(source),
    refunds: readCapturedOrderRefunds(source),
    returns: readCapturedOrderReturns(source),
  };
}

function readCapturedDraftOrderLineItems(draftOrder: Record<string, unknown> | null): DraftOrderLineItemRecord[] {
  return readArrayField(readRecordField(draftOrder, 'lineItems'), 'nodes')
    .filter(isPlainObject)
    .map((lineItem, index) => {
      const title = readStringField(lineItem, 'title');
      return {
        id: readStringField(lineItem, 'id') ?? `gid://shopify/DraftOrderLineItem/conformance-${index}`,
        title,
        name: readStringField(lineItem, 'name') ?? title,
        quantity: readNumberField(lineItem, 'quantity') ?? 0,
        sku: typeof lineItem['sku'] === 'string' ? lineItem['sku'] : null,
        variantTitle:
          readStringField(lineItem, 'variantTitle') ?? readStringField(readRecordField(lineItem, 'variant'), 'title'),
        variantId: readStringField(readRecordField(lineItem, 'variant'), 'id'),
        productId: null,
        custom: readBooleanField(lineItem, 'custom') ?? true,
        requiresShipping: readBooleanField(lineItem, 'requiresShipping') ?? true,
        taxable: readBooleanField(lineItem, 'taxable') ?? true,
        customAttributes: readArrayField(lineItem, 'customAttributes')
          .filter(isPlainObject)
          .map((attribute) => ({
            key: readStringField(attribute, 'key') ?? '',
            value: readStringField(attribute, 'value'),
          }))
          .filter((attribute) => attribute.key.length > 0),
        appliedDiscount: readCapturedDraftOrderAppliedDiscount(lineItem),
        originalUnitPriceSet: readMoneySetField(lineItem, 'originalUnitPriceSet'),
        originalTotalSet: readMoneySetField(lineItem, 'originalTotalSet'),
        discountedTotalSet: readMoneySetField(lineItem, 'discountedTotalSet'),
        totalDiscountSet: readMoneySetField(lineItem, 'totalDiscountSet'),
      };
    });
}

function readCapturedDraftOrderAppliedDiscount(
  source: Record<string, unknown> | null,
): DraftOrderRecord['appliedDiscount'] {
  const appliedDiscount = readRecordField(source, 'appliedDiscount');
  if (!appliedDiscount) {
    return null;
  }

  return {
    title: readStringField(appliedDiscount, 'title'),
    description: readStringField(appliedDiscount, 'description'),
    value: readNumberField(appliedDiscount, 'value'),
    valueType: readStringField(appliedDiscount, 'valueType'),
    amountSet: readMoneySetField(appliedDiscount, 'amountSet'),
  };
}

function readCapturedDraftOrderCustomer(source: Record<string, unknown> | null): DraftOrderRecord['customer'] {
  const customer = readRecordField(source, 'customer');
  const id = readStringField(customer, 'id');
  if (!id) {
    return null;
  }

  return {
    id,
    email: readStringField(customer, 'email'),
    displayName: readStringField(customer, 'displayName'),
  };
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
    customer: readCapturedDraftOrderCustomer(source),
    taxExempt: readBooleanField(source, 'taxExempt') ?? false,
    taxesIncluded: readBooleanField(source, 'taxesIncluded') ?? false,
    reserveInventoryUntil: readStringField(source, 'reserveInventoryUntil'),
    paymentTerms: null,
    appliedDiscount: readCapturedDraftOrderAppliedDiscount(source),
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
    totalDiscountsSet: readMoneySetField(source, 'totalDiscountsSet'),
    totalShippingPriceSet: readMoneySetField(source, 'totalShippingPriceSet'),
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
    address2: readStringField(address, 'address2'),
    company: readStringField(address, 'company'),
    city: readStringField(address, 'city'),
    province: readStringField(address, 'province'),
    provinceCode: readStringField(address, 'provinceCode'),
    country: readStringField(address, 'country'),
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

  hydrateOrderConnectionsFromData(data);

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

function hydrateOrderConnectionsFromData(data: Record<string, unknown> | null): void {
  for (const value of Object.values(data ?? {})) {
    const connection = isPlainObject(value) ? value : null;
    const edges = readArrayField(connection, 'edges').filter(isPlainObject);
    const nodes = readArrayField(connection, 'nodes').filter(isPlainObject);
    const edgeNodes = edges.map((edge) => readRecordField(edge, 'node')).filter(isPlainObject);

    for (const node of [...edgeNodes, ...nodes]) {
      const nodeId = readStringField(node, 'id');
      if (nodeId?.startsWith('gid://shopify/Order/')) {
        const existingOrder = store.getOrderById(nodeId);
        store.upsertBaseOrders([makeSeedOrder(nodeId, existingOrder ? { ...existingOrder, ...node } : node)]);
      }
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

function stripCapturedHtml(value: string): string {
  return value
    .replace(/<[^>]*>/gu, '')
    .replace(/\s+/gu, ' ')
    .trim();
}

function makeSeedCollection(collectionId: string, source: Record<string, unknown> | null = null): CollectionRecord {
  const rawSeo = readRecordField(source, 'seo');
  const rawImage = readRecordField(source, 'image');
  const rawRuleSet = readRecordField(source, 'ruleSet');
  const descriptionHtml = readStringField(source, 'descriptionHtml');
  const rules = readArrayField(rawRuleSet, 'rules').filter(isPlainObject);

  return {
    id: collectionId,
    legacyResourceId: readStringField(source, 'legacyResourceId') ?? collectionId.split('/').at(-1) ?? null,
    title: readStringField(source, 'title') ?? 'Conformance seed collection',
    handle: readStringField(source, 'handle') ?? `conformance-seed-${collectionId.split('/').at(-1) ?? 'collection'}`,
    publicationIds: readArrayField(source, 'publicationIds').filter(
      (publicationId): publicationId is string => typeof publicationId === 'string',
    ),
    updatedAt: readStringField(source, 'updatedAt'),
    description:
      readStringField(source, 'description') ?? (descriptionHtml ? stripCapturedHtml(descriptionHtml) : null),
    descriptionHtml,
    image: rawImage
      ? {
          id: readStringField(rawImage, 'id'),
          altText: readStringField(rawImage, 'altText'),
          url:
            readStringField(rawImage, 'url') ??
            readStringField(rawImage, 'src') ??
            readStringField(rawImage, 'originalSrc') ??
            readStringField(rawImage, 'transformedSrc'),
          width: readNumberField(rawImage, 'width'),
          height: readNumberField(rawImage, 'height'),
        }
      : null,
    sortOrder: readStringField(source, 'sortOrder'),
    templateSuffix: readStringField(source, 'templateSuffix'),
    seo: {
      title: readStringField(rawSeo, 'title'),
      description: readStringField(rawSeo, 'description'),
    },
    ruleSet: rawRuleSet
      ? {
          appliedDisjunctively: readBooleanField(rawRuleSet, 'appliedDisjunctively') ?? false,
          rules: rules
            .map((rule) => {
              const column = readStringField(rule, 'column');
              const relation = readStringField(rule, 'relation');
              const condition = readStringField(rule, 'condition');
              return column && relation && condition !== null
                ? {
                    column,
                    relation,
                    condition,
                    conditionObjectId: readStringField(rule, 'conditionObjectId'),
                  }
                : null;
            })
            .filter((rule): rule is NonNullable<typeof rule> => rule !== null),
        }
      : null,
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
  for (const [position, node] of productNodes.filter(isPlainObject).entries()) {
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
      position,
    });
  }
  for (const membership of collectionMemberships) {
    store.replaceBaseCollectionsForProduct(membership.productId, [membership]);
  }
}

function seedPreexistingProductCollectionsFromReadPayload(source: unknown, stagedCollectionId: string): void {
  const data = readRecordField(isPlainObject(source) ? source : null, 'data');
  if (!data) {
    return;
  }

  for (const value of Object.values(data)) {
    if (!isPlainObject(value)) {
      continue;
    }
    const productId = readStringField(value, 'id');
    if (!productId?.startsWith('gid://shopify/Product/')) {
      continue;
    }

    const memberships = [...store.getEffectiveCollectionsByProductId(productId)];
    for (const node of readArrayField(readRecordField(value, 'collections'), 'nodes').filter(isPlainObject)) {
      const collectionId = readStringField(node, 'id');
      if (!collectionId?.startsWith('gid://shopify/Collection/') || collectionId === stagedCollectionId) {
        continue;
      }

      const collection = makeSeedCollection(collectionId, node);
      store.upsertBaseCollections([collection]);
      if (!memberships.some((membership) => membership.id === collectionId)) {
        memberships.push({
          id: collection.id,
          productId,
          title: collection.title,
          handle: collection.handle,
        });
      }
    }

    if (memberships.length > 0) {
      store.replaceBaseCollectionsForProduct(productId, memberships);
    }
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

function seedFileDeleteMediaReferencePreconditions(capture: unknown, variables: Record<string, unknown>): boolean {
  if (mutationNameFromCapture(capture) !== 'fileDelete') {
    return false;
  }

  const productRead = readRecordField(
    readRecordField(capture as Record<string, unknown>, 'setup'),
    'productReadBeforeDelete',
  );
  const product = readRecordField(readRecordField(productRead, 'data'), 'product');
  const productId = readStringField(product, 'id');
  if (!productId?.startsWith('gid://shopify/Product/')) {
    return false;
  }

  const fileIds = new Set(
    readArrayField(variables, 'fileIds').filter((fileId): fileId is string => typeof fileId === 'string'),
  );
  if (fileIds.size === 0) {
    return false;
  }

  const capturedMedia = readCapturedProductMedia(productId, product).filter(
    (mediaRecord) => typeof mediaRecord.id === 'string' && fileIds.has(mediaRecord.id),
  );
  if (capturedMedia.length === 0) {
    return false;
  }

  store.upsertBaseProducts([makeSeedProduct(productId, product)]);
  store.replaceBaseMediaForProduct(productId, capturedMedia);
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
  if (seedFulfillmentLifecyclePreconditions(capture, mutationName)) {
    return;
  }

  if (seedCustomerMutationPreconditions(capture, variables, mutationName, payload)) {
    return;
  }

  if (seedCustomerByIdentifierPreconditions(capture)) {
    return;
  }

  if (seedShopPreconditions(capture)) {
    return;
  }

  if (seedBusinessEntityPreconditions(capture)) {
    return;
  }

  const readOrderPayload =
    readRecordField(
      readRecordField(readRecordField(capture as Record<string, unknown>, 'response'), 'data'),
      'order',
    ) ?? readRecordField(readRecordField(capture as Record<string, unknown>, 'data'), 'order');
  const readOrderId = readStringField(readOrderPayload, 'id') ?? readStringField(variables, 'id');
  if (!mutationName && readOrderPayload && readOrderId) {
    store.upsertBaseOrders([makeSeedOrder(readOrderId, readOrderPayload)]);
    return;
  }
  if (!mutationName && (capture as Record<string, unknown>)['seedOrderCatalogFromCapture'] === true) {
    const responsePayload = readRecordField(capture as Record<string, unknown>, 'response');
    if (responsePayload) {
      hydrateOrdersFromUpstreamResponse(responsePayload);
    }
    const nextPageResponse = readRecordField(
      readRecordField(capture as Record<string, unknown>, 'nextPage'),
      'response',
    );
    if (nextPageResponse) {
      hydrateOrdersFromUpstreamResponse(nextPageResponse);
    }
  }

  if (
    mutationName === 'orderEditBegin' ||
    mutationName === 'orderEditAddVariant' ||
    mutationName === 'orderEditSetQuantity' ||
    mutationName === 'orderEditCommit'
  ) {
    const setupPreReadOrder = readRecordField(
      readRecordField(
        readRecordField(readRecordField(capture as Record<string, unknown>, 'setup'), 'preRead'),
        'response',
      ),
      'data',
    )?.['order'];
    const seedOrder: Record<string, unknown> | null =
      readRecordField(capture as Record<string, unknown>, 'seedOrder') ??
      (isPlainObject(setupPreReadOrder) ? setupPreReadOrder : null);
    const seedOrderId = readStringField(seedOrder, 'id') ?? readStringField(variables, 'id');
    if (seedOrder && seedOrderId) {
      store.upsertBaseOrders([makeSeedOrder(seedOrderId, seedOrder)]);
    }
    const seedProducts = readArrayField(capture as Record<string, unknown>, 'seedProducts').filter(isPlainObject);
    for (const seedProduct of seedProducts) {
      const productId = readStringField(seedProduct, 'id');
      if (!productId?.startsWith('gid://shopify/Product/')) {
        continue;
      }
      store.upsertBaseProducts([makeSeedProduct(productId, seedProduct)]);
      const variants = readCapturedProductVariants(productId, seedProduct);
      if (variants.length > 0) {
        store.replaceBaseVariantsForProduct(productId, variants);
      }
    }
  }

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

  if (mutationName === 'draftOrderCreate') {
    const draftOrderPayload = readRecordField(payload, 'draftOrder');
    const customerPayload = readRecordField(draftOrderPayload, 'customer');
    const customerId = readStringField(customerPayload, 'id');
    if (customerId) {
      store.upsertBaseCustomers([makeSeedCustomer(customerId, customerPayload)]);
    }

    for (const lineItem of readArrayField(readRecordField(draftOrderPayload, 'lineItems'), 'nodes').filter(
      isPlainObject,
    )) {
      const variant = readRecordField(lineItem, 'variant');
      const variantId = readStringField(variant, 'id');
      if (!variantId) {
        continue;
      }

      const variantResourceId = variantId.split('/').at(-1) ?? '0';
      const productId = `gid://shopify/Product/${variantResourceId}`;
      const productTitle = readStringField(lineItem, 'title') ?? 'Conformance draft-order product';
      store.upsertBaseProducts([
        makeSeedProduct(productId, {
          id: productId,
          title: productTitle,
        }),
      ]);
      store.replaceBaseVariantsForProduct(productId, [
        {
          id: variantId,
          productId,
          title: readStringField(variant, 'title') ?? 'Default Title',
          sku: readStringField(variant, 'sku'),
          barcode: null,
          price: readStringField(
            readRecordField(readRecordField(lineItem, 'originalUnitPriceSet'), 'shopMoney'),
            'amount',
          ),
          compareAtPrice: null,
          taxable: readBooleanField(lineItem, 'taxable'),
          inventoryPolicy: null,
          inventoryQuantity: null,
          selectedOptions: [],
          inventoryItem: {
            id: `gid://shopify/InventoryItem/${variantResourceId}`,
            tracked: null,
            requiresShipping: readBooleanField(lineItem, 'requiresShipping'),
            measurement: null,
            countryCodeOfOrigin: null,
            provinceCodeOfOrigin: null,
            harmonizedSystemCode: null,
            inventoryLevels: [],
          },
        },
      ]);
    }
    return;
  }

  if (mutationName === 'draftOrderComplete') {
    const draftOrderId = readStringField(variables, 'id');
    if (draftOrderId) {
      const setupDraftOrder = readRecordField(
        readRecordField(
          readRecordField(
            readRecordField(
              readRecordField(readRecordField(capture as Record<string, unknown>, 'setup'), 'draftOrderCreate'),
              'mutation',
            ),
            'response',
          ),
          'data',
        ),
        'draftOrderCreate',
      );
      const setupSource = readRecordField(setupDraftOrder, 'draftOrder');
      const completedSource = readRecordField(payload, 'draftOrder');
      const setupInput = readRecordField(
        readRecordField(readRecordField(capture as Record<string, unknown>, 'setup'), 'draftOrderCreate'),
        'variables',
      )?.['input'];
      const seedDraftOrder = makeSeedDraftOrder(draftOrderId, setupSource ?? completedSource);
      if (!seedDraftOrder.note && isPlainObject(setupInput)) {
        seedDraftOrder.note = readStringField(setupInput, 'note');
      }
      store.stageCreateDraftOrder(seedDraftOrder);
    }
    return;
  }

  if (
    mutationName === 'draftOrderUpdate' ||
    mutationName === 'draftOrderDuplicate' ||
    mutationName === 'draftOrderDelete'
  ) {
    const draftOrderId = readStringField(variables, 'id') ?? readStringField(readRecordField(variables, 'input'), 'id');
    if (draftOrderId) {
      const setupDraftOrder = readRecordField(
        readRecordField(
          readRecordField(
            readRecordField(
              readRecordField(readRecordField(capture as Record<string, unknown>, 'setup'), 'draftOrderCreate'),
              'mutation',
            ),
            'response',
          ),
          'data',
        ),
        'draftOrderCreate',
      );
      const setupSource = readRecordField(setupDraftOrder, 'draftOrder');
      if (setupSource) {
        store.stageCreateDraftOrder(makeSeedDraftOrder(draftOrderId, setupSource));
      }
    }
    return;
  }

  if (mutationName === 'draftOrderCreateFromOrder') {
    const orderId = readStringField(variables, 'orderId');
    if (orderId) {
      const setupDraftOrder = readRecordField(
        readRecordField(
          readRecordField(
            readRecordField(
              readRecordField(readRecordField(capture as Record<string, unknown>, 'setup'), 'draftOrderCreate'),
              'mutation',
            ),
            'response',
          ),
          'data',
        ),
        'draftOrderCreate',
      );
      const setupDraftOrderSource = readRecordField(setupDraftOrder, 'draftOrder');
      const completedDraftOrder = readRecordField(
        readRecordField(
          readRecordField(
            readRecordField(
              readRecordField(readRecordField(capture as Record<string, unknown>, 'setup'), 'draftOrderComplete'),
              'mutation',
            ),
            'response',
          ),
          'data',
        ),
        'draftOrderComplete',
      );
      const orderSource =
        setupDraftOrderSource ??
        readRecordField(readRecordField(completedDraftOrder, 'draftOrder'), 'order') ??
        readRecordField(
          readRecordField(
            readRecordField(readRecordField(capture as Record<string, unknown>, 'setup'), 'downstreamOrderRead'),
            'response',
          ),
          'data',
        )?.['order'];
      if (isPlainObject(orderSource)) {
        store.upsertBaseOrders([makeSeedOrder(orderId, orderSource)]);
      }
    }
    return;
  }

  if (
    mutationName === 'orderClose' ||
    mutationName === 'orderOpen' ||
    mutationName === 'orderMarkAsPaid' ||
    mutationName === 'orderCustomerSet' ||
    mutationName === 'orderCustomerRemove' ||
    mutationName === 'orderInvoiceSend'
  ) {
    const orderPayload = readRecordField(payload, 'order');
    const input = readRecordField(variables, 'input');
    const orderId =
      readStringField(input, 'id') ?? readStringField(variables, 'orderId') ?? readStringField(variables, 'id');
    const seedId = readStringField(orderPayload, 'id') ?? orderId;
    if (seedId) {
      store.upsertBaseOrders([makeSeedOrder(seedId, orderPayload)]);
    }
    return;
  }

  if (mutationName === 'orderCancel') {
    const downstreamOrder = readRecordField(
      readRecordField(readRecordField(capture as Record<string, unknown>, 'downstreamRead'), 'response'),
      'data',
    );
    const orderPayload = readRecordField(downstreamOrder, 'order');
    const orderId = readStringField(variables, 'orderId') ?? readStringField(orderPayload, 'id');
    if (orderId) {
      store.upsertBaseOrders([makeSeedOrder(orderId, orderPayload)]);
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

  if (seedFileDeleteMediaReferencePreconditions(capture, variables)) {
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
  const productDeletePayloadId = mutationName === 'productDelete' ? readStringField(payload, 'deletedProductId') : null;
  const isProductDeleteValidationProbe =
    mutationName === 'productDelete' && productDeletePayloadId !== null && productDeletePayloadId !== productId;
  const productUserErrors = readArrayField(payload, 'userErrors').filter(isPlainObject);
  const isMissingProductValidationProbe =
    (mutationName === 'productUpdate' || mutationName === 'productChangeStatus') &&
    productPayload === null &&
    productUserErrors.some((userError) => {
      const fieldPath = readArrayField(userError, 'field');
      return (
        (fieldPath.includes('id') || fieldPath.includes('productId')) &&
        readStringField(userError, 'message') === 'Product does not exist'
      );
    });

  const shouldSeedProduct =
    productId !== null &&
    !(mutationName === 'productCreate' && readStringField(productInput, 'id') === null) &&
    !isProductSetCreate &&
    !isProductDeleteValidationProbe &&
    !isMissingProductValidationProbe;

  if (seedProductDuplicateSource(capture)) {
    return;
  }

  if (shouldSeedProduct) {
    if (mutationName === 'tagsRemove' && seedTagsRemovePreconditions(productId, productPayload, capture, variables)) {
      return;
    }

    const captureSeedProduct = readRecordField(capture as Record<string, unknown>, 'seedProduct');
    const seedSource =
      mutationName === 'tagsAdd'
        ? null
        : readStringField(captureSeedProduct, 'id') === productId
          ? captureSeedProduct
          : (productPayload ?? productInput);
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

  const collectionPayload =
    readRecordField(payload, 'collection') ??
    (readStringField(readRecordField(payload, 'publishable'), 'id')?.startsWith('gid://shopify/Collection/')
      ? readRecordField(payload, 'publishable')
      : null);
  const initialCollectionPayload = readRecordField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'initialCollectionRead'), 'data'),
    'collection',
  );
  const rawCollectionId =
    readStringField(variables, 'id') ??
    readStringField(input, 'id') ??
    readStringField(collectionPayload, 'id') ??
    readStringField(initialCollectionPayload, 'id');
  const collectionId = rawCollectionId?.startsWith('gid://shopify/Collection/') ? rawCollectionId : null;
  if (collectionId) {
    const collection = makeSeedCollection(collectionId, collectionPayload ?? initialCollectionPayload);
    store.upsertBaseCollections([collection]);
    const seedProducts = readArrayField(capture as Record<string, unknown>, 'seedProducts').filter(isPlainObject);
    for (const seedProduct of seedProducts) {
      const productId = readStringField(seedProduct, 'id');
      if (productId?.startsWith('gid://shopify/Product/')) {
        store.upsertBaseProducts([makeSeedProduct(productId, seedProduct)]);
      }
    }
    const rawProductNodes = readRecordField(collectionPayload, 'products')?.['nodes'];
    const productNodes = Array.isArray(rawProductNodes) ? rawProductNodes : [];
    const initialProductNodes = readArrayField(readRecordField(initialCollectionPayload, 'products'), 'nodes');
    if (mutationName === 'collectionReorderProducts') {
      seedCollectionProducts(collection, initialProductNodes);
    } else if (mutationName === 'collectionUpdate') {
      seedCollectionProducts(collection, productNodes);
    } else {
      for (const node of productNodes.filter(isPlainObject)) {
        const productId = readStringField(node, 'id');
        if (productId) {
          store.upsertBaseProducts([makeSeedProduct(productId, node)]);
        }
      }
    }
    seedPreexistingProductCollectionsFromReadPayload(
      readRecordField(capture as Record<string, unknown>, 'initialCollectionRead'),
      collection.id,
    );
    seedPreexistingProductCollectionsFromReadPayload(
      readRecordField(capture as Record<string, unknown>, 'downstreamRead'),
      collection.id,
    );
    for (const productIdValue of readArrayField(variables, 'productIds')) {
      if (typeof productIdValue !== 'string') {
        continue;
      }
      if (mutationName === 'collectionAddProducts' && seedProducts.length > 0) {
        continue;
      }
      if (!store.getEffectiveProductById(productIdValue)) {
        store.upsertBaseProducts([makeSeedProduct(productIdValue)]);
      }
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
