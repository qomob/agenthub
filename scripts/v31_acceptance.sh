#!/bin/bash
# v3.1 Phase 1 acceptance: isolated-home DB migration, approval flow, CAS,
# journal and recovery. Never writes a real agent directory.
set -u

REPO="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$REPO/target/debug/agenthub"
LOG="/tmp/agenthub-v31-acceptance"
FAIL=0
pass() { echo "PASS  $*"; }
fail() { echo "FAIL  $*"; FAIL=1; }
json() { python3 -c "import json,sys; d=json.load(sys.stdin); print(eval('d'+sys.argv[1]))" "$1" 2>/dev/null; }

rm -rf "$LOG"
mkdir -p "$LOG/home/.codex" "$LOG/home/.workbuddy" "$LOG/src/alpha-skill" "$LOG/legacy-home/.agenthub"

cat > "$LOG/src/alpha-skill/SKILL.md" <<'SKILL'
---
name: alpha-v31-acceptance
description: Isolated acceptance skill for the v3.1 safety slice.
version: 1.0.0
---
# Alpha

Acceptance fixture.
SKILL

echo "== build CLI =="
( cd "$REPO" && ./scripts/cargo.sh build -p agenthub-cli ) > "$LOG/build.log" 2>&1
[ $? -eq 0 ] && pass "cargo build -p agenthub-cli" || { fail "cargo build"; tail -20 "$LOG/build.log"; exit 1; }

# ---------------------------------------------------------------------------
# 1. Real user DB copy: migrate v2 -> v3, backup, data preserved.
# ---------------------------------------------------------------------------
echo "== legacy database migration (copy of ~/.agenthub) =="
if [ -f "$HOME/.agenthub/agenthub.db" ]; then
  cp "$HOME/.agenthub/agenthub.db" "$LOG/legacy-home/.agenthub/agenthub.db"
  [ -f "$HOME/.agenthub/agenthub.db-wal" ] && cp "$HOME/.agenthub/agenthub.db-wal" "$LOG/legacy-home/.agenthub/agenthub.db-wal"
  [ -f "$HOME/.agenthub/agenthub.db-shm" ] && cp "$HOME/.agenthub/agenthub.db-shm" "$LOG/legacy-home/.agenthub/agenthub.db-shm"
  BEFORE=$(sqlite3 "$LOG/legacy-home/.agenthub/agenthub.db" 'select count(*) from capabilities;' 2>/dev/null)
  # Point the copied registry at the isolated library root (never the real one).
  sqlite3 "$LOG/legacy-home/.agenthub/agenthub.db" "UPDATE settings SET value='$LOG/legacy-home/.agenthub' WHERE key='library_root';" 2>/dev/null
  sqlite3 "$LOG/legacy-home/.agenthub/agenthub.db" "UPDATE settings SET value='[]' WHERE key='project_roots';" 2>/dev/null
  AGENTHUB_HOME="$LOG/legacy-home" "$BIN" doctor --json > "$LOG/legacy-doctor.json" 2> "$LOG/legacy-doctor.err"
  CODE=$?
  [ "$CODE" -eq 0 ] && pass "doctor on v2 DB copy (exit 0)" || fail "doctor on v2 DB copy (exit $CODE)"
  AFTER=$(sqlite3 "$LOG/legacy-home/.agenthub/agenthub.db" 'select count(*) from capabilities;' 2>/dev/null)
  [ "$BEFORE" = "$AFTER" ] && pass "capabilities preserved ($BEFORE -> $AFTER)" || fail "capabilities changed $BEFORE -> $AFTER"
  VER=$(sqlite3 "$LOG/legacy-home/.agenthub/agenthub.db" 'PRAGMA user_version;')
  [ "$VER" = "3" ] && pass "schema user_version=3" || fail "schema user_version=$VER"
  INTEG=$(sqlite3 "$LOG/legacy-home/.agenthub/agenthub.db" 'PRAGMA integrity_check;')
  [ "$INTEG" = "ok" ] && pass "integrity_check ok" || fail "integrity_check: $INTEG"
  ls "$LOG/legacy-home/.agenthub/backups"/pre-migration-v2-*/agenthub.db >/dev/null 2>&1 \
    && pass "pre-migration backup exists" || fail "pre-migration backup missing"
else
  echo "SKIP  no real ~/.agenthub database on this machine"
fi

# ---------------------------------------------------------------------------
# 2. Import + plan + approve + apply in an isolated home.
# ---------------------------------------------------------------------------
echo "== isolated approval flow =="
export AGENTHUB_HOME="$LOG/home"
export AGENTHUB_DATA_DIR="$LOG/home/.agenthub"

"$BIN" skills import --path "$LOG/src/alpha-skill" --json > "$LOG/import.json" 2> "$LOG/import.err"
CODE=$?
[ "$CODE" -eq 0 ] && pass "import fixture (exit 0)" || { fail "import (exit $CODE)"; cat "$LOG/import.err"; exit 1; }

"$BIN" skills install alpha-v31-acceptance --agents codex,workbuddy --dry-run --json > "$LOG/plan.json" 2> "$LOG/plan.err"
CODE=$?
[ "$CODE" -eq 0 ] && pass "plan install (exit 0)" || { fail "plan (exit $CODE)"; cat "$LOG/plan.err"; exit 1; }
OP=$(json "['plan']['operationId']" < "$LOG/plan.json")
HASH=$(json "['plan']['planHash']" < "$LOG/plan.json")
[ -n "$OP" ] && [ -n "$HASH" ] && pass "operation=$OP planHash=${HASH:0:12}…" || { fail "missing operation/plan hash"; cat "$LOG/plan.json"; exit 1; }

