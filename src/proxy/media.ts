import type { ProxyRuntimeContext } from './runtime-context.js';
import { Kind, type FieldNode, type SelectionNode } from 'graphql';
import { getRootField, getRootFieldArguments, getRootFields } from '../graphql/root-field.js';
import type { FileRecord, ProductMediaRecord } from '../state/types.js';
import { paginateConnectionItems, serializeConnection } from './graphql-helpers.js';

interface FilesUserError {
  field: string[];
  message: string;
  code: string;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function getChildField(field: FieldNode, name: string): FieldNode | null {
  return (
    (field.selectionSet?.selections ?? []).find(
      (selection): selection is FieldNode => selection.kind === Kind.FIELD && selection.name.value === name,
    ) ?? null
  );
}

function readFilesInput(raw: unknown): Record<string, unknown>[] {
  return Array.isArray(raw) ? raw.filter((file): file is Record<string, unknown> => isObject(file)) : [];
}

function readFileUpdateInputs(raw: unknown): Record<string, unknown>[] {
  return Array.isArray(raw) ? raw.filter((file): file is Record<string, unknown> => isObject(file)) : [];
}

function readFileIdsInput(raw: unknown): string[] {
  return Array.isArray(raw) ? raw.filter((fileId): fileId is string => typeof fileId === 'string') : [];
}

function readStagedUploadInputs(raw: unknown): Record<string, unknown>[] {
  return Array.isArray(raw) ? raw.filter((input): input is Record<string, unknown> => isObject(input)) : [];
}

function readIdListInput(raw: unknown): string[] {
  return Array.isArray(raw) ? raw.filter((id): id is string => typeof id === 'string') : [];
}

function isValidUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === 'http:' || url.protocol === 'https:';
  } catch {
    return false;
  }
}

function deriveFilename(originalSource: string): string | null {
  try {
    const url = new URL(originalSource);
    const filename = url.pathname.split('/').filter(Boolean).at(-1);
    return filename && filename.length > 0 ? filename : null;
  } catch {
    return null;
  }
}

function makeSyntheticFileId(runtime: ProxyRuntimeContext, contentType: string | null): string {
  switch (contentType) {
    case 'IMAGE':
      return runtime.syntheticIdentity.makeSyntheticGid('MediaImage');
    case 'VIDEO':
      return runtime.syntheticIdentity.makeSyntheticGid('Video');
    case 'EXTERNAL_VIDEO':
      return runtime.syntheticIdentity.makeSyntheticGid('ExternalVideo');
    case 'MODEL_3D':
      return runtime.syntheticIdentity.makeSyntheticGid('Model3d');
    case 'FILE':
      return runtime.syntheticIdentity.makeSyntheticGid('GenericFile');
    default:
      return runtime.syntheticIdentity.makeSyntheticGid('File');
  }
}

function makeSyntheticStagedUploadId(runtime: ProxyRuntimeContext, index: number): string {
  return runtime.syntheticIdentity.makeSyntheticGid(`StagedUploadTarget${index}`);
}

const googleFormUploadParameterNames = [
  'Content-Type',
  'success_action_status',
  'acl',
  'key',
  'x-goog-date',
  'x-goog-credential',
  'x-goog-algorithm',
  'x-goog-signature',
  'policy',
] as const;

const googleSignedUploadParameterNames = ['GoogleAccessId', 'key', 'policy', 'signature'] as const;

function validateFileInput(input: Record<string, unknown>, index: number): FilesUserError[] {
  const errors: FilesUserError[] = [];
  const originalSource = input['originalSource'];
  const alt = input['alt'];

  if (typeof originalSource !== 'string' || originalSource.length === 0) {
    errors.push({
      field: ['files', String(index), 'originalSource'],
      message: 'Original source is required',
      code: 'REQUIRED',
    });
  } else if (!isValidUrl(originalSource)) {
    errors.push({
      field: ['files', String(index), 'originalSource'],
      message: 'Image URL is invalid',
      code: 'INVALID',
    });
  }

  if (typeof alt === 'string' && alt.length > 512) {
    errors.push({
      field: ['files', String(index), 'alt'],
      message: 'The alt value exceeds the maximum limit of 512 characters.',
      code: 'ALT_VALUE_LIMIT_EXCEEDED',
    });
  }

  return errors;
}

