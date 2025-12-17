#!/usr/bin/env python3
"""
Task Runner v2 - Phase-based agent task execution

Key improvements over v1:
1. Batch tasks by PHASE instead of individual tasks
2. Show completed tasks for context continuity
3. Include task specifications inline (not just file references)
4. Use session continuation when possible
5. Clearer completion protocol
6. Timing and context logging with optional export

Usage:
    ./scripts/task-runner-v2.py [TASKS.md path] [--phase N]
    ./scripts/task-runner-v2.py TASKS.md --export ./runs/  # Save logs to folder
"""

import subprocess
import re
import sys
import os
import json
from pathlib import Path
from datetime import datetime
from dataclasses import dataclass, field, asdict
from typing import Optional

CRU = Path(__file__).parent.parent / "target/release/cru"


@dataclass
class RunMetrics:
    """Metrics collected during a phase run."""
    phase_num: int
    phase_title: str
    tasks_total: int
    tasks_pending: int
    tasks_completed_before: int

    # Timing
    start_time: str = ""
    end_time: str = ""
    duration_seconds: float = 0.0

    # Context size
    prompt_chars: int = 0
    prompt_lines: int = 0
    prompt_tokens_est: int = 0  # Rough estimate: chars / 4

    # Output
    output_chars: int = 0
    output_lines: int = 0

    # Result
    status: str = ""  # complete, blocked, error
    reason: str = ""

    # Previous phases summary
    previous_phases_tasks: int = 0

    def to_dict(self) -> dict:
        return asdict(self)

    def summary(self) -> str:
        return f"""
╔══════════════════════════════════════════════════════════════╗
║  Phase {self.phase_num} Run Metrics
╠══════════════════════════════════════════════════════════════╣
║  Tasks: {self.tasks_pending} pending / {self.tasks_total} total
║  Previous phases: {self.previous_phases_tasks} tasks done
║
║  Timing:
║    Start:    {self.start_time}
║    End:      {self.end_time}
║    Duration: {self.duration_seconds:.1f}s ({self.duration_seconds/60:.1f}m)
║
║  Context:
║    Prompt:   {self.prompt_chars:,} chars / {self.prompt_lines} lines
║    Est tokens: ~{self.prompt_tokens_est:,}
║    Output:   {self.output_chars:,} chars / {self.output_lines} lines
║
║  Result: {self.status.upper()}
║    {self.reason if self.reason else '(success)'}
╚══════════════════════════════════════════════════════════════╝
"""


@dataclass
class RunExport:
    """Data to export for a run."""
    metrics: RunMetrics
    prompt: str = ""
    output: str = ""
    tasks_file_snapshot: str = ""

    def save(self, export_dir: Path) -> Path:
        """Save run data to export directory."""
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        run_dir = export_dir / f"phase{self.metrics.phase_num}_{timestamp}"
        run_dir.mkdir(parents=True, exist_ok=True)

        # Save metrics as JSON
        (run_dir / "metrics.json").write_text(
            json.dumps(self.metrics.to_dict(), indent=2)
        )

        # Save prompt
        (run_dir / "prompt.md").write_text(self.prompt)

        # Save output
        if self.output:
            (run_dir / "output.txt").write_text(self.output)

        # Save TASKS.md snapshot
        if self.tasks_file_snapshot:
            (run_dir / "TASKS_snapshot.md").write_text(self.tasks_file_snapshot)

        # Save human-readable summary
        (run_dir / "summary.txt").write_text(self.metrics.summary())

        return run_dir

# v2: Phase-oriented prompt with full context
AGENT_SYSTEM_PROMPT = """You are implementing Phase {phase_num} of a multi-phase project.

## Project: {title}

## Verification
Run: `{verify_command}`
{tdd_guidance}
## Context Files
{context_files_content}

## Phase {phase_num}: {phase_title}

{phase_description}

### Completed in Previous Phases
{completed_summary}

### Tasks to Complete NOW (do ALL of these)
{task_list}

### Task Specifications
{task_specs}
{conventions_section}
## Quality Requirements

### Scope Discipline
- Complete ONLY the listed tasks for this phase
- Do NOT anticipate or implement future phase work
- If you notice something that needs future work, note it but don't implement it

### Edge Case Testing
For each implementation, write tests for:
- Normal happy path
- Boundary conditions
- Invalid/malformed input
- State interactions (e.g., what happens if X while Y is active?)

### Bug Hunting
Before declaring PHASE COMPLETE:
1. Re-read your implementation
2. Ask: "What inputs could break this?"
3. Write a test for at least one edge case you identify

### Code Review Checklist
- [ ] All methods handle empty/null inputs
- [ ] Mutable state is updated consistently
- [ ] Byte/character position handling is correct after mutations
- [ ] No panics on malformed input

## Completion Protocol

Complete ALL tasks above, then:
1. Run `{verify_command}` to verify all tests pass
2. Output exactly: `PHASE {phase_num} COMPLETE`

If blocked on ANY task:
1. Output: `BLOCKED: <task_id> - <reason>`
2. Stop and wait for help

Do NOT output completion until ALL tasks pass tests.
Begin implementation.
"""

