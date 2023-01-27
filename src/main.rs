use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use chrono_humanize::{Accuracy, HumanTime, Tense};
use clap::{ArgAction, Parser, ValueEnum};
use glob::glob;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

#[derive(Debug)]
struct BlameData {
    pass_filename: PathBuf,
    duration: Duration,
    found_previous: bool,
}

impl BlameData {
    const fn new(pass_filename: PathBuf, duration: Duration, found_previous: bool) -> Self {
        Self {
            pass_filename,
            duration,
            found_previous,
        }
    }
}

fn get_password_age(
    pass_filename: &PathBuf,
    ignore_rev: &Vec<String>,
    ignore_revs_file: &Vec<PathBuf>,
    since: &Option<String>,
) -> Result<BlameData> {
    // Paths need the .gpg added back on
    let mut path = OsString::from(pass_filename);
    path.push(OsString::from(".gpg"));

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
            &ignore_revs_file.as_os_str().to_string_lossy(),
        ]);
    }

    if let Some(since) = since {
        command.args(["--since", since]);
    }

    command.args(["--", &path.to_string_lossy()]);

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
            pass_filename.to_path_buf(),
            duration,
            found_previous,
        ))
    } else {
        return Err(anyhow!("Unable to find the author-time"));
    }
}

#[derive(Parser, Debug)]
#[command(name = "pass")]
#[command(bin_name = "pass age")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Ignore changes made by the revision when assigning blame.
    ///
    /// Ignore changes made by the revision when assigning blame, as if the
    /// change never happened. Lines that were changed or added by an ignored
    /// commit will be blamed on the previous commit that changed that line
    /// or nearby lines. This option may be specified multiple times to ignore
    /// more than one revision. If the `blame.markIgnoredLines` config option is
    /// set, then lines that were changed by an ignored commit and attributed
    /// to another commit will be marked with a `?` in the blame output. If the
    /// `blame.markUnblamableLines` config option is set, then those lines touched
    /// by an ignored commit that we could not attribute to another revision
    /// are marked with a `*`.
    #[arg(long, value_name = "rev")]
    ignore_rev: Vec<String>,

    /// Ignore revisions listed in `file`.
    ///
    /// Ignore revisions listed in `file`, which must be in the same format as an
    /// `fsck.skipList`. This option may be repeated, and these files will be
    /// processed after any files specified with the `blame.ignoreRevsFile`
    /// config option. An empty file name, `""`, will clear the list of revs
    /// from previously processed files.
    #[arg(long, value_name = "file")]
    ignore_revs_file: Vec<PathBuf>,

    /// Show commits more recent than a specific date.
    #[arg(long, value_name = "date")]
    since: Option<String>,

    /// Reverse the sort order
    #[arg(short, long, action=ArgAction::SetFalse)]
    reverse: bool,

    #[arg(short, long, value_enum, default_value_t=SortBy::Name)]
    sort_by: SortBy,

    /// Name of the person to greet
    #[arg(value_name = "file")]
    file: Vec<PathBuf>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum SortBy {
    Name,
    Time,
}

fn main() -> Result<()> {
    let password_store_dir =
        env::var("PASSWORD_STORE_DIR").context("Unable to get PASSWORD_STORE_DIR")?;

    let args = Args::parse();
    println!("{args:?}");

    // We want to grab the ignore_revs_file before we change our own working directory
    // in case the path given was relative.
    let ignore_revs_file = args
        .ignore_revs_file
        .iter()
        .map(|x| {
            let path = if x.is_relative() {
                env::current_dir()
                    .context("Unable to get the current working directory")
                    .unwrap()
                    .join(x)
            } else {
                x.to_path_buf()
            };
            if path.is_file() {
                path
            } else {
                eprintln!(
                    "The file supplied as --ignore-revs-file, \"{}\", is not a file!",
                    path.display()
                );
                exit(1);
            }
        })
        .collect();

    let password_store_dir = Path::new(&password_store_dir);

    if env::set_current_dir(password_store_dir).is_err() {
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

    let mut search_paths = Vec::<String>::new();

    if args.file.is_empty() {
        search_paths.push("**/*.gpg".into());
    } else {
        for pass_filename in args.file {
            if pass_filename.is_dir() {
                let mut new_search_path = pass_filename;
                new_search_path.push("**");
                new_search_path.push("*.gpg");
                search_paths.push(new_search_path.to_string_lossy().to_string());
            } else {
                search_paths.push(pass_filename.to_string_lossy().to_string());
            }
        }
    }

    let mut data = Vec::new();

    while let Some(search_path) = search_paths.pop() {
        println!("Searching {search_path}");
        for entry in glob(&search_path)
            .with_context(|| {
                format!(
                    "Unable to search for .gpg files in {}",
                    password_store_dir.display()
                )
            })?
            .take(5)
        {
            match entry {
                Ok(entry) => {
                    match get_password_age(
                        &entry.with_extension(""),
                        &args.ignore_rev,
                        &ignore_revs_file,
                        &args.since,
                    ) {
                        Ok(blame_data) => data.push(blame_data),
                        Err(e) => {
                            eprintln!("{e}");
                        }
                    }
                }
                Err(e) => eprintln!("{e}"),
            }
        }
    }

    //Files are sorted by name by default so we only need to sort
    // if sorting by time
    if args.sort_by == SortBy::Time {
        data.sort_by(|a, b| a.duration.cmp(&b.duration));
    }

    if args.reverse {
        data.reverse();
    }

    while let Some(blame_data) = data.pop() {
        if blame_data.found_previous {
            println!(
                "{} last modified {}",
                blame_data.pass_filename.display(),
                HumanTime::from(blame_data.duration).to_text_en(Accuracy::Rough, Tense::Past),
            );
        } else {
            let not_modified_since = if let Some(since) = &args.since {
                format!("since={since}")
            } else {
                "it was added to the store".into()
            };

            println!(
                "{} hasn't been updated, {not_modified_since}",
                blame_data.pass_filename.display()
            );
        }
    }
    Ok(())
}
