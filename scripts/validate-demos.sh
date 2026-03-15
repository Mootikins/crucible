#!/usr/bin/env bash
set -euo pipefail

FIXTURES_DIR="assets/fixtures"
GOLDEN_DIR="assets/fixtures/golden"
PASS=0; FAIL=0; TOTAL=0

validate_fixture() {
  local name="$1"
  local fixture="$FIXTURES_DIR/$name.jsonl"
  local golden="$GOLDEN_DIR/$name.keywords"

  echo "=== Validating: $name ==="

  # Check fixture exists
  if [[ ! -f "$fixture" ]]; then
    echo "  FAIL: Fixture not found: $fixture"
    ((FAIL++)) || true; ((TOTAL++)) || true; return
  fi

  # Extract all full_response texts
  local responses
  responses=$(grep '"message_complete"' "$fixture" | jq -r '.data.full_response' 2>/dev/null)

  if [[ -z "$responses" ]]; then
    echo "  FAIL: No message_complete events found"
    ((FAIL++)) || true; ((TOTAL++)) || true; return
  fi

  # Check response length (quality gate: not empty, not too short)
  local char_count
  char_count=$(echo "$responses" | wc -c)
  if (( char_count < 100 )); then
    echo "  FAIL: Response too short ($char_count chars)"
    ((FAIL++)) || true; ((TOTAL++)) || true; return
  fi
  echo "  PASS: Response length ($char_count chars)"
  ((PASS++)) || true; ((TOTAL++)) || true

  # Check golden keywords (case-insensitive, fixed-string to avoid regex issues)
  if [[ -f "$golden" ]]; then
    while IFS= read -r keyword; do
      [[ "$keyword" =~ ^#|^$ ]] && continue  # skip comments/blanks
      if echo "$responses" | grep -qiF "$keyword"; then
        echo "  PASS: Keyword found: $keyword"
        ((PASS++)) || true; ((TOTAL++)) || true
      else
        echo "  FAIL: Keyword missing: $keyword"
        ((FAIL++)) || true; ((TOTAL++)) || true
      fi
    done < "$golden"
  else
    echo "  WARN: No golden file found: $golden"
  fi

  # Check for factual negation patterns
  if echo "$responses" | grep -qiE 'does not (use|support|have) (wikilink|semantic|plugin|lua)'; then
    echo "  FAIL: Detected factual negation about a real feature"
    ((FAIL++)) || true; ((TOTAL++)) || true
  else
    echo "  PASS: No factual negation detected"
    ((PASS++)) || true; ((TOTAL++)) || true
  fi
  echo
}

validate_fixture "demo"
validate_fixture "acp-demo"
validate_fixture "delegation-demo"

echo "=== Summary ==="
echo "PASS: $PASS / TOTAL: $TOTAL / FAIL: $FAIL"

if (( FAIL > 0 )); then
  echo "VERDICT: FAIL"
  exit 1
else
  echo "VERDICT: PASS"
  exit 0
fi
