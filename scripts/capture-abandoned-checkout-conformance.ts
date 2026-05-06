/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const requestDir = path.join('config', 'parity-requests', 'orders');

async function readText(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

async function readJson(filePath: string): Promise<JsonRecord> {
  return JSON.parse(await readFile(filePath, 'utf8')) as JsonRecord;
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function assertCaptured(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

async function captureRead({
  fixtureName,
  requestName,
  variablesName,
}: {
  fixtureName: string;
  requestName: string;
  variablesName: string;
}): Promise<string> {
  const query = await readText(path.join(requestDir, requestName));
  const variables = await readJson(path.join(requestDir, variablesName));
  const result = await runGraphqlRequest(query, variables);
  assertCaptured(fixtureName, result);

  const outputPath = path.join(fixtureDir, fixtureName);
  await writeJson(outputPath, {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    variables,
    response: result.payload,
    upstreamCalls: [],
  });
  return outputPath;
}

const outputs = [
  await captureRead({
    fixtureName: 'abandoned-checkout-empty-read.json',
    requestName: 'abandoned-checkout-empty-read.graphql',
    variablesName: 'abandoned-checkout-empty-read.variables.json',
  }),
  await captureRead({
    fixtureName: 'abandonment-delivery-status-unknown.json',
    requestName: 'abandonment-delivery-status-unknown.graphql',
    variablesName: 'abandonment-delivery-status-unknown.variables.json',
  }),
];

console.log(JSON.stringify({ ok: true, outputs }, null, 2));
