---
title: HTTP Service
description: Local Node HTTP service routes and environment configuration.
---

The JavaScript target exposes a Node `http` adapter over the Gleam runtime. Start it with the root `dev` script during development or `start` after a build.

## Start Commands

```sh
SHOPIFY_ADMIN_ORIGIN=https://your-store.myshopify.com \
PORT=4000 \
corepack pnpm dev
```

```sh
corepack pnpm build
SHOPIFY_ADMIN_ORIGIN=https://your-store.myshopify.com \
PORT=4000 \
corepack pnpm start
```

The service logs a JSON line when it is listening.

## Environment

| Variable                                                                    | Required | Purpose                                                                    |
| --------------------------------------------------------------------------- | -------- | -------------------------------------------------------------------------- |
| `SHOPIFY_ADMIN_ORIGIN`                                                      | Yes      | Upstream Shopify Admin origin, such as `https://your-store.myshopify.com`. |
| `PORT`                                                                      | No       | Local service port. Defaults to `3000`.                                    |
| `SHOPIFY_DRAFT_PROXY_READ_MODE`                                             | No       | `live-hybrid`, `snapshot`, or `passthrough`. Defaults to `live-hybrid`.    |
| `SHOPIFY_DRAFT_PROXY_UNSUPPORTED_MUTATION_MODE`                             | No       | `passthrough` or `reject`. Defaults to `passthrough`.                      |
| `SHOPIFY_DRAFT_PROXY_SNAPSHOT_PATH`                                         | No       | Snapshot file loaded into the proxy at startup.                            |
| `SHOPIFY_DRAFT_PROXY_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES` | No       | Local staged bulk upload size guardrail.                                   |

## Admin GraphQL

```http
POST /admin/api/:version/graphql.json
```

The proxy accepts Shopify-shaped GraphQL JSON request bodies and forwards auth headers unchanged when it needs to call upstream Shopify.

Supported mutations stage locally and append to the mutation log. Unsupported mutations are proxied or rejected based on `unsupportedMutationMode`.

## Meta API

| Route            | Method | Behavior                                                                            |
| ---------------- | ------ | ----------------------------------------------------------------------------------- |
| `/__meta/health` | `GET`  | Returns liveness JSON.                                                              |
| `/__meta/config` | `GET`  | Returns sanitized runtime configuration.                                            |
| `/__meta/log`    | `GET`  | Returns staged, proxied, and committed mutation-log entries in replay order.        |
| `/__meta/state`  | `GET`  | Returns normalized base and staged state buckets.                                   |
| `/__meta/reset`  | `POST` | Clears staged state, logs, and synthetic identity counters.                         |
| `/__meta/commit` | `POST` | Replays staged raw mutations upstream in original order and stops on first failure. |

## Staged Uploads and Bulk Results

| Route                                              | Method        | Behavior                                                       |
| -------------------------------------------------- | ------------- | -------------------------------------------------------------- |
| `/staged-uploads/:target/:filename`                | `POST`, `PUT` | Stores a staged upload body in the instance-owned proxy store. |
| `/__meta/bulk-operations/:encoded_id/result.jsonl` | `GET`         | Serves generated local bulk operation JSONL output.            |

The local staged-upload route is used by bulk operation import flows so tests can exercise upload-backed mutations without adding a process-wide artifact cache.

## Unsupported Routes

`GET /__meta` as an operator UI is intentionally not implemented. The meta API is machine-readable for now.
