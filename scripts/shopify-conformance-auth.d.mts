export const SHOPIFY_CONFORMANCE_AUTH_DIR: string;
export const SHOPIFY_CONFORMANCE_AUTH_PATH: string;
export const SHOPIFY_CONFORMANCE_PKCE_PATH: string;
export const SHOPIFY_CONFORMANCE_AUTH_REQUEST_PATH: string;

export function buildAdminAuthHeaders(token: string): Record<string, string>;
export function resolveDefaultAppRoot(options?: { repoRoot?: string }): string;
export function resolveDefaultAppEnvPath(options?: { repoRoot?: string }): string;

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
