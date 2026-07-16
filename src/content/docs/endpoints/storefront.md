---
title: 'Storefront API'
---

# Storefront API

The Storefront API surface covers `/api/<version>/graphql.json` requests. The proxy supports a 2026-04 slice for store-context roots, core catalog product and collection reads, online-store content roots, custom-data roots, and customer authentication roots that can be hydrated from authenticated Storefront reads, shared Admin-observed state, locally staged Admin writes, shared custom-data state, or the shared customer Store. Storefront requests execute against a Storefront-specific GraphQL schema rather than the Admin schema.

## Current support and limitations

### Supported roots

Read roots:

- `shop`
- `localization`
- `locations`
- `paymentSettings`
- `collection`
- `collectionByHandle`
- `collections`
- `product`
- `productByHandle`
- `productRecommendations`
- `productTags`
- `productTypes`
- `products`
- `cart`
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
- `metaobject`
- `metaobjects`
- `menu`
- `sitemap`
- `urlRedirects`
- `node`
- `nodes`
- `search`
- `predictiveSearch`

Local staged mutation roots:

- `cartCreate`
- `cartLinesAdd`
- `cartLinesUpdate`
- `cartLinesRemove`
- `cartAttributesUpdate`
- `cartNoteUpdate`
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
- `customerUpdate`
- `customerAddressCreate`
- `customerAddressUpdate`
- `customerAddressDelete`
- `customerDefaultAddressUpdate`

Unimplemented Storefront mutation roots remain unsupported for local Storefront execution. In snapshot mode, schema-valid unimplemented mutations return the Storefront snapshot mutation rejection response. In live-hybrid mode, operations containing only unimplemented Storefront roots continue through the Storefront passthrough path and are logged as Storefront traffic. Customer-auth mutations cannot be mixed with unsupported Storefront roots because that would risk forwarding a supported local write.

### Local behavior

Storefront local roots execute only for Storefront API version `2026-04`. That route uses the complete captured 2026-04 Storefront type graph as an independently cached executable schema. GraphQL operation selection, aliases, fragments, built-in directives, argument and variable coercion, field/type validation, response projection, and null propagation are therefore enforced against Storefront types. Storefront roots map to globally unique internal resolver names (`shop` becomes `storefrontShop`), so Admin roots with the same public names stay isolated from Storefront handling.

Customer-auth roots stage locally on the Storefront route and never write upstream or send email during normal runtime. They share the normalized Admin customer Store, so Storefront-created customers and Admin-created customers use one customer graph for downstream reads, Admin updates, Admin deletes, Storefront activation, password/reset state, and token-authenticated `customer(customerAccessToken:)` reads. Supported customer-auth mutations append local Storefront log entries and redact sensitive request bodies and variables from meta logs; they do not enter Admin local dispatch or Admin staged commit replay.

Cart roots stage and read normalized, instance-owned cart and line state on the Storefront route. `cartCreate`, line add/update/remove, attribute update, and note update never write upstream during normal proxy use and do not enter Admin commit replay. `cart(id:)` returns locally staged cart state; unknown, malformed, reset, or other-instance cart IDs return `null`.

Cart IDs, keys, checkout URLs, and line IDs are deterministic opaque synthetic values scoped to one proxy instance. Dump/restore serializes only non-secret internal sequences and normalized records, then reconstructs the same synthetic public identifiers for callers that retain a cart ID. Cart Storefront log entries retain operation type, root names, execution mode, and status while redacting the GraphQL document, variables, and raw body so cart keys and tokens never enter meta logs.

Cart lines reference visible variants from the shared normalized product catalog. Prices, compare-at prices, currency, tracked inventory, availability, variant titles, and total costs derive from that shared state. Identical merchandise, selling-plan, and attribute combinations merge; distinct attributes keep separate lines. Tracked inventory caps quantities across matching variant lines, returns captured not-enough-stock warnings, and retains a zero-quantity line plus out-of-stock warning when a distinct line cannot receive inventory. Zero-quantity line-add input is a no-op, while line update quantity zero removes the line.

Cart and line attributes preserve first-key order with the last duplicate value winning. Mutations enforce the captured 250-element line/attribute limits, the 5,000-character note limit, invalid merchandise and selling-plan branches, missing carts, and stale lines. Cart mutation payloads return captured `CartUserError` and `CartWarning` shapes. Line and cart totals use the shared money helpers; duty and tax amounts remain `null` and estimated flags remain true until checkout context is modeled.

