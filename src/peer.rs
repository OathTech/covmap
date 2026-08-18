//! Checkout-local bindings from durable peer aliases to covmap repositories.

use crate::connection::Connection;
use crate::repo::Repo;
use crate::store::atomic_write;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const SELF: &str = ".";

#[derive(Clone, Debug, Default)]
pub struct Bindings {
    pub entries: BTreeMap<String, String>,
    pub issues: Vec<String>,
}

pub fn valid_alias(alias: &str) -> bool {
    alias == SELF
        || (!alias.is_empty()
            && alias
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric())
            && alias
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
}

pub fn validate_external_alias(alias: &str) -> io::Result<()> {
    if alias == SELF || !valid_alias(alias) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "invalid peer alias '{alias}'; use [A-Za-z0-9][A-Za-z0-9._-]* \
                 ('.' is reserved for self)"
            ),
        ));
    }
    Ok(())
}

pub fn read_bindings(repo: &Repo) -> io::Result<Bindings> {
    let path = repo.peers_file();
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Bindings::default()),
        Err(error) => return Err(error),
    };
    let mut bindings = Bindings::default();
    for (index, line) in raw.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let line_no = index + 1;
        let Some((alias, raw_path)) = line.split_once('\t') else {
            bindings.issues.push(format!(
                "{}:{line_no}: expected <alias> TAB <path>",
                path.display()
            ));
            continue;
        };
        if alias == SELF {
            bindings.issues.push(format!(
                "{}:{line_no}: reserved self peer '.' must not be bound",
                path.display()
            ));
            continue;
        }
        if !valid_alias(alias) {
            bindings.issues.push(format!(
                "{}:{line_no}: invalid peer alias '{alias}'",
                path.display()
            ));
            continue;
        }
        if raw_path.is_empty() || raw_path.contains('\n') || raw_path.contains('\t') {
            bindings.issues.push(format!(
                "{}:{line_no}: invalid path for peer '{alias}'",
                path.display()
            ));
            continue;
        }
        if bindings
            .entries
            .insert(alias.to_string(), raw_path.to_string())
            .is_some()
        {
            bindings.issues.push(format!(
                "{}:{line_no}: duplicate peer alias '{alias}'",
                path.display()
            ));
        }
    }
    Ok(bindings)
}

pub fn serialize_bindings(entries: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for (alias, path) in entries {
        out.push_str(alias);
        out.push('\t');
        out.push_str(path);
        out.push('\n');
    }
    out
}

pub fn write_bindings(repo: &Repo, entries: &BTreeMap<String, String>) -> io::Result<()> {
    atomic_write(&repo.peers_file(), &serialize_bindings(entries))
}

fn binding_path(repo: &Repo, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        repo.root.join(path)
    }
}

pub fn resolve(repo: &Repo, alias: &str) -> io::Result<Repo> {
    if alias == SELF {
        return Ok(repo.clone());
    }
    validate_external_alias(alias)?;
    let bindings = read_bindings(repo)?;
    if let Some(issue) = bindings.issues.first() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid peer bindings: {issue}"),
        ));
    }
    let raw = bindings.entries.get(alias).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("peer '{alias}' is not bound; run covmap peer bind {alias} <path>"),
        )
    })?;
    Repo::discover(&binding_path(repo, raw)).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "peer '{alias}' binding '{}' is unavailable: {error}; \
                 run covmap peer bind {alias} <path>",
                raw
            ),
        )
    })
}

pub fn bind(repo: &Repo, alias: &str, path: &Path) -> io::Result<Repo> {
    validate_external_alias(alias)?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.root.join(path)
    };
    let target = Repo::discover(&candidate)?;
    let raw = target.root.to_string_lossy().to_string();
    if raw.contains('\t') || raw.contains('\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "peer binding path may not contain a tab or newline",
        ));
    }
    let bindings = read_bindings(repo)?;
    if let Some(issue) = bindings.issues.first() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid peer bindings: {issue}"),
        ));
    }
    let mut entries = bindings.entries;
    entries.insert(alias.to_string(), raw);
    write_bindings(repo, &entries)?;
    Ok(target)
}

