# Optional server AI (M25.10)

Status: the hardware-independent part is implemented and tested; **GPU and model qualification
is open**. Nothing here downloads a model, and the server answers no question until an
operator has installed one and named it in the configuration.

## What it does

A campaign member whose desktop campaign is set to "On the campaign's sharing server" can ask
a question. The server answers with a local Ollama runtime that it reaches over a private
Compose network. It uses only the items that account can read in that campaign: the
member's own items, private copies included, and what other members shared. Notes that never
left the desktop are not involved. The answer is an unverified draft with citations (item,
revision, author); it never changes the campaign.

The server does not store questions, answers or prompts and does not log them. Choosing the
server is per campaign on each desktop; the default stays local AI.

## Turning it on

1. Make sure the host has enough memory, and preferably a GPU (see "Qualification").
2. Start the runtime: `docker compose --profile ai up -d`. With an NVIDIA GPU and the
   NVIDIA Container Toolkit, add `-f compose.yaml -f compose.ai-gpu.yaml`.
3. Pull a model. The runtime network has no internet access, so open it only for the
   download:

   ```bash
   docker compose -f compose.yaml -f compose.ai-pull.yaml --profile ai up -d ai
   docker compose exec ai ollama pull qwen3.8:27b-q4_K_M
   docker compose --profile ai up -d --force-recreate ai   # closes internet access again
   docker compose exec ai sh -c 'sha256sum /root/.ollama/models/manifests/registry.ollama.ai/library/qwen3.8/27b-q4_K_M'
   ```

4. Set in `.env` and recreate the server:

   ```bash
   CHRONEXUS_AI_URL=http://ai:11434
   CHRONEXUS_AI_MODELS=qwen3.8:27b-q4_K_M@<64 hex digits>
   ```

   The digest (the model's manifest hash printed above, the same as the `digest` field of
   the runtime's `/api/tags`; `ollama list` shows its first 12 characters) is optional but
   recommended: with it, the server refuses
   to answer if the installed model changes. Several models are separated by commas.

## Settings

| Variable | Default | Limits | Meaning |
| --- | --- | --- | --- |
| `CHRONEXUS_AI_URL` | empty (off) | `http://` host and port only | Runtime address |
| `CHRONEXUS_AI_MODELS` | — (required when on) | names, optional `@` + 64-hex digest | Models members may choose |
| `CHRONEXUS_AI_CONCURRENCY` | 1 | 1–8 | Answers generated at the same time |
| `CHRONEXUS_AI_QUEUE` | 4 | 1–64 | Questions allowed to wait |
| `CHRONEXUS_AI_PER_USER_HOUR` | 30 | 1–1000 | Questions per account per hour |
| `CHRONEXUS_AI_TIMEOUT_SECONDS` | 120 | 1–900 | Wait plus generation time |
| `CHRONEXUS_AI_MEMORY` | 24g | — | Memory limit of the runtime container |

A full queue or an exhausted hourly allowance returns 429; a runtime that is unreachable,
has cloud features on, or lacks the pinned model returns 503; a timeout returns 504. A
cancelled request frees its place at once. The desktop explains each case and offers an
explicit switch back to local AI; it never switches silently.

## Safeguards

- The runtime container has no published port, drops all capabilities and sits on an
  `internal` network without internet access. `OLLAMA_NO_CLOUD=1` is set, and before every
  answer the server checks that the runtime reports cloud features disabled.
- Before every answer the server checks that the chosen model is installed and, when pinned,
  that its digest matches.
- Membership is checked like any other campaign request; sources are selected with the same
  visibility rule as synchronization. Items that look like instructions to the model are
  left out and counted.
- Questions are limited to 2000 characters; at most 8 sources of up to 1200 characters each
  are sent; generation is bounded (context 8192, 512 output tokens, temperature 0).
- Citations that do not match a provided source make the answer "unverified".

## Qualification (open)

Not done yet, because it needs the target hardware and model weights:

- answer quality, citation accuracy and refusal behaviour of each model on synthetic campaigns
  in every supported locale;
- latency and memory on CPU-only and GPU hosts, and the right `CONCURRENCY`/`QUEUE` for them;
- a physical offline test of the running stack (the Docker network test only proves that the
  container has no route).

Until then, the server AI is an experimental option for operators who accept running their
own models.
