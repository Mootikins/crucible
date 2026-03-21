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
				'A knowledge-grounded agent runtime. Agents that draw from a knowledge graph make better decisions.',
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
					label: 'Getting Started',
					items: [
						{ slug: 'guides/getting-started' },
						{ slug: 'guides/your-first-kiln' },
						{ slug: 'guides/basic-commands' },
					],
				},
				{
					label: 'Core Concepts',
					items: [
						{ slug: 'help/concepts/kilns' },
						{ slug: 'help/concepts/precognition' },
						{ slug: 'help/concepts/the-knowledge-graph' },
						{ slug: 'help/concepts/semantic-search' },
						{ slug: 'help/concepts/plaintext-first' },
						{ slug: 'help/wikilinks' },
						{ slug: 'help/core/sessions' },
						{ slug: 'help/frontmatter' },
						{ slug: 'help/tags' },
						{ slug: 'help/block-references' },
					],
				},
				{
					label: 'Configuration',
					items: [
						{ slug: 'help/configuration' },
						{ slug: 'help/configuration/configuration-guide' },
						{ slug: 'help/config/llm' },
						{ slug: 'help/config/embedding' },
						{ slug: 'help/config/storage' },
						{ slug: 'help/config/workspaces' },
						{ slug: 'help/config/mcp' },
						{ slug: 'help/rules-files' },
					],
				},
				{
					label: 'Agent Integration',
					items: [
						{ slug: 'help/concepts/agents-and-protocols' },
						{ slug: 'help/extending/agent-cards' },
						{ slug: 'help/extending/internal-agent' },
						{ slug: 'help/config/agents' },
						{ slug: 'help/task-management' },
					],
				},
				{
					label: 'CLI Reference',
					items: [
						{ slug: 'help/cli' },
						{ slug: 'help/cli/chat' },
						{ slug: 'help/cli/process' },
						{ slug: 'help/cli/search' },
						{ slug: 'help/cli/stats' },
					],
				},
				{
					label: 'Terminal UI',
					items: [
						{ slug: 'help/tui' },
						{ slug: 'help/tui/keybindings' },
						{ slug: 'help/tui/modes' },
						{ slug: 'help/tui/commands' },
						{ slug: 'help/tui/shell-execution' },
						{ slug: 'help/tui/component-architecture' },
						{ slug: 'help/tui/e2e-testing' },
					],
				},
				{
					label: 'Extending Crucible',
					items: [
						{ slug: 'help/concepts/scripting-languages' },
						{ slug: 'help/extending/creating-plugins' },
						{ slug: 'help/extending/plugin-manifest' },
						{ slug: 'help/extending/custom-tools' },
						{ slug: 'help/extending/event-hooks' },
						{ slug: 'help/extending/custom-handlers' },
						{ slug: 'help/extending/markdown-handlers' },
						{ slug: 'help/extending/mcp-gateway' },
						{ slug: 'help/extending/scripted-ui' },
						{ slug: 'help/extending/script-agent-queries' },
						{ slug: 'help/extending/workflow-authoring' },
						{ slug: 'help/extending/http-module' },
						{
							label: 'Lua & Fennel',
							items: [
								{ slug: 'help/lua/language-basics' },
								{ slug: 'help/lua/configuration' },
								{ slug: 'help/plugins/lua-runtime-api' },
								{ slug: 'help/plugins/oil-lua-api' },
							],
						},
					],
				},
				{
					label: 'Advanced',
					items: [
						{ slug: 'help/query' },
						{ slug: 'help/query/query-system' },
						{ slug: 'help/workflows' },
						{ slug: 'help/workflows/workflow-syntax' },
						{ slug: 'help/workflows/markup' },
					],
				},
				{
					label: 'Guides',
					items: [
						{ slug: 'guides/session-search' },
						{ slug: 'guides/openrouter-setup' },
						{ slug: 'guides/github-copilot-setup' },
						{ slug: 'guides/zai-setup' },
						{ slug: 'guides/windows-setup' },
					],
				},
			],
		}),
	],
});
