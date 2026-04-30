// Public re-exports for the Gleam-port shim. Mirrors the surface area
// of the top-level `shopify-draft-proxy` package's `src/index.ts`.
//
// `createApp` and `loadConfig` are placeholders — the HTTP server
// adapter is deferred per `GLEAM_PORT_INTENT.md`. They throw a
// not-implemented error so callers see a clear migration message
// rather than a silent shape mismatch.

export { createDraftProxy, DraftProxy } from './runtime.js';
export {
  DRAFT_PROXY_STATE_DUMP_SCHEMA,
  DraftProxyCommitError,
  type AppConfig,
  type DraftProxyCommitAttempt,
  type DraftProxyCommitResult,
  type DraftProxyConfigSnapshot,
  type DraftProxyGraphQLRequestOptions,
  type DraftProxyHeaderValue,
  type DraftProxyHttpResponse,
  type DraftProxyLogSnapshot,
  type DraftProxyOptions,
  type DraftProxyRequest,
  type DraftProxyStateDump,
  type DraftProxyStateSnapshot,
  type ReadMode,
} from './types.js';

export function createApp(): never {
  throw new Error(
    'createApp is not implemented in the Gleam-port shim yet. The HTTP server adapter (mist on Erlang, Node http on JS) is a separate task — see GLEAM_PORT_INTENT.md.',
  );
}

export function loadConfig(): never {
  throw new Error(
    'loadConfig is not implemented in the Gleam-port shim yet. Construct an AppConfig literal and pass it to createDraftProxy(...) directly.',
  );
}
