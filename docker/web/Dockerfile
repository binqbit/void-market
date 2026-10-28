# syntax=docker/dockerfile:1
FROM node:24.10.0-bookworm-slim AS build
WORKDIR /web
COPY apps/web/package.json apps/web/package-lock.json ./
RUN npm ci --ignore-scripts
COPY apps/web ./
RUN npm run build

FROM nginxinc/nginx-unprivileged:1.28-alpine
COPY docker/web/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /web/dist /usr/share/nginx/html
EXPOSE 8080