Storefront customer records keep non-secret password fingerprints, activation token metadata, reset-token hashes, and Storefront access-token records in instance-owned staged state. `POST /__meta/reset` clears that state, while dump/restore preserves enough non-secret state for tests to keep token lifetimes, revocation, expiry, activation, and reset behavior stable without storing raw passwords, reset tokens, or access tokens. Runtime mutation payloads still return the caller-visible access token value because Shopify does, but meta state, meta logs, captures, and parity fixtures do not retain it in cleartext.

`customerCreate` creates an enabled Storefront customer with the selected public customer fields, marketing acceptance state, empty address/order structures, and a local password fingerprint. Duplicate email, invalid email, blank password, and HTML-name branches return captured `customerUserErrors` / `userErrors` shapes without staging a customer.

`customerAccessTokenCreate`, `customerAccessTokenRenew`, and `customerAccessTokenDelete` issue, preserve, renew, and revoke opaque local Storefront customer access tokens. Invalid credentials, disabled Admin-created customers, expired/revoked token reads, and repeated deletion follow the captured null payload or top-level access-denied behavior.

Admin-created `DISABLED` customers can be activated through `customerActivate` or `customerActivateByUrl` after `customerGenerateAccountActivationUrl` stages a local non-deliverable activation URL on the Admin customer row. Successful activation marks the shared customer `ENABLED`, stores a password fingerprint, issues a Storefront access token, and makes later Admin `customerUpdate` and Storefront authenticated reads observe the same customer state. Admin `customerDelete` tombstones the shared row so token-authenticated Storefront reads return `null`.

`customerRecover`, `customerReset`, and `customerResetByUrl` model the safe local state transitions without sending recovery email. Recovery for a known customer records a hashed local reset token and timestamp; invalid reset tokens and invalid reset URLs return the captured error/nullability shapes. Successful reset updates the password fingerprint, enables the customer when needed, and issues a new local access token.

`customerAccessTokenCreateWithMultipass` is intentionally limited to the captured invalid-request boundary. The proxy has no real Multipass secret, does not decrypt or validate real Multipass payloads, and returns the captured invalid Multipass customer-user-error shape for local replay.

`customerUpdate` authenticates through the local Storefront customer access-token store, updates the shared customer row, and never forwards the profile write to Shopify at runtime. It stages selected Storefront profile fields (`firstName`, `lastName`, `email`, `phone`, and `acceptsMarketing`) into the same customer state Admin reads use, keeps the Storefront email index aligned, and returns payload `customerUserErrors` / deprecated `userErrors` for modeled invalid-token, invalid-email, duplicate-email, blank-password, and HTML-name branches. Updating `password` stores a new non-secret password fingerprint, revokes all existing Storefront access tokens for that customer, and returns a newly issued local access token.

`customerAddressCreate`, `customerAddressUpdate`, `customerAddressDelete`, and `customerDefaultAddressUpdate` authenticate through the same local Storefront token store and mutate the shared customer address graph. Address input uses the existing `MailingAddressInput` normalization and validation path, so country/province normalization, free-text guardrails, duplicate detection, phone normalization, deterministic synthetic address IDs, address connection cursors, default-address assignment, and default reassignment after delete stay aligned with Admin customer address behavior. Storefront payloads use Storefront field names (`customerAddress`, `deletedCustomerAddressId`, and `customer`) and project `CustomerUserError` fields through Storefront selections.

Authenticated `customer(customerAccessToken:)` reads project the shared customer state through Storefront field names. Storefront-created customers, Admin `customerUpdate`, Admin customer address mutations, Storefront profile/address mutations, Admin `orderCreate` / order reassignment state, dump/restore, and reset all observe one customer graph. `defaultAddress` and `addresses` are rendered from the shared `addressesV2` nodes with Storefront `MailingAddress` fields and connection windowing. The `orders` connection is bounded to customer-visible Storefront order fields such as IDs, names, email/phone contact fields, status fields, money fields, processed timestamp, and line-item connection shape; Admin-only order details are not projected into Storefront responses.

