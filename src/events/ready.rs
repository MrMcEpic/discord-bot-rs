use serenity::all::*;

pub async fn handle_ready(ctx: &Context, ready: &Ready) {
    tracing::info!("{} is connected! (ID: {})", ready.user.name, ready.user.id);

    ctx.set_activity(Some(ActivityData::playing("!m help or @ me")));
}
