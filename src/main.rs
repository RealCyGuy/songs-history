use std::{
    collections::HashSet,
    fs::OpenOptions,
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use clap::Parser;
use git2::{
    Delta::{Added, Deleted},
    Repository, Time,
};
use serde_json::Deserializer;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the songs-backup git repository
    directory: PathBuf,

    /// Overwrite the output file
    #[arg(short, long)]
    force: bool,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let repo = match Repository::open(args.directory) {
        Ok(repo) => repo,
        Err(e) => panic!("failed to open: {}", e),
    };

    let mut revwalk = repo.revwalk().unwrap();
    revwalk.push_head().unwrap();

    let file = match OpenOptions::new()
        .write(true)
        .truncate(true)
        .create_new(!args.force)
        .create(args.force)
        .open("output.txt")
    {
        Ok(f) => f,
        Err(e) => match e.kind() {
            std::io::ErrorKind::AlreadyExists => {
                panic!("Output file output.txt already exists. Use -f, --force to force overwriting the destination");
            }
            _ => panic!("failed to open file: {}", e),
        },
    };
    let mut writer = BufWriter::new(file);
    writeln!(writer, "# songs-history")?;

    let current_ids = get_current_ids(&repo).unwrap();

    let mut already_added: HashSet<String> = HashSet::new();

    for commit in revwalk.collect::<Vec<_>>().iter().rev() {
        let commit = repo.find_commit(*commit.as_ref().unwrap()).unwrap();
        let parent = match commit.parent(0) {
            Ok(parent) => parent,
            Err(_) => continue,
        };

        let diff = repo
            .diff_tree_to_tree(
                Some(&parent.tree().unwrap()),
                Some(&commit.tree().unwrap()),
                None,
            )
            .unwrap();

        let mut added: Vec<String> = Vec::new();
        let mut deleted: Vec<String> = Vec::new();

        for delta in diff.deltas() {
            if !matches!(delta.status(), Added | Deleted) {
                continue;
            }
            let new_file = delta.new_file();
            let path = new_file.path().unwrap();
            if !path.starts_with("output/songs") {
                continue;
            }
            let video = path.file_stem().unwrap().to_string_lossy();
            match delta.status() {
                Added => {
                    if already_added.contains(&video.to_string()) {
                        continue;
                    }
                    already_added.insert(video.to_string());
                    added.push(video.to_string());
                }
                Deleted => {
                    if current_ids.contains(&video.to_string()) {
                        continue;
                    }
                    deleted.push(video.to_string());
                }
                _ => {}
            }
        }

        if added.len() + deleted.len() == 0 {
            continue;
        }

        writeln!(writer, "## {}", format_time(&commit.time()))?;
        for video in added {
            writeln!(writer, "Added {}  ", format_video(&video))?;
        }
        for video in deleted {
            writeln!(writer, "Removed {}  ", format_video(&video))?;
        }
    }

    println!("Wrote to output.txt");
    Ok(())
}

fn format_time(time: &Time) -> String {
    let (offset, sign) = match time.offset_minutes() {
        n if n < 0 => (-n, '-'),
        n => (n, '+'),
    };
    let (hours, minutes) = (offset / 60, offset % 60);
    let ts = time::Timespec::new(time.seconds() + (time.offset_minutes() as i64) * 60, 0);
    let time = time::at(ts);

    format!(
        "{} {}{:02}{:02}",
        time.strftime("%a %b %e %T %Y").unwrap(),
        sign,
        hours,
        minutes
    )
}

fn get_current_ids(repo: &Repository) -> Result<HashSet<String>, git2::Error> {
    let obj = repo
        .head()?
        .peel_to_tree()?
        .get_path(Path::new("output/summary.json"))?
        .to_object(&repo)?
        .peel_to_blob()?;
    let mut file_content = String::new();
    obj.content().read_to_string(&mut file_content).unwrap();

    let mut ids: HashSet<String> = HashSet::new();

    let stream = Deserializer::from_str(&file_content).into_iter::<serde_json::Value>();

    for value in stream {
        if let Ok(obj) = value {
            if let Some(items) = obj.get("items") {
                if let Some(items_array) = items.as_array() {
                    for item in items_array {
                        let id = item["id"].as_str().unwrap();
                        ids.insert(id.to_string());
                    }
                }
            }
        }
    }

    Ok(ids)
}

fn format_video(video: &str) -> String {
    format!("[{}](https://youtu.be/{})", video, video)
}
