import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';

type OperationRegistryEntry = {
  name: string;
  domain: string;
  execution: string;
  implemented: boolean;
  runtimeTests?: string[];
};

type ConformanceScenario = {
  id: string;
  operationNames: string[];
  status: string;
  captureFiles: string[];
  paritySpecPath: string;
};

type ParitySpec = {
  scenarioId: string;
  scenarioStatus: string;
  liveCaptureFiles: string[];
  comparisonMode: string;
  proxyRequest?: {
    documentPath?: string | null;
    variablesPath?: string | null;
  };
  blocker?: {
    kind?: string;
    blockerPath?: string;
    details?: {
      requiredScopes?: string[];
      probeRoots?: string[];
      failingMessage?: string;
      requiredTokenMode?: string;
      requiredPermissions?: string[];
      manualStoreAuthStatus?: string;
      manualStoreAuthTokenPath?: string;
      manualStoreAuthCachedScopes?: string[];
      manualStoreAuthAssociatedUserScopes?: string[];
    };
  };
};

const expectedOrderRegistryEntries = [
  {
    name: 'order',
    execution: 'overlay-read',
    implemented: true,
    conformanceStatus: 'covered',
    runtimeTests: ['tests/integration/order-query-shapes.test.ts'],
  },
  {
    name: 'orders',
    execution: 'overlay-read',
    implemented: true,
    conformanceStatus: 'covered',
    runtimeTests: ['tests/integration/order-query-shapes.test.ts'],
  },
  {
    name: 'ordersCount',
    execution: 'overlay-read',
    implemented: true,
    conformanceStatus: 'covered',
    runtimeTests: ['tests/integration/order-query-shapes.test.ts'],
  },
  {
    name: 'orderCreate',
    execution: 'stage-locally',
    implemented: true,
    conformanceStatus: 'covered',
    runtimeTests: ['tests/integration/order-creation-flow.test.ts'],
  },
  {
    name: 'draftOrder',
    execution: 'overlay-read',
    implemented: true,
    conformanceStatus: 'covered',
    runtimeTests: ['tests/integration/order-creation-flow.test.ts'],
  },
  {
    name: 'draftOrders',
    execution: 'overlay-read',
    implemented: true,
    conformanceStatus: 'covered',
    runtimeTests: ['tests/integration/order-creation-flow.test.ts'],
  },
  {
    name: 'draftOrdersCount',
    execution: 'overlay-read',
    implemented: true,
    conformanceStatus: 'covered',
    runtimeTests: ['tests/integration/order-creation-flow.test.ts'],
  },
  {
    name: 'draftOrderCreate',
    execution: 'stage-locally',
    implemented: true,
    conformanceStatus: 'covered',
    runtimeTests: ['tests/integration/order-creation-flow.test.ts'],
  },
  {
    name: 'draftOrderComplete',
    execution: 'stage-locally',
    implemented: true,
    conformanceStatus: 'covered',
    runtimeTests: ['tests/integration/order-creation-flow.test.ts'],
  },
] as const;

