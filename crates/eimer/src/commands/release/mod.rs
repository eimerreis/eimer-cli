//! `eimer release`: placeholder until the port lands.

use anyhow::{Result, bail};
use clap::Args;

#[derive(Args)]
pub struct ReleaseArgs {}

pub fn run(_args: ReleaseArgs) -> Result<()> {
    bail!("'release' is not ported yet")
}
