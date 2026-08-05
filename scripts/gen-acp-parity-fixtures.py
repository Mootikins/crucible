#!/usr/bin/env python3
"""Regenerate the ACP presentation-parity fixture pairs.

Each pair describes *one* agent behaviour told two ways:

  - `acp_parity_internal.jsonl`       — the internal (genai) agent running
    Crucible's own `edit_file` tool, gated and approved interactively.
  - `acp_parity_delegated.jsonl`      — the same edit performed inside a
    delegated ACP agent's own tool loop (`cru chat -a claude`).
  - `acp_parity_read_internal.jsonl`  — the internal agent running `read_file`.
  - `acp_parity_read_delegated.jsonl` — the same read, delegated.

The edit pair's result is 23 characters on one line, which *every* tool's card
collapses the same way, so it proves parity only for one benign shape. The read
pair is the one that bites: a multi-line result is summarized by a table keyed
on the tool name, and the internal snake_case name and the ACP prose title are
not the same string (divergence **A4**).

**Provenance — these are not transcripts of a real `claude` session.** Each
event was emitted by the daemon's own broadcast channel, driven by
`ReactorTestHarness` with two mock agents: `StreamingMockAgent` for the
internal arm (against the real `WorkspaceTools` dispatcher and the real
permission gate) and `OwnsToolsMockAgent` with `agent_name: "claude"` for the
delegated arm. The mocks supply the `TurnEvent`s a real handle would; every
field downstream of them — the `display` object, the `Core`/`Acp:claude`
source, the doubly-wrapped internal result envelope, `terminate: false` — is
the daemon's own work, not this script's invention.

That capture is not a one-off. `agent_manager::tests::parity_capture` reruns
it on every test run and fails when the daemon stops emitting these bytes, so
a fixture cannot quietly outlive the shape it pins. Run this script only after
that test tells you the shape legitimately changed.

See `docs/Meta/Plans/2026-08-03-acp-presentation-parity.md`, Task 7 and A4.

Run: python3 scripts/gen-acp-parity-fixtures.py
"""

import json
import pathlib

OUT = pathlib.Path(__file__).resolve().parent.parent / "assets" / "fixtures"

TS = "2026-08-03T09:14:07.000000+00:00"
OLD = 'fn main() {\n    println!("hello");\n}\n'
NEW = 'fn main() {\n    println!("hello, world");\n}\n'
DIFFS = [{"path": "greeting.rs", "old_content": OLD, "new_content": NEW}]
ARGS = {"path": "greeting.rs", "old_string": "hello", "new_string": "hello, world"}
# A delegated Claude Code edit arrives as `rawInput` keyed `file_path`, not
# `path`. Both are in `ToolDisplay`'s `PATH_KEYS`, so the card shows
# `greeting.rs` either way — keeping each side's real key means the pair tests
# that rather than assuming it.
ARGS_ACP = {
    "file_path": "greeting.rs",
    "old_string": "hello",
    "new_string": "hello, world",
}
DISPLAY = {"kind": "path", "primary": "greeting.rs"}
PREAMBLE = "I'll fix the greeting."
CODA = " Done."
FULL = PREAMBLE + CODA
MSG_ID = "msg-parity-0001"


