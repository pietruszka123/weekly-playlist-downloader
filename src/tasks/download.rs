use std::{ io::Cursor, path::{ Path, PathBuf }, time::Duration };

use anyhow::bail;
use audiotags::{ Album, AudioTagEdit, AudioTagWrite, Picture };
use image::ImageReader;
use tokio::{ sync::Semaphore, time::sleep };

use crate::{ YtdlpSearchResult, listenbrainz_playlist::Track, ytdlp_manager::YtdlpManager };

static IMAGE_SEMAPHORE: Semaphore = Semaphore::const_new(4);
async fn get_image(cover_url: &str) -> anyhow::Result<Vec<u8>> {
    let mut tries = 5;
    let mut request = reqwest::get(cover_url).await;
    while request.is_err() && tries > 0 {
        sleep(Duration::from_secs(1)).await;
        request = reqwest::get(cover_url).await;
        tries -= 1;
    }

    let bytes = request?.bytes().await?;

    let mut image = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?.decode()?;
    let mut converted = Vec::new();
    if image.width() > 1920 || image.height() > 1080 {
        image = image.resize(1920, 1080, image::imageops::FilterType::CatmullRom);
    }
    image.write_to(Cursor::new(&mut converted), image::ImageFormat::Png)?;
    Ok(converted)
}
async fn download(manager: &YtdlpManager, url: &str, filename: &Path) -> anyhow::Result<()> {
    let output = tokio::process::Command
        ::new(manager.ytdlp_path.as_ref().unwrap_or(&"yt-dlp".into()))
        .arg(url)
        // .arg("--embed-metadata")
        .arg("-f")
        .arg("bestaudio[ext=m4a],bestaudio[ext=webm]")
        .arg("-o")
        .arg(filename)
        .output().await?;
    if !output.status.success() {
        bail!(String::from_utf8(output.stderr).unwrap());
    }
    Ok(())
}

pub async fn download_task(
    manager: &YtdlpManager,
    track: &Track,
    search_result: YtdlpSearchResult,
    playlist_title: &str,
    track_number: u16
) -> anyhow::Result<()> {
    let path = PathBuf::from(
        format!("./{}/{}.m4a", playlist_title.replace("/", "-"), track.title.replace("/", "-"))
    );
    download(manager, &search_result.webpage_url, &path).await?;
    let mut tag = audiotags::Mp4Tag::new();
    tag.add_artist(&track.creator);
    tag.set_album_title(&track.album);
    tag.set_title(&track.title);

    let mut image_permit = None;
    let cover_url = if let Some(mbid) = track.extension.get_mbid() {
        image_permit = Some(IMAGE_SEMAPHORE.acquire().await?);
        &format!("https://coverartarchive.org/release/{}/front", mbid)
    } else {
        &search_result.thumbnails.first().unwrap().url
    };

    let converted = if let Ok(image) = get_image(cover_url).await {
        image
    } else {
        get_image(&search_result.thumbnails.first().unwrap().url).await?
    };

    drop(image_permit);
    tag.set_comment(
        format!("url: {} yt-url: {}", track.identifier.first().unwrap(), search_result.webpage_url)
    );
    tag.set_album(
        Album::with_all(
            playlist_title,
            "ListenBrainz",
            Picture::new(&converted, audiotags::MimeType::Png)
        )
    );
    tag.set_track_number(track_number);
    tag.write_to_path(path.to_str().unwrap())?;

    Ok(())
}
