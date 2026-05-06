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

The content model is normalized in memory as generic online-store content records for articles, blogs, pages, and comments. Snapshot mode serves these roots without upstream access. Live-hybrid mode forwards cold catalog/search reads upstream when no local content state exists, then serves the local graph when staged content exists. `blogsCount` and `pagesCount` use a narrow upstream baseline count during staged lifecycle reads because the full downstream read document contains local synthetic IDs and cannot be forwarded verbatim.

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

- `blogCreate` / `blogUpdate` / `blogDelete` stage blog title, handle, template suffix, and comment policy changes. `blogUpdate` also accepts Core's `commentable` input alias and maps `MODERATE` to downstream `commentPolicy: MODERATED`; invalid `commentable` values return `INCLUSION` under `["blog", "commentable"]`.
- `pageCreate` / `pageUpdate` / `pageDelete` stage page title, handle, body/body summary, template suffix, and publication fields; new pages default to published with a local `publishedAt` timestamp when `isPublished` and `publishDate` are both omitted
- `articleCreate` / `articleUpdate` / `articleDelete` stage article title, handle, body, summary, tags, author, publication fields, image fields, ARTICLE-owned metafields, and blog membership; `articleCreate` can also stage an inline blog from the optional `blog` argument
- `commentApprove`, `commentSpam`, `commentNotSpam`, and `commentDelete` stage moderation for comments that already exist in hydrated/snapshot local state, or hydrate the existing comment through a narrow upstream read before staging when needed. Moderation persists only Core `CommentStatus` values: approval sets `status: "PUBLISHED"`, spam sets `status: "SPAM"`, not-spam sets `status: "UNAPPROVED"`, and delete sets `status: "REMOVED"` while returning `deletedCommentId`.
- Removed comments keep their ID for idempotent `commentDelete`, while `commentApprove` and `commentSpam` return an `INVALID` `userErrors` entry without changing the removed status. Approval also sets `isPublished: true` and a local `publishedAt` timestamp when the comment did not already have one.

Page and article create/update reject a supplied future `publishDate` whenever the effective `isPublished` value is `true`, returning `code: "INVALID_PUBLISH_DATE"`, `field: ["page" | "article"]`, and `message: "Can’t set isPublished to true and also set a future publish date."` without staging local content. `isPublished: false` with a future `publishDate` remains valid and stages unpublished scheduled content with `publishedAt` set to the future timestamp.

`pageCreate`, `blogCreate`, and `articleCreate` reject missing or blank `title` inputs before staging any local content. The local resolver returns a title `userErrors` payload with `field: ["page" | "blog" | "article", "title"]` and `code: "BLANK"` for both omitted and blank title values so tests get a stable mutation payload instead of staging empty-title records.

Unknown content IDs return local `userErrors` for supported mutations instead of proxying upstream. Blog, page, and article delete mutations stage tombstones so downstream detail reads return `null` and catalog/nested connections omit the deleted row. Comment delete is modeled as a moderation transition to `REMOVED` so subsequent moderation roots can preserve Shopify's removed-comment guardrails.

Comment creation is not part of the Admin GraphQL root set captured for this ticket, so comment moderation support is intentionally limited to comments supplied by snapshot or hydrated upstream reads.

### Presentation and integration behavior

HAR-372 adds a normalized local integration graph for themes, script tags, web pixels, server pixels, storefront access tokens, and mobile platform applications. These roots are local-only at runtime:

