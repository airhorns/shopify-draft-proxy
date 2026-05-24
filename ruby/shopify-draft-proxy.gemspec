# frozen_string_literal: true

Gem::Specification.new do |spec|
  spec.name = "shopify-draft-proxy"
  spec.version = "0.1.0"
  spec.summary = "Ruby shim for the Shopify Draft Proxy Rust runtime"
  spec.description = "Starts and talks to the Rust Shopify Admin GraphQL draft proxy runtime from Ruby tests."
  spec.authors = ["Harphony"]
  spec.license = "Apache-2.0"
  spec.required_ruby_version = ">= 3.1"

  spec.files = Dir["lib/**/*.rb"] + ["README.md"]
  spec.require_paths = ["lib"]
end
