//// Long-lived runtime state types for the Gleam proxy port.
////
//// `DraftProxy` and its companion types (`Config`, `ReadMode`) live in
//// this module rather than `draft_proxy.gleam` so domain modules
//// (`customers`, `products`, …) can take `DraftProxy` as a parameter
//// without importing `draft_proxy.gleam` and creating an import cycle.
////
//// `draft_proxy.gleam` re-exports these types via type aliases plus
//// thin delegating constructors so existing public callers
//// (`draft_proxy.new()`, `draft_proxy.with_config(...)`, etc.) keep
//// compiling unchanged.

import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/proxy/operation_registry.{type RegistryEntry}
import shopify_draft_proxy/proxy/operation_registry_data
import shopify_draft_proxy/shopify/upstream_client.{type SyncTransport}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

/// How the proxy answers reads. Mirrors the TS `AppConfig['readMode']`.
/// Only the variants actually exercised by the spike are modelled; any
/// extension to TS will need a corresponding variant here.
pub type ReadMode {
  Snapshot
  LiveHybrid
  Live
}

/// Sanitised configuration the proxy was constructed with. Mirrors the
/// fields of `AppConfig` that surface through `GET /__meta/config`.
pub type Config {
  Config(
    read_mode: ReadMode,
    port: Int,
    shopify_admin_origin: String,
    snapshot_path: Option(String),
  )
}

/// Long-lived runtime state owned by the proxy. The TS class wraps
/// this in a stateful `DraftProxy`; here it's just a record threaded
/// through each request.
pub type DraftProxy {
  DraftProxy(
    config: Config,
    synthetic_identity: SyntheticIdentityRegistry,
    store: Store,
    /// Registry-driven dispatch table. Empty by default — when empty,
    /// the dispatcher falls back to the hardcoded `domain_for`
    /// predicates (matches Pass 1–7 behavior so existing tests keep
    /// working). Load via `with_registry` once a real config is
    /// available.
    registry: List(RegistryEntry),
    /// Optional injected transport for upstream calls. When set, every
    /// upstream call (passthrough + handler-issued reads via
    /// `proxy/upstream_query`) is routed through it instead of the
    /// default `upstream_client.send_sync`/`send_async` shims. Parity
    /// tests install a cassette here; production leaves it `None` and
    /// hits real Shopify.
    upstream_transport: Option(SyncTransport),
  )
}

/// Default config, mirroring the values the TS test suite uses when no
/// explicit config is supplied.
pub fn default_config() -> Config {
  Config(
    read_mode: Snapshot,
    port: 4000,
    shopify_admin_origin: "https://shopify.com",
    snapshot_path: None,
  )
}

/// Fresh proxy with default config. Equivalent to `new DraftProxy(...)`.
pub fn new() -> DraftProxy {
  with_config(default_config())
}

/// Fresh proxy with the supplied config.
pub fn with_config(config: Config) -> DraftProxy {
  DraftProxy(
    config: config,
    synthetic_identity: synthetic_identity.new(),
    store: store.new(),
    registry: [],
    upstream_transport: None,
  )
}

/// Install an injected upstream transport. Used by the parity runner
/// to wire a recorded cassette into the proxy; production callers
/// leave this unset.
pub fn with_upstream_transport(
  proxy: DraftProxy,
  transport: SyncTransport,
) -> DraftProxy {
  DraftProxy(..proxy, upstream_transport: Some(transport))
}

/// Attach a parsed operation registry to the proxy. Once attached,
/// query/mutation dispatch routes by capability instead of the
/// hardcoded predicates. Mirrors the dispatcher transition the TS
/// proxy made when `operation-registry.json` started driving
/// `routes.ts`.
pub fn with_registry(
  proxy: DraftProxy,
  registry: List(RegistryEntry),
) -> DraftProxy {
  DraftProxy(..proxy, registry: registry)
}

/// Attach the vendored default registry built from
/// `config/operation-registry.json`.
pub fn with_default_registry(proxy: DraftProxy) -> DraftProxy {
  with_registry(proxy, operation_registry_data.default_registry())
}
