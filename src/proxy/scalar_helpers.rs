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
