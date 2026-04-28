use super::schema;
use shopify_function::prelude::*;
use shopify_function::Result;

#[shopify_function]
fn cart_validations_generate_run(
    _input: schema::cart_validations_generate_run::Input,
) -> Result<schema::CartValidationsGenerateRunResult> {
    let _ = _input;
    Ok(schema::CartValidationsGenerateRunResult { operations: vec![] })
}
