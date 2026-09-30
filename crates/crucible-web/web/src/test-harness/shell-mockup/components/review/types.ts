/** States retained by the mock transcript fixtures; decisions live on proposal files. */
export type HunkState = 'pending' | 'accepted' | 'rejected' | 'absent';

/** Who made an edit: the session's title and identity colour. */
export interface HunkAuthor {
  title: string;
  color: string;
}
