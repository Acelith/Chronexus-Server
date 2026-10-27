# Web interface

Open your server's address (for example `https://sync.example.org/`) in a browser and sign in
with your Chronexus account. Notes are written in the desktop app; the web interface manages
what lives on the server.

## What you can do

| Who | Can |
| --- | --- |
| Everyone | See their campaigns and role, what they keep on the server (items per kind, highest revision, last accepted save, images) and the server's limits; sign out devices |
| Owner | Switch members between player and reader, remove members, create and revoke invitations, delete the campaign on the server (typing its exact name) |
| Player, reader | Leave a campaign |
| Operator | See service status (version, schema, database size, counts) and account metadata (sessions, campaign count, stored bytes) |

Every destructive action first explains what happens to local copies, offline devices and
existing backups, and asks for confirmation.

The page shows what the **server** holds. It cannot know changes a device has not sent yet:
check the status in the desktop app for that.

## Operators

Whoever controls the server's `.env` decides the operators:

```dotenv
CHRONEXUS_OPERATORS=your-user-name,another-user
```

The operator role gives **no** access to campaign content in the interface or the API: no
campaign names, members or items. (Whoever controls the host can still read the database
file; that is outside the application.)

## Security

- Static pages served by the server with a strict Content Security Policy: only its own
  scripts and styles, no framing, no third-party content.
- Same JSON API and permission checks as the desktop. The session token stays in the
  browser tab and ends when you sign out or close the tab. No cookies, so no cross-site
  request forgery; requests from another origin are refused.
- Names and other text from campaigns are always shown as plain text.

The interface is in English for now.
