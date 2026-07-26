/**
 * Drops a document's leading `# Heading`.
 *
 * Kiln notes open with an H1 because they are read as files, where nothing else
 * supplies a title. Starlight renders the frontmatter title as the page's H1, so
 * the note's own would be a duplicate — visually, in the table of contents, and
 * for anyone navigating by heading.
 *
 * The old converter did this with a regex over the raw source. Doing it on the
 * mdast means only a real leading heading matches: a `#` inside a fence, or one
 * that is not actually first, is left alone.
 */
export function remarkStripTitleHeading() {
	return (tree) => {
		const first = tree.children?.[0];
		if (first?.type === 'heading' && first.depth === 1) {
			tree.children.shift();
		}
	};
}

export default remarkStripTitleHeading;
