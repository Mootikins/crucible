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

# === SUPER STRICT Recording Quality Checks ===
check_strict_recording_quality() {
  local fixture="$1"
  local name=$(basename "$fixture" .jsonl)
  
  echo "=== Strict Recording Quality: $name ==="
  
  if [[ ! -f "$fixture" ]]; then
    echo "  FAIL: Fixture not found: $fixture"
    ((FAIL++)) || true; ((TOTAL++)) || true; return
  fi
  
  # Check 1: user_message content doesn't contain prompt engineering
  local user_msgs
  user_msgs=$(grep '"user_message"' "$fixture" | jq -r '.data.content // empty' 2>/dev/null)
  if echo "$user_msgs" | grep -qiE 'use the|call the tool|delegate_session'; then
    echo "  FAIL: user_message contains prompt engineering phrases"
    ((FAIL++)) || true; ((TOTAL++)) || true
  else
    echo "  PASS: user_message clean (no prompt engineering)"
    ((PASS++)) || true; ((TOTAL++)) || true
  fi
  
  # Check 2: message_complete full_response length is 100-2000 chars
  local responses
  responses=$(grep '"message_complete"' "$fixture" | jq -r '.data.full_response // empty' 2>/dev/null)
  local resp_len
  resp_len=$(echo "$responses" | wc -c)
  if (( resp_len < 100 || resp_len > 2000 )); then
    echo "  FAIL: message_complete full_response length out of range ($resp_len chars, need 100-2000)"
    ((FAIL++)) || true; ((TOTAL++)) || true
  else
    echo "  PASS: message_complete full_response length valid ($resp_len chars)"
    ((PASS++)) || true; ((TOTAL++)) || true
  fi
  
  # Check 3: seq numbers are monotonically increasing
  local seq_nums
  seq_nums=$(grep '"seq"' "$fixture" | jq -r '.seq' 2>/dev/null | tr '\n' ' ')
  local prev_seq=0
  local seq_valid=true
  for seq in $seq_nums; do
    if (( seq <= prev_seq )); then
      seq_valid=false
      break
    fi
    prev_seq=$seq
  done
  if [[ "$seq_valid" == "false" ]]; then
    echo "  FAIL: seq numbers not monotonically increasing"
    ((FAIL++)) || true; ((TOTAL++)) || true
  else
    echo "  PASS: seq numbers monotonically increasing"
    ((PASS++)) || true; ((TOTAL++)) || true
  fi
  
  # Check 4: ts timestamps are non-decreasing (ISO 8601 string comparison)
  local ts_list
  ts_list=$(grep '"ts"' "$fixture" | jq -r '.ts' 2>/dev/null | tr '\n' ' ')
  local prev_ts=""
  local ts_valid=true
  for ts in $ts_list; do
    if [[ -n "$prev_ts" && "$ts" < "$prev_ts" ]]; then
      ts_valid=false
      break
    fi
    prev_ts="$ts"
  done
  if [[ "$ts_valid" == "false" ]]; then
    echo "  FAIL: ts timestamps not monotonically increasing"
    ((FAIL++)) || true; ((TOTAL++)) || true
  else
    echo "  PASS: ts timestamps monotonically increasing"
    ((PASS++)) || true; ((TOTAL++)) || true
  fi
  
  # Check 5: no empty event type strings
  local empty_events
  empty_events=$(grep '"event":""' "$fixture" 2>/dev/null | wc -l) || empty_events=0
  if (( empty_events > 0 )); then
    echo "  FAIL: $empty_events event(s) with empty type string"
    ((FAIL++)) || true; ((TOTAL++)) || true
  else
    echo "  PASS: no empty event type strings"
    ((PASS++)) || true; ((TOTAL++)) || true
  fi
  
  # Check 6: footer total_events matches actual event count
  local footer_total
  footer_total=$(tail -1 "$fixture" | jq -r '.total_events // empty' 2>/dev/null)
  local actual_events
  actual_events=$(grep -c '"event"' "$fixture" 2>/dev/null || echo 0)
  if [[ -n "$footer_total" && "$footer_total" != "$actual_events" ]]; then
    echo "  FAIL: footer total_events ($footer_total) doesn't match actual events ($actual_events)"
    ((FAIL++)) || true; ((TOTAL++)) || true
  else
    echo "  PASS: footer total_events matches actual events ($actual_events)"
    ((PASS++)) || true; ((TOTAL++)) || true
  fi
  
  echo
}

