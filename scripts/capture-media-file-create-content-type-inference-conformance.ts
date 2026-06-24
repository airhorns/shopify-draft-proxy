/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type UserError = { field?: string[] | null; message?: string | null; code?: string | null };
type StagedUploadTarget = {
  url?: string | null;
  resourceUrl?: string | null;
  parameters?: Array<{ name?: string | null; value?: string | null } | null> | null;
};
type GraphqlCapture = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const outputPath = path.join(outputDir, 'file-create-content-type-inference.json');
const requestDir = path.join('config', 'parity-requests', 'media');
const createDocumentPath = path.join(requestDir, 'file-create-content-type-inference-create.graphql');
const filesReadDocumentPath = path.join(requestDir, 'file-create-content-type-inference-files-read.graphql');
const videoNodeDocumentPath = path.join(requestDir, 'file-create-content-type-inference-video-node.graphql');
const genericNodeDocumentPath = path.join(requestDir, 'file-create-content-type-inference-generic-node.graphql');
const videoFixtureSource = 'https://interactive-examples.mdn.mozilla.net/media/cc0-videos/flower.mp4';
const modelFixtureSource =
  'https://raw.githubusercontent.com/KhronosGroup/glTF-Sample-Models/main/2.0/Box/glTF-Binary/Box.glb';
const stagedUploadsCreateDocument = `#graphql
  mutation MediaFileCreateContentTypeInferenceStagedUpload($input: [StagedUploadInput!]!) {
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

async function readRequest(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  return {
    query: trimGraphql(query),
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing required capture value: ${label}`);
  }

  return value;
}

function parameterValue(target: StagedUploadTarget, name: string): string {
  for (const parameter of target.parameters ?? []) {
    if (parameter?.name === name) {
      return requireString(parameter.value, `staged upload parameter ${name}`);
    }
  }

  throw new Error(`staged upload target did not include parameter ${name}: ${JSON.stringify(target, null, 2)}`);
}

function expectNoUserErrors(label: string, errors: unknown): void {
  const userErrors = readArray(errors) as UserError[];
  if (userErrors.length === 0) {
    return;
  }

  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
}

function userErrorsFromRoot(captureResult: GraphqlCapture, rootName: string): UserError[] {
  const root = dataRoot(captureResult, rootName);
  return readArray(root['userErrors']) as UserError[];
}

function dataRoot(captureResult: GraphqlCapture, rootName: string): JsonRecord {
  const payload = readRecord(captureResult.response.payload) ?? {};
  if (payload['errors'] !== undefined) {
    throw new Error(`${rootName} returned top-level errors: ${JSON.stringify(payload['errors'], null, 2)}`);
  }
  const data = readRecord(payload['data']) ?? {};
  return readRecord(data[rootName]) ?? {};
}

function createdFiles(captureResult: GraphqlCapture): JsonRecord[] {
  const root = dataRoot(captureResult, 'fileCreate');
  expectNoUserErrors('fileCreate', root['userErrors']);
  return readArray(root['files'])
    .map(readRecord)
    .filter((file): file is JsonRecord => file !== null);
}

function assertCreatedType(files: JsonRecord[], index: number, typename: string, idPrefix: string): string {
  const file = files[index] ?? {};
  if (file['__typename'] !== typename) {
    throw new Error(`Expected created file ${index} to be ${typename}: ${JSON.stringify(file, null, 2)}`);
  }
  const id = requireString(file['id'], `created file ${index} id`);
  if (!id.startsWith(idPrefix)) {
    throw new Error(`Expected created file ${index} id to start with ${idPrefix}: ${id}`);
  }
  return id;
}

async function downloadFixtureBytes(url: string): Promise<ArrayBuffer> {
  const response = await fetch(url);
  if (response.status < 200 || response.status >= 300) {
    throw new Error(`Failed to download video fixture ${url}: HTTP ${response.status}`);
  }

  return response.arrayBuffer();
}

function stagedUploadTarget(captureResult: GraphqlCapture): StagedUploadTarget {
  const root = dataRoot(captureResult, 'stagedUploadsCreate');
  expectNoUserErrors('stagedUploadsCreate', root['userErrors']);
  const target = readArray(root['stagedTargets']).map(readRecord)[0];
  if (target === null) {
    throw new Error(`stagedUploadsCreate did not return a staged target: ${JSON.stringify(root, null, 2)}`);
  }

  return target as StagedUploadTarget;
}

async function uploadStagedVideo(
  target: StagedUploadTarget,
  filename: string,
  bytes: ArrayBuffer,
): Promise<JsonRecord> {
  const uploadUrl = requireString(target.url, 'staged video upload url');
  const form = new FormData();
  for (const parameter of target.parameters ?? []) {
    if (typeof parameter?.name === 'string' && typeof parameter.value === 'string') {
      form.append(parameter.name, parameter.value);
    }
  }
  form.append('file', new Blob([bytes], { type: 'video/mp4' }), filename);

  const response = await fetch(uploadUrl, {
    method: 'POST',
    body: form,
  });
  const body = await response.text();
  if (response.status < 200 || response.status >= 300) {
    throw new Error(`staged video upload failed with HTTP ${response.status}: ${body}`);
  }

  return {
    status: response.status,
    body,
  };
}

