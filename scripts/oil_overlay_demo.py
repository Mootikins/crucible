#!/usr/bin/env python3
"""
OIL Overlay System Demo - Golden Reference for Taffy Migration

Demonstrates the planned overlay architecture:
1. Statusline with right-aligned notifications (auto-fade)
2. :messages drawer (bottom panel with notification history)
3. Generic drawer/panel system (for Lua scripting)

This script is the GOLDEN REFERENCE for what the Taffy-based
OIL renderer should produce after the migration.
"""

from typing import List, Tuple, Optional
from dataclasses import dataclass
from enum import Enum

# ANSI color codes — RGB required for bold to render in Zellij
RESET = "\x1b[0m"
BORDER_FG = "\x1b[38;2;40;44;52m"
SUCCESS = "\x1b[38;2;158;206;106m"
INFO = "\x1b[38;2;0;206;209m"
WARNING = "\x1b[38;2;224;175;104m"
ERROR = "\x1b[38;2;247;118;142m"
TEXT = "\x1b[38;2;192;202;245m"
DIM_TEXT = "\x1b[38;2;100;110;130m"
INPUT_BG = "\x1b[48;2;40;44;52m"

# Statusline colors — matched from cru-chat.cast
MODE_BG = "\x1b[48;5;10m"
MODE_FG = "\x1b[38;5;0m"
GRAY = "\x1b[38;5;8m"
CYAN = "\x1b[38;5;14m"
BRIGHT_GREEN = "\x1b[38;5;10m"
BRIGHT_WHITE = "\x1b[38;5;15m"
BOLD = "\x1b[1m"
DIM = "\x1b[2m"

# Permission prompt colors — from theme.rs
POPUP_BG = "\x1b[48;2;30;34;42m"
TEXT_ACCENT = "\x1b[38;5;14m"
TEXT_MUTED = "\x1b[38;5;8m"

# Box drawing characters
BOX_LIGHT_HORIZONTAL = "─"
BOX_LIGHT_VERTICAL = "│"
BOX_HEAVY_HORIZONTAL = "━"

# Block elements for notification styling
UPPER_HALF = "▀"
LOWER_HALF = "▄"
LEFT_HALF = "▌"

# Quadrant blocks
QUADRANT_LOWER_RIGHT = "▗"
QUADRANT_UPPER_LEFT = "▘"


class NotificationKind(Enum):
    INFO = "info"  # Auto-dismiss after 3s
    WARNING = "warning"  # Persistent until dismissed
    ERROR = "error"  # Persistent until dismissed


@dataclass
class Notification:
    """Notification data structure"""

    kind: NotificationKind
    text: str
    timestamp: str

    @property
    def color(self):
        return {
            NotificationKind.INFO: INFO,
            NotificationKind.WARNING: WARNING,
            NotificationKind.ERROR: ERROR,
        }[self.kind]

    @property
    def block(self):
        """Reverse-video badge. Zellij needs RGB fg + reset-then-bold for bold to render."""
        label = self.kind.value.upper()[:4].ljust(4)
        return f"\x1b[0;1m{self.color}\x1b[7m {label} "


def get_width():
    """Get terminal width"""
    try:
        import shutil

        return shutil.get_terminal_size().columns
    except:
        return 80


def statusline(
    width: int,
    mode: str = "NORMAL",
    model: str = "glm-4.7-flash-iq4",
    status: str = "Ready",
    notification: Optional[Notification] = None,
    counts: Optional[dict] = None,
):
    left = (
        f"{MODE_BG}{MODE_FG}{BOLD} {mode} {RESET}"
        f"{GRAY} {RESET}"
        f"{CYAN}{model}{RESET}"
        f"{GRAY} {RESET}"
        f"{GRAY}{status}{RESET}"
    )
    visible_left = len(f" {mode} ") + 1 + len(model) + 1 + len(status)

    if notification:
        visible_notif = 1 + len(notification.text) + 1 + 6
        padding = width - visible_left - visible_notif
        right = f" {BRIGHT_WHITE}{notification.text}{RESET} {notification.block}{RESET}"
        line = left + " " * max(padding, 1) + right
    elif counts:
        badges = ""
        visible_badges = 0
        for kind, n in counts.items():
            label = kind.value.upper()[:4].ljust(4)
            color = {
                NotificationKind.WARNING: WARNING,
                NotificationKind.ERROR: ERROR,
            }[kind]
            badges += (
                f"\x1b[0;1m{color}\x1b[7m {label} {RESET}\x1b[0;1m{color} {n} {RESET}"
            )
            visible_badges += 6 + 2 + len(str(n)) + 1
        padding = width - visible_left - visible_badges
        line = left + " " * max(padding, 1) + badges
    else:
        line = left

    return line


