/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { readFile, mkdir, writeFile } from 'node:fs/promises';
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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const outputPath = path.join(outputDir, 'webhook-subscription-conformance.json');

const requestDir = path.join('config', 'parity-requests');
const catalogRequestPath = path.join(requestDir, 'webhook-subscription-catalog-read.graphql');
const catalogVariablesPath = path.join(requestDir, 'webhook-subscription-catalog-read.variables.json');
const detailRequestPath = path.join(requestDir, 'webhook-subscription-detail-read.graphql');
const createRequestPath = path.join(requestDir, 'webhookSubscriptionCreate-parity.graphql');
const createVariablesPath = path.join(requestDir, 'webhookSubscriptionCreate-parity.variables.json');
const updateRequestPath = path.join(requestDir, 'webhookSubscriptionUpdate-parity.graphql');
const updateVariablesPath = path.join(requestDir, 'webhookSubscriptionUpdate-parity.variables.json');
const deleteRequestPath = path.join(requestDir, 'webhookSubscriptionDelete-parity.graphql');
const validationRequestPath = path.join(requestDir, 'webhook-subscription-validation-branches.graphql');
const validationVariablesPath = path.join(requestDir, 'webhook-subscription-validation-branches.variables.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readText(relativePath: string): Promise<string> {
  return readFile(path.join(process.cwd(), relativePath), 'utf8');
}

async function readVariables(relativePath: string): Promise<Record<string, unknown>> {
  return JSON.parse(await readText(relativePath)) as Record<string, unknown>;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function requireSuccessfulGraphql(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result.payload)}`);
  }
}

async function capture(documentPath: string, variables: Record<string, unknown>): Promise<CapturedRequest> {
  const document = await readText(documentPath);
  const response = await runGraphqlRequest(document, variables);
  requireSuccessfulGraphql(response, documentPath);

  return {
    documentPath,
    variables,
    response,
  };
}

function readCreatedWebhookId(createCapture: CapturedRequest): string | null {
  const data = createCapture.response.payload.data;
  if (!isObject(data)) {
    return null;
  }

  const payload = data['webhookSubscriptionCreate'];
  if (!isObject(payload)) {
    return null;
  }

  const webhookSubscription = payload['webhookSubscription'];
  if (!isObject(webhookSubscription)) {
    return null;
  }

  const id = webhookSubscription['id'];
  return typeof id === 'string' ? id : null;
}

function withWebhookUri(
  rawVariables: Record<string, unknown>,
  uri: string,
  overrides: Record<string, unknown> = {},
): Record<string, unknown> {
  const input = isObject(rawVariables['webhookSubscription']) ? rawVariables['webhookSubscription'] : {};

  return {
    ...rawVariables,
    webhookSubscription: {
      ...input,
      ...overrides,
      uri,
    },
  };
}

const schemaAndAccessQuery = `#graphql
  query WebhookSubscriptionSchemaAndAccess {
    currentAppInstallation {
      accessScopes {
        handle
      }
    }
    webhookSubscriptionInput: __type(name: "WebhookSubscriptionInput") {
      inputFields {
        name
        type {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
            }
          }
        }
      }
    }
    webhookSubscriptionTopic: __type(name: "WebhookSubscriptionTopic") {
      enumValues {
        name
        isDeprecated
        deprecationReason
      }
    }
    webhookSubscriptionEndpoint: __type(name: "WebhookSubscriptionEndpoint") {
      possibleTypes {
        name
      }
    }
  }
`;

const schemaAndAccess = await runGraphqlRequest(schemaAndAccessQuery);
requireSuccessfulGraphql(schemaAndAccess, 'Webhook subscription schema/access probe');

const suffix = `${Date.now()}`;
const createUri = `https://example.com/hermes-webhook-conformance-${suffix}`;
const updateUri = `${createUri}-updated`;
const unknownId = 'gid://shopify/WebhookSubscription/999999999999';

const catalogVariables = await readVariables(catalogVariablesPath);
const createVariables = withWebhookUri(await readVariables(createVariablesPath), createUri);
const updateVariablesTemplate = await readVariables(updateVariablesPath);
const validationVariables = await readVariables(validationVariablesPath);

let createdId: string | null = null;
let cleanup: CapturedRequest | null = null;
const lifecycle: {
  create: CapturedRequest | null;
  detailAfterCreate: CapturedRequest | null;
  update: CapturedRequest | null;
  detailAfterUpdate: CapturedRequest | null;
  delete: CapturedRequest | null;
  postDeleteDetail: CapturedRequest | null;
} = {
  create: null,
  detailAfterCreate: null,
  update: null,
  detailAfterUpdate: null,
  delete: null,
  postDeleteDetail: null,
};

try {
  lifecycle.create = await capture(createRequestPath, createVariables);
  createdId = readCreatedWebhookId(lifecycle.create);
  if (createdId === null) {
    throw new Error('webhookSubscriptionCreate did not return a webhookSubscription.id.');
  }

  lifecycle.detailAfterCreate = await capture(detailRequestPath, { id: createdId });
  lifecycle.update = await capture(
    updateRequestPath,
    withWebhookUri({ ...updateVariablesTemplate, id: createdId }, updateUri, {
      includeFields: ['id'],
      metafieldNamespaces: [],
    }),
  );
  lifecycle.detailAfterUpdate = await capture(detailRequestPath, { id: createdId });
  lifecycle.delete = await capture(deleteRequestPath, { id: createdId });
  lifecycle.postDeleteDetail = await capture(detailRequestPath, { id: createdId });
} finally {
  if (createdId !== null && lifecycle.delete === null) {
    cleanup = await capture(deleteRequestPath, { id: createdId });
  }
}

const catalog = await capture(catalogRequestPath, {
  ...catalogVariables,
  unknownId,
});
const validation = await capture(validationRequestPath, validationVariables);

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'HAR-267 captures Admin GraphQL API-created webhook subscription roots only.',
        'The script registers, updates, and deletes a temporary SHOP_UPDATE HTTP subscription and does not mutate shop/resource data to trigger webhook delivery.',
        'App TOML webhooks are out of scope for this evidence because webhookSubscriptions only lists API-created subscriptions.',
      ],
      deliveryPolicy: {
        deliveriesTriggeredByScript: false,
        topicUsedForLifecycle: 'SHOP_UPDATE',
        endpointHost: 'example.com',
      },
      schemaAndAccess: {
        request: {
          description:
            'Documents accessible input fields, endpoint union variants, topic enum values, and active app scopes.',
        },
        response: schemaAndAccess,
      },
      catalog,
      lifecycle,
      validation,
      cleanup,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote webhook subscription conformance fixture to ${outputPath}`);
