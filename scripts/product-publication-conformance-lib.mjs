import os from 'node:os';
import path from 'node:path';
import { readdir, readFile, rm, writeFile } from 'node:fs/promises';

export async function clearPublicationScopeBlocker(blockerPath) {
  await rm(blockerPath, { force: true });
}

export function getDefaultShopifyCliConfigPath() {
  return path.join(os.homedir(), '.config', 'shopify-cli-kit-nodejs', 'config.json');
}

export function getDefaultShopifyCliAppConfigPath() {
  return path.join(os.homedir(), '.config', 'shopify-cli-app-nodejs', 'config.json');
}

export function extractCliIdentityFromConfig(config) {
  const rawSessionStore = config?.sessionStore;
  if (typeof rawSessionStore !== 'string') {
    return null;
  }

  const sessionStore = JSON.parse(rawSessionStore);
  const accounts = sessionStore['accounts.shopify.com'];
  if (!accounts || typeof accounts !== 'object') {
    return null;
  }

  const activeSessionId = typeof accounts['active-session'] === 'string' ? accounts['active-session'] : null;
  const sessionIds = Object.keys(accounts).filter((key) => key !== 'active-session');
  const sessionId =
    activeSessionId && sessionIds.includes(activeSessionId)
      ? activeSessionId
      : sessionIds.length === 1
        ? sessionIds[0]
        : null;

  if (!sessionId) {
    return null;
  }

  const identity = accounts[sessionId]?.identity;
  if (!identity || typeof identity !== 'object') {
    return null;
  }

  return {
    sessionId,
    identity: {
      accessToken: typeof identity.accessToken === 'string' ? identity.accessToken : '',
      refreshToken: typeof identity.refreshToken === 'string' ? identity.refreshToken : '',
      expiresAt: typeof identity.expiresAt === 'string' ? identity.expiresAt : '',
    },
  };
}

export function findConfiguredShopifyApp(config, appHandle) {
  if (!config || typeof config !== 'object' || !appHandle) {
    return null;
  }

  for (const [entryPath, value] of Object.entries(config)) {
    if (!value || typeof value !== 'object') {
      continue;
    }

    const directory = typeof value.directory === 'string' ? value.directory : entryPath;
    const title = typeof value.title === 'string' ? value.title : null;
    const directoryName = path.basename(directory);
    if (title !== appHandle && directoryName !== appHandle) {
      continue;
    }

    const configFile = typeof value.configFile === 'string' ? value.configFile : 'shopify.app.toml';
    return {
      directory,
      configFile,
      configPath: path.join(directory, configFile),
      appId: typeof value.appId === 'string' ? value.appId : null,
      title,
    };
  }

  return null;
}

export function extractScopesFromShopifyAppToml(tomlSource) {
  if (typeof tomlSource !== 'string' || tomlSource.length === 0) {
    return [];
  }

  const blockMatch = tomlSource.match(/\[access_scopes\]([\s\S]*?)(?:\n\[[^\]]+\]|$)/);
  if (!blockMatch) {
    return [];
  }

  const scopesMatch = blockMatch[1].match(/^\s*scopes\s*=\s*"([^"]*)"/m);
  if (!scopesMatch) {
    return [];
  }

  return scopesMatch[1]
    .split(',')
    .map((scope) => scope.trim())
    .filter(Boolean);
}

export async function findShopifyChannelConfigExtensions(appDirectory) {
  if (typeof appDirectory !== 'string' || appDirectory.length === 0) {
    return [];
  }

  const extensionsDirectory = path.join(appDirectory, 'extensions');

  let entries = [];
  try {
    entries = await readdir(extensionsDirectory, { withFileTypes: true });
  } catch (error) {
    if (error?.code === 'ENOENT') {
      return [];
    }

    throw error;
  }

  const results = [];
  for (const entry of entries) {
    if (!entry.isDirectory()) {
      continue;
    }

    const extensionPath = path.join(extensionsDirectory, entry.name, 'shopify.extension.toml');
    let source = '';
    try {
      source = await readFile(extensionPath, 'utf8');
    } catch (error) {
      if (error?.code === 'ENOENT') {
        continue;
      }

      throw error;
    }

    if (!/type\s*=\s*"channel_config"/m.test(source)) {
      continue;
    }

    const handleMatch = source.match(/^\s*handle\s*=\s*"([^"]+)"/m);
    const createLegacyMatch = source.match(/^\s*create_legacy_channel_on_app_install\s*=\s*(true|false)/m);

    results.push({
      extensionPath,
      handle: handleMatch ? handleMatch[1] : entry.name,
      createLegacyChannelOnAppInstall: createLegacyMatch ? createLegacyMatch[1] === 'true' : null,
    });
  }

  return results;
}

