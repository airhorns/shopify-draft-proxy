import { existsSync, lstatSync, readFileSync, readlinkSync, rmSync, symlinkSync } from 'node:fs';
import path from 'node:path';

const DEFAULT_CANONICAL_ENV_PATH = '/home/airhorns/code/shopify-draft-proxy/.env';

type EnsureWorkspaceEnvLinkOptions = {
  canonicalEnvPath?: string;
  envExamplePath?: string;
  envPath?: string;
  repoRoot?: string;
};

type EnsureWorkspaceEnvLinkResult = {
  ok: boolean;
  status:
    | 'already-linked'
    | 'kept-existing-env'
    | 'linked'
    | 'missing-canonical-env'
    | 'replaced-example-copy'
    | 'stale-example-copy';
  message: string;
};

function pathsEqual(left: string, right: string): boolean {
  return path.resolve(left) === path.resolve(right);
}

function fileContentsEqual(leftPath: string, rightPath: string): boolean {
  if (!existsSync(leftPath) || !existsSync(rightPath)) {
    return false;
  }

  return readFileSync(leftPath, 'utf8') === readFileSync(rightPath, 'utf8');
}

function resolveLinkTarget(linkPath: string): string {
  const target = readlinkSync(linkPath);
  return path.resolve(path.dirname(linkPath), target);
}

export function ensureWorkspaceEnvLink(options: EnsureWorkspaceEnvLinkOptions = {}): EnsureWorkspaceEnvLinkResult {
  const repoRoot = options.repoRoot ?? process.cwd();
  const envPath = options.envPath ?? path.join(repoRoot, '.env');
  const envExamplePath = options.envExamplePath ?? path.join(repoRoot, '.env.example');
  const canonicalEnvPath =
    options.canonicalEnvPath ?? process.env['SHOPIFY_DRAFT_PROXY_CANONICAL_ENV_PATH'] ?? DEFAULT_CANONICAL_ENV_PATH;

  const canonicalExists = existsSync(canonicalEnvPath) && lstatSync(canonicalEnvPath).isFile();
  const envExists = existsSync(envPath);
  const envLstat = envExists ? lstatSync(envPath) : null;
  const envIsExampleCopy =
    envLstat?.isFile() === true && existsSync(envExamplePath) && fileContentsEqual(envPath, envExamplePath);

  if (!canonicalExists) {
    if (envIsExampleCopy) {
      return {
        ok: false,
        status: 'stale-example-copy',
        message: [
          `${envPath} is byte-identical to ${envExamplePath}, but canonical env ${canonicalEnvPath} is missing.`,
          'Leaving placeholder conformance store values in place can make a valid home-folder token look invalid.',
        ].join(' '),
      };
    }

    return {
      ok: true,
      status: 'missing-canonical-env',
      message: `Canonical env ${canonicalEnvPath} is missing; no stale example-copy .env was found.`,
    };
  }

  if (envLstat?.isSymbolicLink() === true && pathsEqual(resolveLinkTarget(envPath), canonicalEnvPath)) {
    return {
      ok: true,
      status: 'already-linked',
      message: `${envPath} already links to ${canonicalEnvPath}.`,
    };
  }

  if (!envExists || envIsExampleCopy || envLstat?.isSymbolicLink() === true) {
    if (envExists) {
      rmSync(envPath);
    }
    symlinkSync(canonicalEnvPath, envPath);

    return {
      ok: true,
      status: envIsExampleCopy ? 'replaced-example-copy' : 'linked',
      message: `${envPath} now links to ${canonicalEnvPath}.`,
    };
  }

  return {
    ok: true,
    status: 'kept-existing-env',
    message: `${envPath} exists and is not an example copy; leaving it unchanged.`,
  };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const options: EnsureWorkspaceEnvLinkOptions = {};
  const canonicalEnvPath = process.env['SHOPIFY_DRAFT_PROXY_CANONICAL_ENV_PATH'];
  const envExamplePath = process.env['SHOPIFY_DRAFT_PROXY_ENV_EXAMPLE_PATH'];
  const envPath = process.env['SHOPIFY_DRAFT_PROXY_ENV_PATH'];

  if (canonicalEnvPath) {
    options.canonicalEnvPath = canonicalEnvPath;
  }
  if (envExamplePath) {
    options.envExamplePath = envExamplePath;
  }
  if (envPath) {
    options.envPath = envPath;
  }

  const result = ensureWorkspaceEnvLink(options);

  const output = result.ok ? process.stdout : process.stderr;
  output.write(`${result.message}\n`);
  if (!result.ok) {
    process.exitCode = 1;
  }
}
