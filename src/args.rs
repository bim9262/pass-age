use clap::{ArgAction, ArgGroup, Parser, ValueEnum};
use clio::ClioPath;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "pass-age")]
#[command(bin_name = "pass-age")]
#[command(author, version, about, long_about = None)]
#[command(after_long_help = r#"Example usage:

Get passwords in the Financial folder that haven't been changed in the last year:

# pass age --sort-by=last-modified --only-unmodified --since=365days Financial/
"#)]
#[command(group(
    ArgGroup::new("filter")
    .args(["only_unmodified", "only_modified"]),
))]
pub struct Args {
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
    pub ignore_rev: Vec<String>,

    /// Ignore revisions listed in `file`.
    ///
    /// Ignore revisions listed in `file`, which must be in the same format as an
    /// `fsck.skipList`. This option may be repeated, and these files will be
    /// processed after any files specified with the `blame.ignoreRevsFile`
    /// config option.
    #[arg(long, value_name = "file", value_parser = clap::value_parser!(ClioPath).exists().is_file())]
    pub ignore_revs_file: Vec<ClioPath>,

    /// Only display the passwords in the store that have not been modified.
    #[arg(long, action)]
    pub only_unmodified: bool,

    /// Only display the passwords in the store that have been modified.
    #[arg(long, action)]
    pub only_modified: bool,

    /// With --only_unmodified: show the passwords that haven't been modified longer than the duration.
    /// With --only_modified: show passwords that have been modified within the duration.
    #[arg(long, value_name = "date", value_parser= parse_duration, requires="filter")]
    pub since: Option<chrono::Duration>,

    /// Reverse the sort order
    #[arg(short, long, action=ArgAction::SetFalse)]
    pub reverse: bool,

    #[arg(short, long, value_enum, default_value_t=SortBy::Name)]
    pub sort_by: SortBy,

    /// The passwords that match pass-names
    #[arg(value_name = "pass-names")]
    pub file: Vec<PathBuf>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum SortBy {
    Name,
    LastModified,
}

fn parse_duration(arg: &str) -> Result<chrono::Duration, std::num::ParseIntError> {
    let duration = arg.parse::<humantime::Duration>().unwrap();
    let seconds = duration
        .as_secs()
        .try_into()
        .expect("since could not be converted to i64");
    Ok(chrono::Duration::seconds(seconds))
}
