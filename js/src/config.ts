import type { AppConfig, ReadMode, UnsupportedMutationMode } from './types.js';

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

function unsupportedMutationMode(raw: string | undefined): UnsupportedMutationMode {
  if (!raw) return 'passthrough';
  if (raw === 'passthrough' || raw === 'reject') {
    return raw;
  }
  throw new Error(`Invalid SHOPIFY_DRAFT_PROXY_UNSUPPORTED_MUTATION_MODE: ${raw}`);
}

function readPositiveInt(name: string, raw: string | undefined): number | undefined {
  if (!raw) return undefined;
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(`Invalid ${name}: ${raw}`);
  }
  return parsed;
}

function readBool(name: string, raw: string | undefined): boolean | undefined {
  if (!raw) return undefined;
  if (raw === '1' || raw === 'true' || raw === 'TRUE' || raw === 'yes') {
    return true;
  }
  if (raw === '0' || raw === 'false' || raw === 'FALSE' || raw === 'no') {
    return false;
  }
  throw new Error(`Invalid ${name}: ${raw}`);
}

function readCommaSeparated(raw: string | undefined): string[] | undefined {
  if (raw === undefined) return undefined;
  const values = raw
    .split(',')
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
  return values;
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): AppConfig {
  const shopifyAdminOrigin = env['SHOPIFY_ADMIN_ORIGIN'];
  if (!shopifyAdminOrigin) {
    throw new Error('Missing SHOPIFY_ADMIN_ORIGIN');
  }

  const snapshotPath = env['SHOPIFY_DRAFT_PROXY_SNAPSHOT_PATH'];
  const bulkOperationRunMutationMaxInputFileSizeBytes = readPositiveInt(
    'SHOPIFY_DRAFT_PROXY_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES',
    env['SHOPIFY_DRAFT_PROXY_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES'],
  );
  const stagedUploadResourcePermissions = readCommaSeparated(
    env['SHOPIFY_DRAFT_PROXY_STAGED_UPLOAD_RESOURCE_PERMISSIONS'],
  );
  const forceStagedUploadUrlGenerationFailure = readBool(
    'SHOPIFY_DRAFT_PROXY_FORCE_STAGED_UPLOAD_URL_GENERATION_FAILURE',
    env['SHOPIFY_DRAFT_PROXY_FORCE_STAGED_UPLOAD_URL_GENERATION_FAILURE'],
  );

  return {
    port: readPort(env['PORT']),
    shopifyAdminOrigin,
    readMode: readMode(env['SHOPIFY_DRAFT_PROXY_READ_MODE']),
    unsupportedMutationMode: unsupportedMutationMode(env['SHOPIFY_DRAFT_PROXY_UNSUPPORTED_MUTATION_MODE']),
    ...(bulkOperationRunMutationMaxInputFileSizeBytes === undefined
      ? {}
      : { bulkOperationRunMutationMaxInputFileSizeBytes }),
    ...(stagedUploadResourcePermissions === undefined ? {} : { stagedUploadResourcePermissions }),
    ...(forceStagedUploadUrlGenerationFailure === undefined ? {} : { forceStagedUploadUrlGenerationFailure }),
    ...(snapshotPath ? { snapshotPath } : {}),
  };
}
