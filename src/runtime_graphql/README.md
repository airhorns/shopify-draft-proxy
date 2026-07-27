# Runtime GraphQL documents

These documents belong to the runtime implementation. Parity requests under
`config/parity-requests` are evidence-replay inputs and are never compiled into
the proxy.

Files ending in `.graphql.raw` preserve the whitespace of established upstream
requests. The strict cassette matcher treats GraphQL document text as part of
the request identity, so formatters must not rewrite those files. Two legacy
documents had no final newline; their call sites remove the source file's final
newline before transport. Ordinary `.graphql` files use the repository
formatter.
