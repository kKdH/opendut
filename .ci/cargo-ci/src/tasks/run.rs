use std::process::Command;

/// Start the application
#[derive(Clone, clap::Parser)]
pub struct RunCli {
    /// Additional parameters to pass through to the started program
    #[arg(raw=true)]
    pub passthrough: Vec<String>,
}

impl RunCli {
    #[tracing::instrument(name="run", skip(self))]
    pub fn default_handling(&self, package: crate::Package) -> crate::Result {
        Command::new("cargo")
            .args(["run", "--package", &package.ident(), "--"])
            .args(&self.passthrough)
            .status()?;

        Ok(())
    }
}
