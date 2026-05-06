//// Internal products-domain implementation split from proxy/products.gleam.

import gleam/bit_array
import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Definition, type Location, type ObjectField, type Selection,
  type VariableDefinition, Argument, Directive, Field, InlineFragment, NullValue,
  ObjectField, ObjectValue, OperationDefinition, SelectionSet, StringValue,
  VariableDefinition, VariableValue,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/parser
import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, type RootFieldError, BoolVal, FloatVal, IntVal, ListVal,
  NullVal, ObjectVal, StringVal, get_field_arguments, get_root_fields,
}
import shopify_draft_proxy/graphql/source as graphql_source
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  paginate_connection_items, project_graphql_field_value, project_graphql_value,
  serialize_connection, serialize_empty_connection, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, RequiredArgument,
  build_null_argument_error, find_argument, single_root_log_draft,
  validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/products/inventory_l05.{
  sync_product_set_inventory_summary,
}
import shopify_draft_proxy/proxy/products/products_l00.{make_product_preview_url}
import shopify_draft_proxy/proxy/products/products_l04.{
  duplicated_product_record,
}
import shopify_draft_proxy/proxy/products/products_l05.{
  created_product_record, updated_product_record,
}
import shopify_draft_proxy/proxy/products/products_l07.{apply_product_set_graph}
import shopify_draft_proxy/proxy/products/products_l13.{
  product_duplicate_payload, product_set_payload,
}
import shopify_draft_proxy/proxy/products/publications_l02.{
  duplicate_product_relationships,
}
import shopify_draft_proxy/proxy/products/shared_l01.{mutation_result}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, MutationFieldResult,
} as product_types
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type ChannelRecord, type CollectionImageRecord,
  type CollectionRecord, type CollectionRuleRecord, type CollectionRuleSetRecord,
  type InventoryItemRecord, type InventoryLevelRecord,
  type InventoryLocationRecord, type InventoryMeasurementRecord,
  type InventoryQuantityRecord, type InventoryShipmentLineItemRecord,
  type InventoryShipmentRecord, type InventoryShipmentTrackingRecord,
  type InventoryTransferLineItemRecord,
  type InventoryTransferLocationSnapshotRecord, type InventoryTransferRecord,
  type InventoryWeightRecord, type InventoryWeightValue, type LocationRecord,
  type ProductCategoryRecord, type ProductCollectionRecord,
  type ProductFeedRecord, type ProductMediaRecord, type ProductMetafieldRecord,
  type ProductOperationRecord, type ProductOperationUserErrorRecord,
  type ProductOptionRecord, type ProductOptionValueRecord, type ProductRecord,
  type ProductResourceFeedbackRecord, type ProductSeoRecord,
  type ProductVariantRecord, type ProductVariantSelectedOptionRecord,
  type PublicationRecord, type SellingPlanGroupRecord, type SellingPlanRecord,
  type ShopResourceFeedbackRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString, CollectionRecord,
  CollectionRuleRecord, CollectionRuleSetRecord, InventoryItemRecord,
  InventoryLevelRecord, InventoryLocationRecord, InventoryMeasurementRecord,
  InventoryQuantityRecord, InventoryShipmentLineItemRecord,
  InventoryShipmentRecord, InventoryShipmentTrackingRecord,
  InventoryTransferLineItemRecord, InventoryTransferLocationSnapshotRecord,
  InventoryTransferRecord, InventoryWeightFloat, InventoryWeightInt,
  InventoryWeightRecord, LocationRecord, ProductCollectionRecord,
  ProductFeedRecord, ProductMediaRecord, ProductMetafieldRecord,
  ProductOperationRecord, ProductOperationUserErrorRecord, ProductOptionRecord,
  ProductOptionValueRecord, ProductRecord, ProductResourceFeedbackRecord,
  ProductSeoRecord, ProductVariantRecord, ProductVariantSelectedOptionRecord,
  PublicationRecord, SellingPlanGroupRecord, SellingPlanRecord,
  ShopResourceFeedbackRecord,
}

