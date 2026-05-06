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
import shopify_draft_proxy/proxy/products/products_l00.{product_seo_source}
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/products_l03.{
  product_price_range_source,
}
import shopify_draft_proxy/proxy/products/publications_l02.{
  make_product_resource_feedback_record, make_shop_resource_feedback_record,
}
import shopify_draft_proxy/proxy/products/publications_l03.{
  bulk_product_resource_feedback_create_payload,
  shop_resource_feedback_create_payload,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  count_source, empty_connection_source, read_arg_object_list,
}
import shopify_draft_proxy/proxy/products/shared_l01.{mutation_result}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type ProductUserError, MutationFieldResult,
  ProductUserError,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  has_only_default_variant, optional_product_category_source,
}
import shopify_draft_proxy/proxy/products/variants_l01.{
  optional_captured_json_source,
}
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
pub fn product_source_with_relationships(
  product: ProductRecord,
  collections: SourceValue,
  variants: SourceValue,
  media: SourceValue,
  options: SourceValue,
  selling_plan_groups: SourceValue,
  selling_plan_groups_count: SourceValue,
  currency_code: String,
  publication_id: Option(String),
) -> SourceValue {
  let visible_publication_count = case product.status == "ACTIVE" {
    True -> list.length(product.publication_ids)
    False -> 0
  }
  let published_on_publication = case publication_id, product.status {
    Some(id), "ACTIVE" -> list.contains(product.publication_ids, id)
    _, _ -> False
  }
  src_object([
    #("__typename", SrcString("Product")),
    #("id", SrcString(product.id)),
    #(
      "legacyResourceId",
      graphql_helpers.option_string_source(product.legacy_resource_id),
    ),
    #("title", SrcString(product.title)),
    #("handle", SrcString(product.handle)),
    #("status", SrcString(product.status)),
    #("vendor", graphql_helpers.option_string_source(product.vendor)),
    #("productType", graphql_helpers.option_string_source(product.product_type)),
    #("tags", SrcList(list.map(product.tags, SrcString))),
    #("priceRangeV2", product_price_range_source(product, currency_code)),
    #("priceRange", product_price_range_source(product, currency_code)),
    #(
      "totalVariants",
      graphql_helpers.option_int_source(product.total_variants),
    ),
    #(
      "hasOnlyDefaultVariant",
      graphql_helpers.option_bool_source(product.has_only_default_variant),
    ),
    #(
      "hasOutOfStockVariants",
      graphql_helpers.option_bool_source(product.has_out_of_stock_variants),
    ),
    #(
      "totalInventory",
      graphql_helpers.option_int_source(product.total_inventory),
    ),
    #(
      "tracksInventory",
      graphql_helpers.option_bool_source(product.tracks_inventory),
    ),
    #("createdAt", graphql_helpers.option_string_source(product.created_at)),
    #("updatedAt", graphql_helpers.option_string_source(product.updated_at)),
    #("publishedAt", graphql_helpers.option_string_source(product.published_at)),
    #("descriptionHtml", SrcString(product.description_html)),
    #(
      "onlineStorePreviewUrl",
      graphql_helpers.option_string_source(product.online_store_preview_url),
    ),
    #(
      "templateSuffix",
      graphql_helpers.option_string_source(product.template_suffix),
    ),
    #("seo", product_seo_source(product.seo)),
    #("category", optional_product_category_source(product.category)),
    #(
      "contextualPricing",
      optional_captured_json_source(product.contextual_pricing),
    ),
    #("publishedOnCurrentPublication", SrcBool(visible_publication_count > 0)),
    #("publishedOnCurrentChannel", SrcBool(visible_publication_count > 0)),
    #("publishedOnPublication", SrcBool(published_on_publication)),
    #(
      "combinedListingRole",
      graphql_helpers.option_string_source(product.combined_listing_role),
    ),
    #("availablePublicationsCount", count_source(visible_publication_count)),
    #("resourcePublicationsCount", count_source(visible_publication_count)),
    #("collections", collections),
    #("media", media),
    #("images", empty_connection_source()),
    #("options", options),
    #("variants", variants),
    #("requiresSellingPlan", SrcBool(False)),
    #("sellingPlanGroups", selling_plan_groups),
    #("sellingPlanGroupsCount", selling_plan_groups_count),
  ])
}

@internal
pub fn handle_bulk_product_resource_feedback_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let initial = #(store, identity, [], [], [])
  let #(next_store, next_identity, feedback, user_errors, staged_ids) =
    read_arg_object_list(args, "feedbackInput")
    |> enumerate_items()
    |> list.fold(initial, fn(acc, entry) {
      let #(current_store, current_identity, records, errors, ids) = acc
      let #(input, index) = entry
      let #(record, identity_after_record) =
        make_product_resource_feedback_record(current_identity, input)
      case record {
        Some(feedback_record) ->
          case
            store.get_effective_product_by_id(
              current_store,
              feedback_record.product_id,
            )
          {
            Some(_) -> {
              let #(staged, staged_store) =
                store.upsert_staged_product_resource_feedback(
                  current_store,
                  feedback_record,
                )
              #(
                staged_store,
                identity_after_record,
                list.append(records, [staged]),
                errors,
                list.append(ids, [staged.product_id]),
              )
            }
            None -> #(
              current_store,
              identity_after_record,
              records,
              list.append(errors, [
                ProductUserError(
                  ["feedbackInput", int.to_string(index), "productId"],
                  "Product does not exist",
                  None,
                ),
              ]),
              ids,
            )
          }
        None -> #(
          current_store,
          identity_after_record,
          records,
          list.append(errors, [
            ProductUserError(
              ["feedbackInput", int.to_string(index), "productId"],
              "Product does not exist",
              None,
            ),
          ]),
          ids,
        )
      }
    })
  mutation_result(
    key,
    bulk_product_resource_feedback_create_payload(
      feedback,
      user_errors,
      field,
      fragments,
    ),
    next_store,
    next_identity,
    staged_ids,
  )
}

@internal
pub fn handle_shop_resource_feedback_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let #(record, next_identity) =
    make_shop_resource_feedback_record(identity, input)
  case record {
    Some(feedback) -> {
      let #(staged, next_store) =
        store.upsert_staged_shop_resource_feedback(store, feedback)
      mutation_result(
        key,
        shop_resource_feedback_create_payload(
          Some(staged),
          [],
          field,
          fragments,
        ),
        next_store,
        next_identity,
        [staged.id],
      )
    }
    None ->
      mutation_result(
        key,
        shop_resource_feedback_create_payload(
          None,
          [ProductUserError(["input", "state"], "State is invalid", None)],
          field,
          fragments,
        ),
        store,
        next_identity,
        [],
      )
  }
}
