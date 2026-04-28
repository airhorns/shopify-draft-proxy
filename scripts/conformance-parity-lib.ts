import type { ProxyRuntimeContext } from '../src/proxy/runtime-context.js';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { isDeepStrictEqual } from 'node:util';

import { parseOperation, type ParsedOperation } from '../src/graphql/parse-operation.js';
import {
  graphqlVariablesSchema,
  jsonValueSchema,
  parseJsonFileWithSchema,
  type ComparisonContract,
  type ComparisonTarget,
  type ExpectedDifference,
  type JsonValue,
  type Matcher,
  type ParitySpec,
  type ProxyRequestSpec,
} from '../src/json-schemas.js';
export type {
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
import { handleAdminPlatformMutation, handleAdminPlatformQuery } from '../src/proxy/admin-platform.js';
import { handleAppMutation, handleAppQuery, hydrateAppsFromUpstreamResponse } from '../src/proxy/apps.js';
import { handleB2BMutation, handleB2BQuery } from '../src/proxy/b2b.js';
import { handleBulkOperationMutation, handleBulkOperationQuery } from '../src/proxy/bulk-operations.js';
import { handleDeliveryProfileMutation, handleDeliveryProfileQuery } from '../src/proxy/delivery-profiles.js';
import { handleDeliverySettingsQuery } from '../src/proxy/delivery-settings.js';
import { handleDiscountMutation, handleDiscountQuery } from '../src/proxy/discounts.js';
import { handleEventsQuery } from '../src/proxy/events.js';
import { handleFunctionMutation, handleFunctionQuery } from '../src/proxy/functions.js';
import { handleGiftCardMutation, handleGiftCardQuery } from '../src/proxy/gift-cards.js';
import { getOperationCapability, type OperationCapability } from '../src/proxy/capabilities.js';
import { handleInventoryShipmentMutation, handleInventoryShipmentQuery } from '../src/proxy/inventory-shipments.js';
import {
  handleMarketingMutation,
  handleMarketingQuery,
  hydrateMarketingFromUpstreamResponse,
} from '../src/proxy/marketing.js';
import {
  handleMarketMutation,
  handleMarketsQuery,
  hydrateMarketsFromUpstreamResponse,
  seedMarketsFromCapture,
} from '../src/proxy/markets.js';
import {
  handleLocalizationMutation,
  handleLocalizationQuery,
  hydrateLocalizationFromUpstreamResponse,
} from '../src/proxy/localization.js';
import { handleMediaMutation, handleMediaQuery } from '../src/proxy/media.js';
import { handleOrderMutation, handleOrderQuery } from '../src/proxy/orders.js';
import {
  handleOnlineStoreMutation,
  handleOnlineStoreQuery,
  hydrateOnlineStoreFromUpstreamResponse,
} from '../src/proxy/online-store.js';
import { findOperationRegistryEntry, listOperationRegistryEntries } from '../src/proxy/operation-registry.js';
import {
  handleProductMutation,
  handleProductQuery,
  hydrateProductsFromUpstreamResponse,
} from '../src/proxy/products.js';
import {
  handleSavedSearchMutation,
  handleSavedSearchQuery,
  hydrateSavedSearchesFromUpstreamResponse,
} from '../src/proxy/saved-searches.js';
import {
  handleMetafieldDefinitionMutation,
  handleMetafieldDefinitionQuery,
} from '../src/proxy/metafield-definitions.js';
import {
  handleMetaobjectDefinitionMutation,
  handleMetaobjectDefinitionQuery,
  hydrateMetaobjectsFromUpstreamResponse,
} from '../src/proxy/metaobject-definitions.js';
import { handlePaymentMutation, handlePaymentQuery } from '../src/proxy/payments.js';
import {
  handleSegmentMutation,
  handleSegmentsQuery,
  hydrateSegmentsFromUpstreamResponse,
} from '../src/proxy/segments.js';
import { handleStorePropertiesMutation, handleStorePropertiesQuery } from '../src/proxy/store-properties.js';
import {
  handleWebhookSubscriptionMutation,
  handleWebhookSubscriptionQuery,
  hydrateWebhookSubscriptionsFromUpstreamResponse,
} from '../src/proxy/webhooks.js';
import { DEFAULT_ADMIN_API_VERSION } from '../src/shopify/api-version.js';
import { SyntheticIdentityRegistry } from '../src/state/synthetic-identity.js';
import { InMemoryStore } from '../src/state/store.js';
import type {
  B2BCompanyContactRecord,
  B2BCompanyContactRoleRecord,
  B2BCompanyLocationRecord,
  B2BCompanyRecord,
  BulkOperationRecord,
  BusinessEntityRecord,
  CarrierServiceRecord,
  CollectionRecord,
  CustomerAddressRecord,
  CustomerMetafieldRecord,
  CustomerPaymentMethodRecord,
  CustomerRecord,
  DeliveryProfileCountryRecord,
  DeliveryProfileLocationGroupRecord,
  DeliveryProfileLocationGroupZoneRecord,
  DeliveryProfileMethodDefinitionRecord,
  DeliveryProfileRecord,
  DeliveryLocalPickupSettingsRecord,
  DraftOrderLineItemRecord,
  DraftOrderRecord,
  DraftOrderShippingLineRecord,
  GiftCardConfigurationRecord,
  GiftCardRecord,
  InventoryLevelRecord,
  LocaleRecord,
  LocationRecord,
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
  MetafieldDefinitionRecord,
  ChannelRecord,
  PublicationRecord,
  ProductMetafieldRecord,
  ProductMediaRecord,
  ProductOptionRecord,
  ProductRecord,
  SellingPlanGroupRecord,
  ProductVariantRecord,
  ShopifyPaymentsAccountRecord,
  ShopifyFunctionRecord,
  SegmentRecord,
  ShopRecord,
  ShopLocaleRecord,
  DiscountRecord,
  StoreCreditAccountRecord,
  TaxonomyCategoryRecord,
  MoneyV2Record,
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

function readRegisteredParityCapability(parsed: ParsedOperation, fallback: OperationCapability): OperationCapability {
  const registryEntry = findOperationRegistryEntry(parsed.type, [...parsed.rootFields, parsed.name]);
  if (!registryEntry) {
    return fallback;
  }

  const matchedRootField = parsed.rootFields.find((rootField) => registryEntry.matchNames.includes(rootField));
  const operationName =
    matchedRootField ??
    (parsed.name && registryEntry.matchNames.includes(parsed.name) ? parsed.name : registryEntry.name);

  return {
    type: parsed.type,
    operationName,
    domain: registryEntry.domain,
    execution: registryEntry.execution,
  };
}

export type ParityScenarioState =
  | 'ready-for-comparison'
  | 'enforced-by-fixture'
  | 'invalid-missing-comparison-contract'
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

export interface ExecutedOperation {
  type: ParsedOperation['type'];
  name: string | null;
  rootFields: string[];
}

export interface OperationNameValidationResult {
  declaredMutationOperationNames: string[];
  actualMutationOperationNames: string[];
  missingMutationOperationNames: string[];
  unexpectedMutationOperationNames: string[];
  errors: string[];
}

interface CompiledRule extends ExpectedDifference {
  index: number;
  segments: PathSegment[];
}

type PathSegment = string | number | '*';

const PAYMENT_CUSTOMIZATION_MUTATION_ROOTS = new Set([
  'customerPaymentMethodCreateFromDuplicationData',
  'customerPaymentMethodCreditCardCreate',
  'customerPaymentMethodCreditCardUpdate',
  'customerPaymentMethodGetDuplicationData',
  'customerPaymentMethodGetUpdateUrl',
  'customerPaymentMethodPaypalBillingAgreementCreate',
  'customerPaymentMethodPaypalBillingAgreementUpdate',
  'customerPaymentMethodRemoteCreate',
  'customerPaymentMethodRevoke',
  'paymentCustomizationActivation',
  'paymentCustomizationCreate',
  'paymentCustomizationDelete',
  'paymentCustomizationUpdate',
  'paymentReminderSend',
]);
const operationRegistryEntries = listOperationRegistryEntries();
const registeredMutationOperationNames = new Set(
  operationRegistryEntries
    .filter((entry) => entry.type === 'mutation')
    .flatMap((entry) => [entry.name, ...entry.matchNames]),
);
const ORDER_PAYMENT_MUTATION_ROOTS = new Set(['orderCapture', 'transactionVoid', 'orderCreateMandatePayment']);
const PAYMENT_TERMS_MUTATION_ROOTS = new Set(['paymentTermsCreate', 'paymentTermsUpdate', 'paymentTermsDelete']);
const ORDER_ACCESS_DENIED_GUARDRAIL_MUTATION_ROOTS = new Set(['orderCreateManualPayment', 'taxSummaryCreate']);

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

function validateExpectedDifferences(rawRules: unknown, labelPrefix: string): string[] {
  const errors: string[] = [];
  if (!Array.isArray(rawRules)) {
    errors.push(`${labelPrefix} must declare an expectedDifferences array.`);
    return errors;
  }

  for (const [index, rawRule] of rawRules.entries()) {
    const rule = isPlainObject(rawRule) ? rawRule : {};
    const label = `${labelPrefix}[${index}]`;
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

  return errors;
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

  errors.push(...validateExpectedDifferences(candidate['expectedDifferences'], 'expectedDifferences'));

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
        if ('selectedPaths' in target) {
          if (!Array.isArray(target['selectedPaths']) || target['selectedPaths'].length === 0) {
            errors.push(`${label} selectedPaths, when declared, must be a non-empty array.`);
          } else {
            for (const [pathIndex, rawPath] of target['selectedPaths'].entries()) {
              if (typeof rawPath !== 'string' || rawPath.length === 0) {
                errors.push(`${label}.selectedPaths[${pathIndex}] must be a non-empty JSON path.`);
              }
            }
          }
        }
        if ('excludedPaths' in target) {
          if (!Array.isArray(target['excludedPaths']) || target['excludedPaths'].length === 0) {
            errors.push(`${label} excludedPaths, when declared, must be a non-empty array.`);
          } else {
            for (const [pathIndex, rawPath] of target['excludedPaths'].entries()) {
              if (typeof rawPath !== 'string' || rawPath.length === 0) {
                errors.push(`${label}.excludedPaths[${pathIndex}] must be a non-empty JSON path.`);
              }
            }
          }
        }
        if ('selectedPaths' in target && 'excludedPaths' in target) {
          errors.push(`${label} must not declare both selectedPaths and excludedPaths.`);
        }
        if ('expectedDifferences' in target) {
          errors.push(...validateExpectedDifferences(target['expectedDifferences'], `${label}.expectedDifferences`));
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
    if (paritySpec?.comparisonMode === 'captured-fixture' && (paritySpec.liveCaptureFiles?.length ?? 0) > 0) {
      return 'enforced-by-fixture';
    }

    return hasProxyRequest(paritySpec) && hasComparisonContract(paritySpec)
      ? 'ready-for-comparison'
      : 'invalid-missing-comparison-contract';
  }

  return 'not-yet-implemented';
}

export const parityStatusNote =
  'readyForComparison means a captured scenario has a proxy request and an explicit strict-json comparison contract. enforcedByFixture means a captured multi-step fixture is enforced outside the generic parity runner by committed runtime tests. invalid captured scenarios are not allowed in checked-in inventory. notYetImplemented scenarios are legacy non-executable entries; do not add new planned-only parity specs.';

export function validateParityScenarioInventoryEntry(
  scenario: Pick<Scenario, 'id' | 'status' | 'captureFiles'>,
  paritySpec: ParitySpec,
): string[] {
  const errors: string[] = [];
  const mode = paritySpec.comparisonMode;

  if (scenario.status !== 'captured') {
    return errors;
  }

  if (mode === 'planned') {
    errors.push(`Captured scenario ${scenario.id} must use an enforced captured comparison mode.`);
    return errors;
  }

  if (mode === 'captured-fixture') {
    if ((scenario.captureFiles?.length ?? paritySpec.liveCaptureFiles?.length ?? 0) === 0) {
      errors.push(`Captured fixture scenario ${scenario.id} must reference at least one capture fixture.`);
    }
    if ((paritySpec.runtimeTestFiles?.length ?? 0) === 0) {
      errors.push(`Captured fixture scenario ${scenario.id} must reference at least one runtime test file.`);
    }
    return errors;
  }

  if (!hasProxyRequest(paritySpec)) {
    errors.push(`Captured scenario ${scenario.id} must declare a proxy request.`);
  }

  const comparisonErrors = validateComparisonContract(paritySpec.comparison);
  if (comparisonErrors.length > 0) {
    errors.push(...comparisonErrors.map((error) => `Captured scenario ${scenario.id}: ${error}`));
  }

  if (!hasComparisonContract(paritySpec)) {
    errors.push(`Captured scenario ${scenario.id} must declare at least one executable comparison target.`);
  }

  return errors;
}

export function summarizeParityResults(results: Array<{ state: ParityScenarioState }>): {
  readyForComparison: number;
  pending: number;
  statusCounts: Record<
    'readyForComparison' | 'enforcedByFixture' | 'invalidMissingComparisonContract' | 'notYetImplemented',
    number
  >;
  statusNote: string;
} {
  const readyForComparison = results.filter((result) => result.state === 'ready-for-comparison').length;
  const enforcedByFixture = results.filter((result) => result.state === 'enforced-by-fixture').length;
  const invalidMissingComparisonContract = results.filter(
    (result) => result.state === 'invalid-missing-comparison-contract',
  ).length;
  const notYetImplemented = results.filter((result) => result.state === 'not-yet-implemented').length;

  return {
    readyForComparison,
    pending: results.length - readyForComparison - enforcedByFixture,
    statusCounts: {
      readyForComparison,
      enforcedByFixture,
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

function materializeValue(
  rawValue: unknown,
  proxyResponses: Record<string, unknown>,
  previousProxyResponse: unknown,
  capture?: unknown,
): unknown {
  if (Array.isArray(rawValue)) {
    return rawValue.map((item) => materializeValue(item, proxyResponses, previousProxyResponse, capture));
  }

  if (!isPlainObject(rawValue)) {
    return rawValue;
  }

  if (typeof rawValue['fromPrimaryProxyPath'] === 'string') {
    return readJsonPath(proxyResponses['primary'], rawValue['fromPrimaryProxyPath']);
  }

  if (typeof rawValue['fromProxyResponse'] === 'string' && typeof rawValue['path'] === 'string') {
    return readJsonPath(proxyResponses[rawValue['fromProxyResponse']], rawValue['path']);
  }

  if (typeof rawValue['fromPreviousProxyPath'] === 'string') {
    return readJsonPath(previousProxyResponse, rawValue['fromPreviousProxyPath']);
  }

  if (typeof rawValue['fromCapturePath'] === 'string') {
    return readJsonPath(capture, rawValue['fromCapturePath']);
  }

  return Object.fromEntries(
    Object.entries(rawValue).map(([key, value]) => [
      key,
      materializeValue(value, proxyResponses, previousProxyResponse, capture),
    ]),
  );
}

function materializeVariables(
  rawVariables: unknown,
  proxyResponses: Record<string, unknown>,
  previousProxyResponse: unknown,
  capture?: unknown,
): Record<string, unknown> {
  const materialized = materializeValue(rawVariables ?? {}, proxyResponses, previousProxyResponse, capture);
  return isPlainObject(materialized) ? materialized : {};
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

function uniqueSorted(values: string[]): string[] {
  return [...new Set(values)].sort((left, right) => left.localeCompare(right));
}

export function validateParityScenarioOperationNames({
  scenario,
  paritySpec,
  executedOperations,
}: {
  scenario: Scenario;
  paritySpec: Pick<ParitySpec, 'operationNames'>;
  executedOperations: ExecutedOperation[];
}): OperationNameValidationResult {
  const actualMutationOperationNames = uniqueSorted(
    executedOperations.flatMap((operation) => (operation.type === 'mutation' ? operation.rootFields : [])),
  );
  const actualMutationOperationNameSet = new Set(actualMutationOperationNames);
  const declaredMutationOperationNames = uniqueSorted(
    (scenario.operationNames ?? paritySpec.operationNames ?? []).filter(
      (operationName) =>
        registeredMutationOperationNames.has(operationName) || actualMutationOperationNameSet.has(operationName),
    ),
  );
  const declaredMutationOperationNameSet = new Set(declaredMutationOperationNames);
  const missingMutationOperationNames = declaredMutationOperationNames.filter(
    (operationName) => !actualMutationOperationNameSet.has(operationName),
  );
  const unexpectedMutationOperationNames = actualMutationOperationNames.filter(
    (operationName) => !declaredMutationOperationNameSet.has(operationName),
  );
  const errors = [
    ...(missingMutationOperationNames.length > 0
      ? [
          `Scenario ${scenario.id} declares mutation operation(s) ${missingMutationOperationNames.join(
            ', ',
          )} in operationNames but did not execute them. Actual executed mutation operation(s): ${
            actualMutationOperationNames.join(', ') || '(none)'
          }.`,
        ]
      : []),
    ...(unexpectedMutationOperationNames.length > 0
      ? [
          `Scenario ${scenario.id} executed mutation operation(s) ${unexpectedMutationOperationNames.join(
            ', ',
          )} but does not declare them in operationNames. Declared mutation operation(s): ${
            declaredMutationOperationNames.join(', ') || '(none)'
          }.`,
        ]
      : []),
  ];

  return {
    declaredMutationOperationNames,
    actualMutationOperationNames,
    missingMutationOperationNames,
    unexpectedMutationOperationNames,
    errors,
  };
}

const INVENTORY_SHIPMENT_MUTATION_ROOTS = new Set([
  'inventoryShipmentCreate',
  'inventoryShipmentCreateInTransit',
  'inventoryShipmentAddItems',
  'inventoryShipmentRemoveItems',
  'inventoryShipmentUpdateItemQuantities',
  'inventoryShipmentSetTracking',
  'inventoryShipmentMarkInTransit',
  'inventoryShipmentReceive',
  'inventoryShipmentDelete',
]);

async function executeGraphQLAgainstLocalProxy(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
  upstreamPayload?: unknown,
  onExecutedOperation?: (operation: ExecutedOperation) => void,
  apiVersion = DEFAULT_ADMIN_API_VERSION,
): Promise<{ status: number; body: Record<string, unknown> }> {
  const parsed = parseOperation(document);
  onExecutedOperation?.({
    type: parsed.type,
    name: parsed.name,
    rootFields: [...parsed.rootFields],
  });
  const capability = getOperationCapability(parsed);
  const registeredCapability = readRegisteredParityCapability(parsed, capability);

  if (parsed.type === 'mutation') {
    const discountMutation = handleDiscountMutation(runtime, document, variables);
    if (discountMutation) {
      return {
        status: 200,
        body: discountMutation.response,
      };
    }

    if (parsed.rootFields.some((rootField) => ORDER_ACCESS_DENIED_GUARDRAIL_MUTATION_ROOTS.has(rootField))) {
      const body = handleOrderMutation(runtime, document, variables, 'snapshot');
      if (!body) {
        throw new Error(`Order guardrail parity request was not handled locally: ${parsed.rootFields.join(', ')}`);
      }

      return {
        status: 200,
        body,
      };
    }
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'apps') {
    const responseBody = handleAppMutation(runtime, document, variables, 'https://conformance.local');
    if (!responseBody) {
      throw new Error(`App parity request was not handled locally: ${capability.operationName}`);
    }

    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity app billing/access proxy harness.',
    });

    return {
      status: 200,
      body: responseBody,
    };
  }

  if (
    capability.execution === 'stage-locally' &&
    (capability.domain === 'products' ||
      (capability.domain === 'store-properties' && capability.operationName?.startsWith('publishable') === true))
  ) {
    if (parsed.rootFields.some((rootField) => INVENTORY_SHIPMENT_MUTATION_ROOTS.has(rootField))) {
      const inventoryShipmentMutation = handleInventoryShipmentMutation(runtime, document, variables);
      if (!inventoryShipmentMutation) {
        throw new Error(`Inventory shipment parity request was not handled locally: ${capability.operationName}`);
      }

      if (inventoryShipmentMutation.staged) {
        runtime.store.recordMutationLogEntry({
          id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
          receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
          operationName: capability.operationName,
          path: `/admin/api/${apiVersion}/graphql.json`,
          query: document,
          variables,
          status: 'staged',
          interpreted: interpretMutationLogEntry(parsed, capability),
          stagedResourceIds: inventoryShipmentMutation.stagedResourceIds,
          notes: inventoryShipmentMutation.notes,
        });
      }

      return {
        status: 200,
        body: inventoryShipmentMutation.response,
      };
    }

    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleProductMutation(runtime, document, variables, 'snapshot', apiVersion),
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
    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleProductMutation(runtime, document, variables, 'snapshot', apiVersion),
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'media') {
    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleMediaMutation(runtime, document, variables),
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'orders') {
    const body = handleOrderMutation(runtime, document, variables, 'snapshot');
    if (!body) {
      throw new Error(`Order-domain parity request was not handled locally: ${capability.operationName}`);
    }

    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
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

  if (
    capability.execution === 'stage-locally' &&
    capability.domain === 'shipping-fulfillments' &&
    parsed.rootFields.some((rootField) =>
      ['reverseDeliveryCreateWithShipping', 'reverseDeliveryShippingUpdate', 'reverseFulfillmentOrderDispose'].includes(
        rootField,
      ),
    )
  ) {
    const body = handleOrderMutation(runtime, document, variables, 'snapshot');
    if (!body) {
      throw new Error(`Reverse-logistics parity request was not handled locally: ${capability.operationName}`);
    }

    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
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
    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleCustomerMutation(runtime, document, variables),
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'gift-cards') {
    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleGiftCardMutation(runtime, document, variables),
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'functions') {
    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleFunctionMutation(runtime, document, variables),
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'admin-platform') {
    const result = handleAdminPlatformMutation(runtime, document, variables);
    if (!result) {
      throw new Error(`Admin platform parity request was not handled locally: ${capability.operationName}`);
    }

    if (result.staged) {
      runtime.store.recordMutationLogEntry({
        id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
        receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: `/admin/api/${apiVersion}/graphql.json`,
        query: document,
        variables,
        stagedResourceIds: result.stagedResourceIds ?? [],
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        notes: result.notes ?? 'Staged locally in the conformance parity proxy harness.',
      });
    }

    return {
      status: 200,
      body: result.response,
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'privacy') {
    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleCustomerMutation(runtime, document, variables),
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'markets') {
    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleMarketMutation(runtime, document, variables),
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'localization') {
    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleLocalizationMutation(runtime, document, variables),
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'segments') {
    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleSegmentMutation(runtime, document, variables),
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'saved-searches') {
    const savedSearchMutation = handleSavedSearchMutation(runtime, document, variables);
    if (!savedSearchMutation) {
      throw new Error(
        `Registered saved-search parity request was not handled locally: ${
          capability.operationName ?? parsed.rootFields.join(', ')
        }`,
      );
    }

    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      stagedResourceIds: savedSearchMutation.stagedResourceIds,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: savedSearchMutation.response,
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'marketing') {
    const marketingMutation = handleMarketingMutation(runtime, document, variables);
    if (!marketingMutation) {
      throw new Error(`Marketing-domain parity request was not handled locally: ${capability.operationName}`);
    }

    if (marketingMutation.shouldLog) {
      runtime.store.recordMutationLogEntry({
        id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
        receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: `/admin/api/${apiVersion}/graphql.json`,
        query: document,
        variables,
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        stagedResourceIds: marketingMutation.stagedResourceIds,
        notes: marketingMutation.notes,
      });
    }

    return {
      status: 200,
      body: marketingMutation.response,
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'online-store') {
    const onlineStoreMutation = handleOnlineStoreMutation(runtime, document, variables);
    if (!onlineStoreMutation) {
      throw new Error(`Online-store parity request was not handled locally: ${capability.operationName}`);
    }

    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      stagedResourceIds: onlineStoreMutation.stagedResourceIds,
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: onlineStoreMutation.response,
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'bulk-operations') {
    const bulkOperationMutation = handleBulkOperationMutation(runtime, document, variables, {
      readMode: 'snapshot',
      shopifyAdminOrigin: 'https://example.myshopify.com',
    });
    if (!bulkOperationMutation) {
      throw new Error(`Bulk-operation parity request was not handled locally: ${capability.operationName}`);
    }

    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      stagedResourceIds: bulkOperationMutation.stagedResourceIds,
      notes: bulkOperationMutation.notes,
    });

    return {
      status: 200,
      body: bulkOperationMutation.response,
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'b2b') {
    const b2bMutation = handleB2BMutation(runtime, document, variables);
    if (!b2bMutation) {
      throw new Error(`B2B-domain parity request was not handled locally: ${capability.operationName}`);
    }

    if (b2bMutation.staged) {
      runtime.store.recordMutationLogEntry({
        id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
        receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: `/admin/api/${apiVersion}/graphql.json`,
        query: document,
        variables,
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        stagedResourceIds: b2bMutation.stagedResourceIds,
        notes: b2bMutation.notes,
      });
    }

    return {
      status: 200,
      body: b2bMutation.response,
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'webhooks') {
    const webhookSubscriptionMutation = handleWebhookSubscriptionMutation(runtime, document, variables);
    if (!webhookSubscriptionMutation) {
      throw new Error(`Webhook-domain parity request was not handled locally: ${capability.operationName}`);
    }

    if (webhookSubscriptionMutation.staged) {
      runtime.store.recordMutationLogEntry({
        id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
        receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: `/admin/api/${apiVersion}/graphql.json`,
        query: document,
        variables,
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        stagedResourceIds: webhookSubscriptionMutation.stagedResourceIds,
        notes: webhookSubscriptionMutation.notes,
      });
    }

    return {
      status: 200,
      body: webhookSubscriptionMutation.response,
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'metafields') {
    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleMetafieldDefinitionMutation(runtime, document, variables),
    };
  }

  if (capability.execution === 'stage-locally' && capability.domain === 'metaobjects') {
    const body = handleMetaobjectDefinitionMutation(runtime, document, variables);

    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
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

  if (capability.execution === 'stage-locally' && capability.domain === 'store-properties') {
    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleStorePropertiesMutation(runtime, document, variables),
    };
  }

  const shippingCapability = capability.domain === 'shipping-fulfillments' ? capability : registeredCapability;
  if (shippingCapability.execution === 'stage-locally' && shippingCapability.domain === 'shipping-fulfillments') {
    const deliveryProfileMutation = handleDeliveryProfileMutation(runtime, document, variables);
    if (deliveryProfileMutation) {
      runtime.store.recordMutationLogEntry({
        id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
        receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
        operationName: shippingCapability.operationName,
        path: `/admin/api/${apiVersion}/graphql.json`,
        query: document,
        variables,
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, shippingCapability),
        stagedResourceIds: deliveryProfileMutation.stagedResourceIds,
        notes: deliveryProfileMutation.notes,
      });

      return {
        status: 200,
        body: deliveryProfileMutation.response,
      };
    }

    const orderMutationBody = handleOrderMutation(runtime, document, variables, 'snapshot');
    if (orderMutationBody) {
      runtime.store.recordMutationLogEntry({
        id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
        receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
        operationName: shippingCapability.operationName,
        path: `/admin/api/${apiVersion}/graphql.json`,
        query: document,
        variables,
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, shippingCapability),
        notes: 'Staged locally in the conformance parity proxy harness.',
      });

      return {
        status: 200,
        body: orderMutationBody,
      };
    }

    if (capability.domain !== 'shipping-fulfillments') {
      throw new Error(
        `Registered shipping-fulfillment parity request was not handled locally: ${
          shippingCapability.operationName ?? parsed.rootFields.join(', ')
        }`,
      );
    }

    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: shippingCapability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, shippingCapability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleStorePropertiesMutation(runtime, document, variables),
    };
  }

  if (
    capability.execution === 'stage-locally' &&
    capability.domain === 'payments' &&
    parsed.rootFields.some((rootField) => PAYMENT_CUSTOMIZATION_MUTATION_ROOTS.has(rootField))
  ) {
    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
      query: document,
      variables,
      status: 'staged',
      interpreted: interpretMutationLogEntry(parsed, capability),
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handlePaymentMutation(runtime, document, variables),
    };
  }

  if (
    capability.execution === 'stage-locally' &&
    capability.domain === 'payments' &&
    parsed.rootFields.some(
      (rootField) => ORDER_PAYMENT_MUTATION_ROOTS.has(rootField) || PAYMENT_TERMS_MUTATION_ROOTS.has(rootField),
    )
  ) {
    const body = handleOrderMutation(runtime, document, variables, 'snapshot');
    if (!body) {
      throw new Error(`Order-payment parity request was not handled locally: ${capability.operationName}`);
    }

    runtime.store.recordMutationLogEntry({
      id: runtime.syntheticIdentity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
      operationName: capability.operationName,
      path: `/admin/api/${apiVersion}/graphql.json`,
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
    if (parsed.rootFields.includes('inventoryShipment')) {
      return {
        status: 200,
        body: handleInventoryShipmentQuery(runtime, document, variables),
      };
    }

    if (upstreamPayload !== undefined) {
      hydrateProductsFromUpstreamResponse(runtime, document, variables, upstreamPayload);
      if (!hasStagedState(runtime)) {
        return {
          status: 200,
          body: isPlainObject(upstreamPayload) ? upstreamPayload : {},
        };
      }
    }

    return {
      status: 200,
      body: handleProductQuery(
        runtime,
        document,
        variables,
        upstreamPayload === undefined ? 'snapshot' : 'live-hybrid',
      ),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'admin-platform') {
    return {
      status: 200,
      body: handleAdminPlatformQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'metafields') {
    return {
      status: 200,
      body: handleMetafieldDefinitionQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'media') {
    return {
      status: 200,
      body: handleMediaQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'metaobjects') {
    return {
      status: 200,
      body: handleMetaobjectDefinitionQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'customers') {
    if (upstreamPayload !== undefined) {
      hydrateCustomersFromUpstreamResponse(runtime, document, variables, upstreamPayload);
      if (!hasStagedState(runtime)) {
        return {
          status: 200,
          body: isPlainObject(upstreamPayload) ? upstreamPayload : {},
        };
      }
    }

    return {
      status: 200,
      body: handleCustomerQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'gift-cards') {
    return {
      status: 200,
      body: handleGiftCardQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'functions') {
    return {
      status: 200,
      body: handleFunctionQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'orders') {
    const upstreamPayloadIsResponseEnvelope =
      isPlainObject(upstreamPayload) && ('data' in upstreamPayload || 'errors' in upstreamPayload);

    if (upstreamPayload !== undefined && upstreamPayloadIsResponseEnvelope && !hasOrderState(runtime)) {
      return {
        status: 200,
        body: upstreamPayload,
      };
    }

    if (upstreamPayload !== undefined) {
      hydrateOrdersFromUpstreamResponse(runtime, upstreamPayload);
    }

    return {
      status: 200,
      body: handleOrderQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'discounts') {
    return {
      status: 200,
      body: handleDiscountQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'payments') {
    return {
      status: 200,
      body: handlePaymentQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'store-properties') {
    return {
      status: 200,
      body: handleStorePropertiesQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'shipping-fulfillments') {
    const primaryRootField = parsed.rootFields[0] ?? capability.operationName;
    if (
      parsed.rootFields.some((rootField) => rootField === 'deliverySettings' || rootField === 'deliveryPromiseSettings')
    ) {
      return {
        status: 200,
        body: handleDeliverySettingsQuery(document),
      };
    }

    if (parsed.rootFields.some((rootField) => rootField === 'deliveryProfile' || rootField === 'deliveryProfiles')) {
      return {
        status: 200,
        body: handleDeliveryProfileQuery(runtime, document, variables),
      };
    }

    if (
      parsed.rootFields.some((rootField) =>
        [
          'fulfillment',
          'fulfillmentOrder',
          'fulfillmentOrders',
          'assignedFulfillmentOrders',
          'manualHoldsFulfillmentOrders',
          'reverseDelivery',
          'reverseFulfillmentOrder',
        ].includes(rootField),
      )
    ) {
      return {
        status: 200,
        body: handleOrderQuery(runtime, document, variables),
      };
    }

    if (primaryRootField === 'fulfillmentService') {
      return {
        status: 200,
        body: handleStorePropertiesQuery(runtime, document, variables),
      };
    }

    return {
      status: 200,
      body: handleStorePropertiesQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'payments') {
    return {
      status: 200,
      body: handleStorePropertiesQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'markets') {
    if (upstreamPayload !== undefined) {
      hydrateMarketsFromUpstreamResponse(runtime, document, variables, upstreamPayload);
    }

    return {
      status: 200,
      body: handleMarketsQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'localization') {
    if (upstreamPayload !== undefined) {
      hydrateLocalizationFromUpstreamResponse(runtime, upstreamPayload);
    }

    return {
      status: 200,
      body: handleLocalizationQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'segments') {
    if (upstreamPayload !== undefined) {
      hydrateSegmentsFromUpstreamResponse(runtime, document, variables, upstreamPayload);
    }

    return {
      status: 200,
      body: handleSegmentsQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'saved-searches') {
    if (upstreamPayload !== undefined) {
      hydrateSavedSearchesFromUpstreamResponse(runtime, document, upstreamPayload);
    }

    return {
      status: 200,
      body: handleSavedSearchQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'marketing') {
    if (upstreamPayload !== undefined) {
      hydrateMarketingFromUpstreamResponse(runtime, document, variables, upstreamPayload);
    }

    return {
      status: 200,
      body: handleMarketingQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'online-store') {
    if (upstreamPayload !== undefined) {
      hydrateOnlineStoreFromUpstreamResponse(runtime, document, upstreamPayload);
    }

    return {
      status: 200,
      body: handleOnlineStoreQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'events') {
    return {
      status: 200,
      body: handleEventsQuery(document),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'b2b') {
    return {
      status: 200,
      body: handleB2BQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'bulk-operations') {
    return {
      status: 200,
      body: handleBulkOperationQuery(runtime, document, variables),
    };
  }

  if (
    (capability.execution === 'overlay-read' && capability.domain === 'apps') ||
    (registeredCapability.execution === 'overlay-read' && registeredCapability.domain === 'apps')
  ) {
    if (upstreamPayload !== undefined) {
      hydrateAppsFromUpstreamResponse(runtime, upstreamPayload);
    }

    return {
      status: 200,
      body: handleAppQuery(runtime, document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'webhooks') {
    if (upstreamPayload !== undefined) {
      hydrateWebhookSubscriptionsFromUpstreamResponse(runtime, document, variables, upstreamPayload);
      if (!hasStagedState(runtime)) {
        return {
          status: 200,
          body: isPlainObject(upstreamPayload) ? upstreamPayload : {},
        };
      }
    }

    return {
      status: 200,
      body: handleWebhookSubscriptionQuery(runtime, document, variables),
    };
  }

  throw new Error(
    `Parity execution does not allow live Shopify requests or unsupported operations: ${capability.operationName}`,
  );
}

function hasStagedState(runtime: ProxyRuntimeContext): boolean {
  const { stagedState } = runtime.store.getState();
  return (
    Object.keys(stagedState.products).length > 0 ||
    Object.keys(stagedState.productVariants).length > 0 ||
    Object.keys(stagedState.productOptions).length > 0 ||
    Object.keys(stagedState.collections).length > 0 ||
    Object.keys(stagedState.discounts).length > 0 ||
    Object.keys(stagedState.productCollections).length > 0 ||
    Object.keys(stagedState.productMedia).length > 0 ||
    Object.keys(stagedState.files).length > 0 ||
    Object.keys(stagedState.productMetafields).length > 0 ||
    Object.keys(stagedState.inventoryShipments).length > 0 ||
    Object.keys(stagedState.deletedInventoryShipmentIds).length > 0 ||
    Object.keys(stagedState.metafieldDefinitions).length > 0 ||
    Object.keys(stagedState.metaobjectDefinitions).length > 0 ||
    Object.keys(stagedState.deletedMetaobjectDefinitionIds).length > 0 ||
    Object.keys(stagedState.deletedProductIds).length > 0 ||
    Object.keys(stagedState.deletedFileIds).length > 0 ||
    Object.keys(stagedState.deletedCollectionIds).length > 0 ||
    Object.keys(stagedState.customers).length > 0 ||
    Object.keys(stagedState.deletedCustomerIds).length > 0 ||
    Object.keys(stagedState.deletedDiscountIds).length > 0 ||
    Object.keys(stagedState.orders).length > 0 ||
    Object.keys(stagedState.draftOrders).length > 0 ||
    Object.keys(stagedState.calculatedOrders).length > 0 ||
    Object.keys(stagedState.giftCards).length > 0 ||
    Object.keys(stagedState.deletedGiftCardIds).length > 0 ||
    Object.keys(stagedState.appSubscriptions).length > 0 ||
    Object.keys(stagedState.appOneTimePurchases).length > 0 ||
    Object.keys(stagedState.appUsageRecords).length > 0 ||
    Object.keys(stagedState.delegatedAccessTokens).length > 0 ||
    Object.keys(stagedState.webhookSubscriptions).length > 0 ||
    Object.keys(stagedState.deletedWebhookSubscriptionIds).length > 0
  );
}

function hasOrderState(runtime: ProxyRuntimeContext): boolean {
  const { baseState, stagedState } = runtime.store.getState();
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

function readCaptureApiVersion(capture: unknown): string | null {
  return isPlainObject(capture) ? readStringField(capture, 'apiVersion') : null;
}

function readApiVersionFromCapturePath(capturePath: string): string | null {
  const match = /\/(\d{4}-\d{2})\//u.exec(capturePath);
  return match?.[1] ?? null;
}

function readNumberField(value: Record<string, unknown> | null | undefined, key: string): number | null {
  const fieldValue = value?.[key];
  return typeof fieldValue === 'number' ? fieldValue : null;
}

function readNullableNumberField(value: Record<string, unknown> | null | undefined, key: string): number | null {
  const fieldValue = value?.[key];
  return typeof fieldValue === 'number' || fieldValue === null ? fieldValue : null;
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

function giftCardTail(id: string): string {
  return id.split('/').at(-1)?.split('?')[0] ?? id;
}

function readMoneyRecord(value: Record<string, unknown> | null | undefined): MoneyV2Record {
  return {
    amount: readStringField(value, 'amount') ?? '0.0',
    currencyCode: readStringField(value, 'currencyCode') ?? 'CAD',
  };
}

function makeSeedGiftCard(runtime: ProxyRuntimeContext, source: Record<string, unknown>): GiftCardRecord | null {
  const id = readStringField(source, 'id');
  if (!id?.startsWith('gid://shopify/GiftCard/')) {
    return null;
  }

  const lastCharacters = readStringField(source, 'lastCharacters') ?? giftCardTail(id).slice(-4).padStart(4, '0');
  const initialValue = readMoneyRecord(readRecordField(source, 'initialValue'));
  const balance = readMoneyRecord(readRecordField(source, 'balance') ?? readRecordField(source, 'initialValue'));
  const transactionNodes = readArrayField(readRecordField(source, 'transactions'), 'nodes').filter(isPlainObject);
  const recipientAttributesSource = readRecordField(source, 'recipientAttributes');
  const recipientSource = readRecordField(recipientAttributesSource, 'recipient');
  const recipientId =
    readNullableStringField(recipientSource, 'id') ??
    readNullableStringField(readRecordField(source, 'recipient'), 'id');

  return {
    id,
    legacyResourceId: readNullableStringField(source, 'legacyResourceId') ?? giftCardTail(id),
    lastCharacters,
    maskedCode:
      readStringField(source, 'maskedCode') ??
      `\u2022\u2022\u2022\u2022 \u2022\u2022\u2022\u2022 \u2022\u2022\u2022\u2022 ${lastCharacters}`,
    enabled: readBooleanField(source, 'enabled') ?? true,
    deactivatedAt: readNullableStringField(source, 'deactivatedAt'),
    expiresOn: readNullableStringField(source, 'expiresOn'),
    note: readNullableStringField(source, 'note'),
    templateSuffix: readNullableStringField(source, 'templateSuffix'),
    createdAt: readStringField(source, 'createdAt') ?? '2026-01-01T00:00:00Z',
    updatedAt: readStringField(source, 'updatedAt') ?? '2026-01-01T00:00:00Z',
    initialValue,
    balance,
    customerId: readNullableStringField(readRecordField(source, 'customer'), 'id'),
    recipientId,
    recipientAttributes: recipientAttributesSource
      ? {
          id: recipientId,
          message: readNullableStringField(recipientAttributesSource, 'message'),
          preferredName: readNullableStringField(recipientAttributesSource, 'preferredName'),
          sendNotificationAt: readNullableStringField(recipientAttributesSource, 'sendNotificationAt'),
        }
      : null,
    transactions: transactionNodes.map((transaction) => {
      const amount = readMoneyRecord(readRecordField(transaction, 'amount'));
      return {
        id: readStringField(transaction, 'id') ?? runtime.syntheticIdentity.makeSyntheticGid('GiftCardTransaction'),
        kind: (amount.amount ?? '0.0').startsWith('-') ? ('DEBIT' as const) : ('CREDIT' as const),
        amount,
        processedAt: readStringField(transaction, 'processedAt') ?? '2026-01-01T00:00:00Z',
        note: readNullableStringField(transaction, 'note'),
      };
    }),
  };
}

function makeSeedGiftCardConfiguration(
  source: Record<string, unknown> | null | undefined,
): GiftCardConfigurationRecord | null {
  if (!source) {
    return null;
  }

  return {
    issueLimit: readMoneyRecord(readRecordField(source, 'issueLimit')),
    purchaseLimit: readMoneyRecord(readRecordField(source, 'purchaseLimit')),
  };
}

function seedGiftCardLifecyclePreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const recordsById = new Map<string, GiftCardRecord>();
  const addGiftCard = (source: unknown): void => {
    if (!isPlainObject(source)) {
      return;
    }
    const record = makeSeedGiftCard(runtime, source);
    if (record) {
      recordsById.set(record.id, record);
    }
  };

  addGiftCard(readJsonPath(capture, '$.operations.create.response.payload.data.giftCardCreate.giftCard'));
  addGiftCard(readJsonPath(capture, '$.create.response.payload.data.giftCardCreate.giftCard'));

  const emptyReadNodes = readJsonPath(capture, '$.operations.emptyRead.response.payload.data.giftCards.nodes');
  for (const node of (Array.isArray(emptyReadNodes) ? emptyReadNodes : []).filter(isPlainObject)) {
    addGiftCard(node);
  }

  const configuration =
    makeSeedGiftCardConfiguration(
      readJsonPath(capture, '$.operations.configurationRead.response.payload.data.giftCardConfiguration') as Record<
        string,
        unknown
      > | null,
    ) ??
    makeSeedGiftCardConfiguration(
      readJsonPath(capture, '$.configurationRead.response.payload.data.giftCardConfiguration') as Record<
        string,
        unknown
      > | null,
    );

  if (recordsById.size > 0) {
    runtime.store.upsertBaseGiftCards([...recordsById.values()]);
  }
  if (configuration) {
    runtime.store.upsertBaseGiftCardConfiguration(configuration);
  }

  return recordsById.size > 0 || configuration !== null;
}

function readStringArrayField(value: Record<string, unknown> | null | undefined, key: string): string[] {
  return readArrayField(value, key).filter((entry): entry is string => typeof entry === 'string');
}

function readCapturedDiscountRecord(source: Record<string, unknown> | null): DiscountRecord | null {
  const id = readStringField(source, 'id');
  const discount =
    readRecordField(source, 'discount') ??
    readRecordField(source, 'codeDiscount') ??
    readRecordField(source, 'automaticDiscount');
  const typeName = readStringField(discount, '__typename');
  const title = readStringField(discount, 'title');
  if (!id || !discount || !typeName || !title) {
    return null;
  }

  const combinesWith = readRecordField(discount, 'combinesWith');
  const codes = readArrayField(readRecordField(discount, 'codes'), 'nodes')
    .filter(isPlainObject)
    .map((codeNode) => readStringField(codeNode, 'code'))
    .filter((code): code is string => typeof code === 'string' && code.length > 0);
  const redeemCodes = readArrayField(readRecordField(discount, 'codes'), 'nodes')
    .filter(isPlainObject)
    .map((codeNode) => {
      const code = readStringField(codeNode, 'code');
      const codeId = readStringField(codeNode, 'id');
      if (!code || !codeId) {
        return null;
      }

      return {
        id: codeId,
        code,
        asyncUsageCount: readNumberField(codeNode, 'asyncUsageCount') ?? 0,
      };
    })
    .filter((code): code is { id: string; code: string; asyncUsageCount: number } => code !== null);
  const context = readRecordField(discount, 'context');
  const customerGets = readRecordField(discount, 'customerGets');
  const customerGetsValue = readRecordField(customerGets, 'value');
  const customerGetsItems = readRecordField(customerGets, 'items');
  const minimumRequirement = readRecordField(discount, 'minimumRequirement');
  const minimumSubtotal = readRecordField(minimumRequirement, 'greaterThanOrEqualToSubtotal');
  const valueAmount = readRecordField(customerGetsValue, 'amount');
  const eventNodes = [
    ...readArrayField(readRecordField(source, 'events'), 'nodes').filter(isPlainObject),
    ...readArrayField(readRecordField(source, 'events'), 'edges')
      .filter(isPlainObject)
      .map((edge) => readRecordField(edge, 'node'))
      .filter((node): node is Record<string, unknown> => node !== null),
  ];

  return {
    id,
    typeName,
    method: typeName.toLowerCase().includes('code') ? 'code' : 'automatic',
    title,
    status: readStringField(discount, 'status'),
    summary: readStringField(discount, 'summary'),
    startsAt: readStringField(discount, 'startsAt'),
    endsAt: readStringField(discount, 'endsAt'),
    createdAt: readStringField(discount, 'createdAt'),
    updatedAt: readStringField(discount, 'updatedAt'),
    asyncUsageCount: readNumberField(discount, 'asyncUsageCount'),
    discountClasses: readStringArrayField(discount, 'discountClasses'),
    combinesWith: {
      productDiscounts: readBooleanField(combinesWith, 'productDiscounts') ?? false,
      orderDiscounts: readBooleanField(combinesWith, 'orderDiscounts') ?? false,
      shippingDiscounts: readBooleanField(combinesWith, 'shippingDiscounts') ?? false,
    },
    codes,
    redeemCodes,
    context: context
      ? {
          typeName: readStringField(context, '__typename') ?? 'DiscountBuyerSelectionAll',
          all: readNullableStringField(context, 'all'),
        }
      : null,
    customerGets:
      customerGets && customerGetsValue && customerGetsItems
        ? {
            value: {
              typeName: readStringField(customerGetsValue, '__typename') ?? 'DiscountPercentage',
              percentage: readNullableNumberField(customerGetsValue, 'percentage'),
              amount:
                readStringField(valueAmount, 'amount') && readStringField(valueAmount, 'currencyCode')
                  ? {
                      amount: readStringField(valueAmount, 'amount') as string,
                      currencyCode: readStringField(valueAmount, 'currencyCode') as string,
                    }
                  : null,
              appliesOnEachItem: readBooleanField(customerGetsValue, 'appliesOnEachItem'),
            },
            items: {
              typeName: readStringField(customerGetsItems, '__typename') ?? 'AllDiscountItems',
              allItems: readBooleanField(customerGetsItems, 'allItems'),
            },
            appliesOnOneTimePurchase: readBooleanField(customerGets, 'appliesOnOneTimePurchase') ?? true,
            appliesOnSubscription: readBooleanField(customerGets, 'appliesOnSubscription') ?? false,
          }
        : null,
    minimumRequirement: minimumRequirement
      ? {
          typeName: readStringField(minimumRequirement, '__typename') ?? 'DiscountMinimumSubtotal',
          greaterThanOrEqualToQuantity: readNullableStringField(minimumRequirement, 'greaterThanOrEqualToQuantity'),
          greaterThanOrEqualToSubtotal:
            readStringField(minimumSubtotal, 'amount') && readStringField(minimumSubtotal, 'currencyCode')
              ? {
                  amount: readStringField(minimumSubtotal, 'amount') as string,
                  currencyCode: readStringField(minimumSubtotal, 'currencyCode') as string,
                }
              : null,
        }
      : null,
    events: eventNodes
      .map((eventNode) => {
        const eventId = readStringField(eventNode, 'id');
        if (!eventId) {
          return null;
        }

        return {
          id: eventId,
          typeName: readStringField(eventNode, '__typename') ?? 'BasicEvent',
          action: readNullableStringField(eventNode, 'action'),
          message: readNullableStringField(eventNode, 'message'),
          createdAt: readNullableStringField(eventNode, 'createdAt'),
          subjectId: readNullableStringField(eventNode, 'subjectId'),
          subjectType: readNullableStringField(eventNode, 'subjectType'),
        };
      })
      .filter((event) => event !== null),
  };
}

function mergeCapturedDiscountRecord(existing: DiscountRecord, next: DiscountRecord): DiscountRecord {
  return {
    ...existing,
    ...next,
    status: next.status ?? existing.status,
    summary: next.summary ?? existing.summary,
    startsAt: next.startsAt ?? existing.startsAt,
    endsAt: next.endsAt ?? existing.endsAt,
    createdAt: next.createdAt ?? existing.createdAt,
    updatedAt: next.updatedAt ?? existing.updatedAt,
    asyncUsageCount: next.asyncUsageCount ?? existing.asyncUsageCount,
    discountClasses: next.discountClasses.length > 0 ? next.discountClasses : existing.discountClasses,
    codes: next.codes.length > 0 ? next.codes : existing.codes,
    redeemCodes: (next.redeemCodes ?? []).length > 0 ? next.redeemCodes : existing.redeemCodes,
    context: next.context ?? existing.context,
    customerGets: next.customerGets ?? existing.customerGets,
    minimumRequirement: next.minimumRequirement ?? existing.minimumRequirement,
    events: [...(existing.events ?? []), ...(next.events ?? [])],
  };
}

function seedDiscountCatalogPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const responseContainer = readRecordField(capture as Record<string, unknown>, 'response');
  const responseData =
    readRecordField(responseContainer, 'data') ??
    readRecordField(readRecordField(responseContainer, 'response'), 'data');
  const discountNodes = readRecordField(responseData, 'discountNodes');
  const automaticDiscountNodes = readRecordField(responseData, 'automaticDiscountNodes');
  const capturedNodes = readArrayField(discountNodes, 'nodes').filter(isPlainObject);
  const capturedEdgeNodes = readArrayField(discountNodes, 'edges')
    .filter(isPlainObject)
    .map((edge) => readRecordField(edge, 'node'))
    .filter((node): node is Record<string, unknown> => node !== null);
  const capturedAutomaticNodes = readArrayField(automaticDiscountNodes, 'nodes').filter(isPlainObject);
  const capturedAutomaticEdgeNodes = readArrayField(automaticDiscountNodes, 'edges')
    .filter(isPlainObject)
    .map((edge) => readRecordField(edge, 'node'))
    .filter((node): node is Record<string, unknown> => node !== null);
  const seedNodes = readArrayField(capture as Record<string, unknown>, 'seedDiscounts').filter(isPlainObject);
  const singularNodes = [
    readRecordField(responseData, 'codeDiscountNodeByCode'),
    readRecordField(responseData, 'automaticDiscountNode'),
    readRecordField(responseData, 'codeDiscountNode'),
    readRecordField(responseData, 'discountNode'),
  ].filter((node): node is Record<string, unknown> => node !== null);
  const discountsById = new Map<string, DiscountRecord>();

  for (const node of [
    ...capturedEdgeNodes,
    ...capturedAutomaticEdgeNodes,
    ...capturedNodes,
    ...capturedAutomaticNodes,
    ...singularNodes,
    ...seedNodes,
  ]) {
    const discount = readCapturedDiscountRecord(node);
    if (discount) {
      const existing = discountsById.get(discount.id);
      discountsById.set(discount.id, existing ? mergeCapturedDiscountRecord(existing, discount) : discount);
    }
  }

  if (discountsById.size === 0) {
    return false;
  }

  runtime.store.upsertBaseDiscounts([...discountsById.values()]);
  return true;
}

function seedShopifyFunctionPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const seedNodes = readArrayField(capture as Record<string, unknown>, 'seedShopifyFunctions').filter(isPlainObject);
  const functions: ShopifyFunctionRecord[] = seedNodes
    .map((node): ShopifyFunctionRecord | null => {
      const id = readStringField(node, 'id');
      if (!id) {
        return null;
      }
      const app = readRecordField(node, 'app');

      return {
        id,
        title: readNullableStringField(node, 'title'),
        handle: readNullableStringField(node, 'handle'),
        apiType: readNullableStringField(node, 'apiType'),
        description: readNullableStringField(node, 'description') ?? undefined,
        appKey: readNullableStringField(node, 'appKey') ?? undefined,
        ...(app ? { app: app as ShopifyFunctionRecord['app'] } : {}),
      };
    })
    .filter((shopifyFunction): shopifyFunction is ShopifyFunctionRecord => shopifyFunction !== null);

  for (const shopifyFunction of functions) {
    runtime.store.upsertStagedShopifyFunction(shopifyFunction);
  }

  return functions.length > 0;
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
    id: readStringField(address, 'id'),
    firstName: readStringField(address, 'firstName'),
    lastName: readStringField(address, 'lastName'),
    address1: readStringField(address, 'address1'),
    city: readStringField(address, 'city'),
    province: readStringField(address, 'province'),
    provinceCode: readStringField(address, 'provinceCode'),
    country: readStringField(address, 'country'),
    countryCodeV2: readStringField(address, 'countryCodeV2'),
    zip: readStringField(address, 'zip'),
    formattedArea: readStringField(address, 'formattedArea'),
  };
}

function makeSeedCustomerAddress(
  customerId: string,
  address: Record<string, unknown>,
  position: number,
): CustomerAddressRecord | null {
  const id = readStringField(address, 'id');
  if (!id) {
    return null;
  }

  const provinceCode = readStringField(address, 'provinceCode');
  const countryCode = readStringField(address, 'countryCodeV2');
  const city = readStringField(address, 'city');
  const country = readStringField(address, 'country');
  const formattedArea = [city, provinceCode, country ?? countryCode].filter(Boolean).join(', ') || null;

  return {
    id,
    customerId,
    cursor: null,
    position,
    firstName: readStringField(address, 'firstName'),
    lastName: readStringField(address, 'lastName'),
    address1: readStringField(address, 'address1'),
    address2: readStringField(address, 'address2'),
    city,
    company: readStringField(address, 'company'),
    province: readStringField(address, 'province'),
    provinceCode,
    country,
    countryCodeV2: countryCode,
    zip: readStringField(address, 'zip'),
    phone: readStringField(address, 'phone'),
    name: readStringField(address, 'name'),
    formattedArea,
  };
}

function readCustomerAddressRecords(
  customerId: string,
  customer: Record<string, unknown> | null,
): CustomerAddressRecord[] {
  return readArrayField(readRecordField(customer, 'addressesV2'), 'nodes')
    .filter(isPlainObject)
    .map((address, index) => makeSeedCustomerAddress(customerId, address, index))
    .filter((address): address is CustomerAddressRecord => address !== null);
}

function readCustomerMetafieldRecords(
  customerId: string,
  customer: Record<string, unknown> | null,
): CustomerMetafieldRecord[] {
  return readArrayField(readRecordField(customer, 'metafields'), 'nodes')
    .filter(isPlainObject)
    .map((metafield): CustomerMetafieldRecord | null => {
      const id = readStringField(metafield, 'id');
      const namespace = readStringField(metafield, 'namespace');
      const key = readStringField(metafield, 'key');
      if (!id || !namespace || !key) {
        return null;
      }

      return {
        id,
        customerId,
        namespace,
        key,
        type: readStringField(metafield, 'type'),
        value: readStringField(metafield, 'value'),
      };
    })
    .filter((metafield): metafield is CustomerMetafieldRecord => metafield !== null);
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
    dataSaleOptOut: readBooleanField(source, 'dataSaleOptOut') ?? false,
    taxExempt: readBooleanField(source, 'taxExempt') ?? false,
    taxExemptions: readArrayField(source, 'taxExemptions').filter(
      (taxExemption): taxExemption is string => typeof taxExemption === 'string',
    ),
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
    dataSaleOptOut: false,
    taxExempt: false,
    taxExemptions: [],
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

function makeSeedSegment(segmentId: string, source: Record<string, unknown> | null = null): SegmentRecord {
  return {
    id: segmentId,
    name: readStringField(source, 'name'),
    query: readStringField(source, 'query'),
    creationDate: readStringField(source, 'creationDate'),
    lastEditDate: readStringField(source, 'lastEditDate'),
  };
}

function readSeedCustomerPaymentMethod(rawPaymentMethod: Record<string, unknown>): CustomerPaymentMethodRecord | null {
  const id = readStringField(rawPaymentMethod, 'id');
  const customerId = readStringField(rawPaymentMethod, 'customerId');
  if (!id?.startsWith('gid://shopify/CustomerPaymentMethod/') || !customerId?.startsWith('gid://shopify/Customer/')) {
    return null;
  }

  const rawInstrument = readRecordField(rawPaymentMethod, 'instrument');
  const rawInstrumentData = readRecordField(rawInstrument, 'data') ?? rawInstrument;
  const typeName = readStringField(rawInstrument, 'typeName') ?? readStringField(rawInstrumentData, '__typename');
  const instrument = typeName
    ? {
        typeName,
        data: structuredClone(rawInstrumentData ?? { __typename: typeName }) as NonNullable<
          CustomerPaymentMethodRecord['instrument']
        >['data'],
      }
    : null;

  return {
    id,
    customerId,
    cursor: readNullableStringField(rawPaymentMethod, 'cursor') ?? undefined,
    instrument,
    revokedAt: readNullableStringField(rawPaymentMethod, 'revokedAt'),
    revokedReason: readNullableStringField(rawPaymentMethod, 'revokedReason') ?? undefined,
    subscriptionContracts: [],
  };
}

function seedCustomerPaymentMethodPreconditions(runtime: ProxyRuntimeContext, capture: unknown): void {
  const seedCustomers = readArrayField(capture as Record<string, unknown>, 'seedCustomers')
    .filter(isPlainObject)
    .map((customer) => {
      const customerId = readStringField(customer, 'id');
      return customerId?.startsWith('gid://shopify/Customer/') ? makeSeedCustomer(customerId, customer) : null;
    })
    .filter((customer): customer is CustomerRecord => customer !== null);
  if (seedCustomers.length > 0) {
    runtime.store.upsertBaseCustomers(seedCustomers);
  }

  const seedPaymentMethods = readArrayField(capture as Record<string, unknown>, 'seedCustomerPaymentMethods')
    .filter(isPlainObject)
    .map(readSeedCustomerPaymentMethod)
    .filter((paymentMethod): paymentMethod is CustomerPaymentMethodRecord => paymentMethod !== null);
  if (seedPaymentMethods.length > 0) {
    runtime.store.upsertBaseCustomerPaymentMethods(seedPaymentMethods);
  }
}

function seedCustomerMutationPreconditions(
  runtime: ProxyRuntimeContext,
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
    mutationName !== 'customerSmsMarketingConsentUpdate' &&
    mutationName !== 'dataSaleOptOut' &&
    mutationName !== 'customerRequestDataErasure' &&
    mutationName !== 'customerCancelDataErasure' &&
    mutationName !== 'customerAddTaxExemptions' &&
    mutationName !== 'customerRemoveTaxExemptions' &&
    mutationName !== 'customerReplaceTaxExemptions'
  ) {
    return false;
  }

  const input = readRecordField(variables, 'input');
  const customerPayload = readRecordField(payload, 'customer');
  const preconditionPayload =
    firstObjectValue(readJsonPath(capture, '$.precondition.response.data')) ??
    readCustomerCreatePayloadFromCapture(capture as Record<string, unknown>, 'customerCreate');
  const preconditionCustomerPayload = readRecordField(preconditionPayload, 'customer');
  const downstreamRead = readRecordField(capture as Record<string, unknown>, 'downstreamRead');
  const downstreamData =
    readRecordField(downstreamRead, 'data') ?? readRecordField(readRecordField(downstreamRead, 'response'), 'data');
  const downstreamCount = readNumberField(readRecordField(downstreamData, 'customersCount'), 'count');
  const targetCustomerId =
    readStringField(variables, 'customerId') ??
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
      mutationName === 'customerCreate' ||
      mutationName === 'customerUpdate' ||
      mutationName === 'customerAddTaxExemptions' ||
      mutationName === 'customerRemoveTaxExemptions' ||
      mutationName === 'customerReplaceTaxExemptions'
        ? 1
        : 0;
    const placeholderCount = Math.max(0, downstreamCount - targetContributesToDownstreamCount);
    for (let index = 0; index < placeholderCount; index += 1) {
      seedCustomers.push(makePlaceholderCustomer(index));
    }
  }

  if (seedCustomers.length > 0) {
    runtime.store.upsertBaseCustomers(seedCustomers);
  }

  return true;
}

function readStoreCreditMoney(
  value: Record<string, unknown> | null | undefined,
): StoreCreditAccountRecord['balance'] | null {
  const amount = readStringField(value, 'amount');
  const currencyCode = readStringField(value, 'currencyCode');
  if (!amount || !currencyCode) {
    return null;
  }

  return {
    amount,
    currencyCode,
  };
}

function seedStoreCreditAccountPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  if (!isPlainObject(capture)) {
    return false;
  }

  const setupCreditPayload = readRecordField(
    readRecordField(readRecordField(readRecordField(capture, 'setup'), 'createAccountCredit'), 'response'),
    'data',
  );
  const setupTransaction = readRecordField(
    readRecordField(setupCreditPayload, 'storeCreditAccountCredit'),
    'storeCreditAccountTransaction',
  );
  const accountPayload = readRecordField(setupTransaction, 'account');
  const accountId = readStringField(accountPayload, 'id');
  const createdCustomerPayload = readRecordField(
    readRecordField(
      readRecordField(readRecordField(readRecordField(capture, 'setup'), 'createCustomer'), 'response'),
      'data',
    ),
    'customerCreate',
  );
  const createdCustomer = readRecordField(createdCustomerPayload, 'customer');
  const customerId =
    readStringField(readRecordField(accountPayload, 'owner'), 'id') ?? readStringField(createdCustomer, 'id');
  const balance = readStoreCreditMoney(readRecordField(accountPayload, 'balance'));

  if (!accountId || !customerId || !balance) {
    return false;
  }

  const customerPayload = createdCustomer ?? readRecordField(accountPayload, 'owner');
  runtime.store.upsertBaseCustomers([makeSeedCustomer(customerId, customerPayload)]);
  runtime.store.upsertBaseStoreCreditAccounts([
    {
      id: accountId,
      customerId,
      cursor: null,
      balance,
    },
  ]);

  return true;
}

function seedCustomerOrderSummaryPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const seedOrder = readArrayField(
    readRecordField(
      readRecordField(
        readRecordField(readRecordField(capture as Record<string, unknown>, 'seedOrder'), 'response'),
        'data',
      ),
      'orders',
    ),
    'nodes',
  )
    .filter(isPlainObject)
    .at(0);
  const seedOrderId = readStringField(seedOrder, 'id');
  if (!seedOrder || !seedOrderId) {
    return false;
  }

  runtime.store.upsertBaseOrders([makeSeedOrder(seedOrderId, seedOrder)]);

  const beforeSetCount = readNumberField(
    readRecordField(
      readRecordField(
        readRecordField(readRecordField(capture as Record<string, unknown>, 'beforeSet'), 'response'),
        'data',
      ),
      'customersCount',
    ),
    'count',
  );
  if (beforeSetCount !== null) {
    const placeholders = [];
    for (let index = 0; index < Math.max(0, beforeSetCount - 1); index += 1) {
      placeholders.push(makePlaceholderCustomer(index));
    }
    runtime.store.upsertBaseCustomers(placeholders);
  }

  return true;
}

function readCustomerCreatePayloadFromCapture(
  capture: Record<string, unknown>,
  key: string,
): Record<string, unknown> | null {
  return readRecordField(
    readRecordField(
      readRecordField(readRecordField(readRecordField(capture, 'precondition'), key), 'response'),
      'data',
    ),
    'customerCreate',
  );
}

function seedCustomerMergePreconditions(
  runtime: ProxyRuntimeContext,
  capture: unknown,
  _variables: Record<string, unknown>,
  mutationName: string | null,
): boolean {
  if (mutationName !== 'customerMerge' || !isPlainObject(capture)) {
    return false;
  }

  const seedCustomers: CustomerRecord[] = [];
  const seedAddresses: CustomerAddressRecord[] = [];
  const seedMetafieldsByCustomerId = new Map<string, CustomerMetafieldRecord[]>();
  for (const key of ['createOne', 'createTwo']) {
    const customerPayload = readRecordField(readCustomerCreatePayloadFromCapture(capture, key), 'customer');
    const customerId = readStringField(customerPayload, 'id');
    if (customerId) {
      seedCustomers.push(makeSeedCustomer(customerId, customerPayload));
    }
  }

  const attachedBeforeData = readRecordField(
    readRecordField(readRecordField(readRecordField(capture, 'precondition'), 'attachedBeforeMerge'), 'response'),
    'data',
  );
  for (const key of ['source', 'result']) {
    const customerPayload = readRecordField(attachedBeforeData, key);
    const customerId = readStringField(customerPayload, 'id');
    if (!customerId) {
      continue;
    }

    const attachedCustomer = makeSeedCustomer(customerId, customerPayload);
    const existingIndex = seedCustomers.findIndex((customer) => customer.id === customerId);
    if (existingIndex === -1) {
      seedCustomers.push(attachedCustomer);
    } else {
      seedCustomers[existingIndex] = attachedCustomer;
    }
    seedAddresses.push(...readCustomerAddressRecords(customerId, customerPayload));
    seedMetafieldsByCustomerId.set(customerId, readCustomerMetafieldRecords(customerId, customerPayload));
  }

  const order = readRecordField(
    readRecordField(
      readRecordField(readRecordField(readRecordField(capture, 'precondition'), 'orderCreate'), 'response'),
      'data',
    ),
    'orderCreate',
  );
  const orderPayload = readRecordField(order, 'order');
  const orderId = readStringField(orderPayload, 'id');

  const downstreamData = readRecordField(
    readRecordField(readRecordField(capture, 'downstreamRead'), 'response'),
    'data',
  );
  const downstreamCount = readNumberField(readRecordField(downstreamData, 'customersCount'), 'count');
  if (downstreamCount !== null) {
    const placeholderCount = Math.max(0, downstreamCount - 1);
    for (let index = 0; index < placeholderCount; index += 1) {
      seedCustomers.push(makePlaceholderCustomer(index));
    }
  }

  if (seedCustomers.length === 0) {
    return false;
  }

  runtime.store.upsertBaseCustomers(seedCustomers);
  if (seedAddresses.length > 0) {
    runtime.store.upsertBaseCustomerAddresses(seedAddresses);
  }
  for (const [customerId, metafields] of seedMetafieldsByCustomerId) {
    runtime.store.replaceBaseMetafieldsForCustomer(customerId, metafields);
  }
  if (orderId) {
    runtime.store.upsertBaseOrders([makeSeedOrder(orderId, orderPayload)]);
  }
  return true;
}

function seedCustomerByIdentifierPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
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

  runtime.store.upsertBaseCustomers([...seedCustomers.values()]);
  return true;
}

function readCustomerFromCapturedCreate(source: Record<string, unknown> | null): Record<string, unknown> | null {
  return readRecordField(
    readRecordField(readRecordField(readRecordField(source, 'response'), 'data'), 'customerCreate'),
    'customer',
  );
}

function seedCustomerInputValidationPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  if (!isPlainObject(capture)) {
    return false;
  }

  const createScenarios = readRecordField(capture, 'createScenarios');
  const updateScenarios = readRecordField(capture, 'updateScenarios');
  if (!createScenarios || !updateScenarios) {
    return false;
  }

  const seedCustomers = new Map<string, CustomerRecord>();
  const addCustomerPayload = (payload: Record<string, unknown> | null): void => {
    const customerId = readStringField(payload, 'id');
    if (!customerId || seedCustomers.has(customerId)) {
      return;
    }
    seedCustomers.set(customerId, makeSeedCustomer(customerId, payload));
  };
  const addCapturedCreate = (source: Record<string, unknown> | null): void => {
    addCustomerPayload(readCustomerFromCapturedCreate(source));
  };
  const addBaseCustomer = (baseCustomer: Record<string, unknown> | null): void => {
    const customerId = readStringField(baseCustomer, 'id');
    if (!customerId || seedCustomers.has(customerId)) {
      return;
    }
    const email = readStringField(baseCustomer, 'email');
    const phone = readStringField(baseCustomer, 'phone');
    seedCustomers.set(
      customerId,
      makeSeedCustomer(customerId, {
        id: customerId,
        email,
        displayName: email,
        locale: 'en',
        verifiedEmail: email ? true : null,
        taxExempt: false,
        tags: ['input-validation'],
        defaultEmailAddress: email ? { emailAddress: email } : null,
        defaultPhoneNumber: phone ? { phoneNumber: phone } : null,
      }),
    );
  };

  const preconditions = readRecordField(capture, 'preconditions');
  for (const key of ['primary', 'duplicateTarget']) {
    addCapturedCreate(readRecordField(preconditions, key));
  }

  for (const scenario of Object.values(updateScenarios)) {
    if (isPlainObject(scenario)) {
      addBaseCustomer(readRecordField(scenario, 'baseCustomer'));
    }
  }

  const deletedCustomerUpdate = readRecordField(capture, 'deletedCustomerUpdate');
  addCapturedCreate(readRecordField(deletedCustomerUpdate, 'precondition'));

  const mergedCustomerUpdate = readRecordField(capture, 'mergedCustomerUpdate');
  addCapturedCreate(readRecordField(mergedCustomerUpdate, 'mergeSource'));
  addCapturedCreate(readRecordField(mergedCustomerUpdate, 'mergeTarget'));

  if (seedCustomers.size === 0) {
    return false;
  }

  runtime.store.upsertBaseCustomers([...seedCustomers.values()]);
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

function seedShopPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const captureRoot = isPlainObject(capture) ? capture : {};
  const directData = readRecordField(captureRoot, 'data');
  const shopBaseline = readRecordField(readRecordField(captureRoot, 'readOnlyBaselines'), 'shop');
  const baselineData = readRecordField(shopBaseline, 'data');
  const shop = readShopRecord(readRecordField(directData ?? baselineData, 'shop'));

  if (!shop) {
    return false;
  }

  runtime.store.upsertBaseShop(shop);
  return true;
}

function readLocationAddressRecord(source: Record<string, unknown> | null): LocationRecord['address'] {
  if (!source) {
    return null;
  }

  return {
    address1: readNullableStringField(source, 'address1'),
    address2: readNullableStringField(source, 'address2'),
    city: readNullableStringField(source, 'city'),
    country: readNullableStringField(source, 'country'),
    countryCode: readNullableStringField(source, 'countryCode'),
    formatted: readStringArrayField(source, 'formatted'),
    latitude: readNullableNumberField(source, 'latitude'),
    longitude: readNullableNumberField(source, 'longitude'),
    phone: readNullableStringField(source, 'phone'),
    province: readNullableStringField(source, 'province'),
    provinceCode: readNullableStringField(source, 'provinceCode'),
    zip: readNullableStringField(source, 'zip'),
  };
}

function readDeliveryLocalPickupSettingsRecord(
  source: Record<string, unknown> | null,
): DeliveryLocalPickupSettingsRecord | null {
  const pickupTime = readStringField(source, 'pickupTime');
  if (!pickupTime) {
    return null;
  }

  return {
    pickupTime,
    instructions: readStringField(source, 'instructions') ?? '',
  };
}

function readLocationRecord(source: Record<string, unknown> | null): LocationRecord | null {
  const id = readStringField(source, 'id');
  if (!id) {
    return null;
  }

  const fulfillmentService = readRecordField(source, 'fulfillmentService');

  return {
    id,
    name: readNullableStringField(source, 'name'),
    legacyResourceId: readNullableStringField(source, 'legacyResourceId'),
    activatable: readBooleanField(source, 'activatable'),
    addressVerified: readBooleanField(source, 'addressVerified'),
    createdAt: readNullableStringField(source, 'createdAt'),
    deactivatable: readBooleanField(source, 'deactivatable'),
    deactivatedAt: readNullableStringField(source, 'deactivatedAt'),
    deletable: readBooleanField(source, 'deletable'),
    fulfillmentService: fulfillmentService
      ? {
          id: readNullableStringField(fulfillmentService, 'id'),
          handle: readNullableStringField(fulfillmentService, 'handle'),
          serviceName: readNullableStringField(fulfillmentService, 'serviceName'),
        }
      : null,
    fulfillsOnlineOrders: readBooleanField(source, 'fulfillsOnlineOrders'),
    hasActiveInventory: readBooleanField(source, 'hasActiveInventory'),
    hasUnfulfilledOrders: readBooleanField(source, 'hasUnfulfilledOrders'),
    isActive: readBooleanField(source, 'isActive'),
    isFulfillmentService: readBooleanField(source, 'isFulfillmentService'),
    shipsInventory: readBooleanField(source, 'shipsInventory'),
    updatedAt: readNullableStringField(source, 'updatedAt'),
    address: readLocationAddressRecord(readRecordField(source, 'address')),
    suggestedAddresses: readArrayField(source, 'suggestedAddresses')
      .filter(isPlainObject)
      .map((address) => ({
        address1: readNullableStringField(address, 'address1'),
        countryCode: readNullableStringField(address, 'countryCode'),
        formatted: readStringArrayField(address, 'formatted'),
      })),
    localPickupSettings: readDeliveryLocalPickupSettingsRecord(
      readRecordField(source, 'localPickupSettings') ?? readRecordField(source, 'localPickupSettingsV2'),
    ),
  };
}

function readShippingSettingsCarrierServiceRecord(source: Record<string, unknown> | null): CarrierServiceRecord | null {
  const id = readStringField(source, 'id');
  if (!id) {
    return null;
  }

  return {
    id,
    name: readNullableStringField(source, 'name'),
    formattedName: readNullableStringField(source, 'formattedName'),
    callbackUrl: readNullableStringField(source, 'callbackUrl'),
    active: readBooleanField(source, 'active') ?? true,
    supportsServiceDiscovery: readBooleanField(source, 'supportsServiceDiscovery') ?? false,
    createdAt: readStringField(source, 'createdAt') ?? '1970-01-01T00:00:00.000Z',
    updatedAt: readStringField(source, 'updatedAt') ?? '1970-01-01T00:00:00.000Z',
  };
}

function seedShippingSettingsPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const seed = readRecordField(capture as Record<string, unknown>, 'seed');
  const carrierServices = readArrayField(seed, 'carrierServices')
    .filter(isPlainObject)
    .map((service) => readShippingSettingsCarrierServiceRecord(service))
    .filter((service): service is CarrierServiceRecord => service !== null);
  const locations = readArrayField(seed, 'locations')
    .filter(isPlainObject)
    .map((location) => readLocationRecord(location))
    .filter((location): location is LocationRecord => location !== null);

  if (carrierServices.length === 0 && locations.length === 0) {
    return false;
  }

  if (carrierServices.length > 0) {
    runtime.store.upsertBaseCarrierServices(carrierServices);
  }
  if (locations.length > 0) {
    runtime.store.upsertBaseLocations(locations);
  }

  return true;
}

function makeLocationDetailSeedVariant(
  productId: string,
  level: InventoryLevelRecord,
  index: number,
): ProductVariantRecord | null {
  const rawInventoryItemId = level.id.includes('?inventory_item_id=')
    ? decodeURIComponent(level.id.split('?inventory_item_id=').at(-1) ?? '')
    : null;
  const itemId =
    rawInventoryItemId && rawInventoryItemId.startsWith('gid://shopify/InventoryItem/')
      ? rawInventoryItemId
      : rawInventoryItemId
        ? `gid://shopify/InventoryItem/${rawInventoryItemId}`
        : `gid://shopify/InventoryItem/location-detail-${index}`;

  return {
    id: `gid://shopify/ProductVariant/location-detail-${index}`,
    productId,
    title: `Location detail seed variant ${index + 1}`,
    sku: null,
    barcode: null,
    price: null,
    compareAtPrice: null,
    taxable: null,
    inventoryPolicy: null,
    inventoryQuantity: level.quantities.find((quantity) => quantity.name === 'available')?.quantity ?? null,
    selectedOptions: [],
    inventoryItem: {
      id: itemId,
      tracked: true,
      requiresShipping: true,
      measurement: null,
      countryCodeOfOrigin: null,
      provinceCodeOfOrigin: null,
      harmonizedSystemCode: null,
      inventoryLevels: [level],
    },
  };
}

function readCapturedLocationInventoryLevels(location: Record<string, unknown> | null): InventoryLevelRecord[] {
  const connection = readRecordField(location, 'inventoryLevels');
  const pageInfo = readRecordField(connection, 'pageInfo');
  const rawLevels = readArrayField(connection, 'nodes').filter(isPlainObject);
  const levels = rawLevels
    .map((level, index) => {
      const capturedLevel = readCapturedInventoryLevel(level);
      if (!capturedLevel) {
        return null;
      }

      const startCursor = readStringField(pageInfo, 'startCursor');
      const endCursor = readStringField(pageInfo, 'endCursor');
      return {
        ...capturedLevel,
        cursor: index === 0 ? startCursor : index === rawLevels.length - 1 ? endCursor : capturedLevel.id,
      };
    })
    .filter((level): level is InventoryLevelRecord => level !== null);

  if (readBooleanField(pageInfo, 'hasNextPage') === true && levels.length > 0) {
    const last = levels[levels.length - 1]!;
    levels.push({
      ...structuredClone(last),
      id: `${last.id}#unread-page`,
      cursor: `${last.cursor ?? last.id}#unread-page`,
    });
  }

  return levels;
}

function seedLocationDetailPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const locationData = readRecordField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'readOnlyBaselines'), 'location'),
    'data',
  );
  const catalogLocations = readArrayField(
    readRecordField(
      readRecordField(
        readRecordField(readRecordField(capture as Record<string, unknown>, 'readOnlyBaselines'), 'locationsCatalog'),
        'data',
      ),
      'locations',
    ),
    'edges',
  )
    .filter(isPlainObject)
    .map((edge) => readLocationRecord(readRecordField(edge, 'node')))
    .filter((location): location is LocationRecord => location !== null);

  const detailedLocations = ['primary', 'byId', 'byIdentifier']
    .map((key) => readLocationRecord(readRecordField(locationData, key)))
    .filter((location): location is LocationRecord => location !== null);
  const locationsById = new Map(catalogLocations.map((location) => [location.id, location]));
  for (const location of detailedLocations) {
    locationsById.set(location.id, { ...locationsById.get(location.id), ...location });
  }

  if (locationsById.size === 0) {
    return false;
  }

  runtime.store.upsertBaseLocations([...locationsById.values()]);

  const primaryLocation = readRecordField(locationData, 'primary') ?? readRecordField(locationData, 'byId');
  const levels = readCapturedLocationInventoryLevels(primaryLocation);
  if (levels.length > 0) {
    const productId = 'gid://shopify/Product/location-detail-seed';
    runtime.store.upsertBaseProducts([makeSeedProduct(productId, { totalInventory: null, tracksInventory: true })]);
    runtime.store.replaceBaseVariantsForProduct(
      productId,
      levels
        .map((level, index) => makeLocationDetailSeedVariant(productId, level, index))
        .filter((variant): variant is ProductVariantRecord => variant !== null),
    );
  }

  return true;
}

function readDeliveryProfilesCatalogPayload(capture: unknown, key = 'catalogFirst'): Record<string, unknown> | null {
  return readRecordField(
    readRecordField(
      readRecordField(readRecordField(readRecordField(capture as Record<string, unknown>, 'queries'), key), 'result'),
      'payload',
    ),
    'data',
  );
}

function readDeliveryProfileDetailPayload(capture: unknown): Record<string, unknown> | null {
  return readRecordField(
    readRecordField(
      readRecordField(
        readRecordField(readRecordField(capture as Record<string, unknown>, 'queries'), 'detail'),
        'result',
      ),
      'payload',
    ),
    'data',
  );
}

function readConnectionEntries(connection: Record<string, unknown> | null): Array<{
  node: Record<string, unknown>;
  cursor: string | null;
}> {
  const edgeEntries = readArrayField(connection, 'edges')
    .filter(isPlainObject)
    .map((edge) => ({
      node: readRecordField(edge, 'node'),
      cursor: readStringField(edge, 'cursor'),
    }))
    .filter((entry): entry is { node: Record<string, unknown>; cursor: string | null } => entry.node !== null);
  if (edgeEntries.length > 0) {
    return edgeEntries;
  }

  const nodes = readArrayField(connection, 'nodes').filter(isPlainObject);
  const startCursor = readConnectionStartCursor(connection);
  const endCursor = readConnectionEndCursor(connection);
  return nodes.map((node, index) => ({
    node,
    cursor:
      index === 0
        ? (startCursor ?? null)
        : index === nodes.length - 1
          ? (endCursor ?? null)
          : readStringField(node, 'id'),
  }));
}

function readConnectionNodes(connection: Record<string, unknown> | null): Record<string, unknown>[] {
  const edgeEntries = readConnectionEntries(connection);
  if (edgeEntries.length > 0) {
    return edgeEntries.map((entry) => entry.node);
  }

  return readArrayField(connection, 'nodes').filter(isPlainObject);
}

function readConnectionStartCursor(connection: Record<string, unknown> | null): string | null {
  return readStringField(readRecordField(connection, 'pageInfo'), 'startCursor');
}

function readConnectionEndCursor(connection: Record<string, unknown> | null): string | null {
  return readStringField(readRecordField(connection, 'pageInfo'), 'endCursor');
}

function readConnectionHasNextPage(connection: Record<string, unknown> | null): boolean {
  return readBooleanField(readRecordField(connection, 'pageInfo'), 'hasNextPage') === true;
}

function readDeliveryProfileCount(
  source: Record<string, unknown> | null,
): DeliveryProfileRecord['productVariantsCount'] {
  if (!source) {
    return null;
  }

  const count = readNumberField(source, 'count');
  if (count === null) {
    return null;
  }

  return {
    count,
    precision: readStringField(source, 'precision') ?? 'EXACT',
  };
}

function readDeliveryProfileCountry(source: Record<string, unknown> | null): DeliveryProfileCountryRecord | null {
  const id = readStringField(source, 'id');
  const name = readStringField(source, 'name');
  if (!id || !name) {
    return null;
  }

  const code = readRecordField(source, 'code');
  return {
    id,
    name,
    translatedName: readStringField(source, 'translatedName') ?? name,
    code: {
      countryCode: readStringField(code, 'countryCode'),
      restOfWorld: readBooleanField(code, 'restOfWorld') ?? false,
    },
    provinces: readArrayField(source, 'provinces')
      .filter(isPlainObject)
      .map((province) => {
        const provinceId = readStringField(province, 'id');
        const provinceName = readStringField(province, 'name');
        const provinceCode = readStringField(province, 'code');
        return provinceId && provinceName && provinceCode
          ? {
              id: provinceId,
              name: provinceName,
              code: provinceCode,
            }
          : null;
      })
      .filter(
        (province): province is NonNullable<DeliveryProfileCountryRecord['provinces'][number]> => province !== null,
      ),
  };
}

function readDeliveryProfileCountries(source: Record<string, unknown> | null): DeliveryProfileCountryRecord[] {
  return readArrayField(source, 'countries')
    .filter(isPlainObject)
    .map(readDeliveryProfileCountry)
    .filter((country): country is DeliveryProfileCountryRecord => country !== null);
}

function readDeliveryProfileMethodDefinition(
  source: Record<string, unknown>,
  cursor: string | null,
): DeliveryProfileMethodDefinitionRecord | null {
  const id = readStringField(source, 'id');
  const name = readStringField(source, 'name');
  const active = readBooleanField(source, 'active');
  if (!id || !name || active === null) {
    return null;
  }

  const rateProvider = readRecordField(source, 'rateProvider') ?? {};
  const rateProviderTypename =
    readRecordField(rateProvider, 'price') !== null
      ? 'DeliveryRateDefinition'
      : readRecordField(rateProvider, 'fixedFee') !== null
        ? 'DeliveryParticipant'
        : null;

  return {
    id,
    ...(cursor ? { cursor } : {}),
    name,
    active,
    description: readNullableStringField(source, 'description'),
    rateProvider: {
      ...(rateProviderTypename ? { __typename: rateProviderTypename } : {}),
      ...structuredClone(rateProvider),
    },
    methodConditions: readArrayField(source, 'methodConditions')
      .filter(isPlainObject)
      .map((condition) => {
        const conditionId = readStringField(condition, 'id');
        const field = readStringField(condition, 'field');
        const operator = readStringField(condition, 'operator');
        const conditionCriteria = readRecordField(condition, 'conditionCriteria');
        return conditionId && field && operator && conditionCriteria
          ? {
              id: conditionId,
              field,
              operator,
              conditionCriteria: structuredClone(conditionCriteria),
            }
          : null;
      })
      .filter(
        (condition): condition is DeliveryProfileMethodDefinitionRecord['methodConditions'][number] =>
          condition !== null,
      ),
  };
}

function readDeliveryProfileLocationGroupZone(
  source: Record<string, unknown>,
  cursor: string | null,
): DeliveryProfileLocationGroupZoneRecord | null {
  const zone = readRecordField(source, 'zone');
  const zoneId = readStringField(zone, 'id');
  const zoneName = readStringField(zone, 'name');
  if (!zoneId || !zoneName) {
    return null;
  }

  const methodDefinitionsConnection = readRecordField(source, 'methodDefinitions');
  const methodDefinitionEntries = readConnectionEntries(methodDefinitionsConnection);
  const methodDefinitions = methodDefinitionEntries
    .map((entry) => readDeliveryProfileMethodDefinition(entry.node, entry.cursor))
    .filter((methodDefinition): methodDefinition is DeliveryProfileMethodDefinitionRecord => methodDefinition !== null);
  if (readConnectionHasNextPage(methodDefinitionsConnection) && methodDefinitions.length > 0) {
    const last = methodDefinitions[methodDefinitions.length - 1]!;
    methodDefinitions.push({
      ...structuredClone(last),
      id: `${last.id}#unread-page`,
      cursor: `${last.cursor ?? last.id}#unread-page`,
    });
  }

  return {
    ...(cursor ? { cursor } : {}),
    zone: {
      id: zoneId,
      name: zoneName,
      countries: readDeliveryProfileCountries(zone),
    },
    methodDefinitions,
  };
}

function makeDeliveryProfileSeedVariant(
  productId: string,
  variant: Record<string, unknown>,
  index: number,
): ProductVariantRecord | null {
  const id = readStringField(variant, 'id');
  if (!id) {
    return null;
  }

  return {
    id,
    productId,
    title: readStringField(variant, 'title') ?? `Delivery profile variant ${index + 1}`,
    sku: null,
    barcode: null,
    price: null,
    compareAtPrice: null,
    taxable: null,
    inventoryPolicy: null,
    inventoryQuantity: null,
    selectedOptions: [],
    inventoryItem: null,
  };
}

function readDeliveryProfileRecord(
  runtime: ProxyRuntimeContext,
  source: Record<string, unknown>,
  cursor: string | null,
): DeliveryProfileRecord | null {
  const id = readStringField(source, 'id');
  const name = readStringField(source, 'name');
  const defaultProfile = readBooleanField(source, 'default');
  if (!id || !name || defaultProfile === null) {
    return null;
  }

  const profileItemsConnection = readRecordField(source, 'profileItems');
  const profileItemNodes = readConnectionNodes(profileItemsConnection);
  const profileItemStartCursor = readConnectionStartCursor(profileItemsConnection);
  const profileItemEndCursor = readConnectionEndCursor(profileItemsConnection);
  type DeliveryProfileItemSeed = DeliveryProfileRecord['profileItems'][number];
  const profileItems: DeliveryProfileItemSeed[] = profileItemNodes
    .map((item, index): DeliveryProfileItemSeed | null => {
      const product = readRecordField(item, 'product');
      const productId = readStringField(product, 'id');
      if (!productId) {
        return null;
      }

      runtime.store.upsertBaseProducts([makeSeedProduct(productId, product, 'Delivery profile product')]);
      const variantsConnection = readRecordField(item, 'variants');
      const variantNodes = readConnectionNodes(variantsConnection);
      const variants = variantNodes
        .map((variant, variantIndex) => makeDeliveryProfileSeedVariant(productId, variant, variantIndex))
        .filter((variant): variant is ProductVariantRecord => variant !== null);
      runtime.store.replaceBaseVariantsForProduct(productId, variants);

      const variantStartCursor = readConnectionStartCursor(variantsConnection);
      const variantEndCursor = readConnectionEndCursor(variantsConnection);
      const variantCursors = Object.fromEntries(
        variants.map((variant, variantIndex) => [
          variant.id,
          variantIndex === 0
            ? (variantStartCursor ?? variant.id)
            : variantIndex === variants.length - 1
              ? (variantEndCursor ?? variant.id)
              : variant.id,
        ]),
      );
      if (readConnectionHasNextPage(variantsConnection) && variants.length > 0) {
        const last = variants[variants.length - 1]!;
        const unreadVariantId = `${last.id}#unread-page`;
        runtime.store.replaceBaseVariantsForProduct(productId, [
          ...variants,
          {
            ...structuredClone(last),
            id: unreadVariantId,
          },
        ]);
        variantCursors[unreadVariantId] = `${variantCursors[last.id] ?? last.id}#unread-page`;
      }

      return {
        productId,
        variantIds: Object.keys(variantCursors),
        cursor:
          index === 0
            ? (profileItemStartCursor ?? productId)
            : index === profileItemNodes.length - 1
              ? (profileItemEndCursor ?? productId)
              : productId,
        variantCursors,
      };
    })
    .filter((item): item is DeliveryProfileItemSeed => item !== null);
  if (readConnectionHasNextPage(profileItemsConnection) && profileItems.length > 0) {
    const last = profileItems[profileItems.length - 1]!;
    profileItems.push({
      ...structuredClone(last),
      productId: `${last.productId}#unread-page`,
      cursor: `${last.cursor ?? last.productId}#unread-page`,
    });
  }

  const profileLocationGroups = readArrayField(source, 'profileLocationGroups')
    .filter(isPlainObject)
    .map((group): DeliveryProfileLocationGroupRecord | null => {
      const locationGroup = readRecordField(group, 'locationGroup');
      const groupId = readStringField(locationGroup, 'id');
      if (!groupId) {
        return null;
      }

      const locationsConnection = readRecordField(locationGroup, 'locations');
      const locationEntries = readConnectionNodes(locationsConnection);
      const locationStartCursor = readConnectionStartCursor(locationsConnection);
      const locationEndCursor = readConnectionEndCursor(locationsConnection);
      const locations = locationEntries
        .map(readLocationRecord)
        .filter((location): location is LocationRecord => location !== null);
      runtime.store.upsertBaseLocations(locations);

      const locationCursors = Object.fromEntries(
        locations.map((location, index) => [
          location.id,
          index === 0
            ? (locationStartCursor ?? location.id)
            : index === locations.length - 1
              ? (locationEndCursor ?? location.id)
              : location.id,
        ]),
      );
      const zonesConnection = readRecordField(group, 'locationGroupZones');
      const zones = readConnectionEntries(zonesConnection)
        .map((entry) => readDeliveryProfileLocationGroupZone(entry.node, entry.cursor))
        .filter((zone): zone is DeliveryProfileLocationGroupZoneRecord => zone !== null);
      if (readConnectionHasNextPage(zonesConnection) && zones.length > 0) {
        const last = zones[zones.length - 1]!;
        zones.push({
          ...structuredClone(last),
          zone: {
            ...structuredClone(last.zone),
            id: `${last.zone.id}#unread-page`,
          },
          cursor: `${last.cursor ?? last.zone.id}#unread-page`,
        });
      }

      return {
        id: groupId,
        locationIds: locations.map((location) => location.id),
        locationCursors,
        countriesInAnyZone: readArrayField(group, 'countriesInAnyZone')
          .filter(isPlainObject)
          .map((countryAndZone) => {
            const zone = readStringField(countryAndZone, 'zone');
            const country = readDeliveryProfileCountry(readRecordField(countryAndZone, 'country'));
            return zone && country ? { zone, country } : null;
          })
          .filter(
            (countryAndZone): countryAndZone is DeliveryProfileLocationGroupRecord['countriesInAnyZone'][number] =>
              countryAndZone !== null,
          ),
        locationGroupZones: zones,
      };
    })
    .filter((group): group is DeliveryProfileLocationGroupRecord => group !== null);

  const unassignedConnection = readRecordField(source, 'unassignedLocationsPaginated');
  const unassignedLocations = readConnectionNodes(unassignedConnection)
    .map(readLocationRecord)
    .filter((location): location is LocationRecord => location !== null);
  runtime.store.upsertBaseLocations(unassignedLocations);
  const unassignedStartCursor = readConnectionStartCursor(unassignedConnection);
  const unassignedEndCursor = readConnectionEndCursor(unassignedConnection);

  return {
    id,
    ...(cursor ? { cursor } : {}),
    name,
    default: defaultProfile,
    merchantOwned: defaultProfile,
    version: readNumberField(source, 'version') ?? 1,
    activeMethodDefinitionsCount: readNumberField(source, 'activeMethodDefinitionsCount') ?? 0,
    locationsWithoutRatesCount: readNumberField(source, 'locationsWithoutRatesCount') ?? 0,
    originLocationCount: readNumberField(source, 'originLocationCount') ?? profileLocationGroups.length,
    zoneCountryCount: readNumberField(source, 'zoneCountryCount') ?? 0,
    productVariantsCount: readDeliveryProfileCount(readRecordField(source, 'productVariantsCount')),
    sellingPlanGroups: readConnectionNodes(readRecordField(source, 'sellingPlanGroups')).map(
      (node) => structuredClone(node) as DeliveryProfileRecord['sellingPlanGroups'][number],
    ),
    profileItems,
    profileLocationGroups,
    unassignedLocationIds: unassignedLocations.map((location) => location.id),
    unassignedLocationCursors: Object.fromEntries(
      unassignedLocations.map((location, index) => [
        location.id,
        index === 0
          ? (unassignedStartCursor ?? location.id)
          : index === unassignedLocations.length - 1
            ? (unassignedEndCursor ?? location.id)
            : location.id,
      ]),
    ),
  };
}

function seedDeliveryProfilePreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const catalog = readRecordField(readDeliveryProfilesCatalogPayload(capture), 'deliveryProfiles');
  const catalogProfiles = readConnectionEntries(catalog)
    .map((entry) => readDeliveryProfileRecord(runtime, entry.node, entry.cursor))
    .filter((profile): profile is DeliveryProfileRecord => profile !== null);

  const detailProfile = readDeliveryProfileRecord(
    runtime,
    readRecordField(readDeliveryProfileDetailPayload(capture), 'deliveryProfile') ?? {},
    catalogProfiles[0]?.cursor ?? null,
  );
  const profiles = detailProfile
    ? [detailProfile, ...catalogProfiles.filter((profile) => profile.id !== detailProfile.id)]
    : catalogProfiles;
  if (profiles.length === 0) {
    return false;
  }

  runtime.store.upsertBaseDeliveryProfiles(profiles);
  return true;
}

function seedDeliveryProfileWritePreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const mutations = readRecordField(capture as Record<string, unknown>, 'mutations');
  const nestedCreate = readRecordField(mutations, 'nestedCreate');
  const nestedCreateData = readRecordField(
    readRecordField(readRecordField(readRecordField(nestedCreate, 'result'), 'payload'), 'data'),
    'deliveryProfileCreate',
  );
  const createdProfile = readRecordField(nestedCreateData, 'profile');
  if (!createdProfile) {
    return false;
  }

  const profileItems = readConnectionNodes(readRecordField(createdProfile, 'profileItems'));
  for (const profileItem of profileItems) {
    const product = readRecordField(profileItem, 'product');
    const productId = readStringField(product, 'id');
    if (!productId) {
      continue;
    }

    runtime.store.upsertBaseProducts([makeSeedProduct(productId, product, 'Delivery profile write seed product')]);
    const variants = readConnectionNodes(readRecordField(profileItem, 'variants'))
      .map((variant, index) => makeDeliveryProfileSeedVariant(productId, variant, index))
      .filter((variant): variant is ProductVariantRecord => variant !== null);
    runtime.store.replaceBaseVariantsForProduct(productId, variants);
  }

  const locationIds = new Set<string>();
  for (const groupInput of readArrayField(
    readRecordField(readRecordField(nestedCreate, 'variables'), 'profile'),
    'locationGroupsToCreate',
  )) {
    if (!isPlainObject(groupInput)) {
      continue;
    }
    for (const locationId of readArrayField(groupInput, 'locations')) {
      if (typeof locationId === 'string' && locationId.startsWith('gid://shopify/Location/')) {
        locationIds.add(locationId);
      }
    }
  }

  const nestedUpdate = readRecordField(mutations, 'nestedUpdate');
  for (const groupInput of readArrayField(
    readRecordField(readRecordField(nestedUpdate, 'variables'), 'profile'),
    'locationGroupsToUpdate',
  )) {
    if (!isPlainObject(groupInput)) {
      continue;
    }
    for (const locationId of readArrayField(groupInput, 'locationsToAdd')) {
      if (typeof locationId === 'string' && locationId.startsWith('gid://shopify/Location/')) {
        locationIds.add(locationId);
      }
    }
  }

  runtime.store.upsertBaseLocations(
    [...locationIds].map(
      (id, index): LocationRecord => ({
        id,
        name: `Delivery profile write location ${index + 1}`,
        isActive: true,
        shipsInventory: true,
      }),
    ),
  );

  const defaultRemove = readRecordField(mutations, 'defaultRemove');
  const defaultProfileId = readStringField(readRecordField(defaultRemove, 'variables'), 'id');
  if (defaultProfileId) {
    runtime.store.upsertBaseDeliveryProfiles([
      {
        id: defaultProfileId,
        name: 'General profile',
        default: true,
        merchantOwned: true,
        version: 1,
        activeMethodDefinitionsCount: 0,
        locationsWithoutRatesCount: 0,
        originLocationCount: 0,
        zoneCountryCount: 0,
        productVariantsCount: { count: 0, precision: 'EXACT' },
        profileItems: [],
        profileLocationGroups: [],
        unassignedLocationIds: [],
        sellingPlanGroups: [],
      },
    ]);
  }

  return true;
}

function seedBusinessEntityPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
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

  runtime.store.upsertBaseBusinessEntities(businessEntities);
  return true;
}

function readB2BConnectionEntries(connection: Record<string, unknown> | null): Array<{
  node: Record<string, unknown>;
  cursor: string | null;
}> {
  return readConnectionEntries(connection).filter((entry) => readStringField(entry.node, 'id') !== null);
}

function seedB2BCompanyPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const data = readRecordField(capture as Record<string, unknown>, 'data');
  const companiesConnection = readRecordField(data, 'companies');
  const topLevelLocationsConnection = readRecordField(data, 'companyLocations');
  const companies = new Map<string, B2BCompanyRecord>();
  const contacts = new Map<string, B2BCompanyContactRecord>();
  const roles = new Map<string, B2BCompanyContactRoleRecord>();
  const locations = new Map<string, B2BCompanyLocationRecord>();

  const addCompany = (source: Record<string, unknown>, cursor: string | null): void => {
    const id = readStringField(source, 'id');
    if (!id?.startsWith('gid://shopify/Company/')) {
      return;
    }

    const contactIds: string[] = [];
    for (const entry of readB2BConnectionEntries(readRecordField(source, 'contacts'))) {
      const contactId = readStringField(entry.node, 'id');
      if (!contactId) {
        continue;
      }
      contactIds.push(contactId);
      contacts.set(contactId, {
        id: contactId,
        companyId: id,
        cursor: entry.cursor,
        data: structuredClone(entry.node) as B2BCompanyContactRecord['data'],
      });
    }

    const contactRoleIds: string[] = [];
    for (const entry of readB2BConnectionEntries(readRecordField(source, 'contactRoles'))) {
      const roleId = readStringField(entry.node, 'id');
      if (!roleId) {
        continue;
      }
      contactRoleIds.push(roleId);
      roles.set(roleId, {
        id: roleId,
        companyId: id,
        cursor: entry.cursor,
        data: structuredClone(entry.node) as B2BCompanyContactRoleRecord['data'],
      });
    }

    const locationIds: string[] = [];
    for (const entry of readB2BConnectionEntries(readRecordField(source, 'locations'))) {
      const locationId = readStringField(entry.node, 'id');
      if (!locationId) {
        continue;
      }
      locationIds.push(locationId);
      locations.set(locationId, {
        id: locationId,
        companyId: id,
        cursor: entry.cursor,
        data: structuredClone(entry.node) as B2BCompanyLocationRecord['data'],
      });
    }

    const existing = companies.get(id);
    companies.set(id, {
      id,
      cursor: cursor ?? existing?.cursor,
      data: {
        ...existing?.data,
        ...(structuredClone(source) as B2BCompanyRecord['data']),
      },
      contactIds: contactIds.length > 0 ? contactIds : (existing?.contactIds ?? []),
      locationIds: locationIds.length > 0 ? locationIds : (existing?.locationIds ?? []),
      contactRoleIds: contactRoleIds.length > 0 ? contactRoleIds : (existing?.contactRoleIds ?? []),
    });
  };

  for (const entry of readB2BConnectionEntries(companiesConnection)) {
    addCompany(entry.node, entry.cursor);
  }

  for (const entry of readB2BConnectionEntries(topLevelLocationsConnection)) {
    const locationId = readStringField(entry.node, 'id');
    const companyId = readStringField(readRecordField(entry.node, 'company'), 'id');
    if (!locationId || !companyId) {
      continue;
    }

    const existingCompany = companies.get(companyId);
    if (existingCompany && !existingCompany.locationIds.includes(locationId)) {
      existingCompany.locationIds.push(locationId);
    }

    const existingLocation = locations.get(locationId);
    locations.set(locationId, {
      id: locationId,
      companyId,
      cursor: entry.cursor ?? existingLocation?.cursor,
      data: {
        ...existingLocation?.data,
        ...(structuredClone(entry.node) as B2BCompanyLocationRecord['data']),
      },
    });
  }

  const singularCompany = readRecordField(data, 'company');
  if (singularCompany) {
    addCompany(singularCompany, null);
  }

  const singularContact = readRecordField(data, 'companyContact');
  const singularContactId = readStringField(singularContact, 'id');
  const singularContactCompanyId = readStringField(readRecordField(singularContact, 'company'), 'id');
  if (singularContactId && singularContactCompanyId) {
    const existingContact = contacts.get(singularContactId);
    contacts.set(singularContactId, {
      id: singularContactId,
      companyId: singularContactCompanyId,
      cursor: existingContact?.cursor,
      data: {
        ...existingContact?.data,
        ...(structuredClone(singularContact) as B2BCompanyContactRecord['data']),
      },
    });
  }

  const singularRole = readRecordField(data, 'companyContactRole');
  const singularRoleId = readStringField(singularRole, 'id');
  if (singularRoleId) {
    const existingRole = roles.get(singularRoleId);
    roles.set(singularRoleId, {
      id: singularRoleId,
      companyId: existingRole?.companyId ?? '',
      cursor: existingRole?.cursor,
      data: {
        ...existingRole?.data,
        ...(structuredClone(singularRole) as B2BCompanyContactRoleRecord['data']),
      },
    });
  }

  const singularLocation = readRecordField(data, 'companyLocation');
  const singularLocationId = readStringField(singularLocation, 'id');
  const singularLocationCompanyId = readStringField(readRecordField(singularLocation, 'company'), 'id');
  if (singularLocationId && singularLocationCompanyId) {
    const existingLocation = locations.get(singularLocationId);
    locations.set(singularLocationId, {
      id: singularLocationId,
      companyId: singularLocationCompanyId,
      cursor: existingLocation?.cursor,
      data: {
        ...existingLocation?.data,
        ...(structuredClone(singularLocation) as B2BCompanyLocationRecord['data']),
      },
    });
  }

  if (companies.size === 0 && contacts.size === 0 && roles.size === 0 && locations.size === 0) {
    return false;
  }

  runtime.store.upsertBaseB2BCompanies([...companies.values()]);
  runtime.store.upsertBaseB2BCompanyContacts([...contacts.values()]);
  runtime.store.upsertBaseB2BCompanyContactRoles([...roles.values()]);
  runtime.store.upsertBaseB2BCompanyLocations([...locations.values()]);
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

function readCapturedFulfillmentEvents(
  source: Record<string, unknown> | null,
): NonNullable<OrderFulfillmentRecord['events']> {
  return readArrayField(readRecordField(source, 'events'), 'nodes')
    .filter(isPlainObject)
    .map((event, index) => ({
      id: readStringField(event, 'id') ?? `gid://shopify/FulfillmentEvent/conformance-${index}`,
      status: readStringField(event, 'status'),
      message: readStringField(event, 'message'),
      happenedAt: readStringField(event, 'happenedAt') ?? readStringField(event, 'createdAt') ?? '2026-04-19T00:00:00Z',
      createdAt: readStringField(event, 'createdAt'),
      estimatedDeliveryAt: readStringField(event, 'estimatedDeliveryAt'),
      city: readStringField(event, 'city'),
      province: readStringField(event, 'province'),
      country: readStringField(event, 'country'),
      zip: readStringField(event, 'zip'),
      address1: readStringField(event, 'address1'),
      latitude: readNumberField(event, 'latitude'),
      longitude: readNumberField(event, 'longitude'),
    }));
}

function readCapturedFulfillmentLocation(
  source: Record<string, unknown> | null,
): NonNullable<OrderFulfillmentRecord['location']> | null {
  if (!source) {
    return null;
  }

  return {
    id: readStringField(source, 'id'),
    name: readStringField(source, 'name'),
  };
}

function readCapturedFulfillmentService(
  source: Record<string, unknown> | null,
): NonNullable<OrderFulfillmentRecord['service']> | null {
  if (!source) {
    return null;
  }

  return {
    id: readStringField(source, 'id'),
    handle: readStringField(source, 'handle'),
    serviceName: readStringField(source, 'serviceName'),
    trackingSupport: readBooleanField(source, 'trackingSupport'),
    type: readStringField(source, 'type'),
    location: readCapturedFulfillmentLocation(readRecordField(source, 'location')),
  };
}

function readCapturedFulfillmentOriginAddress(
  source: Record<string, unknown> | null,
): NonNullable<OrderFulfillmentRecord['originAddress']> | null {
  if (!source) {
    return null;
  }

  return {
    address1: readStringField(source, 'address1'),
    address2: readStringField(source, 'address2'),
    city: readStringField(source, 'city'),
    countryCode: readStringField(source, 'countryCode'),
    provinceCode: readStringField(source, 'provinceCode'),
    zip: readStringField(source, 'zip'),
  };
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
      deliveredAt: readStringField(fulfillment, 'deliveredAt'),
      estimatedDeliveryAt: readStringField(fulfillment, 'estimatedDeliveryAt'),
      inTransitAt: readStringField(fulfillment, 'inTransitAt'),
      trackingInfo: readArrayField(fulfillment, 'trackingInfo')
        .filter(isPlainObject)
        .map((trackingInfo) => ({
          number: readStringField(trackingInfo, 'number'),
          url: readStringField(trackingInfo, 'url'),
          company: readStringField(trackingInfo, 'company'),
        })),
      events: readCapturedFulfillmentEvents(fulfillment),
      fulfillmentLineItems: readCapturedFulfillmentLineItems(fulfillment),
      service: readCapturedFulfillmentService(readRecordField(fulfillment, 'service')),
      location: readCapturedFulfillmentLocation(readRecordField(fulfillment, 'location')),
      originAddress: readCapturedFulfillmentOriginAddress(readRecordField(fulfillment, 'originAddress')),
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

function seedFulfillmentLifecyclePreconditions(
  runtime: ProxyRuntimeContext,
  capture: unknown,
  mutationName: string | null,
): boolean {
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
  runtime.store.upsertBaseOrders([order]);
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
        lineItemQuantity: readNumberField(lineItem, 'quantity'),
        lineItemFulfillableQuantity: readNumberField(lineItem, 'fulfillableQuantity'),
        totalQuantity: readNumberField(fulfillmentOrderLineItem, 'totalQuantity') ?? 0,
        remainingQuantity: readNumberField(fulfillmentOrderLineItem, 'remainingQuantity') ?? 0,
      };
    });
}

function readCapturedOrderFulfillmentOrders(order: Record<string, unknown> | null): OrderFulfillmentOrderRecord[] {
  return readArrayField(readRecordField(order, 'fulfillmentOrders'), 'nodes')
    .filter(isPlainObject)
    .map((fulfillmentOrder, index) => {
      const assignedLocation = readRecordField(fulfillmentOrder, 'assignedLocation');
      const nestedLocation = readRecordField(assignedLocation, 'location');
      return {
        id: readStringField(fulfillmentOrder, 'id') ?? `gid://shopify/FulfillmentOrder/conformance-${index}`,
        status: readStringField(fulfillmentOrder, 'status'),
        requestStatus: readStringField(fulfillmentOrder, 'requestStatus'),
        fulfillAt: readStringField(fulfillmentOrder, 'fulfillAt'),
        fulfillBy: readStringField(fulfillmentOrder, 'fulfillBy'),
        updatedAt: readStringField(fulfillmentOrder, 'updatedAt'),
        supportedActions: readArrayField(fulfillmentOrder, 'supportedActions')
          .filter(isPlainObject)
          .map((action) => readStringField(action, 'action'))
          .filter((action): action is string => action !== null),
        fulfillmentHolds: readArrayField(fulfillmentOrder, 'fulfillmentHolds')
          .filter(isPlainObject)
          .map((hold, holdIndex) => ({
            id: readStringField(hold, 'id') ?? `gid://shopify/FulfillmentHold/conformance-${index}-${holdIndex}`,
            handle: readStringField(hold, 'handle'),
            reason: readStringField(hold, 'reason'),
            reasonNotes: readStringField(hold, 'reasonNotes'),
            displayReason: readStringField(hold, 'displayReason'),
            heldByRequestingApp: readBooleanField(hold, 'heldByRequestingApp') ?? undefined,
          })),
        assignedLocation: assignedLocation
          ? {
              name: readStringField(assignedLocation, 'name'),
              locationId: readStringField(nestedLocation, 'id'),
            }
          : null,
        lineItems: readCapturedFulfillmentOrderLineItems(fulfillmentOrder),
      };
    });
}

function seedFulfillmentOrderLifecyclePreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const workflows = readRecordField(capture as Record<string, unknown>, 'workflows');
  if (!workflows) {
    return false;
  }

  const seedOrders: OrderRecord[] = [];
  for (const workflow of Object.values(workflows)) {
    if (!isPlainObject(workflow)) {
      continue;
    }
    const orderSource = readRecordField(
      readRecordField(
        readRecordField(readRecordField(readRecordField(workflow, 'create'), 'response'), 'payload'),
        'data',
      ),
      'orderCreate',
    )?.['order'];
    if (!isPlainObject(orderSource)) {
      continue;
    }
    const orderId = readStringField(orderSource, 'id');
    if (!orderId) {
      continue;
    }
    seedOrders.push(makeSeedOrder(orderId, orderSource));
  }

  if (seedOrders.length === 0) {
    return false;
  }

  runtime.store.upsertBaseOrders(seedOrders);
  return true;
}

function readCapturedFulfillmentOrderMerchantRequests(
  fulfillmentOrder: Record<string, unknown> | null,
): NonNullable<OrderFulfillmentOrderRecord['merchantRequests']> {
  return readArrayField(readRecordField(fulfillmentOrder, 'merchantRequests'), 'nodes')
    .filter(isPlainObject)
    .map((merchantRequest, index) => ({
      id:
        readStringField(merchantRequest, 'id') ?? `gid://shopify/FulfillmentOrderMerchantRequest/conformance-${index}`,
      kind: readStringField(merchantRequest, 'kind') ?? 'FULFILLMENT_REQUEST',
      message: readNullableStringField(merchantRequest, 'message'),
      requestOptions: isPlainObject(merchantRequest['requestOptions'])
        ? (merchantRequest['requestOptions'] as Record<string, unknown>)
        : {},
      responseData: isPlainObject(merchantRequest['responseData'])
        ? (merchantRequest['responseData'] as Record<string, unknown>)
        : null,
      sentAt: readStringField(merchantRequest, 'sentAt') ?? '2026-04-26T01:06:46Z',
    }));
}

function buildSeedFulfillmentOrderLineItemsFromPartialSubmit(
  variables: Record<string, unknown> | null,
  originalFulfillmentOrder: Record<string, unknown> | null,
  unsubmittedFulfillmentOrder: Record<string, unknown> | null,
): OrderFulfillmentOrderLineItemRecord[] {
  const requestedQuantitiesById = new Map(
    readArrayField(variables, 'fulfillmentOrderLineItems')
      .filter(isPlainObject)
      .map((lineItem) => [readStringField(lineItem, 'id') ?? '', readNumberField(lineItem, 'quantity') ?? 0] as const)
      .filter(([lineItemId]) => lineItemId.length > 0),
  );
  const unsubmittedQuantitiesByLineItemId = new Map<string, number>();
  for (const node of readArrayField(readRecordField(unsubmittedFulfillmentOrder, 'lineItems'), 'nodes').filter(
    isPlainObject,
  )) {
    const lineItemId = readStringField(readRecordField(node, 'lineItem'), 'id');
    if (lineItemId) {
      unsubmittedQuantitiesByLineItemId.set(lineItemId, readNumberField(node, 'remainingQuantity') ?? 0);
    }
  }

  return readArrayField(readRecordField(originalFulfillmentOrder, 'lineItems'), 'nodes')
    .filter(isPlainObject)
    .map((node, index) => {
      const lineItem = readRecordField(node, 'lineItem');
      const fulfillmentOrderLineItemId =
        readStringField(node, 'id') ??
        [...requestedQuantitiesById.keys()][index] ??
        `gid://shopify/FulfillmentOrderLineItem/conformance-request-${index}`;
      const lineItemId = readStringField(lineItem, 'id');
      const submittedQuantity =
        requestedQuantitiesById.get(fulfillmentOrderLineItemId) ?? readNumberField(node, 'remainingQuantity') ?? 0;
      const unsubmittedQuantity = lineItemId ? (unsubmittedQuantitiesByLineItemId.get(lineItemId) ?? 0) : 0;
      const initialQuantity = submittedQuantity + unsubmittedQuantity;

      return {
        id: fulfillmentOrderLineItemId,
        lineItemId,
        title: readStringField(lineItem, 'title'),
        totalQuantity: initialQuantity,
        remainingQuantity: initialQuantity,
      };
    });
}

function makeMinimalFulfillmentOrder(
  id: string,
  requestStatus: string,
  status: string,
  merchantRequests: NonNullable<OrderFulfillmentOrderRecord['merchantRequests']> = [],
): OrderFulfillmentOrderRecord {
  return {
    id,
    status,
    requestStatus,
    assignedLocation: null,
    lineItems: [
      {
        id: `${id}/line-item-1`,
        lineItemId: `${id}/order-line-item-1`,
        title: 'HAR-233 conformance fulfillment item',
        totalQuantity: 1,
        remainingQuantity: 1,
      },
    ],
    merchantRequests,
  };
}

function seedFulfillmentOrderRequestLifecyclePreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const partialSubmit = readRecordField(capture as Record<string, unknown>, 'partialSubmit');
  if (!partialSubmit) {
    return false;
  }

  const partialSubmitPayload = readRecordField(
    readRecordField(readRecordField(partialSubmit, 'response'), 'data'),
    'fulfillmentOrderSubmitFulfillmentRequest',
  );
  const originalFulfillmentOrder = readRecordField(partialSubmitPayload, 'originalFulfillmentOrder');
  const unsubmittedFulfillmentOrder = readRecordField(partialSubmitPayload, 'unsubmittedFulfillmentOrder');
  const variables = readRecordField(partialSubmit, 'variables');
  const submittedFulfillmentOrderId =
    readStringField(variables, 'id') ?? readStringField(originalFulfillmentOrder, 'id');
  if (!submittedFulfillmentOrderId) {
    return false;
  }

  const requestOrder = makeSeedOrder('gid://shopify/Order/conformance-fulfillment-order-request', {
    id: 'gid://shopify/Order/conformance-fulfillment-order-request',
    name: '#HAR233-FO-REQUEST',
  });
  requestOrder.fulfillmentOrders = [
    {
      id: submittedFulfillmentOrderId,
      status: 'OPEN',
      requestStatus: 'UNSUBMITTED',
      assignedLocation: null,
      merchantRequests: [],
      lineItems: buildSeedFulfillmentOrderLineItemsFromPartialSubmit(
        variables,
        originalFulfillmentOrder,
        unsubmittedFulfillmentOrder,
      ),
    },
  ];

  const rejectFulfillmentOrder = readRecordField(
    readRecordField(
      readRecordField(readRecordField(capture as Record<string, unknown>, 'rejectFulfillmentRequest'), 'response'),
      'data',
    ),
    'fulfillmentOrderRejectFulfillmentRequest',
  );
  const rejectFulfillmentOrderId = readStringField(readRecordField(rejectFulfillmentOrder, 'fulfillmentOrder'), 'id');

  const rejectCancellationOrder = readRecordField(
    readRecordField(
      readRecordField(readRecordField(capture as Record<string, unknown>, 'rejectCancellationRequest'), 'response'),
      'data',
    ),
    'fulfillmentOrderRejectCancellationRequest',
  );
  const rejectCancellationFulfillmentOrder = readRecordField(rejectCancellationOrder, 'fulfillmentOrder');
  const rejectCancellationOrderId = readStringField(rejectCancellationFulfillmentOrder, 'id');

  const supportingFulfillmentOrders: OrderFulfillmentOrderRecord[] = [];
  if (rejectFulfillmentOrderId) {
    supportingFulfillmentOrders.push(
      makeMinimalFulfillmentOrder(rejectFulfillmentOrderId, 'SUBMITTED', 'OPEN', [
        {
          id: 'gid://shopify/FulfillmentOrderMerchantRequest/conformance-reject-fulfillment',
          kind: 'FULFILLMENT_REQUEST',
          message: 'HAR-233 rejection request',
          requestOptions: { notify_customer: false },
          responseData: null,
          sentAt: '2026-04-26T01:06:46Z',
        },
      ]),
    );
  }
  if (rejectCancellationOrderId) {
    supportingFulfillmentOrders.push(
      makeMinimalFulfillmentOrder(
        rejectCancellationOrderId,
        'ACCEPTED',
        'IN_PROGRESS',
        readCapturedFulfillmentOrderMerchantRequests(rejectCancellationFulfillmentOrder),
      ),
    );
  }

  if (supportingFulfillmentOrders.length > 0) {
    const supportOrder = makeSeedOrder('gid://shopify/Order/conformance-fulfillment-order-request-support', {
      id: 'gid://shopify/Order/conformance-fulfillment-order-request-support',
      name: '#HAR233-FO-SUPPORT',
    });
    supportOrder.fulfillmentOrders = supportingFulfillmentOrders;
    runtime.store.upsertBaseOrders([requestOrder, supportOrder]);
    return true;
  }

  runtime.store.upsertBaseOrders([requestOrder]);
  return true;
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

function readCapturedDraftOrderPaymentTerms(
  draftOrder: Record<string, unknown> | null,
): DraftOrderRecord['paymentTerms'] {
  const paymentTerms = readRecordField(draftOrder, 'paymentTerms');
  if (!paymentTerms) {
    return null;
  }

  return {
    id: readStringField(paymentTerms, 'id') ?? 'gid://shopify/PaymentTerms/conformance',
    due: readBooleanField(paymentTerms, 'due') ?? false,
    overdue: readBooleanField(paymentTerms, 'overdue') ?? false,
    dueInDays: readNumberField(paymentTerms, 'dueInDays'),
    paymentTermsName: readStringField(paymentTerms, 'paymentTermsName') ?? '',
    paymentTermsType: readStringField(paymentTerms, 'paymentTermsType') ?? '',
    translatedName: readStringField(paymentTerms, 'translatedName') ?? '',
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
    paymentTerms: readCapturedDraftOrderPaymentTerms(source),
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

function hydrateOrdersFromUpstreamResponse(runtime: ProxyRuntimeContext, upstreamPayload: unknown): void {
  const payload = isPlainObject(upstreamPayload) ? upstreamPayload : {};
  const data = readRecordField(payload, 'data') ?? payload;

  const order = readRecordField(data, 'order');
  const orderId = readStringField(order, 'id');
  if (orderId) {
    runtime.store.upsertBaseOrders([makeSeedOrder(orderId, order)]);
  }

  hydrateOrderConnectionsFromData(runtime, data);

  const draftOrder = readRecordField(data, 'draftOrder');
  const draftOrderId = readStringField(draftOrder, 'id');
  if (draftOrderId) {
    runtime.store.stageCreateDraftOrder(makeSeedDraftOrder(draftOrderId, draftOrder));
  }

  for (const edge of readArrayField(readRecordField(data, 'draftOrders'), 'edges').filter(isPlainObject)) {
    const node = readRecordField(edge, 'node');
    const nodeId = readStringField(node, 'id');
    if (nodeId) {
      runtime.store.stageCreateDraftOrder(makeSeedDraftOrder(nodeId, node));
    }
  }
}

function readDraftOrderInvoiceSendSeedSource(
  capture: Record<string, unknown>,
  pathSegments: string[],
  payloadName: string,
): Record<string, unknown> | null {
  let current: Record<string, unknown> | null = capture;
  for (const segment of pathSegments) {
    current = readRecordField(current, segment);
  }

  const payload = readRecordField(
    readRecordField(readRecordField(readRecordField(current, 'mutation'), 'response'), 'data'),
    payloadName,
  );
  return readRecordField(payload, 'draftOrder');
}

function seedDraftOrderInvoiceSendSafetyPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  if (!isPlainObject(capture) || capture['safetyPolicy'] === undefined) {
    return false;
  }

  const openDraftOrder = readDraftOrderInvoiceSendSeedSource(
    capture,
    ['recipient', 'openNoRecipient', 'setup', 'draftOrderCreate'],
    'draftOrderCreate',
  );
  const openDraftOrderId = readStringField(openDraftOrder, 'id');
  if (openDraftOrderId) {
    runtime.store.stageCreateDraftOrder(makeSeedDraftOrder(openDraftOrderId, openDraftOrder));
  }

  const completedDraftOrder = readDraftOrderInvoiceSendSeedSource(
    capture,
    ['lifecycle', 'completedNoRecipient', 'setup', 'draftOrderComplete'],
    'draftOrderComplete',
  );
  const completedDraftOrderId = readStringField(completedDraftOrder, 'id');
  if (completedDraftOrderId) {
    runtime.store.stageCreateDraftOrder(makeSeedDraftOrder(completedDraftOrderId, completedDraftOrder));
  }

  return Boolean(openDraftOrderId || completedDraftOrderId);
}

function hydrateOrderConnectionsFromData(runtime: ProxyRuntimeContext, data: Record<string, unknown> | null): void {
  for (const value of Object.values(data ?? {})) {
    const connection = isPlainObject(value) ? value : null;
    const edges = readArrayField(connection, 'edges').filter(isPlainObject);
    const nodes = readArrayField(connection, 'nodes').filter(isPlainObject);
    const edgeNodes = edges.map((edge) => readRecordField(edge, 'node')).filter(isPlainObject);

    for (const node of [...edgeNodes, ...nodes]) {
      const nodeId = readStringField(node, 'id');
      if (nodeId?.startsWith('gid://shopify/Order/')) {
        const existingOrder = runtime.store.getOrderById(nodeId);
        runtime.store.upsertBaseOrders([makeSeedOrder(nodeId, existingOrder ? { ...existingOrder, ...node } : node)]);
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
    ...readSeedContextualPricing(source),
  };
}

function readSeedContextualPricing(source: Record<string, unknown> | null): { contextualPricing?: JsonValue } {
  const result = jsonValueSchema.safeParse(source?.['contextualPricing']);
  return result.success ? { contextualPricing: structuredClone(result.data) } : {};
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
  const inventoryLevelConnection = readRecordField(inventoryItem, 'inventoryLevels');
  const inventoryLevelSources =
    readArrayField(inventoryLevelConnection, 'nodes').length > 0
      ? readArrayField(inventoryLevelConnection, 'nodes')
      : readArrayField(inventoryItem, 'inventoryLevels');
  const inventoryLevels = inventoryLevelSources
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
          inventoryLevels,
        }
      : null,
    ...readSeedContextualPricing(source),
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

function readCapturedProductOptions(productId: string, product: Record<string, unknown> | null): ProductOptionRecord[] {
  return readArrayField(product, 'options')
    .filter(isPlainObject)
    .map((option) => {
      const id = readStringField(option, 'id');
      const name = readStringField(option, 'name');
      if (!id || !name) {
        return null;
      }

      const optionValues = readArrayField(option, 'optionValues')
        .filter(isPlainObject)
        .map((optionValue) => {
          const valueId = readStringField(optionValue, 'id');
          const valueName = readStringField(optionValue, 'name');
          if (!valueId || !valueName) {
            return null;
          }

          return {
            id: valueId,
            name: valueName,
            hasVariants: readBooleanField(optionValue, 'hasVariants') ?? false,
          };
        })
        .filter((optionValue): optionValue is ProductOptionRecord['optionValues'][number] => optionValue !== null);

      return {
        id,
        productId,
        name,
        position: readNumberField(option, 'position') ?? 1,
        optionValues,
      };
    })
    .filter((option): option is ProductOptionRecord => option !== null);
}

function seedProductContextualPricingReadPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const data = readRecordField(capture as Record<string, unknown>, 'data');
  const product = readRecordField(data, 'product');
  const variant = readRecordField(data, 'productVariant');
  const productId = readStringField(product, 'id') ?? readStringField(readRecordField(variant, 'product'), 'id');
  if (!productId?.startsWith('gid://shopify/Product/')) {
    return false;
  }

  const seedProduct = readArrayField(capture as Record<string, unknown>, 'seedProducts').find(
    (candidate): candidate is Record<string, unknown> =>
      isPlainObject(candidate) && readStringField(candidate, 'id') === productId,
  );
  runtime.store.upsertBaseProducts([makeSeedProduct(productId, product ?? seedProduct ?? null)]);

  const variants: ProductVariantRecord[] = [];
  if (variant) {
    const capturedVariant = makeCapturedVariant(productId, variant);
    if (capturedVariant) {
      variants.push(capturedVariant);
    }
  }
  if (variants.length === 0) {
    variants.push(...readCapturedProductVariants(productId, seedProduct ?? null));
  }
  if (variants.length > 0) {
    runtime.store.replaceBaseVariantsForProduct(productId, variants);
  }

  return true;
}

function readPreMutationProduct(capture: unknown, productId: string): Record<string, unknown> | null {
  const preMutationRead = readRecordField(capture as Record<string, unknown>, 'preMutationRead');
  const data =
    readRecordField(preMutationRead, 'data') ?? readRecordField(readRecordField(preMutationRead, 'response'), 'data');
  const product = readRecordField(data, 'product');
  return readStringField(product, 'id') === productId ? product : null;
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

function readSeedPublication(source: Record<string, unknown>): PublicationRecord | null {
  const id = readStringField(source, 'id');
  if (!id?.startsWith('gid://shopify/Publication/')) {
    return null;
  }

  return {
    id,
    name: readStringField(source, 'name'),
    autoPublish: readBooleanField(source, 'autoPublish') ?? undefined,
    supportsFuturePublishing: readBooleanField(source, 'supportsFuturePublishing') ?? undefined,
    catalogId: readStringField(source, 'catalogId') ?? undefined,
    channelId: readStringField(source, 'channelId') ?? undefined,
    cursor: readStringField(source, 'cursor') ?? undefined,
  };
}

function readSeedChannel(source: Record<string, unknown>): ChannelRecord | null {
  const id = readStringField(source, 'id');
  if (!id?.startsWith('gid://shopify/Channel/')) {
    return null;
  }

  return {
    id,
    name: readStringField(source, 'name'),
    handle: readStringField(source, 'handle') ?? undefined,
    publicationId: readStringField(source, 'publicationId') ?? undefined,
    cursor: readStringField(source, 'cursor') ?? undefined,
  };
}

function seedProductOptionState(
  runtime: ProxyRuntimeContext,
  productId: string,
  variables: Record<string, unknown>,
  capture?: unknown,
): void {
  const preMutationProduct = capture === undefined ? null : readPreMutationProduct(capture, productId);
  if (preMutationProduct) {
    const capturedOptions = readCapturedProductOptions(productId, preMutationProduct);
    const capturedVariants = readCapturedProductVariants(productId, preMutationProduct);
    if (capturedOptions.length > 0) {
      runtime.store.replaceBaseOptionsForProduct(productId, capturedOptions);
    }
    if (capturedVariants.length > 0) {
      runtime.store.replaceBaseVariantsForProduct(productId, capturedVariants);
    }
    if (capturedOptions.length > 0 || capturedVariants.length > 0) {
      return;
    }
  }

  const optionInput = readRecordField(variables, 'option');
  const optionId =
    readStringField(optionInput, 'id') ??
    readArrayField(variables, 'options').find((option): option is string => typeof option === 'string') ??
    null;
  if (!optionId) {
    runtime.store.replaceBaseOptionsForProduct(productId, [makeDefaultOption(productId)]);
    runtime.store.replaceBaseVariantsForProduct(productId, [makeSeedVariant(productId)]);
    return;
  }

  const valueToUpdate = readArrayField(variables, 'optionValuesToUpdate').find(isPlainObject) ?? null;
  const optionValueId =
    readStringField(valueToUpdate, 'id') ?? `gid://shopify/ProductOptionValue/${productId.split('/').at(-1) ?? '1'}0`;
  runtime.store.replaceBaseOptionsForProduct(productId, [
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
  runtime.store.replaceBaseVariantsForProduct(productId, [
    makeSeedVariant(productId, [
      {
        name: readStringField(optionInput, 'name') ?? 'Color',
        value: 'Red',
      },
    ]),
  ]);
}

function readSeedSellingPlanGroup(source: Record<string, unknown>): SellingPlanGroupRecord | null {
  const id = readStringField(source, 'id');
  if (!id?.startsWith('gid://shopify/SellingPlanGroup/')) {
    return null;
  }

  const productIds = readConnectionNodes(readRecordField(source, 'products'))
    .map((product) => readStringField(product, 'id'))
    .filter((productId): productId is string => productId !== null);
  const productVariantIds = readConnectionNodes(readRecordField(source, 'productVariants'))
    .map((variant) => readStringField(variant, 'id'))
    .filter((variantId): variantId is string => variantId !== null);
  const sellingPlans = readConnectionNodes(readRecordField(source, 'sellingPlans'))
    .map((plan) => {
      const planId = readStringField(plan, 'id');
      return planId ? { id: planId, data: structuredClone(plan) } : null;
    })
    .filter((plan): plan is SellingPlanGroupRecord['sellingPlans'][number] => plan !== null);

  return {
    id,
    appId: readNullableStringField(source, 'appId'),
    name: readStringField(source, 'name') ?? 'Selling plan group',
    merchantCode: readStringField(source, 'merchantCode') ?? 'selling-plan-group',
    description: readNullableStringField(source, 'description'),
    options: readArrayField(source, 'options').filter((option): option is string => typeof option === 'string'),
    position: readNumberField(source, 'position'),
    summary: readNullableStringField(source, 'summary'),
    createdAt: readStringField(source, 'createdAt') ?? '2026-01-01T00:00:00Z',
    productIds,
    productVariantIds,
    sellingPlans,
  };
}

function seedBulkVariantValidationAtomicityPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const seed = readRecordField(capture as Record<string, unknown>, 'seed');
  const seedProductId = readStringField(seed, 'productId');
  const setupProduct = readRecordField(
    readRecordField(readRecordField(readRecordField(seed, 'setupOptionsResponse'), 'data'), 'productOptionsCreate'),
    'product',
  );
  const firstCase = readArrayField(capture as Record<string, unknown>, 'cases').find(isPlainObject) ?? null;
  const beforeProduct = readRecordField(readRecordField(firstCase, 'before'), 'product');
  const productSource = beforeProduct ?? setupProduct;
  if (!productSource) {
    return false;
  }

  const productId = seedProductId ?? readStringField(setupProduct, 'id') ?? readStringField(beforeProduct, 'id');

  if (!productId?.startsWith('gid://shopify/Product/')) {
    return false;
  }

  runtime.store.upsertBaseProducts([makeSeedProduct(productId, productSource)]);

  const optionsSource = readStringField(setupProduct, 'id') === productId ? setupProduct : beforeProduct;
  const options = readCapturedProductOptions(productId, optionsSource);
  if (options.length > 0) {
    runtime.store.replaceBaseOptionsForProduct(productId, options);
  }

  const variants = readCapturedProductVariants(productId, beforeProduct ?? setupProduct);
  if (variants.length > 0) {
    runtime.store.replaceBaseVariantsForProduct(productId, variants);
  }

  return true;
}

function seedCollectionProducts(
  runtime: ProxyRuntimeContext,
  collection: CollectionRecord,
  productNodes: unknown[],
): void {
  const collectionMemberships: ProductCollectionRecord[] = [];
  for (const [position, node] of productNodes.filter(isPlainObject).entries()) {
    const productId = readStringField(node, 'id');
    if (!productId) {
      continue;
    }
    runtime.store.upsertBaseProducts([makeSeedProduct(productId, node)]);
    collectionMemberships.push({
      id: collection.id,
      productId,
      title: collection.title,
      handle: collection.handle,
      position,
    });
  }
  for (const membership of collectionMemberships) {
    runtime.store.replaceBaseCollectionsForProduct(membership.productId, [membership]);
  }
}

function seedPreexistingProductCollectionsFromReadPayload(
  runtime: ProxyRuntimeContext,
  source: unknown,
  stagedCollectionId: string,
): void {
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

    const memberships = [...runtime.store.getEffectiveCollectionsByProductId(productId)];
    for (const node of readArrayField(readRecordField(value, 'collections'), 'nodes').filter(isPlainObject)) {
      const collectionId = readStringField(node, 'id');
      if (!collectionId?.startsWith('gid://shopify/Collection/') || collectionId === stagedCollectionId) {
        continue;
      }

      const collection = makeSeedCollection(collectionId, node);
      runtime.store.upsertBaseCollections([collection]);
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
      runtime.store.replaceBaseCollectionsForProduct(productId, memberships);
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
  runtime: ProxyRuntimeContext,
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

  runtime.store.upsertBaseProducts([
    makeSeedProduct(productId, productPayload, 'Product variant update conformance seed'),
  ]);
  runtime.store.replaceBaseVariantsForProduct(productId, [
    makeProductVariantUpdateCompatibilitySeedVariant(productId, variantId, capturedVariant),
  ]);
  return true;
}

function seedProductVariantDeleteCompatibilityPreconditions(
  runtime: ProxyRuntimeContext,
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

  runtime.store.upsertBaseProducts([
    makeSeedProduct(productId, productPayload, 'Product variant delete conformance seed'),
  ]);
  runtime.store.replaceBaseVariantsForProduct(productId, [
    makeProductVariantUpdateCompatibilitySeedVariant(productId, variantId, null),
    ...retainedVariants.filter((variant) => variant.id !== variantId),
  ]);
  return true;
}

function seedProductVariantsBulkReorderPreconditions(
  runtime: ProxyRuntimeContext,
  capture: unknown,
  productId: string,
): boolean {
  const setup = readRecordField(capture as Record<string, unknown>, 'setup');
  const setupCreatedProduct = readRecordField(
    readRecordField(readRecordField(setup, 'productCreate'), 'data'),
    'productCreate',
  );
  const setupProduct = readRecordField(setupCreatedProduct, 'product');
  const setupVariantCreate = readRecordField(
    readRecordField(readRecordField(setup, 'productVariantsBulkCreate'), 'data'),
    'productVariantsBulkCreate',
  );
  const setupVariantProduct = readRecordField(setupVariantCreate, 'product');
  const seedSource =
    readStringField(setupProduct, 'id') === productId
      ? setupProduct
      : readStringField(setupVariantProduct, 'id') === productId
        ? setupVariantProduct
        : null;

  if (!seedSource) {
    return false;
  }

  runtime.store.upsertBaseProducts([
    makeSeedProduct(productId, seedSource, 'Product variant reorder conformance seed'),
  ]);
  const variants = readCapturedProductVariants(productId, setupVariantProduct);
  if (variants.length > 0) {
    runtime.store.replaceBaseVariantsForProduct(productId, variants);
  }
  return true;
}

function seedProductReorderMediaPreconditions(
  runtime: ProxyRuntimeContext,
  capture: unknown,
  productId: string,
): boolean {
  if (mutationNameFromCapture(capture) !== 'productReorderMedia') {
    return false;
  }

  const setup = readRecordField(capture as Record<string, unknown>, 'setup');
  const setupCreatedProduct = readRecordField(
    readRecordField(readRecordField(setup, 'productCreate'), 'data'),
    'productCreate',
  );
  const setupProduct = readRecordField(setupCreatedProduct, 'product');
  const setupCreateMedia = readRecordField(
    readRecordField(readRecordField(setup, 'productCreateMedia'), 'response'),
    'data',
  );
  const createMediaPayload = readRecordField(setupCreateMedia, 'productCreateMedia');
  const mediaProduct = readRecordField(createMediaPayload, 'product');
  const productSource =
    readStringField(setupProduct, 'id') === productId
      ? setupProduct
      : readStringField(mediaProduct, 'id') === productId
        ? mediaProduct
        : null;

  if (!productSource) {
    return false;
  }

  runtime.store.upsertBaseProducts([
    makeSeedProduct(productId, productSource, 'Product media reorder conformance seed'),
  ]);
  const capturedMedia = readCapturedProductMedia(productId, mediaProduct);
  if (capturedMedia.length > 0) {
    runtime.store.replaceBaseMediaForProduct(productId, capturedMedia);
  }
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
  runtime: ProxyRuntimeContext,
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

  runtime.store.upsertBaseProducts([makeSeedProduct(productId, { ...productPayload, tags: baseTags })]);
  runtime.store.stageUpdateProduct(makeSeedProduct(productId, { ...productPayload, tags: preMutationTags }));
  return true;
}

function seedInventoryAdjustmentPreconditions(runtime: ProxyRuntimeContext, capture: unknown): void {
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

    runtime.store.upsertBaseProducts([
      makeSeedProduct(productId, productPayload, 'Inventory adjustment conformance seed'),
    ]);
    if (variants.length > 0) {
      runtime.store.replaceBaseVariantsForProduct(productId, variants);
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

function seedInventoryLinkagePreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
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

  runtime.store.upsertBaseProducts([
    makeSeedProduct(productId, {
      ...product,
      totalInventory: firstVariant?.inventoryQuantity ?? null,
      tracksInventory: firstVariant?.inventoryItem?.tracked ?? null,
    }),
  ]);
  if (variants.length > 0) {
    runtime.store.replaceBaseVariantsForProduct(productId, variants);
  }

  return true;
}

function makeInventoryQuantityRootSeedLevel(
  inventoryItemId: string,
  location: { id: string; name: string | null },
): InventoryLevelRecord {
  const locationTail = location.id.split('/').at(-1) ?? encodeURIComponent(location.id);

  return {
    id: `gid://shopify/InventoryLevel/${locationTail}?inventory_item_id=${encodeURIComponent(inventoryItemId)}`,
    cursor: `cursor:${inventoryItemId}:${location.id}`,
    location,
    quantities: [
      { name: 'available', quantity: 0, updatedAt: null },
      { name: 'on_hand', quantity: 0, updatedAt: null },
      { name: 'damaged', quantity: 0, updatedAt: null },
    ],
  };
}

function seedInventoryQuantityRootPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const mutationEvidence = readRecordField(capture as Record<string, unknown>, 'mutationEvidence');
  const setup = readRecordField(mutationEvidence, 'setup');
  const productId = readStringField(setup, 'productId');
  const variantId = readStringField(setup, 'variantId');
  const inventoryItemId = readStringField(setup, 'inventoryItemId');
  if (!productId || !variantId || !inventoryItemId) {
    return false;
  }

  const setEvidence = readRecordField(mutationEvidence, 'inventorySetQuantitiesAvailable');
  const setInput = readRecordField(readRecordField(setEvidence, 'variables'), 'input');
  const setQuantities = readArrayField(setInput, 'quantities').filter(isPlainObject);
  const rawSetChanges = readJsonPath(
    capture,
    '$.mutationEvidence.inventorySetQuantitiesAvailable.response.data.inventorySetQuantities.inventoryAdjustmentGroup.changes',
  );
  const setChanges = Array.isArray(rawSetChanges) ? rawSetChanges.filter(isPlainObject) : [];
  const downstreamRead = readRecordField(setEvidence, 'downstreamRead');
  const productTotalInventory = readNumberField(downstreamRead, 'productTotalInventory') ?? 0;
  const locationsById = new Map<string, { id: string; name: string | null }>();

  for (const change of setChanges) {
    const location = readRecordField(change, 'location');
    const locationId = readStringField(location, 'id');
    if (locationId) {
      locationsById.set(locationId, { id: locationId, name: readStringField(location, 'name') });
    }
  }

  for (const quantity of setQuantities) {
    const locationId = readStringField(quantity, 'locationId');
    if (locationId && !locationsById.has(locationId)) {
      locationsById.set(locationId, { id: locationId, name: null });
    }
  }

  runtime.store.upsertBaseLocations([...locationsById.values()]);
  runtime.store.upsertBaseProducts([
    makeSeedProduct(
      productId,
      {
        id: productId,
        title: 'Inventory quantity roots conformance seed',
        totalInventory: productTotalInventory,
        tracksInventory: true,
      },
      'Inventory quantity roots conformance seed',
    ),
  ]);
  runtime.store.replaceBaseVariantsForProduct(productId, [
    {
      id: variantId,
      productId,
      title: 'Default Title',
      sku: null,
      barcode: null,
      price: null,
      compareAtPrice: null,
      taxable: null,
      inventoryPolicy: null,
      inventoryQuantity: 0,
      selectedOptions: [],
      inventoryItem: {
        id: inventoryItemId,
        tracked: true,
        requiresShipping: true,
        measurement: null,
        countryCodeOfOrigin: null,
        provinceCodeOfOrigin: null,
        harmonizedSystemCode: null,
        inventoryLevels: [...locationsById.values()].map((location) =>
          makeInventoryQuantityRootSeedLevel(inventoryItemId, location),
        ),
      },
    },
  ]);

  return true;
}

function seedInventoryItemUpdatePreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
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

  runtime.store.upsertBaseProducts([makeSeedProduct(productId, product)]);
  const variants = readCapturedProductVariants(productId, product);
  if (variants.length > 0) {
    runtime.store.replaceBaseVariantsForProduct(productId, variants);
  }

  return true;
}

function seedMetafieldsSetOwnerProducts(
  runtime: ProxyRuntimeContext,
  capture: unknown,
  variables: Record<string, unknown>,
): void {
  const preconditionProduct = readRecordField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'preconditionRead'), 'data'),
    'product',
  );
  const downstreamProduct = readRecordField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'downstreamRead'), 'data'),
    'product',
  );
  const seedProduct = readRecordField(capture as Record<string, unknown>, 'seedProduct');
  const seedCollection = readRecordField(capture as Record<string, unknown>, 'seedCollection');
  const downstreamProductVariant = readRecordField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'downstreamRead'), 'data'),
    'productVariant',
  );
  const downstreamCollection = readRecordField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'downstreamRead'), 'data'),
    'collection',
  );
  const productSource = preconditionProduct ?? downstreamProduct;
  for (const input of readArrayField(variables, 'metafields').filter(isPlainObject)) {
    const ownerId = readStringField(input, 'ownerId');
    if (!ownerId) {
      continue;
    }

    if (ownerId.startsWith('gid://shopify/Product/')) {
      if (runtime.store.getEffectiveProductById(ownerId)) {
        continue;
      }

      const source = readStringField(productSource, 'id') === ownerId ? productSource : null;
      runtime.store.upsertBaseProducts([makeSeedProduct(ownerId, source)]);
      if (source) {
        runtime.store.replaceBaseMetafieldsForProduct(ownerId, readCapturedProductMetafields(ownerId, source));
      }
      continue;
    }

    if (ownerId.startsWith('gid://shopify/ProductVariant/')) {
      const productId = readStringField(seedProduct, 'id') ?? readStringField(downstreamProduct, 'id');
      if (!productId?.startsWith('gid://shopify/Product/')) {
        continue;
      }

      runtime.store.upsertBaseProducts([makeSeedProduct(productId, seedProduct ?? downstreamProduct)]);
      const variantSource =
        readStringField(downstreamProductVariant, 'id') === ownerId
          ? downstreamProductVariant
          : (readArrayField(readRecordField(seedProduct ?? downstreamProduct, 'variants'), 'nodes')
              .filter(isPlainObject)
              .find((variant) => readStringField(variant, 'id') === ownerId) ?? null);
      const variant = variantSource ? makeCapturedVariant(productId, variantSource) : null;
      if (variant) {
        runtime.store.replaceBaseVariantsForProduct(productId, [variant]);
      }
      continue;
    }

    if (ownerId.startsWith('gid://shopify/Collection/')) {
      const source = readStringField(downstreamCollection, 'id') === ownerId ? downstreamCollection : seedCollection;
      if (source) {
        runtime.store.upsertBaseCollections([makeSeedCollection(ownerId, source)]);
      }
    }
  }
}

function seedProductMetafieldsReadPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const responseProduct = readRecordField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'response'), 'data'),
    'product',
  );
  const legacyConnectionProduct = readRecordField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'connection'), 'data'),
    'product',
  );
  const product = responseProduct ?? legacyConnectionProduct;
  const productId = readStringField(product, 'id');
  if (!product || !productId?.startsWith('gid://shopify/Product/')) {
    return false;
  }

  runtime.store.upsertBaseProducts([makeSeedProduct(productId, product)]);
  runtime.store.replaceBaseMetafieldsForProduct(productId, readCapturedProductMetafields(productId, product));
  return true;
}

