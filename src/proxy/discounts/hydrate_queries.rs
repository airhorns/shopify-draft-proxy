/// Read query used to hydrate a discount that is not staged locally so an
/// activate/deactivate transition or partial update can be applied against its
/// real configuration. Must match recorded `DiscountHydrate` upstream calls
/// byte-for-byte; update cassettes are strict on query text + variables.
pub(super) const DISCOUNT_HYDRATE_QUERY: &str = r#"#graphql
  query DiscountHydrate($id: ID!) {
    codeNode: codeDiscountNode(id: $id) {
      id
      metafields(first: 250) {
        nodes {
          id
          namespace
          key
          type
          value
          createdAt
          updatedAt
        }
      }
      codeDiscount {
        __typename
        ... on DiscountCodeBasic {
          title
          status
          summary
          startsAt
          endsAt
          createdAt
          updatedAt
          asyncUsageCount
          usageLimit
          appliesOncePerCustomer
          recurringCycleLimit
          discountClasses
          combinesWith {
            productDiscounts
            orderDiscounts
            shippingDiscounts
          }
          context {
            __typename
            ... on DiscountBuyerSelectionAll {
              all
            }
            ... on DiscountCustomers {
              customers {
                id
                displayName
              }
            }
            ... on DiscountCustomerSegments {
              segments {
                id
                name
              }
            }
          }
          customerGets {
            value {
              __typename
              ... on DiscountPercentage {
                percentage
              }
              ... on DiscountAmount {
                amount {
                  amount
                  currencyCode
                }
                appliesOnEachItem
              }
            }
            items {
              __typename
              ... on AllDiscountItems {
                allItems
              }
              ... on DiscountProducts {
                products(first: 250) {
                  nodes {
                    id
                  }
                }
                productVariants(first: 250) {
                  nodes {
                    id
                  }
                }
              }
              ... on DiscountCollections {
                collections(first: 250) {
                  nodes {
                    id
                  }
                }
              }
            }
            appliesOnOneTimePurchase
            appliesOnSubscription
          }
          minimumRequirement {
            __typename
            ... on DiscountMinimumSubtotal {
              greaterThanOrEqualToSubtotal {
                amount
                currencyCode
              }
            }
            ... on DiscountMinimumQuantity {
              greaterThanOrEqualToQuantity
            }
          }
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
          createdAt
          updatedAt
          asyncUsageCount
          usageLimit
          appliesOncePerCustomer
          recurringCycleLimit
          discountClasses
          combinesWith {
            productDiscounts
            orderDiscounts
            shippingDiscounts
          }
          context {
            __typename
            ... on DiscountBuyerSelectionAll {
              all
            }
            ... on DiscountCustomers {
              customers {
                id
                displayName
              }
            }
            ... on DiscountCustomerSegments {
              segments {
                id
                name
              }
            }
          }
          appliesOnOneTimePurchase
          appliesOnSubscription
        }
        ... on DiscountCodeBxgy {
          title
          status
          summary
          startsAt
          endsAt
          createdAt
          updatedAt
          asyncUsageCount
          usageLimit
          usesPerOrderLimit
          appliesOncePerCustomer
          discountClasses
          combinesWith {
            productDiscounts
            orderDiscounts
            shippingDiscounts
          }
          context {
            __typename
            ... on DiscountBuyerSelectionAll {
              all
            }
            ... on DiscountCustomers {
              customers {
                id
                displayName
              }
            }
            ... on DiscountCustomerSegments {
              segments {
                id
                name
              }
            }
          }
          customerBuys {
            value {
              __typename
              ... on DiscountQuantity {
                quantity
              }
            }
            items {
              __typename
              ... on DiscountProducts {
                products(first: 250) {
                  nodes {
                    id
                  }
                }
                productVariants(first: 250) {
                  nodes {
                    id
                  }
                }
              }
              ... on DiscountCollections {
                collections(first: 250) {
                  nodes {
                    id
                  }
                }
              }
            }
          }
          customerGets {
            value {
              __typename
              ... on DiscountOnQuantity {
                quantity {
                  quantity
                }
                effect {
                  __typename
                  ... on DiscountPercentage {
                    percentage
                  }
                  ... on DiscountAmount {
                    amount {
                      amount
                      currencyCode
                    }
                    appliesOnEachItem
                  }
                }
              }
            }
            items {
              __typename
              ... on DiscountProducts {
                products(first: 250) {
                  nodes {
                    id
                  }
                }
                productVariants(first: 250) {
                  nodes {
                    id
                  }
                }
              }
              ... on DiscountCollections {
                collections(first: 250) {
                  nodes {
                    id
                  }
                }
              }
            }
          }
          codes(first: 250) {
            nodes {
              id
              code
              asyncUsageCount
            }
          }
        }
        ... on DiscountCodeFreeShipping {
          title
          status
          summary
          startsAt
          endsAt
          createdAt
          updatedAt
          asyncUsageCount
          usageLimit
          appliesOncePerCustomer
          recurringCycleLimit
          discountClasses
          combinesWith {
            productDiscounts
            orderDiscounts
            shippingDiscounts
          }
          context {
            __typename
            ... on DiscountBuyerSelectionAll {
              all
            }
            ... on DiscountCustomers {
              customers {
                id
                displayName
              }
            }
            ... on DiscountCustomerSegments {
              segments {
                id
                name
              }
            }
          }
          destinationSelection {
            __typename
            ... on DiscountCountryAll {
              allCountries
            }
            ... on DiscountCountries {
              countries
              includeRestOfWorld
            }
          }
          maximumShippingPrice {
            amount
            currencyCode
          }
          minimumRequirement {
            __typename
            ... on DiscountMinimumSubtotal {
              greaterThanOrEqualToSubtotal {
                amount
                currencyCode
              }
            }
            ... on DiscountMinimumQuantity {
              greaterThanOrEqualToQuantity
            }
          }
          appliesOnOneTimePurchase
          appliesOnSubscription
          codes(first: 250) {
            nodes {
              id
              code
              asyncUsageCount
            }
          }
        }
      }
    }
    automaticNode: automaticDiscountNode(id: $id) {
      id
      metafields(first: 250) {
        nodes {
          id
          namespace
          key
          type
          value
          createdAt
          updatedAt
        }
      }
      automaticDiscount {
        __typename
        ... on DiscountAutomaticBasic {
          title
          status
          summary
          startsAt
          endsAt
          createdAt
          updatedAt
          asyncUsageCount
          recurringCycleLimit
          discountClasses
          combinesWith {
            productDiscounts
            orderDiscounts
            shippingDiscounts
          }
          context {
            __typename
            ... on DiscountBuyerSelectionAll {
              all
            }
            ... on DiscountCustomers {
              customers {
                id
                displayName
              }
            }
            ... on DiscountCustomerSegments {
              segments {
                id
                name
              }
            }
          }
          customerGets {
            value {
              __typename
              ... on DiscountPercentage {
                percentage
              }
              ... on DiscountAmount {
                amount {
                  amount
                  currencyCode
                }
                appliesOnEachItem
              }
            }
            items {
              __typename
              ... on AllDiscountItems {
                allItems
              }
              ... on DiscountProducts {
                products(first: 250) {
                  nodes {
                    id
                  }
                }
                productVariants(first: 250) {
                  nodes {
                    id
                  }
                }
              }
              ... on DiscountCollections {
                collections(first: 250) {
                  nodes {
                    id
                  }
                }
              }
            }
            appliesOnOneTimePurchase
            appliesOnSubscription
          }
          minimumRequirement {
            __typename
            ... on DiscountMinimumSubtotal {
              greaterThanOrEqualToSubtotal {
                amount
                currencyCode
              }
            }
            ... on DiscountMinimumQuantity {
              greaterThanOrEqualToQuantity
            }
          }
        }
        ... on DiscountAutomaticApp {
          title
          status
          startsAt
          endsAt
          createdAt
          updatedAt
          asyncUsageCount
          recurringCycleLimit
          discountClasses
          combinesWith {
            productDiscounts
            orderDiscounts
            shippingDiscounts
          }
          context {
            __typename
            ... on DiscountBuyerSelectionAll {
              all
            }
            ... on DiscountCustomers {
              customers {
                id
                displayName
              }
            }
            ... on DiscountCustomerSegments {
              segments {
                id
                name
              }
            }
          }
          appliesOnOneTimePurchase
          appliesOnSubscription
        }
        ... on DiscountAutomaticBxgy {
          title
          status
          summary
          startsAt
          endsAt
          createdAt
          updatedAt
          asyncUsageCount
          usesPerOrderLimit
          discountClasses
          combinesWith {
            productDiscounts
            orderDiscounts
            shippingDiscounts
          }
          context {
            __typename
            ... on DiscountBuyerSelectionAll {
              all
            }
            ... on DiscountCustomers {
              customers {
                id
                displayName
              }
            }
            ... on DiscountCustomerSegments {
              segments {
                id
                name
              }
            }
          }
          customerBuys {
            value {
              __typename
              ... on DiscountQuantity {
                quantity
              }
            }
            items {
              __typename
              ... on DiscountProducts {
                products(first: 250) {
                  nodes {
                    id
                  }
                }
                productVariants(first: 250) {
                  nodes {
                    id
                  }
                }
              }
              ... on DiscountCollections {
                collections(first: 250) {
                  nodes {
                    id
                  }
                }
              }
            }
          }
          customerGets {
            value {
              __typename
              ... on DiscountOnQuantity {
                quantity {
                  quantity
                }
                effect {
                  __typename
                  ... on DiscountPercentage {
                    percentage
                  }
                  ... on DiscountAmount {
                    amount {
                      amount
                      currencyCode
                    }
                    appliesOnEachItem
                  }
                }
              }
            }
            items {
              __typename
              ... on DiscountProducts {
                products(first: 250) {
                  nodes {
                    id
                  }
                }
                productVariants(first: 250) {
                  nodes {
                    id
                  }
                }
              }
              ... on DiscountCollections {
                collections(first: 250) {
                  nodes {
                    id
                  }
                }
              }
            }
          }
        }
        ... on DiscountAutomaticFreeShipping {
          title
          status
          summary
          startsAt
          endsAt
          createdAt
          updatedAt
          asyncUsageCount
          recurringCycleLimit
          discountClasses
          combinesWith {
            productDiscounts
            orderDiscounts
            shippingDiscounts
          }
          context {
            __typename
            ... on DiscountBuyerSelectionAll {
              all
            }
            ... on DiscountCustomers {
              customers {
                id
                displayName
              }
            }
            ... on DiscountCustomerSegments {
              segments {
                id
                name
              }
            }
          }
          destinationSelection {
            __typename
            ... on DiscountCountryAll {
              allCountries
            }
            ... on DiscountCountries {
              countries
              includeRestOfWorld
            }
          }
          maximumShippingPrice {
            amount
            currencyCode
          }
          minimumRequirement {
            __typename
            ... on DiscountMinimumSubtotal {
              greaterThanOrEqualToSubtotal {
                amount
                currencyCode
              }
            }
            ... on DiscountMinimumQuantity {
              greaterThanOrEqualToQuantity
            }
          }
          appliesOnOneTimePurchase
          appliesOnSubscription
        }
      }
    }
  }
"#;

pub(super) const DISCOUNT_UPDATE_HYDRATE_QUERY: &str = DISCOUNT_HYDRATE_QUERY;
