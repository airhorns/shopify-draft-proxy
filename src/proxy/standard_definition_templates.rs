use super::*;
use serde::Deserialize;
use std::sync::OnceLock;

const CAPTURED_SHOP_DOMAIN: &str = "harry-test-heelo.myshopify.com";
pub(in crate::proxy) const STANDARD_TEMPLATE_MARKER_FIELD: &str =
    "__shopifyDraftProxyStandardTemplateId";

const METAFIELD_CATALOG_2025_01: &str = include_str!(
    "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/standard-definition-template-resolution.json"
);
const METAFIELD_CATALOG_2026_04: &str = include_str!(
    "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metafields/standard-definition-template-resolution.json"
);
const METAOBJECT_CATALOG_2025_01: &str = include_str!(
    "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metaobjects/standard-metaobject-templates.json"
);
const METAOBJECT_CATALOG_2026_04: &str = include_str!(
    "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/standard-metaobject-templates.json"
);
const METAFIELD_ENABLEMENT_ENRICHMENT: &str =
    include_str!("standard_metafield_definition_templates.json");

const STANDARD_METAFIELD_TEMPLATE_BY_ID_QUERY: &str = r#"
query DraftProxyStandardMetafieldTemplateById($id: ID!) {
  node(id: $id) {
    __typename
    ... on StandardMetafieldDefinitionTemplate {
      id
      namespace
      key
      name
      description
      ownerTypes
      type { name category }
      validations { name value }
      visibleToStorefrontApi
    }
  }
}
"#;

#[derive(Clone, Debug)]
pub(in crate::proxy) struct StandardMetafieldDefinitionTemplate {
    pub id: String,
    pub namespace: String,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_types: Vec<String>,
    pub metafield_type: String,
    pub validations: Vec<StandardMetafieldDefinitionValidation>,
    pub derived_validations: Vec<StandardMetafieldDefinitionValidation>,
    pub constraints: Option<StandardMetafieldDefinitionTemplateConstraints>,
    pub catalog_value: Value,
}

