//// Store operations for product and media records.

import gleam/dict.{type Dict}
import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/string
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store/shared.{
  append_unique_id, compare_collection_membership_entries,
  compare_product_collection_records, dedupe_strings, dict_has, list_to_set,
  option_to_result, product_collection_storage_key, string_compare,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type CollectionRecord, type FileRecord, type InventoryLevelRecord,
  type ProductCollectionRecord, type ProductMediaRecord,
  type ProductOperationRecord, type ProductOptionRecord,
  type ProductOptionValueRecord, type ProductRecord, type ProductVariantRecord,
  type SellingPlanGroupRecord,
} as _

// ---------------------------------------------------------------------------
// Products slice
// ---------------------------------------------------------------------------

/// Upsert product records into the base state. Capture/upstream state wins
/// over stale deleted markers, matching the existing collection slice pattern.
pub fn upsert_base_products(
  store: Store,
  records: List(ProductRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        products: dict.insert(base.products, record.id, record),
        product_order: append_unique_id(base.product_order, record.id),
        deleted_product_ids: dict.delete(base.deleted_product_ids, record.id),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_product_ids: dict.delete(staged.deleted_product_ids, record.id),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn set_base_product_count(store: Store, count: Int) -> Store {
  Store(
    ..store,
    base_state: BaseState(..store.base_state, product_count: Some(count)),
  )
}

pub fn set_staged_product_count(store: Store, count: Int) -> Store {
  Store(
    ..store,
    staged_state: StagedState(..store.staged_state, product_count: Some(count)),
  )
}

pub fn upsert_staged_product(
  store: Store,
  record: ProductRecord,
) -> #(ProductRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.product_order, record.id)
    || list.contains(staged.product_order, record.id)
  let new_order = case already_known {
    True -> staged.product_order
    False -> list.append(staged.product_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      products: dict.insert(staged.products, record.id, record),
      product_order: new_order,
      deleted_product_ids: dict.delete(staged.deleted_product_ids, record.id),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_product(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      products: dict.delete(staged.products, id),
      deleted_product_ids: dict.insert(staged.deleted_product_ids, id, True),
    )
  Store(..store, staged_state: new_staged)
}

pub fn replace_base_media_for_product(
  store: Store,
  product_id: String,
  media: List(ProductMediaRecord),
) -> Store {
  Store(
    ..store,
    base_state: BaseState(
      ..store.base_state,
      product_media: dict.insert(
        store.base_state.product_media,
        product_id,
        media,
      ),
    ),
  )
}

pub fn replace_staged_media_for_product(
  store: Store,
  product_id: String,
  media: List(ProductMediaRecord),
) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      product_media: dict.insert(
        store.staged_state.product_media,
        product_id,
        media,
      ),
    ),
  )
}

pub fn get_effective_media_by_product_id(
  store: Store,
  product_id: String,
) -> List(ProductMediaRecord) {
  case dict.get(store.staged_state.product_media, product_id) {
    Ok(media) -> sort_product_media(media)
    Error(_) ->
      case dict.get(store.base_state.product_media, product_id) {
        Ok(media) -> sort_product_media(media)
        Error(_) -> []
      }
  }
}

fn sort_product_media(
  media: List(ProductMediaRecord),
) -> List(ProductMediaRecord) {
  list.sort(media, fn(left, right) {
    case int.compare(left.position, right.position) {
      order.Eq -> string_compare(left.key, right.key)
      other -> other
    }
  })
}

// ---------------------------------------------------------------------------
// Files slice
// ---------------------------------------------------------------------------

pub fn upsert_base_files(store: Store, files: List(FileRecord)) -> Store {
  list.fold(files, store, fn(current, file) {
    let base = current.base_state
    Store(
      ..current,
      base_state: BaseState(
        ..base,
        files: dict.insert(base.files, file.id, file),
        file_order: append_unique_id(base.file_order, file.id),
        deleted_file_ids: dict.delete(base.deleted_file_ids, file.id),
      ),
      staged_state: StagedState(
        ..current.staged_state,
        deleted_file_ids: dict.delete(
          current.staged_state.deleted_file_ids,
          file.id,
        ),
      ),
    )
  })
}

