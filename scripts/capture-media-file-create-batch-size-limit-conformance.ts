import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const requestPath = path.join('config', 'parity-requests', 'media', 'media-file-create-batch-size-limit.graphql');

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const fixturePath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'media',
  'media-file-create-batch-size-limit.json',
);

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function absolutePath(relativePath: string): string {
  return path.join(repoRoot, relativePath);
}

function files(count: number): JsonRecord[] {
  return Array.from({ length: count }, (_, index) => ({
    originalSource: `https://placehold.co/600x400/media-file-create-batch-size-limit-${index + 1}.png`,
    contentType: 'IMAGE',
  }));
}

function asRecord(value: unknown): JsonRecord | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    return null;
  }

  return value as JsonRecord;
}

function assertMaxInputError(response: ConformanceGraphqlResult<JsonRecord>): void {
  const errors = response.payload.errors;
  if (!Array.isArray(errors) || errors.length !== 1) {
    throw new Error(`Expected one top-level max-input error, got: ${JSON.stringify(response.payload, null, 2)}`);
  }

  const error = asRecord(errors[0]);
  if (error === null) {
    throw new Error(`Expected object error, got: ${JSON.stringify(errors[0])}`);
  }

  if (error['message'] !== 'The input array size of 251 is greater than the maximum allowed of 250.') {
    throw new Error(`Unexpected max-input message: ${JSON.stringify(error['message'])}`);
  }

  if (JSON.stringify(error['path']) !== JSON.stringify(['fileCreate', 'files'])) {
    throw new Error(`Unexpected max-input path: ${JSON.stringify(error['path'])}`);
  }

  const extensions = asRecord(error['extensions']);
  if (extensions?.['code'] !== 'MAX_INPUT_SIZE_EXCEEDED') {
    throw new Error(`Unexpected max-input extensions: ${JSON.stringify(error['extensions'])}`);
  }

  if ('data' in response.payload) {
    throw new Error(`Expected no data payload for max-input failure, got: ${JSON.stringify(response.payload.data)}`);
  }
}

const document = await readFile(absolutePath(requestPath), 'utf8');
const variables = { files: files(251) };
const response = await runGraphqlRequest<JsonRecord>(document, variables);
assertMaxInputError(response);

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
