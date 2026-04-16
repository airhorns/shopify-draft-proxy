# Collection mutation conformance blocker

## What failed

Attempted to capture live conformance for the full collection mutation family:

- `collectionCreate`
- `collectionUpdate`
- `collectionDelete`
- `collectionAddProducts`
- `collectionRemoveProducts`

Live probe still works, but the first mutation capture failed immediately on `collectionCreate` with Shopify Admin GraphQL:

- `ACCESS_DENIED`
- required access: `write_products`

Observed error excerpt:

> Access denied for collectionCreate field. Required access: `write_products` access scope. Also: The app must have access to the input fields used to create the collection. Further, the store must not be on the Starter or Retail plans and user must have a permission to create collection.

## Why this blocks closure

These five operations can now be moved to `planned-with-proxy-request`, but they cannot be promoted to captured/covered without a token that can safely perform collection writes against the conformance store.

Because the failure happens on the first live write, the current token cannot settle:

- mutation payload shape parity
- userErrors parity for successful safe writes
- immediate downstream `collection` / `collections` / `product.collections` read-after-write parity

## What was completed anyway

This run still advanced the family by:

1. adding concrete parity-request scaffolds for all five collection mutations
2. adding focused unit tests proving those request artifacts exist and match the staged runtime slices
3. updating scenario/parity notes so the next live capture step is explicit per operation

## Recommended next step

Switch the repo conformance credential to a store token that includes `write_products` and can mutate the safe dev store, then rerun live collection mutation capture for this family.
