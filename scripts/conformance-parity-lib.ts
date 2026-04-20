import { readFileSync } from 'node:fs';
import path from 'node:path';
import { isDeepStrictEqual } from 'node:util';

import { parseOperation } from '../src/graphql/parse-operation.js';
import { getOperationCapability } from '../src/proxy/capabilities.js';
import {
  handleProductMutation,
  handleProductQuery,
  hydrateProductsFromUpstreamResponse,
} from '../src/proxy/products.js';
import { makeSyntheticGid, makeSyntheticTimestamp, resetSyntheticIdentity } from '../src/state/synthetic-identity.js';
import { store } from '../src/state/store.js';
import type {
  CollectionRecord,
  ProductCollectionRecord,
  ProductOptionRecord,
  ProductRecord,
  ProductVariantRecord,
} from '../src/state/types.js';

export type ParityScenarioState =
  | 'ready-for-comparison'
  | 'invalid-missing-comparison-contract'
  | 'not-yet-implemented';

type Matcher = 'any-string' | 'non-empty-string' | 'any-number' | 'iso-timestamp' | `shopify-gid:${string}`;

export interface ExpectedDifference {
  path: string;
  ignore?: boolean;
  matcher?: Matcher;
  reason?: string;
  regrettable?: true;
}

export interface ProxyRequestSpec {
  documentPath?: string | null;
  variablesPath?: string | null;
  variablesCapturePath?: string | null;
  variables?: Record<string, unknown>;
}

export interface ComparisonTarget {
  name: string;
  capturePath: string;
  proxyPath: string;
  upstreamCapturePath?: string | null;
  proxyRequest?: ProxyRequestSpec;
}

export interface ComparisonContract {
  mode?: string | null;
  expectedDifferences?: ExpectedDifference[] | null;
  targets?: ComparisonTarget[] | null;
}

export interface ParitySpec {
  proxyRequest?: ProxyRequestSpec;
  comparison?: ComparisonContract;
  liveCaptureFiles?: string[];
}

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
  return validateComparisonContract(paritySpec?.comparison).length === 0;
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
  return JSON.parse(readFileSync(path.join(repoRoot, relativePath), 'utf8')) as unknown;
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
      query: document,
      variables,
      status: 'staged',
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleProductMutation(document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'products') {
    if (upstreamPayload !== undefined) {
      hydrateProductsFromUpstreamResponse(upstreamPayload);
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
    Object.keys(stagedState.deletedCollectionIds).length > 0
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

function readArrayField(value: Record<string, unknown> | null | undefined, key: string): unknown[] {
  const fieldValue = value?.[key];
  return Array.isArray(fieldValue) ? fieldValue : [];
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

function seedPreconditionsFromCapture(capture: unknown, variables: Record<string, unknown>): void {
  const payload = mutationPayloadFromCapture(capture);
  const mutationName = mutationNameFromCapture(capture);
  const productInput = readRecordField(variables, 'product');
  const input = readRecordField(variables, 'input');
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

  if (productId) {
    store.upsertBaseProducts([makeSeedProduct(productId, productPayload ?? productInput)]);
    if (readArrayField(variables, 'options').length > 0 || readRecordField(variables, 'option')) {
      seedProductOptionState(productId, variables);
    }
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
  if (Array.isArray(comparison.targets) && comparison.targets.length > 0) {
    return comparison.targets;
  }

  return [
    {
      name: 'primary-response',
      capturePath: '$.mutation.response',
      proxyPath: '$',
    },
  ];
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

  const rawVariables = request.variablesPath ? readJsonFile(repoRoot, request.variablesPath) : request.variables;
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
