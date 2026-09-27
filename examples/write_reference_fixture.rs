//! Write an opt-in disposable project for user-run native acceptance.
//! Usage: cargo run --locked --example write_reference_fixture -- <new.pmcab>
//! Existing destinations are never overwritten; no app preferences are read.
use plan_my_cabinet::{persistence, reference_fixture};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("provide a new .pmcab destination")?);
    if args.next().is_some() || path.extension().is_none_or(|ext| ext != "pmcab") {
        return Err("usage: write_reference_fixture <new.pmcab>".into());
    }
    let bytes = persistence::serialize(&reference_fixture::project())?;
    let mut editor = persistence::prepare_bytes(&bytes)?.into_editor();
    persistence::save_new(&mut editor, &path)?;
    println!("{}", path.display());
    Ok(())
}
