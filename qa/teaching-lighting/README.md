# Teaching sprite lighting, 2026-09-09

The strongest exposure error came from export, not only generation. `NSBitmapImageRep.colorAt` returned calibrated RGB colors; converting those colors to `deviceRGB` lifted midtones. A representative hat pixel was 59/58/58 in the generated source and became 75/75/74 in the old exported frame. The master at the aligned location is 57/57/58.

`scripts/assemble_teaching.swift` now keys raw encoded RGB pixels and uses an explicit sRGB Core Graphics context for resizing and mirroring. The selected corrected diagonal frame exports the sample as 59/59/58 (one-channel resampling rounding), and its cape sample is 60/60/57 versus the master's 59/59/61. No global darkening filter is used.

Both teaching sources received one built-in imagegen lighting edit. The edited sources were preferred after comparing original-source exports with the same corrected export path: the edited pair has less vivid cyan hair and less yellow clothing/boots. Their poses and framing remain close to the existing poses. Generated cel colors are still not pixel-identical to the master: the level frame's dark-material sample is 63/65/60, so a small material tint difference remains.

## Evidence

- `comparison.png`: master, old up, selected up, old level, selected level on a common neutral background.
- `export-comparison.png`: master, old export, original source with corrected exporter, edited source with corrected exporter, edited level.
- `validation.json`: channel samples and alpha/export invariants.
- `check.swift`: repeatable check, run with `swift qa/teaching-lighting/check.swift /path/to/Roxy`.

Both outputs and both mirrors are 1440x1400 RGBA. The median per-channel source-to-export error in flat opaque areas is 0 (3023 and 2920 samples). Mirrored frames match exactly. Opaque magenta residue count is 0; each has over 1.63 million fully transparent pixels. This check verifies alpha/background and encoded color preservation, not complete character-identity equivalence.

Original generated sources are retained in `assets/pet/teaching-source/before-lighting-20260909/`. Prompts and generation provenance are in `assets/pet/prompts/teaching-pointer-lighting-20260909.md`. No native source, manifest, app build or running process was changed for this isolated asset fix.
