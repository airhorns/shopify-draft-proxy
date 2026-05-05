import shopify_draft_proxy/state/synthetic_identity.{
  InvalidSyntheticId, InvalidSyntheticTimestamp, SyntheticIdentityStateDumpV1,
}

pub fn make_synthetic_gid_starts_at_one_test() {
  let r = synthetic_identity.new()
  let #(gid, _) = synthetic_identity.make_synthetic_gid(r, "Product")
  assert gid == "gid://shopify/Product/1"
}

pub fn make_synthetic_gid_increments_test() {
  let r = synthetic_identity.new()
  let #(_a, r) = synthetic_identity.make_synthetic_gid(r, "Product")
  let #(b, r) = synthetic_identity.make_synthetic_gid(r, "Product")
  let #(c, _) = synthetic_identity.make_synthetic_gid(r, "Order")
  assert b == "gid://shopify/Product/2"
  assert c == "gid://shopify/Order/3"
}

pub fn make_proxy_synthetic_gid_includes_marker_test() {
  let r = synthetic_identity.new()
  let #(gid, _) = synthetic_identity.make_proxy_synthetic_gid(r, "Product")
  assert gid == "gid://shopify/Product/1?shopify-draft-proxy=synthetic"
}

pub fn make_synthetic_timestamp_starts_at_2024_test() {
  let r = synthetic_identity.new()
  let #(ts, _) = synthetic_identity.make_synthetic_timestamp(r)
  assert ts == "2024-01-01T00:00:00.000Z"
}

pub fn make_synthetic_timestamp_increments_by_one_second_test() {
  let r = synthetic_identity.new()
  let #(_a, r) = synthetic_identity.make_synthetic_timestamp(r)
  let #(b, r) = synthetic_identity.make_synthetic_timestamp(r)
  let #(c, _) = synthetic_identity.make_synthetic_timestamp(r)
  assert b == "2024-01-01T00:00:01.000Z"
  assert c == "2024-01-01T00:00:02.000Z"
}

pub fn reset_returns_fresh_state_test() {
  let r = synthetic_identity.new()
  let #(_, r) = synthetic_identity.make_synthetic_gid(r, "X")
  let #(_, r) = synthetic_identity.make_synthetic_timestamp(r)
  let r = synthetic_identity.reset(r)
  let #(gid, _) = synthetic_identity.make_synthetic_gid(r, "X")
  assert gid == "gid://shopify/X/1"
}

pub fn dump_state_after_increments_test() {
  let r = synthetic_identity.new()
  let #(_, r) = synthetic_identity.make_synthetic_gid(r, "X")
  let #(_, r) = synthetic_identity.make_synthetic_gid(r, "Y")
  let #(_, r) = synthetic_identity.make_synthetic_timestamp(r)
  let dump = synthetic_identity.dump_state(r)
  assert dump.next_synthetic_id == 3
  assert dump.next_synthetic_timestamp == "2024-01-01T00:00:01.000Z"
}

pub fn restore_state_round_trips_test() {
  let r = synthetic_identity.new()
  let #(_, r) = synthetic_identity.make_synthetic_gid(r, "X")
  let #(_, r) = synthetic_identity.make_synthetic_timestamp(r)
  let dump = synthetic_identity.dump_state(r)
  let assert Ok(restored) = synthetic_identity.restore_state(dump)

  // Continuing from a restored registry should mint the next id and
  // timestamp the original would have produced.
  let #(gid, _) = synthetic_identity.make_synthetic_gid(restored, "X")
  let #(ts, _) = synthetic_identity.make_synthetic_timestamp(restored)
  assert gid == "gid://shopify/X/2"
  assert ts == "2024-01-01T00:00:01.000Z"
}

pub fn restore_state_rejects_zero_id_test() {
  let dump =
    SyntheticIdentityStateDumpV1(
      next_synthetic_id: 0,
      next_synthetic_timestamp: "2024-01-01T00:00:00.000Z",
    )
  let assert Error(InvalidSyntheticId(0)) =
    synthetic_identity.restore_state(dump)
}

pub fn restore_state_rejects_invalid_timestamp_test() {
  let dump =
    SyntheticIdentityStateDumpV1(
      next_synthetic_id: 1,
      next_synthetic_timestamp: "not-a-date",
    )
  let assert Error(InvalidSyntheticTimestamp(_)) =
    synthetic_identity.restore_state(dump)
}

pub fn is_proxy_synthetic_gid_detects_marker_test() {
  assert synthetic_identity.is_proxy_synthetic_gid(
      "gid://shopify/Product/1?shopify-draft-proxy=synthetic",
    )
    == True
  assert synthetic_identity.is_proxy_synthetic_gid("gid://shopify/Product/1")
    == False
  assert synthetic_identity.is_proxy_synthetic_gid("not-a-gid") == False
}