function validateFileUpdateInput(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  index: number,
): FilesUserError[] {
  const errors: FilesUserError[] = [];
  const id = input['id'];
  const alt = input['alt'];
  const originalSource = input['originalSource'];
  const previewImageSource = input['previewImageSource'];

  if (typeof id !== 'string' || id.length === 0) {
    errors.push({
      field: ['files', String(index), 'id'],
      message: 'File id is required',
      code: 'REQUIRED',
    });
  } else if (!runtime.store.hasEffectiveFileById(id)) {
    errors.push({
      field: ['files', String(index), 'id'],
      message: `File id ${id} does not exist.`,
      code: 'FILE_DOES_NOT_EXIST',
    });
  }

  if (typeof alt === 'string' && alt.length > 512) {
    errors.push({
      field: ['files', String(index), 'alt'],
      message: 'The alt value exceeds the maximum limit of 512 characters.',
      code: 'ALT_VALUE_LIMIT_EXCEEDED',
    });
  }

  if (typeof originalSource === 'string' && originalSource.length > 0 && !isValidUrl(originalSource)) {
    errors.push({
      field: ['files', String(index), 'originalSource'],
      message: 'Image URL is invalid',
      code: 'INVALID',
    });
  }

  if (typeof previewImageSource === 'string' && previewImageSource.length > 0 && !isValidUrl(previewImageSource)) {
    errors.push({
      field: ['files', String(index), 'previewImageSource'],
      message: 'Image URL is invalid',
      code: 'INVALID',
    });
  }

  if (
    typeof originalSource === 'string' &&
    originalSource.length > 0 &&
    typeof previewImageSource === 'string' &&
    previewImageSource.length > 0
  ) {
    errors.push({
      field: ['files', String(index)],
      message: 'Specify either originalSource or previewImageSource, not both.',
      code: 'INVALID',
    });
  }

  for (const productId of [
    ...readIdListInput(input['referencesToAdd']),
    ...readIdListInput(input['referencesToRemove']),
  ]) {
    if (!runtime.store.getEffectiveProductById(productId)) {
      errors.push({
        field: ['files', String(index), 'references'],
        message: `Product id ${productId} does not exist.`,
        code: 'INVALID',
      });
    }
  }

  return errors;
}

function makeFileRecord(runtime: ProxyRuntimeContext, input: Record<string, unknown>): FileRecord {
  const contentType = typeof input['contentType'] === 'string' ? input['contentType'] : null;
  const originalSource = typeof input['originalSource'] === 'string' ? input['originalSource'] : '';
  const filename = typeof input['filename'] === 'string' ? input['filename'] : deriveFilename(originalSource);

  return {
    id: makeSyntheticFileId(runtime, contentType),
    alt: typeof input['alt'] === 'string' ? input['alt'] : null,
    contentType,
    createdAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    fileStatus: 'UPLOADED',
    filename,
    originalSource,
    imageUrl: contentType === 'IMAGE' ? originalSource : null,
    imageWidth: null,
    imageHeight: null,
  };
}

function getEffectiveFileRecord(runtime: ProxyRuntimeContext, fileId: string): FileRecord | null {
  const state = runtime.store.getState();
  if (state.stagedState.deletedFileIds[fileId]) {
    return null;
  }

  return state.stagedState.files[fileId] ?? state.baseState.files[fileId] ?? null;
}

function mediaContentTypeToFileContentType(mediaContentType: string | null): string | null {
  switch (mediaContentType) {
    case 'IMAGE':
      return 'IMAGE';
    case 'VIDEO':
      return 'VIDEO';
    case 'EXTERNAL_VIDEO':
      return 'EXTERNAL_VIDEO';
    case 'MODEL_3D':
      return 'MODEL_3D';
    default:
      return mediaContentType;
  }
}

