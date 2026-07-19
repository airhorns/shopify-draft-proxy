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
const outputPath = path.join(outputDir, 'segment-malformed-id-error-locations.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const segmentReadVariableQuery = `query SegmentMalformedIdReadVariable(
  $id: ID!
) {
  segment(id: $id) {
    id
  }
}
`;

const segmentReadInlineQuery = `query SegmentMalformedIdReadInline {
  segment(
    id: "not-a-gid"
  ) {
    id
  }
}
`;

const segmentUpdateVariableMutation = `mutation SegmentMalformedIdUpdateVariable(
  $name: String!
  $id: ID!
) {
  segmentUpdate(
    name: $name
    id: $id
  ) {
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

const segmentUpdateInlineMutation = `mutation SegmentMalformedIdUpdateInline {
  segmentUpdate(
    name: "x"
    id: "not-a-gid"
  ) {
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

const segmentDeleteVariableMutation = `mutation SegmentMalformedIdDeleteVariable(
  $id: ID!
) {
  segmentDelete(id: $id) {
    deletedSegmentId
    userErrors {
      field
      message
    }
  }
}
`;

const segmentDeleteInlineMutation = `mutation SegmentMalformedIdDeleteInline {
  segmentDelete(
    id: "not-a-gid"
  ) {
    deletedSegmentId
    userErrors {
      field
      message
    }
  }
}
`;

function assertTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || !Array.isArray(result.payload.errors)) {
    throw new Error(`${context} did not return expected top-level errors: ${JSON.stringify(result, null, 2)}`);
  }
}

async function captureCase(
  cases: CapturedCase[],
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<void> {
  const response = await runGraphqlRequest(query, variables);
  assertTopLevelErrors(response, name);
  cases.push({
    name,
    request: { query, variables },
    response,
  });
}

const cases: CapturedCase[] = [];

await captureCase(cases, 'segmentReadVariableMalformedId', segmentReadVariableQuery, { id: 'not-a-gid' });
await captureCase(cases, 'segmentReadInlineMalformedId', segmentReadInlineQuery, {});
await captureCase(cases, 'segmentUpdateVariableMalformedId', segmentUpdateVariableMutation, {
  id: 'not-a-gid',
  name: 'x',
});
await captureCase(cases, 'segmentUpdateInlineMalformedId', segmentUpdateInlineMutation, {});
await captureCase(cases, 'segmentDeleteVariableMalformedId', segmentDeleteVariableMutation, { id: 'not-a-gid' });
await captureCase(cases, 'segmentDeleteInlineMalformedId', segmentDeleteInlineMutation, {});

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
        'Live evidence that malformed Segment id values on segment, segmentUpdate, and segmentDelete fail before resolver execution.',
        'Variable malformed ids report the source location of the submitted variable definition, not a fixed line/column.',
        'Inline malformed ids use literal coercion wording and report the source location of the submitted id literal.',
        'Validation-only capture; no live segment setup or cleanup is required.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
