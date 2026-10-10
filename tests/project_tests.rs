use super::*;

const EXAMPLE: &str = r#"
[project]
name = "testnig"
version = "0.0.1"
authors = "Iskaa303"
credits = ""
description = ""
minecraft = "1.21.1"
loader = "neoforge"
platforms = [
    "curseforge",
    "modrinth",
]

[project.links]

[mods]
sodium = "Sodium 0.8.12 for NeoForge 1.21.1"
terrafirmacraft = "TerraFirmaCraft 1.21.1-4.2.6"

[resourcepacks]
"#;

#[test]
fn parses_example_manifest() {
    let m: Manifest = toml::from_str(EXAMPLE).unwrap();
    assert_eq!(m.project.name, "testnig");
    assert_eq!(m.project.loader, "neoforge");
    assert_eq!(m.mods.len(), 2);
    assert_eq!(
        m.mods["sodium"].version_for(true).as_deref(),
        Some("Sodium 0.8.12 for NeoForge 1.21.1")
    );
    assert_eq!(m.mods["sodium"].modrinth_slug("sodium"), "sodium");
}

#[test]
fn parses_detailed_and_inherits_shared_version() {
    let m: Manifest = toml::from_str(
        r#"
[project]
name = "x"
minecraft = "1.21.1"
loader = "neoforge"

[mods]
shared = { version = "1.0", modrinth = { slug = "shared" }, curseforge = { projectId = 42 } }
split = { modrinth = { slug = "split", version = "1.0-mr" }, curseforge = { projectId = 7, version = "1.0-cf" } }
"#,
    )
    .unwrap();
    let shared = &m.mods["shared"];
    assert_eq!(shared.version_for(true).as_deref(), Some("1.0"));
    assert_eq!(shared.version_for(false).as_deref(), Some("1.0"));
    assert_eq!(shared.curseforge_project_id(), Some(42));
    let split = &m.mods["split"];
    assert_eq!(split.version_for(true).as_deref(), Some("1.0-mr"));
    assert_eq!(split.version_for(false).as_deref(), Some("1.0-cf"));
    assert_eq!(split.shared_version(), None);
}

#[test]
fn roundtrip_mixed_simple_and_detailed() {
    let m: Manifest = toml::from_str(
        r#"
[project]
name = "x"
minecraft = "1.21.1"
loader = "neoforge"

[mods]
aaa = { modrinth = { slug = "aaa" }, version = "1.0" }
zzz = "ZZZ 2.0"
"#,
    )
    .unwrap();
    let out = m.render();
    let m2: Manifest = toml::from_str(&out).unwrap();
    assert_eq!(m2.mods.len(), 2);
    assert_eq!(m2.mods["zzz"].version_for(true).as_deref(), Some("ZZZ 2.0"));
}

#[test]
fn renders_detailed_entries_as_inline_tables() {
    let m: Manifest = toml::from_str(
        r#"
[project]
name = "x"
minecraft = "1.21.1"
loader = "neoforge"

[mods]
sodium = "Sodium 0.8.12 for NeoForge 1.21.1"
shared = { modrinth = { slug = "shared" }, curseforge = { projectId = 42 }, version = "1.0" }
split = { modrinth = { slug = "split", version = "1.0-mr" }, curseforge = { projectId = 7, version = "1.0-cf" } }
renamed = { modrinth = { slug = "real-slug" } }
"#,
    )
    .unwrap();
    let out = m.render();
    // inline tables, never [mods.x] sub-tables
    assert!(!out.contains("[mods.shared]"), "sub-table leaked:\n{out}");
    assert!(out.contains("sodium = \"Sodium 0.8.12 for NeoForge 1.21.1\""));
    // Shared version hoisted, default platform identities shortened.
    assert!(
        out.contains("shared = { version = \"1.0\", modrinth = true, curseforge = 42 }"),
        "unexpected shared render:\n{out}"
    );
    // Per-platform versions differ -> both kept inline (slug == key omitted).
    assert!(out.contains(
        "split = { modrinth = { version = \"1.0-mr\" }, curseforge = { projectId = 7, version = \"1.0-cf\" } }"
    ), "unexpected split render:\n{out}");
    // Slug different from the key stays explicit (short string form).
    assert!(
        out.contains("renamed = { modrinth = \"real-slug\" }"),
        "{out}"
    );
    // round-trips
    let m2: Manifest = toml::from_str(&out).unwrap();
    assert_eq!(m2.mods["shared"].curseforge_project_id(), Some(42));
    assert_eq!(m2.mods["shared"].version_for(true).as_deref(), Some("1.0"));
    assert_eq!(
        m2.mods["split"].version_for(true).as_deref(),
        Some("1.0-mr")
    );
    assert_eq!(
        m2.mods["split"].version_for(false).as_deref(),
        Some("1.0-cf")
    );
}

