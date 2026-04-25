import { Kind, type FieldNode, type SelectionNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { parseSearchQuery, type SearchQueryNode, type SearchQueryTerm } from '../search-query-parser.js';
import {
  getFieldResponseKey,
  getSelectedChildFields,
  serializeConnectionPageInfo,
  serializeEmptyConnectionPageInfo,
} from './graphql-helpers.js';
import {
  mergeMetafieldRecords,
  normalizeOwnerMetafield,
  readMetafieldInputObjects,
  serializeMetafieldsConnection,
  serializeMetafieldSelection,
  upsertOwnerMetafields,
} from './metafields.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type {
  CustomerAddressRecord,
  CustomerCatalogConnectionRecord,
  CustomerCatalogPageInfoRecord,
  CustomerMergeRequestRecord,
  CustomerMetafieldRecord,
  CustomerRecord,
} from '../state/types.js';

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function hasOwnField(value: Record<string, unknown>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

const VALID_TAX_EXEMPTIONS = new Set([
  'CA_BC_COMMERCIAL_FISHERY_EXEMPTION',
  'CA_BC_CONTRACTOR_EXEMPTION',
  'CA_BC_PRODUCTION_AND_MACHINERY_EXEMPTION',
  'CA_BC_RESELLER_EXEMPTION',
  'CA_BC_SUB_CONTRACTOR_EXEMPTION',
  'CA_DIPLOMAT_EXEMPTION',
  'CA_MB_COMMERCIAL_FISHERY_EXEMPTION',
  'CA_MB_FARMER_EXEMPTION',
  'CA_MB_RESELLER_EXEMPTION',
  'CA_NS_COMMERCIAL_FISHERY_EXEMPTION',
  'CA_NS_FARMER_EXEMPTION',
  'CA_ON_PURCHASE_EXEMPTION',
  'CA_PE_COMMERCIAL_FISHERY_EXEMPTION',
  'CA_SK_COMMERCIAL_FISHERY_EXEMPTION',
  'CA_SK_CONTRACTOR_EXEMPTION',
  'CA_SK_FARMER_EXEMPTION',
  'CA_SK_PRODUCTION_AND_MACHINERY_EXEMPTION',
  'CA_SK_RESELLER_EXEMPTION',
  'CA_SK_SUB_CONTRACTOR_EXEMPTION',
  'CA_STATUS_CARD_EXEMPTION',
  'EU_REVERSE_CHARGE_EXEMPTION_RULE',
  'US_AK_RESELLER_EXEMPTION',
  'US_AL_RESELLER_EXEMPTION',
  'US_AR_RESELLER_EXEMPTION',
  'US_AZ_RESELLER_EXEMPTION',
  'US_CA_RESELLER_EXEMPTION',
  'US_CO_RESELLER_EXEMPTION',
  'US_CT_RESELLER_EXEMPTION',
  'US_DC_RESELLER_EXEMPTION',
  'US_DE_RESELLER_EXEMPTION',
  'US_FL_RESELLER_EXEMPTION',
  'US_GA_RESELLER_EXEMPTION',
  'US_HI_RESELLER_EXEMPTION',
  'US_IA_RESELLER_EXEMPTION',
  'US_ID_RESELLER_EXEMPTION',
  'US_IL_RESELLER_EXEMPTION',
  'US_IN_RESELLER_EXEMPTION',
  'US_KS_RESELLER_EXEMPTION',
  'US_KY_RESELLER_EXEMPTION',
  'US_LA_RESELLER_EXEMPTION',
  'US_MA_RESELLER_EXEMPTION',
  'US_MD_RESELLER_EXEMPTION',
  'US_ME_RESELLER_EXEMPTION',
  'US_MI_RESELLER_EXEMPTION',
  'US_MN_RESELLER_EXEMPTION',
  'US_MO_RESELLER_EXEMPTION',
  'US_MS_RESELLER_EXEMPTION',
  'US_MT_RESELLER_EXEMPTION',
  'US_NC_RESELLER_EXEMPTION',
  'US_ND_RESELLER_EXEMPTION',
  'US_NE_RESELLER_EXEMPTION',
  'US_NH_RESELLER_EXEMPTION',
  'US_NJ_RESELLER_EXEMPTION',
  'US_NM_RESELLER_EXEMPTION',
  'US_NV_RESELLER_EXEMPTION',
  'US_NY_RESELLER_EXEMPTION',
  'US_OH_RESELLER_EXEMPTION',
  'US_OK_RESELLER_EXEMPTION',
  'US_OR_RESELLER_EXEMPTION',
  'US_PA_RESELLER_EXEMPTION',
  'US_RI_RESELLER_EXEMPTION',
  'US_SC_RESELLER_EXEMPTION',
  'US_SD_RESELLER_EXEMPTION',
  'US_TN_RESELLER_EXEMPTION',
  'US_TX_RESELLER_EXEMPTION',
  'US_UT_RESELLER_EXEMPTION',
  'US_VA_RESELLER_EXEMPTION',
  'US_VT_RESELLER_EXEMPTION',
  'US_WA_RESELLER_EXEMPTION',
  'US_WI_RESELLER_EXEMPTION',
  'US_WV_RESELLER_EXEMPTION',
  'US_WY_RESELLER_EXEMPTION',
]);

const VALID_CUSTOMER_METAFIELD_TYPES = new Set([
  'antenna_gain',
  'area',
  'battery_charge_capacity',
  'battery_energy_capacity',
  'boolean',
  'capacitance',
  'color',
  'concentration',
  'data_storage_capacity',
  'data_transfer_rate',
  'date_time',
  'date',
  'dimension',
  'display_density',
  'distance',
  'duration',
  'electric_current',
  'electrical_resistance',
  'energy',
  'float',
  'frequency',
  'id',
  'illuminance',
  'inductance',
  'integer',
  'json_string',
  'json',
  'language',
  'link',
  'luminous_flux',
  'mass_flow_rate',
  'money',
  'multi_line_text_field',
  'number_decimal',
  'number_integer',
  'power',
  'pressure',
  'rating',
  'resolution',
  'rich_text_field',
  'rotational_speed',
  'single_line_text_field',
  'sound_level',
  'speed',
  'string',
  'temperature',
  'thermal_power',
  'url',
  'voltage',
  'volume',
  'volumetric_flow_rate',
  'weight',
  'company_reference',
  'customer_reference',
  'product_reference',
  'collection_reference',
  'variant_reference',
  'file_reference',
  'product_taxonomy_value_reference',
  'metaobject_reference',
  'mixed_reference',
  'page_reference',
  'article_reference',
  'order_reference',
]);

const CUSTOMER_METAFIELD_LIST_TYPES = Array.from(VALID_CUSTOMER_METAFIELD_TYPES)
  .filter((type) => type !== 'money' && type !== 'rich_text_field' && type !== 'json_string')
  .map((type) => `list.${type}`);

const VALID_CUSTOMER_METAFIELD_TYPE_MESSAGE = `Type must be one of the following: ${[
  ...VALID_CUSTOMER_METAFIELD_TYPES,
  ...CUSTOMER_METAFIELD_LIST_TYPES,
].join(', ')}.`;

const COUNTRY_NAMES_BY_CODE: Record<string, string> = {
  CA: 'Canada',
  US: 'United States',
};

const PROVINCE_NAMES_BY_COUNTRY_CODE: Record<string, Record<string, string>> = {
  CA: {
    ON: 'Ontario',
    QC: 'Quebec',
  },
};

function isValidCustomerMetafieldType(type: string): boolean {
  return VALID_CUSTOMER_METAFIELD_TYPES.has(type) || CUSTOMER_METAFIELD_LIST_TYPES.includes(type);
}

function normalizeStringField(
  raw: Record<string, unknown>,
  key: string,
  fallback: string | null = null,
): string | null {
  if (!hasOwnField(raw, key)) {
    return fallback;
  }

  const value = raw[key];
  return typeof value === 'string' ? value : null;
}

function normalizeStringLikeField(
  raw: Record<string, unknown>,
  key: string,
  fallback: string | null = null,
): string | null {
  if (!hasOwnField(raw, key)) {
    return fallback;
  }

  const value = raw[key];
  if (typeof value === 'string') {
    return value;
  }

  if (typeof value === 'number' && Number.isFinite(value)) {
    return String(value);
  }

  return null;
}

function normalizeCountField(
  raw: Record<string, unknown>,
  key: string,
  fallback: string | number | null = null,
): string | number | null {
  if (!hasOwnField(raw, key)) {
    return fallback;
  }

  const value = raw[key];
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }

  return typeof value === 'string' ? value : null;
}

function normalizeBooleanField(
  raw: Record<string, unknown>,
  key: string,
  fallback: boolean | null = null,
): boolean | null {
  if (!hasOwnField(raw, key)) {
    return fallback;
  }

  return typeof raw[key] === 'boolean' ? raw[key] : null;
}

function normalizeStringArrayField(raw: Record<string, unknown>, key: string, fallback: string[] = []): string[] {
  if (!hasOwnField(raw, key)) {
    return structuredClone(fallback);
  }

  const value = raw[key];
  if (!Array.isArray(value)) {
    return [];
  }

  return value.filter((entry): entry is string => typeof entry === 'string');
}

function normalizeTaxExemptionsField(raw: Record<string, unknown>, fallback: string[] = []): string[] {
  const values = normalizeStringArrayField(raw, 'taxExemptions', fallback);
  return values.filter((value) => VALID_TAX_EXEMPTIONS.has(value));
}

function buildCustomerDisplayName(
  firstName: string | null,
  lastName: string | null,
  email: string | null,
): string | null {
  const parts = [firstName?.trim() ?? '', lastName?.trim() ?? ''].filter((value) => value.length > 0);
  if (parts.length > 0) {
    return parts.join(' ');
  }

  return email?.trim() || null;
}

function maskPhoneNumber(phone: string | null): string | null {
  return phone;
}

function resolveCountryName(countryCode: string | null, fallback: string | null): string | null {
  return countryCode ? (COUNTRY_NAMES_BY_CODE[countryCode] ?? fallback ?? countryCode) : fallback;
}

function resolveProvinceName(
  countryCode: string | null,
  provinceCode: string | null,
  fallback: string | null,
): string | null {
  if (!countryCode || !provinceCode) {
    return fallback;
  }

  return PROVINCE_NAMES_BY_COUNTRY_CODE[countryCode]?.[provinceCode] ?? fallback ?? provinceCode;
}

function buildFormattedArea(city: string | null, provinceCode: string | null, country: string | null): string | null {
  const cityRegion = [city, provinceCode].filter(Boolean).join(' ');
  return [cityRegion || null, country].filter(Boolean).join(', ') || null;
}

function normalizeMoney(raw: unknown, fallback: CustomerRecord['amountSpent'] = null): CustomerRecord['amountSpent'] {
  if (raw === undefined) {
    return fallback;
  }

  if (!isObject(raw)) {
    return null;
  }

  return {
    amount: normalizeStringField(raw, 'amount'),
    currencyCode: normalizeStringField(raw, 'currencyCode'),
  };
}

function normalizeDefaultEmailAddress(
  raw: unknown,
  fallback: CustomerRecord['defaultEmailAddress'] = null,
): CustomerRecord['defaultEmailAddress'] {
  if (raw === undefined) {
    return fallback;
  }

  if (!isObject(raw)) {
    return null;
  }

  return {
    emailAddress: normalizeStringField(raw, 'emailAddress'),
    marketingState: normalizeStringField(raw, 'marketingState', fallback?.marketingState ?? null),
    marketingOptInLevel: normalizeStringField(raw, 'marketingOptInLevel', fallback?.marketingOptInLevel ?? null),
    marketingUpdatedAt: normalizeStringField(raw, 'marketingUpdatedAt', fallback?.marketingUpdatedAt ?? null),
  };
}

function normalizeDefaultPhoneNumber(
  raw: unknown,
  fallback: CustomerRecord['defaultPhoneNumber'] = null,
): CustomerRecord['defaultPhoneNumber'] {
  if (raw === undefined) {
    return fallback;
  }

  if (!isObject(raw)) {
    return null;
  }

  return {
    phoneNumber: normalizeStringField(raw, 'phoneNumber'),
    marketingState: normalizeStringField(raw, 'marketingState', fallback?.marketingState ?? null),
    marketingOptInLevel: normalizeStringField(raw, 'marketingOptInLevel', fallback?.marketingOptInLevel ?? null),
    marketingUpdatedAt: normalizeStringField(raw, 'marketingUpdatedAt', fallback?.marketingUpdatedAt ?? null),
    marketingCollectedFrom: normalizeStringField(
      raw,
      'marketingCollectedFrom',
      fallback?.marketingCollectedFrom ?? null,
    ),
  };
}

function normalizeEmailMarketingConsent(
  raw: unknown,
  fallback: CustomerRecord['emailMarketingConsent'] = null,
): CustomerRecord['emailMarketingConsent'] {
  if (raw === undefined) {
    return fallback ?? null;
  }

  if (!isObject(raw)) {
    return null;
  }

  return {
    marketingState: normalizeStringField(raw, 'marketingState', fallback?.marketingState ?? null),
    marketingOptInLevel: normalizeStringField(raw, 'marketingOptInLevel', fallback?.marketingOptInLevel ?? null),
    consentUpdatedAt: normalizeStringField(raw, 'consentUpdatedAt', fallback?.consentUpdatedAt ?? null),
  };
}

