# Online Store Endpoint Group

The online-store group tracks Admin GraphQL roots for storefront content and presentation: articles, blogs, pages, comments, navigation menus, themes and theme files, script tags, storefront pixels, server pixels, storefront access tokens, and mobile platform applications.

## Current support and limitations

### Implemented roots

Content roots from HAR-303:

- Reads: `article`, `articleAuthors`, `articles`, `articleTags`, `blog`, `blogs`, `blogsCount`, `page`, `pages`, `pagesCount`, `comment`, `comments`
- Mutations: `articleCreate`, `articleUpdate`, `articleDelete`, `blogCreate`, `blogUpdate`, `blogDelete`, `pageCreate`, `pageUpdate`, `pageDelete`, `commentApprove`, `commentSpam`, `commentNotSpam`, `commentDelete`

Presentation and integration roots from HAR-372:

- Reads: `theme`, `themes`, `scriptTag`, `scriptTags`, `webPixel`, `serverPixel`, `mobilePlatformApplication`, `mobilePlatformApplications`
- Mutations: `themeCreate`, `themeUpdate`, `themeDelete`, `themePublish`, `themeFilesCopy`, `themeFilesUpsert`, `themeFilesDelete`, `scriptTagCreate`, `scriptTagUpdate`, `scriptTagDelete`, `webPixelCreate`, `webPixelUpdate`, `webPixelDelete`, `serverPixelCreate`, `serverPixelDelete`, `eventBridgeServerPixelUpdate`, `pubSubServerPixelUpdate`, `storefrontAccessTokenCreate`, `storefrontAccessTokenDelete`, `mobilePlatformApplicationCreate`, `mobilePlatformApplicationUpdate`, `mobilePlatformApplicationDelete`

The content model is normalized in memory as generic online-store content records for articles, blogs, pages, and comments. Snapshot mode serves these roots without upstream access. Live-hybrid mode hydrates captured upstream content reads, then serves the local graph when staged content exists.

Supported lifecycle mutations are staged locally and logged with the original raw GraphQL request for commit replay. They must not write to Shopify at normal runtime.

### Content read behavior

