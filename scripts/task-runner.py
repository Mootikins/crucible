#!/usr/bin/env python3
"""
Task Runner - Automated agent task execution using TASKS.md

Runs agents in a loop, picking tasks and executing them until:
- All tasks complete
- A task is blocked
- User interrupts (Ctrl+C)

Usage:
    ./scripts/task-runner.py [TASKS.md path]
"""

import subprocess
import json
import sys
import os
from pathlib import Path

CRU = Path(__file__).parent.parent / "target/release/cru"

AGENT_SYSTEM_PROMPT = """You are implementing a task from a TASKS.md file.

## IMPORTANT: Testing

Use `just test` for running tests - NOT `cargo test`. Less noise, faster feedback.

## Context Files (read first)

{context_files}

## Current Task

**ID:** {task_id}
**Description:** {task_content}
{test_info}

## Completion Protocol

When done:
1. Run `just test` to verify
2. Output exactly on its own line: `TASK COMPLETE`

If stuck or need help:
1. Output exactly: `BLOCKED: <reason>`
2. Stop working - do not continue

Begin implementation now.
"""


def run_cru(args: list[str], cwd: str = None) -> subprocess.CompletedProcess:
    """Run cru command and return result."""
    cmd = [str(CRU), "--no-process"] + args
    return subprocess.run(cmd, capture_output=True, text=True, cwd=cwd)


def get_next_task(tasks_file: Path) -> dict | None:
    """Get next ready task from TASKS.md."""
    result = run_cru(["tasks", "--file", str(tasks_file), "next"],
                     cwd=str(tasks_file.parent))
    if result.returncode != 0:
        print(f"Error getting next task: {result.stderr}")
        return None

    # Parse text output: "  [ ] Task description (id)"
    import re
    for line in result.stdout.strip().split("\n"):
        match = re.match(r'\s*\[.\]\s+(.+?)\s+\(([^)]+)\)\s*$', line)
        if match:
            content, task_id = match.groups()
            return {"id": task_id, "content": content, "metadata": {}}

    return None


def pick_task(tasks_file: Path, task_id: str) -> bool:
    """Mark task as in-progress."""
    result = run_cru(["tasks", "--file", str(tasks_file), "pick", task_id],
                     cwd=str(tasks_file.parent))
    return result.returncode == 0


def complete_task(tasks_file: Path, task_id: str) -> bool:
    """Mark task as done."""
    result = run_cru(["tasks", "--file", str(tasks_file), "done", task_id],
                     cwd=str(tasks_file.parent))
    return result.returncode == 0


def block_task(tasks_file: Path, task_id: str, reason: str) -> bool:
    """Mark task as blocked."""
    result = run_cru(["tasks", "--file", str(tasks_file), "blocked", task_id, reason],
                     cwd=str(tasks_file.parent))
    return result.returncode == 0


def parse_frontmatter(tasks_file: Path) -> dict:
    """Extract frontmatter from TASKS.md."""
    content = tasks_file.read_text()
    if not content.startswith("---"):
        return {}

    end = content.find("---", 3)
    if end == -1:
        return {}

    import yaml
    try:
        return yaml.safe_load(content[3:end]) or {}
    except:
        return {}


def generate_prompt(task: dict, frontmatter: dict) -> str:
    """Generate agent prompt from task and frontmatter."""
    context_files = frontmatter.get("context_files", [])
    context_str = "\n".join(f"- {f}" for f in context_files) if context_files else "(none specified)"

    # Extract test info if present
    tests = task.get("metadata", {}).get("tests", [])
    test_info = ""
    if tests:
        test_info = f"**Tests to implement:** {', '.join(tests)}"

    return AGENT_SYSTEM_PROMPT.format(
        context_files=context_str,
        task_id=task["id"],
        task_content=task["content"],
        test_info=test_info
    )


def run_agent(prompt: str, working_dir: Path) -> tuple[bool, str]:
    """
    Run claude agent with prompt.
    Returns (success, output).
    """
    print("\n" + "="*60)
    print("Starting agent...")
    print("="*60 + "\n")

    try:
        # Run claude with the prompt
        result = subprocess.run(
            ["claude", "--print", prompt],
            cwd=str(working_dir),
            capture_output=False,  # Let output stream to terminal
            text=True
        )

        # For now, we check exit code
        # In practice, we'd capture and parse output
        if result.returncode == 0:
            return True, ""
        else:
            return False, f"Agent exited with code {result.returncode}"

    except KeyboardInterrupt:
        return False, "Interrupted by user"
    except Exception as e:
        return False, str(e)


