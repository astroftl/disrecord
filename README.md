# Environment Variables
- `LOG_LEVEL`: the log level of Disrecord (default: `trace`)
- `LOG_LEVEL_ALL`: the log level of all other applications (default: `warn`)

Additionally, for a minimal setup, these are required:
- `APP_ID`: the APP ID from Discord
- `BOT_TOKEN`: the bot token from Discord

And these are optional:
- `RECORD_DIR`: the directory to record into; per-server and per-recording subdirectories will be created (default: `/recordings` in Docker, `./recordings` standalone)
- `DATE_FORMAT`: the format string to use for dates (default: `%Y_%m_%d_%H_%M_%S`)
- `ZIP_FILES`: whether to create a .zip of all the recorded files (default: false) (`bool`)
- `EMBED_FILES`: whether to upload the files as an embeded Discord attachment (default: false) (`bool`)
- `POST_CMD_PER_USER`: command to run on each per-user recorded file (default: none)
- `POST_CMD_PER_MIX`: command to run on each per-recording mix; there is always a master mix (default: none)
- `POST_CMD_ZIP`: command to run on the generated .zip file, if there is one (default: none)
- `POST_CMD_ALL`: command to run as a batch over all recording artifacts (default: none)

For all `bool` values, case-insensitive `false`, `no`, and `0` are falsey, as is the var being unset. All other values are truthy.

For more thorough configuration, see the [Configuration section](#Configuration).

# Post Commands
Several post commands are allowed, which will be run after the recording is finished.

The following template values are allowed:
- `{guild}`: the server ID
- `{time}`: the timestamp the recording was started at (formatted with `DATE_FORMAT`)

Additionally, for the per-user, per-mix, and zip post commands:
- `{file}`: the full path of the file
- `{filename}`: the filename of the file

For `POST_CMD_ALL`:
- `{files}`: all of the files, quoted (ex: `"path/to/user1.opus" "path/to/user2.opus" "path/to/recording.zip"`

An example script to upload files to a copyparty instance is found at [example_copyparty_upload.sh]

# Configuration
For much more flexibility, a configuration YAML may be provided. When a config file is present, all environment variables (except for `LOG_LEVEL` and `LOG_LEVEL_ALL`) are overwritten.

The config file allows you to specify per-server overrides of the output settings. When one of these is set for a server, ALL of `default_output` is overridden. Any values not provided in that server override are unset (unset bools evaulate to false).

```yaml
bot_token: put_your_bot_token_here
app_id: 1234567890

record_dir: /recordings
date_format: '%Y_%m_%d_%H_%M_%S'

default_output:
  zip_files: true
  embed_files: true
  # also available: {guild} and {time}
  post_cmd_per_user: echo "{filename}" # runs on each of the per-user streams
  post_cmd_per_mix: ls {file} # runs on each of the mixes (master mix always + any custom mixes)
  post_cmd_zip: ls {file} # runs on the zip (if created)
  post_cmd_all: ls {files} # runs on all of the above in a batch (note {files}, plural; also no {filename})

# server outputs replace ALL fields from default_output
# values not provided are unset
server_output:
  0123456789:
    embed_files: false
    post_cmd_per_mix: curl -T {file} http://localhost
    post_cmd_all: /config/do_something_special.sh {files}
    custom_mixes:
      quieter:
      - !Exclude
        - annoying_person
```

# Installation
The suggested method of installing disrecord is via Docker. This image is not published. To use, pull this repo and run `docker compose build`. See the [compose.yml] for more details.

To get started quickly:
- `mkdir recordings`
- `mkdir config`
- `cp example_config.yml config/config.yml`
- edit `config.yml` to taste (the `app_id` and `bot_token` MUST be set)
