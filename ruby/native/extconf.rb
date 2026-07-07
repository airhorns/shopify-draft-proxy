# frozen_string_literal: true

require "mkmf"
require "rb_sys/mkmf"

# Builds the magnus cdylib in this directory (native/Cargo.toml) and installs it
# at the `shopify_draft_proxy/shopify_draft_proxy_native` load path, where
# lib/shopify_draft_proxy.rb requires it. Bundler runs this at `bundle install`
# for git/rubygems sources (it does not for `path:` gems).
create_rust_makefile("shopify_draft_proxy/shopify_draft_proxy_native")
