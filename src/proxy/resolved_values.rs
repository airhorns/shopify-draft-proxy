use super::*;

pub(in crate::proxy) fn resolved_value_json(value: &ResolvedValue) -> Value {
    match value {
        ResolvedValue::String(value) => json!(value),
        ResolvedValue::Int(value) => json!(value),
        ResolvedValue::Float(value) => json!(value),
        ResolvedValue::Bool(value) => json!(value),
        ResolvedValue::Null => Value::Null,
        ResolvedValue::List(values) => {
            Value::Array(values.iter().map(resolved_value_json).collect())
        }
        ResolvedValue::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), resolved_value_json(value)))
                .collect(),
        ),
    }
}

pub(in crate::proxy) fn resolved_variables_json(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    Value::Object(
        variables
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_json(value)))
            .collect(),
    )
}

pub(in crate::proxy) fn resolved_list_arg(
    arguments: &BTreeMap<String, ResolvedValue>,
    name: &str,
) -> Vec<ResolvedValue> {
    match arguments.get(name) {
        Some(ResolvedValue::List(values)) => values.clone(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn resolved_string_list_arg(
    arguments: &BTreeMap<String, ResolvedValue>,
    name: &str,
) -> Vec<String> {
    resolved_list_arg(arguments, name)
        .iter()
        .filter_map(|value| match value {
            ResolvedValue::String(value) => Some(value.clone()),
            _ => None,
        })
        .collect()
}

pub(in crate::proxy) fn resolved_object_string(
    value: &ResolvedValue,
    name: &str,
) -> Option<String> {
    match value {
        ResolvedValue::Object(fields) => match fields.get(name) {
            Some(ResolvedValue::String(value)) => Some(value.clone()),
            _ => None,
        },
        _ => None,
    }
}

pub(in crate::proxy) fn resolved_value_string(value: &ResolvedValue) -> Option<String> {
    match value {
        ResolvedValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn resolved_string_arg(
    arguments: &BTreeMap<String, ResolvedValue>,
    name: &str,
) -> Option<String> {
    match arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) use self::resolved_string_arg as resolved_string_field;

pub(in crate::proxy) fn list_object_field(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<BTreeMap<String, ResolvedValue>> {
    match input.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(object) => Some(object.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) use self::list_object_field as resolved_object_list_field;

pub(in crate::proxy) fn list_string_field(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<String> {
    match input.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn resolved_i64_field(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Option<i64> {
    match input.get(key) {
        Some(ResolvedValue::Int(value)) => Some(*value),
        _ => None,
    }
}

pub(in crate::proxy) use self::resolved_i64_field as resolved_int_field;

pub(in crate::proxy) fn resolved_number_field(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Option<f64> {
    match input.get(key) {
        Some(ResolvedValue::Int(value)) => Some(*value as f64),
        Some(ResolvedValue::Float(value)) => Some(*value),
        _ => None,
    }
}

pub(in crate::proxy) fn resolved_string_path(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Option<String> {
    match resolved_input_path(input, path) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn resolved_f64_path(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Option<f64> {
    match resolved_input_path(input, path) {
        Some(ResolvedValue::Float(value)) => Some(*value),
        Some(ResolvedValue::Int(value)) => Some(*value as f64),
        Some(ResolvedValue::String(value)) => value.parse::<f64>().ok(),
        _ => None,
    }
}

pub(in crate::proxy) fn resolved_string_list_path(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Vec<String> {
    match resolved_input_path(input, path) {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn resolved_bool_path(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Option<bool> {
    match resolved_input_path(input, path) {
        Some(ResolvedValue::Bool(value)) => Some(*value),
        _ => None,
    }
}

pub(in crate::proxy) fn resolved_i64_path(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Option<i64> {
    resolved_input_path(input, path).and_then(resolved_i64)
}

pub(in crate::proxy) fn resolved_i64(value: &ResolvedValue) -> Option<i64> {
    match value {
        ResolvedValue::Int(n) => Some(*n),
        ResolvedValue::String(raw) => raw.parse::<i64>().ok(),
        _ => None,
    }
}

fn resolved_input_path<'a>(
    input: &'a BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Option<&'a ResolvedValue> {
    let (first, rest) = path.split_first()?;
    resolved_object_path(input.get(*first), rest)
}

pub(in crate::proxy) fn resolved_object_path<'a>(
    value: Option<&'a ResolvedValue>,
    path: &[&str],
) -> Option<&'a ResolvedValue> {
    let mut current = value?;
    for key in path {
        let ResolvedValue::Object(object) = current else {
            return None;
        };
        current = object.get(*key)?;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_nested_resolved_values_to_json() {
        let value = ResolvedValue::Object(BTreeMap::from([
            (
                "items".to_string(),
                ResolvedValue::List(vec![
                    ResolvedValue::String("gid://shopify/Product/1".to_string()),
                    ResolvedValue::Null,
                ]),
            ),
            ("enabled".to_string(), ResolvedValue::Bool(true)),
            ("count".to_string(), ResolvedValue::Int(2)),
            ("ratio".to_string(), ResolvedValue::Float(2.5)),
        ]));

        assert_eq!(
            resolved_value_json(&value),
            json!({
                "items": ["gid://shopify/Product/1", null],
                "enabled": true,
                "count": 2,
                "ratio": 2.5
            })
        );
    }

    #[test]
    fn serializes_resolved_variable_maps() {
        let variables = BTreeMap::from([
            (
                "id".to_string(),
                ResolvedValue::String("gid://shopify/App/1".to_string()),
            ),
            ("dryRun".to_string(), ResolvedValue::Bool(false)),
        ]);

        assert_eq!(
            resolved_variables_json(&variables),
            json!({
                "dryRun": false,
                "id": "gid://shopify/App/1"
            })
        );
    }

    #[test]
    fn reads_nested_resolved_paths() {
        let input = BTreeMap::from([(
            "details".to_string(),
            ResolvedValue::Object(BTreeMap::from([
                (
                    "name".to_string(),
                    ResolvedValue::String("Summer sale".to_string()),
                ),
                ("ratio".to_string(), ResolvedValue::Float(2.5)),
                ("intRatio".to_string(), ResolvedValue::Int(3)),
                (
                    "textRatio".to_string(),
                    ResolvedValue::String("4.75".to_string()),
                ),
                ("enabled".to_string(), ResolvedValue::Bool(true)),
                ("quantity".to_string(), ResolvedValue::Int(6)),
                (
                    "textQuantity".to_string(),
                    ResolvedValue::String("7".to_string()),
                ),
                (
                    "codes".to_string(),
                    ResolvedValue::List(vec![
                        ResolvedValue::String("SAVE".to_string()),
                        ResolvedValue::Int(10),
                    ]),
                ),
            ])),
        )]);

        assert_eq!(
            resolved_string_path(&input, &["details", "name"]),
            Some("Summer sale".to_string())
        );
        assert_eq!(resolved_f64_path(&input, &["details", "ratio"]), Some(2.5));
        assert_eq!(
            resolved_f64_path(&input, &["details", "intRatio"]),
            Some(3.0)
        );
        assert_eq!(
            resolved_f64_path(&input, &["details", "textRatio"]),
            Some(4.75)
        );
        assert_eq!(
            resolved_bool_path(&input, &["details", "enabled"]),
            Some(true)
        );
        assert_eq!(resolved_i64_path(&input, &["details", "quantity"]), Some(6));
        assert_eq!(
            resolved_i64_path(&input, &["details", "textQuantity"]),
            Some(7)
        );
        assert_eq!(
            resolved_string_list_path(&input, &["details", "codes"]),
            vec!["SAVE".to_string()]
        );
        assert_eq!(resolved_string_path(&input, &[]), None);
        assert_eq!(resolved_string_path(&input, &["missing"]), None);
    }
}