- theme mutations stage theme metadata, publish role changes, deletion tombstones, and theme-file copy/upsert/delete effects in memory
- `themePublish` flips the staged target theme to `MAIN` and demotes any effective local main theme to `UNPUBLISHED`, without changing storefront presentation
- `themePublish` returns a local `userErrors` response without staging when the target theme is in a non-publishable `DEMO`, `LOCKED`, or `ARCHIVED` role
- theme-file bodies are stored in local theme records and exposed through `OnlineStoreTheme.files`; no asset upload or CDN write is performed. Local `themeFilesUpsert` accepts only one-level filenames under `templates/`, `sections/`, `snippets/`, `layout/`, `config/`, `locales/`, or `assets/`, computes `checksumMd5` from the persisted body content, and computes `size` from UTF-8 body bytes. `themeFilesCopy` reads the existing local source body before deriving the copied file metadata, and `themeFilesDelete` rejects required config files.
- script tag mutations stage `src`, normalized lowercase `displayScope`, forced `event: "onload"`, and `cache` values, but never load or execute storefront JavaScript. `scriptTagCreate` rejects missing/blank, over-255-character, malformed, and non-HTTPS `src` values before staging; `scriptTagUpdate` applies the same checks only to changed fields and returns update-shaped field paths such as `["src"]` and `["displayScope"]`. GraphQL responses expose display scope as the Admin enum form (`ONLINE_STORE`, `ALL`, or `ORDER_STATUS`) and event as `onload`.
- web pixel and server pixel mutations stage inert configuration only; no browser tracking, EventBridge send, Pub/Sub send, webhook registration, or customer-event subscription is activated
- `webPixelCreate` enforces Shopify Core's one-WebPixel-per-calling-app/api-permission guard in the local staged graph: a duplicate effective WebPixel returns `webPixel: null` plus one `TAKEN` `WebPixelUserError` and does not mint or stage a new WebPixel
- `webPixelUpdate` parses supplied `settings` JSON before staging. Malformed JSON returns `INVALID_CONFIGURATION_JSON` on `["settings"]`, and valid JSON is stored as parsed JSON rather than as the raw string literal.
- `webPixelUpdate` validates supplied `runtimeContext` values against any runtime-context declaration carried on the staged WebPixel record (`runtimeContexts` / `runtime_contexts`) and validates known settings keys against any staged extension `settingsDefinition` / `settings_definition` metadata. Type, range, and regex violations return `INVALID_SETTINGS` on `["settings"]`; runtime-context mismatches return `INVALID_RUNTIME_CONTEXT` on `["webPixel", "runtimeContext"]`.
- WebPixel records persist only WebPixel fields; `webhookEndpointAddress` is kept on ServerPixel records only. Local WebPixel status is derived from settings presence (`CONNECTED` when settings exist, `NEEDS_CONFIGURATION` when they do not) rather than from a hardcoded literal.
- storefront access token create/delete stages the credential lifecycle locally; create returns a deterministic unique `shpat_<16-hex>` token, non-empty storefront access scopes, and selected `shop { id }`, while `Shop.storefrontAccessTokens` downstream reads and meta state keep generated token values redacted as `shpat_redacted`
- mobile platform application mutations stage Android/Apple verification settings locally and expose union-shaped downstream reads. `mobilePlatformApplicationCreate` rejects inputs that specify neither or both platform branches, rejects blank platform identifiers, and enforces one Android and one Apple record per shop before staging. Updates apply only the matching platform sub-input to the staged record, reject wrong-platform sub-inputs with `INVALID` on `["mobilePlatformApplication"]`, and reject blank Android `applicationId` / Apple `appId` values with `BLANK` field-level user errors before mutating local state.

Snapshot/local empty reads return Shopify-like `null` for missing singular roots and empty connections for catalog roots. Local connection support for `themes`, `scriptTags`, and `mobilePlatformApplications` uses shared cursor/window helpers. `themes` supports local `roles` and `names` filters; `scriptTags` supports local `src` and simple text query filtering.

The local model preserves original raw mutations in the meta log for eventual commit replay. Sensitive generated storefront token values are not exposed through `GET /__meta/state`; the original request body is still retained so commit replay can send the merchant-authored mutation in original order.

### Captured quirks

