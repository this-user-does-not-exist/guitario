# UI design

Guitario's look follows Guitar Pro 8's dark editor; the rules follow the sibling apps.

## Tokens

`crates/ui-egui/src/theme.rs` holds every colour: charcoal chrome, a dark LCD with pale-blue text,
white pages on a grey workspace, ink and staff-line greys from Guitar Pro's default stylesheet
(staff lines 180, bar lines 100), a blue edit cursor, a translucent blue selection band and an amber
playback cursor. Widgets read `theme::tokens()`; never hard-code a colour.

## Layout grammar (Guitar Pro)

- Toolbar (`topbar.rs`), Guitar Pro 8's order: home, panel toggles, zoom and display-mode spinners,
  undo/redo, print on the left; transport, two-row LCD, loop and speed, relative tonality in the
  centre; audio track, instrument views, tuner, line-in, Fretlight on the right. Buttons sit in
  rounded groups (`Group`), blue when toggled on; right-end groups drop out first when narrow.
- Edition palette down the left edge: note values, rhythm modifiers, dynamics, articulations and
  techniques, bar tools; buttons light up for the beat and note under the cursor.
- Score in the middle: A4 pages on a slate workspace, drawn as Guitar Pro's default stylesheet
  draws them: title block, "Standard tuning" (or circled strings) and a bold tempo above the first
  system; notation over tab joined by a bracket, a plain T-A-B clef, the time signature in the
  staff only (in the tab when it stands alone); the abbreviated track name upright left of the
  first system, which alone is indented; small grey bar numbers on every bar (red on screen for a
  bar of the wrong length); sections as a boxed letter and a bold name, framed when bracketed.
- Inspector on the right (Song, Track). Fretboard and global view along the bottom.
- Menus mirror Guitar Pro's twelve, in its order (`menu_catalog.rs`); unbuilt items are greyed out.
- No status bar (Guitar Pro has none): messages show as a toast over the bottom of the score.

## Icons

Toolbar icons are drawn with the painter (`icons.rs`); palette symbols come from the SMuFL font
(`glyphs.rs`) with drawn fallbacks.

## Verify

Every visual change: render offscreen (`examples/snapshot.rs`) at 1440×900 and look, in notation
and tab-only views, and with a demo that exercises the change.
