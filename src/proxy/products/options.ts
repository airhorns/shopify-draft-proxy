import type { ProxyRuntimeContext } from '../runtime-context.js';
import type { ProductOptionRecord, ProductRecord, ProductVariantRecord } from '../../state/types.js';
import { isObject } from './helpers.js';

export function makeDefaultInventoryItemRecord(
  runtime: ProxyRuntimeContext,
): NonNullable<ProductVariantRecord['inventoryItem']> {
  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('InventoryItem'),
    tracked: false,
    requiresShipping: true,
    measurement: null,
    countryCodeOfOrigin: null,
    provinceCodeOfOrigin: null,
    harmonizedSystemCode: null,
    inventoryLevels: null,
  };
}

export function makeDefaultVariantRecord(runtime: ProxyRuntimeContext, product: ProductRecord): ProductVariantRecord {
  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('ProductVariant'),
    productId: product.id,
    title: 'Default Title',
    sku: null,
    barcode: null,
    price: null,
    compareAtPrice: null,
    taxable: null,
    inventoryPolicy: null,
    inventoryQuantity: 0,
    selectedOptions: [],
    inventoryItem: makeDefaultInventoryItemRecord(runtime),
  };
}

export function makeDefaultOptionRecord(runtime: ProxyRuntimeContext, product: ProductRecord): ProductOptionRecord {
  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('ProductOption'),
    productId: product.id,
    name: 'Title',
    position: 1,
    optionValues: [
      {
        id: runtime.syntheticIdentity.makeSyntheticGid('ProductOptionValue'),
        name: 'Default Title',
        hasVariants: true,
      },
    ],
  };
}

export function deriveVariantTitle(
  rawTitle: unknown,
  selectedOptions: ProductVariantRecord['selectedOptions'],
  fallbackTitle: string,
): string {
  if (typeof rawTitle === 'string' && rawTitle.trim()) {
    return rawTitle;
  }

  const selectedOptionTitle = selectedOptions
    .map((selectedOption) => selectedOption.value)
    .join(' / ')
    .trim();
  return selectedOptionTitle || fallbackTitle;
}

export function normalizeOptionPositions(options: ProductOptionRecord[]): ProductOptionRecord[] {
  return options.map((option, index) => ({
    ...structuredClone(option),
    position: index + 1,
  }));
}

function readOptionValueCreateInput(
  runtime: ProxyRuntimeContext,
  raw: unknown,
): ProductOptionRecord['optionValues'][number] | null {
  if (!isObject(raw)) {
    return null;
  }

  const rawName = raw['name'];
  if (typeof rawName !== 'string' || !rawName.trim()) {
    return null;
  }

  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('ProductOptionValue'),
    name: rawName,
    hasVariants: false,
  };
}

export function readOptionValueCreateInputs(
  runtime: ProxyRuntimeContext,
  raw: unknown,
): ProductOptionRecord['optionValues'] {
  if (!Array.isArray(raw)) {
    return [];
  }

  return raw
    .map((value) => readOptionValueCreateInput(runtime, value))
    .filter((value): value is ProductOptionRecord['optionValues'][number] => value !== null);
}

export function makeCreatedOptionRecord(
  runtime: ProxyRuntimeContext,
  productId: string,
  input: Record<string, unknown>,
): ProductOptionRecord {
  const rawName = input['name'];
  const optionValues = readOptionValueCreateInputs(runtime, input['values']);

  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('ProductOption'),
    productId,
    name: typeof rawName === 'string' && rawName.trim() ? rawName : 'Option',
    position: 0,
    optionValues,
  };
}

export function insertOptionAtPosition(
  options: ProductOptionRecord[],
  option: ProductOptionRecord,
  rawPosition: unknown,
): ProductOptionRecord[] {
  const nextOptions = options.map((existingOption) => structuredClone(existingOption));
  const normalizedPosition =
    typeof rawPosition === 'number' && Number.isInteger(rawPosition) && rawPosition > 0
      ? Math.min(rawPosition, nextOptions.length + 1)
      : nextOptions.length + 1;

  nextOptions.splice(normalizedPosition - 1, 0, structuredClone(option));
  return normalizeOptionPositions(nextOptions);
}

