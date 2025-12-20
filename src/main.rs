use std::{ env, fs::OpenOptions, path::{ Path, PathBuf }, sync::Arc, time::Duration };

use anyhow::{ anyhow, bail };
use clap::Parser;
use indicatif::{ MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle };
use owo_colors::OwoColorize;
use reqwest::{ Version, header::ACCEPT };
use serde::Deserialize;
use tokio::{ sync::Semaphore, task::JoinSet, time::sleep };

use crate::listenbrainz_playlist::Playlist;

pub mod listenbrainz_playlist;
pub mod tasks;
pub mod ytdlp_manager;

pub fn data_dir() -> anyhow::Result<PathBuf> {
    Ok(dirs::data_dir().ok_or(anyhow::anyhow!("no data directory"))?.join("music_player"))
}
pub fn create_data_dir() -> anyhow::Result<PathBuf> {
    let path = data_dir()?;
    if !std::fs::exists(&path)? {
        std::fs::create_dir(&path)?;
    }
    Ok(path)
}
#[allow(unused)]
#[derive(Debug, Clone, Deserialize)]
pub struct Thumbnail {
    url: String,
    width: u32,
    height: u32,
}

#[allow(unused)]
#[derive(Debug, Clone, Deserialize)]
pub struct YtdlpSearchResult {
    title: String,
    webpage_url: String,
    duration: f64,
    uploader: String,
    view_count: usize,
    thumbnails: Vec<Thumbnail>,
}
fn get_default_output_path() -> PathBuf {
    let mut path = env::current_exe().unwrap();
    path.pop();
    // path.push("log/debug.log");
    path
}
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    playlist_file: Option<PathBuf>,
    #[arg(short, default_value = get_default_output_path().into_os_string())]
    out_folder_path: PathBuf,

    /// Always use youtube thumbnails
    #[arg(short, default_value_t = false, short)]
    always_use_youtube_thumbnails: bool,

    /// How many ytdlp tasks are run at once
    #[arg(short, long, default_value_t = 5)]
    max_conccurent_tasks: usize,

    #[arg(short)]
    listen_brainz_username: Option<String>,

    /// Hide all outputs
    #[arg(short, default_value_t = false)]
    quiet: bool,
}
#[derive(Debug, Deserialize)]
struct RecommendationsPlaylist {
    identifier: String,
}
#[derive(Debug, Deserialize)]
pub struct Nested<T: 'static> {
    playlist: T,
}

#[derive(Debug, Deserialize)]
struct Recommendations {
    playlists: Vec<Nested<RecommendationsPlaylist>>,
}

async fn get_recomendations(username: String) -> anyhow::Result<Recommendations> {
    let res = reqwest::Client
        ::new()
        .get(format!("https://api.listenbrainz.org/1/user/{}/playlists/recommendations", username))
        .header(ACCEPT, "application/json")
        .send().await?
        .error_for_status()?;
    let json: Recommendations = res.json().await?;

    Ok(json)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Args::parse();

    let playlist = if let Some(playlist_file) = cli.playlist_file {
        println!("using file");
        let playlist: Playlist = serde_json::from_reader(
            OpenOptions::new().read(true).open(playlist_file)?
        )?;

        playlist
    } else if let Some(username) = cli.listen_brainz_username {
        let recommendations = get_recomendations(username).await?;
        let entry = recommendations.playlists.first().ok_or(anyhow!("error"))?;
        let id = entry.playlist.identifier.replace("https://listenbrainz.org/playlist/", "");
        listenbrainz_playlist::fetch_playlist(id).await?
    } else {
        bail!("You must specify playlist file path or listenbrainz username");
    };

    let semaphore = Arc::new(Semaphore::new(cli.max_conccurent_tasks));

    let progress = Arc::new(MultiProgress::new());
    let progress_style = ProgressStyle::with_template(
        "{msg} {wide_bar:.green/blue} {pos:>7}/{len:7} [{eta_precise}]"
    ).unwrap();

    if cli.quiet {
        progress.set_draw_target(ProgressDrawTarget::hidden());
    }

    let total_progress = progress
        .add(ProgressBar::new(playlist.track.len() as u64))
        .with_style(progress_style);
    total_progress.enable_steady_tick(Duration::from_millis(120));

    let mut tasks: JoinSet<anyhow::Result<()>> = JoinSet::new();
    let title = Arc::new(playlist.title.clone());

    total_progress.force_draw();
    for (track_number, track) in playlist.track.into_iter().enumerate() {
        let semaphore = semaphore.clone();
        let title = title.clone();
        let p = progress.clone();
        let total_progress = total_progress.clone();
        tasks.spawn(async move {
            let permit = semaphore.acquire().await?;
            let progress_style = ProgressStyle::with_template(
                "{spinner:.red/yellow} {msg}"
            ).unwrap();
            let progress = p.insert_before(
                &total_progress,
                ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr()).with_style(
                    progress_style
                )
            );
            progress.enable_steady_tick(Duration::from_millis(120));
            progress.set_message(format!("{} by {}", track.title.clone(), track.creator.clone()));
            let result = tasks::search::search_task(&track).await?;

            tasks::download::download_task(&track, result, &title, track_number as u16).await?;

            drop(permit);
            progress.finish_with_message("Finished".green().to_string());
            sleep(Duration::from_millis(200)).await;
            p.remove(&progress);
            total_progress.inc(1);

            Ok(())
        });
    }

    while let Some(res) = tasks.join_next().await {
        if let Err(err) = res {
            println!("{}", err);
        }
    }
    total_progress.finish();
    if !cli.quiet {
        println!("Download completed");
    }

    Ok(())
}
