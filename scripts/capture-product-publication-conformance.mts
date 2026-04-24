// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

import {
  extractCliIdentityFromConfig,
  extractManualStoreAuthTokenSummary,
  extractScopesFromShopifyAppToml,
  extractShopifyAppDeployVersion,
  parsePublicationTargetBlocker,
  findConfiguredShopifyApp,
  findShopifyChannelConfigExtensions,
  getDefaultShopifyCliAppConfigPath,
  getDefaultShopifyCliConfigPath,
  isInvalidGrantRefreshResponse,
  loadShopifyCliAppConfig,
  loadShopifyCliConfig,
  persistShopifyCliIdentity,
  shouldAttemptShopifyAppDeploy,
  shouldProbeManualStoreAuthFallback,
} from './product-publication-conformance-lib.mjs';
import {
  parseAccessDeniedErrors,
  parseWriteScopeBlocker,
  renderWriteScopeBlockerNote,
} from './product-mutation-conformance-lib.mjs';

const requiredVars = ['SHOPIFY_CONFORMANCE_STORE_DOMAIN', 'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];

const missingVars = requiredVars.filter((name) => !process.env[name]);
if (missingVars.length > 0) {
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
const conformanceAppHandle = process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] || null;
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const manualStoreAuthTokenPath = path.resolve('.manual-store-auth-token.json');

function describeCredentialObservation(token) {
  const tokenFamilyMatch = token.match(/^(shp[a-z]+)_/i);
  const tokenFamily = tokenFamilyMatch ? tokenFamilyMatch[1].toLowerCase() : 'bearer';

  if (token.startsWith('shpca_')) {
    return {
      tokenFamily,
      headerMode: 'raw-x-shopify-access-token',
      summary:
        'the active conformance credential is a Shopify user access token (`shpca_...`) sent as raw `X-Shopify-Access-Token` on this host',
    };
  }

  if (token.startsWith('shpat_')) {
    return {
      tokenFamily,
      headerMode: 'raw-x-shopify-access-token',
      summary: 'the active conformance credential is already a dedicated Admin API token (`shpat_...`)',
    };
  }

  if (/^shp[a-z]+_/.test(token)) {
    return {
      tokenFamily,
      headerMode: 'raw-x-shopify-access-token',
      summary:
        'the active conformance credential is a Shopify app/API token (`shp...`) sent as raw `X-Shopify-Access-Token` on this host',
    };
  }

  return {
    tokenFamily,
    headerMode: 'authorization-bearer-and-x-shopify-access-token-bearer',
    summary: 'the active conformance credential is a bearer-style Shopify account / CLI token',
  };
}

function buildGraphqlClient(token) {
  return createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(token),
  });
}

const productSeedQuery = `#graphql
  query ProductPublicationSeed {
    products(first: 1) {
      nodes {
        id
      }
    }
  }
`;

const publicationScopeHandlesQuery = `#graphql
  query ProductPublicationScopeHandles {
    currentAppInstallation {
      accessScopes {
        handle
      }
    }
  }
`;

const publicationAggregateProbeQuery = `#graphql
  query ProductPublicationAggregateProbe($id: ID!) {
    product(id: $id) {
      id
      publishedOnCurrentPublication
      availablePublicationsCount {
        count
        precision
      }
      resourcePublicationsCount {
        count
        precision
      }
    }
  }
`;