pub fn upsert_staged_files(store: Store, files: List(FileRecord)) -> Store {
  list.fold(files, store, fn(current, file) {
    let staged = current.staged_state
    let already_known =
      dict.has_key(current.base_state.files, file.id)
      || dict.has_key(staged.files, file.id)
      || list.contains(current.base_state.file_order, file.id)
      || list.contains(staged.file_order, file.id)
    let file_order = case already_known {
      True -> staged.file_order
      False -> list.append(staged.file_order, [file.id])
    }
    Store(
      ..current,
      staged_state: StagedState(
        ..staged,
        files: dict.insert(staged.files, file.id, file),
        file_order: file_order,
        deleted_file_ids: dict.delete(staged.deleted_file_ids, file.id),
      ),
    )
  })
}

pub fn delete_staged_files(store: Store, file_ids: List(String)) -> Store {
  let store =
    list.fold(file_ids, store, fn(current, file_id) {
      let staged = current.staged_state
      Store(
        ..current,
        staged_state: StagedState(
          ..staged,
          files: dict.delete(staged.files, file_id),
          deleted_file_ids: dict.insert(staged.deleted_file_ids, file_id, True),
        ),
      )
    })

  product_ids_with_media_ids(store, file_ids)
  |> list.fold(store, fn(current, product_id) {
    let next_media =
      get_effective_media_by_product_id(current, product_id)
      |> list.filter(fn(media) {
        case media.id {
          Some(id) -> !list.contains(file_ids, id)
          None -> True
        }
      })
    replace_staged_media_for_product(current, product_id, next_media)
  })
}

pub fn has_effective_file_by_id(store: Store, file_id: String) -> Bool {
  case dict_has(store.staged_state.deleted_file_ids, file_id) {
    True -> False
    False ->
      dict.has_key(store.staged_state.files, file_id)
      || dict.has_key(store.base_state.files, file_id)
      || product_media_file_exists(store, file_id)
  }
}

