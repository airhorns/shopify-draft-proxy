import type { ProxyRuntimeContext } from './runtime-context.js';
import type { InMemoryStoreStateDumpV1 } from '../state/store.js';
import type { SyntheticIdentityStateDumpV1 } from '../state/synthetic-identity.js';
import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const FUNCTION_QUERY_ROOTS = new Set([
  'validation',
  'validations',
  'cartTransforms',
  'shopifyFunction',
  'shopifyFunctions',
]);

export const FUNCTION_MUTATION_ROOTS = new Set([
  'validationCreate',
  'validationUpdate',
  'validationDelete',
  'cartTransformCreate',
  'cartTransformDelete',
  'taxAppConfigure',
]);

const DRAFT_PROXY_STATE_DUMP_SCHEMA = 'shopify-draft-proxy/state-dump';
const FUNCTION_STATE_FIELDS = [
  'shopifyFunctions',
  'shopifyFunctionOrder',
  'validations',
  'validationOrder',
  'cartTransforms',
  'cartTransformOrder',
  'taxAppConfiguration',
  'deletedValidationIds',
  'deletedCartTransformIds',
] as const;

interface DraftProxyStateDump {
  schema: typeof DRAFT_PROXY_STATE_DUMP_SCHEMA;
  version: 1;
  createdAt: string;
  store: InMemoryStoreStateDumpV1;
  syntheticIdentity: SyntheticIdentityStateDumpV1;
  extensions: Record<string, unknown>;
}

type ReadMode = 'snapshot' | 'live-hybrid' | 'live' | 'passthrough';

interface BridgeConfig {
  readMode?: ReadMode | undefined;
  port?: number | undefined;
  shopifyAdminOrigin?: string | undefined;
}

interface GleamDraftProxy {
  config: unknown;
  synthetic_identity: unknown;
  store: unknown;
  registry: unknown;
}

interface GleamMutationOutcome {
  data: unknown;
  store: unknown;
  identity: unknown;
}

interface GleamDraftProxyModule {
  Config: new (readMode: unknown, port: number, shopifyAdminOrigin: string, snapshotPath: unknown) => unknown;
  DraftProxy: new (config: unknown, syntheticIdentity: unknown, store: unknown, registry: unknown) => GleamDraftProxy;
  Live: new () => unknown;
  LiveHybrid: new () => unknown;
  Snapshot: new () => unknown;
  dump_state(proxy: GleamDraftProxy, createdAt: string): unknown;
  restore_state(proxy: unknown, dumpJson: string): unknown;
  with_config(config: unknown): unknown;
  with_default_registry(proxy: unknown): unknown;
}

interface GleamFunctionsModule {
  process(store: unknown, document: string, variables: unknown): unknown;
  process_mutation(
    store: unknown,
    identity: unknown,
    requestPath: string,
    document: string,
    variables: unknown,
  ): unknown;
}

interface GleamRootFieldModule {
  BoolVal: new (value: boolean) => unknown;
  FloatVal: new (value: number) => unknown;
  IntVal: new (value: number) => unknown;
  ListVal: new (value: unknown) => unknown;
  NullVal: new () => unknown;
  ObjectVal: new (value: unknown) => unknown;
  StringVal: new (value: string) => unknown;
}

interface GleamOptionModule {
  None: new () => unknown;
}

interface GleamDictModule {
  new$(): unknown;
  insert(dict: unknown, key: string, value: unknown): unknown;
}

interface GleamJsonModule {
  to_string(value: unknown): string;
}

interface GleamPreludeModule {
  Result$Error$0(value: unknown): unknown;
  Result$Ok$0(value: unknown): unknown;
  Result$isOk(value: unknown): boolean;
}

interface GleamCoreModule {
  toList<T>(values: T[]): unknown;
}

function repoRootFromModule(): string {
  const here = dirname(fileURLToPath(import.meta.url));
  for (const candidate of [resolve(here, '..', '..'), resolve(here, '..', '..', '..')]) {
    if (existsSync(resolve(candidate, 'gleam', 'gleam.toml'))) {
      return candidate;
    }
  }
  throw new Error('Could not locate repository root for the Gleam Functions bridge.');
}

const repoRoot = repoRootFromModule();
const gleamRoot = resolve(repoRoot, 'gleam');
const gleamBuildRoot = resolve(gleamRoot, 'build', 'dev', 'javascript');
const draftProxyModulePath = resolve(
  gleamBuildRoot,
  'shopify_draft_proxy',
  'shopify_draft_proxy',
  'proxy',
  'draft_proxy.mjs',
);