function seedCustomDataFieldTypeMatrixPreconditions(runtime: ProxyRuntimeContext, capture: unknown): void {
  const hasCustomDataMatrix =
    readArrayField(capture as Record<string, unknown>, 'metafieldBatches').length > 0 ||
    readArrayField(capture as Record<string, unknown>, 'metaobjectMatrices').length > 0;
  if (!hasCustomDataMatrix) {
    return;
  }

  const seed = readRecordField(capture as Record<string, unknown>, 'seed');
  const productId = readStringField(seed, 'productId');
  if (productId?.startsWith('gid://shopify/Product/')) {
    runtime.store.upsertBaseProducts([makeSeedProduct(productId, null, 'Custom data field type matrix seed product')]);

    const variantId = readStringField(seed, 'variantId');
    if (variantId?.startsWith('gid://shopify/ProductVariant/')) {
      runtime.store.replaceBaseVariantsForProduct(productId, [{ ...makeSeedVariant(productId), id: variantId }]);
    }
  }

  const collectionId = readStringField(seed, 'collectionId');
  if (collectionId?.startsWith('gid://shopify/Collection/')) {
    runtime.store.upsertBaseCollections([makeSeedCollection(collectionId)]);
  }
}

function seedMetaobjectReadPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  if (!isPlainObject(capture)) {
    return false;
  }

  let hydrated = false;
  for (const read of readArrayField(capture, 'seededReads').filter(isPlainObject)) {
    const request = readRecordField(read, 'request');
    const query = readStringField(request, 'query');
    const response = readRecordField(read, 'response');
    if (!query || !response) {
      continue;
    }

    hydrateMetaobjectsFromUpstreamResponse(runtime, query, readRecordField(request, 'variables') ?? {}, response);
    hydrated = true;
  }

  return hydrated && (runtime.store.hasEffectiveMetaobjectDefinitions() || runtime.store.hasEffectiveMetaobjects());
}

