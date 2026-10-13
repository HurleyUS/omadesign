# Consolidation receipt — October 13, 2026

This receipt records the cleanup before [the October 13 release](../release-0.6.3-nightly.20261013.0001/README.md). PR #207 contains Fred Nix’s #202, #203 and #204 commit histories and the legacy ID-allocation correction. #205 is replaced by current canonical branding and native media. Standalone PRs #202–#205 and earlier image PRs #124/#125 are closed as superseded. #207 was open for review at this checkpoint; the release qualification records its final merge and publication.

All 83 earlier origin branch names are retired: this run deleted 81; two were already absent when the deletion batch began. At this checkpoint, only master, gh-pages and fix/canvas-brush-tablet remained on origin. 81 unattached local branches were also retired. Existing checkouts and uncommitted owner work remain intact.

Recovery bundle: `/home/michael/Projects/omadesign/artifacts/branch-archive/2026-10-13-before-pr-207.bundle`. Git verified that it contains complete history. The adjacent JSON files record branch names and exact heads. Restore a specific branch with `git fetch /home/michael/Projects/omadesign/artifacts/branch-archive/2026-10-13-before-pr-207.bundle refs/heads/BRANCH:refs/heads/BRANCH`. Contributor fork branches remain under their author’s control.

The current native candidate and media receipts are in `.worktrees/canvas-brush-fixes/artifacts/`. This initial consolidation did not publish a release or replace the running application. The later release receipt records publication separately.
