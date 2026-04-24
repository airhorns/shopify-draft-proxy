# Collection mutation conformance capture blocker

- Last checked: 2026-04-24 UTC during HAR-162.
- Attempted validation: `corepack pnpm conformance:probe`.
- Result: stored Shopify conformance access token was invalid and refresh failed with `[API] Invalid API key or access token (unrecognized login or wrong password)`.
- Impact: `corepack pnpm conformance:capture-collection-mutations` cannot safely refresh live evidence in this session.
- Fixture policy: existing checked-in collection mutation fixtures were left unchanged.
- Unblock action: regenerate the conformance grant with `corepack pnpm conformance:auth-link`, complete the callback exchange with `corepack pnpm conformance:exchange-auth -- '<full callback url>'`, then rerun `corepack pnpm conformance:probe`.
