//! On-disk format migration orchestration and staged rewrite machinery.

use crate::connection::{Connection, LegacyConnection};
use crate::covering::Covering;
use crate::iterator::Iter;
use crate::peer;
use crate::repo::Repo;
use crate::segment;
use crate::walk;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) fn run(args: &[String]) -> io::Result<u8> {
    let mut recoveries = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--recover-covering" => {
                let name = args.get(index + 1).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "migrate: --recover-covering needs a legacy name",
                    )
                })?;
                recoveries.push(name.clone());
                index += 2;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("migrate: unexpected argument '{other}'"),
                ))
            }
        }
    }

    let repo = Repo::discover_unchecked(&env::current_dir()?)?;
    let mut version = repo.read_version().unwrap_or_default();
    if !matches!(
        version.as_str(),
        "1" | "2" | "3" | "4" | "5" | FORMAT4_TO_5_TRANSITION
    ) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "cannot migrate from format '{version}' \
                 (expected '1', '2', '3', '4', '5', or '{FORMAT4_TO_5_TRANSITION}')"
            ),
        ));
    }
    if matches!(version.as_str(), "4" | "5" | FORMAT4_TO_5_TRANSITION) && !recoveries.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "migrate: --recover-covering applies only to legacy formats",
        ));
    }
    if version == "5" {
        ensure_format4_gitignore(&repo)?;
        cleanup_format4_to_5(&repo)?;
        cleanup_published_migration(&repo)?;
        cleanup_name_migration(&repo)?;
        println!("repository format is 5 - no version migration needed");
        return Ok(0);
    }
    if version == FORMAT4_TO_5_TRANSITION {
        publish_format4_to_5(&repo)?;
        finish_terminal_migration(&repo)?;
        println!("resumed migration and published repository format 5");
        println!("run `covmap fsck` to confirm every partition and endpoint resolves");
        return Ok(0);
    }
    if version == "4" {
        migrate_v4_to_v5(&repo)?;
        finish_terminal_migration(&repo)?;
        println!("run `covmap fsck` to confirm every partition and endpoint resolves");
        return Ok(0);
    }

    normalize_legacy_covering_names(&repo, &recoveries)?;
    match version.as_str() {
        "3" => {
            if cleanup_published_migration(&repo)? {
                println!("format 3 migration was already published; removed stale staging state");
            }
        }
        "1" => {
            migrate_v1_to_v2(&repo)?;
            version = "2".to_string();
        }
        "2" => {}
        _ => unreachable!(),
    }
    if version == "2" {
        migrate_v2_to_v3(&repo)?;
    }
    migrate_v3_to_v4(&repo)?;
    migrate_v4_to_v5(&repo)?;
    finish_terminal_migration(&repo)?;
    println!("run `covmap fsck` to confirm every partition and endpoint resolves");
    Ok(0)
}

const FORMAT4_TO_5_TRANSITION: &str = "migrating-4-to-5";

fn finish_terminal_migration(repo: &Repo) -> io::Result<()> {
    ensure_format4_gitignore(repo)?;
    cleanup_published_migration(repo)?;
    cleanup_name_migration(repo)?;
    cleanup_format4_to_5(repo)?;
    Ok(())
}

const ESCAPED_COVERING_PREFIX: &str = "covmap-escaped-";

#[derive(Clone, Debug, Default)]
struct NameMigrationManifest {
    recoveries: BTreeSet<String>,
    mappings: BTreeMap<String, String>,
    sources: BTreeMap<String, String>,
    selections: BTreeSet<String>,
}

fn migrated_covering_name(name: &str) -> String {
    if crate::repo::valid_covering_name(name) && !name.starts_with(ESCAPED_COVERING_PREFIX) {
        return name.to_string();
    }
    let digest = Sha256::digest(name.as_bytes());
    let mut escaped = String::with_capacity(ESCAPED_COVERING_PREFIX.len() + digest.len() * 2);
    escaped.push_str(ESCAPED_COVERING_PREFIX);
    for byte in digest {
        escaped.push_str(&format!("{byte:02x}"));
    }
    escaped
}

fn name_migration_dir(repo: &Repo) -> PathBuf {
    repo.local_dir().join("migrate-covering-names")
}

fn name_migration_manifest(repo: &Repo) -> PathBuf {
    name_migration_dir(repo).join("manifest")
}

fn cleanup_name_migration(repo: &Repo) -> io::Result<bool> {
    let path = name_migration_dir(repo);
    if path.exists() {
        fs::remove_dir_all(path)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn hex_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn hex_decode(value: &str) -> io::Result<String> {
    if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid hex field in migration manifest",
        ));
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for offset in (0..value.len()).step_by(2) {
        bytes.push(
            u8::from_str_radix(&value[offset..offset + 2], 16).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid hex field in migration manifest",
                )
            })?,
        );
    }
    String::from_utf8(bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "non-UTF-8 field in migration manifest",
        )
    })
}

fn serialize_name_migration_manifest(manifest: &NameMigrationManifest) -> String {
    let mut out = String::from("ready\n");
    for recovery in &manifest.recoveries {
        out.push_str(&format!("recover\t{}\n", hex_encode(recovery)));
    }
    for (old, new) in &manifest.mappings {
        out.push_str(&format!("map\t{}\t{new}\n", hex_encode(old)));
    }
    for (old, source) in &manifest.sources {
        out.push_str(&format!(
            "source\t{}\t{}\n",
            hex_encode(old),
            hex_encode(source)
        ));
    }
    for selection in &manifest.selections {
        out.push_str(&format!("selection\t{}\n", hex_encode(selection)));
    }
    out
}

fn read_name_migration_manifest(repo: &Repo) -> io::Result<Option<NameMigrationManifest>> {
    let path = name_migration_manifest(repo);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut lines = raw.lines();
    if lines.next() != Some("ready") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "invalid covering-name migration manifest at {}",
                path.display()
            ),
        ));
    }
    let mut manifest = NameMigrationManifest::default();
    for line in lines {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["recover", old] => {
                manifest.recoveries.insert(hex_decode(old)?);
            }
            ["map", old, new] if crate::repo::valid_covering_name(new) => {
                manifest
                    .mappings
                    .insert(hex_decode(old)?, (*new).to_string());
            }
            ["source", old, source] => {
                manifest
                    .sources
                    .insert(hex_decode(old)?, hex_decode(source)?);
            }
            ["selection", old] => {
                manifest.selections.insert(hex_decode(old)?);
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid covering-name migration manifest record '{line}'"),
                ))
            }
        }
    }
    Ok(Some(manifest))
}

fn normalize_legacy_covering_names(repo: &Repo, recoveries: &[String]) -> io::Result<()> {
    if let Some(manifest) = read_name_migration_manifest(repo)? {
        let supplied: BTreeSet<String> = recoveries.iter().cloned().collect();
        if !supplied.is_empty() && supplied != manifest.recoveries {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "migrate: recovery arguments differ from the ready migration manifest; \
                 resume without arguments or repeat the same set",
            ));
        }
        println!("resuming staged legacy covering-name normalization");
        publish_name_migration(repo, &manifest)?;
        return Ok(());
    }

    let manifest = stage_name_migration(repo, recoveries)?;
    if manifest.mappings.iter().all(|(old, new)| old == new) && manifest.recoveries.is_empty() {
        cleanup_name_migration(repo)?;
        return Ok(());
    }
    publish_name_migration(repo, &manifest)
}

