#!/usr/bin/env python3
"""Regenerate the ACP presentation-parity fixture pair.

The two files describe *one* agent behaviour told two ways:

  - `acp_parity_internal.jsonl`  — the internal (genai) agent running Crucible's
    own `edit_file` tool, gated and approved interactively by the user.
  - `acp_parity_delegated.jsonl` — the same edit performed inside a delegated
    ACP agent's own tool loop (`cru chat -a claude`).

Every field below was copied from a live capture of the daemon's broadcast
stream (`ReactorTestHarness` + `StreamingMockAgent` / `OwnsToolsMockAgent`),
not written from the docs — see
`docs/Meta/Plans/2026-08-03-acp-presentation-parity.md`, Task 7.

Run: python3 scripts/gen_acp_parity_fixtures.py
"""

import json
import pathlib

OUT = pathlib.Path(__file__).resolve().parent.parent / "assets" / "fixtures"

TS = "2026-08-03T09:14:07.000000+00:00"
OLD = 'fn main() {\n    println!("hello");\n}\n'
NEW = 'fn main() {\n    println!("hello, world");\n}\n'
DIFFS = [{"path": "greeting.rs", "old_content": OLD, "new_content": NEW}]
ARGS = {"path": "greeting.rs", "old_string": "hello", "new_string": "hello, world"}
DISPLAY = {"kind": "path", "primary": "greeting.rs"}
PREAMBLE = "I'll fix the greeting."
CODA = " Done."
FULL = PREAMBLE + CODA
MSG_ID = "msg-parity-0001"


def lines(session_id, events):
    out = []
    for seq, (event, data) in enumerate(events, start=1):
        out.append(
            json.dumps(
                {
                    "ts": TS,
                    "seq": seq,
                    "event": event,
                    "session_id": session_id,
                    "data": data,
                }
            )
        )
    header = json.dumps(
        {
            "version": 1,
            "session_id": session_id,
            "recording_mode": "granular",
            "started_at": TS,
        }
    )
    return header + "\n" + "\n".join(out) + "\n"


common_head = [
    ("user_message", {"message_id": MSG_ID, "content": "fix the greeting"}),
    ("text_delta", {"content": PREAMBLE}),
    ("segment_complete", {"message_id": MSG_ID, "index": 0, "content": PREAMBLE}),
]
common_tail = [
    ("text_delta", {"content": CODA}),
    ("message_complete", {"message_id": MSG_ID, "full_response": FULL}),
]

internal = common_head + [
    # The gate fires before the card: `edit_file` is not read-only, so the user
    # was asked and said yes. An interactive approval leaves no `auto_approved`
    # marker — only a rule/card grant does.
    (
        "interaction_requested",
        {
            "request_id": "perm-parity-0001",
            "request": {
                "kind": "permission",
                "action": {"type": "tool", "name": "edit_file", "args": ARGS},
            },
        },
    ),
    (
        "tool_call",
        {
            "call_id": "call-edit-1",
            "tool": "edit_file",
            "args": ARGS,
            # Registry description: `tool_call.rs` looks this up for Core tools.
            "description": "Edit file by replacing text. old_string must match exactly.",
            "source": "Core",
            "display": DISPLAY,
            # Synthesized up-front by `tools::diff_synth` and carried on the card.
            "diffs": DIFFS,
        },
    ),
    (
        "tool_result",
        {
            "call_id": "call-edit-1",
            "tool": "edit_file",
            # Doubly wrapped on purpose: the tool returns a JSON envelope and
            # the event wraps it again. `unwrap_json_result` unwraps it back to
            # the same text the delegated arm carries plain.
            "result": {"result": json.dumps({"result": "Replaced 1 occurrence(s)"})},
            "terminate": False,
        },
    ),
] + common_tail

delegated = common_head + [
    # No `interaction_requested`: the delegated agent ran its own gate in its
    # own process. No `description` and no `diffs` either — `stream.rs`'s
    # `agent_owns_tools` arm has neither, and Claude Code sends the initial
    # `tool_call` notification with empty content.
    (
        "tool_call",
        {
            "call_id": "call-edit-1",
            # ACP carries no tool name on the wire, only a prose `title`; the
            # client stores `humanize_tool_title(title)` as the name.
            "tool": "Edit File",
            "args": ARGS,
            "source": "Acp:claude",
            "display": DISPLAY,
        },
    ),
    # The late diff: Claude Code attaches `ToolCallContent::Diff` in a follow-up
    # `tool_call_update`, which becomes `StreamingChunk::ToolDiffUpdate` →
    # `TurnEvent::ToolCallDiffUpdate` → this event.
    ("tool_call_diff_update", {"call_id": "call-edit-1", "diffs": DIFFS}),
    (
        "tool_result",
        {
            "call_id": "call-edit-1",
            "tool": "Edit File",
            # `extract_tool_result` stringifies the agent's `rawOutput`.
            "result": {"result": "Replaced 1 occurrence(s)"},
            "terminate": False,
        },
    ),
] + common_tail

(OUT / "acp_parity_internal.jsonl").write_text(
    lines("chat-2026-08-03T0914-parint", internal)
)
(OUT / "acp_parity_delegated.jsonl").write_text(
    lines("chat-2026-08-03T0914-pardel", delegated)
)
print("wrote acp_parity_internal.jsonl and acp_parity_delegated.jsonl")