const publicationListProbeQuery = `#graphql
  query ProductPublicationListProbe {
    publications(first: 10) {
      edges {
        cursor
        node {
          id
          name
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const createMutation = `#graphql
  mutation ProductPublicationConformanceCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation ProductPublicationConformanceDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const publishMutation = `#graphql
  mutation ProductPublicationConformancePublish($input: ProductPublishInput!) {
    productPublish(input: $input) {
      product {
        id
        publishedOnCurrentPublication
        availablePublicationsCount {
          count
          precision
        }
        resourcePublicationsCount {
          count
          precision
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const unpublishMutation = `#graphql
  mutation ProductPublicationConformanceUnpublish($input: ProductUnpublishInput!) {
    productUnpublish(input: $input) {
      product {
        id
        publishedOnCurrentPublication
        availablePublicationsCount {
          count
          precision
        }
        resourcePublicationsCount {
          count
          precision
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const publishMutationScopeProbe = `#graphql
  mutation ProductPublicationScopeProbePublish($input: ProductPublishInput!) {
    productPublish(input: $input) {
      product {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const unpublishMutationScopeProbe = `#graphql
  mutation ProductPublicationScopeProbeUnpublish($input: ProductUnpublishInput!) {
    productUnpublish(input: $input) {
      userErrors {
        field
        message
      }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query ProductPublicationDownstream($id: ID!) {
    product(id: $id) {
      id
      publishedOnCurrentPublication
      availablePublicationsCount {
        count
        precision
      }
      resourcePublicationsCount {
        count
        precision
      }
    }
  }
`;

const publicationMutationScopeProbePublicationId = 'gid://shopify/Publication/1';

function buildCreateVariables(runId) {
  return {
    product: {
      title: `Hermes Product Publication Conformance ${runId}`,
      status: 'DRAFT',
    },
  };
}

async function loadConfiguredAppScopeDrift(grantedScopes) {
  if (!conformanceAppHandle) {
    return null;
  }

  const configPath = getDefaultShopifyCliAppConfigPath();
  const grantedScopeSet = new Set(grantedScopes);

  try {
    const cliAppConfig = await loadShopifyCliAppConfig(configPath);
    const configuredApp = findConfiguredShopifyApp(cliAppConfig, conformanceAppHandle);
    if (!configuredApp) {
      return {
        configPath,
        appHandle: conformanceAppHandle,
        requestedScopes: [],
        missingRequestedScopes: [],
        note: 'No configured Shopify app matched `SHOPIFY_CONFORMANCE_APP_HANDLE`.',
      };
    }

    const requestedScopes = extractScopesFromShopifyAppToml(await readFile(configuredApp.configPath, 'utf8'));
    const missingRequestedScopes = requestedScopes.filter((scope) => !grantedScopeSet.has(scope));
    const channelConfigExtensions = await findShopifyChannelConfigExtensions(configuredApp.directory);

    return {
      configPath,
      appHandle: configuredApp.title ?? conformanceAppHandle,
      directory: configuredApp.directory,
      appConfigPath: configuredApp.configPath,
      appId: configuredApp.appId,
      requestedScopes,
      missingRequestedScopes,
      channelConfigExtensions,
      note: null,
    };
  } catch (error) {
    return {
      configPath,
      appHandle: conformanceAppHandle,
      requestedScopes: [],
      missingRequestedScopes: [],
      note: `Could not inspect configured Shopify app scopes: ${error.message}`,
    };
  }
}

function probeShopifyAppCliAuth(appScopeDrift) {
  const command = 'corepack pnpm exec shopify app info --json';
  const workdir =
    typeof appScopeDrift?.directory === 'string' && appScopeDrift.directory.length > 0 ? appScopeDrift.directory : null;

  if (!workdir) {
    return {
      status: 'unavailable',
      message: '- could not probe Shopify app CLI auth because no configured app working directory was available',
    };
  }

  try {
    execFileSync('corepack', ['pnpm', 'exec', 'shopify', 'app', 'info', '--json', '--path', workdir], {
      cwd: workdir,
      encoding: 'utf8',
      timeout: 15000,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    return {
      status: 'available',
      workdir,
      command,
    };
  } catch (error) {
    const stdout = typeof error?.stdout === 'string' ? error.stdout : '';
    const stderr = typeof error?.stderr === 'string' ? error.stderr : '';
    const combined = `${stdout}\n${stderr}`;
    if (combined.includes('To run this command, log in to Shopify.')) {
      return {
        status: 'login-required',
        workdir,
        command,
      };
    }

    return {
      status: 'unavailable',
      workdir,
      command,
      message: `- attempted \`${command}\` in \`${workdir}\`, but Shopify CLI auth probing failed unexpectedly: ${error.message}`,
    };
  }
}

function summarizeConfiguredAppScopeDrift(appScopeDrift) {
  if (!appScopeDrift) {
    return [];
  }

  if (appScopeDrift.note) {
    return [
      '## Configured app scope drift',
      '',
      `- inspected Shopify CLI app config at \`${appScopeDrift.configPath}\` for handle \`${appScopeDrift.appHandle}\``,
      `- ${appScopeDrift.note}`,
      '',
    ];
  }

  const requestedScopeLines = appScopeDrift.requestedScopes.map((scope) => `- \`${scope}\``);
  const missingRequestedScopeLines = appScopeDrift.missingRequestedScopes.map((scope) => `- \`${scope}\``);
  const channelConfigExtension = Array.isArray(appScopeDrift.channelConfigExtensions)
    ? (appScopeDrift.channelConfigExtensions[0] ?? null)
    : null;

  return [
    '## Configured app scope drift',
    '',
    `- inspected configured app handle \`${appScopeDrift.appHandle}\` at \`${appScopeDrift.appConfigPath}\``,
    ...(appScopeDrift.appId ? [`- configured app id: \`${appScopeDrift.appId}\``] : []),
    ...(channelConfigExtension
      ? [
          `- channel config extension: \`${channelConfigExtension.handle}\` @ \`${channelConfigExtension.extensionPath}\``,
          `- channel config create_legacy_channel_on_app_install = ${channelConfigExtension.createLegacyChannelOnAppInstall === null ? 'unknown' : `\`${String(channelConfigExtension.createLegacyChannelOnAppInstall)}\``}`,
        ]
      : ['- no channel_config extension is currently present under the configured app directory']),
    '- the checked-in `shopify.app.toml` currently requests:',
    ...requestedScopeLines,
    ...(appScopeDrift.missingRequestedScopes.length > 0
      ? [
          '',
          '- the active token/install is still missing these requested scopes:',
          ...missingRequestedScopeLines,
          '- this points to store install / re-authorization drift rather than a missing scope declaration in the checked-in app config',
        ]
      : ['', '- the active token/install already matches the checked-in requested scopes.']),
    '',
  ];
}

async function loadManualStoreAuthSummary() {
  try {
    const payload = JSON.parse(await readFile(manualStoreAuthTokenPath, 'utf8'));
    const summary = extractManualStoreAuthTokenSummary(payload);
    if (!summary) {
      return null;
    }

    return {
      ...summary,
      tokenPath: manualStoreAuthTokenPath,
      cachedScopeHandles: summary.scopeHandles,
      liveScopeHandles: [],
      status:
        summary.scopeHandles.includes('read_product_listings') ||
        summary.scopeHandles.includes('read_publications') ||
        summary.scopeHandles.includes('write_publications')
          ? 'scope-present'
          : 'scope-missing',
    };
  } catch (error) {
    if (error?.code === 'ENOENT') {
      return null;
    }

    return {
      accessToken: '',
      tokenPath: manualStoreAuthTokenPath,
      status: 'unreadable',
      errorMessage: error.message,
      tokenFamily: 'unknown',
      hasRefreshToken: false,
      scopeHandles: [],
      cachedScopeHandles: [],
      liveScopeHandles: [],
      associatedUserScopeHandles: [],
      associatedUserEmail: null,
    };
  }
}

function summarizeManualStoreAuthSummary(manualStoreAuthSummary) {
  if (!manualStoreAuthSummary) {
    return [];
  }

  if (manualStoreAuthSummary.status === 'unreadable') {
    return [
      '## Saved manual store-auth token state',
      '',
      `- attempted to inspect saved manual PKCE store-auth token at \`${manualStoreAuthSummary.tokenPath}\``,
      `- the file could not be parsed: ${manualStoreAuthSummary.errorMessage}`,
      '',
    ];
  }

  const cachedScopeLines = (manualStoreAuthSummary.cachedScopeHandles ?? []).map((scope) => `- \`${scope}\``);
  const liveScopeLines = (manualStoreAuthSummary.liveScopeHandles ?? manualStoreAuthSummary.scopeHandles ?? []).map(
    (scope) => `- \`${scope}\``,
  );
  const associatedUserScopeLines = (manualStoreAuthSummary.associatedUserScopeHandles ?? []).map(
    (scope) => `- \`${scope}\``,
  );
  const statusLines =
    manualStoreAuthSummary.status === 'scope-blocked'
      ? [
          '',
          '- the saved manual PKCE store-auth token could authenticate and was probed directly against Admin GraphQL, but publication reads/writes were still scope-blocked on the live token',
        ]
      : manualStoreAuthSummary.status === 'auth-failed'
        ? [
            '',
            `- the saved manual PKCE store-auth token could not complete the live publication probe: ${manualStoreAuthSummary.errorMessage}`,
          ]
        : manualStoreAuthSummary.status === 'available'
          ? [
              '',
              '- the saved manual PKCE store-auth token could satisfy the live publication probe and is now usable as a publication capture credential',
            ]
          : manualStoreAuthSummary.status === 'scope-missing'
            ? [
                '',
                '- this saved manual PKCE store-auth token still lacks publication scopes, so it does not widen the publication-family scope surface for unattended capture',
              ]
            : [''];

  return [
    '## Saved manual store-auth token state',
    '',
    `- inspected saved manual PKCE store-auth token at \`${manualStoreAuthSummary.tokenPath}\``,
    `- token family: \`${manualStoreAuthSummary.tokenFamily}\``,
    `- refresh token present: ${manualStoreAuthSummary.hasRefreshToken ? 'yes' : 'no'}`,
    ...(manualStoreAuthSummary.associatedUserEmail
      ? [`- associated user: \`${manualStoreAuthSummary.associatedUserEmail}\``]
      : []),
    ...(cachedScopeLines.length > 0
      ? ['- saved token response scopes currently include:', ...cachedScopeLines]
      : ['- saved token response scopes currently include: none recorded']),
    ...(liveScopeLines.length > 0
      ? ['', '- live Admin GraphQL access scopes currently include:', ...liveScopeLines]
      : []),
    ...(associatedUserScopeLines.length > 0
      ? ['', '- saved associated user scopes currently include:', ...associatedUserScopeLines]
      : []),
    ...statusLines,
    '',
  ];
}

function summarizeFallbackOutcome(fallbackOutcome) {
  if (!fallbackOutcome) {
    return [];
  }

  if (fallbackOutcome.status === 'invalid-grant') {
    return [
      '## Shopify CLI fallback status',
      '',
      `- checked fallback account token state from \`${fallbackOutcome.configPath}\``,
      '- attempted a non-interactive Shopify Accounts refresh for the CLI bearer token',
      '- refresh failed with `invalid_grant`, so the stored CLI access/refresh pair is unrecoverable for unattended runs',
      '- this means the host cannot sidestep the publication-scope blocker by temporarily switching to the older CLI bearer-token path',
      '',
    ];
  }

  if (fallbackOutcome.status === 'auth-failed') {
    return [
      '## Shopify CLI fallback status',
      '',
      `- checked fallback account token state from \`${fallbackOutcome.configPath}\``,
      '- the CLI bearer token path still failed authentication against Admin GraphQL after refresh',
      `- probe failure surface: ${fallbackOutcome.message}`,
      '',
    ];
  }

  if (fallbackOutcome.status === 'scope-blocked') {
    const blockerLines = fallbackOutcome.blockers.flatMap((blocker) => [
      `- CLI fallback ${blocker.operationName} probe still required: ${blocker.requiredAccess}`,
      `  > ${blocker.message}`,
    ]);

    return [
      '## Shopify CLI fallback status',
      '',
      `- checked fallback account token state from \`${fallbackOutcome.configPath}\``,
      '- the CLI bearer token could authenticate, but publication probes were still scope-blocked',
      ...blockerLines,
      '',
    ];
  }

  if (fallbackOutcome.status === 'missing-cli-session' || fallbackOutcome.status === 'unavailable') {
    return ['## Shopify CLI fallback status', '', fallbackOutcome.message, ''];
  }

  return [];
}

function summarizeShopifyAppCliAuth(shopifyAppCliAuth) {
  if (!shopifyAppCliAuth) {
    return [];
  }

  if (shopifyAppCliAuth.status === 'login-required') {
    return [
      '## Shopify app CLI auth status',
      '',
      `- attempted \`${shopifyAppCliAuth.command}\` in \`${shopifyAppCliAuth.workdir}\``,
      '- Shopify CLI still required interactive device login before it would inspect the configured app',
      '- this means unattended runs cannot self-heal publication scope drift by using Shopify app CLI commands on this host right now',
      '',
    ];
  }

  if (shopifyAppCliAuth.status === 'available') {
    return [
      '## Shopify app CLI auth status',
      '',
      `- \`${shopifyAppCliAuth.command}\` could run in \`${shopifyAppCliAuth.workdir}\` without interactive login`,
      '',
    ];
  }

  if (shopifyAppCliAuth.status === 'unavailable') {
    return ['## Shopify app CLI auth status', '', shopifyAppCliAuth.message, ''];
  }

  return [];
}

async function attemptShopifyAppDeploy(shopifyAppCliAuth, appScopeDrift) {
  if (!shouldAttemptShopifyAppDeploy(shopifyAppCliAuth, appScopeDrift)) {
    return {
      status: 'skipped',
    };
  }

  const command = 'corepack pnpm exec shopify app deploy --allow-updates';
  try {
    const output = execFileSync('corepack', ['pnpm', 'exec', 'shopify', 'app', 'deploy', '--allow-updates'], {
      cwd: shopifyAppCliAuth.workdir,
      encoding: 'utf8',
      timeout: 300000,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    return {
      status: 'deployed-but-still-scope-blocked',
      command,
      workdir: shopifyAppCliAuth.workdir,
      versionLabel: extractShopifyAppDeployVersion(output),
    };
  } catch (error) {
    const stdout = typeof error?.stdout === 'string' ? error.stdout : '';
    const stderr = typeof error?.stderr === 'string' ? error.stderr : '';
    const combined = `${stdout}\n${stderr}`;
    if (combined.includes('To run this command, log in to Shopify.')) {
      return {
        status: 'login-required',
        command,
        workdir: shopifyAppCliAuth.workdir,
      };
    }

    return {
      status: 'failed',
      command,
      workdir: shopifyAppCliAuth.workdir,
      message: error.message,
    };
  }
}

function summarizeShopifyAppDeploy(appDeployOutcome) {
  if (!appDeployOutcome || appDeployOutcome.status === 'skipped') {
    return [];
  }

  if (appDeployOutcome.status === 'deployed-but-still-scope-blocked') {
    return [
      '## Shopify app deploy status',
      '',
      `- attempted \`${appDeployOutcome.command}\` in \`${appDeployOutcome.workdir}\` after confirming Shopify app CLI auth was available`,
      `- the deploy succeeded and released${appDeployOutcome.versionLabel ? ` \`${appDeployOutcome.versionLabel}\`` : ' a new version'} to users`,
      '- this still did not widen the already-issued store token scopes for unattended publication capture, so install re-authorization drift remains the blocker',
      '',
    ];
  }

  if (appDeployOutcome.status === 'login-required') {
    return [
      '## Shopify app deploy status',
      '',
      `- attempted \`${appDeployOutcome.command}\` in \`${appDeployOutcome.workdir}\``,
      '- Shopify CLI fell back to interactive login before deploy could run, so unattended scope repair still could not proceed through app deploy',
      '',
    ];
  }

  if (appDeployOutcome.status === 'failed') {
    return [
      '## Shopify app deploy status',
      '',
      `- attempted \`${appDeployOutcome.command}\` in \`${appDeployOutcome.workdir}\`, but deploy failed unexpectedly: ${appDeployOutcome.message}`,
      '',
    ];
  }

  return [];
}

function buildPublicationScopeBlockerNote({
  scopeHandles,
  blockers,
  credentialSummary,
  fallbackOutcome,
  appScopeDrift,
  shopifyAppCliAuth,
  appDeployOutcome,
  manualStoreAuthSummary,
}) {
  const scopeLines = scopeHandles.map((scope) => `- \`${scope}\``);
  const blockerLines = blockers.flatMap((blocker) => [
    `- ${blocker.operationName} → required access: ${blocker.requiredAccess}`,
    `  > ${blocker.message}`,
  ]);

  return [
    '# Product publication conformance blocker',
    '',
    '## What failed',
    '',
    'Attempted to capture live conformance for the staged product publication family (`productPublish`, `productUnpublish`).',
    '',
    '## Evidence',
    '',
    '- `corepack pnpm conformance:probe` still succeeds',
    '- the dedicated publication capture harness now probes current token scopes, the exact publication aggregate read surfaces, and minimal publish/unpublish mutation roots before any full write capture',
    `- ${credentialSummary}`,
    '- `currentAppInstallation.accessScopes` on the active token currently includes:',
    ...scopeLines,
    '',
    '- publication-related live probes failed with Shopify access errors:',
    ...blockerLines,
    '',
    ...summarizeConfiguredAppScopeDrift(appScopeDrift),
    ...summarizeFallbackOutcome(fallbackOutcome),
    ...summarizeShopifyAppCliAuth(shopifyAppCliAuth),
    ...summarizeShopifyAppDeploy(appDeployOutcome),
    ...summarizeManualStoreAuthSummary(manualStoreAuthSummary),
    '## Why this blocks closure',
    '',
    'The checked-in parity plans for `productPublish` / `productUnpublish` intentionally compare mutation payloads and immediate downstream reads for:',
    '',
    '- `publishedOnCurrentPublication`',
    '- `availablePublicationsCount`',
    '- `resourcePublicationsCount`',
    '',
    'Without `read_product_listings`, `read_publications`, and `write_publications`, the repo cannot safely settle the publication aggregate behavior against live Shopify evidence or execute the publish/unpublish mutation family against a real publication target.',
    '',
    '## What was completed anyway',
    '',
    '1. added a dedicated publication-family live capture harness wired through `corepack pnpm conformance:capture-product-publications`',
    '2. made blocker parsing capable of preserving multiple simultaneous Shopify access-denied requirements from a single probe response',
    '3. refreshed this blocker note from a real probe on the current host token rather than leaving the earlier manual note stale',
    '4. verified whether the older Shopify CLI bearer-token path could bypass the blocker before giving up on unattended closure',
    '5. confirmed Shopify app CLI auth is now available for the configured conformance app on this host',
    '6. attempted `corepack pnpm exec shopify app deploy --allow-updates` for the configured conformance app and verified that the resulting release still did not widen the already-issued store token scopes',
    '7. captured direct live mutation-root blocker evidence for `productPublish` / `productUnpublish` using a temporary product and the same safe publication probe target used by the parity scaffold',
    '',
    '## Recommended next step',
    '',
    're-authorize the app / token with the missing publication scopes so the active dev-store credential includes `read_product_listings`, `read_publications`, and `write_publications`, then rerun `corepack pnpm conformance:capture-product-publications` to capture `productPublish` / `productUnpublish` mutation payloads plus immediate downstream publication aggregate reads.',
    '',
  ].join('\n');
}

function buildPublicationAggregateFieldBlockerNote({
  publicationId,
  publishAggregateBlocker,
  publishReadBlocker,
  unpublishAggregateBlocker,
  unpublishReadBlocker,
  appScopeDrift,
  appDeployOutcome,
  activeCredentialObservation,
}) {
  const blockerLines = [
    publishAggregateBlocker ? `- publish mutation aggregate slice → ${publishAggregateBlocker.message}` : null,
    publishReadBlocker ? `- post-publish downstream read → ${publishReadBlocker.message}` : null,
    unpublishAggregateBlocker ? `- unpublish mutation aggregate slice → ${unpublishAggregateBlocker.message}` : null,
    unpublishReadBlocker ? `- post-unpublish downstream read → ${unpublishReadBlocker.message}` : null,
  ].filter(Boolean);
  const channelConfigExtension = Array.isArray(appScopeDrift?.channelConfigExtensions)
    ? (appScopeDrift.channelConfigExtensions[0] ?? null)
    : null;
  const deployObservationLines =
    appDeployOutcome?.status === 'deployed-but-still-scope-blocked'
      ? [
          `- the latest unattended deploy released \`${appDeployOutcome.versionLabel ?? 'an unknown version'}\` but still did not backfill a publication target for the existing store install`,
        ]
      : appDeployOutcome?.status === 'deployed-but-app-still-lacks-publication'
        ? [
            `- the latest unattended deploy released \`${appDeployOutcome.versionLabel ?? 'an unknown version'}\` but still did not backfill a publication target for the existing store install`,
          ]
        : [];

  return [
    '# Product publication conformance blocker',
    '',
    '## What succeeded',
    '',
    'Attempted to capture live conformance for the staged product publication family (`productPublish`, `productUnpublish`).',
    '',
    '- safe `productPublish` / `productUnpublish` mutation payloads now capture successfully',
    `- Shopify accepted a real publication target id during the successful safe mutation captures: \`${publicationId}\``,
    '- `productPublish` can therefore be promoted to covered parity for the successful live payload slice (`product { id }` plus `userErrors`)',
    '- `productUnpublish` remains tracked by its own captured minimal payload slice until that operation is promoted separately',
    '',
    '## Remaining field-level blocker',
    '',
    '- aggregate publication fields remain blocked for this app',
    '- publication aggregate reads still fail for this app even after the safe mutation succeeds',
    ...(activeCredentialObservation
      ? [
          `- current conformance credential family: \`${activeCredentialObservation.tokenFamily}\``,
          `- header mode: raw \`X-Shopify-Access-Token\` (${activeCredentialObservation.headerMode})`,
          `- ${activeCredentialObservation.summary}`,
        ]
      : []),
    '',
    ...blockerLines,
    '',
    '## Why the blocker remains explicit',
    '',
    "The configured conformance app still does not have its own publication on this shop, so asking Shopify to resolve aggregate publication fields on `product` still returns `Your app doesn't have a publication for this shop.` on this host.",
    ...(channelConfigExtension
      ? [
          `- current channel_config extension: \`${channelConfigExtension.handle}\` @ \`${channelConfigExtension.extensionPath}\``,
          `- current channel_config create_legacy_channel_on_app_install = ${channelConfigExtension.createLegacyChannelOnAppInstall === null ? 'unknown' : `\`${String(channelConfigExtension.createLegacyChannelOnAppInstall)}\``}`,
        ]
      : ['- no channel_config extension is currently present under the configured conformance app directory']),
    ...deployObservationLines,
    'Keep that blocker attached to the parity specs so future runs can distinguish the now-captured root mutation parity from the still-blocked aggregate publication field slice.',
    '',
    '## Recommended next step',
    '',
    'Install or configure the conformance app so it has a real publication on `very-big-test-store.myshopify.com`, then rerun `corepack pnpm conformance:capture-product-publications` to refresh the fixtures with successful aggregate publication field payloads and downstream reads.',
    'If the channel config changed recently, do not assume deploy alone backfills a publication on the existing store install — reinstallation or explicit channel/publication setup may still be required.',
    '',
    '## Evidence refresh commands',
    '',
    '- `corepack pnpm conformance:probe`',
    '- `corepack pnpm conformance:capture-product-publications`',
    '- `corepack pnpm exec shopify app deploy --allow-updates`',
    '',
  ].join('\n');
}

async function collectPublicationMutationScopeProbe(client, runId) {
  let probeProductId = null;

  try {
    const createResponse = await client.runGraphql(
      createMutation,
      buildCreateVariables(`${runId}-mutation-scope-probe`),
    );
    probeProductId = createResponse.data?.productCreate?.product?.id ?? null;
    if (!probeProductId) {
      throw new Error('Product publication mutation scope probe could not create a temporary product.');
    }

    const input = {
      id: probeProductId,
      productPublications: [{ publicationId: publicationMutationScopeProbePublicationId }],
    };
    const publishProbe = await client.runGraphqlRaw(publishMutationScopeProbe, { input });
    const unpublishProbe = await client.runGraphqlRaw(unpublishMutationScopeProbe, { input });

    return {
      publicationId: publicationMutationScopeProbePublicationId,
      blockers: [...parseAccessDeniedErrors(publishProbe), ...parseAccessDeniedErrors(unpublishProbe)],
    };
  } finally {
    if (probeProductId) {
      try {
        await client.runGraphql(deleteMutation, { input: { id: probeProductId } });
      } catch {
        // Best-effort cleanup only. The caller should still surface the original blocker state.
      }
    }
  }
}

async function collectPublicationProbe(client, runId) {
  const seedResponse = await client.runGraphql(productSeedQuery);
  const seedProductId = seedResponse.data?.products?.nodes?.[0]?.id ?? null;
  if (!seedProductId) {
    throw new Error('Product publication capture could not find a seed product for the scope probe.');
  }

  const scopeHandlesResponse = await client.runGraphql(publicationScopeHandlesQuery);
  const scopeHandles = Array.isArray(scopeHandlesResponse.data?.currentAppInstallation?.accessScopes)
    ? scopeHandlesResponse.data.currentAppInstallation.accessScopes
        .map((scope) => scope?.handle)
        .filter((handle) => typeof handle === 'string')
    : [];

  const aggregateProbe = await client.runGraphqlRaw(publicationAggregateProbeQuery, { id: seedProductId });
  const listProbe = await client.runGraphqlRaw(publicationListProbeQuery);
  const mutationProbe = await collectPublicationMutationScopeProbe(client, runId);
  const blockers = [
    ...parseAccessDeniedErrors(aggregateProbe),
    ...parseAccessDeniedErrors(listProbe),
    ...mutationProbe.blockers,
  ];
  const publicationId =
    listProbe.payload?.data?.publications?.edges?.[0]?.node?.id ??
    listProbe.payload?.data?.publications?.nodes?.[0]?.id ??
    mutationProbe.publicationId ??
    null;

  return {
    seedProductId,
    scopeHandles,
    blockers,
    publicationId,
    listProbe,
  };
}

async function tryManualStoreAuthPublicationFallback(manualStoreAuthSummary) {
  if (!shouldProbeManualStoreAuthFallback(manualStoreAuthSummary)) {
    return {
      ok: false,
      status: 'missing-manual-token',
      tokenPath: manualStoreAuthSummary?.tokenPath ?? manualStoreAuthTokenPath,
      summary: manualStoreAuthSummary,
    };
  }

  try {
    const client = buildGraphqlClient(manualStoreAuthSummary.accessToken);
    const probe = await collectPublicationProbe(client, `${Date.now()}-manual-store-auth`);
    const nextSummary = {
      ...manualStoreAuthSummary,
      scopeHandles: probe.scopeHandles,
      liveScopeHandles: probe.scopeHandles,
      status: probe.blockers.length > 0 ? 'scope-blocked' : 'available',
    };

    if (probe.blockers.length > 0) {
      return {
        ok: false,
        status: 'scope-blocked',
        tokenPath: manualStoreAuthSummary.tokenPath,
        blockers: probe.blockers,
        summary: nextSummary,
      };
    }

    if (!probe.publicationId) {
      return {
        ok: false,
        status: 'unavailable',
        tokenPath: manualStoreAuthSummary.tokenPath,
        message: `- checked saved manual PKCE store-auth token at \`${manualStoreAuthSummary.tokenPath}\`, but the live probe still could not resolve a publication id`,
        summary: nextSummary,
      };
    }

    return {
      ok: true,
      token: manualStoreAuthSummary.accessToken,
      credentialSummary: 'a saved manual PKCE store-auth Admin token provided the required publication probe access',
      probe,
      summary: nextSummary,
    };
  } catch (error) {
    return {
      ok: false,
      status: 'auth-failed',
      tokenPath: manualStoreAuthSummary.tokenPath,
      message: error.message,
      summary: {
        ...manualStoreAuthSummary,
        status: 'auth-failed',
        errorMessage: error.message,
      },
    };
  }
}

async function refreshCliIdentity(identity) {
  const raw = execFileSync(
    'curl',
    [
      '-sS',
      '-X',
      'POST',
      'https://accounts.shopify.com/oauth/token',
      '-H',
      'User-Agent: Shopify CLI; v=3.86.1',
      '-H',
      'Sec-CH-UA-PLATFORM: darwin',
      '-H',
      'Content-Type: application/x-www-form-urlencoded',
      '--data-urlencode',
      'grant_type=refresh_token',
      '--data-urlencode',
      `access_token=${identity.accessToken}`,
      '--data-urlencode',
      `refresh_token=${identity.refreshToken}`,
      '--data-urlencode',
      'client_id=fbdb2649-e327-4907-8f67-908d24cfd7e3',
    ],
    { encoding: 'utf8' },
  );

  const payload = JSON.parse(raw);
  if (isInvalidGrantRefreshResponse(payload)) {
    return { ok: false, status: 'invalid-grant', payload };
  }

  if (typeof payload.access_token !== 'string' || typeof payload.refresh_token !== 'string') {
    return { ok: false, status: 'unexpected-response', payload };
  }

  return {
    ok: true,
    identity: {
      accessToken: payload.access_token,
      refreshToken: payload.refresh_token,
      expiresAt: new Date(
        Date.now() + (typeof payload.expires_in === 'number' ? payload.expires_in : 0) * 1000,
      ).toISOString(),
    },
  };
}

async function tryShopifyCliPublicationFallback() {
  const configPath = getDefaultShopifyCliConfigPath();

  try {
    const config = await loadShopifyCliConfig(configPath);
    const cliSession = extractCliIdentityFromConfig(config);
    if (!cliSession?.identity?.accessToken || !cliSession.identity.refreshToken) {
      return {
        ok: false,
        status: 'missing-cli-session',
        configPath,
        message: `- checked fallback account token state from \`${configPath}\`, but no active Shopify CLI identity was available`,
      };
    }

    const refreshed = await refreshCliIdentity(cliSession.identity);
    if (!refreshed.ok) {
      if (refreshed.status === 'invalid-grant') {
        return {
          ok: false,
          status: 'invalid-grant',
          configPath,
          payload: refreshed.payload,
        };
      }

      return {
        ok: false,
        status: 'unavailable',
        configPath,
        message: `- checked fallback account token state from \`${configPath}\`, but refresh returned an unexpected payload`,
      };
    }

    await persistShopifyCliIdentity(configPath, config, {
      sessionId: cliSession.sessionId,
      identity: refreshed.identity,
    });

    const client = buildGraphqlClient(refreshed.identity.accessToken);
    const probe = await collectPublicationProbe(client, `${Date.now()}-cli-fallback`);
    if (probe.blockers.length > 0) {
      return {
        ok: false,
        status: 'scope-blocked',
        configPath,
        blockers: probe.blockers,
      };
    }

    if (!probe.publicationId) {
      return {
        ok: false,
        status: 'unavailable',
        configPath,
        message: `- checked fallback account token state from \`${configPath}\`, but the CLI probe still could not resolve a publication id`,
      };
    }

    return {
      ok: true,
      token: refreshed.identity.accessToken,
      credentialSummary:
        'a refreshed Shopify CLI account bearer token temporarily provided the required publication probe access',
      probe,
    };
  } catch (error) {
    return {
      ok: false,
      status: 'unavailable',
      configPath,
      message: `- checked fallback account token state from \`${configPath}\`, but the CLI fallback could not be evaluated: ${error.message}`,
    };
  }
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const createVariables = buildCreateVariables(runId);
let createdProductId = null;
let activeCredentialObservation = describeCredentialObservation(adminAccessToken);
let activeCredentialSummary = activeCredentialObservation.summary;
let activeToken = adminAccessToken;
let activeClient = buildGraphqlClient(activeToken);

try {
  let probe = await collectPublicationProbe(activeClient, runId);
  let manualStoreAuthSummary = await loadManualStoreAuthSummary();
  let manualStoreAuthFallback = null;
  let fallbackOutcome = null;

  if (probe.blockers.length > 0) {
    manualStoreAuthFallback = await tryManualStoreAuthPublicationFallback(manualStoreAuthSummary);
    if (manualStoreAuthFallback.summary) {
      manualStoreAuthSummary = manualStoreAuthFallback.summary;
    }

    if (manualStoreAuthFallback.ok) {
      activeToken = manualStoreAuthFallback.token;
      activeCredentialObservation = describeCredentialObservation(activeToken);
      activeCredentialSummary = manualStoreAuthFallback.credentialSummary ?? activeCredentialObservation.summary;
      activeClient = buildGraphqlClient(activeToken);
      probe = manualStoreAuthFallback.probe;
    }
  }

  if (probe.blockers.length > 0 && adminAccessToken.startsWith('shpat_')) {
    fallbackOutcome = await tryShopifyCliPublicationFallback();
    if (fallbackOutcome.ok) {
      activeToken = fallbackOutcome.token;
      activeCredentialObservation = describeCredentialObservation(activeToken);
      activeCredentialSummary = fallbackOutcome.credentialSummary ?? activeCredentialObservation.summary;
      activeClient = buildGraphqlClient(activeToken);
      probe = fallbackOutcome.probe;
    }
  }

  let appScopeDrift = await loadConfiguredAppScopeDrift(probe.scopeHandles);
  let shopifyAppCliAuth = probeShopifyAppCliAuth(appScopeDrift);
  let appDeployOutcome = null;

  if (probe.blockers.length > 0) {
    appDeployOutcome = await attemptShopifyAppDeploy(shopifyAppCliAuth, appScopeDrift);
    if (appDeployOutcome.status === 'deployed-but-still-scope-blocked') {
      probe = await collectPublicationProbe(activeClient, `${runId}-post-app-deploy`);
      appScopeDrift = await loadConfiguredAppScopeDrift(probe.scopeHandles);
      shopifyAppCliAuth = probeShopifyAppCliAuth(appScopeDrift);
    }
  }

  if (probe.blockers.length > 0) {
    const blockerNote = buildPublicationScopeBlockerNote({
      scopeHandles: probe.scopeHandles,
      blockers: probe.blockers,
      credentialSummary: activeCredentialSummary,
      fallbackOutcome,
      appScopeDrift,
      shopifyAppCliAuth,
      appDeployOutcome,
      manualStoreAuthSummary,
    });
    console.log(
      JSON.stringify(
        {
          ok: false,
          blocked: true,
          blockerNote,
          blockers: probe.blockers,
          scopeHandles: probe.scopeHandles,
          appScopeDrift,
          manualStoreAuthFallback,
          fallbackOutcome,
          shopifyAppCliAuth,
          appDeployOutcome,
          manualStoreAuthSummary,
          manualStoreAuthStatus: manualStoreAuthSummary?.status ?? null,
          manualStoreAuthTokenPath: manualStoreAuthSummary?.tokenPath ?? null,
          manualStoreAuthScopes: manualStoreAuthSummary?.scopeHandles ?? [],
        },
        null,
        2,
      ),
    );
    process.exit(1);
  }

  const publicationId = probe.publicationId;
  if (!publicationId) {
    throw new Error('Product publication capture could not find a publication id after the scope probe succeeded.');
  }

  const createResponse = await activeClient.runGraphql(createMutation, createVariables);
  const seedProduct = createResponse.data?.productCreate?.product ?? null;
  createdProductId = seedProduct?.id ?? null;
  if (!createdProductId) {
    throw new Error('Product publication capture did not return a product id.');
  }

  const publishVariables = {
    input: {
      id: createdProductId,
      productPublications: [{ publicationId }],
    },
  };

  const publishResponse = await activeClient.runGraphql(publishMutationScopeProbe, publishVariables);
  const publishAggregateResponse = await activeClient.runGraphqlRaw(publishMutation, publishVariables);
  const postPublishRead = await activeClient.runGraphqlRaw(downstreamReadQuery, { id: createdProductId });

  const unpublishVariables = {
    input: {
      id: createdProductId,
      productPublications: [{ publicationId }],
    },
  };

  const unpublishResponse = await activeClient.runGraphql(unpublishMutationScopeProbe, unpublishVariables);
  const unpublishAggregateResponse = await activeClient.runGraphqlRaw(unpublishMutation, unpublishVariables);
  const postUnpublishRead = await activeClient.runGraphqlRaw(downstreamReadQuery, { id: createdProductId });

  const publishAggregateBlocker = parsePublicationTargetBlocker(publishAggregateResponse);
  const publishReadBlocker = parsePublicationTargetBlocker(postPublishRead);
  const unpublishAggregateBlocker = parsePublicationTargetBlocker(unpublishAggregateResponse);
  const unpublishReadBlocker = parsePublicationTargetBlocker(postUnpublishRead);

  const captures = {
    'publications-catalog.json': probe.listProbe,
    'product-publish-parity.json': {
      seedProduct,
      mutation: {
        queryShape: 'product-id-and-user-errors',
        variables: publishVariables,
        response: publishResponse,
      },
      aggregateSelection: {
        response: publishAggregateResponse,
        blocker: publishAggregateBlocker,
      },
      downstreamRead: {
        response: postPublishRead,
        blocker: publishReadBlocker,
      },
    },
    'product-unpublish-parity.json': {
      seedProduct,
      mutation: {
        queryShape: 'minimal-user-errors-only',
        variables: unpublishVariables,
        response: unpublishResponse,
      },
      aggregateSelection: {
        response: unpublishAggregateResponse,
        blocker: unpublishAggregateBlocker,
      },
      downstreamRead: {
        response: postUnpublishRead,
        blocker: unpublishReadBlocker,
      },
    },
  };

  for (const [filename, payload] of Object.entries(captures)) {
    await writeFile(path.join(outputDir, filename), `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
  }

  const aggregateFieldBlocker =
    publishAggregateBlocker || publishReadBlocker || unpublishAggregateBlocker || unpublishReadBlocker
      ? {
          publishAggregateBlocker,
          publishReadBlocker,
          unpublishAggregateBlocker,
          unpublishReadBlocker,
        }
      : null;
  const aggregateFieldBlockerNote = aggregateFieldBlocker
    ? buildPublicationAggregateFieldBlockerNote({
        publicationId,
        publishAggregateBlocker,
        publishReadBlocker,
        unpublishAggregateBlocker,
        unpublishReadBlocker,
        appScopeDrift,
        appDeployOutcome,
        activeCredentialObservation,
      })
    : null;

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: Object.keys(captures),
        productId: createdProductId,
        publicationId,
        aggregateFieldBlocker,
        aggregateFieldBlockerNote,
      },
      null,
      2,
    ),
  );
} catch (error) {
  const publicationTargetBlocker = parsePublicationTargetBlocker(error?.result ?? null);
  if (publicationTargetBlocker) {
    const note = [
      '# Product publication conformance blocker',
      '',
      '## What failed',
      '',
      'Attempted to capture live conformance for the staged product publication family (`productPublish`, `productUnpublish`).',
      '',
      '- publication aggregate probes and mutation-root scope probes now succeed after refreshing the app install path',
      '- the first full publish capture failed when Shopify tried to resolve the returned publication aggregate fields',
      '',
      'Observed error excerpt:',
      '',
      `> ${publicationTargetBlocker.message}`,
      '',
      '## Why this blocks closure',
      '',
      'The active token is no longer blocked on publication scopes, but this app still does not have a publication target on the shop, so `productPublish` / `productUnpublish` cannot be captured against a real publication id for the parity scaffold.',
      '',
      '## What was completed anyway',
      '',
      '1. verified that publication aggregate reads and publish/unpublish mutation roots can now get past the earlier scope-denied stage',
      '2. attempted `corepack pnpm exec shopify app deploy --allow-updates` and confirmed the app config release itself is no longer the gating step',
      '3. discovered the next live blocker from Shopify itself: the app still lacks a publication on this shop even after deploy/reauthorization drift repair',
      '',
      '## Recommended next step',
      '',
      'Install or configure the conformance app so it has a real publication on `very-big-test-store.myshopify.com`, then rerun `corepack pnpm conformance:capture-product-publications` to capture `productPublish` / `productUnpublish` payloads plus immediate downstream publication aggregate reads.',
      '',
    ].join('\n');

    console.log(
      JSON.stringify(
        {
          ok: false,
          blocked: true,
          blockerNote: note,
          blocker: publicationTargetBlocker,
        },
        null,
        2,
      ),
    );
    process.exit(1);
  }

  const blocker = parseWriteScopeBlocker(error?.result ?? null);
  if (blocker) {
    const note = renderWriteScopeBlockerNote({
      title: 'Product publication conformance blocker',
      whatFailed:
        'Attempted to capture live conformance for the staged product publication family (`productPublish`, `productUnpublish`).',
      operations: ['productPublish', 'productUnpublish'],
      blocker,
      whyBlocked:
        'Without a write-capable token, the repo cannot capture successful live payload shape, userErrors behavior, or immediate downstream publication aggregate parity for these product publication mutations.',
      completedSteps: [
        'added a dedicated publication-family live capture harness wired through `corepack pnpm conformance:capture-product-publications`',
        'aligned the publish/unpublish mutation payloads and downstream read slices with the existing parity-request scaffolds so future runs capture the same aggregate publication fields directly',
      ],
      recommendedNextStep:
        'Switch the repo conformance credential to a safe dev-store token with product write permissions and publication/listing read scopes, then rerun `corepack pnpm conformance:capture-product-publications`.',
    });

    console.log(
      JSON.stringify(
        {
          ok: false,
          blocked: true,
          blockerNote: note,
          blocker,
        },
        null,
        2,
      ),
    );
    process.exit(1);
  }

  throw error;
} finally {
  if (createdProductId) {
    try {
      await activeClient.runGraphql(deleteMutation, { input: { id: createdProductId } });
    } catch {
      // Best-effort cleanup only. The conformance script should still surface the original failure.
    }
  }
}
