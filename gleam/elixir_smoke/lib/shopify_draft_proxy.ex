defmodule ShopifyDraftProxy do
  @moduledoc """
  Thin Elixir-facing wrapper around the Gleam-compiled draft proxy.

  The wrapper keeps the Gleam `DraftProxy` value opaque, returns the next proxy
  state explicitly from request helpers, and exposes JSON response bodies as
  strings so Elixir applications can decode them with their JSON library of
  choice.
  """

  alias :shopify_draft_proxy@proxy@commit, as: Commit
  alias :shopify_draft_proxy@proxy@draft_proxy, as: DraftProxy

  @compile {:no_warn_undefined, {:shopify_draft_proxy@proxy@draft_proxy, :new, 0}}
  @compile {:no_warn_undefined, {:shopify_draft_proxy@proxy@draft_proxy, :with_config, 1}}
  @compile {:no_warn_undefined, {:shopify_draft_proxy@proxy@draft_proxy, :with_default_registry, 1}}
  @compile {:no_warn_undefined, {:shopify_draft_proxy@proxy@draft_proxy, :process_request, 2}}
  @compile {:no_warn_undefined, {:shopify_draft_proxy@proxy@draft_proxy, :default_graphql_path, 1}}
  @compile {:no_warn_undefined, {:shopify_draft_proxy@proxy@draft_proxy, :dump_state, 2}}
  @compile {:no_warn_undefined, {:shopify_draft_proxy@proxy@draft_proxy, :restore_state, 2}}
  @compile {:no_warn_undefined, {:shopify_draft_proxy@proxy@commit, :run_commit_sync, 4}}
  @compile {:no_warn_undefined, {:gleam@json, :to_string, 1}}

  defstruct [:raw]

  defmodule Response do
    @moduledoc false
    defstruct [:status, :body, :headers, :proxy]
  end

  defmodule CommitReport do
    @moduledoc false
    defstruct [:ok, :stop_index, :attempt_count, :raw, :proxy]
  end

  def new do
    %__MODULE__{raw: DraftProxy.new() |> DraftProxy.with_default_registry()}
  end

  def with_config(opts) when is_list(opts) do
    read_mode = Keyword.get(opts, :read_mode, :snapshot)
    port = Keyword.get(opts, :port, 4000)
    origin = Keyword.get(opts, :shopify_admin_origin, "https://shopify.com")
    snapshot_path = option(Keyword.get(opts, :snapshot_path))

    raw =
      {:config, read_mode, port, origin, snapshot_path}
      |> DraftProxy.with_config()
      |> DraftProxy.with_default_registry()

    %__MODULE__{raw: raw}
  end

  def request(%__MODULE__{raw: raw}, method, path, body \\ "", headers \\ %{}) do
    {response, next_raw} = DraftProxy.process_request(raw, {:request, method, path, headers, body})
    wrap_response(response, next_raw)
  end

  def graphql(%__MODULE__{} = proxy, query, opts \\ []) do
    api_version = Keyword.get(opts, :api_version, "2025-01")
    headers = Keyword.get(opts, :headers, %{})
    variables_json = Keyword.get(opts, :variables_json, "{}")
    path = DraftProxy.default_graphql_path(api_version)
    request(proxy, "POST", path, graphql_body(query, variables_json), headers)
  end

  def config(%__MODULE__{} = proxy), do: request(proxy, "GET", "/__meta/config")
  def log(%__MODULE__{} = proxy), do: request(proxy, "GET", "/__meta/log")
  def state(%__MODULE__{} = proxy), do: request(proxy, "GET", "/__meta/state")
  def reset(%__MODULE__{} = proxy), do: request(proxy, "POST", "/__meta/reset")
  def commit(%__MODULE__{} = proxy, headers \\ %{}), do: request(proxy, "POST", "/__meta/commit", "", headers)

  def commit_with(%__MODULE__{raw: {:draft_proxy, config, identity, store, registry, upstream_transport}}, origin, headers, send_fun) do
    {next_store, meta} = Commit.run_commit_sync(store, origin, headers, send_fun)
    next_proxy = %__MODULE__{raw: {:draft_proxy, config, identity, next_store, registry, upstream_transport}}
    {:meta_commit_response, ok, stop_index, attempts} = meta

    %CommitReport{
      ok: ok,
      stop_index: from_option(stop_index),
      attempt_count: length(attempts),
      raw: meta,
      proxy: next_proxy
    }
  end

  def dump_state(%__MODULE__{raw: raw}, created_at) do
    raw
    |> DraftProxy.dump_state(created_at)
    |> :gleam@json.to_string()
  end

  def restore_state(%__MODULE__{raw: raw}, dump_json) do
    case DraftProxy.restore_state(raw, dump_json) do
      {:ok, restored} -> {:ok, %__MODULE__{raw: restored}}
      {:error, reason} -> {:error, reason}
    end
  end

  defp wrap_response({:response, status, json_tree, headers}, next_raw) do
    %Response{
      status: status,
      body: :gleam@json.to_string(json_tree),
      headers: headers,
      proxy: %__MODULE__{raw: next_raw}
    }
  end

  defp graphql_body(query, variables_json) do
    ~s({"query":"#{json_escape(query)}","variables":#{variables_json}})
  end

  defp json_escape(value) do
    value
    |> String.replace("\\", "\\\\")
    |> String.replace("\"", "\\\"")
    |> String.replace("\n", "\\n")
    |> String.replace("\r", "\\r")
    |> String.replace("\t", "\\t")
  end

  defp option(nil), do: :none
  defp option(value), do: {:some, value}

  defp from_option(:none), do: nil
  defp from_option({:some, value}), do: value
end
