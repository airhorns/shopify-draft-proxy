use super::*;

pub(in crate::proxy) fn bulk_operation_record_with(
    id: &str,
    status: &str,
    query: &str,
    count: &str,
    created_at: &str,
    file_size: &str,
) -> Value {
    bulk_operation_record_with_type(id, status, "QUERY", query, count, created_at, file_size)
}

pub(in crate::proxy) fn bulk_operation_record_with_type(
    id: &str,
    status: &str,
    operation_type: &str,
    query: &str,
    count: &str,
    created_at: &str,
    file_size: &str,
) -> Value {
    let completed = status == "COMPLETED";
    let file_size_value = if completed {
        json!(file_size)
    } else {
        Value::Null
    };
    json!({
        "id": id,
        "status": status,
        "type": operation_type,
        "errorCode": null,
        "createdAt": created_at,
        "completedAt": if completed { json!(created_at) } else { Value::Null },
        "objectCount": if completed { count } else { "0" },
        "rootObjectCount": if completed { count } else { "0" },
        "fileSize": file_size_value,
        "url": if completed { json!(format!("/__meta/bulk-operations/{}/result.jsonl", resource_id_path_tail(id))) } else { Value::Null },
        "partialDataUrl": null,
        "query": query
    })
}
