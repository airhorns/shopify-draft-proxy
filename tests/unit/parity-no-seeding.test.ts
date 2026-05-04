// Lockdown lint: enforces the steady-state parity contract.
//
// The parity runner used to pre-seed `base_state` from each capture's
// `seedX` keys before running the proxy request. That hid real coverage
// gaps. The cassette-playback model (see `docs/parity-runner.md`)
// replaces seeding with `LiveHybrid` + recorded `upstreamCalls`. This
// test fails CI if any of the cheating patterns reappear:
//
// - The Gleam parity runner reachable from `run_with_config` calls a
//   `seed_*_preconditions` helper, OR calls `store.upsert_base_*`.
// - Any capture under `fixtures/conformance/**` carries a top-level
//   `seedProducts` / `seedCustomers` / etc. key.
//
import { readFileSync } from 'node:fs';
import { readdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');

const RUNNER_PATH = resolve(repoRoot, 'gleam/test/parity/runner.gleam');
const FIXTURES_ROOT = resolve(repoRoot, 'fixtures/conformance');

const FORBIDDEN_SEED_KEYS = [
  'seedProducts',
  'seedCustomers',
  'seedCollections',
  'seedOrders',
  'seedDiscounts',
  'seedInventory',
  'seedMetafields',
  'seedMetaobjects',
  'seedMarkets',
  'seedPriceLists',
  'seedCatalogs',
  'seedWebPresences',
  'seedB2bCompanies',
  'seedShippingProfiles',
  'seedFulfillmentOrders',
  'seedGiftCards',
  'seedDraftOrders',
  'seedPaymentMethods',
  'seedTranslations',
  'seedSegments',
  'seedFiles',
  'seedMarketingActivities',
  'seedOnlineStorePages',
  'seedOnlineStoreArticles',
  'seedOnlineStoreBlogs',
  'seedAdminPlatformNodes',
  'seedBulkOperations',
];

async function walkJsonFiles(dir: string): Promise<string[]> {
  const out: string[] = [];
  async function visit(d: string): Promise<void> {
    const entries = await readdir(d, { withFileTypes: true });
    for (const entry of entries) {
      const path = resolve(d, entry.name);
      if (entry.isDirectory()) {
        await visit(path);
      } else if (entry.isFile() && entry.name.endsWith('.json')) {
        out.push(path);
      }
    }
  }
  await visit(dir);
  return out;
}

describe('parity lockdown lint', () => {
  it('run_with_config does not reach any seed_*_preconditions helper', () => {
    const source = readFileSync(RUNNER_PATH, 'utf8');
    const lines = source.split('\n');
    const startIdx = lines.findIndex((line) => /^pub\s+fn\s+run_with_config\b/.test(line));
    expect(startIdx, 'run_with_config not found in runner.gleam').toBeGreaterThan(-1);
    let endIdx = lines.length;
    for (let i = startIdx + 1; i < lines.length; i++) {
      const line = lines[i] ?? '';
      if (/^(?:pub\s+)?fn\s+/.test(line) || /^pub\s+type\s+/.test(line) || /^type\s+/.test(line)) {
        endIdx = i;
        break;
      }
    }
    const offenders: string[] = [];
    for (let i = startIdx; i < endIdx; i++) {
      const line = lines[i] ?? '';
      if (/\bseed_[a-zA-Z0-9_]+\b/.test(line)) {
        offenders.push(`runner.gleam:${i + 1}: ${line.trim()}`);
      }
    }
    expect(offenders, `run_with_config body must not reference any seed_* helper:\n${offenders.join('\n')}`).toEqual(
      [],
    );
  });

  it('runner.gleam does not call store.upsert_base_* mutators', () => {
    const source = readFileSync(RUNNER_PATH, 'utf8');
    const lines = source.split('\n');
    const offenders: string[] = [];
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i] ?? '';
      if (/store\w*\.upsert_base_/.test(line) || /\|>\s*upsert_base_/.test(line)) {
        offenders.push(`runner.gleam:${i + 1}: ${line.trim()}`);
      }
    }
    expect(offenders, `Parity runner must not call upsert_base_*:\n${offenders.join('\n')}`).toEqual([]);
  });

  it('no capture under fixtures/conformance/ has top-level seedX keys', async () => {
    const captureFiles = await walkJsonFiles(FIXTURES_ROOT);
    const offenders: string[] = [];
    for (const path of captureFiles) {
      const source = readFileSync(path, 'utf8');
      let parsed: unknown;
      try {
        parsed = JSON.parse(source);
      } catch {
        continue;
      }
      if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) continue;
      const keys = Object.keys(parsed as Record<string, unknown>);
      for (const key of keys) {
        if (FORBIDDEN_SEED_KEYS.includes(key)) {
          offenders.push(`${path.replace(repoRoot + '/', '')}: top-level "${key}"`);
        }
      }
    }
    expect(offenders, `Captures must not carry seedX cheating keys:\n${offenders.join('\n')}`).toEqual([]);
  });
});