# === Delegation-Specific Checks ===
check_delegation_fixture() {
  local fixture="$1"
  
  echo "=== Delegation-Specific Checks ==="
  
  if [[ ! -f "$fixture" ]]; then
    echo "  FAIL: Fixture not found: $fixture"
    ((FAIL++)) || true; ((TOTAL++)) || true; return
  fi
  
  python3 << 'PYEOF'
import json, sys

try:
    with open("assets/fixtures/delegation-demo.jsonl") as f:
        lines = f.readlines()
    
    data = [json.loads(line) for line in lines]
    events = [d for d in data if 'event' in d]
    
    # Check 1: delegation_spawned event present with target_agent
    spawned = [e for e in events if e.get('event') == 'delegation_spawned']
    if not spawned:
        print("  FAIL: delegation_spawned event missing")
        sys.exit(1)
    
    if not spawned[0].get('data', {}).get('target_agent'):
        print("  FAIL: delegation_spawned missing target_agent")
        sys.exit(1)
    
    print(f"  PASS: delegation_spawned event present with target_agent='{spawned[0]['data']['target_agent']}'")
    
    # Check 2: delegation_completed with rich summary (> 50 chars)
    completed = [e for e in events if e.get('event') == 'delegation_completed']
    if not completed:
        print("  FAIL: delegation_completed event missing")
        sys.exit(1)
    
    summary = completed[0].get('data', {}).get('result_summary', '')
    if len(summary) < 50:
        print(f"  FAIL: delegation result_summary too short ({len(summary)} chars, need > 50)")
        sys.exit(1)
    
    print(f"  PASS: delegation_completed with result_summary ({len(summary)} chars)")
    
    # Check 3: tool_call has non-empty tool_name
    tool_calls = [e for e in events if e.get('event') == 'tool_call']
    if tool_calls:
        tool_name = tool_calls[0].get('data', {}).get('tool_name', '')
        if not tool_name:
            print("  FAIL: tool_call event has empty tool_name")
            sys.exit(1)
        print(f"  PASS: tool_call has tool_name='{tool_name}'")
    
    print("  PASS: All delegation-specific checks passed")
    
except Exception as e:
    print(f"  FAIL: {e}")
    sys.exit(1)
PYEOF
  
  if (( $? == 0 )); then
    ((PASS++)) || true; ((TOTAL++)) || true
  else
    ((FAIL++)) || true; ((TOTAL++)) || true
  fi
  
  echo
}

validate_fixture "demo"
validate_fixture "acp-demo"
validate_fixture "delegation-demo"

# Run strict quality checks on all fixtures
check_strict_recording_quality "assets/fixtures/demo.jsonl"
check_strict_recording_quality "assets/fixtures/acp-demo.jsonl"
check_strict_recording_quality "assets/fixtures/delegation-demo.jsonl"

# Run delegation-specific checks
check_delegation_fixture "assets/fixtures/delegation-demo.jsonl"

# === Content Validation Checks ===

# Check: Duplicate paragraph detection
check_duplicate_paragraphs() {
  local fixture="$1"
  local name=$(basename "$fixture" .jsonl)

  echo "=== Duplicate Paragraph Check: $name ==="

  if [[ ! -f "$fixture" ]]; then
    echo "  FAIL: Fixture not found: $fixture"
    ((FAIL++)) || true; ((TOTAL++)) || true; return
  fi

  python3 -c "
import json, sys

fixture = '$fixture'
with open(fixture) as f:
    lines = f.readlines()

data = [json.loads(line) for line in lines]
events = [d for d in data if d.get('event') == 'message_complete']

found_dup = False
for evt in events:
    full_response = evt.get('data', {}).get('full_response', '')
    if not full_response:
        continue
    paragraphs = [p.strip() for p in full_response.split('\n\n') if p.strip()]
    seen = set()
    for p in paragraphs:
        if p in seen:
            print(f'  FAIL: Duplicate paragraph detected: {p[:80]}...')
            found_dup = True
            break
        seen.add(p)
    if found_dup:
        break

if found_dup:
    sys.exit(1)
else:
    print('  PASS: No duplicate paragraphs')
    sys.exit(0)
"
  if (( $? == 0 )); then
    ((PASS++)) || true; ((TOTAL++)) || true
  else
    ((FAIL++)) || true; ((TOTAL++)) || true
  fi

  echo
}

# Check: Text delta consistency
check_text_delta_consistency() {
  local fixture="$1"
  local name=$(basename "$fixture" .jsonl)

  echo "=== Text Delta Consistency: $name ==="

  if [[ ! -f "$fixture" ]]; then
    echo "  FAIL: Fixture not found: $fixture"
    ((FAIL++)) || true; ((TOTAL++)) || true; return
  fi

  python3 -c "
import json, sys

fixture = '$fixture'
with open(fixture) as f:
    lines = f.readlines()

data = [json.loads(line) for line in lines]
events = [d for d in data if 'event' in d]

# Walk events: accumulate text_delta content between user_message and message_complete
# For delegation fixtures, tool_call/tool_result events split the response into phases.
# full_response only contains the final post-tool text, so we reset assembly on tool events.
assembling = False
assembled = ''
all_ok = True

for evt in events:
    etype = evt.get('event', '')
    if etype == 'user_message':
        assembling = True
        assembled = ''
    elif etype in ('tool_call', 'delegation_spawned') and assembling:
        # Reset: text before tool calls is not in full_response
        assembled = ''
    elif etype == 'text_delta' and assembling:
        # Skip thinking events
        if evt.get('data', {}).get('event_type') == 'thinking':
            continue
        assembled += evt.get('data', {}).get('content', '')
    elif etype == 'message_complete' and assembling:
        assembling = False
        full_response = evt.get('data', {}).get('full_response', '')
        if assembled and full_response and assembled != full_response:
            print(f'  FAIL: Assembled text_delta ({len(assembled)} chars) != full_response ({len(full_response)} chars)')
            # Show first divergence point
            for i, (a, b) in enumerate(zip(assembled, full_response)):
                if a != b:
                    print(f'         First diff at char {i}: delta={repr(a)} vs response={repr(b)}')
                    break
            all_ok = False
            break

if all_ok:
    print('  PASS: Text deltas match full_response')
    sys.exit(0)
else:
    sys.exit(1)
"
  if (( $? == 0 )); then
    ((PASS++)) || true; ((TOTAL++)) || true
  else
    ((FAIL++)) || true; ((TOTAL++)) || true
  fi

  echo
}

