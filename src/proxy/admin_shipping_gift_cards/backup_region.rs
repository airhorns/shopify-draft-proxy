use super::*;

const BACKUP_REGION_ACCESS_SCOPES_QUERY: &str =
    "query BackupRegionAccessScopes { currentAppInstallation { accessScopes { handle } } }";
const BACKUP_REGION_ACCESS_SCOPES_CACHE_FIELD: &str =
    "__draftProxyBackupRegionAccessScopesHydrated";
const BACKUP_REGION_CURRENT_HYDRATE_QUERY: &str = r#"query BackupRegionCurrentHydrate {
  backupRegion {
    __typename
    id
    name
    ... on MarketRegionCountry {
      code
    }
  }
}"#;
// Must byte-match the recorded `ShippingDeliveryProfileVariantsHydrate` upstream
// call in the delivery-profile lifecycle captures (strict cassette compares
// query text + variables). Issued so a created/updated profile's
// `variantsToAssociate` resolve to the real product/variant the merchant
// associated — replayed through the cassette instead of fabricating a synthetic
// product id.

enum BackupRegionCountryCodeInput {
    ReadCurrent,
    CountryCode(String),
    Missing,
    Invalid(String),
}

fn backup_region_country_code_coercion_error(
    message: &str,
    operation_path: &str,
    code: &str,
    location: SourceLocation,
) -> Value {
    let mut extensions = serde_json::Map::from_iter([("code".to_string(), json!(code))]);
    if code == "missingRequiredInputObjectAttribute" {
        extensions.insert("argumentName".to_string(), json!("countryCode"));
        extensions.insert("argumentType".to_string(), json!("CountryCode!"));
        extensions.insert(
            "inputObjectType".to_string(),
            json!("BackupRegionUpdateInput"),
        );
    } else {
        extensions.insert("typeName".to_string(), json!("InputObject"));
        extensions.insert("argumentName".to_string(), json!("countryCode"));
    }

    json!({
        "message": message,
        "locations": [{ "line": location.line, "column": location.column }],
        "path": [operation_path, "backupRegionUpdate", "region", "countryCode"],
        "extensions": extensions
    })
}

impl DraftProxy {
    pub(in crate::proxy) fn backup_region_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let document = parsed_document(query, variables);
        let root_field = document.as_ref().and_then(|document| {
            document
                .root_fields
                .iter()
                .find(|field| field.name == "backupRegionUpdate")
        });
        let response_key = root_field
            .map(|field| field.response_key.clone())
            .unwrap_or_else(|| "backupRegionUpdate".to_string());
        let operation_path = document
            .as_ref()
            .map(|document| document.operation_path.as_str())
            .unwrap_or("mutation");
        let access_denied_location = root_field
            .map(|field| field.location)
            .unwrap_or(SourceLocation { line: 1, column: 1 });
        let access_denied_error =
            || backup_region_update_access_denied_error(&response_key, access_denied_location);
        let error_outcome = |error| {
            ResolverOutcome::value(Value::Null)
                .with_errors(root_field_errors_from_json(&[error], &response_key))
        };
        let country_code = match backup_region_update_country_code(root_field) {
            BackupRegionCountryCodeInput::ReadCurrent => None,
            BackupRegionCountryCodeInput::CountryCode(country_code) => {
                if !location_country_code_is_valid(&country_code) {
                    return error_outcome(backup_region_country_code_coercion_error(
                        &format!(
                            "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' has an invalid value ({country_code}). Expected type 'CountryCode!'."
                        ),
                        operation_path,
                        "argumentLiteralsIncompatible",
                        backup_region_update_region_value_location(query, root_field),
                    ));
                }
                Some(country_code.to_ascii_uppercase())
            }
            BackupRegionCountryCodeInput::Missing => {
                return error_outcome(backup_region_country_code_coercion_error(
                    "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' is required. Expected type CountryCode!",
                    operation_path,
                    "missingRequiredInputObjectAttribute",
                    backup_region_update_region_value_location(query, root_field),
                ));
            }
            BackupRegionCountryCodeInput::Invalid(value) => {
                return error_outcome(backup_region_country_code_coercion_error(
                    &format!(
                        "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' has an invalid value ({value}). Expected type 'CountryCode!'."
                    ),
                    operation_path,
                    "argumentLiteralsIncompatible",
                    backup_region_update_region_value_location(query, root_field),
                ));
            }
        };
        if self.backup_region_update_lacks_markets_access(request) {
            return error_outcome(access_denied_error());
        }
        let hydrate_current = Self::hydrate_current_backup_region_from_upstream;

