# easypacker

A simple TUI/CLI tool for creating Minecraft Modpacks.

`Easypacker.toml` is the manifest you edit (by hand or with the tool);
`Easypacker.lock` is generated from it on every run, cargo-style.

## Usage

Run `easypacker` in a project directory to open the TUI; use `-C <dir>` to
point it at another project.

```
easypacker search <query>              # search mods (--platform, --version, --loader, --type, --sort, -n, --offset)
easypacker init                        # create a project (--name, --minecraft, --loader, --platforms, ...)
easypacker add <slug-or-id>            # add a project (--type, --version, --platform, --side)
easypacker versions <slug-or-id>       # list versions to pick from (--platform, -n)
easypacker pin <key> [version]         # pin an entry (default: latest matching; --platform per platform)
easypacker link <key> <other-id>       # link an entry to the same project on the other platform
                                         #   (different slugs/ids/names per platform: --platform, --version)
easypacker remove <key-or-slug>        # remove an entry (--type)
easypacker list                        # list the pack contents (--type)
easypacker set                         # edit project settings (same flags as init)
easypacker export <curseforge|modrinth|prism>  # build a pack file (--server, --output)
```

## The manifest

```toml
[project]
name = "My Pack"
version = "1.0.0"
minecraft = "1.21.1"
loader = "neoforge"
platforms = ["modrinth", "curseforge"]

[mods]
# Both platforms, slug/id is the key, newest matching version.
sodium = "latest"
# A pinned version name (platform-specific names can differ — see `link`).
jei = "19.21.0.247"
# CurseForge-only, by project id (numbers) or slug (strings).
parcool = { curseforge = 482378 }
# Modrinth-only with a slug that differs from the key.
irons-spells = { modrinth = "irons-spells-n-spellbooks", version = "1.21.1-3.16.2" }
# Both platforms, forced side, per-platform names left to the resolver.
distant-horizons = { side = "both", curseforge = 508933, modrinth = "distanthorizons" }
```

An entry naming no platform at all targets every platform in
`[project].platforms`. `version = "latest"` (or no `version`) means the newest
version matching the pack's minecraft/loader. Required dependencies are
resolved transitively into the lockfile; optional ones only when you add them.

## License

This project is licensed under the **GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later)** - see the [LICENSE](LICENSE) file for details.

## Acknowledgements

* **[Pakku](https://github.com/juraj-hrivnak/Pakku)**: Inspiration for this Minecraft modpack manager.
  * Licensed under the EUPL-1.2 - see [licenses/Pakku.license](licenses/Pakku.license).
* **[Cargo](https://github.com/rust-lang/cargo)**: Inspiration for the TOML/Lockfile format. Also adapted some code from it.
  * Used under the MIT License - see [licenses/Cargo.license](licenses/Cargo.license).
* **[Prism Launcher](https://github.com/PrismLauncher/PrismLauncher)**: Reference for how mod dependencies are modelled (required/optional/incompatible relations from Modrinth and CurseForge).
