# Shared dock sizing

The native Character-pane check exposed a preference mismatch: the keyboard Panels dialog resized only one stacked group, while the shared workspace derives the column width from the widest group. A smaller request could therefore leave the actual column unchanged.

Resize panel now updates every visible group on the selected dock edge, matching the existing pointer divider behavior. Other edges, hidden groups and separate floating windows remain independent. The existing captured-workspace validation, Apply and real focus request remain.

Source-only pending native dialog/actual canonical geometry and existing panel-action checks. No document/history, workspace model, dependency, schema or version change. Full Craft parity remains open.