function seedMetafieldsDeleteOwnerProducts(
  runtime: ProxyRuntimeContext,
  capture: unknown,
  variables: Record<string, unknown>,
): boolean {
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
  const capturedDeletedMetafields = readArrayField(mutationPayloadFromCapture(capture), 'deletedMetafields');
  const hasCapturedDeletedMetafields = capturedDeletedMetafields.length > 0;
  const deletedMetafields = deletedIdentifiers
    .map((identifier, index): ProductMetafieldRecord | null => {
      const capturedDeletedMetafield = hasCapturedDeletedMetafields
        ? isPlainObject(capturedDeletedMetafields[index])
          ? capturedDeletedMetafields[index]
          : null
        : identifier;
      if (!capturedDeletedMetafield) {
        return null;
      }

      const namespace = readStringField(capturedDeletedMetafield, 'namespace');
      const key = readStringField(capturedDeletedMetafield, 'key');
      const productId = readStringField(capturedDeletedMetafield, 'ownerId') ?? ownerId;
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
        compareDigest: null,
        jsonValue: null,
        createdAt: null,
        updatedAt: null,
        ownerType: 'PRODUCT',
      };
    })
    .filter((metafield): metafield is ProductMetafieldRecord => metafield !== null);

  runtime.store.upsertBaseProducts([makeSeedProduct(ownerId, downstreamProduct)]);
  runtime.store.replaceBaseMetafieldsForProduct(ownerId, [...retainedMetafields, ...deletedMetafields]);
  return true;
}

