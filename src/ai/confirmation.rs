use serenity::all::*;
use std::time::Duration;

use crate::error::BotError;

const CONFIRM_TIMEOUT: Duration = Duration::from_secs(30);

fn describe_action(name: &str, args: &serde_json::Value) -> String {
    match name {
        "tempban" => {
            let user_id = args["user_id"].as_str().unwrap_or("?");
            let duration = args["duration"].as_str().unwrap_or("?");
            let reason = args["reason"].as_str();
            let mut desc = format!("**Tempban** <@{user_id}> for `{duration}`");
            if let Some(r) = reason {
                desc.push_str(&format!(" — {r}"));
            }
            desc
        }
        "unban" => {
            let user_id = args["user_id"].as_str().unwrap_or("?");
            format!("**Unban** <@{user_id}>")
        }
        "nuke" => {
            let count = args["count"].as_u64().unwrap_or(0);
            format!("**Nuke** {count} message{}", if count == 1 { "" } else { "s" })
        }
        _ => format!("**{name}**"),
    }
}

fn required_permission(name: &str) -> Permissions {
    match name {
        "tempban" | "unban" => Permissions::BAN_MEMBERS,
        "nuke" => Permissions::MANAGE_MESSAGES,
        _ => Permissions::ADMINISTRATOR,
    }
}

/// Show a confirmation embed with Approve/Cancel buttons.
/// Returns true if approved by a user with the required permission.
pub async fn request_confirmation(
    ctx: &serenity::client::Context,
    channel_id: ChannelId,
    message: &Message,
    name: &str,
    args: &serde_json::Value,
) -> Result<bool, BotError> {
    let required_perm = required_permission(name);

    // Pre-check permission
    if let Some(member) = &message.member {
        let perms = member.permissions.unwrap_or(Permissions::empty());
        if !perms.contains(required_perm) {
            channel_id.say(&ctx.http, "You don't have permission to do that.").await?;
            return Ok(false);
        }
    }

    let confirm_id = format!("mod_confirm_{}", chrono::Utc::now().timestamp_millis());
    let deny_id = format!("mod_deny_{}", chrono::Utc::now().timestamp_millis());

    let description = describe_action(name, args);
    let requester_name = message.member.as_ref()
        .and_then(|m| m.nick.as_deref())
        .unwrap_or(&message.author.name);

    let embed = CreateEmbed::new()
        .color(0xfee75c)
        .title("Confirm Mod Action")
        .description(&description)
        .footer(CreateEmbedFooter::new(format!("Requested by {requester_name} · Expires in 30s")));

    let buttons = vec![CreateActionRow::Buttons(vec![
        CreateButton::new(&confirm_id).label("Approve").style(ButtonStyle::Success),
        CreateButton::new(&deny_id).label("Cancel").style(ButtonStyle::Danger),
    ])];

    let reply = CreateMessage::new()
        .embed(embed)
        .components(buttons)
        .reference_message(MessageReference::from((channel_id, message.id)));

    let mut confirm_msg = channel_id.send_message(&ctx.http, reply).await?;

    let interaction = confirm_msg
        .await_component_interaction(ctx.shard.clone())
        .timeout(CONFIRM_TIMEOUT)
        .custom_ids(vec![confirm_id.clone(), deny_id.clone()])
        .await;

    match interaction {
        Some(interaction) => {
            let has_perm = interaction.member.as_ref()
                .and_then(|m| m.permissions)
                .map_or(false, |p| p.contains(required_perm));

            if !has_perm {
                let _ = interaction.create_response(&ctx.http, CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("You don't have permission to approve this action.")
                        .ephemeral(true),
                )).await;
                return Ok(false);
            }

            let approved = interaction.data.custom_id == confirm_id;
            let responder_name = interaction.member.as_ref()
                .and_then(|m| m.nick.as_deref())
                .or(interaction.user.global_name.as_deref())
                .unwrap_or(&interaction.user.name);

            let (color, title, footer_text) = if approved {
                (0x57f287, "Approved", format!("Approved by {responder_name}"))
            } else {
                (0xed4245, "Cancelled", format!("Cancelled by {responder_name}"))
            };

            let update_embed = CreateEmbed::new()
                .color(color)
                .title(title)
                .description(&description)
                .footer(CreateEmbedFooter::new(footer_text));

            let _ = interaction.create_response(&ctx.http, CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new().embed(update_embed).components(vec![]),
            )).await;

            Ok(approved)
        }
        None => {
            let expired_embed = CreateEmbed::new()
                .color(0x95a5a6)
                .title("Expired")
                .description(&description)
                .footer(CreateEmbedFooter::new("No response — action cancelled"));

            let _ = confirm_msg.edit(&ctx.http, EditMessage::new().embed(expired_embed).components(vec![])).await;
            Ok(false)
        }
    }
}
