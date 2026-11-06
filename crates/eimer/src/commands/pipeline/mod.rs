//! `eimer pipeline`: placeholder until the port lands.

use anyhow::{Result, bail};
use clap::Args;

#[derive(Args)]
pub struct PipelineArgs {}

pub fn run(_args: PipelineArgs) -> Result<()> {
    bail!("'pipeline' is not ported yet")
}
