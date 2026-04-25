import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CapturedMutation = {
  variables: JsonRecord;
  mutation: {
    response: ConformanceGraphqlPayload<JsonRecord>;
  };
};

type CapturedDraftOrderSetup = CapturedMutation & {
  downstreamRead: {
    variables: JsonRecord;
    response: ConformanceGraphqlPayload<JsonRecord>;
  };
};

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function absolutePath(relativePath: string): string {
  return path.join(repoRoot, relativePath);
}

async function readText(relativePath: string): Promise<string> {
  return readFile(absolutePath(relativePath), 'utf8');
}

async function readJson(relativePath: string): Promise<JsonRecord> {
  return JSON.parse(await readText(relativePath)) as JsonRecord;
}

function cloneRecord(value: JsonRecord): JsonRecord {
  return JSON.parse(JSON.stringify(value)) as JsonRecord;
}

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readRecord(value: unknown, key: string): JsonRecord | null {
  return asRecord(asRecord(value)?.[key]);
}

function readArray(value: unknown, key: string): unknown[] {
  const fieldValue = asRecord(value)?.[key];
  return Array.isArray(fieldValue) ? fieldValue : [];
}

function readString(value: unknown, key: string): string | null {
  const fieldValue = asRecord(value)?.[key];
  return typeof fieldValue === 'string' && fieldValue.length > 0 ? fieldValue : null;
}

function responseData(payload: ConformanceGraphqlPayload<JsonRecord>): JsonRecord {
  const data = asRecord(payload.data);
  if (!data) {
    throw new Error(`Expected GraphQL payload data, got: ${JSON.stringify(payload, null, 2)}`);
  }
  return data;
}

function mutationField(payload: ConformanceGraphqlPayload<JsonRecord>, name: string): JsonRecord {
  const field = readRecord(responseData(payload), name);
  if (!field) {
    throw new Error(`Expected ${name} mutation payload, got: ${JSON.stringify(payload, null, 2)}`);
  }
  return field;
}

function draftOrderIdFromPayload(payload: ConformanceGraphqlPayload<JsonRecord>, name: string): string {
  const id = readString(readRecord(mutationField(payload, name), 'draftOrder'), 'id');
  if (!id) {
    throw new Error(`Expected ${name}.draftOrder.id in payload: ${JSON.stringify(payload, null, 2)}`);
  }
  return id;
}

function orderIdFromDraftOrderComplete(payload: ConformanceGraphqlPayload<JsonRecord>): string {
  const orderId = readString(
    readRecord(readRecord(mutationField(payload, 'draftOrderComplete'), 'draftOrder'), 'order'),
    'id',
  );
  if (!orderId) {
    throw new Error(`Expected draftOrderComplete.draftOrder.order.id in payload: ${JSON.stringify(payload, null, 2)}`);
  }
  return orderId;
}

async function writeJson(relativePath: string, value: unknown): Promise<void> {
  await mkdir(path.dirname(absolutePath(relativePath)), { recursive: true });
  await writeFile(absolutePath(relativePath), `${JSON.stringify(value, null, 2)}\n`);
}

const draftOrderCreateDocument = await readText('config/parity-requests/draftOrderCreate-parity-plan.graphql');
const draftOrderCompleteDocument = await readText('config/parity-requests/draftOrderComplete-parity-plan.graphql');
const draftOrderDownstreamReadDocument = await readText(
  'config/parity-requests/draftOrderCreate-downstream-read.graphql',
);
const draftOrderDetailReadDocument = await readText('config/parity-requests/draftOrder-read-parity-plan.graphql');
const draftOrderCreateFromOrderDownstreamReadDocument = await readText(
  'config/parity-requests/draftOrderCreateFromOrder-downstream-read.graphql',
);
const orderDownstreamReadDocument = await readText('config/parity-requests/orderCreate-downstream-read.graphql');
const draftOrderUpdateDocument = await readText('config/parity-requests/draftOrderUpdate-parity-plan.graphql');
const draftOrderDuplicateDocument = await readText('config/parity-requests/draftOrderDuplicate-parity-plan.graphql');
const draftOrderDeleteDocument = await readText('config/parity-requests/draftOrderDelete-parity-plan.graphql');
const draftOrderCreateFromOrderDocument = await readText(
  'config/parity-requests/draftOrderCreateFromOrder-parity-plan.graphql',
);

const draftOrderCreateBaseVariables = await readJson(
  'config/parity-requests/draftOrderCreate-parity-plan.variables.json',
);
const draftOrderCompleteBaseVariables = await readJson(
  'config/parity-requests/draftOrderComplete-parity-plan.variables.json',
);
const draftOrderUpdateBaseVariables = await readJson(
  'config/parity-requests/draftOrderUpdate-parity-plan.variables.json',
);
const draftOrderDuplicateBaseVariables = await readJson(
  'config/parity-requests/draftOrderDuplicate-parity-plan.variables.json',
);
const draftOrderDeleteBaseVariables = await readJson(
  'config/parity-requests/draftOrderDelete-parity-plan.variables.json',
);
const draftOrderCreateFromOrderBaseVariables = await readJson(
  'config/parity-requests/draftOrderCreateFromOrder-parity-plan.variables.json',
);