def run_cru(args: list[str], cwd: str = None) -> subprocess.CompletedProcess:
    """Run cru command and return result."""
    cmd = [str(CRU), "--no-process"] + args
    return subprocess.run(cmd, capture_output=True, text=True, cwd=cwd)


def parse_tasks_file(tasks_file: Path) -> dict:
    """Parse TASKS.md into structured phases and tasks."""
    content = tasks_file.read_text()

    # Extract frontmatter
    frontmatter = {}
    if content.startswith("---"):
        end = content.find("---", 3)
        if end != -1:
            import yaml
            try:
                frontmatter = yaml.safe_load(content[3:end]) or {}
            except:
                pass
            content = content[end+3:]

    # Parse phases and tasks
    phases = {}
    current_phase = None
    current_section = None

    lines = content.split('\n')
    i = 0
    while i < len(lines):
        line = lines[i]

        # Phase header: ## Phase N: Title
        phase_match = re.match(r'^##\s+Phase\s+(\d+):\s*(.+)', line)
        if phase_match:
            phase_num = int(phase_match.group(1))
            phase_title = phase_match.group(2).strip()
            current_phase = {
                'num': phase_num,
                'title': phase_title,
                'description': '',
                'tasks': [],
                'sections': {}
            }
            phases[phase_num] = current_phase
            i += 1
            continue

        # Section header: ### N.N Title
        section_match = re.match(r'^###\s+(\d+\.\d+)\s+(.+)', line)
        if section_match and current_phase:
            current_section = section_match.group(1)
            current_phase['sections'][current_section] = {
                'title': section_match.group(2).strip(),
                'tasks': []
            }
            i += 1
            continue

        # Task line: - [x] or - [ ] or - [/]
        # Format: - [x] Task description [id:: 1.2.3] [deps:: 1.2.2]
        task_match = re.match(r'^-\s+\[([x /])\]\s+(.+?)\s+\[id::\s*([^\]]+)\]', line)
        if task_match and current_phase:
            status_char = task_match.group(1)
            status = 'done' if status_char == 'x' else ('in_progress' if status_char == '/' else 'pending')
            task_content = task_match.group(2).strip()
            task_id = task_match.group(3).strip() if task_match.group(3) else None

            # Collect task specification (indented lines following)
            spec_lines = []
            j = i + 1
            while j < len(lines) and (lines[j].startswith('  ') or lines[j].strip() == ''):
                if lines[j].strip():
                    spec_lines.append(lines[j])
                j += 1

            task = {
                'id': task_id,
                'content': task_content,
                'status': status,
                'spec': '\n'.join(spec_lines),
                'section': current_section
            }

            current_phase['tasks'].append(task)
            if current_section and current_section in current_phase['sections']:
                current_phase['sections'][current_section]['tasks'].append(task)

            i = j
            continue

        # Phase description (lines after phase header, before first section)
        if current_phase and not current_section and line.strip() and not line.startswith('#'):
            current_phase['description'] += line + '\n'

        i += 1

    return {
        'frontmatter': frontmatter,
        'phases': phases
    }


def get_phase_status(phase: dict) -> tuple[list, list, list]:
    """Return (completed, pending, in_progress) task lists."""
    completed = [t for t in phase['tasks'] if t['status'] == 'done']
    pending = [t for t in phase['tasks'] if t['status'] == 'pending']
    in_progress = [t for t in phase['tasks'] if t['status'] == 'in_progress']
    return completed, pending, in_progress


def get_next_phase(parsed: dict) -> int | None:
    """Find the next phase with pending tasks."""
    for num in sorted(parsed['phases'].keys()):
        phase = parsed['phases'][num]
        _, pending, in_progress = get_phase_status(phase)
        if pending or in_progress:
            return num
    return None