function seedProductDuplicateSource(runtime: ProxyRuntimeContext, capture: unknown): boolean {
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

  hydrateProductsFromUpstreamResponse(runtime, 'query ProductDuplicateSourceSeed { product { id } }', {}, sourceRead);
  return true;
}

function seedFileDeleteMediaReferencePreconditions(
  runtime: ProxyRuntimeContext,
  capture: unknown,
  variables: Record<string, unknown>,
): boolean {
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

  runtime.store.upsertBaseProducts([makeSeedProduct(productId, product)]);
  runtime.store.replaceBaseMediaForProduct(productId, capturedMedia);
  return true;
}

function readCapturedOwnerMetafields(
  ownerId: string,
  ownerType: string,
  source: Record<string, unknown>,
): ProductMetafieldRecord[] {
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
      ...(ownerType === 'PRODUCT' ? { productId: ownerId } : {}),
      ownerId,
      namespace,
      key,
      type: readStringField(candidate, 'type'),
      value: readStringField(candidate, 'value'),
      compareDigest: readStringField(candidate, 'compareDigest'),
      jsonValue: Object.prototype.hasOwnProperty.call(candidate, 'jsonValue')
        ? (candidate['jsonValue'] as ProductMetafieldRecord['jsonValue'])
        : undefined,
      createdAt: readStringField(candidate, 'createdAt'),
      updatedAt: readStringField(candidate, 'updatedAt'),
      ownerType: readStringField(candidate, 'ownerType') ?? ownerType,
    });
  };

  for (const value of Object.values(source)) {
    addMetafield(value);
  }

  for (const value of Object.values(source)) {
    const connection = isPlainObject(value) ? value : null;
    for (const node of readArrayField(connection, 'nodes')) {
      addMetafield(node);
    }
    for (const edge of readArrayField(connection, 'edges').filter(isPlainObject)) {
      addMetafield(readRecordField(edge, 'node'));
    }
  }

  return Array.from(byIdentity.values()).sort(
    (left, right) =>
      left.namespace.localeCompare(right.namespace) ||
      left.key.localeCompare(right.key) ||
      left.id.localeCompare(right.id),
  );
}

