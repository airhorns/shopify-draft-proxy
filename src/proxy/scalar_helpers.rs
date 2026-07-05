pub(in crate::proxy) fn token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

pub(in crate::proxy) fn token_chars_valid(value: &str) -> bool {
    value.chars().all(token_char)
}

pub(in crate::proxy) fn graphql_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

pub(in crate::proxy) fn graphql_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub(in crate::proxy) fn file_extension(value: &str) -> String {
    let path = value.split(['?', '#']).next().unwrap_or(value);
    let filename = path
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("");
    filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_string())
        .unwrap_or_default()
}

pub(in crate::proxy) fn search_comparator(value: &str) -> (&str, &str) {
    for operator in [">=", "<=", ">", "<", "="] {
        if let Some(rest) = value.strip_prefix(operator) {
            return (operator, rest);
        }
    }
    ("=", value)
}

pub(in crate::proxy) fn search_datetime_value<'a>(actual: &'a str, expected: &str) -> &'a str {
    if expected.contains('T') {
        actual
    } else {
        actual
            .split_once('T')
            .map(|(date, _)| date)
            .unwrap_or(actual)
    }
}

pub(in crate::proxy) fn normalized_search_query_value(value: &str) -> String {
    value
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase()
}

pub(in crate::proxy) fn ascii_word_starts_with(value: &str, prefix: &str) -> bool {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|part| part.starts_with(prefix))
}
