# Online Store Endpoint Group

The online-store group tracks Admin GraphQL roots for storefront content and presentation: articles, blogs, pages, comments, navigation menus, themes and theme files, script tags, storefront pixels, server pixels, storefront access tokens, and mobile platform applications.

## Current support and limitations

### Implemented roots

Content roots from HAR-303:

- Reads: `article`, `articleAuthors`, `articles`, `articleTags`, `blog`, `blogs`, `blogsCount`, `page`, `pages`, `pagesCount`, `comment`, `comments`
- Mutations: `articleCreate`, `articleUpdate`, `articleDelete`, `blogCreate`, `blogUpdate`, `blogDelete`, `pageCreate`, `pageUpdate`, `pageDelete`, `commentApprove`, `commentSpam`, `commentNotSpam`, `commentDelete`

The content model is normalized in memory as generic online-store content records for articles, blogs, pages, and comments. Snapshot mode serves these roots without upstream access. Live-hybrid mode hydrates captured upstream content reads, then serves the local graph when staged content exists.

Supported lifecycle mutations are staged locally and logged with the original raw GraphQL request for commit replay. They must not write to Shopify at normal runtime.

### Content read behavior

Snapshot/local empty behavior follows the 2025-01 capture in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/online-store-content-lifecycle.json`:

- missing `article(id:)`, `blog(id:)`, and `page(id:)` return `null`
- empty `articles`, `articleAuthors`, `blogs`, `pages`, and `comments` connections return empty `nodes`/`edges`, false page booleans, and null cursors when no local rows exist
- `articleTags(limit:)` returns an empty list when no local article tags exist
- `blogsCount` and `pagesCount` return `Count` payloads with `precision: "EXACT"`

Local connection support uses the shared GraphQL connection helpers for selected `nodes`, `edges`, and `pageInfo` fields. The local model supports simple id/title/handle/status text filtering, common sort keys, reverse ordering, and cursor windows. Captured opaque Shopify cursors are not decoded; newly staged local rows use stable synthetic cursor values.

Nested content behavior:

- `Blog.articles` and `Blog.articlesCount` are derived from effective local articles with that blog as parent
- `Article.blog` resolves through the local blog graph
- `Article.comments` and `Article.commentsCount` are derived from effective local comments with that article as parent
- `Comment.article` resolves through the local article graph
- `Article.author`, `articleAuthors`, and `articleTags` are derived from local article data

The HAR-352 parity promotion captures a Shopify quirk where top-level `articles` omitted a locally updated unpublished article while the same article still appeared through `Blog.articles`, `Article.blog`, `articleAuthors`, and `articleTags`. The proxy now mirrors that boundary: top-level `articles` filters out records with `isPublished: false`, while nested blog/article helper reads continue to expose the effective local graph.

Metafield, translation, and event subresources on these content types remain shallow: local reads return empty connections, empty translation lists, or `null` singular metafields rather than inventing unsupported subresource data.

### Content mutation behavior

Implemented local staging:

- `blogCreate` / `blogUpdate` / `blogDelete` stage blog title, handle, template suffix, and comment policy changes
- `pageCreate` / `pageUpdate` / `pageDelete` stage page title, handle, body/body summary, template suffix, and publication fields
- `articleCreate` / `articleUpdate` / `articleDelete` stage article title, handle, body, summary, tags, author, publication fields, and blog membership; `articleCreate` can also stage an inline blog from the optional `blog` argument
- `commentApprove`, `commentSpam`, `commentNotSpam`, and `commentDelete` stage moderation or tombstones for comments that already exist in hydrated/snapshot local state

Unknown content IDs return local `userErrors` for supported mutations instead of proxying upstream. Delete mutations stage tombstones so downstream detail reads return `null` and catalog/nested connections omit the deleted row.

Comment creation is not part of the Admin GraphQL root set captured for this ticket, so comment moderation support is intentionally limited to comments supplied by snapshot or hydrated upstream reads.

### Captured quirks

The 2025-01 live capture showed `comment(id:)` with an unknown synthetic ID returning a Shopify internal error, while unknown-id `commentApprove`, `commentSpam`, `commentNotSpam`, and `commentDelete` returned normal `userErrors` with `field: ["id"]` and message `Comment does not exist`. The proxy does not emulate the internal-error branch for local snapshot reads; local missing comments return `null` to preserve stable no-data behavior.

HAR-304 is registry/documentation inventory only for the presentation/integration roots below. Those entries remain declared gaps and must not be treated as supported runtime capabilities until a local model can reproduce downstream reads without sending supported mutations to Shopify.

### Safe read gaps tracked by the registry

Planned overlay reads:

- `menu`
- `menus`
- `theme`
- `themes`
- `scriptTag`
- `scriptTags`
- `webPixel`
- `serverPixel`
- `mobilePlatformApplication`
- `mobilePlatformApplications`

The checked-in Admin GraphQL root introspection fixture confirms these roots exist in the captured schema, but this repository does not yet have captured parity fixtures for their response shapes. Support remains blocked on safe live captures or equivalent fixture-backed evidence for:

- singular unknown-id/null behavior
- empty catalog behavior for connection roots
- pagination, selected filters, and sort keys where Shopify supports them
- theme role/status semantics and theme-file read behavior
- menu item tree shape and nested resource references
- app/scope constraints for script tags, pixels, and mobile platform applications

`storefrontAccessToken` has no read root in the checked-in root introspection fixture. Token work in this group is therefore tracked only through create/delete mutation blockers until a later schema capture proves a read surface.

### Side-effect mutation gaps tracked by the registry

Planned local-staging mutations:

- `menuCreate`
- `menuUpdate`
- `menuDelete`
- `themeCreate`
- `themeUpdate`
- `themeDelete`
- `themePublish`
- `themeFilesCopy`
- `themeFilesUpsert`
- `themeFilesDelete`
- `scriptTagCreate`
- `scriptTagUpdate`
- `scriptTagDelete`
- `webPixelCreate`
- `webPixelUpdate`
- `webPixelDelete`
- `serverPixelCreate`
- `serverPixelDelete`
- `eventBridgeServerPixelUpdate`
- `pubSubServerPixelUpdate`
- `storefrontAccessTokenCreate`
- `storefrontAccessTokenDelete`
- `mobilePlatformApplicationCreate`
- `mobilePlatformApplicationUpdate`
- `mobilePlatformApplicationDelete`

These roots can affect the live storefront, tracking integrations, theme assets, or access credentials. They must remain unsupported until local staging covers the mutation lifecycle, validation/userErrors, downstream read-after-write effects, and original raw mutation commit replay order.

## Historical and developer notes

### Evidence and blockers

- Current content evidence: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/online-store-content-lifecycle.json` captures content catalog/detail/empty reads, blog/page/article lifecycle success paths with downstream reads, and unknown-id comment moderation/delete userErrors. HAR-352 promotes this fixture through `config/parity-specs/online-store-content-lifecycle.json` and `config/parity-requests/online-store-content-*.graphql`; `corepack pnpm conformance:parity` seeds the captured baseline read, replays local create/update/read/delete/comment guardrail requests, and strictly compares stable payload/count/null-empty/userErrors fields against the recording.
- Current presentation/integration evidence: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-graphql-root-operation-introspection.json` proves the HAR-304 query and mutation root names exist in the captured Admin GraphQL schema.
- Current HAR-304 blocker: no checked-in parity scenario or live capture records presentation/integration read shapes, validation branches, or safe lifecycle behavior for those roots.
- Safety boundary: publish, theme-file, pixel, script tag, token, and mobile-platform mutations are externally visible and must not be marked implemented from validation-only or branch-only evidence.
- Parity-spec boundary: do not add planned-only parity specs for these gaps. Add `config/parity-specs` entries only when backed by captured interactions and executable comparison or runtime-test-backed fixture evidence.

### Validation anchors

- Registry schema/discovery: `corepack pnpm conformance:check`
- Root coverage snapshot: `corepack pnpm vitest run tests/unit/graphql-operation-coverage.test.ts`
- Online-store routing guard: `corepack pnpm vitest run tests/unit/online-store-registry.test.ts`
- Online-store content flow: `corepack pnpm exec vitest run tests/integration/online-store-content-flow.test.ts`
