# frozen_string_literal: true

require "logger"

# CatalogImporter is a small, realistic Shopify app: it seeds a store's product
# catalog from a plain data structure (think a JSON/CSV feed) using the official
# `shopify_api` GraphQL Admin client.
#
# It is deliberately written the way a real embedder would write it — it knows
# nothing about the draft proxy. It just takes a `ShopifyAPI::Clients::Graphql::Admin`
# client and issues real Admin GraphQL operations. In tests we hand it a client
# whose HTTP layer is pointed at the in-process draft proxy; in production you'd
# hand it a client pointed at a real shop.
class CatalogImporter
  # Raised when Shopify (or the proxy standing in for it) reports the request
  # could not be fulfilled — either GraphQL-level `errors` or domain `userErrors`.
  class ImportError < StandardError
    attr_reader :user_errors

    def initialize(message, user_errors: [])
      super(message)
      @user_errors = user_errors
    end
  end

  # A product as it came back from the API after a successful create.
  ImportedProduct = Struct.new(:handle, :id, :title, keyword_init: true)

  CREATE_PRODUCT = <<~GRAPHQL
    mutation CreateProduct($input: ProductInput!) {
      productCreate(input: $input) {
        product { id title handle vendor productType status tags }
        userErrors { field message }
      }
    }
  GRAPHQL

  ADD_TAGS = <<~GRAPHQL
    mutation AddTags($id: ID!, $tags: [String!]!) {
      tagsAdd(id: $id, tags: $tags) {
        node { id }
        userErrors { field message }
      }
    }
  GRAPHQL

  UPDATE_STATUS = <<~GRAPHQL
    mutation UpdateStatus($input: ProductInput!) {
      productUpdate(input: $input) {
        product { id status }
        userErrors { field message }
      }
    }
  GRAPHQL

  FETCH_PRODUCT = <<~GRAPHQL
    query FetchProduct($id: ID!) {
      product(id: $id) {
        id title handle vendor productType status tags descriptionHtml
      }
    }
  GRAPHQL

  SEARCH_PRODUCTS = <<~GRAPHQL
    query SearchProducts($query: String!, $first: Int!) {
      products(first: $first, query: $query) {
        nodes { id title vendor status tags }
      }
    }
  GRAPHQL

  SAVE_SEARCH = <<~GRAPHQL
    mutation SaveSearch($input: SavedSearchCreateInput!) {
      savedSearchCreate(input: $input) {
        savedSearch { id name query resourceType }
        userErrors { field message }
      }
    }
  GRAPHQL

  def initialize(client:, logger: Logger.new(IO::NULL))
    @client = client
    @logger = logger
  end

  # Imports an array of product specs (symbol- or string-keyed hashes). Each spec
  # supports: :title (required), :handle, :description_html, :vendor,
  # :product_type, :status, :tags, and :extra_tags (applied via a second
  # `tagsAdd` call to model a follow-up enrichment step).
  #
  # Returns an array of ImportedProduct. Raises ImportError on the first failure
  # so a botched feed fails loudly rather than silently half-importing.
  def import(products)
    products.map { |spec| import_one(symbolize(spec)) }
  end

  # Reads a single product back by id (read-after-write).
  def fetch(id)
    execute(FETCH_PRODUCT, { id: id }, "product")
  end

  # Returns products matching an Admin search query, e.g. "vendor:Northwind".
  def search(query, first: 50)
    execute(SEARCH_PRODUCTS, { query: query, first: first }, "products").fetch("nodes")
  end

  # Saves a product saved search (a merchandising convenience a real importer
  # might create alongside the catalog).
  def save_search(name:, query:)
    execute(
      SAVE_SEARCH,
      { input: { name: name, query: query, resourceType: "PRODUCT" } },
      "savedSearchCreate",
    ).fetch("savedSearch")
  end

  private

  def import_one(spec)
    # We deliberately do NOT pre-validate the title client-side: letting an empty
    # title travel to the API exercises the server's own `userErrors` handling,
    # which is exactly the round-trip we want to test against the draft proxy.
    input = {
      title: spec[:title],
      handle: spec[:handle],
      descriptionHtml: spec[:description_html],
      vendor: spec[:vendor],
      productType: spec[:product_type],
      status: spec[:status] || "ACTIVE",
      tags: Array(spec[:tags]),
    }.compact

    payload = execute(CREATE_PRODUCT, { input: input }, "productCreate")
    product = payload.fetch("product")
    @logger.info("created #{product["id"]} (#{product["title"]})")

    extra = Array(spec[:extra_tags])
    unless extra.empty?
      execute(ADD_TAGS, { id: product.fetch("id"), tags: extra }, "tagsAdd")
      @logger.info("tagged #{product["id"]} with #{extra.join(", ")}")
    end

    ImportedProduct.new(
      handle: product["handle"],
      id: product.fetch("id"),
      title: product.fetch("title"),
    )
  end

  # Runs one GraphQL operation through the official client and unwraps it the way
  # a careful embedder would: surface transport/GraphQL errors, then surface
  # domain userErrors, then return the named root field's payload.
  def execute(query, variables, root)
    response = @client.query(query: query, variables: variables)

    unless response.ok?
      raise ImportError, "Shopify returned HTTP #{response.code}"
    end

    body = response.body
    if (errors = body["errors"]) && !errors.empty?
      raise ImportError, "GraphQL errors: #{errors.map { |e| e["message"] }.join("; ")}"
    end

    payload = body.fetch("data").fetch(root)
    user_errors = payload.is_a?(Hash) ? Array(payload["userErrors"]) : []
    unless user_errors.empty?
      messages = user_errors.map { |e| "#{Array(e["field"]).join(".")}: #{e["message"]}" }
      raise ImportError.new("#{root} failed: #{messages.join("; ")}", user_errors: user_errors)
    end

    payload
  end

  def symbolize(spec)
    spec.each_with_object({}) { |(key, value), acc| acc[key.to_sym] = value }
  end
end
