# Server AI

An optional assistant that runs **on the server**, for groups where not every player has a
computer strong enough for local AI. It is off unless the operator turns it on, and each
player decides per campaign in the desktop app whether to use it.

## For players

In the app: *Campaign settings → Where the assistant runs → On the campaign's sharing
server*, press **Check the server AI**, pick a model and **Use this choice**. The assistant
view then sends your question to the server.

- Only what you can read on the server in that campaign is used: what you synchronized
  (private copies included) and what others share. Notes kept only on your computer are not.
- Answers are drafts with sources (title, author, revision). Check them before relying on them.
- If the server is busy, over your hourly limit or unavailable, the app says so and offers
  **Try again** or **Switch this campaign to local AI**. It never switches by itself.
- The server keeps neither your questions nor the answers.

## For operators

Start the runtime with `docker compose --profile ai up -d`, pull a model yourself, then set
`CHRONEXUS_AI_URL` and `CHRONEXUS_AI_MODELS` in `.env`. The runtime has no internet access
and no published port. Limits (questions per hour, parallel answers, queue, timeout) are
configurable. Full steps and safeguards: [docs/AI.md](https://github.com/Acelith/Chronexus-Server/blob/main/docs/AI.md).

Model quality and hardware sizing are **not qualified yet**: treat it as experimental.
