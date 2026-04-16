# Virtual Stock Trading

A play-money trading game backed by real-time stock quotes. Every user
in a guild gets a virtual `$1,000` portfolio, can buy and sell US-listed
stocks at live prices, and can compete against the rest of the server on
a leaderboard ranked by total portfolio value.

## What it does

- Each `(guild, user)` pair has its own portfolio: a cash balance and a
  set of holdings.
- Quotes come from the [Finnhub](https://finnhub.io) `/quote` endpoint,
  using your free or paid `FINNHUB_API_KEY`.
- Buys and sells settle at the live quote at the moment the order
  executes (no slippage simulation, no order book).
- Realized profit and loss is computed on each sell, using the
  weighted-average cost basis tracked in the database.
- A per-server leaderboard ranks the top ten portfolios by total value
  (cash + holdings at current prices).
- A trade history shows your last ten transactions with timestamps.

The game is **per-guild**, so the same Discord user has independent
portfolios on each server they're in.

## Activation

Stocks is always-on in the sense that the `!m stock` command tree is
always registered. Every command requires `FINNHUB_API_KEY` in the
environment, however. Without it, every subcommand returns:

> Stock trading is not configured. The bot owner needs to set
> `FINNHUB_API_KEY`.

To activate the feature you need exactly two things:

1. A Finnhub account. The free tier is generous (60 calls/minute) and
   covers all US equities.
2. The key in your bot's `.env` as `FINNHUB_API_KEY=…`.

Restart the bot after editing `.env`.

## Commands

All commands live under the `m` parent. With the default `!` prefix
that means **`!m stock <subcommand>`**. The prefix is configurable per
instance.

| Command | Aliases | Description |
|---|---|---|
| `!m stock` | `!m stocks`, `!m st` | Bare command; shows your portfolio. |
| `!m stock buy <ticker> <qty\|$amount>` | `!m stock b` | Buy shares. `qty` is a share count (`10`, `0.5`); `$amount` (with literal `$`) buys whatever fraction of a share that dollar amount maps to at the current quote. |
| `!m stock sell <ticker> <qty\|all>` | `!m stock s` | Sell shares. `all` sells your full position. |
| `!m stock portfolio [@user]` | `!m stock port`, `!m stock pf`, `!m stock p` | Show your own portfolio, or somebody else's if you mention them. |
| `!m stock price <ticker>` | `!m stock quote`, `!m stock q` | Look up the current quote. No portfolio needed. |
| `!m stock leaderboard` | `!m stock lb`, `!m stock top` | Top 10 portfolios by total value. |
| `!m stock history` | `!m stock hist`, `!m stock h` | Your last 10 trades. |
| `!m stock reset confirm` | — | Wipe your holdings and history; reset cash to `$1,000`. Requires the literal word `confirm`. |

Tickers are case-insensitive and normalized to uppercase. The Finnhub
universe is **US-listed equities** — `AAPL`, `MSFT`, `TSLA`,
`NVDA` and so on. Crypto, forex, indices, and non-US listings will
return "Could not find stock symbol".

## Examples

```
!m stock buy AAPL 5            # buy 5 whole shares of Apple
!m stock buy NVDA $250         # spend $250 on Nvidia (fractional shares OK)
!m stock sell AAPL 2           # sell 2 shares
!m stock sell NVDA all         # close the Nvidia position entirely
!m stock price MSFT            # look up Microsoft, no trade
!m stock portfolio @someone    # peek at another user's portfolio
!m stock leaderboard           # see the top 10
!m stock reset confirm         # nuclear option
```

The bot can also drive these commands via the AI tool layer: ask the
bot in plain language ("buy me $500 of Tesla") and the AI invokes
`stock_buy` with the right arguments. See
[AI Chat](ai-chat.md#tool-use).

## How it works

### Starting balance

The first time you touch any stock command, the bot calls
`get_or_create_portfolio` and seeds your account with `$1,000.00` in
cash. From that point on, your balance changes only via buys, sells,
and `!m stock reset`.

### Quotes and caching

Every command that needs a live price calls
`stocks::api::get_quote`. The function:

1. Checks the database price cache for that symbol.
2. If a fresh entry exists, returns it immediately. Otherwise hits
   `https://finnhub.io/api/v1/quote?symbol=<TICKER>` with the API key
   in the `X-Finnhub-Token` header.
3. Caches the result on the way back.

The cache is what keeps you under the Finnhub rate limit when a busy
server pulls the leaderboard or many people check the same stock.
Quotes contain three numbers: current price, previous close, and the
day's percent change.

### Buy and sell semantics

A buy debits cash, increments the holding, and updates the holding's
weighted-average `avg_cost`. A sell credits cash and computes the
realized P/L for that sale: `(sell_price − avg_cost) × quantity`.

Buys are sized one of two ways:

- **By share count**: `!m stock buy AAPL 10` — exactly ten shares.
  Fractional counts are allowed (`!m stock buy AAPL 0.5`).
- **By dollar amount**: `!m stock buy AAPL $500` — spend `$500`,
  receive `$500 / current_price` shares. Always fractional.

If you don't have the cash, the bot replies "Insufficient funds"
and the trade is rejected. If you try to sell more than you own,
the bot tells you your actual share count and refuses the trade.

### Portfolio embed

`!m stock portfolio` (or just `!m stock`) shows:

- Your cash balance.
- Each holding: ticker, share count, current price, market value,
  P/L percent.
- Total value (cash + market value of holdings).
- Total P/L versus the original `$1,000` baseline, in both dollars
  and percent.

The embed border colour reflects whether you're up (green) or down
(red) overall.

### Leaderboard

`!m stock leaderboard` walks every portfolio in the guild, fetches a
current quote for every holding, and ranks the top ten by total
value. The P/L column is `total − $1,000`, since that's the
baseline everybody started from. The top three get medals
(🥇🥈🥉).

On large servers with many active portfolios this command does a
lot of API calls. The price cache absorbs most of it; if you find
yourself rate-limited, slow it down or switch to a paid Finnhub
tier.

### History

`!m stock history` shows your last ten transactions: buy or sell,
share count, price, total amount, and a relative timestamp. Older
trades are still in the database — the embed is just capped at ten
for readability.

### Reset

`!m stock reset` is destructive. The first call gets a confirmation
prompt; only `!m stock reset confirm` actually wipes the account.
A reset clears all holdings and trade history and resets cash to
`$1,000.00`. You can reset as often as you like.

## Permissions

There are no Discord permission gates on the stock commands. Anyone
in a guild can play. If you want to restrict it, gate the channel
itself with role permissions.

## Common issues

- **"Stock trading is not configured"** — `FINNHUB_API_KEY` is
  missing or empty. Set it and restart.
- **"Could not find stock symbol *X*"** — Finnhub returned `0`
  for that ticker. Verify on [finnhub.io](https://finnhub.io);
  crypto and non-US equities aren't on the free tier.
- **"Finnhub API returned status 429"** — rate-limited. Wait
  a minute or upgrade your Finnhub tier.
- **Stale price** — the quote came from cache. Trades use a
  fresh quote every time.
- **Fractional-share rounding looks weird** — share counts are
  stored to four decimals; values can drift by a fraction of a
  cent on round trips.
- **Lost my position after `reset`** — irreversible by design.
- **Markets closed** — Finnhub returns the last traded price.
  Trades still execute; there's no "market closed" rejection.

## Cross-references

- [Environment Variables](../configuration/environment-variables.md) —
  `FINNHUB_API_KEY`.
- [AI Chat](ai-chat.md#tool-use) — the `stock_buy`, `stock_sell`,
  `stock_price`, `stock_portfolio`, and `stock_leaderboard` tools.
- [Wordle](games-wordle.md) and [Connections](games-connections.md) —
  the other always-on game features.
