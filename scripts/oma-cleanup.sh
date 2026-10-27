#!/usr/bin/env bash
# Preview (default) or apply cleanup of merged worktrees/branches and stale
# build output in an omadesign checkout. See docs/cleanup.md.
#
#   scripts/oma-cleanup.sh            # preview only, deletes nothing
#   scripts/oma-cleanup.sh --apply    # delete what the preview lists
#
# Never touches: master/main, wip/*, the current checkout, worktrees with
# uncommitted or untracked changes, locked worktrees, remote branches, or any
# path/branch matched by --keep or the untracked .oma-cleanup.keep file, or
# anything under .artifacts/craft-gap-stack/implementation.
set -euo pipefail

apply=0
# Always kept (Michael, 2026-10-27): the craft-gap-stack implementation
# evidence tree, including its RAW camera data, whatever its age.
keep=(".artifacts/craft-gap-stack/implementation" ".artifacts/craft-gap-stack/implementation/*")
artifact_days=${OMA_CLEANUP_ARTIFACT_DAYS:-14}
target_days=${OMA_CLEANUP_TARGET_DAYS:-14}
dist_keep=${OMA_CLEANUP_DIST_KEEP:-3}
usage() { sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; }
while [ $# -gt 0 ]; do
  case "$1" in
    --apply) apply=1 ;;
    --keep) keep+=("$2"); shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

root=$(git rev-parse --path-format=absolute --git-common-dir)
root=${root%/.git}
cd "$root"
if [ -f .oma-cleanup.keep ]; then
  while IFS= read -r line; do
    case "$line" in ''|'#'*) ;; *) keep+=("$line") ;; esac
  done < .oma-cleanup.keep
fi
base=$(git rev-parse --verify -q origin/master || git rev-parse --verify -q master)
main_branch=$(git -C "$root" symbolic-ref -q --short HEAD || true)