function fileContentTypeToMediaContentType(contentType: string | null): string | null {
  switch (contentType) {
    case 'IMAGE':
      return 'IMAGE';
    case 'VIDEO':
      return 'VIDEO';
    case 'EXTERNAL_VIDEO':
      return 'EXTERNAL_VIDEO';
    case 'MODEL_3D':
      return 'MODEL_3D';
    default:
      return contentType;
  }
}

function makeFileRecordFromProductMedia(runtime: ProxyRuntimeContext, media: ProductMediaRecord): FileRecord {
  const originalSource = media.sourceUrl ?? media.imageUrl ?? media.previewImageUrl ?? '';
  return {
    id: media.id ?? makeSyntheticFileId(runtime, mediaContentTypeToFileContentType(media.mediaContentType)),
    alt: media.alt,
    contentType: mediaContentTypeToFileContentType(media.mediaContentType),
    createdAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    fileStatus: media.status ?? 'READY',
    filename: deriveFilename(originalSource),
    originalSource,
    imageUrl: media.imageUrl ?? media.previewImageUrl,
    imageWidth: media.imageWidth ?? null,
    imageHeight: media.imageHeight ?? null,
  };
}

function getEffectiveProductMediaFileRecord(runtime: ProxyRuntimeContext, fileId: string): FileRecord | null {
  for (const product of runtime.store.listEffectiveProducts()) {
    const media = runtime.store
      .getEffectiveMediaByProductId(product.id)
      .find((mediaRecord) => mediaRecord.id === fileId);
    if (media) {
      return makeFileRecordFromProductMedia(runtime, media);
    }
  }

  return null;
}

function getEffectiveFileLikeRecord(runtime: ProxyRuntimeContext, fileId: string): FileRecord | null {
  return getEffectiveFileRecord(runtime, fileId) ?? getEffectiveProductMediaFileRecord(runtime, fileId);
}

function updateFileRecord(existing: FileRecord, input: Record<string, unknown>): FileRecord {
  const rawAlt = input['alt'];
  const rawFilename = input['filename'];
  const rawOriginalSource = input['originalSource'];
  const rawPreviewImageSource = input['previewImageSource'];
  const nextOriginalSource =
    typeof rawOriginalSource === 'string' && rawOriginalSource.length > 0 ? rawOriginalSource : existing.originalSource;
  const nextImageUrl =
    typeof rawOriginalSource === 'string' && rawOriginalSource.length > 0
      ? rawOriginalSource
      : typeof rawPreviewImageSource === 'string' && rawPreviewImageSource.length > 0
        ? rawPreviewImageSource
        : existing.imageUrl;

  return {
    ...structuredClone(existing),
    alt: typeof rawAlt === 'string' ? rawAlt : existing.alt,
    fileStatus: 'READY',
    filename: typeof rawFilename === 'string' ? rawFilename : existing.filename,
    originalSource: nextOriginalSource,
    imageUrl: existing.contentType === 'IMAGE' ? nextImageUrl : existing.imageUrl,
  };
}

function acknowledgeFileUpdateFailure(runtime: ProxyRuntimeContext, file: FileRecord): FileRecord {
  return {
    ...structuredClone(file),
    updateFailureAcknowledgedAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
  };
}

function makeProductMediaRecordFromFile(
  runtime: ProxyRuntimeContext,
  productId: string,
  file: FileRecord,
  position: number,
): ProductMediaRecord {
  return {
    key: `${productId}:media:${position}`,
    productId,
    position,
    id: file.id,
    mediaContentType: fileContentTypeToMediaContentType(file.contentType),
    alt: file.alt,
    status: file.fileStatus,
    productImageId: file.contentType === 'IMAGE' ? runtime.syntheticIdentity.makeSyntheticGid('ProductImage') : null,
    imageUrl: file.imageUrl,
    imageWidth: file.imageWidth,
    imageHeight: file.imageHeight,
    previewImageUrl: file.imageUrl,
    sourceUrl: file.originalSource,
  };
}

function updateProductMediaRecordFromFile(media: ProductMediaRecord, file: FileRecord): ProductMediaRecord {
  return {
    ...structuredClone(media),
    mediaContentType: fileContentTypeToMediaContentType(file.contentType),
    alt: file.alt,
    status: file.fileStatus,
    imageUrl: file.imageUrl,
    imageWidth: file.imageWidth,
    imageHeight: file.imageHeight,
    previewImageUrl: file.imageUrl,
    sourceUrl: file.originalSource,
  };
}

