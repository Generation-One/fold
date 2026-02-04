# Fold Tools

Utility scripts for managing Fold server administration.

## create-admin

Creates an admin user and generates an API token for Fold.

Both scripts read configuration from the `.env` file in the srv root directory, specifically the `DATABASE_PATH` variable.

### PowerShell (Windows)

```powershell
cd srv/tools
.\create-admin.ps1 -AdminEmail "admin@example.com" -DisplayName "Admin Name"
```

Options:
- `-AdminEmail`: Email address for the admin user (default: `admin@fold.local`)
- `-DisplayName`: Display name for the admin user (default: `Admin User`)
- `-EnvPath`: Path to .env file (default: `../..env`, i.e., srv root)

### Bash (Unix/Linux/macOS)

```bash
cd srv/tools
./create-admin.sh [admin-email] [display-name]
```

Arguments:
- First argument: Email address for the admin user (default: `admin@fold.local`)
- Second argument: Display name for the admin user (default: `Admin User`)

The script reads the `.env` file from the srv root automatically.

### Output

Both scripts output:
- Admin user ID (UUID)
- API token ID (UUID)
- Email address
- **API token** - Use this token for API requests as a Bearer token

The token is also exported to the environment as `FOLD_TOKEN`.

### Example Usage

**PowerShell:**
```powershell
.\create-admin.ps1 -AdminEmail "you@example.com" -DisplayName "Your Name"
# Output includes: FOLD_TOKEN=fold_xxxxxxxxxxxx
$env:FOLD_TOKEN = "fold_xxxxxxxxxxxx"
```

**Bash:**
```bash
TOKEN=$(./create-admin.sh "you@example.com" "Your Name")
export FOLD_TOKEN="$TOKEN"
```

### Prerequisites

- SQLite3 command-line tool (must be in PATH)
- For Bash: Python3 (for UUID and random token generation)

### Configuration

The scripts read the `DATABASE_PATH` from `.env`:

```env
DATABASE_PATH=./data/fold.db
```

If not set, it defaults to `./data/fold.db` relative to the srv root.
