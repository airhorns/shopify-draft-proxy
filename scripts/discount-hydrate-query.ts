import { readFile } from 'node:fs/promises';

export type DiscountHydrateKind = 'automatic' | 'code';

const hydrateQueryPatterns: Record<DiscountHydrateKind, RegExp> = {
  automatic: /const DISCOUNT_AUTOMATIC_HYDRATE_QUERY: &str = r#"(.*?)"#;/su,
  code: /const DISCOUNT_CODE_HYDRATE_QUERY: &str = r#"(.*?)"#;/su,
};

export async function readDiscountHydrateDocument(kind: DiscountHydrateKind = 'code'): Promise<string> {
  const source = await readFile('src/proxy/discounts/hydrate_queries.rs', 'utf8');
  const match = hydrateQueryPatterns[kind].exec(source);
  const document = match?.[1];
  if (!document) {
    throw new Error(`Could not extract the ${kind} discount hydrate query from src/proxy/discounts/hydrate_queries.rs`);
  }
  return document;
}