function nextMediaPosition(media: ProductMediaRecord[]): number {
  const positions = media.map((mediaRecord) => mediaRecord.position).filter((position) => Number.isFinite(position));
  return positions.length > 0 ? Math.max(...positions) + 1 : 0;
}

function stageProductMediaFileUpdate(
  runtime: ProxyRuntimeContext,
  file: FileRecord,
  input: Record<string, unknown>,
): void {
  const referencesToAdd = new Set(readIdListInput(input['referencesToAdd']));
  const referencesToRemove = new Set(readIdListInput(input['referencesToRemove']));
  const impactedProductIds = new Set<string>([...referencesToAdd, ...referencesToRemove]);

  for (const product of runtime.store.listEffectiveProducts()) {
    if (runtime.store.getEffectiveMediaByProductId(product.id).some((mediaRecord) => mediaRecord.id === file.id)) {
      impactedProductIds.add(product.id);
    }
  }

  for (const productId of impactedProductIds) {
    const existingMedia = runtime.store.getEffectiveMediaByProductId(productId);
    let changed = false;
    let nextMedia = existingMedia
      .filter((mediaRecord) => {
        const keep = mediaRecord.id !== file.id || !referencesToRemove.has(productId);
        changed ||= !keep;
        return keep;
      })
      .map((mediaRecord) => {
        if (mediaRecord.id !== file.id) {
          return mediaRecord;
        }

        changed = true;
        return updateProductMediaRecordFromFile(mediaRecord, file);
      });

    if (referencesToAdd.has(productId) && !nextMedia.some((mediaRecord) => mediaRecord.id === file.id)) {
      changed = true;
      nextMedia = [
        ...nextMedia,
        makeProductMediaRecordFromFile(runtime, productId, file, nextMediaPosition(nextMedia)),
      ];
    }

    if (changed) {
      runtime.store.replaceStagedMediaForProduct(productId, nextMedia);
    }
  }
}

function serializeFilesUserError(error: FilesUserError, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'field':
        result[key] = error.field;
        break;
      case 'message':
        result[key] = error.message;
        break;
      case 'code':
        result[key] = error.code;
        break;
      default:
        break;
    }
  }

  return result;
}

function serializeUserError(error: FilesUserError, selections: readonly SelectionNode[]): Record<string, unknown> {
  return serializeFilesUserError(error, selections);
}

function serializeImageSelectionSet(file: FileRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'url':
        result[key] = file.imageUrl;
        break;
      case 'width':
        result[key] = file.imageWidth;
        break;
      case 'height':
        result[key] = file.imageHeight;
        break;
      default:
        break;
    }
  }

  return result;
}

function inlineFragmentApplies(file: FileRecord, typeName: string): boolean {
  if (typeName === 'File') {
    return true;
  }

  return (
    (file.contentType === 'IMAGE' && typeName === 'MediaImage') ||
    (file.contentType === 'VIDEO' && typeName === 'Video') ||
    (file.contentType === 'EXTERNAL_VIDEO' && typeName === 'ExternalVideo') ||
    (file.contentType === 'MODEL_3D' && typeName === 'Model3d') ||
    (file.contentType === 'FILE' && typeName === 'GenericFile')
  );
}

function getFileTypename(file: FileRecord): string {
  switch (file.contentType) {
    case 'IMAGE':
      return 'MediaImage';
    case 'VIDEO':
      return 'Video';
    case 'EXTERNAL_VIDEO':
      return 'ExternalVideo';
    case 'MODEL_3D':
      return 'Model3d';
    case 'FILE':
      return 'GenericFile';
    default:
      return 'File';
  }
}