fn stage_name_migration(repo: &Repo, recoveries: &[String]) -> io::Result<NameMigrationManifest> {
    let root = name_migration_dir(repo);
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    let staged = root.join("staged");
    fs::create_dir_all(staged.join("coverings"))?;
    fs::create_dir_all(staged.join("connections"))?;
    fs::create_dir_all(staged.join("iterators"))?;
    fs::create_dir_all(staged.join("selections"))?;

    let mut manifest = NameMigrationManifest::default();
    let mut local_names = BTreeSet::new();

    let raw_head = fs::read_to_string(repo.head_file())?;
    let head_name = raw_head.trim_end_matches(&['\r', '\n'][..]).to_string();
    if head_name.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "legacy HEAD contains an empty covering name",
        ));
    }
    manifest.mappings.insert(head_name.clone(), String::new());
    local_names.insert(head_name);

    let connection_files = legacy_regular_files(&repo.connections_dir(), false)?;
    for (name, path) in &connection_files {
        let raw = fs::read_to_string(path)?;
        if let Some(source) = stored_metadata_value(&raw, "# source_covering=") {
            manifest.mappings.insert(source.clone(), String::new());
            local_names.insert(source);
        }
        if let Some(target) = stored_metadata_value(&raw, "# target_covering=") {
            manifest.mappings.insert(target, String::new());
        }
        crate::store::atomic_write(&staged.join("connections").join(name), &raw)?;
    }

    let iterator_files = legacy_regular_files(&repo.iterators_dir(), false)?;
    for (name, path) in &iterator_files {
        let raw = fs::read_to_string(path)?;
        if let Some(covering) = stored_metadata_value(&raw, "# covering=") {
            manifest.mappings.insert(covering.clone(), String::new());
            local_names.insert(covering);
        }
        crate::store::atomic_write(&staged.join("iterators").join(name), &raw)?;
    }

    for recovery in recoveries {
        if migrated_covering_name(recovery) == *recovery {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "migrate: recovery name '{recovery}' is already a normal format-4 covering name"
                ),
            ));
        }
        manifest.recoveries.insert(recovery.clone());
        manifest.mappings.insert(recovery.clone(), String::new());
        local_names.insert(recovery.clone());
    }

    let direct_coverings = legacy_regular_files(&repo.coverings_dir(), true)?;
    for (name, path) in direct_coverings {
        manifest.mappings.insert(name.clone(), String::new());
        manifest.sources.insert(
            name,
            relative_to_dot(repo, &path)?.to_string_lossy().to_string(),
        );
    }

    inventory_legacy_selections(repo, &mut manifest)?;

    for name in &local_names {
        if manifest.sources.contains_key(name) {
            continue;
        }
        if let Some(path) = legacy_covering_source(repo, name)? {
            manifest.sources.insert(
                name.clone(),
                relative_to_dot(repo, &path)?.to_string_lossy().to_string(),
            );
        } else if manifest.recoveries.contains(name) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("migrate: recovered covering '{name}' has no safe legacy ref"),
            ));
        }
    }
    validate_nested_covering_sources(repo, &manifest)?;

    let names: Vec<String> = manifest.mappings.keys().cloned().collect();
    let mut outputs = BTreeMap::<String, String>::new();
    for old in names {
        let new = migrated_covering_name(&old);
        crate::repo::validate_covering_name(&new)?;
        if let Some(previous) = outputs.insert(new.clone(), old.clone()) {
            if previous != old {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "covering-name migration collision: '{previous}' and '{old}' both map to '{new}'"
                    ),
                ));
            }
        }
        manifest.mappings.insert(old, new);
    }

    let mut claimed_sources = BTreeMap::<String, String>::new();
    for (old, source) in &manifest.sources {
        if let Some(previous) = claimed_sources.insert(source.clone(), old.clone()) {
            if previous != *old {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "legacy covering source '{}' is claimed by both '{}' and '{}'",
                        repo.dot().join(source).display(),
                        previous,
                        old
                    ),
                ));
            }
        }
        let raw = fs::read_to_string(repo.dot().join(source))?;
        let new = &manifest.mappings[old];
        Covering::parse(new, &raw).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "legacy covering '{}' at {} is not a valid covering ref: {error}",
                    old,
                    repo.dot().join(source).display()
                ),
            )
        })?;
        crate::store::atomic_write(&staged.join("coverings").join(new), &raw)?;
    }

    stage_rewritten_metadata(repo, &staged, &manifest, &connection_files, &iterator_files)?;
    crate::store::atomic_write(
        &name_migration_manifest(repo),
        &serialize_name_migration_manifest(&manifest),
    )?;
    Ok(manifest)
}

fn legacy_regular_files(dir: &Path, allow_directories: bool) -> io::Result<Vec<(String, PathBuf)>> {
    let metadata = match fs::symlink_metadata(dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "legacy artifact path is not a regular directory: {}",
                dir.display()
            ),
        ));
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().into_string().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "legacy artifact name is not valid UTF-8: {}",
                    entry.path().display()
                ),
            )
        })?;
        if crate::store::is_atomic_temp_name(&name) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("migration refuses symbolic link {}", entry.path().display()),
            ));
        }
        if metadata.is_file() {
            files.push((name, entry.path()));
        } else if !allow_directories || !metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unexpected legacy artifact entry {}",
                    entry.path().display()
                ),
            ));
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

fn stored_metadata_value(raw: &str, prefix: &str) -> Option<String> {
    raw.lines()
        .find_map(|line| line.strip_prefix(prefix).map(str::to_string))
}

fn relative_to_dot<'a>(repo: &Repo, path: &'a Path) -> io::Result<&'a Path> {
    path.strip_prefix(repo.dot()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "migration source escapes {}: {}",
                repo.dot().display(),
                path.display()
            ),
        )
    })
}

fn legacy_covering_source(repo: &Repo, name: &str) -> io::Result<Option<PathBuf>> {
    use std::path::Component;

    let mut components = vec![std::ffi::OsString::from("coverings")];
    for component in Path::new(name).components() {
        match component {
            Component::Normal(value) => components.push(value.to_os_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                if components.pop().is_none() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("legacy covering '{name}' resolves outside .covmap"),
                    ));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("legacy covering '{name}' is an absolute path"),
                ))
            }
        }
    }
    if components.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("legacy covering '{name}' resolves outside .covmap"),
        ));
    }
    if components.len() == 1 {
        let reserved = components[0].to_string_lossy();
        if matches!(
            reserved.as_ref(),
            "version"
                | "HEAD"
                | "include"
                | "ignore"
                | ".gitignore"
                | "objects"
                | "connections"
                | "iterators"
                | "local"
                | "selections"
        ) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "legacy covering '{name}' resolves to reserved metadata {}",
                    repo.dot().join(&components[0]).display()
                ),
            ));
        }
    }

    let mut path = repo.dot();
    for (offset, component) in components.iter().enumerate() {
        path.push(component);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if metadata.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("migration refuses symbolic link {}", path.display()),
            ));
        }
        let final_component = offset + 1 == components.len();
        if (!final_component && !metadata.is_dir()) || (final_component && !metadata.is_file()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "legacy covering source is not a regular file: {}",
                    path.display()
                ),
            ));
        }
    }
    Ok(Some(path))
}

fn inventory_legacy_selections(
    repo: &Repo,
    manifest: &mut NameMigrationManifest,
) -> io::Result<()> {
    let root = repo.selections_dir();
    let metadata = match fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "selection root is not a regular directory: {}",
                root.display()
            ),
        ));
    }
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let name = entry.file_name().into_string().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "selection directory name is not valid UTF-8: {}",
                    entry.path().display()
                ),
            )
        })?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "selection entry is not a regular directory: {}",
                    entry.path().display()
                ),
            ));
        }
        for child in fs::read_dir(entry.path())? {
            let child = child?;
            if !matches!(child.file_name().to_str(), Some("include" | "ignore")) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "unexpected selection policy entry {}",
                        child.path().display()
                    ),
                ));
            }
            let child_metadata = fs::symlink_metadata(child.path())?;
            if child_metadata.file_type().is_symlink() || !child_metadata.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "selection policy is not a regular file: {}",
                        child.path().display()
                    ),
                ));
            }
            fs::read_to_string(child.path()).map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!(
                        "cannot read selection policy {}: {error}",
                        child.path().display()
                    ),
                )
            })?;
        }
        manifest.mappings.insert(name.clone(), String::new());
        manifest.selections.insert(name);
    }
    Ok(())
}

