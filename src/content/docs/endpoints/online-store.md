---
title: 'Online Store Endpoint Group'
description: 'Coverage notes and fidelity boundaries for Online Store Endpoint Group.'
---

The online-store group tracks Admin GraphQL roots for storefront content and presentation: articles, blogs, pages, comments, navigation menus, themes and theme files, script tags, storefront pixels, server pixels, storefront access tokens, and mobile platform applications.

## Current support and limitations

### Supported roots

Content roots:

- Reads: `article`, `articleAuthors`, `articles`, `articleTags`, `blog`, `blogs`, `blogsCount`, `page`, `pages`, `pagesCount`, `comment`, `comments`
- Mutations: `articleCreate`, `articleUpdate`, `articleDelete`, `blogCreate`, `blogUpdate`, `blogDelete`, `pageCreate`, `pageUpdate`, `pageDelete`, `commentApprove`, `commentSpam`, `commentNotSpam`, `commentDelete`

URL redirect read roots from the metaobject redirect-new-handle slice:

- Reads: `urlRedirect`, `urlRedirects`

Presentation and integration roots:

- Reads: `theme`, `themes`, `scriptTag`, `scriptTags`, `webPixel`, `serverPixel`, `mobilePlatformApplication`, `mobilePlatformApplications`
- Mutations: `themeCreate`, `themeUpdate`, `themeDelete`, `themePublish`, `themeFilesCopy`, `themeFilesUpsert`, `themeFilesDelete`, `scriptTagCreate`, `scriptTagUpdate`, `scriptTagDelete`, `webPixelCreate`, `webPixelUpdate`, `serverPixelCreate`, `eventBridgeServerPixelUpdate`, `pubSubServerPixelUpdate`, `storefrontAccessTokenCreate`, `mobilePlatformApplicationCreate`, `mobilePlatformApplicationUpdate`

Tracked but unimplemented integration delete roots: `webPixelDelete`, `serverPixelDelete`, `storefrontAccessTokenDelete`, and `mobilePlatformApplicationDelete`. They remain registry-only/unsupported and must not be cited as local lifecycle support until downstream read-after-delete behavior is modeled.

The content model is normalized in memory as generic online-store content records for articles, blogs, pages, and comments. Snapshot mode serves these roots without upstream access. In live-hybrid mode, cold content reads can pass through to Shopify and hydrate observed records into the local graph; once local content state exists, supported content reads are answered from that effective graph rather than forwarding read-after-write documents upstream. Count reads hydrate captured `blogsCount` / `pagesCount` baselines through narrow upstream count reads, then report that baseline plus local synthetic creates and tombstones.

Supported lifecycle mutations are staged locally and logged with the original raw GraphQL request for commit replay. They must not write to Shopify at normal runtime.

Effective `Article`, `Blog`, `Comment`, and `Page` records are exposed through generic `node(id:)` / `nodes(ids:)` dispatch. Those reads use the same local content serializers as the dedicated content roots, so staged page/article/blog create and update flows are visible through the Admin `Node` interface without runtime Shopify writes.

URL redirect reads are local overlays for redirect rows staged by supported domain behavior, currently `metaobjectUpdate(..., metaobject: { handle, redirectNewHandle: true })` on online-store-renderable metaobjects. Snapshot mode returns local state only. Live-hybrid mode forwards cold `urlRedirect`/`urlRedirects` reads upstream when no local redirect state or requested local ID is present; once local redirect state exists, `urlRedirects` filters with the shared Admin search-query parser and supports `id:`, `path:`, `target:`, and default text terms. URL redirect create/update/delete/import/bulk-delete mutations remain unsupported and must not be treated as locally modeled lifecycle roots.

### Content read behavior

