pub async fn status(source: Option<&str>) -> fpm::Result<()> {
    let config = fpm::Config::read().await?;
    let snapshots = fpm::snapshot::get_latest_snapshots(config.root.as_str())?;
    if let Some(source) = source {
        file_status(config.root.as_str(), source, &snapshots).await?;
        return Ok(());
    }
    all_status(&config, &snapshots).await
}

async fn file_status(
    base_path: &str,
    source: &str,
    snapshots: &std::collections::BTreeMap<String, String>,
) -> fpm::Result<()> {
    let path = format!("{}/{}", base_path, source);
    if std::fs::metadata(&path).is_err() {
        if snapshots.contains_key(source) {
            println!("{:?}: {}", FileStatus::Removed, source);
        } else {
            eprintln!("Error: {} does not exists", source);
        }
        return Ok(());
    }

    let file = fpm::process_file(std::path::PathBuf::from(path), base_path).await?;

    let file_status = get_file_status(&file, snapshots).await?;
    let track_status = get_track_status(&file, snapshots, base_path)?;

    let mut clean = true;
    if !file_status.eq(&FileStatus::None) {
        println!("{:?}: {}", file_status, source);
        clean = false;
    }
    for (i, j) in track_status {
        println!("{}: {} -> {}", j.to_string(), source, i);
        clean = false;
    }
    if clean {
        println!("Nothing to sync, clean working tree");
    }
    Ok(())
}

async fn all_status(
    config: &fpm::Config,
    snapshots: &std::collections::BTreeMap<String, String>,
) -> fpm::Result<()> {
    let mut file_status = std::collections::BTreeMap::new();
    let mut track_status = std::collections::BTreeMap::new();
    for doc in fpm::get_documents(config).await? {
        let status = get_file_status(&doc, snapshots).await?;
        let track = get_track_status(&doc, snapshots, config.root.as_str())?;
        if !track.is_empty() {
            track_status.insert(doc.get_id(), track);
        }
        file_status.insert(doc.get_id(), status);
    }

    let clean_file_status = print_file_status(snapshots, &file_status);
    let clean_track_status = print_track_status(&track_status);
    if clean_file_status && clean_track_status {
        println!("Nothing to sync, clean working tree");
    }
    Ok(())
}

async fn get_file_status(
    doc: &fpm::File,
    snapshots: &std::collections::BTreeMap<String, String>,
) -> fpm::Result<FileStatus> {
    if let Some(timestamp) = snapshots.get(&doc.get_id()) {
        let file_extension = if let Some((_, b)) = doc.get_id().rsplit_once('.') {
            Some(b.to_string())
        } else {
            None
        };

        let path = format!("{}/.history/{}", doc.get_base_path().as_str(), {
            if let Some(ref ext) = file_extension {
                doc.get_id()
                    .replace(&format!(".{}", ext), &format!(".{}.{}", timestamp, ext))
            } else {
                format!(".{}", timestamp)
            }
        });

        let content = tokio::fs::read_to_string(&doc.get_full_path()).await?;

        let existing_doc = tokio::fs::read_to_string(&path).await?;
        if content.eq(&existing_doc) {
            return Ok(FileStatus::None);
        }
        return Ok(FileStatus::Modified);
    }
    Ok(FileStatus::Added)
}

fn get_track_status(
    doc: &fpm::File,
    snapshots: &std::collections::BTreeMap<String, String>,
    base_path: &str,
) -> fpm::Result<std::collections::BTreeMap<String, TrackStatus>> {
    let path = format!(
        "{}/.tracks/{}",
        doc.get_base_path().as_str(),
        format!("{}.track", doc.get_id())
    );
    let mut track_list = std::collections::BTreeMap::new();
    if std::fs::metadata(&path).is_err() {
        return Ok(track_list);
    }
    let tracks = fpm::tracker::get_tracks(base_path, &path)?;
    for track in tracks.values() {
        if !snapshots.contains_key(&track.document_name) {
            eprintln!(
                "Error: {} is tracked by {}, but {} is either removed or never synced",
                track.document_name,
                doc.get_id(),
                track.document_name
            );
            continue;
        }
        let timestamp = snapshots.get(&track.document_name).unwrap();
        let track_status = if track.other_timestamp.is_none() {
            TrackStatus::NeverMarked
        } else if timestamp.eq(track.other_timestamp.as_ref().unwrap()) {
            TrackStatus::UptoDate
        } else {
            let now = timestamp.parse::<u128>().unwrap();
            let then = track
                .other_timestamp
                .as_ref()
                .unwrap()
                .parse::<u128>()
                .unwrap();
            let diff = std::time::Duration::from_nanos((now - then) as u64);
            TrackStatus::OutOfDate {
                days: format!("{:?}", diff.as_secs() / 86400),
            }
        };
        track_list.insert(track.document_name.to_string(), track_status);
    }
    Ok(track_list)
}

fn print_track_status(
    track_status: &std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, TrackStatus>,
    >,
) -> bool {
    let mut status = true;
    for (k, v) in track_status {
        for (i, j) in v {
            if j.eq(&TrackStatus::UptoDate) {
                continue;
            }
            println!("{}: {} -> {}", j.to_string(), k, i);
            status = false;
        }
    }
    status
}

fn print_file_status(
    snapshots: &std::collections::BTreeMap<String, String>,
    file_status: &std::collections::BTreeMap<String, FileStatus>,
) -> bool {
    let mut any_file_removed = false;
    for id in snapshots.keys() {
        if let Some(status) = file_status.get(id) {
            if status.eq(&FileStatus::None) {
                continue;
            }
            println!("{:?}: {}", status, id);
        } else {
            any_file_removed = true;
            println!("{:?}: {}", FileStatus::Removed, id);
        }
    }

    for (id, status) in file_status
        .iter()
        .filter(|(_, f)| f.eq(&&FileStatus::Added))
        .collect::<Vec<(&String, &FileStatus)>>()
    {
        println!("{:?}: {}", status, id);
    }
    if !(file_status.iter().any(|(_, f)| !f.eq(&FileStatus::None)) || any_file_removed) {
        return true;
    }
    false
}

#[derive(Debug, PartialEq)]
enum FileStatus {
    Modified,
    Added,
    Removed,
    None,
}

#[derive(Debug, PartialEq)]
enum TrackStatus {
    UptoDate,
    NeverMarked,
    OutOfDate { days: String },
}

impl ToString for TrackStatus {
    fn to_string(&self) -> String {
        match self {
            TrackStatus::UptoDate => "Up to date".to_string(),
            TrackStatus::NeverMarked => "Never marked".to_string(),
            TrackStatus::OutOfDate { days } => format!("{} days out of date", days),
        }
    }
}