def get_git_root(start_path: Path) -> Path:
    """Get git repository root directory."""
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            cwd=str(start_path),
            capture_output=True,
            text=True
        )
        if result.returncode == 0:
            return Path(result.stdout.strip())
    except:
        pass
    # Fallback: walk up to find .git
    current = start_path
    while current.parent != current:
        if (current / ".git").exists():
            return current
        current = current.parent
    return start_path


def read_context_files(frontmatter: dict, base_path: Path) -> str:
    """Read and format context files from frontmatter."""
    context_files = frontmatter.get('context_files', [])
    if not context_files:
        return "(none specified)"

    parts = []
    git_root = get_git_root(base_path)

    for file_path in context_files:
        full_path = git_root / file_path
        if full_path.exists():
            try:
                content = full_path.read_text()
                # Truncate very long files
                if len(content) > 5000:
                    content = content[:5000] + "\n... (truncated)"
                parts.append(f"### {file_path}\n```\n{content}\n```")
            except Exception as e:
                parts.append(f"### {file_path}\n(Error reading: {e})")
        else:
            parts.append(f"### {file_path}\n(File not found)")

    return '\n\n'.join(parts)


def extract_conventions(tasks_file: Path) -> str:
    """Extract Conventions section from TASKS.md."""
    content = tasks_file.read_text()

    # Find Conventions section
    match = re.search(r'^## Conventions\s*\n(.*?)(?=^## |\Z)', content, re.MULTILINE | re.DOTALL)
    if match:
        return f"\n## Conventions\n{match.group(1).strip()}\n"
    return ""


def generate_phase_prompt(parsed: dict, phase_num: int, tasks_file: Path) -> str:
    """Generate prompt for entire phase."""
    phase = parsed['phases'][phase_num]
    frontmatter = parsed['frontmatter']

    # Get verify command and TDD setting
    verify_command = frontmatter.get('verify', 'just test')
    tdd_enabled = frontmatter.get('tdd', False)

    tdd_guidance = ""
    if tdd_enabled:
        tdd_guidance = """
### TDD Workflow (REQUIRED)
1. Write failing test FIRST
2. Run test, confirm it fails
3. Implement minimal code to pass
4. Run test, confirm it passes
5. Refactor if needed
"""

    # Read context files
    context_files_content = read_context_files(frontmatter, tasks_file.parent)

    # Extract conventions
    conventions_section = extract_conventions(tasks_file)

    # Completed summary from previous phases
    completed_summary_parts = []
    for num in sorted(parsed['phases'].keys()):
        if num >= phase_num:
            break
        p = parsed['phases'][num]
        completed, _, _ = get_phase_status(p)
        if completed:
            completed_summary_parts.append(f"Phase {num} ({p['title']}): {len(completed)} tasks done")

    completed_summary = '\n'.join(completed_summary_parts) if completed_summary_parts else "(Starting fresh)"

    # Task list for this phase
    completed, pending, in_progress = get_phase_status(phase)

    task_lines = []
    for t in completed:
        task_lines.append(f"- [x] {t['content']} ({t['id']})")
    for t in in_progress:
        task_lines.append(f"- [/] {t['content']} ({t['id']}) ← IN PROGRESS")
    for t in pending:
        task_lines.append(f"- [ ] {t['content']} ({t['id']})")

    task_list = '\n'.join(task_lines)

    # Task specifications (only for pending/in-progress)
    spec_parts = []
    for t in in_progress + pending:
        if t['spec']:
            spec_parts.append(f"#### {t['id']}: {t['content']}\n{t['spec']}")

    task_specs = '\n\n'.join(spec_parts) if spec_parts else "(See task descriptions above)"

    return AGENT_SYSTEM_PROMPT.format(
        phase_num=phase_num,
        title=frontmatter.get('title', 'Unknown'),
        verify_command=verify_command,
        tdd_guidance=tdd_guidance,
        context_files_content=context_files_content,
        phase_title=phase['title'],
        phase_description=phase['description'].strip(),
        completed_summary=completed_summary,
        task_list=task_list,
        task_specs=task_specs,
        conventions_section=conventions_section,
    )


def mark_phase_tasks(tasks_file: Path, phase: dict, status: str) -> None:
    """Mark all pending tasks in phase as done or blocked."""
    _, pending, in_progress = get_phase_status(phase)
    for task in pending + in_progress:
        if task['id']:
            if status == 'done':
                run_cru(["tasks", "--file", str(tasks_file), "done", task['id']],
                       cwd=str(tasks_file.parent))
            # For blocked, we'd need per-task handling


