//// Narrow Products smoke foundation for the BEAM embedder wrapper.
////
//// This is intentionally smaller than the TypeScript products domain. It
//// supports the productCreate -> product(id:) read-after-write lifecycle that
//// the Elixir wrapper smoke needs, without claiming the broader products
//// endpoint family is ported.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, SrcBool, SrcInt, SrcList, SrcNull, SrcString,
  get_document_fragments, get_field_response_key, project_graphql_value,
  src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{read_optional_string}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type ProductRecord, type ProductVariantRecord, ProductRecord,
  ProductVariantRecord,
}

pub type ProductsError {
  ParseFailed(root_field.RootFieldError)
}

pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
  )
}

type UserError {
  UserError(field: Option(List(String)), message: String)
}

pub fn is_product_query_root(name: String) -> Bool {
  name == "product"
}

pub fn is_product_mutation_root(name: String) -> Bool {
  name == "productCreate"
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, ProductsError) {
  use data <- result.try(handle_product_query(store, document, variables))
  Ok(wrap_data(data))
}

pub fn handle_product_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, ProductsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(
        json.object(
          list.map(fields, fn(field) {
            #(
              get_field_response_key(field),
              root_payload(store, field, fragments, variables),
            )
          }),
        ),
      )
    }
  }
}

fn root_payload(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(
      name: name,
      selection_set: Some(SelectionSet(selections: selections, ..)),
      ..,
    ) ->
      case name.value {
        "product" -> {
          let args = case root_field.get_field_arguments(field, variables) {
            Ok(d) -> d
            Error(_) -> dict.new()
          }
          case read_optional_string(args, "id") {
            Some(id) ->
              case store.get_effective_product_by_id(store, id) {
                Some(product) ->
                  project_graphql_value(
                    product_source(store, product),
                    selections,
                    fragments,
                  )
                None -> json.null()
              }
            None -> json.null()
          }
        }
        _ -> json.null()
      }
    _ -> json.null()
  }
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, ProductsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let initial =
        MutationOutcome(
          data: json.object([]),
          store: store,
          identity: identity,
          staged_resource_ids: [],
        )
      let #(entries, outcome) =
        list.fold(fields, #([], initial), fn(acc, field) {
          let #(pairs, current) = acc
          case field {
            Field(name: name, ..) ->
              case name.value {
                "productCreate" -> {
                  let #(key, payload, next) =
                    handle_create(
                      current.store,
                      current.identity,
                      request_path,
                      document,
                      field,
                      fragments,
                      variables,
                    )
                  #(list.append(pairs, [#(key, payload)]), next)
                }
                _ -> #(pairs, current)
              }
            _ -> #(pairs, current)
          }
        })
      Ok(MutationOutcome(..outcome, data: wrap_data(json.object(entries))))
    }
  }
}

fn handle_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let args = case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
  let input = read_product_input(args)
  let errors = validate_product_create_input(input)
  let #(record_opt, store_after, identity_after, staged_ids) = case
    input,
    errors
  {
    Some(input_dict), [] -> {
      let #(product, variant, identity_after) =
        make_product(identity, input_dict)
      let #(_, store_after) = store.stage_product(store, product, variant)
      #(Some(product), store_after, identity_after, [product.id, variant.id])
    }
    _, _ -> #(None, store, identity, [])
  }
  let payload =
    project_create_payload(record_opt, errors, field, fragments, store_after)
  let #(log_id, identity_after_log) =
    synthetic_identity.make_synthetic_gid(identity_after, "MutationLogEntry")
  let #(received_at, identity_final) =
    synthetic_identity.make_synthetic_timestamp(identity_after_log)
  let status = case errors {
    [] -> store.Staged
    _ -> store.Failed
  }
  let entry =
    build_log_entry(
      log_id,
      received_at,
      request_path,
      document,
      staged_ids,
      status,
    )
  let store_logged = store.record_mutation_log_entry(store_after, entry)
  #(
    key,
    payload,
    MutationOutcome(
      data: json.object([]),
      store: store_logged,
      identity: identity_final,
      staged_resource_ids: staged_ids,
    ),
  )
}

fn read_product_input(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(args, "product") {
    Ok(root_field.ObjectVal(fields)) -> Some(fields)
    _ -> None
  }
}

fn validate_product_create_input(
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> List(UserError) {
  case input {
    None -> [
      UserError(field: Some(["product"]), message: "Product input is required"),
    ]
    Some(fields) ->
      case read_optional_string(fields, "title") {
        Some(title) ->
          case string.trim(title) {
            "" -> [title_blank_error()]
            _ -> []
          }
        None -> [title_blank_error()]
      }
  }
}

fn title_blank_error() -> UserError {
  UserError(field: Some(["title"]), message: "Title can't be blank")
}

fn make_product(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
) -> #(ProductRecord, ProductVariantRecord, SyntheticIdentityRegistry) {
  let #(product_id, identity) =
    synthetic_identity.make_synthetic_gid(identity, "Product")
  let #(variant_id, identity) =
    synthetic_identity.make_synthetic_gid(identity, "ProductVariant")
  let #(inventory_item_id, identity) =
    synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
  let #(created_at, identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let title = read_optional_string(input, "title") |> option.unwrap("")
  let handle = case read_optional_string(input, "handle") {
    Some(h) -> h
    None -> handle_from_title(title)
  }
  let status = case read_optional_string(input, "status") {
    Some(s) -> s
    None -> "ACTIVE"
  }
  let product =
    ProductRecord(
      id: product_id,
      legacy_resource_id: legacy_resource_id(product_id),
      title: title,
      handle: handle,
      status: status,
      created_at: created_at,
      updated_at: created_at,
      default_variant_id: variant_id,
    )
  let variant =
    ProductVariantRecord(
      id: variant_id,
      legacy_resource_id: legacy_resource_id(variant_id),
      product_id: product_id,
      title: "Default Title",
      inventory_quantity: 0,
      inventory_item_id: inventory_item_id,
    )
  #(product, variant, identity)
}

