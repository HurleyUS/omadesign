# Worktree, target and artifact cleanup

`scripts/oma-cleanup.sh` frees disk space from finished work. It **only previews by default**. Nothing is deleted unless you pass `--apply`.

```sh
scripts/oma-cleanup.sh                 # preview
scripts/oma-cleanup.sh --keep '*qa-*'  # preview, also protecting matching paths/branches
scripts/oma-cleanup.sh --apply         # delete what the preview lists
```

## What it removes

- Worktrees and local branches already merged into `origin/master`, including squash and rebase merges (detected with `git cherry` and a squash-commit probe).
- `target/` dirs in worktrees with nothing newer than 14 days.
- `dist/` builds beyond the newest 3 versions. Prerelease suffixes `nightly|rc|beta|alpha` are part of the version.
- `.artifacts/` files older than 14 days. The output is summarized per directory.

## What it never touches

- `master`, `main`, `wip/*` and the current checkout.
- Locked worktrees and worktrees with uncommitted changes.
- Worktrees holding ignored local files other than build output (`target/`, `dist/`, `node_modules/`, `.artifacts/`). Examples are `.env*`, `.vercel/`, `.qa-runs/` and browser profiles. Removing the worktree would lose these.
- Files inside a registered worktree (some live under `.artifacts/`).
- Remote branches.
- Anything matched by `--keep GLOB` or by a line in the untracked `.oma-cleanup.keep` file at the repo root.

## Automation

- `scripts/release.sh` prints a preview at the end of every release.
- `scripts/install-cleanup-timer.sh` installs `scripts/systemd/oma-cleanup.{service,timer}` as a weekly systemd user timer that runs `--apply`. Remove it with `--uninstall`.
  The timer is **not installed** on m1pro16 yet. It deletes data, so install it only after you have reviewed a preview of the real worktree set and added any keepers to `.oma-cleanup.keep`.

## Verification

The script was tested end to end, including `--apply`, on a throwaway repo. Against the real m1pro16 repo it was run in preview mode only (see the #486 PR).
