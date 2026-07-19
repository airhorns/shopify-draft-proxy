use isocountry::CountryCode;

pub(in crate::proxy) fn country_display_name(country_code: &str) -> String {
    match country_code {
        // Preserve the shorter names already observed in discount summaries.
        "GB" => "United Kingdom".to_string(),
        "US" => "United States".to_string(),
        code => CountryCode::for_alpha2_caseless(code)
            .map(|country| country.name().to_string())
            .unwrap_or_else(|_| country_code.to_string()),
    }
}
