use super::*;

pub(in crate::proxy) fn shallow_merge_object(target: &mut Value, source: Value) {
    if let (Value::Object(target), Value::Object(source)) = (target, source) {
        for (key, value) in source {
            target.insert(key, value);
        }
    }
}

pub(in crate::proxy) fn shallow_merged_object(mut left: Value, right: Value) -> Value {
    if left.is_object() && right.is_object() {
        shallow_merge_object(&mut left, right);
        left
    } else {
        right
    }
}

pub(in crate::proxy) fn push_unique_string(values: &mut Vec<String>, value: impl AsRef<str>) {
    let value = value.as_ref();
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

pub(in crate::proxy) fn extend_unique_strings<I, S>(values: &mut Vec<String>, incoming: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for value in incoming {
        push_unique_string(values, value);
    }
}

pub(in crate::proxy) fn set_relation(
    object: &mut Value,
    id_key: &str,
    relation_key: &str,
    id: Option<&str>,
) {
    if let Some(object) = object.as_object_mut() {
        if let Some(id) = id {
            object.insert(id_key.to_string(), json!(id));
            object.insert(relation_key.to_string(), json!({ "id": id }));
        } else {
            object.insert(id_key.to_string(), Value::Null);
            object.insert(relation_key.to_string(), Value::Null);
        }
    }
}

pub(in crate::proxy) fn string_array_from_json(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect()
}