function normalizeSmsMarketingConsent(
  raw: unknown,
  fallback: CustomerRecord['smsMarketingConsent'] = null,
): CustomerRecord['smsMarketingConsent'] {
  if (raw === undefined) {
    return fallback ?? null;
  }

  if (!isObject(raw)) {
    return null;
  }

  return {
    marketingState: normalizeStringField(raw, 'marketingState', fallback?.marketingState ?? null),
    marketingOptInLevel: normalizeStringField(raw, 'marketingOptInLevel', fallback?.marketingOptInLevel ?? null),
    consentUpdatedAt: normalizeStringField(raw, 'consentUpdatedAt', fallback?.consentUpdatedAt ?? null),
    consentCollectedFrom: normalizeStringField(raw, 'consentCollectedFrom', fallback?.consentCollectedFrom ?? null),
  };
}

function emailConsentFromDefaultAddress(
  defaultEmailAddress: CustomerRecord['defaultEmailAddress'],
): CustomerRecord['emailMarketingConsent'] {
  if (!defaultEmailAddress?.marketingState) {
    return null;
  }

  return {
    marketingState: defaultEmailAddress.marketingState,
    marketingOptInLevel: defaultEmailAddress.marketingOptInLevel ?? null,
    consentUpdatedAt: defaultEmailAddress.marketingUpdatedAt ?? null,
  };
}

function smsConsentFromDefaultPhoneNumber(
  defaultPhoneNumber: CustomerRecord['defaultPhoneNumber'],
): CustomerRecord['smsMarketingConsent'] {
  if (!defaultPhoneNumber?.marketingState) {
    return null;
  }

  return {
    marketingState: defaultPhoneNumber.marketingState,
    marketingOptInLevel: defaultPhoneNumber.marketingOptInLevel ?? null,
    consentUpdatedAt: defaultPhoneNumber.marketingUpdatedAt ?? null,
    consentCollectedFrom: defaultPhoneNumber.marketingCollectedFrom ?? null,
  };
}

function normalizeDefaultAddress(
  raw: unknown,
  fallback: CustomerRecord['defaultAddress'] = null,
): CustomerRecord['defaultAddress'] {
  if (raw === undefined) {
    return fallback;
  }

  if (!isObject(raw)) {
    return null;
  }

  return {
    id: normalizeStringField(raw, 'id'),
    firstName: normalizeStringField(raw, 'firstName'),
    lastName: normalizeStringField(raw, 'lastName'),
    address1: normalizeStringField(raw, 'address1'),
    address2: normalizeStringField(raw, 'address2'),
    city: normalizeStringField(raw, 'city'),
    company: normalizeStringField(raw, 'company'),
    province: normalizeStringField(raw, 'province'),
    provinceCode: normalizeStringField(raw, 'provinceCode'),
    country: normalizeStringField(raw, 'country'),
    countryCodeV2: normalizeStringField(raw, 'countryCodeV2'),
    zip: normalizeStringField(raw, 'zip'),
    phone: normalizeStringField(raw, 'phone'),
    name: normalizeStringField(raw, 'name'),
    formattedArea: normalizeStringField(raw, 'formattedArea'),
  };
}

function normalizeCustomerAddress(
  customerId: string,
  raw: unknown,
  options: { fallback?: CustomerAddressRecord | null; cursor?: string | null; position?: number } = {},
): CustomerAddressRecord | null {
  if (!isObject(raw)) {
    return null;
  }

  const rawId = raw['id'];
  const id = typeof rawId === 'string' && rawId ? rawId : (options.fallback?.id ?? makeSyntheticGid('CustomerAddress'));
  const firstName = normalizeStringField(raw, 'firstName', options.fallback?.firstName ?? null);
  const lastName = normalizeStringField(raw, 'lastName', options.fallback?.lastName ?? null);
  const address1 = normalizeStringField(raw, 'address1', options.fallback?.address1 ?? null);
  const address2 = normalizeStringField(raw, 'address2', options.fallback?.address2 ?? null);
  const city = normalizeStringField(raw, 'city', options.fallback?.city ?? null);
  const company = normalizeStringField(raw, 'company', options.fallback?.company ?? null);
  const provinceCode = normalizeStringField(raw, 'provinceCode', options.fallback?.provinceCode ?? null);
  const countryCodeV2 =
    normalizeStringField(raw, 'countryCodeV2', options.fallback?.countryCodeV2 ?? null) ??
    normalizeStringField(raw, 'countryCode', options.fallback?.countryCodeV2 ?? null);
  const country = normalizeStringField(
    raw,
    'country',
    resolveCountryName(countryCodeV2, options.fallback?.country ?? null),
  );
  const province = normalizeStringField(
    raw,
    'province',
    resolveProvinceName(countryCodeV2, provinceCode, options.fallback?.province ?? null),
  );
  const zip = normalizeStringField(raw, 'zip', options.fallback?.zip ?? null);
  const phone = normalizeStringField(raw, 'phone', options.fallback?.phone ?? null);
  const rawName = normalizeStringField(raw, 'name', options.fallback?.name ?? null);
  const fallbackName = [firstName, lastName].filter(Boolean).join(' ') || null;
  const name = rawName ?? fallbackName;

  return {
    id,
    customerId,
    cursor: options.cursor ?? options.fallback?.cursor ?? null,
    position: options.position ?? options.fallback?.position ?? 0,
    firstName,
    lastName,
    address1,
    address2,
    city,
    company,
    province,
    provinceCode,
    country,
    countryCodeV2,
    zip,
    phone,
    name,
    formattedArea: normalizeStringField(
      raw,
      'formattedArea',
      options.fallback?.formattedArea ?? buildFormattedArea(city, provinceCode, country),
    ),
  };
}

function customerAddressToDefaultAddress(address: CustomerAddressRecord): CustomerRecord['defaultAddress'] {
  return {
    id: address.id,
    firstName: address.firstName,
    lastName: address.lastName,
    address1: address.address1,
    address2: address.address2,
    city: address.city,
    company: address.company,
    province: address.province,
    provinceCode: address.provinceCode,
    country: address.country,
    countryCodeV2: address.countryCodeV2,
    zip: address.zip,
    phone: address.phone,
    name: address.name,
    formattedArea: address.formattedArea,
  };
}

function normalizeCustomer(raw: unknown): CustomerRecord | null {
  if (!isObject(raw)) {
    return null;
  }

  const id = raw['id'];
  if (typeof id !== 'string' || !id) {
    return null;
  }

  const existing = store.getEffectiveCustomerById(id);
  const defaultEmailAddress = normalizeDefaultEmailAddress(
    hasOwnField(raw, 'defaultEmailAddress') ? raw['defaultEmailAddress'] : undefined,
    existing?.defaultEmailAddress ?? null,
  );
  const defaultPhoneNumber = normalizeDefaultPhoneNumber(
    hasOwnField(raw, 'defaultPhoneNumber') ? raw['defaultPhoneNumber'] : undefined,
    existing?.defaultPhoneNumber ?? null,
  );
  const emailMarketingConsent = normalizeEmailMarketingConsent(
    hasOwnField(raw, 'emailMarketingConsent') ? raw['emailMarketingConsent'] : undefined,
    hasOwnField(raw, 'defaultEmailAddress')
      ? emailConsentFromDefaultAddress(defaultEmailAddress)
      : (existing?.emailMarketingConsent ?? emailConsentFromDefaultAddress(defaultEmailAddress)),
  );
  const smsMarketingConsent = normalizeSmsMarketingConsent(
    hasOwnField(raw, 'smsMarketingConsent') ? raw['smsMarketingConsent'] : undefined,
    hasOwnField(raw, 'defaultPhoneNumber')
      ? smsConsentFromDefaultPhoneNumber(defaultPhoneNumber)
      : (existing?.smsMarketingConsent ?? smsConsentFromDefaultPhoneNumber(defaultPhoneNumber)),
  );

  return {
    id,
    firstName: normalizeStringField(raw, 'firstName', existing?.firstName ?? null),
    lastName: normalizeStringField(raw, 'lastName', existing?.lastName ?? null),
    displayName: normalizeStringField(raw, 'displayName', existing?.displayName ?? null),
    email: normalizeStringField(raw, 'email', existing?.email ?? null),
    legacyResourceId: normalizeStringLikeField(raw, 'legacyResourceId', existing?.legacyResourceId ?? null),
    locale: normalizeStringField(raw, 'locale', existing?.locale ?? null),
    note: normalizeStringField(raw, 'note', existing?.note ?? null),
    canDelete: normalizeBooleanField(raw, 'canDelete', existing?.canDelete ?? null),
    verifiedEmail: normalizeBooleanField(raw, 'verifiedEmail', existing?.verifiedEmail ?? null),
    taxExempt: normalizeBooleanField(raw, 'taxExempt', existing?.taxExempt ?? null),
    taxExemptions: normalizeTaxExemptionsField(raw, existing?.taxExemptions ?? []),
    state: normalizeStringField(raw, 'state', existing?.state ?? null),
    tags: normalizeStringArrayField(raw, 'tags', existing?.tags ?? []),
    numberOfOrders: normalizeCountField(raw, 'numberOfOrders', existing?.numberOfOrders ?? null),
    amountSpent: normalizeMoney(
      hasOwnField(raw, 'amountSpent') ? raw['amountSpent'] : undefined,
      existing?.amountSpent ?? null,
    ),
    defaultEmailAddress,
    defaultPhoneNumber,
    emailMarketingConsent,
    smsMarketingConsent,
    defaultAddress: normalizeDefaultAddress(
      hasOwnField(raw, 'defaultAddress') ? raw['defaultAddress'] : undefined,
      existing?.defaultAddress ?? null,
    ),
    createdAt: normalizeStringField(raw, 'createdAt', existing?.createdAt ?? null),
    updatedAt: normalizeStringField(raw, 'updatedAt', existing?.updatedAt ?? null),
  };
}

function serializeMoneySelection(
  field: FieldNode,
  value: CustomerRecord['amountSpent'],
): Record<string, unknown> | null {
  if (!value) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'amount':
        result[key] = value.amount;
        break;
      case 'currencyCode':
        result[key] = value.currencyCode;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDefaultEmailSelection(
  field: FieldNode,
  value: CustomerRecord['defaultEmailAddress'],
): Record<string, unknown> | null {
  if (!value) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'emailAddress':
        result[key] = value.emailAddress;
        break;
      case 'marketingState':
        result[key] = value.marketingState ?? null;
        break;
      case 'marketingOptInLevel':
        result[key] = value.marketingOptInLevel ?? null;
        break;
      case 'marketingUpdatedAt':
        result[key] = value.marketingUpdatedAt ?? null;
        break;
      case 'sourceLocation':
        result[key] = null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDefaultPhoneNumberSelection(
  field: FieldNode,
  value: CustomerRecord['defaultPhoneNumber'],
): Record<string, unknown> | null {
  if (!value) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'phoneNumber':
        result[key] = value.phoneNumber;
        break;
      case 'marketingState':
        result[key] = value.marketingState ?? null;
        break;
      case 'marketingOptInLevel':
        result[key] = value.marketingOptInLevel ?? null;
        break;
      case 'marketingUpdatedAt':
        result[key] = value.marketingUpdatedAt ?? null;
        break;
      case 'marketingCollectedFrom':
        result[key] = value.marketingCollectedFrom ?? null;
        break;
      case 'sourceLocation':
        result[key] = null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeDefaultAddressSelection(
  field: FieldNode,
  value: CustomerRecord['defaultAddress'],
): Record<string, unknown> | null {
  if (!value) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = value.id ?? null;
        break;
      case 'firstName':
        result[key] = value.firstName ?? null;
        break;
      case 'lastName':
        result[key] = value.lastName ?? null;
        break;
      case 'address1':
        result[key] = value.address1;
        break;
      case 'address2':
        result[key] = value.address2 ?? null;
        break;
      case 'city':
        result[key] = value.city;
        break;
      case 'company':
        result[key] = value.company ?? null;
        break;
      case 'province':
        result[key] = value.province;
        break;
      case 'provinceCode':
        result[key] = value.provinceCode ?? null;
        break;
      case 'country':
        result[key] = value.country;
        break;
      case 'countryCodeV2':
        result[key] = value.countryCodeV2 ?? null;
        break;
      case 'zip':
        result[key] = value.zip;
        break;
      case 'phone':
        result[key] = value.phone ?? null;
        break;
      case 'name':
        result[key] = value.name ?? null;
        break;
      case 'formattedArea':
        result[key] = value.formattedArea;
        break;
      case 'formatted':
        result[key] = [value.address1, value.city, value.province, value.zip, value.country].filter(Boolean);
        break;
      case 'coordinatesValidated':
        result[key] = false;
        break;
      case 'latitude':
      case 'longitude':
      case 'timeZone':
      case 'validationResultSummary':
        result[key] = null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCustomerAddressSelection(field: FieldNode, address: CustomerAddressRecord): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = address.id;
        break;
      case 'firstName':
        result[key] = address.firstName;
        break;
      case 'lastName':
        result[key] = address.lastName;
        break;
      case 'address1':
        result[key] = address.address1;
        break;
      case 'address2':
        result[key] = address.address2;
        break;
      case 'city':
        result[key] = address.city;
        break;
      case 'company':
        result[key] = address.company;
        break;
      case 'province':
        result[key] = address.province;
        break;
      case 'provinceCode':
        result[key] = address.provinceCode;
        break;
      case 'country':
        result[key] = address.country;
        break;
      case 'countryCodeV2':
        result[key] = address.countryCodeV2;
        break;
      case 'zip':
        result[key] = address.zip;
        break;
      case 'phone':
        result[key] = address.phone;
        break;
      case 'name':
        result[key] = address.name;
        break;
      case 'formattedArea':
        result[key] = address.formattedArea;
        break;
      case 'formatted':
        result[key] = [address.address1, address.city, address.province, address.zip, address.country].filter(Boolean);
        break;
      case 'coordinatesValidated':
        result[key] = false;
        break;
      case 'latitude':
      case 'longitude':
      case 'timeZone':
      case 'validationResultSummary':
        result[key] = null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCustomerAddressesConnectionSelection(
  customerId: string,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const first =
    typeof args['first'] === 'number' && Number.isFinite(args['first']) ? Math.max(0, Math.floor(args['first'])) : null;
  const addresses = store.listEffectiveCustomerAddresses(customerId);
  const visibleAddresses = first === null ? addresses : addresses.slice(0, first);
  const hasNextPage = first !== null && addresses.length > visibleAddresses.length;
  const connection: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        connection[key] = visibleAddresses.map((address) => serializeCustomerAddressSelection(selection, address));
        break;
      case 'edges':
        connection[key] = visibleAddresses.map((address, index) => {
          const cursor = address.cursor ?? `customer-address-${customerId}-${index}`;
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edge[edgeKey] = cursor;
                break;
              case 'node':
                edge[edgeKey] = serializeCustomerAddressSelection(edgeSelection, address);
                break;
              default:
                edge[edgeKey] = null;
                break;
            }
          }
          return edge;
        });
        break;
      case 'pageInfo':
        connection[key] = serializeConnectionPageInfo(
          selection,
          visibleAddresses,
          hasNextPage,
          false,
          (address) => address.cursor ?? `customer-address-${customerId}-${address.id}`,
          { prefixCursors: false },
        );
        break;
      default:
        connection[key] = null;
        break;
    }
  }

  return connection;
}

