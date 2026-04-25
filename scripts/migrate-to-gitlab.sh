#!/usr/bin/env bash
# migrate-to-gitlab.sh — one-shot migration of the ferrum repo from GitHub to GitLab.
#
# Run from the repo root:
#   GITLAB_TOKEN=glpat-xxx ./scripts/migrate-to-gitlab.sh
#
# What it does (in order):
#   1. Sanity-checks: GITLAB_TOKEN env var present, working tree clean,
#      we're inside the ferrum repo, GitLab project URL is reachable AND
#      empty (won't clobber existing history).
#   2. Adds the GitLab remote as `gitlab` (token embedded in URL only for
#      the push — re-set without the token afterward).
#   3. Pushes every branch that exists on `origin` (GitHub) as a real
#      branch on GitLab, plus every annotated tag.
#   4. Renames remotes: `origin` → `github` (kept as a read-only backup
#      so you can still `git fetch github` for a while), `gitlab` → `origin`.
#      The token-embedded URL is replaced with the clean URL after the
#      remote becomes the new origin so it doesn't sit in `.git/config`.
#   5. Re-points local branch tracking from origin/* (now github/*) to the
#      new origin/* (GitLab).
#
# Idempotency: re-running on an already-migrated repo will detect the
# non-empty GitLab project and abort with a clear message — fix or
# re-create the GitLab project before retrying.

set -euo pipefail

GITLAB_HOST="gitlab.com"
GITLAB_NAMESPACE="hanzasmp-group"
GITLAB_PROJECT="ferrum"
GITLAB_REPO_URL="https://${GITLAB_HOST}/${GITLAB_NAMESPACE}/${GITLAB_PROJECT}.git"
GITLAB_API_URL="https://${GITLAB_HOST}/api/v4/projects/${GITLAB_NAMESPACE}%2F${GITLAB_PROJECT}"

# Branches to skip when mirroring — local-only Claude worktree branches and
# any other ad-hoc work that shouldn't end up on the canonical remote.
SKIP_BRANCH_PATTERN='^(HEAD|claude/)'

red()    { printf '\033[0;31m%s\033[0m\n' "$*" >&2; }
green()  { printf '\033[0;32m%s\033[0m\n' "$*"; }
yellow() { printf '\033[0;33m%s\033[0m\n' "$*"; }
bold()   { printf '\033[1m%s\033[0m\n'   "$*"; }

step() { echo; bold "── $* ──"; }

# ── 1. Pre-flight checks ────────────────────────────────────────────────────
step "Pre-flight"

if [[ -z "${GITLAB_TOKEN:-}" ]]; then
  red "GITLAB_TOKEN env var is required (Personal Access Token with api+write_repository scope)."
  red "Run with:  GITLAB_TOKEN=glpat-xxx $0"
  exit 1
fi

if ! command -v git  >/dev/null; then red "git not on PATH";  exit 1; fi
if ! command -v curl >/dev/null; then red "curl not on PATH"; exit 1; fi

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || true)"
if [[ -z "$REPO_ROOT" ]]; then
  red "Not inside a git repository."
  exit 1
fi
cd "$REPO_ROOT"

# Refuse to run on a dirty tree — migration touches `.git/config`, easier to
# reason about with a clean working dir.
if [[ -n "$(git status --porcelain)" ]]; then
  red "Working tree is not clean. Commit or stash first."
  git status --short >&2
  exit 1
fi

# Verify origin is the GitHub repo we expect — guard against running this in
# the wrong checkout by accident.
ORIGIN_URL="$(git remote get-url origin 2>/dev/null || true)"
if [[ "$ORIGIN_URL" != *"github.com"* ]]; then
  red "Expected current 'origin' to point at github.com, got: $ORIGIN_URL"
  red "Refusing to migrate from an unknown remote."
  exit 1
fi
green "origin = $ORIGIN_URL"

# ── 2. Verify GitLab project is reachable + empty ───────────────────────────
step "Verifying GitLab destination"

PROJECT_INFO="$(curl -sS -w '\n%{http_code}' \
    --header "PRIVATE-TOKEN: $GITLAB_TOKEN" \
    "$GITLAB_API_URL")"
HTTP_CODE="$(tail -n1 <<<"$PROJECT_INFO")"
PROJECT_BODY="$(sed '$d' <<<"$PROJECT_INFO")"

if [[ "$HTTP_CODE" != "200" ]]; then
  red "GitLab API returned $HTTP_CODE for $GITLAB_API_URL"
  red "Check: token scopes (need api+write_repository), project exists, namespace correct."
  echo "$PROJECT_BODY" >&2
  exit 1
