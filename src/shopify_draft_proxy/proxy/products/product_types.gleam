//// Internal products-domain implementation split from proxy/products.gleam.

import gleam/json.{type Json}

import gleam/option.{type Option, Some}

import shopify_draft_proxy/graphql/root_field.{type RootFieldError}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{type ProductVariantSelectedOptionRecord}

pub type ProductsError {
  ParseFailed(RootFieldError)
}

@internal
pub type ProductSetInventoryQuantityInput {
  ProductSetInventoryQuantityInput(
    location_id: Option(String),
    name: String,
    quantity: Int,
  )
}

@internal
pub const product_set_variant_limit = 2048

@internal
pub const product_set_option_limit = 3

@internal
pub const product_set_option_value_limit = 100

@internal
pub const product_set_file_limit = 250

@internal
pub const product_set_inventory_quantities_limit = 250

@internal
pub const product_tag_limit = 250

@internal
pub const product_option_name_limit = 255

@internal
pub const product_tag_character_limit = 255

@internal
pub const collection_title_character_limit = 255

@internal
pub const collection_handle_character_limit = 255

@internal
pub const product_string_character_limit = 255

@internal
pub const product_description_html_limit_bytes = 524_287

@internal
pub type ProductUserError {
  ProductUserError(field: List(String), message: String, code: Option(String))
}

@internal
pub const product_user_error_code_blank = "BLANK"

@internal
pub const product_user_error_code_invalid = "INVALID"

@internal
pub const product_user_error_code_taken = "TAKEN"

@internal
pub const product_user_error_code_greater_than = "GREATER_THAN"

@internal
pub const product_user_error_code_less_than = "LESS_THAN"

@internal
pub const product_user_error_code_inclusion = "INCLUSION"

@internal
pub const product_user_error_code_not_a_number = "NOT_A_NUMBER"

@internal
pub const product_user_error_code_product_does_not_exist = "PRODUCT_DOES_NOT_EXIST"

@internal
pub const product_user_error_code_product_not_found = "PRODUCT_NOT_FOUND"

@internal
pub const product_user_error_code_product_variant_does_not_exist = "PRODUCT_VARIANT_DOES_NOT_EXIST"

@internal
pub const product_user_error_code_invalid_inventory_item = "INVALID_INVENTORY_ITEM"

@internal
pub const product_user_error_code_invalid_location = "INVALID_LOCATION"

@internal
pub const product_user_error_code_invalid_name = "INVALID_NAME"

@internal
pub const product_user_error_code_invalid_quantity_negative = "INVALID_QUANTITY_NEGATIVE"

@internal
pub const product_user_error_code_invalid_quantity_too_high = "INVALID_QUANTITY_TOO_HIGH"

@internal
pub const product_user_error_code_invalid_quantity_too_low = "INVALID_QUANTITY_TOO_LOW"

@internal
pub fn product_user_error(
  field: List(String),
  message: String,
  code: String,
) -> ProductUserError {
  ProductUserError(field, message, Some(code))
}

@internal
pub fn blank_product_user_error(
  field: List(String),
  message: String,
) -> ProductUserError {
  product_user_error(field, message, product_user_error_code_blank)
}

@internal
pub fn product_does_not_exist_user_error(
  field: List(String),
) -> ProductUserError {
  product_user_error(
    field,
    "Product does not exist",
    product_user_error_code_product_does_not_exist,
  )
}

@internal
pub type BulkVariantUserError {
  BulkVariantUserError(
    field: Option(List(String)),
    message: String,
    code: Option(String),
  )
}

@internal
pub type VariantValidationProblem {
  VariantValidationProblem(
    kind: String,
    suffix: List(String),
    bulk_suffix: List(String),
    message: String,
    bulk_code: Option(String),
    product_code: Option(String),
  )
}

@internal
pub type NumericRead {
  NumericValue(Float)
  NumericNotANumber
  NumericMissing
  NumericNull
}

@internal
pub type QuantityRead {
  QuantityInt(Int)
  QuantityFloat(Float)
  QuantityNotANumber
  QuantityMissing
  QuantityNull
}

@internal
pub const max_product_variants = 2048

