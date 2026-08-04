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
"#,
    )
    .unwrap();
    let out = m.render();
    // inline tables, never [mods.x] sub-tables
    assert!(!out.contains("[mods.shared]"), "sub-table leaked:\n{out}");
    assert!(out.contains("sodium = \"Sodium 0.8.12 for NeoForge 1.21.1\""));
    assert!(out.contains(
        "shared = { modrinth = { slug = \"shared\" }, curseforge = { projectId = 42 }, version = \"1.0\" }"
    ));
    assert!(out.contains(
        "split = { modrinth = { slug = \"split\", version = \"1.0-mr\" }, curseforge = { projectId = 7, version = \"1.0-cf\" } }"
    ));
    // round-trips
    let m2: Manifest = toml::from_str(&out).unwrap();
    assert_eq!(m2.mods["shared"].curseforge_project_id(), Some(42));
}

#[test]
fn tstr_escapes_quotes_and_backslashes() {
    assert_eq!(tstr("a\"b\\c"), "\"a\\\"b\\\\c\"");
    assert_eq!(tstr("plain"), "\"plain\"");
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
    assert!(!out.contains("[mods.iceandfire]"), "sub-table leaked:\n{out}");
    assert!(!out.contains("dependencies"), "dependencies leaked into manifest:\n{out}");
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
