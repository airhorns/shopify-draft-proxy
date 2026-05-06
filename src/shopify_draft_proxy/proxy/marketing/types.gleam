//// Shared internal marketing constants and types.

import shopify_draft_proxy/state/types.{type MarketingRecord}

@internal
pub type MarketingKind {
  ActivityKind
  EventKind
}

@internal
pub const activity_id_prefix: String = "gid://shopify/MarketingActivity/"

@internal
pub const event_id_prefix: String = "gid://shopify/MarketingEvent/"

@internal
pub type CollectedMarketingRecords {
  CollectedMarketingRecords(
    activities: List(MarketingRecord),
    events: List(MarketingRecord),
  )
}

@internal
pub type MarketingConnectionItem {
  MarketingConnectionItem(
    record: MarketingRecord,
    pagination_cursor: String,
    output_cursor: String,
  )
}

@internal
pub fn is_marketing_mutation_root(name: String) -> Bool {
  case name {
    "marketingActivityCreate" -> True
    "marketingActivityUpdate" -> True
    "marketingActivityCreateExternal" -> True
    "marketingActivityUpdateExternal" -> True
    "marketingActivityUpsertExternal" -> True
    "marketingActivityDeleteExternal" -> True
    "marketingActivitiesDeleteAllExternal" -> True
    "marketingEngagementCreate" -> True
    "marketingEngagementsDelete" -> True
    _ -> False
  }
}
