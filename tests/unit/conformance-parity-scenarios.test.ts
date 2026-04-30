import { readFileSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  classifyParityScenarioState,
  executeParityScenario,
  type ExecutedOperation,
  type ParitySpec,
  validateParityScenarioOperationNames,
  validateParityScenarioInventoryEntry,
} from '../../scripts/conformance-parity-lib.js';
import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';
import { parseOperation } from '../../src/graphql/parse-operation.js';

const repoRoot = path.resolve(import.meta.dirname, '../..');
const gleamOwnedScenarioIds = new Set([
  'app-billing-access-local-staging',
  'delegate-access-token-current-input-local-staging',
]);

function readParitySpec(relativePath: string): ParitySpec {
  return JSON.parse(readFileSync(path.join(repoRoot, relativePath), 'utf8')) as ParitySpec;
}

function readDocumentOperation(relativePath: string): ExecutedOperation {
  const parsed = parseOperation(readFileSync(path.join(repoRoot, relativePath), 'utf8'));
  return {
    type: parsed.type,
    name: parsed.name,
    rootFields: parsed.rootFields,
  };
}

function readDocumentOperationsFromSpec(paritySpec: ParitySpec): ExecutedOperation[] {
  const documentPaths = new Set<string>();
  if (paritySpec.proxyRequest?.documentPath) {
    documentPaths.add(paritySpec.proxyRequest.documentPath);
  }
  for (const target of paritySpec.comparison?.targets ?? []) {
    if (target.proxyRequest?.documentPath) {
      documentPaths.add(target.proxyRequest.documentPath);
    }
  }
  return [...documentPaths].map(readDocumentOperation);
}

const discoveredScenarios = loadConformanceScenarios(repoRoot).map((scenario) => ({
  ...scenario,
  paritySpec: readParitySpec(scenario.paritySpecPath),
}));

const readyScenarios = discoveredScenarios.filter(
  (scenario) => classifyParityScenarioState(scenario, scenario.paritySpec) === 'ready-for-comparison',
);
const typescriptReadyScenarios = readyScenarios.filter((scenario) => !gleamOwnedScenarioIds.has(scenario.id));

describe('conformance parity scenarios (convention-driven suite)', () => {
  it('rejects checked-in captured scenarios without executable enforcement', () => {
    const errors = discoveredScenarios.flatMap((scenario) =>
      validateParityScenarioInventoryEntry(scenario, scenario.paritySpec),
    );

    expect(errors).toEqual([]);
  });

  it('discovers at least one ready-for-comparison scenario by convention', () => {
    expect(readyScenarios.length).toBeGreaterThan(0);
  });

  it('validates declared mutation operations against executed mutation roots', () => {
    const validation = validateParityScenarioOperationNames({
      scenario: {
        id: 'operation-name-validation-example',
        status: 'captured',
        operationNames: ['fileCreate', 'fileUpdate', 'files'],
      },
      paritySpec: {},
      executedOperations: [
        {
          type: 'mutation',
          name: 'CreateFile',
          rootFields: ['fileCreate'],
        },
        {
          type: 'query',
          name: 'ReadFiles',
          rootFields: ['files'],
        },
        {
          type: 'mutation',
          name: 'StageUpload',
          rootFields: ['stagedUploadsCreate'],
        },
      ],
    });

    expect(validation.declaredMutationOperationNames).toEqual(['fileCreate', 'fileUpdate']);
    expect(validation.actualMutationOperationNames).toEqual(['fileCreate', 'stagedUploadsCreate']);
    expect(validation.missingMutationOperationNames).toEqual(['fileUpdate']);
    expect(validation.unexpectedMutationOperationNames).toEqual(['stagedUploadsCreate']);
    expect(validation.errors).toEqual([
      'Scenario operation-name-validation-example declares mutation operation(s) fileUpdate in operationNames but did not execute them. Actual executed mutation operation(s): fileCreate, stagedUploadsCreate.',
      'Scenario operation-name-validation-example executed mutation operation(s) stagedUploadsCreate but does not declare them in operationNames. Declared mutation operation(s): fileCreate, fileUpdate.',
    ]);
  });

  it('does not let runtime test files satisfy ready parity mutation execution claims', () => {
    const validation = validateParityScenarioOperationNames({
      scenario: {
        id: 'operation-name-runtime-test-gap-example',
        status: 'captured',
        operationNames: ['appPurchaseOneTimeCreate', 'appSubscriptionCancel'],
      },
      paritySpec: {},
      executedOperations: [
        {
          type: 'mutation',
          name: 'CancelSubscription',
          rootFields: ['appSubscriptionCancel'],
        },
      ],
    });

    expect(validation.missingMutationOperationNames).toEqual(['appPurchaseOneTimeCreate']);
    expect(validation.errors).toEqual([
      'Scenario operation-name-runtime-test-gap-example declares mutation operation(s) appPurchaseOneTimeCreate in operationNames but did not execute them. Actual executed mutation operation(s): appSubscriptionCancel.',
    ]);
  });

  it('keeps the app billing ready parity spec scoped to executed mutation roots', async () => {
    const scenario = readyScenarios.find((candidate) => candidate.id === 'app-billing-access-local-staging');
    expect(scenario).toBeDefined();

    const operationNameValidation = validateParityScenarioOperationNames({
      scenario: scenario!,
      paritySpec: scenario!.paritySpec,
      executedOperations: readDocumentOperationsFromSpec(scenario!.paritySpec),
    });

    expect(operationNameValidation.declaredMutationOperationNames).toEqual([
      'appPurchaseOneTimeCreate',
      'appRevokeAccessScopes',
      'appSubscriptionCancel',
      'appSubscriptionCreate',
      'appSubscriptionLineItemUpdate',
      'appSubscriptionTrialExtend',
      'appUninstall',
      'appUsageRecordCreate',
      'delegateAccessTokenCreate',
      'delegateAccessTokenDestroy',
    ]);
    expect(operationNameValidation.actualMutationOperationNames).toEqual([
      'appPurchaseOneTimeCreate',
      'appRevokeAccessScopes',
      'appSubscriptionCancel',
      'appSubscriptionCreate',
      'appSubscriptionLineItemUpdate',
      'appSubscriptionTrialExtend',
      'appUninstall',
      'appUsageRecordCreate',
      'delegateAccessTokenCreate',
      'delegateAccessTokenDestroy',
    ]);
    expect(operationNameValidation.errors).toEqual([]);
  });

  it.each(typescriptReadyScenarios.map((scenario) => [scenario.id, scenario] as const))(
    'executes ready-for-comparison scenario %s against the local proxy harness',
    async (_id, scenario) => {
      const result = await executeParityScenario({
        repoRoot,
        scenario,
        paritySpec: scenario.paritySpec,
      });

      expect(result.primaryProxyStatus).toBe(200);
      expect(
        result.comparisons.filter((comparison) => !comparison.ok),
        `scenario ${scenario.id} had failing comparisons`,
      ).toEqual([]);
      expect(
        result.operationNameValidation.errors,
        `scenario ${scenario.id} declared mutation operationNames that did not match executed mutation roots`,
      ).toEqual([]);
      expect(result.ok).toBe(true);
    },
  );
});
