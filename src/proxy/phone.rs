use super::*;

pub(in crate::proxy) fn normalize_phone_with_country_context(
    raw: &str,
    country_code: Option<&str>,
    allow_masked: bool,
) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if allow_masked && trimmed.contains('*') {
        return Some(trimmed.to_string());
    }

    let starts_with_plus = trimmed.starts_with('+') || trimmed.starts_with('\u{FF0B}');
    if !starts_with_plus && trimmed.chars().any(|c| c == '+' || c == '\u{FF0B}') {
        return None;
    }
    if !trimmed.chars().all(phone_supported_character) {
        return None;
    }

    let digits = trimmed
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }

    let e164_digits = if starts_with_plus {
        digits
    } else {
        let calling_code = country_code.and_then(country_calling_code)?;
        if digits.starts_with(calling_code) && digits.len() > 10 {
            digits
        } else {
            format!("{calling_code}{digits}")
        }
    };

    if (8..=15).contains(&e164_digits.len()) {
        Some(format!("+{e164_digits}"))
    } else {
        None
    }
}

pub(in crate::proxy) fn normalize_phone_with_existing_e164_context(
    raw: &str,
    existing_e164: Option<&str>,
    allow_masked: bool,
) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if allow_masked && trimmed.contains('*') {
        return Some(trimmed.to_string());
    }

    let starts_with_plus = trimmed.starts_with('+') || trimmed.starts_with('\u{FF0B}');
    if starts_with_plus {
        return normalize_phone_with_country_context(raw, None, allow_masked);
    }
    if trimmed
        .chars()
        .any(|c| c == '+' || c == '\u{FF0B}' || !phone_supported_character(c))
    {
        return None;
    }

    let existing = existing_e164?.trim();
    let existing_digits = existing
        .strip_prefix('+')
        .or_else(|| existing.strip_prefix('\u{FF0B}'))?;
    if existing_digits.is_empty() || !existing_digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let digits = trimmed
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    let calling_code_len = existing_digits.len().checked_sub(digits.len())?;
    if !(1..=3).contains(&calling_code_len) {
        return None;
    }

    let e164_digits = format!("{}{}", &existing_digits[..calling_code_len], digits);
    if (8..=15).contains(&e164_digits.len()) {
        Some(format!("+{e164_digits}"))
    } else {
        None
    }
}

pub(in crate::proxy) fn shop_country_code(shop: &Value) -> Option<&str> {
    shop.pointer("/shopAddress/countryCodeV2")
        .and_then(Value::as_str)
        .or_else(|| {
            shop.pointer("/shopAddress/countryCode")
                .and_then(Value::as_str)
        })
        .filter(|code| !code.trim().is_empty())
}

pub(in crate::proxy) fn value_country_code(value: &Value) -> Option<&str> {
    value
        .get("countryCodeV2")
        .and_then(Value::as_str)
        .or_else(|| value.get("countryCode").and_then(Value::as_str))
        .filter(|code| !code.trim().is_empty())
}

fn phone_supported_character(c: char) -> bool {
    c.is_ascii_digit()
        || matches!(
            c,
            '+' | '\u{FF0B}'
                | ' '
                | '\t'
                | '\n'
                | '\r'
                | '('
                | ')'
                | '-'
                | '.'
                | '\u{2010}'
                | '\u{2011}'
                | '\u{2012}'
                | '\u{2013}'
                | '\u{2014}'
                | '\u{00A0}'
        )
}

