use serenity::all::*;

pub async fn handle_ready(ctx: &Context, ready: &Ready, cmd_prefix: &str) {
	tracing::info!("{} is connected! (ID: {})", ready.user.name, ready.user.id);

	// `cmd_prefix` is pre-rendered in main.rs from instance config:
	//   "!m " (default), "!bot " (renamed parent), "!" (flat / empty root).
	// Templating the status keeps the suggestion in sync with whatever
	// users actually type to invoke the help command.
	let status = format!("{cmd_prefix}help or @ me");
	ctx.set_activity(Some(ActivityData::playing(status)));
}
