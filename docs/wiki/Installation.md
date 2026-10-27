# Installation

You need Docker with Compose and a domain name pointing at the machine.

```sh
git clone https://github.com/Acelith/Chronexus-Server.git && cd Chronexus-Server
cp .env.example .env            # set CHRONEXUS_DOMAIN=sync.your-domain.example
```

## Mode A — bundled HTTPS

Ports 80 and 443 must reach the host. The bundled Caddy obtains and renews the
certificate.

```sh
docker compose --profile https up -d --build
docker compose exec -T server chronexus-server invite
```

## Mode B — your own reverse proxy

```sh
docker compose up -d --build
```

Point Caddy, nginx or Traefik at `127.0.0.1:8080`. The proxy must terminate TLS for the
exact public origin and **overwrite** `Host`, `X-Forwarded-Proto: https` and a single
`X-Forwarded-For`. Do not buffer responses: change notifications are a long-lived stream.
Examples are in `docs/INSTALLATION.md`.

## Configuration (`.env`)

| Variable | Default | Meaning |
| --- | --- | --- |
| `CHRONEXUS_DOMAIN` | — (required) | Public host name |
| `CHRONEXUS_PUBLIC_ORIGIN` | `https://$CHRONEXUS_DOMAIN` | Set for a nonstandard port |
| `CHRONEXUS_HOST_PORT` / `CHRONEXUS_BIND_ADDRESS` | `8080` / `127.0.0.1` | Where the backend listens on the host |
| `CHRONEXUS_HTTP_PORT` / `CHRONEXUS_HTTPS_PORT` | `80` / `443` | Bundled proxy ports |
| `CHRONEXUS_NETWORK_SUBNET`, `…_GATEWAY`, `CHRONEXUS_PROXY_IP`, `CHRONEXUS_NETWORK_DYNAMIC_RANGE` | `172.30.253.0/24`… | Change together if the subnet is taken |
| `CHRONEXUS_TRUSTED_PROXIES` | gateway and bundled proxy | Never `0.0.0.0/0` |
| `CHRONEXUS_IMAGE` | built locally | Use a published image instead (none published yet) |

## First account

`chronexus-server invite` prints a 24-hour, one-use invitation. Send it privately. In
Chronexus: **App settings → Campaign sharing**, enter `https://your-domain`, **Verify and
save server**, then redeem the invitation with a user name, display name and password.

## Trying it locally

For a test on one computer, use `CHRONEXUS_DOMAIN=chronexus.localhost`,
`CHRONEXUS_PUBLIC_ORIGIN=https://chronexus.localhost:8443`, `CHRONEXUS_HTTPS_PORT=8443` and
`CHRONEXUS_HTTP_PORT=8088`: Caddy issues a certificate from its own local authority. The
released app only trusts publicly valid certificates, so this setup is for development
(see the app's `docs/QA.md`).
