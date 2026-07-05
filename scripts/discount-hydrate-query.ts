import { readFile } from 'node:fs/promises';

const hydrateQueryPattern = /pub\(super\) const DISCOUNT_HYDRATE_QUERY: &str = r#"(.*?)"#;/su;

export async function readDiscountHydrateDocument(): Promise<string> {
  const source = await readFile('src/proxy/discounts/hydrate_queries.rs', 'utf8');
  const match = hydrateQueryPattern.exec(source);
  const document = match?.[1];
  if (!document) {
    throw new Error('Could not extract DISCOUNT_HYDRATE_QUERY from src/proxy/discounts/hydrate_queries.rs');
  }
  return document;
}
