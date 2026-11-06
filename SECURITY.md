# Security Policy

Overlord can ban, kick, timeout, read messages, and store ticket transcripts. Treat vulnerabilities as high priority.

## Supported versions

Only the latest commit on `main` is supported. If you are not running that, upgrade before reporting unless the issue clearly still applies.

## Reporting a vulnerability

**Do not open a public issue, pull request, or Discord post.**

Use GitHub's private reporting form:

https://github.com/Thrainkrilleve/overlord/security/advisories/new

Please include:

- A description of the issue and its impact
- Steps to reproduce, or a proof of concept
- The commit hash or release you tested
- Whether you have already shared it with anyone else

You should get an acknowledgement within 7 days. Please give us a chance to fix and release a patch before any public disclosure. If we confirm the report, we will credit you unless you ask us not to.

If private reporting is unavailable, open a [blank issue](https://github.com/Thrainkrilleve/overlord/issues/new) titled "Security: please contact me" with no technical details, and we will follow up privately.

## In scope

- Bypassing verification, role checks, or the staff exemption
- Causing Overlord to ban, kick, timeout, or purge without authorization
- Getting around the honeypot's staff protection, or tricking it into punishing the wrong people
- Reading or leaking ticket transcripts, infraction records, or other stored member data
- Stealing, logging, or exposing `DISCORD_TOKEN` or other secrets
- Injection against the SQLite database or other untrusted input that changes bot behavior
- Ways for a person or another bot to make Overlord take privileged actions on their behalf

## Out of scope

- Self-hosting mistakes, such as publishing `.env`, inviting the bot with Administrator, or putting the honeypot where ordinary members can see it
- Discord platform bugs, rate limits, or API outages
- The bot acting as designed: honoring a documented staff command, or banning someone who posted in a correctly configured honeypot
- Denial of service against Discord itself

## Handling secrets and reports

Never commit `.env`, bot tokens, session files, or `*.db` files. If a token may have leaked, reset it in the [Discord Developer Portal](https://discord.com/developers/applications) immediately.

When you report a problem, redact tokens, invite links you do not mean to share, and other people's message content or personal data.
