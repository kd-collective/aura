//! All functionality involving the `-C` command.

use crate::error::Error;
use crate::{a, aln};
use alpm::Alpm;
use aura_core as core;
use chrono::{DateTime, Local};
use colored::*;
use i18n_embed::fluent::FluentLanguageLoader;
use i18n_embed_fl::fl;
use log::debug;
use pbr::ProgressBar;
use rayon::prelude::*;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use ubyte::ToByteUnit;

/// Print cache data for given packages.
pub fn info(
    fll: FluentLanguageLoader,
    alpm: &Alpm,
    path: &Path,
    packages: Vec<String>,
) -> Result<(), Error> {
    let db = alpm.localdb();

    packages
        .iter()
        .filter_map(|p| core::cache::info(path, p).ok())
        .filter_map(|ci| ci)
        .for_each(|ci| {
            let name = fl!(fll, "common-name");
            let ver = fl!(fll, "cache-info-latest");
            let created = fl!(fll, "cache-info-created");
            let sig = fl!(fll, "cache-info-sig");
            let size = fl!(fll, "cache-info-size");
            let av = fl!(fll, "cache-info-avail");
            let long = vec![&name, &ver, &created, &sig, &size, &av]
                .iter()
                .map(|s| s.len())
                .max()
                .unwrap();

            let dt = DateTime::<Local>::from(ci.created).format("%F %T");
            let is_in = if let Ok(pkg) = db.pkg(ci.name.as_str()) {
                if ci.version == pkg.version().as_str() {
                    format!("[{}]", fl!(fll, "cache-info-installed"))
                        .cyan()
                        .bold()
                } else {
                    format!("[{}: {}]", fl!(fll, "cache-info-installed"), pkg.version())
                        .yellow()
                        .bold()
                }
            } else {
                "".normal()
            };
            let sig_yes_no = if ci.signature {
                fl!(fll, "common-yes").green().bold()
            } else {
                fl!(fll, "common-no").yellow()
            };

            println!("{:w$} : {}", name.bold(), ci.name, w = long);
            println!("{:w$} : {} {}", ver.bold(), ci.version, is_in, w = long);
            println!("{:w$} : {}", created.bold(), dt, w = long);
            println!("{:w$} : {}", sig.bold(), sig_yes_no, w = long);
            println!("{:w$} : {}", size.bold(), ci.size.bytes(), w = long);
            println!("{:w$} : {}", av.bold(), ci.available.join(", "), w = long);
            println!();
        });

    Ok(())
}

/// Print all package filepaths from the cache that match some search term.
pub fn search(path: &Path, term: &str) -> Result<(), Error> {
    let matches = core::cache::search(path, term)?;
    for file in matches {
        println!("{}", file.path().display());
    }
    Ok(())
}

/// Backup the package cache to a given directory.
pub fn backup(fll: FluentLanguageLoader, source: &Path, target: &Path) -> Result<(), Error> {
    // The full, absolute path to copy files to.
    let full: PathBuf = if target.is_absolute() {
        target.to_path_buf()
    } else {
        let mut curr = std::env::current_dir()?;
        curr.push(target);
        curr
    };
    let ts = full.to_str().unwrap();
    if target.is_file() {
        let msg = fl!(fll, "cache-backup-file", target = ts);
        aln!(msg.red());
        Err(Error::Silent)
    } else {
        // How big is the current cache?
        let cache_size: core::cache::CacheSize = core::cache::size(source)?;
        let size = format!("{}", cache_size.bytes.bytes());
        aln!(fl!(fll, "cache-backup-size", size = size));

        // Is the target directory empty?
        let target_count = target.read_dir().map(|d| d.count()).unwrap_or(0);
        if target_count > 0 {
            aln!(fl!(fll, "cache-backup-nonempty", target = ts).yellow());
        } else {
            aln!(fl!(fll, "cache-backup-target", target = ts));
        }

        // Proceed if the user accepts.
        let msg = format!("{} {} ", fl!(fll, "proceed"), fl!(fll, "proceed-yes"));
        crate::utils::prompt(&a!(msg))?;
        copy(source, &full, cache_size.files)
    }
}

/// Copy all the cache files concurrently.
fn copy(source: &Path, target: &Path, file_count: u64) -> Result<(), Error> {
    debug!("Begin cache copying.");

    // TODO Change the bar style.
    // A progress bar to display the copying progress.
    let pb = Arc::new(Mutex::new(ProgressBar::new(file_count)));

    // Silently succeeds if the directory already exists.
    std::fs::create_dir_all(target)?;

    source
        .read_dir()?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let from = entry.path();
            entry.path().file_name().map(|name| {
                let mut to = target.to_path_buf();
                to.push(name);
                (from, to)
            })
        })
        .par_bridge()
        .for_each(|(from, to)| {
            if std::fs::copy(from, to).is_ok() {
                pb.lock().unwrap().inc();
            }
        });
    Ok(())
}