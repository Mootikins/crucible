import { describe, it, expect } from 'vitest';
import { stopReasonNotice } from '../stop-reason';

describe('stopReasonNotice', () => {
  it('names the two reasons a reader needs told about', () => {
    expect(stopReasonNotice('max_tokens')).toContain('output limit');
    expect(stopReasonNotice('refusal')).toContain('declined');
  });

  it('says nothing about a turn that finished, or one with no reason', () => {
    expect(stopReasonNotice('end_turn')).toBeNull();
    expect(stopReasonNotice('cancelled')).toBeNull();
    expect(stopReasonNotice('empty')).toBeNull();
    expect(stopReasonNotice(undefined)).toBeNull();
  });

  // A newer daemon can name a reason this page has never heard of. Drawing the
  // raw word would put daemon vocabulary in front of a reader.
  it('says nothing about a reason it does not know', () => {
    expect(stopReasonNotice('some_future_reason')).toBeNull();
  });
});
