import starlight from '@astrojs/starlight';
import { defineConfig } from 'astro/config';

export default defineConfig({
  site: 'https://airhorns.github.io',
  base: '/shopify-draft-proxy',
  integrations: [
    starlight({
      title: 'Shopify Draft Proxy',
      description: 'Docs for the Shopify Admin GraphQL digital twin / draft proxy.',
      lastUpdated: false,
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/airhorns/shopify-draft-proxy',
        },
      ],
      customCss: ['./src/styles/custom.css'],
      sidebar: [
        {
          label: 'Start Here',
          items: ['index', 'getting-started'],
        },
        {
          label: 'API Reference',
          items: [
            { label: 'JavaScript Library', slug: 'api/javascript' },
            { label: 'Ruby Gem', slug: 'api/ruby' },
            { label: 'Python Library', slug: 'api/python' },
            { label: 'HTTP Service', slug: 'api/http-service' },
          ],
        },
        {
          label: 'Endpoint Reference',
          items: [{ autogenerate: { directory: 'endpoints' } }],
        },
        {
          label: 'Operations',
          items: ['cli-guide', 'architecture', 'robustness'],
        },
      ],
    }),
  ],
});
