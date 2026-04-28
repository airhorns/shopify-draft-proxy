import { DEFAULT_ADMIN_API_VERSION } from '../src/shopify/api-version.js';

/* oxlint-disable no-console -- CLI scripts intentionally write missing-env errors to stderr. */

export type ConformanceScriptConfig = {
  storeDomain: string;
  adminOrigin: string;
  apiVersion: string;
};

export type ConformanceScriptConfigOptions = {
  defaultApiVersion?: string;
  env?: NodeJS.ProcessEnv;
  exitOnMissing?: boolean;
  requireAdminOrigin?: boolean;
};

const STORE_DOMAIN_ENV = 'SHOPIFY_CONFORMANCE_STORE_DOMAIN';
const ADMIN_ORIGIN_ENV = 'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN';
const API_VERSION_ENV = 'SHOPIFY_CONFORMANCE_API_VERSION';

function readRequiredEnv(env: NodeJS.ProcessEnv, names: string[], exitOnMissing: boolean): Record<string, string> {
  const missingVars = names.filter((name) => !env[name]);
  if (missingVars.length > 0) {
    const message = `Missing required environment variables: ${missingVars.join(', ')}`;
    if (exitOnMissing) {
      console.error(message);
      process.exit(1);
    }

    throw new Error(message);
  }

  return Object.fromEntries(names.map((name) => [name, env[name] as string]));
}

export function readConformanceScriptConfig({
  defaultApiVersion = DEFAULT_ADMIN_API_VERSION,
  env = process.env,
  exitOnMissing = false,
  requireAdminOrigin = true,
}: ConformanceScriptConfigOptions = {}): ConformanceScriptConfig {
  const requiredNames = requireAdminOrigin ? [STORE_DOMAIN_ENV, ADMIN_ORIGIN_ENV] : [STORE_DOMAIN_ENV];
  const requiredEnv = readRequiredEnv(env, requiredNames, exitOnMissing);
  const storeDomain = requiredEnv[STORE_DOMAIN_ENV];
  if (storeDomain === undefined) {
    throw new Error(`Missing required environment variables: ${STORE_DOMAIN_ENV}`);
  }

  const requiredAdminOrigin = requiredEnv[ADMIN_ORIGIN_ENV];
  let adminOrigin: string;
  if (requireAdminOrigin) {
    if (requiredAdminOrigin === undefined) {
      throw new Error(`Missing required environment variables: ${ADMIN_ORIGIN_ENV}`);
    }
    adminOrigin = requiredAdminOrigin;
  } else {
    adminOrigin = env[ADMIN_ORIGIN_ENV] || `https://${storeDomain}`;
  }

  return {
    storeDomain,
    adminOrigin,
    apiVersion: env[API_VERSION_ENV] || defaultApiVersion,
  };
}
