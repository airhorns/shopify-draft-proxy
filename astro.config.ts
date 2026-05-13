import starlight from '@astrojs/starlight';
import { defineConfig } from 'astro/config';

export default defineConfig({
  integrations: [
    starlight({
      title: 'Shopify Draft Proxy',
      description: 'Docs for the Shopify Admin GraphQL digital twin / draft proxy.',
      lastUpdated: false,
      sidebar: [
        {
          label: 'Start Here',
          items: ['index', 'getting-started'],
        },
        {
          label: 'API Reference',
          items: [
            { label: 'JavaScript Library', slug: 'api/javascript' },
            { label: 'Elixir Library', slug: 'api/elixir' },
            { label: 'HTTP Service', slug: 'api/http-service' },
          ],
        },
        {
          label: 'Operations',
          items: ['cli-guide', 'architecture', 'robustness'],
        },
      ],
    }),
  ],
});
