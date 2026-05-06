/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedCase = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'segments');
const outputPath = path.join(outputDir, 'segment-update-delete-malformed-gid.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const segmentUpdateMutation = `#graphql
  mutation SegmentUpdateMalformedGid($id: ID!, $name: String!) {
    segmentUpdate(id: $id, name: $name) {
      segment {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const segmentDeleteMutation = `#graphql
  mutation SegmentDeleteMalformedGid($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

function assertStatusOk(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} failed with HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  assertStatusOk(result, context);
  if (!Array.isArray(result.payload.errors) || result.payload.errors.length === 0) {
    throw new Error(`${context} did not return top-level errors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertUserErrors(result: ConformanceGraphqlResult, root: string, context: string): void {
  assertStatusOk(result, context);
  if (result.payload.errors) {
    throw new Error(`${context} returned top-level errors: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const data = result.payload.data as Record<string, unknown> | undefined;
  const payload = data?.[root] as Record<string, unknown> | undefined;
  const userErrors = payload?.['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length === 0) {
    throw new Error(`${context} did not return payload userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

async function captureCase(
  cases: CapturedCase[],
  name: string,
  query: string,
  variables: Record<string, unknown>,
  assertResult: (result: ConformanceGraphqlResult) => void,
): Promise<void> {
  const response = await runGraphqlRequest(query, variables);
  assertResult(response);
  cases.push({
    name,
    request: { query, variables },
    response,
  });
}

const cases: CapturedCase[] = [];

await captureCase(cases, 'segmentUpdateMalformedGid', segmentUpdateMutation, { id: 'not-a-gid', name: 'x' }, (result) =>
  assertTopLevelErrors(result, 'segmentUpdate malformed gid'),
);
await captureCase(cases, 'segmentUpdateEmptyGid', segmentUpdateMutation, { id: '', name: 'x' }, (result) =>
  assertTopLevelErrors(result, 'segmentUpdate empty gid'),
);
await captureCase(
  cases,
  'segmentUpdateWrongTypeGid',
  segmentUpdateMutation,
  { id: 'gid://shopify/Order/1', name: 'x' },
  (result) => assertTopLevelErrors(result, 'segmentUpdate wrong-type gid'),
);
await captureCase(
  cases,
  'segmentUpdateUnknownSegment',
  segmentUpdateMutation,
  { id: 'gid://shopify/Segment/999999999999', name: 'x' },
  (result) => assertUserErrors(result, 'segmentUpdate', 'segmentUpdate unknown segment'),
);
await captureCase(cases, 'segmentDeleteMalformedGid', segmentDeleteMutation, { id: 'not-a-gid' }, (result) =>
  assertTopLevelErrors(result, 'segmentDelete malformed gid'),
);
await captureCase(cases, 'segmentDeleteEmptyGid', segmentDeleteMutation, { id: '' }, (result) =>
  assertTopLevelErrors(result, 'segmentDelete empty gid'),
);
await captureCase(
  cases,
  'segmentDeleteWrongTypeGid',
  segmentDeleteMutation,
  { id: 'gid://shopify/Order/1' },
  (result) => assertTopLevelErrors(result, 'segmentDelete wrong-type gid'),
);
await captureCase(
  cases,
  'segmentDeleteUnknownSegment',
  segmentDeleteMutation,
  { id: 'gid://shopify/Segment/999999999999' },
  (result) => assertUserErrors(result, 'segmentDelete', 'segmentDelete unknown segment'),
);

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cases,
      notes: [
        'Malformed and empty segment ids are rejected as top-level variable errors before resolver execution.',
        'Non-Segment Shopify GIDs are rejected as top-level resource-not-found errors with null mutation data.',
        'Well-formed but unknown Segment GIDs keep the payload-level UserError shape.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
