/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedCase = {
  name: string;
  documentPath: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'segments');
const localDispatchOutputPath = path.join(outputDir, 'segment-local-runtime-dispatch-validation.json');
const payloadOutputPath = path.join(outputDir, 'segment-payload-non-null-fields.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const localDispatchDocumentPath = 'config/parity-requests/segments/segment-local-runtime-dispatch-validation.graphql';
const payloadCreateDocumentPath = 'config/parity-requests/segments/segment-payload-non-null-fields-create.graphql';
const payloadUpdateDocumentPath = 'config/parity-requests/segments/segment-payload-non-null-fields-update.graphql';
const payloadReadDocumentPath = 'config/parity-requests/segments/segment-payload-non-null-fields-read.graphql';

const deleteMutation = `#graphql
  mutation SegmentPayloadShapeCaptureDelete($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

function assertGraphqlOk(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readSegmentId(result: ConformanceGraphqlResult, rootField: 'segmentCreate' | 'segmentUpdate'): string {
  const data = result.payload.data as Record<string, unknown> | undefined;
  const payload = data?.[rootField] as Record<string, unknown> | undefined;
  const segment = payload?.['segment'] as Record<string, unknown> | undefined;
  const id = segment?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`${rootField} did not return a segment id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return id;
}

async function captureCase(
  name: string,
  documentPath: string,
  variables: Record<string, unknown>,
): Promise<CapturedCase> {
  const query = await readFile(documentPath, 'utf8');
  const response = await runGraphqlRequest(query, variables);
  assertGraphqlOk(response, name);
  return {
    name,
    documentPath,
    request: { query, variables },
    response,
  };
}

async function deleteSegment(id: string, context: string): Promise<void> {
  const cleanup = await runGraphqlRequest(deleteMutation, { id });
  assertGraphqlOk(cleanup, `${context} cleanup`);
}

const marker = `segment-payload-shape-${Date.now()}`;
const createdSegmentIds = new Set<string>();

try {
  const localDispatchCase = await captureCase('neutralSegmentCreate', localDispatchDocumentPath, {
    name: `Neutral live payload ${marker}`,
    query: 'number_of_orders >= 1',
  });
  createdSegmentIds.add(readSegmentId(localDispatchCase.response, 'segmentCreate'));

  const payloadCreateCase = await captureCase('segmentPayloadCreate', payloadCreateDocumentPath, {
    name: `Payload public fields ${marker}`,
    query: 'number_of_orders >= 1',
  });
  const payloadSegmentId = readSegmentId(payloadCreateCase.response, 'segmentCreate');
  createdSegmentIds.add(payloadSegmentId);

  const payloadUpdateCase = await captureCase('segmentPayloadUpdate', payloadUpdateDocumentPath, {
    id: payloadSegmentId,
    name: `Payload public fields updated ${marker}`,
    query: 'number_of_orders >= 2',
  });
  readSegmentId(payloadUpdateCase.response, 'segmentUpdate');

  const payloadReadCase = await captureCase('segmentPayloadRead', payloadReadDocumentPath, {
    id: payloadSegmentId,
  });

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    localDispatchOutputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        scenarioId: 'segment-local-runtime-dispatch-validation',
        cases: [localDispatchCase],
        notes: [
          'Live Shopify payload capture for the neutral LocalSegmentCreate document that previously relied on local-runtime parity evidence.',
          'No-upstream local dispatch and mutation-log behavior are proxy-only semantics and are covered by Rust integration tests.',
        ],
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
  await writeFile(
    payloadOutputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        scenarioId: 'segment-payload-non-null-fields',
        cases: [payloadCreateCase, payloadUpdateCase, payloadReadCase],
        notes: [
          'Live Shopify evidence for Segment create/update/read payload fields exposed by the public Admin GraphQL schema.',
          'Admin GraphQL 2025-01 and 2026-04 expose only id, name, query, creationDate, and lastEditDate on Segment for this conformance shop.',
          'Private Core Segment fields absent from public introspection remain covered by Rust runtime tests instead of parity fixtures.',
        ],
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
} finally {
  for (const id of createdSegmentIds) {
    await deleteSegment(id, id);
  }
}

console.log(`Wrote ${localDispatchOutputPath}`);
console.log(`Wrote ${payloadOutputPath}`);