#[derive(Clone, Debug, Deserialize)]
pub(in crate::proxy) struct StandardMetafieldDefinitionValidation {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(in crate::proxy) struct StandardMetafieldDefinitionTemplateConstraints {
    pub key: String,
    pub values: Vec<String>,
}

#[derive(Clone, Debug)]
struct StandardMetafieldCatalogEntry {
    cursor: String,
    template: StandardMetafieldDefinitionTemplate,
    available: bool,
    constrained: bool,
    enablement_proven: bool,
}

#[derive(Debug)]
struct StandardDefinitionCatalog {
    store_domain: String,
    api_version: String,
    metafield_templates: Vec<StandardMetafieldCatalogEntry>,
    constraint_subtypes: BTreeMap<(String, String), BTreeSet<String>>,
    metaobject_templates: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapturedMetafieldCatalog {
    store_domain: String,
    api_version: String,
    catalogs: CapturedMetafieldCatalogs,
    captures: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapturedMetafieldCatalogs {
    all: CapturedCatalogSlice,
    available: CapturedCatalogSlice,
    constrained: CapturedCatalogSlice,
    proven_constraint_subtypes: Vec<CapturedConstraintSubtype>,
}

#[derive(Deserialize)]
struct CapturedCatalogSlice {
    edges: Vec<CapturedCatalogEdge>,
}

#[derive(Deserialize)]
struct CapturedCatalogEdge {
    cursor: String,
    node: Value,
}

#[derive(Deserialize)]
struct CapturedConstraintSubtype {
    key: String,
    value: String,
    edges: Vec<CapturedCatalogEdge>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapturedMetaobjectCatalog {
    store_domain: String,
    api_version: String,
    templates: Vec<Value>,
}

#[derive(Deserialize)]
struct EnablementEnrichmentCatalog {
    templates: Vec<EnablementEnrichment>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnablementEnrichment {
    id: String,
    #[serde(default)]
    derived_validations: Vec<StandardMetafieldDefinitionValidation>,
    constraints: Option<StandardMetafieldDefinitionTemplateConstraints>,
    standard_metaobject_definition_type: Option<String>,
}

static CATALOG_2025_01: OnceLock<StandardDefinitionCatalog> = OnceLock::new();
static CATALOG_2026_04: OnceLock<StandardDefinitionCatalog> = OnceLock::new();
static ENABLEMENT_ENRICHMENTS: OnceLock<BTreeMap<String, EnablementEnrichment>> = OnceLock::new();

fn enablement_enrichments() -> &'static BTreeMap<String, EnablementEnrichment> {
    ENABLEMENT_ENRICHMENTS.get_or_init(|| {
        let catalog: EnablementEnrichmentCatalog =
            serde_json::from_str(METAFIELD_ENABLEMENT_ENRICHMENT)
                .expect("standard metafield enablement enrichment must be valid JSON");
        catalog
            .templates
            .into_iter()
            .map(|template| (template.id.clone(), template))
            .collect()
    })
}

fn catalog_for_version(api_version: &str) -> Option<&'static StandardDefinitionCatalog> {
    match api_version {
        "2025-01" => Some(CATALOG_2025_01.get_or_init(|| {
            build_standard_definition_catalog(METAFIELD_CATALOG_2025_01, METAOBJECT_CATALOG_2025_01)
        })),
        "2026-04" => Some(CATALOG_2026_04.get_or_init(|| {
            build_standard_definition_catalog(METAFIELD_CATALOG_2026_04, METAOBJECT_CATALOG_2026_04)
        })),
        _ => None,
    }
}

fn build_standard_definition_catalog(
    metafield_fixture: &str,
    metaobject_fixture: &str,
) -> StandardDefinitionCatalog {
    let metafield: CapturedMetafieldCatalog = serde_json::from_str(metafield_fixture)
        .expect("captured standard metafield catalog must be valid JSON");
    let metaobject: CapturedMetaobjectCatalog = serde_json::from_str(metaobject_fixture)
        .expect("captured standard metaobject catalog must be valid JSON");
    assert_eq!(metafield.store_domain, metaobject.store_domain);
    assert_eq!(metafield.api_version, metaobject.api_version);

    let available = captured_ids(&metafield.catalogs.available.edges);
    let constrained = captured_ids(&metafield.catalogs.constrained.edges);
    let constraint_subtypes = metafield
        .catalogs
        .proven_constraint_subtypes
        .into_iter()
        .map(|subtype| ((subtype.key, subtype.value), captured_ids(&subtype.edges)))
        .collect::<BTreeMap<_, _>>();
    let metaobject_types = metaobject
        .templates
        .iter()
        .filter_map(|template| template.get("type").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let enrichments = enablement_enrichments();
    let captured_enablements =
        captured_enablement_enrichments(&metafield.captures, &metaobject_types);
    let use_legacy_enrichment = metafield.api_version == "2025-01";
    let metafield_templates = metafield
        .catalogs
        .all
        .edges
        .into_iter()
        .map(|edge| {
            let id = edge.node["id"].as_str().unwrap_or_default().to_string();
            let enrichment = captured_enablements.get(&id).or_else(|| {
                use_legacy_enrichment
                    .then(|| enrichments.get(&id))
                    .flatten()
            });
            let enrichment_is_supported = enrichment
                .and_then(|item| item.standard_metaobject_definition_type.as_deref())
                .is_none_or(|meta_type| metaobject_types.contains(meta_type));
            let supported_enrichment = enrichment.filter(|_| enrichment_is_supported);
            StandardMetafieldCatalogEntry {
                cursor: edge.cursor,
                available: available.contains(&id),
                constrained: constrained.contains(&id),
                enablement_proven: !constrained.contains(&id)
                    || supported_enrichment
                        .and_then(|item| item.constraints.as_ref())
                        .is_some(),
                template: template_from_catalog_value(edge.node, supported_enrichment),
            }
        })
        .collect();

    StandardDefinitionCatalog {
        store_domain: metafield.store_domain,
        api_version: metafield.api_version,
        metafield_templates,
        constraint_subtypes,
        metaobject_templates: metaobject.templates,
    }
}

fn captured_enablement_enrichments(
    captures: &Value,
    metaobject_types: &BTreeSet<&str>,
) -> BTreeMap<String, EnablementEnrichment> {
    [
        ("materialEnable", "materialRead"),
        ("colorPatternEnable", "colorPatternRead"),
    ]
    .into_iter()
    .filter_map(|(enable_capture, read_capture)| {
        captured_enablement_enrichment(captures, metaobject_types, enable_capture, read_capture)
    })
    .collect()
}

fn captured_enablement_enrichment(
    captures: &Value,
    metaobject_types: &BTreeSet<&str>,
    enable_capture: &str,
    read_capture: &str,
) -> Option<(String, EnablementEnrichment)> {
    let definition = captures
        .pointer(&format!(
            "/{enable_capture}/response/data/standardMetafieldDefinitionEnable/createdDefinition"
        ))
        .filter(|definition| definition.is_object())?;
    let template_id = captures
        .pointer(&format!(
            "/{read_capture}/response/data/metafieldDefinition/standardTemplate/id"
        ))
        .and_then(Value::as_str)?;
    let key = definition
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let standard_metaobject_definition_type = format!("shopify--{key}");
    let derived_validations = definition["validations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|validation| {
            let name = validation.get("name")?.as_str()?.to_string();
            let value = if name == "metaobject_definition_id"
                && metaobject_types.contains(standard_metaobject_definition_type.as_str())
            {
                format!(
                    "gid://shopify/MetaobjectDefinition/standard-{key}?shopify-draft-proxy=synthetic"
                )
            } else {
                validation.get("value")?.as_str()?.to_string()
            };
            Some(StandardMetafieldDefinitionValidation { name, value })
        })
        .collect();
    let constraints = definition.get("constraints").and_then(|constraints| {
        Some(StandardMetafieldDefinitionTemplateConstraints {
            key: constraints.get("key")?.as_str()?.to_string(),
            values: constraints
                .pointer("/values/nodes")?
                .as_array()?
                .iter()
                .filter_map(|node| node.get("value").and_then(Value::as_str))
                .map(str::to_string)
                .collect(),
        })
    });
    Some((
        template_id.to_string(),
        EnablementEnrichment {
            id: template_id.to_string(),
            derived_validations,
            constraints,
            standard_metaobject_definition_type: Some(standard_metaobject_definition_type),
        },
    ))
}

fn captured_ids(edges: &[CapturedCatalogEdge]) -> BTreeSet<String> {
    edges
        .iter()
        .filter_map(|edge| edge.node.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn template_from_catalog_value(
    catalog_value: Value,
    enrichment: Option<&EnablementEnrichment>,
) -> StandardMetafieldDefinitionTemplate {
    let validations = catalog_value["validations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|validation| {
            Some(StandardMetafieldDefinitionValidation {
                name: validation.get("name")?.as_str()?.to_string(),
                value: validation.get("value")?.as_str()?.to_string(),
            })
        })
        .collect();
    StandardMetafieldDefinitionTemplate {
        id: catalog_value["id"].as_str().unwrap_or_default().to_string(),
        namespace: catalog_value["namespace"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        key: catalog_value["key"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        name: catalog_value["name"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        description: catalog_value["description"].as_str().map(str::to_string),
        owner_types: catalog_value["ownerTypes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        metafield_type: catalog_value["type"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        validations,
        derived_validations: enrichment
            .map(|item| item.derived_validations.clone())
            .unwrap_or_default(),
        constraints: enrichment.and_then(|item| item.constraints.clone()),
        catalog_value,
    }
}

fn configured_shop_domain(admin_origin: &str) -> Option<String> {
    url::Url::parse(admin_origin)
        .ok()?
        .host_str()
        .map(str::to_string)
}

impl DraftProxy {
    fn standard_definition_catalog(
        &self,
        api_version: &str,
    ) -> Option<&'static StandardDefinitionCatalog> {
        let catalog = catalog_for_version(api_version)?;
        let shop_domain = configured_shop_domain(&self.config.shopify_admin_origin)?;
        (shop_domain == CAPTURED_SHOP_DOMAIN
            && catalog.store_domain == shop_domain
            && catalog.api_version == api_version)
            .then_some(catalog)
    }

    pub(crate) fn standard_metafield_definition_templates_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode != ReadMode::Snapshot {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        if arguments.contains_key("last") && !arguments.contains_key("before") {
            return graphql_error_outcome(
                vec![json!({
                    "message": "using last without before is not supported",
                    "extensions": {"code": "BAD_REQUEST"},
                    "path": [invocation.response_key]
                })],
                invocation.response_key,
            );
        }
        let Some(catalog) = self.standard_definition_catalog(invocation.api_version.as_str())
        else {
            return ResolverOutcome::value(connection_json_with_cursor(
                Vec::new(),
                |_, _| String::new(),
                empty_page_info(),
            ));
        };

        let constraint_status = resolved_string_field(&arguments, "constraintStatus")
            .unwrap_or_else(|| "CONSTRAINED_AND_UNCONSTRAINED".to_string());
        let constraint_subtype =
            resolved_object_field(&arguments, "constraintSubtype").and_then(|subtype| {
                Some((
                    resolved_string_field(&subtype, "key")?,
                    resolved_string_field(&subtype, "value")?,
                ))
            });
        let subtype_ids = constraint_subtype
            .as_ref()
            .and_then(|subtype| catalog.constraint_subtypes.get(subtype));
        let exclude_activated =
            resolved_bool_field(&arguments, "excludeActivated").unwrap_or(false);
        let mut entries = catalog
            .metafield_templates
            .iter()
            .filter(|entry| match constraint_status.as_str() {
                "CONSTRAINED_ONLY" => entry.constrained,
                "UNCONSTRAINED_ONLY" => !entry.constrained,
                _ => true,
            })
            .filter(|entry| {
                constraint_subtype.is_none()
                    || subtype_ids.is_some_and(|ids| ids.contains(&entry.template.id))
            })
            .filter(|entry| {
                !exclude_activated
                    || (entry.available
                        && !self.standard_metafield_template_is_staged(&entry.template.id))
            })
            .collect::<Vec<_>>();
        if resolved_bool_field(&arguments, "reverse").unwrap_or(false) {
            entries.reverse();
        }
        let (entries, page_info) =
            connection_window(&entries, &arguments, |entry| entry.cursor.clone());
        let nodes = entries
            .iter()
            .map(|entry| entry.template.catalog_value.clone())
            .collect::<Vec<_>>();
        let edges = entries
            .iter()
            .zip(nodes.iter())
            .map(|(entry, node)| json!({"cursor": entry.cursor, "node": node}))
            .collect::<Vec<_>>();
        ResolverOutcome::value(json!({
            "nodes": nodes,
            "edges": edges,
            "pageInfo": page_info
        }))
    }

    pub(in crate::proxy) fn resolved_standard_metafield_template(
        &mut self,
        request: &Request,
        api_version: &str,
        id: Option<&str>,
        namespace: Option<&str>,
        key: Option<&str>,
    ) -> StandardMetafieldTemplateResolution {
        if id.is_none() && (namespace.is_none() || key.is_none()) {
            return StandardMetafieldTemplateResolution::MissingSelector;
        }
        if self.config.read_mode == ReadMode::Snapshot {
            let Some(catalog) = self.standard_definition_catalog(api_version) else {
                return StandardMetafieldTemplateResolution::ContextUnavailable;
            };
            let template = catalog.metafield_templates.iter().find(|entry| {
                id.map_or_else(
                    || {
                        Some(entry.template.namespace.as_str()) == namespace
                            && Some(entry.template.key.as_str()) == key
                    },
                    |id| entry.template.id == id,
                )
            });
            return match template {
                Some(entry) if entry.enablement_proven => {
                    StandardMetafieldTemplateResolution::Found(Box::new(entry.template.clone()))
                }
                Some(_) => StandardMetafieldTemplateResolution::EnablementUnavailable,
                None => StandardMetafieldTemplateResolution::NotFound {
                    selected_by_id: id.is_some(),
                },
            };
        }
        if id.is_none() {
            return StandardMetafieldTemplateResolution::ContextUnavailable;
        }

        let id = id.expect("checked above");
        if let Some(cached) = self
            .execution_session
            .standard_metafield_template_hydration
            .get(id)
            .cloned()
        {
            return cached
                .map(|template| StandardMetafieldTemplateResolution::Found(Box::new(template)))
                .unwrap_or(StandardMetafieldTemplateResolution::NotFound {
                    selected_by_id: true,
                });
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": STANDARD_METAFIELD_TEMPLATE_BY_ID_QUERY,
                "operationName": "DraftProxyStandardMetafieldTemplateById",
                "variables": {"id": id}
            }),
        );
        if !(200..300).contains(&response.status) || response.body.get("errors").is_some() {
            return StandardMetafieldTemplateResolution::ContextUnavailable;
        }
        let Some(node) = response.body.pointer("/data/node") else {
            return StandardMetafieldTemplateResolution::ContextUnavailable;
        };
        let template = if node.is_null() {
            None
        } else if node.get("__typename").and_then(Value::as_str)
            == Some("StandardMetafieldDefinitionTemplate")
        {
            Some(template_from_catalog_value(node.clone(), None))
        } else if node.get("__typename").and_then(Value::as_str).is_some() {
            None
        } else {
            return StandardMetafieldTemplateResolution::ContextUnavailable;
        };
        self.execution_session
            .standard_metafield_template_hydration
            .insert(id.to_string(), template.clone());
        template
            .map(|template| StandardMetafieldTemplateResolution::Found(Box::new(template)))
            .unwrap_or(StandardMetafieldTemplateResolution::NotFound {
                selected_by_id: true,
            })
    }

    pub(in crate::proxy) fn resolved_standard_metaobject_template(
        &self,
        api_version: &str,
        meta_type: &str,
    ) -> StandardMetaobjectTemplateResolution {
        if self.config.read_mode != ReadMode::Snapshot {
            return StandardMetaobjectTemplateResolution::ContextUnavailable;
        }
        let Some(catalog) = self.standard_definition_catalog(api_version) else {
            return StandardMetaobjectTemplateResolution::ContextUnavailable;
        };
        catalog
            .metaobject_templates
            .iter()
            .find(|template| template.get("type").and_then(Value::as_str) == Some(meta_type))
            .cloned()
            .map(StandardMetaobjectTemplateResolution::Found)
            .unwrap_or(StandardMetaobjectTemplateResolution::NotFound)
    }

    fn standard_metafield_template_is_staged(&self, template_id: &str) -> bool {
        self.store
            .staged
            .metafield_definitions
            .values()
            .any(|definition| {
                definition
                    .get(STANDARD_TEMPLATE_MARKER_FIELD)
                    .and_then(Value::as_str)
                    == Some(template_id)
            })
    }
}

pub(in crate::proxy) enum StandardMetafieldTemplateResolution {
    Found(Box<StandardMetafieldDefinitionTemplate>),
    MissingSelector,
    NotFound { selected_by_id: bool },
    EnablementUnavailable,
    ContextUnavailable,
}

pub(in crate::proxy) enum StandardMetaobjectTemplateResolution {
    Found(Value),
    NotFound,
    ContextUnavailable,
}
