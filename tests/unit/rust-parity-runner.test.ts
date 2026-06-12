import { execFileSync } from 'node:child_process';
import { mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const repoRoot = new URL('../..', import.meta.url);
const paritySpecRoot = new URL('../../config/parity-specs/', import.meta.url);
const parityCliTimeoutMs = 30_000;

function countParitySpecs(directory: URL): number {
  return readdirSync(directory, { withFileTypes: true }).reduce((count, entry) => {
    if (entry.isDirectory()) return count + countParitySpecs(new URL(`${entry.name}/`, directory));
    return entry.isFile() && entry.name.endsWith('.json') ? count + 1 : count;
  }, 0);
}

describe('Rust parity runner CLI', () => {
  it(
    'discovers every checked-in parity spec before executing scenarios',
    () => {
      const output = execFileSync('corepack', ['pnpm', 'parity:run', '--', '--dry-run'], {
        cwd: repoRoot,
        encoding: 'utf8',
      });
      expect(output).toContain(`[parity] ${countParitySpecs(paritySpecRoot)} spec(s) selected`);
    },
    parityCliTimeoutMs,
  );

  it(
    'uses the captured target response as the passthrough cassette fallback for unsupported roots',
    () => {
      const output = execFileSync(
        'corepack',
        [
          'pnpm',
          'parity',
          '--',
          '--spec',
          'config/parity-specs/admin-platform/admin-platform-backup-region-update-access-blocker.json',
        ],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('admin-platform-backup-region-update-access-blocker.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'unwraps captured response.body payloads for passthrough cassette fallbacks',
    () => {
      const output = execFileSync(
        'corepack',
        ['pnpm', 'parity', '--', '--spec', 'config/parity-specs/admin-platform/by-id-not-found-read.json'],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('by-id-not-found-read.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'does not require local Rust handlers to consume every captured upstream call when output matches',
    () => {
      const output = execFileSync(
        'corepack',
        ['pnpm', 'parity', '--', '--spec', 'config/parity-specs/products/product-empty-state-read.json'],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('product-empty-state-read.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'uses each comparison target capture as fallback even when unrelated upstream recordings remain',
    () => {
      const output = execFileSync(
        'corepack',
        [
          'pnpm',
          'parity',
          '--',
          '--spec',
          'config/parity-specs/products/collectionCreate-and-add-products-parity.json',
        ],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('collectionCreate-and-add-products-parity.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'resolves capture-path variables before replaying recorded passthrough node reads',
    () => {
      const output = execFileSync(
        'corepack',
        [
          'pnpm',
          'parity',
          '--',
          '--spec',
          'config/parity-specs/admin-platform/admin-platform-delivery-profile-node-reads.json',
        ],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('admin-platform-delivery-profile-node-reads.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'executes proxyUpload targets as side-effect assertions for staged upload parity',
    () => {
      const output = execFileSync(
        'corepack',
        [
          'pnpm',
          'parity',
          '--',
          '--spec',
          'config/parity-specs/bulk-operations/bulk-operation-run-mutation-client-identifier-validation.json',
        ],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('bulk-operation-run-mutation-client-identifier-validation.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'uses the primary capture target, not the first target request, as primary passthrough fallback',
    () => {
      const output = execFileSync(
        'corepack',
        ['pnpm', 'parity', '--', '--spec', 'config/parity-specs/customers/customer-account-page-data-erasure.json'],
        { cwd: repoRoot, encoding: 'utf8' },
      );
      expect(output).toContain('customer-account-page-data-erasure.json passed');
    },
    parityCliTimeoutMs,
  );

  it(
    'fails a no-upstream target when the response was satisfied by cassette fallback',
    () => {
      const tempDir = mkdtempSync(join(tmpdir(), 'draft-proxy-parity-spec-'));
      const specPath = join(tempDir, 'no-upstream-fallback-regression.json');
      const documentPath = join(tempDir, 'unsupported-mutation.graphql');
      const fixturePath = join(tempDir, 'unsupported-mutation.json');
      writeFileSync(
        documentPath,
        `mutation UnsupportedMutationFallbackRegression {
  definitelyUnsupportedMutation {
    userErrors {
      message
    }
  }
}
`,
      );
      writeFileSync(
        fixturePath,
        `${JSON.stringify({
          response: {
            data: {
              definitelyUnsupportedMutation: {
                userErrors: [],
              },
            },
          },
        })}\n`,
      );
      writeFileSync(
        specPath,
        `${JSON.stringify(
          {
            scenarioId: 'no-upstream-fallback-regression',
            liveCaptureFiles: [fixturePath],
            proxyRequest: {
              documentPath,
            },
            comparison: {
              expectedDifferences: [],
              targets: [
                {
                  name: 'unsupported-mutation-data',
                  capturePath: '$.response.data',
                  proxyPath: '$.data',
                  upstreamCapturePath: null,
                  expectNoUpstream: true,
                },
              ],
            },
          },
          null,
          2,
        )}\n`,
      );

      try {
        expect(() =>
          execFileSync('corepack', ['pnpm', 'parity', '--', '--spec', specPath], {
            cwd: repoRoot,
            encoding: 'utf8',
            stdio: 'pipe',
          }),
        ).toThrow(/expected local proxy handling without upstream calls/u);
      } finally {
        rmSync(tempDir, { recursive: true, force: true });
      }
    },
    parityCliTimeoutMs,
  );
});
