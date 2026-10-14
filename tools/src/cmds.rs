//! Additional subcommands.
pub fn dispatch(args: &[String]) -> Result<(), String> {
    Err(format!("unknown command {:?}", args[0]))
}