fn country_calling_code(country_code: &str) -> Option<&'static str> {
    match country_code.trim().to_ascii_uppercase().as_str() {
        "AC" => Some("247"),
        "AD" => Some("376"),
        "AE" => Some("971"),
        "AF" => Some("93"),
        "AG" | "AI" | "AS" | "BB" | "BM" | "BS" | "CA" | "DM" | "DO" | "GD" | "GU" | "JM"
        | "KN" | "KY" | "LC" | "MP" | "MS" | "PR" | "SX" | "TC" | "TT" | "US" | "VC" | "VG"
        | "VI" => Some("1"),
        "AL" => Some("355"),
        "AM" => Some("374"),
        "AO" => Some("244"),
        "AR" => Some("54"),
        "AT" => Some("43"),
        "AU" | "CC" | "CX" => Some("61"),
        "AW" => Some("297"),
        "AX" | "FI" => Some("358"),
        "AZ" => Some("994"),
        "BA" => Some("387"),
        "BD" => Some("880"),
        "BE" => Some("32"),
        "BF" => Some("226"),
        "BG" => Some("359"),
        "BH" => Some("973"),
        "BI" => Some("257"),
        "BJ" => Some("229"),
        "BL" | "FR" | "GF" | "GP" | "MF" | "MQ" | "RE" | "YT" => Some("33"),
        "BN" => Some("673"),
        "BO" => Some("591"),
        "BQ" | "CW" => Some("599"),
        "BR" => Some("55"),
        "BT" => Some("975"),
        "BW" => Some("267"),
        "BY" => Some("375"),
        "BZ" => Some("501"),
        "CD" => Some("243"),
        "CF" => Some("236"),
        "CG" => Some("242"),
        "CH" => Some("41"),
        "CI" => Some("225"),
        "CK" => Some("682"),
        "CL" => Some("56"),
        "CM" => Some("237"),
        "CN" => Some("86"),
        "CO" => Some("57"),
        "CR" => Some("506"),
        "CU" => Some("53"),
        "CV" => Some("238"),
        "CY" => Some("357"),
        "CZ" => Some("420"),
        "DE" => Some("49"),
        "DJ" => Some("253"),
        "DK" => Some("45"),
        "DZ" => Some("213"),
        "EC" => Some("593"),
        "EE" => Some("372"),
        "EG" => Some("20"),
        "ER" => Some("291"),
        "ES" => Some("34"),
        "ET" => Some("251"),
        "FJ" => Some("679"),
        "FK" => Some("500"),
        "FM" => Some("691"),
        "FO" => Some("298"),
        "GA" => Some("241"),
        "GB" | "GG" | "IM" | "JE" => Some("44"),
        "GE" => Some("995"),
        "GH" => Some("233"),
        "GI" => Some("350"),
        "GL" => Some("299"),
        "GM" => Some("220"),
        "GN" => Some("224"),
        "GQ" => Some("240"),
        "GR" => Some("30"),
        "GT" => Some("502"),
        "GW" => Some("245"),
        "GY" => Some("592"),
        "HK" => Some("852"),
        "HN" => Some("504"),
        "HR" => Some("385"),
        "HT" => Some("509"),
        "HU" => Some("36"),
        "ID" => Some("62"),
        "IE" => Some("353"),
        "IL" => Some("972"),
        "IN" => Some("91"),
        "IO" => Some("246"),
        "IQ" => Some("964"),
        "IR" => Some("98"),
        "IS" => Some("354"),
        "IT" | "VA" => Some("39"),
        "JO" => Some("962"),
        "JP" => Some("81"),
        "KE" => Some("254"),
        "KG" => Some("996"),
        "KH" => Some("855"),
        "KI" => Some("686"),
        "KM" => Some("269"),
        "KP" => Some("850"),
        "KR" => Some("82"),
        "KW" => Some("965"),
        "KZ" | "RU" => Some("7"),
        "LA" => Some("856"),
        "LB" => Some("961"),
        "LI" => Some("423"),
        "LK" => Some("94"),
        "LR" => Some("231"),
        "LS" => Some("266"),
        "LT" => Some("370"),
        "LU" => Some("352"),
        "LV" => Some("371"),
        "LY" => Some("218"),
        "MA" | "EH" => Some("212"),
        "MC" => Some("377"),
        "MD" => Some("373"),
        "ME" => Some("382"),
        "MG" => Some("261"),
        "MH" => Some("692"),
        "MK" => Some("389"),
        "ML" => Some("223"),
        "MM" => Some("95"),
        "MN" => Some("976"),
        "MO" => Some("853"),
        "MR" => Some("222"),
        "MT" => Some("356"),
        "MU" => Some("230"),
        "MV" => Some("960"),
        "MW" => Some("265"),
        "MX" => Some("52"),
        "MY" => Some("60"),
        "MZ" => Some("258"),
        "NA" => Some("264"),
        "NC" => Some("687"),
        "NE" => Some("227"),
        "NF" => Some("672"),
        "NG" => Some("234"),
        "NI" => Some("505"),
        "NL" => Some("31"),
        "NO" | "SJ" => Some("47"),
        "NP" => Some("977"),
        "NR" => Some("674"),
        "NU" => Some("683"),
        "NZ" => Some("64"),
        "OM" => Some("968"),
        "PA" => Some("507"),
        "PE" => Some("51"),
        "PF" => Some("689"),
        "PG" => Some("675"),
        "PH" => Some("63"),
        "PK" => Some("92"),
        "PL" => Some("48"),
        "PM" => Some("508"),
        "PS" => Some("970"),
        "PT" => Some("351"),
        "PW" => Some("680"),
        "PY" => Some("595"),
        "QA" => Some("974"),
        "RO" => Some("40"),
        "RS" => Some("381"),
        "RW" => Some("250"),
        "SA" => Some("966"),
        "SB" => Some("677"),
        "SC" => Some("248"),
        "SD" => Some("249"),
        "SE" => Some("46"),
        "SG" => Some("65"),
        "SH" | "TA" => Some("290"),
        "SI" => Some("386"),
        "SK" => Some("421"),
        "SL" => Some("232"),
        "SM" => Some("378"),
        "SN" => Some("221"),
        "SO" => Some("252"),
        "SR" => Some("597"),
        "SS" => Some("211"),
        "ST" => Some("239"),
        "SV" => Some("503"),
        "SY" => Some("963"),
        "SZ" => Some("268"),
        "TD" => Some("235"),
        "TG" => Some("228"),
        "TH" => Some("66"),
        "TJ" => Some("992"),
        "TK" => Some("690"),
        "TL" => Some("670"),
        "TM" => Some("993"),
        "TN" => Some("216"),
        "TO" => Some("676"),
        "TR" => Some("90"),
        "TV" => Some("688"),
        "TW" => Some("886"),
        "TZ" => Some("255"),
        "UA" => Some("380"),
        "UG" => Some("256"),
        "UY" => Some("598"),
        "UZ" => Some("998"),
        "VE" => Some("58"),
        "VN" => Some("84"),
        "VU" => Some("678"),
        "WF" => Some("681"),
        "WS" => Some("685"),
        "XK" => Some("383"),
        "YE" => Some("967"),
        "ZA" => Some("27"),
        "ZM" => Some("260"),
        "ZW" => Some("263"),
        _ => None,
    }
}