function readMetafieldDefinitionCapability(source: Record<string, unknown> | null): {
  enabled: boolean;
  eligible: boolean;
  status?: string | null;
} {
  const status = readNullableStringField(source, 'status');
  return {
    enabled: readBooleanField(source, 'enabled') ?? false,
    eligible: readBooleanField(source, 'eligible') ?? false,
    ...(status !== null ? { status } : {}),
  };
}

function readCapturedMetafieldDefinition(source: Record<string, unknown> | null): MetafieldDefinitionRecord | null {
  const id = readStringField(source, 'id');
  const name = readStringField(source, 'name');
  const namespace = readStringField(source, 'namespace');
  const key = readStringField(source, 'key');
  const ownerType = readStringField(source, 'ownerType');
  const type = readRecordField(source, 'type');
  const typeName = readStringField(type, 'name');
  if (!id || !name || !namespace || !key || !ownerType || !typeName) {
    return null;
  }

  const capabilities = readRecordField(source, 'capabilities');
  const constraints = readRecordField(source, 'constraints');
  const constraintValuesConnection = readRecordField(constraints, 'values');

  return {
    id,
    name,
    namespace,
    key,
    ownerType,
    type: {
      name: typeName,
      category: readNullableStringField(type, 'category'),
    },
    description: readNullableStringField(source, 'description'),
    validations: readArrayField(source, 'validations')
      .filter(isPlainObject)
      .map((validation) => ({
        name: readStringField(validation, 'name') ?? '',
        value: readNullableStringField(validation, 'value'),
      }))
      .filter((validation) => validation.name.length > 0),
    access: (readRecordField(source, 'access') ?? {}) as MetafieldDefinitionRecord['access'],
    capabilities: {
      adminFilterable: readMetafieldDefinitionCapability(readRecordField(capabilities, 'adminFilterable')),
      smartCollectionCondition: readMetafieldDefinitionCapability(
        readRecordField(capabilities, 'smartCollectionCondition'),
      ),
      uniqueValues: readMetafieldDefinitionCapability(readRecordField(capabilities, 'uniqueValues')),
    },
    constraints: constraints
      ? {
          key: readNullableStringField(constraints, 'key'),
          values: readArrayField(constraintValuesConnection, 'nodes')
            .filter(isPlainObject)
            .map((value) => ({ value: readStringField(value, 'value') ?? '' }))
            .filter((value) => value.value.length > 0),
        }
      : null,
    pinnedPosition: readNullableNumberField(source, 'pinnedPosition'),
    validationStatus: readStringField(source, 'validationStatus') ?? 'ALL_VALID',
  };
}