Live-hybrid reads hydrate missing first-slice base state through explicit Storefront upstream calls, then answer the caller from the instance-owned store. The hydrated state includes Storefront shop fields, context-keyed localization, payment settings, locations with captured cursors, and public API versions. Snapshot reads do not invent shop, localization, payment, location, market, or API-version values. Empty connections and lists remain empty; absent objects resolve to null when the schema permits it, while absent non-null roots produce a GraphQL execution error and normal null propagation instead of invalid partial data.

The accepted `2025-01` route has no complete captured executable schema. Live-hybrid and passthrough requests for that version are forwarded unchanged; snapshot requests retain the legacy schema-shaped no-data fallback. The runtime does not silently substitute either an Admin schema or the 2026-04 Storefront schema.

Storefront catalog reads dispatch locally only when the proxy has shared product state and explicit publication visibility context in live-hybrid mode: either the current-channel publication is resolved or the store has a known publication catalog with staged product membership. Without that local catalog basis, live-hybrid product reads continue through the Storefront passthrough path. Snapshot mode answers from the local state only: unknown, draft, unpublished, deleted, or publication-unresolved products return `null` for `product` / `productByHandle` and are omitted from `products(...)`.

`@inContext(country:, language:, preferredLocationId:, buyer:)` is parsed from the original operation into a reusable Storefront request context. The engine-facing copy removes only that custom directive and variables used exclusively by it because the dynamic executor cannot register its runtime behavior; all other directives and variable uses remain under normal GraphQL validation. Country, language, preferred location, buyer company location, and the request-scoped customer token stay isolated between requests. Raw buyer tokens are not written into Storefront base state. An unknown buyer customer token returns Shopify's top-level `The token provided is not valid` error before any selected roots execute.

`shop` projects selected Storefront fields from captured Storefront shop state when available. It may reuse Admin-observed `shop`, `primaryDomain`, shop policy, money-format, and payment-setting fields when those shapes line up. First-slice Storefront hydration records the stable shop policy fields `privacyPolicy`, `refundPolicy`, `shippingPolicy`, and `termsOfService`; fields such as `contactInformation`, `legalNotice`, and `termsOfSale` are version- or availability-sensitive and are outside the first-slice hydration request. The proxy does not fabricate policy handles, domains, brand assets, or payment account values when neither Storefront nor Admin state has supplied them.

`localization` is context-keyed. Default context and country/language reads hydrate separate records so later Storefront calls can observe the same country, language, currency, and market selection without another upstream request. A locally allocated Admin location ID is not sent to Shopify during hydration because it is not a valid upstream Storefront identity; when the same country/language context is already observed, preferred-location reads reuse it and compute availability only from locally available Storefront-visible location state.

`locations` projects a Storefront connection from captured Storefront locations plus locally staged or Admin-observed active, non-fulfillment-service locations. Captured Storefront cursors are retained. Locally observed Admin locations use deterministic ID cursors when no Storefront cursor has been captured. `first`, `after`, `last`, `before`, `reverse`, and representative Storefront sort keys are handled through the shared connection helpers.

`paymentSettings` uses captured Storefront payment settings first. When only Admin shop state is available, it projects the overlapping currency, presentment currency, country, and digital wallet fields and leaves Storefront-only payment fields null or absent according to the caller selection.

`publicApiVersions` returns captured Storefront API version records only after Storefront hydration. Snapshot mode returns an empty list rather than deriving API versions from checked-in schema metadata.

`node(id:)` and `nodes(ids:)` parse the Shopify GID resource type and dispatch through the same Storefront-visible serializers as the dedicated roots. The supported local Node types are `Product`, `ProductVariant`, `Collection`, `Article`, `Blog`, `Page`, `Metaobject`, `Location`, and `Menu`. `nodes(ids:)` preserves input order, duplicates, and null slots. Valid but missing, deleted, unpublished, or inaccessible IDs return `null`; malformed global IDs return Shopify's captured coercion error. Aliases, named and inline fragments, the `Node` interface, and concrete type fragments remain under the Storefront executable schema.

`search(...)` indexes the effective locally known Storefront-visible `Product`, `Article`, and `Page` state. It excludes product/content tombstones, unpublished catalog resources, draft content, and records hidden by the active Storefront publication context. Terms use AND semantics; `prefix: LAST` applies word-prefix matching to the final term. An omitted `types` argument spans all supported result types, while Shopify's captured explicit multi-type list constrains search to its first entry. Product availability/tag/type/vendor/price/variant-option filters, `unavailableProducts`, `PRICE`/deterministic `RELEVANCE` ordering, `reverse`, cursor windows, `totalCount`, `pageInfo`, and availability/price filter payloads are computed from that effective candidate set.