function ensureGleamJavaScriptBuild(): void {
  if (existsSync(draftProxyModulePath)) {
    return;
  }
  execFileSync('gleam', ['build', '--target', 'javascript'], {
    cwd: gleamRoot,
    stdio: 'inherit',
  });
}

async function importGleamModule<T>(...segments: string[]): Promise<T> {
  return (await import(resolve(gleamBuildRoot, ...segments))) as T;
}

let draftProxy = null as unknown as GleamDraftProxyModule;
let functions = null as unknown as GleamFunctionsModule;
let rootField = null as unknown as GleamRootFieldModule;
let option = null as unknown as GleamOptionModule;
let dict = null as unknown as GleamDictModule;
let json = null as unknown as GleamJsonModule;
let prelude = null as unknown as GleamPreludeModule;
let gleamCore = null as unknown as GleamCoreModule;
let gleamModulesPromise: Promise<void> | undefined;

async function loadGleamModules(): Promise<void> {
  if (!gleamModulesPromise) {
    ensureGleamJavaScriptBuild();
    gleamModulesPromise = (async () => {
      draftProxy = await importGleamModule<GleamDraftProxyModule>(
        'shopify_draft_proxy',
        'shopify_draft_proxy',
        'proxy',
        'draft_proxy.mjs',
      );
      functions = await importGleamModule<GleamFunctionsModule>(
        'shopify_draft_proxy',
        'shopify_draft_proxy',
        'proxy',
        'functions.mjs',
      );
      rootField = await importGleamModule<GleamRootFieldModule>(
        'shopify_draft_proxy',
        'shopify_draft_proxy',
        'graphql',
        'root_field.mjs',
      );
      option = await importGleamModule<GleamOptionModule>('gleam_stdlib', 'gleam', 'option.mjs');
      dict = await importGleamModule<GleamDictModule>('gleam_stdlib', 'gleam', 'dict.mjs');
      json = await importGleamModule<GleamJsonModule>('gleam_json', 'gleam', 'json.mjs');
      prelude = await importGleamModule<GleamPreludeModule>('prelude.mjs');
      gleamCore = await importGleamModule<GleamCoreModule>('shopify_draft_proxy', 'gleam.mjs');
    })();
  }
  await gleamModulesPromise;
}

function unwrapResult<T>(result: unknown, action: string): T {
  if (prelude.Result$isOk(result)) {
    return prelude.Result$Ok$0(result) as T;
  }
  const error = prelude.Result$Error$0(result);
  throw new Error(`Gleam Functions ${action} failed: ${String(error)}`);
}

function jsonToPlain(value: unknown): Record<string, unknown> {
  return JSON.parse(json.to_string(value)) as Record<string, unknown>;
}

function readModeToGleam(mode: ReadMode | undefined): unknown {
  switch (mode ?? 'snapshot') {
    case 'live':
    case 'passthrough':
      return new draftProxy.Live();
    case 'live-hybrid':
      return new draftProxy.LiveHybrid();
    case 'snapshot':
      return new draftProxy.Snapshot();
  }
}

function createBaseGleamProxy(config: BridgeConfig | undefined): unknown {
  return draftProxy.with_default_registry(
    draftProxy.with_config(
      new draftProxy.Config(
        readModeToGleam(config?.readMode),
        config?.port ?? 4000,
        config?.shopifyAdminOrigin ?? 'https://shopify.com',
        new option.None(),
      ),
    ),
  );
}

function plainFieldValue(
  dump: InMemoryStoreStateDumpV1,
  fieldName: 'baseState' | 'stagedState',
): Record<string, unknown> {
  const field = dump.fields[fieldName];
  if (!field || field.kind !== 'plain' || typeof field.value !== 'object' || field.value === null) {
    throw new Error(`Expected plain ${fieldName} field in TypeScript runtime state dump.`);
  }
  return field.value as Record<string, unknown>;
}

function copyFunctionFields(target: Record<string, unknown>, source: Record<string, unknown>): void {
  for (const key of FUNCTION_STATE_FIELDS) {
    if (key in source) {
      target[key] = structuredClone(source[key]);
    }
  }
}

function createEmptyGleamStateDump(config: BridgeConfig | undefined): DraftProxyStateDump {
  return jsonToPlain(
    draftProxy.dump_state(createBaseGleamProxy(config) as GleamDraftProxy, new Date().toISOString()),
  ) as unknown as DraftProxyStateDump;
}

