import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightBlog from 'starlight-blog';

export default defineConfig({
  integrations: [
    starlight({
      plugins: [
        starlightBlog({
          title: 'Blog',
          authors: {
            tama: {
              name: 'tama team',
              title: 'Core contributors',
              picture: '/favicon.svg',
              url: 'https://github.com/mlnja/tama',
            },
          },
        }),
      ],
      title: 'tama',
      description: 'Markdown-native AI agent orchestration',
      logo: {
        src: './src/assets/logo.svg',
        replacesTitle: false,
      },
      favicon: '/favicon.svg',
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/mlnja/tama' }],
      customCss: ['./src/styles/custom.css'],
      expressiveCode: { themes: ['github-light', 'github-dark'] },

      sidebar: [
        {
          label: 'Getting Started',
          items: [
            { label: 'Introduction', slug: 'getting-started/introduction' },
            { label: 'Installation', slug: 'getting-started/installation' },
            { label: 'Quickstart', slug: 'getting-started/quickstart' },
            { label: 'Core Concepts', slug: 'getting-started/concepts' },
          ],
        },
        {
          label: 'Patterns',
          items: [
            { label: 'Overview', slug: 'patterns/overview' },
            { label: 'oneshot', slug: 'patterns/oneshot' },
            { label: 'react', slug: 'patterns/react' },
            { label: 'scatter', slug: 'patterns/scatter' },
            { label: 'parallel', slug: 'patterns/parallel' },
            { label: 'fsm', slug: 'patterns/fsm' },
            { label: 'critic', slug: 'patterns/critic' },
            { label: 'reflexion', slug: 'patterns/reflexion' },
            { label: 'constitutional', slug: 'patterns/constitutional' },
            { label: 'chain-of-verification', slug: 'patterns/chain-of-verification' },
            { label: 'plan-execute', slug: 'patterns/plan-execute' },
            { label: 'debate', slug: 'patterns/debate' },
            { label: 'best-of-n', slug: 'patterns/best-of-n' },
            { label: 'human', slug: 'patterns/human' },
          ],
        },
        {
          label: 'Skills',
          items: [
            { label: 'Overview', slug: 'skills/overview' },
            { label: 'Writing a Skill', slug: 'skills/writing' },
          ],
        },
        {
          label: 'CLI Reference',
          items: [
            { label: 'tama init', slug: 'cli/init' },
            { label: 'tama add', slug: 'cli/add' },
            { label: 'tama lint', slug: 'cli/lint' },
            { label: 'tama brew', slug: 'cli/brew' },
            { label: 'tamad (runtime)', slug: 'cli/tamad' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'AGENT.md format', slug: 'reference/agent-md' },
            { label: 'SKILL.md format', slug: 'reference/skill-md' },
            { label: 'tama.toml', slug: 'reference/tama-toml' },
            { label: 'Step files', slug: 'reference/step-files' },
            { label: 'Model Configuration', slug: 'reference/models' },
          ],
        },
        {
          label: 'Guides',
          items: [
            { label: 'Tracing & Observability', slug: 'guides/tracing' },
            { label: 'Deploying with brew', slug: 'guides/deploy' },
          ],
        },
        {
          label: 'About',
          items: [
            { label: 'Competitor Analysis', slug: 'about/competitor-analysis' },
          ],
        },
      ],
    }),
  ],
});
