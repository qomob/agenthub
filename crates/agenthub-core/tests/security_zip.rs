//! Integration tests: hostile zip archives must never write outside the
//! extraction root and must be rejected when they contain traversal or symlink
//! entries.

mod common;

use std::fs;
use std::io::Write;
use std::path::Path;

use agenthub_core::model::*;
use agenthub_core::Error;

use common::*;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

enum ZipEntry<'a> {
    File(&'a [u8]),
    /// A real unix symlink entry (the target is stored as the entry content).
    Symlink(&'a str),
}

/// Build a zip archive from `(name, entry)` pairs.
fn build_zip(path: &Path, entries: &[(&str, ZipEntry<'_>)]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create zip parent");
    }
    let file = fs::File::create(path).expect("create zip file");
    let mut writer = ZipWriter::new(file);
    for (name, entry) in entries {
        match entry {
            ZipEntry::File(data) => {
                writer
                    .start_file(*name, SimpleFileOptions::default())
                    .unwrap_or_else(|err| panic!("zip entry {name}: {err}"));
                writer.write_all(data).expect("write zip entry");
            }
            ZipEntry::Symlink(target) => {
                // add_symlink sets the S_IFLNK file type bits in the entry mode.
                writer
                    .add_symlink(*name, *target, SimpleFileOptions::default())
                    .unwrap_or_else(|err| panic!("zip symlink entry {name}: {err}"));
            }
        }
    }
    writer.finish().expect("finish zip");
}

fn archive_import(path: &Path) -> ImportRequest {
    ImportRequest {
        source: ImportSource::Archive {
            path: path.to_string_lossy().to_string(),
        },
        skill_path: None,
        version_label: None,
        notes: None,
    }
}

#[test]
fn benign_archive_imports_successfully() {
    let fixture = Fixture::new();
    let zip_path = fixture.project.join("good.zip");
    let skill = skill_md("zipped", "A skill imported from a zip archive.", "zipped body");
    build_zip(
        &zip_path,
        &[("zipped/SKILL.md", ZipEntry::File(skill.as_bytes()))],
    );
    let hub = fixture.hub();

    let imported = hub
        .import_skill(archive_import(&zip_path))
        .expect("benign zip import");
    assert!(imported.created);
    assert_eq!(imported.skill.slug, "zipped");
    assert!(Path::new(&imported.version.storage_path)
        .join("SKILL.md")
        .is_file());
}

#[test]
fn path_traversal_entries_are_rejected() {
    let fixture = Fixture::new();
    let zip_path = fixture.project.join("traversal.zip");
    let skill = skill_md("evil", "Malicious traversal archive.", "body");
    build_zip(
        &zip_path,
        &[
            ("SKILL.md", ZipEntry::File(skill.as_bytes())),
            ("../escape.txt", ZipEntry::File(b"escaped content")),
        ],
    );
    let hub = fixture.hub();

    let err = hub
        .import_skill(archive_import(&zip_path))
        .expect_err("traversal archive must be rejected");
    assert!(
        matches!(err, Error::Blocked(_)),
        "expected Blocked, got {err:?}"
    );
    assert!(
        err.to_string().contains("path traversal"),
        "unexpected error text: {err}"
    );
    assert!(
        find_file_named(&fixture.home, "escape.txt").is_none(),
        "nothing may be written outside the extraction root"
    );
    assert!(
        find_file_named(&fixture.project, "escape.txt").is_none(),
        "nothing may be written outside the extraction root"
    );
    assert!(
        hub.list_skills().expect("list skills").is_empty(),
        "rejected archives must not register a skill"
    );
}

#[test]
fn symlink_entries_are_rejected() {
    let fixture = Fixture::new();
    let zip_path = fixture.project.join("symlink.zip");
    let skill = skill_md("evil-link", "Archive containing a symlink.", "body");
    build_zip(
        &zip_path,
        &[
            ("SKILL.md", ZipEntry::File(skill.as_bytes())),
            ("escapes", ZipEntry::Symlink("/etc/passwd")),
        ],
    );
    let hub = fixture.hub();

    let err = hub
        .import_skill(archive_import(&zip_path))
        .expect_err("symlink archive must be rejected");
    assert!(
        matches!(err, Error::Blocked(_)),
        "expected Blocked, got {err:?}"
    );
    assert!(
        err.to_string().contains("symlink"),
        "unexpected error text: {err}"
    );
    assert!(find_file_named(&fixture.home, "escapes").is_none());
    assert!(find_file_named(&fixture.project, "escapes").is_none());
    assert!(hub.list_skills().expect("list skills").is_empty());
}

#[test]
fn absolute_entry_paths_never_escape_the_extraction_root() {
    let fixture = Fixture::new();
    let zip_path = fixture.project.join("absolute.zip");
    let absolute_name = format!(
        "/agenthub-absolute-escape-{}.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
    let absolute_path = Path::new(&absolute_name);
    assert!(!absolute_path.exists());

    let skill = skill_md("absolute", "Archive containing an absolute path.", "body");
    build_zip(
        &zip_path,
        &[
            ("SKILL.md", ZipEntry::File(skill.as_bytes())),
            (absolute_name.as_str(), ZipEntry::File(b"must not escape")),
        ],
    );
    let hub = fixture.hub();

    // The engine currently sanitises absolute entry names into paths relative
    // to the extraction root instead of rejecting the archive outright. Either
    // outcome is acceptable as long as nothing is written outside the sandbox.
    let outcome = hub.import_skill(archive_import(&zip_path));
    assert!(
        !absolute_path.exists(),
        "absolute archive entry escaped the extraction root to {}",
        absolute_path.display()
    );
    match outcome {
        Ok(imported) => {
            assert!(
                Path::new(&imported.version.storage_path)
                    .join("SKILL.md")
                    .is_file(),
                "a sanitised-but-accepted archive must still store a valid skill"
            );
        }
        Err(err) => {
            assert!(
                matches!(err, Error::Blocked(_)),
                "rejections must be Blocked, got {err:?}"
            );
        }
    }
}