async function createStagedVideoSource(runId: string): Promise<{
  source: string;
  stagedUpload: GraphqlCapture;
  upload: JsonRecord;
}> {
  const filename = `${runId}-video.mp4`;
  const bytes = await downloadFixtureBytes(videoFixtureSource);
  const stagedUpload = await capture(stagedUploadsCreateDocument, {
    input: [
      {
        filename,
        mimeType: 'video/mp4',
        resource: 'VIDEO',
        httpMethod: 'POST',
        fileSize: String(bytes.byteLength),
      },
    ],
  });
  const target = stagedUploadTarget(stagedUpload);
  const upload = await uploadStagedVideo(target, filename, bytes);
  const key = parameterValue(target, 'key');
  const externalVideoId = new URL(requireString(target.resourceUrl, 'staged video resourceUrl')).searchParams.get(
    'external_video_id',
  );
  if (externalVideoId === null || externalVideoId.length === 0) {
    throw new Error(`staged video resourceUrl did not include external_video_id: ${target.resourceUrl ?? null}`);
  }

  return {
    source: `${requireString(target.url, 'staged video upload url').replace(/\/$/u, '')}/${key}?external_video_id=${externalVideoId}`,
    stagedUpload,
    upload,
  };
}

async function cleanupCreatedFiles(fileIds: string[]): Promise<GraphqlCapture | null> {
  if (fileIds.length === 0) {
    return null;
  }

  const cleanupDocument = `#graphql
    mutation MediaFileCreateContentTypeInferenceCleanup($fileIds: [ID!]!) {
      fileDelete(fileIds: $fileIds) {
        deletedFileIds
        userErrors { field message code }
      }
    }
  `;
  let lastCleanup: GraphqlCapture | null = null;
  for (let attempt = 0; attempt < 30; attempt += 1) {
    lastCleanup = await capture(cleanupDocument, { fileIds });
    const userErrors = userErrorsFromRoot(lastCleanup, 'fileDelete');
    if (userErrors.length === 0) {
      return lastCleanup;
    }
    const retryable = userErrors.some(
      (error) =>
        error.code === 'FILE_LOCKED' ||
        (typeof error.message === 'string' && error.message.includes('pending operation')),
    );
    if (!retryable) {
      return lastCleanup;
    }
    await delay(5000);
  }

  return lastCleanup;
}

const createDocument = await readRequest(createDocumentPath);
const filesReadDocument = await readRequest(filesReadDocumentPath);
const videoNodeDocument = await readRequest(videoNodeDocumentPath);
const genericNodeDocument = await readRequest(genericNodeDocumentPath);

const runId = `media-file-content-type-inference-${Date.now()}`;
const videoSetup = await createStagedVideoSource(runId);
const createVariables = {
  files: [
    {
      originalSource: 'https://placehold.co/600x400.png',
      filename: `${runId}-image.png`,
      alt: `${runId} image`,
    },
    {
      originalSource: videoSetup.source,
      filename: `${runId}-video.mp4`,
      alt: `${runId} video`,
    },
    {
      originalSource: 'https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf',
      filename: `${runId}-document.pdf`,
      alt: `${runId} document`,
    },
    {
      originalSource: 'https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy',
      filename: `${runId}-extensionless`,
      alt: `${runId} extensionless`,
    },
    {
      originalSource: modelFixtureSource,
      filename: `${runId}-model.glb`,
      alt: `${runId} model glb`,
    },
  ],
};

const createdFileIds: string[] = [];
let fixture: JsonRecord | null = null;

try {
  const create = await capture(createDocument, createVariables);
  const files = createdFiles(create);
  const imageId = assertCreatedType(files, 0, 'MediaImage', 'gid://shopify/MediaImage/');
  const videoId = assertCreatedType(files, 1, 'Video', 'gid://shopify/Video/');
  const documentId = assertCreatedType(files, 2, 'GenericFile', 'gid://shopify/GenericFile/');
  const extensionlessId = assertCreatedType(files, 3, 'GenericFile', 'gid://shopify/GenericFile/');
  const modelId = assertCreatedType(files, 4, 'GenericFile', 'gid://shopify/GenericFile/');
  createdFileIds.push(imageId, videoId, documentId, extensionlessId, modelId);

  const filesRead = await capture(filesReadDocument);
  const videoNode = await capture(videoNodeDocument, { id: videoId });
  const documentNode = await capture(genericNodeDocument, { id: documentId });
  const extensionlessNode = await capture(genericNodeDocument, { id: extensionlessId });
  const modelNode = await capture(genericNodeDocument, { id: modelId });

  fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'file_create_content_type_inference',
    notes: [
      'Captures fileCreate with omitted contentType for deterministic image, video, document, and extensionless source URLs.',
      'Shopify derives image/video types from extension/MIME and defaults document, 3D model, and undetectable sources to GenericFile; this script uses extension-bearing URLs except the explicit extensionless branch to avoid relying on proxy-side network inference.',
      'The video branch uploads a real MP4 to Shopify staged storage first, then passes a Shopify-accepted .mp4 staged-video URL with external_video_id to fileCreate without contentType.',
    ],
    setup: {
      videoFixtureSource,
      modelFixtureSource,
      videoStagedUpload: videoSetup.stagedUpload,
      videoUpload: videoSetup.upload,
    },
    create,
    reads: {
      filesRead,
      videoNode,
      documentNode,
      extensionlessNode,
      modelNode,
    },
    upstreamCalls: [],
  };
} finally {
  let cleanup: GraphqlCapture | null = null;
  cleanup = await cleanupCreatedFiles(createdFileIds);

  if (fixture !== null) {
    fixture['cleanup'] = {
      variables: { fileIds: createdFileIds },
      response: cleanup,
    };
    await mkdir(outputDir, { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
    console.log(`wrote ${outputPath}`);
  }
}
