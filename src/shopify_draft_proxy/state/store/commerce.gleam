//// Store operations for commerce and publication records.

import gleam/dict
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store/products
import shopify_draft_proxy/state/store/shared.{
  append_unique_id, channel_from_publication, dedupe_strings, dict_has,
  list_to_set, string_compare,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type ChannelRecord, type CollectionRecord, type LocationRecord,
  type ProductFeedRecord, type ProductResourceFeedbackRecord,
  type PublicationRecord, type ShopResourceFeedbackRecord, PublicationRecord,
} as _

pub fn upsert_base_collections(
  store: Store,
  records: List(CollectionRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        collections: dict.insert(base.collections, record.id, record),
        collection_order: append_unique_id(base.collection_order, record.id),
        deleted_collection_ids: dict.delete(
          base.deleted_collection_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_collection_ids: dict.delete(
          staged.deleted_collection_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn upsert_staged_collections(
  store: Store,
  records: List(CollectionRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let staged = acc.staged_state
    let new_staged =
      StagedState(
        ..staged,
        collections: dict.insert(staged.collections, record.id, record),
        collection_order: append_unique_id(staged.collection_order, record.id),
        deleted_collection_ids: dict.delete(
          staged.deleted_collection_ids,
          record.id,
        ),
      )
    Store(..acc, staged_state: new_staged)
  })
}

pub fn delete_staged_collection(store: Store, id: String) -> Store {
  let base = store.base_state
  let staged = store.staged_state
  let base_product_collections =
    base.product_collections
    |> dict.keys()
    |> list.fold(base.product_collections, fn(acc, key) {
      case dict.get(acc, key) {
        Ok(record) ->
          case record.collection_id == id {
            True -> dict.delete(acc, key)
            False -> acc
          }
        Error(_) -> acc
      }
    })
  let staged_product_collections =
    staged.product_collections
    |> dict.keys()
    |> list.fold(staged.product_collections, fn(acc, key) {
      case dict.get(acc, key) {
        Ok(record) ->
          case record.collection_id == id {
            True -> dict.delete(acc, key)
            False -> acc
          }
        Error(_) -> acc
      }
    })
  let new_base =
    BaseState(..base, product_collections: base_product_collections)
  let new_staged =
    StagedState(
      ..staged,
      collections: dict.delete(staged.collections, id),
      product_collections: staged_product_collections,
      deleted_collection_ids: dict.insert(
        staged.deleted_collection_ids,
        id,
        True,
      ),
    )
  Store(..store, base_state: new_base, staged_state: new_staged)
}

pub fn upsert_base_locations(
  store: Store,
  records: List(LocationRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        locations: dict.insert(base.locations, record.id, record),
        location_order: append_unique_id(base.location_order, record.id),
      ),
    )
  })
}

pub fn get_effective_location_by_id(
  store: Store,
  id: String,
) -> Option(LocationRecord) {
  case dict.get(store.base_state.locations, id) {
    Ok(record) -> Some(record)
    Error(_) -> None
  }
}