Snapshot/local empty behavior follows the 2025-01 capture in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/online-store/online-store-content-lifecycle.json`:

- missing `article(id:)`, `blog(id:)`, and `page(id:)` return `null`
- empty `articles`, `articleAuthors`, `blogs`, `pages`, and `comments` connections return empty `nodes`/`edges`, false page booleans, and null cursors when no local rows exist
- `articleTags(limit:)` returns an empty list when no local article tags exist
- `blogsCount` and `pagesCount` return `Count` payloads with `precision: "EXACT"`

Local connection support uses the shared GraphQL connection helpers for selected `nodes`, `edges`, and `pageInfo` fields. The local model supports common sort keys, reverse ordering, and cursor windows. Captured opaque Shopify cursors are not decoded; newly staged local rows use stable synthetic cursor values.

Search/filter support covers the local subset that matters for the captured content lifecycle: default text terms plus fields such as `id`, `title`, `handle`, `created_at`, `updated_at`, `published_at`, `published_status`, `status`, article `author`, `blog_id`, `blog_title`, `tag`, and `tag_not`. Unfiltered top-level `articles` returns published and unpublished effective records; explicit `published_status:published`, `published_status:unpublished`, and `published_status:any` filters control status slices when supplied. Unknown fielded filters are treated as explicit local no-matches instead of silently returning the full local content graph. Nested blog article reads expose the effective local graph.

Nested content behavior:

- `Blog.articles` and `Blog.articlesCount` are derived from effective local articles with that blog as parent
- `Article.blog` resolves through the local blog graph
- `Article.comments` and `Article.commentsCount` are derived from effective local comments with that article as parent
- `Comment.article` resolves through the local article graph
- `Article.author`, `articleAuthors`, and `articleTags` are derived from local article data

Top-level `articles` no longer applies a published-only default when `query` is omitted, so staged unpublished content remains visible in unfiltered local catalog reads as well as through parent blog and detail relationships. Explicit `published_status` query filters still narrow the status slice.

Article subresource fidelity covers the Admin GraphQL fields that are actually present in the captured schema:

- `Article.image` is staged from `ArticleImageInput.url` / `altText` on create and update, and downstream article reads return the latest local image object
- `Article.metafield(namespace:, key:)` and `Article.metafields(...)` are backed by ARTICLE-owned metafield inputs on `articleCreate` and `articleUpdate`
- Shopify replaces an existing article metafield by `(namespace, key)` while preserving its metafield ID; the local model mirrors that replacement behavior for staged article metafields
- Shopify creates a new `ArticleImage` ID when article image input is replaced; the local model also generates a new synthetic image ID for explicit image replacement

The local proxy does not fetch remote image bytes during staging, so synthetic article images preserve the selected ID, alt text, and input URL but return `null` for derived dimensions. Hydrated upstream images keep whatever dimensions Shopify returned. Translation and event subresources on content types remain shallow: local reads return empty translation lists or empty event connections rather than inventing unsupported subresource data. Non-article content metafields still return empty connections or `null` singular metafields until separately captured.

### Content mutation behavior

Implemented local staging:

- `blogCreate` / `blogUpdate` / `blogDelete` stage blog title, handle, template suffix, and comment policy changes. `blogUpdate` also accepts Core's `commentable` input alias and maps `MODERATE` to downstream `commentPolicy: MODERATED`; invalid `commentable` values return `INCLUSION` under `["blog", "commentable"]`.
- `pageCreate` / `pageUpdate` / `pageDelete` stage page title, handle, body/body summary, template suffix, and publication fields; new pages default to published with a local `publishedAt` timestamp when `isPublished` and `publishDate` are both omitted
- `articleCreate` / `articleUpdate` / `articleDelete` stage article title, handle, body, summary, tags, author, publication fields, image fields, ARTICLE-owned metafields, and blog membership; `articleCreate` can also stage an inline blog from the optional `blog` argument. `articleDelete` silently cascades to staged/effective comments for that article so downstream comment detail and connection reads no longer expose orphaned comments.
- `commentApprove`, `commentSpam`, `commentNotSpam`, and `commentDelete` stage moderation for comments that already exist in hydrated upstream, staged, or snapshot local state. Moderation persists only Core `CommentStatus` values for non-delete roots: approval moves `UNAPPROVED` to `PUBLISHED`, spam moves `UNAPPROVED`/`PUBLISHED`/`PENDING` to `SPAM`, and not-spam moves `SPAM` to `PUBLISHED`.
- Comment moderation follows Shopify's state-machine guardrails for captured source states. `commentApprove` on `PUBLISHED`, `commentNotSpam` on `PUBLISHED`, and `commentSpam` on `SPAM` return the existing comment unchanged, preserving fields such as `publishedAt` and avoiding staged-resource churn. `commentApprove` on `SPAM` and `commentNotSpam` on `UNAPPROVED` return `userErrors` under `field: ["id"]` with the captured invalid-transition message. Removed comments supplied by legacy snapshots or cascades return an `INVALID` `userErrors` entry from moderation roots without changing the removed status. Approval also sets `isPublished: true` and a local `publishedAt` timestamp when the comment did not already have one.
- `commentDelete` records a local hard deletion marker and returns `deletedCommentId`, matching Shopify's destroy path for the public mutation. Deleted comments disappear from `comment(id:)`, root `comments`, `Article.comments`, and `Article.commentsCount`; repeated or unknown local `commentDelete` returns the captured `NOT_FOUND` userError shape.

Page, blog, and article template suffix inputs are stored from create/update payloads and returned by downstream detail reads. Captured Admin API behavior returns explicit `null` as `null`, while an empty-string `templateSuffix` remains an empty string in both mutation payloads and read-after-write detail results.

Page and article create/update reject a supplied future `publishDate` whenever the effective `isPublished` value is `true`, returning `code: "INVALID_PUBLISH_DATE"`, `field: ["page" | "article"]`, and `message: "Can’t set isPublished to true and also set a future publish date."` without staging local content. `isPublished: false` with a future `publishDate` remains valid and stages unpublished scheduled content with `publishedAt` set to the future timestamp.

`pageCreate`, `blogCreate`, and `articleCreate` reject missing or blank `title` inputs before staging any local content. The local resolver returns a title `userErrors` payload with `field: ["page" | "blog" | "article", "title"]` and `code: "BLANK"` for both omitted and blank title values so tests get a stable mutation payload instead of staging empty-title records.

Content create/update mutations also enforce Shopify's captured length guardrails before staging. Blog titles and handles are capped at 255 characters; page titles and handles are capped at 255 characters; article titles are capped at 255 characters; article handles are capped at 265 characters. Oversized title/handle inputs return `TOO_LONG` under the resource field path. Page bodies over 524287 UTF-8 bytes return `TOO_BIG`; public Admin API captures show oversized article bodies return the same "Content is too big (maximum is 1 MB)" message with `code: null`, so the proxy preserves that public response shape.

`articleCreate` and `articleUpdate` validate supplied `blogId` values against effective local/hydrated blogs before staging. A non-existent blog ID returns `NOT_FOUND` on `field: ["article"]` with `message: "Must reference an existing blog."`; create does not stage an article, and update leaves the existing article/blog binding unchanged. The inline `blog` argument on `articleCreate` remains a local blog-create path. `articleUpdate` also runs article-specific cross-field validation before staging: it rejects simultaneous `author.name` and `author.userId` with `AMBIGUOUS_AUTHOR`, rejects unresolved `author.userId` with `AUTHOR_MUST_EXIST`, and rejects `image.altText` without a new URL when the existing article has no image with `INVALID` on `["article", "image"]`. Current public Admin schema captures reject non-executable `authorV2` / inline-blog update inputs before resolver execution, so those branches remain schema-boundary evidence rather than local mutation behavior.

Unknown content IDs return local `userErrors` for supported mutations instead of proxying upstream. Blog, page, and article delete mutations stage tombstones so downstream detail reads return `null` and catalog/nested connections omit the deleted row. `blogDelete` also silently tombstones articles for the deleted blog and their comments, matching Shopify's dependent destroy behavior without adding cascade `userErrors`. `commentDelete` is the exception among online-store content deletes: Shopify destroys the comment row, so the proxy stages a hard local deletion marker rather than a `REMOVED` content record.

Comment creation is not part of the captured Admin GraphQL root set, so comment moderation support is intentionally limited to comments supplied by snapshot or hydrated upstream reads.

### Presentation and integration behavior

The normalized local integration graph covers themes, script tags, web pixels, server pixels, storefront access tokens, and mobile platform applications. In live-hybrid mode, cold sales-channel reads for themes, script tags, web pixels, server pixels, and mobile platform applications forward upstream and hydrate observed records into this graph. Once the relevant records are known locally, downstream reads are served from the graph with staged mutations overlaid. Supported presentation and integration mutations remain local-only at runtime:

- theme mutations stage theme metadata, publish role changes, deletion tombstones, and theme-file copy/upsert/delete effects in memory
- `themeUpdate` stages only `name` changes; non-`name` input fields are rejected at GraphQL schema validation, blank or whitespace names return `INVALID`, and `LOCKED` themes return `CANNOT_UPDATE_LOCKED_THEME` without mutating local state
- `themePublish` flips the staged target theme to `MAIN` and demotes any effective local main theme to `UNPUBLISHED`, without changing storefront presentation
- `themePublish` rejects `DEVELOPMENT` themes without staging role changes, returning `field: ["base"]`, `code: null`, and `message: "You cannot publish a development theme."`
- `themePublish` returns a local `userErrors` response without staging when the target theme is in a non-publishable `DEMO`, `LOCKED`, or `ARCHIVED` role
- `themeDelete` stages local deletion tombstones for deletable themes, but refuses to delete the only effective local `MAIN` theme with `field: ["id"]`, message `You can't delete your only published theme.`, and `INVALID`
- theme-file bodies are stored in local theme records and exposed through `OnlineStoreTheme.files`; no asset upload, CDN write, or remote URL fetch is performed. Local `themeFilesUpsert` accepts filenames under `templates/`, `sections/`, `snippets/`, `layout/`, `config/`, `locales/`, or `assets/`, rejects blank, invalid, duplicate, and `_drafts/` filenames, enforces the 50-file input cap, decodes `TEXT` and `BASE64` bodies into persisted content, stages `URL` bodies as inert URL markers, computes `checksumMd5` from the persisted body content, computes `size` from UTF-8 body bytes, rejects stale `checksumMd5` compare inputs with `CONFLICT`, and assigns deterministic synthetic `createdAt`/`updatedAt` timestamps. Local `themeFilesUpsert` payloads always expose `job`; rejected and inline `TEXT`/`BASE64` writes return `job: null`, while accepted `URL` body inputs return a synthetic pending `Job`.
- `themeFilesCopy` reads the existing local source body before deriving copied file metadata, preserves source `NOT_FOUND` userErrors, and rejects duplicate `dstFilename` values or batches over 50 files before staging. `themeFilesDelete` rejects duplicate filenames, batches over 100 files, and required theme files from the configured undeletable-file list.
- `themeFilesUpsert`, `themeFilesCopy`, and `themeFilesDelete` serialize `OnlineStoreThemeFileOperationResult` entries with selected `filename`, `createdAt`, `updatedAt`, `size`, `checksumMd5`, and `body` fields from the staged theme-file record.
- Theme-limited-plan entitlement state is not currently represented in the proxy store or config, so the Shopify `theme_limited_plan` userError gate for `themeFilesUpsert`/`themeFilesDelete` remains out of scope instead of being guessed from unrelated shop fields.
- script tag mutations stage `src`, normalized lowercase `displayScope`, forced `event: "onload"`, and `cache` values, but never load or execute storefront JavaScript. `scriptTagCreate` rejects missing/blank, over-255-character, malformed, and non-HTTPS `src` values before staging; `scriptTagUpdate` applies the same checks only to changed fields, returns update-shaped field paths such as `["src"]` and `["displayScope"]`, and returns `NOT_FOUND` for ids that are not already known from local staging or upstream observation. GraphQL responses expose display scope as the Admin enum form (`ONLINE_STORE`, `ALL`, or `ORDER_STATUS`) and event as `onload`.
- web pixel and server pixel mutations stage inert configuration only; no browser tracking, EventBridge send, Pub/Sub send, webhook registration, or customer-event subscription is activated. Server pixel endpoint updates require an existing current ServerPixel, reject malformed EventBridge ARNs and blank Pub/Sub project/topic values before staging, and return `ServerPixelUserError` codes for endpoint configuration failures.
- `webPixelCreate` enforces Shopify Core's one-WebPixel-per-calling-app/api-permission guard in the local staged graph: a duplicate effective WebPixel returns `webPixel: null` plus one `TAKEN` `WebPixelUserError` and does not mint or stage a new WebPixel
- `webPixelUpdate` parses supplied `settings` JSON before staging. Malformed JSON returns `INVALID_CONFIGURATION_JSON` on `["settings"]`, and valid JSON is stored as parsed JSON rather than as the raw string literal.
- `webPixelUpdate` validates supplied `runtimeContext` values against any runtime-context declaration carried on the staged WebPixel record (`runtimeContexts` / `runtime_contexts`) and validates known settings keys against any staged extension `settingsDefinition` / `settings_definition` metadata. Type, range, and regex violations return `INVALID_SETTINGS` on `["settings"]`; runtime-context mismatches return `INVALID_RUNTIME_CONTEXT` on `["webPixel", "runtimeContext"]`.
- WebPixel records persist only WebPixel fields; `webhookEndpointAddress` is kept on ServerPixel records only. Successful local WebPixel create/update responses return `status: "CONNECTED"` and non-null parsed JSON settings, using `{}` when no settings were supplied.
- `storefrontAccessTokenCreate` stages credential creation locally; create returns a deterministic unique `shpat_<16-hex>` token, non-empty storefront access scopes, and selected `shop { id }`, while `Shop.storefrontAccessTokens` downstream reads and meta state keep generated token values redacted as `shpat_redacted`. `storefrontAccessTokenDelete` remains unsupported and is not locally modeled.
- mobile platform application mutations stage Android/Apple verification settings locally and expose union-shaped downstream reads. `mobilePlatformApplicationCreate` rejects inputs that specify neither or both platform branches, rejects blank platform identifiers, enforces the 100-character Android `applicationId` / Apple `appId` cap, requires non-empty Android `sha256CertFingerprints`, requires Apple `appClipApplicationId` when `appClipsEnabled` is true, and caps that app-clip ID at 255 characters. Repeated same-platform creates are accepted as separate records, matching Core's lack of a per-shop platform uniqueness constraint. Updates apply only the matching platform sub-input to the staged record, reject wrong-platform sub-inputs with `INVALID` on `["mobilePlatformApplication"]`, and run the same changed-field model validations before mutating local state.