pub fn unbind(repo: &Repo, alias: &str) -> io::Result<bool> {
    validate_external_alias(alias)?;
    let bindings = read_bindings(repo)?;
    if let Some(issue) = bindings.issues.first() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid peer bindings: {issue}"),
        ));
    }
    let mut entries = bindings.entries;
    let removed = entries.remove(alias).is_some();
    if removed {
        write_bindings(repo, &entries)?;
    }
    Ok(removed)
}

pub fn referenced_aliases(repo: &Repo) -> io::Result<BTreeSet<String>> {
    let mut aliases = BTreeSet::new();
    for name in Connection::list(repo)? {
        let connection = Connection::read(repo, &name)?;
        aliases.insert(connection.target_peer);
    }
    Ok(aliases)
}

pub fn read_rename_journal(repo: &Repo) -> io::Result<Option<(String, String)>> {
    let path = repo.peer_rename_file();
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let line = raw.trim_end();
    let (old, new) = line.split_once('\t').ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("malformed peer rename journal at {}", path.display()),
        )
    })?;
    Ok(Some((old.to_string(), new.to_string())))
}

pub fn rename(repo: &Repo, requested_old: &str, requested_new: &str) -> io::Result<usize> {
    validate_external_alias(requested_old)?;
    if requested_new != SELF {
        validate_external_alias(requested_new)?;
    }
    if requested_old == requested_new {
        return Ok(0);
    }

    let journal = read_rename_journal(repo)?;
    let operation = match journal.clone() {
        Some(existing) if existing == (requested_old.to_string(), requested_new.to_string()) => {
            existing
        }
        Some((old, new)) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("peer rename {old} -> {new} is unfinished; resume that operation first"),
            ))
        }
        None => (requested_old.to_string(), requested_new.to_string()),
    };
    let (old, new) = operation;

    let bindings = read_bindings(repo)?;
    if let Some(issue) = bindings.issues.first() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid peer bindings: {issue}"),
        ));
    }
    let mut entries = bindings.entries;
    let old_path = entries.get(&old).cloned();

    if journal.is_none() {
        let old_path = old_path.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("peer '{old}' is not bound"),
            )
        })?;
        let referenced = referenced_aliases(repo)?;
        if new != SELF && (entries.contains_key(&new) || referenced.contains(&new)) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("peer alias '{new}' is already in use"),
            ));
        }
        if new == SELF {
            let target = Repo::discover(&binding_path(repo, old_path))?;
            if target.root != repo.root {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "peer '{old}' resolves to {}, not the current repository",
                        target.root.display()
                    ),
                ));
            }
        }
        atomic_write(&repo.peer_rename_file(), &format!("{old}\t{new}\n"))?;
    }

    if new != SELF && !entries.contains_key(&new) {
        let path = old_path.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("cannot resume peer rename {old} -> {new}: neither alias has a binding"),
            )
        })?;
        entries.insert(new.clone(), path);
        write_bindings(repo, &entries)?;
    }

    let mut rewritten = 0usize;
    for name in Connection::list(repo)? {
        let mut connection = Connection::read(repo, &name)?;
        if connection.target_peer == old {
            connection.target_peer = new.clone();
            connection.write(repo)?;
            rewritten += 1;
        }
    }

    let bindings = read_bindings(repo)?;
    let mut entries = bindings.entries;
    entries.remove(&old);
    write_bindings(repo, &entries)?;
    match fs::remove_file(repo.peer_rename_file()) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(rewritten)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn roots(tag: &str) -> (Repo, Repo) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base =
            std::env::temp_dir().join(format!("covmap-peer-{tag}-{}-{nonce}", std::process::id()));
        let source = base.join("source");
        let target = base.join("target");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        (Repo::init(&source).unwrap(), Repo::init(&target).unwrap())
    }

    #[test]
    fn binding_roundtrip_resolves_relative_to_source_root() {
        let (source, target) = roots("bind");
        let bound = bind(&source, "impl", Path::new("../target")).unwrap();

        assert_eq!(bound.root, target.root);
        assert_eq!(resolve(&source, "impl").unwrap().root, target.root);
        assert!(unbind(&source, "impl").unwrap());
        assert_eq!(
            resolve(&source, "impl").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        fs::remove_dir_all(source.root.parent().unwrap()).unwrap();
    }

    #[test]
    fn rename_rewrites_connections_and_resumes_from_a_journal() {
        let (source, target) = roots("rename");
        bind(&source, "old", &target.root).unwrap();
        Connection {
            name: "map".to_string(),
            source_covering: "main".to_string(),
            target_peer: "old".to_string(),
            target_covering: "main".to_string(),
            links: Vec::new(),
        }
        .write(&source)
        .unwrap();
        atomic_write(&source.peer_rename_file(), "old\tnew\n").unwrap();

        let rewritten = rename(&source, "old", "new").unwrap();

        assert_eq!(rewritten, 1);
        assert_eq!(Connection::read(&source, "map").unwrap().target_peer, "new");
        let bindings = read_bindings(&source).unwrap();
        assert!(!bindings.entries.contains_key("old"));
        assert_eq!(
            bindings.entries.get("new"),
            Some(&target.root.to_string_lossy().to_string())
        );
        assert!(!source.peer_rename_file().exists());
        fs::remove_dir_all(source.root.parent().unwrap()).unwrap();
    }

    #[test]
    fn rename_finishes_after_refs_and_bindings_were_already_published() {
        let (source, target) = roots("rename-cleanup");
        bind(&source, "new", &target.root).unwrap();
        Connection {
            name: "map".to_string(),
            source_covering: "main".to_string(),
            target_peer: "new".to_string(),
            target_covering: "main".to_string(),
            links: Vec::new(),
        }
        .write(&source)
        .unwrap();
        atomic_write(&source.peer_rename_file(), "old\tnew\n").unwrap();

        let rewritten = rename(&source, "old", "new").unwrap();

        assert_eq!(rewritten, 0);
        assert_eq!(Connection::read(&source, "map").unwrap().target_peer, "new");
        assert!(!source.peer_rename_file().exists());
        fs::remove_dir_all(source.root.parent().unwrap()).unwrap();
    }

    #[test]
    fn rename_to_self_canonicalizes_refs_and_removes_the_binding() {
        let (source, _) = roots("rename-self");
        bind(&source, "legacy-self", &source.root).unwrap();
        Connection {
            name: "map".to_string(),
            source_covering: "main".to_string(),
            target_peer: "legacy-self".to_string(),
            target_covering: "main".to_string(),
            links: Vec::new(),
        }
        .write(&source)
        .unwrap();

        let rewritten = rename(&source, "legacy-self", ".").unwrap();

        assert_eq!(rewritten, 1);
        assert_eq!(Connection::read(&source, "map").unwrap().target_peer, ".");
        assert!(!read_bindings(&source)
            .unwrap()
            .entries
            .contains_key("legacy-self"));
        assert!(!source.peer_rename_file().exists());
        fs::remove_dir_all(source.root.parent().unwrap()).unwrap();
    }

    #[test]
    fn malformed_bindings_are_retained_as_issues() {
        let (source, _) = roots("invalid");
        fs::write(
            source.peers_file(),
            ".\t/tmp/self\nbad alias\t/tmp/other\nmissing-tab\n",
        )
        .unwrap();

        let bindings = read_bindings(&source).unwrap();

        assert_eq!(bindings.issues.len(), 3);
        fs::remove_dir_all(source.root.parent().unwrap()).unwrap();
    }
}
