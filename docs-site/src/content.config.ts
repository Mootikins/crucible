import { defineCollection, z } from 'astro:content';
import { docsSchema } from '@astrojs/starlight/schema';
import { kilnDocsLoader } from './lib/kiln-loader.mjs';

export const collections = {
	docs: defineCollection({
		// Reads docs/Help and docs/Guides directly. There is no generated copy
		// any more — see src/lib/kiln-loader.mjs for why.
		loader: kilnDocsLoader(),
		schema: docsSchema({
			// Kiln notes carry frontmatter Starlight knows nothing about. These
			// are declared rather than dropped so a typo in one is still an
			// error, and so `status` stays available if the site ever wants to
			// mark a page as planned rather than implemented.
			extend: z.object({
				tags: z.array(z.string()).optional(),
				status: z.string().optional(),
				order: z.number().optional(),
			}),
		}),
	}),
};
