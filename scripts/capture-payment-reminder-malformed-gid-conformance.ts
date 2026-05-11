/* oxlint-disable no-console -- Capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig, type ConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CaptureClient = {
  config: ConformanceScriptConfig;
  runGraphqlRequest: (query: string, variables?: JsonRecord) => Promise<ConformanceGraphqlResult<JsonRecord>>;
};

const reminderRequestDocument = `mutation PaymentReminderSendMalformedGid($paymentScheduleId: ID!) {
  paymentReminderSend(paymentScheduleId: $paymentScheduleId) {
    success
    userErrors {
      field
      code
      message
    }
  }
}`;

async function clientFor(apiVersion: string): Promise<CaptureClient> {
  const config = readConformanceScriptConfig({
    defaultApiVersion: apiVersion,
    env: { ...process.env, SHOPIFY_CONFORMANCE_API_VERSION: apiVersion },
    exitOnMissing: true,
  });
  const accessToken = await getValidConformanceAccessToken({
    adminOrigin: config.adminOrigin,
    apiVersion: config.apiVersion,
  });
  const client = createAdminGraphqlClient({
    adminOrigin: config.adminOrigin,
    apiVersion: config.apiVersion,
    headers: buildAdminAuthHeaders(accessToken),
  });
  return { config, runGraphqlRequest: client.runGraphqlRequest };
}

function outputPath(config: ConformanceScriptConfig, domain: string, filename: string): string {
  return path.join('fixtures', 'conformance', config.storeDomain, config.apiVersion, domain, filename);
}

async function writeJson(filePath: string, value: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

async function captureMalformedCase(
  client: CaptureClient,
  name: string,
  paymentScheduleId: string,
): Promise<JsonRecord> {
  const variables = { paymentScheduleId };
  const response = await client.runGraphqlRequest(reminderRequestDocument, variables);
  return {
    name,
    request: {
      query: reminderRequestDocument,
      variables,
    },
    response: {
      status: response.status,
      payload: response.payload,
    },
  };
}

async function capturePaymentReminderSendMalformedGid(): Promise<string> {
  const client = await clientFor('2025-01');
  const filePath = outputPath(client.config, 'payments', 'payment-reminder-send-malformed-gid.json');
  const cases = [
    await captureMalformedCase(client, 'emptyScheduleId', ''),
    await captureMalformedCase(client, 'nonGidScheduleId', 'not-a-gid'),
    await captureMalformedCase(client, 'wrongTypeScheduleId', 'gid://shopify/Order/1'),
  ];

  await writeJson(filePath, {
    capturedAt: new Date().toISOString(),
    storeDomain: client.config.storeDomain,
    apiVersion: client.config.apiVersion,
    scenarioId: 'payment-reminder-send-malformed-gid',
    notes:
      'Live capture for paymentReminderSend paymentScheduleId GID coercion. Empty and non-GID variables are rejected before resolver execution with INVALID_VARIABLE; wrong-resource Shopify GIDs return RESOURCE_NOT_FOUND with null paymentReminderSend data.',
    requestDocument: reminderRequestDocument.replace(/\s+/gu, ' ').trim(),
    cases,
    upstreamCalls: [],
  });
  return filePath;
}

capturePaymentReminderSendMalformedGid()
  .then((filePath) => {
    console.log(`Captured ${filePath}`);
  })
  .catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
