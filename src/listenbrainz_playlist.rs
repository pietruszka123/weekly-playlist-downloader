use serde::Deserialize;

use crate::Nested;

#[derive(Debug, Deserialize, Clone)]
pub struct Playlist {
    pub title: String,
    pub track: Vec<Track>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AdditonalMetadata {
    caa_release_mbid: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub enum Extension {
    #[serde(rename = "https://musicbrainz.org/doc/jspf#track")] Jspf {
        additional_metadata: AdditonalMetadata,
    },
}
impl Extension {
    pub fn get_mbid(&self) -> Option<String> {
        match self {
            Extension::Jspf { additional_metadata } => additional_metadata.caa_release_mbid.clone(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Track {
    pub album: String,
    pub creator: String,
    pub title: String,
    pub identifier: Vec<String>,
    pub extension: Extension,
}

// pub struct Relation{
//     url:
// }

pub async fn fetch_playlist(playlist_mbid: String) -> anyhow::Result<Playlist> {
    let res = reqwest::get(
        format!("https://api.listenbrainz.org/1/playlist/{}?fetch_metadata=true", playlist_mbid)
    ).await?;

    let n: Nested<Playlist> = res.json().await?;

    Ok(n.playlist)
}
