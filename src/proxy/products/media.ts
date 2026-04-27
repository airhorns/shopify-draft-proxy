import { getVariableDefinitionLocation, type GraphqlErrorLocation } from '../graphql-helpers.js';
import { makeSyntheticGid } from '../../state/synthetic-identity.js';
import type { ProductMediaRecord } from '../../state/types.js';

function makeSyntheticMediaId(mediaContentType: string | null | undefined): string {
  if (mediaContentType === 'IMAGE') {
    return makeSyntheticGid('MediaImage');
  }

  return makeSyntheticGid('Media');
}

function makeSyntheticProductImageId(mediaContentType: string | null | undefined): string | null {
  if (mediaContentType === 'IMAGE') {
    return makeSyntheticGid('ProductImage');
  }

  return null;
}

export const CREATE_MEDIA_CONTENT_TYPES = new Set(['VIDEO', 'EXTERNAL_VIDEO', 'MODEL_3D', 'IMAGE']);

export function isValidMediaSource(value: unknown): value is string {
  if (typeof value !== 'string' || !value.trim()) {
    return false;
  }

  try {
    const parsed = new URL(value);
    return parsed.protocol === 'http:' || parsed.protocol === 'https:';
  } catch {
    return false;
  }
}

export function mediaValidationProductNotFoundPayload(shape: 'create' | 'update' | 'delete') {
  if (shape === 'delete') {
    return {
      deletedMediaIds: null,
      deletedProductImageIds: null,
      mediaUserErrors: [{ field: ['productId'], message: 'Product does not exist' }],
      product: null,
    };
  }

  return {
    media: null,
    mediaUserErrors: [{ field: ['productId'], message: 'Product does not exist' }],
    ...(shape === 'create' ? { product: null } : {}),
  };
}

export function buildInvalidProductMediaContentTypeVariableError(
  media: unknown[],
  mediaIndex: number,
  mediaContentType: string,
  document: string,
): {
  errors: Array<{
    message: string;
    locations: GraphqlErrorLocation[];
    extensions: {
      code: 'INVALID_VARIABLE';
      value: unknown[];
      problems: Array<{ path: Array<string | number>; explanation: string }>;
    };
  }>;
} {
  const explanation = `Expected "${mediaContentType}" to be one of: VIDEO, EXTERNAL_VIDEO, MODEL_3D, IMAGE`;
  return {
    errors: [
      {
        message: `Variable $media of type [CreateMediaInput!]! was provided invalid value for ${mediaIndex}.mediaContentType (${explanation})`,
        locations: getVariableDefinitionLocation(document, 'media'),
        extensions: {
          code: 'INVALID_VARIABLE',
          value: structuredClone(media),
          problems: [{ path: [mediaIndex, 'mediaContentType'], explanation }],
        },
      },
    ],
  };
}

export function buildInvalidProductMediaProductIdVariableError(
  productId: string,
  document: string,
): {
  errors: Array<{
    message: string;
    locations: GraphqlErrorLocation[];
    extensions: {
      code: 'INVALID_VARIABLE';
      value: string;
      problems: Array<{ path: never[]; explanation: string; message: string }>;
    };
  }>;
} {
  const message = `Invalid global id '${productId}'`;
  return {
    errors: [
      {
        message: 'Variable $productId of type ID! was provided invalid value',
        locations: getVariableDefinitionLocation(document, 'productId'),
        extensions: {
          code: 'INVALID_VARIABLE',
          value: productId,
          problems: [{ path: [], explanation: message, message }],
        },
      },
    ],
  };
}

export function makeCreatedMediaRecord(
  productId: string,
  input: Record<string, unknown>,
  position: number,
): ProductMediaRecord {
  const rawMediaContentType = input['mediaContentType'];
  const mediaContentType = typeof rawMediaContentType === 'string' ? rawMediaContentType : 'IMAGE';
  const rawAlt = input['alt'];
  const rawOriginalSource = input['originalSource'];
  const sourceUrl = typeof rawOriginalSource === 'string' && rawOriginalSource.trim() ? rawOriginalSource : null;

  return {
    key: `${productId}:media:${position}`,
    productId,
    position,
    id: makeSyntheticMediaId(mediaContentType),
    mediaContentType,
    alt: typeof rawAlt === 'string' ? rawAlt : null,
    status: 'UPLOADED',
    productImageId: makeSyntheticProductImageId(mediaContentType),
    imageUrl: null,
    imageWidth: null,
    imageHeight: null,
    previewImageUrl: null,
    sourceUrl,
  };
}

export function transitionMediaToProcessing(media: ProductMediaRecord): ProductMediaRecord {
  return {
    ...structuredClone(media),
    status: 'PROCESSING',
    imageUrl: null,
    imageWidth: null,
    imageHeight: null,
    previewImageUrl: null,
  };
}

export function transitionMediaToReady(media: ProductMediaRecord): ProductMediaRecord {
  const readyUrl = media.sourceUrl ?? media.imageUrl ?? media.previewImageUrl ?? null;
  return {
    ...structuredClone(media),
    status: 'READY',
    imageUrl: readyUrl,
    previewImageUrl: readyUrl,
  };
}

export function updateMediaRecord(existing: ProductMediaRecord, input: Record<string, unknown>): ProductMediaRecord {
  const rawAlt = input['alt'];
  const rawPreviewImageSource = input['previewImageSource'];
  const rawOriginalSource = input['originalSource'];
  const nextImageUrl =
    typeof rawPreviewImageSource === 'string' && rawPreviewImageSource.trim()
      ? rawPreviewImageSource
      : typeof rawOriginalSource === 'string' && rawOriginalSource.trim()
        ? rawOriginalSource
        : (existing.imageUrl ?? existing.previewImageUrl ?? existing.sourceUrl ?? null);

  return {
    ...structuredClone(existing),
    alt: typeof rawAlt === 'string' ? rawAlt : existing.alt,
    status: 'READY',
    imageUrl: nextImageUrl,
    imageWidth: existing.imageWidth ?? null,
    imageHeight: existing.imageHeight ?? null,
    previewImageUrl: nextImageUrl,
    sourceUrl: existing.sourceUrl ?? nextImageUrl,
  };
}
