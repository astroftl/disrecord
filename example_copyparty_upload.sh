#!/bin/bash

if [ "$#" -lt 2 ]; then
    echo "Error: this script requires at least 2 arguments!" >&2
    echo "Usage: $0 <time> <files...>" >&2
    exit 1
fi

time="$1"
shift

for filename in "$@"; do
    if [ ! -f "$filename" ]; then
        echo "Warning: File '$filename' not found. Skipping..." >&2
        continue
    fi

    echo "Uploading '$filename' to folder '$time'..."

    curl -sS --netrc-optional \
         -H "x-auth-request-preferred-username:disrecord" \
         -H "Accept:url" \
         -T "$filename" \
         "http://copyparty:3923/recordings/$time/"
done

# the stdout of this script is what gets sent in the Discord message!
echo "Recording files available at: https://public.copyparty.url/recordings/$time/"
