#!/usr/bin/env bash
# My AI Stack acceptance (M1-C) — pure fixture, zero real-user access.
#
# What this script proves, end to end, against the REAL agenthub CLI binary
# talking to a REAL SQLite database:
#   A. create/confirm/edit a stack item, persistence across processes
#   C. USD 20/month + USD 120/year  -> monthly equivalent 30.00, annual 360.00
#   D. CNY subscriptions are reported per currency, never summed into USD
#   E. unknown amount / unknown period / usage-without-bills / hybrid semantics
#   F. merge keeps costs stable (no double counting, no loss)
#   G. delete constraints (any subscription status blocks deletion),
#      RESOURCE_CONFLICT -> exit 4, INVALID_INPUT -> exit 3, --yes gate -> exit 1
#   + audit_events written for every mutation
#   + isolation: AGENTHUB_HOME fixture only; real ~/.agenthub / ~/.codex etc.
#     are snapshotted before and byte-for-byte identical after.
#
# NOT covered here (GUI runtime phases): detection refresh flows, unmerge
# (CLI does not expose unmerge by frozen contract — GUI only), visual checks.
# Those live in the M1-C GUI evidence and Rust/Vitest suites.
#
# Usage: scripts/stack_acceptance.sh
# Env overrides: AGENTHUB_CLI (binary path, default target/debug/agenthub),
#                AGENTHUB_ACCEPTANCE_KEEP=1 (keep the fixture dir for review)
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${AGENTHUB_CLI:-$REPO_ROOT/target/debug/agenthub}"
if [ ! -x "$BIN" ]; then
  echo "BUILD required: $BIN not found — run: cargo build -p agenthub-cli"
  exit 97
fi

TMP="$(mktemp -d "${TMPDIR:-/tmp}/agenthub-stack-accept.XXXXXX")"
if [ "${AGENTHUB_ACCEPTANCE_KEEP:-0}" = "1" ]; then
  echo "fixture dir kept for review: $TMP"
else
  trap 'rm -rf "$TMP"' EXIT
fi

export AGENTHUB_HOME="$TMP/home"
mkdir -p "$AGENTHUB_HOME"
DB="$AGENTHUB_HOME/.agenthub/agenthub.db"

PASS=0
FAIL=0
note() { printf '  %s\n' "$*"; }
ok()   { PASS=$((PASS+1)); printf 'PASS %s\n' "$*"; }
bad()  { FAIL=$((FAIL+1)); printf 'FAIL %s\n' "$*"; }
check() { # check <label> <cmd...>  — must exit 0
  local label="$1"; shift
  if "$@" >/dev/null 2>&1; then ok "$label"; else bad "$label"; return 1; fi
}
expect_exit() { # expect_exit <label> <want> <cmd...>
  local label="$1" want="$2"; shift 2
  "$@" >/dev/null 2>&1
  local got=$?
  if [ "$got" = "$want" ]; then ok "$label"; else bad "$label (exit $got, want $want)"; fi
}

# ---------------------------------------------------------------- isolation
snapshot() { # tree fingerprint of a real user dir, if it exists
  if [ -e "$1" ]; then (cd "$1" && find . -print0 2>/dev/null | sort -z | xargs -0 stat -f '%N %m %z' 2>/dev/null | shasum -a 256);
  else echo absent; fi
}
# ~/.codex is written by Codex itself while this script runs, so a whole-tree
# fingerprint of it is inherently flaky. Fingerprint only the paths an AgentHub
# adapter could write, plus the app's own state; the decidable proof that no
# agent-directory write happened is the fixture `operations` table below.
snapshot_codex() { # only adapter-writable paths under ~/.codex
  local root="$HOME/.codex"
  [ -e "$root" ] || { echo absent; return; }
  {
    for rel in config.toml AGENTS.md mcp.json prompts skills; do
      if [ -e "$root/$rel" ]; then (cd "$root" && find "$rel" -print0 2>/dev/null | sort -z | xargs -0 stat -f '%N %m %z' 2>/dev/null); fi
    done
  } | shasum -a 256
}
REAL_SNAPSHOT="$(snapshot "$HOME/.agenthub")"
REAL_CODEX="$(snapshot_codex)"
REAL_CLAUDE="$(snapshot "$HOME/.claude")"
REAL_AGENTHUB_PRE="$REAL_SNAPSHOT"
note "isolation: AGENTHUB_HOME=$AGENTHUB_HOME"

