# Operator commands

Run inside the container: `docker compose exec -T server chronexus-server <command>`
(`restore` with the service stopped: `docker compose run --rm --no-deps server …`).

| Command | What it does |
| --- | --- |
| `serve` | Starts the server (the container's default) |
| `invite` | Prints a one-use, 24-hour server invitation |
| `recover USERNAME` | Prints a one-use, one-hour recovery code for that account |
| `backup FILE` | Consistent, verified copy of the database; never overwrites |
| `verify FILE` | Checks a backup without changing anything |
| `restore FILE` | Replaces the database with a verified backup; revokes all sessions |
| `revoke-sessions` | Ends every session (for example after restoring an old backup) |
| `compact [DAYS]` | Drops tombstones older than DAYS (default 180) |
| `healthcheck` | Used by the Compose healthcheck |
| `openapi` | Prints the API contract |

Health endpoints: `GET /health/live` and `GET /health/ready` (loopback only).
