//! Subcommand dispatch. Each workstream owns one module.
mod gfx;
mod script;
mod audio;

pub fn dispatch(args: &[String]) -> Result<(), String> {
    if let Some(r) = gfx::dispatch(args) { return r }
    if let Some(r) = script::dispatch(args) { return r }
    if let Some(r) = audio::dispatch(args) { return r }
    Err(format!("unknown command {:?}", args[0]))
}