def compact(value):
    """Serialize as the daemon does.

    A dispatched tool's result reaches the event as
    `serde_json::Value::to_string()`, which is compact. Python's default
    `json.dumps` puts a space after `:` and would pin a shape the daemon never
    emits.
    """
    return json.dumps(value, separators=(",", ":"))


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
    # marker — only a rule/card grant does. The request carries the same
    # synthesized diffs the card does: `handle_permission_request` builds
    # `PermRequest::tool(..).with_diffs(synthesize_diffs(..))`, the same call
    # that put `diffs` on the `tool_call`.
    (
        "interaction_requested",
        {
            "request_id": "perm-parity-0001",
            "request": {
                "kind": "permission",
                "action": {"type": "tool", "name": "edit_file", "args": ARGS},
                "diffs": DIFFS,
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
            # Doubly wrapped on purpose: `WorkspaceTools` answers
            # `{"result": text}` and `tool_call.rs` wraps that value's
            # `to_string()` again. `unwrap_json_result` unwraps it back to the
            # same text the delegated arm carries plain.
            "result": {"result": compact({"result": "Replaced 1 occurrence(s)"})},
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
            "args": ARGS_ACP,
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

# --- the read pair (divergence A4) -----------------------------------------
#
# Same story, a tool whose result does not fit on one line. `read_file` renders
# the file with `cat -n` gutters and a trailing counter; the card is expected to
# collapse that to the counter rather than paint the file body into the header.

READ_MSG_ID = "msg-parity-0002"
READ_PREAMBLE = "Let me look."
READ_CODA = " That's the greeting."
READ_FULL = READ_PREAMBLE + READ_CODA
READ_ARGS = {"path": "greeting.rs"}
# A delegated Claude Code read arrives as `rawInput` keyed `file_path`. Both
# keys are in `ToolDisplay`'s `PATH_KEYS`, so the card shows `greeting.rs`
# either way — keeping the real key on each side tests that rather than
# assuming it.
READ_ARGS_ACP = {"file_path": "greeting.rs"}
READ_OUTPUT = (
    '     1\tfn main() {\n     2\t    println!("hello");\n     3\t}\n'
    "\n[3 lines read, 3 total]"
)

read_head = [
    ("user_message", {"message_id": READ_MSG_ID, "content": "read the greeting"}),
    ("text_delta", {"content": READ_PREAMBLE}),
    (
        "segment_complete",
        {"message_id": READ_MSG_ID, "index": 0, "content": READ_PREAMBLE},
    ),
]
read_tail = [
    ("text_delta", {"content": READ_CODA}),
    ("message_complete", {"message_id": READ_MSG_ID, "full_response": READ_FULL}),
]

read_internal = (
    read_head
    + [
        # No `interaction_requested`: `read_file` is read-only, so `is_safe`
        # exempts it from the gate.
        (
            "tool_call",
            {
                "call_id": "call-read-1",
                "tool": "read_file",
                "args": READ_ARGS,
                "description": "Read file contents. Returns content with line numbers.",
                "source": "Core",
                "display": DISPLAY,
            },
        ),
        (
            "tool_result",
            {
                "call_id": "call-read-1",
                "tool": "read_file",
                "result": {"result": compact({"result": READ_OUTPUT})},
                "terminate": False,
            },
        ),
    ]
    + read_tail
)

read_delegated = (
    read_head
    + [
        (
            "tool_call",
            {
                "call_id": "call-read-1",
                # `mcp__crucible__read_file` is what a delegated agent calls to
                # reach Crucible's own read tool over MCP; the client stores
                # `humanize_tool_title` of it, i.e. `Read File`. Same tool, same
                # output, a different string on the wire — which is A4.
                "tool": "Read File",
                "args": READ_ARGS_ACP,
                "source": "Acp:claude",
                "display": DISPLAY,
            },
        ),
        (
            "tool_result",
            {
                "call_id": "call-read-1",
                "tool": "Read File",
                # Flat, not doubly wrapped: the ACP arm stringifies `rawOutput`
                # once. The text is the internal tool's own output, because the
                # pair is about presenting equivalent output — not about two
                # tools formatting it differently.
                "result": {"result": READ_OUTPUT},
                "terminate": False,
            },
        ),
    ]
    + read_tail
)

for name, session_id, events in [
    ("acp_parity_internal.jsonl", "chat-2026-08-03T0914-parint", internal),
    ("acp_parity_delegated.jsonl", "chat-2026-08-03T0914-pardel", delegated),
    ("acp_parity_read_internal.jsonl", "chat-2026-08-03T0914-rdint", read_internal),
    ("acp_parity_read_delegated.jsonl", "chat-2026-08-03T0914-rddel", read_delegated),
]:
    (OUT / name).write_text(lines(session_id, events))
    print(f"wrote {name}")
