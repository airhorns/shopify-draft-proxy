// Shopify-derived Markets country/region support data.
//
// Generated from live Admin GraphQL conformance capture:
// fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/market-create-unsupported-country-region.json

pub(in crate::proxy) fn is_unsupported_country_region(country_code: &str) -> bool {
    UNSUPPORTED_COUNTRY_REGION_CODES.contains(&country_code)
}

pub(in crate::proxy) const UNSUPPORTED_COUNTRY_REGION_CODES: &[&str] =
    &["AN", "BV", "CU", "HM", "IR", "KP", "SY"];
