//! `eimer pr`: placeholder until the port lands.

use anyhow::{Result, bail};
use clap::Args;

#[derive(Args)]
pub struct PrArgs {}

pub fn run(_args: PrArgs) -> Result<()> {
    bail!("'pr' is not ported yet")
}
