# Bundled fonts

All fonts in this directory are licensed under the **SIL Open Font License,
Version 1.1**. Each family folder contains its `OFL.txt` with the copyright
notice of the font's authors. The font files are distributed unmodified, as
static TrueType instances served by Google Fonts (fonts.google.com); the
license texts come from the `google/fonts` repository.

| Family | Folder | Faces | Version | Upstream |
|---|---|---|---|---|
| Roboto | `roboto/` | Regular, Bold, Italic, Bold Italic | 3.015 | https://github.com/googlefonts/roboto-3-classic |
| Open Sans | `opensans/` | Regular, Bold, Italic, Bold Italic | 3.003 | https://github.com/googlefonts/opensans |
| Lato | `lato/` | Regular, Bold, Italic, Bold Italic | 1.104 | https://www.latofonts.com |
| Montserrat | `montserrat/` | Regular, Bold, Italic, Bold Italic | 9.000 | https://github.com/JulietaUla/Montserrat |
| Lora | `lora/` | Regular, Bold, Italic, Bold Italic | 3.008 | https://github.com/cyrealtype/Lora-Cyrillic |
| Source Serif 4 | `sourceserif4/` | Regular, Bold, Italic, Bold Italic | 4.004 | https://github.com/adobe-fonts/source-serif |
| Roboto Mono | `robotomono/` | Regular, Bold, Italic, Bold Italic | 3.001 | https://github.com/googlefonts/robotomono |
| Caveat | `caveat/` | Regular, Bold | 2.000 | https://github.com/googlefonts/caveat |

Lato, Lora and Source Serif 4 carry Reserved Font Names ("Lato", "Lora",
"Source"); they must not be modified (subset, converted, renamed) and shipped
under those names. Embedding the unmodified fonts in PDF documents is
permitted by the OFL.

The installer copies every `OFL.txt` to `licenses/fonts/` next to the app
(see `bundle.resources` in `apps/desktop-tauri/src-tauri/tauri.conf.json`).