export function productUsesOnlyDefaultOptionState(
  options: ProductOptionRecord[],
  variants: ProductVariantRecord[],
): boolean {
  const selectedOptions = variants[0]?.selectedOptions ?? [];
  const variantUsesDefaultSelection =
    selectedOptions.length === 0 ||
    (selectedOptions.length === 1 &&
      selectedOptions[0]?.name === 'Title' &&
      selectedOptions[0]?.value === 'Default Title');

  return (
    options.length === 1 &&
    options[0]?.name === 'Title' &&
    options[0]?.optionValues.length === 1 &&
    options[0]?.optionValues[0]?.name === 'Default Title' &&
    variants.length === 1 &&
    variantUsesDefaultSelection
  );
}

export function remapDefaultVariantToCreatedOptions(
  variant: ProductVariantRecord,
  options: ProductOptionRecord[],
): ProductVariantRecord {
  const selectedOptions = options
    .map((option) => {
      const firstValue = option.optionValues[0]?.name;
      if (typeof firstValue !== 'string' || !firstValue.trim()) {
        return null;
      }
      return {
        name: option.name,
        value: firstValue,
      };
    })
    .filter((value): value is ProductVariantRecord['selectedOptions'][number] => value !== null);

  return {
    ...structuredClone(variant),
    title: deriveVariantTitle(null, selectedOptions, 'Default Title'),
    selectedOptions,
  };
}

export function restoreDefaultOptionState(
  runtime: ProxyRuntimeContext,
  product: ProductRecord,
  variants: ProductVariantRecord[],
): {
  options: ProductOptionRecord[];
  variants: ProductVariantRecord[];
} {
  const baseVariant = variants[0] ? structuredClone(variants[0]) : makeDefaultVariantRecord(runtime, product);
  return {
    options: [makeDefaultOptionRecord(runtime, product)],
    variants: [
      {
        ...baseVariant,
        productId: product.id,
        title: 'Default Title',
        selectedOptions: [{ name: 'Title', value: 'Default Title' }],
      },
    ],
  };
}

export function remapVariantSelectionsForOptionUpdate(
  variants: ProductVariantRecord[],
  previousOptionName: string,
  nextOptionName: string,
  renamedValues: Map<string, string>,
): ProductVariantRecord[] {
  return variants.map((variant) => {
    const selectedOptions = variant.selectedOptions.map((selectedOption) => {
      if (selectedOption.name !== previousOptionName) {
        return selectedOption;
      }
      return {
        name: nextOptionName,
        value: renamedValues.get(selectedOption.value) ?? selectedOption.value,
      };
    });

    return {
      ...structuredClone(variant),
      title: deriveVariantTitle(null, selectedOptions, variant.title),
      selectedOptions,
    };
  });
}

export function reorderVariantSelectionsForOptions(
  variants: ProductVariantRecord[],
  options: ProductOptionRecord[],
): ProductVariantRecord[] {
  return variants.map((variant) => {
    const selectedByName = new Map(
      variant.selectedOptions.map((selectedOption) => [selectedOption.name, selectedOption]),
    );
    const selectedOptions = options
      .map((option) => selectedByName.get(option.name) ?? null)
      .filter(
        (selectedOption): selectedOption is ProductVariantRecord['selectedOptions'][number] => selectedOption !== null,
      );

    return {
      ...structuredClone(variant),
      title: deriveVariantTitle(null, selectedOptions, variant.title),
      selectedOptions,
    };
  });
}

