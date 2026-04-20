import { Kind, type FieldNode, type SelectionNode } from 'graphql';
import { getRootField, getRootFieldArguments } from '../graphql/root-field.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { FileRecord } from '../state/types.js';

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

function makeSyntheticFileId(contentType: string | null): string {
  switch (contentType) {
    case 'IMAGE':
      return makeSyntheticGid('MediaImage');
    case 'VIDEO':
      return makeSyntheticGid('Video');
    case 'EXTERNAL_VIDEO':
      return makeSyntheticGid('ExternalVideo');
    case 'MODEL_3D':
      return makeSyntheticGid('Model3d');
    case 'FILE':
      return makeSyntheticGid('GenericFile');
    default:
      return makeSyntheticGid('File');
  }
}

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

function makeFileRecord(input: Record<string, unknown>): FileRecord {
  const contentType = typeof input['contentType'] === 'string' ? input['contentType'] : null;
  const originalSource = typeof input['originalSource'] === 'string' ? input['originalSource'] : '';
  const filename = typeof input['filename'] === 'string' ? input['filename'] : deriveFilename(originalSource);

  return {
    id: makeSyntheticFileId(contentType),
    alt: typeof input['alt'] === 'string' ? input['alt'] : null,
    contentType,
    createdAt: makeSyntheticTimestamp(),
    fileStatus: 'READY',
    filename,
    originalSource,
    imageUrl: contentType === 'IMAGE' ? originalSource : null,
    imageWidth: null,
    imageHeight: null,
  };
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
        result[key] = file.contentType === 'IMAGE' ? 'MediaImage' : 'File';
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
      case 'image':
        if (file.contentType === 'IMAGE') {
          result[key] = serializeImageSelectionSet(file, selection.selectionSet?.selections ?? []);
        }
        break;
      default:
        break;
    }
  }

  return result;
}

export function handleMediaMutation(query: string, variables: Record<string, unknown>): Record<string, unknown> {
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

      const createdFiles = store.stageCreateFiles(files.map((file) => makeFileRecord(file)));
      return {
        data: {
          [responseKey]: {
            files: createdFiles.map((file) => serializeFileSelectionSet(file, filesField?.selectionSet?.selections ?? [])),
            userErrors: [],
          },
        },
      };
    }
    default:
      return { data: { [responseKey]: null } };
  }
}