        let region = match country_code.as_deref() {
            None => {
                if self.store.staged.backup_region.is_null()
                    && self.config.read_mode != ReadMode::Snapshot
                    && self.hydrate_access_denied(request, hydrate_current)
                {
                    return error_outcome(access_denied_error());
                }
                (!self.store.staged.backup_region.is_null())
                    .then(|| self.store.staged.backup_region.clone())
            }
            Some(code) => {
                let mut region = self
                    .backup_region_country_for_code(code)
                    .or_else(|| self.available_backup_region_for_code(code));
                if region.is_none() && self.config.read_mode != ReadMode::Snapshot {
                    let hydrate = self.hydrate_available_backup_regions_from_upstream(request);
                    if backup_region_response_is_access_denied(&hydrate.body) {
                        return error_outcome(access_denied_error());
                    }
                    region = self.available_backup_region_for_code(code);
                }
                if region.is_none() {
                    if self.store.staged.backup_region.is_null()
                        && self.config.read_mode != ReadMode::Snapshot
                        && self.hydrate_access_denied(request, hydrate_current)
                    {
                        return error_outcome(access_denied_error());
                    }
                    region = self.current_backup_region_for_code(code);
                }
                region
            }
        };
        match (country_code.as_deref(), region) {
            (None, region) => {
                let backup_region = region
                    .as_ref()
                    .map(|region| selected_backup_region_value(region, root_field))
                    .unwrap_or(Value::Null);
                ResolverOutcome::value(json!({
                    "backupRegion": backup_region,
                    "userErrors": []
                }))
            }
            (Some(_), Some(region)) => {
                self.store.staged.backup_region = region.clone();
                let staged_id = region
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("gid://shopify/MarketRegionCountry/local")
                    .to_string();
                ResolverOutcome::value(json!({
                    "backupRegion": selected_backup_region_value(&region, root_field),
                    "userErrors": []
                }))
                .with_log_draft(LogDraft::staged(
                    "backupRegionUpdate",
                    "admin-platform",
                    vec![staged_id],
                ))
            }
            (Some(_), None) => {
                let mut user_error = serde_json::Map::from_iter([
                    ("field".to_string(), json!(["region"])),
                    ("message".to_string(), json!("Region not found.")),
                    ("code".to_string(), json!("REGION_NOT_FOUND")),
                ]);
                let include_user_error_typename = root_field
                    .map(|field| nested_selected_fields(&field.selection, &["userErrors"]))
                    .unwrap_or_default()
                    .iter()
                    .any(|field| field.name == "__typename");
                if include_user_error_typename {
                    user_error.insert("__typename".to_string(), json!("MarketUserError"));
                }
                ResolverOutcome::value(json!({
                    "backupRegion": null,
                    "userErrors": [Value::Object(user_error)]
                }))
            }
        }
    }

    fn backup_region_update_lacks_markets_access(&mut self, request: &Request) -> bool {
        if let Some(token) = request_access_token(request) {
            if let Some(record) = self.store.staged.delegate_access_tokens.get(&token) {
                let scopes = string_array_field(record, "accessScopes");
                return !backup_region_scopes_include_markets(&scopes);
            }
        }
        if let Some(scopes) = self.cached_backup_region_access_scopes(request) {
            return !backup_region_scopes_include_markets(&scopes);
        }
        if self.config.read_mode == ReadMode::Snapshot {
            return false;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": BACKUP_REGION_ACCESS_SCOPES_QUERY,
                "operationName": "BackupRegionAccessScopes",
                "variables": {}
            }),
        );
        if backup_region_response_is_access_denied(&response.body) {
            return true;
        }
        let Some(scopes) = current_app_installation_access_scopes(&response.body) else {
            return false;
        };
        self.observe_current_app_installation_data(request, &response.body["data"]);
        self.mark_backup_region_access_scopes_cached(request);
        !backup_region_scopes_include_markets(&scopes)
    }

    fn cached_backup_region_access_scopes(&self, request: &Request) -> Option<Vec<String>> {
        if request_header(request, ACCESS_SCOPES_HEADER).is_some() {
            return Some(
                app_access_scope_handles(&current_app_installation_from_request(request))
                    .into_iter()
                    .collect(),
            );
        }
        let request_app_id = request_app_gid(request);
        let app_id = self.current_app_installation_app_id_for_request(&request_app_id)?;
        let installation = self.app_installation_for_app(&app_id)?;
        (installation
            .get(BACKUP_REGION_ACCESS_SCOPES_CACHE_FIELD)
            .and_then(Value::as_bool)
            == Some(true))
        .then(|| app_access_scope_handles(installation).into_iter().collect())
    }

    fn mark_backup_region_access_scopes_cached(&mut self, request: &Request) {
        let request_app_id = request_app_gid(request);
        let Some(app_id) = self.current_app_installation_app_id_for_request(&request_app_id) else {
            return;
        };
        let Some(Value::Object(installation)) = self.store.staged.installed_apps.get_mut(&app_id)
        else {
            return;
        };
        installation.insert(
            BACKUP_REGION_ACCESS_SCOPES_CACHE_FIELD.to_string(),
            json!(true),
        );
    }

    fn current_backup_region_for_code(&self, country_code: &str) -> Option<Value> {
        if !self.store.staged.backup_region.is_null() {
            return (self.store.staged.backup_region["code"].as_str() == Some(country_code))
                .then(|| self.store.staged.backup_region.clone());
        }
        let shop = self.store.effective_shop();
        let current_code = shop_country_code(&shop).map(str::to_ascii_uppercase)?;
        (current_code == country_code).then(|| backup_region_country_from_code(country_code))
    }

    fn hydrate_access_denied(
        &mut self,
        request: &Request,
        hydrate: fn(&mut Self, &Request) -> Response,
    ) -> bool {
        let hydrate = hydrate(self, request);
        backup_region_response_is_access_denied(&hydrate.body)
    }

    pub(in crate::proxy) fn hydrate_current_backup_region_from_upstream(
        &mut self,
        request: &Request,
    ) -> Response {
        let response = self.upstream_post(
            request,
            json!({
                "query": BACKUP_REGION_CURRENT_HYDRATE_QUERY,
                "operationName": "BackupRegionCurrentHydrate",
                "variables": {}
            }),
        );
        if response.status < 400 && !backup_region_response_is_access_denied(&response.body) {
            if let Some(region) = response.body["data"].get("backupRegion") {
                self.store.staged.backup_region = region.clone();
            }
        }
        response
    }
}