fi
green "GitLab project exists and is accessible."

# Check it's empty — refuse to push into a project that already has commits.
EMPTY_FLAG="$(grep -o '"empty_repo":[a-z]*' <<<"$PROJECT_BODY" | head -1 | cut -d: -f2)"
if [[ "$EMPTY_FLAG" != "true" ]]; then
  red "GitLab project is NOT empty. Refusing to push and risk overwriting history."
  red "If this is a re-run after a prior migration, that's expected — abort here."
  red "If you need to start over, delete the GitLab project and re-create it empty."
  exit 1
fi
green "GitLab project is empty — safe to push."

# ── 3. Add the GitLab remote (with token in URL — temporary) ────────────────
step "Adding gitlab remote"

# URL-encode the token in case it ever contains special chars.
GITLAB_PUSH_URL="https://oauth2:${GITLAB_TOKEN}@${GITLAB_HOST}/${GITLAB_NAMESPACE}/${GITLAB_PROJECT}.git"

if git remote get-url gitlab >/dev/null 2>&1; then
  yellow "Remote 'gitlab' already exists — replacing URL."
  git remote set-url gitlab "$GITLAB_PUSH_URL"
else
  git remote add gitlab "$GITLAB_PUSH_URL"
fi
green "Remote 'gitlab' configured (token embedded for push)."

# ── 4. Push every origin branch + every tag to GitLab ───────────────────────
step "Pushing branches"

# Refresh origin so the branch list is current.
git fetch origin --prune --tags

# Use `while read` instead of mapfile — macOS ships bash 3.2 which lacks mapfile.
BRANCHES=()
while IFS= read -r br; do
  BRANCHES+=("$br")
done < <(
  git for-each-ref --format='%(refname:short)' refs/remotes/origin \
    | sed 's|^origin/||' \
    | grep -Ev "$SKIP_BRANCH_PATTERN" \
    | sort -u
)

if [[ ${#BRANCHES[@]} -eq 0 ]]; then
  red "No origin branches found to push — bailing."
  exit 1
fi

echo "Branches to push:"
printf '  - %s\n' "${BRANCHES[@]}"

for br in "${BRANCHES[@]}"; do
  echo
  bold "Pushing branch: $br"
  # Push the *remote-tracking* ref so we get the actual GitHub state, not
  # a possibly-stale local branch.
  git push gitlab "refs/remotes/origin/${br}:refs/heads/${br}"
done

step "Pushing tags"
git push gitlab --tags

# ── 5. Swap remotes: origin → github (backup), gitlab → origin ──────────────
step "Swapping remotes"

# Strip the token from the gitlab URL before it becomes 'origin'.
git remote set-url gitlab "$GITLAB_REPO_URL"

# Keep GitHub around as 'github' for now — read-only backup the user can
# fetch from while they verify the migration.
git remote rename origin github
git remote rename gitlab origin

green "Remotes after swap:"
git remote -v

# ── 6. Re-point local branch tracking ───────────────────────────────────────
step "Re-pointing local branch tracking"

# Refresh from the new origin so refs/remotes/origin/* exists for tracking.
git fetch origin --prune --tags

LOCAL_BRANCHES=()
while IFS= read -r br; do
  LOCAL_BRANCHES+=("$br")
done < <(git for-each-ref --format='%(refname:short)' refs/heads/)
for br in "${LOCAL_BRANCHES[@]}"; do
  if git show-ref --verify --quiet "refs/remotes/origin/${br}"; then
    git branch --set-upstream-to="origin/${br}" "$br" >/dev/null
    green "  $br → origin/$br"
  else
    yellow "  $br has no matching remote on GitLab — leaving tracking alone."
  fi
done

# ── 7. Done ─────────────────────────────────────────────────────────────────
step "Migration complete"

cat <<EOF

GitLab is now the primary remote (origin):
  $(git remote get-url origin)

GitHub is preserved as a read-only backup remote (github):
  $(git remote get-url github)

Next steps (manual):
  1. ROTATE the GitLab token you used — it was passed via env var but
     git's transient process tree may have logged it.
  2. Verify the dashboard at:
       https://${GITLAB_HOST}/${GITLAB_NAMESPACE}/${GITLAB_PROJECT}
     and confirm branches + tags are present.
  3. Once you're satisfied, archive the GitHub repo:
       gh repo archive Gabowatt/ferrum
     (or via the GitHub web UI: Settings → Archive this repository).
  4. Remove the github backup remote when you no longer need it:
       git remote remove github
EOF
