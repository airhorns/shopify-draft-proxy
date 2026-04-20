import { readFileSync } from 'node:fs';
import path from 'node:path';

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const worklistPath = path.join(repoRoot, 'docs', 'shopify-admin-worklist.md');
const registryPath = path.join(repoRoot, 'config', 'operation-registry.json');

const content = readFileSync(worklistPath, 'utf8');
const registry = JSON.parse(readFileSync(registryPath, 'utf8'));

if (!content.includes('## Product domain')) {
  throw new Error('Worklist must include the product domain section.');
}

const implementedWorklistOperations = new Set(
  content
    .split('\n')
    .filter((line) => line.includes('[x]'))
    .flatMap((line) => Array.from(line.matchAll(/`([^`]+)`/g), (match) => match[1])),
);

const registryByName = new Map(registry.map((entry) => [entry.name, entry]));

for (const entry of registry) {
  if (!entry.implemented) {
    continue;
  }
  if (!implementedWorklistOperations.has(entry.name)) {
    throw new Error(`Implemented registry operation ${entry.name} must appear as [x] in docs/shopify-admin-worklist.md.`);
  }
}

const assertWorklistIncludes = (snippet, message) => {
  if (!content.includes(snippet)) {
    throw new Error(message);
  }
};

const assertWorklistExcludes = (snippet, message) => {
  if (content.includes(snippet)) {
    throw new Error(message);
  }
};

const mediaOps = ['productCreateMedia', 'productUpdateMedia', 'productDeleteMedia'];
const mediaCovered = mediaOps.every((name) => registryByName.get(name)?.conformance?.status === 'covered');
if (mediaCovered) {
  assertWorklistIncludes('- [x] media-on-product read coverage', 'Covered media reads must be marked as [x] in docs/shopify-admin-worklist.md.');
  assertWorklistIncludes('- [x] media create family', 'Covered media create family must be marked as [x] in docs/shopify-admin-worklist.md.');
  assertWorklistIncludes('- [x] media update family', 'Covered media update family must be marked as [x] in docs/shopify-admin-worklist.md.');
  assertWorklistIncludes('- [x] media delete family', 'Covered media delete family must be marked as [x] in docs/shopify-admin-worklist.md.');
}

const customerOpsCovered = ['customer', 'customers'].every((name) => registryByName.get(name)?.conformance?.status === 'covered');
if (customerOpsCovered) {
  assertWorklistExcludes(
    'real customer detail fixture capture is externally blocked',
    'Covered customer detail reads must not still be documented as externally blocked in docs/shopify-admin-worklist.md.',
  );
  assertWorklistExcludes(
    'real customer catalog fixture capture is externally blocked',
    'Covered customers catalog reads must not still be documented as externally blocked in docs/shopify-admin-worklist.md.',
  );
  assertWorklistIncludes(
    'live customer detail capture now records a real Shopify fixture',
    'Covered customer detail reads must mention the live checked-in fixture in docs/shopify-admin-worklist.md.',
  );
  assertWorklistIncludes(
    'live customer catalog capture now records a real Shopify fixture',
    'Covered customers catalog reads must mention the live checked-in fixture in docs/shopify-admin-worklist.md.',
  );
}

assertWorklistIncludes(
  'the current orders-domain conformance probe on this host is auth-regressed',
  'docs/shopify-admin-worklist.md must mention that the current orders-domain probe is auth-regressed in its live-conformance status note.',
);
assertWorklistIncludes(
  '`corepack pnpm conformance:probe` currently fails with `401` / `Invalid API key or access token` against the repo credential for `very-big-test-store.myshopify.com`',
  'docs/shopify-admin-worklist.md must record the current auth-regressed order-domain probe state on the repo credential.',
);
assertWorklistIncludes(
  'the checked-in fixtures are the last verified live references and `corepack pnpm conformance:capture-orders` refreshes blocker notes without overwriting those safe fixtures with `401` payloads',
  'docs/shopify-admin-worklist.md must describe the auth-regressed fixture-preservation rule honestly.',
);
assertWorklistIncludes(
  'the same command also refreshes `pending/order-editing-conformance-scope-blocker.md` from the current auth-regressed context while preserving the last verified `write_order_edits` blocker details',
  'docs/shopify-admin-worklist.md must describe the order-editing blocker note using the current auth-regressed state.',
);
assertWorklistIncludes(
  '`pending/draft-order-read-conformance-scope-blocker.md` is recreated on the current auth-regressed run while the checked-in `draft-orders-catalog.json` / `draft-orders-count.json` fixtures remain the last verified live baseline',
  'docs/shopify-admin-worklist.md must describe the auth-regressed draft-order read baseline preservation honestly.',
);
assertWorklistIncludes(
  'the remaining creation blocker after auth repair is still `draftOrderComplete`',
  'docs/shopify-admin-worklist.md must describe the remaining creation blocker honestly while the probe is auth-regressed.',
);
assertWorklistIncludes(
  'the auth regression does not invalidate the checked-in `draftOrders` / `draftOrdersCount` baseline, and a failed repo-local refresh now has a concrete meaning on this host: `corepack pnpm conformance:refresh-auth` can return `invalid_request` / `This request requires an active refresh_token` once the saved grant is no longer refreshable',
  'docs/shopify-admin-worklist.md must describe the current inactive-refresh-token auth branch honestly.',
);
assertWorklistIncludes(
  'if `corepack pnpm conformance:refresh-auth` now fails with `invalid_request` / `This request requires an active refresh_token`, stop retrying the dead saved grant and generate a fresh manual store-auth link before rerunning `corepack pnpm conformance:probe` plus `corepack pnpm conformance:capture-orders`',
  'docs/shopify-admin-worklist.md must keep the inactive-refresh-token remediation guidance as the next step from the current auth-regressed probe.',
);
assertWorklistExcludes(
  'draft-order catalog/count still needs a non-empty Shopify baseline',
  'docs/shopify-admin-worklist.md must not regress back to the stale pre-capture draft-order read-gap wording.',
);
assertWorklistExcludes(
  'the current orders-domain conformance probe on this host is healthy again',
  'docs/shopify-admin-worklist.md must not keep stale healthy-probe wording while the probe is auth-regressed.',
);
assertWorklistExcludes(
  '`corepack pnpm conformance:probe` now succeeds against the repo credential for `very-big-test-store.myshopify.com`',
  'docs/shopify-admin-worklist.md must not claim the current order-domain conformance probe is healthy while it is auth-regressed.',
);
assertWorklistExcludes(
  'healthy capture refreshed `draft-orders-catalog.json` and `draft-orders-count.json`, so `pending/draft-order-read-conformance-scope-blocker.md` is removed while the checked-in fixtures remain the current live baseline',
  'docs/shopify-admin-worklist.md must not keep stale healthy draft-order read wording during the auth regression.',
);

console.log('worklist ok');