export function updateOptionRecords(
  runtime: ProxyRuntimeContext,
  productId: string,
  options: ProductOptionRecord[],
  variants: ProductVariantRecord[],
  optionInput: Record<string, unknown>,
  optionValuesToAddRaw: unknown,
  optionValuesToUpdateRaw: unknown,
  optionValuesToDeleteRaw: unknown,
): { options: ProductOptionRecord[]; variants: ProductVariantRecord[] } | null {
  const rawOptionId = optionInput['id'];
  if (typeof rawOptionId !== 'string') {
    return null;
  }

  const existingIndex = options.findIndex((option) => option.id === rawOptionId && option.productId === productId);
  if (existingIndex < 0) {
    return null;
  }

  const nextOptions = options.map((option) => structuredClone(option));
  const existingTarget = nextOptions[existingIndex];
  if (!existingTarget) {
    return null;
  }

  const target = structuredClone(existingTarget);
  const previousOptionName = existingTarget.name;
  const renamedValues = new Map<string, string>();
  const rawName = optionInput['name'];
  if (typeof rawName === 'string' && rawName.trim()) {
    target.name = rawName;
  }

  const deleteIds = Array.isArray(optionValuesToDeleteRaw)
    ? optionValuesToDeleteRaw.filter((value): value is string => typeof value === 'string')
    : [];
  if (deleteIds.length > 0) {
    target.optionValues = target.optionValues.filter((value) => !deleteIds.includes(value.id));
  }

  if (Array.isArray(optionValuesToUpdateRaw)) {
    for (const rawValue of optionValuesToUpdateRaw) {
      if (!isObject(rawValue)) {
        continue;
      }

      const optionValueId = rawValue['id'];
      const optionValueName = rawValue['name'];
      if (typeof optionValueId !== 'string' || typeof optionValueName !== 'string' || !optionValueName.trim()) {
        continue;
      }

      const existingValue = target.optionValues.find((optionValue) => optionValue.id === optionValueId);
      if (existingValue) {
        renamedValues.set(existingValue.name, optionValueName);
        existingValue.name = optionValueName;
      }
    }
  }

  const optionValuesToAdd = readOptionValueCreateInputs(runtime, optionValuesToAddRaw);
  if (optionValuesToAdd.length > 0) {
    target.optionValues = [...target.optionValues, ...optionValuesToAdd];
  }

  nextOptions.splice(existingIndex, 1);
  const reorderedOptions = insertOptionAtPosition(nextOptions, target, optionInput['position']);
  const remappedVariants = remapVariantSelectionsForOptionUpdate(
    variants,
    previousOptionName,
    target.name,
    renamedValues,
  );
  return {
    options: reorderedOptions,
    variants: reorderVariantSelectionsForOptions(remappedVariants, reorderedOptions),
  };
}

export function deleteOptionRecords(
  productId: string,
  options: ProductOptionRecord[],
  rawOptionIds: unknown,
): { options: ProductOptionRecord[]; deletedOptionIds: string[] } {
  const optionIds = Array.isArray(rawOptionIds)
    ? rawOptionIds.filter((value): value is string => typeof value === 'string')
    : [];
  const deletedOptionIds = options
    .filter((option) => option.productId === productId && optionIds.includes(option.id))
    .map((option) => option.id);
  const nextOptions = options.filter((option) => !(option.productId === productId && optionIds.includes(option.id)));

  return {
    options: normalizeOptionPositions(nextOptions),
    deletedOptionIds,
  };
}

export function buildProductSetOptionRecords(
  runtime: ProxyRuntimeContext,
  productId: string,
  rawOptions: unknown,
): ProductOptionRecord[] {
  const existingOptions = runtime.store.getEffectiveOptionsByProductId(productId);
  const existingOptionsById = new Map(existingOptions.map((option) => [option.id, option]));

  if (!Array.isArray(rawOptions)) {
    return [];
  }

  return normalizeOptionPositions(
    rawOptions
      .filter((value): value is Record<string, unknown> => isObject(value))
      .map((value, index) => {
        const rawId = value['id'];
        const existing = typeof rawId === 'string' ? (existingOptionsById.get(rawId) ?? null) : null;
        const created = makeCreatedOptionRecord(runtime, productId, value);
        const optionValuesInput = Array.isArray(value['values']) ? value['values'] : [];
        const existingValuesById = new Map(
          (existing?.optionValues ?? []).map((optionValue) => [optionValue.id, optionValue]),
        );
        const optionValues = optionValuesInput
          .filter((entry): entry is Record<string, unknown> => isObject(entry))
          .map((entry) => {
            const rawValueId = entry['id'];
            const rawValueName = entry['name'];
            const existingValue = typeof rawValueId === 'string' ? (existingValuesById.get(rawValueId) ?? null) : null;
            return {
              id: existingValue?.id ?? runtime.syntheticIdentity.makeSyntheticGid('ProductOptionValue'),
              name:
                typeof rawValueName === 'string' && rawValueName.trim()
                  ? rawValueName
                  : (existingValue?.name ?? 'Option value'),
              hasVariants: existingValue?.hasVariants ?? false,
            };
          });

        return {
          id: existing?.id ?? created.id,
          productId,
          name:
            typeof value['name'] === 'string' && value['name'].trim()
              ? value['name']
              : (existing?.name ?? created.name),
          position: typeof value['position'] === 'number' ? value['position'] : index + 1,
          optionValues,
        };
      }),
  );
}
