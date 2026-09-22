import { test, expect, type Locator, type Page } from '@playwright/test';
import { createStory } from './_helpers/story';
import { setupBasicMocks } from '../helpers/mock-api';
import { appReady, openSession } from '../helpers/nav';
import { MOCK_PROJECT, MOCK_SESSION } from '../helpers/fixtures';

/**
 * Story: a comment of the diff pane reaches the chat of that pane (WS-327).
 *
 * The user opens the branch diff of the project of the session, selects a
 * line range and writes a comment. **Comment** stores the comment through
 * `POST /api/diff/comment` AND puts a chip into the composer of the chat of
 * the pane. The chip carries the reference of the comment, so `POST
 * /api/chat/send` sends `{ id, source }` and never the text: the daemon
 * builds the context block.
 *
 * The chip and the stored comment are one thing. The `×` of the chip deletes
 * the comment through `POST /api/diff/comment/delete`, so the pane loses it
 * too. **Attach** on a comment with no chip puts the chip back. A comment
 * that a message already carried is the exception: the agent has it, so a
 * later `×` only drops the chip.
 *
 * The second story covers a pane with no chat: the comment still stores, and
 * the box says that no chat takes it.
 */

const ROOT = MOCK_PROJECT.path;
/** The section of `src/server.rs`, whose current text changes on line 17. */
const FILE = `diff-file-${ROOT}:src/server.rs`;
/** The source that `openDiff` builds for the working tree of the root. */
const SOURCE = { kind: 'branch', root: ROOT, base: '', head: null };

/** The number of one line of the current side, in the gutter of one file. */
function lineNumber(page: Page, line: number): Locator {
  return page.getByTestId(FILE).locator(`[data-testid="diff-line-${line}"]`);
}

/**
 * Select a range of lines with the pointer, as a user does: a press on the
 * first number, a move over the last, and a release. The box opens under the
 * last line of the range.
 */
async function selectLines(page: Page, first: number, last: number): Promise<void> {
  const start = await lineNumber(page, first).boundingBox();
  const end = await lineNumber(page, last).boundingBox();
  if (!start || !end) throw new Error(`no line numbers ${first} and ${last}`);
  await page.mouse.move(start.x + start.width / 2, start.y + start.height / 2);
  await page.mouse.down();
  await page.mouse.move(end.x + end.width / 2, end.y + end.height / 2);
  await page.mouse.up();
}

/** Open the Files tab of the right edge panel. */
async function openFilesPanel(page: Page): Promise<void> {
  // The store, not the pointer: the pointer path races the JS tween of the
  // panel (see root-dropdown-pick).
  await page.evaluate(() => {
    const store = (window as unknown as Record<string, any>).__windowStore;
    const actions = (window as unknown as Record<string, any>).__windowActions;
    if (store.edgePanels?.right?.mode !== 'docked') actions.toggleEdgePanel('right');
    const firstGroup = (node: any): string | null => {
      if (!node || typeof node !== 'object') return null;
      if (node.type === 'pane') return node.tabGroupId ?? null;
      return firstGroup(node.first) ?? firstGroup(node.second);
    };
    const groupId = firstGroup(store.edgePanels.right.layout);
    if (!groupId) throw new Error('right edge panel has no tab group');
    actions.setActiveTab(groupId, 'files-tab');
  });
  await expect(page.getByTestId('root-dropdown')).toBeVisible();
}

/** Open the branch diff of the browsed root from the Files panel header. */
async function openBranchDiff(page: Page): Promise<void> {
  const open = page.getByTestId('open-branch-diff');
  await expect(open).toBeVisible();
  await open.click();
  await expect(page.getByTestId(FILE)).toBeVisible();
}

/**
 * Put the diff tab in its own pane, to the right of the chat, so the story
 * shows the comment and the composer together.
 *
 * `splitPaneAndDrop` is the action that a drag of the tab onto the right half
 * of the pane calls. The drag itself belongs to the windowing specs; this
 * story needs the layout, not the pointer path.
 */
async function moveDiffBesideTheChat(page: Page): Promise<void> {
  await page.evaluate(() => {
    const store = (window as unknown as Record<string, any>).__windowStore;
    const actions = (window as unknown as Record<string, any>).__windowActions;
    const panes: Array<{ id: string; tabGroupId: string | null }> = [];
    const walk = (node: any): void => {
      if (!node || typeof node !== 'object') return;
      if (node.type === 'pane') return void panes.push(node);
      walk(node.first);
      walk(node.second);
    };
    walk(store.layout);
    for (const pane of panes) {
      const group = pane.tabGroupId ? store.tabGroups[pane.tabGroupId] : null;
      const tab = group?.tabs.find((t: { id: string }) => t.id.startsWith('tab-diff-'));
      if (group && tab) {
        actions.splitPaneAndDrop(pane.id, 'right', group.id, tab.id);
        return;
      }
    }
    throw new Error('no centre pane holds a diff tab');
  });
  await expect(page.getByTestId('chat-input')).toBeVisible();
  await expect(page.getByTestId(FILE)).toBeVisible();
}

/** Write one comment on the open box, and press Comment. */
async function writeComment(page: Page, text: string): Promise<void> {
  const box = page.getByTestId('diff-comment-box');
  await expect(box).toBeVisible();
  await box.getByTestId('diff-comment-input').fill(text);
  await box.getByTestId('diff-comment-submit').click();
}

