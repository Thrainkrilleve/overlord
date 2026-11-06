# Contributing to Overlord

Thanks for helping. Overlord is a Discord security and moderation bot, so changes are reviewed with extra care around permissions, bans, and stored user data.

## Before you start

- Use your own Discord application and a test server. Do not point a development build at someone else's production bot token.
- Never commit `.env`, tokens, database files, or real user transcripts.
- Read [SECURITY.md](SECURITY.md) if your change touches auth, moderation, the honeypot, tickets, or stored data.
- Be kind in issues and reviews. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Development setup

You need Rust 1.88 or newer. For a full server install, follow [SETUP.md](SETUP.md).

```
git clone https://github.com/Thrainkrilleve/overlord.git
cd overlord
cp .env.example .env   # add a test bot token and your test server IDs
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

To run locally without Docker: `cargo run --release`. Docker is `docker compose up --build`.

## Pull requests

1. Fork the repo and create a branch from `main`.
2. Keep the change focused. A permission or query change is easier to review on its own than mixed with refactors.
3. Add or update tests when behavior changes. Several modules already have unit tests (`automod`, `anti_raid`, `honeypot` paths, transcripts, verification).
4. Run the same checks CI runs:

```
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

5. Open a pull request against `main` and explain what changed and how you tested it.

Please do not submit generated code dumps with no explanation, or changes whose purpose is to evade moderation or target people.
