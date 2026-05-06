//// Shared internal Markets domain types.

import shopify_draft_proxy/proxy/graphql_helpers.{type SourceValue}

@internal
pub type MarketConnectionItem {
  MarketConnectionItem(
    source: SourceValue,
    pagination_cursor: String,
    output_cursor: String,
  )
}

@internal
pub type MarketRegionInput {
  MarketRegionInput(field: List(String), country_code: String)
}
