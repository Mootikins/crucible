#!/usr/bin/env python3
"""
Agent Symlink Setup Script

This script creates the proper symlink structure for AI agents to access
both global and project-specific configurations.

Architecture:
- Global dotfiles: ~/dotfiles/agents/
  - dot-agents/        # Global agent configurations (committed)
  - dot-claude/       # Claude-specific symlinks to global configs
  - dot-crush/        # Crush-specific symlinks to global configs
  - dot-cursor/       # Cursor-specific symlinks to global configs

- Project-specific: [auto-detected project root]/
  - .agents/          # Project-specific configurations (committed)
  - .claude/          # Symlinks to project .agents/
  - .crush/           # Symlinks to project .agents/
  - .cursor/          # Symlinks to project .agents/

Agent Specification:
Each agent expects a specific directory structure with metadata headers
containing triggers, prompts, tools, workflows, etc. This script creates
the proper mapping between agent-specific directory names and the
standardized .agents/ structure.

Symlink Mapping:
- claude: commands -> commands, config -> config, etc.
- crush:  commands -> commands, config -> config, etc.
- cursor: commands -> rules, config -> config, etc. (Cursor uses 'rules')

Usage:
  # Auto-detect project root (recommended)
  python3 scripts/setup-agent-symlinks.py

  # Specify custom paths
  python3 scripts/setup-agent-symlinks.py --project-root /path/to/project --global-agents-root /path/to/global/configs
"""

import argparse
import os
import shutil
import sys
from pathlib import Path
from typing import Dict, List, Tuple

# Configuration - auto-detect project root and allow overrides
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))

# Auto-detect project root by walking up from scripts/ directory
# until we find a directory that looks like a project root
def detect_project_root():
    """Auto-detect the project root by looking for common project indicators."""
    current_dir = os.path.dirname(SCRIPT_DIR)  # Parent of scripts/

    # Common indicators of a project root
    project_indicators = [
        '.git',           # Git repository
        'package.json',   # Node.js project
        'Cargo.toml',     # Rust project
        'pyproject.toml', # Python project
        'go.mod',         # Go project
        'README.md',      # Documentation
    ]

    # Walk up the directory tree
    while current_dir != os.path.dirname(current_dir):  # Stop at filesystem root
        if any(os.path.exists(os.path.join(current_dir, indicator))
               for indicator in project_indicators):
            return current_dir
        current_dir = os.path.dirname(current_dir)

    # Fallback to parent of scripts/
    return os.path.dirname(SCRIPT_DIR)

DEFAULT_PROJECT_ROOT = detect_project_root()
DEFAULT_GLOBAL_AGENTS_ROOT = os.path.expanduser("~/dotfiles/agents")
DEFAULT_HOME_AGENTS = os.path.expanduser("~/.agents")


# Agent directory mappings: agent -> [(agent_dir, global_dir), ...]
AGENT_MAPPINGS: Dict[str, List[Tuple[str, str]]] = {
    "claude": [
        ("commands", "commands"),
        ("config", "config"),
        ("contexts", "contexts"),
        ("tools", "tools"),
        ("workflows", "workflows"),
    ],
    "crush": [
        ("commands", "commands"),
        ("config", "config"),
        ("contexts", "contexts"),
        ("tools", "tools"),
        ("workflows", "workflows"),
    ],
    "cursor": [
        ("commands", "commands"),
        ("config", "config"),
        ("contexts", "contexts"),
        ("tools", "tools"),
        ("workflows", "workflows"),
        ("rules", "commands"),  # Cursor uses 'rules' for commands
    ],
}