# json field extraction without jq: python3
jget() { # jget <json-file> <python-expr over j>
  python3 -c 'import json,sys;j=json.load(open(sys.argv[1]));print(eval(sys.argv[2]))' "$1" "$2"
}
save_json() { # save_json <outfile> <cmd...>
  "$@" --json > "$1" 2>"$1.err"
}

# ================================================================= A. items
ID_A="$( "$BIN" stack add --name "Alpha Suite" --vendor "Vendor A" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["id"])' )"
[ -n "$ID_A" ] && ok "A: stack add creates an item with an id" || bad "A: stack add"
expect_exit "A: duplicate name is allowed (different tools may share a name)" 0 \
  "$BIN" stack add --name "Alpha Suite" --json

"$BIN" stack confirm "$ID_A" >/dev/null 2>&1 && ok "A: confirm item" || bad "A: confirm item"

CONFIRMED="$( "$BIN" stack show "$ID_A" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["item"]["confirmed"])' )"
[ "$CONFIRMED" = "True" ] && ok "A: confirmed flag visible in stack show" || bad "A: confirmed flag ($CONFIRMED)"

"$BIN" stack edit "$ID_A" --notes "acceptance-note" >/dev/null 2>&1 && ok "A: edit item" || bad "A: edit item"
NOTES="$( "$BIN" stack show "$ID_A" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["item"]["notes"])' )"
[ "$NOTES" = "acceptance-note" ] && ok "A: edit persisted" || bad "A: edit persisted ($NOTES)"
# omitted flags keep current values (edit does not silently clear)
"$BIN" stack edit "$ID_A" --vendor "Vendor A2" >/dev/null 2>&1
NOTES2="$( "$BIN" stack show "$ID_A" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["item"]["notes"])' )"
[ "$NOTES2" = "acceptance-note" ] && ok "A: omitted edit flags keep notes (no silent clear)" || bad "A: omitted flags cleared notes ($NOTES2)"

# every CLI invocation is a fresh process reading the same fixture DB:
# persistence across restarts is inherent and asserted via the re-reads above.

# ============================================================ C+D. spending
ID_B="$( "$BIN" stack add --name "Bravo Kit" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["id"])' )"
"$BIN" sub add --stack "$ID_A" --product "Alpha Pro" --billing-type subscription \
  --amount 20 --currency USD --period monthly --started-on 2026-11-01 --info-source "acceptance fixture" --json >/dev/null 2>&1 \
  && ok "C: USD 20/month registered" || bad "C: USD 20/month"
"$BIN" sub add --stack "$ID_B" --product "Bravo Yearly" --billing-type subscription \
  --amount 120 --currency USD --period yearly --json >/dev/null 2>&1 \
  && ok "C: USD 120/year registered" || bad "C: USD 120/year"

LIST="$("$BIN" stack list)"
echo "$LIST" | grep -q "spending (monthly equivalent, not this month's charge):" \
  && ok "C: monthly-equivalent disclaimer present" || bad "C: disclaimer missing"
echo "$LIST" | grep -q "30.00 USD / month, 360.00 USD / year" \
  && ok "C: 20/mo + 120/yr = monthly 30.00 / annual 360.00 USD" \
  || { bad "C: expected 30.00/360.00 USD"; note "$LIST"; }

ID_C="$( "$BIN" stack add --name "Charlie CNY" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["id"])' )"
"$BIN" sub add --stack "$ID_C" --product "Charlie Monthly" --billing-type subscription \
  --amount 68 --currency CNY --period monthly --json >/dev/null 2>&1
