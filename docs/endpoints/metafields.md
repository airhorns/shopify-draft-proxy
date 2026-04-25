# Metafields

## Metafield definition reads

The product-owner definition slice supports the Admin GraphQL read roots:

- `metafieldDefinition(identifier:)`
- `metafieldDefinitions(ownerType:, first:, after:, reverse:, sortKey:, query:, namespace:, key:, pinnedStatus:, constraintStatus:, constraintSubtype:)`

The implementation is snapshot/local only for definition reads. In snapshot mode, missing singular definitions return `null`, and catalog misses return a non-null empty connection with empty `nodes` / `edges` and falsey `pageInfo`.

Normalized state stores definition records separately from product metafields. The supported owner type is `PRODUCT`; definition-scoped `metafields` and `metafieldsCount` are derived from the effective product-owned metafield set by matching `namespace` and `key`. This keeps staged `metafieldsSet` writes visible through a matching product definition without adding definition lifecycle mutation support.

The serializer currently covers these selected definition fields:

- `id`, `name`, `namespace`, `key`, `ownerType`
- `type { name category }`
- `description`
- `validations { name value }`
- `access`
- `capabilities`
- `constraints`
- `pinnedPosition`
- `validationStatus`
- `metafieldsCount`
- `metafields`

Catalog filters are intentionally limited to the fixture-backed product-owner slice: owner type, namespace, key, pinned status, constraint status/subtype, and search query terms for `id`, `namespace`, `key`, `owner_type`, and `type`. `sortKey: PINNED_POSITION` follows the captured Shopify ordering where higher pinned positions sort before lower pinned positions.

Definition lifecycle mutations other than `standardMetafieldDefinitionEnable` remain unsupported and must not be registered as local staged capabilities until they are modeled and covered separately.

## Standard metafield definition enablement

`standardMetafieldDefinitionEnable` stages a normalized metafield definition locally from the HAR-257 captured standard template slice. Supported selectors are the fixture-backed template IDs/namespaces in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/standard-metafield-definition-enable-validation.json`.

Successful local enablement:

- creates or replaces a staged `MetafieldDefinition` record without sending the mutation to Shopify
- supports `id` or `namespace` / `key` template selection for the captured template slice
- applies `ownerType`, selected `access`, selected `capabilities`, and `pin`
- returns a Shopify-like `createdDefinition` payload
- makes downstream `metafieldDefinition(identifier:)` and `metafieldDefinitions(...)` reads observe the staged definition

HAR-257 captured safe no-side-effect validation behavior in `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/standard-metafield-definition-enable-validation.json`:

- no `id` and no `namespace` / `key` returns `createdDefinition: null` with `TEMPLATE_NOT_FOUND`
- an unknown template ID returns `field: ["id"]`, `TEMPLATE_NOT_FOUND`
- an unknown namespace/key selector returns `field: null`, `TEMPLATE_NOT_FOUND`
- template ID `1` with incompatible owner type `CUSTOMER` returns the same invalid-template-ID branch