function dumpFunctionsStateForGleam(
  runtime: ProxyRuntimeContext,
  config: BridgeConfig | undefined,
): DraftProxyStateDump {
  const tsStoreDump = runtime.store.dumpRuntimeState();
  const gleamDump = createEmptyGleamStateDump(config);
  copyFunctionFields(plainFieldValue(gleamDump.store, 'baseState'), plainFieldValue(tsStoreDump, 'baseState'));
  copyFunctionFields(plainFieldValue(gleamDump.store, 'stagedState'), plainFieldValue(tsStoreDump, 'stagedState'));
  gleamDump.syntheticIdentity = runtime.syntheticIdentity.dumpState();
  return gleamDump;
}

function dumpRuntimeState(runtime: ProxyRuntimeContext, config: BridgeConfig | undefined): DraftProxyStateDump {
  return {
    schema: DRAFT_PROXY_STATE_DUMP_SCHEMA,
    version: 1,
    createdAt: new Date().toISOString(),
    store: dumpFunctionsStateForGleam(runtime, config).store,
    syntheticIdentity: runtime.syntheticIdentity.dumpState(),
    extensions: {},
  };
}

function restoreGleamProxy(runtime: ProxyRuntimeContext, config: BridgeConfig | undefined): GleamDraftProxy {
  return unwrapResult<GleamDraftProxy>(
    draftProxy.restore_state(createBaseGleamProxy(config), JSON.stringify(dumpRuntimeState(runtime, config))),
    'state restore',
  );
}

function applyGleamState(runtime: ProxyRuntimeContext, proxy: GleamDraftProxy): void {
  const gleamDump = jsonToPlain(
    draftProxy.dump_state(proxy, new Date().toISOString()),
  ) as unknown as DraftProxyStateDump;
  const tsStoreDump = runtime.store.dumpRuntimeState();
  copyFunctionFields(plainFieldValue(tsStoreDump, 'baseState'), plainFieldValue(gleamDump.store, 'baseState'));
  copyFunctionFields(plainFieldValue(tsStoreDump, 'stagedState'), plainFieldValue(gleamDump.store, 'stagedState'));
  runtime.store.restoreRuntimeState(tsStoreDump);
  runtime.syntheticIdentity.restoreState({
    version: 1,
    nextSyntheticId: gleamDump.syntheticIdentity.nextSyntheticId,
    nextSyntheticTimestamp: gleamDump.syntheticIdentity.nextSyntheticTimestamp,
  });
}

function variableToResolvedValue(value: unknown): unknown {
  if (value === null || value === undefined) {
    return new rootField.NullVal();
  }
  if (typeof value === 'string') {
    return new rootField.StringVal(value);
  }
  if (typeof value === 'boolean') {
    return new rootField.BoolVal(value);
  }
  if (typeof value === 'number') {
    return Number.isInteger(value) ? new rootField.IntVal(value) : new rootField.FloatVal(value);
  }
  if (Array.isArray(value)) {
    return new rootField.ListVal(gleamCore.toList(value.map((entry) => variableToResolvedValue(entry))));
  }
  if (typeof value === 'object') {
    let objectDict = dict.new$();
    for (const [key, entry] of Object.entries(value as Record<string, unknown>)) {
      objectDict = dict.insert(objectDict, key, variableToResolvedValue(entry));
    }
    return new rootField.ObjectVal(objectDict);
  }
  return new rootField.NullVal();
}

function variablesToGleam(variables: Record<string, unknown>): unknown {
  let result = dict.new$();
  for (const [key, value] of Object.entries(variables)) {
    result = dict.insert(result, key, variableToResolvedValue(value));
  }
  return result;
}

export async function handleFunctionQuery(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
  config?: BridgeConfig | undefined,
): Promise<Record<string, unknown>> {
  await loadGleamModules();
  const proxy = restoreGleamProxy(runtime, config);
  const envelope = unwrapResult<unknown>(
    functions.process(proxy.store, document, variablesToGleam(variables)),
    'query',
  );
  return jsonToPlain(envelope);
}

export async function handleFunctionMutation(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
  config?: BridgeConfig | undefined,
  requestPath = '/admin/api/2025-01/graphql.json',
): Promise<Record<string, unknown>> {
  await loadGleamModules();
  const proxy = restoreGleamProxy(runtime, config);
  const outcome = unwrapResult<GleamMutationOutcome>(
    functions.process_mutation(
      proxy.store,
      proxy.synthetic_identity,
      requestPath,
      document,
      variablesToGleam(variables),
    ),
    'mutation',
  );
  applyGleamState(runtime, new draftProxy.DraftProxy(proxy.config, outcome.identity, outcome.store, proxy.registry));
  return jsonToPlain(outcome.data);
}
