#[derive(serde::Deserialize, Debug, Clone, Default)]
pub struct Track {
    pub filename: String,
    pub package: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "other-timestamp")]
    pub other_timestamp: Option<u128>,
    #[serde(rename = "self-timestamp")]
    pub self_timestamp: Option<u128>,
    #[serde(rename = "last-merged-version")]
    pub last_merged_version: Option<u128>,
}

impl Track {
    pub fn new(filename: &str) -> fpm::Track {
        fpm::Track {
            filename: filename.to_string(),
            ..Default::default()
        }
    }

    pub fn set_other_timestamp(mut self, other_timestamp: Option<u128>) -> fpm::Track {
        self.other_timestamp = other_timestamp;
        self
    }
}

pub(crate) fn get_tracks(
    base_path: &str,
    path: &camino::Utf8PathBuf,
) -> fpm::Result<std::collections::BTreeMap<String, Track>> {
    let mut tracks = std::collections::BTreeMap::new();
    if !path.exists() {
        return Ok(tracks);
    }

    let lib = fpm::FPMLibrary::default();
    let doc = std::fs::read_to_string(&path)?;
    let b = match fpm::doc::parse_ftd(base_path, doc.as_str(), &lib) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("failed to parse {}: {:?}", base_path, &e);
            todo!();
        }
    };
    let track_list: Vec<Track> = b.get("fpm#track")?;
    for track in track_list {
        tracks.insert(track.filename.to_string(), track);
    }
    Ok(tracks)
}

pub(crate) async fn get_cr_track(
    config: &fpm::Config,
    cr_number: usize,
) -> fpm::Result<std::collections::BTreeMap<String, fpm::Track>> {
    let cr_track_path = config.cr_path(cr_number).join(".track.ftd");
    if !cr_track_path.exists() {
        return Ok(Default::default());
    }

    let doc = std::fs::read_to_string(&cr_track_path)?;
    resolve_cr_track(&doc).await
}

pub(crate) async fn resolve_cr_track(
    content: &str,
) -> fpm::Result<std::collections::BTreeMap<String, fpm::Track>> {
    if content.trim().is_empty() {
        return Err(fpm::Error::UsageError {
            message: "Content is empty in cr about".to_string(),
        });
    }
    let lib = fpm::FPMLibrary::default();
    let b = match fpm::doc::parse_ftd(".track.ftd", content, &lib) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("failed to parse .latest.ftd: {:?}", &e);
            todo!();
        }
    };
    let track_list: Vec<fpm::Track> = b.get("fpm#track")?;
    let mut tracks = std::collections::BTreeMap::new();
    for track in track_list {
        tracks.insert(track.filename.to_string(), track);
    }
    Ok(tracks)
}

pub(crate) async fn create_cr_track(
    config: &fpm::Config,
    cr_tracks: &[fpm::Track],
    cr_number: usize,
) -> fpm::Result<()> {
    let track_content = generate_cr_track_content(cr_tracks);
    fpm::utils::update(
        &config.cr_path(cr_number),
        ".track.ftd",
        track_content.as_bytes(),
    )
    .await?;
    Ok(())
}

pub(crate) fn generate_cr_track_content(cr_tracks: &[fpm::Track]) -> String {
    let mut track_content = "-- import: fpm".to_string();
    for track in cr_tracks {
        let mut track_data = format!("-- fpm.track: {}\n", track.filename);
        if let Some(self_timestamp) = track.self_timestamp {
            track_data = format!("{}self-timestamp: {}\n", track_data, self_timestamp);
        }
        if let Some(base) = track.other_timestamp {
            track_data = format!("{}other-timestamp: {}\n", track_data, base);
        }
        track_content = format!("{}\n\n\n{}", track_content, track_data);
    }
    track_content
}
