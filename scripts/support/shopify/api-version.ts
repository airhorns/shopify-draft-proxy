import manifest from '../../../config/admin-graphql/manifest.json' with { type: 'json' };

export const EXECUTABLE_ADMIN_API_VERSIONS = Object.freeze([...manifest.executableVersions]);
export const DEFAULT_ADMIN_API_VERSION = manifest.defaultVersion;

if (!EXECUTABLE_ADMIN_API_VERSIONS.includes(DEFAULT_ADMIN_API_VERSION)) {
  throw new Error(`Default Admin API version ${DEFAULT_ADMIN_API_VERSION} has no executable schema`);
}
