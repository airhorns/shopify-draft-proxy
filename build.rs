use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

fn main() {
    let root =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"));
    let admin_manifest_path = root.join("config/admin-graphql/manifest.json");
    // Storefront captures are protected Shopify evidence. Keep this runtime
    // route/version configuration beside, rather than inside, that directory.
    let storefront_manifest_path = root.join("config/storefront-graphql-manifest.json");
    println!("cargo:rerun-if-changed={}", admin_manifest_path.display());
    println!(
        "cargo:rerun-if-changed={}",
        storefront_manifest_path.display()
    );

    let admin = read_manifest(&admin_manifest_path);
    let storefront = read_manifest(&storefront_manifest_path);
    let admin_versions = string_array(&admin, "executableVersions", &admin_manifest_path);
    let storefront_versions =
        string_array(&storefront, "executableVersions", &storefront_manifest_path);
    let storefront_accepted =
        string_array(&storefront, "acceptedVersions", &storefront_manifest_path);
    let admin_default = string_value(&admin, "defaultVersion", &admin_manifest_path);
    let storefront_default = string_value(&storefront, "defaultVersion", &storefront_manifest_path);
    require_member(
        &admin_versions,
        &admin_default,
        "Admin defaultVersion",
        &admin_manifest_path,
    );
    require_member(
        &storefront_versions,
        &storefront_default,
        "Storefront defaultVersion",
        &storefront_manifest_path,
    );
    require_member(
        &storefront_accepted,
        &storefront_default,
        "Storefront acceptedVersions",
        &storefront_manifest_path,
    );

    let mut generated = String::new();
    generate_version_enum(
        &mut generated,
        VersionCatalog {
            enum_name: "AdminApiVersion",
            route_prefix: "/admin/api/",
            route_suffix: "/graphql.json",
            versions: &admin_versions,
            default_version: &admin_default,
            source_method: "schema_sdl",
            source_pattern: "config/admin-graphql/{version}/schema.graphql",
        },
    );
    generate_version_enum(
        &mut generated,
        VersionCatalog {
            enum_name: "StorefrontApiVersion",
            route_prefix: "/api/",
            route_suffix: "/graphql.json",
            versions: &storefront_versions,
            default_version: &storefront_default,
            source_method: "introspection_capture",
            source_pattern: "config/storefront-graphql/{version}/schema.json",
        },
    );
    writeln!(
        generated,
        "pub(crate) const STOREFRONT_ACCEPTED_API_VERSIONS: &[&str] = &[{}];",
        storefront_accepted
            .iter()
            .map(|version| format!("\"{version}\""))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();
    generated.push_str(
        "pub(crate) fn storefront_route_version_is_accepted(version: &str) -> bool {\n    STOREFRONT_ACCEPTED_API_VERSIONS.contains(&version)\n}\n",
    );

    let output = PathBuf::from(env::var("OUT_DIR").expect("Cargo sets OUT_DIR"))
        .join("graphql_surface_catalogs.rs");
    fs::write(output, generated).expect("write generated GraphQL surface catalogs");
}

struct VersionCatalog<'a> {
    enum_name: &'a str,
    route_prefix: &'a str,
    route_suffix: &'a str,
    versions: &'a [String],
    default_version: &'a str,
    source_method: &'a str,
    source_pattern: &'a str,
}

fn generate_version_enum(output: &mut String, catalog: VersionCatalog<'_>) {
    let variants = catalog
        .versions
        .iter()
        .map(|version| version_variant(version))
        .collect::<Vec<_>>();
    writeln!(
        output,
        "#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]\npub enum {} {{",
        catalog.enum_name
    )
    .unwrap();
    for variant in &variants {
        writeln!(output, "    {variant},").unwrap();
    }
    output.push_str("}\n");
    writeln!(output, "impl {} {{", catalog.enum_name).unwrap();
    writeln!(
        output,
        "    pub const ALL: [Self; {}] = [{}];",
        variants.len(),
        variants
            .iter()
            .map(|variant| format!("Self::{variant}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();
    writeln!(output, "    pub const COUNT: usize = {};", variants.len()).unwrap();
    writeln!(
        output,
        "    pub const DEFAULT: Self = Self::{};",
        version_variant(catalog.default_version)
    )
    .unwrap();
    output.push_str("    pub fn as_str(self) -> &'static str {\n        match self {\n");
    for (version, variant) in catalog.versions.iter().zip(&variants) {
        writeln!(output, "            Self::{variant} => \"{version}\",").unwrap();
    }
    output.push_str("        }\n    }\n");
    writeln!(
        output,
        "    pub fn from_route(path: &str) -> Option<Self> {{\n        let version = path.strip_prefix(\"{}\")?.strip_suffix(\"{}\")?;\n        Self::parse(version)\n    }}",
        catalog.route_prefix, catalog.route_suffix
    )
    .unwrap();
    output.push_str("    pub fn parse(version: &str) -> Option<Self> {\n        match version {\n");
    for (version, variant) in catalog.versions.iter().zip(&variants) {
        writeln!(
            output,
            "            \"{version}\" => Some(Self::{variant}),"
        )
        .unwrap();
    }
    output.push_str("            _ => None,\n        }\n    }\n");
    output.push_str("    pub(crate) const fn index(self) -> usize {\n        match self {\n");
    for (index, variant) in variants.iter().enumerate() {
        writeln!(output, "            Self::{variant} => {index},").unwrap();
    }
    output.push_str("        }\n    }\n");
    writeln!(
        output,
        "    pub(crate) fn {}(self) -> &'static str {{\n        match self {{",
        catalog.source_method
    )
    .unwrap();
    for (version, variant) in catalog.versions.iter().zip(&variants) {
        let source = catalog.source_pattern.replace("{version}", version);
        writeln!(
            output,
            "            Self::{variant} => include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{source}\")),"
        )
        .unwrap();
    }
    output.push_str("        }\n    }\n}\n");
    writeln!(
        output,
        "impl std::fmt::Display for {} {{\n    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{\n        formatter.write_str(self.as_str())\n    }}\n}}",
        catalog.enum_name
    )
    .unwrap();
}

fn read_manifest(path: &Path) -> Value {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("could not parse {}: {error}", path.display()))
}

fn string_array(manifest: &Value, key: &str, path: &Path) -> Vec<String> {
    manifest
        .get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} must contain a {key} array", path.display()))
        .iter()
        .map(|value| {
            value
                .as_str()
                .unwrap_or_else(|| panic!("{key} entries in {} must be strings", path.display()))
                .to_string()
        })
        .collect()
}

fn string_value(manifest: &Value, key: &str, path: &Path) -> String {
    manifest
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{} must contain a string {key}", path.display()))
        .to_string()
}

fn require_member(values: &[String], expected: &str, label: &str, path: &Path) {
    assert!(
        values.iter().any(|value| value == expected),
        "{label} {expected} is not listed in {}",
        path.display()
    );
}

fn version_variant(version: &str) -> String {
    let mut variant = String::from("V");
    for character in version.chars() {
        if character.is_ascii_alphanumeric() {
            variant.push(character);
        } else {
            variant.push('_');
        }
    }
    variant
}