# Applying without approval must fail with exit 4 and not write.
"$BIN" operations apply "$OP" --token deadbeef --json > "$LOG/apply-no-approval.json" 2> "$LOG/apply-no-approval.err"
CODE=$?
[ "$CODE" -eq 4 ] && pass "apply without approval exits 4" || fail "apply without approval exit $CODE (expected 4)"
[ ! -e "$LOG/home/.codex/skills/alpha-v31-acceptance" ] && pass "no write without approval" || fail "write happened without approval"

"$BIN" operations approve "$OP" --plan-hash "$HASH" --json > "$LOG/approve.json" 2> "$LOG/approve.err"
CODE=$?
[ "$CODE" -eq 0 ] && pass "approve (exit 0)" || { fail "approve (exit $CODE)"; cat "$LOG/approve.err"; exit 1; }
TOKEN=$(json "['data']['approvalToken']" < "$LOG/approve.json")
[ -z "$TOKEN" ] && TOKEN=$(json "['approvalToken']" < "$LOG/approve.json")
[ -n "$TOKEN" ] && pass "approval token issued (${#TOKEN} chars)" || { fail "no approval token"; cat "$LOG/approve.json"; exit 1; }

"$BIN" operations apply "$OP" --token "$TOKEN" --idempotency-key acc-1 --json > "$LOG/apply.json" 2> "$LOG/apply.err"
CODE=$?
[ "$CODE" -eq 0 ] && pass "apply approved (exit 0)" || { fail "apply (exit $CODE)"; cat "$LOG/apply.err"; exit 1; }
for target in "$LOG/home/.codex/skills/alpha-v31-acceptance" "$LOG/home/.workbuddy/skills/alpha-v31-acceptance"; do
  [ -f "$target/SKILL.md" ] && pass "deployed $target" || fail "missing $target"
done

"$BIN" operations preview "$OP" --json > "$LOG/preview.json" 2> "$LOG/preview.err"
STEPS=$(json "['data']['steps'].__len__()" < "$LOG/preview.json")
[ "$STEPS" = "2" ] && pass "journal has 2 steps" || fail "journal steps=$STEPS"

# Idempotent replay returns the stored result.
"$BIN" operations apply "$OP" --token "$TOKEN" --idempotency-key acc-1 --json > "$LOG/replay.json" 2> "$LOG/replay.err"
CODE=$?
[ "$CODE" -eq 0 ] && pass "idempotent replay exits 0" || fail "idempotent replay exit $CODE"

# ---------------------------------------------------------------------------
# 3. CAS: target created after planning is never overwritten.
# ---------------------------------------------------------------------------
echo "== external modification before apply =="
rm -rf "$LOG/home/.workbuddy/skills/alpha-v31-acceptance"
"$BIN" skills install alpha-v31-acceptance --agents workbuddy --dry-run --json > "$LOG/plan2.json" 2>/dev/null
OP2=$(json "['plan']['operationId']" < "$LOG/plan2.json")
HASH2=$(json "['plan']['planHash']" < "$LOG/plan2.json")
"$BIN" operations approve "$OP2" --plan-hash "$HASH2" --json > "$LOG/approve2.json" 2>/dev/null
TOKEN2=$(json "['data']['approvalToken']" < "$LOG/approve2.json")
[ -z "$TOKEN2" ] && TOKEN2=$(json "['approvalToken']" < "$LOG/approve2.json")
mkdir -p "$LOG/home/.workbuddy/skills/alpha-v31-acceptance"
echo "foreign content" > "$LOG/home/.workbuddy/skills/alpha-v31-acceptance/SKILL.md"
"$BIN" operations apply "$OP2" --token "$TOKEN2" --json > "$LOG/cas.json" 2> "$LOG/cas.err"
CODE=$?
[ "$CODE" -eq 5 ] && pass "CAS mismatch exits 5" || fail "CAS mismatch exit $CODE (expected 5)"
CONTENT=$(cat "$LOG/home/.workbuddy/skills/alpha-v31-acceptance/SKILL.md")
[ "$CONTENT" = "foreign content" ] && pass "external content preserved" || fail "external content overwritten: $CONTENT"

# ---------------------------------------------------------------------------
# 4. Recovery refuses to overwrite external edits and reports manual recovery.
# ---------------------------------------------------------------------------
echo "== recovery =="
printf 'edited by another app\n' > "$LOG/home/.codex/skills/alpha-v31-acceptance/SKILL.md"
# Simulate a crash after partial apply (the CLI has no state-forcing command).
sqlite3 "$AGENTHUB_DATA_DIR/agenthub.db" "UPDATE operations SET status='needs_manual_recovery' WHERE id='$OP';" 2>/dev/null
"$BIN" operations recover "$OP" --json > "$LOG/recover.json" 2> "$LOG/recover.err"
CODE=$?
[ "$CODE" -eq 5 ] && pass "recovery with external edit exits 5" || fail "recovery exit $CODE (expected 5)"
RECOVERED=$(cat "$LOG/home/.codex/skills/alpha-v31-acceptance/SKILL.md")
[ "$RECOVERED" = "edited by another app" ] && pass "recovery preserved external edit" || fail "recovery overwrote external edit"
grep -q 'needs_manual_recovery' "$LOG/recover.json" && pass "recovery reports needs_manual_recovery" || fail "recovery JSON lacks needs_manual_recovery"

echo
if [ "$FAIL" -eq 0 ]; then
  echo "RESULT: PASS (logs in $LOG)"
  exit 0
else
  echo "RESULT: FAIL (logs in $LOG)"
  exit 1
fi
