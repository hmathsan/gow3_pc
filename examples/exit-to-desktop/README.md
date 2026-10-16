# Exit to Desktop: an example mod and the tool that builds it

Adds an **Exit to Desktop** row to the game's own pause menu and to its Options screen (the main
menu's Options opens the same screen). It asks the game's own "Are you sure you want to Quit?"
question; *No* returns to the menu, *Yes* closes the game. Quit Game and Restart are unchanged.

The menus are one compiled script inside `EXEC/WAD/PS4_LE/R_UI.WAD`, so the mod is a patched copy
of that file, made from your own copy of the game (it cannot be shipped). The port's loader has one
small hook (`src/menu_hook.h`) that turns the new rows' call into the end of the process; it is
always installed and does nothing without this mod.

```sh
python pause_menu.py "<game>/EXEC/WAD/PS4_LE/R_UI.WAD" --out "mods/Exit to Desktop/EXEC/WAD/PS4_LE/R_UI.WAD"
```

Then enable **Exit to Desktop** on the launcher's Mods page. The log shows
`Pause menu: Exit to Desktop ready (needs the mod)` and `Mods: 1 game files replaced`.

The tool refuses (and writes nothing) when the script is not the one it was made for
(CUSA01623 01.02) or already has the rows.

## With other R_UI.WAD mods

A mod's `R_UI.WAD` replaces the whole file, and the last mod in the load order wins, so this mod
cannot be combined with another one that replaces it. To use both, build this one from the other
mod's file instead of the game's (the script is untouched by texture mods such as
[controller-prompts](../controller-prompts/README.md)):

```sh
python pause_menu.py "mods/Xbox Prompts/EXEC/WAD/PS4_LE/R_UI.WAD" --out "mods/Xbox Prompts + Exit/EXEC/WAD/PS4_LE/R_UI.WAD"
```

and enable only the combined one. The launcher's **In-game button icons** option works from the
game as the mods leave it, so its copy keeps the rows.

## How it works

`pause_menu.py` documents the script format (state records, code blocks, opcodes). In short:

- **Pause menu**: `WadChoose`, a cheat-only level loader after `PauseMain`'s subtree, becomes its
  sixth row, with its code written over `WadChoose`'s dead code.
- **Options**: a fifth row is inserted after `HLDisplay`; the state table and everything after it
  move, so state references, parents, subtree sizes, the script header and the WAD entry are
  corrected.
- **Confirmation**: the dialog is the script's own `FailSafe` machine. The rows send Quit Game's
  request with a third message parameter of 5; `FailSafeDisplay` keeps it in an unused local and
  `FailSafeYes`, given that mark, calls builtin `0xfff` (the hook) instead of what Quit Game does.

`python -m unittest test_pause_menu` (in `tests/`) checks the result on the game's `R_UI.WAD` when
`GOW3_GAME_DIR` points at it. Everything here is GPL-2.0-or-later like the rest of the port.
