# Online Store Endpoint Group

The online-store group tracks Admin GraphQL roots for storefront presentation and integration state: navigation menus, themes and theme files, script tags, storefront pixels, server pixels, storefront access tokens, and mobile platform applications.

## Implemented roots

None yet.

HAR-304 is registry/documentation inventory only. Registry entries in this group are declared gaps and must not be treated as supported runtime capabilities until a local model can reproduce downstream reads without sending supported mutations to Shopify.

## Safe read gaps tracked by the registry

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

## Side-effect mutation gaps tracked by the registry

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

## Evidence and blockers

- Current evidence: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-graphql-root-operation-introspection.json` proves the listed query and mutation root names exist in the captured Admin GraphQL schema.
- Current blocker: no checked-in parity scenario or live capture records online-store read shapes, validation branches, or safe lifecycle behavior for these roots.
- Safety boundary: publish, theme-file, pixel, script tag, token, and mobile-platform mutations are externally visible and must not be marked implemented from validation-only or branch-only evidence.
- Parity-spec boundary: do not add planned-only parity specs for these gaps. Add `config/parity-specs` entries only when backed by captured interactions and executable comparison or runtime-test-backed fixture evidence.

## Validation anchors

- Registry schema/discovery: `corepack pnpm conformance:check`
- Root coverage snapshot: `corepack pnpm vitest run tests/unit/graphql-operation-coverage.test.ts`
- Online-store routing guard: `corepack pnpm vitest run tests/unit/online-store-registry.test.ts`