Snapshot/local empty reads return Shopify-like `null` for missing singular roots and empty connections for catalog roots. Live-hybrid cold reads forward sales-channel roots upstream before falling back to local rendering, then observed records satisfy later reads without another upstream call. Local connection support for `themes`, `scriptTags`, and `mobilePlatformApplications` uses shared cursor/window helpers. `themes` supports local `roles` and `names` filters; `scriptTags` supports local `src` and simple text query filtering.

The local model preserves original raw mutations in the meta log for eventual commit replay. Sensitive generated storefront token values are not exposed through `GET /__meta/state`; the original request body is still retained so commit replay can send the merchant-authored mutation in original order.

### Captured quirks

The 2025-01 live captures showed `comment(id:)` with an unknown synthetic ID, and with a just-destroyed comment ID after `commentDelete`, returning a Shopify internal error, while unknown-id `commentApprove`, `commentSpam`, `commentNotSpam`, and `commentDelete` returned normal `userErrors` with `field: ["id"]` and message `Comment does not exist`. The proxy does not emulate the internal-error branch for local snapshot reads; local missing comments return `null` to preserve stable no-data behavior.

Live evidence in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/online-store/online-store-content-required-fields.json` shows omitted required `title` fields are rejected by Shopify GraphQL validation before the mutation resolver runs. Blank-string titles reach model validation and return title `userErrors`; Shopify returns `code: "BLANK"` for page/article and `code: null` for blog in the captured API version. The proxy deliberately normalizes missing and blank titles to a local `BLANK` userError for all three roots to satisfy stable draft-proxy validation and avoid staging empty-title records.

Live evidence in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/online-store/online-store-body-script-verbatim.json` and `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/online-store/online-store-body-script-verbatim.json` shows Admin GraphQL preserves `pageCreate` and `articleCreate` body HTML verbatim, including `<script>` blocks and event-handler attributes, in both mutation payloads and immediate `page(id:)` / `article(id:)` reads. `Page.bodySummary` strips tags but keeps script text; `Article.summary` remains `null` when omitted. The proxy mirrors that behavior and does not scrub supported page/article body inputs locally.

