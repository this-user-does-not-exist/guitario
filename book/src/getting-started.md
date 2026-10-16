# Getting started

Build and start the editor with `cargo run --release -p guitario`, or open a score directly:
`cargo run --release -p guitario -- demos/01-thrash-attack.gt`.

The window follows Guitar Pro's layout: the top bar holds the transport and the LCD (track, bar,
beat, time signature, time and tempo); the edition palette runs down the left; pages sit in the
middle; the inspector is on the right; the fretboard and the global view (tracks and bars) are at the
bottom. Toggle panels from the top bar or the View menu; Guitario remembers your layout.

**View › Layout** switches between pages, one long sheet as wide as the window (**Screen**) and one
line of bars (**Horizontal**). **View › Multitrack** shows every track of the song stacked in each
system, lined up in time; click any track's notes to edit that track.

The `demos/` folder has original metal pieces to try: open one, click a bar and press **Space**.