function serializeEmptyConnectionSelection(field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
      case 'edges':
        result[key] = [];
        break;
      case 'pageInfo':
        result[key] = serializeEmptyConnectionPageInfo(selection);
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}
function serializeCustomerSelection(
  customer: CustomerRecord,
  field: FieldNode,
  variables: Record<string, unknown> = {},
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = customer.id;
        break;
      case 'firstName':
        result[key] = customer.firstName;
        break;
      case 'lastName':
        result[key] = customer.lastName;
        break;
      case 'displayName':
        result[key] = customer.displayName;
        break;
      case 'email':
        result[key] = customer.email;
        break;
      case 'legacyResourceId':
        result[key] = customer.legacyResourceId;
        break;
      case 'locale':
        result[key] = customer.locale;
        break;
      case 'note':
        result[key] = customer.note;
        break;
      case 'canDelete':
        result[key] = customer.canDelete;
        break;
      case 'verifiedEmail':
        result[key] = customer.verifiedEmail;
        break;
      case 'taxExempt':
        result[key] = customer.taxExempt;
        break;
      case 'taxExemptions':
        result[key] = structuredClone(customer.taxExemptions ?? []);
        break;
      case 'state':
        result[key] = customer.state;
        break;
      case 'tags':
        result[key] = structuredClone(customer.tags);
        break;
      case 'numberOfOrders':
        result[key] = customer.numberOfOrders;
        break;
      case 'amountSpent':
        result[key] = serializeMoneySelection(selection, customer.amountSpent);
        break;
      case 'defaultEmailAddress':
        result[key] = serializeDefaultEmailSelection(selection, customer.defaultEmailAddress);
        break;
      case 'defaultPhoneNumber':
        result[key] = serializeDefaultPhoneNumberSelection(selection, customer.defaultPhoneNumber);
        break;
      case 'emailMarketingConsent':
        result[key] = serializeEmailMarketingConsentSelection(selection, customer.emailMarketingConsent ?? null);
        break;
      case 'smsMarketingConsent':
        result[key] = serializeSmsMarketingConsentSelection(selection, customer.smsMarketingConsent ?? null);
        break;
      case 'defaultAddress':
        result[key] = serializeDefaultAddressSelection(selection, customer.defaultAddress);
        break;
      case 'addresses':
      case 'companyContactProfiles':
        result[key] = [];
        break;
      case 'addressesV2':
        result[key] = serializeCustomerAddressesConnectionSelection(customer.id, selection, variables);
        break;
      case 'events':
      case 'orders':
      case 'paymentMethods':
      case 'storeCreditAccounts':
      case 'subscriptionContracts':
        result[key] = serializeEmptyConnectionSelection(selection);
        break;
      case 'lastOrder':
        result[key] = null;
        break;
      case 'metafield': {
        const args = getFieldArguments(selection, variables);
        const namespace = typeof args['namespace'] === 'string' ? args['namespace'] : null;
        const metafieldKey = typeof args['key'] === 'string' ? args['key'] : null;
        const metafield =
          namespace && metafieldKey
            ? store
                .getEffectiveMetafieldsByCustomerId(customer.id)
                .find((candidate) => candidate.namespace === namespace && candidate.key === metafieldKey)
            : null;
        result[key] = metafield ? serializeMetafieldSelection(metafield, selection) : null;
        break;
      }
      case 'metafields':
        result[key] = serializeMetafieldsConnection(
          store.getEffectiveMetafieldsByCustomerId(customer.id),
          selection,
          variables,
        );
        break;
      case 'createdAt':
        result[key] = customer.createdAt;
        break;
      case 'updatedAt':
        result[key] = customer.updatedAt;
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function serializeEmailMarketingConsentSelection(
  field: FieldNode,
  value: CustomerRecord['emailMarketingConsent'],
): Record<string, unknown> | null {
  if (!value) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'marketingState':
        result[key] = value.marketingState;
        break;
      case 'marketingOptInLevel':
        result[key] = value.marketingOptInLevel;
        break;
      case 'consentUpdatedAt':
        result[key] = value.consentUpdatedAt;
        break;
      case 'sourceLocation':
        result[key] = null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeSmsMarketingConsentSelection(
  field: FieldNode,
  value: CustomerRecord['smsMarketingConsent'],
): Record<string, unknown> | null {
  if (!value) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'marketingState':
        result[key] = value.marketingState;
        break;
      case 'marketingOptInLevel':
        result[key] = value.marketingOptInLevel;
        break;
      case 'consentUpdatedAt':
        result[key] = value.consentUpdatedAt;
        break;
      case 'consentCollectedFrom':
        result[key] = value.consentCollectedFrom;
        break;
      case 'sourceLocation':
        result[key] = null;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function buildSyntheticCustomerCursor(customerId: string): string {
  return `cursor:${customerId}`;
}

function resolveCatalogCustomerCursor(
  customerId: string,
  catalogConnection: CustomerCatalogConnectionRecord | null,
): string {
  return catalogConnection?.cursorByCustomerId[customerId] ?? buildSyntheticCustomerCursor(customerId);
}

function listCustomersForConnection(catalogConnection: CustomerCatalogConnectionRecord | null): CustomerRecord[] {
  if (!catalogConnection) {
    return store.listEffectiveCustomers();
  }

  const orderedCustomers = catalogConnection.orderedCustomerIds
    .map((customerId) => store.getEffectiveCustomerById(customerId))
    .filter((customer): customer is CustomerRecord => customer !== null);
  const seenCustomerIds = new Set(orderedCustomers.map((customer) => customer.id));
  const extraCustomers = store.listEffectiveCustomers().filter((customer) => !seenCustomerIds.has(customer.id));
  return [...orderedCustomers, ...extraCustomers];
}

function normalizeCustomerSearchValue(value: string | null | undefined): string {
  return (value ?? '').trim().toLowerCase();
}

function normalizeCustomerIdentifierValue(value: unknown): string | null {
  return typeof value === 'string' && value.trim().length > 0 ? value.trim() : null;
}

function findCustomerByIdentifier(identifier: Record<string, unknown>): CustomerRecord | null {
  const id = normalizeCustomerIdentifierValue(identifier['id']);
  if (id) {
    return store.getEffectiveCustomerById(id);
  }

  const emailAddress = normalizeCustomerIdentifierValue(identifier['emailAddress']);
  if (emailAddress) {
    const normalizedEmailAddress = emailAddress.toLowerCase();
    return (
      store
        .listEffectiveCustomers()
        .find(
          (customer) =>
            customer.email?.trim().toLowerCase() === normalizedEmailAddress ||
            customer.defaultEmailAddress?.emailAddress?.trim().toLowerCase() === normalizedEmailAddress,
        ) ?? null
    );
  }

  const phoneNumber = normalizeCustomerIdentifierValue(identifier['phoneNumber']);
  if (phoneNumber) {
    return (
      store
        .listEffectiveCustomers()
        .find((customer) => customer.defaultPhoneNumber?.phoneNumber?.trim() === phoneNumber) ?? null
    );
  }

  return null;
}

function countProvidedCustomerIdentifiers(identifier: Record<string, unknown>): number {
  return ['id', 'emailAddress', 'phoneNumber', 'customId'].filter((key) => identifier[key] !== undefined).length;
}

function buildInvalidCustomerIdentifierError(identifier: Record<string, unknown>): Record<string, unknown> {
  const providedCount = countProvidedCustomerIdentifiers(identifier);
  return {
    message: 'Variable $identifier of type CustomerIdentifierInput! was provided invalid value',
    extensions: {
      code: 'INVALID_VARIABLE',
      value: structuredClone(identifier),
      problems: [
        {
          path: [],
          explanation: `'CustomerIdentifierInput' requires exactly one argument, but ${providedCount} were provided.`,
        },
      ],
    },
  };
}

function buildMissingCustomerIdentifierArgumentError(field: FieldNode): Record<string, unknown> {
  return {
    message: "Field 'customerByIdentifier' is missing required arguments: identifier",
    path: [getFieldResponseKey(field)],
    extensions: {
      code: 'missingRequiredArguments',
      className: 'Field',
      name: 'customerByIdentifier',
      arguments: 'identifier',
    },
  };
}

function buildCustomerCustomIdentifierError(field: FieldNode): Record<string, unknown> {
  return {
    message: "Metafield definition of type 'id' is required when using custom ids.",
    path: [getFieldResponseKey(field)],
    extensions: {
      code: 'NOT_FOUND',
    },
  };
}

function isPrefixPattern(rawValue: string): boolean {
  return rawValue.endsWith('*');
}

function matchesStringValue(
  candidate: string | null | undefined,
  rawValue: string,
  matchMode: 'includes' | 'exact',
): boolean {
  const value = rawValue.trim().toLowerCase();
  if (!value) {
    return true;
  }

  const prefixMode = isPrefixPattern(value);
  const normalizedValue = prefixMode ? value.slice(0, -1) : value;
  if (!normalizedValue) {
    return true;
  }

  const normalizedCandidate = normalizeCustomerSearchValue(candidate);
  if (prefixMode) {
    if (normalizedCandidate.startsWith(normalizedValue)) {
      return true;
    }

    return normalizedCandidate.split(/[^a-z0-9]+/u).some((part) => part.startsWith(normalizedValue));
  }

  return matchMode === 'exact'
    ? normalizedCandidate === normalizedValue
    : normalizedCandidate.includes(normalizedValue);
}

function customerMatchesBareToken(customer: CustomerRecord, token: string): boolean {
  const normalizedToken = normalizeCustomerSearchValue(token);
  if (!normalizedToken) {
    return true;
  }

  const haystacks = [
    customer.displayName,
    customer.email,
    customer.defaultEmailAddress?.emailAddress ?? null,
    customer.firstName,
    customer.lastName,
    ...customer.tags,
  ];

  return haystacks.some((value) => matchesStringValue(value, normalizedToken, 'includes'));
}

function searchTermValue(term: SearchQueryTerm): string {
  return term.comparator === null ? term.value : `${term.comparator}${term.value}`;
}

function customerMatchesPositiveQueryTerm(customer: CustomerRecord, term: SearchQueryTerm): boolean {
  if (term.field === null) {
    return customerMatchesBareToken(customer, term.value);
  }
  if (term.field === '') {
    return customerMatchesBareToken(customer, term.raw);
  }

  const rawField = term.field;
  const rawValue = searchTermValue(term);
  const field = normalizeCustomerSearchValue(rawField);

  switch (field) {
    case 'email':
      return (
        matchesStringValue(customer.email, rawValue, 'includes') ||
        matchesStringValue(customer.defaultEmailAddress?.emailAddress ?? null, rawValue, 'includes')
      );
    case 'state':
      return matchesStringValue(customer.state, rawValue, 'exact');
    case 'tag':
    case 'tags':
      return customer.tags.some((tag) => matchesStringValue(tag, rawValue, 'exact'));
    case 'first_name':
      return matchesStringValue(customer.firstName, rawValue, 'includes');
    case 'last_name':
      return matchesStringValue(customer.lastName, rawValue, 'includes');
    case 'name':
    case 'display_name':
      return matchesStringValue(customer.displayName, rawValue, 'includes');
    default:
      return customerMatchesBareToken(customer, term.raw);
  }
}

function customerMatchesQueryNode(customer: CustomerRecord, node: SearchQueryNode): boolean {
  switch (node.type) {
    case 'term': {
      const matches = customerMatchesPositiveQueryTerm(customer, node.term);
      return node.term.negated ? !matches : matches;
    }
    case 'and':
      return node.children.every((child) => customerMatchesQueryNode(customer, child));
    case 'or':
      return node.children.some((child) => customerMatchesQueryNode(customer, child));
    case 'not':
      return !customerMatchesQueryNode(customer, node.child);
    default:
      return true;
  }
}

function filterCustomersByQuery(customers: CustomerRecord[], rawQuery: unknown): CustomerRecord[] {
  if (typeof rawQuery !== 'string' || !rawQuery.trim()) {
    return customers;
  }

  const parsedQuery = parseSearchQuery(rawQuery, { quoteCharacters: ['"'] });
  if (!parsedQuery) {
    return customers;
  }

  return customers.filter((customer) => customerMatchesQueryNode(customer, parsedQuery));
}

function compareNullableStrings(left: string | null, right: string | null): number {
  return (left ?? '').localeCompare(right ?? '');
}

function normalizeSortableString(value: string | null): string {
  return (value ?? '').trim().toLocaleLowerCase();
}

function compareCustomerIds(leftId: string, rightId: string): number {
  const leftTail = Number.parseInt(leftId.split('/').at(-1) ?? '', 10);
  const rightTail = Number.parseInt(rightId.split('/').at(-1) ?? '', 10);

  if (Number.isFinite(leftTail) && Number.isFinite(rightTail)) {
    return leftTail - rightTail;
  }

  return leftId.localeCompare(rightId);
}

function buildCustomerSortName(customer: CustomerRecord): string {
  const lastName = normalizeSortableString(customer.lastName);
  const firstName = normalizeSortableString(customer.firstName);
  const displayName = normalizeSortableString(customer.displayName);

  if (lastName || firstName) {
    return `${lastName}, ${firstName}`;
  }

  return displayName;
}

function compareSortableTuple(left: Array<string | null>, right: Array<string | null>): number {
  for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
    const comparison = normalizeSortableString(left[index] ?? null).localeCompare(
      normalizeSortableString(right[index] ?? null),
    );
    if (comparison !== 0) {
      return comparison;
    }
  }

  return 0;
}

function buildCustomerSearchConnectionKey(rawQuery: unknown, rawSortKey: unknown, rawReverse: unknown): string | null {
  const query = typeof rawQuery === 'string' ? rawQuery.trim() : '';
  const sortKey = typeof rawSortKey === 'string' ? rawSortKey : '';
  if (!query || sortKey !== 'RELEVANCE') {
    return null;
  }

  return JSON.stringify({
    query,
    sortKey,
    reverse: rawReverse === true,
  });
}

function sortCustomers(customers: CustomerRecord[], rawSortKey: unknown, rawReverse: unknown): CustomerRecord[] {
  const reverse = rawReverse === true;
  const sortKey = typeof rawSortKey === 'string' ? rawSortKey : null;
  const sorted = [...customers];

  if (sortKey === 'UPDATED_AT') {
    sorted.sort(
      (left, right) => compareNullableStrings(left.updatedAt, right.updatedAt) || left.id.localeCompare(right.id),
    );
  } else if (sortKey === 'CREATED_AT') {
    sorted.sort(
      (left, right) => compareNullableStrings(left.createdAt, right.createdAt) || left.id.localeCompare(right.id),
    );
  } else if (sortKey === 'NAME') {
    sorted.sort(
      (left, right) =>
        buildCustomerSortName(left).localeCompare(buildCustomerSortName(right)) || left.id.localeCompare(right.id),
    );
  } else if (sortKey === 'ID') {
    sorted.sort((left, right) => compareCustomerIds(left.id, right.id) || left.id.localeCompare(right.id));
  } else if (sortKey === 'LOCATION') {
    sorted.sort(
      (left, right) =>
        compareSortableTuple(
          [
            left.defaultAddress?.country ?? null,
            left.defaultAddress?.province ?? null,
            left.defaultAddress?.city ?? null,
          ],
          [
            right.defaultAddress?.country ?? null,
            right.defaultAddress?.province ?? null,
            right.defaultAddress?.city ?? null,
          ],
        ) || left.id.localeCompare(right.id),
    );
  }

  if (reverse) {
    sorted.reverse();
  }

  return sorted;
}

type CustomerSearchExtensionEntry = {
  path: string[];
  query: string;
  parsed: {
    field: string;
    match_all: string;
  };
  warnings: Array<{
    field: string;
    message: string;
    code: string;
  }>;
};

function buildCustomersCountSearchExtension(rawQuery: unknown, path: string[]): CustomerSearchExtensionEntry | null {
  if (typeof rawQuery !== 'string') {
    return null;
  }

  const query = rawQuery.trim();
  if (!query) {
    return null;
  }

  const singleFieldMatch = query.match(/^([A-Za-z_]+):(.*)$/u);
  if (!singleFieldMatch) {
    return null;
  }

  const field = singleFieldMatch[1]?.trim().toLowerCase() ?? '';
  const matchAll = singleFieldMatch[2]?.trim() ?? '';
  if (!matchAll) {
    return null;
  }

  if (field !== 'email' && field !== 'state') {
    return null;
  }

  return {
    path,
    query,
    parsed: {
      field,
      match_all: matchAll,
    },
    warnings: [
      {
        field,
        message: 'Invalid search field for this query.',
        code: 'invalid_field',
      },
    ],
  };
}

function serializeCustomersCount(_rawQuery: unknown, selections: readonly SelectionNode[]): Record<string, unknown> {
  const allCustomers = store.listEffectiveCustomers();
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'count':
        result[key] = allCustomers.length;
        break;
      case 'precision':
        result[key] = 'EXACT';
        break;
      default:
        result[key] = null;
        break;
    }
  }

  return result;
}

