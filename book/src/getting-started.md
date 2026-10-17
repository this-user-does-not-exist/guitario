# Getting started

Build and start the editor with `cargo run --release -p guitario`, or open a score directly:
`cargo run --release -p guitario -- demos/01-thrash-attack.gt`.

The window follows Guitar Pro's layout. The toolbar runs, left to right: Home (new, open, recent
files), the panel toggles (palette, global view, inspector), zoom, display mode, undo and redo,
print, the transport, the LCD, loop and relative speed, relative tonality, and the instrument views
(fretboard, keyboard, drums). The LCD shows the track (click it to pick another), count-in and
metronome, the note under the cursor, then the bar, whether the bar is full (✔ green, ! orange
when short, ! red when too long, with the written and expected beats), the time, the triplet feel,
the tempo (click it to edit the tempo automation) and the time signature. The edition palette runs
down the left; pages sit in the middle; the inspector is on the right; the instrument views and the
global view (tracks and bars) are at the bottom. Toggle panels from the toolbar or the View menu;
Guitario remembers your layout.

**View › Layout** switches between pages, one long sheet as wide as the window (**Screen**) and one
line of bars (**Horizontal**). **View › Multitrack** shows every track of the song stacked in each
system, lined up in time; click any track's notes to edit that track.

The `demos/` folder has original metal pieces to try: open one, click a bar and press **Space**.