pub fn list_effective_locations(store: Store) -> List(LocationRecord) {
  let ordered_records =
    list.filter_map(store.base_state.location_order, fn(id) {
      case get_effective_location_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(store.base_state.location_order)
  let unordered =
    dict.keys(store.base_state.locations)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
    |> list.filter_map(fn(id) {
      case get_effective_location_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered)
}

pub fn upsert_base_publications(
  store: Store,
  records: List(PublicationRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        publications: dict.insert(base.publications, record.id, record),
        publication_order: append_unique_id(base.publication_order, record.id),
        deleted_publication_ids: dict.delete(
          base.deleted_publication_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_publication_ids: dict.delete(
          staged.deleted_publication_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn upsert_base_channels(
  store: Store,
  records: List(ChannelRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        channels: dict.insert(base.channels, record.id, record),
        channel_order: append_unique_id(base.channel_order, record.id),
      ),
    )
  })
}

pub fn upsert_staged_publication(
  store: Store,
  record: PublicationRecord,
) -> #(PublicationRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.publication_order, record.id)
    || list.contains(staged.publication_order, record.id)
  let new_order = case already_known {
    True -> staged.publication_order
    False -> list.append(staged.publication_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      publications: dict.insert(staged.publications, record.id, record),
      publication_order: new_order,
      deleted_publication_ids: dict.delete(
        staged.deleted_publication_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_publication(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      publications: dict.delete(staged.publications, id),
      deleted_publication_ids: dict.insert(
        staged.deleted_publication_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_publication_by_id(
  store: Store,
  id: String,
) -> Option(PublicationRecord) {
  let deleted =
    dict_has(store.base_state.deleted_publication_ids, id)
    || dict_has(store.staged_state.deleted_publication_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.publications, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.publications, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_publications(store: Store) -> List(PublicationRecord) {
  let ordered_ids =
    list.append(
      store.base_state.publication_order,
      store.staged_state.publication_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_publication_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.publications, store.staged_state.publications)
  let relation_ids =
    list.append(
      list.flat_map(products.list_effective_products(store), fn(product) {
        product.publication_ids
      }),
      list.flat_map(products.list_effective_collections(store), fn(collection) {
        collection.publication_ids
      }),
    )
  let relation_publications =
    relation_ids
    |> dedupe_strings()
    |> list.filter(fn(id) {
      !dict_has(ordered_set, id)
      && !dict_has(merged, id)
      && !dict_has(store.base_state.deleted_publication_ids, id)
      && !dict_has(store.staged_state.deleted_publication_ids, id)
    })
    |> list.map(fn(id) {
      PublicationRecord(
        id: id,
        name: None,
        auto_publish: None,
        supports_future_publishing: None,
        catalog_id: None,
        channel_id: None,
        cursor: None,
      )
    })
  let unordered =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(resource_ids.compare_shopify_resource_ids)
    |> list.filter_map(fn(id) {
      case get_effective_publication_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(list.append(ordered_records, unordered), relation_publications)
}

pub fn get_effective_channel_by_id(
  store: Store,
  id: String,
) -> Option(ChannelRecord) {
  case dict.get(store.base_state.channels, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case
        list.find(list_effective_channels(store), fn(channel) {
          channel.id == id
        })
      {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn list_effective_channels(store: Store) -> List(ChannelRecord) {
  let direct_channels =
    list.filter_map(store.base_state.channel_order, fn(id) {
      case dict.get(store.base_state.channels, id) {
        Ok(record) -> Ok(record)
        Error(_) -> Error(Nil)
      }
    })
  let direct_set =
    list_to_set(list.map(direct_channels, fn(channel) { channel.id }))
  let derived =
    list_effective_publications(store)
    |> list.filter_map(fn(publication) {
      case channel_from_publication(publication) {
        Some(channel) ->
          case dict_has(direct_set, channel.id) {
            True -> Error(Nil)
            False -> Ok(channel)
          }
        None -> Error(Nil)
      }
    })
  list.append(direct_channels, derived)
  |> list.sort(fn(left, right) {
    resource_ids.compare_shopify_resource_ids(left.id, right.id)
  })
}

pub fn upsert_base_product_feeds(
  store: Store,
  records: List(ProductFeedRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        product_feeds: dict.insert(base.product_feeds, record.id, record),
        product_feed_order: append_unique_id(base.product_feed_order, record.id),
        deleted_product_feed_ids: dict.delete(
          base.deleted_product_feed_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_product_feed_ids: dict.delete(
          staged.deleted_product_feed_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn upsert_staged_product_feed(
  store: Store,
  record: ProductFeedRecord,
) -> #(ProductFeedRecord, Store) {
  let base = store.base_state
  let staged = store.staged_state
  let already_known =
    list.contains(base.product_feed_order, record.id)
    || list.contains(staged.product_feed_order, record.id)
  let new_order = case already_known {
    True -> staged.product_feed_order
    False -> list.append(staged.product_feed_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      product_feeds: dict.insert(staged.product_feeds, record.id, record),
      product_feed_order: new_order,
      deleted_product_feed_ids: dict.delete(
        staged.deleted_product_feed_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_product_feed(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      product_feeds: dict.delete(staged.product_feeds, id),
      deleted_product_feed_ids: dict.insert(
        staged.deleted_product_feed_ids,
        id,
        True,
      ),
    ),
  )
}

pub fn get_effective_product_feed_by_id(
  store: Store,
  id: String,
) -> Option(ProductFeedRecord) {
  let deleted =
    dict_has(store.base_state.deleted_product_feed_ids, id)
    || dict_has(store.staged_state.deleted_product_feed_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.product_feeds, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.product_feeds, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_product_feeds(store: Store) -> List(ProductFeedRecord) {
  let ordered_ids =
    list.append(
      store.base_state.product_feed_order,
      store.staged_state.product_feed_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_product_feed_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.product_feeds, store.staged_state.product_feeds)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_product_feed_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn upsert_base_product_resource_feedback(
  store: Store,
  records: List(ProductResourceFeedbackRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        product_resource_feedback: dict.insert(
          base.product_resource_feedback,
          record.product_id,
          record,
        ),
      ),
    )
  })
}

pub fn upsert_staged_product_resource_feedback(
  store: Store,
  record: ProductResourceFeedbackRecord,
) -> #(ProductResourceFeedbackRecord, Store) {
  let staged = store.staged_state
  let next_staged =
    StagedState(
      ..staged,
      product_resource_feedback: dict.insert(
        staged.product_resource_feedback,
        record.product_id,
        record,
      ),
    )
  #(record, Store(..store, staged_state: next_staged))
}

pub fn get_effective_product_resource_feedback(
  store: Store,
  product_id: String,
) -> Option(ProductResourceFeedbackRecord) {
  case dict.get(store.staged_state.product_resource_feedback, product_id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.product_resource_feedback, product_id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn upsert_staged_shop_resource_feedback(
  store: Store,
  record: ShopResourceFeedbackRecord,
) -> #(ShopResourceFeedbackRecord, Store) {
  let staged = store.staged_state
  let next_staged =
    StagedState(
      ..staged,
      shop_resource_feedback: dict.insert(
        staged.shop_resource_feedback,
        record.id,
        record,
      ),
    )
  #(record, Store(..store, staged_state: next_staged))
}
