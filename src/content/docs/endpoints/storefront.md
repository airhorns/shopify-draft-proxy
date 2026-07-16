---
title: 'Storefront API'
---

# Storefront API

The Storefront API surface covers `/api/<version>/graphql.json` requests. The proxy supports a 2026-04 slice for store-context roots, online-store content roots, and customer authentication roots that can be hydrated from authenticated Storefront reads, shared Admin-observed state, locally staged Admin content writes, or the shared customer Store. Storefront requests execute against a Storefront-specific GraphQL schema rather than the Admin schema.

## Current support and limitations

### Supported roots

Read roots:

- `shop`
- `localization`
- `locations`
- `paymentSettings`
- `publicApiVersions`
- `customer`
- `article`
- `articles`
- `blog`
- `blogByHandle`
- `blogs`
- `page`
- `pageByHandle`
- `pages`
- `menu`
- `sitemap`
- `urlRedirects`

Local staged mutation roots:

- `customerCreate`
- `customerAccessTokenCreate`
- `customerAccessTokenRenew`
- `customerAccessTokenDelete`
- `customerActivate`
- `customerActivateByUrl`
- `customerRecover`
- `customerReset`
- `customerResetByUrl`
- `customerAccessTokenCreateWithMultipass` for the captured invalid-Multipass boundary only

Unimplemented Storefront mutation roots remain unsupported for local Storefront execution. In snapshot mode, schema-valid unimplemented mutations return the Storefront snapshot mutation rejection response. In live-hybrid mode, operations containing only unimplemented Storefront roots continue through the Storefront passthrough path and are logged as Storefront traffic. Customer-auth mutations cannot be mixed with unsupported Storefront roots because that would risk forwarding a supported local write.

### Local behavior

Storefront local roots execute only for Storefront API version `2026-04`. That route uses the complete captured 2026-04 Storefront type graph as an independently cached executable schema. GraphQL operation selection, aliases, fragments, built-in directives, argument and variable coercion, field/type validation, response projection, and null propagation are therefore enforced against Storefront types. Storefront roots map to globally unique internal resolver names (`shop` becomes `storefrontShop`), so Admin roots with the same public names stay isolated from Storefront handling.

Customer-auth roots stage locally on the Storefront route and never write upstream or send email during normal runtime. They share the normalized Admin customer Store, so Storefront-created customers and Admin-created customers use one customer graph for downstream reads, Admin updates, Admin deletes, Storefront activation, password/reset state, and token-authenticated `customer(customerAccessToken:)` reads. Supported customer-auth mutations append local Storefront log entries and redact sensitive request bodies and variables from meta logs; they do not enter Admin local dispatch or Admin staged commit replay.

Storefront customer records keep non-secret password fingerprints, activation token metadata, reset-token hashes, and Storefront access-token records in instance-owned staged state. `POST /__meta/reset` clears that state, while dump/restore preserves enough non-secret state for tests to keep token lifetimes, revocation, expiry, activation, and reset behavior stable without storing raw passwords, reset tokens, or access tokens. Runtime mutation payloads still return the caller-visible access token value because Shopify does, but meta state, meta logs, captures, and parity fixtures do not retain it in cleartext.

`customerCreate` creates an enabled Storefront customer with the selected public customer fields, marketing acceptance state, empty address/order structures, and a local password fingerprint. Duplicate email, invalid email, blank password, and HTML-name branches return captured `customerUserErrors` / `userErrors` shapes without staging a customer.

`customerAccessTokenCreate`, `customerAccessTokenRenew`, and `customerAccessTokenDelete` issue, preserve, renew, and revoke opaque local Storefront customer access tokens. Invalid credentials, disabled Admin-created customers, expired/revoked token reads, and repeated deletion follow the captured null payload or top-level access-denied behavior.

Admin-created `DISABLED` customers can be activated through `customerActivate` or `customerActivateByUrl` after `customerGenerateAccountActivationUrl` stages a local non-deliverable activation URL on the Admin customer row. Successful activation marks the shared customer `ENABLED`, stores a password fingerprint, issues a Storefront access token, and makes later Admin `customerUpdate` and Storefront authenticated reads observe the same customer state. Admin `customerDelete` tombstones the shared row so token-authenticated Storefront reads return `null`.