The 2025-01 live capture showed `comment(id:)` with an unknown synthetic ID returning a Shopify internal error, while unknown-id `commentApprove`, `commentSpam`, `commentNotSpam`, and `commentDelete` returned normal `userErrors` with `field: ["id"]` and message `Comment does not exist`. The proxy does not emulate the internal-error branch for local snapshot reads; local missing comments return `null` to preserve stable no-data behavior.

HAR-558 live evidence in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/online-store/online-store-content-required-fields.json` shows omitted required `title` fields are rejected by Shopify GraphQL validation before the mutation resolver runs. Blank-string titles reach model validation and return title `userErrors`; Shopify returns `code: "BLANK"` for page/article and `code: null` for blog in the captured API version. The proxy deliberately normalizes missing and blank titles to a local `BLANK` userError for all three roots to satisfy stable draft-proxy validation and avoid staging empty-title records.

HAR-741 live evidence in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/online-store/online-store-body-script-verbatim.json` and `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/online-store/online-store-body-script-verbatim.json` shows Admin GraphQL preserves `pageCreate` and `articleCreate` body HTML verbatim, including `<script>` blocks and event-handler attributes, in both mutation payloads and immediate `page(id:)` / `article(id:)` reads. `Page.bodySummary` strips tags but keeps script text; `Article.summary` remains `null` when omitted. The proxy mirrors that behavior and does not scrub supported page/article body inputs locally.

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
- HAR-410 evidence: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/online-store-article-media-navigation-follow-through.json` records live article image/metafield create/update/read behavior, confirms the Page `onlineStoreUrl` and Article `seo` schema boundaries, and captures page-to-menu follow-through behavior for future navigation modeling. Focused runtime coverage covers the newly supported article image/metafield behavior; menu roots remain unimplemented.
- HAR-451 review: Shopify's current PageCreateInput documentation keeps the historical default that `isPublished` is `true` when no publish date is specified; the local page staging model now mirrors that default. Rework evidence in `config/parity-specs/online-store/online-store-page-default-publish-local-staging.json` executes omitted `isPublished`/`publishDate` page creation plus downstream `page` and published `pages` reads through the generic parity runner. The review did not add a new comment-success live recording because Admin GraphQL exposes moderation roots but no captured content comment-create root for disposable setup; existing parity evidence still covers unknown-comment userErrors, while focused runtime coverage covers successful approve, spam, not-spam, delete, downstream nested article reads, and no upstream fetches for locally staged moderation.
- HAR-537 cassette migration: `online-store-content-search-filters` uses LiveHybrid Pattern 1 passthrough for cold `articles` / `blogs` / `pages` search reads, backed by a hand-synthesized cassette from the captured search response. `online-store-content-lifecycle` keeps staged create/update/read/delete flows local, but fetches hand-synthesized upstream `blogsCount` / `pagesCount` baseline cassettes and adds newly staged local content so downstream count reads match Shopify without forwarding supported mutations.
- HAR-558 evidence: `online-store-content-required-fields` captures missing-title schema errors and blank-title model `userErrors` for `pageCreate`, `articleCreate`, and `blogCreate`; the executable parity targets replay the blank-title userError branches, while focused runtime tests cover the proxy's omitted-title normalization and no-staging behavior.
- HAR-372 live evidence: `corepack pnpm conformance:probe` and `SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm conformance:probe` succeeded against `harry-test-heelo.myshopify.com` during implementation. Live 2026-04 safe-read probes confirmed `themes` and `shop.storefrontAccessTokens` response shape, and confirmed current credential blockers for `scriptTags`, `webPixel`, `serverPixel`, and `mobilePlatformApplications` (`ACCESS_DENIED` / missing read scopes). Runtime support for blocked-scope families is based on schema introspection plus local executable tests, with the scope blocker recorded here rather than a checked-in passive parity spec.
- HAR-452 review: Shopify docs/examples and public Admin GraphQL examples for themes/theme files, script tags, web/server pixels, mobile platform applications, and storefront access tokens were reviewed against the existing HAR-372 model. The risky paths are still the externally visible effects: real theme asset writes and publish state, storefront script loading, browser/customer-event pixel activation, EventBridge/Pub/Sub delivery, Storefront API credential issuance, and mobile app verification settings. The proxy deliberately stages inert local records for these effects and does not emulate the external delivery/activation systems. HAR-452 tightened executable coverage for the token path by proving staged tokens appear through `Shop.storefrontAccessTokens` with redacted token values and disappear after `storefrontAccessTokenDelete`.
- HAR-452 rework evidence: `config/parity-specs/online-store/storefront-access-token-local-staging.json` replays storefront access token create, downstream `shop.storefrontAccessTokens`, delete, and downstream empty read through the generic parity runner. The fixture records 2026-04 live safe-read evidence for Shopify's empty token connection shape plus the current live `ACCESS_DENIED` blocker for `storefrontAccessTokenCreate`, so successful token-write parity remains local-runtime-backed until a write-capable conformance credential is available.
- HAR-584 evidence: `config/parity-specs/online-store/storefront-access-token-create-shape.json` replays the local create shape with selected `shop { id }`, a `storefront-access-token` matcher for the create-only `shpat_` token, and strict non-empty storefront `accessScopes`; focused runtime tests cover repeated-token uniqueness, current-app storefront-scope filtering, blank-title `BLANK` userErrors, and the 100-token synthetic limit.
- HAR-553 evidence: `config/parity-specs/online-store/theme-publish-demotes-previous-main.json` replays local creation of a current `MAIN` theme, creation of a second `UNPUBLISHED` theme, `themePublish` of the second theme, and downstream `theme(id:)` / `themes(roles: [MAIN])` reads through the generic Gleam parity runner. The scenario proves the staged graph preserves Shopify's one-main-theme invariant without runtime Shopify writes.
- HAR-585 evidence: `config/parity-specs/online-store/theme-files-checksums-and-validation.json` replays local theme creation followed by two `themeFilesUpsert` calls for the same filename with different bodies and an invalid `evil/path.liquid` upsert. Focused runtime tests also cover copy `NOT_FOUND`, copied body checksum/size inheritance, required config delete rejection, and downstream `OnlineStoreTheme.files` reads.
- HAR-572 evidence: `config/parity-specs/online-store/web-pixel-create-duplicate-returns-taken.json` replays `webPixelCreate` twice in one local proxy session and strictly compares the second response to the Shopify Core duplicate guard shape: `webPixel: null`, nullable `field`, `code: "TAKEN"`, and `__typename: "WebPixelUserError"`. Runtime coverage in `test/shopify_draft_proxy/proxy/online_store_test.gleam` also locks WebPixel status derivation and the WebPixel/ServerPixel `webhookEndpointAddress` persisted-field boundary.
- WebPixel update validation evidence: `config/parity-specs/online-store/web-pixel-update-validation-local-runtime.json` replays local create/update branches through the generic parity runner and strictly compares malformed settings JSON plus parsed valid settings/status payloads. Focused runtime tests cover staged extension declaration behavior for runtime context, setting type, range, and regex validation because the generic parity runner does not pre-seed declaration metadata.
- Current presentation/integration schema evidence: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json` proves the HAR-372 root names exist in the captured Admin GraphQL schema. Live 2026-04 introspection confirmed payload, input, union, and theme-file result fields used by the local model.
- Safety boundary: publish, theme-file, pixel, script tag, token, and mobile-platform mutations are externally visible in Shopify. Supported proxy handling stages them locally only; real Shopify effects happen only during explicit commit replay or deliberate conformance setup/cleanup.
- Parity-spec boundary: do not add planned-only parity specs for these gaps. Add `config/parity-specs` entries only when backed by captured interactions and executable comparison or runtime-test-backed fixture evidence.

### Validation anchors

- Registry schema/discovery: `corepack pnpm conformance:check`