fn project_create_payload(
  record: Option(ProductRecord),
  errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
  store: Store,
) -> Json {
  let product = case record {
    Some(p) -> product_source(store, p)
    None -> SrcNull
  }
  let payload =
    src_object([
      #("product", product),
      #("userErrors", SrcList(list.map(errors, user_error_to_source))),
    ])
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(payload, selections, fragments)
    _ -> json.object([])
  }
}

fn product_source(
  store: Store,
  product: ProductRecord,
) -> graphql_helpers.SourceValue {
  let variant =
    store.get_effective_product_variant_by_id(store, product.default_variant_id)
  let variants = case variant {
    Some(v) -> [variant_source(v)]
    None -> []
  }
  src_object([
    #("__typename", SrcString("Product")),
    #("id", SrcString(product.id)),
    #("legacyResourceId", SrcString(product.legacy_resource_id)),
    #("title", SrcString(product.title)),
    #("handle", SrcString(product.handle)),
    #("status", SrcString(product.status)),
    #("createdAt", SrcString(product.created_at)),
    #("updatedAt", SrcString(product.updated_at)),
    #("totalInventory", SrcInt(0)),
    #("tracksInventory", SrcBool(False)),
    #("variants", connection_source(variants)),
  ])
}

fn variant_source(
  variant: ProductVariantRecord,
) -> graphql_helpers.SourceValue {
  src_object([
    #("__typename", SrcString("ProductVariant")),
    #("id", SrcString(variant.id)),
    #("legacyResourceId", SrcString(variant.legacy_resource_id)),
    #("productId", SrcString(variant.product_id)),
    #("title", SrcString(variant.title)),
    #("inventoryQuantity", SrcInt(variant.inventory_quantity)),
    #(
      "inventoryItem",
      src_object([
        #("__typename", SrcString("InventoryItem")),
        #("id", SrcString(variant.inventory_item_id)),
        #("tracked", SrcBool(False)),
        #("requiresShipping", SrcBool(True)),
      ]),
    ),
  ])
}

fn connection_source(
  nodes: List(graphql_helpers.SourceValue),
) -> graphql_helpers.SourceValue {
  let edges =
    list.index_map(nodes, fn(node, index) {
      let cursor = "cursor:" <> int.to_string(index)
      src_object([#("cursor", SrcString(cursor)), #("node", node)])
    })
  let start_cursor = case nodes {
    [] -> SrcNull
    _ -> SrcString("cursor:0")
  }
  let end_cursor = case list.length(nodes) {
    0 -> SrcNull
    n -> SrcString("cursor:" <> int.to_string(n - 1))
  }
  src_object([
    #("nodes", SrcList(nodes)),
    #("edges", SrcList(edges)),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", start_cursor),
        #("endCursor", end_cursor),
      ]),
    ),
  ])
}

fn user_error_to_source(error: UserError) -> graphql_helpers.SourceValue {
  let field_value = case error.field {
    Some(parts) -> SrcList(list.map(parts, fn(part) { SrcString(part) }))
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", field_value),
    #("message", SrcString(error.message)),
  ])
}

fn build_log_entry(
  log_id: String,
  received_at: String,
  request_path: String,
  document: String,
  staged_ids: List(String),
  status: store.EntryStatus,
) -> store.MutationLogEntry {
  store.MutationLogEntry(
    id: log_id,
    received_at: received_at,
    operation_name: None,
    path: request_path,
    query: document,
    variables: dict.new(),
    staged_resource_ids: staged_ids,
    status: status,
    interpreted: store.InterpretedMetadata(
      operation_type: store.Mutation,
      operation_name: None,
      root_fields: ["productCreate"],
      primary_root_field: Some("productCreate"),
      capability: store.Capability(
        operation_name: Some("productCreate"),
        domain: "products",
        execution: "stage-locally",
      ),
    ),
    notes: Some("Locally staged productCreate in shopify-draft-proxy."),
  )
}

fn legacy_resource_id(id: String) -> String {
  let without_query = case string.split(id, "?") {
    [head, ..] -> head
    [] -> id
  }
  case list.last(string.split(without_query, "/")) {
    Ok(part) -> part
    Error(_) -> id
  }
}

fn handle_from_title(title: String) -> String {
  let lowered = string.lowercase(string.trim(title))
  lowered
  |> string.replace(" ", "-")
  |> string.replace("_", "-")
}

pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}