`predictiveSearch(...)` supports `PRODUCT`, `COLLECTION`, `ARTICLE`, `PAGE`, and `QUERY` results. Resource lists reuse the supported Storefront serializers and visibility rules. `searchableFields`, `unavailableProducts`, the 1–10 limit, and `EACH`/`ALL` limit scopes are applied locally; an out-of-range limit returns Shopify's captured `INVALID_FIELD_ARGUMENTS` error. Query suggestions are derived deterministically from visible resource terms, with matching highlighted text and stable tracking parameters. Shopify's suggestion corpus and relevance ranking are opaque, so captured parity limits the difference to suggestion scalar values after resource matching, result types, limits, payload shapes, and computable order are aligned.

Snapshot discovery reads use local snapshot plus staged state and never hydrate upstream. Live-hybrid discovery stays passthrough when no supported discovery state is locally known; once shared supported-domain state exists, these roots resolve locally so staged Admin additions, updates, publication changes, unpublishes, and tombstones are immediately reflected without runtime Shopify writes.

`article` and `page` support ID lookup for locally staged visible content. `blog` and `pageByHandle` support handle lookup, and `blogByHandle` follows the Storefront alias root for blog handle lookup. Missing IDs or handles return `null`.

`articles`, `blogs`, and `pages` project Storefront connections from shared staged Admin online-store content. They support `first`, `after`, `last`, `before`, `reverse`, representative `sortKey` values, and Storefront-style search terms through the shared staged-connection helpers. Locally staged cursors are deterministic resource-ID cursors; captured Storefront cursors are preserved only for hydrated Storefront base state.

Article and page visibility follows the staged Admin content publication flags. Unpublished or deleted staged articles and pages do not appear through Storefront singular roots, nested blog article reads, content connections, or sitemap resources. Blogs remain visible as content containers while locally staged and not deleted.

Projected content fields include the selected handle, title, body/content HTML and text summaries, article tags, author/authorV2 names, default-null SEO fields, blog nesting, `Blog.articleByHandle`, `Blog.articles`, and distinct author lists for staged blog articles. Unsupported content subfields return Shopify-like null or empty values rather than fabricated data.

`menu(handle:)` reads from authenticated Storefront menu hydration in live-hybrid mode or restored Storefront base state in snapshot mode. It preserves captured nested items, item counts, item/resource IDs, resource links, tags, URLs, and resource union selections. Snapshot mode returns `null` for absent menus instead of inventing a main menu.

`sitemap(type:)` projects sitemap resources from locally visible staged content for `ARTICLE`, `BLOG`, and `PAGE`. It returns Shopify-like count objects, resource windows, and selected resource fields for modeled staged records only; snapshot mode does not fabricate default sitemap URLs or theme routes.

`urlRedirects` projects the staged URL redirect state already modeled by the Admin online-store surface. Empty/no-data queries return an empty connection with Storefront pageInfo shape. Storefront does not create, update, or delete redirects locally.

`product(id:|handle:)`, `productByHandle(handle:)`, and `products(...)` project the Storefront `Product` shape from the shared normalized product records. Supported fields include core identity, title, handle, description/HTML description, vendor, product type, tags, SEO, publication timestamp, availability, total inventory, images/media derived from product media, option values, variant connections, variant selected options, SKU, barcode, price, compare-at price, quantity availability, and basic connection `pageInfo`/cursor behavior. Staged image media retains the merchant-supplied source URL and derives Storefront image/media identities and source dimensions from shared media state; it does not invent a CDN URL.

`productTags` and `productTypes` return sorted, unique string connections with Storefront cursor and pageInfo behavior. Snapshot mode computes them from the locally visible product catalog. Live-hybrid mode hydrates the authenticated Storefront taxonomy connection with the exact caller-independent hydration document and preserves the observed upstream catalog page.

