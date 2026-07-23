#!/bin/sh
set -eu

: "${DATABASE_URL:?DATABASE_URL is required}"
: "${COWIKI_REPO_ROOT:?COWIKI_REPO_ROOT is required}"
: "${COWIKI_PUBLIC_ORIGIN:?COWIKI_PUBLIC_ORIGIN is required}"
: "${COWIKI_TOKEN_PEPPER:?COWIKI_TOKEN_PEPPER is required}"
: "${GITHUB_CLIENT_ID:?GITHUB_CLIENT_ID is required}"
: "${GITHUB_CLIENT_SECRET:?GITHUB_CLIENT_SECRET is required}"

if [ ! -d "$COWIKI_REPO_ROOT" ] || [ ! -w "$COWIKI_REPO_ROOT" ]; then
  echo "COWIKI_REPO_ROOT must be an existing writable directory: $COWIKI_REPO_ROOT" >&2
  exit 1
fi

exec /usr/bin/tini -- /usr/local/bin/cowiki-cloud
