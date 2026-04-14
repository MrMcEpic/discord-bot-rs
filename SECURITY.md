# Security Policy

## Supported Versions

Only the latest release is supported with security updates. If you're running an older version, please upgrade before reporting a vulnerability.

| Version | Supported          |
| ------- | ------------------ |
| Latest  | :white_check_mark: |
| Older   | :x:                |

## Reporting a Vulnerability

**Please do not open public issues for security vulnerabilities.**

Use GitHub's private vulnerability reporting:

1. Go to https://github.com/MrMcEpic/discord-bot-rs/security/advisories
2. Click "Report a vulnerability"
3. Fill out the form with as much detail as possible

Alternatively, you can contact the maintainer directly through GitHub.

## What to Include

A good report has:

- A clear description of the vulnerability
- The affected component(s) — bot crate, mcp-gateway, specific feature module, etc.
- Steps to reproduce (or a proof-of-concept)
- The impact (what can an attacker do?)
- Your suggested fix, if you have one

## Response Expectations

- **Acknowledgment** within 72 hours
- **Initial assessment** within one week
- **Fix or mitigation plan** within two weeks for high-severity issues

This is a personal/community project, not a funded enterprise product. Response times are best-effort, but reports are taken seriously.

## Scope

In scope:

- The main `discord-bot` crate and everything under `src/`
- The `mcp-gateway` crate
- The Docker build and compose configuration
- Default configuration templates

Out of scope:

- Dependency vulnerabilities without a direct exploit against this project (file with the upstream project instead)
- Social engineering of maintainers
- Issues in third-party services (Discord, DeepSeek, Gemini, Finnhub, etc.)
- Self-inflicted issues from modified forks or custom configurations

## Credit

Reporters who want public credit are named in release notes after the fix ships. Reporters who prefer to stay anonymous are respected.