`productRecommendations(productId:|productHandle:)` returns `null` when the source product is missing or not Storefront-visible. For a visible source it ranks other visible products deterministically by shared tags, matching product type, matching vendor, normalized title, and product ID, excluding the source, unpublished, draft, and deleted products. Shopify's `RELATED` ranking is opaque and may return unrelated catalog products in a different order; the proxy deliberately exposes a stable useful local ranking rather than fabricating Shopify's result.

Product and variant `metafield(namespace:, key:)` and `metafields(identifiers:)` read the shared owner-metafield and definition stores. Only definitions with `access.storefront: PUBLIC_READ` are visible. Hidden or missing identifiers return `null`, and identifier-list reads preserve null placeholders in caller order.

Product `sellingPlanGroups(...)` and variant `sellingPlanAllocations(...)` are projected from staged selling-plan group membership and pricing policies. Group/plan options, recurring delivery flags, and percentage/fixed adjustments come from the shared selling-plan records. Allocation checkout, remaining-balance, per-delivery, and comparison money values are calculated from the effective contextual variant price; no plan or adjustment is synthesized when the product or variant is not a member.

Contextual money first resolves the hydrated Storefront market, then matches it to staged market state by observed ID or handle and follows the active catalog to its attached price list. A fixed variant price and compare-at price override the base product price in that context, and currency comes from the hydrated localization or attached price list. Without authoritative currency or pricing state, the enrichment path does not invent a market conversion. The captured preferred-location context leaves price and empty store availability unchanged. Quantity rules remain the captured default `minimum: 1`, `maximum: null`, `increment: 1` for this market-catalog surface, with empty quantity-price-break and store-availability connections.

`products(...)` filters the visible local catalog through the shared product search helper, then applies Storefront sort keys `TITLE`, `PRODUCT_TYPE`, `VENDOR`, `UPDATED_AT`, `CREATED_AT`, `BEST_SELLING`, `PRICE`, `ID`, and `RELEVANCE`, followed by `reverse` and cursor windowing through the shared connection helpers. Opaque Shopify relevance scoring and sales velocity are not reconstructed; those sort keys use deterministic local ordering.

`collection(id:|handle:)`, `collectionByHandle(handle:)`, and `collections(...)` project Storefront collections from the shared normalized Admin collection graph. Visible collection fields include identity, title, handle, description/HTML description, updated time, image, SEO, and Collection-owned metafields whose definitions grant Storefront `PUBLIC_READ` access. Unknown, unpublished, or deleted collections return `null` through singular roots and are omitted from collection connections.

Collection visibility follows the same current-channel publication state used by the shared product catalog. Admin collection create, update, delete, publish/unpublish, product membership changes, and manual reorder operations are reflected by later Storefront reads. Product update, delete, and publish/unpublish operations also flow through the shared product records, so collection membership never forks into a Storefront-only model.

`collections(...)` applies query filtering, Storefront `TITLE`, `UPDATED_AT`, `ID`, and `RELEVANCE` ordering, `reverse`, and cursor windowing through the shared staged-connection helpers. `Collection.products(...)` preserves captured collection-default/manual order before applying product visibility and connection filters. The modeled filters are `available`, `productType`, `productVendor`, and `tag`; modeled sort keys are `COLLECTION_DEFAULT`, `MANUAL`, `TITLE`, `PRICE`, `CREATED`, `ID`, `INVENTORY`, `BEST_SELLING`, and `RELEVANCE`.

Collection product nodes use the same Storefront Product projector as the top-level product roots. The supported nested boundary is the core identity/content, availability/inventory, vendor/type/tags, SEO, media/image, option/variant, and price/compare-at-price fields described for Storefront products above. Fields requiring contextual price lists, selling-plan allocations, bundles, store availability, or unmodeled product subgraphs remain outside that boundary and resolve through the normal null/empty behavior.

When live-hybrid mode has no collection graph, a missing collection read hydrates through an explicit authenticated Storefront request and observes returned collection and product connection aliases into the shared store. Snapshot mode never performs that hydration: absent collections return `null`, while `collections(...)` returns an empty connection with false/null `pageInfo` values.

`metaobject(handle:)`, `metaobject(id:)`, and `metaobjects(type:)` read from the shared normalized Admin metaobject and metaobject definition state. Storefront reads only expose entries whose definition has `access.storefront: PUBLIC_READ`; publishable definitions also require the entry status to be `ACTIVE`. Draft, private, deleted, missing, or unsupported-type entries return `null` through singular roots and are omitted from connections. Storefront field projection uses Storefront shape (`key`, `type`, `value`, `reference`, and `references`) and orders `fields` by key to match captured Storefront output.