def input_box(width: int, prompt: str = " > ", content: str = ""):
    top = BORDER_FG + LOWER_HALF * width + RESET

    text = f"{prompt}{content}"
    middle = INPUT_BG + text + " " * (width - len(text)) + RESET

    bottom = BORDER_FG + UPPER_HALF * width + RESET

    return [top, middle, bottom]


def drawer_panel(
    width: int,
    title: str,
    items: List[Tuple[str, str]],
    max_items: int = 10,
):
    lines = []

    display_items = items[:max_items]

    top = BORDER_FG + LOWER_HALF * width + RESET
    lines.append(top)

    for label, content in display_items:
        label_part = f" {label}: "
        visible_label_len = len(label_part)

        import re

        content_stripped = re.sub(r"\x1b\[[0-9;]*m", "", content)

        import unicodedata

        visible_content_len = sum(
            2 if unicodedata.east_asian_width(c) in "FW" else 1
            for c in content_stripped
        )

        padding = width - visible_label_len - visible_content_len

        line = (
            INPUT_BG
            + TEXT
            + label_part
            + content
            + RESET
            + INPUT_BG
            + " " * padding
            + RESET
        )
        lines.append(line)

    bottom = BORDER_FG + UPPER_HALF * width + RESET
    lines.append(bottom)

    return lines


def messages_drawer(
    width: int, notifications: List[Notification], right_aligned: bool = False
):
    items = []
    for notif in notifications:
        label = f"{notif.timestamp}"
        content = f"{notif.block}{RESET}{INPUT_BG} {TEXT}{notif.text}"
        items.append((label, content))

    return drawer_panel(width, "Messages", items, max_items=10)


def demo_scenario(
    width: int,
    notification: Optional[Notification] = None,
    input_prompt: str = " > ",
    input_content: str = "",
    drawer: Optional[List[str]] = None,
    mode: str = "NORMAL",
    model: str = "glm-4.7-flash-iq4",
    status: str = "Ready",
    counts: Optional[dict] = None,
    drawer_name: str = "",
):
    lines = []

    if drawer:
        lines.extend(drawer)
        drawer_bg = {
            "MESSAGES": "\x1b[48;5;14m",
            "TASKS": "\x1b[48;5;13m",
        }.get(drawer_name, MODE_BG)
        drawer_label = (
            f"{drawer_bg}{MODE_FG}{BOLD} {drawer_name} {RESET}" if drawer_name else ""
        )
        drawer_fg = {
            "MESSAGES": "\x1b[38;5;14m",
            "TASKS": "\x1b[38;5;13m",
        }.get(drawer_name, "")
        key_hints = f" {drawer_fg}ESC/q{RESET} {DIM_TEXT}close{RESET}"
        lines.append(drawer_label + key_hints)
    else:
        lines.extend(input_box(width, input_prompt, input_content))
        lines.append(
            statusline(
                width,
                mode=mode,
                model=model,
                status=status,
                notification=notification,
                counts=counts,
            )
        )

    return lines


PERM_BG = "\x1b[48;5;9m"


DIFF_ADD = "\x1b[38;2;158;206;106m"
DIFF_DEL = "\x1b[38;2;247;118;142m"
DIFF_CTX = "\x1b[38;2;100;110;130m"
DIFF_HUNK = "\x1b[38;2;0;206;209m"
DIFF_BG = "\x1b[48;2;28;32;40m"
DIFF_FG = "\x1b[38;2;28;32;40m"


GUTTER_FG = "\x1b[38;2;70;75;90m"
GUTTER_WIDTH = 4


def _diff_line(width: int, prefix: str, text: str, color: str, lineno: str = "") -> str:
    gutter = f"{GUTTER_FG}{lineno:>{GUTTER_WIDTH}} {RESET}{DIFF_BG}"
    gutter_visible = GUTTER_WIDTH + 1
    content = f"{prefix} {text}"
    pad = " " * max(0, width - gutter_visible - len(content))
    return f"{DIFF_BG}{gutter}{color}{content}{pad}{RESET}"


