# Native conic gradients

The precise float compositor now samples conic gradient fills and strokes analytically from the same angle field the 8-bit route uses (silhouette bounds included), instead of refusing them. Degenerate (zero-length) conic axes refuse. Shape-distance gradients, patterns, effects and rich span paint keep their named refusals.

At `8be9f1c150b67489ad82159f83a804224e8b7ee7` that route passed 64 tests. Receipt `2af009a5c168652f37e0b566bb87535d6726d4ea9f2af38383928558a5af9cd7`.

Native conic receipt follows.