fn collect_covering_tree_files(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("migration refuses symbolic link {}", entry.path().display()),
            ));
        }
        if metadata.is_dir() {
            collect_covering_tree_files(&entry.path(), out)?;
        } else if metadata.is_file() {
            let name = entry.file_name();
            if !name.to_str().is_some_and(crate::store::is_atomic_temp_name) {
                out.push(entry.path());
            }
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected covering artifact {}", entry.path().display()),
            ));
        }
    }
    Ok(())
}

fn validate_nested_covering_sources(
    repo: &Repo,
    manifest: &NameMigrationManifest,
) -> io::Result<()> {
    let claimed: BTreeSet<String> = manifest.sources.values().cloned().collect();
    let mut files = Vec::new();
    collect_covering_tree_files(&repo.coverings_dir(), &mut files)?;
    for file in files {
        let relative = relative_to_dot(repo, &file)?.to_string_lossy().to_string();
        if !claimed.contains(&relative) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "legacy covering path {} has no unambiguous covering identity; \
                     reference it from HEAD/metadata or pass --recover-covering",
                    file.display()
                ),
            ));
        }
    }
    Ok(())
}

fn rewrite_metadata_fields(
    raw: &str,
    prefixes: &[&str],
    mappings: &BTreeMap<String, String>,
) -> io::Result<String> {
    let mut out = String::new();
    for line in raw.lines() {
        let mut rewritten = false;
        for prefix in prefixes {
            if let Some(value) = line.strip_prefix(prefix) {
                let mapped = mappings.get(value).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("migration inventory omitted covering name '{value}'"),
                    )
                })?;
                out.push_str(prefix);
                out.push_str(mapped);
                out.push('\n');
                rewritten = true;
                break;
            }
        }
        if !rewritten {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !raw.ends_with('\n') {
        out.pop();
    }
    Ok(out)
}

fn stage_rewritten_metadata(
    repo: &Repo,
    staged: &Path,
    manifest: &NameMigrationManifest,
    connection_files: &[(String, PathBuf)],
    iterator_files: &[(String, PathBuf)],
) -> io::Result<()> {
    let version = repo.read_version()?;
    let raw = fs::read_to_string(repo.head_file())?;
    let old = raw.trim_end_matches(&['\r', '\n'][..]);
    let new = manifest.mappings.get(old).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("migration inventory omitted HEAD covering '{old}'"),
        )
    })?;
    crate::store::atomic_write(&staged.join("HEAD"), &format!("{new}\n"))?;
    for (name, path) in connection_files {
        let raw = fs::read_to_string(path)?;
        let rewritten = rewrite_metadata_fields(
            &raw,
            &["# source_covering=", "# target_covering="],
            &manifest.mappings,
        )?;
        match version.as_str() {
            "1" | "2" => {
                LegacyConnection::parse(name, &rewritten)?;
            }
            "3" => {
                Connection::parse_format4(name, &rewritten)?;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("cannot stage covering-name migration for format '{other}'"),
                ))
            }
        }
        crate::store::atomic_write(&staged.join("connections").join(name), &rewritten)?;
    }
    for (name, path) in iterator_files {
        let raw = fs::read_to_string(path)?;
        let rewritten = rewrite_metadata_fields(&raw, &["# covering="], &manifest.mappings)?;
        Iter::parse_format4(name, &rewritten)?;
        crate::store::atomic_write(&staged.join("iterators").join(name), &rewritten)?;
    }
    for old in &manifest.selections {
        let new = &manifest.mappings[old];
        let source = repo.selections_dir().join(old);
        let target = staged.join("selections").join(new);
        fs::create_dir_all(&target)?;
        for policy in ["include", "ignore"] {
            let path = source.join(policy);
            match fs::read_to_string(&path) {
                Ok(raw) => crate::store::atomic_write(&target.join(policy), &raw)?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}

fn publish_name_migration(repo: &Repo, manifest: &NameMigrationManifest) -> io::Result<()> {
    let staged = name_migration_dir(repo).join("staged");
    publish_staged_directory(&staged.join("coverings"), &repo.coverings_dir())?;
    publish_staged_directory(&staged.join("connections"), &repo.connections_dir())?;
    publish_staged_directory(&staged.join("iterators"), &repo.iterators_dir())?;
    if staged.join("HEAD").is_file() {
        let head = fs::read_to_string(staged.join("HEAD"))?;
        repo.write_head(head.trim_end_matches(&['\r', '\n'][..]))?;
    }
    if staged.join("selections").is_dir() {
        for entry in fs::read_dir(staged.join("selections"))? {
            let entry = entry?;
            let name = entry.file_name();
            let target = repo.selections_dir().join(&name);
            fs::create_dir_all(&target)?;
            publish_staged_directory(&entry.path(), &target)?;
        }
    }

    for (old, source) in &manifest.sources {
        let target = PathBuf::from("coverings").join(&manifest.mappings[old]);
        if Path::new(source) != target {
            let path = repo.dot().join(source);
            if path.exists() {
                fs::remove_file(&path)?;
                remove_empty_covering_parents(repo, path.parent())?;
            }
        }
    }
    for old in &manifest.selections {
        if manifest.mappings[old] != *old {
            let path = repo.selections_dir().join(old);
            if path.exists() {
                fs::remove_dir_all(path)?;
            }
        }
    }

    for (old, new) in &manifest.mappings {
        if old != new {
            println!("covering name '{old}' -> '{new}'");
        }
    }
    fs::remove_dir_all(name_migration_dir(repo))?;
    Ok(())
}

fn remove_empty_covering_parents(repo: &Repo, mut path: Option<&Path>) -> io::Result<()> {
    let root = repo.coverings_dir();
    while let Some(current) = path {
        if current == root || !current.starts_with(&root) {
            break;
        }
        match fs::remove_dir(current) {
            Ok(()) => path = current.parent(),
            Err(error) => {
                if fs::read_dir(current)?.next().is_some() {
                    break;
                }
                return Err(error);
            }
        }
    }
    Ok(())
}

fn cleanup_published_migration(repo: &Repo) -> io::Result<bool> {
    let path = migration_dir(repo);
    if path.exists() {
        fs::remove_dir_all(path)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Upgrade format 1 to the legacy format-2 state expected by the portable
/// artifact migration.
fn migrate_v1_to_v2(repo: &Repo) -> io::Result<()> {
    // old segment-hash -> new content-only hash, learned from the coverings
    // (the only place we can recover a segment's content + old label).
    let mut remap: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let objects = repo.objects_dir();
    let mut objects_written = 0usize;

    for cname in Covering::list(repo)? {
        let mut c = Covering::read(repo, &cname)?;
        for e in &mut c.entries {
            // Read the v1 object body (`label \0 content`) and recover the
            // pure content, then re-key it under the content-only hash.
            let content = read_v1_content(&objects, &e.hash)?;
            let new_hash = segment::hash_of(&content);
            if new_hash != e.hash {
                remap.insert(e.hash.clone(), new_hash.clone());
            }
            let seg = crate::segment::Segment {
                file: e.file.clone(),
                a: e.a,
                b: e.b,
                label: e.label.clone(),
                content,
            };
            segment::write_object(&objects, &seg)?;
            objects_written += 1;
            e.hash = new_hash;
        }
        c.write(repo)?;
    }

    // Rewrite bare-hash endpoints in connections. `@name` endpoints and
    // endpoints into a not-yet-migrated target repo are left untouched
    // (the latter resolve again once that repo is migrated).
    let mut conn_eps = 0usize;
    for kname in Connection::list(repo)? {
        let mut k = LegacyConnection::read(repo, &kname)?;
        for l in &mut k.links {
            if let Some(n) = remap.get(&l.a) {
                l.a = n.clone();
                conn_eps += 1;
            }
            if let Some(n) = remap.get(&l.b) {
                l.b = n.clone();
                conn_eps += 1;
            }
        }
        k.write(repo)?;
    }

    // Rewrite frozen task hashes in iterators (the cursor is an index, not
    // a hash, so it needs no rewrite).
    let mut iter_tasks = 0usize;
    for iname in crate::iterator::Iter::list(repo)? {
        let mut it = crate::iterator::Iter::read_format4(repo, &iname)?;
        for t in &mut it.tasks {
            if let Some(n) = remap.get(&t.hash) {
                t.hash = n.clone();
                iter_tasks += 1;
            }
        }
        it.write_format4(repo)?;
    }

    crate::store::atomic_write(&repo.dot().join("version"), "2\n")?;
    println!(
        "migrated to format 2: {} object(s) re-keyed ({} hash change(s)), \
         {} connection endpoint(s) and {} iterator task(s) rewritten",
        objects_written,
        remap.len(),
        conn_eps,
        iter_tasks
    );
    Ok(())
}

fn ensure_default_gitignore(repo: &Repo) -> io::Result<()> {
    let path = repo.dot().join(".gitignore");
    match fs::read_to_string(&path) {
        Ok(existing) => warn_gitignore_policy(&existing),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            crate::store::atomic_write(&path, crate::repo::DEFAULT_GITIGNORE)
        }
        Err(error) => Err(error),
    }
}

fn warn_gitignore_policy(existing: &str) -> io::Result<()> {
    for required in [
        "/local/",
        "/objects/",
        "/iterators/",
        "/*.html",
        r"\#covmap-tmp-*.tmp",
    ] {
        if !existing.lines().any(|line| line.trim() == required) {
            eprintln!("warning: existing .covmap/.gitignore does not exclude '{required}'");
        }
    }
    if existing.lines().any(|line| line.trim() == "*.tmp") {
        eprintln!(
            "warning: existing .covmap/.gitignore pattern '*.tmp' can hide valid project refs; \
             replace it with '\\#covmap-tmp-*.tmp'"
        );
    }
    Ok(())
}

fn ensure_format4_gitignore(repo: &Repo) -> io::Result<()> {
    let path = repo.dot().join(".gitignore");
    match fs::read_to_string(&path) {
        Ok(existing)
            if matches!(
                existing.as_str(),
                crate::repo::FORMAT3_DEFAULT_GITIGNORE | crate::repo::BROAD_TMP_DEFAULT_GITIGNORE
            ) =>
        {
            crate::store::atomic_write(&path, crate::repo::DEFAULT_GITIGNORE)?;
            println!("updated generated .covmap/.gitignore atomic-temp rule");
            Ok(())
        }
        Ok(existing) => warn_gitignore_policy(&existing),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            crate::store::atomic_write(&path, crate::repo::DEFAULT_GITIGNORE)
        }
        Err(error) => Err(error),
    }
}

fn legacy_target_is_self(repo: &Repo, target: &str) -> bool {
    let current = fs::canonicalize(&repo.root).unwrap_or_else(|_| repo.root.clone());
    fs::canonicalize(target)
        .ok()
        .is_some_and(|path| path == current)
        || Repo::discover_unchecked(Path::new(target))
            .ok()
            .is_some_and(|target_repo| target_repo.root == current)
}

fn alias_seed(target: &str) -> String {
    let basename = Path::new(target)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "peer".to_string());
    let mut alias: String = basename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    while !alias.is_empty()
        && !alias
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
    {
        alias.remove(0);
    }
    if alias.is_empty() || alias == peer::SELF {
        "peer".to_string()
    } else {
        alias
    }
}

fn unique_alias(seed: &str, used: &mut std::collections::BTreeSet<String>) -> String {
    if used.insert(seed.to_string()) {
        return seed.to_string();
    }
    let mut suffix = 2usize;
    loop {
        let candidate = format!("{seed}-{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn migration_dir(repo: &Repo) -> PathBuf {
    repo.local_dir().join("migrate-2-to-3")
}

fn migration_manifest(repo: &Repo) -> PathBuf {
    migration_dir(repo).join("manifest")
}

fn read_migration_manifest(repo: &Repo) -> io::Result<Option<Vec<(String, String)>>> {
    let path = migration_manifest(repo);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut lines = raw.lines();
    if lines.next() != Some("ready") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid migration manifest at {}", path.display()),
        ));
    }
    let mut aliases = Vec::new();
    for line in lines {
        let (alias, path) = line.split_once('\t').ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid migration manifest record '{line}'"),
            )
        })?;
        aliases.push((alias.to_string(), path.to_string()));
    }
    Ok(Some(aliases))
}

