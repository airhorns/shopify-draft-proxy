//// Products-domain submodule: variants_validation.
//// Combines layered files: variants_l06, variants_l07.

import gleam/dict.{type Dict}

import gleam/int

import gleam/list
import gleam/option.{type Option, None, Some}

import shopify_draft_proxy/graphql/ast.{type Selection}

import shopify_draft_proxy/graphql/root_field.{type ResolvedValue}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcNull, get_field_response_key,
}

import shopify_draft_proxy/proxy/products/inventory_apply.{
  sync_product_inventory_summary,
}
import shopify_draft_proxy/proxy/products/inventory_validation.{
  apply_product_set_inventory_quantities,
  validate_bulk_create_inventory_quantities,
}
import shopify_draft_proxy/proxy/products/product_types.{
  type BulkVariantUserError, type MutationFieldResult, type ProductUserError,
  BulkVariantUserError, ProductUserError, RecomputeProductTotalInventory,
  VariantValidationProblem, max_product_variants, product_set_option_limit,
}
import shopify_draft_proxy/proxy/products/products_core.{enumerate_items}
import shopify_draft_proxy/proxy/products/products_records.{product_source}
import shopify_draft_proxy/proxy/products/shared.{
  mutation_result, read_object_list_field, read_string_field,
}
import shopify_draft_proxy/proxy/products/variants_helpers.{
  find_variant_update, has_variant_id, has_variant_option_input,
  variant_staged_ids,
}
import shopify_draft_proxy/proxy/products/variants_options.{
  create_option_input_errors, create_variant_strategy_errors,
  make_created_variant_record, product_variant_delete_payload,
  update_variant_record, validate_bulk_variant_option_input,
  variant_validation_problems,
}
import shopify_draft_proxy/proxy/products/variants_options_core.{
  bulk_variant_error_from_problem, product_set_variant_defaults,
}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type ProductOperationUserErrorRecord, type ProductOptionRecord,
  type ProductVariantRecord, ProductOperationUserErrorRecord,
}

// ===== from variants_l06 =====
@internal
pub fn variant_product_source(store: Store, product_id: String) -> SourceValue {
  case store.get_effective_product_by_id(store, product_id) {
    Some(product) -> product_source(product)
    None -> SrcNull
  }
}

@internal
pub fn product_set_scalar_variant_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  read_object_list_field(input, "variants")
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(variant_input, index) = pair
    variant_validation_problems(variant_input)
    |> list.map(fn(problem) {
      let VariantValidationProblem(suffix: suffix, message: message, ..) =
        problem
      ProductOperationUserErrorRecord(
        field: Some(["input", "variants", int.to_string(index), ..suffix]),
        message: message,
        code: Some("INVALID_VARIANT"),
      )
    })
  })
}

