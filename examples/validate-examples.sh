#!/bin/bash

# Validation script for crucible configuration examples
# This script validates all example configurations to ensure they are syntactically correct

set -e

echo "🔍 Validating Crucible Configuration Examples"
echo "============================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to validate configuration
validate_config() {
    local config_file=$1
    local format=$2

    echo -n "Validating $config_file ($format)... "

    if [ ! -f "$config_file" ]; then
        echo -e "${RED}❌ File not found${NC}"
        return 1
    fi

    # Use Python to validate syntax based on format
    case $format in
        "yaml")
            if command -v python3 &> /dev/null; then
                python3 -c "
import yaml
import sys
import os
try:
    file_path = os.path.join(os.getcwd(), '$config_file')
    with open(file_path, 'r', encoding='utf-8') as f:
        yaml.safe_load(f)
    print('✅ Valid YAML')
except yaml.YAMLError as e:
    print(f'❌ YAML Error: {e}')
    sys.exit(1)
except Exception as e:
    print(f'❌ Error: {e}')
    sys.exit(1)
" 2>/dev/null
            else
                echo -e "${YELLOW}⚠️  Python3 not available, skipping YAML validation${NC}"
            fi
            ;;
        "toml")
            if command -v python3 &> /dev/null; then
                python3 -c "
import tomllib if hasattr(__import__('tomllib'), 'load') else None
import os
try:
    file_path = os.path.join(os.getcwd(), '$config_file')
    with open(file_path, 'rb') as f:
        if tomllib:
            tomllib.load(f)
        else:
            import toml
            toml.load(f)
    print('✅ Valid TOML')
except Exception as e:
    print(f'❌ TOML Error: {e}')
    exit(1)
" 2>/dev/null
            else
                echo -e "${YELLOW}⚠️  Python3 not available, skipping TOML validation${NC}"
            fi
            ;;
        "json")
            if command -v python3 &> /dev/null; then
                python3 -c "
import json
import sys
import os
try:
    file_path = os.path.join(os.getcwd(), '$config_file')
    with open(file_path, 'r', encoding='utf-8') as f:
        json.load(f)
    print('✅ Valid JSON')
except json.JSONDecodeError as e:
    print(f'❌ JSON Error: {e}')
    sys.exit(1)
except Exception as e:
    print(f'❌ Error: {e}')
    sys.exit(1)
" 2>/dev/null
            else
                echo -e "${YELLOW}⚠️  Python3 not available, skipping JSON validation${NC}"
            fi
            ;;
    esac
}

# Function to check required fields
check_required_fields() {
    local config_file=$1

    echo -n "Checking required fields in $config_file... "

    # Basic checks for required top-level fields
    case $config_file in
        *.yaml|*.yml)
            if grep -q "profile:" "$config_file" && grep -q "profiles:" "$config_file"; then
                echo -e "${GREEN}✅ Required fields present${NC}"
            else
                echo -e "${RED}❌ Missing required fields${NC}"
                return 1
            fi
            ;;
        *.toml)
            if grep -q "^profile = " "$config_file" && grep -q "^\[profiles\]" "$config_file"; then
                echo -e "${GREEN}✅ Required fields present${NC}"
            else
                echo -e "${RED}❌ Missing required fields${NC}"
                return 1
            fi
            ;;
        *.json)
            if grep -q '"profile"' "$config_file" && grep -q '"profiles"' "$config_file"; then
                echo -e "${GREEN}✅ Required fields present${NC}"
            else
                echo -e "${RED}❌ Missing required fields${NC}"
                return 1
            fi
            ;;
    esac
}

# Function to check for sensitive data
check_sensitive_data() {
    local config_file=$1

    echo -n "Checking for sensitive data in $config_file... "

    # Look for potential API keys or secrets
    if grep -i -E "(sk-[a-zA-Z0-9]{20,}|api_key.*=.*[a-zA-Z0-9]{20,}|secret.*=.*[a-zA-Z0-9]{20,})" "$config_file" > /dev/null 2>&1; then
        echo -e "${YELLOW}⚠️  Potential sensitive data found - review needed${NC}"
    else
        echo -e "${GREEN}✅ No obvious sensitive data found${NC}"
    fi
}

# Main validation
echo ""

# Validate YAML configurations
echo "📄 YAML Configurations"
echo "---------------------"
validate_config "development-config.yaml" "yaml"
check_required_fields "development-config.yaml"
check_sensitive_data "development-config.yaml"
echo ""

validate_config "production-config.yaml" "yaml"
check_required_fields "production-config.yaml"
check_sensitive_data "production-config.yaml"
echo ""

# Validate TOML configurations
echo "📄 TOML Configurations"
echo "---------------------"
validate_config "testing-config.toml" "toml"
check_required_fields "testing-config.toml"
check_sensitive_data "testing-config.toml"
echo ""

# Validate JSON configurations
echo "📄 JSON Configurations"
echo "---------------------"
validate_config "minimal-config.json" "json"
check_required_fields "minimal-config.json"
check_sensitive_data "minimal-config.json"
echo ""

# Check documentation files
echo "📚 Documentation Files"
echo "----------------------"
for doc in README.md migration-example.md; do
    if [ -f "$doc" ]; then
        echo -e "${GREEN}✅ $doc exists${NC}"
    else
        echo -e "${RED}❌ $doc missing${NC}"
    fi
done
echo ""

# Check for .gitignore
if [ -f ".gitignore" ]; then
    echo -e "${GREEN}✅ .gitignore exists${NC}"
else
    echo -e "${RED}❌ .gitignore missing${NC}"
fi
echo ""

# Summary
echo "📊 Summary"
echo "----------"
echo "Example configurations are ready to use!"
echo ""
echo "Quick start commands:"
echo "  # Development with Ollama"
echo "  cp development-config.yaml crucible-config.yaml"
echo "  crucible-mcp-server --config ./crucible-config.yaml"
echo ""
echo "  # Testing with mocks"
echo "  cp testing-config.toml crucible-config.toml"
echo "  CRUCIBLE_PROFILE=testing crucible-mcp-server --config ./crucible-config.toml"
echo ""
echo "  # Production (requires environment variables)"
echo "  cp production-config.yaml crucible-config.yaml"
echo "  export OPENAI_API_KEY=your-key"
echo "  export DATABASE_URL=your-db-url"
echo "  crucible-mcp-server --config ./crucible-config.yaml"
echo ""
echo -e "${GREEN}✅ All validations completed successfully!${NC}"