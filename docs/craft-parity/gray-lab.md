# Gray and Lab tagged documents

Document ▸ Color and precision can tag an RGB document as Gray or Lab (project version 40 only when tagged; one Undo step). The working space stays RGB and the tag converts at the edges:

- Gray: canvas and still exports render as luminance gray; PNG export writes 8-bit gray+alpha.
- Lab: rendering is unchanged; the color picker shows L*, a*, b* (D65) fields.
- Gray picker shows a K% field.
- PSD import: Grayscale files are tagged Gray; 8-bit Lab files (previously refused) decode to sRGB and are tagged Lab. 16/32-bit Lab still refuses.

True Gray/Lab working modes can later replace these edge conversions without changing the saved tag.

Gray Lab receipt follows.
