# Native shape gradients

Shape-distance gradient fills and strokes now render in the precise float compositor. The distance field is the same chamfer field the 8-bit texture uses (shared `distance_field`), built at the render scale and sampled bilinearly, with stops interpolated in float. Type spans painted with a shape gradient still refuse, because they have no whole-object field. Patterns, effects and rich span paint keep their named refusals.

Native shape gradient receipt follows.
