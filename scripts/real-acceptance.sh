#!/bin/bash
# AgentHub real-machine acceptance (explicitly approved by the machine owner).
# Adds one new skill directory to Codex and ZCode, runs an update + rollback,
# then uninstalls the deployments. Snapshots and the central library remain.
set -u

REPO="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$REPO/target/debug/agenthub"
LOG="/tmp/agenthub-real-acceptance"
ORIGIN="${AGENTHUB_ACCEPTANCE_SKILL:-$HOME/Documents/Memory Music/.dsh/skills/memory-maker}"
SLUG="agenthub-accept-$(date +%H%M%S)"
SRC="/tmp/$SLUG-v1"
V2_SRC="/tmp/$SLUG-v2"
CODEX="$HOME/.codex/skills/$SLUG"
ZCODE="$HOME/.zcode/skills/$SLUG"
FAIL=0
CREATED=0

rm -rf "$LOG" "$SRC" "$V2_SRC"
mkdir -p "$LOG"
cp -R "$ORIGIN" "$SRC"
cp -R "$ORIGIN" "$V2_SRC"
python3 - "$SRC/SKILL.md" "$V2_SRC/SKILL.md" "$SLUG" <<'PY2'
import re, sys
for path, slug in ((sys.argv[1], sys.argv[3]), (sys.argv[2], sys.argv[3])):
    text = open(path, encoding="utf-8").read()
    text = re.sub(r"^name:.*$", f"name: {slug}", text, count=1, flags=re.M)
    open(path, "w", encoding="utf-8").write(text)
PY2
echo "acceptance slug: $SLUG"

pass() { echo "PASS  $*"; }
fail() { echo "FAIL  $*"; FAIL=1; }

cleanup_on_failure() {
  if [ "$FAIL" -ne 0 ] && [ "$CREATED" -eq 1 ]; then
    echo "cleanup: removing deployments created by this script"
    rm -rf "$CODEX" "$ZCODE"
  fi
}
trap cleanup_on_failure EXIT

json_value() { # file, python expression
  python3 - "$1" "$2" <<'PY'
import json, sys
doc = json.load(open(sys.argv[1]))
print(eval(sys.argv[2], {"json": json, "doc": doc}))
PY
}

echo "== preconditions =="
if [ -e "$CODEX" ] || [ -e "$ZCODE" ]; then
  echo "ABORT: a target directory already exists ($CODEX / $ZCODE)"
  exit 1
fi
[ -f "$ORIGIN/SKILL.md" ] || { echo "ABORT: origin skill missing"; exit 1; }
[ -f "$SRC/SKILL.md" ] || { echo "ABORT: prepared skill missing"; exit 1; }
[ -x "$BIN" ] || { echo "ABORT: CLI binary missing"; exit 1; }
echo "ok: prepared $SLUG from $ORIGIN"

echo
echo "== 1. import into the real central library =="
"$BIN" --json skills import --path "$SRC" --notes "AgentHub Phase 1 real acceptance" >"$LOG/import1.json" 2>"$LOG/import1.err"
IMPORT_CODE=$?
SKILL_ID="$(json_value "$LOG/import1.json" 'doc["skill"]["id"]' 2>/dev/null)"
V1="$(json_value "$LOG/import1.json" 'doc["version"]["contentHash"]' 2>/dev/null)"
if [ "$IMPORT_CODE" -eq 0 ] && [ -n "$SKILL_ID" ]; then
  pass "import memory-maker (skill=$SKILL_ID v1=${V1:0:12})"
else
  fail "import (exit $IMPORT_CODE)"; cat "$LOG/import1.err"; exit 1
fi

echo
echo "== 2. distribute to Codex and ZCode (copy strategy) =="
"$BIN" --json skills install "$SKILL_ID" --agents codex,zcode --yes >"$LOG/install1.json" 2>"$LOG/install1.err"
INSTALL_CODE=$?
if [ "$INSTALL_CODE" -eq 0 ]; then
  pass "plan applied (exit 0)"; CREATED=1
