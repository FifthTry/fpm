pub async fn status() -> fpm::Result<()> {
    let config = fpm::Config::read().await?;
    let snapshots = fpm::snaphot::get_latest_snapshots(config.root.as_str())?;
    let mut filestatus = std::collections::BTreeMap::new();

    let mut trackstatus = std::collections::BTreeMap::new();
    for doc in fpm::process_dir(config.root.as_str(), &config, fpm::ignore_history()).await? {
        if let fpm::FileFound::FTDDocument(doc) = doc {
            let status = get_file_status(&doc, &snapshots).await?;
            let track = get_track_status(&doc, &snapshots, config.root.as_str())?;
            if !track.is_empty() {
                trackstatus.insert(doc.id.to_string(), track);
            }
            filestatus.insert(doc.id, status);
        }
    }

    let mut status = print_file_status(&snapshots, &filestatus);
    status = status && print_track_status(&trackstatus);
    if status {
        println!("Nothing to sync, clean working tree");
    }
    Ok(())
}

async fn get_file_status(
    doc: &fpm::Document,
    snapshots: &std::collections::BTreeMap<String, String>,
) -> fpm::Result<FileStatus> {
    if let Some(timestamp) = snapshots.get(&doc.id) {
        let path = format!(
            "{}/.history/{}",
            doc.base_path.as_str(),
            doc.id.replace(".ftd", &format!(".{}.ftd", timestamp))
        );

        let existing_doc = tokio::fs::read_to_string(&path).await?;
        if doc.document.eq(&existing_doc) {
            return Ok(FileStatus::None);
        }
        return Ok(FileStatus::Modified);
    }
    Ok(FileStatus::Added)
}

fn get_track_status(
    doc: &fpm::Document,
    snapshots: &std::collections::BTreeMap<String, String>,
    base_path: &str,
) -> fpm::Result<std::collections::BTreeMap<String, TrackStatus>> {
    let path = format!(
        "{}/.tracks/{}",
        doc.base_path.as_str(),
        doc.id.replace(".ftd", ".track")
    );
    let mut track_list = std::collections::BTreeMap::new();
    if std::fs::metadata(&path).is_err() {
        return Ok(track_list);
    }
    let tracks = fpm::track_data::get_track(base_path, &path)?;
    for track in tracks.values() {
        if let Some(timestamp) = snapshots.get(&track.document_name) {
            let trackstatus = if track.other_timestamp.is_none() {
                TrackStatus::NeverMarked
            } else if timestamp.eq(track.other_timestamp.as_ref().unwrap()) {
                TrackStatus::Uptodate
            } else {
                let now = timestamp.parse::<u128>().unwrap();
                let then = track
                    .other_timestamp
                    .as_ref()
                    .unwrap()
                    .parse::<u128>()
                    .unwrap();
                let diff = std::time::Duration::from_nanos((now - then) as u64);
                TrackStatus::Outofdate {
                    days: format!("{:?}", diff.as_secs()),
                }
            };
            track_list.insert(track.document_name.to_string(), trackstatus);
        }
    }
    Ok(track_list)
}

fn print_track_status(
    trackstatus: &std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, TrackStatus>,
    >,
) -> bool {
    let mut status = true;
    for (k, v) in trackstatus {
        for (i, j) in v {
            println!("{:?}: {} -> {}", j, k, i);
            status = false;
        }
    }
    status
}

fn print_file_status(
    snapshots: &std::collections::BTreeMap<String, String>,
    filestatus: &std::collections::BTreeMap<String, FileStatus>,
) -> bool {
    let mut any_file_removed = false;
    for id in snapshots.keys() {
        if let Some(status) = filestatus.get(id) {
            if status.eq(&FileStatus::None) {
                continue;
            }
            println!("{:?}: {}", status, id);
        } else {
            any_file_removed = true;
            println!("{:?}: {}", FileStatus::Removed, id);
        }
    }

    for (id, status) in filestatus
        .iter()
        .filter(|(_, f)| f.eq(&&FileStatus::Added))
        .collect::<Vec<(&String, &FileStatus)>>()
    {
        println!("{:?}: {}", status, id);
    }
    if !(filestatus.iter().any(|(_, f)| !f.eq(&FileStatus::None)) || any_file_removed) {
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
    Uptodate,
    NeverMarked,
    Outofdate { days: String },
}