`customerRecover`, `customerReset`, and `customerResetByUrl` model the safe local state transitions without sending recovery email. Recovery for a known customer records a hashed local reset token and timestamp; invalid reset tokens and invalid reset URLs return the captured error/nullability shapes. Successful reset updates the password fingerprint, enables the customer when needed, and issues a new local access token.

`customerAccessTokenCreateWithMultipass` is intentionally limited to the captured invalid-request boundary. The proxy has no real Multipass secret, does not decrypt or validate real Multipass payloads, and returns the captured invalid Multipass customer-user-error shape for local replay.

Live-hybrid reads hydrate missing first-slice base state through explicit Storefront upstream calls, then answer the caller from the instance-owned store. The hydrated state includes Storefront shop fields, context-keyed localization, payment settings, locations with captured cursors, and public API versions. Snapshot reads do not invent shop, localization, payment, location, market, or API-version values; empty state returns null objects or empty connections/lists according to the local no-data boundary.

The accepted `2025-01` route has no complete captured executable schema. Live-hybrid and passthrough requests for that version are forwarded unchanged; snapshot requests retain the legacy schema-shaped no-data fallback. The runtime does not silently substitute either an Admin schema or the 2026-04 Storefront schema.

Live-hybrid reads hydrate missing first-slice base state through explicit Storefront upstream calls, then answer the caller from the instance-owned store. The hydrated state includes Storefront shop fields, context-keyed localization, payment settings, locations with captured cursors, and public API versions. Snapshot reads do not invent shop, localization, payment, location, market, or API-version values. Empty connections and lists remain empty; absent objects resolve to null when the schema permits it, while absent non-null roots produce a GraphQL execution error and normal null propagation instead of invalid partial data.

`@inContext(country:, language:)` is parsed from the original operation into a reusable Storefront request context. The engine-facing copy removes only that custom directive and variables used exclusively by it because the dynamic executor cannot register its runtime behavior; all other directives and variable uses remain under normal GraphQL validation. The current context model stores country and language values.

`shop` projects selected Storefront fields from captured Storefront shop state when available. It may reuse Admin-observed `shop`, `primaryDomain`, shop policy, money-format, and payment-setting fields when those shapes line up. First-slice Storefront hydration records the stable shop policy fields `privacyPolicy`, `refundPolicy`, `shippingPolicy`, and `termsOfService`; fields such as `contactInformation`, `legalNotice`, and `termsOfSale` are version- or availability-sensitive and are outside the first-slice hydration request. The proxy does not fabricate policy handles, domains, brand assets, or payment account values when neither Storefront nor Admin state has supplied them.

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

### Boundaries

This is not local behavioral support for every field exposed by the captured `Shop`, `Localization`, `Location`, `PaymentSettings`, content, menu, sitemap, redirect, or related nested types. The schema validates those fields, but fields outside the selected and hydrated boundary have no modeled Storefront state and therefore resolve through the documented null/empty boundary or schema null propagation. Storefront `Shop` policy fields that are not selected by first-slice hydration, including `contactInformation`, `legalNotice`, and `termsOfSale`, should be treated as availability-sensitive until a dedicated live Storefront capture promotes them.

Admin blog/page/article create, update, and delete effects are visible through the Storefront content roots when those Admin operations are locally supported. Admin menu CRUD is not locally modeled, so Storefront menu support is captured Storefront hydration/restored base-state projection only. URL redirect mutation lifecycle is not implemented for Storefront.

Theme rendering, Online Store routing, canonical URL generation, storefront policy pages, product/collection content linked from menus, cart, checkout, customer email delivery, real account/recovery email URLs, real Multipass validation, and Storefront mutation domains outside the named customer-auth roots remain outside this slice unless another endpoint document names them explicitly.

Live-hybrid operations that include unimplemented roots are forwarded as one unchanged Storefront request, while snapshot mode returns schema-shaped no-data behavior or rejects mutations.
