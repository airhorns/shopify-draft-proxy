use super::{DraftProxy, StagedRecords, StagedSortValue, Store};
use serde::ser::{
    Error as SerdeError, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant, Serializer,
};
use serde::Serialize;
use std::fmt;

pub(in crate::proxy) const SYNTHETIC_MARKER: &str = "shopify-draft-proxy=synthetic";
const SHOPIFY_GID_PREFIX: &str = "gid://shopify/";

pub(in crate::proxy) fn shopify_gid(resource_type: &str, id: impl std::fmt::Display) -> String {
    format!("{SHOPIFY_GID_PREFIX}{resource_type}/{id}")
}

pub(in crate::proxy) fn synthetic_shopify_gid(
    resource_type: &str,
    id: impl std::fmt::Display,
) -> String {
    format!("{}?{SYNTHETIC_MARKER}", shopify_gid(resource_type, id))
}

pub(in crate::proxy) fn is_synthetic_gid(id: &str) -> bool {
    has_shopify_gid_prefix(id) && id.contains(SYNTHETIC_MARKER)
}

pub(in crate::proxy) fn has_shopify_gid_prefix(id: &str) -> bool {
    id.starts_with(SHOPIFY_GID_PREFIX)
}

pub(in crate::proxy) fn resource_id_path_tail(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}

pub(in crate::proxy) fn resource_id_tail(id: &str) -> &str {
    resource_id_path_tail(id)
        .split('?')
        .next()
        .unwrap_or_default()
}

pub(in crate::proxy) fn shopify_gid_tail_for_type<'a>(
    id: &'a str,
    resource_type: &str,
) -> Option<&'a str> {
    typed_shopify_gid_tail(id, resource_type).filter(|tail| !tail.is_empty())
}

pub(in crate::proxy) fn is_shopify_gid_of_type(id: &str, resource_type: &str) -> bool {
    typed_shopify_gid_tail(id, resource_type).is_some()
}

fn shopify_gid_identity(id: &str) -> Option<(&str, &str)> {
    let rest = id.strip_prefix(SHOPIFY_GID_PREFIX)?;
    let (resource_type, resource_id) = rest.split_once('/')?;
    let tail = resource_id.split('?').next()?;
    (!resource_type.is_empty() && !tail.is_empty() && !tail.contains('/'))
        .then_some((resource_type, tail))
}

pub(in crate::proxy) fn shopify_gid_identities_overlap(left: &str, right: &str) -> bool {
    shopify_gid_identity(left)
        .zip(shopify_gid_identity(right))
        .is_some_and(|(left, right)| left == right)
}

#[derive(Clone, Copy)]
struct ShopifyGidIdentityScan<'a> {
    identities: &'a std::cell::RefCell<std::collections::BTreeSet<String>>,
}

#[derive(Debug)]
struct ShopifyGidIdentityFound;

impl fmt::Display for ShopifyGidIdentityFound {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Shopify GID identity found")
    }
}

impl std::error::Error for ShopifyGidIdentityFound {}

impl SerdeError for ShopifyGidIdentityFound {
    fn custom<T>(_message: T) -> Self
    where
        T: fmt::Display,
    {
        Self
    }
}

impl<'a> ShopifyGidIdentityScan<'a> {
    fn inspect<T>(self, value: &T) -> Result<(), ShopifyGidIdentityFound>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }
}

impl<'a> Serializer for ShopifyGidIdentityScan<'a> {
    type Ok = ();
    type Error = ShopifyGidIdentityFound;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, _value: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_i8(self, _value: i8) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_i16(self, _value: i16) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_i32(self, _value: i32) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_i64(self, _value: i64) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_u8(self, _value: u8) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_u16(self, _value: u16) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_u32(self, _value: u32) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_u64(self, _value: u64) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_f32(self, _value: f32) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_f64(self, _value: f64) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_char(self, value: char) -> Result<(), Self::Error> {
        self.serialize_str(&value.to_string())
    }

