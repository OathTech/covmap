//! Reconstruction and hydration of content-addressed segment objects.

use crate::covering::CoveringEntry;
use crate::repo::Repo;
use crate::segment::{self, Segment};
use crate::walk;
use std::collections::BTreeMap;
use std::io;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HydrateOutcome {
    Present,
    Written,
}

/// Read one checked-out source file for verification or reconstruction.
pub fn source_lines(repo: &Repo, file: &str) -> io::Result<Vec<String>> {
    let path = repo.root.join(file);
    let lines = match walk::read_text_lines(&path) {
        Ok(Some(lines)) => lines,
        Ok(None) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("source '{file}' is not UTF-8"),
            ))
        }
        Err(error) => {
            return Err(io::Error::new(
                error.kind(),
                format!("cannot read source '{file}': {error}"),
            ))
        }
    };
    Ok(lines)
}

/// Extract one entry from source lines that have already been read, then
/// verify that it still has the entry's recorded content hash.
pub fn source_content_from_lines(lines: &[String], entry: &CoveringEntry) -> io::Result<String> {
    let content = segment::extract_content(lines, entry.a, entry.a_col, entry.b, entry.b_col)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("source range {} no longer fits", entry.loc()),
            )
        })?;
    let actual = segment::hash_of(&content);
    if actual != entry.hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "source drift: recorded hash {}, current hash {}",
                entry.hash, actual
            ),
        ));
    }
    Ok(content)
}

/// Reconstruct one covering entry from the checked-out source and verify that
/// it still has the entry's recorded content hash.
pub fn source_content(repo: &Repo, entry: &CoveringEntry) -> io::Result<String> {
    let lines = source_lines(repo, &entry.file)?;
    source_content_from_lines(&lines, entry)
}

fn install_content(
    repo: &Repo,
    entry: &CoveringEntry,
    content: String,
) -> io::Result<HydrateOutcome> {
    match segment::read_object(&repo.objects_dir(), &entry.hash) {
        Ok(_) => Ok(HydrateOutcome::Present),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let segment = Segment {
                file: entry.file.clone(),
                a: entry.a,
                b: entry.b,
                label: entry.label.clone(),
                content,
            };
            let written = segment::write_object(&repo.objects_dir(), &segment)?;
            if written != entry.hash {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "hydrated object hash changed for {}: expected {}, got {}",
                        entry.loc(),
                        entry.hash,
                        written
                    ),
                ));
            }
            Ok(HydrateOutcome::Written)
        }
        Err(error) => Err(error),
    }
}

/// Verify or create one live cache object from source lines that have already
/// been read.
pub fn hydrate_entry_from_lines(
    repo: &Repo,
    entry: &CoveringEntry,
    lines: &[String],
) -> io::Result<HydrateOutcome> {
    let content = source_content_from_lines(lines, entry)?;
    install_content(repo, entry, content)
}

/// Verify or create one live cache object from matching checked-out source.
#[cfg(test)]
pub fn hydrate_entry(repo: &Repo, entry: &CoveringEntry) -> io::Result<HydrateOutcome> {
    let lines = source_lines(repo, &entry.file)?;
    hydrate_entry_from_lines(repo, entry, &lines)
}

fn recorded_bytes_unavailable(entry: &CoveringEntry, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!(
            "recorded bytes unavailable for {}: {error}; restore a source revision matching \
             the recorded hash, run `covmap hydrate`, then return to this edit",
            entry.loc()
        ),
    )
}

/// Read recorded bytes, reconstructing a missing live object only when source
/// still reproduces its recorded hash.
pub fn read_recorded_content(repo: &Repo, entry: &CoveringEntry) -> io::Result<String> {
    match segment::read_object(&repo.objects_dir(), &entry.hash) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let content = source_content(repo, entry)
                .map_err(|error| recorded_bytes_unavailable(entry, error))?;
            let segment = Segment {
                file: entry.file.clone(),
                a: entry.a,
                b: entry.b,
                label: entry.label.clone(),
                content: content.clone(),
            };
            let written = segment::write_object(&repo.objects_dir(), &segment)?;
            if written != entry.hash {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "reconstructed object hash changed for {}: expected {}, got {}",
                        entry.loc(),
                        entry.hash,
                        written
                    ),
                ));
            }
            Ok(content)
        }
        Err(error) => Err(error),
    }
}

