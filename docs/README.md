# Developer documentation

For contributors and AI agents. User documentation lives on the documentation
site (`site/`). Agent rules: [AGENTS.md](../AGENTS.md).

| Document | Contents |
| --- | --- |
| [architecture.md](architecture.md) | Crates, processes, IPC, config flow, events, mapper, popups |
| [development.md](development.md) | Environment, device permissions, running, CLI, paths, env vars, troubleshooting |
| [testing.md](testing.md) | Test levels, everyday checks, UI scenarios, mapper e2e, VM stands, release acceptance |
| [manual-acceptance.md](manual-acceptance.md) | Manual acceptance scope, operator steps, expected results and hardware checks |
| [platforms.md](platforms.md) | Capability matrix, Linux/GNOME/X11 and Cinnamon/Windows/macOS specifics, adding backends |
| [config-format.md](config-format.md) | `config.json` and keymap formats, compatibility contract, schemas |
| [i18n.md](i18n.md) | Translations, `@tr`, `i18n::Msg`, PO rules |
| [ui-principles.md](ui-principles.md) | Rules for the settings UI and popups |
| [release.md](release.md) | Release flow, compatibility gate, CHANGELOG, artefacts, workflow |
| [design/](design/) | Proposals not yet implemented: [chord input](design/chord-input.md), [XKB extra symbols](design/xkb-extra-symbols.md) |

Elsewhere in the tree: `apps/slint-shell/vendor/*/PATCHES.md` (vendored crate
patches), `apps/slint-shell/ui/widgets/README.md` (local Fluent widget copy),
`schemas/` (published JSON Schemas), `SECURITY.md`.
