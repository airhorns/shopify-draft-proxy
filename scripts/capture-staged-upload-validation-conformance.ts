/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlVariables = Record<string, unknown>;
type StagedUploadsCreateData = {
  stagedUploadsCreate?: {
    stagedTargets?: Array<{
      url?: string | null;
      resourceUrl?: string | null;
      parameters?: Array<{ name?: string | null; value?: string | null } | null> | null;
    } | null> | null;
    userErrors?: Array<{ field?: string[] | null; message?: string | null } | null> | null;
  } | null;
};

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

const stagedUploadsCreateMutation = `#graphql
  mutation StagedUploadValidation($input: [StagedUploadInput!]!) {
    stagedUploadsCreate(input: $input) {
      stagedTargets {
        url
        resourceUrl
        parameters {
          name
          value
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const requests: Array<{ name: string; variables: GraphqlVariables }> = [
  {
    name: 'videoMissingFileSize',
    variables: {
      input: [{ resource: 'VIDEO', filename: 'har-704-video.mp4', mimeType: 'video/mp4' }],
    },
  },
  {
    name: 'imagePng',
    variables: {
      input: [{ resource: 'IMAGE', filename: 'har-704-image.png', mimeType: 'image/png', httpMethod: 'POST' }],
    },
  },
  {
    name: 'unknownResource',
    variables: {
      input: [{ resource: 'BANANA', filename: 'har-704-banana', mimeType: 'x/x' }],
    },
  },
  {
    name: 'imageUnsupportedMime',
    variables: {
      input: [
        {
          resource: 'IMAGE',
          filename: 'har-704-image.exe',
          mimeType: 'application/x-msdownload',
          httpMethod: 'POST',
        },
      ],
    },
  },
  {
    name: 'model3dGlb',
    variables: {
      input: [
        {
          resource: 'MODEL_3D',
          filename: 'har-704-model.glb',
          mimeType: 'model/gltf-binary',
          httpMethod: 'POST',
          fileSize: '1024',
        },
      ],
    },
  },
];

await mkdir(outputDir, { recursive: true });

const cases: CaptureCase[] = [];
for (const request of requests) {
  const response = await runGraphqlRequest<StagedUploadsCreateData>(stagedUploadsCreateMutation, request.variables);
  cases.push({
    name: request.name,
    request: {
      document: stagedUploadsCreateMutation,
      variables: request.variables,
    },
    response,
  });
}

const outputPath = path.join(outputDir, 'media-staged-uploads-create-validation.json');
const payload = {
  notes:
    'HAR-704 captures stagedUploadsCreate validation branches and success target metadata. No upload bytes are sent; this fixture records resolver validation, enum coercion, and signed target metadata only.',
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