/// Detect input variants whose option-value tuples collide with an
/// earlier variant in the same `productSet` input. Shopify rejects these
/// at the API layer with one userError per offending later occurrence;
/// without local detection the proxy stages the duplicates and the
/// failure only surfaces at __meta/commit replay (see QA evidence in
/// `config/parity-specs/products/productSet-duplicate-variants.json`).
@internal
pub fn handle_product_variant_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        product_variant_delete_payload(
          None,
          [ProductUserError(["id"], "Variant id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(variant_id) ->
      case store.get_effective_variant_by_id(store, variant_id) {
        None ->
          mutation_result(
            key,
            product_variant_delete_payload(
              None,
              [ProductUserError(["id"], "Variant not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(existing_variant) -> {
          let next_variants =
            store.get_effective_variants_by_product_id(
              store,
              existing_variant.product_id,
            )
            |> list.filter(fn(variant) { variant.id != variant_id })
          let next_store =
            store.replace_staged_variants_for_product(
              store,
              existing_variant.product_id,
              next_variants,
            )
          let #(_, next_store, final_identity) =
            sync_product_inventory_summary(
              next_store,
              identity,
              existing_variant.product_id,
              RecomputeProductTotalInventory,
            )
          mutation_result(
            key,
            product_variant_delete_payload(
              Some(variant_id),
              [],
              field,
              fragments,
            ),
            next_store,
            final_identity,
            [variant_id],
          )
        }
      }
  }
}

@internal
pub fn product_set_variant_records(
  store: Store,
  identity: SyntheticIdentityRegistry,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry, List(String)) {
  let existing_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  let #(reversed, final_identity, ids) =
    list.fold(inputs, #([], identity, []), fn(acc, input) {
      let #(records, current_identity, collected_ids) = acc
      let existing = case read_string_field(input, "id") {
        Some(id) ->
          existing_variants
          |> list.find(fn(variant) { variant.id == id })
          |> option.from_result
        None -> None
      }
      let #(variant, identity_after_variant) = case existing {
        Some(variant) -> update_variant_record(current_identity, variant, input)
        None ->
          make_created_variant_record(current_identity, product_id, input, None)
      }
      let variant = product_set_variant_defaults(variant)
      let #(variant, next_identity) =
        apply_product_set_inventory_quantities(
          store,
          identity_after_variant,
          variant,
          input,
        )
      #(
        [variant, ..records],
        next_identity,
        list.append(collected_ids, variant_staged_ids(variant)),
      )
    })
  #(list.reverse(reversed), final_identity, ids)
}

@internal
pub fn make_created_variant_records(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
  defaults: Option(ProductVariantRecord),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry) {
  let #(reversed, final_identity) =
    list.fold(inputs, #([], identity), fn(acc, input) {
      let #(variants, current_identity) = acc
      let #(variant, next_identity) =
        make_created_variant_record(
          current_identity,
          product_id,
          input,
          defaults,
        )
      #([variant, ..variants], next_identity)
    })
  #(list.reverse(reversed), final_identity)
}

@internal
pub fn validate_bulk_variant_scalar_input(
  input: Dict(String, ResolvedValue),
  variant_index: Int,
) -> List(BulkVariantUserError) {
  variant_validation_problems(input)
  |> list.flat_map(fn(problem) {
    bulk_variant_error_from_problem(problem, variant_index)
  })
}

@internal
pub fn validate_product_variant_scalar_input(
  input: Dict(String, ResolvedValue),
  prefix: List(String),
) -> List(ProductUserError) {
  variant_validation_problems(input)
  |> list.map(fn(problem) {
    let VariantValidationProblem(
      suffix: suffix,
      message: message,
      product_code: code,
      ..,
    ) = problem
    ProductUserError(list.append(prefix, suffix), message, code)
  })
}

@internal
pub fn update_variant_records(
  identity: SyntheticIdentityRegistry,
  variants: List(ProductVariantRecord),
  updates: List(Dict(String, ResolvedValue)),
) -> #(
  List(ProductVariantRecord),
  List(ProductVariantRecord),
  SyntheticIdentityRegistry,
) {
  let #(reversed_variants, reversed_updated, final_identity) =
    list.fold(variants, #([], [], identity), fn(acc, variant) {
      let #(next_variants, updated_variants, current_identity) = acc
      case find_variant_update(updates, variant.id) {
        Some(input) -> {
          let #(updated, next_identity) =
            update_variant_record(current_identity, variant, input)
          #(
            [updated, ..next_variants],
            [updated, ..updated_variants],
            next_identity,
          )
        }
        None -> #(
          [variant, ..next_variants],
          updated_variants,
          current_identity,
        )
      }
    })
  #(
    list.reverse(reversed_variants),
    list.reverse(reversed_updated),
    final_identity,
  )
}