fn stage_v2_to_v3(repo: &Repo) -> io::Result<Vec<(String, String)>> {
    ensure_default_gitignore(repo)?;
    let root = migration_dir(repo);
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    let staged = root.join("staged");
    fs::create_dir_all(staged.join("coverings"))?;
    fs::create_dir_all(staged.join("connections"))?;
    fs::create_dir_all(staged.join("local"))?;

    let mut legacy = Vec::new();
    let mut targets = std::collections::BTreeSet::new();
    let mut unavailable_targets = std::collections::BTreeSet::new();
    for name in Connection::list(repo)? {
        let connection = LegacyConnection::read(repo, &name)?;
        if !legacy_target_is_self(repo, &connection.target_repo) {
            if connection.target_repo.contains('\t') || connection.target_repo.contains('\n') {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "legacy target path for connection '{}' contains a tab or newline",
                        connection.name
                    ),
                ));
            }
            if Repo::discover_unchecked(Path::new(&connection.target_repo)).is_err() {
                unavailable_targets.insert(connection.target_repo.clone());
            }
            targets.insert(connection.target_repo.clone());
        }
        legacy.push(connection);
    }

    let existing = peer::read_bindings(repo)?;
    if let Some(issue) = existing.issues.first() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid existing peer bindings: {issue}"),
        ));
    }
    let mut bindings = existing.entries;
    let mut used: std::collections::BTreeSet<String> = bindings.keys().cloned().collect();
    let mut by_target = std::collections::BTreeMap::new();
    let mut generated = Vec::new();
    for target in targets {
        let alias = unique_alias(&alias_seed(&target), &mut used);
        bindings.insert(alias.clone(), target.clone());
        by_target.insert(target.clone(), alias.clone());
        if unavailable_targets.contains(&target) {
            eprintln!(
                "warning: legacy target '{}' is unavailable; migrated it as peer '{}'. \
                 If it denotes this repository, run `covmap peer bind {} .` then \
                 `covmap peer rename {} .`",
                target, alias, alias, alias
            );
        }
        generated.push((alias, target));
    }

    for name in Covering::list(repo)? {
        let covering = Covering::read(repo, &name)?;
        crate::store::atomic_write(&staged.join("coverings").join(name), &covering.serialize())?;
    }
    for connection in legacy {
        let target_peer = if legacy_target_is_self(repo, &connection.target_repo) {
            peer::SELF.to_string()
        } else {
            by_target[&connection.target_repo].clone()
        };
        let connection = Connection {
            name: connection.name,
            source_covering: connection.source_covering,
            target_peer,
            target_covering: connection.target_covering,
            links: connection.links,
        };
        crate::store::atomic_write(
            &staged.join("connections").join(&connection.name),
            &connection.serialize_format4(),
        )?;
    }
    crate::store::atomic_write(
        &staged.join("local/peers"),
        &peer::serialize_bindings(&bindings),
    )?;

    let mut manifest = String::from("ready\n");
    for (alias, path) in &generated {
        manifest.push_str(&format!("{alias}\t{path}\n"));
    }
    crate::store::atomic_write(&migration_manifest(repo), &manifest)?;
    Ok(generated)
}

fn publish_staged_directory(from: &Path, to: &Path) -> io::Result<()> {
    let mut names = Vec::new();
    if from.is_dir() {
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                names.push(entry.file_name());
            }
        }
    }
    names.sort();
    for name in names {
        let content = fs::read_to_string(from.join(&name))?;
        crate::store::atomic_write(&to.join(name), &content)?;
    }
    Ok(())
}