def _collapsed_line(width: int, count: int) -> str:
    gutter = " " * (GUTTER_WIDTH + 1)
    label = f"  ··· {count} lines collapsed ···"
    pad = " " * max(0, width - len(gutter) - len(label))
    return f"{DIFF_BG}{GUTTER_FG}{gutter}{label}{pad}{RESET}"


def diff_panel(
    width: int, file_path: str, action: str, diff_lines: list, queue_label: str = ""
) -> List[str]:
    lines = []

    lines.append(f"{INPUT_BG}{DIFF_FG}{LOWER_HALF * width}{RESET}")

    for entry in diff_lines:
        if entry[0] == "collapsed":
            lines.append(_collapsed_line(width, entry[1]))
            continue
        kind, text, old_no, new_no = entry
        if kind == "hunk":
            lines.append(_diff_line(width, " ", text, DIFF_HUNK))
        elif kind == "+":
            lineno = str(new_no) if new_no else ""
            lines.append(_diff_line(width, "+", text, DIFF_ADD, lineno))
        elif kind == "-":
            lineno = str(old_no) if old_no else ""
            lines.append(_diff_line(width, "-", text, DIFF_DEL, lineno))
        else:
            lineno = str(new_no) if new_no else (str(old_no) if old_no else "")
            lines.append(_diff_line(width, " ", text, DIFF_CTX, lineno))

    lines.append(f"{INPUT_BG}{DIFF_FG}{UPPER_HALF * width}{RESET}")

    prefix = f"  {queue_label}" if queue_label else "  "
    file_label = f"{prefix}{file_path} [{action}]"
    file_pad = " " * (width - len(file_label))
    lines.append(f"{INPUT_BG}{BRIGHT_WHITE}{file_label}{file_pad}{RESET}")
    lines.append(f"{INPUT_BG}{' ' * width}{RESET}")

    return lines


def perm_prompt(
    width: int,
    type_label: str,
    action_detail: str,
    options: List[Tuple[str, str]],
    selected: int = 0,
    queue_total: int = 1,
    diff: Optional[List[str]] = None,
):
    lines = []

    lines.append(BORDER_FG + LOWER_HALF * width + RESET)

    blank = f"{INPUT_BG}{' ' * width}{RESET}"

    if diff:
        lines.extend(diff)
    else:
        action_text = f"  {action_detail}"
        if queue_total > 1:
            action_text = f"  [{1}/{queue_total}] {action_detail}"
        action_pad = " " * (width - len(action_text))
        lines.append(f"{INPUT_BG}{BRIGHT_WHITE}{action_text}{action_pad}{RESET}")
        lines.append(blank)

    for i, (key, label) in enumerate(options):
        if i == selected:
            text = f"    > [{key}] {label}"
            pad = " " * (width - len(text))
            lines.append(
                f"{INPUT_BG}{TEXT_ACCENT}{BOLD}{text}{RESET}{INPUT_BG}{pad}{RESET}"
            )
        else:
            text = f"      [{key}] {label}"
            pad = " " * (width - len(text))
            lines.append(
                f"{INPUT_BG}{TEXT}      [{key}]{RESET}{INPUT_BG} {BRIGHT_WHITE}{label}{pad}{RESET}"
            )

    lines.append(BORDER_FG + UPPER_HALF * width + RESET)

    shortcut_keys = "/".join(k for k, _ in options)
    perm_color = "\x1b[38;2;247;118;142m"
    badge_label = (
        f"\x1b[0;1m{perm_color}\x1b[7m PERMISSION {RESET}"
        f"\x1b[0;1m{perm_color} {type_label.upper()} {RESET}"
    )
    diff_hint = f"  {perm_color}h{RESET} {DIM_TEXT}diff{RESET}" if diff else ""
    hints = (
        f" {perm_color}↑/↓{RESET} {DIM_TEXT}navigate{RESET}"
        f"  {perm_color}Enter{RESET} {DIM_TEXT}select{RESET}"
        f"  {perm_color}{shortcut_keys}{RESET} {DIM_TEXT}shortcuts{RESET}"
        f"{diff_hint}"
        f"  {perm_color}Esc{RESET} {DIM_TEXT}cancel{RESET}"
    )
    lines.append(badge_label + hints)

    return lines