function buildCatalogPageInfo(
  visibleCustomers: CustomerRecord[],
  field: FieldNode,
  hasNextPage: boolean,
  hasPreviousPage: boolean,
  catalogConnection: CustomerCatalogConnectionRecord | null,
  options: { preserveBaselinePageInfo: boolean },
): Record<string, boolean | string | null> {
  return serializeConnectionPageInfo(
    field,
    visibleCustomers,
    hasNextPage,
    hasPreviousPage,
    (customer) => resolveCatalogCustomerCursor(customer.id, catalogConnection),
    {
      prefixCursors: false,
      fallbackStartCursor: options.preserveBaselinePageInfo ? (catalogConnection?.pageInfo.startCursor ?? null) : null,
      fallbackEndCursor: options.preserveBaselinePageInfo ? (catalogConnection?.pageInfo.endCursor ?? null) : null,
    },
  ) as Record<string, boolean | string | null>;
}

function serializeCustomersConnection(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  const args = getFieldArguments(field, variables);
  const first =
    typeof args['first'] === 'number' && Number.isFinite(args['first']) ? Math.max(0, Math.floor(args['first'])) : null;
  const last =
    typeof args['last'] === 'number' && Number.isFinite(args['last']) ? Math.max(0, Math.floor(args['last'])) : null;
  const after = typeof args['after'] === 'string' ? args['after'] : null;
  const before = typeof args['before'] === 'string' ? args['before'] : null;

  const catalogConnection = store.getBaseCustomerCatalogConnection();
  const searchConnectionKey = buildCustomerSearchConnectionKey(args['query'], args['sortKey'], args['reverse']);
  const searchConnection = searchConnectionKey ? store.getBaseCustomerSearchConnection(searchConnectionKey) : null;
  const activeConnection = searchConnection ?? catalogConnection;
  const preserveBaselinePageInfo =
    ((typeof args['query'] !== 'string' && args['sortKey'] === undefined && args['reverse'] !== true) ||
      searchConnection !== null) &&
    before === null &&
    last === null;
  const allCustomers = sortCustomers(
    filterCustomersByQuery(listCustomersForConnection(activeConnection), args['query']),
    args['sortKey'],
    args['reverse'],
  );
  const afterIndex = after
    ? allCustomers.findIndex((customer) => resolveCatalogCustomerCursor(customer.id, activeConnection) === after)
    : -1;
  const beforeIndex = before
    ? allCustomers.findIndex((customer) => resolveCatalogCustomerCursor(customer.id, activeConnection) === before)
    : -1;
  const startIndex = afterIndex >= 0 ? afterIndex + 1 : 0;
  const endIndex = beforeIndex >= 0 ? beforeIndex : allCustomers.length;
  const cursorWindow = allCustomers.slice(startIndex, endIndex);
  const firstWindow = first === null ? cursorWindow : cursorWindow.slice(0, first);
  const visibleCustomers = last === null ? firstWindow : firstWindow.slice(Math.max(0, firstWindow.length - last));
  const visibleStartIndex =
    last === null ? startIndex : Math.max(startIndex, startIndex + firstWindow.length - visibleCustomers.length);
  const visibleEndIndex = visibleStartIndex + visibleCustomers.length;
  const calculatedHasPreviousPage = visibleStartIndex > 0;
  const calculatedHasNextPage = visibleEndIndex < allCustomers.length;
  const hasPreviousPage =
    calculatedHasPreviousPage || (preserveBaselinePageInfo && (activeConnection?.pageInfo.hasPreviousPage ?? false));
  const hasNextPage =
    calculatedHasNextPage || (preserveBaselinePageInfo && (activeConnection?.pageInfo.hasNextPage ?? false));

  const connection: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'nodes':
        connection[key] = visibleCustomers.map((customer) =>
          serializeCustomerSelection(customer, selection, variables),
        );
        break;
      case 'edges':
        connection[key] = visibleCustomers.map((customer) => {
          const edge: Record<string, unknown> = {};
          for (const edgeSelection of getSelectedChildFields(selection)) {
            const edgeKey = getFieldResponseKey(edgeSelection);
            switch (edgeSelection.name.value) {
              case 'cursor':
                edge[edgeKey] = resolveCatalogCustomerCursor(customer.id, activeConnection);
                break;
              case 'node':
                edge[edgeKey] = serializeCustomerSelection(customer, edgeSelection, variables);
                break;
              default:
                edge[edgeKey] = null;
                break;
            }
          }
          return edge;
        });
        break;
      case 'pageInfo':
        connection[key] = buildCatalogPageInfo(
          visibleCustomers,
          selection,
          hasNextPage,
          hasPreviousPage,
          activeConnection,
          {
            preserveBaselinePageInfo,
          },
        );
        break;
      default:
        connection[key] = null;
        break;
    }
  }

  return connection;
}

function normalizeCustomerCatalogPageInfo(raw: unknown): CustomerCatalogPageInfoRecord {
  if (!isObject(raw)) {
    return {
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: null,
      endCursor: null,
    };
  }

  return {
    hasNextPage: raw['hasNextPage'] === true,
    hasPreviousPage: raw['hasPreviousPage'] === true,
    startCursor: typeof raw['startCursor'] === 'string' ? raw['startCursor'] : null,
    endCursor: typeof raw['endCursor'] === 'string' ? raw['endCursor'] : null,
  };
}

function collectHydratableCustomers(document: string, raw: unknown): CustomerRecord[] {
  if (!isObject(raw)) {
    return [];
  }

  const customers: CustomerRecord[] = [];
  for (const field of getRootFields(document)) {
    const responseKey = getFieldResponseKey(field);
    if (field.name.value === 'customer' || field.name.value === 'customerByIdentifier') {
      const customer = normalizeCustomer(raw[responseKey]);
      if (customer) {
        customers.push(customer);
      }
    }

    if (field.name.value === 'customers') {
      const customersConnection = raw[responseKey];
      if (!isObject(customersConnection)) {
        continue;
      }

      const connectionCustomers = [
        ...(Array.isArray(customersConnection['nodes']) ? customersConnection['nodes'] : []),
        ...(Array.isArray(customersConnection['edges'])
          ? customersConnection['edges']
              .filter((edge): edge is Record<string, unknown> => isObject(edge))
              .map((edge) => edge['node'])
          : []),
      ]
        .map((candidate) => normalizeCustomer(candidate))
        .filter((candidate): candidate is CustomerRecord => candidate !== null);
      customers.push(...connectionCustomers);
    }
  }

  return customers;
}

function collectCustomerAddressesFromRawCustomer(rawCustomer: unknown): CustomerAddressRecord[] {
  if (!isObject(rawCustomer) || typeof rawCustomer['id'] !== 'string') {
    return [];
  }

  const customerId = rawCustomer['id'];
  const addressesV2 = rawCustomer['addressesV2'];
  if (!isObject(addressesV2)) {
    return [];
  }

  const addresses: CustomerAddressRecord[] = [];
  const seenAddressIds = new Set<string>();
  const addAddress = (rawAddress: unknown, cursor: string | null, position: number): void => {
    const address = normalizeCustomerAddress(customerId, rawAddress, { cursor, position });
    if (!address || seenAddressIds.has(address.id)) {
      return;
    }
    seenAddressIds.add(address.id);
    addresses.push(address);
  };

  if (Array.isArray(addressesV2['nodes'])) {
    addressesV2['nodes'].forEach((node, index) => addAddress(node, null, index));
  }

  if (Array.isArray(addressesV2['edges'])) {
    addressesV2['edges'].forEach((edge, index) => {
      if (!isObject(edge)) {
        return;
      }
      addAddress(edge['node'], typeof edge['cursor'] === 'string' ? edge['cursor'] : null, index);
    });
  }

  return addresses;
}

