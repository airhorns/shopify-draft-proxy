use super::*;
use crate::graphql::{parsed_document, ParsedDocument, RawArgumentValue};
use std::collections::{BTreeMap, BTreeSet};

mod inventory_helpers;
mod marketing_helpers;
mod webhook_helpers;

pub(in crate::proxy) use self::inventory_helpers::*;
pub(in crate::proxy) use self::webhook_helpers::webhook_subscription_sort_key_validation_error;
fn comparison_operator_prefix<'a>(
    value: &'a str,
    operators: &[&'static str],
) -> Option<(&'static str, &'a str)> {
    operators
        .iter()
        .find_map(|&operator| value.strip_prefix(operator).map(|tail| (operator, tail)))
}