fn publish_v2_to_v3(repo: &Repo) -> io::Result<()> {
    let staged = migration_dir(repo).join("staged");
    publish_staged_directory(&staged.join("coverings"), &repo.coverings_dir())?;
    publish_staged_directory(&staged.join("connections"), &repo.connections_dir())?;
    let peers = fs::read_to_string(staged.join("local/peers"))?;
    crate::store::atomic_write(&repo.peers_file(), &peers)?;
    crate::store::atomic_write(&repo.dot().join("version"), "3\n")?;
    fs::remove_dir_all(migration_dir(repo))?;
    Ok(())
}

fn migrate_v2_to_v3(repo: &Repo) -> io::Result<()> {
    let generated = match read_migration_manifest(repo)? {
        Some(generated) => {
            println!("resuming staged format 2 -> 3 migration");
            generated
        }
        None => stage_v2_to_v3(repo)?,
    };
    publish_v2_to_v3(repo)?;
    println!("migrated to format 3: portable peer refs and canonical Git-trackable artifacts");
    for (alias, path) in generated {
        println!("  peer {alias} -> {path}");
    }
    println!(
        "note: ignore rules do not untrack indexed files; if needed run:\n  \
         git rm -r --cached --ignore-unmatch .covmap/objects .covmap/iterators .covmap/local"
    );
    Ok(())
}

fn migrate_v3_to_v4(repo: &Repo) -> io::Result<()> {
    let version = repo.read_version()?;
    if version != "3" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("format 3 -> 4 migration expected version 3, found '{version}'"),
        ));
    }

    repo.read_head()?;
    for name in Covering::list(repo)? {
        Covering::read(repo, &name)?;
    }
    for name in Connection::list(repo)? {
        Connection::read_format4(repo, &name)?;
    }
    for name in Iter::list(repo)? {
        Iter::read_format4(repo, &name)?;
    }
    let policy_issues = walk::layout_issues(repo);
    if !policy_issues.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "cannot activate format 4 with invalid selection policy:\n  {}",
                policy_issues.join("\n  ")
            ),
        ));
    }

    ensure_format4_gitignore(repo)?;
    fs::create_dir_all(repo.selections_dir())?;
    crate::store::atomic_write(&repo.dot().join("version"), "4\n")?;
    println!(
        "migrated to format 4: covering-input selection is tracked; \
         existing ignore-only discovery behavior is unchanged"
    );
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Format4To5Manifest {
    connections: BTreeSet<String>,
    iterators: BTreeSet<String>,
}

fn format4_to_5_dir(repo: &Repo) -> PathBuf {
    repo.local_dir().join("migrate-4-to-5")
}

fn format4_to_5_manifest(repo: &Repo) -> PathBuf {
    format4_to_5_dir(repo).join("manifest")
}

fn cleanup_format4_to_5(repo: &Repo) -> io::Result<bool> {
    let path = format4_to_5_dir(repo);
    if path.exists() {
        fs::remove_dir_all(path)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn serialize_format4_to_5_manifest(manifest: &Format4To5Manifest) -> String {
    let mut out = String::from("ready\n");
    for name in &manifest.connections {
        out.push_str(&format!("connection\t{}\n", hex_encode(name)));
    }
    for name in &manifest.iterators {
        out.push_str(&format!("iterator\t{}\n", hex_encode(name)));
    }
    out
}

fn read_format4_to_5_manifest(repo: &Repo) -> io::Result<Format4To5Manifest> {
    let path = format4_to_5_manifest(repo);
    let raw = fs::read_to_string(&path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unfinished format migration has no ready manifest at {}; \
                     restore the migration journal or recover from version control",
                    path.display()
                ),
            )
        } else {
            error
        }
    })?;
    let mut lines = raw.lines();
    if lines.next() != Some("ready") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid format 4 -> 5 manifest at {}", path.display()),
        ));
    }
    let mut manifest = Format4To5Manifest::default();
    for line in lines {
        let fields: Vec<&str> = line.split('\t').collect();
        let (kind, encoded) = match fields.as_slice() {
            [kind @ ("connection" | "iterator"), encoded] => (*kind, *encoded),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid format 4 -> 5 manifest record '{line}'"),
                ))
            }
        };
        let name = hex_decode(encoded)?;
        let inserted = match kind {
            "connection" => manifest.connections.insert(name),
            "iterator" => manifest.iterators.insert(name),
            _ => unreachable!(),
        };
        if !inserted {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("duplicate format 4 -> 5 manifest record '{line}'"),
            ));
        }
    }
    Ok(manifest)
}

fn regular_file_names(dir: &Path) -> io::Result<BTreeSet<String>> {
    Ok(legacy_regular_files(dir, false)?
        .into_iter()
        .map(|(name, _)| name)
        .collect())
}

fn require_staged_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "format 4 -> 5 staged snapshot is missing directory {}: {error}",
                path.display()
            ),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "format 4 -> 5 staged snapshot path is not a regular directory: {}",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn prepare_format4_to_5(repo: &Repo) -> io::Result<Format4To5Manifest> {
    let root = format4_to_5_dir(repo);
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    let staged = root.join("staged");
    fs::create_dir_all(staged.join("connections"))?;
    fs::create_dir_all(staged.join("iterators"))?;

    let mut manifest = Format4To5Manifest::default();
    for (name, path) in legacy_regular_files(&repo.connections_dir(), false)? {
        let raw = fs::read_to_string(&path)?;
        let connection = Connection::parse_format4(&name, &raw)?;
        crate::store::atomic_write(
            &staged.join("connections").join(&name),
            &connection.serialize(),
        )?;
        manifest.connections.insert(name);
    }
    for (name, path) in legacy_regular_files(&repo.iterators_dir(), false)? {
        let raw = fs::read_to_string(&path)?;
        let iterator = Iter::parse_format4(&name, &raw)?;
        crate::store::atomic_write(&staged.join("iterators").join(&name), &iterator.serialize())?;
        manifest.iterators.insert(name);
    }
    crate::store::atomic_write(
        &format4_to_5_manifest(repo),
        &serialize_format4_to_5_manifest(&manifest),
    )?;
    Ok(manifest)
}

fn validate_format4_to_5_snapshot(repo: &Repo, manifest: &Format4To5Manifest) -> io::Result<()> {
    let staged = format4_to_5_dir(repo).join("staged");
    let staged_connections = staged.join("connections");
    let staged_iterators = staged.join("iterators");
    require_staged_directory(&staged_connections)?;
    require_staged_directory(&staged_iterators)?;
    let connection_names = regular_file_names(&staged_connections)?;
    let iterator_names = regular_file_names(&staged_iterators)?;
    if connection_names != manifest.connections || iterator_names != manifest.iterators {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "format 4 -> 5 staged snapshot does not match its ready manifest",
        ));
    }
    for name in &manifest.connections {
        let raw = fs::read_to_string(staged.join("connections").join(name))?;
        Connection::parse(name, &raw)?;
    }
    for name in &manifest.iterators {
        let raw = fs::read_to_string(staged.join("iterators").join(name))?;
        Iter::parse(name, &raw)?;
    }
    Ok(())
}

fn publish_staged_ref(path: &Path, raw: &str) -> io::Result<()> {
    match fs::read_to_string(path) {
        Ok(live) if live == raw => Ok(()),
        Ok(_) => crate::store::atomic_write(path, raw),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            crate::store::atomic_write(path, raw)
        }
        Err(error) => Err(error),
    }
}

fn activate_format4_to_5(repo: &Repo) -> io::Result<()> {
    if repo.read_version()? != "4" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "format 4 -> 5 activation requires published version 4",
        ));
    }
    let manifest = read_format4_to_5_manifest(repo)?;
    validate_format4_to_5_snapshot(repo, &manifest)?;
    crate::store::atomic_write(
        &repo.dot().join("version"),
        &format!("{FORMAT4_TO_5_TRANSITION}\n"),
    )
}

