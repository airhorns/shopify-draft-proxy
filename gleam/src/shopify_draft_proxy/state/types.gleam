//// Mirrors the slices of `src/state/types.ts` that the Gleam port
//// currently exercises. Only resource types this port knows about are
//// included; everything else is deliberately deferred until the
//// corresponding domain handler lands.
////
//// Putting the resource records here (rather than in either the
//// `state/store` or `proxy/saved_searches` module) avoids a circular
//// import: the store needs to know the shapes of the records it stores,
//// and the domain handler needs to read them back; both depend on this
//// module.

import gleam/option.{type Option}

/// A single saved-search record. Mirrors `SavedSearchRecord` in
/// `src/state/types.ts`. `cursor` is set on records the proxy stages
/// from upstream-hybrid responses; static defaults and freshly-created
/// records carry `None`.
pub type SavedSearchRecord {
  SavedSearchRecord(
    id: String,
    legacy_resource_id: String,
    name: String,
    query: String,
    resource_type: String,
    search_terms: String,
    filters: List(SavedSearchFilter),
    cursor: Option(String),
  )
}

/// One key/value filter on a saved search. Mirrors
/// `SavedSearchRecord['filters'][number]`.
pub type SavedSearchFilter {
  SavedSearchFilter(key: String, value: String)
}