#[test]
fn tstr_escapes_quotes_and_backslashes() {
    assert_eq!(tstr("a\"b\\c"), "\"a\\\"b\\\\c\"");
    assert_eq!(tstr("plain"), "\"plain\"");
}

#[test]
fn parses_every_short_platform_form() {
    let m: Manifest = toml::from_str(
        r#"
[project]
name = "x"
minecraft = "1.21.1"
loader = "neoforge"

[mods]
both-default = { version = "1.0", modrinth = true, curseforge = true }
cf-id = { curseforge = 394468 }
cf-slug = { curseforge = "real-cf-slug" }
mr-slug = { modrinth = "real-mr-slug" }
side-only = { side = "client" }
"#,
    )
    .unwrap();
    assert_eq!(m.mods["both-default"].curseforge_project_id(), None);
    assert!(m.mods["both-default"].has_modrinth());
    assert!(m.mods["both-default"].has_curseforge());
    assert_eq!(m.mods["cf-id"].curseforge_project_id(), Some(394_468));
    assert!(!m.mods["cf-id"].has_modrinth());
    assert_eq!(
        m.mods["cf-slug"].curseforge_slug("cf-slug").as_deref(),
        Some("real-cf-slug")
    );
    assert_eq!(m.mods["mr-slug"].modrinth_slug("mr-slug"), "real-mr-slug");
    assert_eq!(m.mods["side-only"].side(), Some(Side::Client));
    assert!(m.mods["side-only"].has_modrinth() && m.mods["side-only"].has_curseforge());
}

#[test]
fn render_picks_the_shortest_form() {
    // Both platforms with default identities: the plain string form, with
    // `latest` spelled out when nothing is pinned.
    let m: Manifest = toml::from_str(
        r#"
[project]
name = "x"
minecraft = "1.21.1"
loader = "neoforge"

[mods]
pinned = { version = "1.2.3", modrinth = true, curseforge = true }
unpinned = { modrinth = true, curseforge = true }
"#,
    )
    .unwrap();
    let out = m.render();
    assert!(out.contains("pinned = \"1.2.3\""), "{out}");
    assert!(out.contains("unpinned = \"latest\""), "{out}");
    // And both spellings round-trip to the same resolution.
    let m2: Manifest = toml::from_str(&out).unwrap();
    assert_eq!(
        m2.mods["pinned"].version_for(true).as_deref(),
        Some("1.2.3")
    );
    assert_eq!(
        m2.mods["unpinned"].version_for(true).as_deref(),
        Some(LATEST)
    );
    assert!(is_latest("LATest"));
}

#[test]
fn platform_less_table_targets_every_platform() {
    // `{ version = ... }` / `{ side = ... }` name no platform, so they mean
    // every platform the pack targets — and normalize to the string form.
    let m: Manifest = toml::from_str(
        r#"
[project]
name = "x"
minecraft = "1.21.1"
loader = "neoforge"

[mods]
bare-version = { version = "1.0" }
client-only = { side = "client" }
"#,
    )
    .unwrap();
    for key in ["bare-version", "client-only"] {
        assert!(m.mods[key].has_modrinth(), "{key}");
        assert!(m.mods[key].has_curseforge(), "{key}");
    }
    let out = m.render();
    assert!(out.contains("bare-version = \"1.0\""), "{out}");
    assert!(out.contains("client-only = { side = \"client\" }"), "{out}");
    // Round-trips with the same meaning (`None` = newest matching).
    let m2: Manifest = toml::from_str(&out).unwrap();
    assert_eq!(m2.mods["client-only"].version_for(true), None);
    assert_eq!(m2.mods["client-only"].side(), Some(Side::Client));
    assert_eq!(
        m2.mods["bare-version"].version_for(true).as_deref(),
        Some("1.0")
    );
}