function readCapturedMetafieldDefinitionProductMetafields(
  definition: Record<string, unknown>,
): ProductMetafieldRecord[] {
  const connection = readRecordField(definition, 'metafields');
  return readArrayField(connection, 'nodes')
    .filter(isPlainObject)
    .flatMap((metafield): ProductMetafieldRecord[] => {
      const owner = readRecordField(metafield, 'owner');
      const productId = readStringField(owner, 'id');
      const id = readStringField(metafield, 'id');
      const namespace = readStringField(metafield, 'namespace');
      const key = readStringField(metafield, 'key');
      if (!productId?.startsWith('gid://shopify/Product/') || !id || !namespace || !key) {
        return [];
      }

      return [
        {
          id,
          productId,
          namespace,
          key,
          type: readStringField(metafield, 'type'),
          value: readStringField(metafield, 'value'),
          ownerType: readStringField(metafield, 'ownerType') ?? 'PRODUCT',
        },
      ];
    });
}

function seedMetafieldDefinitionPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const responseData = readRecordField(readRecordField(capture as Record<string, unknown>, 'response'), 'data');
  const definitionNodes = ['metafieldDefinitions', 'seedCatalog']
    .flatMap((fieldName) => readArrayField(readRecordField(responseData, fieldName), 'nodes'))
    .filter(isPlainObject);
  const singularDefinition = readRecordField(responseData, 'byIdentifier');
  const definitions = [
    ...definitionNodes.map(readCapturedMetafieldDefinition),
    readCapturedMetafieldDefinition(singularDefinition),
  ].filter((definition): definition is MetafieldDefinitionRecord => definition !== null);

  if (definitions.length === 0) {
    return false;
  }

  runtime.store.upsertBaseMetafieldDefinitions(definitions);

  const metafieldsByProductId = new Map<string, ProductMetafieldRecord[]>();
  for (const metafield of [
    ...definitionNodes.flatMap(readCapturedMetafieldDefinitionProductMetafields),
    ...(singularDefinition ? readCapturedMetafieldDefinitionProductMetafields(singularDefinition) : []),
  ]) {
    if (!metafield.productId) {
      continue;
    }

    const metafields = metafieldsByProductId.get(metafield.productId) ?? [];
    if (!metafields.some((candidate) => candidate.id === metafield.id)) {
      metafields.push(metafield);
    }
    metafieldsByProductId.set(metafield.productId, metafields);
  }

  for (const [productId, metafields] of metafieldsByProductId) {
    runtime.store.upsertBaseProducts([makeSeedProduct(productId)]);
    runtime.store.replaceBaseMetafieldsForProduct(productId, metafields);
  }

  return true;
}

