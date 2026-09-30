/** Changes is the session diff itself, with no intermediate file-list tab. */
import type { Component } from 'solid-js';
import { ReviewContainer } from './ReviewContainer';

export const ChangesContainer: Component<{ sid: string; path?: string }> = (props) => (
  <ReviewContainer source="record" sid={props.sid} path={props.path} />
);
