# Photoshop channel interchange

PSD and PSB export now write the root document's saved alpha and spot channels as merged-image extra channels. Names go in resource 1045 (Unicode) and display settings in resource 1077. Import reads them back as saved channels, with names, spot inks and solidity, at 8, 16 or 32 bits. Each channel is resampled onto the canvas grid with nearest samples, so translated or rotated channel placements land where they appear. Channels inside compositions are still refused rather than dropped.

## Decisions to review

- Spot channel data is written inverted (0 = full ink), as Photoshop stores it, and alpha data is written as coverage (255 = selected). Both are converted back on import.
- Alpha channels export with Photoshop's default display: red at 50%, "selected areas". Imported alpha display colors are not kept, because Omadesign alpha channels have no display color.
- Imported spot inks in non-RGB display spaces (Lab, CMYK, book colors) get a neutral display color and a warning. Their coverage is kept.
- Extra merged channels without a 1045 names resource stay unimported and keep the existing warning, so unknown extras are never mislabeled.

Calculations remain open.
