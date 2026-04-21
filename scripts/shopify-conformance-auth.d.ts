export declare const SHOPIFY_CONFORMANCE_AUTH_DIR: string;
export declare const SHOPIFY_CONFORMANCE_AUTH_PATH: string;
export declare const SHOPIFY_CONFORMANCE_PKCE_PATH: string;
export declare const SHOPIFY_CONFORMANCE_AUTH_REQUEST_PATH: string;

export declare function buildAdminAuthHeaders(token: string): Record<string, string>;
export declare function resolveDefaultAppRoot(options?: { repoRoot?: string }): string;
export declare function resolveDefaultAppEnvPath(options?: { repoRoot?: string }): string;

export declare function refreshConformanceAccessToken(options?: {
  credentialPath?: string;
  appEnvPath?: string;
  fetchImpl?: typeof fetch;
}): Promise<Record<string, unknown>>;

export declare function getValidConformanceAccessToken(options?: {
  adminOrigin: string;
  apiVersion?: string;
  credentialPath?: string;
  appEnvPath?: string;
  fetchImpl?: typeof fetch;
}): Promise<string>;

export declare function createConformanceAuthRequest(options?: {
  storeDomain: string;
  clientId: string;
  scopes: string[];
  redirectUri?: string;
  authRequestPath?: string;
  pkcePath?: string;
}): Promise<Record<string, unknown>>;

export declare function exchangeConformanceAuthCallback(options?: {
  callbackUrl: string;
  credentialPath?: string;
  authRequestPath?: string;
  appEnvPath?: string;
  fetchImpl?: typeof fetch;
}): Promise<Record<string, unknown>>;
