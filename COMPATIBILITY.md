# Compatibility

This fork intentionally supports only **Mario Kart 7 for Nintendo 3DS** as a directly launchable game.

| Region | Base-game title ID | Base game | CTGP-7 v1.2 update title ID | Update version |
|---|---|---|---|---:|
| Japan | 0004000000030600 | Supported | 0004000E00030600 | 3152 |
| Europe | 0004000000030700 | Supported | 0004000E00030700 | 3152 |
| USA | 0004000000030800 | Supported | 0004000E00030800 | 3152 |
| Korea | 0004000000030A00 | Supported | 0004000E00030A00 | 3088 |
| Taiwan | 000400000008B400 | Supported | Not supported by CTGP-7 | — |
| China | 000400000008B500 | Supported | Not supported by CTGP-7 | — |

## Update handling

Update data is a separate title with the `0004000E` high word. The regional suffix matches the base game's title ID. Put the decrypted update CIA in the same folder as the base game; the library pairs it automatically and selects the highest update version it finds. A base ROM's title ID does not change just because update data is present.

## CTGP-7

CTGP-7 officially requires the matching regional Mario Kart 7 v1.2 update and supports USA, Europe, Japan and Korea. This fork recognizes and attaches those four regional update titles, but **CTGP-7 is not currently playable here**: the runtime does not implement Luma3DS 3GX plugin execution or the title-install flow used by the CTGP-7 installer. RomFS and code-patch mod support alone cannot replace those systems.

All other Nintendo 3DS titles are filtered out of the library and rejected when started directly. Decrypted game data is required. The title-specific recompiled output is generated locally and is not distributed with the project.
