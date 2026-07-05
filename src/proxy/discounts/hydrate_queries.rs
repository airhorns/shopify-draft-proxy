macro_rules! discount_hydrate_query {
    ($code_free_shipping_extra:literal, $automatic_free_shipping_extra:literal) => {
        concat!(
            r#"#graphql
  query DiscountHydrate($id: ID!) {
    codeNode: codeDiscountNode(id: $id) {
      id
      codeDiscount {
        __typename
        ... on DiscountCodeBasic {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
          codes(first: 250) {
            nodes {
              id
              code
              asyncUsageCount
            }
          }
        }
        ... on DiscountCodeApp {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountCodeBxgy {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountCodeFreeShipping {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
"#,
            $code_free_shipping_extra,
            r#"        }
      }
    }
    automaticNode: automaticDiscountNode(id: $id) {
      id
      automaticDiscount {
        __typename
        ... on DiscountAutomaticBasic {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountAutomaticApp {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountAutomaticBxgy {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountAutomaticFreeShipping {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
"#,
            $automatic_free_shipping_extra,
            r#"        }
      }
    }
  }
"#
        )
    };
}

/// Read query used to hydrate a discount that is not staged locally so an
/// activate/deactivate transition can be applied against its real dates and
/// status. Must match the recorded cassette `DiscountHydrate` upstream call
/// byte-for-byte (the cassette matcher is strict on query text + variables).
pub(super) const DISCOUNT_HYDRATE_QUERY: &str = discount_hydrate_query!("", "");

pub(super) const DISCOUNT_UPDATE_HYDRATE_QUERY: &str = discount_hydrate_query!(
    r#"          codes(first: 250) {
            nodes {
              id
              code
              asyncUsageCount
            }
          }
          appliesOnOneTimePurchase
          appliesOnSubscription
"#,
    r#"          appliesOnOneTimePurchase
          appliesOnSubscription
"#
);