@internal
pub fn validate_product_options_create_inputs(
  existing_options: List(ProductOptionRecord),
  existing_variants: List(ProductVariantRecord),
  inputs: List(Dict(String, ResolvedValue)),
  should_create_option_variants: Bool,
  replacing_default: Bool,
) -> List(ProductUserError) {
  let total_option_errors = case
    list.length(existing_options) + list.length(inputs)
    > product_set_option_limit
  {
    True -> [
      ProductUserError(
        ["options"],
        "Can only specify a maximum of 3 options",
        Some("OPTIONS_OVER_LIMIT"),
      ),
    ]
    False -> []
  }
  list.append(
    total_option_errors,
    list.append(
      create_option_input_errors(
        existing_options,
        existing_variants,
        inputs,
        replacing_default,
      ),
      create_variant_strategy_errors(
        existing_options,
        existing_variants,
        inputs,
        should_create_option_variants,
      ),
    ),
  )
}

// ===== from variants_l07 =====
@internal
pub fn product_create_variant_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  read_object_list_field(input, "variants")
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(variant_input, index) = pair
    validate_product_variant_scalar_input(variant_input, [
      "variants",
      int.to_string(index),
    ])
  })
}

@internal
pub fn validate_bulk_create_variant_batch(
  store: Store,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
  retained_variant_count: Int,
) -> List(BulkVariantUserError) {
  case retained_variant_count + list.length(inputs) > max_product_variants {
    True -> [
      BulkVariantUserError(
        None,
        "You can only have a maximum of "
          <> int.to_string(max_product_variants)
          <> " variants per product",
        Some("LIMIT_EXCEEDED"),
      ),
    ]
    False ->
      inputs
      |> enumerate_items()
      |> list.flat_map(fn(pair) {
        let #(input, index) = pair
        let scalar_errors = validate_bulk_variant_scalar_input(input, index)
        let #(selected_options, option_errors) =
          validate_bulk_variant_option_input(
            store,
            product_id,
            input,
            index,
            "create",
          )
        let inventory_errors =
          validate_bulk_create_inventory_quantities(
            store,
            input,
            index,
            selected_options,
          )
        list.append(scalar_errors, list.append(option_errors, inventory_errors))
      })
  }
}

@internal
pub fn validate_bulk_update_variant_batch(
  store: Store,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
  variants: List(ProductVariantRecord),
) -> List(BulkVariantUserError) {
  case inputs {
    [] -> [
      BulkVariantUserError(
        None,
        "Something went wrong, please try again.",
        None,
      ),
    ]
    _ ->
      inputs
      |> enumerate_items()
      |> list.flat_map(fn(pair) {
        let #(input, index) = pair
        case read_string_field(input, "id") {
          None -> [
            BulkVariantUserError(
              Some(["variants", int.to_string(index), "id"]),
              "Product variant is missing ID attribute",
              Some("PRODUCT_VARIANT_ID_MISSING"),
            ),
          ]
          Some(variant_id) ->
            case has_variant_id(variants, variant_id) {
              False -> [
                BulkVariantUserError(
                  Some(["variants", int.to_string(index), "id"]),
                  "Product variant does not exist",
                  Some("PRODUCT_VARIANT_DOES_NOT_EXIST"),
                ),
              ]
              True ->
                case dict.has_key(input, "inventoryQuantities") {
                  True -> [
                    BulkVariantUserError(
                      Some([
                        "variants",
                        int.to_string(index),
                        "inventoryQuantities",
                      ]),
                      "Inventory quantities can only be provided during create. To update inventory for existing variants, use inventoryAdjustQuantities.",
                      Some("NO_INVENTORY_QUANTITIES_ON_VARIANTS_UPDATE"),
                    ),
                  ]
                  False ->
                    list.append(
                      validate_bulk_variant_scalar_input(input, index),
                      case has_variant_option_input(input) {
                        True -> {
                          let #(_, errors) =
                            validate_bulk_variant_option_input(
                              store,
                              product_id,
                              input,
                              index,
                              "update",
                            )
                          errors
                        }
                        False -> []
                      },
                    )
                }
            }
        }
      })
  }
}
