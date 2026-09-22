/**
 * Wire types for the comments of a session record.
 *
 * Every shape here is an alias into the generated contract. The files of a
 * session record are `DiffFileEntry` rows from `lib/diffset.ts`; the daemon no
 * longer lists hunks.
 */
import type { components } from './api-schema';

type Schemas = components['schemas'];

export type ReviewComment = Schemas['ReviewCommentRow'];
