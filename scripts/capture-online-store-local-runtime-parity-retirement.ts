/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  requestPath: string;
  request: {
    query: string;
    variables: JsonRecord;
  };
  status: number;
  response: unknown;
};

type RecordedCall = {
  operationName: string;
  variables: JsonRecord;
  query: string;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'online-store');
const outputPath = path.join(outputDir, 'online-store-salvaged-parity-recordings.json');
const requestRoot = path.join('config', 'parity-requests', 'online-store');
const captures: Record<string, Record<string, GraphqlCapture>> = {};
const cleanup: Record<string, GraphqlCapture[]> = {};
const upstreamCalls: RecordedCall[] = [];

function requestPath(name: string): string {
  return path.join(requestRoot, name);
}

async function readGraphql(relativePath: string): Promise<string> {
  return await readFile(relativePath, 'utf8');
}

function groupFor(scenarioId: string): Record<string, GraphqlCapture> {
  captures[scenarioId] ??= {};
  return captures[scenarioId];
}

function cleanupFor(label: string): GraphqlCapture[] {
  cleanup[label] ??= [];
  return cleanup[label];
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function pathValue(value: unknown, pathSegments: string[]): unknown {
  return pathSegments.reduce<unknown>((cursor, segment) => {
    if (!isRecord(cursor)) {
      return undefined;
    }
    return cursor[segment];
  }, value);
}

function readId(capture: GraphqlCapture, pathSegments: string[]): string | null {
  const value = pathValue(capture.response, pathSegments);
  return typeof value === 'string' && value.length > 0 ? value : null;
}

async function captureGraphql(
  scenarioId: string,
  name: string,
  relativeRequestPath: string,
  variables: JsonRecord = {},
): Promise<GraphqlCapture> {
  const query = await readGraphql(relativeRequestPath);
  const result = await runGraphqlRaw(query, variables);
  const capture: GraphqlCapture = {
    requestPath: relativeRequestPath,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
  groupFor(scenarioId)[name] = capture;
  upstreamCalls.push({
    operationName: name,
    variables,
    query,
    response: {
      status: result.status,
      body: result.payload,
    },
  });
  return capture;
}

async function captureInline(
  label: string,
  name: string,
  query: string,
  variables: JsonRecord = {},
): Promise<GraphqlCapture> {
  const result = await runGraphqlRaw(query, variables);
  const capture: GraphqlCapture = {
    requestPath: '<inline-cleanup>',
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
  cleanupFor(label).push(capture);
  upstreamCalls.push({
    operationName: name,
    variables,
    query,
    response: {
      status: result.status,
      body: result.payload,
    },
  });
  return capture;
}

async function cleanupById(
  label: string,
  kind: 'page' | 'blog' | 'theme' | 'scriptTag' | 'webPixel',
  id: string | null,
) {
  if (!id) {
    return;
  }
  const documents = {
    page: `#graphql
      mutation OnlineStoreSalvagePageCleanup($id: ID!) {
        pageDelete(id: $id) { deletedPageId userErrors { field message code } }
      }
    `,
    blog: `#graphql
      mutation OnlineStoreSalvageBlogCleanup($id: ID!) {
        blogDelete(id: $id) { deletedBlogId userErrors { field message code } }
      }
    `,
    theme: `#graphql
      mutation OnlineStoreSalvageThemeCleanup($id: ID!) {
        themeDelete(id: $id) { deletedThemeId userErrors { field message code } }
      }
    `,
    scriptTag: `#graphql
      mutation OnlineStoreSalvageScriptTagCleanup($id: ID!) {
        scriptTagDelete(id: $id) { deletedScriptTagId userErrors { field message code } }
      }
    `,
    webPixel: `#graphql
      mutation OnlineStoreSalvageWebPixelCleanup($id: ID!) {
        webPixelDelete(id: $id) { deletedWebPixelId userErrors { field message code } }
      }
    `,
  };
  await captureInline(label, `${label}-${kind}-cleanup`, documents[kind], { id });
}

async function cleanupStorefrontAccessToken(label: string, id: string | null): Promise<void> {
  if (!id) {
    return;
  }
  const document = `#graphql
    mutation OnlineStoreSalvageStorefrontAccessTokenCleanup($input: StorefrontAccessTokenDeleteInput!) {
      storefrontAccessTokenDelete(input: $input) {
        deletedStorefrontAccessTokenId
        userErrors { field message }
      }
    }
  `;
  await captureInline(label, `${label}-storefront-access-token-cleanup`, document, { input: { id } });
}

async function captureSchemaAndAccessBranches(): Promise<void> {
  await captureGraphql(
    'event-bridge-server-pixel-update-arn-format',
    'malformedArn',
    requestPath('event_bridge_server_pixel_update_malformed_arn.graphql'),
  );
  await captureGraphql(
    'event-bridge-server-pixel-update-arn-format',
    'blankArn',
    requestPath('event_bridge_server_pixel_update_blank_arn.graphql'),
  );
  await captureGraphql(
    'event-bridge-server-pixel-update-arn-format',
    'missingArn',
    requestPath('event_bridge_server_pixel_update_missing_arn.graphql'),
  );

  await captureGraphql(
    'pub-sub-server-pixel-update-blank-required',
    'blankProject',
    requestPath('pub_sub_server_pixel_update_blank_project.graphql'),
  );
  await captureGraphql(
    'pub-sub-server-pixel-update-blank-required',
    'blankTopic',
    requestPath('pub_sub_server_pixel_update_blank_topic.graphql'),
  );
  await captureGraphql(
    'pub-sub-server-pixel-update-blank-required',
    'missingProject',
    requestPath('pub_sub_server_pixel_update_missing_project.graphql'),
  );
  await captureGraphql(
    'pub-sub-server-pixel-update-blank-required',
    'missingTopic',
    requestPath('pub_sub_server_pixel_update_missing_topic.graphql'),
  );

  await captureGraphql(
    'server-pixel-endpoint-update-no-pixel',
    'endpointUpdateNoPixel',
    requestPath('server_pixel_endpoint_update_no_pixel.graphql'),
  );
  await captureGraphql(
    'integration-delete-not-found-codes',
    'deleteNotFoundCodes',
    requestPath('integration-delete-not-found-codes.graphql'),
  );
}

async function captureContentBranches(suffix: string): Promise<void> {
  const pageTitle = `Salvaged Default Publish Page ${suffix}`;
  const pageCreate = await captureGraphql(
    'online-store-page-default-publish-local-staging',
    'create',
    requestPath('online-store-page-default-publish-create.graphql'),
    {
      page: {
        title: pageTitle,
        body: `<p>${pageTitle}</p>`,
      },
    },
  );
  const pageId = readId(pageCreate, ['data', 'pageCreate', 'page', 'id']);
  await captureGraphql(
    'online-store-page-default-publish-local-staging',
    'readAfterCreate',
    requestPath('online-store-page-default-publish-read.graphql'),
    {
      id: pageId ?? 'gid://shopify/Page/0',
      publishedQuery: `published_status:published title:'${pageTitle}'`,
    },
  );
  await cleanupById('online-store-page-default-publish-local-staging', 'page', pageId);

  const blogTitle = `Salvaged Commentable Blog ${suffix}`;
  const blogCreate = await captureGraphql(
    'online-store-blog-commentable-local-staging',
    'create',
    requestPath('online-store-blog-commentable-create.graphql'),
    {
      blog: {
        title: blogTitle,
        commentPolicy: 'CLOSED',
      },
    },
  );
  const blogId = readId(blogCreate, ['data', 'blogCreate', 'blog', 'id']);
  await captureGraphql(
    'online-store-blog-commentable-local-staging',
    'updateCommentable',
    requestPath('online-store-blog-commentable-update.graphql'),
    {
      id: blogId ?? 'gid://shopify/Blog/0',
      blog: {
        commentable: 'MODERATE',
      },
    },
  );
  await captureGraphql(
    'online-store-blog-commentable-local-staging',
    'readAfterUpdate',
    requestPath('online-store-blog-commentable-read.graphql'),
    {
      id: blogId ?? 'gid://shopify/Blog/0',
    },
  );
  await captureGraphql(
    'online-store-blog-commentable-local-staging',
    'invalidCommentable',
    requestPath('online-store-blog-commentable-update.graphql'),
    {
      id: blogId ?? 'gid://shopify/Blog/0',
      blog: {
        commentable: 'INVALID_VALUE',
      },
    },
  );
  await cleanupById('online-store-blog-commentable-local-staging', 'blog', blogId);
}

async function captureCommentBranches(): Promise<void> {
  const variables = { id: 'gid://shopify/Comment/9999999999' };
  await captureGraphql(
    'comment-moderation-status-enums',
    'spamUnknown',
    requestPath('comment-moderation-status-spam.graphql'),
    variables,
  );
  await captureGraphql(
    'comment-moderation-status-enums',
    'notSpamUnknown',
    requestPath('comment-moderation-status-not-spam.graphql'),
    variables,
  );
  await captureGraphql(
    'comment-moderation-status-enums',
    'approveUnknown',
    requestPath('comment-moderation-status-approve.graphql'),
    variables,
  );
  await captureGraphql(
    'comment-moderation-status-enums',
    'readUnknown',
    requestPath('comment-moderation-status-read.graphql'),
    variables,
  );
  await captureGraphql(
    'comment-moderation-status-enums',
    'deleteUnknown',
    requestPath('comment-moderation-status-delete.graphql'),
    variables,
  );
}

async function captureScriptTagBranches(): Promise<void> {
  await captureGraphql(
    'online-store/script-tag-create-validates-src',
    'createValidation',
    requestPath('script-tag-create-validates-src.graphql'),
  );
  const create = await captureGraphql(
    'online-store/script-tag-update-validation',
    'create',
    requestPath('script-tag-update-validation-create.graphql'),
  );
  await captureGraphql(
    'online-store/script-tag-update-validation',
    'validation',
    requestPath('script-tag-update-validation-errors.graphql'),
  );
  await captureGraphql(
    'online-store/script-tag-update-validation',
    'eventForceOnload',
    requestPath('script-tag-update-event-force-onload.graphql'),
  );
  await captureGraphql(
    'online-store/script-tag-update-validation',
    'readback',
    requestPath('script-tag-update-readback.graphql'),
  );
  await cleanupById(
    'online-store/script-tag-update-validation',
    'scriptTag',
    readId(create, ['data', 'scriptTagCreate', 'scriptTag', 'id']),
  );
}

async function captureMobilePlatformApplicationBranches(): Promise<void> {
  const longApplicationId = `com.example.${'a'.repeat(101)}`;
  const longAppClipApplicationId = `com.example.clip.${'b'.repeat(256)}`;
  await captureGraphql(
    'mobile-platform-application-create-blank-application-id',
    'blankApplicationId',
    requestPath('mobile_platform_application_create_blank_application_id.graphql'),
  );
  await captureGraphql(
    'mobile-platform-application-create-model-validation',
    'modelValidation',
    requestPath('mobile_platform_application_create_model_validation.graphql'),
    {
      longApplicationId,
      longAppClipApplicationId,
    },
  );
  await captureGraphql(
    'mobile-platform-application-create-model-validation',
    'requiresOnePlatform',
    requestPath('mobile_platform_application_create_requires_one_platform.graphql'),
  );
  await captureGraphql(
    'mobile-platform-application-create-model-validation',
    'duplicateAndroidProbe',
    requestPath('mobile_platform_application_create_duplicate_android.graphql'),
  );
  await captureGraphql(
    'mobile-platform-application-create-model-validation',
    'duplicateAppleProbe',
    requestPath('mobile_platform_application_create_duplicate_apple.graphql'),
  );

  const create = await captureGraphql(
    'online-store/mobile-platform-application-update-local-staging',
    'create',
    requestPath('mobile-platform-application-update-create.graphql'),
  );
  const appleId =
    readId(create, ['data', 'appleCreate', 'mobilePlatformApplication', 'id']) ??
    'gid://shopify/MobilePlatformApplication/9999999998';
  const androidId =
    readId(create, ['data', 'androidCreate', 'mobilePlatformApplication', 'id']) ??
    'gid://shopify/MobilePlatformApplication/9999999999';
  await captureGraphql(
    'online-store/mobile-platform-application-update-local-staging',
    'appleUpdate',
    requestPath('mobile-platform-application-update-apple.graphql'),
    { id: appleId },
  );
  await captureGraphql(
    'online-store/mobile-platform-application-update-local-staging',
    'appleRead',
    requestPath('mobile-platform-application-update-read-apple.graphql'),
    { id: appleId },
  );
  await captureGraphql(
    'online-store/mobile-platform-application-update-local-staging',
    'androidUpdate',
    requestPath('mobile-platform-application-update-android.graphql'),
    { id: androidId },
  );
  await captureGraphql(
    'online-store/mobile-platform-application-update-local-staging',
    'androidRead',
    requestPath('mobile-platform-application-update-read-android.graphql'),
    { id: androidId },
  );
  await captureGraphql(
    'online-store/mobile-platform-application-update-local-staging',
    'validation',
    requestPath('mobile-platform-application-update-validation.graphql'),
    {
      appleId,
      androidId,
      missingId: 'gid://shopify/MobilePlatformApplication/9999999997',
    },
  );
  await captureGraphql(
    'online-store/mobile-platform-application-update-local-staging',
    'readAfterValidation',
    requestPath('mobile-platform-application-update-read-after-validation.graphql'),
    { appleId, androidId },
  );
}

async function capturePixelAndStorefrontTokenBranches(suffix: string): Promise<void> {
  const webPixelVariables = { webPixel: { settings: JSON.stringify({ accountID: `salvage-${suffix}` }) } };
  const firstWebPixel = await captureGraphql(
    'web-pixel-create-duplicate-returns-taken',
    'firstCreate',
    requestPath('web-pixel-create-duplicate-returns-taken.graphql'),
    webPixelVariables,
  );
  await captureGraphql(
    'web-pixel-create-duplicate-returns-taken',
    'secondCreate',
    requestPath('web-pixel-create-duplicate-returns-taken.graphql'),
    webPixelVariables,
  );
  await cleanupById(
    'web-pixel-create-duplicate-returns-taken',
    'webPixel',
    readId(firstWebPixel, ['data', 'webPixelCreate', 'webPixel', 'id']),
  );

  await captureGraphql(
    'web-pixel-update-validation-local-runtime',
    'updateValidation',
    requestPath('web-pixel-update-validation-local-runtime.graphql'),
  );

  const tokenInput = { input: { title: `Salvaged token ${suffix}` } };
  const tokenCreate = await captureGraphql(
    'storefront-access-token-local-staging',
    'create',
    requestPath('storefront-access-token-create.graphql'),
    tokenInput,
  );
  await captureGraphql(
    'storefront-access-token-create-shape',
    'createShape',
    requestPath('storefront-access-token-create-shape.graphql'),
    tokenInput,
  );
  await captureGraphql(
    'storefront-access-token-local-staging',
    'readAfterCreate',
    requestPath('storefront-access-token-read.graphql'),
  );
  const tokenId = readId(tokenCreate, ['data', 'storefrontAccessTokenCreate', 'storefrontAccessToken', 'id']);
  if (tokenId) {
    await captureGraphql(
      'storefront-access-token-local-staging',
      'delete',
      requestPath('storefront-access-token-delete.graphql'),
      { input: { id: tokenId } },
    );
  }
  await cleanupStorefrontAccessToken('storefront-access-token-local-staging', tokenId);
  await captureGraphql(
    'storefront-access-token-local-staging',
    'readAfterDelete',
    requestPath('storefront-access-token-read.graphql'),
  );
}

async function captureThemeBranches(suffix: string): Promise<void> {
  const themeVariables = {
    source: 'https://example.com/salvage-theme.zip',
    name: `Salvaged theme ${suffix}`,
  };
  const createRenamed = await captureGraphql(
    'online-store/theme-update-validation-local-runtime',
    'createRenamed',
    requestPath('theme-update-validation-create-unpublished.graphql'),
    themeVariables,
  );
  const renamedThemeId =
    readId(createRenamed, ['data', 'themeCreate', 'theme', 'id']) ?? 'gid://shopify/OnlineStoreTheme/9999999998';
  await captureGraphql(
    'online-store/theme-update-validation-local-runtime',
    'rename',
    requestPath('theme-update-valid-rename.graphql'),
    { id: renamedThemeId },
  );
  await captureGraphql(
    'online-store/theme-update-validation-local-runtime',
    'blankName',
    requestPath('theme-update-blank-name-invalid.graphql'),
    { id: renamedThemeId },
  );
  await cleanupById('online-store/theme-update-validation-local-runtime', 'theme', renamedThemeId);

  const locked = await captureGraphql(
    'online-store/theme-update-validation-local-runtime',
    'createLocked',
    requestPath('theme-update-validation-create-locked.graphql'),
    {
      source: 'https://example.com/salvage-locked-theme.zip',
      name: `Salvaged locked theme ${suffix}`,
    },
  );
  const lockedThemeId =
    readId(locked, ['data', 'themeCreate', 'theme', 'id']) ?? 'gid://shopify/OnlineStoreTheme/9999999997';
  await captureGraphql(
    'online-store/theme-update-validation-local-runtime',
    'lockedThemeRejected',
    requestPath('theme-update-locked-rejected.graphql'),
    { id: lockedThemeId },
  );
  await captureGraphql(
    'online-store/theme-update-validation-local-runtime',
    'readback',
    requestPath('theme-update-validation-readback.graphql'),
    { id: renamedThemeId },
  );
  await cleanupById('online-store/theme-update-validation-local-runtime', 'theme', lockedThemeId);

  const themeFilesJob = await captureGraphql(
    'online-store/theme-files-upsert-job',
    'jobPayload',
    requestPath('theme-files-upsert-job.graphql'),
  );
  await cleanupById(
    'online-store/theme-files-upsert-job',
    'theme',
    readId(themeFilesJob, ['data', 'themeCreate', 'theme', 'id']),
  );
  const checksums = await captureGraphql(
    'online-store/theme-files-checksums-and-validation',
    'checksumsAndValidation',
    requestPath('theme-files-checksums-and-validation.graphql'),
  );
  await cleanupById(
    'online-store/theme-files-checksums-and-validation',
    'theme',
    readId(checksums, ['data', 'themeCreate', 'theme', 'id']),
  );

  const currentMain = await captureGraphql(
    'online-store/theme-publish-demotes-previous-main',
    'createMain',
    requestPath('theme-publish-create-main.graphql'),
    {
      source: 'https://example.com/salvage-current-main.zip',
      name: `Salvaged current main ${suffix}`,
    },
  );
  const previousId = readId(currentMain, ['data', 'themeCreate', 'theme', 'id']);
  const next = await captureGraphql(
    'online-store/theme-publish-demotes-previous-main',
    'createUnpublished',
    requestPath('theme-publish-create-unpublished.graphql'),
    {
      source: 'https://example.com/salvage-next-main.zip',
      name: `Salvaged next main ${suffix}`,
    },
  );
  const nextId = readId(next, ['data', 'themeCreate', 'theme', 'id']) ?? 'gid://shopify/OnlineStoreTheme/9999999996';
  await captureGraphql(
    'online-store/theme-publish-demotes-previous-main',
    'publish',
    requestPath('theme-publish-publish.graphql'),
    { id: nextId },
  );
  await captureGraphql(
    'online-store/theme-publish-demotes-previous-main',
    'read',
    requestPath('theme-publish-read.graphql'),
    { previousId: previousId ?? 'gid://shopify/OnlineStoreTheme/9999999995' },
  );
  await cleanupById('online-store/theme-publish-demotes-previous-main', 'theme', nextId);
  await cleanupById('online-store/theme-publish-demotes-previous-main', 'theme', previousId);
}

async function captureIntegrationBranches(suffix: string): Promise<void> {
  const integration = await captureGraphql(
    'online-store-integrations-local-staging',
    'integrations',
    requestPath('online-store-integrations-local-staging.graphql'),
    {
      source: 'https://example.com/salvage-integrations-theme.zip',
      themeName: `Salvaged integrations theme ${suffix}`,
      scriptTag: {
        src: `https://cdn.example.com/salvage-${suffix}.js`,
        displayScope: 'ONLINE_STORE',
        cache: true,
      },
      webPixel: {
        settings: JSON.stringify({ accountId: `salvage-${suffix}` }),
      },
      token: {
        title: `Salvaged integrations token ${suffix}`,
      },
      mobile: {
        android: {
          applicationId: `com.example.salvage.${suffix.toLowerCase()}`,
          appLinksEnabled: true,
          sha256CertFingerprints: ['AA:BB'],
        },
      },
    },
  );
  await cleanupById(
    'online-store-integrations-local-staging',
    'theme',
    readId(integration, ['data', 'themeCreate', 'theme', 'id']),
  );
  await cleanupById(
    'online-store-integrations-local-staging',
    'scriptTag',
    readId(integration, ['data', 'scriptTagCreate', 'scriptTag', 'id']),
  );
  await cleanupById(
    'online-store-integrations-local-staging',
    'webPixel',
    readId(integration, ['data', 'webPixelCreate', 'webPixel', 'id']),
  );
  await cleanupStorefrontAccessToken(
    'online-store-integrations-local-staging',
    readId(integration, ['data', 'storefrontAccessTokenCreate', 'storefrontAccessToken', 'id']),
  );

  const dispatch = await captureGraphql(
    'online-store-integration-root-dispatch-local-runtime',
    'create',
    requestPath('online-store-integration-root-dispatch-local-runtime.graphql'),
  );
  const scriptId =
    readId(dispatch, ['data', 'createdScript', 'scriptTag', 'id']) ?? 'gid://shopify/ScriptTag/9999999998';
  await captureGraphql(
    'online-store-integration-root-dispatch-local-runtime',
    'delete',
    requestPath('online-store-integration-root-dispatch-delete-local-runtime.graphql'),
    { scriptId },
  );
  await captureGraphql(
    'online-store-integration-root-dispatch-local-runtime',
    'readAfterDelete',
    requestPath('online-store-integration-root-dispatch-read-local-runtime.graphql'),
    { scriptId },
  );
  await cleanupById(
    'online-store-integration-root-dispatch-local-runtime',
    'theme',
    readId(dispatch, ['data', 'createdTheme', 'theme', 'id']),
  );
  await cleanupById('online-store-integration-root-dispatch-local-runtime', 'scriptTag', scriptId);
}

const suffix = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

await captureSchemaAndAccessBranches();
await captureContentBranches(suffix);
await captureCommentBranches();
await captureScriptTagBranches();
await captureMobilePlatformApplicationBranches();
await capturePixelAndStorefrontTokenBranches(suffix);
await captureThemeBranches(suffix);
await captureIntegrationBranches(suffix);

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'online-store-salvaged-parity-recordings',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      summary:
        'Live Shopify Admin GraphQL capture attempts for the retired online-store synthetic parity scenarios. The fixture records the exact GraphQL documents, variables, HTTP status, and response payload returned by the disposable conformance shop for each salvage candidate.',
      scenarioIds: Object.keys(captures).sort(),
      captures,
      cleanup,
      upstreamCalls,
      evidence: {
        source: 'live-shopify',
        notes: [
          `Captured against ${storeDomain} using API ${apiVersion}.`,
          'Successful disposable content/token/pixel/theme/script resources are cleaned up when their create responses return IDs.',
          'Access-denied, schema-validation, and app-context responses are retained as live Shopify evidence for scenarios whose local proxy behavior remains enforced by Rust integration tests.',
        ],
      },
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