fn selected_backup_region_value(region: &Value, root_field: Option<&RootFieldSelection>) -> Value {
    let selection = root_field
        .and_then(|field| selected_child_selection(&field.selection, "backupRegion"))
        .unwrap_or_default();
    selected_json(region, &selection)
}

fn backup_region_country_from_code(country_code: &str) -> Value {
    let code = country_code.to_ascii_uppercase();
    let name = country_name_for_code(&code).unwrap_or(&code);
    json!({
        "__typename": "MarketRegionCountry",
        "id": shopify_gid("MarketRegionCountry", format_args!("local-{code}")),
        "name": name,
        "code": code
    })
}

fn backup_region_update_access_denied_error(response_key: &str, location: SourceLocation) -> Value {
    const REQUIRED_ACCESS: &str =
        "`read_markets` for queries and both `read_markets` as well as `write_markets` for mutations.";
    top_level_access_denied_error_envelope(
        format!("Access denied for backupRegionUpdate field. Required access: {REQUIRED_ACCESS}"),
        Some(location),
        vec![json!(response_key)],
        Some(REQUIRED_ACCESS),
    )
}

fn backup_region_scopes_include_markets(scopes: &[String]) -> bool {
    scopes.iter().any(|scope| scope == "read_markets")
        && scopes.iter().any(|scope| scope == "write_markets")
}

fn current_app_installation_access_scopes(body: &Value) -> Option<Vec<String>> {
    let scopes = body
        .get("data")?
        .get("currentAppInstallation")?
        .get("accessScopes")?
        .as_array()?;
    Some(
        scopes
            .iter()
            .filter_map(|scope| {
                scope
                    .get("handle")
                    .and_then(Value::as_str)
                    .or_else(|| scope.as_str())
                    .map(str::to_string)
            })
            .collect(),
    )
}

fn string_array_field(record: &Value, field: &str) -> Vec<String> {
    record
        .get(field)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn backup_region_response_is_access_denied(body: &Value) -> bool {
    body.get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|error| error["extensions"]["code"].as_str() == Some("ACCESS_DENIED"))
}

fn backup_region_update_region_value_location(
    query: &str,
    root_field: Option<&RootFieldSelection>,
) -> SourceLocation {
    let fallback = root_field
        .map(|field| field.location)
        .unwrap_or(SourceLocation { line: 1, column: 1 });
    let Some(field_offset) = byte_offset_for_location(query, fallback) else {
        return fallback;
    };
    let Some(after_field) = query.get(field_offset..) else {
        return fallback;
    };
    let Some(region_relative) = after_field.find("region") else {
        return fallback;
    };
    let region_offset = field_offset + region_relative;
    source_location_after_field_colon(query, region_offset, "region").unwrap_or(fallback)
}

fn source_location_after_field_colon(
    query: &str,
    field_offset: usize,
    field_name: &str,
) -> Option<SourceLocation> {
    let after_field_name = field_offset + field_name.len();
    let after_field = query.get(after_field_name..)?;
    let colon_relative = after_field.find(':')?;
    let mut value_offset = after_field_name + colon_relative + 1;
    while query
        .as_bytes()
        .get(value_offset)
        .is_some_and(u8::is_ascii_whitespace)
    {
        value_offset += 1;
    }
    source_location_for_byte_offset(query, value_offset)
}

