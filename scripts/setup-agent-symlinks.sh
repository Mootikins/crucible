#!/bin/bash

# Setup agent symlinks for Crucible project
# This script creates project-level agent directories and symlinks them to the global agent system

set -e

# Configuration dictionary for agent directory mappings
declare -A AGENT_MAPPINGS=(
    # Agent: "global_dir:agent_dir"
    ["claude:commands"]="commands:commands"
    ["claude:config"]="config:config"
    ["claude:contexts"]="contexts:contexts"
    ["claude:tools"]="tools:tools"
    ["claude:workflows"]="workflows:workflows"
    
    ["crush:commands"]="commands:commands"
    ["crush:config"]="config:config"
    ["crush:contexts"]="contexts:contexts"
    ["crush:tools"]="tools:tools"
    ["crush:workflows"]="workflows:workflows"
    
    ["cursor:commands"]="commands:rules"
    ["cursor:config"]="config:config"
    ["cursor:contexts"]="contexts:contexts"
    ["cursor:tools"]="tools:tools"
    ["cursor:workflows"]="workflows:workflows"
)

# Global agent system path
GLOBAL_AGENTS="/home/moot/.agents"

# Project root
PROJECT_ROOT="/home/moot/crucible"

# Project agents path
PROJECT_AGENTS="$PROJECT_ROOT/.agents"

echo "🔧 Setting up agent symlinks for Crucible project..."

# Step 1: Remove individual agent directories from global system
echo "📁 Cleaning up global agent directories..."
cd "$GLOBAL_AGENTS"
for agent in claude crush cursor; do
    if [ -d "$agent" ]; then
        echo "  Removing $agent directory from global system"
        rm -rf "$agent"
    fi
done

# Step 2: Create project-level agent directories
echo "📁 Creating project-level agent directories..."
cd "$PROJECT_ROOT"
for agent in claude crush cursor; do
    agent_dir=".$agent"
    if [ ! -d "$agent_dir" ]; then
        echo "  Creating $agent_dir directory"
        mkdir -p "$agent_dir"
    fi
done

# Step 3: Create symlinks based on agent mappings
echo "🔗 Creating agent-specific symlinks..."
cd "$PROJECT_AGENTS"

# Get all unique global directories
global_dirs=($(ls -1 "$GLOBAL_AGENTS" | grep -v '^\.'))

for global_dir in "${global_dirs[@]}"; do
    echo "  Processing global directory: $global_dir"
    
    # Create symlink for each agent
    for agent in claude crush cursor; do
        mapping_key="$agent:$global_dir"
        if [[ -n "${AGENT_MAPPINGS[$mapping_key]}" ]]; then
            # Parse mapping: "global_dir:agent_dir"
            IFS=':' read -r src_dir dst_dir <<< "${AGENT_MAPPINGS[$mapping_key]}"
            
            # Create agent directory if it doesn't exist
            agent_dir="$PROJECT_ROOT/.$agent"
            mkdir -p "$agent_dir"
            
            # Create symlink
            src_path="$GLOBAL_AGENTS/$src_dir"
            dst_path="$agent_dir/$dst_dir"
            
            if [ -L "$dst_path" ] || [ -e "$dst_path" ]; then
                echo "    Removing existing $dst_path"
                rm -rf "$dst_path"
            fi
            
            echo "    Creating symlink: $dst_path -> $src_path"
            ln -sf "$src_path" "$dst_path"
        else
            echo "    No mapping for $agent:$global_dir (leaving empty)"
        fi
    done
done

# Step 4: Create project-level symlinks to agent directories
echo "🔗 Creating project-level symlinks..."
cd "$PROJECT_AGENTS"

# For now, default to cursor (can be changed later)
for global_dir in "${global_dirs[@]}"; do
    mapping_key="cursor:$global_dir"
    if [[ -n "${AGENT_MAPPINGS[$mapping_key]}" ]]; then
        IFS=':' read -r src_dir dst_dir <<< "${AGENT_MAPPINGS[$mapping_key]}"
        
        src_path="$PROJECT_ROOT/.cursor/$dst_dir"
        dst_path="$PROJECT_AGENTS/$global_dir"
        
        if [ -L "$dst_path" ] || [ -e "$dst_path" ]; then
            echo "  Removing existing $dst_path"
            rm -rf "$dst_path"
        fi
        
        echo "  Creating project symlink: $dst_path -> $src_path"
        ln -sf "$src_path" "$dst_path"
    fi
done

echo "✅ Agent symlinks setup complete!"
echo ""
echo "📋 Summary:"
echo "  Global system: $GLOBAL_AGENTS"
echo "  Project agents: $PROJECT_AGENTS"
echo "  Agent directories:"
for agent in claude crush cursor; do
    echo "    .$agent -> $PROJECT_ROOT/.$agent"
done
echo ""
echo "🔧 To switch default agent, update the project-level symlinks in $PROJECT_AGENTS"