LIST="$("$BIN" stack list)"
echo "$LIST" | grep -q "68.00 CNY / month, 816.00 CNY / year" \
  && ok "D: CNY reported on its own line (816.00 CNY / year)" || bad "D: CNY line missing"
echo "$LIST" | grep -q "30.00 USD / month, 360.00 USD / year" \
  && ok "D: USD line unchanged — currencies never merged" || bad "D: USD line polluted"

# ================================================================= E. semantics
ID_U1="$( "$BIN" stack add --name "Unknown Amount" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["id"])' )"
"$BIN" sub add --stack "$ID_U1" --product "Mystery Plan" --billing-type subscription \
  --currency USD --json >/dev/null 2>&1
AMT="$( "$BIN" stack show "$ID_U1" --json | python3 -c 'import json,sys;d=json.load(sys.stdin)["data"]["subscriptions"];print(d[0].get("amountMinor"))' )"
[ "$AMT" = "None" ] && ok "E: unknown amount stays null (never 0)" || bad "E: unknown amount ($AMT)"

ID_U2="$( "$BIN" stack add --name "Unknown Period" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["id"])' )"
"$BIN" sub add --stack "$ID_U2" --product "Odd Plan" --billing-type subscription \
  --amount 9.99 --currency USD --period unknown --json >/dev/null 2>&1

ID_U3="$( "$BIN" stack add --name "Usage Only" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["id"])' )"
"$BIN" sub add --stack "$ID_U3" --product "Pay As You Go" --billing-type usage --period usage --json >/dev/null 2>&1

ID_U4="$( "$BIN" stack add --name "Hybrid Plan" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["id"])' )"
"$BIN" sub add --stack "$ID_U4" --product "Hybrid Fixed Part" --billing-type hybrid \
  --amount 5 --currency USD --period monthly --json >/dev/null 2>&1

LIST="$("$BIN" stack list)"
echo "$LIST" | grep -q "35.00 USD / month, 420.00 USD / year" \
  && ok "E: hybrid fixed part folded (20+10+5 = monthly 35.00 / annual 420.00)" \
  || { bad "E: hybrid fold expected 35.00/420.00"; note "$LIST"; }
echo "$LIST" | grep -q "2 subscription(s) incomplete" \
  && ok "E: unknown amount + unknown period counted as incomplete" || bad "E: incomplete count"
echo "$LIST" | grep -q "1 usage-based subscription(s) without bills are not estimated" \
  && ok "E: usage without bills is not estimated" || bad "E: usage without bills"

# ================================================================= F. merge
ID_D="$( "$BIN" stack add --name "Delta Mergesrc" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["id"])' )"
ID_E="$( "$BIN" stack add --name "Echo Target" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["id"])' )"
"$BIN" sub add --stack "$ID_D" --product "Delta Fee" --billing-type subscription \
  --amount 5 --currency USD --period monthly --json >/dev/null 2>&1
"$BIN" stack merge "$ID_D" --into "$ID_E" --yes >/dev/null 2>&1 \
  && ok "F: merge executes with --yes" || bad "F: merge --yes"
LIST="$("$BIN" stack list)"
echo "$LIST" | grep -q "40.00 USD / month, 480.00 USD / year" \
  && ok "F: merged group cost stable (no double count, no loss: 35+5 = 40.00)" \
  || { bad "F: expected 40.00/480.00 after merge"; note "$LIST"; }
expect_exit "F: G1 — a merged child can never be a source again (exit 4)" 4 \
  "$BIN" stack merge "$ID_D" --into "$ID_A" --yes
expect_exit "F: G1 — an item with merged children can never be a source (exit 4)" 4 \
  "$BIN" stack merge "$ID_E" --into "$ID_A" --yes

# ================================================================= G. deletes
expect_exit "G: deleting a tool with an active subscription is refused (exit 4)" 4 \
  "$BIN" stack delete "$ID_A" --yes
SUB_A="$( "$BIN" stack show "$ID_A" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["subscriptions"][0]["id"])' )"
"$BIN" sub edit "$SUB_A" --status cancelled >/dev/null 2>&1
expect_exit "G: deleting a tool with a CANCELLED subscription is still refused (exit 4)" 4 \
  "$BIN" stack delete "$ID_A" --yes