# Check: Personal config leakage
check_config_leakage() {
  local fixture="$1"
  local name=$(basename "$fixture" .jsonl)

  echo "=== Config Leakage Check: $name ==="

  if [[ ! -f "$fixture" ]]; then
    echo "  FAIL: Fixture not found: $fixture"
    ((FAIL++)) || true; ((TOTAL++)) || true; return
  fi

  local leaked=false

  # Check for home directory paths
  if grep -qE '"/home/[a-zA-Z]|"/Users/[a-zA-Z]' "$fixture" 2>/dev/null; then
    echo "  FAIL: Home directory path leaked in fixture"
    leaked=true
  fi

  # Check for API key patterns
  if grep -qE 'sk-[a-zA-Z0-9]{20,}|ANTHROPIC_API_KEY=' "$fixture" 2>/dev/null; then
    echo "  FAIL: API key pattern detected in fixture"
    leaked=true
  fi

  # Check for personal config references
  if grep -qE '~/.claude|~/.config/crucible' "$fixture" 2>/dev/null; then
    echo "  FAIL: Personal config path reference in fixture"
    leaked=true
  fi

  if [[ "$leaked" == "true" ]]; then
    ((FAIL++)) || true; ((TOTAL++)) || true
  else
    echo "  PASS: No personal config leakage detected"
    ((PASS++)) || true; ((TOTAL++)) || true
  fi

  echo
}

# Check: Event ordering sanity
check_event_ordering() {
  local fixture="$1"
  local name=$(basename "$fixture" .jsonl)

  echo "=== Event Ordering Check: $name ==="

  if [[ ! -f "$fixture" ]]; then
    echo "  FAIL: Fixture not found: $fixture"
    ((FAIL++)) || true; ((TOTAL++)) || true; return
  fi

  python3 -c "
import json, sys

fixture = '$fixture'
with open(fixture) as f:
    lines = f.readlines()

data = [json.loads(line) for line in lines]
events = [d for d in data if 'event' in d]

if not events:
    print('  FAIL: No events found')
    sys.exit(1)

errors = []

# Check 1: First event after header must be user_message
# Header lines lack 'event' key, so events[0] is the first real event
first_event = events[0].get('event', '')
if first_event != 'user_message':
    errors.append(f'First event is \"{first_event}\", expected \"user_message\"')

# Check 2: Last event must be message_complete or post_llm_call
# Footer lines lack 'event' key, so events[-1] is the last real event
last_event = events[-1].get('event', '')
if last_event not in ('message_complete', 'post_llm_call'):
    errors.append(f'Last event is \"{last_event}\", expected \"message_complete\" or \"post_llm_call\"')

# Check 3: No text_delta after message_complete for the same message_id
completed_ids = set()
for evt in events:
    etype = evt.get('event', '')
    mid = evt.get('data', {}).get('message_id', evt.get('message_id', ''))
    if etype == 'message_complete':
        completed_ids.add(mid)
    elif etype == 'text_delta' and mid in completed_ids:
        errors.append(f'text_delta after message_complete for message_id={mid}')
        break

if errors:
    for e in errors:
        print(f'  FAIL: {e}')
    sys.exit(1)
else:
    print('  PASS: Event ordering is valid')
    sys.exit(0)
"
  if (( $? == 0 )); then
    ((PASS++)) || true; ((TOTAL++)) || true
  else
    ((FAIL++)) || true; ((TOTAL++)) || true
  fi

  echo
}

# Run content validation checks on all fixtures
for fx in assets/fixtures/demo.jsonl assets/fixtures/acp-demo.jsonl assets/fixtures/delegation-demo.jsonl; do
  check_duplicate_paragraphs "$fx"
  check_text_delta_consistency "$fx"
  check_config_leakage "$fx"
  check_event_ordering "$fx"
done

echo "=== Summary ==="
echo "PASS: $PASS / WARN: $WARN / FAIL: $FAIL / TOTAL: $TOTAL"

if (( FAIL > 0 )); then
  echo "VERDICT: FAIL"
  exit 1
else
  echo "VERDICT: PASS"
  exit 0
fi
