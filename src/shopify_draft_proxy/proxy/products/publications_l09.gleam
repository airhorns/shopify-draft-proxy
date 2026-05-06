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
import shopify_draft_proxy/proxy/products/publications_l00.{
  ensure_default_publication_baseline, make_unique_publication_gid,
}
import shopify_draft_proxy/proxy/products/publications_l01.{
  remove_publication_from_publishables,
}
import shopify_draft_proxy/proxy/products/publications_l08.{
  publication_mutation_payload,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_bool_field, read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{mutation_result}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type ProductUserError, MutationFieldResult,
  ProductUserError,
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
pub fn handle_publication_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case root_name {
    "publicationCreate" -> {
      let store = ensure_default_publication_baseline(store)
      let input =
        graphql_helpers.read_arg_object(args, "input")
        |> option.unwrap(dict.new())
      let #(publication_id, next_identity) =
        make_unique_publication_gid(store, identity)
      let name = read_string_field(input, "name")
      let publication =
        PublicationRecord(
          id: publication_id,
          name: name,
          auto_publish: read_bool_field(input, "autoPublish"),
          supports_future_publishing: Some(False),
          catalog_id: read_string_field(input, "catalogId"),
          channel_id: read_string_field(input, "channelId"),
          cursor: None,
        )
      let #(staged, next_store) =
        store.upsert_staged_publication(store, publication)
      mutation_result(
        key,
        publication_mutation_payload(
          next_store,
          "PublicationCreatePayload",
          Some(staged),
          None,
          [],
          field,
          fragments,
        ),
        next_store,
        next_identity,
        [staged.id],
      )
    }
    "publicationUpdate" -> {
      let input =
        graphql_helpers.read_arg_object(args, "input")
        |> option.unwrap(dict.new())
      let publication_id =
        graphql_helpers.read_arg_string(args, "id")
        |> option.or(read_string_field(input, "id"))
      case publication_id {
        None ->
          mutation_result(
            key,
            publication_mutation_payload(
              store,
              "PublicationUpdatePayload",
              None,
              None,
              [ProductUserError(["id"], "Publication id is required", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(id) ->
          case store.get_effective_publication_by_id(store, id) {
            None ->
              mutation_result(
                key,
                publication_mutation_payload(
                  store,
                  "PublicationUpdatePayload",
                  None,
                  None,
                  [ProductUserError(["id"], "Publication not found", None)],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(existing) -> {
              let publication =
                PublicationRecord(
                  ..existing,
                  name: read_string_field(input, "name")
                    |> option.or(existing.name),
                  auto_publish: read_bool_field(input, "autoPublish")
                    |> option.or(existing.auto_publish),
                  supports_future_publishing: read_bool_field(
                      input,
                      "supportsFuturePublishing",
                    )
                    |> option.or(existing.supports_future_publishing),
                  catalog_id: read_string_field(input, "catalogId")
                    |> option.or(existing.catalog_id),
                  channel_id: read_string_field(input, "channelId")
                    |> option.or(existing.channel_id),
                )
              let #(staged, next_store) =
                store.upsert_staged_publication(store, publication)
              mutation_result(
                key,
                publication_mutation_payload(
                  next_store,
                  "PublicationUpdatePayload",
                  Some(staged),
                  None,
                  [],
                  field,
                  fragments,
                ),
                next_store,
                identity,
                [staged.id],
              )
            }
          }
      }
    }
    "publicationDelete" -> {
      case graphql_helpers.read_arg_string(args, "id") {
        None ->
          mutation_result(
            key,
            publication_mutation_payload(
              store,
              "PublicationDeletePayload",
              None,
              None,
              [ProductUserError(["id"], "Publication id is required", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(id) ->
          case store.get_effective_publication_by_id(store, id) {
            None ->
              mutation_result(
                key,
                publication_mutation_payload(
                  store,
                  "PublicationDeletePayload",
                  None,
                  None,
                  [ProductUserError(["id"], "Publication not found", None)],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(existing) -> {
              let next_store =
                store
                |> remove_publication_from_publishables(id)
                |> store.delete_staged_publication(id)
              mutation_result(
                key,
                publication_mutation_payload(
                  next_store,
                  "PublicationDeletePayload",
                  Some(existing),
                  Some(id),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                identity,
                [id],
              )
            }
          }
      }
    }
    _ -> mutation_result(key, json.null(), store, identity, [])
  }
}