else
  fail "distribution (exit $INSTALL_CODE)"; cat "$LOG/install1.err"
fi
[ -f "$CODEX/SKILL.md" ] && pass "codex deployment exists" || fail "codex deployment missing"
[ -f "$ZCODE/SKILL.md" ] && pass "zcode deployment exists" || fail "zcode deployment missing"
cmp -s "$CODEX/SKILL.md" "$SRC/SKILL.md" && pass "codex bytes equal the library source" || fail "codex content differs"
cmp -s "$ZCODE/SKILL.md" "$SRC/SKILL.md" && pass "zcode bytes equal the library source" || fail "zcode content differs"

echo
echo "== 3. scan shows both deployments in sync =="
"$BIN" --json skills scan >"$LOG/scan1.json" 2>"$LOG/scan1.err"
SYNC_STATE="$(python3 - "$LOG/scan1.json" "$SLUG" <<'PY'
import json, sys
doc = json.load(open(sys.argv[1]))
out = {}
for agent in doc["agents"]:
    for skill in agent["skills"]:
        if skill["slug"] == sys.argv[2]:
            out[agent["agentKey"]] = skill["inSync"]
print(json.dumps(out, sort_keys=True))
PY
)"
echo "scan state: $SYNC_STATE"
echo "$SYNC_STATE" | grep -q '"codex": true' && pass "codex scan inSync" || fail "codex scan not in sync ($SYNC_STATE)"
echo "$SYNC_STATE" | grep -q '"zcode": true' && pass "zcode scan inSync" || fail "zcode scan not in sync ($SYNC_STATE)"

echo
echo "== 4. import a modified version, detect and apply the update =="
printf '\n\n<!-- AgentHub real acceptance v2 marker -->\n' >>"$V2_SRC/SKILL.md"
"$BIN" --json skills import --path "$V2_SRC" --notes "acceptance v2" >"$LOG/import2.json" 2>"$LOG/import2.err"
V2="$(json_value "$LOG/import2.json" 'doc["version"]["contentHash"]' 2>/dev/null)"
if [ -n "$V2" ] && [ "$V2" != "$V1" ]; then
  pass "new version imported (v2=${V2:0:12}), old version kept"
else
  fail "second import did not create a distinct version"; cat "$LOG/import2.err"
fi
"$BIN" --json skills sync --dry-run >"$LOG/sync-dry.json" 2>"$LOG/sync-dry.err"
SYNC_DRY_CODE=$?
if [ "$SYNC_DRY_CODE" -eq 2 ] || [ "$SYNC_DRY_CODE" -eq 0 ]; then
  UPDATES="$(json_value "$LOG/sync-dry.json" 'len(doc["check"]["updates"])' 2>/dev/null)"
  PLANS="$(json_value "$LOG/sync-dry.json" 'len(doc["plans"])' 2>/dev/null)"
  if [ "${UPDATES:-0}" -ge 1 ] && [ "${PLANS:-0}" -ge 1 ]; then
    pass "sync --dry-run detects the update (updates=$UPDATES plans=$PLANS, exit $SYNC_DRY_CODE) and does not write"
  else
    fail "sync --dry-run did not report the pending update (updates=${UPDATES:-?} plans=${PLANS:-?})"
  fi
else
  fail "sync --dry-run exit $SYNC_DRY_CODE"
fi
grep -q "v2 marker" "$CODEX/SKILL.md" && fail "dry-run modified the deployment" || pass "dry-run left Codex deployment untouched"

"$BIN" --json skills sync --yes >"$LOG/sync.json" 2>"$LOG/sync.err"
SYNC_CODE=$?
if [ "$SYNC_CODE" -eq 0 ]; then
  pass "sync applied (exit 0)"
else
  fail "sync (exit $SYNC_CODE)"; cat "$LOG/sync.err"
fi
grep -q "v2 marker" "$CODEX/SKILL.md" && pass "codex got v2 content" || fail "codex still on v1"
grep -q "v2 marker" "$ZCODE/SKILL.md" && pass "zcode got v2 content" || fail "zcode still on v1"

