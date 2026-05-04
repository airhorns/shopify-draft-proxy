import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { Kind, parse } from 'graphql';
import { describe, expect, it } from 'vitest';

import { classifyParityScenarioState, type ParitySpec } from '../../scripts/conformance-parity-spec.js';
import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';
import { parseOperation } from '../../src/graphql/parse-operation.js';
import { getOperationCapability } from '../../src/proxy/capabilities.js';
import { findOperationRegistryEntry } from '../../src/proxy/operation-registry.js';

const repoRoot = resolve(import.meta.dirname, '../..');
const fixturePath =
  'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-conformance.json';
const specPath = 'config/parity-specs/webhooks/webhook-subscription-conformance.json';
const requiredArgumentSpecPath = 'config/parity-specs/webhooks/webhook-subscription-required-argument-validation.json';

function readText(relativePath: string): string {
  return readFileSync(resolve(repoRoot, relativePath), 'utf8');
}

function readJson<T>(relativePath: string): T {
  return JSON.parse(readText(relativePath)) as T;
}

function asRecord(value: unknown): Record<string, unknown> {
  expect(value).toEqual(expect.any(Object));
  expect(Array.isArray(value)).toBe(false);
  return value as Record<string, unknown>;
}

function rootFieldNames(relativePath: string): string[] {
  const document = parse(readText(relativePath));
  const operation = document.definitions.find((definition) => definition.kind === Kind.OPERATION_DEFINITION);
  expect(operation?.kind).toBe(Kind.OPERATION_DEFINITION);

  return (
    operation?.selectionSet.selections
      .filter((selection) => selection.kind === Kind.FIELD)
      .map((selection) => selection.name.value) ?? []
  );
}