const expectedCapturedScenarios = [
  {
    id: 'order-create-inline-missing-order-argument-error',
    operationName: 'orderCreate',
    captureFile:
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-create-inline-missing-order.json',
    paritySpecPath: 'config/parity-specs/orderCreate-inline-missing-order-parity.json',
    documentPath: 'config/parity-requests/orderCreate-inline-missing-order-parity.graphql',
    variablesPath: 'config/parity-requests/orderCreate-inline-missing-order-parity.variables.json',
  },
  {
    id: 'order-create-inline-null-order-argument-error',
    operationName: 'orderCreate',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-create-inline-null-order.json',
    paritySpecPath: 'config/parity-specs/orderCreate-inline-null-order-parity.json',
    documentPath: 'config/parity-requests/orderCreate-inline-null-order-parity.graphql',
    variablesPath: 'config/parity-requests/orderCreate-inline-null-order-parity.variables.json',
  },
  {
    id: 'order-create-missing-order-invalid-variable',
    operationName: 'orderCreate',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-create-missing-order.json',
    paritySpecPath: 'config/parity-specs/orderCreate-missing-order-parity.json',
    documentPath: 'config/parity-requests/orderCreate-missing-order-parity.graphql',
    variablesPath: 'config/parity-requests/orderCreate-missing-order-parity.variables.json',
  },
  {
    id: 'order-create-live-parity',
    operationName: 'orderCreate',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-create-parity.json',
    paritySpecPath: 'config/parity-specs/orderCreate-parity-plan.json',
    documentPath: 'config/parity-requests/orderCreate-parity-plan.graphql',
    variablesPath: 'config/parity-requests/orderCreate-parity-plan.variables.json',
  },
  {
    id: 'draft-order-create-inline-missing-input-argument-error',
    operationName: 'draftOrderCreate',
    captureFile:
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-create-inline-missing-input.json',
    paritySpecPath: 'config/parity-specs/draftOrderCreate-inline-missing-input-parity.json',
    documentPath: 'config/parity-requests/draftOrderCreate-inline-missing-input-parity.graphql',
    variablesPath: 'config/parity-requests/draftOrderCreate-inline-missing-input-parity.variables.json',
  },
  {
    id: 'draft-order-create-inline-null-input-argument-error',
    operationName: 'draftOrderCreate',
    captureFile:
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-create-inline-null-input.json',
    paritySpecPath: 'config/parity-specs/draftOrderCreate-inline-null-input-parity.json',
    documentPath: 'config/parity-requests/draftOrderCreate-inline-null-input-parity.graphql',
    variablesPath: 'config/parity-requests/draftOrderCreate-inline-null-input-parity.variables.json',
  },
  {
    id: 'draft-order-create-missing-input-invalid-variable',
    operationName: 'draftOrderCreate',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-create-missing-input.json',
    paritySpecPath: 'config/parity-specs/draftOrderCreate-missing-input-parity.json',
    documentPath: 'config/parity-requests/draftOrderCreate-missing-input-parity.graphql',
    variablesPath: 'config/parity-requests/draftOrderCreate-missing-input-parity.variables.json',
  },
  {
    id: 'draft-order-create-live-parity',
    operationName: 'draftOrderCreate',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-create-parity.json',
    paritySpecPath: 'config/parity-specs/draftOrderCreate-parity-plan.json',
    documentPath: 'config/parity-requests/draftOrderCreate-parity-plan.graphql',
    variablesPath: 'config/parity-requests/draftOrderCreate-parity-plan.variables.json',
  },
  {
    id: 'draft-order-complete-inline-missing-id-argument-error',
    operationName: 'draftOrderComplete',
    captureFile:
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-complete-inline-missing-id.json',
    paritySpecPath: 'config/parity-specs/draftOrderComplete-inline-missing-id-parity.json',
    documentPath: 'config/parity-requests/draftOrderComplete-inline-missing-id-parity.graphql',
    variablesPath: 'config/parity-requests/draftOrderComplete-inline-missing-id-parity.variables.json',
  },
  {
    id: 'draft-order-complete-inline-null-id-argument-error',
    operationName: 'draftOrderComplete',
    captureFile:
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-complete-inline-null-id.json',
    paritySpecPath: 'config/parity-specs/draftOrderComplete-inline-null-id-parity.json',
    documentPath: 'config/parity-requests/draftOrderComplete-inline-null-id-parity.graphql',
    variablesPath: 'config/parity-requests/draftOrderComplete-inline-null-id-parity.variables.json',
  },
  {
    id: 'draft-order-complete-missing-id-invalid-variable',
    operationName: 'draftOrderComplete',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-complete-missing-id.json',
    paritySpecPath: 'config/parity-specs/draftOrderComplete-missing-id-parity.json',
    documentPath: 'config/parity-requests/draftOrderComplete-missing-id-parity.graphql',
    variablesPath: 'config/parity-requests/draftOrderComplete-missing-id-parity.variables.json',
  },
] as const;