fn publish_format4_to_5(repo: &Repo) -> io::Result<()> {
    if repo.read_version()? != FORMAT4_TO_5_TRANSITION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("format 4 -> 5 publication requires version '{FORMAT4_TO_5_TRANSITION}'"),
        ));
    }
    let manifest = read_format4_to_5_manifest(repo)?;
    validate_format4_to_5_snapshot(repo, &manifest)?;
    let staged = format4_to_5_dir(repo).join("staged");
    for name in &manifest.connections {
        let raw = fs::read_to_string(staged.join("connections").join(name))?;
        publish_staged_ref(&repo.connections_dir().join(name), &raw)?;
    }
    for name in &manifest.iterators {
        let raw = fs::read_to_string(staged.join("iterators").join(name))?;
        publish_staged_ref(&repo.iterators_dir().join(name), &raw)?;
    }
    crate::store::atomic_write(&repo.dot().join("version"), "5\n")?;
    fs::remove_dir_all(format4_to_5_dir(repo))?;
    Ok(())
}

fn migrate_v4_to_v5(repo: &Repo) -> io::Result<()> {
    let version = repo.read_version()?;
    match version.as_str() {
        "4" => {
            // A ready snapshot under version 4 was never activated and may be
            // stale because an older binary could have changed live refs.
            cleanup_format4_to_5(repo)?;
            prepare_format4_to_5(repo)?;
            activate_format4_to_5(repo)?;
        }
        FORMAT4_TO_5_TRANSITION => {}
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("format 4 -> 5 migration expected version 4, found '{other}'"),
            ))
        }
    }
    publish_format4_to_5(repo)?;
    println!("migrated to format 5: link and iterator notes use multiline-safe adjacent stanzas");
    Ok(())
}