echo
echo "== 5. one-click rollback restores the previous version =="
"$BIN" --json skills rollback last >"$LOG/rollback.json" 2>"$LOG/rollback.err"
ROLLBACK_CODE=$?
if [ "$ROLLBACK_CODE" -eq 0 ]; then
  pass "rollback applied (exit 0)"
else
  fail "rollback (exit $ROLLBACK_CODE)"; cat "$LOG/rollback.err"
fi
grep -q "v2 marker" "$CODEX/SKILL.md" && fail "codex still has v2 after rollback" || pass "codex v2 marker removed"
cmp -s "$CODEX/SKILL.md" "$SRC/SKILL.md" && pass "codex restored to the exact v1 content" || fail "codex content is not v1"
cmp -s "$ZCODE/SKILL.md" "$SRC/SKILL.md" && pass "zcode restored to the exact v1 content" || fail "zcode content is not v1"

echo
echo "== 6. uninstall the deployments (library and snapshots stay) =="
"$BIN" --json skills list >"$LOG/list.json" 2>"$LOG/list.err"
INSTALL_IDS="$(python3 - "$LOG/list.json" "$SLUG" <<'PY'
import json, sys
doc = json.load(open(sys.argv[1]))
ids = []
for skill in doc:
    if skill["slug"] == sys.argv[2]:
        for inst in skill.get("installations", []):
            if inst["agentKey"] in ("codex", "zcode"):
                ids.append(inst["id"])
print(" ".join(ids))
PY
)"
echo "installation ids: $INSTALL_IDS"
if [ -n "$INSTALL_IDS" ]; then
  # shellcheck disable=SC2086
  "$BIN" --json skills uninstall $INSTALL_IDS --yes >"$LOG/uninstall.json" 2>"$LOG/uninstall.err"
  UNINSTALL_CODE=$?
  if [ "$UNINSTALL_CODE" -eq 0 ]; then
    pass "uninstall applied (exit 0)"; CREATED=0
  else
    fail "uninstall (exit $UNINSTALL_CODE)"; cat "$LOG/uninstall.err"
  fi
else
  fail "no installation ids found"
fi
[ -e "$CODEX" ] && fail "codex deployment still present" || pass "codex deployment removed"
[ -e "$ZCODE" ] && fail "zcode deployment still present" || pass "zcode deployment removed"
LIB_VERSIONS="$(find "$HOME/.agenthub/library/skills/$SLUG" -name SKILL.md 2>/dev/null | wc -l | tr -d ' ')"
if [ "$LIB_VERSIONS" -ge 2 ]; then
  pass "central library still holds $LIB_VERSIONS version(s) of $SLUG"
else
  fail "central library lost $SLUG (versions=$LIB_VERSIONS)"
fi

echo
echo "== 7. backup + doctor on the real profile =="
"$BIN" --json backup create >"$LOG/backup.json" 2>"$LOG/backup.err" && pass "backup created" || fail "backup failed"
"$BIN" --json doctor >"$LOG/doctor.json" 2>"$LOG/doctor.err" && pass "doctor ran" || fail "doctor failed"
python3 - "$LOG/doctor.json" <<'PY' && pass "doctor.ok is true" || fail "doctor.ok is false"
import json, sys
sys.exit(0 if json.load(open(sys.argv[1])).get("ok") else 1)
PY

echo
echo "== 8. install the built app into /Applications (approved) =="
rm -rf /Applications/AgentHub.app
ditto "$REPO/target/release/bundle/macos/AgentHub.app" /Applications/AgentHub.app
if [ -x /Applications/AgentHub.app/Contents/MacOS/agenthub ]; then
  pass "AgentHub.app installed at /Applications/AgentHub.app"
else
  fail "could not install AgentHub.app"
fi

echo
if [ "$FAIL" -eq 0 ]; then
  echo "REAL ACCEPTANCE: ALL CHECKS PASSED"
  echo "evidence: $LOG"
  exit 0
else
  echo "REAL ACCEPTANCE: FAILURES PRESENT"
  exit 1
fi
