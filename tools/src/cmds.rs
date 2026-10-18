//! Subcommand dispatch. Each workstream owns one module.
mod gfx;
mod script;
mod audio;
mod actions;
mod voice;
mod voice_sub;
mod cdda_sub;
mod import;
pub mod gfx_i18n;
pub mod i18n;

pub fn dispatch(args: &[String]) -> Result<(), String> {
    if let Some(r) = gfx::dispatch(args) { return r }
    if let Some(r) = script::dispatch(args) { return r }
    if let Some(r) = audio::dispatch(args) { return r }
    if let Some(r) = actions::dispatch(args) { return r }
    if let Some(r) = i18n::dispatch(args) { return r }
    if let Some(r) = gfx_i18n::dispatch(args) { return r }
    if let Some(r) = voice::dispatch(args) { return r }
    if let Some(r) = import::dispatch(args) { return r }
    Err(format!("unknown command {:?}", args[0]))
}
