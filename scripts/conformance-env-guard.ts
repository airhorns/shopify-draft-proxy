const PLACEHOLDER_STORE_DOMAIN = 'your-store.myshopify.com';

type ConformanceTarget = {
  adminOrigin: string;
  storeDomain: string;
};

export function resolveConformanceTargetEnv(env: NodeJS.ProcessEnv = process.env): ConformanceTarget {
  const storeDomain = env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
  const adminOrigin = env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
  const missingVars = [
    !storeDomain ? 'SHOPIFY_CONFORMANCE_STORE_DOMAIN' : null,
    !adminOrigin ? 'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN' : null,
  ].filter(Boolean);

  if (missingVars.length > 0 || !storeDomain || !adminOrigin) {
    throw new Error(`Missing required environment variables: ${missingVars.join(', ')}`);
  }

  if (storeDomain === PLACEHOLDER_STORE_DOMAIN || adminOrigin === `https://${PLACEHOLDER_STORE_DOMAIN}`) {
    throw new Error(
      [
        'Conformance environment still contains .env.example placeholder store values.',
        'Link workspace .env to /home/airhorns/code/shopify-draft-proxy/.env or run `corepack pnpm workspace:ensure-env`.',
      ].join(' '),
    );
  }

  const expectedOrigin = `https://${storeDomain}`;
  if (adminOrigin !== expectedOrigin) {
    throw new Error(
      `Expected SHOPIFY_CONFORMANCE_ADMIN_ORIGIN=${expectedOrigin} to match SHOPIFY_CONFORMANCE_STORE_DOMAIN=${storeDomain}`,
    );
  }

  return { adminOrigin, storeDomain };
}