class AgentSymlinkManager:
    """Manages agent symlink creation and cleanup."""

    def __init__(self, project_root: Path, global_agents_root: Path):
        self.project_root = Path(project_root)
        self.global_agents_root = Path(global_agents_root)
        self.project_agents = self.project_root / ".agents"
        self.global_dot_agents = self.global_agents_root / "dot-agents"

    def cleanup_global_agents(self):
        """Remove individual agent directories from global system."""
        print("🧹 Cleaning up global agent directories...")
        global_agents_path = Path(DEFAULT_HOME_AGENTS)
        for agent in AGENT_MAPPINGS.keys():
            agent_dir = global_agents_path / agent
            if agent_dir.exists():
                print(f"  Removing {agent} directory from global system")
                shutil.rmtree(agent_dir)

    def create_global_directories(self):
        """Create global agent directories."""
        print("📁 Creating global agent directories...")
        for agent in AGENT_MAPPINGS.keys():
            agent_root = self.global_agents_root / f"dot-{agent}"
            agent_root.mkdir(parents=True, exist_ok=True)

            # Create subdirectories
            for agent_dir, _ in AGENT_MAPPINGS[agent]:
                (agent_root / agent_dir).mkdir(parents=True, exist_ok=True)

    def create_project_directories(self):
        """Create project-specific agent directories."""
        print("📁 Creating project-specific agent directories...")
        for agent in AGENT_MAPPINGS.keys():
            agent_root = self.project_root / f".{agent}"
            agent_root.mkdir(parents=True, exist_ok=True)

            # Create subdirectories
            for agent_dir, _ in AGENT_MAPPINGS[agent]:
                (agent_root / agent_dir).mkdir(parents=True, exist_ok=True)

    def create_global_symlinks(self):
        """Create symlinks in global dotfiles."""
        print("🔗 Creating global symlinks...")
        for agent in AGENT_MAPPINGS.keys():
            print(f"  Creating symlinks for {agent}...")
            agent_root = self.global_agents_root / f"dot-{agent}"

            for agent_dir, global_dir in AGENT_MAPPINGS[agent]:
                target = self.global_dot_agents / global_dir
                link_path = agent_root / agent_dir

                if link_path.exists() or link_path.is_symlink():
                    link_path.unlink()

                try:
                    link_path.symlink_to(target, target_is_directory=True)
                    print(f"    {agent_dir} -> {target}")
                except OSError as e:
                    print(f"    Failed to create symlink {link_path} -> {target}: {e}")

    def create_project_symlinks(self):
        """Create symlinks in project."""
        print("🔗 Creating project symlinks...")
        for agent in AGENT_MAPPINGS.keys():
            print(f"  Creating symlinks for {agent} in project...")
            agent_root = self.project_root / f".{agent}"

            for agent_dir, global_dir in AGENT_MAPPINGS[agent]:
                target = self.project_agents / global_dir
                link_path = agent_root / agent_dir

                if link_path.exists() or link_path.is_symlink():
                    link_path.unlink()

                try:
                    link_path.symlink_to(target, target_is_directory=True)
                    print(f"    {agent_dir} -> {target}")
                except OSError as e:
                    print(f"    Failed to create symlink {link_path} -> {target}: {e}")

    def create_home_symlinks(self):
        """Create home directory symlinks."""
        print("🏠 Creating home directory symlinks...")
        home_agents = Path(DEFAULT_HOME_AGENTS)
        home_agents.mkdir(parents=True, exist_ok=True)

        # Remove individual agent directories from home
        for agent in AGENT_MAPPINGS.keys():
            agent_dir = home_agents / agent
            if agent_dir.exists():
                shutil.rmtree(agent_dir)

        # Create symlinks to project .agents
        for agent in AGENT_MAPPINGS.keys():
            agent_dir = home_agents / agent
            try:
                if agent_dir.exists() or agent_dir.is_symlink():
                    agent_dir.unlink()
                agent_dir.symlink_to(self.project_agents, target_is_directory=True)
                print(f"  ~/.agents/{agent} -> {self.project_agents}")
            except OSError as e:
                print(f"  Failed to create home symlink for {agent}: {e}")

    def setup(self):
        """Run the complete setup process."""
        print("🔧 Setting up agent symlinks for Crucible project...")
        print(f"  Project root: {self.project_root}")
        print(f"  Global agents root: {self.global_agents_root}")
        print(f"  Project agents: {self.project_agents}")

        self.cleanup_global_agents()
        self.create_global_directories()
        self.create_project_directories()
        self.create_global_symlinks()
        self.create_project_symlinks()
        self.create_home_symlinks()

        print("✅ Agent symlink setup complete!")


def main():
    parser = argparse.ArgumentParser(description="Setup agent symlinks for Crucible project")
    parser.add_argument("--project-root", default=DEFAULT_PROJECT_ROOT,
                       help=f"Path to project root (default: {DEFAULT_PROJECT_ROOT})")
    parser.add_argument("--global-agents-root", default=DEFAULT_GLOBAL_AGENTS_ROOT,
                       help=f"Path to global agents root (default: {DEFAULT_GLOBAL_AGENTS_ROOT})")

    args = parser.parse_args()

    manager = AgentSymlinkManager(args.project_root, args.global_agents_root)
    manager.setup()


if __name__ == "__main__":
    main()