pub fn get_effective_file_by_id(
  store: Store,
  file_id: String,
) -> Option(FileRecord) {
  case dict_has(store.staged_state.deleted_file_ids, file_id) {
    True -> None
    False ->
      case dict.get(store.staged_state.files, file_id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.files, file_id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_files(store: Store) -> List(FileRecord) {
  let ids =
    list.append(store.base_state.file_order, store.staged_state.file_order)
    |> dedupe_strings
  let ordered =
    ids
    |> list.filter_map(fn(id) {
      case get_effective_file_by_id(store, id) {
        Some(file) -> Ok(file)
        None -> Error(Nil)
      }
    })
  let ordered_ids = list.map(ordered, fn(file) { file.id })
  let unordered =
    list.append(
      dict.values(store.base_state.files),
      dict.values(store.staged_state.files),
    )
    |> list.filter(fn(file) { !list.contains(ordered_ids, file.id) })
    |> list.filter(fn(file) {
      !dict_has(store.staged_state.deleted_file_ids, file.id)
    })
    |> list.sort(fn(left, right) { string_compare(left.id, right.id) })
  list.append(ordered, unordered)
}

pub fn list_effective_product_media(store: Store) -> List(ProductMediaRecord) {
  let product_ids =
    list.append(
      dict.keys(store.base_state.product_media),
      dict.keys(store.staged_state.product_media),
    )
    |> dedupe_strings
  product_ids
  |> list.flat_map(fn(product_id) {
    get_effective_media_by_product_id(store, product_id)
  })
}

fn product_media_file_exists(store: Store, file_id: String) -> Bool {
  list.any(list_effective_product_media(store), fn(media) {
    media.id == Some(file_id)
  })
}

fn product_ids_with_media_ids(
  store: Store,
  file_ids: List(String),
) -> List(String) {
  list_effective_product_media(store)
  |> list.filter_map(fn(media) {
    case media.id {
      Some(id) ->
        case list.contains(file_ids, id) {
          True -> Ok(media.product_id)
          False -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
  |> dedupe_strings
}

pub fn get_effective_product_by_id(
  store: Store,
  id: String,
) -> Option(ProductRecord) {
  let deleted =
    dict_has(store.base_state.deleted_product_ids, id)
    || dict_has(store.staged_state.deleted_product_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.products, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.products, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn get_effective_product_by_handle(
  store: Store,
  handle: String,
) -> Option(ProductRecord) {
  case
    list.find(list_effective_products(store), fn(product) {
      product.handle == handle
    })
  {
    Ok(product) -> Some(product)
    Error(_) -> None
  }
}

pub fn list_effective_products(store: Store) -> List(ProductRecord) {
  let ordered_ids =
    list.append(
      store.base_state.product_order,
      store.staged_state.product_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_product_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.products, store.staged_state.products)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_product_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn get_effective_product_count(store: Store) -> Int {
  case store.staged_state.product_count {
    Some(count) -> count
    None ->
      case store.base_state.product_count {
        Some(count) -> count
        None -> list.length(list_effective_products(store))
      }
  }
}

pub fn replace_base_products_for_collection(
  store: Store,
  collection_id: String,
  records: List(ProductCollectionRecord),
) -> Store {
  let base = store.base_state
  let retained =
    base.product_collections
    |> dict.keys()
    |> list.fold(base.product_collections, fn(acc, key) {
      case dict.get(acc, key) {
        Ok(record) ->
          case record.collection_id == collection_id {
            True -> dict.delete(acc, key)
            False -> acc
          }
        Error(_) -> acc
      }
    })
  let next =
    list.fold(records, retained, fn(acc, record) {
      dict.insert(acc, product_collection_storage_key(record), record)
    })
  Store(..store, base_state: BaseState(..base, product_collections: next))
}

pub fn upsert_base_product_collections(
  store: Store,
  records: List(ProductCollectionRecord),
) -> Store {
  let base = store.base_state
  let product_collections =
    list.fold(records, base.product_collections, fn(acc, record) {
      dict.insert(acc, product_collection_storage_key(record), record)
    })
  Store(
    ..store,
    base_state: BaseState(..base, product_collections: product_collections),
  )
}

pub fn upsert_staged_product_collections(
  store: Store,
  records: List(ProductCollectionRecord),
) -> Store {
  let staged = store.staged_state
  let product_collections =
    list.fold(records, staged.product_collections, fn(acc, record) {
      dict.insert(acc, product_collection_storage_key(record), record)
    })
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      product_collections: product_collections,
    ),
  )
}

pub fn replace_staged_collections_for_product(
  store: Store,
  product_id: String,
  records: List(ProductCollectionRecord),
) -> Store {
  let staged = store.staged_state
  let retained =
    staged.product_collections
    |> dict.keys()
    |> list.fold(staged.product_collections, fn(acc, key) {
      case dict.get(acc, key) {
        Ok(record) ->
          case record.product_id == product_id {
            True -> dict.delete(acc, key)
            False -> acc
          }
        Error(_) -> acc
      }
    })
  let product_collections =
    list.fold(records, retained, fn(acc, record) {
      dict.insert(acc, product_collection_storage_key(record), record)
    })
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      product_collections: product_collections,
      staged_product_collection_families: dict.insert(
        staged.staged_product_collection_families,
        product_id,
        True,
      ),
    ),
  )
}

pub fn get_effective_collection_by_id(
  store: Store,
  id: String,
) -> Option(CollectionRecord) {
  let deleted =
    dict_has(store.base_state.deleted_collection_ids, id)
    || dict_has(store.staged_state.deleted_collection_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.collections, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.collections, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn get_effective_collection_by_handle(
  store: Store,
  handle: String,
) -> Option(CollectionRecord) {
  list.find(list_effective_collections(store), fn(collection) {
    collection.handle == handle
  })
  |> option.from_result
}

pub fn list_effective_collections(store: Store) -> List(CollectionRecord) {
  let ordered_ids =
    list.append(
      store.base_state.collection_order,
      store.staged_state.collection_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_collection_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.collections, store.staged_state.collections)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_collection_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn list_effective_products_for_collection(
  store: Store,
  collection_id: String,
) -> List(#(ProductRecord, ProductCollectionRecord)) {
  let memberships =
    list_effective_products(store)
    |> list.flat_map(fn(product) {
      list_effective_collections_for_product(store, product.id)
    })
    |> list.map(fn(entry) {
      let #(_, membership) = entry
      membership
    })
    |> list.filter(fn(record) { record.collection_id == collection_id })
    |> list.sort(compare_product_collection_records)
  list.filter_map(memberships, fn(membership) {
    case get_effective_product_by_id(store, membership.product_id) {
      Some(product) -> Ok(#(product, membership))
      None -> Error(Nil)
    }
  })
}

pub fn list_effective_collections_for_product(
  store: Store,
  product_id: String,
) -> List(#(CollectionRecord, ProductCollectionRecord)) {
  let staged_memberships =
    dict.values(store.staged_state.product_collections)
    |> list.filter(fn(record) { record.product_id == product_id })
  let source_memberships = case
    dict_has(store.staged_state.staged_product_collection_families, product_id)
  {
    True -> staged_memberships
    False ->
      dict.values(store.base_state.product_collections)
      |> list.append(staged_memberships)
      |> list.filter(fn(record) { record.product_id == product_id })
  }
  let memberships =
    source_memberships
    |> list.sort(compare_product_collection_records)
  list.filter_map(memberships, fn(membership) {
    case get_effective_collection_by_id(store, membership.collection_id) {
      Some(collection) -> Ok(#(collection, membership))
      None -> Error(Nil)
    }
  })
  |> list.sort(compare_collection_membership_entries)
}

pub fn upsert_base_product_variants(
  store: Store,
  records: List(ProductVariantRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let new_base =
      BaseState(
        ..base,
        product_variants: dict.insert(base.product_variants, record.id, record),
        product_variant_order: append_unique_id(
          base.product_variant_order,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base)
  })
}

pub fn set_base_product_variant_count(store: Store, count: Int) -> Store {
  Store(
    ..store,
    base_state: BaseState(
      ..store.base_state,
      product_variant_count: Some(count),
    ),
  )
}

pub fn set_staged_product_variant_count(store: Store, count: Int) -> Store {
  Store(
    ..store,
    staged_state: StagedState(
      ..store.staged_state,
      product_variant_count: Some(count),
    ),
  )
}

pub fn upsert_staged_product_variant(
  store: Store,
  record: ProductVariantRecord,
) -> #(ProductVariantRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.product_variant_order, record.id)
    || list.contains(staged.product_variant_order, record.id)
  let new_order = case already_known {
    True -> staged.product_variant_order
    False -> list.append(staged.product_variant_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      product_variants: dict.insert(staged.product_variants, record.id, record),
      product_variant_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn replace_staged_variants_for_product(
  store: Store,
  product_id: String,
  variants: List(ProductVariantRecord),
) -> Store {
  let staged = store.staged_state
  let retained =
    remove_variants_for_product(staged.product_variants, product_id)
  let retained_order =
    staged.product_variant_order
    |> list.filter(fn(id) { dict.has_key(retained, id) })
  let next_variants =
    list.fold(variants, retained, fn(acc, variant) {
      dict.insert(acc, variant.id, variant)
    })
  let next_order =
    list.fold(variants, retained_order, fn(acc, variant) {
      append_unique_id(acc, variant.id)
    })
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      product_variants: next_variants,
      product_variant_order: next_order,
    ),
  )
}

pub fn get_base_variants_by_product_id(
  store: Store,
  product_id: String,
) -> List(ProductVariantRecord) {
  case product_is_deleted(store, product_id) {
    True -> []
    False ->
      list_variant_records(
        store.base_state.product_variants,
        store.base_state.product_variant_order,
      )
      |> list.filter(fn(variant) { variant.product_id == product_id })
  }
}

pub fn get_effective_variants_by_product_id(
  store: Store,
  product_id: String,
) -> List(ProductVariantRecord) {
  case product_is_deleted(store, product_id) {
    True -> []
    False -> {
      let staged_variants =
        list_variant_records(
          store.staged_state.product_variants,
          store.staged_state.product_variant_order,
        )
        |> list.filter(fn(variant) { variant.product_id == product_id })
      case staged_variants {
        [] -> get_base_variants_by_product_id(store, product_id)
        _ -> staged_variants
      }
    }
  }
}

pub fn get_effective_variant_by_id(
  store: Store,
  variant_id: String,
) -> Option(ProductVariantRecord) {
  case dict.get(store.staged_state.product_variants, variant_id) {
    Ok(variant) ->
      case product_is_deleted(store, variant.product_id) {
        True -> None
        False -> Some(variant)
      }
    Error(_) ->
      case dict.get(store.base_state.product_variants, variant_id) {
        Ok(variant) ->
          case
            product_is_deleted(store, variant.product_id)
            || has_staged_variant_family(store, variant.product_id)
          {
            True -> None
            False -> Some(variant)
          }
        Error(_) -> None
      }
  }
}

pub fn find_effective_variant_by_inventory_item_id(
  store: Store,
  inventory_item_id: String,
) -> Option(ProductVariantRecord) {
  case
    list.find(list_effective_product_variants(store), fn(variant) {
      case variant.inventory_item {
        Some(item) -> item.id == inventory_item_id
        None -> False
      }
    })
  {
    Ok(variant) -> Some(variant)
    Error(_) -> None
  }
}

pub fn find_effective_inventory_level_by_id(
  store: Store,
  inventory_level_id: String,
) -> Option(InventoryLevelRecord) {
  store
  |> list_effective_product_variants
  |> list.filter_map(fn(variant) {
    case variant.inventory_item {
      Some(item) ->
        case
          list.find(item.inventory_levels, fn(level) {
            level.id == inventory_level_id
          })
        {
          Ok(level) -> Ok(level)
          Error(_) -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
  |> list.first
  |> option.from_result
}

pub fn list_effective_product_variants(
  store: Store,
) -> List(ProductVariantRecord) {
  list_effective_products(store)
  |> list.flat_map(fn(product) {
    get_effective_variants_by_product_id(store, product.id)
  })
}

pub fn get_effective_product_variant_count(store: Store) -> Int {
  case store.staged_state.product_variant_count {
    Some(count) -> count
    None ->
      case store.base_state.product_variant_count {
        Some(count) -> count
        None -> list.length(list_effective_product_variants(store))
      }
  }
}

pub fn replace_base_options_for_product(
  store: Store,
  product_id: String,
  options: List(ProductOptionRecord),
) -> Store {
  let base = store.base_state
  let retained = remove_options_for_product(base.product_options, product_id)
  let next_options =
    list.fold(options, retained, fn(acc, option) {
      dict.insert(acc, option.id, option)
    })
  Store(..store, base_state: BaseState(..base, product_options: next_options))
}

pub fn replace_staged_options_for_product(
  store: Store,
  product_id: String,
  options: List(ProductOptionRecord),
) -> Store {
  let staged = store.staged_state
  let retained = remove_options_for_product(staged.product_options, product_id)
  let next_options =
    list.fold(options, retained, fn(acc, option) {
      dict.insert(acc, option.id, option)
    })
  Store(
    ..store,
    staged_state: StagedState(..staged, product_options: next_options),
  )
}

pub fn get_effective_options_by_product_id(
  store: Store,
  product_id: String,
) -> List(ProductOptionRecord) {
  case product_is_deleted(store, product_id) {
    True -> []
    False -> {
      let staged_options =
        store.staged_state.product_options
        |> dict.values()
        |> list.filter(fn(option) { option.product_id == product_id })
      let source_options = case staged_options {
        [] ->
          store.base_state.product_options
          |> dict.values()
          |> list.filter(fn(option) { option.product_id == product_id })
        _ -> staged_options
      }
      list.sort(source_options, compare_product_options)
    }
  }
}

pub fn get_effective_product_option_by_id(
  store: Store,
  option_id: String,
) -> Option(ProductOptionRecord) {
  store
  |> list_effective_products
  |> list.find_map(fn(product) {
    get_effective_options_by_product_id(store, product.id)
    |> list.find(fn(option) { option.id == option_id })
  })
  |> option.from_result
}

pub fn get_effective_product_option_value_by_id(
  store: Store,
  option_value_id: String,
) -> Option(ProductOptionValueRecord) {
  store
  |> list_effective_products
  |> list.find_map(fn(product) {
    get_effective_options_by_product_id(store, product.id)
    |> list.find_map(fn(option) {
      option.option_values
      |> list.find(fn(value) { value.id == option_value_id })
    })
  })
  |> option.from_result
}

pub fn stage_product_operation(
  store: Store,
  operation: ProductOperationRecord,
) -> #(ProductOperationRecord, Store) {
  let staged = store.staged_state
  let next_staged =
    StagedState(
      ..staged,
      product_operations: dict.insert(
        staged.product_operations,
        operation.id,
        operation,
      ),
    )
  #(operation, Store(..store, staged_state: next_staged))
}

pub fn upsert_base_product_operations(
  store: Store,
  operations: List(ProductOperationRecord),
) -> Store {
  list.fold(operations, store, fn(acc, operation) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        product_operations: dict.insert(
          base.product_operations,
          operation.id,
          operation,
        ),
      ),
    )
  })
}

pub fn get_effective_product_operation_by_id(
  store: Store,
  id: String,
) -> Option(ProductOperationRecord) {
  case dict.get(store.staged_state.product_operations, id) {
    Ok(operation) -> Some(operation)
    Error(_) ->
      case dict.get(store.base_state.product_operations, id) {
        Ok(operation) -> Some(operation)
        Error(_) -> None
      }
  }
}

pub fn upsert_base_selling_plan_groups(
  store: Store,
  groups: List(SellingPlanGroupRecord),
) -> Store {
  list.fold(groups, store, fn(acc, group) {
    let base = acc.base_state
    let staged = acc.staged_state
    let next_base =
      BaseState(
        ..base,
        selling_plan_groups: dict.insert(
          base.selling_plan_groups,
          group.id,
          group,
        ),
        selling_plan_group_order: append_unique_id(
          base.selling_plan_group_order,
          group.id,
        ),
        deleted_selling_plan_group_ids: dict.delete(
          base.deleted_selling_plan_group_ids,
          group.id,
        ),
      )
    let next_staged =
      StagedState(
        ..staged,
        deleted_selling_plan_group_ids: dict.delete(
          staged.deleted_selling_plan_group_ids,
          group.id,
        ),
      )
    Store(..acc, base_state: next_base, staged_state: next_staged)
  })
}

pub fn upsert_staged_selling_plan_group(
  store: Store,
  group: SellingPlanGroupRecord,
) -> #(SellingPlanGroupRecord, Store) {
  let base = store.base_state
  let staged = store.staged_state
  let already_known =
    list.contains(base.selling_plan_group_order, group.id)
    || list.contains(staged.selling_plan_group_order, group.id)
  let next_order = case already_known {
    True -> staged.selling_plan_group_order
    False -> list.append(staged.selling_plan_group_order, [group.id])
  }
  let next_staged =
    StagedState(
      ..staged,
      selling_plan_groups: dict.insert(
        staged.selling_plan_groups,
        group.id,
        group,
      ),
      selling_plan_group_order: next_order,
      deleted_selling_plan_group_ids: dict.delete(
        staged.deleted_selling_plan_group_ids,
        group.id,
      ),
    )
  #(group, Store(..store, staged_state: next_staged))
}

pub fn delete_staged_selling_plan_group(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      selling_plan_groups: dict.delete(staged.selling_plan_groups, id),
      deleted_selling_plan_group_ids: dict.insert(
        staged.deleted_selling_plan_group_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_selling_plan_group_by_id(
  store: Store,
  id: String,
) -> Option(SellingPlanGroupRecord) {
  let deleted =
    dict_has(store.base_state.deleted_selling_plan_group_ids, id)
    || dict_has(store.staged_state.deleted_selling_plan_group_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.selling_plan_groups, id) {
        Ok(group) -> Some(group)
        Error(_) ->
          case dict.get(store.base_state.selling_plan_groups, id) {
            Ok(group) -> Some(group)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_selling_plan_groups(
  store: Store,
) -> List(SellingPlanGroupRecord) {
  let ordered_ids =
    list.append(
      store.base_state.selling_plan_group_order,
      store.staged_state.selling_plan_group_order,
    )
    |> dedupe_strings()
  let ordered =
    list.filter_map(ordered_ids, fn(id) {
      get_effective_selling_plan_group_by_id(store, id) |> option_to_result
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.selling_plan_groups,
      store.staged_state.selling_plan_groups,
    )
  let unordered =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(resource_ids.compare_shopify_resource_ids)
    |> list.filter_map(fn(id) {
      get_effective_selling_plan_group_by_id(store, id) |> option_to_result
    })
  list.append(ordered, unordered)
}

pub fn list_effective_selling_plan_groups_for_product(
  store: Store,
  product_id: String,
) -> List(SellingPlanGroupRecord) {
  list_effective_selling_plan_groups(store)
  |> list.filter(fn(group) { list.contains(group.product_ids, product_id) })
}

pub fn list_effective_selling_plan_groups_visible_for_product(
  store: Store,
  product_id: String,
) -> List(SellingPlanGroupRecord) {
  list_effective_selling_plan_groups(store)
  |> list.filter(fn(group) {
    list.contains(group.product_ids, product_id)
    || list.any(group.product_variant_ids, fn(variant_id) {
      case get_effective_variant_by_id(store, variant_id) {
        Some(variant) -> variant.product_id == product_id
        None -> False
      }
    })
  })
}

pub fn list_effective_selling_plan_groups_for_product_variant(
  store: Store,
  variant_id: String,
) -> List(SellingPlanGroupRecord) {
  list_effective_selling_plan_groups(store)
  |> list.filter(fn(group) {
    list.contains(group.product_variant_ids, variant_id)
  })
}

pub fn list_effective_selling_plan_groups_visible_for_product_variant(
  store: Store,
  variant_id: String,
) -> List(SellingPlanGroupRecord) {
  let product_id = case get_effective_variant_by_id(store, variant_id) {
    Some(variant) -> Some(variant.product_id)
    None -> None
  }
  list_effective_selling_plan_groups(store)
  |> list.filter(fn(group) {
    list.contains(group.product_variant_ids, variant_id)
    || case product_id {
      Some(id) -> list.contains(group.product_ids, id)
      None -> False
    }
  })
}

fn remove_options_for_product(
  options: Dict(String, ProductOptionRecord),
  product_id: String,
) -> Dict(String, ProductOptionRecord) {
  options
  |> dict.keys()
  |> list.fold(options, fn(acc, id) {
    case dict.get(options, id) {
      Ok(option) ->
        case option.product_id == product_id {
          True -> dict.delete(acc, id)
          False -> acc
        }
      Error(_) -> acc
    }
  })
}

fn remove_variants_for_product(
  variants: Dict(String, ProductVariantRecord),
  product_id: String,
) -> Dict(String, ProductVariantRecord) {
  variants
  |> dict.keys()
  |> list.fold(variants, fn(acc, id) {
    case dict.get(variants, id) {
      Ok(variant) ->
        case variant.product_id == product_id {
          True -> dict.delete(acc, id)
          False -> acc
        }
      Error(_) -> acc
    }
  })
}

fn compare_product_options(
  left: ProductOptionRecord,
  right: ProductOptionRecord,
) -> order.Order {
  case int.compare(left.position, right.position) {
    order.Eq -> string.compare(left.id, right.id)
    other -> other
  }
}

fn product_is_deleted(store: Store, product_id: String) -> Bool {
  dict_has(store.base_state.deleted_product_ids, product_id)
  || dict_has(store.staged_state.deleted_product_ids, product_id)
}

fn has_staged_variant_family(store: Store, product_id: String) -> Bool {
  store.staged_state.product_variants
  |> dict.values()
  |> list.any(fn(variant) { variant.product_id == product_id })
}

fn list_variant_records(
  records: Dict(String, ProductVariantRecord),
  order: List(String),
) -> List(ProductVariantRecord) {
  let ordered_records =
    list.filter_map(order, fn(id) {
      case dict.get(records, id) {
        Ok(record) -> Ok(record)
        Error(_) -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(order)
  let unordered_records =
    dict.keys(records)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
    |> list.filter_map(fn(id) {
      case dict.get(records, id) {
        Ok(record) -> Ok(record)
        Error(_) -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}
