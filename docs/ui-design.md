# UI design

Guitario's look follows Guitar Pro 8's dark editor; the rules follow the sibling apps.

## Tokens

`crates/ui-egui/src/theme.rs` holds every colour: charcoal chrome, a dark LCD with pale-blue text,
white pages on a grey workspace, ink and staff-line greys from Guitar Pro's default stylesheet
(staff lines 180, bar lines 100), a blue edit cursor, a translucent blue selection band and an amber
playback cursor. Widgets read `theme::tokens()`; never hard-code a colour.

## Layout grammar (Guitar Pro)

- Top bar: document buttons left, transport and LCD centre, speed and view toggles right.
- Edition palette down the left edge: note values, rhythm modifiers, dynamics, articulations and
  techniques, bar tools; buttons light up for the beat and note under the cursor.
- Score in the middle: A4 pages, notation over tab joined by a bracket, TAB clef, short track name
  in the left margin, red bar numbers, boxed section names, tempo marks above the first system.
- Inspector on the right (Song, Track). Fretboard and global view along the bottom.
- Menus mirror Guitar Pro's (`menu_catalog.rs`); unbuilt items are greyed out.

## Icons

Toolbar icons are drawn with the painter (`icons.rs`); palette symbols come from the SMuFL font
(`glyphs.rs`) with drawn fallbacks.

## Verify

Every visual change: render offscreen (`examples/snapshot.rs`) at 1440×900 and look, in notation
and tab-only views, and with a demo that exercises the change.
