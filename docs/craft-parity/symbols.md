# Vector symbols

This layer uses the existing component definition/instance model. It does not introduce a second symbol document or a new serialized schema.

Symbols contains actual definition thumbnails and native selection/drag responses. New creates a transparent, unclipped definition wrapper around adjacent top-level vector artwork on one layer, or uses an eligible existing frame. Place and canvas drops insert linked instances. Edit selects the original definition; ordinary edits propagate through existing component synchronization. Break Link retains editable instance artwork. Remove Definition clears links while keeping every instance's artwork, with one Undo step. Rename uses the captured definition name to avoid overwriting a concurrent edit.

Creation across layers or from roots inside another frame remains explicit follow-on work, along with external symbol libraries, redefinition and symbolism tools. Invalid, locked, hidden and stale targets must preserve the document. This source layer requires its own focused and native qualification; source authoring alone does not close the full symbol-parity item.

## Instance commands

Duplicate makes a separate definition beside the source artwork. It remaps the definition's family and slots, including exposed property bindings, so later edits remain independent. The copied artwork is visible on the canvas under the current component-backed symbol model; this is not a separate hidden library.

Select All Instances selects visible, unlocked instance roots of the chosen definition without changing the document or history, and reports skipped hidden or locked roots. Reset and Break Link act on the whole concrete instance selection in one transaction. Selecting both an instance and one of its children does not apply the command twice. Locked, hidden or missing members reject the complete mutation instead of partially editing the remaining instances.

Cross-definition replacement, redefinition from alternate artwork, hidden library-only definitions, external libraries and symbolism tools remain open. Existing automatic definition-to-instance updates continue through the shared component synchronizer.

## Replace symbol

The Symbols panel's replace button swaps every selected instance for an instance of the chosen symbol in one Undo step. Each replacement keeps the old instance's top-left position, parent frame and stacking order, and the new instances stay selected. Locked, hidden or missing members refuse the whole command, and so do a missing symbol and a selection that already uses it.

### Decisions to review

- Replacement anchors on the old instance's bounding-box top-left, not its centre, as Craft's "Replace" does for different-sized symbols.
- Overrides on the old instance are not carried over, because the two definitions share no slots. The new instance shows the chosen symbol's own artwork.

At `f02e52f274aaa770ba7dd2adc7a10e30dc11c412` that route passed 11 tests. Receipt `634119fb69a27a6c338c7000e1181f91d8e52aa3b1dfe9b1e8364cde362f9e05`.

Symbols panel ▸ Import symbols from file… copies every symbol definition from another .oma document onto a hidden, locked "Symbol library: <file>" layer with fresh identities, in one Undo step; same-named symbols get the file name as a suffix. Symbol import receipt follows.

At `b95655ebb5e783d940f98defcde5f951f3c2d097` that route passed 13 tests. Receipt `c29fced97807105a154f7e452f2d494b3671aba49ab5286d8a4fd52bd2be04eb`.

Symbolism spray tools remain out of scope.