function collectCustomerAddresses(document: string, raw: unknown): CustomerAddressRecord[] {
  if (!isObject(raw)) {
    return [];
  }

  const addresses: CustomerAddressRecord[] = [];
  for (const field of getRootFields(document)) {
    const responseKey = getFieldResponseKey(field);
    if (field.name.value === 'customer' || field.name.value === 'customerByIdentifier') {
      addresses.push(...collectCustomerAddressesFromRawCustomer(raw[responseKey]));
    }

    if (field.name.value === 'customers') {
      const customersConnection = raw[responseKey];
      if (!isObject(customersConnection)) {
        continue;
      }
      for (const customer of Array.isArray(customersConnection['nodes']) ? customersConnection['nodes'] : []) {
        addresses.push(...collectCustomerAddressesFromRawCustomer(customer));
      }
      for (const edge of Array.isArray(customersConnection['edges']) ? customersConnection['edges'] : []) {
        if (isObject(edge)) {
          addresses.push(...collectCustomerAddressesFromRawCustomer(edge['node']));
        }
      }
    }
  }

  return addresses;
}

function collectCustomerMetafieldsFromSelection(
  customerId: string,
  rawCustomer: Record<string, unknown>,
  customerField: FieldNode,
): CustomerMetafieldRecord[] {
  const metafields: CustomerMetafieldRecord[] = [];

  for (const selection of getSelectedChildFields(customerField)) {
    const responseKey = getFieldResponseKey(selection);
    if (selection.name.value === 'metafield') {
      const metafield = normalizeOwnerMetafield('customerId', customerId, rawCustomer[responseKey]);
      if (metafield) {
        metafields.push(metafield);
      }
      continue;
    }

    if (selection.name.value !== 'metafields' || !isObject(rawCustomer[responseKey])) {
      continue;
    }

    const connection = rawCustomer[responseKey];
    const nodes = Array.isArray(connection['nodes']) ? connection['nodes'] : [];
    const edgeNodes = Array.isArray(connection['edges'])
      ? connection['edges']
          .filter((edge): edge is Record<string, unknown> => isObject(edge))
          .map((edge) => edge['node'])
      : [];
    for (const rawMetafield of [...nodes, ...edgeNodes]) {
      const metafield = normalizeOwnerMetafield('customerId', customerId, rawMetafield);
      if (metafield) {
        metafields.push(metafield);
      }
    }
  }

  return metafields;
}

function collectCustomerMetafields(
  document: string,
  rawData: Record<string, unknown>,
): Record<string, CustomerMetafieldRecord[]> {
  const metafieldsByCustomerId: Record<string, CustomerMetafieldRecord[]> = {};

  const addMetafields = (customerId: string, metafields: CustomerMetafieldRecord[]): void => {
    if (metafields.length === 0) {
      return;
    }

    metafieldsByCustomerId[customerId] = mergeMetafieldRecords(metafieldsByCustomerId[customerId] ?? [], metafields);
  };

  for (const field of getRootFields(document)) {
    const rootValue = rawData[getFieldResponseKey(field)];
    if (field.name.value === 'customer' && isObject(rootValue)) {
      const customer = normalizeCustomer(rootValue);
      if (customer) {
        addMetafields(customer.id, collectCustomerMetafieldsFromSelection(customer.id, rootValue, field));
      }
      continue;
    }

    if (field.name.value !== 'customers' || !isObject(rootValue)) {
      continue;
    }

    for (const connectionSelection of getSelectedChildFields(field)) {
      const connectionValue = rootValue[getFieldResponseKey(connectionSelection)];
      if (connectionSelection.name.value === 'nodes' && Array.isArray(connectionValue)) {
        for (const rawCustomer of connectionValue) {
          const customer = normalizeCustomer(rawCustomer);
          if (customer && isObject(rawCustomer)) {
            addMetafields(
              customer.id,
              collectCustomerMetafieldsFromSelection(customer.id, rawCustomer, connectionSelection),
            );
          }
        }
      }

      if (connectionSelection.name.value === 'edges' && Array.isArray(connectionValue)) {
        for (const rawEdge of connectionValue) {
          if (!isObject(rawEdge) || !isObject(rawEdge['node'])) {
            continue;
          }

          const customer = normalizeCustomer(rawEdge['node']);
          const nodeSelection = getSelectedChildFields(connectionSelection).find(
            (edgeSelection) => edgeSelection.name.value === 'node',
          );
          if (customer && nodeSelection) {
            addMetafields(
              customer.id,
              collectCustomerMetafieldsFromSelection(customer.id, rawEdge['node'], nodeSelection),
            );
          }
        }
      }
    }
  }

  return metafieldsByCustomerId;
}

function collectCustomerCatalogConnection(
  raw: unknown,
  responseKey = 'customers',
): CustomerCatalogConnectionRecord | null {
  if (!isObject(raw) || !isObject(raw[responseKey])) {
    return null;
  }

  const customersConnection = raw[responseKey];
  const connectionEdges = Array.isArray(customersConnection['edges'])
    ? customersConnection['edges'].filter((edge): edge is Record<string, unknown> => isObject(edge))
    : [];

  const orderedCustomerIds: string[] = [];
  const cursorByCustomerId: Record<string, string> = {};
  for (const edge of connectionEdges) {
    const customer = normalizeCustomer(edge['node']);
    const cursor = typeof edge['cursor'] === 'string' ? edge['cursor'] : null;
    if (!customer) {
      continue;
    }

    orderedCustomerIds.push(customer.id);
    if (cursor) {
      cursorByCustomerId[customer.id] = cursor;
    }
  }

  if (orderedCustomerIds.length === 0) {
    return null;
  }

  return {
    orderedCustomerIds,
    cursorByCustomerId,
    pageInfo: normalizeCustomerCatalogPageInfo(customersConnection['pageInfo']),
  };
}

function collectCustomerSearchConnections(
  document: string,
  variables: Record<string, unknown>,
  rawData: Record<string, unknown>,
): Record<string, CustomerCatalogConnectionRecord> {
  const connections: Record<string, CustomerCatalogConnectionRecord> = {};
  for (const field of getRootFields(document)) {
    if (field.name.value !== 'customers') {
      continue;
    }

    const args = getFieldArguments(field, variables);
    const key = buildCustomerSearchConnectionKey(args['query'], args['sortKey'], args['reverse']);
    if (!key) {
      continue;
    }

    const connection = collectCustomerCatalogConnection(rawData, getFieldResponseKey(field));
    if (!connection) {
      continue;
    }

    connections[key] = connection;
  }

  return connections;
}

type CustomerMutationUserError = {
  field: string[] | null;
  message: string;
  code?: string | null;
};

type CustomerMergeError = {
  errorFields: string[];
  message: string;
};

function serializeCustomerMutationUserErrors(
  field: FieldNode,
  userErrors: CustomerMutationUserError[],
): Array<Record<string, unknown>> {
  return userErrors.map((userError) => {
    const result: Record<string, unknown> = {};
    for (const selection of getSelectedChildFields(field)) {
      const key = getFieldResponseKey(selection);
      switch (selection.name.value) {
        case 'field':
          result[key] = userError.field;
          break;
        case 'message':
          result[key] = userError.message;
          break;
        case 'code':
          result[key] = userError.code ?? null;
          break;
        default:
          result[key] = null;
          break;
      }
    }
    return result;
  });
}

function serializeCustomerMergeErrors(field: FieldNode, errors: CustomerMergeError[]): Array<Record<string, unknown>> {
  return errors.map((error) => {
    const result: Record<string, unknown> = {};
    for (const selection of getSelectedChildFields(field)) {
      const key = getFieldResponseKey(selection);
      switch (selection.name.value) {
        case 'errorFields':
          result[key] = structuredClone(error.errorFields);
          break;
        case 'message':
          result[key] = error.message;
          break;
        default:
          result[key] = null;
          break;
      }
    }
    return result;
  });
}