describe('webhook subscription conformance fixture', () => {
  it('discovers executable parity evidence while routing supported create staging locally', () => {
    const scenarios = loadConformanceScenarios(repoRoot);
    const scenario = scenarios.find((candidate) => candidate.id === 'webhook-subscription-conformance');
    const paritySpec = readJson<ParitySpec>(specPath);
    const requiredArgumentScenario = scenarios.find(
      (candidate) => candidate.id === 'webhook-subscription-required-argument-validation',
    );
    const requiredArgumentSpec = readJson<ParitySpec>(requiredArgumentSpecPath);

    expect(scenario).toMatchObject({
      status: 'captured',
      operationNames: [
        'webhookSubscription',
        'webhookSubscriptions',
        'webhookSubscriptionsCount',
        'webhookSubscriptionCreate',
        'webhookSubscriptionUpdate',
        'webhookSubscriptionDelete',
      ],
      captureFiles: [fixturePath],
    });
    expect(classifyParityScenarioState(scenario!, paritySpec)).toBe('ready-for-comparison');
    expect(paritySpec.comparison?.targets?.length).toBeGreaterThan(0);
    expect(requiredArgumentScenario).toMatchObject({
      status: 'captured',
      operationNames: ['webhookSubscriptionCreate', 'webhookSubscriptionUpdate'],
      captureFiles: [fixturePath],
    });
    expect(classifyParityScenarioState(requiredArgumentScenario!, requiredArgumentSpec)).toBe('ready-for-comparison');

    const parsedCreate = parseOperation(
      readText('config/parity-requests/webhooks/webhookSubscriptionCreate-parity.graphql'),
    );
    const parsedCatalog = parseOperation(
      readText('config/parity-requests/webhooks/webhook-subscription-catalog-read.graphql'),
    );
    expect(getOperationCapability(parsedCatalog)).toMatchObject({
      domain: 'webhooks',
      execution: 'overlay-read',
      operationName: 'webhookSubscriptions',
    });
    expect(findOperationRegistryEntry('query', ['webhookSubscriptions'])).toMatchObject({
      domain: 'webhooks',
      implemented: true,
    });
    expect(getOperationCapability(parsedCreate)).toMatchObject({
      domain: 'webhooks',
      execution: 'stage-locally',
      operationName: 'webhookSubscriptionCreate',
    });
    expect(findOperationRegistryEntry('mutation', ['webhookSubscriptionCreate'])).toMatchObject({
      domain: 'webhooks',
      implemented: true,
    });
  });

  it('keeps request files aligned with the captured root-operation surface', () => {
    expect(readText('scripts/capture-webhook-subscription-conformance.ts')).toContain(
      "path.join('config', 'parity-requests', 'webhooks')",
    );
    expect(rootFieldNames('config/parity-requests/webhooks/webhook-subscription-catalog-read.graphql')).toEqual([
      'webhookSubscriptions',
      'webhookSubscriptionsCount',
      'webhookSubscriptionsCount',
      'webhookSubscription',
    ]);
    expect(rootFieldNames('config/parity-requests/webhooks/webhook-subscription-detail-read.graphql')).toEqual([
      'webhookSubscription',
    ]);
    expect(rootFieldNames('config/parity-requests/webhooks/webhookSubscriptionCreate-parity.graphql')).toEqual([
      'webhookSubscriptionCreate',
    ]);
    expect(rootFieldNames('config/parity-requests/webhooks/webhookSubscriptionUpdate-parity.graphql')).toEqual([
      'webhookSubscriptionUpdate',
    ]);
    expect(rootFieldNames('config/parity-requests/webhooks/webhookSubscriptionDelete-parity.graphql')).toEqual([
      'webhookSubscriptionDelete',
    ]);
    expect(rootFieldNames('config/parity-requests/webhooks/webhook-subscription-validation-branches.graphql')).toEqual([
      'webhookSubscriptionUpdate',
      'webhookSubscriptionDelete',
      'webhookSubscriptionCreate',
    ]);
    expect(rootFieldNames('config/parity-requests/webhooks/webhook-subscription-missing-create-topic.graphql')).toEqual(
      ['webhookSubscriptionCreate'],
    );
    expect(rootFieldNames('config/parity-requests/webhooks/webhook-subscription-null-update-input.graphql')).toEqual([
      'webhookSubscriptionUpdate',
    ]);
  });

  it('records safe live read, lifecycle, and validation evidence', () => {
    const fixture = readJson<Record<string, unknown>>(fixturePath);
    expect(fixture['apiVersion']).toBe('2026-04');
    expect(fixture['deliveryPolicy']).toMatchObject({
      deliveriesTriggeredByScript: false,
      topicUsedForLifecycle: 'SHOP_UPDATE',
    });

    const schemaData = asRecord(asRecord(asRecord(fixture['schemaAndAccess'])['response'])['payload'])['data'];
    const schemaRecord = asRecord(schemaData);
    expect(asRecord(schemaRecord['webhookSubscriptionInput'])['inputFields']).toEqual(
      expect.arrayContaining([expect.objectContaining({ name: 'uri' })]),
    );
    expect(asRecord(schemaRecord['webhookSubscriptionEndpoint'])['possibleTypes']).toEqual(
      expect.arrayContaining([expect.objectContaining({ name: 'WebhookHttpEndpoint' })]),
    );

    const catalogData = asRecord(asRecord(asRecord(fixture['catalog'])['response'])['payload'])['data'];
    expect(catalogData).toMatchObject({
      webhookSubscriptions: {
        nodes: [],
        edges: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
      webhookSubscriptionsCount: {
        count: 0,
        precision: 'EXACT',
      },
      filteredCount: {
        count: 0,
        precision: 'EXACT',
      },
      unknown: null,
    });

    const lifecycle = asRecord(fixture['lifecycle']);
    const createData = asRecord(asRecord(asRecord(lifecycle['create'])['response'])['payload'])['data'];
    const createPayload = asRecord(asRecord(createData)['webhookSubscriptionCreate']);
    const createdWebhook = asRecord(createPayload['webhookSubscription']);
    const createdId = createdWebhook['id'];
    expect(typeof createdId).toBe('string');
    expect(createPayload['userErrors']).toEqual([]);
    expect(createdWebhook).toMatchObject({
      topic: 'SHOP_UPDATE',
      format: 'JSON',
      includeFields: ['id', 'name'],
      metafieldNamespaces: ['custom'],
      endpoint: {
        __typename: 'WebhookHttpEndpoint',
      },
    });

    const updateData = asRecord(asRecord(asRecord(lifecycle['update'])['response'])['payload'])['data'];
    const updatePayload = asRecord(asRecord(updateData)['webhookSubscriptionUpdate']);
    expect(updatePayload['userErrors']).toEqual([]);
    expect(asRecord(updatePayload['webhookSubscription'])).toMatchObject({
      id: createdId,
      includeFields: ['id'],
      metafieldNamespaces: [],
    });

    const deleteData = asRecord(asRecord(asRecord(lifecycle['delete'])['response'])['payload'])['data'];
    expect(deleteData).toMatchObject({
      webhookSubscriptionDelete: {
        deletedWebhookSubscriptionId: createdId,
        userErrors: [],
      },
    });
    const postDeleteData = asRecord(asRecord(asRecord(lifecycle['postDeleteDetail'])['response'])['payload'])['data'];
    expect(postDeleteData).toEqual({ webhookSubscription: null });

    const validationData = asRecord(asRecord(asRecord(fixture['validation'])['response'])['payload'])['data'];
    expect(validationData).toMatchObject({
      updateUnknown: {
        webhookSubscription: null,
        userErrors: [{ field: ['id'], message: 'Webhook subscription does not exist' }],
      },
      deleteUnknown: {
        deletedWebhookSubscriptionId: null,
        userErrors: [{ field: ['id'], message: 'Webhook subscription does not exist' }],
      },
      createMissingUri: {
        webhookSubscription: null,
        userErrors: [{ field: ['webhookSubscription', 'callbackUrl'], message: "Address can't be blank" }],
      },
    });

    const graphqlValidation = asRecord(fixture['graphqlValidation']);
    const missingCreateTopicErrors = asRecord(
      asRecord(asRecord(graphqlValidation['missingCreateTopic'])['response'])['payload'],
    )['errors'];
    expect(missingCreateTopicErrors).toEqual([
      {
        message: "Field 'webhookSubscriptionCreate' is missing required arguments: topic",
        locations: [{ line: 2, column: 3 }],
        path: ['mutation MissingCreateWebhookTopic', 'webhookSubscriptionCreate'],
        extensions: {
          code: 'missingRequiredArguments',
          className: 'Field',
          name: 'webhookSubscriptionCreate',
          arguments: 'topic',
        },
      },
    ]);

    const nullUpdateInputErrors = asRecord(
      asRecord(asRecord(graphqlValidation['nullUpdateInput'])['response'])['payload'],
    )['errors'];
    expect(nullUpdateInputErrors).toEqual([
      {
        message:
          "Argument 'webhookSubscription' on Field 'webhookSubscriptionUpdate' has an invalid value (null). Expected type 'WebhookSubscriptionInput!'.",
        locations: [{ line: 2, column: 3 }],
        path: ['mutation NullUpdateWebhookInput', 'webhookSubscriptionUpdate', 'webhookSubscription'],
        extensions: {
          code: 'argumentLiteralsIncompatible',
          typeName: 'Field',
          argumentName: 'webhookSubscription',
        },
      },
    ]);
  });
});
