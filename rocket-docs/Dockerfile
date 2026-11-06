# syntax=docker/dockerfile:1

# --- Build stage ---------------------------------------------------------
FROM node:22-slim AS build

RUN npm install -g bun@1

WORKDIR /app

# Cache the dependency install separately from source changes.
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile

COPY . .

RUN bun run build

# --- Runtime stage -------------------------------------------------------
FROM caddy:2-alpine AS runtime

COPY Caddyfile /etc/caddy/Caddyfile
COPY --from=build /app/dist /srv

# Railway sets PORT at runtime. The Caddyfile reads it.
ENV PORT=8080
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget -qO- "http://127.0.0.1:${PORT}/" >/dev/null || exit 1