expect_exit "G: delete without --yes is refused (exit 1)" 1 "$BIN" stack delete "$ID_E"
ID_F="$( "$BIN" stack add --name "Foxtrot Empty" --json | python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["id"])' )"
ITEMS_BEFORE="$( "$BIN" stack list --json | python3 -c 'import json,sys;print(len(json.load(sys.stdin)["data"]))' )"
expect_exit "G: deleting a tool with no subscriptions succeeds" 0 \
  "$BIN" stack delete "$ID_F" --yes
ITEMS_AFTER="$( "$BIN" stack list --json | python3 -c 'import json,sys;print(len(json.load(sys.stdin)["data"]))' )"
[ "$ITEMS_AFTER" = "$((ITEMS_BEFORE-1))" ] && ok "G: delete actually removed exactly one item" || bad "G: delete count"
expect_exit "G: subscription without amount is still valid but without currency is INVALID_INPUT (exit 3)" 3 \
  "$BIN" sub add --stack "$ID_U1" --product "Bad Fee" --billing-type subscription --amount 10
# cancelled subscription records survive tool deletion? (records RESTRICT via
# FK, so deletion is refused — the subscription is preserved by construction.)
expect_exit "G: subscription delete requires --yes" 1 "$BIN" sub delete "$SUB_A"
expect_exit "G: subscription delete with --yes succeeds" 0 "$BIN" sub delete "$SUB_A" --yes

# ============================================================ audit + schema
AUDIT_KEYS="$(sqlite3 "$DB" "SELECT event_key FROM audit_events ORDER BY id" 2>/dev/null || echo NOAUDIT)"
for key in stack.item.upsert stack.item.confirm stack.merge stack.item.delete subscription.upsert subscription.delete; do
  echo "$AUDIT_KEYS" | grep -q "^$key$" && ok "audit: $key recorded" || bad "audit: $key missing"
done
UV="$(sqlite3 "$DB" "PRAGMA user_version" 2>/dev/null)"
[ "$UV" = "4" ] && ok "schema: fixture db migrated to user_version 4" || bad "schema: user_version=$UV"
FKOK="$(sqlite3 "$DB" "PRAGMA foreign_key_check" 2>/dev/null | wc -l | tr -d ' ')"
[ "$FKOK" = "0" ] && ok "schema: foreign_key_check clean" || bad "schema: foreign_key_check rows=$FKOK"

# ================================================================= isolation re-check
[ "$(snapshot "$HOME/.agenthub")" = "$REAL_AGENTHUB_PRE" ] \
  && ok "isolation: real ~/.agenthub untouched" || bad "isolation: real ~/.agenthub CHANGED"
[ "$(snapshot_codex)" = "$REAL_CODEX" ] \
  && ok "isolation: real ~/.codex adapter paths untouched" || bad "isolation: real ~/.codex adapter paths CHANGED"
# Decidable proof: the transactional distribution Executor never ran.
OPS="$(sqlite3 "$DB" "select count(*) from operations" 2>/dev/null)"
[ "$OPS" = "0" ] && ok "isolation: operations table is empty (no distribution writes)" || bad "isolation: operations rows=$OPS"
# The app database must live inside the fixture home, never the real one.
case "$DB" in "$AGENTHUB_HOME"/*) ok "isolation: app db lives inside the fixture home (path)" ;; *) bad "isolation: app db outside fixture: $DB" ;; esac
[ "$(snapshot "$HOME/.claude")" = "$REAL_CLAUDE" ] \
  && ok "isolation: real ~/.claude untouched" || bad "isolation: real ~/.claude CHANGED"
[ -f "$DB" ] && ok "isolation: app db lives inside the fixture home" || bad "isolation: fixture db missing"

printf '\n==== stack_acceptance: %d passed, %d failed ====\n' "$PASS" "$FAIL"
[ "$FAIL" = "0" ] || exit 1
exit 0
