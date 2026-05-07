//// Shopify-derived Markets country/region support data.
////
//// Generated from live Admin GraphQL conformance capture:
//// fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/market-create-unsupported-country-region.json

import gleam/list

@internal
pub fn is_unsupported_country_region(country_code: String) -> Bool {
  list.contains(unsupported_country_region_codes(), country_code)
}

@internal
pub fn unsupported_country_region_codes() -> List(String) {
  [
    "AN",
    "BV",
    "CU",
    "HM",
    "IR",
    "KP",
    "SY",
  ]
}
