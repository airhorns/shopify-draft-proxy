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

Definition lifecycle mutations remain unsupported and must not be registered as local staged capabilities until they are modeled and covered separately.