function serializeFileSelectionSet(file: FileRecord, selections: readonly SelectionNode[]): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind === Kind.INLINE_FRAGMENT) {
      const typeName = selection.typeCondition?.name.value;
      if (typeName && inlineFragmentApplies(file, typeName)) {
        Object.assign(result, serializeFileSelectionSet(file, selection.selectionSet.selections));
      }
      continue;
    }

    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case '__typename':
        result[key] = getFileTypename(file);
        break;
      case 'id':
        result[key] = file.id;
        break;
      case 'alt':
        result[key] = file.alt;
        break;
      case 'createdAt':
        result[key] = file.createdAt;
        break;
      case 'fileStatus':
        result[key] = file.fileStatus;
        break;
      case 'filename':
        result[key] = file.filename;
        break;
      case 'image':
        if (file.contentType === 'IMAGE') {
          result[key] =
            file.fileStatus === 'READY'
              ? serializeImageSelectionSet(file, selection.selectionSet?.selections ?? [])
              : null;
        }
        break;
      default:
        break;
    }
  }

  return result;
}

function serializeFilesConnection(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const files = runtime.store.listEffectiveFiles();
  const { items, hasNextPage, hasPreviousPage } = paginateConnectionItems(files, field, variables, (file) => file.id);

  return serializeConnection(field, {
    items,
    hasNextPage,
    hasPreviousPage,
    getCursorValue: (file) => file.id,
    serializeNode: (file, selection) => serializeFileSelectionSet(file, selection.selectionSet?.selections ?? []),
  });
}

function serializeEmptyConnection(field: FieldNode): Record<string, unknown> {
  return serializeConnection(field, {
    items: [],
    hasNextPage: false,
    hasPreviousPage: false,
    getCursorValue: () => '',
    serializeNode: () => null,
  });
}

function validateStagedUploadInput(input: Record<string, unknown>, index: number): FilesUserError[] {
  const errors: FilesUserError[] = [];

  for (const fieldName of ['filename', 'mimeType', 'resource']) {
    if (typeof input[fieldName] !== 'string' || input[fieldName].length === 0) {
      errors.push({
        field: ['input', String(index), fieldName],
        message: `${fieldName} is required`,
        code: 'REQUIRED',
      });
    }
  }

  return errors;
}

function serializeStagedUploadParameters(
  parametersField: FieldNode | null,
  parameters: Array<{ name: string; value: string }>,
): Record<string, string>[] {
  const selections = parametersField?.selectionSet?.selections ?? [];
  return parameters.map((parameter) => {
    const result: Record<string, string> = {};
    for (const selection of selections) {
      if (selection.kind !== Kind.FIELD) {
        continue;
      }

      const key = selection.alias?.value ?? selection.name.value;
      switch (selection.name.value) {
        case 'name':
          result[key] = parameter.name;
          break;
        case 'value':
          result[key] = parameter.value;
          break;
        default:
          break;
      }
    }
    return result;
  });
}

function serializeStagedTarget(
  target: { url: string; resourceUrl: string; parameters: Array<{ name: string; value: string }> },
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const parametersField =
    selections.find(
      (selection): selection is FieldNode => selection.kind === Kind.FIELD && selection.name.value === 'parameters',
    ) ?? null;

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'url':
        result[key] = target.url;
        break;
      case 'resourceUrl':
        result[key] = target.resourceUrl;
        break;
      case 'parameters':
        result[key] = serializeStagedUploadParameters(parametersField, target.parameters);
        break;
      default:
        break;
    }
  }

  return result;
}