def run_agent_interactive(prompt: str, working_dir: Path, phase_num: int,
                          allow_writes: bool = False) -> tuple[str, str, str]:
    """Run claude agent, return (status, reason, output)."""
    import tempfile
    import time

    print("\n" + "="*60)
    print(f"Starting agent for Phase {phase_num}..." +
          (" (writes allowed)" if allow_writes else ""))
    print("="*60 + "\n")

    output_file = tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False)
    output_path = output_file.name
    output_file.close()

    output = ""

    try:
        claude_cmd = ["claude"]
        if allow_writes:
            claude_cmd.append("--dangerously-skip-permissions")
        # Disable subagents for linear, predictable execution
        claude_cmd.extend(["--disallowedTools", "Task"])
        claude_cmd.extend(["--print", "-p", prompt])

        with open(output_path, 'w') as outfile:
            process = subprocess.Popen(
                claude_cmd,
                cwd=str(working_dir),
                stdout=outfile,
                stderr=subprocess.STDOUT,
            )
            process.wait()

        output = Path(output_path).read_text()

        # Check for phase completion
        if f"PHASE {phase_num} COMPLETE" in output:
            return "complete", "", output

        # Check for blocked
        for line in output.split("\n"):
            if line.startswith("BLOCKED:"):
                return "blocked", line[8:].strip(), output

        if process.returncode != 0:
            return "error", f"Agent exited with code {process.returncode}", output

        return "blocked", "Agent finished without PHASE COMPLETE marker", output

    except KeyboardInterrupt:
        return "blocked", "Interrupted by user", output
    except Exception as e:
        return "error", str(e), output
    finally:
        try:
            os.unlink(output_path)
        except:
            pass


def main():
    import argparse
    import time

    parser = argparse.ArgumentParser(description="Phase-based task runner with metrics")
    parser.add_argument("tasks_file", nargs="?", default="TASKS.md")
    parser.add_argument("--phase", type=int, help="Run specific phase only")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--allow-writes", action="store_true")
    parser.add_argument("--export", type=str, help="Export run data to this directory")
    args = parser.parse_args()

    tasks_file = Path(args.tasks_file).resolve()
    if not tasks_file.exists():
        print(f"Error: {tasks_file} not found")
        sys.exit(1)

    parsed = parse_tasks_file(tasks_file)
    title = parsed['frontmatter'].get('title', 'Unknown')

    print(f"Task Runner v2 - {title}")
    print("="*60)
    print(f"Phases: {list(parsed['phases'].keys())}")

    # Find working directory (git root)
    working_dir = get_git_root(tasks_file.parent)
    print(f"Working dir: {working_dir}")

    # Setup export directory
    export_dir = None
    if args.export:
        export_dir = Path(args.export)
        export_dir.mkdir(parents=True, exist_ok=True)
        print(f"Export dir: {export_dir}")

    print()

    # Determine which phase to run
    if args.phase:
        phase_num = args.phase
    else:
        phase_num = get_next_phase(parsed)

    if phase_num is None:
        print("All phases complete!")
        return

    phase = parsed['phases'].get(phase_num)
    if not phase:
        print(f"Phase {phase_num} not found")
        return

    completed, pending, in_progress = get_phase_status(phase)

    # Count tasks from previous phases
    prev_tasks = 0
    for num in sorted(parsed['phases'].keys()):
        if num >= phase_num:
            break
        prev_completed, _, _ = get_phase_status(parsed['phases'][num])
        prev_tasks += len(prev_completed)

    print(f"Phase {phase_num}: {phase['title']}")
    print(f"  Completed: {len(completed)}, Pending: {len(pending)}, In Progress: {len(in_progress)}")
    print(f"  Previous phases: {prev_tasks} tasks done")

    if not pending and not in_progress:
        print(f"Phase {phase_num} already complete!")
        return

    prompt = generate_phase_prompt(parsed, phase_num, tasks_file)

    # Initialize metrics
    metrics = RunMetrics(
        phase_num=phase_num,
        phase_title=phase['title'],
        tasks_total=len(phase['tasks']),
        tasks_pending=len(pending) + len(in_progress),
        tasks_completed_before=len(completed),
        previous_phases_tasks=prev_tasks,
        prompt_chars=len(prompt),
        prompt_lines=prompt.count('\n') + 1,
        prompt_tokens_est=len(prompt) // 4,
    )

    if args.dry_run:
        print(f"\n[DRY RUN] Prompt for Phase {phase_num}:")
        print(f"  Chars: {metrics.prompt_chars:,}")
        print(f"  Lines: {metrics.prompt_lines}")
        print(f"  Est tokens: ~{metrics.prompt_tokens_est:,}")
        print("-" * 40)
        print(prompt)
        print("-" * 40)

        if export_dir:
            export = RunExport(
                metrics=metrics,
                prompt=prompt,
                tasks_file_snapshot=tasks_file.read_text()
            )
            run_dir = export.save(export_dir)
            print(f"\n[DRY RUN] Exported to: {run_dir}")
        return

    # Mark all pending as in-progress
    for task in pending:
        if task['id']:
            run_cru(["tasks", "--file", str(tasks_file), "pick", task['id']],
                   cwd=str(tasks_file.parent))

    # Run with timing
    metrics.start_time = datetime.now().isoformat()
    start = time.time()

    status, reason, output = run_agent_interactive(prompt, working_dir, phase_num, args.allow_writes)

    end = time.time()
    metrics.end_time = datetime.now().isoformat()
    metrics.duration_seconds = end - start
    metrics.status = status
    metrics.reason = reason
    metrics.output_chars = len(output)
    metrics.output_lines = output.count('\n') + 1 if output else 0

    if status == "complete":
        # Mark all tasks done
        for task in pending + in_progress:
            if task['id']:
                run_cru(["tasks", "--file", str(tasks_file), "done", task['id']],
                       cwd=str(tasks_file.parent))
        print(f"\n[x] Phase {phase_num} completed!")
    else:
        print(f"\n[!] Phase {phase_num} blocked: {reason}")

    # Print metrics summary
    print(metrics.summary())

    # Export if requested
    if export_dir:
        export = RunExport(
            metrics=metrics,
            prompt=prompt,
            output=output,
            tasks_file_snapshot=tasks_file.read_text()
        )
        run_dir = export.save(export_dir)
        print(f"Exported to: {run_dir}")

    print(f"\nRun again to continue with next phase.")