const stamp = Date.now();

type DraftOrderSeedReferences = {
  customerId: string | null;
  variantId: string | null;
};

async function readDraftOrderSeedReferences(): Promise<DraftOrderSeedReferences> {
  const response = await runGraphql<JsonRecord>(
    `#graphql
      query DraftOrderFamilySeedReferences {
        customers(first: 10) {
          nodes {
            id
          }
        }
        products(first: 20) {
          nodes {
            variants(first: 20) {
              nodes {
                id
              }
            }
          }
        }
      }
    `,
    {},
  );
  const data = responseData(response);
  const customerId =
    readArray(readRecord(data, 'customers'), 'nodes')
      .map((customer) => readString(customer, 'id'))
      .find((id): id is string => id !== null) ?? null;
  const variantId =
    readArray(readRecord(data, 'products'), 'nodes')
      .flatMap((product) => readArray(readRecord(product, 'variants'), 'nodes'))
      .map((variant) => readString(variant, 'id'))
      .find((id): id is string => id !== null) ?? null;

  return { customerId, variantId };
}

function applyDraftOrderSeedReferences(variables: JsonRecord, seedReferences: DraftOrderSeedReferences): void {
  const input = readRecord(variables, 'input');
  if (!input) {
    return;
  }

  if (seedReferences.customerId) {
    input['purchasingEntity'] = {
      ...readRecord(input, 'purchasingEntity'),
      customerId: seedReferences.customerId,
    };
  } else {
    delete input['purchasingEntity'];
  }

  const lineItems = input['lineItems'];
  if (!Array.isArray(lineItems)) {
    return;
  }

  if (!seedReferences.variantId) {
    input['lineItems'] = lineItems.filter(
      (lineItem) => !(typeof lineItem === 'object' && lineItem !== null && 'variantId' in lineItem),
    );
    return;
  }

  const variantLineItem = lineItems.find(
    (lineItem): lineItem is JsonRecord => typeof lineItem === 'object' && lineItem !== null && 'variantId' in lineItem,
  );
  if (variantLineItem) {
    variantLineItem['variantId'] = seedReferences.variantId;
  }
}

const draftOrderSeedReferences = await readDraftOrderSeedReferences();

async function createDraftOrder(label: string): Promise<CapturedDraftOrderSetup & { id: string }> {
  const variables = cloneRecord(draftOrderCreateBaseVariables);
  applyDraftOrderSeedReferences(variables, draftOrderSeedReferences);
  const input = readRecord(variables, 'input');
  if (!input) {
    throw new Error('draftOrderCreate parity variables are missing input.');
  }
  input['email'] = `hermes-draft-order-${label}-${stamp}@example.com`;
  input['note'] = `draft order ${label} setup`;
  input['tags'] = ['parity-capture', 'draft-order-family', label];

  const response = await runGraphql<JsonRecord>(draftOrderCreateDocument, variables);
  const id = draftOrderIdFromPayload(response, 'draftOrderCreate');
  const downstreamRead = await runGraphql<JsonRecord>(draftOrderDownstreamReadDocument, { id });

  return {
    id,
    variables,
    mutation: {
      response,
    },
    downstreamRead: {
      variables: { id },
      response: downstreamRead,
    },
  };
}

async function captureDraftOrderDetail(): Promise<void> {
  const setup = await createDraftOrder('detail');
  const response = await runGraphql<JsonRecord>(draftOrderDetailReadDocument, { id: setup.id });
  await writeJson(path.join(fixtureDir, 'draft-order-detail.json'), {
    variables: { id: setup.id },
    response,
  });
}

async function completeDraftOrder(label: string): Promise<
  CapturedDraftOrderSetup & {
    completed: CapturedMutation;
    completedDraftOrderId: string;
    orderId: string;
    downstreamOrderRead: { variables: JsonRecord; response: ConformanceGraphqlPayload<JsonRecord> };
  }
> {
  const setup = await createDraftOrder(label);
  const variables = cloneRecord(draftOrderCompleteBaseVariables);
  variables['id'] = setup.id;
  const response = await runGraphql<JsonRecord>(draftOrderCompleteDocument, variables);
  const completedDraftOrderId = draftOrderIdFromPayload(response, 'draftOrderComplete');
  const orderId = orderIdFromDraftOrderComplete(response);
  const downstreamOrderRead = await runGraphql<JsonRecord>(orderDownstreamReadDocument, { id: orderId });

  return {
    ...setup,
    completed: {
      variables,
      mutation: {
        response,
      },
    },
    completedDraftOrderId,
    orderId,
    downstreamOrderRead: {
      variables: { id: orderId },
      response: downstreamOrderRead,
    },
  };
}

