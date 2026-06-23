/* oxlint-disable no-console -- shared CLI capture helpers intentionally write progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

export type JsonRecord = Record<string, unknown>;

export function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

export function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

export function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing required capture value: ${label}`);
  }
  return value;
}

export interface ConformanceCapture {
  storeDomain: string;
  apiVersion: string;
  /** A short UTC stamp (yyyymmddHHMMSS) unique per capture run. */
  stamp: string;
  /** The capture's start instant, for deriving safely-future dates. */
  now: Date;
  /** Raw GraphQL transport — use for best-effort cleanup where userErrors are tolerated. */
  runGraphqlRequest: <T = JsonRecord>(query: string, variables: JsonRecord) => Promise<ConformanceGraphqlResult<T>>;
  /** Run a document live; throws on transport error or top-level GraphQL errors. */
  run: (query: string, variables: JsonRecord, label: string) => Promise<JsonRecord>;
  /** Extract a mutation root and assert it has no userErrors. */
  mutationRoot: (payload: JsonRecord, rootName: string, label: string) => JsonRecord;
  /** Read a request document from config/parity-requests/<domain>/<name>. */
  readRequest: (domain: string, name: string) => Promise<string>;
  /** Read a parity-request file's raw bytes (e.g. a shared hydrate .graphql). */
  readRequestRaw: (domain: string, name: string) => Promise<string>;
  /** Resolve the fixture path under fixtures/conformance/<store>/<version>/<...segments>. */
  fixturePath: (...segments: string[]) => string;
  /** Pretty-write a JSON fixture (trailing newline), creating parent dirs. */
  writeJson: (filePath: string, payload: unknown) => Promise<void>;
}

export async function createConformanceCapture(): Promise<ConformanceCapture> {
  const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
  const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
  const { runGraphqlRequest } = createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(adminAccessToken),
  });

  const requestDir = path.join('config', 'parity-requests');

  const now = new Date();
  const stamp = now
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14);

  async function run(query: string, variables: JsonRecord, label: string): Promise<JsonRecord> {
    const result: ConformanceGraphqlResult<JsonRecord> = await runGraphqlRequest<JsonRecord>(query, variables);
    if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
      throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
    }
    return result.payload as JsonRecord;
  }

  function mutationRoot(payload: JsonRecord, rootName: string, label: string): JsonRecord {
    const root = readRecord(readRecord(payload['data'])?.[rootName]);
    if (!root) {
      throw new Error(`${label} missing ${rootName}: ${JSON.stringify(payload, null, 2)}`);
    }
    const userErrors = readArray(root['userErrors']);
    if (userErrors.length > 0) {
      throw new Error(`${label} ${rootName} userErrors: ${JSON.stringify(userErrors, null, 2)}`);
    }
    return root;
  }

  return {
    storeDomain,
    apiVersion,
    stamp,
    now,
    runGraphqlRequest,
    run,
    mutationRoot,
    readRequest: (domain: string, name: string) => readFile(path.join(requestDir, domain, name), 'utf8'),
    readRequestRaw: (domain: string, name: string) => readFile(path.join(requestDir, domain, name), 'utf8'),
    fixturePath: (...segments: string[]) => path.join('fixtures', 'conformance', storeDomain, apiVersion, ...segments),
    writeJson: async (filePath: string, payload: unknown) => {
      await mkdir(path.dirname(filePath), { recursive: true });
      await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
    },
  };
}

/** A safely-future ISO instant `days` out at noon UTC, for reserve-until style inputs. */
export function futureNoonIso(from: Date, days: number): string {
  return `${new Date(from.getTime() + days * 24 * 60 * 60 * 1000).toISOString().slice(0, 10)}T12:00:00Z`;
}