kept() { # $1 = path or branch
  local item=$1 pattern
  for pattern in "${keep[@]+"${keep[@]}"}"; do
    # shellcheck disable=SC2053
    [[ $item == $pattern || $item == *"/$pattern" ]] && return 0
  done
  return 1
}
protected_branch() {
  case "$1" in master|main|wip/*|"$main_branch") return 0 ;; esac
  kept "$1"
}
merged() { # $1 = commit-ish; true when merged, rebased or squash-merged into base
  local tip=$1 mb squash
  git merge-base --is-ancestor "$tip" "$base" && return 0
  ! git cherry "$base" "$tip" | grep -q '^+' && return 0
  mb=$(git merge-base "$base" "$tip") || return 1
  squash=$(git commit-tree "$tip^{tree}" -p "$mb" -m squash-probe)
  git cherry "$base" "$squash" | grep -q '^-'
}
say() { if [ $apply = 1 ]; then echo "delete: $*"; else echo "would delete: $*"; fi; }
run() { if [ $apply = 1 ]; then "$@"; fi; }
size() { du -sh "$1" 2>/dev/null | cut -f1; }

declare -A worktree_branch=()
worktrees=()
while IFS= read -r line; do
  case "$line" in
    "worktree "*) path=${line#worktree }; worktrees+=("$path"); worktree_branch[$path]="" ;;
    "branch refs/heads/"*) worktree_branch[$path]=${line#branch refs/heads/} ;;
    locked*) worktree_branch[$path]="__locked__" ;;
  esac
done < <(git worktree list --porcelain)

inside_worktree() { # true when $1 is inside a registered linked worktree
  local p=$1 w
  for w in "${worktrees[@]}"; do
    [ "$w" = "$root" ] && continue
    [[ $p == "$w" || $p == "$w"/* || $w == "$p"/* ]] && return 0
  done
  return 1
}

echo "== worktrees merged into ${base:0:8}"
removed_branches=()
for w in "${worktrees[@]}"; do
  [ "$w" = "$root" ] && continue
  branch=${worktree_branch[$w]}
  if [ "$branch" = "__locked__" ] || kept "$w"; then echo "keep (protected): $w"; continue; fi
  if [ -n "$branch" ] && protected_branch "$branch"; then echo "keep (protected branch $branch): $w"; continue; fi
  if [ ! -d "$w" ]; then echo "stale registration: $w"; run git worktree prune; continue; fi
  if [ -n "$(git -C "$w" status --porcelain --ignore-submodules 2>/dev/null)" ]; then
    echo "keep (uncommitted changes): $w"; continue
  fi
  # Ignored files other than rebuildable output (e.g. .env, local profiles) block removal.
  extra=$(git -C "$w" status --porcelain --ignored=matching 2>/dev/null | sed -n 's/^!! //p' \
    | grep -Ev '^(target|dist|node_modules|site/node_modules|remotion/node_modules|\.artifacts)/' | head -3 || true)
  if [ -n "$extra" ]; then
    echo "keep (ignored local files: $(echo $extra)): $w"; continue
  fi
  if merged "$(git -C "$w" rev-parse HEAD)"; then
    say "worktree $w [${branch:-detached}] ($(size "$w"))"
    run git worktree remove "$w"
    [ -n "$branch" ] && removed_branches+=("$branch")
  fi
done

echo "== local branches merged into ${base:0:8}"
checked_out=" ${worktree_branch[*]} "
while IFS= read -r branch; do
  protected_branch "$branch" && continue
  if [[ $checked_out == *" $branch "* ]] && [[ " ${removed_branches[*]-} " != *" $branch "* ]]; then continue; fi
  if merged "$branch"; then
    say "branch $branch ($(git rev-parse --short "$branch"))"
    run git branch -D "$branch"
  fi
done < <(git for-each-ref --format='%(refname:short)' refs/heads)

echo "== target/ dirs untouched for ${target_days}+ days"
for w in "${worktrees[@]}"; do
  t="$w/target"
  [ -d "$t" ] || continue
  kept "$w" && continue
  if [ -z "$(find "$t" -maxdepth 3 -newermt "-${target_days} days" -print -quit 2>/dev/null)" ]; then
    say "$t ($(size "$t"))"; run rm -rf -- "$t"
  fi
done

echo "== dist/ builds beyond the newest ${dist_keep} versions"
if [ -d dist ]; then
  mapfile -t versions < <(find dist -maxdepth 1 -name 'omadesign-*' -printf '%f\n' \
    | sed -E 's/^omadesign-v?([0-9][^-]*(-(nightly|rc|beta|alpha)[^-]*)?)-.*/\1/' | sort -uV)
  count=${#versions[@]}
  for ((i = 0; i < count - dist_keep; i++)); do
    for f in dist/omadesign-"${versions[$i]}"-* dist/omadesign-v"${versions[$i]}"-*; do
      [ -e "$f" ] || continue
      say "$f ($(size "$f"))"; run rm -rf -- "$f"
    done
  done
fi

echo "== .artifacts/ files older than ${artifact_days} days"
if [ -d .artifacts ]; then
  old_files=()
  while IFS= read -r -d '' f; do
    inside_worktree "$root/$f" && continue
    kept "$f" && continue
    old_files+=("$f")
  done < <(find .artifacts -type f -mtime "+${artifact_days}" -print0)
  # Summarise per .artifacts/<dir>/<dir> instead of listing every file.
  if [ ${#old_files[@]} -gt 0 ]; then
    printf '%s\n' "${old_files[@]}" | awk -F/ -v verb="$([ $apply = 1 ] && echo delete || echo 'would delete')" '
      { key = $1 "/" $2 (NF > 3 ? "/" $3 : ""); n[key]++ }
      END { for (k in n) printf "%s: %d old file(s) under %s\n", verb, n[k], k }' | sort
    if [ $apply = 1 ]; then
      printf '%s\0' "${old_files[@]}" | xargs -0 rm -f --
      find .artifacts -mindepth 1 -type d -empty -delete 2>/dev/null || true
    fi
  fi
fi

[ $apply = 1 ] || echo "(preview only; re-run with --apply to delete)"
