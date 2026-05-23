import { execFileSync } from 'node:child_process';
import { describe, expect, it } from 'vitest';

const repoRoot = new URL('../..', import.meta.url);

describe('Rust parity runner CLI', () => {
  it('discovers the same full parity corpus as main before executing scenarios', () => {
    const output = execFileSync('corepack', ['pnpm', 'parity:run', '--', '--dry-run'], {
      cwd: repoRoot,
      encoding: 'utf8',
    });
    expect(output).toContain('[parity] 911 spec(s) selected');
  });

  it('uses the captured target response as the passthrough cassette fallback for unsupported roots', () => {
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
  });

  it('unwraps captured response.body payloads for passthrough cassette fallbacks', () => {
    const output = execFileSync(
      'corepack',
      ['pnpm', 'parity', '--', '--spec', 'config/parity-specs/admin-platform/by-id-not-found-read.json'],
      { cwd: repoRoot, encoding: 'utf8' },
    );
    expect(output).toContain('by-id-not-found-read.json passed');
  });

  it('does not require local Rust handlers to consume every captured upstream call when output matches', () => {
    const output = execFileSync(
      'corepack',
      ['pnpm', 'parity', '--', '--spec', 'config/parity-specs/products/product-empty-state-read.json'],
      { cwd: repoRoot, encoding: 'utf8' },
    );
    expect(output).toContain('product-empty-state-read.json passed');
  });

  it('uses each comparison target capture as fallback even when unrelated upstream recordings remain', () => {
    const output = execFileSync(
      'corepack',
      ['pnpm', 'parity', '--', '--spec', 'config/parity-specs/products/collectionCreate-and-add-products-parity.json'],
      { cwd: repoRoot, encoding: 'utf8' },
    );
    expect(output).toContain('collectionCreate-and-add-products-parity.json passed');
  });

  it('resolves capture-path variables before replaying recorded passthrough node reads', () => {
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
  });

  it('executes proxyUpload targets as side-effect assertions for staged upload parity', () => {
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
  });

  it('uses the primary capture target, not the first target request, as primary passthrough fallback', () => {
    const output = execFileSync(
      'corepack',
      ['pnpm', 'parity', '--', '--spec', 'config/parity-specs/customers/customer-account-page-data-erasure.json'],
      { cwd: repoRoot, encoding: 'utf8' },
    );
    expect(output).toContain('customer-account-page-data-erasure.json passed');
  });
});
