# Layer mask filters

A layer's mask menu now has **Filter mask ⏵** with Gaussian blur 2 px, Gaussian blur 8 px, Threshold 50% and Increase contrast (Levels 25–75%). Each runs the shared scalar channel filter engine on the mask's coverage, so mask, saved alpha and Quick Mask use one implementation. A pixel selection matching the mask grid limits the edit, and a mismatched one is refused rather than resampled. Native float masks keep float samples, and 8-bit masks round once on write.

Each filter is one ordinary `SetLayerMask` Undo step and saves with the existing mask format. Missing masks, locked, hidden or busy layers and no-op results change nothing. The full parameter dialog remains the Channels route for saved alpha and Quick Mask.

At `a5db9d6647b308a3b0ee5b8fb01068b2b58cf915` that route passed 10 tests. Receipt `f1ef4b2ceb5a8082c664381921f233fdf05f7f5f1e1588d356f6d3a106bbf428`.

Color/spot targets, calculations and interchange remain open.