Live schema evidence showed `Page.onlineStoreUrl` is not an Admin GraphQL `Page` field in the probed versions, including 2025-01, 2026-04, and `unstable`; selecting it returned a GraphQL schema error rather than a nullable field. The same capture showed `Article.seo` is absent from the Article schema, even though `ArticleCreateInput` and `ArticleUpdateInput` expose `image` and `metafields`. Do not synthesize page online-store URLs or article SEO fields locally unless a later Shopify schema capture proves a supported Admin GraphQL surface.

### Registry-only navigation roots

- `menu`
- `menus`
- `menuCreate`
- `menuUpdate`
- `menuDelete`

Navigation/menu support remains blocked on menu item tree shape, nested resource references, navigation-specific validation, and downstream read-after-write evidence. Do not promote those roots until a local navigation model exists.

Disposable page-to-menu evidence shows `menuCreate` with a PAGE item returned `Menu.items[0]` with `type: "PAGE"`, `resourceId` equal to the created page ID, and `url: "/pages/<page-handle>"`; `menu(id:)` returned the created menu, `menuUpdate` replaced items and generated new menu-item IDs, `menuDelete` returned `deletedMenuId`, and a downstream `menu(id:)` read returned `null`. The same capture showed `menus(first: 5, query: "handle:<created-handle>")` returning the shop default menus rather than the newly created custom menu, so menu catalog behavior needs dedicated navigation coverage before these roots can be marked implemented.
