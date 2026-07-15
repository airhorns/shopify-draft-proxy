export const SHOPIFY_CONFORMANCE_AUTH_DIR: string;
export const SHOPIFY_CONFORMANCE_AUTH_PATH: string;
export const SHOPIFY_CONFORMANCE_PKCE_PATH: string;
export const SHOPIFY_CONFORMANCE_AUTH_REQUEST_PATH: string;
export const STOREFRONT_CONFORMANCE_APP_HANDLE: string;
export const STOREFRONT_CONFORMANCE_REDIRECT_URI: string;
export const SHOPIFY_CONFORMANCE_STOREFRONT_ADMIN_AUTH_PATH: string;
export const SHOPIFY_CONFORMANCE_STOREFRONT_ADMIN_PKCE_PATH: string;
export const SHOPIFY_CONFORMANCE_STOREFRONT_ADMIN_AUTH_REQUEST_PATH: string;
export const SHOPIFY_CONFORMANCE_STOREFRONT_AUTH_PATH: string;

export type StoredStorefrontAuth = {
  shop: string;
  storefront_access_token: string;
  storefront_token_id: string;
  storefront_token_title: string;
  storefront_access_scopes: string[];
  obtained_at: string;
};

export function buildAdminAuthHeaders(token: string): Record<string, string>;
export function buildStorefrontRequestHeaders(storefrontToken: string): Record<string, string>;
export function getStorefrontConformanceAuthProfile(): {
  appHandle: string;
  redirectUri: string;
  credentialPath: string;
  authRequestPath: string;
  pkcePath: string;
};
export function resolveDefaultAppRoot(options?: { repoRoot?: string; appHandle?: string }): string;
export function resolveDefaultAppEnvPath(options?: { repoRoot?: string; appHandle?: string }): string;

export function refreshConformanceAccessToken(options?: {
  credentialPath?: string;
  appEnvPath?: string;
  fetchImpl?: typeof fetch;
}): Promise<Record<string, unknown>>;

export function getValidConformanceAccessToken(options?: {
  adminOrigin: string;
  apiVersion?: string;
  credentialPath?: string;
  appEnvPath?: string;
  fetchImpl?: typeof fetch;
}): Promise<string>;

export function grantStorefrontAccessToken(options: {
  adminOrigin: string;
  apiVersion?: string;
  title?: string;
  credentialPath?: string;
  storefrontCredentialPath?: string;
  appEnvPath?: string;
  fetchImpl?: typeof fetch;
}): Promise<StoredStorefrontAuth>;

export function getStoredStorefrontAccessToken(options?: {
  storefrontCredentialPath?: string;
}): Promise<StoredStorefrontAuth>;

export function createConformanceAuthRequest(options?: {
  storeDomain: string;
  clientId: string;
  scopes: string[];
  redirectUri?: string;
  authRequestPath?: string;
  pkcePath?: string;
}): Promise<Record<string, unknown>>;

export function exchangeConformanceAuthCallback(options?: {
  callbackUrl: string;
  credentialPath?: string;
  authRequestPath?: string;
  appEnvPath?: string;
  fetchImpl?: typeof fetch;
}): Promise<Record<string, unknown>>;
