/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig, type ConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CaptureClient = {
  config: ConformanceScriptConfig;
  runGraphqlRequest: (query: string, variables?: JsonRecord) => Promise<ConformanceGraphqlResult<JsonRecord>>;
};

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

async function readText(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

async function readJson<T>(filePath: string): Promise<T> {
  return JSON.parse(await readText(filePath)) as T;
}

async function writeJson(filePath: string, value: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function outputPath(config: ConformanceScriptConfig, domain: string, filename: string): string {
  return path.join('fixtures', 'conformance', config.storeDomain, config.apiVersion, domain, filename);
}

function readRecord(value: unknown): JsonRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : {};
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function readPath(value: unknown, segments: string[]): unknown {
  let current = value;
  for (const segment of segments) {
    const record = readRecord(current);
    if (!(segment in record)) return null;
    current = record[segment];
  }
  return current;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult<JsonRecord>, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function upstreamReadCall(
  operationName: string,
  query: string,
  variables: JsonRecord,
  result: ConformanceGraphqlResult<JsonRecord>,
): JsonRecord {
  return {
    operationName,
    variables,
    query,
    response: {
      status: result.status,
      body: result.payload,
    },
  };
}

async function captureEvents(): Promise<string[]> {
  const client = await clientFor('2025-01');
  const written: string[] = [];

  const emptyQuery = await readText('config/parity-requests/events/event-empty-read.graphql');
  const emptyVariables = await readJson<JsonRecord>('config/parity-requests/events/event-empty-read.variables.json');
  const emptyResult = await client.runGraphqlRequest(emptyQuery, emptyVariables);
  assertNoTopLevelErrors(emptyResult, 'event empty read');

  const emptyFilePath = outputPath(client.config, 'events', 'event-empty-read.json');
  await writeJson(emptyFilePath, {
    variables: emptyVariables,
    response: emptyResult.payload,
    upstreamCalls: [upstreamReadCall('EventEmptyRead', emptyQuery, emptyVariables, emptyResult)],
  });
  written.push(emptyFilePath);

  const nonEmptyQuery = await readText('config/parity-requests/events/event-non-empty-read.graphql');
  const discoveryQuery = `#graphql
    query EventNonEmptyDiscovery($first: Int!) {
      events(first: $first, sortKey: ID, reverse: true) {
        nodes {
          id
        }
      }
    }
  `;
  const discoveryResult = await client.runGraphqlRequest(discoveryQuery, { first: 1 });
  assertNoTopLevelErrors(discoveryResult, 'event non-empty discovery');
  const discoveryNodes = readArray(readPath(discoveryResult.payload, ['data', 'events', 'nodes']));
  const eventId = readString(readRecord(discoveryNodes[0])['id']);
  if (eventId === null) {
    throw new Error('event non-empty discovery returned no event nodes for the live store');
  }

  const nonEmptyVariables = { eventId, first: 3 };
  const nonEmptyVariablesPath = 'config/parity-requests/events/event-non-empty-read.variables.json';
  await writeJson(nonEmptyVariablesPath, nonEmptyVariables);
  written.push(nonEmptyVariablesPath);

  const nonEmptyResult = await client.runGraphqlRequest(nonEmptyQuery, nonEmptyVariables);
  assertNoTopLevelErrors(nonEmptyResult, 'event non-empty read');
  const nonEmptyNodes = readArray(readPath(nonEmptyResult.payload, ['data', 'events', 'nodes']));
  if (nonEmptyNodes.length === 0) {
    throw new Error('event non-empty read returned no event nodes for the live store');
  }

  const nonEmptyFilePath = outputPath(client.config, 'events', 'event-non-empty-read.json');
  await writeJson(nonEmptyFilePath, {
    variables: nonEmptyVariables,
    response: nonEmptyResult.payload,
    upstreamCalls: [upstreamReadCall('EventNonEmptyRead', nonEmptyQuery, nonEmptyVariables, nonEmptyResult)],
  });
  written.push(nonEmptyFilePath);

  return written;
}

const written = await captureEvents();
console.log(JSON.stringify({ ok: true, domain: 'events', written }, null, 2));