async function captureDraftOrderUpdate(): Promise<void> {
  const setup = await createDraftOrder('update');
  const variables = cloneRecord(draftOrderUpdateBaseVariables);
  variables['id'] = setup.id;
  const input = readRecord(variables, 'input');
  if (!input) {
    throw new Error('draftOrderUpdate parity variables are missing input.');
  }
  input['email'] = `hermes-draft-order-update-${stamp}@example.com`;
  input['note'] = 'draft order update live parity capture';

  const response = await runGraphql<JsonRecord>(draftOrderUpdateDocument, variables);
  const downstreamRead = await runGraphql<JsonRecord>(draftOrderDownstreamReadDocument, { id: setup.id });
  await writeJson(path.join(fixtureDir, 'draft-order-update-parity.json'), {
    setup: {
      draftOrderCreate: setup,
    },
    variables,
    mutation: {
      response,
    },
    downstreamRead: {
      variables: { id: setup.id },
      response: downstreamRead,
    },
  });
}

async function captureDraftOrderDuplicate(): Promise<void> {
  const setup = await createDraftOrder('duplicate');
  const variables = cloneRecord(draftOrderDuplicateBaseVariables);
  variables['id'] = setup.id;
  const response = await runGraphql<JsonRecord>(draftOrderDuplicateDocument, variables);
  const duplicatedId = draftOrderIdFromPayload(response, 'draftOrderDuplicate');
  const downstreamRead = await runGraphql<JsonRecord>(draftOrderDownstreamReadDocument, { id: duplicatedId });
  await writeJson(path.join(fixtureDir, 'draft-order-duplicate-parity.json'), {
    setup: {
      draftOrderCreate: setup,
    },
    variables,
    mutation: {
      response,
    },
    downstreamRead: {
      variables: { id: duplicatedId },
      response: downstreamRead,
    },
  });
}

async function captureDraftOrderDelete(): Promise<void> {
  const setup = await createDraftOrder('delete');
  const variables = cloneRecord(draftOrderDeleteBaseVariables);
  const input = readRecord(variables, 'input');
  if (!input) {
    throw new Error('draftOrderDelete parity variables are missing input.');
  }
  input['id'] = setup.id;

  const response = await runGraphql<JsonRecord>(draftOrderDeleteDocument, variables);
  const downstreamRead = await runGraphql<JsonRecord>(draftOrderDownstreamReadDocument, { id: setup.id });
  await writeJson(path.join(fixtureDir, 'draft-order-delete-parity.json'), {
    setup: {
      draftOrderCreate: setup,
    },
    variables,
    mutation: {
      response,
    },
    downstreamRead: {
      variables: { id: setup.id },
      response: downstreamRead,
    },
  });
}

async function captureDraftOrderCreateFromOrder(): Promise<void> {
  const setup = await completeDraftOrder('create-from-order');
  const variables = cloneRecord(draftOrderCreateFromOrderBaseVariables);
  variables['orderId'] = setup.orderId;

  const response = await runGraphql<JsonRecord>(draftOrderCreateFromOrderDocument, variables);
  const draftOrderId = draftOrderIdFromPayload(response, 'draftOrderCreateFromOrder');
  const downstreamRead = await runGraphql<JsonRecord>(draftOrderCreateFromOrderDownstreamReadDocument, {
    id: draftOrderId,
  });
  await writeJson(path.join(fixtureDir, 'draft-order-create-from-order-parity.json'), {
    setup: {
      draftOrderCreate: {
        variables: setup.variables,
        mutation: setup.mutation,
        downstreamRead: setup.downstreamRead,
      },
      draftOrderComplete: setup.completed,
      downstreamOrderRead: setup.downstreamOrderRead,
    },
    variables,
    mutation: {
      response,
    },
    downstreamRead: {
      variables: { id: draftOrderId },
      response: downstreamRead,
    },
  });
}

await captureDraftOrderDetail();
await captureDraftOrderUpdate();
await captureDraftOrderDuplicate();
await captureDraftOrderDelete();
await captureDraftOrderCreateFromOrder();

// oxlint-disable-next-line no-console -- CLI scripts intentionally write status output to stdout.
console.log(
  JSON.stringify(
    {
      ok: true,
      storeDomain,
      apiVersion,
      files: [
        path.join(fixtureDir, 'draft-order-detail.json'),
        path.join(fixtureDir, 'draft-order-update-parity.json'),
        path.join(fixtureDir, 'draft-order-duplicate-parity.json'),
        path.join(fixtureDir, 'draft-order-delete-parity.json'),
        path.join(fixtureDir, 'draft-order-create-from-order-parity.json'),
      ],
    },
    null,
    2,
  ),
);
