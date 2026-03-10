use serde_json::{json, Value};

/// Tool definitions matching the TS bot's DeepSeek tool schemas.
pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "Search the web for current information, news, facts, or anything you're unsure about.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "The search query" }
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "play_song",
                "description": "Search for and play a song in the user's voice channel.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Song name, artist, or YouTube URL" }
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "skip",
                "description": "Skip the currently playing song",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "stop",
                "description": "Stop playback, clear the queue, and leave the voice channel",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "pause",
                "description": "Pause the currently playing song",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "resume",
                "description": "Resume paused playback",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "show_queue",
                "description": "Show the current music queue",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "now_playing",
                "description": "Show what song is currently playing",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "shuffle",
                "description": "Shuffle the songs in the music queue",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "set_loop",
                "description": "Set the loop mode for music playback",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "mode": { "type": "string", "enum": ["off", "track", "queue"], "description": "off = no loop, track = repeat current, queue = repeat all" }
                    },
                    "required": ["mode"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "remove_from_queue",
                "description": "Remove a song from the queue by position number",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "position": { "type": "number", "description": "Position in queue (1-based)" }
                    },
                    "required": ["position"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "tempban",
                "description": "Temporarily ban a user. Duration: s=seconds, m=minutes, h=hours, d=days, w=weeks.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "user_id": { "type": "string", "description": "Discord user ID to ban" },
                        "duration": { "type": "string", "description": "Ban duration: e.g. 30m, 2h, 3d, 1w" },
                        "reason": { "type": "string", "description": "Reason for the ban (optional)" }
                    },
                    "required": ["user_id", "duration"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "unban",
                "description": "Unban a user early from a tempban",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "user_id": { "type": "string", "description": "Discord user ID to unban" }
                    },
                    "required": ["user_id"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "nuke",
                "description": "Bulk delete messages from the current channel",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "count": { "type": "number", "description": "Number of messages to delete (1-100)" }
                    },
                    "required": ["count"]
                }
            }
        }),
    ]
}

pub fn is_moderation_tool(name: &str) -> bool {
    matches!(name, "tempban" | "unban" | "nuke")
}

pub fn is_search_tool(name: &str) -> bool {
    name == "web_search"
}
