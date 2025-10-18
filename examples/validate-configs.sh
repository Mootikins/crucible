#!/bin/bash

# Simple validation script for crucible configuration examples
set -e

echo "🔍 Validating Crucible Configuration Examples"
echo "============================================="

# Function to check if file exists and has content
check_file() {
    local file=$1
    if [ -f "$file" ] && [ -s "$file" ]; then
        echo -e "✅ $file exists and has content"
        return 0
    else
        echo -e "❌ $file is missing or empty"
        return 1
    fi
}

# Function to validate YAML
validate_yaml() {
    local file=$1
    echo -n "Validating $file (YAML)... "

    if python3 -c "
import yaml
import sys
try:
    with open('$file', 'r', encoding='utf-8') as f:
        yaml.safe_load(f)
    print('✅ Valid YAML')
except Exception as e:
    print(f'❌ Error: {e}')
    sys.exit(1)
" 2>/dev/null; then
        return 0
    else
        return 1
    fi
}

# Function to validate TOML
validate_toml() {
    local file=$1
    echo -n "Validating $file (TOML)... "

    if python3 -c "
import tomllib
import sys
try:
    with open('$file', 'rb') as f:
        tomllib.load(f)
    print('✅ Valid TOML')
except Exception as e:
    print(f'❌ Error: {e}')
    sys.exit(1)
" 2>/dev/null; then
        return 0
    else
        return 1
    fi
}

# Function to validate JSON
validate_json() {
    local file=$1
    echo -n "Validating $file (JSON)... "

    if python3 -c "
import json
import sys
try:
    with open('$file', 'r', encoding='utf-8') as f:
        json.load(f)
    print('✅ Valid JSON')
except Exception as e:
    print(f'❌ Error: {e}')
    sys.exit(1)
" 2>/dev/null; then
        return 0
    else
        return 1
    fi
}

# Check all files exist
echo ""
echo "📁 Checking Files"
echo "----------------"
check_file "development-config.yaml"
check_file "production-config.yaml"
check_file "testing-config.toml"
check_file "minimal-config.json"
check_file "README.md"
check_file "migration-example.md"
check_file ".gitignore"
check_file "validate-configs.sh"
echo ""

# Validate configuration files
echo "📄 Validating Configuration Files"
echo "--------------------------------"

# YAML files
validate_yaml "development-config.yaml"
validate_yaml "production-config.yaml"

# TOML files
validate_toml "testing-config.toml"

# JSON files
validate_json "minimal-config.json"

echo ""
echo "📊 Configuration Summary"
echo "------------------------"

# Extract some basic info from configs
echo "📋 Development Config (YAML):"
if grep -q "profile.*development" development-config.yaml; then
    echo "  ✅ Has development profile"
fi
if grep -q "ollama" development-config.yaml; then
    echo "  ✅ Uses Ollama provider"
fi

echo ""
echo "📋 Production Config (YAML):"
if grep -q "profile.*production" production-config.yaml; then
    echo "  ✅ Has production profile"
fi
if grep -q "openai" production-config.yaml; then
    echo "  ✅ Uses OpenAI provider"
fi

echo ""
echo "📋 Testing Config (TOML):"
if grep -q "^profile.*testing" testing-config.toml; then
    echo "  ✅ Has testing profile"
fi
if grep -q "custom" testing-config.toml; then
    echo "  ✅ Uses custom (mock) provider"
fi

echo ""
echo "📋 Minimal Config (JSON):"
if grep -q '"profile".*"minimal"' minimal-config.json; then
    echo "  ✅ Has minimal profile"
fi

echo ""
echo "🎯 Usage Examples"
echo "----------------"
echo "# Development:"
echo "cp development-config.yaml crucible-config.yaml"
echo "crucible-mcp-server --config ./crucible-config.yaml"
echo ""
echo "# Testing:"
echo "cp testing-config.toml crucible-config.toml"
echo "CRUCIBLE_PROFILE=testing crucible-mcp-server --config ./crucible-config.toml"
echo ""
echo "# Production:"
echo "cp production-config.yaml crucible-config.yaml"
echo "export OPENAI_API_KEY=your-key"
echo "export DATABASE_URL=your-db-url"
echo "crucible-mcp-server --config ./crucible-config.yaml"

echo ""
echo "✅ All validations completed successfully!"
echo "🚚 Ready to use crucible-config system!"