export function isInvalidGrantRefreshResponse(payload) {
  if (!payload || typeof payload !== 'object') {
    return false;
  }

  if (payload.error === 'invalid_grant') {
    return true;
  }

  if (!Array.isArray(payload.errors)) {
    return false;
  }

  return payload.errors.some((entry) => {
    if (!entry || typeof entry !== 'object') {
      return false;
    }

    return entry.code === 'invalid_grant';
  });
}

export function extractManualStoreAuthTokenSummary(payload) {
  if (!payload || typeof payload !== 'object') {
    return null;
  }

  const accessToken = typeof payload.access_token === 'string' ? payload.access_token : '';
  if (!accessToken) {
    return null;
  }

  const scopeHandles =
    typeof payload.scope === 'string'
      ? payload.scope
          .split(',')
          .map((scope) => scope.trim())
          .filter(Boolean)
      : [];
  const associatedUserScopeHandles =
    typeof payload.associated_user_scope === 'string'
      ? payload.associated_user_scope
          .split(',')
          .map((scope) => scope.trim())
          .filter(Boolean)
      : [];
  const associatedUserEmail = typeof payload.associated_user?.email === 'string' ? payload.associated_user.email : null;

  const tokenFamilyMatch = accessToken.match(/^(shp[a-z]+)_/i);

  return {
    accessToken,
    tokenFamily: tokenFamilyMatch ? tokenFamilyMatch[1].toLowerCase() : 'unknown',
    hasRefreshToken: typeof payload.refresh_token === 'string' && payload.refresh_token.length > 0,
    scopeHandles,
    associatedUserScopeHandles,
    associatedUserEmail,
  };
}

export function shouldProbeManualStoreAuthFallback(summary) {
  return !!(
    summary &&
    typeof summary === 'object' &&
    typeof summary.accessToken === 'string' &&
    summary.accessToken.length > 0
  );
}

export function shouldAttemptShopifyAppDeploy(shopifyAppCliAuth, appScopeDrift) {
  return !!(
    shopifyAppCliAuth &&
    shopifyAppCliAuth.status === 'available' &&
    typeof shopifyAppCliAuth.workdir === 'string' &&
    shopifyAppCliAuth.workdir.length > 0 &&
    appScopeDrift &&
    Array.isArray(appScopeDrift.missingRequestedScopes) &&
    appScopeDrift.missingRequestedScopes.length > 0
  );
}

export function extractShopifyAppDeployVersion(output) {
  if (typeof output !== 'string' || output.length === 0) {
    return null;
  }

  const match = output.match(/\b([a-z0-9][a-z0-9-]*-\d+)\s+\[\d+\]/i);
  return match ? match[1] : null;
}

export function parsePublicationTargetBlocker(result) {
  const errors = Array.isArray(result?.payload?.errors) ? result.payload.errors : [];
  for (const error of errors) {
    if (error?.extensions?.code !== 'NOT_FOUND') {
      continue;
    }

    if (typeof error?.message !== 'string' || !error.message.includes("doesn't have a publication for this shop")) {
      continue;
    }

    return {
      operationName: Array.isArray(error?.path) && typeof error.path[0] === 'string' ? error.path[0] : 'unknown',
      message: error.message,
      errorCode: error.extensions.code,
    };
  }

  return null;
}

export async function loadShopifyCliConfig(configPath = getDefaultShopifyCliConfigPath()) {
  return JSON.parse(await readFile(configPath, 'utf8'));
}

export async function loadShopifyCliAppConfig(configPath = getDefaultShopifyCliAppConfigPath()) {
  return JSON.parse(await readFile(configPath, 'utf8'));
}

export async function loadShopifyAppScopeConfig(configPath) {
  return extractScopesFromShopifyAppToml(await readFile(configPath, 'utf8'));
}

export async function persistShopifyCliIdentity(configPath, config, { sessionId, identity }) {
  const rawSessionStore = config?.sessionStore;
  if (typeof rawSessionStore !== 'string') {
    throw new Error('Shopify CLI config is missing a string sessionStore payload.');
  }

  const sessionStore = JSON.parse(rawSessionStore);
  const accounts = sessionStore['accounts.shopify.com'];
  if (!accounts || typeof accounts !== 'object' || !accounts[sessionId] || typeof accounts[sessionId] !== 'object') {
    throw new Error(`Shopify CLI config is missing session ${sessionId}.`);
  }

  accounts[sessionId].identity = {
    ...accounts[sessionId].identity,
    accessToken: identity.accessToken,
    refreshToken: identity.refreshToken,
    expiresAt: identity.expiresAt,
  };
  sessionStore['accounts.shopify.com'] = accounts;
  config.sessionStore = JSON.stringify(sessionStore);
  await writeFile(configPath, `${JSON.stringify(config, null, 2)}\n`, 'utf8');
}
