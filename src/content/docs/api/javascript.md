---
title: JavaScript Library
description: TypeScript and JavaScript API reference for embedding the proxy.
---

The npm package entry point is `shopify-draft-proxy`. It exports a TypeScript shim over the Gleam-emitted JavaScript runtime.

## Exports

```ts
import {
  createApp,
  createDraftProxy,
  DraftProxy,
  DraftProxyHttpApp,
  loadConfig,
  DraftProxyCommitError,
} from 'shopify-draft-proxy';
```

The package also exports TypeScript types for requests, responses, config snapshots, mutation logs, state dumps, commit reports, read modes, and unsupported mutation modes.

## Configuration

```ts
type ReadMode = 'snapshot' | 'live-hybrid' | 'passthrough';
type UnsupportedMutationMode = 'passthrough' | 'reject';

interface AppConfig {
  readMode: ReadMode;
  port: number;
  shopifyAdminOrigin: string;
  snapshotPath?: string;
  unsupportedMutationMode?: UnsupportedMutationMode;
  bulkOperationRunMutationMaxInputFileSizeBytes?: number;
}
```

`live-hybrid` is the default service posture. `snapshot` avoids upstream reads and answers from local state. `passthrough` is a debugging baseline, not a way to mark known mutation roots as supported.

## Embedded Request API

```ts
import { createDraftProxy } from 'shopify-draft-proxy';

const proxy = createDraftProxy({
  readMode: 'snapshot',
  unsupportedMutationMode: 'reject',
  port: 4000,
  shopifyAdminOrigin: 'https://your-store.myshopify.com',
});

const response = await proxy.processRequest({
  method: 'POST',
  path: '/admin/api/2025-01/graphql.json',
  headers: {
    'x-shopify-access-token': 'shpat_test_token',
  },
  body: {
    query: '{ shop { name } }',
  },
});

console.log(response.status, response.body);
```

The JavaScript class mutates its private inner proxy value after each request. That keeps JS tests ergonomic while preserving the Gleam runtime invariant: every request produces a next `DraftProxy` state.

## GraphQL Convenience API

```ts
await proxy.processGraphQLRequest(
  {
    query: 'query Product($id: ID!) { product(id: $id) { id title } }',
    variables: { id: 'gid://shopify/Product/1' },
  },
  {
    apiVersion: '2025-01',
    headers: { 'x-shopify-access-token': 'shpat_test_token' },
  },
);
```

Use `processRequest()` when testing exact HTTP route behavior. Use `processGraphQLRequest()` when the test only needs Admin GraphQL dispatch.

## State and Meta Helpers

```ts
proxy.getConfig();
proxy.getLog();
proxy.getState();

const dump = proxy.dumpState();
proxy.restoreState(dump);

proxy.reset();
```

State dumps include the normalized store and synthetic identity cursor so a test can persist and restore an isolated proxy session.

## Commit Replay

```ts
try {
  const result = await proxy.commit({
    'x-shopify-access-token': 'shpat_real_token',
  });
  console.log(result.attempts.length);
} catch (error) {
  if (error instanceof DraftProxyCommitError) {
    console.error(error.result.stopIndex, error.result.attempts);
  }
}
```

Commit replay sends the original staged mutation bodies upstream in original order and stops on the first failed attempt.

## HTTP Adapter

```ts
import { createApp } from 'shopify-draft-proxy';

const app = createApp({
  readMode: 'live-hybrid',
  unsupportedMutationMode: 'passthrough',
  port: 4000,
  shopifyAdminOrigin: 'https://your-store.myshopify.com',
});

app.listen(4000);
```

`createApp(config, proxy?)` returns a `DraftProxyHttpApp` with `callback()`, `listen()`, and `handle()` methods backed by Node `http`.