function serializeJobSelection(
  field: FieldNode,
  job: { id: string; done: boolean } | null,
): Record<string, unknown> | null {
  if (!job) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = job.id;
        break;
      case 'done':
        result[key] = job.done;
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCustomerMergeRequestSelection(
  request: CustomerMergeRequestRecord,
  field: FieldNode,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'jobId':
        result[key] = request.jobId;
        break;
      case 'resultingCustomerId':
        result[key] = request.resultingCustomerId;
        break;
      case 'status':
        result[key] = request.status;
        break;
      case 'customerMergeErrors':
        result[key] = serializeCustomerMergeErrors(selection, request.customerMergeErrors);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeShopSelection(field: FieldNode): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'id':
        result[key] = 'gid://shopify/Shop/1';
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function readCustomerInput(raw: unknown): Record<string, unknown> {
  return isObject(raw) ? raw : {};
}

function normalizeCustomerTags(raw: unknown, fallback: string[]): string[] {
  if (!Array.isArray(raw)) {
    return structuredClone(fallback);
  }

  return raw
    .filter((value): value is string => typeof value === 'string' && value.trim().length > 0)
    .sort((left, right) => left.localeCompare(right));
}

function normalizeCustomerTaxExemptions(raw: unknown, fallback: string[]): string[] {
  if (!Array.isArray(raw)) {
    return structuredClone(fallback);
  }

  return raw
    .filter((value): value is string => typeof value === 'string' && VALID_TAX_EXEMPTIONS.has(value))
    .sort((left, right) => left.localeCompare(right));
}

function validateCustomerTaxExemptionInput(input: Record<string, unknown>): CustomerMutationUserError[] {
  if (!hasOwnField(input, 'taxExemptions')) {
    return [];
  }

  const rawTaxExemptions = input['taxExemptions'];
  if (!Array.isArray(rawTaxExemptions)) {
    return [{ field: ['taxExemptions'], message: 'Tax exemptions must be an array' }];
  }

  return rawTaxExemptions.flatMap((value, index): CustomerMutationUserError[] =>
    typeof value === 'string' && VALID_TAX_EXEMPTIONS.has(value)
      ? []
      : [
          {
            field: ['taxExemptions', String(index)],
            message: 'Tax exemption is not a valid value',
          },
        ],
  );
}

function validateCustomerMetafieldInputs(rawMetafields: unknown, customerId: string): CustomerMutationUserError[] {
  if (rawMetafields === undefined) {
    return [];
  }

  if (!Array.isArray(rawMetafields)) {
    return [{ field: ['metafields'], message: 'Metafields must be an array' }];
  }

  const existingMetafields = store.getEffectiveMetafieldsByCustomerId(customerId);
  return rawMetafields.flatMap((rawMetafield, index): CustomerMutationUserError[] => {
    if (!isObject(rawMetafield)) {
      return [{ field: ['metafields', String(index)], message: 'Metafield input must be an object' }];
    }

    const existingById =
      typeof rawMetafield['id'] === 'string'
        ? (existingMetafields.find((metafield) => metafield.id === rawMetafield['id']) ?? null)
        : null;
    const namespace = typeof rawMetafield['namespace'] === 'string' ? rawMetafield['namespace'].trim() : '';
    const key = typeof rawMetafield['key'] === 'string' ? rawMetafield['key'].trim() : '';
    const type = typeof rawMetafield['type'] === 'string' ? rawMetafield['type'].trim() : '';
    const errors: CustomerMutationUserError[] = [];

    if (typeof rawMetafield['id'] === 'string' && !existingById) {
      errors.push({ field: ['metafields', String(index), 'id'], message: 'Metafield does not exist' });
    }

    if (namespace && key && type && !isValidCustomerMetafieldType(type)) {
      errors.push({
        field: ['metafields', String(index), 'type'],
        message: VALID_CUSTOMER_METAFIELD_TYPE_MESSAGE,
      });
    }

    return errors;
  });
}

function upsertMetafieldsForCustomer(customerId: string, inputs: Record<string, unknown>[]): CustomerMetafieldRecord[] {
  return upsertOwnerMetafields('customerId', customerId, inputs, store.getEffectiveMetafieldsByCustomerId(customerId), {
    allowIdLookup: true,
    trimIdentity: true,
  }).metafields;
}

function readCustomerAddressInput(raw: unknown): Record<string, unknown> {
  return isObject(raw) ? raw : {};
}

function buildCreatedCustomerAddress(customerId: string, input: Record<string, unknown>): CustomerAddressRecord {
  const position = store.listEffectiveCustomerAddresses(customerId).length;
  return normalizeCustomerAddress(customerId, input, {
    position,
    cursor: `customer-address-${customerId}-${position}`,
  })!;
}

function buildUpdatedCustomerAddress(
  existing: CustomerAddressRecord,
  input: Record<string, unknown>,
): CustomerAddressRecord {
  return normalizeCustomerAddress(existing.customerId, input, {
    fallback: existing,
    cursor: existing.cursor ?? `customer-address-${existing.customerId}-${existing.position}`,
    position: existing.position,
  })!;
}

function buildCustomerWithDefaultAddress(
  customer: CustomerRecord,
  defaultAddress: CustomerAddressRecord | null,
): CustomerRecord {
  return {
    ...customer,
    defaultAddress: defaultAddress ? customerAddressToDefaultAddress(defaultAddress) : null,
    updatedAt: makeSyntheticTimestamp(),
  };
}

function buildMissingCustomerAddressCustomerError(fieldPath: string[]): CustomerMutationUserError {
  return {
    field: fieldPath,
    message: 'Customer does not exist',
  };
}

function buildCustomerAddressResourceNotFoundError(field: FieldNode): Record<string, unknown> {
  return {
    message: 'invalid id',
    path: [getFieldResponseKey(field)],
    extensions: { code: 'RESOURCE_NOT_FOUND' },
  };
}

function buildCreatedCustomer(input: Record<string, unknown>): CustomerRecord {
  const id = makeSyntheticGid('Customer');
  const timestamp = makeSyntheticTimestamp();
  const email = typeof input['email'] === 'string' && input['email'].trim().length > 0 ? input['email'].trim() : null;
  const firstName =
    typeof input['firstName'] === 'string' && input['firstName'].trim().length > 0 ? input['firstName'].trim() : null;
  const lastName =
    typeof input['lastName'] === 'string' && input['lastName'].trim().length > 0 ? input['lastName'].trim() : null;
  const locale =
    typeof input['locale'] === 'string' && input['locale'].trim().length > 0 ? input['locale'].trim() : null;
  const note = typeof input['note'] === 'string' && input['note'].trim().length > 0 ? input['note'] : null;
  const phone = typeof input['phone'] === 'string' && input['phone'].trim().length > 0 ? input['phone'].trim() : null;
  const taxExempt = input['taxExempt'] === true;
  const taxExemptions = normalizeCustomerTaxExemptions(input['taxExemptions'], []);
  const tags = normalizeCustomerTags(input['tags'], []);

  return {
    id,
    firstName,
    lastName,
    displayName: buildCustomerDisplayName(firstName, lastName, email),
    email,
    legacyResourceId: id.split('/').at(-1) ?? null,
    locale,
    note,
    canDelete: true,
    verifiedEmail: email ? true : null,
    taxExempt,
    taxExemptions,
    state: 'DISABLED',
    tags,
    numberOfOrders: 0,
    amountSpent: null,
    defaultEmailAddress: email
      ? {
          emailAddress: email,
          marketingState: 'NOT_SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          marketingUpdatedAt: null,
        }
      : null,
    defaultPhoneNumber: phone
      ? {
          phoneNumber: maskPhoneNumber(phone),
          marketingState: 'NOT_SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          marketingUpdatedAt: null,
          marketingCollectedFrom: null,
        }
      : null,
    emailMarketingConsent: email
      ? {
          marketingState: 'NOT_SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: null,
        }
      : null,
    smsMarketingConsent: phone
      ? {
          marketingState: 'NOT_SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: null,
          consentCollectedFrom: null,
        }
      : null,
    defaultAddress: null,
    createdAt: timestamp,
    updatedAt: timestamp,
  };
}

function buildUpdatedCustomer(existing: CustomerRecord, input: Record<string, unknown>): CustomerRecord {
  const email = typeof input['email'] === 'string' ? input['email'].trim() || null : existing.email;
  const firstName = typeof input['firstName'] === 'string' ? input['firstName'].trim() || null : existing.firstName;
  const lastName = typeof input['lastName'] === 'string' ? input['lastName'].trim() || null : existing.lastName;
  const locale = typeof input['locale'] === 'string' ? input['locale'].trim() || null : existing.locale;
  const note = typeof input['note'] === 'string' ? input['note'] || null : existing.note;
  const phone =
    typeof input['phone'] === 'string'
      ? input['phone'].trim() || null
      : (existing.defaultPhoneNumber?.phoneNumber ?? null);

  return {
    ...existing,
    firstName,
    lastName,
    displayName: buildCustomerDisplayName(firstName, lastName, email),
    email,
    locale,
    note,
    verifiedEmail: email ? true : existing.verifiedEmail,
    taxExempt: typeof input['taxExempt'] === 'boolean' ? input['taxExempt'] : existing.taxExempt,
    taxExemptions: normalizeCustomerTaxExemptions(input['taxExemptions'], existing.taxExemptions ?? []),
    tags: normalizeCustomerTags(input['tags'], existing.tags),
    defaultEmailAddress: email
      ? {
          emailAddress: email,
          marketingState:
            existing.defaultEmailAddress?.marketingState ?? existing.emailMarketingConsent?.marketingState,
          marketingOptInLevel:
            existing.defaultEmailAddress?.marketingOptInLevel ?? existing.emailMarketingConsent?.marketingOptInLevel,
          marketingUpdatedAt:
            existing.defaultEmailAddress?.marketingUpdatedAt ?? existing.emailMarketingConsent?.consentUpdatedAt,
        }
      : null,
    defaultPhoneNumber: phone
      ? {
          phoneNumber: maskPhoneNumber(phone),
          marketingState: existing.defaultPhoneNumber?.marketingState ?? existing.smsMarketingConsent?.marketingState,
          marketingOptInLevel:
            existing.defaultPhoneNumber?.marketingOptInLevel ?? existing.smsMarketingConsent?.marketingOptInLevel,
          marketingUpdatedAt:
            existing.defaultPhoneNumber?.marketingUpdatedAt ?? existing.smsMarketingConsent?.consentUpdatedAt,
          marketingCollectedFrom:
            existing.defaultPhoneNumber?.marketingCollectedFrom ?? existing.smsMarketingConsent?.consentCollectedFrom,
        }
      : null,
    emailMarketingConsent: email ? existing.emailMarketingConsent : null,
    smsMarketingConsent: phone ? existing.smsMarketingConsent : null,
    updatedAt: makeSyntheticTimestamp(),
  };
}

function readCustomerMergeOverrideFields(raw: unknown): Record<string, unknown> {
  return isObject(raw) ? raw : {};
}

function readCustomerIdOverride(
  overrideFields: Record<string, unknown>,
  key: string,
  customerOne: CustomerRecord,
  customerTwo: CustomerRecord,
): CustomerRecord | null {
  const value = overrideFields[key];
  if (value === customerOne.id) {
    return customerOne;
  }
  if (value === customerTwo.id) {
    return customerTwo;
  }
  return null;
}

function selectCustomerMergeField<T>(
  overrideFields: Record<string, unknown>,
  key: string,
  customerOne: CustomerRecord,
  customerTwo: CustomerRecord,
  readValue: (customer: CustomerRecord) => T,
): T {
  const selectedCustomer = readCustomerIdOverride(overrideFields, key, customerOne, customerTwo);
  return readValue(selectedCustomer ?? customerTwo);
}

function mergeCustomerNotes(
  customerOne: CustomerRecord,
  customerTwo: CustomerRecord,
  overrideFields: Record<string, unknown>,
): string | null {
  if (typeof overrideFields['note'] === 'string') {
    return overrideFields['note'];
  }

  const notes = [customerTwo.note, customerOne.note].filter((note): note is string => !!note?.trim());
  return notes.length > 0 ? notes.join(' ') : null;
}

function mergeCustomerTags(
  customerOne: CustomerRecord,
  customerTwo: CustomerRecord,
  overrideFields: Record<string, unknown>,
): string[] {
  if (Array.isArray(overrideFields['tags'])) {
    return normalizeCustomerTags(overrideFields['tags'], []);
  }

  return [...new Set([...customerTwo.tags, ...customerOne.tags])].sort((left, right) => left.localeCompare(right));
}

function buildMergedCustomer(
  customerOne: CustomerRecord,
  customerTwo: CustomerRecord,
  overrideFields: Record<string, unknown>,
  options: { updateTimestamp?: boolean } = {},
): CustomerRecord {
  const firstName = selectCustomerMergeField(
    overrideFields,
    'customerIdOfFirstNameToKeep',
    customerOne,
    customerTwo,
    (customer) => customer.firstName,
  );
  const lastName = selectCustomerMergeField(
    overrideFields,
    'customerIdOfLastNameToKeep',
    customerOne,
    customerTwo,
    (customer) => customer.lastName,
  );
  const emailSource =
    readCustomerIdOverride(overrideFields, 'customerIdOfEmailToKeep', customerOne, customerTwo) ?? customerTwo;
  const phoneSource =
    readCustomerIdOverride(overrideFields, 'customerIdOfPhoneNumberToKeep', customerOne, customerTwo) ?? customerTwo;
  const defaultAddress = selectCustomerMergeField(
    overrideFields,
    'customerIdOfDefaultAddressToKeep',
    customerOne,
    customerTwo,
    (customer) => customer.defaultAddress,
  );

  return {
    ...customerTwo,
    firstName,
    lastName,
    displayName: buildCustomerDisplayName(firstName, lastName, emailSource.email),
    email: emailSource.email,
    note: mergeCustomerNotes(customerOne, customerTwo, overrideFields),
    tags: mergeCustomerTags(customerOne, customerTwo, overrideFields),
    defaultEmailAddress: emailSource.defaultEmailAddress,
    defaultPhoneNumber: phoneSource.defaultPhoneNumber,
    emailMarketingConsent: emailSource.emailMarketingConsent,
    smsMarketingConsent: phoneSource.smsMarketingConsent,
    defaultAddress,
    updatedAt: options.updateTimestamp === false ? customerTwo.updatedAt : makeSyntheticTimestamp(),
  };
}

function buildCustomerMergeMissingArgumentError(field: FieldNode, missingArguments: string[]): Record<string, unknown> {
  return {
    message: `Field '${field.name.value}' is missing required arguments: ${missingArguments.join(', ')}`,
    path: [getFieldResponseKey(field)],
    extensions: {
      code: 'missingRequiredArguments',
      className: 'Field',
      name: field.name.value,
      arguments: missingArguments.join(', '),
    },
  };
}

function customerIdTail(customerId: string): string {
  return customerId.split('/').at(-1) ?? customerId;
}

function buildCustomerMergeMissingCustomerError(field: string, customerId: string): CustomerMutationUserError {
  return {
    field: [field],
    message: `Customer does not exist with ID ${customerIdTail(customerId)}`,
    code: 'INVALID_CUSTOMER_ID',
  };
}

function validateCustomerCreateInput(input: Record<string, unknown>): CustomerMutationUserError[] {
  const hasEmail = typeof input['email'] === 'string' && input['email'].trim().length > 0;
  const hasPhone = typeof input['phone'] === 'string' && input['phone'].trim().length > 0;
  const hasFirstName = typeof input['firstName'] === 'string' && input['firstName'].trim().length > 0;
  const hasLastName = typeof input['lastName'] === 'string' && input['lastName'].trim().length > 0;
  if (hasEmail || hasPhone || hasFirstName || hasLastName) {
    return [];
  }

  return [{ field: null, message: 'A name, phone number, or email address must be present' }];
}

function readConsentPayload(input: Record<string, unknown>, key: string): Record<string, unknown> {
  const value = input[key];
  return isObject(value) ? value : {};
}

function validateMarketingState(
  input: Record<string, unknown>,
  consentKey: string,
  acceptedStates: string[],
): CustomerMutationUserError[] {
  const consent = readConsentPayload(input, consentKey);
  const marketingState = consent['marketingState'];
  if (typeof marketingState !== 'string' || !acceptedStates.includes(marketingState)) {
    return [
      {
        field: ['input', consentKey, 'marketingState'],
        message: 'Marketing state is invalid',
        code: 'INVALID',
      },
    ];
  }

  return [];
}

function buildEmailMarketingConsentUpdatedCustomer(
  existing: CustomerRecord,
  input: Record<string, unknown>,
): CustomerRecord {
  const consent = readConsentPayload(input, 'emailMarketingConsent');
  const consentUpdatedAt =
    typeof consent['consentUpdatedAt'] === 'string' && consent['consentUpdatedAt'].trim().length > 0
      ? consent['consentUpdatedAt']
      : makeSyntheticTimestamp();
  const marketingState = typeof consent['marketingState'] === 'string' ? consent['marketingState'] : 'NOT_SUBSCRIBED';
  const marketingOptInLevel =
    typeof consent['marketingOptInLevel'] === 'string'
      ? consent['marketingOptInLevel']
      : (existing.emailMarketingConsent?.marketingOptInLevel ??
        existing.defaultEmailAddress?.marketingOptInLevel ??
        'SINGLE_OPT_IN');
  const emailAddress = existing.defaultEmailAddress?.emailAddress ?? existing.email;

  return {
    ...existing,
    defaultEmailAddress: emailAddress
      ? {
          ...(existing.defaultEmailAddress ?? { emailAddress }),
          emailAddress,
          marketingState,
          marketingOptInLevel,
          marketingUpdatedAt: consentUpdatedAt,
        }
      : null,
    emailMarketingConsent: {
      marketingState,
      marketingOptInLevel,
      consentUpdatedAt,
    },
    updatedAt: makeSyntheticTimestamp(),
  };
}

function buildSmsMarketingConsentUpdatedCustomer(
  existing: CustomerRecord,
  input: Record<string, unknown>,
): CustomerRecord {
  const consent = readConsentPayload(input, 'smsMarketingConsent');
  const consentUpdatedAt =
    typeof consent['consentUpdatedAt'] === 'string' && consent['consentUpdatedAt'].trim().length > 0
      ? consent['consentUpdatedAt']
      : makeSyntheticTimestamp();
  const marketingState = typeof consent['marketingState'] === 'string' ? consent['marketingState'] : 'NOT_SUBSCRIBED';
  const marketingOptInLevel =
    typeof consent['marketingOptInLevel'] === 'string'
      ? consent['marketingOptInLevel']
      : (existing.smsMarketingConsent?.marketingOptInLevel ??
        existing.defaultPhoneNumber?.marketingOptInLevel ??
        'SINGLE_OPT_IN');
  const phoneNumber = existing.defaultPhoneNumber?.phoneNumber ?? null;

  return {
    ...existing,
    defaultPhoneNumber: phoneNumber
      ? {
          ...(existing.defaultPhoneNumber ?? { phoneNumber }),
          phoneNumber,
          marketingState,
          marketingOptInLevel,
          marketingUpdatedAt: consentUpdatedAt,
          marketingCollectedFrom: 'OTHER',
        }
      : null,
    smsMarketingConsent: {
      marketingState,
      marketingOptInLevel,
      consentUpdatedAt,
      consentCollectedFrom: 'OTHER',
    },
    updatedAt: makeSyntheticTimestamp(),
  };
}

function buildAccountInviteBufferedCustomer(existing: CustomerRecord): CustomerRecord {
  return {
    ...existing,
    state: existing.state === 'ENABLED' ? existing.state : 'INVITED',
    updatedAt: makeSyntheticTimestamp(),
  };
}

function buildSyntheticAccountActivationUrl(customerId: string): string {
  const customerKey = customerId.split('/').pop() ?? 'customer';
  const token = makeSyntheticGid('CustomerAccountActivationToken').split('/').pop() ?? 'token';
  return `https://shopify-draft-proxy.local/customer-activation/${encodeURIComponent(customerKey)}?token=${encodeURIComponent(token)}`;
}

function isCustomerPaymentMethodGid(value: string): boolean {
  return value.startsWith('gid://shopify/CustomerPaymentMethod/');
}

function serializeCustomerMutationPayload(
  field: FieldNode,
  payload: {
    customer?: CustomerRecord | null;
    customerAddress?: CustomerAddressRecord | null;
    accountActivationUrl?: string | null;
    deletedCustomerId?: string | null;
    deletedCustomerAddressId?: string | null;
    resultingCustomerId?: string | null;
    job?: { id: string; done: boolean } | null;
    shop?: boolean;
    userErrors: CustomerMutationUserError[];
  },
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'customer':
        result[key] = payload.customer ? serializeCustomerSelection(payload.customer, selection, variables) : null;
        break;
      case 'accountActivationUrl':
        result[key] = payload.accountActivationUrl ?? null;
        break;
      case 'customerAddress':
      case 'address':
        result[key] = payload.customerAddress
          ? serializeCustomerAddressSelection(selection, payload.customerAddress)
          : null;
        break;
      case 'deletedCustomerId':
        result[key] = payload.deletedCustomerId ?? null;
        break;
      case 'deletedCustomerAddressId':
      case 'deletedAddressId':
        result[key] = payload.deletedCustomerAddressId ?? null;
        break;
      case 'resultingCustomerId':
        result[key] = payload.resultingCustomerId ?? null;
        break;
      case 'job':
        result[key] = serializeJobSelection(selection, payload.job ?? null);
        break;
      case 'shop':
        result[key] = payload.shop ? serializeShopSelection(selection) : null;
        break;
      case 'userErrors':
        result[key] = serializeCustomerMutationUserErrors(selection, payload.userErrors);
        break;
      default:
        result[key] = null;
        break;
    }
  }
  return result;
}

