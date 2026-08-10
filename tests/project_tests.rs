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
    // the toml: modrinth = { slug, version } + curseforge = { projectId, version }.
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
        "patchouli = { modrinth = { slug = \"patchouli\", version = \"1.21.1-92-neoforge\" }, curseforge = { projectId = 306770, version = \"Patchouli-1.21.1-92-NEOFORGE.jar\" } }"
    ), "per-platform versions not rendered:\n{out}");
    // Round-trips and each platform resolves its own version.
    let m2: Manifest = toml::from_str(&out).unwrap();
    let spec = &m2.mods["patchouli"];
    assert_eq!(spec.version_for(true).as_deref(), Some("1.21.1-92-neoforge"));
    assert_eq!(
        spec.version_for(false).as_deref(),
        Some("Patchouli-1.21.1-92-NEOFORGE.jar")
    );
}
