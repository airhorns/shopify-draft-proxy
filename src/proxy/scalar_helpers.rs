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

pub(in crate::proxy) enum EmailValidationMode {
    Basic,
    Strict,
    AtSign,
}

pub(in crate::proxy) fn shopify_email_is_valid(email: &str, mode: EmailValidationMode) -> bool {
    match mode {
        EmailValidationMode::Basic => {
            shopify_basic_email_parts(email).is_some_and(|(_, domain)| {
                domain.contains('.')
                    && !domain.starts_with('.')
                    && !domain.ends_with('.')
                    && !email.contains(' ')
            })
        }
        EmailValidationMode::Strict => shopify_data_sale_opt_out_email_is_valid(email),
        EmailValidationMode::AtSign => email.contains('@'),
    }
}

fn shopify_basic_email_parts(email: &str) -> Option<(&str, &str)> {
    let (local, domain) = email.split_once('@')?;
    (!local.is_empty() && !domain.is_empty() && !domain.contains('@')).then_some((local, domain))
}

fn shopify_data_sale_opt_out_email_is_valid(email: &str) -> bool {
    let Some((local, domain)) = shopify_basic_email_parts(email) else {
        return false;
    };
    let local_valid = !local.is_empty()
        && local.chars().count() <= 128
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local.split('.').all(|atom| {
            !atom.is_empty()
                && atom.chars().all(|character| {
                    character.is_alphanumeric() || "!\"#$%&'*+-/=?^_`{|}~".contains(character)
                })
        });
    if domain.is_empty()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || domain.contains("..")
    {
        return false;
    }
    let labels = domain.split('.').collect::<Vec<_>>();
    let Some(tld) = labels.last() else {
        return false;
    };
    let domain_valid = labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.chars().next().is_some_and(char::is_alphanumeric)
                && label.chars().last().is_some_and(char::is_alphanumeric)
                && label
                    .chars()
                    .all(|character| character.is_alphanumeric() || character == '-')
        })
        && (1..=64).contains(&tld.chars().count())
        && tld.chars().all(char::is_alphabetic);
    email.chars().count() <= 255 && local_valid && domain_valid
}
