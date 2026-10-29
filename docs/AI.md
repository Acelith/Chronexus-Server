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

## Qualification

`scripts/qualify-ai.py` runs the repeatable check against a running deployment with synthetic
data only: three accounts, one campaign, a shared place, a shared rumour (a claim), the owner's
private note and an instruction-like note. It asks 14 questions (all ten locales, a claim, a
missing fact, a private fact and a translated name) and records citations, scoping, exclusion,
latency and the runtime's peak memory. It downloads nothing.

An answer passes only when it is verified (every `[S…]` key is a provided source, or it starts
with the not-found sentence), stays within about three sentences and does not start with
visible reasoning.

### 29 October 2026 — Docker Desktop on Apple silicon, CPU only

Host: macOS arm64, Docker Desktop VM with 10 CPUs and 8 GB; runtime limit 6 GB; Ollama 0.40.10
(pinned digest) on the internal network; `qwen3:4b` pinned by manifest digest `359d7dd4…`,
copied from an existing local volume (no download). Deployment: bundled Caddy at
`https://chronexus.localhost:8443`.

| Check | Result |
| --- | --- |
| Outsider asks | refused (403/404) |
| Owner's private note cited to a player | never |
| Instruction-like note sent to the model | never (counted as excluded) |
| Runtime port on the host / internet from the `ai` network | closed / no route (DNS fails) |
| Latency per answer (wall, 1 at a time) | median 28 s, max 76 s (Arabic) |
| Runtime memory | peak about 5.0 GiB |
| Answers passing the quality check | **2 of 14**: the model writes its reasoning ("Okay, let's tackle…") in the answer despite `think: false`, ignores the length limit and often the language |

**Verdict: `qwen3:4b` is not qualified** (consistent with the desktop's M18 retirement of the
small Qwen models). The run found and fixed a server defect: the general 20-second request
bound cut every answer longer than 20 s with 408, so `CHRONEXUS_AI_TIMEOUT_SECONDS` never
took effect; AI answers now use only their own deadline.

Retrieval is lexical: a question that translates a name ("Torre del Sale" for "Salt Tower")
finds no source and returns the not-found answer. Ask with the names used in the notes.

### Still open

- Qwen3.8 27B (primary) and Gemma 4 12B need more memory than this Docker VM has (8 GB);
  they are not qualified on any server host yet, CPU or GPU.
- GPU hosts (NVIDIA with `compose.ai-gpu.yaml`) are untested.
- A physical offline test (network cable removed) of the running stack; the Docker check only
  proves that the container has no route.

Until a model passes, the server AI is an experimental option for operators who accept running
their own models.