@internal
pub fn stage_product_set(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  existing: Option(ProductRecord),
  input: Dict(String, ResolvedValue),
  shopify_admin_origin: String,
  synchronous: Bool,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> MutationFieldResult {
  let #(product, identity_after_product) = case existing {
    Some(product) -> updated_product_record(identity, product, input)
    None -> {
      let #(created, next_identity) =
        created_product_record(store, identity, shopify_admin_origin, input)
      #(
        ProductRecord(
          ..created,
          online_store_preview_url: Some(make_product_preview_url(created)),
        ),
        next_identity,
      )
    }
  }
  let #(_, store_after_product) = store.upsert_staged_product(store, product)
  let #(store_after_graph, identity_after_graph, staged_ids) =
    apply_product_set_graph(
      store_after_product,
      identity_after_product,
      existing,
      product.id,
      input,
    )
  let #(synced_product, next_store, identity_after_summary) =
    sync_product_set_inventory_summary(
      store_after_graph,
      identity_after_graph,
      product.id,
      existing,
    )
  let product = synced_product |> option.unwrap(product)
  case synchronous {
    True ->
      mutation_result(
        key,
        product_set_payload(
          next_store,
          Some(product),
          None,
          [],
          field,
          variables,
          fragments,
        ),
        next_store,
        identity_after_summary,
        [product.id, ..staged_ids],
      )
    False -> {
      let #(operation_id, identity_after_operation) =
        synthetic_identity.make_synthetic_gid(
          identity_after_summary,
          "ProductSetOperation",
        )
      let operation =
        ProductOperationRecord(
          id: operation_id,
          type_name: "ProductSetOperation",
          product_id: Some(product.id),
          new_product_id: None,
          status: "COMPLETE",
          user_errors: [],
        )
      let #(staged_operation, store_after_operation) =
        store.stage_product_operation(next_store, operation)
      let initial_operation =
        ProductOperationRecord(
          ..staged_operation,
          product_id: None,
          status: "CREATED",
        )
      mutation_result(
        key,
        product_set_payload(
          store_after_operation,
          None,
          Some(initial_operation),
          [],
          field,
          variables,
          fragments,
        ),
        store_after_operation,
        identity_after_operation,
        [product.id, operation_id, ..staged_ids],
      )
    }
  }
}

@internal
pub fn stage_missing_async_product_duplicate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> MutationFieldResult {
  let #(operation_id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "ProductDuplicateOperation")
  let operation =
    ProductOperationRecord(
      id: operation_id,
      type_name: "ProductDuplicateOperation",
      product_id: None,
      new_product_id: None,
      status: "COMPLETE",
      user_errors: [
        ProductOperationUserErrorRecord(
          Some(["productId"]),
          "Product does not exist",
          None,
        ),
      ],
    )
  let #(staged_operation, next_store) =
    store.stage_product_operation(store, operation)
  let initial_operation =
    ProductOperationRecord(
      ..staged_operation,
      status: "CREATED",
      user_errors: [],
    )
  mutation_result(
    key,
    product_duplicate_payload(
      next_store,
      None,
      Some(initial_operation),
      [],
      field,
      variables,
      fragments,
    ),
    next_store,
    next_identity,
    [operation_id],
  )
}

@internal
pub fn stage_product_duplicate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  source_product: ProductRecord,
  new_title: Option(String),
  synchronous: Bool,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> MutationFieldResult {
  let #(duplicate_product, identity_after_product) =
    duplicated_product_record(store, identity, source_product, new_title)
  let #(_, store_after_product) =
    store.upsert_staged_product(store, duplicate_product)
  let #(store_after_relationships, identity_after_relationships, staged_ids) =
    duplicate_product_relationships(
      store_after_product,
      identity_after_product,
      product_id,
      duplicate_product.id,
    )
  case synchronous {
    True ->
      mutation_result(
        key,
        product_duplicate_payload(
          store_after_relationships,
          Some(duplicate_product),
          None,
          [],
          field,
          variables,
          fragments,
        ),
        store_after_relationships,
        identity_after_relationships,
        [duplicate_product.id, ..staged_ids],
      )
    False -> {
      let #(operation_id, identity_after_operation) =
        synthetic_identity.make_synthetic_gid(
          identity_after_relationships,
          "ProductDuplicateOperation",
        )
      let operation =
        ProductOperationRecord(
          id: operation_id,
          type_name: "ProductDuplicateOperation",
          product_id: Some(product_id),
          new_product_id: Some(duplicate_product.id),
          status: "COMPLETE",
          user_errors: [],
        )
      let #(staged_operation, next_store) =
        store.stage_product_operation(store_after_relationships, operation)
      let initial_operation =
        ProductOperationRecord(
          ..staged_operation,
          new_product_id: None,
          status: "CREATED",
        )
      mutation_result(
        key,
        product_duplicate_payload(
          next_store,
          None,
          Some(initial_operation),
          [],
          field,
          variables,
          fragments,
        ),
        next_store,
        identity_after_operation,
        [duplicate_product.id, operation_id, ..staged_ids],
      )
    }
  }
}