def main():
    w = get_width()

    print("\n" + "█" * w)
    print(f"OIL OVERLAY SYSTEM DEMO (width={w})")
    print("Golden Reference for Taffy Migration")
    print("█" * w)

    # ========================================================================
    # SCENARIO 1: Statusline with single notification (auto-fade toast)
    # ========================================================================
    print("\n" + BOX_HEAVY_HORIZONTAL * w)
    print("1. STATUSLINE NOTIFICATION (toast, auto-fade after 3s)")
    print(BOX_HEAVY_HORIZONTAL * w + "\n")

    notif1 = Notification(
        kind=NotificationKind.INFO,
        text="Ctrl+C again to quit",
        timestamp="14:32:15",
    )

    for line in demo_scenario(w, notification=notif1):
        print(line)

    # ========================================================================
    # SCENARIO 2: Statusline with progress notification (persistent)
    # ========================================================================
    print("\n" + BOX_HEAVY_HORIZONTAL * w)
    print("2. STATUSLINE NOTIFICATION (progress, persistent)")
    print(BOX_HEAVY_HORIZONTAL * w + "\n")

    notif2 = Notification(
        kind=NotificationKind.INFO,
        text="Indexing... 45%",
        timestamp="14:32:18",
    )

    for line in demo_scenario(w, notification=notif2):
        print(line)

    # ========================================================================
    # SCENARIO 3: Statusline with warning (persistent)
    # ========================================================================
    print("\n" + BOX_HEAVY_HORIZONTAL * w)
    print("3. STATUSLINE NOTIFICATION (warning, persistent)")
    print(BOX_HEAVY_HORIZONTAL * w + "\n")

    notif3 = Notification(
        kind=NotificationKind.WARNING,
        text="Context at 85%",
        timestamp="14:32:20",
    )

    for line in demo_scenario(w, notification=notif3):
        print(line)

    # ========================================================================
    # SCENARIO 4: :messages drawer (notification history)
    # ========================================================================
    print("\n" + BOX_HEAVY_HORIZONTAL * w)
    print("4. :messages DRAWER (notification history)")
    print(BOX_HEAVY_HORIZONTAL * w + "\n")

    history = [
        Notification(NotificationKind.INFO, "Session saved", "14:30:12"),
        Notification(NotificationKind.INFO, "Thinking display: on", "14:31:45"),
        Notification(NotificationKind.INFO, "Indexing... 45%", "14:32:18"),
        Notification(NotificationKind.WARNING, "Context at 85%", "14:32:20"),
        Notification(NotificationKind.INFO, "Ctrl+C again to quit", "14:32:15"),
    ]

    drawer = messages_drawer(w, history)

    for line in demo_scenario(w, drawer=drawer, drawer_name="MESSAGES"):
        print(line)

    # ========================================================================
    # SCENARIO 5: Generic drawer (Lua plugin example)
    # ========================================================================
    print("\n" + BOX_HEAVY_HORIZONTAL * w)
    print("5. GENERIC DRAWER (Lua plugin example)")
    print(BOX_HEAVY_HORIZONTAL * w + "\n")

    lua_drawer_items = [
        ("Plugin", "task-tracker.lua"),
        ("Status", "3 tasks pending"),
        ("Next", "Review PR #42"),
        ("Due", "Today 17:00"),
    ]

    lua_drawer = drawer_panel(w, "Task Tracker", lua_drawer_items, max_items=10)

    for line in demo_scenario(w, drawer=lua_drawer, drawer_name="TASKS"):
        print(line)

    # ========================================================================
    # SCENARIO 6: Drawer + statusline notification (both visible)
    # ========================================================================
    print("\n" + BOX_HEAVY_HORIZONTAL * w)
    print("6. DRAWER + STATUSLINE NOTIFICATION (both visible)")
    print(BOX_HEAVY_HORIZONTAL * w + "\n")

    notif6 = Notification(
        kind=NotificationKind.ERROR,
        text="Connection lost",
        timestamp="14:33:05",
    )

    for line in demo_scenario(
        w, notification=notif6, drawer=drawer, drawer_name="MESSAGES"
    ):
        print(line)

    # ========================================================================
    # SCENARIO 7: Statusline with notification counts (no active message)
    # ========================================================================
    print("\n" + BOX_HEAVY_HORIZONTAL * w)
    print("7. STATUSLINE COUNTS (warn/error summary, no active message)")
    print(BOX_HEAVY_HORIZONTAL * w + "\n")

    for line in demo_scenario(
        w,
        counts={
            NotificationKind.WARNING: 3,
            NotificationKind.ERROR: 1,
        },
    ):
        print(line)

    # ========================================================================
    # SCENARIO 8: Permission prompt — Bash command
    # ========================================================================
    print("\n" + BOX_HEAVY_HORIZONTAL * w)
    print("8. PERMISSION PROMPT (Bash command, allow selected)")
    print(BOX_HEAVY_HORIZONTAL * w + "\n")

    for line in perm_prompt(
        w,
        type_label="Bash",
        action_detail="npm install lodash",
        options=[("y", "Allow once"), ("n", "Deny"), ("p", "Allow + Save pattern")],
        selected=0,
    ):
        print(line)

    # ========================================================================
    # SCENARIO 9: Permission prompt — File write (queued)
    # ========================================================================
    print("\n" + BOX_HEAVY_HORIZONTAL * w)
    print("9. PERMISSION PROMPT (File write, queue 1/3, deny selected)")
    print(BOX_HEAVY_HORIZONTAL * w + "\n")

    sample_diff = diff_panel(
        w,
        "src/main.rs",
        "write",
        [
            ("hunk", "@@ -10,7 +10,8 @@ fn main() {", "", ""),
            ("ctx", "    let config = Config::load()?;", 10, 10),
            ("ctx", "    let db = Database::connect(&config.db_url).await?;", 11, 11),
            ("-", "    let app = App::new(config, db);", 12, ""),
            ("+", "    let cache = Cache::new(256);", "", 12),
            ("+", "    let app = App::new(config, db, cache);", "", 13),
            ("ctx", "    app.run().await", 13, 14),
            ("ctx", "}", 14, 15),
        ],
        queue_label="[1/3] ",
    )

    for line in perm_prompt(
        w,
        type_label="Write",
        action_detail="",
        options=[("y", "Allow once"), ("n", "Deny"), ("p", "Allow + Save pattern")],
        selected=1,
        queue_total=3,
        diff=sample_diff,
    ):
        print(line)

    # ========================================================================
    # SCENARIO 10: Permission prompt — Tool call (no pattern option)
    # ========================================================================
    print("\n" + BOX_HEAVY_HORIZONTAL * w)
    print("10. PERMISSION PROMPT (Tool call, no pattern option)")
    print(BOX_HEAVY_HORIZONTAL * w + "\n")

    for line in perm_prompt(
        w,
        type_label="Tool",
        action_detail='search_notes(query="auth", limit=10)',
        options=[("y", "Allow once"), ("n", "Deny")],
        selected=0,
    ):
        print(line)

    # ========================================================================
    # SCENARIO 11: Permission prompt — multi-hunk diff with collapsed regions
    # ========================================================================
    print("\n" + BOX_HEAVY_HORIZONTAL * w)
    print("11. PERMISSION PROMPT (multi-hunk diff, collapsed regions)")
    print(BOX_HEAVY_HORIZONTAL * w + "\n")

    multi_diff = diff_panel(
        w,
        "src/lib.rs",
        "write",
        [
            ("hunk", "@@ -3,6 +3,7 @@ use std::collections::HashMap;", "", ""),
            ("ctx", "use std::io;", 3, 3),
            ("+", "use std::sync::Arc;", "", 4),
            ("ctx", "", 4, 5),
            ("ctx", "pub struct Config {", 5, 6),
            ("collapsed", 42),
            ("hunk", "@@ -51,7 +52,9 @@ impl App {", "", ""),
            ("ctx", "    pub fn run(&self) -> Result<()> {", 51, 52),
            ("-", '        println!("starting");', 52, ""),
            ("+", '        log::info!("starting app");', "", 53),
            ("+", "        self.cache.warm_up()?;", "", 54),
            ("ctx", "        self.server.listen()", 53, 55),
            ("ctx", "    }", 54, 56),
            ("collapsed", 18),
            ("hunk", "@@ -75,4 +78,5 @@ impl Drop for App {", "", ""),
            ("ctx", "    fn drop(&mut self) {", 75, 78),
            ("ctx", "        self.server.shutdown();", 76, 79),
            ("+", "        self.cache.flush();", "", 80),
            ("ctx", "    }", 77, 81),
        ],
    )

    for line in perm_prompt(
        w,
        type_label="Write",
        action_detail="/src/lib.rs",
        options=[("y", "Allow once"), ("n", "Deny"), ("p", "Allow + Save pattern")],
        selected=0,
        diff=multi_diff,
    ):
        print(line)

    print("\n" + "█" * w + "\n")


if __name__ == "__main__":
    main()
