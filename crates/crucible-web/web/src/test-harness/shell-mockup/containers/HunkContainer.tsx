/** Legacy fixture blocks render as disk text; proposals do not overlay notes. */
import { Markdown } from '../components/primitives/Markdown';
import { hunkLines, state } from '../state';
export const HunkContainer = (props: { id: string }) => {
  const h = () => state.hunks[props.id];
  return (
    <Markdown
      source={(h()?.session === 's3' ? hunkLines(props.id).del : hunkLines(props.id).add).join(
        '\n',
      )}
    />
  );
};
