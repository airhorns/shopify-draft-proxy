---
title: 'Storefront API'
---

# Storefront API

The Storefront API surface covers `/api/<version>/graphql.json` requests. The proxy supports a read-only 2026-04 slice for store context roots, online-store content roots, and custom-data roots that can be hydrated from authenticated Storefront reads, shared Admin-observed state, or locally staged Admin content writes.

## Current support and limitations

### Supported roots

Read roots:

- `shop`
- `localization`
- `locations`
- `paymentSettings`
- `publicApiVersions`
- `article`
- `articles`
- `blog`
- `blogByHandle`
- `blogs`
- `page`
- `pageByHandle`
- `pages`
- `metaobject`
- `metaobjects`
- `menu`
- `sitemap`
- `urlRedirects`

Mutation roots remain unsupported for local Storefront execution. In snapshot mode they return the Storefront snapshot mutation rejection response; in live-hybrid mode unimplemented Storefront roots continue through the Storefront passthrough path and are logged as Storefront traffic.

### Local behavior

Storefront local roots dispatch only for Storefront API version `2026-04`. Dispatch is keyed by the Storefront surface plus parsed root fields, so Admin roots with the same names stay isolated from Storefront handling. Selection aliases, fragments, built-in directives, GraphQL validation, and the selected API version are preserved by the Storefront route before local projection runs.

Live-hybrid reads hydrate missing first-slice base state through explicit Storefront upstream calls, then answer the caller from the instance-owned store. The hydrated state includes Storefront shop fields, context-keyed localization, payment settings, locations with captured cursors, and public API versions. Snapshot reads do not invent shop, localization, payment, location, market, or API-version values; empty state returns null objects or empty connections/lists according to the local no-data boundary.

`@inContext(country:, language:)` is parsed into a reusable Storefront request context. The current context model stores country and language values and leaves room for later buyer, company, and location context without adding a separate dispatcher.

`shop` projects selected Storefront fields from captured Storefront shop state when available. It may reuse Admin-observed `shop`, `primaryDomain`, shop policy, money-format, and payment-setting fields when those shapes line up. It does not fabricate policy handles, domains, brand assets, or payment account values when neither Storefront nor Admin state has supplied them.

`localization` is context-keyed. Default context and `@inContext(country:, language:)` reads hydrate separate records so later Storefront calls can observe the same country, language, and market selection without another upstream request.

`locations` projects a Storefront connection from captured Storefront locations plus locally staged or Admin-observed active, non-fulfillment-service locations. Captured Storefront cursors are retained. Locally observed Admin locations use deterministic ID cursors when no Storefront cursor has been captured. `first`, `after`, `last`, `before`, `reverse`, and representative Storefront sort keys are handled through the shared connection helpers.

`paymentSettings` uses captured Storefront payment settings first. When only Admin shop state is available, it projects the overlapping currency, presentment currency, country, and digital wallet fields and leaves Storefront-only payment fields null or absent according to the caller selection.

`publicApiVersions` returns captured Storefront API version records only after Storefront hydration. Snapshot mode returns an empty list rather than deriving API versions from checked-in schema metadata.

`article` and `page` support ID lookup for locally staged visible content. `blog` and `pageByHandle` support handle lookup, and `blogByHandle` follows the Storefront alias root for blog handle lookup. Missing IDs or handles return `null`.

`articles`, `blogs`, and `pages` project Storefront connections from shared staged Admin online-store content. They support `first`, `after`, `last`, `before`, `reverse`, representative `sortKey` values, and Storefront-style search terms through the shared staged-connection helpers. Locally staged cursors are deterministic resource-ID cursors; captured Storefront cursors are preserved only for hydrated Storefront base state.

Article and page visibility follows the staged Admin content publication flags. Unpublished or deleted staged articles and pages do not appear through Storefront singular roots, nested blog article reads, content connections, or sitemap resources. Blogs remain visible as content containers while locally staged and not deleted.

