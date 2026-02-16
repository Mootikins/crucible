// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	// GitHub Pages: https://mootikins.github.io/crucible/
	site: 'https://mootikins.github.io',
	base: '/crucible',

	integrations: [
		starlight({
			title: '⚗️ Crucible',
			description:
				'A local-first AI assistant where every conversation becomes a searchable note.',
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/Mootikins/crucible',
				},
			],
			favicon: '/favicon.svg',
			customCss: ['./src/styles/custom.css'],
			sidebar: [
				{
					label: 'Guides',
					autogenerate: { directory: 'guides' },
				},
				{
					label: 'Core Concepts',
					autogenerate: { directory: 'help/concepts' },
				},
				{
					label: 'CLI Reference',
					autogenerate: { directory: 'help/cli' },
				},
				{
					label: 'Configuration',
					autogenerate: { directory: 'help/config' },
				},
				{
					label: 'Terminal UI',
					autogenerate: { directory: 'help/tui' },
				},
				{
					label: 'Extending Crucible',
					autogenerate: { directory: 'help/extending' },
				},
				{
					label: 'Scripting',
					items: [
						{ label: 'Lua', autogenerate: { directory: 'help/lua' } },
						{ label: 'Plugins', autogenerate: { directory: 'help/plugins' } },
					],
				},
				{
					label: 'Advanced',
					items: [
						{ label: 'Query', autogenerate: { directory: 'help/query' } },
						{
							label: 'Workflows',
							autogenerate: { directory: 'help/workflows' },
						},
					],
				},
				{
					label: 'Reference',
					items: [
						{ slug: 'help/wikilinks' },
						{ slug: 'help/frontmatter' },
						{ slug: 'help/tags' },
						{ slug: 'help/block-references' },
						{ slug: 'help/rules-files' },
						{ slug: 'help/task-management' },
						{ slug: 'help/configuration' },
						{ slug: 'help/core/sessions' },
						{ slug: 'help/configuration/configuration-guide' },
					],
				},
			],
		}),
	],
});