function makeStagedTarget(
  runtime: ProxyRuntimeContext,
  input: Record<string, unknown>,
  index: number,
): {
  url: string;
  resourceUrl: string;
  parameters: Array<{ name: string; value: string }>;
} {
  const id = makeSyntheticStagedUploadId(runtime, index);
  const filename = typeof input['filename'] === 'string' ? input['filename'] : `upload-${index}`;
  const mimeType = typeof input['mimeType'] === 'string' ? input['mimeType'] : 'application/octet-stream';
  const resource = typeof input['resource'] === 'string' ? input['resource'] : 'FILE';
  const method = typeof input['httpMethod'] === 'string' ? input['httpMethod'] : 'POST';
  const encodedId = encodeURIComponent(id);
  const encodedFilename = encodeURIComponent(filename);
  const key = `shopify-draft-proxy/${id}/${filename}`;

  let parameters: Array<{ name: string; value: string }>;
  switch (resource) {
    case 'IMAGE':
    case 'FILE':
      parameters = googleFormUploadParameterNames.map((name) => {
        switch (name) {
          case 'Content-Type':
            return { name, value: mimeType };
          case 'success_action_status':
            return { name, value: '201' };
          case 'acl':
            return { name, value: 'private' };
          case 'key':
            return { name, value: key };
          case 'x-goog-algorithm':
            return { name, value: 'GOOG4-RSA-SHA256' };
          default:
            return { name, value: `shopify-draft-proxy-placeholder-${name}` };
        }
      });
      break;
    case 'VIDEO':
    case 'MODEL_3D':
      parameters = googleSignedUploadParameterNames.map((name) => {
        if (name === 'key') {
          return { name, value: key };
        }

        return { name, value: `shopify-draft-proxy-placeholder-${name}` };
      });
      break;
    default:
      parameters = [
        { name: 'key', value: key },
        { name: 'Content-Type', value: mimeType },
        { name: 'x-shopify-draft-proxy-resource', value: resource },
        { name: 'x-shopify-draft-proxy-http-method', value: method },
      ];
      break;
  }

  return {
    url: `https://shopify-draft-proxy.local/staged-uploads/${encodedId}`,
    resourceUrl: `https://shopify-draft-proxy.local/staged-uploads/${encodedId}/${encodedFilename}`,
    parameters,
  };
}

export function handleMediaQuery(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};

  for (const field of getRootFields(document)) {
    const responseKey = field.alias?.value ?? field.name.value;
    switch (field.name.value) {
      case 'files':
        data[responseKey] = serializeFilesConnection(runtime, field, variables);
        break;
      case 'fileSavedSearches':
        data[responseKey] = serializeEmptyConnection(field);
        break;
      default:
        data[responseKey] = null;
        break;
    }
  }

  return { data };
}

