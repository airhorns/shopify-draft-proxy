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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'webhooks');
const outputPath = path.join(outputDir, 'webhook-subscription-conformance.json');

const requestDir = path.join('config', 'parity-requests', 'webhooks');
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
const missingCreateTopicRequestPath = path.join(requestDir, 'webhook-subscription-missing-create-topic.graphql');
const nullUpdateInputRequestPath = path.join(requestDir, 'webhook-subscription-null-update-input.graphql');

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

async function captureDocument(
  documentPath: string,
  document: string,
  variables: Record<string, unknown>,
): Promise<CapturedRequest> {
  const response = await runGraphqlRequest(document, variables);
  requireSuccessfulGraphql(response, documentPath);

  return {
    documentPath,
    variables,
    response,
  };
}

async function capture(documentPath: string, variables: Record<string, unknown>): Promise<CapturedRequest> {
  return captureDocument(documentPath, await readText(documentPath), variables);
}

async function captureGraphqlValidation(documentPath: string): Promise<CapturedRequest> {
  const document = await readText(documentPath);
  const response = await runGraphqlRequest(document, {});

  if (response.status < 200 || response.status >= 300 || !response.payload.errors) {
    throw new Error(`${documentPath} did not capture GraphQL validation errors: ${JSON.stringify(response.payload)}`);
  }

  return {
    documentPath,
    variables: {},
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

function readCreatedCustomerId(createCapture: CapturedRequest): string | null {
  const data = createCapture.response.payload.data;
  if (!isObject(data)) {
    return null;
  }

  const payload = data['customerCreate'];
  if (!isObject(payload)) {
    return null;
  }

  const customer = payload['customer'];
  if (!isObject(customer)) {
    return null;
  }

  const id = customer['id'];
  return typeof id === 'string' ? id : null;
}

function gidNumericTail(id: string): string {
  const tail = id.split('/').at(-1);
  return typeof tail === 'string' ? tail : id;
}

function withWebhookUri(
  rawVariables: Record<string, unknown>,
  uri: string,
  overrides: Record<string, unknown> = {},
  omitInputKeys: string[] = [],
): Record<string, unknown> {
  const input = isObject(rawVariables['webhookSubscription']) ? rawVariables['webhookSubscription'] : {};
  const webhookSubscription: Record<string, unknown> = {
    ...input,
    ...overrides,
    uri,
  };

  for (const key of omitInputKeys) {
    delete webhookSubscription[key];
  }

  return {
    ...rawVariables,
    webhookSubscription,
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

const filterValidationCustomerCreate = `#graphql
  mutation WebhookFilterValidationCustomerCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        id
        email
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const filterValidationCustomerDelete = `#graphql
  mutation WebhookFilterValidationCustomerDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

const schemaAndAccess = await runGraphqlRequest(schemaAndAccessQuery);
requireSuccessfulGraphql(schemaAndAccess, 'Webhook subscription schema/access probe');

const suffix = `${Date.now()}`;
const createUri = `https://example.com/hermes-webhook-conformance-${suffix}`;
const updateUri = `${createUri}-updated`;
const filterDefaultUri = `${createUri}-filter-default`;
const filterDefaultEmptyUri = `${createUri}-filter-empty`;
const filterValidationValidUri = `${createUri}-filter-validation-valid`;
const filterValidationUpdateUri = `${createUri}-filter-validation-update`;
const filterValidationInvalidUri = `${createUri}-filter-validation-invalid`;
const unknownId = 'gid://shopify/WebhookSubscription/999999999999';
const invalidFilter = 'totally bogus syntax';

const catalogVariables = await readVariables(catalogVariablesPath);
const createVariables = withWebhookUri(await readVariables(createVariablesPath), createUri);
const updateVariablesTemplate = await readVariables(updateVariablesPath);
const validationVariables = await readVariables(validationVariablesPath);

let createdId: string | null = null;
let cleanup: CapturedRequest | null = null;
let updateValidationCreatedId: string | null = null;
let filterDefaultWithoutId: string | null = null;
let filterDefaultEmptyId: string | null = null;
let filterValidationValidId: string | null = null;
let filterValidationUpdateId: string | null = null;
let filterValidationCustomerId: string | null = null;
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
const filterDefault: {
  createWithoutFilter: CapturedRequest | null;
  detailWithoutFilter: CapturedRequest | null;
  deleteWithoutFilter: CapturedRequest | null;
  createWithEmptyFilter: CapturedRequest | null;
  deleteWithEmptyFilter: CapturedRequest | null;
  cleanupWithoutFilter: CapturedRequest | null;
  cleanupWithEmptyFilter: CapturedRequest | null;
} = {
  createWithoutFilter: null,
  detailWithoutFilter: null,
  deleteWithoutFilter: null,
  createWithEmptyFilter: null,
  deleteWithEmptyFilter: null,
  cleanupWithoutFilter: null,
  cleanupWithEmptyFilter: null,
};
const filterValidation: {
  setupCustomer: CapturedRequest | null;
  createValidFilter: CapturedRequest | null;
  detailValidFilter: CapturedRequest | null;
  deleteValidFilter: CapturedRequest | null;
  createUpdateBase: CapturedRequest | null;
  updateValidFilter: CapturedRequest | null;
  detailAfterValidUpdate: CapturedRequest | null;
  updateInvalidFilter: CapturedRequest | null;
  deleteUpdateBase: CapturedRequest | null;
  createInvalidFilter: CapturedRequest | null;
  cleanupValidFilter: CapturedRequest | null;
  cleanupUpdateBase: CapturedRequest | null;
  cleanupCustomer: CapturedRequest | null;
} = {
  setupCustomer: null,
  createValidFilter: null,
  detailValidFilter: null,
  deleteValidFilter: null,
  createUpdateBase: null,
  updateValidFilter: null,
  detailAfterValidUpdate: null,
  updateInvalidFilter: null,
  deleteUpdateBase: null,
  createInvalidFilter: null,
  cleanupValidFilter: null,
  cleanupUpdateBase: null,
  cleanupCustomer: null,
};
const updateValidation: {
  create: CapturedRequest | null;
  updateBlankUri: CapturedRequest | null;
  updateHttpUri: CapturedRequest | null;
  updateInvalidPubSub: CapturedRequest | null;
  detailAfterFailedUpdates: CapturedRequest | null;
  delete: CapturedRequest | null;
  cleanup: CapturedRequest | null;
} = {
  create: null,
  updateBlankUri: null,
  updateHttpUri: null,
  updateInvalidPubSub: null,
  detailAfterFailedUpdates: null,
  delete: null,
  cleanup: null,
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

try {
  filterDefault.createWithoutFilter = await capture(
    createRequestPath,
    withWebhookUri(createVariables, filterDefaultUri, {}, ['filter']),
  );
  filterDefaultWithoutId = readCreatedWebhookId(filterDefault.createWithoutFilter);
  if (filterDefaultWithoutId === null) {
    throw new Error('filter-default omitted-filter create did not return a webhookSubscription.id.');
  }

  filterDefault.detailWithoutFilter = await capture(detailRequestPath, { id: filterDefaultWithoutId });
  filterDefault.deleteWithoutFilter = await capture(deleteRequestPath, { id: filterDefaultWithoutId });
} finally {
  if (filterDefaultWithoutId !== null && filterDefault.deleteWithoutFilter === null) {
    filterDefault.cleanupWithoutFilter = await capture(deleteRequestPath, { id: filterDefaultWithoutId });
  }
}

try {
  filterDefault.createWithEmptyFilter = await capture(
    createRequestPath,
    withWebhookUri(createVariables, filterDefaultEmptyUri, { filter: '' }),
  );
  filterDefaultEmptyId = readCreatedWebhookId(filterDefault.createWithEmptyFilter);
  if (filterDefaultEmptyId === null) {
    throw new Error('filter-default empty-filter create did not return a webhookSubscription.id.');
  }

  filterDefault.deleteWithEmptyFilter = await capture(deleteRequestPath, { id: filterDefaultEmptyId });
} finally {
  if (filterDefaultEmptyId !== null && filterDefault.deleteWithEmptyFilter === null) {
    filterDefault.cleanupWithEmptyFilter = await capture(deleteRequestPath, { id: filterDefaultEmptyId });
  }
}

try {
  filterValidation.setupCustomer = await captureDocument(
    'inline:webhook-filter-validation-customer-create',
    filterValidationCustomerCreate,
    {
      input: {
        email: `hermes-webhook-filter-${suffix}@example.com`,
        firstName: 'Hermes',
        lastName: 'WebhookFilter',
      },
    },
  );
  filterValidationCustomerId = readCreatedCustomerId(filterValidation.setupCustomer);
  if (filterValidationCustomerId === null) {
    throw new Error('filter-validation customer setup did not return a customer.id.');
  }

  const validCustomerIdFilter = `customer_id:${gidNumericTail(filterValidationCustomerId)}`;

  try {
    filterValidation.createValidFilter = await capture(
      createRequestPath,
      withWebhookUri({ ...createVariables, topic: 'CUSTOMERS_UPDATE' }, filterValidationValidUri, {
        filter: validCustomerIdFilter,
      }),
    );
    filterValidationValidId = readCreatedWebhookId(filterValidation.createValidFilter);
    if (filterValidationValidId === null) {
      throw new Error('filter-validation valid create did not return a webhookSubscription.id.');
    }

    filterValidation.detailValidFilter = await capture(detailRequestPath, { id: filterValidationValidId });
    filterValidation.deleteValidFilter = await capture(deleteRequestPath, { id: filterValidationValidId });
  } finally {
    if (filterValidationValidId !== null && filterValidation.deleteValidFilter === null) {
      filterValidation.cleanupValidFilter = await capture(deleteRequestPath, { id: filterValidationValidId });
    }
  }

  try {
    filterValidation.createUpdateBase = await capture(
      createRequestPath,
      withWebhookUri({ ...createVariables, topic: 'CUSTOMERS_UPDATE' }, filterValidationUpdateUri, {}, ['filter']),
    );
    filterValidationUpdateId = readCreatedWebhookId(filterValidation.createUpdateBase);
    if (filterValidationUpdateId === null) {
      throw new Error('filter-validation update setup did not return a webhookSubscription.id.');
    }

    filterValidation.updateValidFilter = await capture(
      updateRequestPath,
      withWebhookUri({ ...updateVariablesTemplate, id: filterValidationUpdateId }, filterValidationUpdateUri, {
        filter: validCustomerIdFilter,
        includeFields: ['id'],
        metafieldNamespaces: [],
      }),
    );
    filterValidation.detailAfterValidUpdate = await capture(detailRequestPath, { id: filterValidationUpdateId });
    filterValidation.updateInvalidFilter = await capture(
      updateRequestPath,
      withWebhookUri({ ...updateVariablesTemplate, id: filterValidationUpdateId }, filterValidationUpdateUri, {
        filter: invalidFilter,
        includeFields: ['id'],
        metafieldNamespaces: [],
      }),
    );
    filterValidation.deleteUpdateBase = await capture(deleteRequestPath, { id: filterValidationUpdateId });
  } finally {
    if (filterValidationUpdateId !== null && filterValidation.deleteUpdateBase === null) {
      filterValidation.cleanupUpdateBase = await capture(deleteRequestPath, { id: filterValidationUpdateId });
    }
  }

  filterValidation.createInvalidFilter = await capture(
    createRequestPath,
    withWebhookUri({ ...createVariables, topic: 'CUSTOMERS_UPDATE' }, filterValidationInvalidUri, {
      filter: invalidFilter,
    }),
  );
} finally {
  if (filterValidationCustomerId !== null) {
    filterValidation.cleanupCustomer = await captureDocument(
      'inline:webhook-filter-validation-customer-delete',
      filterValidationCustomerDelete,
      { input: { id: filterValidationCustomerId } },
    );
  }
}

try {
  updateValidation.create = await capture(
    createRequestPath,
    withWebhookUri(createVariables, `${createUri}-validation`),
  );
  updateValidationCreatedId = readCreatedWebhookId(updateValidation.create);
  if (updateValidationCreatedId === null) {
    throw new Error('update validation setup did not return a webhookSubscription.id.');
  }

  updateValidation.updateBlankUri = await capture(
    updateRequestPath,
    withWebhookUri({ ...updateVariablesTemplate, id: updateValidationCreatedId }, '', {
      includeFields: ['id'],
      metafieldNamespaces: [],
    }),
  );
  updateValidation.updateHttpUri = await capture(
    updateRequestPath,
    withWebhookUri({ ...updateVariablesTemplate, id: updateValidationCreatedId }, 'http://example.com', {
      includeFields: ['id'],
      metafieldNamespaces: [],
    }),
  );
  updateValidation.updateInvalidPubSub = await capture(
    updateRequestPath,
    withWebhookUri({ ...updateVariablesTemplate, id: updateValidationCreatedId }, 'pubsub://valid-project:', {
      includeFields: ['id'],
      metafieldNamespaces: [],
    }),
  );
  updateValidation.detailAfterFailedUpdates = await capture(detailRequestPath, { id: updateValidationCreatedId });
  updateValidation.delete = await capture(deleteRequestPath, { id: updateValidationCreatedId });
} finally {
  if (updateValidationCreatedId !== null && updateValidation.delete === null) {
    updateValidation.cleanup = await capture(deleteRequestPath, { id: updateValidationCreatedId });
  }
}

const catalog = await capture(catalogRequestPath, {
  ...catalogVariables,
  unknownId,
});
const validation = await capture(validationRequestPath, validationVariables);
const graphqlValidation = {
  missingCreateTopic: await captureGraphqlValidation(missingCreateTopicRequestPath),
  nullUpdateInput: await captureGraphqlValidation(nullUpdateInputRequestPath),
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
      filterDefault,
      filterValidation,
      updateValidation,
      validation,
      graphqlValidation,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote webhook subscription conformance fixture to ${outputPath}`);
