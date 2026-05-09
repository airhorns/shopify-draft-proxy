/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedRequest = {
  documentPath: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'webhooks');
const outputPath = path.join(outputDir, 'webhook-subscription-pub-sub-required-fields.json');
const specPath = path.join('config', 'parity-specs', 'webhooks', 'webhook-subscription-pub-sub-required-fields.json');

const requestDir = path.join('config', 'parity-requests', 'webhooks');
const createMissingProjectRequestPath = path.join(
  requestDir,
  'pubSubWebhookSubscriptionCreate-missing-project.graphql',
);
const createMissingTopicRequestPath = path.join(requestDir, 'pubSubWebhookSubscriptionCreate-missing-topic.graphql');
const createMissingBothRequestPath = path.join(
  requestDir,
  'pubSubWebhookSubscriptionCreate-missing-project-topic.graphql',
);
const updateMissingProjectRequestPath = path.join(
  requestDir,
  'pubSubWebhookSubscriptionUpdate-missing-project.graphql',
);
const updateMissingTopicRequestPath = path.join(requestDir, 'pubSubWebhookSubscriptionUpdate-missing-topic.graphql');
const updateMissingBothRequestPath = path.join(
  requestDir,
  'pubSubWebhookSubscriptionUpdate-missing-project-topic.graphql',
);

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readText(relativePath: string): Promise<string> {
  return readFile(path.join(process.cwd(), relativePath), 'utf8');
}

function requireTopLevelErrors(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || !Array.isArray(result.payload.errors)) {
    throw new Error(`${label} did not return top-level GraphQL errors: ${JSON.stringify(result.payload)}`);
  }
  if ('data' in result.payload) {
    throw new Error(`${label} unexpectedly returned data: ${JSON.stringify(result.payload)}`);
  }
}

async function capture(documentPath: string, variables: Record<string, unknown>): Promise<CapturedRequest> {
  const document = await readText(documentPath);
  const response = await runGraphqlRequest(document, variables);
  requireTopLevelErrors(response, documentPath);

  return { documentPath, variables, response };
}

function createVariables(webhookSubscription: Record<string, unknown>): Record<string, unknown> {
  return {
    topic: 'SHOP_UPDATE',
    webhookSubscription,
  };
}

function updateVariables(webhookSubscription: Record<string, unknown>): Record<string, unknown> {
  return {
    id: 'gid://shopify/WebhookSubscription/1',
    webhookSubscription,
  };
}

const validation = {
  createMissingProject: await capture(
    createMissingProjectRequestPath,
    createVariables({
      pubSubTopic: 'topic-1',
    }),
  ),
  createMissingTopic: await capture(
    createMissingTopicRequestPath,
    createVariables({
      pubSubProject: 'valid-project',
    }),
  ),
  createMissingBoth: await capture(createMissingBothRequestPath, createVariables({})),
  updateMissingProject: await capture(
    updateMissingProjectRequestPath,
    updateVariables({
      pubSubTopic: 'topic-1',
    }),
  ),
  updateMissingTopic: await capture(
    updateMissingTopicRequestPath,
    updateVariables({
      pubSubProject: 'valid-project',
    }),
  ),
  updateMissingBoth: await capture(updateMissingBothRequestPath, updateVariables({})),
};

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'Captures PubSubWebhookSubscriptionInput required-field coercion errors for the deprecated dedicated Pub/Sub create/update roots.',
        'All cases fail in GraphQL variable validation before the resolver runs, so no webhook subscription is created, updated, deleted, or delivered.',
      ],
      validation,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);
await writeFile(specPath, `${JSON.stringify(buildSpec(), null, 2)}\n`, 'utf8');

console.log(`Wrote Pub/Sub webhook required-fields fixture to ${outputPath}`);
console.log(`Wrote Pub/Sub webhook required-fields parity spec to ${specPath}`);

function buildSpec(): Record<string, unknown> {
  return {
    scenarioId: 'webhook-subscription-pub-sub-required-fields',
    operationNames: ['pubSubWebhookSubscriptionCreate', 'pubSubWebhookSubscriptionUpdate'],
    scenarioStatus: 'captured',
    assertionKinds: ['graphql-validation-parity', 'no-local-staging-on-validation-error'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['test/parity_test.gleam', 'test/shopify_draft_proxy/proxy/draft_proxy_test.gleam'],
    proxyRequest: {
      documentPath: createMissingProjectRequestPath,
      variablesCapturePath: '$.validation.createMissingProject.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Strict parity for PubSubWebhookSubscriptionInput required pubSubProject/pubSubTopic validation on the dedicated Pub/Sub create/update roots. These branches return top-level GraphQL errors before resolver side effects.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        validationTarget(
          'create-missing-project-errors',
          '$.validation.createMissingProject.response.payload.errors',
          '$.validation.createMissingProject.variables',
          createMissingProjectRequestPath,
        ),
        validationTarget(
          'create-missing-topic-errors',
          '$.validation.createMissingTopic.response.payload.errors',
          '$.validation.createMissingTopic.variables',
          createMissingTopicRequestPath,
        ),
        validationTarget(
          'create-missing-both-errors',
          '$.validation.createMissingBoth.response.payload.errors',
          '$.validation.createMissingBoth.variables',
          createMissingBothRequestPath,
        ),
        validationTarget(
          'update-missing-project-errors',
          '$.validation.updateMissingProject.response.payload.errors',
          '$.validation.updateMissingProject.variables',
          updateMissingProjectRequestPath,
        ),
        validationTarget(
          'update-missing-topic-errors',
          '$.validation.updateMissingTopic.response.payload.errors',
          '$.validation.updateMissingTopic.variables',
          updateMissingTopicRequestPath,
        ),
        validationTarget(
          'update-missing-both-errors',
          '$.validation.updateMissingBoth.response.payload.errors',
          '$.validation.updateMissingBoth.variables',
          updateMissingBothRequestPath,
        ),
      ],
    },
  };
}

function validationTarget(
  name: string,
  capturePath: string,
  variablesCapturePath: string,
  documentPath: string,
): Record<string, unknown> {
  return {
    name,
    capturePath,
    proxyPath: '$.errors',
    upstreamCapturePath: null,
    proxyRequest: {
      documentPath,
      variablesCapturePath,
      apiVersion,
    },
  };
}