export function handleMediaMutation(
  runtime: ProxyRuntimeContext,
  query: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const field = getRootField(query);
  const args = getRootFieldArguments(query, variables);
  const responseKey = field.alias?.value ?? field.name.value;

  switch (field.name.value) {
    case 'fileCreate': {
      const files = readFilesInput(args['files']);
      const userErrors = files.flatMap((file, index) => validateFileInput(file, index));
      const filesField = getChildField(field, 'files');
      const userErrorsField = getChildField(field, 'userErrors');

      if (userErrors.length > 0) {
        return {
          data: {
            [responseKey]: {
              files: [],
              userErrors: userErrors.map((error) =>
                serializeFilesUserError(error, userErrorsField?.selectionSet?.selections ?? []),
              ),
            },
          },
        };
      }

      const createdFiles = runtime.store.stageCreateFiles(files.map((file) => makeFileRecord(runtime, file)));
      return {
        data: {
          [responseKey]: {
            files: createdFiles.map((file) =>
              serializeFileSelectionSet(file, filesField?.selectionSet?.selections ?? []),
            ),
            userErrors: [],
          },
        },
      };
    }
    case 'fileUpdate': {
      const files = readFileUpdateInputs(args['files']);
      const userErrors = files.flatMap((file, index) => validateFileUpdateInput(runtime, file, index));
      const filesField = getChildField(field, 'files');
      const userErrorsField = getChildField(field, 'userErrors');

      if (userErrors.length > 0) {
        return {
          data: {
            [responseKey]: {
              files: [],
              userErrors: userErrors.map((error) =>
                serializeFilesUserError(error, userErrorsField?.selectionSet?.selections ?? []),
              ),
            },
          },
        };
      }

      const updatedFiles = files.flatMap((fileInput) => {
        const id = fileInput['id'];
        if (typeof id !== 'string') {
          return [];
        }

        const existingFile = getEffectiveFileLikeRecord(runtime, id);
        if (!existingFile) {
          return [];
        }

        const nextFile = updateFileRecord(existingFile, fileInput);
        runtime.store.stageCreateFiles([nextFile]);
        stageProductMediaFileUpdate(runtime, nextFile, fileInput);
        return [nextFile];
      });

      return {
        data: {
          [responseKey]: {
            files: updatedFiles.map((file) =>
              serializeFileSelectionSet(file, filesField?.selectionSet?.selections ?? []),
            ),
            userErrors: [],
          },
        },
      };
    }
    case 'fileDelete': {
      const fileIds = readFileIdsInput(args['fileIds']);
      const deletedFileIdsField = getChildField(field, 'deletedFileIds');
      const userErrorsField = getChildField(field, 'userErrors');
      const missingFileId = fileIds.find((fileId) => !runtime.store.hasEffectiveFileById(fileId));
      const userErrors: FilesUserError[] = missingFileId
        ? [
            {
              field: ['fileIds'],
              message: `File id ${missingFileId} does not exist.`,
              code: 'FILE_DOES_NOT_EXIST',
            },
          ]
        : [];

      if (userErrors.length > 0) {
        return {
          data: {
            [responseKey]: {
              deletedFileIds: null,
              userErrors: userErrors.map((error) =>
                serializeFilesUserError(error, userErrorsField?.selectionSet?.selections ?? []),
              ),
            },
          },
        };
      }

      runtime.store.stageDeleteFiles(fileIds);
      return {
        data: {
          [responseKey]: {
            deletedFileIds: deletedFileIdsField ? fileIds : undefined,
            userErrors: [],
          },
        },
      };
    }
    case 'fileAcknowledgeUpdateFailed': {
      const fileIds = readFileIdsInput(args['fileIds']);
      const filesField = getChildField(field, 'files');
      const userErrorsField = getChildField(field, 'userErrors');
      const userErrors: FilesUserError[] = [];

      for (const fileId of fileIds) {
        const file = getEffectiveFileLikeRecord(runtime, fileId);
        if (!file) {
          userErrors.push({
            field: ['fileIds'],
            message: `File id ${fileId} does not exist.`,
            code: 'FILE_DOES_NOT_EXIST',
          });
          continue;
        }

        if (file.fileStatus !== 'READY') {
          userErrors.push({
            field: ['fileIds'],
            message: `File with id ${fileId} is not in the READY state.`,
            code: 'NON_READY_STATE',
          });
        }
      }

      if (userErrors.length > 0) {
        return {
          data: {
            [responseKey]: {
              files: null,
              userErrors: userErrors.map((error) =>
                serializeFilesUserError(error, userErrorsField?.selectionSet?.selections ?? []),
              ),
            },
          },
        };
      }

      const acknowledgedFiles = fileIds.flatMap((fileId) => {
        const file = getEffectiveFileLikeRecord(runtime, fileId);
        if (!file) {
          return [];
        }

        const acknowledgedFile = acknowledgeFileUpdateFailure(runtime, file);
        runtime.store.stageCreateFiles([acknowledgedFile]);
        return [acknowledgedFile];
      });

      return {
        data: {
          [responseKey]: {
            files: acknowledgedFiles.map((file) =>
              serializeFileSelectionSet(file, filesField?.selectionSet?.selections ?? []),
            ),
            userErrors: [],
          },
        },
      };
    }
    case 'stagedUploadsCreate': {
      const inputs = readStagedUploadInputs(args['input']);
      const userErrors = inputs.flatMap((input, index) => validateStagedUploadInput(input, index));
      const stagedTargetsField = getChildField(field, 'stagedTargets');
      const userErrorsField = getChildField(field, 'userErrors');

      if (userErrors.length > 0) {
        return {
          data: {
            [responseKey]: {
              stagedTargets: [],
              userErrors: userErrors.map((error) =>
                serializeUserError(error, userErrorsField?.selectionSet?.selections ?? []),
              ),
            },
          },
        };
      }

      return {
        data: {
          [responseKey]: {
            stagedTargets: inputs.map((input, index) =>
              serializeStagedTarget(
                makeStagedTarget(runtime, input, index),
                stagedTargetsField?.selectionSet?.selections ?? [],
              ),
            ),
            userErrors: [],
          },
        },
      };
    }
    default:
      return { data: { [responseKey]: null } };
  }
}
