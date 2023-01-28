use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use chrono_humanize::{Accuracy, HumanTime, Tense};
use clap::Parser;
use clio::ClioPath;
use glob::glob;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

mod args;
use args::{Args, SortBy};

#[derive(Debug)]
struct BlameData {
    pass_filename: PathBuf,
    last_modified: Duration,
    found_previous_commit: bool,
}

impl BlameData {
    const fn new(pass_filename: PathBuf, duration: Duration, found_previous: bool) -> Self {
        Self {
            pass_filename,
            last_modified: duration,
            found_previous_commit: found_previous,
        }
    }
}

fn get_password_age(
    pass_filename: &Path,
    ignore_rev: &Vec<String>,
    ignore_revs_file: &Vec<ClioPath>,
) -> Result<BlameData> {
    //Let's build up the git command
    let mut command = Command::new("git");

    //Blame in the porcalain format the first line (the password)
    command.args(["blame", "-pL", ",1"]);

    // Add in the arguments, if given
    for ignore_rev in ignore_rev {
        command.args(["--ignore-rev", ignore_rev]);
    }

    for ignore_revs_file in ignore_revs_file {
        command.args([
            "--ignore-revs-file",
            &ignore_revs_file.path().as_os_str().to_string_lossy(),
        ]);
    }

    command.args(["--", &pass_filename.as_os_str().to_string_lossy()]);

    let git_output = command.output()?;

    if !git_output.status.success() {
        return Err(anyhow!("{}", String::from_utf8(git_output.stderr)?));
    }

    let git_stdout = String::from_utf8(git_output.stdout)?;

    let mut duration = None;
    let mut found_previous = false;

    for line in git_stdout.lines() {
        if line.starts_with("author-time") {
            let author_time = line
                .split_ascii_whitespace()
                .last()
                .with_context(|| format!("Unable to get author-time value from: {line}"))?;
            let author_time_dt = DateTime::<Utc>::from_utc(
                NaiveDateTime::parse_from_str(author_time, "%s")
                    .with_context(|| format!("Unable to parse timestamp: {author_time}"))?,
                Utc,
            );
            duration = Some(Utc::now() - author_time_dt);
        } else if line.starts_with("previous") {
            found_previous = true;
        }
    }
    if let Some(duration) = duration {
        Ok(BlameData::new(
            pass_filename.to_path_buf().with_extension(""),
            duration,
            found_previous,
        ))
    } else {
        Err(anyhow!("Unable to find the author-time"))
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let password_store_dir = PathBuf::from(env!("PASSWORD_STORE_DIR"));

    if env::set_current_dir(password_store_dir.clone()).is_err() {
        eprintln!(
            "Unable to changed working directory to {}!",
            password_store_dir.display()
        );
        exit(1);
    }

    let password_store_git_dir = password_store_dir.join(".git");
    if !password_store_git_dir.exists() {
        eprintln!("Unable to find {}", password_store_git_dir.display());
        eprintln!("Please make sure you've run `pass git init`!",);
        exit(1);
    }

    let mut search_paths = Vec::<PathBuf>::new();

    if args.file.is_empty() {
        // No file specified, let's find all the passwords
        search_paths.push("**/*.gpg".into());
    } else {
        for pass_filename in args.file {
            if pass_filename.is_dir() {
                // Append dir names with /**/*.gpg
                let mut new_search_path = pass_filename;
                new_search_path.push("**");
                new_search_path.push("*.gpg");
                search_paths.push(new_search_path);
            } else {
                // Append file name with .gpg
                let mut new_search_path = OsString::from(pass_filename);
                new_search_path.push(OsString::from(".gpg"));
                search_paths.push(new_search_path.into());
            }
        }
    }

    let mut data = Vec::new();

    while let Some(search_path) = search_paths.pop() {
        println!("Searching {}", search_path.display());
        let glob_matches = glob(&search_path.to_string_lossy()).with_context(|| {
            format!(
                "Unable to search for .gpg files in {}",
                password_store_dir.display()
            )
        })?;
        let mut any_matches_found = false;
        for entry in glob_matches {
            match entry {
                Ok(entry) => {
                    match get_password_age(&entry, &args.ignore_rev, &args.ignore_revs_file) {
                        Ok(blame_data) => {
                            data.push(blame_data);
                        }
                        Err(e) => {
                            eprintln!("{e}");
                        }
                    }
                }
                Err(e) => eprintln!("{e}"),
            }
            any_matches_found = true;
        }
        if !any_matches_found {
            eprintln!(
                "Warning: {} is not in the password store.",
                search_path.with_extension("").display()
            );
        }
    }

    match args.sort_by {
        SortBy::Name => {
            data.sort_unstable_by(|a, b| a.pass_filename.cmp(&b.pass_filename));
        }
        SortBy::LastModified => {
            data.sort_unstable_by_key(|k| k.last_modified);
        }
    }

    if args.reverse {
        data.reverse();
    }

    if let Some(since) = args.since {
        if args.only_unmodified {
            // Show the passwords that haven't been modified longer than the duration
            data.retain(|blame_data| blame_data.last_modified > since);
        } else if args.only_modified {
            // Show passwords that have been modified within the duration
            data.retain(|blame_data| blame_data.last_modified < since);
        }
    }

    while let Some(blame_data) = data.pop() {
        if blame_data.found_previous_commit {
            if !args.only_unmodified {
                println!(
                    "{} last modified {}",
                    blame_data.pass_filename.display(),
                    HumanTime::from(blame_data.last_modified)
                        .to_text_en(Accuracy::Rough, Tense::Past),
                );
            }
        } else if !args.only_modified {
            println!(
                "{} hasn't been modified, since it was added to the store, {}",
                blame_data.pass_filename.display(),
                HumanTime::from(blame_data.last_modified).to_text_en(Accuracy::Rough, Tense::Past)
            );
        }
    }
    Ok(())
}