Storefront metaobject connections support `first`, `after`, `last`, `before`, `reverse`, and representative string `sortKey` values through the shared staged-connection helpers. Locally staged cursors are deterministic staged-resource cursors; captured Storefront cursors remain opaque in fixtures.

Metaobject reference and list-reference fields resolve through the Storefront-visible node boundary. Visible referenced metaobjects project as Storefront `Metaobject` nodes; draft, private, deleted, missing, or unsupported references resolve to `null` or are omitted from reference connections. Cycles are bounded by the caller selection depth because the proxy only projects the selected nested fields.

`shop.metafield(namespace:, key:)` and `shop.metafields(identifiers:)` expose staged Shop-owned metafields only when the matching metafield definition has `access.storefront: PUBLIC_READ`. Definitions with `NONE`, missing identifiers, and missing records return `null` in the selected Storefront shape. Shop metafield reads can be answered from staged Shop owner-metafield state without hydrating broader Shop fields when the caller selects only `metafield`, `metafields`, and `__typename`.

### Boundaries

This is not local behavioral support for every field exposed by the captured `Shop`, `Localization`, `Location`, `PaymentSettings`, content, menu, sitemap, redirect, custom-data, `Product`, `ProductVariant`, or related nested types. The schema validates those fields, but fields outside the selected and hydrated boundary have no modeled Storefront state and therefore resolve through the documented null/empty boundary or schema null propagation. Storefront `Shop` policy fields that are not selected by first-slice hydration, including `contactInformation`, `legalNotice`, and `termsOfSale`, should be treated as availability-sensitive until a dedicated live Storefront capture promotes them.

Admin blog/page/article create, update, and delete effects are visible through the Storefront content roots when those Admin operations are locally supported. Admin menu CRUD is not locally modeled, so Storefront menu support is captured Storefront hydration/restored base-state projection only. URL redirect mutation lifecycle is not implemented for Storefront.

Storefront metafields for Customer, Article, Blog, Page, and other HasMetafields owners remain deferred until those owner models expose Storefront-visible owner reads. Product-, ProductVariant-, and Collection-owned metafields are supported for locally staged or hydrated owners when the matching definition grants Storefront `PUBLIC_READ` access. Metaobject-owned metafields are not a separate owner-metafield surface in this slice; metaobject custom fields are supported through `Metaobject.field` and `Metaobject.fields`.

Storefront field `jsonValue` is not selected by the supported Storefront custom-data projection because the live Storefront schema exposes `value`, `reference`, and `references` for these public types. Admin custom-data serializers still expose Admin `jsonValue` where those Admin roots support it.

Positive company-catalog quantity rules/price breaks, authenticated B2B buyer pricing, non-empty preferred-location availability, bundle component/grouping details, theme rendering, Online Store routing, canonical URL generation, storefront policy pages, product/collection content linked from menus, checkout/completion, delivery selection, discount and gift-card application, cart buyer/customer identity, customer email delivery, real account/recovery email URLs, real Multipass validation, and Storefront mutation domains outside the named customer-auth and cart roots remain outside this slice unless another endpoint document names them explicitly. Market-catalog quantity-rule writes are rejected on the Admin surface with Shopify's captured unsupported-context error rather than being exposed as Storefront quantity pricing.

Storefront customer support intentionally omits unsupported privacy-sensitive and Admin-only fields. Avatar/social-login fields, customer metafields, unsupported order subfields, and sensitive Admin-only customer/order data resolve as null, empty, or schema validation failures according to the Storefront schema and the selected local projection; the proxy does not fabricate private customer data to satisfy Storefront reads.

Live-hybrid operations that include unimplemented roots are forwarded as one unchanged Storefront request, while snapshot mode returns schema-shaped no-data behavior or rejects mutations.

Generic Storefront Node dispatch does not claim the remaining Storefront `Node` implementors. Search does not return collections because the captured `SearchResultItem` interface contains only products, articles, and pages; collection results are supported through `predictiveSearch`. Query suggestions are a deterministic approximation, not a reconstruction of Shopify's private search corpus or request-scoped ranking session.
