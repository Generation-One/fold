#!/bin/bash

# Create admin user and API token for Fold
# Reads configuration from .env file in the srv root directory

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRV_ROOT="$(dirname "$SCRIPT_DIR")"
ENV_PATH="${SRV_ROOT}/.env"

ADMIN_EMAIL="${1:-admin@fold.local}"
DISPLAY_NAME="${2:-Admin User}"

# Load environment variables from .env
load_env() {
    local env_file="$1"
    if [ ! -f "$env_file" ]; then
        echo "Warning: .env file not found at $env_file" >&2
        return 1
    fi

    # Source the .env file, handling comments and empty lines
    while IFS='=' read -r key value; do
        key=$(echo "$key" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
        value=$(echo "$value" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

        if [ -n "$key" ] && [[ ! "$key" =~ ^# ]]; then
            export "$key"="$value"
        fi
    done < "$env_file"
}

# Load environment
load_env "$ENV_PATH"

# Determine database path
DB_PATH="${DATABASE_PATH:-./data/fold.db}"

# Resolve relative paths relative to srv root
if [[ ! "$DB_PATH" = /* ]]; then
    DB_PATH="${SRV_ROOT}/${DB_PATH}"
fi

echo "Using database: $DB_PATH" >&2

# Check if database exists
if [ ! -f "$DB_PATH" ]; then
    echo "Error: Database not found at $DB_PATH" >&2
    exit 1
fi

# Generate UUIDs for user and token
USER_ID=$(python3 -c "import uuid; print(str(uuid.uuid4()))")
TOKEN_ID=$(python3 -c "import uuid; print(str(uuid.uuid4()))")

# Generate token in format: fold_{8-char-prefix}_{32-char-secret}
# Using alphanumeric characters (base62)
PREFIX=$(python3 -c "import secrets, string; chars = string.ascii_letters + string.digits; print(''.join(secrets.choice(chars) for _ in range(8)))")
SECRET=$(python3 -c "import secrets, string; chars = string.ascii_letters + string.digits; print(''.join(secrets.choice(chars) for _ in range(32)))")
TOKEN="fold_${PREFIX}_${SECRET}"

# Hash the full token with SHA256
HASH=$(echo -n "$TOKEN" | sha256sum | cut -d' ' -f1)

# Create admin user (role='admin')
CREATE_USER_SQL="INSERT INTO users (id, provider, subject, email, display_name, role, created_at, updated_at)
VALUES ('$USER_ID', 'local', 'admin', '$ADMIN_EMAIL', '$DISPLAY_NAME', 'admin', datetime('now'), datetime('now'));"

# Create API token for admin user
CREATE_TOKEN_SQL="INSERT INTO api_tokens (id, user_id, name, token_hash, token_prefix, project_ids, created_at)
VALUES ('$TOKEN_ID', '$USER_ID', 'Admin Token', '$HASH', '$PREFIX', '[]', datetime('now'));"

if sqlite3 "$DB_PATH" "$CREATE_USER_SQL" 2>&1; then
    echo "Admin user created successfully!" >&2
else
    echo "Error: Failed to create admin user" >&2
    exit 1
fi

if sqlite3 "$DB_PATH" "$CREATE_TOKEN_SQL" 2>&1; then
    echo "API token created successfully!" >&2
else
    echo "Error: Failed to create API token" >&2
    exit 1
fi

echo "" >&2
echo "FOLD_TOKEN=$TOKEN" >&2
echo "" >&2
echo "User ID: $USER_ID" >&2
echo "Token ID: $TOKEN_ID" >&2
echo "Email: $ADMIN_EMAIL" >&2
echo "" >&2
echo "Set this in your environment:" >&2
echo "export FOLD_TOKEN=\"$TOKEN\"" >&2
echo "" >&2

# Output token on stdout for scripting
echo "$TOKEN"
