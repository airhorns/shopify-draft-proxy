import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const fixturePath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'orders',
  'draftOrder-line-items-max.json',
);
const requestPath = path.join('config', 'parity-requests', 'orders', 'draftOrder-line-items-max.graphql');

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function absolutePath(relativePath: string): string {
  return path.join(repoRoot, relativePath);
}

function lineItems(count: number): JsonRecord[] {
  return Array.from({ length: count }, (_, index) => ({
    title: `Line item max probe ${index + 1}`,
    quantity: 1,
    originalUnitPrice: '1.00',
  }));
}

function assertMaxInputErrors(response: ConformanceGraphqlResult<JsonRecord>): void {
  const errors = response.payload.errors;
  if (!Array.isArray(errors) || errors.length !== 3) {
    throw new Error(`Expected three top-level max-input errors, got: ${JSON.stringify(response.payload, null, 2)}`);
  }

  const expectedRoots = ['draftOrderCreate', 'draftOrderUpdate', 'draftOrderCalculate'];
  for (const [index, root] of expectedRoots.entries()) {
    const error = errors[index];
    if (typeof error !== 'object' || error === null || Array.isArray(error)) {
      throw new Error(`Expected object error for ${root}, got: ${JSON.stringify(error)}`);
    }
    const record = error as JsonRecord;
    if (record['message'] !== 'The input array size of 500 is greater than the maximum allowed of 499.') {
      throw new Error(`Unexpected ${root} max-input message: ${JSON.stringify(record['message'])}`);
    }
    if (JSON.stringify(record['path']) !== JSON.stringify([root, 'input', 'lineItems'])) {
      throw new Error(`Unexpected ${root} max-input path: ${JSON.stringify(record['path'])}`);
    }
    const extensions = record['extensions'];
    if (
      typeof extensions !== 'object' ||
      extensions === null ||
      Array.isArray(extensions) ||
      (extensions as JsonRecord)['code'] !== 'MAX_INPUT_SIZE_EXCEEDED'
    ) {
      throw new Error(`Unexpected ${root} max-input extensions: ${JSON.stringify(extensions)}`);
    }
  }

  if ('data' in response.payload) {
    throw new Error(`Expected no data payload for max-input failure, got: ${JSON.stringify(response.payload.data)}`);
  }
}

const document = await readFile(absolutePath(requestPath), 'utf8');
const oversizedInput = { lineItems: lineItems(500) };
const variables = {
  createInput: oversizedInput,
  updateInput: oversizedInput,
  calculateInput: oversizedInput,
};

const response = await runGraphqlRequest<JsonRecord>(document, variables);
assertMaxInputErrors(response);

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  document,
  variables,
  mutation: {
    response,
  },
  upstreamCalls: [],
};

const absoluteFixturePath = absolutePath(fixturePath);
await mkdir(path.dirname(absoluteFixturePath), { recursive: true });
await writeFile(absoluteFixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

process.stdout.write(`${JSON.stringify({ fixturePath, response }, null, 2)}\n`);