/// Read a format-1 object body and return its content, stripping the
/// leading `label \0` prefix. Format-1 objects always contain a NUL; if
/// one is somehow absent, treat the whole body as content.
fn read_v1_content(objects_dir: &std::path::Path, hash: &str) -> io::Result<String> {
    let path = segment::object_path(objects_dir, hash);
    let buf = fs::read(&path)?;
    let content = match buf.iter().position(|&b| b == 0) {
        Some(nul) => &buf[nul + 1..],
        None => &buf[..],
    };
    Ok(String::from_utf8_lossy(content).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::Link;
    use crate::covering::CoveringEntry;
    use crate::segment::HASH_LEN;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_repo(tag: &str) -> Repo {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "covmap-migrate-{tag}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join(".covmap")).unwrap();
        Repo { root }
    }

    fn initialize_test_repo(repo: &Repo) {
        fs::create_dir_all(repo.objects_dir()).unwrap();
        fs::create_dir_all(repo.coverings_dir()).unwrap();
        fs::create_dir_all(repo.connections_dir()).unwrap();
        fs::write(repo.head_file(), "main\n").unwrap();
    }

    fn format2_repo(tag: &str) -> Repo {
        let repo = temp_repo(tag);
        initialize_test_repo(&repo);
        fs::create_dir_all(repo.iterators_dir()).unwrap();
        fs::write(repo.dot().join("version"), "2\n").unwrap();
        fs::write(repo.root.join("file.txt"), "alpha\n").unwrap();
        let entry = CoveringEntry {
            hash: segment::hash_of("alpha\n"),
            file: "file.txt".to_string(),
            a: 1,
            a_col: None,
            b: 1,
            b_col: None,
            label: "mapped".to_string(),
            name: Some("alpha".to_string()),
            message: None,
        };
        Covering {
            name: "main".to_string(),
            entries: vec![entry],
        }
        .write(&repo)
        .unwrap();
        repo
    }

    fn format4_repo(tag: &str) -> Repo {
        let repo = format2_repo(tag);
        fs::create_dir_all(repo.local_dir()).unwrap();
        fs::create_dir_all(repo.selections_dir()).unwrap();
        fs::write(repo.dot().join("version"), "4\n").unwrap();
        repo
    }

    #[test]
    fn format2_migration_is_resumable_and_canonicalizes_self() {
        let repo = format2_repo("migrate-resume");
        LegacyConnection {
            name: "self".to_string(),
            source_covering: "main".to_string(),
            target_repo: repo.root.to_string_lossy().to_string(),
            target_covering: "main".to_string(),
            links: vec![Link {
                a: "@alpha".to_string(),
                b: "@alpha".to_string(),
                note: "self".to_string(),
            }],
        }
        .write(&repo)
        .unwrap();

        stage_v2_to_v3(&repo).unwrap();
        let staged =
            fs::read_to_string(migration_dir(&repo).join("staged/connections/self")).unwrap();
        fs::write(repo.connections_dir().join("self"), staged).unwrap();
        migrate_v2_to_v3(&repo).unwrap();

        assert_eq!(repo.read_version().unwrap(), "3");
        assert_eq!(
            Connection::read_format4(&repo, "self").unwrap().target_peer,
            "."
        );
        assert_eq!(
            fs::read_to_string(repo.dot().join(".gitignore")).unwrap(),
            crate::repo::DEFAULT_GITIGNORE
        );
        assert!(!migration_dir(&repo).exists());
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn format2_migration_assigns_deterministic_alias_collisions() {
        let repo = format2_repo("migrate-aliases");
        for (name, target) in [("first", "/tmp/a/api"), ("second", "/tmp/b/api")] {
            LegacyConnection {
                name: name.to_string(),
                source_covering: "main".to_string(),
                target_repo: target.to_string(),
                target_covering: "main".to_string(),
                links: Vec::new(),
            }
            .write(&repo)
            .unwrap();
        }

        migrate_v2_to_v3(&repo).unwrap();

        assert_eq!(
            Connection::read_format4(&repo, "first")
                .unwrap()
                .target_peer,
            "api"
        );
        assert_eq!(
            Connection::read_format4(&repo, "second")
                .unwrap()
                .target_peer,
            "api-2"
        );
        let bindings = peer::read_bindings(&repo).unwrap();
        assert_eq!(
            bindings.entries.get("api").map(String::as_str),
            Some("/tmp/a/api")
        );
        assert_eq!(
            bindings.entries.get("api-2").map(String::as_str),
            Some("/tmp/b/api")
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn format2_migration_preserves_custom_gitignore() {
        let repo = format2_repo("migrate-ignore");
        let custom = "# project policy\n/objects/\n";
        fs::write(repo.dot().join(".gitignore"), custom).unwrap();

        migrate_v2_to_v3(&repo).unwrap();

        assert_eq!(
            fs::read_to_string(repo.dot().join(".gitignore")).unwrap(),
            custom
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn format1_migration_chains_through_format5() {
        use sha2::{Digest, Sha256};

        let repo = temp_repo("migrate-chain");
        initialize_test_repo(&repo);
        fs::create_dir_all(repo.iterators_dir()).unwrap();
        fs::write(repo.dot().join("version"), "1\n").unwrap();
        fs::write(repo.root.join("file.txt"), "alpha\n").unwrap();
        let label = "mapped";
        let content = "alpha\n";
        let mut digest = Sha256::new();
        digest.update(label.as_bytes());
        digest.update([0]);
        digest.update(content.as_bytes());
        let old_hash = digest
            .finalize()
            .iter()
            .take(HASH_LEN / 2)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let old_path = segment::object_path(&repo.objects_dir(), &old_hash);
        fs::create_dir_all(old_path.parent().unwrap()).unwrap();
        fs::write(&old_path, format!("{label}\0{content}")).unwrap();
        Covering {
            name: "main".to_string(),
            entries: vec![CoveringEntry {
                hash: old_hash.clone(),
                file: "file.txt".to_string(),
                a: 1,
                a_col: None,
                b: 1,
                b_col: None,
                label: label.to_string(),
                name: None,
                message: None,
            }],
        }
        .write(&repo)
        .unwrap();
        LegacyConnection {
            name: "self".to_string(),
            source_covering: "main".to_string(),
            target_repo: repo.root.to_string_lossy().to_string(),
            target_covering: "main".to_string(),
            links: vec![Link {
                a: old_hash.clone(),
                b: old_hash,
                note: String::new(),
            }],
        }
        .write(&repo)
        .unwrap();

        migrate_v1_to_v2(&repo).unwrap();
        migrate_v2_to_v3(&repo).unwrap();
        migrate_v3_to_v4(&repo).unwrap();
        migrate_v4_to_v5(&repo).unwrap();

        let new_hash = segment::hash_of(content);
        assert_eq!(repo.read_version().unwrap(), "5");
        assert_eq!(
            Covering::read(&repo, "main").unwrap().entries[0].hash,
            new_hash
        );
        assert_eq!(
            segment::read_object(&repo.objects_dir(), &new_hash).unwrap(),
            content
        );
        let connection = Connection::read(&repo, "self").unwrap();
        assert_eq!(connection.target_peer, ".");
        assert_eq!(connection.links[0].a, new_hash);
        assert_eq!(connection.links[0].b, new_hash);
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn covering_name_migration_is_deterministic_and_reserves_its_prefix() {
        let escaped = migrated_covering_name("foo:bar");
        assert!(escaped.starts_with(ESCAPED_COVERING_PREFIX));
        assert_eq!(escaped.len(), ESCAPED_COVERING_PREFIX.len() + 64);
        assert_eq!(escaped, migrated_covering_name("foo:bar"));
        assert_eq!(migrated_covering_name("ordinary"), "ordinary");
        assert_ne!(
            migrated_covering_name(&format!("{ESCAPED_COVERING_PREFIX}ordinary")),
            format!("{ESCAPED_COVERING_PREFIX}ordinary")
        );
    }

    #[test]
    fn format3_migration_rewrites_unsafe_names_across_all_artifacts() {
        let repo = temp_repo("migrate-unsafe-name");
        initialize_test_repo(&repo);
        fs::create_dir_all(repo.iterators_dir()).unwrap();
        fs::create_dir_all(repo.selections_dir().join("foo:bar")).unwrap();
        fs::write(repo.dot().join("version"), "3\n").unwrap();
        fs::write(repo.head_file(), "foo:bar\n").unwrap();
        fs::write(repo.coverings_dir().join("foo:bar"), "").unwrap();
        fs::write(
            repo.connections_dir().join("self"),
            "# source_covering=foo:bar\n# target_peer=.\n# target_covering=foo:bar\n",
        )
        .unwrap();
        fs::write(
            repo.iterators_dir().join("work"),
            "# covering=foo:bar\n# order=file\n",
        )
        .unwrap();
        fs::write(repo.selections_dir().join("foo:bar/include"), "src/\n").unwrap();

        normalize_legacy_covering_names(&repo, &[]).unwrap();
        let escaped = migrated_covering_name("foo:bar");

        assert_eq!(repo.read_head().unwrap(), escaped);
        assert!(repo.covering_path(&escaped).unwrap().is_file());
        assert!(!repo.coverings_dir().join("foo:bar").exists());
        let connection = Connection::read_format4(&repo, "self").unwrap();
        assert_eq!(connection.source_covering, escaped);
        assert_eq!(connection.target_covering, escaped);
        assert_eq!(Iter::read_format4(&repo, "work").unwrap().covering, escaped);
        assert_eq!(
            fs::read_to_string(repo.selection_dir(&escaped).unwrap().join("include")).unwrap(),
            "src/\n"
        );
        assert!(!repo.selections_dir().join("foo:bar").exists());

        migrate_v3_to_v4(&repo).unwrap();
        assert_eq!(repo.read_version().unwrap(), "4");
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn migration_recovery_repairs_an_unreferenced_parent_traversal_ref() {
        let repo = temp_repo("migrate-recovery");
        initialize_test_repo(&repo);
        fs::create_dir_all(repo.iterators_dir()).unwrap();
        fs::write(repo.dot().join("version"), "3\n").unwrap();
        fs::write(repo.dot().join("evil"), "").unwrap();

        let manifest = stage_name_migration(&repo, &["../evil".to_string()]).unwrap();
        assert!(manifest.recoveries.contains("../evil"));
        normalize_legacy_covering_names(&repo, &[]).unwrap();

        let escaped = migrated_covering_name("../evil");
        assert!(repo.covering_path(&escaped).unwrap().is_file());
        assert!(!repo.dot().join("evil").exists());
        assert!(!name_migration_dir(&repo).exists());
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn migration_repairs_a_referenced_parent_traversal_ref() {
        let repo = temp_repo("migrate-referenced-traversal");
        initialize_test_repo(&repo);
        fs::create_dir_all(repo.iterators_dir()).unwrap();
        fs::write(repo.dot().join("version"), "3\n").unwrap();
        fs::write(repo.head_file(), "../evil\n").unwrap();
        fs::write(repo.dot().join("evil"), "").unwrap();

        normalize_legacy_covering_names(&repo, &[]).unwrap();

        let escaped = migrated_covering_name("../evil");
        assert_eq!(repo.read_head().unwrap(), escaped);
        assert!(repo.covering_path(&escaped).unwrap().is_file());
        assert!(!repo.dot().join("evil").exists());
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn migration_recovery_refuses_paths_outside_covmap() {
        let repo = temp_repo("migrate-recovery-outside");
        initialize_test_repo(&repo);
        fs::create_dir_all(repo.iterators_dir()).unwrap();
        fs::write(repo.dot().join("version"), "3\n").unwrap();

        let error = stage_name_migration(&repo, &["../../outside".to_string()])
            .unwrap_err()
            .to_string();
        assert!(error.contains("outside .covmap"));
        assert_eq!(repo.read_version().unwrap(), "3");
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn format3_name_validation_failure_does_not_publish_staged_rewrites() {
        let repo = temp_repo("migrate-validation-atomic");
        initialize_test_repo(&repo);
        fs::create_dir_all(repo.iterators_dir()).unwrap();
        fs::write(repo.dot().join("version"), "3\n").unwrap();
        fs::write(repo.head_file(), "foo:bar\n").unwrap();
        fs::write(repo.coverings_dir().join("foo:bar"), "").unwrap();
        fs::write(
            repo.connections_dir().join("broken"),
            "# source_covering=foo:bar\n# target_covering=foo:bar\n",
        )
        .unwrap();

        let error = normalize_legacy_covering_names(&repo, &[])
            .unwrap_err()
            .to_string();
        let escaped = migrated_covering_name("foo:bar");

        assert!(error.contains("missing required field 'target_peer'"));
        assert_eq!(fs::read_to_string(repo.head_file()).unwrap(), "foo:bar\n");
        assert!(repo.coverings_dir().join("foo:bar").is_file());
        assert!(!repo.covering_path(&escaped).unwrap().exists());
        assert_eq!(repo.read_version().unwrap(), "3");
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn format3_to_format4_preserves_pre_authored_selection_policy() {
        let repo = format2_repo("migrate-selection");
        fs::write(repo.dot().join("version"), "3\n").unwrap();
        fs::write(
            repo.dot().join(".gitignore"),
            crate::repo::FORMAT3_DEFAULT_GITIGNORE,
        )
        .unwrap();
        fs::create_dir_all(repo.selection_dir("main").unwrap()).unwrap();
        fs::write(repo.dot().join("include"), "*.md\n").unwrap();
        fs::write(repo.dot().join("ignore"), "vendor/\n").unwrap();
        fs::write(
            repo.selection_dir("main").unwrap().join("include"),
            "src/\n",
        )
        .unwrap();

        normalize_legacy_covering_names(&repo, &[]).unwrap();
        migrate_v3_to_v4(&repo).unwrap();

        assert_eq!(repo.read_version().unwrap(), "4");
        assert_eq!(
            fs::read_to_string(repo.dot().join("include")).unwrap(),
            "*.md\n"
        );
        assert_eq!(
            fs::read_to_string(repo.dot().join("ignore")).unwrap(),
            "vendor/\n"
        );
        assert_eq!(
            fs::read_to_string(repo.selection_dir("main").unwrap().join("include")).unwrap(),
            "src/\n"
        );
        assert_eq!(
            fs::read_to_string(repo.dot().join(".gitignore")).unwrap(),
            crate::repo::DEFAULT_GITIGNORE
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn format4_migrate_repairs_the_broad_generated_tmp_rule() {
        let repo = format2_repo("migrate-format4-gitignore");
        fs::write(repo.dot().join("version"), "4\n").unwrap();
        fs::write(
            repo.dot().join(".gitignore"),
            crate::repo::BROAD_TMP_DEFAULT_GITIGNORE,
        )
        .unwrap();

        ensure_format4_gitignore(&repo).unwrap();

        assert_eq!(
            fs::read_to_string(repo.dot().join(".gitignore")).unwrap(),
            crate::repo::DEFAULT_GITIGNORE
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn format4_migrate_preserves_a_custom_gitignore() {
        let repo = format2_repo("migrate-format4-custom-gitignore");
        fs::write(repo.dot().join("version"), "4\n").unwrap();
        let custom = "# project policy\n/objects/\n*.tmp\n";
        fs::write(repo.dot().join(".gitignore"), custom).unwrap();

        ensure_format4_gitignore(&repo).unwrap();

        assert_eq!(
            fs::read_to_string(repo.dot().join(".gitignore")).unwrap(),
            custom
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn published_format3_migration_cleanup_is_idempotent() {
        let repo = format2_repo("migrate-cleanup");
        fs::write(repo.dot().join("version"), "3\n").unwrap();
        fs::create_dir_all(migration_dir(&repo).join("staged")).unwrap();
        fs::write(migration_manifest(&repo), "ready\n").unwrap();

        assert!(cleanup_published_migration(&repo).unwrap());
        assert!(!migration_dir(&repo).exists());
        assert!(!cleanup_published_migration(&repo).unwrap());
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn format4_to_5_preserves_inline_notes_and_publishes_stanzas() {
        let repo = format4_repo("format5-notes");
        fs::write(
            repo.connections_dir().join("self"),
            "# source_covering=main\n\
             # target_peer=.\n\
             # target_covering=main\n\
             @alpha\t@alpha\tlink\twith-tab\n",
        )
        .unwrap();
        fs::write(
            repo.iterators_dir().join("work"),
            "# covering=main\n\
             # order=file\n\
             # cursor=0\n\
             abc\tfile.txt\t1\t1\tmapped\talpha\tdone\ttask\twith-tab\n",
        )
        .unwrap();

        migrate_v4_to_v5(&repo).unwrap();

        assert_eq!(repo.read_version().unwrap(), "5");
        let connection = Connection::read(&repo, "self").unwrap();
        assert_eq!(connection.links[0].note, "link\twith-tab");
        let iterator = Iter::read(&repo, "work").unwrap();
        assert_eq!(iterator.tasks[0].note, "task\twith-tab");
        assert!(fs::read_to_string(repo.connections_dir().join("self"))
            .unwrap()
            .contains("# note 1\nlink\twith-tab\n"));
        assert!(fs::read_to_string(repo.iterators_dir().join("work"))
            .unwrap()
            .contains("# note 1\ntask\twith-tab\n"));
        assert!(!format4_to_5_dir(&repo).exists());
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn format4_to_5_without_notes_keeps_ref_bytes() {
        let repo = format4_repo("format5-no-notes");
        let connection = "# source_covering=main\n\
                          # target_peer=.\n\
                          # target_covering=main\n\
                          @alpha\t@alpha\n";
        let iterator = "# covering=main\n\
                        # order=file\n\
                        abc\tfile.txt\t1\t1\tmapped\talpha\tpending\n";
        fs::write(repo.connections_dir().join("self"), connection).unwrap();
        fs::write(repo.iterators_dir().join("work"), iterator).unwrap();

        migrate_v4_to_v5(&repo).unwrap();

        assert_eq!(
            fs::read_to_string(repo.connections_dir().join("self")).unwrap(),
            connection
        );
        assert_eq!(
            fs::read_to_string(repo.iterators_dir().join("work")).unwrap(),
            iterator
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn malformed_format4_line_blocks_migration_before_mutation() {
        let repo = format4_repo("format5-malformed");
        let raw = "# source_covering=main\n\
                   # target_peer=.\n\
                   # target_covering=main\n\
                   not-a-record\n";
        fs::write(repo.connections_dir().join("broken"), raw).unwrap();

        let error = migrate_v4_to_v5(&repo).unwrap_err().to_string();

        assert!(error.contains("connection 'broken' line 4"));
        assert_eq!(repo.read_version().unwrap(), "4");
        assert_eq!(
            fs::read_to_string(repo.connections_dir().join("broken")).unwrap(),
            raw
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn version4_ready_snapshot_is_discarded_and_restaged() {
        let repo = format4_repo("format5-restage");
        let path = repo.connections_dir().join("self");
        fs::write(
            &path,
            "# source_covering=main\n# target_peer=.\n# target_covering=main\n@a\t@b\told\n",
        )
        .unwrap();
        prepare_format4_to_5(&repo).unwrap();
        fs::write(
            &path,
            "# source_covering=main\n# target_peer=.\n# target_covering=main\n@a\t@b\tnew\n",
        )
        .unwrap();

        migrate_v4_to_v5(&repo).unwrap();

        assert_eq!(
            Connection::read(&repo, "self").unwrap().links[0].note,
            "new"
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn transitional_version_gates_commands_and_resumes_from_staged_bytes() {
        let repo = format4_repo("format5-resume");
        fs::write(
            repo.connections_dir().join("self"),
            "# source_covering=main\n# target_peer=.\n# target_covering=main\n@a\t@b\tnote\n",
        )
        .unwrap();
        prepare_format4_to_5(&repo).unwrap();
        activate_format4_to_5(&repo).unwrap();
        let staged =
            fs::read_to_string(format4_to_5_dir(&repo).join("staged/connections/self")).unwrap();
        fs::write(repo.connections_dir().join("self"), &staged).unwrap();

        let error = Repo::discover(&repo.root).unwrap_err().to_string();
        assert!(error.contains("unfinished"));
        migrate_v4_to_v5(&repo).unwrap();

        assert_eq!(repo.read_version().unwrap(), "5");
        assert_eq!(
            Connection::read(&repo, "self").unwrap().links[0].note,
            "note"
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn transitional_version_without_manifest_fails_without_touching_refs() {
        let repo = format4_repo("format5-no-manifest");
        let path = repo.connections_dir().join("self");
        fs::write(
            &path,
            "# source_covering=main\n# target_peer=.\n# target_covering=main\n@a\t@b\n",
        )
        .unwrap();
        fs::write(repo.dot().join("version"), "migrating-4-to-5\n").unwrap();
        let before = fs::read_to_string(&path).unwrap();

        let error = migrate_v4_to_v5(&repo).unwrap_err().to_string();

        assert!(error.contains("no ready manifest"));
        assert_eq!(fs::read_to_string(path).unwrap(), before);
        assert_eq!(repo.read_version().unwrap(), "migrating-4-to-5");
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn ready_snapshot_with_a_missing_staged_directory_cannot_activate() {
        let repo = format4_repo("format5-missing-staged-dir");
        prepare_format4_to_5(&repo).unwrap();
        fs::remove_dir_all(format4_to_5_dir(&repo).join("staged/iterators")).unwrap();

        let error = activate_format4_to_5(&repo).unwrap_err().to_string();

        assert!(error.contains("staged snapshot is missing directory"));
        assert_eq!(repo.read_version().unwrap(), "4");
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn format5_terminal_cleanup_removes_stale_journal_only() {
        let repo = format4_repo("format5-cleanup");
        fs::write(repo.dot().join("version"), "5\n").unwrap();
        fs::create_dir_all(format4_to_5_dir(&repo).join("staged")).unwrap();
        fs::write(format4_to_5_manifest(&repo), "ready\n").unwrap();
        let before = fs::read_to_string(repo.coverings_dir().join("main")).unwrap();

        cleanup_format4_to_5(&repo).unwrap();

        assert!(!format4_to_5_dir(&repo).exists());
        assert_eq!(
            fs::read_to_string(repo.coverings_dir().join("main")).unwrap(),
            before
        );
        fs::remove_dir_all(repo.root).unwrap();
    }
}
