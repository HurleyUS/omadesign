# Native shape gradients

Shape-distance gradient fills and strokes now render in the precise float compositor. The distance field is the same chamfer field the 8-bit texture uses (shared `distance_field`), built at the render scale and sampled bilinearly, with stops interpolated in float. Type spans painted with a shape gradient still refuse, because they have no whole-object field. Patterns, effects and rich span paint keep their named refusals.

At `1780c364ccbea2882c115640c59abdc8b56c3d75` that route passed 65 tests. Receipt `3a00adcf7148669a64d963298047d082b5f5e28560c7e6e28e00803a9496bf72`.

Native shape gradient receipt follows.
