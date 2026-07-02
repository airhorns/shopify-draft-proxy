import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { isGraphqlDocumentText } from '../../scripts/parity-cassette.js';
import { listConformanceParitySpecPaths } from '../../scripts/conformance-scenario-registry.js';

const repoRoot = resolve(import.meta.dirname, '../..');
const descriptorPattern =
  /^(?:local-runtime|hand-synthesized|sha:|cassette-backed|recorded by|captured live hydrate)/iu;
const existingLocalRuntimeCaptureAllowlist = new Set([
  'fixtures/conformance/local-runtime/2026-04/orders/order-payment-transaction-local-staging.json',
]);

function readJson(path: string): Record<string, unknown> {
  return JSON.parse(readFileSync(resolve(repoRoot, path), 'utf8')) as Record<string, unknown>;
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === 'string') : [];
}

function recordArray(value: unknown): Array<Record<string, unknown>> {
  return Array.isArray(value)
    ? value.filter((entry): entry is Record<string, unknown> => typeof entry === 'object' && entry !== null)
    : [];
}

describe('payments parity provenance', () => {
  it('does not use local-runtime captures or descriptor cassettes as payments parity evidence', () => {
    const failures: string[] = [];
    const paymentSpecPaths = listConformanceParitySpecPaths(repoRoot).filter((path) =>
      path.startsWith('config/parity-specs/payments/'),
    );

    for (const specPath of paymentSpecPaths) {
      const spec = readJson(specPath);
      for (const captureFile of stringArray(spec['liveCaptureFiles'])) {
        if (
          captureFile.startsWith('fixtures/conformance/local-runtime/') &&
          !existingLocalRuntimeCaptureAllowlist.has(captureFile)
        ) {
          failures.push(`${specPath}: local-runtime liveCaptureFile ${captureFile}`);
          continue;
        }

        if (!existsSync(resolve(repoRoot, captureFile))) {
          failures.push(`${specPath}: missing liveCaptureFile ${captureFile}`);
          continue;
        }

        const fixture = readJson(captureFile);
        for (const [index, call] of recordArray(fixture['upstreamCalls']).entries()) {
          const query = call['query'];
          if (typeof query !== 'string') {
            failures.push(`${captureFile}: upstreamCalls[${index}].query is not a string`);
            continue;
          }

          if (descriptorPattern.test(query) || !isGraphqlDocumentText(query)) {
            failures.push(`${captureFile}: upstreamCalls[${index}].query is not exact GraphQL`);
          }
        }
      }
    }

    expect(failures).toEqual([]);
  });
});
