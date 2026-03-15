pub mod admin;
pub mod connections;
pub mod help;
pub mod moderation;
pub mod music;
pub mod stocks;
pub mod wordle;
pub mod minecraft;

use crate::error::BotError;
use crate::Data;

/// Parent `m` command with all subcommands
#[poise::command(
    prefix_command,
    subcommands(
        "music::play",
        "music::playlist",
        "music::skip",
        "music::stop",
        "music::pause",
        "music::resume",
        "music::queue",
        "music::nowplaying",
        "music::remove",
        "music::loop_cmd",
        "music::shuffle",
        "moderation::ban",
        "moderation::unban",
        "moderation::banlist",
        "moderation::nuke",
        "admin::setlog",
        "admin::djrole",
        "admin::djmode",
        "stocks::stock",
        "connections::connections",
        "wordle::wordle",
        "help::help",
    )
)]
pub async fn m(
    _ctx: poise::Context<'_, Data, BotError>,
) -> Result<(), BotError> {
    // Parent command does nothing — subcommands handle everything
    Ok(())
}
