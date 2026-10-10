# Keyboard and mouse reference

[Documentation home](../README.md) · [Interface](../guides/interface.md)

These apply with canvas/app focus. Text entry, search, selectable Messages, or
an open dialog have their own handling. On macOS use Cmd; elsewhere use Ctrl.
The window menus display macOS-style labels, while platform keybindings select
the appropriate modifier.

## Editing and files

| Shortcut           | Action                                                                                                     |
| ------------------ | ---------------------------------------------------------------------------------------------------------- |
| Cmd/Ctrl-N         | New circuit                                                                                                |
| Cmd/Ctrl-F         | Find gates/nets by name                                                                                    |
| Cmd/Ctrl-O         | Open/upload                                                                                                |
| Cmd/Ctrl-S         | Save/download                                                                                              |
| Shift-Cmd/Ctrl-S   | Save As                                                                                                    |
| Cmd/Ctrl-Z         | Undo                                                                                                       |
| Shift-Cmd/Ctrl-Z   | Redo                                                                                                       |
| Cmd/Ctrl-X / C / V | Cut / copy / paste components; focused text/log uses text operations                                       |
| Cmd/Ctrl-A         | Select all gates/wires, or all focused text                                                                |
| Delete / Backspace | Delete selected gates/wire branches                                                                        |
| Enter              | Properties for selection; Apply in most dialogs                                                            |
| Escape             | Cancel unfinished gesture/wire, close ordinary dialog, end placement; recovery requires an explicit choice |
| R / Shift-R        | Rotate clockwise / counterclockwise                                                                        |
| V                  | Select/move                                                                                                |
| W                  | Connect wires                                                                                              |
| P                  | Drag-to-pan                                                                                                |
| D                  | Cut wire/delete tool                                                                                       |
| G                  | Toggle grid                                                                                                |
| Cmd/Ctrl-0         | Fit module                                                                                                 |
| + or = / −         | Zoom in / out                                                                                              |
| Cmd/Ctrl-Q         | Quit desktop; browser tells you to close the tab                                                           |
| F1                 | Getting started help                                                                                       |

## Quick placement

| Key | Component   |
| --- | ----------- |
| A   | AND         |
| O   | OR          |
| X   | XOR         |
| N   | NOT         |
| S   | Switch      |
| L   | LED         |
| C   | Clock       |
| F   | D flip-flop |
| M   | Multiplexer |
| T   | Comment     |

All remaining entries are in the searchable Components palette. Escape returns
to selection. A palette drag places a single component rather than
continuous-placement mode.

## Simulation

| Shortcut                      | Action                                                    |
| ----------------------------- | --------------------------------------------------------- |
| Space                         | Run/pause                                                 |
| F6                            | Step event timestamp                                      |
| Tab                           | Advance one root clock period                             |
| Stop toolbar button           | Reset/discard runtime and return to Edit                  |
| Double-click wire / ○ in Nets | Toggle waveform probe                                     |
| Click Switch/GPIO             | Toggle / increment runtime output                         |
| Click DIP                     | Enter runtime hex value                                   |
| Double-click RAM/ROM          | Runtime memory inspector                                  |
| Double-click TTY              | Live terminal                                             |
| Double-click module           | Enter definition in Edit, exact live instance in Simulate |

## Mouse

- Shift-click gates/wires adds or toggles selection.
- Drag empty canvas to box-select gates and intersecting wire segments;
  Shift-drag adds.
- Drag selected gate group to move it. Drag a horizontal wire segment vertically
  or vertical segment horizontally.
- Scroll pans schematic; Cmd/Ctrl-scroll zooms around pointer. P or middle-drag
  pans; native trackpad pinch zooms where supported.
- Right-click canvas for context menu.
- Drag sidebar edges, Modules/Nets separator, or top of bottom panel to resize.
- Drag a Frame's bottom-right corner to resize.
- Cmd/Ctrl-click or double-click comment links follows supported targets.
- Drag Messages text, then Cmd/Ctrl-C to copy across lines.

## Waveform-specific controls

Click/drag timeline sets A; Shift-click/drag sets B. Cmd/Ctrl-scroll zooms;
Shift/horizontal scroll or middle-drag pans time; vertical scroll moves signal
rows. Drag name-column divider to resize it. Use toolbar Fit/Follow, Clear A/B,
previous/next edge, radix, reorder, grouping, filter, remove, and VCD export.

Search fields don't invoke canvas placement hotkeys. Escape clears component
search and returns focus; Enter returns focus without clearing. Clicking the
canvas restores canvas focus when normal shortcuts seem inactive.