const expectedBlockedScenarios = [
  {
    id: 'draft-order-complete-live-parity',
    operationName: 'draftOrderComplete',
    paritySpecPath: 'config/parity-specs/draftOrderComplete-parity-plan.json',
    documentPath: 'config/parity-requests/draftOrderComplete-parity-plan.graphql',
    variablesPath: 'config/parity-requests/draftOrderComplete-parity-plan.variables.json',
    blockerKind: 'missing-live-draft-order-complete-access',
    requiredScopes: ['write_draft_orders'],
    probeRoots: ['draftOrderComplete'],
    failingMessageIncludes: 'mark as paid',
    requiredTokenMode: undefined,
    operationLine: 'mutation DraftOrderCompleteParityPlan($id: ID!, $paymentGatewayId: ID, $sourceName: String)',
  },
] as const;

describe('order creation scaffolding', () => {
  it('keeps the order capture script syntactically valid so conformance:capture-orders can execute', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');

    expect(() =>
      execFileSync(process.execPath, ['--check', resolve(repoRoot, 'scripts/capture-order-conformance.mts')], {
        cwd: repoRoot,
        stdio: 'pipe',
      }),
    ).not.toThrow();
  });

  it('registers the initial order and draft-order family with explicit implemented-vs-blocked order-domain status', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];

    for (const expected of expectedOrderRegistryEntries) {
      expect(registry).toContainEqual(
        expect.objectContaining({
          name: expected.name,
          domain: 'orders',
          execution: expected.execution,
          implemented: expected.implemented,
          ...('runtimeTests' in expected ? { runtimeTests: expected.runtimeTests } : {}),
        }),
      );
    }
  });

  it('tracks the first captured-vs-blocked order-creation scenarios explicitly instead of leaving them ad hoc', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const scenarios = loadConformanceScenarios(repoRoot) as ConformanceScenario[];

    for (const expected of expectedCapturedScenarios) {
      expect(scenarios).toContainEqual(
        expect.objectContaining({
          id: expected.id,
          operationNames: [expected.operationName],
          status: 'captured',
          captureFiles: [expected.captureFile],
          paritySpecPath: expected.paritySpecPath,
        }),
      );
    }

    for (const expected of expectedBlockedScenarios) {
      expect(scenarios).toContainEqual(
        expect.objectContaining({
          id: expected.id,
          operationNames: [expected.operationName],
          status: 'planned',
          captureFiles: [],
          paritySpecPath: expected.paritySpecPath,
        }),
      );
    }
  });

  it('keeps the captured orderCreate inline-argument validation parity artifacts executable', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const inlineMissingExpected = expectedCapturedScenarios[0];
    const inlineNullExpected = expectedCapturedScenarios[1];

    const inlineMissingSpec = JSON.parse(
      readFileSync(resolve(repoRoot, inlineMissingExpected.paritySpecPath), 'utf8'),
    ) as ParitySpec;
    const inlineMissingDocument = readFileSync(resolve(repoRoot, inlineMissingExpected.documentPath), 'utf8');
    const inlineMissingVariables = JSON.parse(
      readFileSync(resolve(repoRoot, inlineMissingExpected.variablesPath), 'utf8'),
    ) as Record<string, unknown>;

    expect(inlineMissingSpec).toEqual(
      expect.objectContaining({
        scenarioId: inlineMissingExpected.id,
        scenarioStatus: 'captured',
        liveCaptureFiles: [inlineMissingExpected.captureFile],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          documentPath: inlineMissingExpected.documentPath,
          variablesPath: inlineMissingExpected.variablesPath,
        },
      }),
    );
    expect(inlineMissingDocument).toContain('mutation InlineMissingOrderArg');
    expect(inlineMissingDocument).toContain('orderCreate');
    expect(inlineMissingDocument).not.toContain('orderCreate(order:');
    expect(inlineMissingVariables).toEqual({});

    const inlineNullSpec = JSON.parse(
      readFileSync(resolve(repoRoot, inlineNullExpected.paritySpecPath), 'utf8'),
    ) as ParitySpec;
    const inlineNullDocument = readFileSync(resolve(repoRoot, inlineNullExpected.documentPath), 'utf8');
    const inlineNullVariables = JSON.parse(
      readFileSync(resolve(repoRoot, inlineNullExpected.variablesPath), 'utf8'),
    ) as Record<string, unknown>;

    expect(inlineNullSpec).toEqual(
      expect.objectContaining({
        scenarioId: inlineNullExpected.id,
        scenarioStatus: 'captured',
        liveCaptureFiles: [inlineNullExpected.captureFile],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          documentPath: inlineNullExpected.documentPath,
          variablesPath: inlineNullExpected.variablesPath,
        },
      }),
    );
    expect(inlineNullDocument).toContain('mutation InlineNullOrderArg');
    expect(inlineNullDocument).toContain('orderCreate(order: null)');
    expect(inlineNullVariables).toEqual({});
  });

  it('keeps the captured orderCreate missing-order parity artifacts executable', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const expected = expectedCapturedScenarios[2];

    const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.paritySpecPath), 'utf8')) as ParitySpec;
    const document = readFileSync(resolve(repoRoot, expected.documentPath), 'utf8');
    const variables = JSON.parse(readFileSync(resolve(repoRoot, expected.variablesPath), 'utf8')) as Record<
      string,
      unknown
    >;

    expect(spec).toEqual(
      expect.objectContaining({
        scenarioId: expected.id,
        scenarioStatus: 'captured',
        liveCaptureFiles: [expected.captureFile],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          documentPath: expected.documentPath,
          variablesPath: expected.variablesPath,
        },
      }),
    );
    expect(document).toContain('mutation OrderCreateMissingOrderParity($order: OrderCreateOrderInput!)');
    expect(document).toContain('orderCreate(order: $order)');
    expect(document).toContain('userErrors');
    expect(variables).toEqual({});
  });

  it('keeps the captured orderCreate happy-path parity artifacts executable', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const expected = expectedCapturedScenarios[3];

    const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.paritySpecPath), 'utf8')) as ParitySpec;
    const document = readFileSync(resolve(repoRoot, expected.documentPath), 'utf8');
    const variables = JSON.parse(readFileSync(resolve(repoRoot, expected.variablesPath), 'utf8')) as Record<
      string,
      unknown
    >;

    expect(spec).toEqual(
      expect.objectContaining({
        scenarioId: expected.id,
        scenarioStatus: 'captured',
        liveCaptureFiles: [expected.captureFile],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          documentPath: expected.documentPath,
          variablesPath: expected.variablesPath,
        },
      }),
    );
    expect(document).toContain('billingAddress');
    expect(document).toContain('shippingAddress');
    expect(document).toContain('shippingLines(first: 5)');
    expect(document).toContain('customAttributes');
    expect(document).toContain('lineItems');
    expect(document).toContain('totalPriceSet');
    expect(document).toContain('userErrors');
    expect(variables).toMatchObject({
      order: expect.objectContaining({
        email: expect.stringContaining('hermes-order-parity-plan-'),
        note: 'order create parity plan',
        test: true,
        customAttributes: expect.arrayContaining([
          expect.objectContaining({ key: 'source', value: 'hermes-parity-plan' }),
        ]),
        billingAddress: expect.objectContaining({
          city: 'Toronto',
          countryCode: 'CA',
        }),
        shippingAddress: expect.objectContaining({
          city: 'Toronto',
          countryCode: 'CA',
        }),
        shippingLines: expect.arrayContaining([expect.objectContaining({ title: 'Standard', code: 'STANDARD' })]),
      }),
    });
  });

  it('keeps the captured draftOrderCreate inline-argument validation parity artifacts executable', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const inlineMissingExpected = expectedCapturedScenarios[4];
    const inlineNullExpected = expectedCapturedScenarios[5];

    const inlineMissingSpec = JSON.parse(
      readFileSync(resolve(repoRoot, inlineMissingExpected.paritySpecPath), 'utf8'),
    ) as ParitySpec;
    const inlineMissingDocument = readFileSync(resolve(repoRoot, inlineMissingExpected.documentPath), 'utf8');
    const inlineMissingVariables = JSON.parse(
      readFileSync(resolve(repoRoot, inlineMissingExpected.variablesPath), 'utf8'),
    ) as Record<string, unknown>;

    expect(inlineMissingSpec).toEqual(
      expect.objectContaining({
        scenarioId: inlineMissingExpected.id,
        scenarioStatus: 'captured',
        liveCaptureFiles: [inlineMissingExpected.captureFile],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          documentPath: inlineMissingExpected.documentPath,
          variablesPath: inlineMissingExpected.variablesPath,
        },
      }),
    );
    expect(inlineMissingDocument).toContain('mutation InlineMissingDraftOrderInput');
    expect(inlineMissingDocument).toContain('draftOrderCreate {');
    expect(inlineMissingVariables).toEqual({});

    const inlineNullSpec = JSON.parse(
      readFileSync(resolve(repoRoot, inlineNullExpected.paritySpecPath), 'utf8'),
    ) as ParitySpec;
    const inlineNullDocument = readFileSync(resolve(repoRoot, inlineNullExpected.documentPath), 'utf8');
    const inlineNullVariables = JSON.parse(
      readFileSync(resolve(repoRoot, inlineNullExpected.variablesPath), 'utf8'),
    ) as Record<string, unknown>;

    expect(inlineNullSpec).toEqual(
      expect.objectContaining({
        scenarioId: inlineNullExpected.id,
        scenarioStatus: 'captured',
        liveCaptureFiles: [inlineNullExpected.captureFile],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          documentPath: inlineNullExpected.documentPath,
          variablesPath: inlineNullExpected.variablesPath,
        },
      }),
    );
    expect(inlineNullDocument).toContain('mutation InlineNullDraftOrderInput');
    expect(inlineNullDocument).toContain('draftOrderCreate(input: null)');
    expect(inlineNullVariables).toEqual({});
  });

  it('keeps the captured draftOrderCreate missing-input parity artifacts executable', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const expected = expectedCapturedScenarios[6];

    const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.paritySpecPath), 'utf8')) as ParitySpec;
    const document = readFileSync(resolve(repoRoot, expected.documentPath), 'utf8');
    const variables = JSON.parse(readFileSync(resolve(repoRoot, expected.variablesPath), 'utf8')) as Record<
      string,
      unknown
    >;

    expect(spec).toEqual(
      expect.objectContaining({
        scenarioId: expected.id,
        scenarioStatus: 'captured',
        liveCaptureFiles: [expected.captureFile],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          documentPath: expected.documentPath,
          variablesPath: expected.variablesPath,
        },
      }),
    );
    expect(document).toContain('mutation DraftOrderCreateMissingInputParity($input: DraftOrderInput!)');
    expect(document).toContain('draftOrderCreate(input: $input)');
    expect(document).toContain('userErrors');
    expect(variables).toEqual({});
  });

  it('keeps the captured draftOrderComplete validation parity artifacts executable', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const expectedScenarios = expectedCapturedScenarios.filter(
      (scenario) => scenario.operationName === 'draftOrderComplete',
    );

    for (const expected of expectedScenarios) {
      const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.paritySpecPath), 'utf8')) as ParitySpec;
      const document = readFileSync(resolve(repoRoot, expected.documentPath), 'utf8');
      const variables = JSON.parse(readFileSync(resolve(repoRoot, expected.variablesPath), 'utf8')) as Record<
        string,
        unknown
      >;

      expect(spec).toEqual(
        expect.objectContaining({
          scenarioId: expected.id,
          scenarioStatus: 'captured',
          liveCaptureFiles: [expected.captureFile],
          comparisonMode: 'captured-vs-proxy-request',
          proxyRequest: {
            documentPath: expected.documentPath,
            variablesPath: expected.variablesPath,
          },
        }),
      );
      expect(document).toContain('draftOrderComplete');
      expect(document).toContain('draftOrder {');
      expect(document).not.toContain('\n            order {');
      expect(document).toContain('userErrors');

      if (expected.id === 'draft-order-complete-missing-id-invalid-variable') {
        expect(document).toContain(
          'mutation DraftOrderCompleteMissingIdParity($id: ID!, $paymentGatewayId: ID, $sourceName: String)',
        );
        expect(document).toContain(
          'draftOrderComplete(id: $id, paymentGatewayId: $paymentGatewayId, sourceName: $sourceName)',
        );
        expect(variables).toEqual({
          paymentGatewayId: null,
          sourceName: 'hermes-cron-orders',
        });
      } else {
        expect(document).toMatch(/mutation DraftOrderCompleteInline(Missing|Null)IdParity/);
        expect(variables).toEqual({});
      }
    }
  });

  it('keeps blocked order-creation parity specs executable with concrete request scaffolds and blocker metadata', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');

    for (const expected of expectedBlockedScenarios) {
      const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.paritySpecPath), 'utf8')) as ParitySpec;
      const document = readFileSync(resolve(repoRoot, expected.documentPath), 'utf8');
      const variables = JSON.parse(readFileSync(resolve(repoRoot, expected.variablesPath), 'utf8')) as Record<
        string,
        unknown
      >;

      expect(spec).toEqual(
        expect.objectContaining({
          scenarioId: expected.id,
          scenarioStatus: 'planned',
          liveCaptureFiles: [],
          comparisonMode: 'planned',
          proxyRequest: {
            documentPath: expected.documentPath,
            variablesPath: expected.variablesPath,
          },
          blocker: expect.objectContaining({
            kind: expected.blockerKind,
            blockerPath: 'pending/order-creation-conformance-scope-blocker.md',
            details: expect.objectContaining({
              requiredScopes: expected.requiredScopes,
              probeRoots: expected.probeRoots,
            }),
          }),
        }),
      );
      expect(document).toContain(expected.operationLine);
      expect(document).toContain('userErrors');
      expect(document).toContain('lineItems');
      expect(document).toContain('totalPriceSet');
      expect(spec.blocker?.details?.failingMessage).toContain(expected.failingMessageIncludes);
      expect(spec.blocker?.details?.manualStoreAuthStatus).toBe('present-shpca-user-token-not-offline-capable');
      expect(spec.blocker?.details?.manualStoreAuthTokenPath).toBe('.manual-store-auth-token.json');
      expect(spec.blocker?.details?.manualStoreAuthCachedScopes).toContain('write_orders');
      expect(spec.blocker?.details?.manualStoreAuthAssociatedUserScopes ?? []).toEqual([]);
      if (expected.requiredTokenMode) {
        expect(spec.blocker?.details?.requiredTokenMode).toBe(expected.requiredTokenMode);
      }
      if (expected.id === 'draft-order-complete-live-parity') {
        expect(document).toContain('draftOrderComplete');
        expect(document).toContain('draftOrder {');
        expect(document).not.toContain('\n      order {');
        expect(document).toContain('ready');
        expect(document).toContain('invoiceUrl');
        expect(document).toContain('totalPriceSet');
        expect(variables).toMatchObject({
          id: 'gid://shopify/DraftOrder/0',
          sourceName: 'hermes-cron-orders',
        });
      }
    }
  });

  it('keeps the live order capture harness aligned with the richer merchant-realistic order-creation parity scaffolds and validation slices', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const captureScript = readFileSync(resolve(repoRoot, 'scripts/capture-order-conformance.mts'), 'utf8');

    expect(captureScript).toContain('customAttributes');
    expect(captureScript).toContain('billingAddress');
    expect(captureScript).toContain('shippingAddress');
    expect(captureScript).toContain('shippingLines');
    expect(captureScript).toContain('shippingLine');
    expect(captureScript).toContain("key: 'channel'");
    expect(captureScript).toContain("value: 'cron-orders-bootstrap'");
    expect(captureScript).toContain("amount: '15.00'");
    expect(captureScript).toContain("title: 'Standard'");
    expect(captureScript).toContain('orderCreateInlineMissingOrderFixturePath');
    expect(captureScript).toContain('mutation InlineMissingOrderArg');
    expect(captureScript).toContain('orderCreateInlineMissingOrderResult');
    expect(captureScript).toContain('orderCreateInlineNullOrderFixturePath');
    expect(captureScript).toContain('mutation InlineNullOrderArg');
    expect(captureScript).toContain('orderCreateInlineNullOrderResult');
    expect(captureScript).toContain('orderCreateMissingOrderFixturePath');
    expect(captureScript).toContain('mutation OrderCreateMissingOrder');
    expect(captureScript).toContain('orderCreateMissingOrderResult');
    expect(captureScript).toContain('draftOrderCreateInlineMissingInputFixturePath');
    expect(captureScript).toContain('mutation InlineMissingDraftOrderInput');
    expect(captureScript).toContain('draftOrderCreateInlineMissingInputResult');
    expect(captureScript).toContain('draftOrderCreateInlineNullInputFixturePath');
    expect(captureScript).toContain('mutation InlineNullDraftOrderInput');
    expect(captureScript).toContain('draftOrderCreateInlineNullInputResult');
    expect(captureScript).toContain('draftOrderCreateMissingInputFixturePath');
    expect(captureScript).toContain('mutation DraftOrderCreateMissingInput');
    expect(captureScript).toContain('draftOrderCreateMissingInputResult');
    expect(captureScript).toContain('draftOrderCompleteMissingIdFixturePath');
    expect(captureScript).toContain('mutation DraftOrderCompleteMissingId');
    expect(captureScript).toContain('draftOrderCompleteMissingIdResult');
    expect(captureScript).not.toContain('draftOrderComplete(order:');
  });

  it('refreshes a shared blocker note with the current healthy creation state while preserving the remaining draft-order completion blocker', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const blockerNote = readFileSync(resolve(repoRoot, 'pending/order-creation-conformance-scope-blocker.md'), 'utf8');

    expect(blockerNote).toContain('orderCreate');
    expect(blockerNote).toContain('order-create-parity.json');
    expect(blockerNote).toContain('last verified happy-path fixture');
    expect(blockerNote).toContain('draftOrderCreate');
    expect(blockerNote).toContain('draft-order-create-parity.json');
    expect(blockerNote).toContain('draft-order-detail.json');
    expect(blockerNote).toContain('draftOrderComplete');
    expect(blockerNote).toContain('last verified family-specific access-denied evidence');
    expect(blockerNote).toContain('mark-as-paid');
    expect(blockerNote).toContain('.manual-store-auth-token.json');
    expect(blockerNote).toContain('corepack pnpm conformance:capture-orders');
    expect(blockerNote).toContain(
      'current run is auth-regressed before the family-specific creation roots can be reprobed',
    );
    expect(blockerNote).toContain(
      'remaining creation-family live blocker after auth is repaired is still `draftOrderComplete`',
    );
    expect(blockerNote).not.toContain('missing requiredScopes blocker metadata');
    expect(blockerNote).not.toContain(
      'still lacks any writable happy-path local runtime support for the whole creation family',
    );
  });

  it('documents the first safe orderCreate validation slice alongside the still-blocked happy path', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const weirdNotes = readFileSync(resolve(repoRoot, 'docs/hard-and-weird-notes.md'), 'utf8');

    expect(weirdNotes).toContain('orderCreate');
    expect(weirdNotes).toContain('missing required arguments: order');
    expect(weirdNotes).toContain("Argument 'order' on Field 'orderCreate' has an invalid value (null)");
    expect(weirdNotes).toContain('draftOrderCreate');
    expect(weirdNotes).toContain('draftOrderComplete');
    expect(weirdNotes).toContain('missing required `$order` variable');
    expect(weirdNotes).toContain('missing required `$input` variable');
    expect(weirdNotes).toContain('missing required `$id` variable');
    expect(weirdNotes).toContain("Field 'order' doesn't exist on type 'DraftOrderCompletePayload'");
    expect(weirdNotes).toContain('INVALID_VARIABLE');
  });
});
