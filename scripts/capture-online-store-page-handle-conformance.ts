/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'online-store');
const outputPath = path.join(outputDir, 'online-store-page-handle-dedupe-and-takenness.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const pageCreateMutation = `#graphql
  mutation OnlineStorePageHandleDedupeAndTakenness($page: PageCreateInput!) {
    pageCreate(page: $page) {
      page {
        id
        title
        handle
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const pageDeleteMutation = `#graphql
  mutation OnlineStorePageHandleCleanup($id: ID!) {
    pageDelete(id: $id) {
      deletedPageId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

async function capture(
  captures: Capture[],
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<unknown> {
  const result = await runGraphqlRaw(query, variables);
  captures.push({
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  });
  return result.payload;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  return pathSegments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) {
      return null;
    }
    return (current as Record<string, unknown>)[segment] ?? null;
  }, value);
}

function readPageId(payload: unknown): string | null {
  const id = readPath(payload, ['data', 'pageCreate', 'page', 'id']);
  return typeof id === 'string' && id.length > 0 ? id : null;
}

function assertGraphqlOk(payload: unknown, label: string): void {
  if (readPath(payload, ['errors'])) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(payload, null, 2)}`);
  }
}

const suffix = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const aboutTitle = `HAR 551 ${suffix} About`;
const aboutHandle = `har-551-${suffix}-about`;
const punctuationTitle = `HAR 551 ${suffix} Hello, World!`;
const captures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
const createdPageIds: string[] = [];

try {
  const first = await capture(captures, 'pageCreate-about-first', pageCreateMutation, {
    page: { title: aboutTitle },
  });
  assertGraphqlOk(first, 'pageCreate-about-first');
  const firstId = readPageId(first);
  if (firstId) createdPageIds.push(firstId);

  const second = await capture(captures, 'pageCreate-about-second', pageCreateMutation, {
    page: { title: aboutTitle },
  });
  assertGraphqlOk(second, 'pageCreate-about-second');
  const secondId = readPageId(second);
  if (secondId) createdPageIds.push(secondId);

  const explicitTaken = await capture(captures, 'pageCreate-explicit-handle-taken', pageCreateMutation, {
    page: { title: `${aboutTitle} Explicit`, handle: aboutHandle },
  });
  assertGraphqlOk(explicitTaken, 'pageCreate-explicit-handle-taken');

  const punctuation = await capture(captures, 'pageCreate-punctuation-title', pageCreateMutation, {
    page: { title: punctuationTitle },
  });
  assertGraphqlOk(punctuation, 'pageCreate-punctuation-title');
  const punctuationId = readPageId(punctuation);
  if (punctuationId) createdPageIds.push(punctuationId);
} finally {
  for (const id of createdPageIds.reverse()) {
    await capture(cleanupCaptures, 'pageDelete-cleanup', pageDeleteMutation, { id });
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'online-store/page-handle-dedupe-and-takenness',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      interactions: captures,
      cleanup: cleanupCaptures,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
