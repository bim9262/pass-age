use clap::CommandFactory;
use clap_complete::{
    generate_to,
    shells::{Bash, Zsh},
};
use clap_mangen::Man;
use std::{
    env,
    io::{Error, ErrorKind},
    path::PathBuf,
    process::Command,
};

#[path = "src/args.rs"]
mod args;
use args::Args;

fn main() -> Result<(), Error> {
    // This is the path where the output is placed
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or(ErrorKind::NotFound)?);

    let cmd = Args::command();
    // The name of the binary that will be used
    let bin_name = cmd.get_bin_name().unwrap();

    // Generate the man page
    let man = Man::new(cmd.clone());
    let mut buffer: Vec<u8> = Default::default();
    man.render(&mut buffer)?;

    // Write the man page
    std::fs::write(out_dir.join(bin_name).with_extension("1"), buffer)?;

    // Generate shell completion for bash and zsh
    generate_to(Bash, &mut cmd.clone(), bin_name, out_dir.clone())?;
    generate_to(Zsh, &mut cmd.clone(), bin_name, out_dir.clone())?;

    println!("cargo:warning=out_dir={}", out_dir.display());

    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .map(|o| String::from_utf8(o.stdout).unwrap());
    let date = Command::new("git")
        .args(["log", "--pretty=format:'%ad'", "-n1", "--date=short"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .map(|o| String::from_utf8(o.stdout).unwrap());
    if let (Ok(hash), Ok(date)) = (hash, date) {
        let ver = format!(
            "{} (commit {} {})",
            env!("CARGO_PKG_VERSION"),
            hash.trim(),
            date.trim_matches('\'')
        );
        println!("cargo:rustc-env=CARGO_PKG_VERSION={ver}");
    }

    Ok(())
}
