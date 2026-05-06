//// Captured standard metafield definition template slice used by local
//// standardMetafieldDefinitionEnable handling. The first three templates
//// come from the 2025-01 validation fixture; the Shopify taxonomy templates
//// come from the 2026-04 template-catalog probe and constrained-template
//// pin guard capture documented in docs/endpoints/metafields.md.

import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/metafield_definitions/types.{
  type StandardMetafieldDefinitionTemplate, StandardMetafieldDefinitionTemplate,
}
import shopify_draft_proxy/state/types as state_types

pub fn templates() -> List(StandardMetafieldDefinitionTemplate) {
  [
    StandardMetafieldDefinitionTemplate(
      id: "gid://shopify/StandardMetafieldDefinitionTemplate/1",
      namespace: "descriptors",
      key: "subtitle",
      name: "Product subtitle",
      description: Some("Used as a shorthand for a product name"),
      owner_types: ["PRODUCT", "PRODUCTVARIANT"],
      type_: state_types.MetafieldDefinitionTypeRecord(
        name: "single_line_text_field",
        category: Some("TEXT"),
      ),
      validations: [
        state_types.MetafieldDefinitionValidationRecord(
          name: "max",
          value: Some("70"),
        ),
      ],
      constraints: empty_constraints(),
      visible_to_storefront_api: True,
    ),
    StandardMetafieldDefinitionTemplate(
      id: "gid://shopify/StandardMetafieldDefinitionTemplate/2",
      namespace: "descriptors",
      key: "care_guide",
      name: "Care guide",
      description: Some("Instructions for taking care of a product or apparel"),
      owner_types: ["PRODUCT", "PRODUCTVARIANT"],
      type_: state_types.MetafieldDefinitionTypeRecord(
        name: "multi_line_text_field",
        category: Some("TEXT"),
      ),
      validations: [
        state_types.MetafieldDefinitionValidationRecord(
          name: "max",
          value: Some("500"),
        ),
      ],
      constraints: empty_constraints(),
      visible_to_storefront_api: True,
    ),
    StandardMetafieldDefinitionTemplate(
      id: "gid://shopify/StandardMetafieldDefinitionTemplate/3",
      namespace: "facts",
      key: "isbn",
      name: "ISBN",
      description: Some("International Standard Book Number"),
      owner_types: ["PRODUCT", "PRODUCTVARIANT"],
      type_: state_types.MetafieldDefinitionTypeRecord(
        name: "single_line_text_field",
        category: Some("TEXT"),
      ),
      validations: [
        state_types.MetafieldDefinitionValidationRecord(
          name: "regex",
          value: Some(
            "^((\\d{3})?([\\-\\s])?(\\d{1,5})([\\-\\s])?(\\d{1,7})([\\-\\s])?(\\d{6})([\\-\\s])?(\\d{1}))$",
          ),
        ),
      ],
      constraints: empty_constraints(),
      visible_to_storefront_api: True,
    ),
    StandardMetafieldDefinitionTemplate(
      id: "gid://shopify/StandardMetafieldDefinitionTemplate/10001",
      namespace: "shopify",
      key: "color-pattern",
      name: "Color",
      description: Some(
        "Defines the primary color or pattern, such as blue or striped",
      ),
      owner_types: ["PRODUCT"],
      type_: state_types.MetafieldDefinitionTypeRecord(
        name: "list.metaobject_reference",
        category: Some("REFERENCE"),
      ),
      validations: [],
      constraints: state_types.MetafieldDefinitionConstraintsRecord(
        key: Some("category"),
        values: [
          state_types.MetafieldDefinitionConstraintValueRecord(
            value: "ap-2-1-1",
          ),
        ],
      ),
      visible_to_storefront_api: True,
    ),
    StandardMetafieldDefinitionTemplate(
      id: "gid://shopify/StandardMetafieldDefinitionTemplate/10004",
      namespace: "shopify",
      key: "material",
      name: "Material",
      description: Some(
        "Defines a product's primary material, such as cotton or wool",
      ),
      owner_types: ["PRODUCT"],
      type_: state_types.MetafieldDefinitionTypeRecord(
        name: "list.metaobject_reference",
        category: Some("REFERENCE"),
      ),
      validations: [],
      constraints: state_types.MetafieldDefinitionConstraintsRecord(
        key: Some("category"),
        values: [
          state_types.MetafieldDefinitionConstraintValueRecord(
            value: "ap-2-1-1",
          ),
        ],
      ),
      visible_to_storefront_api: True,
    ),
  ]
}

fn empty_constraints() -> state_types.MetafieldDefinitionConstraintsRecord {
  state_types.MetafieldDefinitionConstraintsRecord(key: None, values: [])
}