test.describe('A comment of the diff pane goes to the chat of the pane', () => {
  test('the comment shows under its lines, and its chip rides the next message', async ({
    page,
  }, testInfo) => {
    const story = createStory(testInfo);
    const api = await setupBasicMocks(page, {
      projects: [{ ...MOCK_PROJECT, repository: { root: ROOT, is_worktree: false } }],
    });

    await page.goto('/');
    await appReady(page);
    await openSession(page, MOCK_SESSION.session_id);
    await openFilesPanel(page);
    await openBranchDiff(page);
    // The tree did its work. Close the rail, so the chat and the diff share
    // the width and the code does not wrap.
    await page.getByTestId('ribbon-toggle-right').click();
    await moveDiffBesideTheChat(page);

    // The pane names the chat that takes its comments: the open session.
    await expect(page.getByTestId('diff-chat-target')).toContainText('Test Session');
    await story.step(page, 'the diff pane opens and names its chat');

    await selectLines(page, 17, 19);
    await expect(page.getByTestId('diff-comment-box')).toContainText('Lines 17-19');
    await story.step(page, 'a drag over the line numbers opens the comment box');

    await writeComment(page, 'The timeout is now 10 seconds. Why?');

    // The comment is stored, and it shows under the lines it names.
    await expect(page.getByTestId(FILE).getByTestId('diff-comment')).toContainText(
      'The timeout is now 10 seconds. Why?',
    );
    // The chip of the composer of that chat carries the same comment.
    const chip = page.getByTestId('composer-attachment');
    await expect(chip).toHaveText(/server\.rs L17–19/);
    await story.step(page, 'the comment shows under its lines and a chip enters the composer');

    expect(api.comments).toHaveLength(1);
    expect(api.comments[0].line_range).toEqual({ start: 17, end: 20 });

    // The chip and the comment are one thing: the `×` deletes the comment,
    // so it leaves the pane as well.
    await page.getByTestId('composer-attachment-remove').click();
    await expect(chip).toHaveCount(0);
    await expect(page.getByTestId(FILE).getByTestId('diff-comment')).toHaveCount(0);
    expect(api.comments).toHaveLength(0);
    await story.step(page, 'the chip goes and the comment goes with it');

    // A second comment attaches again, and the message takes it. The pane
    // says that this comment is in the composer.
    await selectLines(page, 17, 17);
    await writeComment(page, 'Name the reason in a line of the doc comment.');
    await expect(chip).toHaveText(/server\.rs L17$/);
    const saved = page.getByTestId(FILE).getByTestId('diff-comment');
    await expect(saved.getByTestId('diff-comment-attached')).toBeVisible();
    await story.step(page, 'a second comment attaches its chip again');

    await page.getByTestId('chat-input').fill('Answer both comments, please.');
    await page.getByTestId('send-button').click();

    await expect.poll(() => api.sent.length).toBe(1);
    const sent = api.sent[0];
    expect(sent.content).toBe('Answer both comments, please.');
    // The reference only: the id and the source of the diffset. The daemon
    // reads the text of the comment from its own store.
    expect(sent.comments).toEqual([{ id: 'c-2', source: SOURCE }]);
    expect(JSON.stringify(sent)).not.toContain('Name the reason');
    // The sent chip leaves the composer. The stored comment does not change.
    await expect(chip).toHaveCount(0);
    expect(api.comments).toHaveLength(1);
    await story.step(page, 'the message carries the reference and the composer clears');

    // The pair works the other way: a comment with no chip attaches itself
    // again. The agent already received this one, so the `×` of that chip
    // only drops the chip.
    await saved.getByTestId('diff-comment-attach').click();
    await expect(chip).toHaveText(/server\.rs L17$/);
    await page.getByTestId('composer-attachment-remove').click();
    await expect(chip).toHaveCount(0);
    await expect(saved).toBeVisible();
    expect(api.comments).toHaveLength(1);
    await story.step(page, 'a sent comment attaches again, and its chip never deletes it');
  });

  test('with no chat, the comment stores and the box says that no chat takes it', async ({
    page,
  }, testInfo) => {
    const story = createStory(testInfo);
    const api = await setupBasicMocks(page, {
      projects: [{ ...MOCK_PROJECT, repository: { root: ROOT, is_worktree: false } }],
    });

    await page.goto('/');
    await appReady(page);
    // No session opens, so no chat is active and the pane names none. The
    // tree follows no session either, so the user browses the project.
    await openFilesPanel(page);
    await page.getByTestId('root-dropdown').click();
    await page.click('[role="listbox"] :text-is("project")');
    await openBranchDiff(page);
    await expect(page.getByTestId('diff-chat-target')).toContainText('No chat');

    await selectLines(page, 17, 17);
    const box = page.getByTestId('diff-comment-box');
    await expect(box.getByTestId('diff-comment-no-chat')).toContainText('No chat takes this');
    await story.step(page, 'the box of a pane with no chat says so');

    await writeComment(page, 'Nothing reads this, and the daemon keeps it.');

    await expect(page.getByTestId(FILE).getByTestId('diff-comment')).toContainText(
      'Nothing reads this, and the daemon keeps it.',
    );
    expect(api.comments).toHaveLength(1);
    await expect(page.getByTestId('composer-attachment')).toHaveCount(0);
    await story.step(page, 'the comment is stored, and no chip goes anywhere');
  });
});
