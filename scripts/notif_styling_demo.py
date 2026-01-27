#!/usr/bin/env python3

from typing import List, Tuple

RESET = "\x1b[0m"
BORDER_FG = "\x1b[38;2;40;44;52m"
SUCCESS = "\x1b[32m"
INFO = "\x1b[36m"
WARNING = "\x1b[33m"
TEXT = "\x1b[97m"
INPUT_BG = "\x1b[48;2;40;44;52m"

# Box drawing characters
BOX_LIGHT_DOWN_RIGHT = "┌"
BOX_LIGHT_DOWN_LEFT = "┐"
BOX_LIGHT_UP_RIGHT = "└"
BOX_LIGHT_UP_LEFT = "┘"
BOX_LIGHT_HORIZONTAL = "─"
BOX_LIGHT_VERTICAL = "│"
BOX_HEAVY_HORIZONTAL = "━"
BOX_HEAVY_VERTICAL = "┃"

# Block elements
UPPER_HALF = "▀"
LOWER_HALF = "▄"
LEFT_HALF = "▌"
RIGHT_HALF = "▐"
FULL_BLOCK = "█"

# Quadrant blocks (1/4 filled)
QUADRANT_UPPER_LEFT = "▘"
QUADRANT_UPPER_RIGHT = "▝"
QUADRANT_LOWER_LEFT = "▖"
QUADRANT_LOWER_RIGHT = "▗"

# Quadrant blocks (2/4 filled - diagonal)
QUADRANT_UPPER_LEFT_LOWER_RIGHT = "▚"
QUADRANT_UPPER_RIGHT_LOWER_LEFT = "▞"

# Quadrant blocks (3/4 filled - named by MISSING quadrant)
QUADRANT_MISSING_UPPER_RIGHT = "▙"
QUADRANT_MISSING_LOWER_LEFT = "▜"
QUADRANT_MISSING_LOWER_RIGHT = "▛"
QUADRANT_MISSING_UPPER_LEFT = "▟"

# Shade characters
LIGHT_SHADE = "░"
MEDIUM_SHADE = "▒"
DARK_SHADE = "▓"

# Current notification style
TOP_LEFT = QUADRANT_LOWER_RIGHT
TOP_EDGE = LOWER_HALF
BOTTOM_EDGE = UPPER_HALF
LEFT_BORDER = LEFT_HALF
NOTCH = QUADRANT_UPPER_LEFT


def get_width():
    try:
        import shutil

        return shutil.get_terminal_size().columns
    except:
        return 80


def input_box(width, prompt=" > ", content="", notif_end_col=None):
    if notif_end_col is not None:
        top_left = BORDER_FG + TOP_EDGE * (notif_end_col) + RESET
        top_connect = BORDER_FG + QUADRANT_MISSING_UPPER_RIGHT + RESET
        top_right = BORDER_FG + TOP_EDGE * (width - notif_end_col - 1) + RESET
        top = top_left + top_connect + top_right
    else:
        top = BORDER_FG + TOP_EDGE * width + RESET

    text = f"{prompt}{content}"
    middle = INPUT_BG + text + " " * (width - len(text)) + RESET
    bottom = BORDER_FG + BOTTOM_EDGE * width + RESET
    return [top, middle, bottom]


def notification_card(messages, width):
    if not messages:
        return []

    max_len = max(len(text) for _, _, text in messages)
    card_width = max_len + 5
    start_col = width - card_width

    lines = []
    top = QUADRANT_MISSING_LOWER_RIGHT + BOTTOM_EDGE * (card_width - 1)
    lines.append(" " * start_col + BORDER_FG + top + RESET)

    for icon, color, text in messages:
        padded = text.ljust(max_len)
        line = (
            " " * start_col
            + BORDER_FG
            + LEFT_BORDER
            + " "
            + color
            + icon
            + " "
            + RESET
            + TEXT
            + padded
            + RESET
        )
        lines.append(line)

    return lines


def main():
    w = get_width()

    print("\n" + "█" * w)
    print(f"NOTIFICATION STYLING (width={w})")
    print("█" * w)

    print("\n" + "─" * w)
    print("1. SINGLE NOTIFICATION")
    print("─" * w + "\n")

    msgs1 = [("✓", SUCCESS, "Ctrl+C again to quit")]
    max_len1 = max(len(text) for _, _, text in msgs1)
    notif_end1 = w - (max_len1 + 5)
    for line in notification_card(msgs1, w):
        print(line)
    for line in input_box(w, notif_end_col=notif_end1):
        print(line)

    print("\n" + "─" * w)
    print("2. STACKED NOTIFICATIONS")
    print("─" * w + "\n")

    msgs2 = [
        (" ✓", SUCCESS, "Session saved"),
        ("⏳", INFO, "Indexing... 45%"),
        ("⚠ ", WARNING, "Context at 85%"),
    ]
    max_len2 = max(len(text) for _, _, text in msgs2)
    notif_end2 = w - (max_len2 + 5)
    for line in notification_card(msgs2, w):
        print(line)
    for line in input_box(w, notif_end_col=notif_end2):
        print(line)

    print("\n" + "─" * w)
    print("3. VARIABLE LENGTH (alignment test)")
    print("─" * w + "\n")

    msgs3 = [
        ("✓", SUCCESS, "Thinking display: on"),
        ("✓", SUCCESS, "Thinking display: off"),
        ("✓", SUCCESS, "Ctrl+C again to quit"),
    ]
    max_len3 = max(len(text) for _, _, text in msgs3)
    notif_end3 = w - (max_len3 + 5)
    for line in notification_card(msgs3, w):
        print(line)
    for line in input_box(w, notif_end_col=notif_end3):
        print(line)

    print("\n" + "█" * w + "\n")


if __name__ == "__main__":
    main()
