# Accounts and invitations

- **Invitation only.** The operator runs `chronexus-server invite`; the printed code works
  once, for 24 hours. A campaign owner can also create campaign invitations from the app.
- **User names:** 3–64 characters (letters, digits, `.`, `_`, `-`), case-insensitive.
  **Passwords:** 12–128 characters. Passwords are stored only as Argon2id hashes.
- **Attempt limits:** five per account or code and thirty per client address in fifteen
  minutes, kept across restarts.
- **Sessions:** one per device; they expire after seven days of inactivity and thirty days
  at most; up to twenty active sessions per account. Users can list and revoke their
  devices.
- **Lost password:** the operator verifies the person outside the service and runs
  `chronexus-server recover USERNAME`; the one-hour recovery code sets a new password and
  ends every old session.
- **Deleting an account** is done by its owner from the app, after the password check.
  Owned campaigns shared with others are deleted only with explicit confirmation.