def summarize_runs(export_dir: Path) -> None:
    """Summarize all runs in an export directory."""
    runs = []
    for run_dir in sorted(export_dir.iterdir()):
        metrics_file = run_dir / "metrics.json"
        if metrics_file.exists():
            metrics = json.loads(metrics_file.read_text())
            runs.append(metrics)

    if not runs:
        print("No runs found.")
        return

    print(f"\n{'='*70}")
    print(f"  Run Summary - {len(runs)} phases")
    print(f"{'='*70}\n")

    total_duration = sum(r['duration_seconds'] for r in runs)
    total_tasks = sum(r['tasks_pending'] for r in runs)
    total_prompt_tokens = sum(r['prompt_tokens_est'] for r in runs)
    total_output_chars = sum(r['output_chars'] for r in runs)

    completed = [r for r in runs if r['status'] == 'complete']
    blocked = [r for r in runs if r['status'] == 'blocked']

    print(f"  Phases completed: {len(completed)} / {len(runs)}")
    print(f"  Tasks attempted:  {total_tasks}")
    print(f"  Total duration:   {total_duration:.1f}s ({total_duration/60:.1f}m)")
    print(f"  Total prompt tokens: ~{total_prompt_tokens:,}")
    print(f"  Total output chars:  {total_output_chars:,}")
    print()

    if total_tasks > 0 and total_duration > 0:
        print(f"  Avg time per task: {total_duration/total_tasks:.1f}s")
        print(f"  Avg tokens per phase: ~{total_prompt_tokens/len(runs):.0f}")
    print()

    # Per-phase breakdown
    print(f"  {'Phase':<8} {'Tasks':<8} {'Duration':<12} {'Status':<10}")
    print(f"  {'-'*8} {'-'*8} {'-'*12} {'-'*10}")
    for r in runs:
        duration_str = f"{r['duration_seconds']:.1f}s" if r['duration_seconds'] else "-"
        print(f"  {r['phase_num']:<8} {r['tasks_pending']:<8} {duration_str:<12} {r['status'] or 'pending':<10}")

    if blocked:
        print(f"\n  Blocked phases:")
        for r in blocked:
            print(f"    Phase {r['phase_num']}: {r['reason']}")

    print(f"\n{'='*70}\n")


if __name__ == "__main__":
    import sys

    # Check for summary subcommand
    if len(sys.argv) > 1 and sys.argv[1] == "summary":
        if len(sys.argv) < 3:
            print("Usage: task-runner-v2.py summary <export_dir>")
            sys.exit(1)
        summarize_runs(Path(sys.argv[2]))
    else:
        main()