function serializeCustomerMergeFieldSet(
  customer: CustomerRecord,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'firstName':
        result[key] = customer.firstName;
        break;
      case 'lastName':
        result[key] = customer.lastName;
        break;
      case 'displayName':
        result[key] = customer.displayName;
        break;
      case 'email':
        result[key] = serializeDefaultEmailSelection(selection, customer.defaultEmailAddress);
        break;
      case 'phoneNumber':
        result[key] = serializeDefaultPhoneNumberSelection(selection, customer.defaultPhoneNumber);
        break;
      case 'defaultAddress':
        result[key] = serializeDefaultAddressSelection(selection, customer.defaultAddress);
        break;
      case 'note':
        result[key] = customer.note;
        break;
      case 'tags':
        result[key] = structuredClone(customer.tags);
        break;
      case 'addresses':
      case 'discountNodes':
      case 'draftOrders':
      case 'giftCards':
      case 'orders':
        result[key] = {
          nodes: [],
          edges: [],
          pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
        };
        break;
      case 'discountNodeCount':
      case 'draftOrderCount':
      case 'giftCardCount':
      case 'metafieldCount':
      case 'orderCount':
        result[key] = 0;
        break;
      default:
        result[key] = serializeCustomerSelection(customer, selection, variables)[key] ?? null;
        break;
    }
  }
  return result;
}

function serializeCustomerMergePreview(
  field: FieldNode,
  customerOne: CustomerRecord,
  customerTwo: CustomerRecord,
  overrideFields: Record<string, unknown>,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const defaultCustomer = buildMergedCustomer(customerOne, customerTwo, {}, { updateTimestamp: false });
  const alternateCustomer = buildMergedCustomer(customerTwo, customerOne, {}, { updateTimestamp: false });
  const resultingCustomer = buildMergedCustomer(customerOne, customerTwo, overrideFields, { updateTimestamp: false });
  const result: Record<string, unknown> = {};

  for (const selection of getSelectedChildFields(field)) {
    const key = getFieldResponseKey(selection);
    switch (selection.name.value) {
      case 'resultingCustomerId':
        result[key] = customerTwo.id;
        break;
      case 'defaultFields':
        result[key] = serializeCustomerMergeFieldSet(defaultCustomer, selection, variables);
        break;
      case 'alternateFields':
        result[key] = serializeCustomerMergeFieldSet(alternateCustomer, selection, variables);
        break;
      case 'blockingFields':
        result[key] = null;
        break;
      case 'customerMergeErrors':
        result[key] = null;
        break;
      default:
        result[key] = serializeCustomerSelection(resultingCustomer, selection, variables)[key] ?? null;
        break;
    }
  }

  return result;
}

export function handleCustomerMutation(
  document: string,
  variables: Record<string, unknown> = {},
): { data?: Record<string, unknown>; errors?: Array<Record<string, unknown>> } {
  const data: Record<string, unknown> = {};
  const errors: Array<Record<string, unknown>> = [];

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);

    if (field.name.value === 'customerCreate') {
      const input = readCustomerInput(args['input']);
      const customerIdForValidation = 'gid://shopify/Customer/__pending__';
      const userErrors = [
        ...validateCustomerCreateInput(input),
        ...validateCustomerTaxExemptionInput(input),
        ...validateCustomerMetafieldInputs(input['metafields'], customerIdForValidation),
      ];
      if (userErrors.length > 0) {
        data[key] = serializeCustomerMutationPayload(field, { customer: null, userErrors }, variables);
        continue;
      }

      const customer = store.stageCreateCustomer(buildCreatedCustomer(input));
      const metafieldInputs = readMetafieldInputObjects(input['metafields']);
      if (metafieldInputs.length > 0) {
        store.replaceStagedMetafieldsForCustomer(
          customer.id,
          upsertMetafieldsForCustomer(customer.id, metafieldInputs),
        );
      }
      data[key] = serializeCustomerMutationPayload(field, { customer, userErrors: [] }, variables);
      continue;
    }

    if (field.name.value === 'customerUpdate') {
      const input = readCustomerInput(args['input']);
      const customerId = typeof input['id'] === 'string' ? input['id'] : null;
      const existingCustomer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      if (!existingCustomer) {
        data[key] = serializeCustomerMutationPayload(
          field,
          {
            customer: null,
            userErrors: [{ field: ['id'], message: 'Customer does not exist' }],
          },
          variables,
        );
        continue;
      }

      const userErrors = [
        ...validateCustomerTaxExemptionInput(input),
        ...validateCustomerMetafieldInputs(input['metafields'], existingCustomer.id),
      ];
      if (userErrors.length > 0) {
        data[key] = serializeCustomerMutationPayload(field, { customer: null, userErrors }, variables);
        continue;
      }

      const customer = store.stageUpdateCustomer(buildUpdatedCustomer(existingCustomer, input));
      const metafieldInputs = readMetafieldInputObjects(input['metafields']);
      if (metafieldInputs.length > 0) {
        store.replaceStagedMetafieldsForCustomer(
          customer.id,
          upsertMetafieldsForCustomer(customer.id, metafieldInputs),
        );
      }
      data[key] = serializeCustomerMutationPayload(field, { customer, userErrors: [] }, variables);
      continue;
    }

    if (field.name.value === 'customerGenerateAccountActivationUrl') {
      const customerId = typeof args['customerId'] === 'string' ? args['customerId'] : null;
      const existingCustomer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      if (!customerId || !existingCustomer) {
        data[key] = serializeCustomerMutationPayload(
          field,
          {
            accountActivationUrl: null,
            userErrors: [{ field: ['customerId'], message: "The customer can't be found." }],
          },
          variables,
        );
        continue;
      }

      data[key] = serializeCustomerMutationPayload(
        field,
        {
          accountActivationUrl: buildSyntheticAccountActivationUrl(customerId),
          userErrors: [],
        },
        variables,
      );
      continue;
    }

    if (field.name.value === 'customerSendAccountInviteEmail') {
      const customerId = typeof args['customerId'] === 'string' ? args['customerId'] : null;
      const existingCustomer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      if (!customerId || !existingCustomer) {
        data[key] = serializeCustomerMutationPayload(
          field,
          {
            customer: null,
            userErrors: [{ field: ['customerId'], message: "Customer can't be found" }],
          },
          variables,
        );
        continue;
      }

      const customer = store.stageUpdateCustomer(buildAccountInviteBufferedCustomer(existingCustomer));
      data[key] = serializeCustomerMutationPayload(field, { customer, userErrors: [] }, variables);
      continue;
    }

    if (field.name.value === 'customerPaymentMethodSendUpdateEmail') {
      const customerPaymentMethodId =
        typeof args['customerPaymentMethodId'] === 'string' ? args['customerPaymentMethodId'] : null;
      const paymentMethod =
        customerPaymentMethodId && isCustomerPaymentMethodGid(customerPaymentMethodId)
          ? store.getEffectiveCustomerPaymentMethodById(customerPaymentMethodId)
          : null;
      if (!customerPaymentMethodId || !paymentMethod) {
        data[key] = serializeCustomerMutationPayload(
          field,
          {
            customer: null,
            userErrors: [{ field: ['customerPaymentMethodId'], message: 'Customer payment method does not exist' }],
          },
          variables,
        );
        continue;
      }

      const customer = paymentMethod.customerId ? store.getEffectiveCustomerById(paymentMethod.customerId) : null;
      data[key] = serializeCustomerMutationPayload(field, { customer, userErrors: [] }, variables);
      continue;
    }

    if (field.name.value === 'customerEmailMarketingConsentUpdate') {
      const input = readCustomerInput(args['input']);
      const customerId = typeof input['customerId'] === 'string' ? input['customerId'] : null;
      const existingCustomer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      if (!existingCustomer) {
        data[key] = serializeCustomerMutationPayload(
          field,
          {
            customer: null,
            userErrors: [{ field: ['input', 'customerId'], message: 'Customer not found', code: 'INVALID' }],
          },
          variables,
        );
        continue;
      }

      const userErrors = validateMarketingState(input, 'emailMarketingConsent', [
        'NOT_SUBSCRIBED',
        'PENDING',
        'SUBSCRIBED',
        'UNSUBSCRIBED',
        'REDACTED',
        'INVALID',
      ]);
      if (userErrors.length > 0) {
        data[key] = serializeCustomerMutationPayload(field, { customer: null, userErrors }, variables);
        continue;
      }

      const customer = store.stageUpdateCustomer(buildEmailMarketingConsentUpdatedCustomer(existingCustomer, input));
      data[key] = serializeCustomerMutationPayload(field, { customer, userErrors: [] }, variables);
      continue;
    }

    if (field.name.value === 'customerSmsMarketingConsentUpdate') {
      const input = readCustomerInput(args['input']);
      const customerId = typeof input['customerId'] === 'string' ? input['customerId'] : null;
      const existingCustomer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      if (!existingCustomer) {
        data[key] = serializeCustomerMutationPayload(
          field,
          {
            customer: null,
            userErrors: [{ field: null, message: 'Customer not found', code: null }],
          },
          variables,
        );
        continue;
      }

      const userErrors = validateMarketingState(input, 'smsMarketingConsent', [
        'NOT_SUBSCRIBED',
        'PENDING',
        'SUBSCRIBED',
        'UNSUBSCRIBED',
        'REDACTED',
      ]);
      if (userErrors.length > 0) {
        data[key] = serializeCustomerMutationPayload(field, { customer: null, userErrors }, variables);
        continue;
      }

      const customer = store.stageUpdateCustomer(buildSmsMarketingConsentUpdatedCustomer(existingCustomer, input));
      data[key] = serializeCustomerMutationPayload(field, { customer, userErrors: [] }, variables);
      continue;
    }

    if (field.name.value === 'customerAddressCreate') {
      const customerId = typeof args['customerId'] === 'string' ? args['customerId'] : null;
      const existingCustomer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      if (!customerId || !existingCustomer) {
        data[key] = serializeCustomerMutationPayload(
          field,
          {
            customerAddress: null,
            userErrors: [buildMissingCustomerAddressCustomerError(['customerId'])],
          },
          variables,
        );
        continue;
      }

      const address = store.stageUpsertCustomerAddress(
        buildCreatedCustomerAddress(customerId, readCustomerAddressInput(args['address'])),
      );
      if (!existingCustomer.defaultAddress || args['setAsDefault'] === true) {
        store.stageUpdateCustomer(buildCustomerWithDefaultAddress(existingCustomer, address));
      }
      data[key] = serializeCustomerMutationPayload(field, { customerAddress: address, userErrors: [] }, variables);
      continue;
    }

    if (field.name.value === 'customerAddressUpdate') {
      const customerId = typeof args['customerId'] === 'string' ? args['customerId'] : null;
      const addressId =
        typeof args['id'] === 'string' ? args['id'] : typeof args['addressId'] === 'string' ? args['addressId'] : null;
      const existingCustomer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      const existingAddress = addressId ? store.getEffectiveCustomerAddressById(addressId) : null;
      if (!customerId || !existingCustomer) {
        data[key] = serializeCustomerMutationPayload(
          field,
          {
            customerAddress: null,
            userErrors: [buildMissingCustomerAddressCustomerError(['customerId'])],
          },
          variables,
        );
        continue;
      }
      if (!existingAddress || existingAddress.customerId !== customerId) {
        data[key] = null;
        errors.push(buildCustomerAddressResourceNotFoundError(field));
        continue;
      }

      const address = store.stageUpsertCustomerAddress(
        buildUpdatedCustomerAddress(existingAddress, readCustomerAddressInput(args['address'])),
      );
      if (existingCustomer.defaultAddress?.id === address.id || args['setAsDefault'] === true) {
        store.stageUpdateCustomer(buildCustomerWithDefaultAddress(existingCustomer, address));
      }
      data[key] = serializeCustomerMutationPayload(field, { customerAddress: address, userErrors: [] }, variables);
      continue;
    }

    if (field.name.value === 'customerAddressDelete') {
      const customerId = typeof args['customerId'] === 'string' ? args['customerId'] : null;
      const addressId =
        typeof args['id'] === 'string' ? args['id'] : typeof args['addressId'] === 'string' ? args['addressId'] : null;
      const existingCustomer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      const existingAddress = addressId ? store.getEffectiveCustomerAddressById(addressId) : null;
      if (!customerId || !existingCustomer) {
        data[key] = serializeCustomerMutationPayload(
          field,
          {
            deletedCustomerAddressId: null,
            userErrors: [buildMissingCustomerAddressCustomerError(['customerId'])],
          },
          variables,
        );
        continue;
      }
      if (!addressId || !existingAddress || existingAddress.customerId !== customerId) {
        data[key] = null;
        errors.push(buildCustomerAddressResourceNotFoundError(field));
        continue;
      }

      store.stageDeleteCustomerAddress(addressId);
      if (existingCustomer.defaultAddress?.id === addressId) {
        const nextDefaultAddress = store.listEffectiveCustomerAddresses(customerId)[0] ?? null;
        store.stageUpdateCustomer(buildCustomerWithDefaultAddress(existingCustomer, nextDefaultAddress));
      }
      data[key] = serializeCustomerMutationPayload(
        field,
        {
          deletedCustomerAddressId: addressId,
          userErrors: [],
        },
        variables,
      );
      continue;
    }

    if (field.name.value === 'customerUpdateDefaultAddress') {
      const customerId = typeof args['customerId'] === 'string' ? args['customerId'] : null;
      const addressId = typeof args['addressId'] === 'string' ? args['addressId'] : null;
      const existingCustomer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      const existingAddress = addressId ? store.getEffectiveCustomerAddressById(addressId) : null;
      if (!customerId || !existingCustomer) {
        data[key] = serializeCustomerMutationPayload(
          field,
          {
            customer: null,
            userErrors: [buildMissingCustomerAddressCustomerError(['customerId'])],
          },
          variables,
        );
        continue;
      }
      if (!existingAddress || existingAddress.customerId !== customerId) {
        data[key] = null;
        errors.push(buildCustomerAddressResourceNotFoundError(field));
        continue;
      }

      const customer = store.stageUpdateCustomer(buildCustomerWithDefaultAddress(existingCustomer, existingAddress));
      data[key] = serializeCustomerMutationPayload(field, { customer, userErrors: [] }, variables);
      continue;
    }

    if (field.name.value === 'customerDelete') {
      const input = readCustomerInput(args['input']);
      const customerId = typeof input['id'] === 'string' ? input['id'] : null;
      const existingCustomer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      if (!existingCustomer || !customerId) {
        data[key] = serializeCustomerMutationPayload(
          field,
          {
            deletedCustomerId: null,
            shop: true,
            userErrors: [{ field: ['id'], message: "Customer can't be found" }],
          },
          variables,
        );
        continue;
      }

      store.stageDeleteCustomer(customerId);
      data[key] = serializeCustomerMutationPayload(
        field,
        {
          deletedCustomerId: customerId,
          shop: true,
          userErrors: [],
        },
        variables,
      );
    }

    if (field.name.value === 'customerMerge') {
      const missingArguments = ['customerOneId', 'customerTwoId'].filter((argument) => !hasOwnField(args, argument));
      if (missingArguments.length > 0) {
        errors.push(buildCustomerMergeMissingArgumentError(field, missingArguments));
        continue;
      }

      const customerOneId = typeof args['customerOneId'] === 'string' ? args['customerOneId'] : null;
      const customerTwoId = typeof args['customerTwoId'] === 'string' ? args['customerTwoId'] : null;
      if (customerOneId && customerTwoId && customerOneId === customerTwoId) {
        data[key] = serializeCustomerMutationPayload(
          field,
          {
            resultingCustomerId: null,
            job: null,
            userErrors: [
              {
                field: null,
                message: 'Customers IDs should not match',
                code: 'INVALID_CUSTOMER_ID',
              },
            ],
          },
          variables,
        );
        continue;
      }

      const customerOne = customerOneId ? store.getEffectiveCustomerById(customerOneId) : null;
      const customerTwo = customerTwoId ? store.getEffectiveCustomerById(customerTwoId) : null;
      const userErrors = [
        ...(customerOneId && !customerOne
          ? [buildCustomerMergeMissingCustomerError('customerOneId', customerOneId)]
          : []),
        ...(customerTwoId && !customerTwo
          ? [buildCustomerMergeMissingCustomerError('customerTwoId', customerTwoId)]
          : []),
      ];
      if (!customerOneId) {
        userErrors.push({
          field: ['customerOneId'],
          message: 'Customer does not exist with ID null',
          code: 'INVALID_CUSTOMER_ID',
        });
      }
      if (!customerTwoId) {
        userErrors.push({
          field: ['customerTwoId'],
          message: 'Customer does not exist with ID null',
          code: 'INVALID_CUSTOMER_ID',
        });
      }
      if (userErrors.length > 0 || !customerOne || !customerTwo || !customerTwoId) {
        data[key] = serializeCustomerMutationPayload(
          field,
          {
            resultingCustomerId: null,
            job: null,
            userErrors,
          },
          variables,
        );
        continue;
      }

      const overrideFields = readCustomerMergeOverrideFields(args['overrideFields']);
      const mergedCustomer = buildMergedCustomer(customerOne, customerTwo, overrideFields);
      const jobId = makeSyntheticGid('Job');
      const customer = store.stageMergeCustomers(customerOne.id, mergedCustomer, {
        jobId,
        resultingCustomerId: customerTwoId,
        status: 'COMPLETED',
        customerMergeErrors: [],
      });
      data[key] = serializeCustomerMutationPayload(
        field,
        {
          resultingCustomerId: customer.id,
          job: { id: jobId, done: false },
          userErrors: [],
        },
        variables,
      );
      continue;
    }
  }

  return {
    ...(Object.keys(data).length > 0 ? { data } : {}),
    ...(errors.length > 0 ? { errors } : {}),
  };
}

