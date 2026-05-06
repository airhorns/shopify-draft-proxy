/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlVariables = Record<string, unknown>;

type CaptureCase = {
  name: string;
  request: {
    document: string;
    variables: GraphqlVariables;
  };
  response: {
    status: number;
    payload: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fieldMessageDocument = `#graphql
  mutation StagedUploadUserErrorsShape($input: [StagedUploadInput!]!) {
    stagedUploadsCreate(input: $input) {
      userErrors {
        field
        message
      }
    }
  }
`;

const codeSelectionDocument = `#graphql
  mutation StagedUploadUserErrorsShapeCode($input: [StagedUploadInput!]!) {
    stagedUploadsCreate(input: $input) {
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const variables = {
  input: [{ resource: 'VIDEO', filename: 'har-795-video.mp4', mimeType: 'video/mp4' }],
};

const requests: Array<{ name: string; document: string; variables: GraphqlVariables }> = [
  {
    name: 'videoMissingFileSizeUserError',
    document: fieldMessageDocument,
    variables,
  },
  {
    name: 'videoMissingFileSizeCodeSelection',
    document: codeSelectionDocument,
    variables,
  },
];

await mkdir(outputDir, { recursive: true });

const cases: CaptureCase[] = [];
for (const request of requests) {
  const response = await runGraphqlRequest(request.document, request.variables);
  cases.push({
    name: request.name,
    request: {
      document: request.document,
      variables: request.variables,
    },
    response,
  });
}

const outputPath = path.join(outputDir, 'media-staged-uploads-create-user-errors-shape.json');
const payload = {
  notes:
    'Captures stagedUploadsCreate userErrors shape in Shopify Admin GraphQL 2026-04: field/message resolves as UserError, while selecting code is rejected by schema validation because UserError does not expose code.',
  storeDomain,
  apiVersion,
  cases,
  upstreamCalls: [],
};

await writeFile(outputPath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      apiVersion,
      storeDomain,
      caseCount: cases.length,
    },
    null,
    2,
  ),
);
