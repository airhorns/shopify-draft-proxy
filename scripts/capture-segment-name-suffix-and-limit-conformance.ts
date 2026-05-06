/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type SegmentCreatePayload = {
  segment?: { id?: unknown; name?: unknown } | null;
  userErrors?: unknown;
};

type SegmentNode = {
  id: string;
  name: string | null;
};

const maxSegmentsPerShop = 6000;
const prefix = 'Draft Proxy Segment Suffix Parity';
const query = 'number_of_orders >= 1';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'segments');
const outputPath = path.join(outputDir, 'segment-name-suffix-and-limit.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createMutation = `#graphql
  mutation SegmentNameSuffixLimitCreate($name: String!, $query: String!) {
    segmentCreate(name: $name, query: $query) {
      segment {
        id
        name
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation SegmentNameSuffixLimitCleanup($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

const segmentCatalogQuery = `#graphql
  query SegmentNameSuffixLimitCatalog($first: Int!, $after: String) {
    segments(first: $first, after: $after) {
      nodes {
        id
        name
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
    segmentsCount {
      count
      precision
    }
  }
`;

function assertGraphqlOk(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readCreatePayload(result: ConformanceGraphqlResult, context: string): SegmentCreatePayload {
  const data = result.payload.data as Record<string, unknown> | undefined;
  const create = data?.['segmentCreate'] as SegmentCreatePayload | undefined;
  if (!create) {
    throw new Error(`${context} did not return segmentCreate: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return create;
}

function readCreatedSegmentId(result: ConformanceGraphqlResult, context: string): string {
  const create = readCreatePayload(result, context);
  const id = create.segment?.id;
  if (typeof id !== 'string') {
    throw new Error(`${context} did not create a segment: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return id;
}

async function createSegment(name: string, context: string): Promise<ConformanceGraphqlResult> {
  const result = await runGraphqlRequest(createMutation, { name, query });
  assertGraphqlOk(result, context);
  return result;
}

async function deleteSegment(id: string): Promise<void> {
  const result = await runGraphqlRequest(deleteMutation, { id });
  assertGraphqlOk(result, `segmentDelete cleanup ${id}`);
}

async function listSegments(): Promise<{ nodes: SegmentNode[]; count: number }> {
  const nodes: SegmentNode[] = [];
  let after: string | null = null;
  let count = 0;

  for (;;) {
    const result = await runGraphqlRequest(segmentCatalogQuery, {
      first: 250,
      after,
    });
    assertGraphqlOk(result, 'segments cleanup catalog');
    const data = result.payload.data as
      | {
          segments?: {
            nodes?: SegmentNode[];
            pageInfo?: { hasNextPage?: boolean; endCursor?: string | null };
          };
          segmentsCount?: { count?: number };
        }
      | undefined;
    nodes.push(...(data?.segments?.nodes ?? []));
    count = typeof data?.segmentsCount?.count === 'number' ? data.segmentsCount.count : count;

    if (data?.segments?.pageInfo?.hasNextPage !== true) {
      return { nodes, count };
    }

    after = data.segments.pageInfo.endCursor ?? null;
    if (!after) {
      return { nodes, count };
    }
  }
}

async function cleanupPrefixSegments(): Promise<number> {
  const { nodes } = await listSegments();
  const stale = nodes.filter((node) => node.name?.startsWith(prefix));
  for (const node of stale) {
    await deleteSegment(node.id);
  }
  return stale.length;
}

async function cleanupCreatedSegments(ids: string[]): Promise<void> {
  for (const id of ids.reverse()) {
    await deleteSegment(id);
  }
}

const createdIds: string[] = [];
const notes: string[] = [];
let suffixSetup: ConformanceGraphqlResult | null = null;
let suffixBump: ConformanceGraphqlResult | null = null;
let retryTaken: ConformanceGraphqlResult | null = null;
let segmentLimit: ConformanceGraphqlResult | null = null;
let capSetupCreatedCount = 0;

try {
  const staleDeleted = await cleanupPrefixSegments();
  notes.push(`Deleted ${staleDeleted} stale disposable segments with the parity prefix before capture.`);

  suffixSetup = await createSegment(`${prefix} Foo (5)`, 'segmentCreate suffix setup');
  createdIds.push(readCreatedSegmentId(suffixSetup, 'segmentCreate suffix setup'));

  suffixBump = await createSegment(`${prefix} Foo (5)`, 'segmentCreate suffix bump');
  createdIds.push(readCreatedSegmentId(suffixBump, 'segmentCreate suffix bump'));

  for (let index = 1; index <= 11; index += 1) {
    const name = index === 1 ? `${prefix} Bar` : `${prefix} Bar (${index})`;
    const result = await createSegment(name, `segmentCreate retry setup ${index}`);
    createdIds.push(readCreatedSegmentId(result, `segmentCreate retry setup ${index}`));
  }

  retryTaken = await createSegment(`${prefix} Bar`, 'segmentCreate retry exhaustion');

  const beforeCap = await listSegments();
  const neededForCap = Math.max(0, maxSegmentsPerShop - beforeCap.count);
  notes.push(
    `segmentsCount before cap setup was ${beforeCap.count}; creating ${neededForCap} disposable cap segments.`,
  );

  for (let index = 1; index <= neededForCap; index += 1) {
    const result = await createSegment(`${prefix} Cap ${index}`, `segmentCreate cap setup ${index}`);
    createdIds.push(readCreatedSegmentId(result, `segmentCreate cap setup ${index}`));
    capSetupCreatedCount += 1;

    if (index % 250 === 0) {
      console.log(`created ${index}/${neededForCap} cap setup segments`);
    }
  }

  segmentLimit = await createSegment(`${prefix} Extra`, 'segmentCreate segment limit');
} finally {
  await cleanupCreatedSegments(createdIds);
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      maxSegmentsPerShop,
      capSetupCreatedCount,
      names: {
        suffix: `${prefix} Foo (5)`,
        retryBase: `${prefix} Bar`,
        segmentLimitExtra: `${prefix} Extra`,
      },
      results: {
        suffixSetup,
        suffixBump,
        retryTaken,
        segmentLimit,
      },
      notes: [
        ...notes,
        'Live Shopify evidence covers trailing numeric suffix bumping, duplicate retry exhaustion, and the segment-limit userError field/message shape.',
        'The disposable cap setup creates enough segments to reach the live shop cap, captures the cap rejection, then deletes every segment created by this script.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