export function hydrateCustomersFromUpstreamResponse(
  document: string,
  variables: Record<string, unknown>,
  upstreamBody: unknown,
): void {
  if (!isObject(upstreamBody) || !isObject(upstreamBody['data'])) {
    return;
  }

  const customers = collectHydratableCustomers(document, upstreamBody['data']);
  if (customers.length > 0) {
    store.upsertBaseCustomers(customers);
  }

  const customerAddresses = collectCustomerAddresses(document, upstreamBody['data']);
  if (customerAddresses.length > 0) {
    store.upsertBaseCustomerAddresses(customerAddresses);
  }

  const customerMetafields = collectCustomerMetafields(document, upstreamBody['data']);
  for (const [customerId, metafields] of Object.entries(customerMetafields)) {
    store.replaceBaseMetafieldsForCustomer(customerId, metafields);
  }

  const customerCatalogConnection = collectCustomerCatalogConnection(upstreamBody['data']);
  if (customerCatalogConnection) {
    store.setBaseCustomerCatalogConnection(customerCatalogConnection);
  }

  const customerSearchConnections = collectCustomerSearchConnections(document, variables, upstreamBody['data']);
  for (const [key, connection] of Object.entries(customerSearchConnections)) {
    store.setBaseCustomerSearchConnection(key, connection);
  }
}

export function handleCustomerQuery(
  document: string,
  variables: Record<string, unknown> = {},
): {
  data?: Record<string, unknown>;
  errors?: Array<Record<string, unknown>>;
  extensions?: { search: CustomerSearchExtensionEntry[] };
} {
  const rootFields = getRootFields(document);
  const data: Record<string, unknown> = {};
  const errors: Array<Record<string, unknown>> = [];
  const searchExtensions: CustomerSearchExtensionEntry[] = [];

  for (const field of rootFields) {
    const key = getFieldResponseKey(field);
    if (field.name.value === 'customer') {
      const args = getFieldArguments(field, variables);
      const customerId = typeof args['id'] === 'string' ? args['id'] : null;
      const customer = customerId ? store.getEffectiveCustomerById(customerId) : null;
      data[key] = customer ? serializeCustomerSelection(customer, field, variables) : null;
      continue;
    }

    if (field.name.value === 'customerByIdentifier') {
      const args = getFieldArguments(field, variables);
      if (!hasOwnField(args, 'identifier')) {
        errors.push(buildMissingCustomerIdentifierArgumentError(field));
        continue;
      }

      const identifier = args['identifier'];
      if (!isObject(identifier)) {
        errors.push(buildInvalidCustomerIdentifierError({}));
        continue;
      }

      const providedIdentifierCount = countProvidedCustomerIdentifiers(identifier);
      if (providedIdentifierCount !== 1) {
        errors.push(buildInvalidCustomerIdentifierError(identifier));
        continue;
      }

      if (identifier['customId'] !== undefined) {
        data[key] = null;
        errors.push(buildCustomerCustomIdentifierError(field));
        continue;
      }

      const customer = findCustomerByIdentifier(identifier);
      data[key] = customer ? serializeCustomerSelection(customer, field, variables) : null;
      continue;
    }

    if (field.name.value === 'customerMergePreview') {
      const args = getFieldArguments(field, variables);
      const missingArguments = ['customerOneId', 'customerTwoId'].filter((argument) => !hasOwnField(args, argument));
      if (missingArguments.length > 0) {
        errors.push(buildCustomerMergeMissingArgumentError(field, missingArguments));
        continue;
      }

      const customerOneId = typeof args['customerOneId'] === 'string' ? args['customerOneId'] : null;
      const customerTwoId = typeof args['customerTwoId'] === 'string' ? args['customerTwoId'] : null;
      if (customerOneId && customerTwoId && customerOneId === customerTwoId) {
        data[key] = null;
        errors.push({
          message: 'Customers must be different.',
          path: [key],
          extensions: { code: 'BAD_REQUEST' },
        });
        continue;
      }

      const customerOne = customerOneId ? store.getEffectiveCustomerById(customerOneId) : null;
      const customerTwo = customerTwoId ? store.getEffectiveCustomerById(customerTwoId) : null;
      if (!customerOne || !customerTwo) {
        data[key] = null;
        const missingCustomerId = customerOne ? customerTwoId : customerOneId;
        errors.push({
          message: `Customer does not exist with ID ${customerIdTail(missingCustomerId ?? 'null')}`,
          path: [key],
          extensions: { code: 'BAD_REQUEST' },
        });
        continue;
      }

      data[key] = serializeCustomerMergePreview(
        field,
        customerOne,
        customerTwo,
        readCustomerMergeOverrideFields(args['overrideFields']),
        variables,
      );
      continue;
    }

    if (field.name.value === 'customerMergeJobStatus') {
      const args = getFieldArguments(field, variables);
      const jobId = typeof args['jobId'] === 'string' ? args['jobId'] : null;
      const request = jobId ? store.getCustomerMergeRequest(jobId) : null;
      data[key] = request ? serializeCustomerMergeRequestSelection(request, field) : null;
      continue;
    }

    if (field.name.value === 'customers') {
      data[key] = serializeCustomersConnection(field, variables);
      continue;
    }

    if (field.name.value === 'customersCount') {
      const args = getFieldArguments(field, variables);
      data[key] = serializeCustomersCount(args['query'], field.selectionSet?.selections ?? []);
      const searchExtension = buildCustomersCountSearchExtension(args['query'], [key]);
      if (searchExtension) {
        searchExtensions.push(searchExtension);
      }
    }
  }

  if (searchExtensions.length > 0) {
    return {
      data,
      ...(errors.length > 0 ? { errors } : {}),
      extensions: {
        search: searchExtensions,
      },
    };
  }

  return {
    ...(Object.keys(data).length > 0 ? { data } : {}),
    ...(errors.length > 0 ? { errors } : {}),
  };
}