Projected content fields include the selected handle, title, body/content HTML and text summaries, article tags, author/authorV2 names, default-null SEO fields, blog nesting, `Blog.articleByHandle`, `Blog.articles`, and distinct author lists for staged blog articles. Unsupported content subfields return Shopify-like null or empty values rather than fabricated data.

`menu(handle:)` reads from authenticated Storefront menu hydration in live-hybrid mode or restored Storefront base state in snapshot mode. It preserves captured nested items, item counts, item/resource IDs, resource links, tags, URLs, and resource union selections. Snapshot mode returns `null` for absent menus instead of inventing a main menu.

`sitemap(type:)` projects sitemap resources from locally visible staged content for `ARTICLE`, `BLOG`, and `PAGE`. It returns Shopify-like count objects, resource windows, and selected resource fields for modeled staged records only; snapshot mode does not fabricate default sitemap URLs or theme routes.

`urlRedirects` projects the staged URL redirect state already modeled by the Admin online-store surface. Empty/no-data queries return an empty connection with Storefront pageInfo shape. Storefront does not create, update, or delete redirects locally.

`metaobject(handle:)`, `metaobject(id:)`, and `metaobjects(type:)` read from the shared normalized Admin metaobject and metaobject definition state. Storefront reads only expose entries whose definition has `access.storefront: PUBLIC_READ`; publishable definitions also require the entry status to be `ACTIVE`. Draft, private, deleted, missing, or unsupported-type entries return `null` through singular roots and are omitted from connections. Storefront field projection uses Storefront shape (`key`, `type`, `value`, `reference`, and `references`) and orders `fields` by key to match captured Storefront output.

Storefront metaobject connections support `first`, `after`, `last`, `before`, `reverse`, and representative string `sortKey` values through the shared staged-connection helpers. Locally staged cursors are deterministic staged-resource cursors; captured Storefront cursors remain opaque in fixtures.

Metaobject reference and list-reference fields resolve through the Storefront-visible node boundary. Visible referenced metaobjects project as Storefront `Metaobject` nodes; draft, private, deleted, missing, or unsupported references resolve to `null` or are omitted from reference connections. Cycles are bounded by the caller selection depth because the proxy only projects the selected nested fields.

`shop.metafield(namespace:, key:)` and `shop.metafields(identifiers:)` expose staged Shop-owned metafields only when the matching metafield definition has `access.storefront: PUBLIC_READ`. Definitions with `NONE`, missing identifiers, and missing records return `null` in the selected Storefront shape. Shop metafield reads can be answered from staged Shop owner-metafield state without hydrating broader Shop fields when the caller selects only `metafield`, `metafields`, and `__typename`.

### Boundaries

This is not support for every field on `Shop`, `Localization`, `Location`, `PaymentSettings`, content, menu, sitemap, redirect, or related nested types. Fields outside the selected and hydrated boundary return null/empty values when no shared store state has supplied them.

Admin blog/page/article create, update, and delete effects are visible through the Storefront content roots when those Admin operations are locally supported. Admin menu CRUD is not locally modeled, so Storefront menu support is captured Storefront hydration/restored base-state projection only. URL redirect mutation lifecycle is not implemented for Storefront.

Storefront metafields for Product, ProductVariant, Collection, Customer, Article, Blog, Page, and other HasMetafields owners remain deferred until those owner models expose Storefront-visible owner reads. Metaobject-owned metafields are not a separate owner-metafield surface in this slice; metaobject custom fields are supported through `Metaobject.field` and `Metaobject.fields`.

Storefront field `jsonValue` is not selected by the supported Storefront custom-data projection because the live Storefront schema exposes `value`, `reference`, and `references` for these public types. Admin custom-data serializers still expose Admin `jsonValue` where those Admin roots support it.

Theme rendering, Online Store routing, canonical URL generation, storefront policy pages, product/collection content linked from menus, cart, customer, checkout, and Storefront mutation domains remain outside this slice unless another endpoint document names them explicitly.
