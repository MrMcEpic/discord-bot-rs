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
        json!({
            "type": "function",
            "function": {
                "name": "stock_buy",
                "description": "Buy shares of a stock for the user using their virtual portfolio. Provide either quantity OR dollar_amount, not both.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string", "description": "Stock ticker symbol (e.g. AAPL, TSLA, MSFT)" },
                        "quantity": { "type": "number", "description": "Number of shares to buy (use this OR dollar_amount)" },
                        "dollar_amount": { "type": "number", "description": "Dollar amount to spend (use this OR quantity)" }
                    },
                    "required": ["symbol"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "stock_sell",
                "description": "Sell shares of a stock from the user's virtual portfolio",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string", "description": "Stock ticker symbol" },
                        "quantity": { "type": "number", "description": "Number of shares to sell. Omit to sell all shares." }
                    },
                    "required": ["symbol"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "stock_price",
                "description": "Check the current price of a stock",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string", "description": "Stock ticker symbol" }
                    },
                    "required": ["symbol"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "stock_portfolio",
                "description": "View the user's virtual stock portfolio with holdings, cash balance, and profit/loss",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "stock_leaderboard",
                "description": "Show the server's stock trading leaderboard — top portfolios ranked by total value",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "connections_start",
                "description": "Start a NYT Connections puzzle game in the current channel. Players work together to find groups of 4 related words.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "mode": { "type": "string", "enum": ["today", "random"], "description": "today = today's puzzle, random = random historical puzzle" }
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "wordle_start",
                "description": "Start a Wordle game in the current channel. Players guess a 5-letter word in 6 tries by typing words in chat.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "mode": { "type": "string", "enum": ["today", "random"], "description": "today = today's puzzle, random = random historical puzzle" }
                    }
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

pub fn is_stock_tool(name: &str) -> bool {
    matches!(
        name,
        "stock_buy" | "stock_sell" | "stock_price" | "stock_portfolio" | "stock_leaderboard"
    )
}

pub fn is_connections_tool(name: &str) -> bool {
    name == "connections_start"
}

pub fn is_wordle_tool(name: &str) -> bool {
    name == "wordle_start"
}
