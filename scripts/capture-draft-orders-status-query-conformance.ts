/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import { readFile } from 'node:fs/promises';

import { createConformanceCapture, type JsonRecord } from './conformance-capture-lib.js';

const cap = await createConformanceCapture();
const fixturePath = cap.fixturePath('orders', 'draft-orders-status-query-read.json');
const document = await cap.readRequestRaw('orders', 'draftOrders-status-query-read.graphql');
const variables = JSON.parse(
  await readFile('config/parity-requests/orders/draftOrders-status-query-read.variables.json', 'utf8'),
) as JsonRecord;

const response = await cap.run(document, variables, 'draftOrders status query read');

await cap.writeJson(fixturePath, {
  scenarioId: 'draft-orders-status-query-read',
  apiVersion: cap.apiVersion,
  storeDomain: cap.storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  notes:
    'Live draftOrders/draftOrdersCount query evidence for a valid status:open search filter. The fixture records the cold read as an upstream cassette so the proxy replay compares the Shopify response without local setup seeding.',
  variables,
  response,
  upstreamCalls: [
    {
      operationName: 'DraftOrdersStatusQueryRead',
      variables,
      query: document,
      response: {
        status: 200,
        body: response,
      },
    },
  ],
});

console.log(JSON.stringify({ fixturePath, variables } satisfies JsonRecord, null, 2));
