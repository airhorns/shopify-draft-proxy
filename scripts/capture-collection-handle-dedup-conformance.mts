/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

type CapturedGraphqlResult = {
  status: number;
  payload: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'collection-handle-dedup-parity.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const collectionCreateMutation = `#graphql
  mutation CollectionHandleLifecycleCreate($input: CollectionInput!) {
    collectionCreate(input: $input) {
      collection {
        id
        title
        handle
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const collectionDeleteMutation = `#graphql
  mutation CollectionHandleLifecycleCleanup($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors {
        field
        message
      }
    }
  }
`;

async function capture(query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = (await runGraphqlRaw(query, variables)) as CapturedGraphqlResult;
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  return pathSegments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) {
      return null;
    }
    return (current as Record<string, unknown>)[segment] ?? null;
  }, value);
}

function requireCollection(result: Capture, label: string): { id: string; handle: string } {
  const errors = readPath(result.response, ['errors']);
  if (errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(errors)}`);
  }
  const userErrors = readPath(result.response, ['data', 'collectionCreate', 'userErrors']);
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${label} returned collectionCreate userErrors: ${JSON.stringify(userErrors)}`);
  }
  const id = readPath(result.response, ['data', 'collectionCreate', 'collection', 'id']);
  const handle = readPath(result.response, ['data', 'collectionCreate', 'collection', 'handle']);
  if (typeof id !== 'string' || typeof handle !== 'string') {
    throw new Error(`${label} did not return a collection id and handle.`);
  }
  return { id, handle };
}

function nextNumericTail(handle: string): string {
  const match = /^(.*?)(\d+)$/u.exec(handle);
  if (!match) {
    throw new Error(`Expected a trailing numeric handle, received ${JSON.stringify(handle)}.`);
  }
  return `${match[1]}${BigInt(match[2]) + 1n}`;
}

const captureToken = Date.now()
  .toString(36)
  .replace(/[0-9]/gu, (digit) => String.fromCharCode('a'.charCodeAt(0) + Number(digit)));
const nonnumericTitle = `Collection Handle Nonnumeric ${captureToken}`;
const numericTitle = `Collection Handle Numeric ${captureToken} 41`;
const operations: Record<string, Capture> = {};
const cleanup: Capture[] = [];
const collectionIds: string[] = [];

try {
  operations.nonnumericFirst = await capture(collectionCreateMutation, {
    input: { title: nonnumericTitle },
  });
  const nonnumericFirst = requireCollection(operations.nonnumericFirst, 'nonnumericFirst');
  collectionIds.push(nonnumericFirst.id);

  operations.nonnumericSecond = await capture(collectionCreateMutation, {
    input: { title: nonnumericTitle },
  });
  const nonnumericSecond = requireCollection(operations.nonnumericSecond, 'nonnumericSecond');
  collectionIds.push(nonnumericSecond.id);
  if (nonnumericSecond.handle !== `${nonnumericFirst.handle}-1`) {
    throw new Error(
      `Expected nonnumeric collision family ${nonnumericFirst.handle}-1, received ${nonnumericSecond.handle}.`,
    );
  }

  operations.numericFirst = await capture(collectionCreateMutation, {
    input: { title: numericTitle },
  });
  const numericFirst = requireCollection(operations.numericFirst, 'numericFirst');
  collectionIds.push(numericFirst.id);
  if (!numericFirst.handle.endsWith('-41')) {
    throw new Error(`Expected the numeric source handle to end in -41, received ${numericFirst.handle}.`);
  }

  operations.numericSecond = await capture(collectionCreateMutation, {
    input: { title: numericTitle },
  });
  const numericSecond = requireCollection(operations.numericSecond, 'numericSecond');
  collectionIds.push(numericSecond.id);
  const expectedNumericHandle = nextNumericTail(numericFirst.handle);
  if (numericSecond.handle !== expectedNumericHandle) {
    throw new Error(`Expected numeric collision handle ${expectedNumericHandle}, received ${numericSecond.handle}.`);
  }
} finally {
  for (const id of collectionIds.reverse()) {
    cleanup.push(await capture(collectionDeleteMutation, { input: { id } }));
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'collection-handle-dedup',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      operations,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