fn backup_region_update_country_code(
    root_field: Option<&RootFieldSelection>,
) -> BackupRegionCountryCodeInput {
    let Some(field) = root_field else {
        return BackupRegionCountryCodeInput::ReadCurrent;
    };
    match field.raw_arguments.get("region") {
        None | Some(RawArgumentValue::Null) => BackupRegionCountryCodeInput::ReadCurrent,
        Some(RawArgumentValue::Variable { value, .. }) => {
            backup_region_update_variable_region_country_code(value.as_ref())
        }
        Some(RawArgumentValue::Object(region)) => backup_region_update_object_country_code(region),
        Some(value) => BackupRegionCountryCodeInput::Invalid(raw_argument_display(value)),
    }
}

fn backup_region_update_variable_region_country_code(
    value: Option<&ResolvedValue>,
) -> BackupRegionCountryCodeInput {
    match value {
        None | Some(ResolvedValue::Null) => BackupRegionCountryCodeInput::ReadCurrent,
        Some(ResolvedValue::Object(region)) => {
            backup_region_update_resolved_object_country_code(region)
        }
        Some(value) => BackupRegionCountryCodeInput::Invalid(resolved_value_display(value)),
    }
}

fn backup_region_update_object_country_code(
    region: &BTreeMap<String, RawArgumentValue>,
) -> BackupRegionCountryCodeInput {
    match region.get("countryCode") {
        None => BackupRegionCountryCodeInput::Missing,
        Some(RawArgumentValue::Enum(country_code)) => {
            BackupRegionCountryCodeInput::CountryCode(country_code.clone())
        }
        Some(RawArgumentValue::Variable { value, .. }) => {
            backup_region_update_variable_country_code(value.as_ref())
        }
        Some(value) => BackupRegionCountryCodeInput::Invalid(raw_argument_display(value)),
    }
}

fn backup_region_update_variable_country_code(
    value: Option<&ResolvedValue>,
) -> BackupRegionCountryCodeInput {
    match value {
        Some(ResolvedValue::String(country_code)) => {
            BackupRegionCountryCodeInput::CountryCode(country_code.clone())
        }
        Some(value) => BackupRegionCountryCodeInput::Invalid(resolved_value_display(value)),
        None => BackupRegionCountryCodeInput::Invalid("null".to_string()),
    }
}

fn backup_region_update_resolved_object_country_code(
    region: &BTreeMap<String, ResolvedValue>,
) -> BackupRegionCountryCodeInput {
    match region.get("countryCode") {
        None => BackupRegionCountryCodeInput::Missing,
        Some(ResolvedValue::String(country_code)) => {
            BackupRegionCountryCodeInput::CountryCode(country_code.clone())
        }
        Some(value) => BackupRegionCountryCodeInput::Invalid(resolved_value_display(value)),
    }
}

fn raw_argument_display(value: &RawArgumentValue) -> String {
    match value {
        RawArgumentValue::String(value) => json!(value).to_string(),
        RawArgumentValue::Int(value) => value.to_string(),
        RawArgumentValue::Float(value) => value.to_string(),
        RawArgumentValue::Bool(value) => value.to_string(),
        RawArgumentValue::Null => "null".to_string(),
        RawArgumentValue::Enum(value) => value.clone(),
        RawArgumentValue::List(values) => {
            let values = values.iter().map(raw_argument_json).collect::<Vec<_>>();
            Value::Array(values).to_string()
        }
        RawArgumentValue::Object(fields) => {
            let fields = fields
                .iter()
                .map(|(key, value)| (key.clone(), raw_argument_json(value)))
                .collect();
            Value::Object(fields).to_string()
        }
        RawArgumentValue::Variable { value, .. } => value
            .as_ref()
            .map(resolved_value_display)
            .unwrap_or_else(|| "null".to_string()),
    }
}

fn raw_argument_json(value: &RawArgumentValue) -> Value {
    match value {
        RawArgumentValue::String(value) | RawArgumentValue::Enum(value) => json!(value),
        RawArgumentValue::Int(value) => json!(value),
        RawArgumentValue::Float(value) => json!(value),
        RawArgumentValue::Bool(value) => json!(value),
        RawArgumentValue::Null => Value::Null,
        RawArgumentValue::List(values) => {
            Value::Array(values.iter().map(raw_argument_json).collect())
        }
        RawArgumentValue::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), raw_argument_json(value)))
                .collect(),
        ),
        RawArgumentValue::Variable { value, .. } => value
            .as_ref()
            .map(resolved_value_json)
            .unwrap_or(Value::Null),
    }
}

fn resolved_value_display(value: &ResolvedValue) -> String {
    resolved_values::resolved_value_json(value).to_string()
}
