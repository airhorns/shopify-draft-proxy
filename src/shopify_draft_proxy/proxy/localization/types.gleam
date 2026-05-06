//// Shared localization domain types and validation error helpers.

import gleam/option.{type Option}

pub type TranslationErrorCode {
  FailsResourceValidation
  InvalidKeyForModel
  InvalidLocaleForShop
  InvalidTranslatableContent
  InvalidValueForHandleTranslation
  MarketCustomContentNotAllowed
  MarketDoesNotExist
  ResourceNotFound
  ResourceNotMarketCustomizable
  SameLocaleAsShopPrimary
  TooManyKeysForResource
  ValueMatchesOriginalContent
}

@internal
pub type AnyUserError {
  TranslationError(field: List(String), message: String, code: String)
  ShopLocaleError(field: List(String), message: String, code: String)
}

/// One translatable content slot on a translatable resource. `digest`
/// is `None` when no captured source digest is available.
@internal
pub type TranslatableContent {
  TranslatableContent(
    key: String,
    value: Option(String),
    digest: Option(String),
    locale: String,
    type_: String,
  )
}

@internal
pub type TranslatableResource {
  TranslatableResource(
    resource_id: String,
    resource_type: String,
    content: List(TranslatableContent),
  )
}

@internal
pub const max_keys_per_translation_mutation = 100

pub fn translation_error_code_allow_list() -> List(String) {
  [
    "FAILS_RESOURCE_VALIDATION",
    "INVALID_KEY_FOR_MODEL",
    "INVALID_LOCALE_FOR_SHOP",
    "INVALID_TRANSLATABLE_CONTENT",
    "INVALID_VALUE_FOR_HANDLE_TRANSLATION",
    "MARKET_CUSTOM_CONTENT_NOT_ALLOWED",
    "MARKET_DOES_NOT_EXIST",
    "RESOURCE_NOT_FOUND",
    "RESOURCE_NOT_MARKET_CUSTOMIZABLE",
    "SAME_LOCALE_AS_SHOP_PRIMARY",
    "TOO_MANY_KEYS_FOR_RESOURCE",
    "VALUE_MATCHES_ORIGINAL_CONTENT",
  ]
}

pub fn emitted_translation_mutation_error_codes() -> List(String) {
  [
    translation_error_code_to_string(FailsResourceValidation),
    translation_error_code_to_string(InvalidKeyForModel),
    translation_error_code_to_string(InvalidLocaleForShop),
    translation_error_code_to_string(InvalidTranslatableContent),
    translation_error_code_to_string(ResourceNotFound),
    translation_error_code_to_string(SameLocaleAsShopPrimary),
    translation_error_code_to_string(TooManyKeysForResource),
  ]
}

@internal
pub fn translation_error_code_to_string(code: TranslationErrorCode) -> String {
  case code {
    FailsResourceValidation -> "FAILS_RESOURCE_VALIDATION"
    InvalidKeyForModel -> "INVALID_KEY_FOR_MODEL"
    InvalidLocaleForShop -> "INVALID_LOCALE_FOR_SHOP"
    InvalidTranslatableContent -> "INVALID_TRANSLATABLE_CONTENT"
    InvalidValueForHandleTranslation -> "INVALID_VALUE_FOR_HANDLE_TRANSLATION"
    MarketCustomContentNotAllowed -> "MARKET_CUSTOM_CONTENT_NOT_ALLOWED"
    MarketDoesNotExist -> "MARKET_DOES_NOT_EXIST"
    ResourceNotFound -> "RESOURCE_NOT_FOUND"
    ResourceNotMarketCustomizable -> "RESOURCE_NOT_MARKET_CUSTOMIZABLE"
    SameLocaleAsShopPrimary -> "SAME_LOCALE_AS_SHOP_PRIMARY"
    TooManyKeysForResource -> "TOO_MANY_KEYS_FOR_RESOURCE"
    ValueMatchesOriginalContent -> "VALUE_MATCHES_ORIGINAL_CONTENT"
  }
}

@internal
pub fn translation_error(
  field: List(String),
  message: String,
  code: TranslationErrorCode,
) -> AnyUserError {
  TranslationError(
    field: field,
    message: message,
    code: translation_error_code_to_string(code),
  )
}
