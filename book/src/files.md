# Files

- **Open** `.gt` (Guitario; `.gt` files from before the rename too) and Guitar Pro files
  (`.gp`, `.gpx`, `.gp5`, `.gp4`, `.gp3`), drum tracks and up to four voices included. Some notation
  (whammy dives, fingering) is skipped for now, and Guitario tells you what.
- **Save** as `.gt`. Saves are atomic: a crash or full disk never damages the previous save.
- **Export** Guitar Pro 8 (`.gp`), Guitar Pro 5, PDF, MIDI, WAV audio and ASCII tab from File › Export.
- **PDF and print.** File › Export › PDF… writes the track on screen as A4 pages (vector, with the
  fonts embedded and the text selectable). File › Print… (Cmd+P) sends the same pages to a printer:
  choose the printer, copies, collate, sides and black and white, and one track or all. Pages are
  shrunk to fit other paper. File › Print Preview… shows the pages first, and File › Page Setup… chooses
  A4, A5, Letter or Legal, portrait or landscape, and the margins; the choice is saved in the file and
  in Guitar Pro exports. From a script: `guitario-cli convert song.gt song.pdf [--tracks 0,2] [--tab-only]`,
  or the `file.exportPdf`, `file.print` and `file.printers` commands.
- **Recent files** are under File › Open Recent.