function readCapturedProductMetafields(productId: string, product: Record<string, unknown>): ProductMetafieldRecord[] {
  return readCapturedOwnerMetafields(productId, 'PRODUCT', product);
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

function seedExplicitProductMediaPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const mediaByProductId = new Map<string, ProductMediaRecord[]>();

  for (const [index, seedMedia] of readArrayField(capture as Record<string, unknown>, 'seedProductMedia')
    .filter(isPlainObject)
    .entries()) {
    const productId = readStringField(seedMedia, 'productId');
    const id = readStringField(seedMedia, 'id');
    if (!productId?.startsWith('gid://shopify/Product/') || !id?.startsWith('gid://shopify/')) {
      continue;
    }

    const position = readNumberField(seedMedia, 'position') ?? index;
    const mediaRecords = mediaByProductId.get(productId) ?? [];
    mediaRecords.push({
      key: readStringField(seedMedia, 'key') ?? `${productId}:media:${position}:${id}`,
      productId,
      position,
      id,
      mediaContentType: readStringField(seedMedia, 'mediaContentType') ?? 'IMAGE',
      alt: readNullableStringField(seedMedia, 'alt'),
      status: readStringField(seedMedia, 'status') ?? 'READY',
      productImageId: readNullableStringField(seedMedia, 'productImageId'),
      imageUrl: readNullableStringField(seedMedia, 'imageUrl'),
      previewImageUrl: readNullableStringField(seedMedia, 'previewImageUrl'),
      sourceUrl: readNullableStringField(seedMedia, 'sourceUrl'),
    });
    mediaByProductId.set(productId, mediaRecords);
  }

  for (const [productId, mediaRecords] of mediaByProductId) {
    if (!runtime.store.getEffectiveProductById(productId)) {
      runtime.store.upsertBaseProducts([makeSeedProduct(productId)]);
    }
    runtime.store.replaceBaseMediaForProduct(productId, mediaRecords);
  }

  return mediaByProductId.size > 0;
}

function seedLocalizationPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const readCaptureData = readRecordField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'readCapture'), 'response'),
    'data',
  );
  if (!readCaptureData) {
    return false;
  }

  const locales = readArrayField(readCaptureData, 'availableLocalesExcerpt')
    .filter(isPlainObject)
    .flatMap((locale): LocaleRecord[] => {
      const isoCode = readStringField(locale, 'isoCode');
      const name = readStringField(locale, 'name');
      return isoCode && name ? [{ isoCode, name }] : [];
    });
  if (locales.length > 0) {
    runtime.store.replaceBaseAvailableLocales(locales);
  }

  const shopLocales = readArrayField(readCaptureData, 'allShopLocales')
    .filter(isPlainObject)
    .flatMap((locale): ShopLocaleRecord[] => {
      const localeCode = readStringField(locale, 'locale');
      const name = readStringField(locale, 'name');
      const primary = readBooleanField(locale, 'primary');
      const published = readBooleanField(locale, 'published');
      if (!localeCode || !name || primary === null || published === null) {
        return [];
      }

      return [
        {
          locale: localeCode,
          name,
          primary,
          published,
          marketWebPresenceIds: readArrayField(locale, 'marketWebPresences')
            .filter(isPlainObject)
            .flatMap((presence) => {
              const id = readStringField(presence, 'id');
              return id ? [id] : [];
            }),
        },
      ];
    });
  if (shopLocales.length > 0) {
    runtime.store.upsertBaseShopLocales(shopLocales);
  }

  const resources = readArrayField(readRecordField(readCaptureData, 'resources'), 'nodes').filter(isPlainObject);
  for (const resource of resources) {
    const productId = readStringField(resource, 'resourceId');
    if (!productId?.startsWith('gid://shopify/Product/')) {
      continue;
    }

    const contentByKey = new Map(
      readArrayField(resource, 'translatableContent')
        .filter(isPlainObject)
        .map((content) => [readStringField(content, 'key'), readStringField(content, 'value')] as const)
        .filter((entry): entry is [string, string] => entry[0] !== null && entry[1] !== null),
    );
    runtime.store.upsertBaseProducts([
      makeSeedProduct(productId, {
        id: productId,
        title: contentByKey.get('title'),
        handle: contentByKey.get('handle'),
        productType: contentByKey.get('product_type'),
      }),
    ]);
  }

  return locales.length > 0 || shopLocales.length > 0 || resources.length > 0;
}

function seedOnlineStoreContentPreconditions(runtime: ProxyRuntimeContext, capture: unknown): void {
  const interactions = readArrayField(capture as Record<string, unknown>, 'interactions').filter(isPlainObject);
  for (const interaction of interactions) {
    if (interaction['name'] !== 'baseline-catalog-detail-empty') {
      continue;
    }

    const request = readRecordField(interaction, 'request');
    const response = readRecordField(interaction, 'response');
    const query = readStringField(request, 'query');
    if (query && response) {
      hydrateOnlineStoreFromUpstreamResponse(runtime, query, response);
    }
    return;
  }
}

const bulkOperationStatuses = new Set([
  'CANCELED',
  'CANCELING',
  'COMPLETED',
  'CREATED',
  'EXPIRED',
  'FAILED',
  'RUNNING',
]);
const bulkOperationTypes = new Set(['MUTATION', 'QUERY']);

function readBulkOperationRecord(value: Record<string, unknown> | null | undefined): BulkOperationRecord | null {
  const id = readStringField(value, 'id');
  const status = readStringField(value, 'status');
  const type = readStringField(value, 'type');
  const createdAt = readStringField(value, 'createdAt');
  if (
    !id?.startsWith('gid://shopify/BulkOperation/') ||
    !status ||
    !bulkOperationStatuses.has(status) ||
    !type ||
    !bulkOperationTypes.has(type) ||
    !createdAt
  ) {
    return null;
  }

  return {
    id,
    status: status as BulkOperationRecord['status'],
    type: type as BulkOperationRecord['type'],
    errorCode: readNullableStringField(value, 'errorCode') as BulkOperationRecord['errorCode'],
    createdAt,
    completedAt: readNullableStringField(value, 'completedAt'),
    objectCount: readStringField(value, 'objectCount') ?? '0',
    rootObjectCount: readStringField(value, 'rootObjectCount') ?? '0',
    fileSize: readNullableStringField(value, 'fileSize'),
    url: readNullableStringField(value, 'url'),
    partialDataUrl: readNullableStringField(value, 'partialDataUrl'),
    query: readNullableStringField(value, 'query'),
  };
}

function readBulkOperationFromInteraction(
  interaction: Record<string, unknown> | null | undefined,
  responseField: string,
): BulkOperationRecord | null {
  return readBulkOperationRecord(
    readRecordField(readRecordField(readRecordField(interaction, 'response'), 'data'), responseField),
  );
}

function readBulkOperationPayloadFromInteraction(
  interaction: Record<string, unknown> | null | undefined,
  payloadField: string,
): BulkOperationRecord | null {
  return readBulkOperationRecord(
    readRecordField(
      readRecordField(readRecordField(readRecordField(interaction, 'response'), 'data'), payloadField),
      'bulkOperation',
    ),
  );
}

function seedProductsFromBulkOperationResult(
  runtime: ProxyRuntimeContext,
  result: Record<string, unknown> | null | undefined,
): number {
  const products = readArrayField(result, 'records')
    .filter(isPlainObject)
    .flatMap((record, index) => {
      const productId = readStringField(record, 'id');
      if (!productId?.startsWith('gid://shopify/Product/')) {
        return [];
      }

      const orderedTimestamp = new Date(Date.UTC(2026, 3, 27, 0, 0, 0, 0) - index).toISOString();
      return [
        makeSeedProduct(productId, {
          ...record,
          createdAt: orderedTimestamp,
          updatedAt: orderedTimestamp,
        }),
      ];
    });

  if (products.length > 0) {
    runtime.store.upsertBaseProducts(products);
  }

  return products.length;
}

function seedBulkOperationPreconditions(runtime: ProxyRuntimeContext, capture: unknown): boolean {
  const captureRecord = isPlainObject(capture) ? capture : {};
  const reads = readRecordField(captureRecord, 'reads');
  const lifecycle = readRecordField(captureRecord, 'lifecycle');
  if (!reads && !lifecycle) {
    return false;
  }

  const baseOperations = new Map<string, BulkOperationRecord>();
  const addBaseOperation = (operation: BulkOperationRecord | null): void => {
    if (operation) {
      baseOperations.set(operation.id, operation);
    }
  };

  for (const key of ['catalogDefault', 'catalogEmptyRunningQuery', 'catalogEmptyRunningMutation'] as const) {
    const nodes = readArrayField(
      readRecordField(
        readRecordField(readRecordField(readRecordField(reads, key), 'response'), 'data'),
        'bulkOperations',
      ),
      'nodes',
    );
    for (const node of nodes.filter(isPlainObject)) {
      addBaseOperation(readBulkOperationRecord(node));
    }
  }

  addBaseOperation(readBulkOperationFromInteraction(readRecordField(reads, 'currentQuery'), 'currentBulkOperation'));
  addBaseOperation(readBulkOperationFromInteraction(readRecordField(reads, 'currentMutation'), 'currentBulkOperation'));

  const terminalLifecycle = readRecordField(lifecycle, 'queryExportToTerminal');
  addBaseOperation(
    readBulkOperationPayloadFromInteraction(readRecordField(terminalLifecycle, 'run'), 'bulkOperationRunQuery'),
  );
  for (const poll of readArrayField(terminalLifecycle, 'statusPolls').filter(isPlainObject)) {
    addBaseOperation(readBulkOperationFromInteraction(poll, 'bulkOperation'));
  }
  const terminalCatalogNode = readArrayField(
    readRecordField(
      readRecordField(readRecordField(readRecordField(terminalLifecycle, 'catalogById'), 'response'), 'data'),
      'bulkOperations',
    ),
    'nodes',
  )
    .filter(isPlainObject)
    .map(readBulkOperationRecord)
    .find((operation): operation is BulkOperationRecord => operation !== null);
  addBaseOperation(terminalCatalogNode ?? null);
  addBaseOperation(
    readBulkOperationFromInteraction(
      readRecordField(terminalLifecycle, 'currentQueryOperation'),
      'currentBulkOperation',
    ),
  );

  const stagedImmediateCancelOperation = readBulkOperationPayloadFromInteraction(
    readRecordField(readRecordField(lifecycle, 'queryExportImmediateCancel'), 'run'),
    'bulkOperationRunQuery',
  );
  const seededBulkResultProducts = seedProductsFromBulkOperationResult(
    runtime,
    readRecordField(terminalLifecycle, 'result'),
  );

  if (baseOperations.size > 0) {
    runtime.store.upsertBaseBulkOperations([...baseOperations.values()]);
  }
  if (stagedImmediateCancelOperation) {
    runtime.store.stageBulkOperation(stagedImmediateCancelOperation);
  }

  return baseOperations.size > 0 || stagedImmediateCancelOperation !== null || seededBulkResultProducts > 0;
}

function seedAdminPlatformNodePreconditions(runtime: ProxyRuntimeContext, capture: unknown): void {
  const nodeSeeds = readRecordField(capture as Record<string, unknown>, 'nodeSeeds');
  if (!nodeSeeds) {
    return;
  }

  const products = readArrayField(nodeSeeds, 'products')
    .filter(isPlainObject)
    .map((product) => {
      const id = readStringField(product, 'id');
      return id?.startsWith('gid://shopify/Product/') ? makeSeedProduct(id, product) : null;
    })
    .filter((product): product is ProductRecord => product !== null);
  if (products.length > 0) {
    runtime.store.upsertBaseProducts(products);
  }

  const collections = readArrayField(nodeSeeds, 'collections')
    .filter(isPlainObject)
    .map((collection) => {
      const id = readStringField(collection, 'id');
      return id?.startsWith('gid://shopify/Collection/') ? makeSeedCollection(id, collection) : null;
    })
    .filter((collection): collection is CollectionRecord => collection !== null);
  if (collections.length > 0) {
    runtime.store.upsertBaseCollections(collections);
  }

  const customers = readArrayField(nodeSeeds, 'customers')
    .filter(isPlainObject)
    .map((customer) => {
      const id = readStringField(customer, 'id');
      return id?.startsWith('gid://shopify/Customer/') ? makeSeedCustomer(id, customer) : null;
    })
    .filter((customer): customer is CustomerRecord => customer !== null);
  if (customers.length > 0) {
    runtime.store.upsertBaseCustomers(customers);
  }

  const locations = readArrayField(nodeSeeds, 'locations')
    .filter(isPlainObject)
    .map((location) => readLocationRecord(location))
    .filter((location): location is LocationRecord => location !== null);
  if (locations.length > 0) {
    runtime.store.upsertBaseLocations(locations);
  }
}

function readTaxonomyCategoryRecord(
  node: Record<string, unknown>,
  cursor: string | null,
): TaxonomyCategoryRecord | null {
  const id = readStringField(node, 'id');
  const name = readStringField(node, 'name');
  const fullName = readStringField(node, 'fullName');
  const isRoot = readBooleanField(node, 'isRoot');
  const isLeaf = readBooleanField(node, 'isLeaf');
  const level = readNumberField(node, 'level');
  const isArchived = readBooleanField(node, 'isArchived');
  if (!id || !name || !fullName || isRoot === null || isLeaf === null || level === null || isArchived === null) {
    return null;
  }

  return {
    id,
    cursor,
    name,
    fullName,
    isRoot,
    isLeaf,
    level,
    parentId: readNullableStringField(node, 'parentId'),
    ancestorIds: readArrayField(node, 'ancestorIds').filter(
      (ancestorId): ancestorId is string => typeof ancestorId === 'string',
    ),
    childrenIds: readArrayField(node, 'childrenIds').filter(
      (childId): childId is string => typeof childId === 'string',
    ),
    isArchived,
  };
}

function readTaxonomyConnectionCategories(connection: Record<string, unknown> | null): TaxonomyCategoryRecord[] {
  if (!connection) {
    return [];
  }

  const cursorByNodeId = new Map<string, string>();
  for (const edge of readArrayField(connection, 'edges').filter(isPlainObject)) {
    const cursor = readStringField(edge, 'cursor');
    const node = readRecordField(edge, 'node');
    const id = readStringField(node, 'id');
    if (cursor && id) {
      cursorByNodeId.set(id, cursor);
    }
  }

  return readArrayField(connection, 'nodes')
    .filter(isPlainObject)
    .map((node) => readTaxonomyCategoryRecord(node, cursorByNodeId.get(readStringField(node, 'id') ?? '') ?? null))
    .filter((category): category is TaxonomyCategoryRecord => category !== null);
}

function readTaxonomyCaptureCategories(
  captures: Record<string, unknown> | null,
  captureName: string,
): TaxonomyCategoryRecord[] {
  const payload = readRecordField(readRecordField(readRecordField(captures, captureName), 'result'), 'payload');
  const data = readRecordField(payload, 'data');
  const taxonomy = readRecordField(data, 'taxonomy');
  return readTaxonomyConnectionCategories(readRecordField(taxonomy, 'categories'));
}

function seedAdminPlatformTaxonomyPreconditions(runtime: ProxyRuntimeContext, capture: unknown): void {
  const captures = readRecordField(capture as Record<string, unknown>, 'captures');
  const categories = [
    ...readTaxonomyCaptureCategories(captures, 'taxonomyCatalogFirstPage'),
    ...readTaxonomyCaptureCategories(captures, 'taxonomyCatalogNextPage'),
    ...readTaxonomyCaptureCategories(captures, 'taxonomySearchApparel'),
    ...readTaxonomyCaptureCategories(captures, 'taxonomySearchApparelOverflowSeed'),
  ];
  if (categories.length > 0) {
    runtime.store.upsertBaseTaxonomyCategories(categories);
  }
}