@internal
pub const max_variant_price = 1.0e18

@internal
pub const max_variant_weight = 2.0e9

@internal
pub const min_inventory_quantity = -1_000_000_000

@internal
pub const max_inventory_quantity = 1_000_000_000

@internal
pub const max_variant_text_length = 255

@internal
pub type InventoryTransferLineItemInput {
  InventoryTransferLineItemInput(
    inventory_item_id: Option(String),
    quantity: Option(Int),
  )
}

@internal
pub type InventoryTransferLineItemUpdate {
  InventoryTransferLineItemUpdate(
    inventory_item_id: String,
    new_quantity: Int,
    delta_quantity: Int,
  )
}

@internal
pub type CollectionProductMove {
  CollectionProductMove(id: String, new_position: Int)
}

@internal
pub type CollectionProductPlacement {
  AppendProducts
  PrependReverseProducts
}

@internal
pub type VariantMediaInput {
  VariantMediaInput(variant_id: String, media_ids: List(String))
}

@internal
pub type NullableFieldUserError {
  NullableFieldUserError(field: Option(List(String)), message: String)
}

@internal
pub type InventoryAdjustmentChange {
  InventoryAdjustmentChange(
    inventory_item_id: String,
    location_id: String,
    name: String,
    delta: Int,
    quantity_after_change: Option(Int),
    ledger_document_uri: Option(String),
  )
}

@internal
pub type InventoryAdjustmentChangeInput {
  InventoryAdjustmentChangeInput(
    inventory_item_id: Option(String),
    location_id: Option(String),
    ledger_document_uri: Option(String),
    delta: Option(Int),
    change_from_quantity: Option(Int),
  )
}

@internal
pub type InventoryAdjustmentGroup {
  InventoryAdjustmentGroup(
    id: String,
    created_at: String,
    reason: String,
    reference_document_uri: Option(String),
    changes: List(InventoryAdjustmentChange),
  )
}

@internal
pub type InventorySetQuantityInput {
  InventorySetQuantityInput(
    inventory_item_id: Option(String),
    location_id: Option(String),
    quantity: Option(Int),
    compare_quantity: Option(Int),
    change_from_quantity: Option(Int),
  )
}

@internal
pub type InventoryMoveTerminalInput {
  InventoryMoveTerminalInput(
    location_id: Option(String),
    name: Option(String),
    ledger_document_uri: Option(String),
  )
}

@internal
pub type InventoryMoveQuantityInput {
  InventoryMoveQuantityInput(
    inventory_item_id: Option(String),
    quantity: Option(Int),
    from: InventoryMoveTerminalInput,
    to: InventoryMoveTerminalInput,
  )
}

@internal
pub type ProductVariantPositionInput {
  ProductVariantPositionInput(id: String, position: Int)
}

@internal
pub type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    top_level_errors: List(Json),
    top_level_error_data_entries: List(#(String, Json)),
    /// True when local validation rejected the input before staging
    /// any state. The dispatch site records the mutation log entry as
    /// Failed (rather than Staged) so __meta/commit replay does not
    /// re-send a payload Shopify will also reject.
    staging_failed: Bool,
  )
}

@internal
pub type InventoryShipmentDelta {
  InventoryShipmentDelta(
    inventory_item_id: String,
    incoming: Int,
    available: Option(Int),
  )
}

@internal
pub type CollectionRuleSetPresence {
  RuleSetAbsent
  RuleSetCustom
  RuleSetSmart
}

@internal
pub type ProductTotalInventorySync {
  PreserveProductTotalInventory
  RecomputeProductTotalInventory
}

@internal
pub type ProductDerivedSummary {
  ProductDerivedSummary(
    price_range_min: Option(String),
    price_range_max: Option(String),
    total_variants: Option(Int),
    has_only_default_variant: Option(Bool),
    has_out_of_stock_variants: Option(Bool),
    total_inventory: Option(Int),
    tracks_inventory: Option(Bool),
  )
}

@internal
pub type RenamedOptionValue =
  #(String, String)

@internal
pub type VariantCombination =
  List(ProductVariantSelectedOptionRecord)
