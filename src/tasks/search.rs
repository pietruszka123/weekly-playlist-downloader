use anyhow::bail;
use nucleo_matcher::pattern::{ Atom, AtomKind };

use crate::{ YtdlpSearchResult, listenbrainz_playlist::Track, ytdlp_manager::YtdlpManager };

pub async fn search_yt(manager: &YtdlpManager, track: &Track) -> anyhow::Result<YtdlpSearchResult> {
    let a = tokio::process::Command
        ::new(manager.ytdlp_path.as_ref().unwrap_or(&"yt-dlp".into()))
        .arg(format!("ytsearch{}:{} {}",10, track.title, track.creator))
        .arg("--flat-playlist")
        .arg("--skip-download")
        .arg("--quiet")
        .arg("--ignore-errors")
        .arg("--print")
        .arg("%(.{title,webpage_url,duration,uploader,thumbnails,thumbnail,view_count})j,")
        .output().await?;
    if !a.stderr.is_empty() {
        bail!("{}", String::from_utf8(a.stderr).unwrap());
    }

    let mut b = String::from_utf8(a.stdout).unwrap();
    b.insert(0, '[');
    b = b.trim_end_matches(",\n").to_string();
    b.insert(b.len(), ']');
    let v: Vec<YtdlpSearchResult> = serde_json::from_str(&b)?;

    if v.is_empty() {
        bail!("No video was found for \"{}\" by {}", track.title, track.creator);
    }

    let mut matcher = nucleo_matcher::Matcher::new(nucleo_matcher::Config::DEFAULT);
    let mut titles = Vec::new();
    let mut uploaders = Vec::new();
    for values in &v {
        titles.push(values.title.clone());
        uploaders.push(values.uploader.clone());
    }
    // dbg!(&track.title, tokio::task::id(), v.len());
    // println!("l {} {}", titles.len(), uploaders.len());
    // dbg!(&titles, &uploaders);
    let titles = Atom::new(
        &track.title,
        nucleo_matcher::pattern::CaseMatching::Ignore,
        nucleo_matcher::pattern::Normalization::Smart,
        AtomKind::Fuzzy,
        false
    ).match_list(titles, &mut matcher);
    let uploaders = Atom::new(
        &track.creator,
        nucleo_matcher::pattern::CaseMatching::Ignore,
        nucleo_matcher::pattern::Normalization::Smart,
        AtomKind::Fuzzy,
        false
    ).match_list(uploaders, &mut matcher);
    // println!("l2 {} {}", titles.len(), uploaders.len());
    // dbg!(&titles, &uploaders);
    let mut max_score = 0;
    let mut max_index = 0;

    for (i, result) in v.iter().enumerate() {
        let title_score = titles
            .iter()
            .find_map(|(title, score)| (*title == *result.title).then_some(*score));
        let uploader_score = uploaders
            .iter()
            .find_map(|(uploader, score)| (*uploader == *result.uploader).then_some(*score));
        let mut score = 0;
        if let Some(u) = uploader_score {
            score += u;
        }
        if let Some(t) = title_score {
            score += t;
        }
        if score > max_score {
            max_index = i;
            max_score = score;
        }
    }

    // dbg!(max_index, max_score, &v[max_index], v.len(), tokio::task::id());

    Ok(v.into_iter().nth(max_index).unwrap())
}

pub async fn search_task(
    manager: &YtdlpManager,
    track: &Track
) -> anyhow::Result<YtdlpSearchResult> {
    // let r = reqwest::get(
    //     &format!(
    //         "https://musicbrainz.org/ws/2/recording/{}?inc=url-rels&fmt=json",
    //         track.identifier
    //     )
    // ).await;

    let result = search_yt(manager, track).await?;

    Ok(result)
}
