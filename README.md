# Environment Variables
- `APP_ID`: the APP ID from Discord
- `BOT_TOKEN`: the bot token from Discord
- `LOG_LEVEL`: the log level of Disrecord (default: `trace`)
- `LOG_LEVEL_ALL`: the log level of all other applications (default: `warn`)
- `RECORD_DIR`: the output directory of recordings

An example `.env` file is provided at [`.env.example`](.env.example)

# Example Docker Compose
```yaml
services:
  disrecord:
    build:
      context: .
      target: final
    container_name: disrecord
    environment:
      - APP_ID=01234567890
      - BOT_TOKEN=abcdefghijklmnopqrstuvwxyz0123456789
      - LOG_LEVEL=trace # optional, trace is the default
      - LOG_LEVEL_ALL=warn # optional, warn is the default
    volumes:
      - ./recordings:/recordings # /recordings is baked into the image as RECORD_DIR
    restart: unless-stopped
```