Snapshot/local empty behavior follows the 2025-01 capture in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/online-store/online-store-content-lifecycle.json`:

- missing `article(id:)`, `blog(id:)`, and `page(id:)` return `null`
- empty `articles`, `articleAuthors`, `blogs`, `pages`, and `comments` connections return empty `nodes`/`edges`, false page booleans, and null cursors when no local rows exist
- `articleTags(limit:)` returns an empty list when no local article tags exist
- `blogsCount` and `pagesCount` return `Count` payloads with `precision: "EXACT"`

Local connection support uses the shared GraphQL connection helpers for selected `nodes`, `edges`, and `pageInfo` fields. The local model supports common sort keys, reverse ordering, and cursor windows. Captured opaque Shopify cursors are not decoded; newly staged local rows use stable synthetic cursor values.

Search/filter support uses the shared Shopify Admin search-query helpers instead of a resource-local parser. The implemented local subset covers default text terms, boolean `OR` / negation grouping, and the fields that matter for the captured content lifecycle: `id`, `title`, `handle`, `created_at`, `updated_at`, `published_at`, `published_status`, `status`, article `author`, `blog_id`, `blog_title`, `tag`, and `tag_not`. Top-level `articles` still applies the captured published-article boundary before query filtering; nested blog article reads expose the effective local graph.

Nested content behavior:

- `Blog.articles` and `Blog.articlesCount` are derived from effective local articles with that blog as parent
- `Article.blog` resolves through the local blog graph
- `Article.comments` and `Article.commentsCount` are derived from effective local comments with that article as parent
- `Comment.article` resolves through the local article graph
- `Article.author`, `articleAuthors`, and `articleTags` are derived from local article data

The HAR-352 parity promotion captures a Shopify quirk where top-level `articles` omitted a locally updated unpublished article while the same article still appeared through `Blog.articles`, `Article.blog`, `articleAuthors`, and `articleTags`. The proxy now mirrors that boundary: top-level `articles` filters out records with `isPublished: false`, while nested blog/article helper reads continue to expose the effective local graph.

HAR-410 expands article subresource fidelity for the Admin GraphQL fields that are actually present in the captured schema:

- `Article.image` is staged from `ArticleImageInput.url` / `altText` on create and update, and downstream article reads return the latest local image object
- `Article.metafield(namespace:, key:)` and `Article.metafields(...)` are backed by ARTICLE-owned metafield inputs on `articleCreate` and `articleUpdate`
- Shopify replaces an existing article metafield by `(namespace, key)` while preserving its metafield ID; the local model mirrors that replacement behavior for staged article metafields
- Shopify creates a new `ArticleImage` ID when article image input is replaced; the local model also generates a new synthetic image ID for explicit image replacement

The local proxy does not fetch remote image bytes during staging, so synthetic article images preserve the selected ID, alt text, and input URL but return `null` for derived dimensions. Hydrated upstream images keep whatever dimensions Shopify returned. Translation and event subresources on content types remain shallow: local reads return empty translation lists or empty event connections rather than inventing unsupported subresource data. Non-article content metafields still return empty connections or `null` singular metafields until separately captured.

### Content mutation behavior

Implemented local staging:

- `blogCreate` / `blogUpdate` / `blogDelete` stage blog title, handle, template suffix, and comment policy changes
- `pageCreate` / `pageUpdate` / `pageDelete` stage page title, handle, body/body summary, template suffix, and publication fields
- `articleCreate` / `articleUpdate` / `articleDelete` stage article title, handle, body, summary, tags, author, publication fields, image fields, ARTICLE-owned metafields, and blog membership; `articleCreate` can also stage an inline blog from the optional `blog` argument
- `commentApprove`, `commentSpam`, `commentNotSpam`, and `commentDelete` stage moderation or tombstones for comments that already exist in hydrated/snapshot local state; approval sets `status: "PUBLISHED"`, `isPublished: true`, and a local `publishedAt` timestamp when the comment did not already have one

Unknown content IDs return local `userErrors` for supported mutations instead of proxying upstream. Delete mutations stage tombstones so downstream detail reads return `null` and catalog/nested connections omit the deleted row.

Comment creation is not part of the Admin GraphQL root set captured for this ticket, so comment moderation support is intentionally limited to comments supplied by snapshot or hydrated upstream reads.

### Presentation and integration behavior

HAR-372 adds a normalized local integration graph for themes, script tags, web pixels, server pixels, storefront access tokens, and mobile platform applications. These roots are local-only at runtime:

- theme mutations stage theme metadata, publish role changes, deletion tombstones, and theme-file copy/upsert/delete effects in memory
- `themePublish` flips the staged target theme to `MAIN` and demotes any effective local main theme to `UNPUBLISHED`, without changing storefront presentation
- theme-file bodies are stored in local theme records and exposed through `OnlineStoreTheme.files`; no asset upload or CDN write is performed
- script tag mutations stage `src`, `displayScope`, and `cache` values, but never load or execute storefront JavaScript
- web pixel and server pixel mutations stage inert configuration only; no browser tracking, EventBridge send, Pub/Sub send, webhook registration, or customer-event subscription is activated
- storefront access token create/delete stages the credential lifecycle locally; generated token values are redacted from GraphQL responses, `Shop.storefrontAccessTokens` downstream reads, and meta state as `shpat_redacted`
- mobile platform application mutations stage Android/Apple verification settings locally and expose union-shaped downstream reads

Snapshot/local empty reads return Shopify-like `null` for missing singular roots and empty connections for catalog roots. Local connection support for `themes`, `scriptTags`, and `mobilePlatformApplications` uses shared cursor/window helpers. `themes` supports local `roles` and `names` filters; `scriptTags` supports local `src` and simple text query filtering.

The local model preserves original raw mutations in the meta log for eventual commit replay. Sensitive generated storefront token values are not exposed through `GET /__meta/state`; the original request body is still retained so commit replay can send the merchant-authored mutation in original order.

### Captured quirks

The 2025-01 live capture showed `comment(id:)` with an unknown synthetic ID returning a Shopify internal error, while unknown-id `commentApprove`, `commentSpam`, `commentNotSpam`, and `commentDelete` returned normal `userErrors` with `field: ["id"]` and message `Comment does not exist`. The proxy does not emulate the internal-error branch for local snapshot reads; local missing comments return `null` to preserve stable no-data behavior.

HAR-410 live schema evidence showed `Page.onlineStoreUrl` is not an Admin GraphQL `Page` field in the probed versions, including 2025-01, 2026-04, and `unstable`; selecting it returned a GraphQL schema error rather than a nullable field. The same capture showed `Article.seo` is absent from the Article schema, even though `ArticleCreateInput` and `ArticleUpdateInput` expose `image` and `metafields`. Do not synthesize page online-store URLs or article SEO fields locally unless a later Shopify schema capture proves a supported Admin GraphQL surface.

### Remaining navigation gaps tracked by the registry

- `menu`
- `menus`
- `menuCreate`
- `menuUpdate`
- `menuDelete`

Navigation/menu support remains blocked on menu item tree shape, nested resource references, navigation-specific validation, and downstream read-after-write evidence. Do not promote those roots until a local navigation model exists.

HAR-410 removed the previous credential blocker and captured a disposable page-to-menu flow. `menuCreate` with a PAGE item returned `Menu.items[0]` with `type: "PAGE"`, `resourceId` equal to the created page ID, and `url: "/pages/<page-handle>"`; `menu(id:)` returned the created menu, `menuUpdate` replaced items and generated new menu-item IDs, `menuDelete` returned `deletedMenuId`, and a downstream `menu(id:)` read returned `null`. The same capture showed `menus(first: 5, query: "handle:<created-handle>")` returning the shop default menus rather than the newly created custom menu, so menu catalog behavior needs dedicated navigation coverage before these roots can be marked implemented.

## Historical and developer notes

### Evidence and blockers

- Current content evidence: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/online-store/online-store-content-lifecycle.json` captures content catalog/detail/empty reads, blog/page/article lifecycle success paths with downstream reads, and unknown-id comment moderation/delete userErrors. HAR-352 promotes this fixture through `config/parity-specs/online-store/online-store-content-lifecycle.json` and `config/parity-requests/online-store/online-store-content-*.graphql`; `corepack pnpm conformance:parity` seeds the captured baseline read, replays local create/update/read/delete/comment guardrail requests, and strictly compares stable payload/count/null-empty/userErrors fields against the recording.
- HAR-393 executable review added local integration coverage for Shopify-style boolean/fielded content search and comment approval `publishedAt` materialization. It also adds `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/online-store/online-store-content-search-filters.json` plus `config/parity-specs/online-store/online-store-content-search-filters.json`, which prove article tag/author, blog title, and page published-status/title filters against live Shopify and replay them through `corepack pnpm conformance:parity`. External examples reviewed during the ticket show apps commonly requesting page `onlineStoreUrl`, article image/SEO/metafield fields, and navigation-menu insertion after page creation; HAR-410 now covers the supported article media/metafield subset and leaves schema-absent page/SEO fields plus navigation roots as explicit boundaries.
- HAR-410 evidence: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/online-store-article-media-navigation-follow-through.json` records live article image/metafield create/update/read behavior, confirms the Page `onlineStoreUrl` and Article `seo` schema boundaries, and captures page-to-menu follow-through behavior for future navigation modeling. Runtime coverage for the newly supported article image/metafield behavior lives in `tests/integration/online-store-content-flow.test.ts`; menu roots remain unimplemented.
- HAR-372 live evidence: `corepack pnpm conformance:probe` and `SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm conformance:probe` succeeded against `harry-test-heelo.myshopify.com` during implementation. Live 2026-04 safe-read probes confirmed `themes` and `shop.storefrontAccessTokens` response shape, and confirmed current credential blockers for `scriptTags`, `webPixel`, `serverPixel`, and `mobilePlatformApplications` (`ACCESS_DENIED` / missing read scopes). Runtime support for blocked-scope families is based on schema introspection plus local executable tests, with the scope blocker recorded here rather than a checked-in passive parity spec.
- HAR-452 review: Shopify docs/examples and public Admin GraphQL examples for themes/theme files, script tags, web/server pixels, mobile platform applications, and storefront access tokens were reviewed against the existing HAR-372 model. The risky paths are still the externally visible effects: real theme asset writes and publish state, storefront script loading, browser/customer-event pixel activation, EventBridge/Pub/Sub delivery, Storefront API credential issuance, and mobile app verification settings. The proxy deliberately stages inert local records for these effects and does not emulate the external delivery/activation systems. HAR-452 tightened executable coverage for the token path by proving staged tokens appear through `Shop.storefrontAccessTokens` with redacted token values and disappear after `storefrontAccessTokenDelete`.
- Current presentation/integration schema evidence: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json` proves the HAR-372 root names exist in the captured Admin GraphQL schema. Live 2026-04 introspection confirmed payload, input, union, and theme-file result fields used by the local model.
- Safety boundary: publish, theme-file, pixel, script tag, token, and mobile-platform mutations are externally visible in Shopify. Supported proxy handling stages them locally only; real Shopify effects happen only during explicit commit replay or deliberate conformance setup/cleanup.
- Parity-spec boundary: do not add planned-only parity specs for these gaps. Add `config/parity-specs` entries only when backed by captured interactions and executable comparison or runtime-test-backed fixture evidence.

### Validation anchors

- Registry schema/discovery: `corepack pnpm conformance:check`
- Root coverage snapshot: `corepack pnpm vitest run tests/unit/graphql-operation-coverage.test.ts`
- Online-store routing guard: `corepack pnpm vitest run tests/unit/online-store-registry.test.ts`
- Online-store content flow: `corepack pnpm exec vitest run tests/integration/online-store-content-flow.test.ts`
- Online-store presentation/integration flow: `corepack pnpm exec vitest run tests/integration/online-store-integrations-flow.test.ts`