    fn serialize_str(self, value: &str) -> Result<(), Self::Error> {
        let mut remaining = value;
        while let Some(start) = remaining.find(SHOPIFY_GID_PREFIX) {
            let candidate = &remaining[start..];
            let rest = &candidate[SHOPIFY_GID_PREFIX.len()..];
            let Some((resource_type, resource_id)) = rest.split_once('/') else {
                break;
            };
            let tail_length = resource_id
                .find(|character: char| {
                    !character.is_ascii_alphanumeric() && character != '-' && character != '_'
                })
                .unwrap_or(resource_id.len());
            let tail = &resource_id[..tail_length];
            if !resource_type.is_empty()
                && resource_type.chars().all(|character| {
                    character.is_ascii_alphanumeric() || character == '-' || character == '_'
                })
                && !tail.is_empty()
            {
                self.identities
                    .borrow_mut()
                    .insert(format!("{resource_type}/{tail}"));
            }
            remaining = &candidate[SHOPIFY_GID_PREFIX.len()..];
        }
        if let Some((resource_type, tail)) = shopify_gid_identity(value) {
            self.identities
                .borrow_mut()
                .insert(format!("{resource_type}/{tail}"));
        }
        Ok(())
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_none(self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_some<T>(self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inspect(value)
    }

    fn serialize_unit(self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inspect(value)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inspect(value)
    }

    fn serialize_seq(self, _length: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple(self, _length: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(self)
    }

    fn serialize_map(self, _length: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(self)
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Ok(self)
    }

    fn collect_str<T>(self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + fmt::Display,
    {
        self.serialize_str(&value.to_string())
    }
}

macro_rules! impl_identity_scan_sequence {
    ($trait_name:ident, $method_name:ident) => {
        impl<'a> $trait_name for ShopifyGidIdentityScan<'a> {
            type Ok = ();
            type Error = ShopifyGidIdentityFound;

            fn $method_name<T>(&mut self, value: &T) -> Result<(), Self::Error>
            where
                T: ?Sized + Serialize,
            {
                self.inspect(value)
            }

            fn end(self) -> Result<(), Self::Error> {
                Ok(())
            }
        }
    };
}

impl_identity_scan_sequence!(SerializeSeq, serialize_element);
impl_identity_scan_sequence!(SerializeTuple, serialize_element);
impl_identity_scan_sequence!(SerializeTupleStruct, serialize_field);
impl_identity_scan_sequence!(SerializeTupleVariant, serialize_field);

impl<'a> SerializeMap for ShopifyGidIdentityScan<'a> {
    type Ok = ();
    type Error = ShopifyGidIdentityFound;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inspect(key)
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inspect(value)
    }

    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<'a> SerializeStruct for ShopifyGidIdentityScan<'a> {
    type Ok = ();
    type Error = ShopifyGidIdentityFound;

    fn serialize_field<T>(&mut self, _key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inspect(value)
    }

    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<'a> SerializeStructVariant for ShopifyGidIdentityScan<'a> {
    type Ok = ();
    type Error = ShopifyGidIdentityFound;

    fn serialize_field<T>(&mut self, _key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.inspect(value)
    }

    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub(in crate::proxy) fn shopify_gid_resource_type(id: &str) -> Option<&str> {
    let rest = id.strip_prefix(SHOPIFY_GID_PREFIX)?;
    let (resource_type, resource_id) = rest.split_once('/')?;
    (!resource_type.is_empty() && !resource_id.is_empty()).then_some(resource_type)
}

pub(in crate::proxy) fn staged_record_key_for_shopify_gid<T>(
    records: &StagedRecords<T>,
    submitted_id: &str,
    resource_type: &str,
) -> Option<String> {
    if records.records.contains_key(submitted_id) || records.tombstones.contains(submitted_id) {
        return Some(submitted_id.to_string());
    }

    let tail = unmarked_shopify_gid_tail_for_type(submitted_id, resource_type)?;
    records
        .records
        .keys()
        .chain(records.tombstones.iter())
        .find(|candidate| staged_synthetic_key_matches_tail(candidate, resource_type, tail))
        .cloned()
}

fn unmarked_shopify_gid_tail_for_type<'a>(id: &'a str, resource_type: &str) -> Option<&'a str> {
    let tail = typed_shopify_gid_tail(id, resource_type)?;
    (!tail.is_empty() && !tail.contains('/') && !tail.contains('?')).then_some(tail)
}

fn staged_synthetic_key_matches_tail(candidate: &str, resource_type: &str, tail: &str) -> bool {
    is_synthetic_gid(candidate)
        && shopify_gid_tail_for_type(candidate, resource_type)
            .is_some_and(|candidate_tail| resource_id_tail(candidate_tail) == tail)
}

fn typed_shopify_gid_tail<'a>(id: &'a str, resource_type: &str) -> Option<&'a str> {
    let rest = id.strip_prefix(SHOPIFY_GID_PREFIX)?;
    let (candidate_type, tail) = rest.split_once('/')?;
    (candidate_type == resource_type).then_some(tail)
}

pub(in crate::proxy) fn resource_id_tail_sort_value(id: Option<&str>) -> StagedSortValue {
    let tail = id.map(resource_id_tail).unwrap_or_default();
    tail.parse::<i64>()
        .map(StagedSortValue::I64)
        .unwrap_or_else(|_| StagedSortValue::String(tail.to_ascii_lowercase()))
}

pub(in crate::proxy) fn resource_id_matches_gid_or_tail(id: &str, value: &str) -> bool {
    id == value || resource_id_tail(id) == value || resource_id_path_tail(id) == value
}

pub(in crate::proxy) fn metafield_owner_gid_resource_type(id: &str) -> String {
    shopify_gid_resource_type(id).unwrap_or(id).to_string()
}

impl Store {
    fn refresh_synthetic_identity_cache(&mut self, log_entries: &[serde_json::Value]) {
        if self.synthetic_identity_cache_current.get() {
            return;
        }

        ShopifyGidIdentityScan {
            identities: &self.synthetic_identities,
        }
        .inspect(&*self)
        .expect("proxy store identity state should serialize");
        ShopifyGidIdentityScan {
            identities: &self.synthetic_identities,
        }
        .inspect(log_entries)
        .expect("proxy mutation log identity state should serialize");
        self.synthetic_identity_cache_current.set(true);
    }

    pub(in crate::proxy) fn observe_shopify_gid_identities<T>(&self, value: &T)
    where
        T: ?Sized + Serialize,
    {
        ShopifyGidIdentityScan {
            identities: &self.synthetic_identities,
        }
        .inspect(value)
        .expect("proxy request identity state should serialize");
    }

    fn broker_shopify_gid<Format>(
        &mut self,
        resource_type: &str,
        log_entries: &[serde_json::Value],
        format: Format,
    ) -> String
    where
        Format: Fn(&str, u64) -> String,
    {
        self.refresh_synthetic_identity_cache(log_entries);
        loop {
            let id = self.next_synthetic_id;
            self.next_synthetic_id = id
                .checked_add(1)
                .expect("proxy synthetic identity sequence exhausted");
            let identity = format!("{resource_type}/{id}");
            if self.synthetic_identities.borrow_mut().insert(identity) {
                return format(resource_type, id);
            }
        }
    }

    fn broker_proxy_synthetic_gid(
        &mut self,
        resource_type: &str,
        log_entries: &[serde_json::Value],
    ) -> String {
        self.broker_shopify_gid(resource_type, log_entries, synthetic_shopify_gid)
    }

    fn broker_plain_shopify_gid(
        &mut self,
        resource_type: &str,
        log_entries: &[serde_json::Value],
    ) -> String {
        self.broker_shopify_gid(resource_type, log_entries, |resource_type, id| {
            shopify_gid(resource_type, id)
        })
    }

    pub(in crate::proxy) fn synthetic_id_sequence(&self) -> u64 {
        self.next_synthetic_id
    }

    pub(in crate::proxy) fn reset_synthetic_id_sequence(&mut self) {
        self.next_synthetic_id = 1;
        self.synthetic_identity_cache_current.set(false);
        self.synthetic_identities.borrow_mut().clear();
    }

    pub(in crate::proxy) fn restore_synthetic_id_sequence(&mut self, next_id: u64) {
        debug_assert!(next_id > 0);
        self.next_synthetic_id = next_id;
        self.synthetic_identity_cache_current.set(false);
        self.synthetic_identities.borrow_mut().clear();
    }

    pub(in crate::proxy) fn invalidate_synthetic_identity_cache(&self) {
        self.synthetic_identity_cache_current.set(false);
    }

    pub(in crate::proxy) fn reserve_synthetic_id(&mut self) {
        self.next_synthetic_id = self
            .next_synthetic_id
            .checked_add(1)
            .expect("proxy synthetic identity sequence exhausted");
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn next_proxy_synthetic_gid(&mut self, resource_type: &str) -> String {
        self.store
            .broker_proxy_synthetic_gid(resource_type, &self.log_entries)
    }

    /// Mint a plain `gid://shopify/<type>/<id>` without the proxy-synthetic
    /// marker. Used for
    /// entities (e.g. media files) the proxy fabricates with stable identifiers
    /// rather than commit-rewritten placeholders.
    pub(in crate::proxy) fn next_synthetic_gid(&mut self, resource_type: &str) -> String {
        self.store
            .broker_plain_shopify_gid(resource_type, &self.log_entries)
    }

    /// Reserve a synthetic id for a mutation-log entry at the start of every successful mutation. This keeps entity ids in lockstep with the current synthetic-id contract: each mutation advances the counter once for its log entry before allocating the resources it creates.
    pub(in crate::proxy) fn reserve_synthetic_log_id(&mut self) {
        self.store.reserve_synthetic_id();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::ProductRecord;
    use serde_json::{json, Value};

    #[test]
    fn builds_plain_and_synthetic_shopify_gids() {
        assert_eq!(shopify_gid("Product", 42), "gid://shopify/Product/42");
        assert_eq!(
            synthetic_shopify_gid("Product", 42),
            "gid://shopify/Product/42?shopify-draft-proxy=synthetic"
        );
        assert!(shopify_gid_identities_overlap(
            "gid://shopify/Product/42",
            "gid://shopify/Product/42?shopify-draft-proxy=synthetic"
        ));
        assert!(!shopify_gid_identities_overlap(
            "gid://shopify/Product/42",
            "gid://shopify/Customer/42?shopify-draft-proxy=synthetic"
        ));
        assert!(!shopify_gid_identities_overlap(
            "gid://shopify/Market/42",
            "gid://shopify/Market/Region/42"
        ));
    }

    #[test]
    fn store_broker_is_monotonic_across_domains_state_and_logs() {
        let mut store = Store::default();
        store.products.base.insert(
            "gid://shopify/Product/1".to_string(),
            ProductRecord {
                id: "gid://shopify/Product/1".to_string(),
                ..ProductRecord::default()
            },
        );
        store
            .staged
            .deleted_market_ids
            .insert("gid://shopify/Market/3?shopify-draft-proxy=synthetic".to_string());
        store.staged.inventory_level_ids.insert(
            (
                "gid://shopify/InventoryItem/100".to_string(),
                "gid://shopify/Location/8".to_string(),
            ),
            "gid://shopify/InventoryLevel/100?inventory_item_id=100&location_id=8".to_string(),
        );
        let log_entries = vec![json!({
            "stagedResourceIds": ["gid://shopify/MarketCatalog/5"]
        })];

        assert_eq!(
            store.broker_proxy_synthetic_gid("Product", &log_entries),
            "gid://shopify/Product/2?shopify-draft-proxy=synthetic"
        );
        assert_eq!(
            store.broker_proxy_synthetic_gid("Market", &log_entries),
            "gid://shopify/Market/4?shopify-draft-proxy=synthetic"
        );
        assert_eq!(
            store.broker_proxy_synthetic_gid("MarketCatalog", &log_entries),
            "gid://shopify/MarketCatalog/6?shopify-draft-proxy=synthetic"
        );
        assert_eq!(
            store.broker_plain_shopify_gid("Customer", &log_entries),
            "gid://shopify/Customer/7"
        );
        assert_eq!(
            store.broker_proxy_synthetic_gid("Location", &log_entries),
            "gid://shopify/Location/9?shopify-draft-proxy=synthetic"
        );
        assert_eq!(store.synthetic_id_sequence(), 10);
    }

    #[test]
    fn store_broker_reserves_shopify_gids_embedded_in_request_json() {
        let mut store = Store::default();
        store.observe_shopify_gid_identities(
            r#"{"query":"mutation { node(id: \"gid://shopify/Market/1\") { id } }","variables":{"id":"gid://shopify/Market/2?shopify-draft-proxy=synthetic"}}"#,
        );

        assert_eq!(
            store.broker_proxy_synthetic_gid("Market", &[]),
            "gid://shopify/Market/3?shopify-draft-proxy=synthetic"
        );
    }

    #[test]
    fn extracts_resource_id_tails_with_and_without_query_strings() {
        assert_eq!(resource_id_path_tail("gid://shopify/Product/42"), "42");
        assert_eq!(
            resource_id_path_tail("gid://shopify/Product/42?shopify-draft-proxy=synthetic"),
            "42?shopify-draft-proxy=synthetic"
        );
        assert_eq!(
            resource_id_tail("gid://shopify/Product/42?shopify-draft-proxy=synthetic"),
            "42"
        );
        assert_eq!(resource_id_tail("42"), "42");
    }

    #[test]
    fn extracts_type_checked_shopify_gid_tails() {
        assert_eq!(
            shopify_gid_tail_for_type("gid://shopify/Product/42", "Product"),
            Some("42")
        );
        assert_eq!(
            shopify_gid_tail_for_type(
                "gid://shopify/Product/42?shopify-draft-proxy=synthetic",
                "Product"
            ),
            Some("42?shopify-draft-proxy=synthetic")
        );
        assert_eq!(
            shopify_gid_tail_for_type("gid://shopify/Product/42", "Customer"),
            None
        );
        assert!(is_shopify_gid_of_type(
            "gid://shopify/Product/42",
            "Product"
        ));
        assert!(is_shopify_gid_of_type("gid://shopify/Product/", "Product"));
        assert_eq!(
            shopify_gid_tail_for_type("gid://shopify/Product/", "Product"),
            None
        );
        assert!(has_shopify_gid_prefix("gid://shopify/"));
    }

    #[test]
    fn compares_ids_against_full_gid_tail_and_path_tail() {
        let synthetic = "gid://shopify/Product/42?shopify-draft-proxy=synthetic";
        assert!(resource_id_matches_gid_or_tail(synthetic, synthetic));
        assert!(resource_id_matches_gid_or_tail(synthetic, "42"));
        assert!(resource_id_matches_gid_or_tail(
            synthetic,
            "42?shopify-draft-proxy=synthetic"
        ));
        assert!(!resource_id_matches_gid_or_tail(synthetic, "43"));
    }

    #[test]
    fn sorts_gid_tails_as_numeric_then_lowercase_string() {
        assert_eq!(
            resource_id_tail_sort_value(Some("gid://shopify/Product/42")),
            StagedSortValue::I64(42)
        );
        assert_eq!(
            resource_id_tail_sort_value(Some("gid://shopify/Product/abc")),
            StagedSortValue::String("abc".to_string())
        );
        assert_eq!(
            resource_id_tail_sort_value(Some("gid://shopify/Product/ABC")),
            StagedSortValue::String("abc".to_string())
        );
    }

    #[test]
    fn extracts_shopify_gid_resource_types_only_for_complete_shopify_gids() {
        assert_eq!(
            shopify_gid_resource_type("gid://shopify/Customer/123"),
            Some("Customer")
        );
        assert_eq!(
            shopify_gid_resource_type("gid://shopify/Customer/123?shopify-draft-proxy=synthetic"),
            Some("Customer")
        );
        assert_eq!(shopify_gid_resource_type("gid://shopify/Customer/"), None);
        assert_eq!(shopify_gid_resource_type("not-a-gid"), None);
    }

    #[test]
    fn detects_synthetic_shopify_gids() {
        assert!(is_synthetic_gid(
            "gid://shopify/Product/42?shopify-draft-proxy=synthetic"
        ));
        assert!(!is_synthetic_gid("gid://shopify/Product/42"));
        assert!(!is_synthetic_gid("not-a-gid?shopify-draft-proxy=synthetic"));
    }

    #[test]
    fn maps_metafield_owner_gid_types_without_collapsing_unknown_resource_types() {
        assert_eq!(
            metafield_owner_gid_resource_type("gid://shopify/ProductVariant/1"),
            "ProductVariant"
        );
        assert_eq!(
            metafield_owner_gid_resource_type("gid://shopify/Company/1"),
            "Company"
        );
        assert_eq!(
            metafield_owner_gid_resource_type("gid://shopify/Unknown/1"),
            "Unknown"
        );
        assert_eq!(metafield_owner_gid_resource_type("not-a-gid"), "not-a-gid");
    }

    fn staged_records_with_ids(ids: &[&str]) -> StagedRecords<Value> {
        let mut records = StagedRecords::default();
        for id in ids {
            records.insert((*id).to_string(), json!({"id": id}));
        }
        records
    }

    #[test]
    fn resolves_exact_staged_keys_before_canonical_synthetic_fallback() {
        let synthetic = synthetic_shopify_gid("Metaobject", 42);
        let canonical = shopify_gid("Metaobject", 42);
        let records = staged_records_with_ids(&[&synthetic, &canonical]);

        assert_eq!(
            staged_record_key_for_shopify_gid(&records, &synthetic, "Metaobject"),
            Some(synthetic.clone())
        );
        assert_eq!(
            staged_record_key_for_shopify_gid(&records, &canonical, "Metaobject"),
            Some(canonical)
        );
    }

    #[test]
    fn resolves_unmarked_canonical_gid_to_staged_synthetic_key() {
        let synthetic = synthetic_shopify_gid("Metaobject", 42);
        let canonical = shopify_gid("Metaobject", 42);
        let records = staged_records_with_ids(&[&synthetic]);

        assert_eq!(
            staged_record_key_for_shopify_gid(&records, &canonical, "Metaobject"),
            Some(synthetic)
        );
    }

    #[test]
    fn resolves_unmarked_canonical_gid_to_synthetic_tombstone_key() {
        let synthetic = synthetic_shopify_gid("Metaobject", 42);
        let canonical = shopify_gid("Metaobject", 42);
        let mut records = staged_records_with_ids(&[&synthetic]);
        records.remove(&synthetic);
        records.tombstone(synthetic.clone());

        assert_eq!(
            staged_record_key_for_shopify_gid(&records, &canonical, "Metaobject"),
            Some(synthetic)
        );
    }

    #[test]
    fn resolves_exact_tombstone_before_canonical_synthetic_fallback() {
        let synthetic = synthetic_shopify_gid("Metaobject", 42);
        let canonical = shopify_gid("Metaobject", 42);
        let mut records = staged_records_with_ids(&[&synthetic]);
        records.tombstone(canonical.clone());

        assert_eq!(
            staged_record_key_for_shopify_gid(&records, &canonical, "Metaobject"),
            Some(canonical)
        );
    }

    #[test]
    fn rejects_noncanonical_or_wrong_type_staged_key_fallbacks() {
        let synthetic = synthetic_shopify_gid("Metaobject", 42);
        let definition_synthetic = synthetic_shopify_gid("MetaobjectDefinition", 42);
        let records = staged_records_with_ids(&[&synthetic, &definition_synthetic]);

        for rejected in [
            "gid://shopify/Metaobject/43?shopify-draft-proxy=synthetic",
            "gid://shopify/Metaobject/42?other=query",
            "gid://shopify/Metaobject/",
            "gid://shopify/Metaobject/42/extra",
            "gid://shopify/Product/42",
            "42",
            "not-a-gid",
            "gid://shopify/",
        ] {
            assert_eq!(
                staged_record_key_for_shopify_gid(&records, rejected, "Metaobject"),
                None,
                "{rejected} should not resolve by fallback"
            );
        }
        assert_eq!(
            staged_record_key_for_shopify_gid(
                &records,
                "gid://shopify/Metaobject/43",
                "Metaobject"
            ),
            None
        );
        assert_eq!(
            staged_record_key_for_shopify_gid(
                &records,
                "gid://shopify/Metaobject/42",
                "MetaobjectDefinition",
            ),
            None
        );
    }
}
