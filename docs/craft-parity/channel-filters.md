# Scalar channel filters

Saved alpha and Quick Mask targets accept Gaussian blur, Levels and Threshold. The Channels toolbar opens these controls; the existing raster filter entry routes the same supported kinds to the scalar dialog. Other layer-only filters remain unavailable for scalar targets.

The existing captured adjustment worker owns the immutable source, its selection and placement. Previews never edit the document; Apply renders the full original plane and commits through the existing scalar command. Cancel, changed selection, document switches and changed allocation owners retire pending results. Original view hides the proxy without changing source data.

Gaussian blur shares the raster engine's three-box approximation, using one float component for coverage and four for RGBA. Levels includes input black/white, gamma and output black/white. Threshold compares original float values directly. Selection coverage blends the completed filter once; untouched samples retain their original bits. Working buffers use the shared resource pool and cancellation checks.

This source is not qualified yet. Scalar math, native fields, existing RGBA behavior, Undo/save/reopen and a real desktop workflow still require execution. Broader scalar filters, color/spot targets, interchange and complete Craft parity remain open. No document schema, dependency or app-version change is involved. The existing human QA window stays open and untouched.