def run_agent_interactive(prompt: str, working_dir: Path, allow_writes: bool = False) -> tuple[str, str]:
    """
    Run claude agent interactively, capturing output for parsing.
    Returns (status, reason) where status is 'complete', 'blocked', or 'error'.
    """
    import tempfile

    print("\n" + "="*60)
    print("Starting agent..." + (" (writes allowed)" if allow_writes else ""))
    print("="*60 + "\n")

    output_file = tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False)
    output_path = output_file.name
    output_file.close()

    try:
        # Build claude command - use -p with file path for cleaner handling
        claude_cmd = ["claude"]
        if allow_writes:
            claude_cmd.append("--dangerously-skip-permissions")
        claude_cmd.extend(["--print", "-p", prompt])

        # Run with output capture
        with open(output_path, 'w') as outfile:
            process = subprocess.Popen(
                claude_cmd,
                cwd=str(working_dir),
                stdout=outfile,
                stderr=subprocess.STDOUT,
            )
            process.wait()

        # Parse output
        output = Path(output_path).read_text()

        if "TASK COMPLETE" in output:
            return "complete", ""

        for line in output.split("\n"):
            if line.startswith("BLOCKED:"):
                reason = line[8:].strip()
                return "blocked", reason

        if process.returncode != 0:
            return "error", f"Agent exited with code {process.returncode}"

        # No explicit completion marker - treat as incomplete
        return "blocked", "Agent finished without TASK COMPLETE marker"

    except KeyboardInterrupt:
        return "blocked", "Interrupted by user"
    except Exception as e:
        return "error", str(e)
    finally:
        try:
            os.unlink(output_path)
        except:
            pass


def main():
    import argparse
    parser = argparse.ArgumentParser(description="Automated task runner using TASKS.md")
    parser.add_argument("tasks_file", nargs="?", default="TASKS.md",
                        help="Path to TASKS.md (default: TASKS.md in cwd)")
    parser.add_argument("--dry-run", action="store_true",
                        help="Show what would be done without running agents")
    parser.add_argument("--allow-writes", action="store_true",
                        help="Allow file writes without approval (uses --dangerously-skip-permissions)")
    args = parser.parse_args()

    tasks_file = Path(args.tasks_file).resolve()

    if not tasks_file.exists():
        print(f"Error: {tasks_file} not found")
        sys.exit(1)

    print(f"Task Runner - {tasks_file}")
    print("="*60)

    # Parse frontmatter for context
    frontmatter = parse_frontmatter(tasks_file)
    title = frontmatter.get("title", "Unknown")
    print(f"Project: {title}")

    # Working directory is the repo root (parent of thoughts/)
    working_dir = tasks_file.parent
    while working_dir.name != "crucible" and working_dir.parent != working_dir:
        working_dir = working_dir.parent

    print(f"Working dir: {working_dir}")
    print()

    # Main loop
    completed = 0
    while True:
        task = get_next_task(tasks_file)

        if task is None:
            print("\n" + "="*60)
            print(f"All tasks complete! ({completed} tasks done this session)")
            print("="*60)
            break

        task_id = task["id"]
        task_content = task["content"]

        print(f"\n>> Next task: [{task_id}] {task_content[:50]}...")

        # Generate prompt
        prompt = generate_prompt(task, frontmatter)

        if args.dry_run:
            print(f"\n[DRY RUN] Would run agent with prompt:")
            print("-" * 40)
            print(prompt[:500] + "..." if len(prompt) > 500 else prompt)
            print("-" * 40)
            break

        # Pick the task
        if not pick_task(tasks_file, task_id):
            print(f"Error: Could not pick task {task_id}")
            break

        # Run agent
        status, reason = run_agent_interactive(prompt, working_dir, args.allow_writes)

        if status == "complete":
            complete_task(tasks_file, task_id)
            completed += 1
            print(f"\n[x] Task {task_id} completed!")
        else:
            reason = reason or "Unknown error"
            block_task(tasks_file, task_id, reason)
            print(f"\n[!] Task {task_id} blocked: {reason}")
            print("\nStopping task runner. Fix the issue and run again.")
            break

    print(f"\nSession summary: {completed} tasks completed")


if __name__ == "__main__":
    main()
