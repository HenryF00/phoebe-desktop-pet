# Teaching pointer illumination correction

Built-in image_gen edit mode, 2026-09-09. Both existing poses are retained. The master is assets/pet/frames/master.png. Previous chroma-key sources are retained under assets/pet/teaching-source/before-lighting-20260909.

## Diagonal pointer

Use case: lighting-weather.
Asset type: anime desktop-pet teaching animation keyframe.
Input image 1 is the EDIT TARGET, current pointer-up pose on flat magenta. Input image 2 is the COLOR AND LIGHTING MASTER, same character without pointer.
Primary request: Correct ONLY the target character's material colors and cel shadow values to match the master. Target is too bright, too warm, too yellow. Match master subdued periwinkle-blue hair (less saturated cyan highlights), neutral charcoal hat/cape/leggings, dusty muted brown boots and cape interior, off-white cream blouse and trim with gray-taupe fold shadows, warm neutral peach face with the master's soft hat-brim shading. The master is NOT under a spotlight. Use the master's restrained matte anime palette and defined cel shadow shapes. Retain crisp existing linework.
Composition invariants: absolutely preserve the exact target pose, face/eyes/head size, body proportions, hat silhouette, braided hair outline, diagonal pointer and hand locations, free hand, costume, boots, facial expression, framing and margins. No repositioning, no crop, no new pose. Keep whole character precisely at the same canvas locations. Recolor, do not redraw.
Backdrop: Keep perfectly uniform pure magenta #FF00FF background for deterministic sprite chroma extraction, absolutely no light spill, no checkerboard, no ground shadow.
Do not apply a whole-image dark filter; match each individual material to the master. No glossy highlights, bloom, rim light, white halo, grain, decoration or text. Output ONE full image at the same near-square aspect ratio as the first reference.

## Level pointer

Use case: lighting-weather.
Asset type: second anime desktop-pet teaching animation keyframe.
Input image 1 is EDIT TARGET (horizontal/level pointer pose). Input image 2 is the original COLOR/LIGHTING MASTER (no pointer). Input image 3 is the just-corrected diagonal-pointer teaching frame to ensure this pair has identical colors.
Primary request: Correct ONLY target material colors and cel shadow values to match master and corrected companion. Current target looks overexposed, too warm and yellow. Match restrained matte periwinkle-blue hair (reduce vivid cyan highlights); neutral charcoal hat/cape/leggings; dusty muted brown boots and cape interior; off-white cream blouse and trim with gray-taupe fold shadows; warm neutral peach skin with the master's soft hat-brim shading. Use exact same material palette as corrected companion image 3 so alternating these two frames has no exposure flicker. Image 2 is the master for natural subdued cel-shading, not dramatic lighting.
Composition invariants: Preserve target's exact closed-mouth facial expression, eyes and face geometry, head/body proportions, character/hat/feet positions, braids, costume silhouette, free hand, HORIZONTAL POINTER and holding hand. No pose changes. Recolor with crisp existing outlines, do not redraw, crop or zoom.
Backdrop: keep entirely uniform pure magenta #FF00FF background for sprite chroma extraction, no spill, checkerboard, floor, ground shadow, halo or texture.
Do not apply a whole-image dark filter; correct material colors and shadows individually. No glossy highlights, bloom, rim light, text or extra objects. Output ONE full image at the same near-square aspect ratio as target.

Generated sources were copied into assets/pet/teaching-source/pointer-up.png and pointer-level.png. The existing Swift alpha extraction exports the four 1440x1400 directional frames without a global color filter. This is a generated appearance match, not a pixel-identical reconstruction of the master.

During comparison QA the original exporter was found to lift midtones during an NSCalibratedRGBColorSpace-to-deviceRGB conversion. The exporter was corrected to preserve encoded RGB bytes through an explicit sRGB CGContext. Selected files use that corrected exporter; the old source files and old exported images are retained for comparison. See qa/teaching-lighting/README.md and validation.json.
