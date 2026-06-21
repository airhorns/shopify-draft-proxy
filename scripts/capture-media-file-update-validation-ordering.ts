/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlVariables = Record<string, unknown>;
type UserError = { field?: string[] | null; message?: string | null; code?: string | null };
type ImageValue = { url?: string | null; width?: number | null; height?: number | null };
type FileNode = {
  id?: string | null;
  __typename?: string | null;
  alt?: string | null;
  createdAt?: string | null;
  fileStatus?: string | null;
  image?: ImageValue | null;
  preview?: { image?: ImageValue | null } | null;
};
type GraphqlPayload<TData> = {
  data?: TData;
  errors?: unknown;
  extensions?: unknown;
};
type FileCreateData = {
  fileCreate?: {
    files?: Array<FileNode | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};
type FileUpdateData = {
  fileUpdate?: {
    files?: Array<FileNode | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};
type FileDeleteData = {
  fileDelete?: {
    deletedFileIds?: string[] | null;
    userErrors?: UserError[] | null;
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

if (apiVersion !== '2026-04') {
  throw new Error(
    `media-file-update-validation-ordering requires SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${apiVersion}`,
  );
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const outputFile = path.join(outputDir, 'media-file-update-validation-ordering.json');
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphql: <TData>(query: string, variables?: GraphqlVariables) => Promise<GraphqlPayload<TData>>;
  runGraphqlRaw: <TData>(
    query: string,
    variables?: GraphqlVariables,
  ) => Promise<{ status: number; payload: GraphqlPayload<TData> }>;
};

const fileSelection = `#graphql
  id
  __typename
  alt
  createdAt
  fileStatus
  ... on MediaImage {
    image {
      url
      width
      height
    }
    preview {
      image {
        url
        width
        height
      }
    }
  }
`;

const fileCreateMutation = `#graphql
  mutation MediaFileUpdateValidationOrderingSeed($files: [FileCreateInput!]!) {
    fileCreate(files: $files) {
      files {
        ${fileSelection}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const fileUpdateMutation = `#graphql
  mutation MediaFileUpdateValidationOrdering($files: [FileUpdateInput!]!) {
    fileUpdate(files: $files) {
      files {
        ${fileSelection}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const fileDeleteMutation = `#graphql
  mutation MediaFileUpdateValidationOrderingCleanup($fileIds: [ID!]!) {
    fileDelete(fileIds: $fileIds) {
      deletedFileIds
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function expectNoUserErrors(label: string, errors: UserError[] | null | undefined): void {
  if (Array.isArray(errors) && errors.length === 0) {
    return;
  }

  throw new Error(`${label} returned userErrors: ${JSON.stringify(errors ?? null, null, 2)}`);
}

function requireId(label: string, node: FileNode | null | undefined): string {
  if (typeof node?.id === 'string' && node.id.length > 0) {
    return node.id;
  }

  throw new Error(`${label} did not return a file id: ${JSON.stringify(node ?? null, null, 2)}`);
}

async function runFileUpdate(variables: GraphqlVariables): Promise<GraphqlPayload<FileUpdateData>> {
  const response = await runGraphqlRaw<FileUpdateData>(fileUpdateMutation, variables);
  if (response.status < 200 || response.status >= 300) {
    throw new Error(`fileUpdate returned HTTP ${response.status}: ${JSON.stringify(response.payload, null, 2)}`);
  }
  return response.payload;
}

function assertFileUpdateErrors(label: string, payload: GraphqlPayload<FileUpdateData>, errors: UserError[]): void {
  const fileUpdate = payload.data?.fileUpdate;
  if (JSON.stringify(fileUpdate?.files ?? null) !== JSON.stringify([])) {
    throw new Error(`${label} returned unexpected files: ${JSON.stringify(fileUpdate?.files ?? null, null, 2)}`);
  }

  if (JSON.stringify(fileUpdate?.userErrors ?? null) !== JSON.stringify(errors)) {
    throw new Error(
      `${label} returned unexpected userErrors: ${JSON.stringify(fileUpdate?.userErrors ?? null, null, 2)}`,
    );
  }
}

const timestamp = Date.now();
const createdFileIds: string[] = [];
const missingImageId = 'gid://shopify/MediaImage/999999999997';
const longAlt = 'a'.repeat(513);
const nonReadyCreateVariables = {
  files: [
    {
      contentType: 'IMAGE',
      filename: `validation-ordering-non-ready-${timestamp}.jpg`,
      originalSource: 'https://placehold.co/600x400.jpg',
      alt: 'Validation ordering non-ready image',
    },
  ],
};
const missingLongAltVariables = {
  files: [{ id: missingImageId, alt: longAlt }],
};
const missingSimultaneousVariables = {
  files: [
    {
      id: missingImageId,
      originalSource: 'https://placehold.co/800x600.jpg',
      previewImageSource: 'https://placehold.co/320x240.jpg',
    },
  ],
};

let capture: Record<string, unknown> | null = null;

try {
  const nonReadyCreate = await runGraphql<FileCreateData>(fileCreateMutation, nonReadyCreateVariables);
  expectNoUserErrors('non-ready fileCreate', nonReadyCreate.data?.fileCreate?.userErrors);
  const nonReadyImageId = requireId('non-ready fileCreate', nonReadyCreate.data?.fileCreate?.files?.[0]);
  createdFileIds.push(nonReadyImageId);

  const nonReadySimultaneousVariables = {
    files: [
      {
        id: nonReadyImageId,
        originalSource: 'https://placehold.co/801x601.jpg',
        previewImageSource: 'https://placehold.co/321x241.jpg',
      },
    ],
  };

  const missingLongAlt = await runFileUpdate(missingLongAltVariables);
  assertFileUpdateErrors('missing id plus long alt', missingLongAlt, [
    {
      field: ['files'],
      message: `File id ["${missingImageId}"] does not exist.`,
      code: 'FILE_DOES_NOT_EXIST',
    },
  ]);

  const nonReadySimultaneous = await runFileUpdate(nonReadySimultaneousVariables);
  assertFileUpdateErrors('non-ready id plus simultaneous sources', nonReadySimultaneous, [
    {
      field: ['files'],
      message: 'Non-ready files cannot be updated.',
      code: 'NON_READY_STATE',
    },
  ]);

  const missingSimultaneous = await runFileUpdate(missingSimultaneousVariables);
  assertFileUpdateErrors('missing id plus simultaneous sources', missingSimultaneous, [
    {
      field: ['files'],
      message: `File id ["${missingImageId}"] does not exist.`,
      code: 'FILE_DOES_NOT_EXIST',
    },
  ]);

  capture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'media-file-update-validation-ordering',
    setup: {
      nonReadyCreate: { variables: nonReadyCreateVariables, response: nonReadyCreate },
    },
    cases: {
      missingLongAlt: { variables: missingLongAltVariables, response: missingLongAlt },
      nonReadySimultaneous: { variables: nonReadySimultaneousVariables, response: nonReadySimultaneous },
      missingSimultaneous: { variables: missingSimultaneousVariables, response: missingSimultaneous },
    },
    upstreamCalls: [],
    notes:
      'Public Admin GraphQL 2026-04 does not expose FileUpdateInput.revertToVersionId, so source/version conflict ordering remains covered by runtime tests and endpoint documentation rather than live public-schema capture.',
  };
} finally {
  let cleanup: GraphqlPayload<FileDeleteData> | null = null;
  if (createdFileIds.length > 0) {
    cleanup = await runGraphql<FileDeleteData>(fileDeleteMutation, { fileIds: createdFileIds });
  }

  if (capture) {
    capture['cleanup'] = {
      variables: { fileIds: createdFileIds },
      response: cleanup,
    };
    await mkdir(outputDir, { recursive: true });
    await writeFile(outputFile, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
    console.log(`Wrote ${outputFile}`);
  }
}
