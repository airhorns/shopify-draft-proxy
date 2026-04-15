export type ReadMode = 'live-hybrid' | 'snapshot' | 'passthrough';

export interface AppConfig {
  port: number;
  shopifyAdminOrigin: string;
  readMode: ReadMode;
  snapshotPath?: string;
}

function readPort(raw: string | undefined): number {
  if (!raw) return 3000;
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(`Invalid PORT: ${raw}`);
  }
  return parsed;
}

function readMode(raw: string | undefined): ReadMode {
  if (!raw) return 'live-hybrid';
  if (raw === 'live-hybrid' || raw === 'snapshot' || raw === 'passthrough') {
    return raw;
  }
  throw new Error(`Invalid SHOPIFY_DRAFT_PROXY_READ_MODE: ${raw}`);
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): AppConfig {
  const shopifyAdminOrigin = env['SHOPIFY_ADMIN_ORIGIN'];
  if (!shopifyAdminOrigin) {
    throw new Error('Missing SHOPIFY_ADMIN_ORIGIN');
  }

  const snapshotPath = env['SHOPIFY_DRAFT_PROXY_SNAPSHOT_PATH'];

  return {
    port: readPort(env['PORT']),
    shopifyAdminOrigin,
    readMode: readMode(env['SHOPIFY_DRAFT_PROXY_READ_MODE']),
    ...(snapshotPath ? { snapshotPath } : {}),
  };
}