#[test]
fn rejects_false_platform_marker() {
    // `false` would be a second way to say "drop the platform"; removing the
    // key is the one true spelling, so it is a parse error.
    assert!(
        toml::from_str::<Manifest>(
            r#"
[project]
name = "x"
minecraft = "1.21.1"
loader = "neoforge"

[mods]
nope = { modrinth = false }
"#,
        )
        .is_err()
    );
}

#[test]
fn tkey_quotes_unsafe_keys() {
    assert_eq!(tkey("sodium"), "sodium");
    assert_eq!(tkey("a.b c"), "\"a.b c\"");
}

#[test]
fn init_writes_user_facing_shape() {
    let dir = std::env::temp_dir().join(format!("easypacker-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let m = Manifest::init_project(
        &dir,
        ModpackProject {
            name: "testnig".into(),
            version: "0.0.1".into(),
            authors: "Iskaa303".into(),
            credits: String::new(),
            description: String::new(),
            minecraft: "1.21.1".into(),
            loader: "neoforge".into(),
            platforms: vec!["curseforge".into(), "modrinth".into()],
            links: ProjectLinks::default(),
        },
    )
    .unwrap();
    assert!(dir.join(MANIFEST_FILE).exists());
    assert_eq!(m.project.version, "0.0.1");
    let raw = std::fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap();
    assert!(raw.contains("[project]"));
    assert!(raw.contains("[project.links]"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn renders_and_roundtrips_dependency_overrides() {
    // Deps become plain manifest mod entries (keyed by slug) — no
    // `dependencies` field ever appears in the TOML.
    let m: Manifest = toml::from_str(
        r#"
[project]
name = "x"
minecraft = "1.21.1"
loader = "neoforge"

[mods]
iceandfire = { modrinth = { slug = "iceandfire", version = "2.1" } }
patchouli = { modrinth = { slug = "patchouli", version = "1.21.1-93" } }
"#,
    )
    .unwrap();
    let out = m.render();
    // No sub-tables, no dependencies field.
    assert!(
        !out.contains("[mods.iceandfire]"),
        "sub-table leaked:\n{out}"
    );
    assert!(
        !out.contains("dependencies"),
        "dependencies leaked into manifest:\n{out}"
    );
    // round-trips.
    let m2: Manifest = toml::from_str(&out).unwrap();
    assert!(matches!(&m2.mods["patchouli"], ModSpec::Detailed(_)));
}

#[test]
fn default_deps_absent_from_render() {
    // No `dependencies` map => nothing about deps appears in the TOML.
    let m: Manifest = toml::from_str(
        r#"
[project]
name = "x"
minecraft = "1.21.1"
loader = "neoforge"

[mods]
sodium = "Sodium 0.8.12"
"#,
    )
    .unwrap();
    let out = m.render();
    assert!(!out.contains("dependencies"));
}

#[test]
fn same_file_matches_by_hash_not_name() {
    use crate::api::types::ProjectFile;
    let base = || ProjectFile {
        name: String::new(),
        mc_versions: vec![],
        loaders: vec![],
        date_published: String::new(),
        downloads: 0,
        size: 0,
        url: None,
        platforms: vec![],
        modrinth_version_id: None,
        modrinth_url: None,
        curseforge_file_id: None,
        curseforge_url: None,
        curseforge_project_id: None,
        sha1: None,
        modrinth_name: None,
        curseforge_name: None,
        dependencies: vec![],
    };
    // Different names, same sha1 -> same file.
    let mut a = base();
    a.name = "Mod-1.0-mc1.21.1.jar".into();
    a.sha1 = Some("abc123".into());
    a.size = 100;
    let mut b = base();
    b.name = "completely-different-name.zip".into();
    b.sha1 = Some("ABC123".into());
    b.size = 9999;
    assert!(a.same_file_as(&b), "sha1 match should win over name/size");
    // Same name+size but different sha1 -> NOT the same file.
    let mut c = base();
    c.name = "Mod-1.0-mc1.21.1.jar".into();
    c.sha1 = Some("def456".into());
    c.size = 100;
    assert!(!a.same_file_as(&c), "different sha1 should not match");
    // No sha1 on one side -> fall back to size+name.
    let mut d = base();
    d.name = "Mod-1.0-mc1.21.1.jar".into();
    d.sha1 = None;
    d.size = 100;
    assert!(a.same_file_as(&d), "no hash -> size+name fallback");
}

#[test]
fn renders_per_platform_versions() {
    // A dep pinned with different names on each platform keeps both names in
    // the toml: modrinth = { version } + curseforge = { projectId, version }.
    // (The slug equal to the key is omitted; the differing versions stay.)
    let m: Manifest = toml::from_str(
        r#"
[project]
name = "x"
minecraft = "1.21.1"
loader = "neoforge"

[mods]
patchouli = { modrinth = { slug = "patchouli", version = "1.21.1-92-neoforge" }, curseforge = { projectId = 306770, version = "Patchouli-1.21.1-92-NEOFORGE.jar" } }
"#,
    )
    .unwrap();
    let out = m.render();
    assert!(out.contains(
        "patchouli = { modrinth = { version = \"1.21.1-92-neoforge\" }, curseforge = { projectId = 306770, version = \"Patchouli-1.21.1-92-NEOFORGE.jar\" } }"
    ), "per-platform versions not rendered:\n{out}");
    // Round-trips and each platform resolves its own version.
    let m2: Manifest = toml::from_str(&out).unwrap();
    let spec = &m2.mods["patchouli"];
    assert_eq!(
        spec.version_for(true).as_deref(),
        Some("1.21.1-92-neoforge")
    );
    assert_eq!(
        spec.version_for(false).as_deref(),
        Some("Patchouli-1.21.1-92-NEOFORGE.jar")
    );
}

#[test]
fn renders_equal_platform_versions_as_one_shared_version() {
    // The TUI writes each platform's own version name; when they're equal the
    // renderer collapses them into a single shared `version`.
    let m: Manifest = toml::from_str(
        r#"
[project]
name = "x"
minecraft = "1.21.1"
loader = "neoforge"

[mods]
sodium = { modrinth = { slug = "sodium", version = "Sodium 0.8.13 for NeoForge 1.21.1" }, curseforge = { projectId = 394468, version = "Sodium 0.8.13 for NeoForge 1.21.1" } }
"#,
    )
    .unwrap();
    let out = m.render();
    assert!(
        out.contains(
            "sodium = { version = \"Sodium 0.8.13 for NeoForge 1.21.1\", modrinth = true, curseforge = 394468 }"
        ),
        "equal versions not hoisted:\n{out}"
    );
    // Both platforms still resolve the hoisted version.
    let m2: Manifest = toml::from_str(&out).unwrap();
    let spec = &m2.mods["sodium"];
    assert_eq!(
        spec.version_for(true).as_deref(),
        Some("Sodium 0.8.13 for NeoForge 1.21.1")
    );
    assert_eq!(
        spec.version_for(false).as_deref(),
        Some("Sodium 0.8.13 for NeoForge 1.21.1")
    );
}

#[test]
fn shared_version_hides_equal_platform_overrides() {
    // A platform version equal to the shared one is redundant and omitted;
    // a different one stays as an override.
    let m: Manifest = toml::from_str(
        r#"
[project]
name = "x"
minecraft = "1.21.1"
loader = "neoforge"

[mods]
mod1 = { version = "1.0", modrinth = { slug = "mod1", version = "1.0" }, curseforge = { projectId = 9, version = "1.0-cf" } }
"#,
    )
    .unwrap();
    let out = m.render();
    assert!(out.contains(
        "mod1 = { version = \"1.0\", modrinth = true, curseforge = { projectId = 9, version = \"1.0-cf\" } }"
    ), "unexpected render:\n{out}");
    let m2: Manifest = toml::from_str(&out).unwrap();
    let spec = &m2.mods["mod1"];
    assert_eq!(spec.version_for(true).as_deref(), Some("1.0"));
    assert_eq!(spec.version_for(false).as_deref(), Some("1.0-cf"));
}