function seedPreconditionsFromCapture(
  runtime: ProxyRuntimeContext,
  capture: unknown,
  variables: Record<string, unknown>,
): void {
  seedAdminPlatformNodePreconditions(runtime, capture);
  seedAdminPlatformTaxonomyPreconditions(runtime, capture);

  if (seedBulkVariantValidationAtomicityPreconditions(runtime, capture)) {
    return;
  }

  seedOnlineStoreContentPreconditions(runtime, capture);

  if (seedBulkOperationPreconditions(runtime, capture)) {
    return;
  }

  const seedCustomers = readArrayField(capture as Record<string, unknown>, 'seedCustomers')
    .filter(isPlainObject)
    .map((customer): CustomerRecord | null => {
      const customerId = readStringField(customer, 'id');
      return customerId ? makeSeedCustomer(customerId, customer) : null;
    })
    .filter((customer): customer is CustomerRecord => customer !== null);
  if (seedCustomers.length > 0) {
    runtime.store.upsertBaseCustomers(seedCustomers);
  }

  const seedSegments = readArrayField(capture as Record<string, unknown>, 'seedSegments')
    .filter(isPlainObject)
    .map((segment): SegmentRecord | null => {
      const segmentId = readStringField(segment, 'id');
      return segmentId ? makeSeedSegment(segmentId, segment) : null;
    })
    .filter((segment): segment is SegmentRecord => segment !== null);
  if (seedSegments.length > 0) {
    runtime.store.upsertBaseSegments(seedSegments);
  }

  const seedProducts = readArrayField(capture as Record<string, unknown>, 'seedProducts').filter(isPlainObject);
  for (const seedProduct of seedProducts) {
    const productId = readStringField(seedProduct, 'id');
    if (!productId?.startsWith('gid://shopify/Product/')) {
      continue;
    }
    runtime.store.upsertBaseProducts([makeSeedProduct(productId, seedProduct)]);
    const variants = readCapturedProductVariants(productId, seedProduct);
    if (variants.length > 0) {
      runtime.store.replaceBaseVariantsForProduct(productId, variants);
    }
    const options = readCapturedProductOptions(productId, seedProduct);
    if (options.length > 0) {
      runtime.store.replaceBaseOptionsForProduct(productId, options);
    }
  }
  seedCustomDataFieldTypeMatrixPreconditions(runtime, capture);
  if (seedProductContextualPricingReadPreconditions(runtime, capture)) {
    return;
  }
  const seedCollections = readArrayField(capture as Record<string, unknown>, 'seedCollections').filter(isPlainObject);
  for (const seedCollection of seedCollections) {
    const collectionId = readStringField(seedCollection, 'id');
    if (collectionId?.startsWith('gid://shopify/Collection/')) {
      runtime.store.upsertBaseCollections([makeSeedCollection(collectionId, seedCollection)]);
    }
  }
  const seedPublications = readArrayField(capture as Record<string, unknown>, 'seedPublications')
    .filter(isPlainObject)
    .map(readSeedPublication)
    .filter((publication): publication is PublicationRecord => publication !== null);
  if (seedPublications.length > 0) {
    runtime.store.upsertBasePublications(seedPublications);
  }
  const seedSellingPlanGroups = readArrayField(capture as Record<string, unknown>, 'seedSellingPlanGroups')
    .filter(isPlainObject)
    .map(readSeedSellingPlanGroup)
    .filter((group): group is SellingPlanGroupRecord => group !== null);
  if (seedSellingPlanGroups.length > 0) {
    runtime.store.upsertBaseSellingPlanGroups(seedSellingPlanGroups);
  }
  const seedChannels = readArrayField(capture as Record<string, unknown>, 'seedChannels')
    .filter(isPlainObject)
    .map(readSeedChannel)
    .filter((channel): channel is ChannelRecord => channel !== null);
  if (seedChannels.length > 0) {
    runtime.store.upsertBaseChannels(seedChannels);
  }
  const sellingPlanInput = readRecordField(variables, 'input');
  const sellingPlanResources = readRecordField(variables, 'resources');
  const isSellingPlanGroupLifecycleSeed =
    seedProducts.length > 0 &&
    (readArrayField(sellingPlanInput, 'sellingPlansToCreate').length > 0 ||
      readArrayField(sellingPlanInput, 'sellingPlansToUpdate').length > 0 ||
      readArrayField(sellingPlanResources, 'productIds').length > 0 ||
      readArrayField(sellingPlanResources, 'productVariantIds').length > 0);
  if (isSellingPlanGroupLifecycleSeed) {
    return;
  }
  seedExplicitProductMediaPreconditions(runtime, capture);
  seedLocalizationPreconditions(runtime, capture);

  seedProductMetafieldsReadPreconditions(runtime, capture);
  seedMetafieldDefinitionPreconditions(runtime, capture);
  if (seedMetaobjectReadPreconditions(runtime, capture)) {
    return;
  }
  if (seedInventoryLinkagePreconditions(runtime, capture)) {
    return;
  }

  if (seedInventoryQuantityRootPreconditions(runtime, capture)) {
    return;
  }

  if (seedGiftCardLifecyclePreconditions(runtime, capture)) {
    return;
  }

  if (seedMetafieldsDeleteOwnerProducts(runtime, capture, variables)) {
    return;
  }

  if (seedProductVariantUpdateCompatibilityPreconditions(runtime, capture, variables)) {
    return;
  }

  if (seedProductVariantDeleteCompatibilityPreconditions(runtime, capture, variables)) {
    return;
  }

  const payload = mutationPayloadFromCapture(capture);
  const mutationName = mutationNameFromCapture(capture);
  if (mutationName?.startsWith('sellingPlanGroup') && seedProducts.length > 0) {
    return;
  }

  if (seedFulfillmentLifecyclePreconditions(runtime, capture, mutationName)) {
    return;
  }

  if (seedFulfillmentOrderLifecyclePreconditions(runtime, capture)) {
    return;
  }

  if (seedFulfillmentOrderRequestLifecyclePreconditions(runtime, capture)) {
    return;
  }

  if (seedDraftOrderInvoiceSendSafetyPreconditions(runtime, capture)) {
    return;
  }

  if (seedCustomerMergePreconditions(runtime, capture, variables, mutationName)) {
    return;
  }

  if (seedCustomerInputValidationPreconditions(runtime, capture)) {
    return;
  }

  if (seedCustomerOrderSummaryPreconditions(runtime, capture)) {
    return;
  }

  if (seedStoreCreditAccountPreconditions(runtime, capture)) {
    return;
  }

  seedCustomerPaymentMethodPreconditions(runtime, capture);

  if (seedCustomerMutationPreconditions(runtime, capture, variables, mutationName, payload)) {
    return;
  }

  if (seedCustomerByIdentifierPreconditions(runtime, capture)) {
    return;
  }

  if (seedB2BCompanyPreconditions(runtime, capture)) {
    return;
  }

  if (seedShippingSettingsPreconditions(runtime, capture)) {
    return;
  }

  const seededShop = seedShopPreconditions(runtime, capture);
  const seededLocations = seedLocationDetailPreconditions(runtime, capture);
  const seededBusinessEntities = seedBusinessEntityPreconditions(runtime, capture);
  const seededStoreProperties = seededShop || seededLocations || seededBusinessEntities;
  if (seededStoreProperties) {
    return;
  }

  if (seedDeliveryProfilePreconditions(runtime, capture)) {
    return;
  }

  if (seedDeliveryProfileWritePreconditions(runtime, capture)) {
    return;
  }

  if (seedMarketsFromCapture(runtime, capture)) {
    return;
  }

  if (seedDiscountCatalogPreconditions(runtime, capture)) {
    seedShopifyFunctionPreconditions(runtime, capture);
    return;
  }

  if (seedShopifyFunctionPreconditions(runtime, capture)) {
    return;
  }

  const explicitSeedOrder = readRecordField(capture as Record<string, unknown>, 'seedOrder');
  const explicitSeedOrderId = readStringField(explicitSeedOrder, 'id');
  if (explicitSeedOrder && explicitSeedOrderId) {
    runtime.store.upsertBaseOrders([makeSeedOrder(explicitSeedOrderId, explicitSeedOrder)]);
  }

  const explicitSeedDraftOrder = readRecordField(capture as Record<string, unknown>, 'seedDraftOrder');
  const explicitSeedDraftOrderId = readStringField(explicitSeedDraftOrder, 'id');
  if (explicitSeedDraftOrder && explicitSeedDraftOrderId) {
    runtime.store.stageCreateDraftOrder(makeSeedDraftOrder(explicitSeedDraftOrderId, explicitSeedDraftOrder));
  }

  const readOrderPayload =
    readRecordField(
      readRecordField(readRecordField(capture as Record<string, unknown>, 'response'), 'data'),
      'order',
    ) ?? readRecordField(readRecordField(capture as Record<string, unknown>, 'data'), 'order');
  const readOrderId = readStringField(readOrderPayload, 'id') ?? readStringField(variables, 'id');
  if (!mutationName && readOrderPayload && readOrderId) {
    runtime.store.upsertBaseOrders([makeSeedOrder(readOrderId, readOrderPayload)]);
    return;
  }
  if (!mutationName && (capture as Record<string, unknown>)['seedOrderCatalogFromCapture'] === true) {
    const responsePayload = readRecordField(capture as Record<string, unknown>, 'response');
    if (responsePayload) {
      hydrateOrdersFromUpstreamResponse(runtime, responsePayload);
    }
    const nextPageResponse = readRecordField(
      readRecordField(capture as Record<string, unknown>, 'nextPage'),
      'response',
    );
    if (nextPageResponse) {
      hydrateOrdersFromUpstreamResponse(runtime, nextPageResponse);
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
      runtime.store.upsertBaseOrders([makeSeedOrder(seedOrderId, seedOrder)]);
    }
    const seedProducts = readArrayField(capture as Record<string, unknown>, 'seedProducts').filter(isPlainObject);
    for (const seedProduct of seedProducts) {
      const productId = readStringField(seedProduct, 'id');
      if (!productId?.startsWith('gid://shopify/Product/')) {
        continue;
      }
      runtime.store.upsertBaseProducts([makeSeedProduct(productId, seedProduct)]);
      const variants = readCapturedProductVariants(productId, seedProduct);
      if (variants.length > 0) {
        runtime.store.replaceBaseVariantsForProduct(productId, variants);
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
        runtime.store.upsertBaseOrders([makeSeedOrder(orderId, seedSource)]);
      }
    }
    return;
  }

  if (mutationName === 'draftOrderCreate') {
    const draftOrderPayload = readRecordField(payload, 'draftOrder');
    const customerPayload = readRecordField(draftOrderPayload, 'customer');
    const customerId = readStringField(customerPayload, 'id');
    if (customerId) {
      runtime.store.upsertBaseCustomers([makeSeedCustomer(customerId, customerPayload)]);
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
      runtime.store.upsertBaseProducts([
        makeSeedProduct(productId, {
          id: productId,
          title: productTitle,
        }),
      ]);
      runtime.store.replaceBaseVariantsForProduct(productId, [
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
      runtime.store.stageCreateDraftOrder(seedDraftOrder);
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
        runtime.store.stageCreateDraftOrder(makeSeedDraftOrder(draftOrderId, setupSource));
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
        runtime.store.upsertBaseOrders([makeSeedOrder(orderId, orderSource)]);
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
      runtime.store.upsertBaseOrders([makeSeedOrder(seedId, orderPayload)]);
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
      runtime.store.upsertBaseOrders([makeSeedOrder(orderId, orderPayload)]);
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
        runtime.store.upsertBaseOrders([makeSeedOrder(orderId, seedSource)]);
      }
    }
    return;
  }

  if (mutationName === 'inventoryAdjustQuantities') {
    seedInventoryAdjustmentPreconditions(runtime, capture);
    return;
  }

  if (seedInventoryItemUpdatePreconditions(runtime, capture)) {
    return;
  }

  if (seedFileDeleteMediaReferencePreconditions(runtime, capture, variables)) {
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
  const duplicateOperationPayload = readRecordField(payload, 'productDuplicateOperation');
  const duplicateOperationReadPayload = readRecordField(
    readRecordField(readRecordField(capture as Record<string, unknown>, 'operationRead'), 'response'),
    'data',
  );
  const duplicateOperationUserErrors = readArrayField(
    readRecordField(duplicateOperationReadPayload, 'productOperation'),
    'userErrors',
  ).filter(isPlainObject);
  const isMissingProductValidationProbe =
    ((mutationName === 'productUpdate' || mutationName === 'productChangeStatus') &&
      productPayload === null &&
      productUserErrors.some((userError) => {
        const fieldPath = readArrayField(userError, 'field');
        return (
          (fieldPath.includes('id') || fieldPath.includes('productId')) &&
          readStringField(userError, 'message') === 'Product does not exist'
        );
      })) ||
    (mutationName === 'productDuplicate' &&
      readRecordField(duplicateOperationPayload, 'product') === null &&
      readRecordField(duplicateOperationPayload, 'newProduct') === null &&
      duplicateOperationUserErrors.some((userError) => {
        const fieldPath = readArrayField(userError, 'field');
        return fieldPath.includes('productId') && readStringField(userError, 'message') === 'Product does not exist';
      }));

  const shouldSeedProduct =
    productId !== null &&
    !(mutationName === 'productCreate' && readStringField(productInput, 'id') === null) &&
    !isProductSetCreate &&
    !isProductDeleteValidationProbe &&
    !isMissingProductValidationProbe;

  if (seedProductDuplicateSource(runtime, capture)) {
    return;
  }

  if (shouldSeedProduct) {
    if (
      mutationName === 'tagsRemove' &&
      seedTagsRemovePreconditions(runtime, productId, productPayload, capture, variables)
    ) {
      return;
    }

    if (
      mutationName === 'productVariantsBulkReorder' &&
      seedProductVariantsBulkReorderPreconditions(runtime, capture, productId)
    ) {
      return;
    }

    if (mutationName === 'productReorderMedia' && seedProductReorderMediaPreconditions(runtime, capture, productId)) {
      return;
    }

    const captureSeedProduct = readRecordField(capture as Record<string, unknown>, 'seedProduct');
    const seedSource =
      mutationName === 'tagsAdd'
        ? null
        : readStringField(captureSeedProduct, 'id') === productId
          ? captureSeedProduct
          : (productPayload ?? productInput);
    runtime.store.upsertBaseProducts([makeSeedProduct(productId, seedSource)]);
    if (
      mutationName === 'productVariantsBulkCreate' ||
      mutationName === 'productVariantsBulkUpdate' ||
      mutationName === 'productVariantsBulkDelete'
    ) {
      const preMutationProduct =
        mutationName === 'productVariantsBulkCreate' ? readPreMutationProduct(capture, productId) : null;
      const downstreamProduct = readRecordField(
        readRecordField(readRecordField(capture as Record<string, unknown>, 'downstreamRead'), 'data'),
        'product',
      );
      const variantsSource =
        preMutationProduct ??
        (readStringField(downstreamProduct, 'id') === productId ? downstreamProduct : productPayload);
      const variants =
        mutationName === 'productVariantsBulkCreate'
          ? readCapturedProductVariants(productId, variantsSource).filter((variant) =>
              preMutationProduct ? true : !readCapturedCreatedVariantIds(payload).has(variant.id),
            )
          : mutationName === 'productVariantsBulkUpdate'
            ? readBulkUpdateSeedVariants(productId, variantsSource)
            : readCapturedProductVariants(productId, variantsSource);
      if (variants.length > 0) {
        runtime.store.replaceBaseVariantsForProduct(productId, variants);
      }
      if (preMutationProduct) {
        const options = readCapturedProductOptions(productId, preMutationProduct);
        if (options.length > 0) {
          runtime.store.replaceBaseOptionsForProduct(productId, options);
        }
      }
    }
    if (readArrayField(variables, 'options').length > 0 || readRecordField(variables, 'option')) {
      seedProductOptionState(runtime, productId, variables, capture);
    }

    if (mutationName === 'productUpdateMedia') {
      const downstreamProduct = readRecordField(
        readRecordField(readRecordField(capture as Record<string, unknown>, 'downstreamRead'), 'data'),
        'product',
      );
      const mediaSource = readStringField(downstreamProduct, 'id') === productId ? downstreamProduct : null;
      const capturedMedia = readCapturedProductMedia(productId, mediaSource);
      if (capturedMedia.length > 0) {
        runtime.store.replaceBaseMediaForProduct(productId, capturedMedia);
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
        runtime.store.replaceBaseMediaForProduct(
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
    seedMetafieldsSetOwnerProducts(runtime, capture, variables);
  }

  const collectionPayload =
    readRecordField(payload, 'collection') ??
    (readStringField(readRecordField(payload, 'publishable'), 'id')?.startsWith('gid://shopify/Collection/')
      ? readRecordField(payload, 'publishable')
      : null);
  const initialCollectionPayload = readRecordField(
    readRecordField(
      readRecordField(readRecordField(capture as Record<string, unknown>, 'initialCollectionRead'), 'response'),
      'data',
    ) ?? readRecordField(readRecordField(capture as Record<string, unknown>, 'initialCollectionRead'), 'data'),
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
    runtime.store.upsertBaseCollections([collection]);
    const seedProducts = readArrayField(capture as Record<string, unknown>, 'seedProducts').filter(isPlainObject);
    for (const seedProduct of seedProducts) {
      const productId = readStringField(seedProduct, 'id');
      if (productId?.startsWith('gid://shopify/Product/')) {
        runtime.store.upsertBaseProducts([makeSeedProduct(productId, seedProduct)]);
      }
    }
    const rawProductNodes = readRecordField(collectionPayload, 'products')?.['nodes'];
    const productNodes = Array.isArray(rawProductNodes) ? rawProductNodes : [];
    const initialProductNodes = readArrayField(readRecordField(initialCollectionPayload, 'products'), 'nodes');
    if (mutationName === 'collectionReorderProducts') {
      seedCollectionProducts(runtime, collection, initialProductNodes);
    } else if (mutationName === 'collectionUpdate') {
      seedCollectionProducts(runtime, collection, productNodes);
    } else {
      for (const node of productNodes.filter(isPlainObject)) {
        const productId = readStringField(node, 'id');
        if (productId) {
          runtime.store.upsertBaseProducts([makeSeedProduct(productId, node)]);
        }
      }
    }
    seedPreexistingProductCollectionsFromReadPayload(
      runtime,
      readRecordField(capture as Record<string, unknown>, 'initialCollectionRead'),
      collection.id,
    );
    seedPreexistingProductCollectionsFromReadPayload(
      runtime,
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
      if (!runtime.store.getEffectiveProductById(productIdValue)) {
        runtime.store.upsertBaseProducts([makeSeedProduct(productIdValue)]);
      }
    }
  }
}

function readComparisonTargets(comparison: ComparisonContract): ComparisonTarget[] {
  return Array.isArray(comparison.targets) ? comparison.targets : [];
}

function selectComparisonPaths(value: unknown, selectedPaths: string[] | undefined): unknown {
  if (!selectedPaths) {
    return value;
  }

  return Object.fromEntries(selectedPaths.map((selectedPath) => [selectedPath, readJsonPath(value, selectedPath)]));
}

function cloneJsonLikeValue(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(cloneJsonLikeValue);
  }

  if (isPlainObject(value)) {
    return Object.fromEntries(Object.entries(value).map(([key, child]) => [key, cloneJsonLikeValue(child)]));
  }

  return value;
}

function deleteJsonPath(value: unknown, segments: PathSegment[]): void {
  if (segments.length === 0 || value === null || typeof value !== 'object') {
    return;
  }

  const segment = segments[0]!;
  const rest = segments.slice(1);
  if (segment === '*') {
    const children = Array.isArray(value) ? value : Object.values(value as Record<string, unknown>);
    for (const child of children) {
      deleteJsonPath(child, rest);
    }
    return;
  }

  if (rest.length === 0) {
    if (Array.isArray(value) && typeof segment === 'number') {
      value.splice(segment, 1);
      return;
    }

    delete (value as Record<string | number, unknown>)[segment];
    return;
  }

  deleteJsonPath((value as Record<string | number, unknown>)[segment], rest);
}

export function excludeComparisonPaths(value: unknown, excludedPaths: string[] | undefined): unknown {
  if (!excludedPaths) {
    return value;
  }

  const clone = cloneJsonLikeValue(value);
  for (const excludedPath of excludedPaths) {
    deleteJsonPath(clone, parsePath(excludedPath));
  }
  return clone;
}

function prepareComparisonValue(value: unknown, target: ComparisonTarget): unknown {
  return excludeComparisonPaths(selectComparisonPaths(value, target.selectedPaths), target.excludedPaths);
}

function readRequestVariables(
  repoRoot: string,
  request: ProxyRequestSpec,
  capture: unknown,
  proxyResponses: Record<string, unknown>,
  previousProxyResponse: unknown,
): Record<string, unknown> {
  if (request.variablesCapturePath) {
    return materializeVariables(
      readJsonPath(capture, request.variablesCapturePath),
      proxyResponses,
      previousProxyResponse,
      capture,
    );
  }

  const rawVariables = request.variablesPath
    ? parseJsonFileWithSchema(path.join(repoRoot, request.variablesPath), graphqlVariablesSchema)
    : request.variables;
  return materializeVariables(rawVariables, proxyResponses, previousProxyResponse, capture);
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
  operationNameValidation: OperationNameValidationResult;
}> {
  const runtimeStore = new InMemoryStore();
  const syntheticIdentity = new SyntheticIdentityRegistry();
  const runtime = { store: runtimeStore, syntheticIdentity };
  return executeParityScenarioInRuntime(runtime, {
    repoRoot,
    scenario,
    paritySpec,
  });
}

async function executeParityScenarioInRuntime(
  runtime: ProxyRuntimeContext,
  {
    repoRoot,
    scenario,
    paritySpec,
  }: {
    repoRoot: string;
    scenario: Scenario;
    paritySpec: ParitySpec;
  },
): Promise<{
  ok: boolean;
  primaryProxyStatus: number;
  comparisons: Array<{ name: string; ok: boolean; differences: Difference[] }>;
  operationNameValidation: OperationNameValidationResult;
}> {
  if (!paritySpec.proxyRequest?.documentPath) {
    throw new Error(`Scenario ${scenario.id} does not define a proxy request.`);
  }
  if (validateComparisonContract(paritySpec.comparison).length > 0 || !paritySpec.comparison) {
    throw new Error(`Scenario ${scenario.id} does not define a valid comparison contract.`);
  }
  if (readComparisonTargets(paritySpec.comparison).length === 0) {
    throw new Error(`Scenario ${scenario.id} must declare at least one comparison target.`);
  }

  runtime.store.reset();

  const capturePath = scenario.captureFiles?.[0] ?? paritySpec.liveCaptureFiles?.[0];
  if (typeof capturePath !== 'string') {
    throw new Error(`Scenario ${scenario.id} does not reference a capture fixture.`);
  }

  const capture = readJsonFile(repoRoot, capturePath);
  const captureApiVersion = readCaptureApiVersion(capture) ?? readApiVersionFromCapturePath(capturePath);
  const primaryDocument = readTextFile(repoRoot, paritySpec.proxyRequest.documentPath);
  const proxyResponses: Record<string, unknown> = {};
  const primaryVariables = readRequestVariables(repoRoot, paritySpec.proxyRequest, capture, proxyResponses, {});
  const executedOperations: ExecutedOperation[] = [];
  seedPreconditionsFromCapture(runtime, capture, primaryVariables);
  const primaryProxyResponse = await executeGraphQLAgainstLocalProxy(
    runtime,
    primaryDocument,
    primaryVariables,
    readPrimaryUpstreamPayload(capture, paritySpec.comparison, primaryDocument),
    (operation) => executedOperations.push(operation),
    paritySpec.proxyRequest.apiVersion ?? captureApiVersion ?? DEFAULT_ADMIN_API_VERSION,
  );
  proxyResponses['primary'] = primaryProxyResponse.body;

  const comparisons = [];
  let previousProxyResponseBody: unknown = primaryProxyResponse.body;
  for (const target of readComparisonTargets(paritySpec.comparison)) {
    const expected = readJsonPath(capture, target.capturePath);
    let proxyResponseBody: unknown = primaryProxyResponse.body;

    if (target.proxyRequest?.documentPath) {
      if (typeof target.proxyRequest.waitBeforeMs === 'number' && target.proxyRequest.waitBeforeMs > 0) {
        await sleep(target.proxyRequest.waitBeforeMs);
      }
      const document = readTextFile(repoRoot, target.proxyRequest.documentPath);
      const variables = readRequestVariables(
        repoRoot,
        target.proxyRequest,
        capture,
        proxyResponses,
        previousProxyResponseBody,
      );
      const upstreamPayload =
        target.upstreamCapturePath === null
          ? undefined
          : typeof target.upstreamCapturePath === 'string'
            ? readJsonPath(capture, target.upstreamCapturePath)
            : undefined;
      const proxyResponse = await executeGraphQLAgainstLocalProxy(
        runtime,
        document,
        variables,
        upstreamPayload,
        (operation) => executedOperations.push(operation),
        target.proxyRequest.apiVersion ??
          paritySpec.proxyRequest?.apiVersion ??
          captureApiVersion ??
          DEFAULT_ADMIN_API_VERSION,
      );
      proxyResponseBody = proxyResponse.body;
      previousProxyResponseBody = proxyResponse.body;
      proxyResponses[target.name] = proxyResponse.body;
    }

    const actual = readJsonPath(proxyResponseBody, target.proxyPath);
    const expectedDifferences = [
      ...(paritySpec.comparison.expectedDifferences ?? []),
      ...(target.expectedDifferences ?? []),
    ];
    const comparison = compareJsonPayloads(
      prepareComparisonValue(expected, target),
      prepareComparisonValue(actual, target),
      { expectedDifferences },
    );
    comparisons.push({
      name: target.name,
      ok: comparison.ok,
      differences: comparison.differences,
    });
  }

  const operationNameValidation = validateParityScenarioOperationNames({
    scenario,
    paritySpec,
    executedOperations,
  });

  return {
    ok: comparisons.every((comparison) => comparison.ok) && operationNameValidation.errors.length === 0,
    primaryProxyStatus: primaryProxyResponse.status,
    comparisons,
    operationNameValidation,
  };
}
