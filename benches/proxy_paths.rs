use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use serde_json::{json, Value};
use shopify_draft_proxy::proxy::{Config, DraftProxy, ProductRecord, ReadMode, Request};

const GRAPHQL_PATH: &str = "/admin/api/2026-04/graphql.json";

fn snapshot_proxy() -> DraftProxy {
    DraftProxy::new(Config {
        read_mode: ReadMode::Snapshot,
        unsupported_mutation_mode: None,
        bulk_operation_run_mutation_max_input_file_size_bytes: None,
        port: 0,
        shopify_admin_origin: "https://shopify.com".to_string(),
        snapshot_path: None,
    })
}

fn request(method: &str, path: &str, body: &str) -> Request {
    Request {
        method: method.to_string(),
        path: path.to_string(),
        headers: Default::default(),
        body: body.to_string(),
    }
}

fn graphql_request(query: &str, variables: Value) -> Request {
    request(
        "POST",
        GRAPHQL_PATH,
        &json!({ "query": query, "variables": variables }).to_string(),
    )
}

fn product_record(index: usize) -> ProductRecord {
    ProductRecord {
        id: format!("gid://shopify/Product/{index}"),
        title: format!("Benchmark product {index:03}"),
        handle: format!("benchmark-product-{index:03}"),
        status: if index.is_multiple_of(3) {
            "DRAFT"
        } else {
            "ACTIVE"
        }
        .to_string(),
        description_html: format!("<p>Benchmark product {index:03}</p>"),
        vendor: "Benchmark Vendor".to_string(),
        product_type: "Benchmark Type".to_string(),
        tags: vec!["benchmark".to_string(), format!("bucket-{}", index % 5)],
        template_suffix: String::new(),
        seo_title: format!("Benchmark product {index:03}"),
        seo_description: format!("SEO description for benchmark product {index:03}"),
    }
}

fn seeded_proxy(product_count: usize) -> DraftProxy {
    snapshot_proxy().with_base_products((1..=product_count).map(product_record).collect())
}

fn staged_product_read_setup() -> (DraftProxy, Request) {
    let mut proxy = seeded_proxy(50);
    let create_response = proxy.process_request(graphql_request(
        r#"
        mutation CreateBenchmarkProduct($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id title handle status tags }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Bench staged product",
                "handle": "bench-staged-product",
                "status": "ACTIVE",
                "tags": ["benchmark", "staged"]
            }
        }),
    ));
    let product_id = create_response.body["data"]["productCreate"]["product"]["id"]
        .as_str()
        .expect("productCreate benchmark setup should return an id")
        .to_string();

    let read_request = graphql_request(
        r#"
        query ReadBenchmarkProduct($id: ID!) {
          product(id: $id) {
            id
            title
            handle
            status
            tags
          }
        }
        "#,
        json!({ "id": product_id }),
    );

    (proxy, read_request)
}

fn bench_meta_health(c: &mut Criterion) {
    let request = request("GET", "/__meta/health", "");

    c.bench_function("meta health route", |b| {
        b.iter_batched(
            snapshot_proxy,
            |mut proxy| black_box(proxy.process_request(request.clone())),
            BatchSize::SmallInput,
        )
    });
}

fn bench_product_create(c: &mut Criterion) {
    let create_request = graphql_request(
        r#"
        mutation CreateBenchmarkProduct($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product {
              id
              title
              handle
              status
              tags
              variants(first: 5) {
                nodes {
                  id
                  title
                  inventoryItem { id tracked }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "product": {
                "title": "Bench created product",
                "handle": "bench-created-product",
                "status": "ACTIVE",
                "tags": ["benchmark", "created"]
            }
        }),
    );

    c.bench_function("productCreate supported mutation", |b| {
        b.iter_batched(
            snapshot_proxy,
            |mut proxy| black_box(proxy.process_request(create_request.clone())),
            BatchSize::SmallInput,
        )
    });
}

fn bench_staged_product_read(c: &mut Criterion) {
    c.bench_function("product read after staged create", |b| {
        b.iter_batched(
            staged_product_read_setup,
            |(mut proxy, request)| black_box(proxy.process_request(request)),
            BatchSize::SmallInput,
        )
    });
}

fn bench_products_catalog_read(c: &mut Criterion) {
    let catalog_request = graphql_request(
        r#"
        query ProductsCatalog {
          products(first: 25, query: "tag:benchmark", sortKey: TITLE) {
            nodes {
              id
              title
              handle
              status
              tags
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          productsCount(query: "tag:benchmark") {
            count
            precision
          }
        }
        "#,
        json!({}),
    );

    c.bench_function("products catalog read from local state", |b| {
        b.iter_batched(
            || seeded_proxy(100),
            |mut proxy| black_box(proxy.process_request(catalog_request.clone())),
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    benches,
    bench_meta_health,
    bench_product_create,
    bench_staged_product_read,
    bench_products_catalog_read
);
criterion_main!(benches);
