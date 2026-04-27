export type ShopifyCliIdentitySummary = {
  sessionId: string;
  identity: {
    accessToken: string;
    refreshToken: string;
    expiresAt: string;
  };
};

export type ConfiguredShopifyApp = {
  directory: string;
  configFile: string;
  configPath: string;
  appId: string | null;
  title: string | null;
};

export type ShopifyChannelConfigExtension = {
  extensionPath: string;
  handle: string;
  createLegacyChannelOnAppInstall: boolean | null;
};

export type ManualStoreAuthTokenSummary = {
  accessToken: string;
  tokenFamily: string;
  hasRefreshToken: boolean;
  scopeHandles: string[];
  associatedUserScopeHandles: string[];
  associatedUserEmail: string | null;
};

export type PublicationTargetBlocker = {
  operationName: string;
  message: string;
  errorCode: string;
};

export function extractCliIdentityFromConfig(config: unknown): ShopifyCliIdentitySummary | null;
export function extractManualStoreAuthTokenSummary(value: unknown): ManualStoreAuthTokenSummary | null;
export function extractScopesFromShopifyAppToml(value: unknown): string[];
export function extractShopifyAppDeployVersion(value: unknown): string | null;
export function findConfiguredShopifyApp(config: unknown, appHandle: string): ConfiguredShopifyApp | null;
export function findShopifyChannelConfigExtensions(appDirectory: string): Promise<ShopifyChannelConfigExtension[]>;
export function isInvalidGrantRefreshResponse(value: unknown): boolean;
export function parsePublicationTargetBlocker(value: unknown): PublicationTargetBlocker | null;
export function shouldAttemptShopifyAppDeploy(shopifyAppCliAuth: unknown, appScopeDrift: unknown): boolean;
export function shouldProbeManualStoreAuthFallback(summary: unknown): boolean;