/// Ensure every selected iterator task has recorded bytes. Existing valid
/// cache objects remain authoritative after source drift; only misses are
/// grouped by source file and reconstructed from one read of that file.
pub fn ensure_recorded_entries(repo: &Repo, entries: &[&CoveringEntry]) -> io::Result<()> {
    let mut missing_by_file: BTreeMap<&str, Vec<&CoveringEntry>> = BTreeMap::new();
    for entry in entries {
        match segment::read_object(&repo.objects_dir(), &entry.hash) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing_by_file
                    .entry(entry.file.as_str())
                    .or_default()
                    .push(entry);
            }
            Err(error) => return Err(error),
        }
    }

    for (file, missing) in missing_by_file {
        let lines = source_lines(repo, file)
            .map_err(|error| recorded_bytes_unavailable(missing[0], error))?;
        let mut verified = Vec::with_capacity(missing.len());
        for entry in missing {
            let content = source_content_from_lines(&lines, entry)
                .map_err(|error| recorded_bytes_unavailable(entry, error))?;
            verified.push((entry, content));
        }
        for (entry, content) in verified {
            install_content(repo, entry, content)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::covering::CoveringEntry;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture(tag: &str) -> (Repo, CoveringEntry) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("covmap-cache-{tag}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(root.join(".covmap/objects")).unwrap();
        fs::write(root.join("source.txt"), "alpha\nbeta\n").unwrap();
        let content = "alpha\nbeta\n";
        (
            Repo { root },
            CoveringEntry {
                hash: segment::hash_of(content),
                file: "source.txt".to_string(),
                a: 1,
                a_col: None,
                b: 2,
                b_col: None,
                label: "unknown".to_string(),
                name: None,
                message: None,
            },
        )
    }

    #[test]
    fn hydrate_reconstructs_a_missing_object_idempotently() {
        let (repo, entry) = fixture("hydrate");

        assert_eq!(
            hydrate_entry(&repo, &entry).unwrap(),
            HydrateOutcome::Written
        );
        assert_eq!(
            hydrate_entry(&repo, &entry).unwrap(),
            HydrateOutcome::Present
        );
        assert_eq!(
            segment::read_object(&repo.objects_dir(), &entry.hash).unwrap(),
            "alpha\nbeta\n"
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn source_verification_requires_source_even_with_a_valid_cache() {
        let (repo, entry) = fixture("source-required");
        assert_eq!(
            hydrate_entry(&repo, &entry).unwrap(),
            HydrateOutcome::Written
        );
        fs::remove_file(repo.root.join("source.txt")).unwrap();

        let error = source_content(&repo, &entry).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(error
            .to_string()
            .contains("cannot read source 'source.txt'"));
        assert_eq!(
            segment::read_object(&repo.objects_dir(), &entry.hash).unwrap(),
            "alpha\nbeta\n"
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn hydration_does_not_replace_a_corrupt_object() {
        let (repo, entry) = fixture("corrupt");
        let path = segment::object_path(&repo.objects_dir(), &entry.hash);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "corrupt\n").unwrap();

        let error = hydrate_entry(&repo, &entry).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("cache corruption"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "corrupt\n");
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn reconstruction_rejects_drift() {
        let (repo, entry) = fixture("drift");
        fs::write(repo.root.join("source.txt"), "changed\nbeta\n").unwrap();

        let error = source_content(&repo, &entry).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("source drift"));
        assert!(!segment::object_path(&repo.objects_dir(), &entry.hash).exists());
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn recorded_content_lazily_hydrates_clean_source() {
        let (repo, entry) = fixture("recorded");

        let content = read_recorded_content(&repo, &entry).unwrap();

        assert_eq!(content, "alpha\nbeta\n");
        assert!(segment::object_path(&repo.objects_dir(), &entry.hash).exists());
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn recorded_content_rejects_drift_when_cache_is_missing() {
        let (repo, entry) = fixture("recorded-drift");
        fs::write(repo.root.join("source.txt"), "changed\nbeta\n").unwrap();

        let error = read_recorded_content(&repo, &entry).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("source drift"));
        assert!(error
            .to_string()
            .contains("restore a source revision matching the recorded hash"));
        assert!(error.to_string().contains("run `covmap hydrate`"));
        assert!(!segment::object_path(&repo.objects_dir(), &entry.hash).exists());
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn recorded_content_prefers_a_valid_cache_after_source_drifts() {
        let (repo, entry) = fixture("recorded-cache");
        assert_eq!(
            hydrate_entry(&repo, &entry).unwrap(),
            HydrateOutcome::Written
        );
        fs::write(repo.root.join("source.txt"), "changed\nbeta\n").unwrap();

        let content = read_recorded_content(&repo, &entry).unwrap();

        assert_eq!(content, "alpha\nbeta\n");
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn iterator_retention_hydrates_all_missing_entries_from_one_file() {
        let (repo, mut first) = fixture("iterator-clean");
        first.b = 1;
        first.hash = segment::hash_of("alpha\n");
        let second = CoveringEntry {
            hash: segment::hash_of("beta\n"),
            file: first.file.clone(),
            a: 2,
            a_col: None,
            b: 2,
            b_col: None,
            label: "unknown".to_string(),
            name: None,
            message: None,
        };

        ensure_recorded_entries(&repo, &[&first, &second]).unwrap();

        assert_eq!(
            segment::read_object(&repo.objects_dir(), &first.hash).unwrap(),
            "alpha\n"
        );
        assert_eq!(
            segment::read_object(&repo.objects_dir(), &second.hash).unwrap(),
            "beta\n"
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn iterator_retention_rejects_a_drifted_cache_miss_with_recovery() {
        let (repo, entry) = fixture("iterator-drift");
        fs::write(repo.root.join("source.txt"), "changed\nbeta\n").unwrap();

        let error = ensure_recorded_entries(&repo, &[&entry]).unwrap_err();

        assert!(error.to_string().contains("recorded bytes unavailable"));
        assert!(error.to_string().contains("run `covmap hydrate`"));
        assert!(!segment::object_path(&repo.objects_dir(), &entry.hash).exists());
        fs::remove_dir_all(repo.root).unwrap();
    }
}
