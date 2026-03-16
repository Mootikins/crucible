#!/usr/bin/env bash
set -euo pipefail

FIXTURES_DIR="assets/fixtures"
GOLDEN_DIR="assets/fixtures/golden"
PASS=0; FAIL=0; WARN=0; TOTAL=0

warn() {
  echo "  WARN: $1"
  ((WARN++)) || true
}

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

  # FAIL Check 1: Tool errors in recording
  local tool_errors
  tool_errors=$(grep '"tool_result"' "$fixture" 2>/dev/null | \
    jq -r 'select(.data.result.error != null) | .data.result.error' 2>/dev/null | wc -l) || tool_errors=0
  if (( tool_errors > 0 )); then
    echo "  FAIL: $tool_errors tool call(s) resulted in errors"
    ((FAIL++)) || true; ((TOTAL++)) || true
  else
    echo "  PASS: No tool errors"
    ((PASS++)) || true; ((TOTAL++)) || true
  fi

  # FAIL Check 2: Precognition present in hero demo only
  if [[ "$name" == "demo" ]]; then
    local precog_count
    precog_count=$(grep -c '"precognition_complete"' "$fixture" 2>/dev/null || true)
    if (( precog_count == 0 )); then
      echo "  FAIL: Hero demo missing precognition_complete event"
      ((FAIL++)) || true; ((TOTAL++)) || true
    else
      echo "  PASS: Precognition event present ($precog_count)"
      ((PASS++)) || true; ((TOTAL++)) || true
    fi
  fi

  # WARN Check 3: Thinking event count
  if [[ "$name" == "demo" ]]; then
    local think_count
    think_count=$(grep -c '"thinking"' "$fixture" 2>/dev/null || echo 0)
    if (( think_count > 150 )); then
      warn "High thinking event count ($think_count > 150); model over-deliberating"
    else
      echo "  PASS: Thinking event count ($think_count)"
      ((PASS++)) || true; ((TOTAL++)) || true
    fi
  fi

  # WARN Check 4: Response length per fixture
  local max_chars=600
  [[ "$name" == "acp-demo" ]] && max_chars=800
  [[ "$name" == "delegation-demo" ]] && max_chars=300
  if (( char_count > max_chars )); then
    warn "Response too long ($char_count chars > $max_chars limit)"
  fi

  # WARN Check 5: Prompt engineering phrases in user messages
  local user_msgs
  user_msgs=$(grep '"user_message"' "$fixture" | jq -r '.data.content // .data.text // empty' 2>/dev/null)
  if echo "$user_msgs" | grep -qiE 'make sure|be (concise|brief|detailed|specific)|in markdown|format (your|the)|please use|do not use|avoid using|act as|you are a'; then
    warn "Visible prompt engineering in user message(s)"
  fi

  # WARN Check 6: Hedging language in responses
  if echo "$responses" | grep -qiE "i (cannot|can't|don't|am not able)|i'm not sure|i would note|i should mention|it's (important|worth) (to )?(note|mention)|please note that"; then
    warn "Hedging/disclaimer language detected in response"
  fi

  echo
}

validate_fixture "demo"
validate_fixture "acp-demo"
validate_fixture "delegation-demo"

echo "=== Summary ==="
echo "PASS: $PASS / WARN: $WARN / FAIL: $FAIL / TOTAL: $TOTAL"

if (( FAIL > 0 )); then
  echo "VERDICT: FAIL"
  exit 1
else
  echo "VERDICT: PASS"
  exit 0
fi
